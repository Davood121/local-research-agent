//! User-initiated local research.
//!
//! The webview only sends a question. This module chooses the search results,
//! validates every page URL, reads a bounded amount of public text, ranks the
//! excerpts locally and then streams a cited answer through the same local
//! inference path as chat (embedded llama.cpp engine, or Ollama as fallback).
//! Cancellation covers the whole pipeline, not just generation.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Manager, State};
use tokio::sync::broadcast;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::commands::chat::{
    fetch_tags, read_error_body, short, stream_chat, stream_engine_chat, StreamOutcome,
};
use crate::engine;
use crate::error::AppError;
use crate::events::{AppEvent, StepStatus};
use crate::ollama::{
    context_length_for, is_local_model, local_models, validate_chat_request, ChatMessage,
    StartChatRequest,
};
use crate::research::{
    build_context, build_messages, build_system_prompt, canonical_url_key, chunk_text,
    citation_footer, clean_fragment, clean_plain_text, domain_for, evaluate_evidence, extract_html,
    has_citation_labels, is_public_ip, normalize_results, parse_bing_html, parse_duckduckgo_html,
    plan_research, rank_chunks, rank_hits, retry_query, text_matches_query, validate_page_url,
    validate_research_request, Chunk, ChunkKind, PlannedQuery, ResearchSource, SearchHit,
    SearxResponse, SearxResult, StartResearchRequest, CHUNK_CHARS, CHUNK_OVERLAP,
    MAX_CANDIDATE_SOURCES, MAX_CONTEXT_CHUNKS, MAX_FETCHED_PAGES, MAX_PAGE_BYTES, MAX_REDIRECTS,
    MAX_RETRY_PAGES, MAX_SEARCH_RESULTS, MAX_SOURCE_SNIPPET_CHARS, SEARXNG_BASE_URL,
};
use crate::state::AppState;

const STATUS_TIMEOUT: Duration = Duration::from_secs(2);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(15);
const SEARCH_BODY_LIMIT: usize = 2_000_000;
const BODY_CHUNK_TIMEOUT: Duration = Duration::from_secs(12);
const DNS_TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPT_PAGES: &str = "text/html,application/xhtml+xml,text/plain;q=0.9,*/*;q=0.1";
/// Built-in search reads one public result page directly, so research keeps
/// working on a machine without SearXNG.
const BUILTIN_SEARCH_TIMEOUT: Duration = Duration::from_secs(15);
const BUILTIN_SEARCH_BODY_LIMIT: usize = 1_500_000;
const DUCKDUCKGO_SEARCH_URL: &str = "https://html.duckduckgo.com/html/";
const BING_SEARCH_URL: &str = "https://www.bing.com/search";
const SEARCH_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
    (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Debug, Clone, Serialize)]
pub struct ResearchStatus {
    pub connected: bool,
    /// `searxng` when the local instance answers, `builtin` when research falls
    /// back to the engine compiled into the app.
    pub provider: String,
    pub error: Option<String>,
}

/// Report which search backend research will use. A missing SearXNG is not an
/// error: the built-in engine covers it.
#[tauri::command]
pub async fn research_status(state: State<'_, AppState>) -> Result<ResearchStatus, AppError> {
    let Some(client) = state.http.as_ref() else {
        return Ok(ResearchStatus {
            connected: false,
            provider: "none".to_owned(),
            error: Some("The local HTTP client could not be initialised.".to_owned()),
        });
    };

    let provider = match probe_searxng(client).await {
        Ok(()) => "searxng",
        Err(_) => "builtin",
    };

    Ok(ResearchStatus {
        connected: true,
        provider: provider.to_owned(),
        error: None,
    })
}

#[tauri::command]
pub async fn start_research(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartResearchRequest,
) -> Result<(), AppError> {
    validate_research_request(&request).map_err(AppError::new)?;

    let Some(client) = state.http.as_ref().cloned() else {
        return Err(AppError::new(
            "The local HTTP client could not be initialised.",
        ));
    };
    let Some(reader) = state.research_http.as_ref().cloned() else {
        return Err(AppError::new(
            "The research reader could not be initialised.",
        ));
    };

    // The embedded engine owns any run whose model name matches a GGUF in the
    // app-data models directory (same routing as chat). Only fall through to
    // the Ollama validation when the model is not an installed GGUF.
    let engine_model = engine::models_dir(&app)
        .ok()
        .and_then(|dir| engine::find_model_file(&dir, &request.model))
        .map(|path| (request.model.clone(), path));

    let context_length = if engine_model.is_some() {
        // The engine reads the window from the GGUF itself.
        None
    } else {
        let tags = fetch_tags(&client).await.map_err(|error| {
            AppError::new(format!(
                "{error} Start Ollama with `ollama serve`, then refresh the model list."
            ))
        })?;
        let models = local_models(tags);
        if !is_local_model(&models, &request.model) {
            return Err(AppError::new(
                "That model is not available as an installed local chat model. Pull it with `ollama pull <model>` or pick another model.",
            ));
        }
        context_length_for(&models, &request.model)
    };
    let chat_cancel = state.claim_chat(&request.run_id)?;
    let tx = state.tx.clone();
    let app_cancel = state.cancel.clone();
    let run_id = request.run_id;
    let model = request.model;
    let query = request.query.trim().to_owned();
    let history = request.history;

    tauri::async_runtime::spawn(async move {
        run_research(
            &app,
            &client,
            &reader,
            &tx,
            &run_id,
            &model,
            &query,
            &history,
            context_length,
            engine_model,
            &chat_cancel,
            &app_cancel,
        )
        .await;
        app.state::<AppState>().release_chat(&run_id);
    });

    Ok(())
}

