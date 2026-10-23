use std::time::Duration;

#[cfg(not(test))]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(not(test))]
use tauri::{AppHandle, Manager};
#[cfg(not(test))]
use tokio::sync::broadcast::Sender;
#[cfg(not(test))]
use tokio_util::sync::CancellationToken;

use crate::config::{TELEMETRY_GENERATING_MS, TELEMETRY_IDLE_MS, TELEMETRY_MODEL_LOADED_MS};
#[cfg(not(test))]
use crate::events::AppEvent;

pub mod format;
pub mod gpu;
pub mod sys;

#[cfg(not(test))]
use gpu::GpuReader;
#[cfg(not(test))]
use sys::SysSampler;

/// Stub until the model manager lands. Used only to pick a cadence.
pub fn is_model_loaded() -> bool {
    false
}

/// Stub until generation exists. Used only to pick a cadence.
pub fn is_generating() -> bool {
    false
}

pub fn poll_interval() -> Duration {
    if is_generating() {
        Duration::from_millis(TELEMETRY_GENERATING_MS)
    } else if is_model_loaded() {
        Duration::from_millis(TELEMETRY_MODEL_LOADED_MS)
    } else {
        Duration::from_millis(TELEMETRY_IDLE_MS)
    }
}

/// Starts the single telemetry task. The task is cancellable and never panics:
/// every failure degrades to a zero/unavailable field instead of aborting.
#[cfg(not(test))]
pub fn spawn(app: AppHandle, tx: Sender<AppEvent>, cancel: CancellationToken) {
    tauri::async_runtime::spawn(async move {
        run(app, tx, cancel).await;
    });
}

#[cfg(not(test))]
async fn run(app: AppHandle, tx: Sender<AppEvent>, cancel: CancellationToken) {
    let data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    if let Err(error) = std::fs::create_dir_all(&data_dir) {
        // Disk telemetry needs a real path; without one we report 0.0 free.
        eprintln!("telemetry: could not create app data dir: {error}");
    }

    let mut sampler = SysSampler::new();
    let gpu = GpuReader::detect().await;
    eprintln!("telemetry: gpu backend = {}", gpu.label());

    // Give the webview a moment to install the forwarder so the first sample is
    // not lost between window creation and subscribe_events.
    tokio::select! {
        _ = cancel.cancelled() => return,
        _ = tokio::time::sleep(Duration::from_millis(500)) => {}
    }

    loop {
        let snapshot = sampler.sample(&data_dir);
        let gpu_stat = gpu.read((snapshot.ram_used_gb, snapshot.ram_total_gb));

        let _ = tx.send(AppEvent::Telemetry {
            ram_used_gb: snapshot.ram_used_gb,
            ram_total_gb: snapshot.ram_total_gb,
            disk_free_gb: snapshot.disk_free_gb,
            gpu: gpu_stat,
            ts: now_ms(),
        });

        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(poll_interval()) => {}
        }
    }
}

#[cfg(not(test))]
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_cadence_matches_the_contract() {
        // Stubs return false today, so the idle cadence is always selected.
        assert_eq!(poll_interval(), Duration::from_millis(5_000));
    }
}
