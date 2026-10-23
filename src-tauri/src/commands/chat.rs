//! Local-only chat commands.
//!
//! Two inference paths share one command surface and one event stream:
//! - the embedded llama.cpp engine (`crate::engine`), fed GGUF files from the
//!   app-data models directory, and
//! - an externally running Ollama server on the fixed loopback endpoint.
//!
//! The engine path never touches HTTP. The webview can choose an installed
//! model name and provide conversation text, but it cannot choose a host, port,
//! proxy, header, or remote model. Cloud-routed Ollama models are filtered out
//! of both the model list and the chat path.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::engine::{self, GenerationOutcome, LoadedEngineModel};
use crate::error::AppError;
use crate::events::{AppEvent, StepStatus};
use crate::ollama::{
    context_length_for, is_local_model, local_models, validate_chat_request,
    with_chat_system_prompt, ChatChunk, ChatMessage, ErrorResponse, NdjsonBuffer, OllamaModel,
    OllamaStatus, StartChatRequest, TagsResponse, OLLAMA_BASE_URL,
};
use crate::state::AppState;

const STATUS_TIMEOUT: Duration = Duration::from_secs(3);
const VERSION_TIMEOUT: Duration = Duration::from_secs(2);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(120);
const ERROR_BODY_LIMIT: usize = 4_096;
const MAX_ERROR_CHARS: usize = 400;

#[derive(Debug, Deserialize)]
struct VersionResponse {
    #[serde(default)]
    version: String,
}

#[derive(Serialize)]
struct ChatPayload<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    think: bool,
}

pub(crate) enum StreamOutcome {
    Completed,
    Cancelled,
}

#[tauri::command]
pub async fn ollama_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<OllamaStatus, AppError> {
    // Embedded engine GGUFs are first-class citizens in the model list: users
    // should not need a running Ollama just to see the models this app can run
    // by itself. An Ollama model with the same name wins so the two paths stay
    // unambiguous for the `start_chat` router.
    let mut engine_models = Vec::new();
    if let Ok(dir) = engine::models_dir(&app) {
        engine_models = engine::scan_model_files(&dir);
    }

    let mut status = match state.http.as_ref() {
        Some(client) => match fetch_tags(client).await {
            Ok(tags) => {
                let version = fetch_version(client).await;
                OllamaStatus {
                    connected: true,
                    version,
                    models: local_models(tags),
                    error: None,
                }
            }
            Err(error) => OllamaStatus {
                connected: false,
                version: None,
                models: Vec::new(),
                error: Some(error),
            },
        },
        None => OllamaStatus {
            connected: false,
            version: None,
            models: Vec::new(),
            error: Some("The local HTTP client could not be initialised.".to_owned()),
        },
    };

    for file in &engine_models {
        if status.models.iter().any(|model| model.name == file.name) {
            continue;
        }
        status.models.push(OllamaModel {
            name: file.name.clone(),
            size_bytes: file.size_bytes,
            parameter_size: String::new(),
            quantization: String::new(),
            family: "embedded gguf".to_owned(),
            context_length: None,
        });
    }
    if !engine_models.is_empty() {
        // The embedded engine can serve every chat by itself, so having engine
        // models alone puts the app in the "connected" phase: local inference
        // is available without any server. The Ollama error text still tells
        // the user why Ollama itself is idle; it is not a blocker.
        status.connected = true;
    }
    status
        .models
        .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(status)
}