async fn probe_searxng(client: &reqwest::Client) -> Result<(), String> {
    let mut last = "no response".to_owned();
    for path in ["/healthz", "/"] {
        let url = format!("{SEARXNG_BASE_URL}{path}");
        match client.get(url).timeout(STATUS_TIMEOUT).send().await {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) => last = format!("HTTP {}", response.status()),
            Err(error) => last = short(&error.to_string()),
        }
    }
    Err(format!(
        "SearXNG did not answer at 127.0.0.1:8080 ({last}). Start it with the setup in docs/RESEARCH.md, then refresh."
    ))
}

#[allow(clippy::too_many_arguments)]
async fn run_research(
    app: &AppHandle,
    client: &reqwest::Client,
    reader: &reqwest::Client,
    tx: &broadcast::Sender<AppEvent>,
    run_id: &str,
    model: &str,
    query: &str,
    history: &[ChatMessage],
    context_length: Option<u64>,
    engine_model: Option<(String, PathBuf)>,
    chat_cancel: &CancellationToken,
    app_cancel: &CancellationToken,
) {
    let research_started = Instant::now();
    send_step(tx, run_id, "research", StepStatus::Started, None);

    // 1. Plan the question locally: intent, sub-queries, engines, time range.
    let plan_started = Instant::now();
    send_step(tx, run_id, "plan", StepStatus::Started, None);
    let plan = plan_research(query);
    if plan.queries.is_empty() {
        let args = json!({ "intent": plan.intent.label() });
        send_tool(
            tx,
            run_id,
            "plan_research",
            args,
            StepStatus::Failed,
            Some(elapsed_ms(plan_started)),
            Some("the query could not be planned".to_owned()),
        );
        send_step(
            tx,
            run_id,
            "plan",
            StepStatus::Failed,
            Some(elapsed_ms(plan_started)),
        );
        fail_research(
            tx,
            run_id,
            "The research query could not be planned.".to_owned(),
            elapsed_ms(research_started),
        );
        return;
    }

    let planned_queries = serde_json::to_value(&plan.queries).unwrap_or_else(|_| json!([]));
    let plan_duration = elapsed_ms(plan_started);
    send_tool(
        tx,
        run_id,
        "plan_research",
        json!({
            "intent": plan.intent.label(),
            "queries": planned_queries,
        }),
        StepStatus::Done,
        Some(plan_duration),
        Some(format!(
            "{} · {} search path(s)",
            plan.intent.label(),
            plan.queries.len()
        )),
    );
    send_step(tx, run_id, "plan", StepStatus::Done, Some(plan_duration));

    // The full prompt stays the model's question, but lexical ranking should
    // use the focused content query when the planner produced one. Question
    // words such as \"what\" and \"is\" otherwise create accidental matches in
    // unrelated snippets.
    let ranking_query = plan
        .queries
        .iter()
        .find(|query| query.purpose == "focus")
        .or_else(|| plan.queries.first())
        .map(|query| query.query.as_str())
        .unwrap_or(query);

    // 2. Search every planned path in parallel. A failed engine route falls
    // back to the general category inside search_planned_query.
    let search_started = Instant::now();
    send_step(tx, run_id, "search", StepStatus::Started, None);
    let search_args = json!({
        "intent": plan.intent.label(),
        "queries": plan.queries.iter().map(|query| query.query.clone()).collect::<Vec<_>>(),
        "engines": plan.queries.iter().map(|query| query.engines.clone()).collect::<Vec<_>>(),
    });
    send_tool(
        tx,
        run_id,
        "web_search",
        search_args.clone(),
        StepStatus::Started,
        None,
        None,
    );

    let search_batch =
        match search_plan(client, reader, &plan.queries, app_cancel, chat_cancel).await {
            Phase::Cancelled => {
                cancel_phase(
                    tx,
                    run_id,
                    "web_search",
                    search_args,
                    "search",
                    search_started,
                    research_started,
                );
                return;
            }
            Phase::Done(batch) => batch,
        };
    let SearchBatch { hits, errors } = search_batch;
    if hits.is_empty() {
        let detail = errors.first().cloned().unwrap_or_else(|| {
            "SearXNG returned no usable results. Try a different query or check the instance configuration."
                .to_owned()
        });
        send_tool(
            tx,
            run_id,
            "web_search",
            search_args,
            StepStatus::Failed,
            Some(elapsed_ms(search_started)),
            Some(short(&detail)),
        );
        send_step(
            tx,
            run_id,
            "search",
            StepStatus::Failed,
            Some(elapsed_ms(search_started)),
        );
        fail_research(tx, run_id, detail, elapsed_ms(research_started));
        return;
    }

    let mut ranked = rank_hits(ranking_query, hits, MAX_CANDIDATE_SOURCES);
    let search_duration = elapsed_ms(search_started);
    let error_note = if errors.is_empty() {
        String::new()
    } else {
        format!(" · {} sub-query error(s)", errors.len())
    };
    send_tool(
        tx,
        run_id,
        "web_search",
        search_args,
        StepStatus::Done,
        Some(search_duration),
        Some(format!(
            "{} unique source(s) from {} search path(s){}",
            ranked.len(),
            plan.queries.len(),
            error_note
        )),
    );
    send_step(
        tx,
        run_id,
        "search",
        StepStatus::Done,
        Some(search_duration),
    );

    // 3. Read pages in one bounded wave. The retry below reads at most two
    // additional pages, so a weak first pass cannot grow without limit.
    let read_started = Instant::now();
    send_step(tx, run_id, "read", StepStatus::Started, None);
    let mut fetched: HashMap<String, Result<FetchedPage, String>> = HashMap::new();
    let initial_keys = next_fetch_keys(&ranked, MAX_FETCHED_PAGES, &fetched);
    let read_args = json!({ "pages": initial_keys.len(), "adaptive": true });
    send_tool(
        tx,
        run_id,
        "read_pages",
        read_args.clone(),
        StepStatus::Started,
        None,
        None,
    );
    if let Phase::Cancelled = fetch_hits(
        reader,
        &ranked,
        &initial_keys,
        &mut fetched,
        app_cancel,
        chat_cancel,
    )
    .await
    {
        cancel_phase(
            tx,
            run_id,
            "read_pages",
            read_args,
            "read",
            read_started,
            research_started,
        );
        return;
    }

    let (mut sources, mut chunks, _) = assemble_sources(&ranked, &fetched, ranking_query);
    let initial_read_summary = read_summary(initial_keys.len(), &fetched, sources.len());
    let initial_read_duration = elapsed_ms(read_started);
    send_tool(
        tx,
        run_id,
        "read_pages",
        read_args,
        StepStatus::Done,
        Some(initial_read_duration),
        Some(initial_read_summary),
    );
    send_step(
        tx,
        run_id,
        "read",
        StepStatus::Done,
        Some(initial_read_duration),
    );

    // 4. Evaluate evidence locally. If coverage or source diversity is weak,
    // run one broader search and read at most two more pages.
    let verify_started = Instant::now();
    send_step(tx, run_id, "verify", StepStatus::Started, None);
    let mut report = evaluate_evidence(ranking_query, &sources, &chunks);
    let mut retried = false;

    if report.is_weak() {
        let retry_text = retry_query(&plan);
        if !retry_text.is_empty() {
            retried = true;
            let retry_query = PlannedQuery {
                query: retry_text,
                engines: Vec::new(),
                purpose: "retry".to_owned(),
                time_range: None,
            };
            let retry_args = json!({
                "query": retry_query.query.clone(),
                "retry": true,
            });
            send_tool(
                tx,
                run_id,
                "web_search",
                retry_args.clone(),
                StepStatus::Started,
                None,
                None,
            );

            let retry_batch = match search_plan(
                client,
                reader,
                std::slice::from_ref(&retry_query),
                app_cancel,
                chat_cancel,
            )
            .await
            {
                Phase::Cancelled => {
                    cancel_phase(
                        tx,
                        run_id,
                        "web_search",
                        retry_args,
                        "verify",
                        verify_started,
                        research_started,
                    );
                    return;
                }
                Phase::Done(batch) => batch,
            };

            if !retry_batch.hits.is_empty() {
                let mut combined = ranked;
                combined.extend(retry_batch.hits);
                ranked = rank_hits(ranking_query, combined, MAX_CANDIDATE_SOURCES);

                let retry_keys = next_fetch_keys(&ranked, MAX_RETRY_PAGES, &fetched);
                if !retry_keys.is_empty() {
                    let retry_read_args = json!({ "pages": retry_keys.len(), "retry": true });
                    send_tool(
                        tx,
                        run_id,
                        "read_pages",
                        retry_read_args.clone(),
                        StepStatus::Started,
                        None,
                        None,
                    );
                    if let Phase::Cancelled = fetch_hits(
                        reader,
                        &ranked,
                        &retry_keys,
                        &mut fetched,
                        app_cancel,
                        chat_cancel,
                    )
                    .await
                    {
                        cancel_phase(
                            tx,
                            run_id,
                            "read_pages",
                            retry_read_args,
                            "verify",
                            verify_started,
                            research_started,
                        );
                        return;
                    }
                }

                let assembled = assemble_sources(&ranked, &fetched, ranking_query);
                sources = assembled.0;
                chunks = assembled.1;
                report = evaluate_evidence(ranking_query, &sources, &chunks);
            }

            let retry_summary = if retry_batch.errors.is_empty() {
                format!("retry searched \"{}\"", retry_query.query)
            } else {
                format!("retry search failed: {}", short(&retry_batch.errors[0]))
            };
            send_tool(
                tx,
                run_id,
                "web_search",
                retry_args,
                StepStatus::Done,
                Some(elapsed_ms(verify_started)),
                Some(retry_summary),
            );
        }
    }

    if chunks.is_empty() {
        send_tool(
            tx,
            run_id,
            "verify_evidence",
            json!({ "retried": retried }),
            StepStatus::Failed,
            Some(elapsed_ms(verify_started)),
            Some("no readable evidence".to_owned()),
        );
        send_step(
            tx,
            run_id,
            "verify",
            StepStatus::Failed,
            Some(elapsed_ms(verify_started)),
        );
        fail_research(
            tx,
            run_id,
            "The research run returned no readable page text. Try a different query or check the local search engines.".to_owned(),
            elapsed_ms(research_started),
        );
        return;
    }

    let verify_duration = elapsed_ms(verify_started);
    let weak_note = if report.is_weak() {
        " · thin evidence; answer will be qualified"
    } else {
        " · evidence sufficient"
    };
    send_tool(
        tx,
        run_id,
        "verify_evidence",
        json!({
            "coverage": report.coverage_percent(),
            "domains": report.domain_count,
            "read": report.read_count,
            "sources": report.source_count,
            "retried": retried,
        }),
        StepStatus::Done,
        Some(verify_duration),
        Some(format!("{}{}", report.summary(), weak_note)),
    );
    send_step(
        tx,
        run_id,
        "verify",
        StepStatus::Done,
        Some(verify_duration),
    );

    if retried {
        let total_read_summary = read_summary(fetched.len(), &fetched, sources.len());
        send_tool(
            tx,
            run_id,
            "read_pages",
            json!({ "pages": fetched.len(), "adaptive": true }),
            StepStatus::Done,
            Some(elapsed_ms(read_started)),
            Some(total_read_summary),
        );
    }

    let _ = tx.send(AppEvent::Sources {
        run_id: run_id.to_owned(),
        sources: sources.clone(),
    });

    let selected = rank_chunks(ranking_query, &chunks, MAX_CONTEXT_CHUNKS);
    let context = build_context(&sources, &chunks, &selected);
    let mut system_prompt = build_system_prompt(&context, sources.len());
    if report.is_weak() {
        system_prompt.push_str(
            "\n\nEVIDENCE NOTE: The retrieved evidence is thin or comes from few independent domains. Qualify uncertain claims, do not assert details that the sources do not support, and briefly say what could not be verified. Do not mention this note.",
        );
    }
    let messages = build_messages(system_prompt, history, query);

    let chat_request = StartChatRequest {
        run_id: run_id.to_owned(),
        model: model.to_owned(),
        messages: messages.clone(),
    };
    if let Err(error) = validate_chat_request(&chat_request) {
        fail_research(
            tx,
            run_id,
            format!("The research context could not be prepared: {error}"),
            elapsed_ms(research_started),
        );
        return;
    }

    send_step(
        tx,
        run_id,
        "research",
        StepStatus::Done,
        Some(elapsed_ms(research_started)),
    );

    let chat_started = Instant::now();
    send_step(tx, run_id, "chat", StepStatus::Started, None);
    // Collected only to check whether the model cited anything; small models
    // often skip inline labels even when instructed.
    let answer = Arc::new(Mutex::new(String::new()));
    let outcome = match engine_model.as_ref() {
        Some((engine_name, engine_path)) => {
            // App exit must interrupt generation too; mirror the app token into
            // a child token, exactly like the chat engine path does.
            let gen_cancel = chat_cancel.child_token();
            let mirror_cancel = gen_cancel.clone();
            let app_cancel = app_cancel.clone();
            tokio::spawn(async move {
                app_cancel.cancelled().await;
                mirror_cancel.cancel();
            });
            stream_engine_chat(
                app,
                tx,
                run_id,
                engine_name,
                engine_path,
                &messages,
                gen_cancel,
                Some(&answer),
            )
            .await
        }
        None => {
            stream_chat(
                client,
                run_id,
                model,
                &messages,
                context_length,
                chat_started,
                tx,
                chat_cancel,
                app_cancel,
                Some(&answer),
            )
            .await
        }
    };
    let chat_duration = elapsed_ms(chat_started);

    match outcome {
        Ok(StreamOutcome::Completed) => {
            // Guarantee the numbered-citation promise: if the model answered in
            // prose only, append the sources whose excerpts were in the context.
            let answer_text = match answer.lock() {
                Ok(text) => text.clone(),
                Err(poisoned) => poisoned.into_inner().clone(),
            };
            if !has_citation_labels(&answer_text) {
                let footer = citation_footer(&sources, &chunks, &selected);
                if !footer.is_empty() {
                    let _ = tx.send(AppEvent::Token {
                        run_id: run_id.to_owned(),
                        delta: footer,
                        index: 0,
                    });
                }
            }
            send_step(tx, run_id, "chat", StepStatus::Done, Some(chat_duration));
        }
        Ok(StreamOutcome::Cancelled) => {
            let _ = tx.send(AppEvent::Error {
                scope: "research".to_owned(),
                message: "Generation stopped.".to_owned(),
                recoverable: true,
            });
            send_step(tx, run_id, "chat", StepStatus::Failed, Some(chat_duration));
        }
        Err(error) => {
            let _ = tx.send(AppEvent::Error {
                scope: "research".to_owned(),
                message: error,
                recoverable: true,
            });
            send_step(tx, run_id, "chat", StepStatus::Failed, Some(chat_duration));
        }
    }
}

