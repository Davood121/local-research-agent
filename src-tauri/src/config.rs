//! Shared constants. Keeping them here stops magic numbers from spreading
//! across the event bus, telemetry poller and docs.

/// Capacity of the in-process broadcast bus. Bursts larger than this cause the
/// slowest subscriber to lag; the forwarder reports that as a recoverable error.
pub const EVENT_CHANNEL_CAPACITY: usize = 256;

/// The single Tauri event name used for every `AppEvent`.
pub const APP_EVENT_NAME: &str = "app-event";

/// Idle telemetry cadence (no model resident).
pub const TELEMETRY_IDLE_MS: u64 = 5_000;

/// Telemetry cadence while a model is loaded.
pub const TELEMETRY_MODEL_LOADED_MS: u64 = 2_000;

/// Telemetry cadence while tokens are being generated.
pub const TELEMETRY_GENERATING_MS: u64 = 1_000;