#[tauri::command]
pub async fn start_chat(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartChatRequest,
) -> Result<(), AppError> {
    validate_chat_request(&request).map_err(AppError::new)?;

    // The embedded engine owns any chat whose model name matches a GGUF in the
    // app-data models directory. This is a fresh scan, never a webview-supplied
    // path; the name only ever becomes a file inside that directory.
    let engine_path = engine::models_dir(&app)
        .ok()
        .and_then(|dir| engine::find_model_file(&dir, &request.model));

    if let Some(model_path) = engine_path {
        return run_engine_chat(app, state, request, model_path).await;
    }

    // --- Ollama fallback ----------------------------------------------------
    let Some(client) = state.http.as_ref().cloned() else {
        return Err(AppError::new(
            "The local HTTP client could not be initialised.",
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
    let messages = with_chat_system_prompt(request.messages);

    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        let _ = tx.send(AppEvent::Step {
            run_id: run_id.clone(),
            node: "chat".to_owned(),
            status: StepStatus::Started,
            duration_ms: None,
        });

        let outcome = stream_chat(
            &client,
            &run_id,
            &model,
            &messages,
            context_length,
            started,
            &tx,
            &chat_cancel,
            &app_cancel,
            None,
        )
        .await;
        let duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);

        match outcome {
            Ok(StreamOutcome::Completed) => {
                let _ = tx.send(AppEvent::Step {
                    run_id: run_id.clone(),
                    node: "chat".to_owned(),
                    status: StepStatus::Done,
                    duration_ms: Some(duration_ms),
                });
            }
            Ok(StreamOutcome::Cancelled) => {
                let _ = tx.send(AppEvent::Error {
                    scope: "ollama".to_owned(),
                    message: "Generation stopped.".to_owned(),
                    recoverable: true,
                });
                let _ = tx.send(AppEvent::Step {
                    run_id: run_id.clone(),
                    node: "chat".to_owned(),
                    status: StepStatus::Failed,
                    duration_ms: Some(duration_ms),
                });
            }
            Err(error) => {
                let _ = tx.send(AppEvent::Error {
                    scope: "ollama".to_owned(),
                    message: error,
                    recoverable: true,
                });
                let _ = tx.send(AppEvent::Step {
                    run_id: run_id.clone(),
                    node: "chat".to_owned(),
                    status: StepStatus::Failed,
                    duration_ms: Some(duration_ms),
                });
            }
        }

        app.state::<AppState>().release_chat(&run_id);
    });

    Ok(())
}

/// Emit the terminal `Step` (and any recoverable notice) for the engine path.
fn finish_engine_step(
    tx: &broadcast::Sender<AppEvent>,
    run_id: &str,
    status: StepStatus,
    duration_ms: u32,
    notice: Option<&str>,
) {
    let _ = tx.send(AppEvent::Step {
        run_id: run_id.to_owned(),
        node: "chat".to_owned(),
        status,
        duration_ms: Some(duration_ms),
    });
    if let Some(message) = notice {
        let _ = tx.send(AppEvent::Error {
            scope: "chat".to_owned(),
            message: message.to_owned(),
            recoverable: true,
        });
    }
}

/// Resolve engine weights: reuse what is resident when the name matches, else
/// load the GGUF (dropping any other resident model first). Blocking; call it
/// from a worker thread.
fn take_engine_model(
    app: &AppHandle,
    name: &str,
    path: &std::path::Path,
) -> Result<crate::engine::LlamaModelHandle, AppError> {
    let state = app.state::<AppState>();
    let device = state.inference_device()?;
    {
        let slot = state.engine_lock()?;
        if let Some(loaded) = slot.as_ref() {
            if loaded.name == name && loaded.device == device {
                return Ok(loaded.model.clone());
            }
        }
    }

    // The previous weights are dropped before the new model loads, so at most
    // one model stays in memory — that matters on 8 GB laptops and on a 6 GB
    // GPU where the old and new weights would otherwise overlap in VRAM.
    {
        let mut slot = state.engine_lock()?;
        *slot = None;
    }

    let model = engine::load_model(path, name, device)?;
    {
        let mut slot = state.engine_lock()?;
        *slot = Some(LoadedEngineModel {
            name: name.to_owned(),
            path: path.to_path_buf(),
            model: model.clone(),
            device,
        });
    }
    Ok(model)
}

