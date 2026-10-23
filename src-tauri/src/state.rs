use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(debug_assertions)]
use std::sync::Arc;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::config::EVENT_CHANNEL_CAPACITY;
use crate::engine::{InferenceDevice, LoadedEngineModel};
use crate::error::AppError;
use crate::events::AppEvent;

struct ActiveChat {
    run_id: String,
    cancel: CancellationToken,
}

struct ActiveDownload {
    id: String,
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
    /// Models resident in the embedded llama.cpp engine. At most one model is
    /// held at a time: loading a new one replaces the old weights.
    pub engine: Mutex<Option<LoadedEngineModel>>,
    /// Preference for the next model load. Persisted by the engine commands.
    device: Mutex<InferenceDevice>,
    chat: Mutex<Option<ActiveChat>>,
    download: Mutex<Option<ActiveDownload>>,
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
            engine: Mutex::new(None),
            device: Mutex::new(InferenceDevice::Cpu),
            chat: Mutex::new(None),
            download: Mutex::new(None),
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

    /// Access the embedded engine's resident-model slot. A poisoned lock means
    /// the process is shutting down mid-load; commands fail closed.
    pub fn engine_lock(&self) -> Result<MutexGuard<'_, Option<LoadedEngineModel>>, AppError> {
        self.engine
            .lock()
            .map_err(|_| AppError::new("the embedded engine state is unavailable"))
    }

    /// Current inference-device preference.
    pub fn inference_device(&self) -> Result<InferenceDevice, AppError> {
        self.device
            .lock()
            .map(|device| *device)
            .map_err(|_| AppError::new("the inference-device setting is unavailable"))
    }

    /// Replace the device preference. The caller unloads resident weights when
    /// the device actually changes, because offload is fixed at load time.
    pub fn set_inference_device(&self, device: InferenceDevice) -> Result<(), AppError> {
        let mut slot = self
            .device
            .lock()
            .map_err(|_| AppError::new("the inference-device setting is unavailable"))?;
        *slot = device;
        Ok(())
    }

    /// Change the device preference only while no chat is active. The chat lock
    /// keeps a new run from starting between the idle check and model unload.
    pub fn change_inference_device(
        &self,
        device: InferenceDevice,
        persist: impl FnOnce() -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        let chat = self
            .chat
            .lock()
            .map_err(|_| AppError::new("chat state is unavailable"))?;
        if chat.is_some() {
            return Err(AppError::new(
                "Wait for the active chat to finish before changing the inference device.",
            ));
        }

        let mut current = self
            .device
            .lock()
            .map_err(|_| AppError::new("the inference-device setting is unavailable"))?;
        if *current == device {
            return Ok(());
        }

        let mut engine = self.engine_lock()?;
        persist()?;
        *current = device;
        *engine = None;
        Ok(())
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

    /// Reserve the single model-download slot. One download at a time keeps
    /// request counts and disk usage predictable.
    pub fn claim_download(&self, id: &str) -> Result<CancellationToken, AppError> {
        let mut slot = self
            .download
            .lock()
            .map_err(|_| AppError::new("The download state is unavailable."))?;
        if slot.is_some() {
            return Err(AppError::new("Another model download is already running."));
        }

        let cancel = CancellationToken::new();
        *slot = Some(ActiveDownload {
            id: id.to_owned(),
            cancel: cancel.clone(),
        });
        Ok(cancel)
    }

    /// Release the download slot only if it still belongs to this id.
    pub fn release_download(&self, id: &str) {
        if let Ok(mut slot) = self.download.lock() {
            if slot.as_ref().is_some_and(|active| active.id == id) {
                *slot = None;
            }
        }
    }

    /// Cancel the active download, if any. Idempotent.
    pub fn cancel_download(&self) -> bool {
        match self.download.lock() {
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