enum Phase<T> {
    Done(T),
    Cancelled,
}

struct SearchBatch {
    hits: Vec<SearchHit>,
    errors: Vec<String>,
}

async fn search_plan(
    client: &reqwest::Client,
    reader: &reqwest::Client,
    queries: &[PlannedQuery],
    app_cancel: &CancellationToken,
    chat_cancel: &CancellationToken,
) -> Phase<SearchBatch> {
    let mut tasks = JoinSet::new();
    for (index, query) in queries.iter().enumerate() {
        let client = client.clone();
        let reader = reader.clone();
        let query = query.clone();
        tasks.spawn(async move { (index, search_planned_query(&client, &reader, &query).await) });
    }

    let mut results: Vec<Option<Result<Vec<SearchHit>, String>>> =
        queries.iter().map(|_| None).collect();
    loop {
        if tasks.is_empty() {
            break;
        }
        let joined = tokio::select! {
            _ = app_cancel.cancelled() => {
                tasks.abort_all();
                return Phase::Cancelled;
            }
            _ = chat_cancel.cancelled() => {
                tasks.abort_all();
                return Phase::Cancelled;
            }
            joined = tasks.join_next() => joined,
        };
        let Some(joined) = joined else {
            break;
        };
        if let Ok((index, result)) = joined {
            if let Some(slot) = results.get_mut(index) {
                *slot = Some(result);
            }
        }
    }

    let mut hits = Vec::new();
    let mut errors = Vec::new();
    for result in results.into_iter().flatten() {
        match result {
            Ok(mut found) => hits.append(&mut found),
            Err(error) => errors.push(error),
        }
    }
    Phase::Done(SearchBatch { hits, errors })
}

