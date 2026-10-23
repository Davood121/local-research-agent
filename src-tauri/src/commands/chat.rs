//! Local-only Ollama commands.
//!
//! The endpoint is a compile-time constant. The webview can choose an installed
//! model name and provide conversation text, but it cannot choose a host, port,
//! proxy, header, or remote model. Cloud-routed Ollama models are filtered out
//! of both the model list and the chat path.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::error::AppError;
use crate::events::{AppEvent, StepStatus};
use crate::ollama::{
    context_length_for, is_local_model, local_models, validate_chat_request,
    with_chat_system_prompt, ChatChunk, ChatMessage, ErrorResponse, NdjsonBuffer, OllamaStatus,
    StartChatRequest, TagsResponse, OLLAMA_BASE_URL,
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
pub async fn ollama_status(state: State<'_, AppState>) -> Result<OllamaStatus, AppError> {
    let Some(client) = state.http.as_ref() else {
        return Ok(OllamaStatus {
            connected: false,
            version: None,
            models: Vec::new(),
            error: Some("The local HTTP client could not be initialised.".to_owned()),
        });
    };

    match fetch_tags(client).await {
        Ok(tags) => {
            let version = fetch_version(client).await;
            Ok(OllamaStatus {
                connected: true,
                version,
                models: local_models(tags),
                error: None,
            })
        }
        Err(error) => Ok(OllamaStatus {
            connected: false,
            version: None,
            models: Vec::new(),
            error: Some(error),
        }),
    }
}

#[tauri::command]
pub async fn start_chat(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartChatRequest,
) -> Result<(), AppError> {
    validate_chat_request(&request).map_err(AppError::new)?;

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
