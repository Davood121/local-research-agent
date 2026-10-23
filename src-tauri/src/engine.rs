//! Embedded llama.cpp inference engine.
//!
//! This is "our own Ollama": the same engine Ollama itself uses, compiled into
//! the app binary and driven directly over the safe `llama-cpp-2` bindings.
//! Models are ordinary GGUF files placed in `<app data>/models/`. No helper
//! process, no port, and no HTTP loop between the UI and the engine.
//!
//! Security and lifetime model:
//! - The engine only ever loads files the backend itself discovers in its
//!   app-data models directory. The webview selects a scanned model *by name*;
//!   the command layer re-scans that directory to resolve the path, so a
//!   malicious renderer can never feed the engine an arbitrary file path.
//! - Generation runs on a blocking task and is cancelled through the shared
//!   `CancellationToken`, so Stop and app-exit never orphan a decode loop.
//!
//! GPU offload is a runtime choice: the binary is built with the Vulkan
//! backend, and `InferenceDevice` decides between zero GPU layers (CPU) and
//! full offload to the best discrete Vulkan device. The same binary can do
//! either, so switching never requires a rebuild.

use std::num::{NonZeroU16, NonZeroU32};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaChatMessage, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::{list_llama_ggml_backend_devices, LlamaBackendDeviceType};
use serde::Serialize;
use tauri::Manager;
use tokio_util::sync::CancellationToken;

use crate::error::AppError;
use crate::ollama::{ChatMessage, MAX_MODEL_CHARS};

/// Models folder name under the app data directory.
pub const MODELS_DIR_NAME: &str = "models";

/// Ceiling of generated tokens per turn: a lively default for a desktop
/// assistant answer. The context window always wins over this ceiling.
const MAX_NEW_TOKENS: usize = 1_024;

/// Interactive-model context clamp. GGUF headers often advertise much longer
/// windows than a laptop can hold CPU-only; the KV cache stays predictable.
/// 8192 keeps cited research prompts (8 excerpts) inside the window while the
/// cache for a sub-2B model stays around 100 MB.
const CTX_FLOOR: u32 = 1_024;
const CTX_CEILING: u32 = 8_192;
const CTX_HEADROOM_TOKENS: u32 = 64;

/// llama.cpp logits-batch sizing for the prompt ingest sweep.
const PREFILL_TOKENS_PER_BATCH: usize = 256;

/// A stable seed keeps answers reproducible for the same prompt; in a research
/// assistant that quotes sources, determinism beats stochastic variety.
const SAMPLER_SEED: u32 = 0x1A2B_3C4D;
const SAMPLER_TEMP: f32 = 0.7;
const SAMPLER_TOP_K: i32 = 40;
const SAMPLER_TOP_P: f32 = 0.95;

/// A GGUF file found in the models directory, safe to expose to the webview.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EngineModelFile {
    /// Filename stem, used as the model name everywhere (UI, chat request).
    pub name: String,
    /// Size on disk, shown in the model list.
    pub size_bytes: u64,
}

/// Where inference runs. `Cpu` keeps every layer on the host; `Gpu` offloads
/// all layers to the best discrete Vulkan device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InferenceDevice {
    Cpu,
    Gpu,
}

impl InferenceDevice {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cpu" => Some(Self::Cpu),
            "gpu" => Some(Self::Gpu),
            _ => None,
        }
    }
}

/// A Vulkan device the engine can offload to, safe to expose to the webview.
#[derive(Debug, Clone, Serialize)]
pub struct GpuInfo {
    /// Device description, e.g. "NVIDIA GeForce RTX 4050 Laptop GPU".
    pub name: String,
    /// Backend name, e.g. "Vulkan".
    pub backend: String,
    pub memory_total_bytes: u64,
    pub memory_free_bytes: u64,
}

/// Alias so command code can hold loaded weights without importing
/// `llama_cpp_2` directly; the engine crate stays the only C-boundary.
pub type LlamaModelHandle = Arc<LlamaModel>;

/// A model currently resident in the engine.
pub struct LoadedEngineModel {
    /// File stem the model was loaded from; also the chat `model` name.
    pub name: String,
    /// Where it was loaded from, for status display.
    pub path: PathBuf,
    /// The loaded weights. `Send`/`Sync` are implemented by `llama-cpp-2`.
    pub model: Arc<LlamaModel>,
    /// Device chosen when these weights were loaded.
    pub device: InferenceDevice,
}