async fn search_planned_query(
    client: &reqwest::Client,
    reader: &reqwest::Client,
    query: &PlannedQuery,
) -> Result<Vec<SearchHit>, String> {
    let primary = search_web(
        client,
        reader,
        &query.query,
        &query.engines,
        query.time_range.as_deref(),
    )
    .await;

    if matches!(&primary, Ok(hits) if !hits.is_empty()) {
        return primary;
    }

    // A route can name an engine the user has not enabled, and some engines do
    // not support a recent time range. A broad general search without a time
    // filter is the most reliable single fallback; it also keeps research
    // working with a custom SearXNG configuration.
    if !query.engines.is_empty() || query.time_range.is_some() {
        return search_web(client, reader, &query.query, &[], None).await;
    }

    primary
}

/// Search through the local SearXNG instance when it is running, and through the
/// built-in engine otherwise. Research never fails just because SearXNG is not
/// installed.
async fn search_web(
    client: &reqwest::Client,
    reader: &reqwest::Client,
    query: &str,
    engines: &[String],
    time_range: Option<&str>,
) -> Result<Vec<SearchHit>, String> {
    match search_searxng(client, query, engines, time_range).await {
        Ok(hits) if !hits.is_empty() => Ok(hits),
        Ok(_) => search_builtin(reader, query, engines, time_range).await,
        Err(searx_error) => match search_builtin(reader, query, engines, time_range).await {
            Ok(hits) if !hits.is_empty() => Ok(hits),
            Ok(_) => Err(searx_error),
            Err(builtin_error) => Err(format!(
                "{searx_error} The built-in search also failed ({builtin_error})."
            )),
        },
    }
}