/// The command-side half of the engine path: identical event shape to the
/// Ollama path (Step chat started/done, Token, Metrics) but no HTTP anywhere.
async fn run_engine_chat(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartChatRequest,
    model_path: PathBuf,
) -> Result<(), AppError> {
    let chat_cancel = state.claim_chat(&request.run_id)?;
    let tx = state.tx.clone();
    let run_id = request.run_id;
    let model_name = request.model;
    let messages = Arc::new(with_chat_system_prompt(request.messages));
    // App exit must interrupt generation too; the chat token can't be cancelled
    // by app shutdown directly, so a watcher mirrors it into a child token.
    let gen_cancel = chat_cancel.child_token();
    let app_cancel = state.cancel.clone();

    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        let _ = tx.send(AppEvent::Step {
            run_id: run_id.clone(),
            node: "chat".to_owned(),
            status: StepStatus::Started,
            duration_ms: None,
        });

        {
            let mirror_cancel = gen_cancel.clone();
            tokio::spawn(async move {
                app_cancel.cancelled().await;
                mirror_cancel.cancel();
            });
        }

        let resolve_app = app.clone();
        let resolve_name = model_name.clone();
        let resolved = tauri::async_runtime::spawn_blocking(move || {
            take_engine_model(&resolve_app, &resolve_name, &model_path)
        })
        .await;

        let model = match resolved {
            Ok(Ok(model)) => model,
            Ok(Err(error)) => {
                let duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
                finish_engine_step(
                    &tx,
                    &run_id,
                    StepStatus::Failed,
                    duration_ms,
                    Some(&error.message),
                );
                app.state::<AppState>().release_chat(&run_id);
                return;
            }
            Err(_) => {
                let duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
                finish_engine_step(
                    &tx,
                    &run_id,
                    StepStatus::Failed,
                    duration_ms,
                    Some("The local inference task could not be started."),
                );
                app.state::<AppState>().release_chat(&run_id);
                return;
            }
        };

        let outcome = run_engine_generation(&tx, &run_id, model, messages, gen_cancel, None).await;
        let duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);

        match outcome {
            Ok(GenerationOutcome::Completed {
                prompt_tokens,
                gen_tokens,
                tok_per_sec,
                ttft_ms,
                ctx_used_pct,
            }) => {
                let _ = tx.send(AppEvent::Metrics {
                    run_id: run_id.clone(),
                    ttft_ms,
                    tok_per_sec,
                    prompt_tokens,
                    gen_tokens,
                    ctx_used_pct,
                });
                finish_engine_step(&tx, &run_id, StepStatus::Done, duration_ms, None);
            }
            Ok(GenerationOutcome::Cancelled) => {
                finish_engine_step(
                    &tx,
                    &run_id,
                    StepStatus::Failed,
                    duration_ms,
                    Some("Generation stopped."),
                );
            }
            Ok(GenerationOutcome::Failed(message)) => {
                finish_engine_step(
                    &tx,
                    &run_id,
                    StepStatus::Failed,
                    duration_ms,
                    Some(&message),
                );
            }
            Err(message) => {
                finish_engine_step(
                    &tx,
                    &run_id,
                    StepStatus::Failed,
                    duration_ms,
                    Some(&message),
                );
            }
        }

        app.state::<AppState>().release_chat(&run_id);
    });

    Ok(())
}

/// Append a streamed fragment to the optional answer sink.
///
/// Research uses the sink to check whether the model cited anything; chat
/// passes `None` and pays nothing.
fn remember_fragment(answer: Option<&Arc<Mutex<String>>>, delta: &str) {
    if let Some(sink) = answer {
        match sink.lock() {
            Ok(mut text) => text.push_str(delta),
            Err(poisoned) => poisoned.into_inner().push_str(delta),
        }
    }
}

/// The blocking half of the engine chat: streams tokens into the event bus.
async fn run_engine_generation(
    tx: &broadcast::Sender<AppEvent>,
    run_id: &str,
    model: crate::engine::LlamaModelHandle,
    messages: Arc<Vec<ChatMessage>>,
    gen_cancel: CancellationToken,
    answer: Option<Arc<Mutex<String>>>,
) -> Result<GenerationOutcome, String> {
    let stream_tx = tx.clone();
    let stream_run_id = run_id.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let mut token_index: u32 = 0;
        let mut on_token = |delta: &str| {
            remember_fragment(answer.as_ref(), delta);
            let _ = stream_tx.send(AppEvent::Token {
                run_id: stream_run_id.clone(),
                delta: delta.to_owned(),
                index: token_index,
            });
            token_index = token_index.saturating_add(1);
        };
        engine::generate_stream(model, messages.as_slice(), &gen_cancel, &mut on_token)
    })
    .await
    .map_err(|_| "The local generation task was lost.".to_owned())
}