/// Snapshot returned by the `engine_status` command.
#[derive(Debug, Clone, Serialize)]
pub struct EngineStatus {
    pub loaded: Option<LoadedModelStatus>,
    pub files: Vec<EngineModelFile>,
    /// Absolute scanned directory; helps users place their GGUFs.
    pub models_dir: Option<String>,
    /// Current preference, used for the next model load.
    pub device: InferenceDevice,
    /// Best discrete Vulkan device, when the build and driver expose one.
    pub gpu: Option<GpuInfo>,
}

/// Metadata about the resident model, safe to cross IPC.
#[derive(Debug, Clone, Serialize)]
pub struct LoadedModelStatus {
    pub name: String,
    pub path: String,
    pub context_length: u32,
    pub device: InferenceDevice,
}

/// Normalised result of one generation run.
pub enum GenerationOutcome {
    Completed {
        prompt_tokens: u32,
        gen_tokens: u32,
        tok_per_sec: f32,
        ttft_ms: u32,
        ctx_used_pct: f32,
    },
    Cancelled,
    Failed(String),
}

/// The process-wide backend. llama.cpp requires a single `init` per process;
/// the safe binding encodes that by making a second `init()` an error, so the
/// handle is created lazily exactly once and kept for the process lifetime.
fn backend() -> Result<&'static LlamaBackend, AppError> {
    static BACKEND: OnceLock<Result<LlamaBackend, String>> = OnceLock::new();
    let storage = BACKEND.get_or_init(|| {
        LlamaBackend::init()
            .map_err(|error| format!("The embedded inference engine did not start: {error}"))
    });
    storage
        .as_ref()
        .map_err(|message| AppError::new(message.clone()))
}

/// The best discrete GPU exposed by the compiled backend, if any.
///
/// Integrated GPUs are ignored: offloading to Intel/AMD iGPUs shares the same
/// memory bus as the CPU and is not the switch this app offers. When several
/// discrete devices exist, the one with the most VRAM wins.
pub fn gpu_device() -> Option<GpuInfo> {
    backend().ok()?;
    list_llama_ggml_backend_devices()
        .into_iter()
        .filter(|device| {
            device.device_type == LlamaBackendDeviceType::Gpu
                && !device.backend.eq_ignore_ascii_case("cpu")
        })
        .max_by_key(|device| device.memory_total)
        .map(|device| GpuInfo {
            name: device.description,
            backend: device.backend,
            memory_total_bytes: u64::try_from(device.memory_total).unwrap_or(u64::MAX),
            memory_free_bytes: u64::try_from(device.memory_free).unwrap_or(u64::MAX),
        })
}

/// Backend device index for [`gpu_device`], used by `with_devices`.
fn gpu_device_index() -> Option<usize> {
    backend().ok()?;
    list_llama_ggml_backend_devices()
        .into_iter()
        .filter(|device| {
            device.device_type == LlamaBackendDeviceType::Gpu
                && !device.backend.eq_ignore_ascii_case("cpu")
        })
        .max_by_key(|device| device.memory_total)
        .map(|device| device.index)
}

/// `<app data>/models`, created eagerly so users have a visible target folder.
pub fn models_dir(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| AppError::new("The application data directory is unavailable."))?
        .join(MODELS_DIR_NAME);
    if let Err(error) = std::fs::create_dir_all(&dir) {
        return Err(AppError::new(format!(
            "The models folder could not be created ({}).",
            short_error(&error.to_string())
        )));
    }
    Ok(dir)
}

/// GGUF files in the models directory that may be offered to the webview.
///
/// The directory is scanned fresh on every call, so dropping a file in is all
/// it takes for it to appear in the model list. Stems that fail the same name
/// rules as chat model names are skipped: they could never be addressed by the
/// chat command, so advertising them would be a dead end.
pub fn scan_model_files(dir: &Path) -> Vec<EngineModelFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut files: Vec<EngineModelFile> = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("gguf") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if !is_valid_engine_model_name(stem) {
            continue;
        }
        let size_bytes = entry.metadata().map(|meta| meta.len()).unwrap_or_default();
        files.push(EngineModelFile {
            name: stem.to_owned(),
            size_bytes,
        });
    }

    files.sort_by(|a, b| a.name.cmp(&b.name));
    files.dedup_by(|a, b| a.name == b.name);
    files
}

/// Resolve a model *name* against the models directory. This is the only path
/// from a webview-chosen name to disk: the name is matched against a fresh
/// scan, never joined into a path directly.
pub fn find_model_file(dir: &Path, name: &str) -> Option<PathBuf> {
    if !is_valid_engine_model_name(name) {
        return None;
    }
    let mut found = None;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|ext| ext.to_str()) == Some("gguf")
                && path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .is_some_and(|stem| stem == name)
            {
                found = Some(path);
                break;
            }
        }
    }
    found
}