/// Query a public search page directly, without SearXNG, and normalize the
/// results exactly like a SearXNG response.
async fn search_builtin(
    reader: &reqwest::Client,
    query: &str,
    engines: &[String],
    time_range: Option<&str>,
) -> Result<Vec<SearchHit>, String> {
    let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    if query.is_empty() {
        return Err("The research query is empty.".to_owned());
    }

    let prefers_bing = engines.iter().any(|engine| engine == "bing");
    let mut errors = Vec::new();

    for provider in if prefers_bing {
        [BuiltinProvider::Bing, BuiltinProvider::DuckDuckGo]
    } else {
        [BuiltinProvider::DuckDuckGo, BuiltinProvider::Bing]
    } {
        match provider.search(reader, &query, time_range).await {
            Ok(results) if !results.is_empty() => {
                return Ok(normalize_results(
                    SearxResponse { results },
                    MAX_SEARCH_RESULTS,
                ));
            }
            Ok(_) => errors.push(format!("{} returned no results", provider.label())),
            Err(error) => errors.push(format!("{}: {error}", provider.label())),
        }
    }

    Err(errors.join("; "))
}

#[derive(Debug, Clone, Copy)]
enum BuiltinProvider {
    DuckDuckGo,
    Bing,
}

impl BuiltinProvider {
    fn label(self) -> &'static str {
        match self {
            Self::DuckDuckGo => "DuckDuckGo",
            Self::Bing => "Bing",
        }
    }

    async fn search(
        self,
        reader: &reqwest::Client,
        query: &str,
        time_range: Option<&str>,
    ) -> Result<Vec<SearxResult>, String> {
        let mut endpoint = match self {
            Self::DuckDuckGo => reqwest::Url::parse(DUCKDUCKGO_SEARCH_URL),
            Self::Bing => reqwest::Url::parse(BING_SEARCH_URL),
        }
        .map_err(|_| "The built-in search endpoint is invalid.".to_owned())?;
        {
            let mut pairs = endpoint.query_pairs_mut();
            pairs.append_pair("q", query);
            if matches!(self, Self::Bing) {
                pairs.append_pair("count", "20");
            }
            if let (Self::DuckDuckGo, Some(code)) =
                (self, time_range.and_then(duckduckgo_time_range))
            {
                pairs.append_pair("df", code);
            }
        }

        let response = reader
            .get(endpoint)
            .header(reqwest::header::ACCEPT, ACCEPT_PAGES)
            .header(reqwest::header::USER_AGENT, SEARCH_USER_AGENT)
            .timeout(BUILTIN_SEARCH_TIMEOUT)
            .send()
            .await
            .map_err(|error| short(&error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!("HTTP {status}"));
        }

        let body = read_limited_body(response, BUILTIN_SEARCH_BODY_LIMIT).await?;
        let html = String::from_utf8_lossy(&body);
        Ok(match self {
            Self::DuckDuckGo => parse_duckduckgo_html(&html),
            Self::Bing => parse_bing_html(&html),
        })
    }
}

