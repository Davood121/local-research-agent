//! User-initiated local research.
//!
//! The webview only sends a question. This module chooses the search results,
//! validates every page URL, reads a bounded amount of public text, ranks the
//! excerpts locally and then streams a cited answer through the same Ollama
//! path as chat. Cancellation covers the whole pipeline, not just generation.

use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Manager, State};
use tokio::sync::broadcast;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::commands::chat::{fetch_tags, read_error_body, short, stream_chat, StreamOutcome};
use crate::error::AppError;
use crate::events::{AppEvent, StepStatus};
use crate::ollama::{
    context_length_for, is_local_model, local_models, validate_chat_request, ChatMessage,
    StartChatRequest,
};
use crate::research::{
    build_context, build_messages, build_system_prompt, chunk_text, clean_fragment,
    clean_plain_text, domain_for, extract_html, is_public_ip, normalize_results, rank_chunks,
    search_query, validate_page_url, validate_research_request, Chunk, ChunkKind, ResearchSource,
    SearxResponse, StartResearchRequest, CHUNK_CHARS, CHUNK_OVERLAP, MAX_CONTEXT_CHUNKS,
    MAX_FETCHED_PAGES, MAX_PAGE_BYTES, MAX_REDIRECTS, MAX_SEARCH_RESULTS, MAX_SOURCE_SNIPPET_CHARS,
    SEARXNG_BASE_URL,
};
use crate::state::AppState;

const STATUS_TIMEOUT: Duration = Duration::from_secs(2);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(15);
const SEARCH_BODY_LIMIT: usize = 2_000_000;
const BODY_CHUNK_TIMEOUT: Duration = Duration::from_secs(12);
const DNS_TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPT_PAGES: &str = "text/html,application/xhtml+xml,text/plain;q=0.9,*/*;q=0.1";

#[derive(Debug, Clone, Serialize)]
pub struct ResearchStatus {
    pub connected: bool,
    pub error: Option<String>,
}