/// Filename rule for engine models. Deliberately the same alphabet as the chat
/// model name rule so a scanned file flows through `validate_chat_request`
/// without its own special case.
#[must_use]
pub fn is_valid_engine_model_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_MODEL_CHARS
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

/// Load a GGUF into memory on the requested device.
///
/// `Cpu` keeps every weight on the host. `Gpu` selects the best discrete Vulkan
/// device explicitly (so a hybrid laptop never lands on its iGPU) and offloads
/// all layers. The caller unloads the previous model first, so VRAM is free.
pub fn load_model(
    path: &Path,
    name: &str,
    device: InferenceDevice,
) -> Result<Arc<LlamaModel>, AppError> {
    let backend = backend()?;
    let mut params = LlamaModelParams::default().with_n_gpu_layers(match device {
        InferenceDevice::Cpu => 0,
        InferenceDevice::Gpu => 999,
    });

    if device == InferenceDevice::Gpu {
        let index = gpu_device_index().ok_or_else(|| {
            AppError::new("GPU inference was requested, but no discrete Vulkan GPU is available.")
        })?;
        params = params.with_devices(&[index]).map_err(|error| {
            AppError::new(format!(
                "The GPU device could not be selected ({}).",
                short_error(&error.to_string())
            ))
        })?;
    }

    let model = LlamaModel::load_from_file(backend, path, &params).map_err(|error| {
        AppError::new(format!(
            "The model '{name}' could not be loaded ({}).",
            short_error(&error.to_string())
        ))
    })?;
    Ok(Arc::new(model))
}

/// Interactive-model context for a loaded model.
fn context_length_for(model: &LlamaModel) -> u32 {
    model.n_ctx_train().clamp(CTX_FLOOR, CTX_CEILING)
}

/// Status view of a resident model, for `engine_status`.
pub fn loaded_model_status(loaded: &LoadedEngineModel) -> LoadedModelStatus {
    LoadedModelStatus {
        name: loaded.name.clone(),
        path: loaded.path.display().to_string(),
        context_length: context_length_for(&loaded.model),
        device: loaded.device,
    }
}

/// Run one chat turn to completion.
///
/// `on_token` receives complete UTF-8 fragments as they are produced; partial
/// multi-byte sequences are held back until the next token completes them.
///
/// Blocking by design: always call from a `spawn_blocking`/thread context.
pub fn generate_stream(
    model: Arc<LlamaModel>,
    messages: &[ChatMessage],
    cancel: &CancellationToken,
    on_token: &mut dyn FnMut(&str),
) -> GenerationOutcome {
    match generate_inner(&model, messages, cancel, on_token) {
        Ok(outcome) => outcome,
        Err(GenerationStopError(GenerationStop::Cancelled)) => GenerationOutcome::Cancelled,
        Err(GenerationStopError(GenerationStop::Failed(message))) => {
            GenerationOutcome::Failed(message)
        }
    }
}

/// Internal failure channel: cancellation must not surface as an error.
#[derive(Debug)]
enum GenerationStop {
    Cancelled,
    Failed(String),
}

/// The deep decode loop terminates through this error type instead of
/// threading stray `Result`s through every step by hand.
#[derive(Debug)]
struct GenerationStopError(GenerationStop);

impl From<String> for GenerationStopError {
    fn from(message: String) -> Self {
        Self(GenerationStop::Failed(message))
    }
}

impl From<std::ffi::NulError> for GenerationStopError {
    fn from(_: std::ffi::NulError) -> Self {
        Self::from(
            "The prompt contained a character that cannot be passed to the engine.".to_owned(),
        )
    }
}