/// Engine counterpart of [`stream_chat`] for callers that already own the chat
/// step (research runs): tokens and metrics go onto the same event bus, but no
/// step or run bookkeeping is emitted here beyond honouring `gen_cancel`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn stream_engine_chat(
    app: &AppHandle,
    tx: &broadcast::Sender<AppEvent>,
    run_id: &str,
    model_name: &str,
    model_path: &std::path::Path,
    messages: &[ChatMessage],
    gen_cancel: CancellationToken,
    answer: Option<&Arc<Mutex<String>>>,
) -> Result<StreamOutcome, String> {
    let resolve_app = app.clone();
    let resolve_name = model_name.to_owned();
    let resolve_path = model_path.to_path_buf();
    let model = tauri::async_runtime::spawn_blocking(move || {
        take_engine_model(&resolve_app, &resolve_name, &resolve_path)
    })
    .await
    .map_err(|_| "The local inference task could not be started.".to_owned())?
    .map_err(|error| error.message)?;

    let messages = Arc::new(messages.to_vec());
    match run_engine_generation(tx, run_id, model, messages, gen_cancel, answer.cloned()).await {
        Ok(GenerationOutcome::Completed {
            prompt_tokens,
            gen_tokens,
            tok_per_sec,
            ttft_ms,
            ctx_used_pct,
        }) => {
            let _ = tx.send(AppEvent::Metrics {
                run_id: run_id.to_owned(),
                ttft_ms,
                tok_per_sec,
                prompt_tokens,
                gen_tokens,
                ctx_used_pct,
            });
            Ok(StreamOutcome::Completed)
        }
        Ok(GenerationOutcome::Cancelled) => Ok(StreamOutcome::Cancelled),
        Ok(GenerationOutcome::Failed(message)) => Err(message),
        Err(message) => Err(message),
    }
}

#[tauri::command]
pub async fn cancel_chat(state: State<'_, AppState>) -> Result<(), AppError> {
    if state.cancel_chat() {
        Ok(())
    } else {
        Err(AppError::new("No local chat is running."))
    }
}

pub(crate) async fn fetch_tags(client: &reqwest::Client) -> Result<TagsResponse, String> {
    let url = format!("{OLLAMA_BASE_URL}/api/tags");
    let response = tokio::time::timeout(STATUS_TIMEOUT, client.get(url).send())
        .await
        .map_err(|_| "Ollama did not answer the status request in time.".to_owned())?
        .map_err(|error| {
            format!(
                "Ollama is not reachable at 127.0.0.1:11434 ({}).",
                short(&error.to_string())
            )
        })?;

    let status = response.status();
    if !status.is_success() {
        let detail = read_error_body(response).await;
        return Err(format!(
            "Ollama rejected the status request with HTTP {status}{}.",
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        ));
    }

    response
        .json::<TagsResponse>()
        .await
        .map_err(|_| "Ollama returned an unreadable model list.".to_owned())
}

