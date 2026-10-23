use tauri::{Emitter, State, WebviewWindow};
use tokio::sync::broadcast::error::RecvError;

use crate::config::APP_EVENT_NAME;
use crate::error::AppError;
use crate::events::AppEvent;
use crate::state::AppState;

/// Installs the one-way bridge from the Rust broadcast bus to the webview.
///
/// Idempotent: only the first call owns the forwarder. A second call (React
/// StrictMode, a reload) returns immediately instead of duplicating events.
#[tauri::command]
pub async fn subscribe_events(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    if !state.claim_forwarder() {
        return Ok(());
    }

    let mut receiver = state.tx.subscribe();
    let cancel = state.cancel.clone();

    tauri::async_runtime::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                message = receiver.recv() => match message {
                    Ok(event) => {
                        // A closed webview means nobody is left to notify.
                        if window.emit(APP_EVENT_NAME, event).is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        let notice = AppEvent::Error {
                            scope: "event_bus".to_owned(),
                            message: format!("dropped {skipped} events because the UI fell behind"),
                            recoverable: true,
                        };
                        if window.emit(APP_EVENT_NAME, notice).is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Closed) => break,
                },
            }
        }
    });

    Ok(())
}