fn generate_inner(
    model: &LlamaModel,
    messages: &[ChatMessage],
    cancel: &CancellationToken,
    on_token: &mut dyn FnMut(&str),
) -> Result<GenerationOutcome, GenerationStopError> {
    let process_start = Instant::now();
    if cancel.is_cancelled() {
        return Ok(GenerationOutcome::Cancelled);
    }

    let backend =
        backend().map_err(|error| GenerationStopError(GenerationStop::Failed(error.message)))?;

    let template = model.chat_template(None).map_err(|error| {
        GenerationStopError::from(format!(
            "The model does not define a usable chat template ({}).",
            short_error(&error.to_string())
        ))
    })?;

    let mut chat: Vec<LlamaChatMessage> = Vec::with_capacity(messages.len());
    for message in messages {
        chat.push(
            LlamaChatMessage::new(message.role.clone(), message.content.clone()).map_err(
                |error| GenerationStopError::from(format!("The chat prompt is not valid: {error}")),
            )?,
        );
    }

    let prompt = model
        .apply_chat_template(&template, &chat, true)
        .map_err(|error| {
            GenerationStopError::from(format!(
                "The prompt could not be built from the model template ({}).",
                short_error(&error.to_string())
            ))
        })?;

    let vocab = model.vocab();
    let tokens = vocab.tokenize(prompt.as_bytes(), true, true);
    if tokens.is_empty() {
        return Err(
            "The prompt did not turn into any tokens; the model file may be corrupt."
                .to_owned()
                .into(),
        );
    }

    let n_ctx = context_length_for(model);
    let max_prompt =
        usize::try_from(n_ctx.saturating_sub(CTX_HEADROOM_TOKENS)).unwrap_or(usize::MAX);
    if tokens.len() > max_prompt {
        return Err(format!(
            "This conversation is longer than the loaded model can hold ({} token window). Start a new chat or pick a bigger model.",
            n_ctx.saturating_sub(CTX_HEADROOM_TOKENS)
        )
        .into());
    }

    // Thread-count note: generation speed plateaus around the physical core
    // count and *regresses* on hybrid (P/E) laptop CPUs when all logical
    // threads are used — benchmarked on an 8 core / 12 thread Intel Core 5
    // 210H: 8 threads = 68.7 tok/s, 12 threads = 55.9 tok/s. Clamp to the
    // sweet spot instead of available_parallelism.
    const MAX_GENERATION_THREADS: usize = 8;
    let n_threads = i32::try_from(
        thread::available_parallelism()
            .map_or(2, |value| value.get().min(MAX_GENERATION_THREADS))
            .max(2),
    )
    .unwrap_or(2);

    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(n_ctx))
        .with_n_threads(n_threads)
        .with_n_threads_batch(n_threads);
    let mut ctx = model.new_context(backend, ctx_params).map_err(|error| {
        GenerationStopError::from(format!(
            "The model context could not be created ({}).",
            short_error(&error.to_string())
        ))
    })?;

    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::top_k(SAMPLER_TOP_K),
        LlamaSampler::temp(SAMPLER_TEMP),
        LlamaSampler::top_p(SAMPLER_TOP_P, 1),
        LlamaSampler::dist(SAMPLER_SEED),
    ]);

    let mut batch = LlamaBatch::new(PREFILL_TOKENS_PER_BATCH, 1);
    let mut sample_idx = ingest_prompt(&mut ctx, &mut batch, &tokens, cancel)?;

    // --- generation ---------------------------------------------------------
    // `sample_idx` is always a batch-local logits id: after the prompt ingest it
    // points at the last token of the final prefill chunk, and after every
    // single-token generation decode it points at slot 0 of `gen_batch`.
    let mut pending_bytes: Vec<u8> = Vec::new();
    let mut next_pos = i32::try_from(tokens.len()).unwrap_or(i32::MAX);
    let mut generated = 0usize;
    let mut gen_start: Option<Instant> = None;
    let mut ttft_ms = 0u32;
    // One-token batch reused across the whole generation loop.
    let mut gen_batch = LlamaBatch::new(1, 1);

    loop {
        if cancel.is_cancelled() {
            // Best effort: surface any complete fragment that already arrived.
            let _ = flush_partial(&mut pending_bytes, on_token);
            return Ok(GenerationOutcome::Cancelled);
        }
        if generated >= MAX_NEW_TOKENS {
            break;
        }

        let token = sampler.sample(&ctx, sample_idx);
        if vocab.is_eog(token) {
            break;
        }

        let piece = vocab.token_to_piece(token, false, None::<NonZeroU16>);
        if !piece.is_empty() {
            if gen_start.is_none() {
                gen_start = Some(Instant::now());
                ttft_ms = u32::try_from(process_start.elapsed().as_millis()).unwrap_or(u32::MAX);
            }
            pending_bytes.extend_from_slice(&piece);
            let _ = flush_partial(&mut pending_bytes, on_token);
        }
        sampler.accept(token);

        gen_batch.clear();
        gen_batch
            .add(token, next_pos, &[0], true)
            .map_err(|error| GenerationStopError::from(format!("Generation failed: {error}")))?;
        ctx.decode(&mut gen_batch)
            .map_err(|error| GenerationStopError::from(format!("Generation failed: {error}")))?;
        sample_idx = 0;
        next_pos = next_pos.saturating_add(1);
        generated += 1;

        if cancel.is_cancelled() {
            let _ = flush_partial(&mut pending_bytes, on_token);
            return Ok(GenerationOutcome::Cancelled);
        }
    }

    let _ = flush_partial(&mut pending_bytes, on_token);

    let gen_tokens = u32::try_from(generated).unwrap_or(0);
    let gen_elapsed: Duration = gen_start.map_or(Duration::ZERO, |start| start.elapsed());
    let tok_per_sec = if gen_elapsed.as_secs_f64() > 0.0 {
        f64::from(gen_tokens) / gen_elapsed.as_secs_f64()
    } else {
        0.0
    } as f32;

    let total = tokens.len() + generated;
    let ctx_used_pct = if n_ctx > 0 {
        (u64::try_from(total).unwrap_or(0) * 100 / u64::from(n_ctx)).min(100) as f32
    } else {
        0.0
    };

    Ok(GenerationOutcome::Completed {
        prompt_tokens: u32::try_from(tokens.len()).unwrap_or(0),
        gen_tokens,
        tok_per_sec,
        ttft_ms,
        ctx_used_pct,
    })
}

