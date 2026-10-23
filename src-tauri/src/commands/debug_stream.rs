use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::State;

use crate::error::AppError;
use crate::events::{AppEvent, StepStatus};
use crate::state::AppState;

/// 100 ticks at 100 ms = 10 seconds.
const DEBUG_STREAM_TICK_MS: u64 = 100;
const DEBUG_STREAM_TICKS: u32 = 100;

const DEBUG_WORDS: [&str; 8] = [
    "Local", "only.", "No", "network", "calls.", "This", "pipe", "works.",
];

/// Development-only probe: fake Token + Metrics events at 10 Hz for 10 seconds.
///
/// Compiled out of release builds and not registered in the release invoke
/// handler, so a shipped webview has no way to trigger it.
#[tauri::command]
pub async fn debug_start_stream(state: State<'_, AppState>) -> Result<(), AppError> {
    let active = state
        .try_start_debug_stream()
        .ok_or_else(|| AppError::new("debug stream is already running"))?;

    let tx = state.tx.clone();
    let cancel = state.cancel.clone();

    tauri::async_runtime::spawn(async move {
        let run_id = "debug-stream".to_owned();
        let mut ticker = tokio::time::interval(Duration::from_millis(DEBUG_STREAM_TICK_MS));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        let _ = tx.send(AppEvent::Step {
            run_id: run_id.clone(),
            node: "debug.stream".to_owned(),
            status: StepStatus::Started,
            duration_ms: None,
        });

        let mut completed = true;
        for index in 0..DEBUG_STREAM_TICKS {
            tokio::select! {
                _ = cancel.cancelled() => {
                    completed = false;
                    break;
                }
                _ = ticker.tick() => {}
            }

            let delta = format!("{} ", DEBUG_WORDS[index as usize % DEBUG_WORDS.len()]);
            let _ = tx.send(AppEvent::Token {
                run_id: run_id.clone(),
                delta,
                index,
            });

            if index % 10 == 0 {
                let _ = tx.send(AppEvent::Metrics {
                    run_id: run_id.clone(),
                    ttft_ms: 180,
                    tok_per_sec: 32.4,
                    prompt_tokens: 128,
                    gen_tokens: index + 1,
                    ctx_used_pct: (index + 1) as f32 / DEBUG_STREAM_TICKS as f32 * 100.0,
                });
            }
        }

        if completed {
            let _ = tx.send(AppEvent::Step {
                run_id,
                node: "debug.stream".to_owned(),
                status: StepStatus::Done,
                duration_ms: Some((DEBUG_STREAM_TICK_MS * u64::from(DEBUG_STREAM_TICKS)) as u32),
            });
        }

        active.store(false, Ordering::SeqCst);
    });

    Ok(())
}
