//! Embedded engine commands: status, CPU/GPU preference, and unload of resident
//! GGUF weights.
//!
//! Loading happens lazily inside `start_chat` (engine path), so there is no
//! separate load command: a chat is the only consumer of the engine and a load
//! without an upcoming chat on an 8 GB laptop is pointless memory. The device
//! preference is persisted next to the models folder so a restart keeps the
//! user's choice.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::engine::{self, EngineStatus, InferenceDevice};
use crate::error::AppError;
use crate::state::AppState;

const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Serialize, Deserialize)]
struct Settings {
    inference_device: String,
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join(SETTINGS_FILE))
}

fn build_status(app: &AppHandle, state: &AppState) -> Result<EngineStatus, AppError> {
    let dir = engine::models_dir(app)?;
    let files = engine::scan_model_files(&dir);
    let loaded = state
        .engine_lock()?
        .as_ref()
        .map(engine::loaded_model_status);

    Ok(EngineStatus {
        loaded,
        files,
        models_dir: Some(dir.display().to_string()),
        device: state.inference_device()?,
        gpu: engine::gpu_device(),
    })
}

#[tauri::command]
pub async fn engine_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<EngineStatus, AppError> {
    build_status(&app, &state)
}

/// Persist the preference and drop resident weights so the next run loads them
/// on the chosen device. Offload is fixed when a model is loaded, so this is the
/// only correct moment to switch.
#[tauri::command]
pub async fn set_inference_device(
    app: AppHandle,
    state: State<'_, AppState>,
    device: String,
) -> Result<EngineStatus, AppError> {
    let requested = InferenceDevice::parse(&device)
        .ok_or_else(|| AppError::new("Unknown inference device."))?;

    if requested == InferenceDevice::Gpu && engine::gpu_device().is_none() {
        return Err(AppError::new(
            "No discrete Vulkan GPU is available; this build can only run on the CPU.",
        ));
    }

    state.change_inference_device(requested, || save_device(&app, requested))?;

    build_status(&app, &state)
}

/// Drop the resident model to release its memory. Safe to call any time; a
/// running chat keeps its own reference to the weights, so stopping the
/// resident slot does not affect an active run.
#[tauri::command]
pub async fn engine_unload(state: State<'_, AppState>) -> Result<(), AppError> {
    *state.engine_lock()? = None;
    Ok(())
}

fn save_device(app: &AppHandle, device: InferenceDevice) -> Result<(), AppError> {
    let path = settings_path(app)
        .ok_or_else(|| AppError::new("The application data directory is unavailable."))?;
    let settings = Settings {
        inference_device: match device {
            InferenceDevice::Cpu => "cpu".to_owned(),
            InferenceDevice::Gpu => "gpu".to_owned(),
        },
    };
    let payload = serde_json::to_vec_pretty(&settings)
        .map_err(|_| AppError::new("The device setting could not be encoded."))?;
    std::fs::write(&path, payload).map_err(|error| {
        AppError::new(format!(
            "The device setting could not be saved ({}).",
            error
        ))
    })
}

/// Restore the saved preference at startup. A saved GPU choice is ignored when
/// the current driver exposes no discrete Vulkan device, so the app never gets
/// stuck failing every chat after a driver change.
pub fn restore_device(app: &AppHandle) {
    let Some(path) = settings_path(app) else {
        return;
    };
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let Ok(settings) = serde_json::from_slice::<Settings>(&bytes) else {
        return;
    };
    let Some(device) = InferenceDevice::parse(&settings.inference_device) else {
        return;
    };
    let state = app.state::<AppState>();
    let device = if device == InferenceDevice::Gpu && engine::gpu_device().is_none() {
        InferenceDevice::Cpu
    } else {
        device
    };
    let _ = state.set_inference_device(device);
}