/// Batched prompt ingest. Logits are requested only on the very last prompt
/// token, exactly like llama.cpp's own chunked-prefill example. Returns the
/// index that must be passed to sampling afterwards: llama.cpp's
/// `get_logits_ith` is addressed relative to the batch that was decoded last,
/// not relative to the whole prompt, so the caller cannot reuse the global
/// prompt position.
fn ingest_prompt(
    ctx: &mut llama_cpp_2::context::LlamaContext<'_>,
    batch: &mut LlamaBatch,
    tokens: &[llama_cpp_2::token::LlamaToken],
    cancel: &CancellationToken,
) -> Result<i32, GenerationStopError> {
    let total = tokens.len();
    let mut ingested = 0usize;
    let mut last_logits_idx = 0i32;
    while ingested < total {
        if cancel.is_cancelled() {
            return Err(GenerationStopError(GenerationStop::Cancelled));
        }
        let end = (ingested + PREFILL_TOKENS_PER_BATCH).min(total);
        batch.clear();
        for (offset, &token) in tokens[ingested..end].iter().enumerate() {
            let position = i32::try_from(ingested + offset).unwrap_or(i32::MAX);
            let is_prompt_end = ingested + offset == total - 1;
            batch
                .add(token, position, &[0], is_prompt_end)
                .map_err(|error| GenerationStopError::from(format!("Prefill failed: {error}")))?;
        }
        ctx.decode(batch)
            .map_err(|error| GenerationStopError::from(format!("Prefill failed: {error}")))?;
        // Index of the last token of this chunk inside `batch`; when the prompt
        // ended in this chunk that is also the logits slot.
        last_logits_idx = i32::try_from(end - ingested - 1).unwrap_or(0);
        ingested = end;
    }
    Ok(last_logits_idx)
}

/// Emit only the longest complete UTF-8 prefix of the byte accumulator, the
/// partial-decode pattern llama.cpp itself recommends. Returns characters sent.
fn flush_partial(pending: &mut Vec<u8>, on_token: &mut dyn FnMut(&str)) -> usize {
    if pending.is_empty() {
        return 0;
    }
    let emitted = match std::str::from_utf8(pending) {
        Ok(text) => text.len(),
        Err(error) => error.valid_up_to(),
    };
    if emitted == 0 {
        return 0;
    }
    let text = String::from_utf8_lossy(&pending[..emitted]);
    if !text.is_empty() {
        on_token(&text);
    }
    pending.drain(..emitted);
    emitted
}

/// Readable single-line error text, matching the Ollama path's hygiene.
fn short_error(message: &str) -> String {
    message
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .take(200)
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::InferenceDevice;

    #[test]
    fn parses_known_devices_case_insensitively() {
        assert_eq!(InferenceDevice::parse("cpu"), Some(InferenceDevice::Cpu));
        assert_eq!(InferenceDevice::parse("GPU"), Some(InferenceDevice::Gpu));
        assert_eq!(
            InferenceDevice::parse("  Gpu  "),
            Some(InferenceDevice::Gpu)
        );
    }

    #[test]
    fn rejects_unknown_devices() {
        assert_eq!(InferenceDevice::parse(""), None);
        assert_eq!(InferenceDevice::parse("tpu"), None);
        assert_eq!(InferenceDevice::parse("gpu0"), None);
    }

    #[test]
    fn serializes_as_lowercase_names() {
        assert_eq!(
            serde_json::to_string(&InferenceDevice::Cpu).unwrap(),
            "\"cpu\""
        );
        assert_eq!(
            serde_json::to_string(&InferenceDevice::Gpu).unwrap(),
            "\"gpu\""
        );
    }
}