fn duckduckgo_time_range(range: &str) -> Option<&'static str> {
    match range {
        "day" => Some("d"),
        "week" => Some("w"),
        "month" => Some("m"),
        "year" => Some("y"),
        _ => None,
    }
}

async fn fetch_hits(
    reader: &reqwest::Client,
    hits: &[SearchHit],
    keys: &[String],
    fetched: &mut HashMap<String, Result<FetchedPage, String>>,
    app_cancel: &CancellationToken,
    chat_cancel: &CancellationToken,
) -> Phase<()> {
    let mut tasks = JoinSet::new();
    for key in keys {
        let Some(hit) = hits.iter().find(|hit| canonical_url_key(&hit.url) == *key) else {
            continue;
        };
        let reader = reader.clone();
        let url = hit.url.clone();
        let title = hit.title.clone();
        let key = key.clone();
        tasks.spawn(async move { (key, fetch_page(&reader, &url, &title).await) });
    }

    loop {
        if tasks.is_empty() {
            break;
        }
        let joined = tokio::select! {
            _ = app_cancel.cancelled() => {
                tasks.abort_all();
                return Phase::Cancelled;
            }
            _ = chat_cancel.cancelled() => {
                tasks.abort_all();
                return Phase::Cancelled;
            }
            joined = tasks.join_next() => joined,
        };
        let Some(joined) = joined else {
            break;
        };
        if let Ok((key, result)) = joined {
            fetched.insert(key, result);
        }
    }

    Phase::Done(())
}

fn next_fetch_keys(
    hits: &[SearchHit],
    limit: usize,
    fetched: &HashMap<String, Result<FetchedPage, String>>,
) -> Vec<String> {
    let mut keys = Vec::new();
    for hit in hits {
        let key = canonical_url_key(&hit.url);
        if key.is_empty() || fetched.contains_key(&key) || keys.contains(&key) {
            continue;
        }
        keys.push(key);
        if keys.len() >= limit {
            break;
        }
    }
    keys
}