/// Probe the fixed loopback SearXNG endpoint. The search API itself validates
/// JSON support; this only answers "is the local service up?".
#[tauri::command]
pub async fn research_status(state: State<'_, AppState>) -> Result<ResearchStatus, AppError> {
    let Some(client) = state.http.as_ref() else {
        return Ok(ResearchStatus {
            connected: false,
            error: Some("The local HTTP client could not be initialised.".to_owned()),
        });
    };

    match probe_searxng(client).await {
        Ok(()) => Ok(ResearchStatus {
            connected: true,
            error: None,
        }),
        Err(error) => Ok(ResearchStatus {
            connected: false,
            error: Some(error),
        }),
    }
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
    let context_length = context_length_for(&models, &request.model);
    let chat_cancel = state.claim_chat(&request.run_id)?;
    let tx = state.tx.clone();
    let app_cancel = state.cancel.clone();
    let run_id = request.run_id;
    let model = request.model;
    let query = request.query.trim().to_owned();
    let history = request.history;

    tauri::async_runtime::spawn(async move {
        run_research(
            &client,
            &reader,
            &tx,
            &run_id,
            &model,
            &query,
            &history,
            context_length,
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
    client: &reqwest::Client,
    reader: &reqwest::Client,
    tx: &broadcast::Sender<AppEvent>,
    run_id: &str,
    model: &str,
    query: &str,
    history: &[ChatMessage],
    context_length: Option<u64>,
    chat_cancel: &CancellationToken,
    app_cancel: &CancellationToken,
) {
    let research_started = Instant::now();
    send_step(tx, run_id, "research", StepStatus::Started, None);

    let search_started = Instant::now();
    send_tool(
        tx,
        run_id,
        "web_search",
        json!({ "query": search_query(query) }),
        StepStatus::Started,
        None,
        None,
    );
    let search_result = tokio::select! {
        _ = app_cancel.cancelled() => None,
        _ = chat_cancel.cancelled() => None,
        result = search_searxng(client, query) => Some(result),
    };
    let Some(search_result) = search_result else {
        send_tool(
            tx,
            run_id,
            "web_search",
            json!({ "query": search_query(query) }),
            StepStatus::Failed,
            Some(elapsed_ms(search_started)),
            Some("cancelled".to_owned()),
        );
        fail_research(
            tx,
            run_id,
            "Generation stopped.".to_owned(),
            elapsed_ms(research_started),
        );
        return;
    };

    let hits = match search_result {
        Ok(hits) => {
            send_tool(
                tx,
                run_id,
                "web_search",
                json!({ "query": search_query(query) }),
                StepStatus::Done,
                Some(elapsed_ms(search_started)),
                Some(format!("{} result(s) from the local SearXNG", hits.len())),
            );
            hits
        }
        Err(error) => {
            send_tool(
                tx,
                run_id,
                "web_search",
                json!({ "query": search_query(query) }),
                StepStatus::Failed,
                Some(elapsed_ms(search_started)),
                Some(short(&error)),
            );
            fail_research(tx, run_id, error, elapsed_ms(research_started));
            return;
        }
    };

    if hits.is_empty() {
        fail_research(
            tx,
            run_id,
            "SearXNG returned no usable results. Try a different query or check the instance configuration."
                .to_owned(),
            elapsed_ms(research_started),
        );
        return;
    }

    let fetch_count = hits.len().min(MAX_FETCHED_PAGES);
    let read_started = Instant::now();
    send_tool(
        tx,
        run_id,
        "read_pages",
        json!({ "pages": fetch_count }),
        StepStatus::Started,
        None,
        None,
    );

    let mut tasks = JoinSet::new();
    for (index, hit) in hits.iter().take(fetch_count).enumerate() {
        let reader = reader.clone();
        let url = hit.url.clone();
        let title = hit.title.clone();
        tasks.spawn(async move { (index, fetch_page(&reader, &url, &title).await) });
    }

    let mut pages: Vec<Option<Result<FetchedPage, String>>> =
        (0..fetch_count).map(|_| None).collect();
    loop {
        if tasks.is_empty() {
            break;
        }
        let joined = tokio::select! {
            _ = app_cancel.cancelled() => {
                tasks.abort_all();
                send_tool(
                    tx,
                    run_id,
                    "read_pages",
                    json!({ "pages": fetch_count }),
                    StepStatus::Failed,
                    Some(elapsed_ms(read_started)),
                    Some("cancelled".to_owned()),
                );
                fail_research(
                    tx,
                    run_id,
                    "Generation stopped.".to_owned(),
                    elapsed_ms(research_started),
                );
                return;
            }
            _ = chat_cancel.cancelled() => {
                tasks.abort_all();
                send_tool(
                    tx,
                    run_id,
                    "read_pages",
                    json!({ "pages": fetch_count }),
                    StepStatus::Failed,
                    Some(elapsed_ms(read_started)),
                    Some("cancelled".to_owned()),
                );
                fail_research(
                    tx,
                    run_id,
                    "Generation stopped.".to_owned(),
                    elapsed_ms(research_started),
                );
                return;
            }
            joined = tasks.join_next() => joined,
        };
        let Some(joined) = joined else {
            break;
        };
        if let Ok((index, result)) = joined {
            if let Some(slot) = pages.get_mut(index) {
                *slot = Some(result);
            }
        }
    }

    let mut sources = Vec::new();
    let mut chunks = Vec::new();
    let mut read_count = 0usize;

    for (index, hit) in hits.iter().enumerate() {
        let page = pages
            .get(index)
            .and_then(|slot| slot.as_ref())
            .and_then(|result| result.as_ref().ok());
        let snippet = if !hit.snippet.is_empty() {
            hit.snippet.clone()
        } else {
            page.map(|page| clean_fragment(&page.text, MAX_SOURCE_SNIPPET_CHARS))
                .unwrap_or_default()
        };
        if page.is_none() && snippet.is_empty() {
            continue;
        }

        let id = u32::try_from(sources.len() + 1).unwrap_or(u32::MAX);
        let title = page
            .map(|page| page.title.trim())
            .filter(|title| !title.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| hit.title.clone());
        let url = page
            .map(|page| page.final_url.clone())
            .unwrap_or_else(|| hit.url.clone());
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

    let failed_pages = fetch_count.saturating_sub(read_count);
    let read_summary = if failed_pages == 0 {
        format!("read {read_count} page(s)")
    } else {
        format!("read {read_count}/{fetch_count} pages; used snippets for the rest")
    };
    send_tool(
        tx,
        run_id,
        "read_pages",
        json!({ "pages": fetch_count }),
        StepStatus::Done,
        Some(elapsed_ms(read_started)),
        Some(read_summary),
    );

    if chunks.is_empty() {
        fail_research(
            tx,
            run_id,
            "The search returned no readable page text. Try a different query.".to_owned(),
            elapsed_ms(research_started),
        );
        return;
    }

    let _ = tx.send(AppEvent::Sources {
        run_id: run_id.to_owned(),
        sources: sources.clone(),
    });

    let selected = rank_chunks(query, &chunks, MAX_CONTEXT_CHUNKS);
    let context = build_context(&sources, &chunks, &selected);
    let system_prompt = build_system_prompt(&context, sources.len());
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
    let outcome = stream_chat(
        client,
        run_id,
        model,
        &messages,
        context_length,
        chat_started,
        tx,
        chat_cancel,
        app_cancel,
    )
    .await;
    let chat_duration = elapsed_ms(chat_started);

    match outcome {
        Ok(StreamOutcome::Completed) => {
            send_step(tx, run_id, "chat", StepStatus::Done, Some(chat_duration));
        }
        Ok(StreamOutcome::Cancelled) => {
            let _ = tx.send(AppEvent::Error {
                scope: "ollama".to_owned(),
                message: "Generation stopped.".to_owned(),
                recoverable: true,
            });
            send_step(tx, run_id, "chat", StepStatus::Failed, Some(chat_duration));
        }
        Err(error) => {
            let _ = tx.send(AppEvent::Error {
                scope: "ollama".to_owned(),
                message: error,
                recoverable: true,
            });
            send_step(tx, run_id, "chat", StepStatus::Failed, Some(chat_duration));
        }
    }
}

async fn search_searxng(
    client: &reqwest::Client,
    prompt: &str,
) -> Result<Vec<crate::research::SearchHit>, String> {
    let query = search_query(prompt)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if query.is_empty() {
        return Err("The research query is empty.".to_owned());
    }

    let mut endpoint = reqwest::Url::parse(&format!("{SEARXNG_BASE_URL}/search"))
        .map_err(|_| "The local search endpoint is invalid.".to_owned())?;
    endpoint
        .query_pairs_mut()
        .append_pair("q", &query)
        .append_pair("format", "json")
        .append_pair("categories", "general")
        .append_pair("safesearch", "1");

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