async fn fetch_version(client: &reqwest::Client) -> Option<String> {
    let url = format!("{OLLAMA_BASE_URL}/api/version");
    let response = tokio::time::timeout(VERSION_TIMEOUT, client.get(url).send())
        .await
        .ok()?
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let version = response.json::<VersionResponse>().await.ok()?;
    if version.version.is_empty() {
        None
    } else {
        Some(version.version)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn stream_chat(
    client: &reqwest::Client,
    run_id: &str,
    model: &str,
    messages: &[ChatMessage],
    context_length: Option<u64>,
    started: Instant,
    tx: &broadcast::Sender<AppEvent>,
    chat_cancel: &CancellationToken,
    app_cancel: &CancellationToken,
    answer: Option<&Arc<Mutex<String>>>,
) -> Result<StreamOutcome, String> {
    let url = format!("{OLLAMA_BASE_URL}/api/chat");
    let payload = ChatPayload {
        model,
        messages,
        stream: true,
        think: false,
    };
    let request = client.post(url).json(&payload);

    let mut response = tokio::select! {
        _ = app_cancel.cancelled() => return Ok(StreamOutcome::Cancelled),
        _ = chat_cancel.cancelled() => return Ok(StreamOutcome::Cancelled),
        response = tokio::time::timeout(RESPONSE_TIMEOUT, request.send()) => {
            match response {
                Err(_) => return Err("Ollama took too long to start responding.".to_owned()),
                Ok(Err(error)) => {
                    return Err(format!(
                        "The local chat request failed ({}).",
                        short(&error.to_string())
                    ));
                }
                Ok(Ok(response)) => response,
            }
        }
    };

    let status = response.status();
    if !status.is_success() {
        let detail = read_error_body(response).await;
        return Err(if detail.is_empty() {
            format!("Ollama rejected the chat request with HTTP {status}.")
        } else {
            format!("Ollama rejected the chat request with HTTP {status}: {detail}")
        });
    }

    let mut parser = NdjsonBuffer::default();
    let mut index = 0u32;
    let mut first_token_at: Option<Instant> = None;
    let mut done_chunk: Option<ChatChunk> = None;

    'stream: loop {
        let chunk = tokio::select! {
            _ = app_cancel.cancelled() => return Ok(StreamOutcome::Cancelled),
            _ = chat_cancel.cancelled() => return Ok(StreamOutcome::Cancelled),
            chunk = response.chunk() => chunk,
        };

        match chunk {
            Ok(Some(bytes)) => {
                let chunks = parser.push(&bytes).map_err(|error| short(&error))?;
                for chunk in chunks {
                    if let Some(error) = chunk.error.as_deref() {
                        return Err(short(error));
                    }
                    if let Some(message) = chunk.message.as_ref() {
                        if !message.content.is_empty() {
                            first_token_at.get_or_insert_with(Instant::now);
                            remember_fragment(answer, &message.content);
                            let _ = tx.send(AppEvent::Token {
                                run_id: run_id.to_owned(),
                                delta: message.content.clone(),
                                index,
                            });
                            index = index.saturating_add(1);
                        }
                    }
                    if chunk.done {
                        done_chunk = Some(chunk);
                        break 'stream;
                    }
                }
            }
            Ok(None) => break,
            Err(error) => {
                return Err(format!(
                    "The local response stream was interrupted ({}).",
                    short(&error.to_string())
                ));
            }
        }
    }

    if done_chunk.is_none() {
        let chunks = parser.finish().map_err(|error| short(&error))?;
        for chunk in chunks {
            if let Some(error) = chunk.error.as_deref() {
                return Err(short(error));
            }
            if let Some(message) = chunk.message.as_ref() {
                if !message.content.is_empty() {
                    first_token_at.get_or_insert_with(Instant::now);
                    remember_fragment(answer, &message.content);
                    let _ = tx.send(AppEvent::Token {
                        run_id: run_id.to_owned(),
                        delta: message.content.clone(),
                        index,
                    });
                    index = index.saturating_add(1);
                }
            }
            if chunk.done {
                done_chunk = Some(chunk);
                break;
            }
        }
    }

    let Some(done) = done_chunk else {
        return Err("Ollama closed the stream before finishing the answer.".to_owned());
    };

    let prompt_tokens = done.prompt_eval_count.unwrap_or(0);
    let gen_tokens = done.eval_count.unwrap_or(index);
    let tok_per_sec = match done.eval_duration {
        Some(nanos) if nanos > 0 => {
            (f64::from(gen_tokens) / (nanos as f64 / 1_000_000_000.0)) as f32
        }
        _ => {
            let seconds = first_token_at
                .map(|first| first.elapsed().as_secs_f64())
                .unwrap_or(0.0);
            if seconds > 0.0 {
                gen_tokens as f32 / seconds as f32
            } else {
                0.0
            }
        }
    };
    let ctx_used_pct = match context_length {
        Some(context) if context > 0 => {
            (prompt_tokens.saturating_add(gen_tokens) as f32 / context as f32 * 100.0)
                .clamp(0.0, 100.0)
        }
        _ => 0.0,
    };
    let ttft_ms = first_token_at
        .and_then(|first| u32::try_from(first.saturating_duration_since(started).as_millis()).ok())
        .unwrap_or(0);

    let _ = tx.send(AppEvent::Metrics {
        run_id: run_id.to_owned(),
        ttft_ms,
        tok_per_sec,
        prompt_tokens,
        gen_tokens,
        ctx_used_pct,
    });

    Ok(StreamOutcome::Completed)
}

pub(crate) async fn read_error_body(mut response: reqwest::Response) -> String {
    let mut body = Vec::new();
    while body.len() < ERROR_BODY_LIMIT {
        match tokio::time::timeout(Duration::from_secs(2), response.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                body.extend_from_slice(&chunk);
                if body.len() >= ERROR_BODY_LIMIT {
                    break;
                }
            }
            _ => break,
        }
    }

    let text = String::from_utf8_lossy(&body);
    let text = text.trim();
    if let Ok(parsed) = serde_json::from_str::<ErrorResponse>(text) {
        if !parsed.error.trim().is_empty() {
            return short(&parsed.error);
        }
    }
    short(text)
}

pub(crate) fn short(message: &str) -> String {
    let sanitized = message
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .take(MAX_ERROR_CHARS)
        .collect::<String>();
    sanitized.trim().to_owned()
}
