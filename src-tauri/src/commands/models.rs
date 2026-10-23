//! In-app GGUF downloader.
//!
//! The catalog is a fixed allowlist: the webview names an id, never a URL, so a
//! compromised renderer cannot turn the backend into an arbitrary downloader.
//! Files come from Hugging Face over HTTPS and land in the same `<app
//! data>/models` folder the engine scans, so a finished download appears in the
//! picker without a restart.

use std::path::Path;
use std::time::{Duration, Instant};

use tauri::{AppHandle, State};
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;

use crate::engine::models_dir;
use crate::error::AppError;
use crate::events::{AppEvent, DownloadStatus};
use crate::model_catalog::{
    catalog_entry, catalog_models, download_url, is_allowed_download_host, CatalogEntry,
    CatalogModel,
};
use crate::state::AppState;

fn send_status(
    tx: &broadcast::Sender<AppEvent>,
    id: &str,
    status: DownloadStatus,
    downloaded_bytes: u64,
    total_bytes: u64,
    message: Option<String>,
) {
    let _ = tx.send(AppEvent::ModelDownload {
        id: id.to_owned(),
        status,
        downloaded_bytes,
        total_bytes,
        message,
    });
}

#[tauri::command]
pub fn list_model_catalog(app: AppHandle) -> Result<Vec<CatalogModel>, AppError> {
    let dir = models_dir(&app)?;
    Ok(catalog_models(&dir))
}

/// Download one catalog model. Progress and the terminal state are broadcast as
/// `ModelDownload` events; the returned `AppError` only carries the failure
/// message for the immediate invoke promise.
#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    let entry = catalog_entry(&id)
        .ok_or_else(|| AppError::new("That model is not in the download catalog."))?;
    let dir = models_dir(&app)?;
    let target = dir.join(entry.filename);
    if target.is_file() {
        return Err(AppError::new(format!(
            "{} is already installed.",
            entry.name
        )));
    }

    let cancel = state.claim_download(entry.id)?;
    let outcome = perform_download(entry, &dir, &target, &state.tx, &cancel).await;
    state.release_download(entry.id);

    match outcome {
        Ok(DownloadOutcome::Completed(size)) => {
            send_status(&state.tx, entry.id, DownloadStatus::Done, size, size, None);
            Ok(())
        }
        Ok(DownloadOutcome::Cancelled) => {
            send_status(
                &state.tx,
                entry.id,
                DownloadStatus::Cancelled,
                0,
                entry.size_bytes,
                None,
            );
            Ok(())
        }
        Err(message) => {
            send_status(
                &state.tx,
                entry.id,
                DownloadStatus::Failed,
                0,
                entry.size_bytes,
                Some(message.clone()),
            );
            Err(AppError::new(message))
        }
    }
}

#[tauri::command]
pub fn cancel_model_download(state: State<'_, AppState>) -> bool {
    state.cancel_download()
}

enum DownloadOutcome {
    Completed(u64),
    Cancelled,
}

/// A partial file is never left behind: every error path removes it, and a
/// cancelled run removes it too. A fresh request starts from zero rather than
/// trusting bytes from an interrupted version of a possibly updated file.
async fn remove_partial(part: &Path, message: String) -> String {
    let _ = tokio::fs::remove_file(part).await;
    message
}

async fn perform_download(
    entry: &CatalogEntry,
    dir: &Path,
    target: &Path,
    tx: &broadcast::Sender<AppEvent>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<DownloadOutcome, String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let allowed_host = attempt
                .url()
                .host_str()
                .is_some_and(is_allowed_download_host);
            if attempt.previous().len() >= 5 {
                attempt.error("too many redirects")
            } else if attempt.url().scheme() == "https" && allowed_host {
                attempt.follow()
            } else {
                attempt.error("redirect outside the download allowlist")
            }
        }))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .user_agent("LocalResearchAgent/0.1 (+local model downloader)")
        .build()
        .map_err(|error| {
            format!(
                "The downloader could not start ({}).",
                short_error(&error.to_string())
            )
        })?;

    let url = download_url(entry);
    let response = tokio::select! {
        _ = cancel.cancelled() => return Ok(DownloadOutcome::Cancelled),
        result = client.get(&url).send() => result,
    }
    .map_err(|error| {
        format!(
            "The model could not be downloaded ({}).",
            short_error(&error.to_string())
        )
    })?;

    if !response.status().is_success() {
        return Err(format!(
            "Hugging Face returned HTTP {} for this model.",
            response.status().as_u16()
        ));
    }
    let total = response.content_length().unwrap_or(entry.size_bytes);

    let part = dir.join(format!(".{}.part", entry.filename));
    let _ = tokio::fs::remove_file(&part).await;
    let mut file = tokio::fs::File::create(&part).await.map_err(|error| {
        format!(
            "The model file could not be created ({}).",
            short_error(&error.to_string())
        )
    })?;

    send_status(tx, entry.id, DownloadStatus::Started, 0, total, None);

    let mut response = response;
    let mut downloaded: u64 = 0;
    let mut last_emit = Instant::now();
    let mut last_percent: u64 = 0;

    loop {
        let next = tokio::select! {
            _ = cancel.cancelled() => {
                drop(file);
                let _ = tokio::fs::remove_file(&part).await;
                return Ok(DownloadOutcome::Cancelled);
            }
            chunk = response.chunk() => chunk,
        };

        let chunk = match next {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => {
                return Err(remove_partial(
                    &part,
                    format!(
                        "The download was interrupted ({}).",
                        short_error(&error.to_string())
                    ),
                )
                .await)
            }
        };

        if let Err(error) = file.write_all(&chunk).await {
            return Err(remove_partial(
                &part,
                format!(
                    "The model file could not be written ({}).",
                    short_error(&error.to_string())
                ),
            )
            .await);
        }

        downloaded = downloaded.saturating_add(chunk.len() as u64);
        let percent = (downloaded.saturating_mul(100))
            .checked_div(total)
            .unwrap_or(0)
            .min(100);
        if percent > last_percent || last_emit.elapsed() >= Duration::from_millis(250) {
            send_status(
                tx,
                entry.id,
                DownloadStatus::Progress,
                downloaded,
                total,
                None,
            );
            last_percent = percent;
            last_emit = Instant::now();
        }
    }

    if let Err(error) = file.flush().await {
        return Err(remove_partial(
            &part,
            format!(
                "The model file could not be flushed ({}).",
                short_error(&error.to_string())
            ),
        )
        .await);
    }
    drop(file);

    if downloaded == 0 || (total > 0 && downloaded != total) {
        return Err(remove_partial(
            &part,
            format!("The download ended early ({downloaded} of {total} bytes)."),
        )
        .await);
    }

    if let Err(error) = tokio::fs::rename(&part, target).await {
        return Err(remove_partial(
            &part,
            format!(
                "The model file could not be finalized ({}).",
                short_error(&error.to_string())
            ),
        )
        .await);
    }

    Ok(DownloadOutcome::Completed(downloaded))
}

/// Repository constants are short, but OS errors can be long. Keep command
/// messages readable in the UI.
fn short_error(message: &str) -> String {
    const LIMIT: usize = 160;
    let trimmed = message.trim();
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_owned();
    }
    let mut shortened: String = trimmed.chars().take(LIMIT).collect();
    shortened.push('…');
    shortened
}
