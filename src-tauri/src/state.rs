use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(debug_assertions)]
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::config::EVENT_CHANNEL_CAPACITY;
use crate::error::AppError;
use crate::events::AppEvent;

struct ActiveChat {
    run_id: String,
    cancel: CancellationToken,
}

/// Process-wide application state managed by Tauri.
pub struct AppState {
    /// Fan-out bus; every subscriber sees every event.
    pub tx: broadcast::Sender<AppEvent>,
    /// Cancels telemetry, chat and (in dev) the debug stream when the app exits.
    pub cancel: CancellationToken,
    /// Loopback-only HTTP client for Ollama and SearXNG. `None` means the
    /// platform HTTP stack could not initialise; those commands then fail
    /// closed instead of panicking.
    pub http: Option<reqwest::Client>,
    /// User-initiated research reader. Redirects are handled manually so every
    /// hop can be revalidated, and proxies are never used.
    pub research_http: Option<reqwest::Client>,
    chat: Mutex<Option<ActiveChat>>,
    subscribed: AtomicBool,
    #[cfg(debug_assertions)]
    debug_active: Arc<AtomicBool>,
}

impl AppState {
    pub fn new() -> Self {
        let (tx, _keep_alive) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let http = reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(5))
            .build()
            .ok();
        let research_http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(6))
            .timeout(Duration::from_secs(25))
            .user_agent("LocalResearchAgent/0.1 (+local research reader)")
            .build()
            .ok();
        Self {
            tx,
            cancel: CancellationToken::new(),
            http,
            research_http,
            chat: Mutex::new(None),
            subscribed: AtomicBool::new(false),
            #[cfg(debug_assertions)]
            debug_active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// True exactly once. The forwarder is a process-wide singleton because a
    /// second one would duplicate every event in the webview.
    pub fn claim_forwarder(&self) -> bool {
        !self.subscribed.swap(true, Ordering::SeqCst)
    }

    /// Reserve the single chat slot and return its cancellation token. A second
    /// concurrent chat is rejected so two local models cannot compete for VRAM.
    pub fn claim_chat(&self, run_id: &str) -> Result<CancellationToken, AppError> {
        let mut slot = self
            .chat
            .lock()
            .map_err(|_| AppError::new("chat state is unavailable"))?;
        if slot.is_some() {
            return Err(AppError::new("A local chat is already running."));
        }

        let cancel = CancellationToken::new();
        *slot = Some(ActiveChat {
            run_id: run_id.to_owned(),
            cancel: cancel.clone(),
        });
        Ok(cancel)
    }

    /// Release the slot only if it still belongs to this run. A late finishing
    /// task must never clear a newer chat.
    pub fn release_chat(&self, run_id: &str) {
        if let Ok(mut slot) = self.chat.lock() {
            if slot.as_ref().is_some_and(|active| active.run_id == run_id) {
                *slot = None;
            }
        }
    }

    /// Cancel the active chat, if any. Idempotent: repeat calls return false.
    pub fn cancel_chat(&self) -> bool {
        match self.chat.lock() {
            Ok(slot) => match slot.as_ref() {
                Some(active) => {
                    active.cancel.cancel();
                    true
                }
                None => false,
            },
            Err(_) => false,
        }
    }

    #[cfg(debug_assertions)]
    pub fn try_start_debug_stream(&self) -> Option<Arc<AtomicBool>> {
        if self.debug_active.swap(true, Ordering::SeqCst) {
            return None;
        }
        Some(Arc::clone(&self.debug_active))
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