fn assemble_sources(
    hits: &[SearchHit],
    fetched: &HashMap<String, Result<FetchedPage, String>>,
    query: &str,
) -> (Vec<ResearchSource>, Vec<Chunk>, usize) {
    let mut sources = Vec::new();
    let mut chunks = Vec::new();
    let mut read_count = 0usize;
    let mut seen = std::collections::HashSet::new();

    for hit in hits {
        let key = canonical_url_key(&hit.url);
        let page = fetched.get(&key).and_then(|result| result.as_ref().ok());
        let snippet = if !hit.snippet.is_empty() {
            hit.snippet.clone()
        } else {
            page.map(|page| clean_fragment(&page.text, MAX_SOURCE_SNIPPET_CHARS))
                .unwrap_or_default()
        };
        if page.is_none() && snippet.is_empty() {
            continue;
        }
        if page.is_none() && !text_matches_query(query, &hit.title, &hit.snippet, 2) {
            continue;
        }

        let title = page
            .map(|page| page.title.trim())
            .filter(|title| !title.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| hit.title.clone());
        let url = page
            .map(|page| page.final_url.clone())
            .unwrap_or_else(|| hit.url.clone());
        let final_key = canonical_url_key(&url);
        if final_key.is_empty() || !seen.insert(final_key) {
            continue;
        }

        let id = u32::try_from(sources.len() + 1).unwrap_or(u32::MAX);
        let read = page.is_some();
        if read {
            read_count = read_count.saturating_add(1);
        }
        sources.push(ResearchSource {
            id,
            title,
            domain: domain_for(&url),
            url,
            snippet: snippet.clone(),
            engine: hit.engine.clone(),
            read,
        });

        match page {
            Some(page) => {
                let page_chunks = chunk_text(&page.text, CHUNK_CHARS, CHUNK_OVERLAP);
                if page_chunks.is_empty() {
                    chunks.push(Chunk {
                        source_id: id,
                        text: page.text.clone(),
                        kind: ChunkKind::Page,
                    });
                } else {
                    for text in page_chunks {
                        chunks.push(Chunk {
                            source_id: id,
                            text,
                            kind: ChunkKind::Page,
                        });
                    }
                }
            }
            None => {
                if !snippet.is_empty() {
                    chunks.push(Chunk {
                        source_id: id,
                        text: snippet,
                        kind: ChunkKind::Snippet,
                    });
                }
            }
        }
    }

    (sources, chunks, read_count)
}

fn read_summary(
    attempted: usize,
    fetched: &HashMap<String, Result<FetchedPage, String>>,
    source_count: usize,
) -> String {
    let read = fetched
        .values()
        .filter(|result| result.as_ref().is_ok())
        .count();
    if read >= attempted {
        format!("read {read} page(s) for {source_count} source(s)")
    } else {
        format!(
            "read {read}/{attempted} pages for {source_count} source(s); used snippets for the rest"
        )
    }
}

fn cancel_phase(
    tx: &broadcast::Sender<AppEvent>,
    run_id: &str,
    tool_name: &str,
    args: serde_json::Value,
    step_node: &str,
    started: Instant,
    research_started: Instant,
) {
    send_tool(
        tx,
        run_id,
        tool_name,
        args,
        StepStatus::Failed,
        Some(elapsed_ms(started)),
        Some("cancelled".to_owned()),
    );
    send_step(
        tx,
        run_id,
        step_node,
        StepStatus::Failed,
        Some(elapsed_ms(started)),
    );
    fail_research(
        tx,
        run_id,
        "Generation stopped.".to_owned(),
        elapsed_ms(research_started),
    );
}

async fn search_searxng(
    client: &reqwest::Client,
    query: &str,
    engines: &[String],
    time_range: Option<&str>,
) -> Result<Vec<SearchHit>, String> {
    let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    if query.is_empty() {
        return Err("The research query is empty.".to_owned());
    }

    let mut endpoint = reqwest::Url::parse(&format!("{SEARXNG_BASE_URL}/search"))
        .map_err(|_| "The local search endpoint is invalid.".to_owned())?;
    endpoint
        .query_pairs_mut()
        .append_pair("q", &query)
        .append_pair("format", "json")
        .append_pair("safesearch", "1");
    if engines.is_empty() {
        endpoint
            .query_pairs_mut()
            .append_pair("categories", "general");
    } else {
        // SearXNG applies an explicit engine list when it is sent without a
        // category filter. Sending both makes the category win and silently
        // queries every general engine again.
        endpoint
            .query_pairs_mut()
            .append_pair("engines", &engines.join(","));
    }
    if let Some(range) = time_range {
        if matches!(range, "day" | "week" | "month" | "year") {
            endpoint.query_pairs_mut().append_pair("time_range", range);
        }
    }

    let response = client
        .get(endpoint)
        .header(reqwest::header::ACCEPT, "application/json")
        .timeout(SEARCH_TIMEOUT)
        .send()
        .await
        .map_err(|error| {
            format!(
                "SearXNG is not reachable at 127.0.0.1:8080 ({}). Start it, then refresh.",
                short(&error.to_string())
            )
        })?;

    let status = response.status();
    if !status.is_success() {
        let detail = read_error_body(response).await;
        return Err(if detail.is_empty() {
            format!("SearXNG rejected the search with HTTP {status}.")
        } else {
            format!("SearXNG rejected the search with HTTP {status}: {detail}")
        });
    }

    let body = read_limited_body(response, SEARCH_BODY_LIMIT).await?;
    if body.is_empty() {
        return Err("SearXNG returned an empty search response.".to_owned());
    }
    let parsed: SearxResponse = serde_json::from_slice(&body).map_err(|_| {
        "SearXNG did not return JSON. Add `json` to `search.formats` in its settings.yml, restart it, then try again."
            .to_owned()
    })?;

    Ok(normalize_results(parsed, MAX_SEARCH_RESULTS))
}
struct FetchedPage {
    final_url: String,
    title: String,
    text: String,
}

async fn fetch_page(
    client: &reqwest::Client,
    raw_url: &str,
    fallback_title: &str,
) -> Result<FetchedPage, String> {
    let mut current = validate_page_url(raw_url)?;

    for redirect in 0..=MAX_REDIRECTS {
        ensure_public_host(&current).await?;
        let response = client
            .get(current.clone())
            .header(reqwest::header::ACCEPT, ACCEPT_PAGES)
            .send()
            .await
            .map_err(|error| {
                format!(
                    "Could not read {} ({}).",
                    domain_for(current.as_str()),
                    short(&error.to_string())
                )
            })?;

        let status = response.status();
        if status.is_redirection() {
            if redirect == MAX_REDIRECTS {
                return Err("The page redirected too many times.".to_owned());
            }
            let Some(location) = response.headers().get(reqwest::header::LOCATION) else {
                return Err("The page redirect had no destination.".to_owned());
            };
            let location = location
                .to_str()
                .map_err(|_| "The page redirect was not valid.".to_owned())?;
            let next = current
                .join(location)
                .map_err(|_| "The page redirect was not valid.".to_owned())?;
            current = validate_page_url(next.as_str())?;
            continue;
        }

        if !status.is_success() {
            let detail = read_error_body(response).await;
            return Err(if detail.is_empty() {
                format!("{} returned HTTP {status}.", domain_for(current.as_str()))
            } else {
                format!(
                    "{} returned HTTP {status}: {detail}",
                    domain_for(current.as_str())
                )
            });
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if !content_type.is_empty()
            && !matches!(
                content_type.as_str(),
                "text/html" | "application/xhtml+xml" | "text/plain"
            )
        {
            return Err(format!("That page is not readable text ({content_type})."));
        }

        let body = read_limited_body(response, MAX_PAGE_BYTES).await?;
        let raw = String::from_utf8_lossy(&body).into_owned();
        let (title, text) = if content_type == "text/plain" {
            (clean_fragment(fallback_title, 200), clean_plain_text(&raw))
        } else {
            extract_html(&raw, fallback_title)
        };

        if text.trim().len() < 40 {
            return Err("The page did not contain enough readable text.".to_owned());
        }

        return Ok(FetchedPage {
            final_url: current.to_string(),
            title,
            text,
        });
    }

    Err("The page redirected too many times.".to_owned())
}

async fn ensure_public_host(url: &reqwest::Url) -> Result<(), String> {
    let host = url
        .host_str()
        .ok_or_else(|| "The page URL has no host.".to_owned())?;
    let host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "The page URL has no port.".to_owned())?;

    let addresses = tokio::time::timeout(DNS_TIMEOUT, tokio::net::lookup_host((host, port)))
        .await
        .map_err(|_| "The page host lookup timed out.".to_owned())?
        .map_err(|_| "The page host could not be resolved.".to_owned())?;

    let mut found = false;
    for address in addresses {
        found = true;
        if !is_public_ip(address.ip()) {
            return Err("The page host resolved to a private address.".to_owned());
        }
    }
    if !found {
        return Err("The page host had no addresses.".to_owned());
    }
    Ok(())
}

async fn read_limited_body(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err("The response exceeded the local size limit.".to_owned());
    }

    let mut body = Vec::new();
    loop {
        let chunk = tokio::time::timeout(BODY_CHUNK_TIMEOUT, response.chunk())
            .await
            .map_err(|_| "The response body timed out.".to_owned())?;
        match chunk {
            Ok(Some(bytes)) => {
                if body.len().saturating_add(bytes.len()) > limit {
                    return Err("The response exceeded the local size limit.".to_owned());
                }
                body.extend_from_slice(&bytes);
            }
            Ok(None) => break,
            Err(error) => {
                return Err(format!(
                    "The response body failed ({}).",
                    short(&error.to_string())
                ));
            }
        }
    }
    Ok(body)
}

fn send_step(
    tx: &broadcast::Sender<AppEvent>,
    run_id: &str,
    node: &str,
    status: StepStatus,
    duration_ms: Option<u32>,
) {
    let _ = tx.send(AppEvent::Step {
        run_id: run_id.to_owned(),
        node: node.to_owned(),
        status,
        duration_ms,
    });
}

fn send_tool(
    tx: &broadcast::Sender<AppEvent>,
    run_id: &str,
    name: &str,
    args: serde_json::Value,
    status: StepStatus,
    duration_ms: Option<u32>,
    summary: Option<String>,
) {
    let _ = tx.send(AppEvent::Tool {
        run_id: run_id.to_owned(),
        name: name.to_owned(),
        args,
        status,
        duration_ms,
        summary,
    });
}

fn fail_research(
    tx: &broadcast::Sender<AppEvent>,
    run_id: &str,
    message: String,
    duration_ms: u32,
) {
    let _ = tx.send(AppEvent::Error {
        scope: "research".to_owned(),
        message: short(&message),
        recoverable: true,
    });
    send_step(
        tx,
        run_id,
        "research",
        StepStatus::Failed,
        Some(duration_ms),
    );
}

fn elapsed_ms(started: Instant) -> u32 {
    u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}
