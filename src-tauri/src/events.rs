use serde::Serialize;

use crate::research::ResearchSource;

/// The complete, versioned wire format for the `app-event` channel.
///
/// Internally tagged with `kind` so TypeScript can discriminate the union
/// without unwrapping a single-key object. Field names stay snake_case to keep
/// `docs/EVENTS.md` and `src/lib/events.ts` a literal mirror.
// The schema is complete now; later milestones become the first producers of
// variants such as `Tool`. Release builds emit only `Telemetry` and `Error`, so
// without this allow the unused-variant lint would fail `-D warnings`.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum AppEvent {
    Telemetry {
        ram_used_gb: f32,
        ram_total_gb: f32,
        disk_free_gb: f32,
        gpu: GpuStat,
        ts: u64,
    },
    Token {
        run_id: String,
        delta: String,
        index: u32,
    },
    Metrics {
        run_id: String,
        ttft_ms: u32,
        tok_per_sec: f32,
        prompt_tokens: u32,
        gen_tokens: u32,
        ctx_used_pct: f32,
    },
    Step {
        run_id: String,
        node: String,
        status: StepStatus,
        duration_ms: Option<u32>,
    },
    Tool {
        run_id: String,
        name: String,
        args: serde_json::Value,
        status: StepStatus,
        duration_ms: Option<u32>,
        summary: Option<String>,
    },
    Sources {
        run_id: String,
        sources: Vec<ResearchSource>,
    },
    Error {
        scope: String,
        message: String,
        recoverable: bool,
    },
    ModelDownload {
        id: String,
        status: DownloadStatus,
        downloaded_bytes: u64,
        total_bytes: u64,
        message: Option<String>,
    },
}

// Part of the wire contract; `Failed` is produced once agent nodes exist.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub enum StepStatus {
    Started,
    Done,
    Failed,
}

/// Lifecycle of a model download. `message` on the event carries the reason for
/// `Failed`; other states leave it `None`.
#[derive(Debug, Clone, Serialize)]
pub enum DownloadStatus {
    Started,
    Progress,
    Done,
    Failed,
    Cancelled,
}

// Each variant is only constructed on its own platform, but the wire schema is
// shared by all of them.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub enum GpuStat {
    Nvidia {
        vram_used_gb: f32,
        vram_total_gb: f32,
        util_pct: u8,
    },
    Amd {
        vram_used_gb: f32,
        vram_total_gb: f32,
    },
    AppleUnified {
        mem_used_gb: f32,
        mem_total_gb: f32,
    },
    Unavailable,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use serde_json::json;

    #[test]
    fn telemetry_serializes_with_kind_tag_and_typed_gpu() {
        let event = AppEvent::Telemetry {
            ram_used_gb: 7.2,
            ram_total_gb: 16.0,
            disk_free_gb: 45.3,
            gpu: GpuStat::Nvidia {
                vram_used_gb: 4.1,
                vram_total_gb: 8.0,
                util_pct: 38,
            },
            ts: 1_700_000_000_000,
        };

        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["kind"], json!("Telemetry"));
        assert_eq!(value["gpu"]["Nvidia"]["util_pct"], json!(38));
    }

    #[test]
    fn unavailable_gpu_is_a_plain_string_and_optional_fields_are_null() {
        let event = AppEvent::Step {
            run_id: "r1".into(),
            node: "plan".into(),
            status: StepStatus::Done,
            duration_ms: None,
        };

        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["status"], json!("Done"));
        assert!(value["duration_ms"].is_null());

        let gpu = serde_json::to_value(GpuStat::Unavailable).unwrap();
        assert_eq!(gpu, json!("Unavailable"));
    }

    #[test]
    fn model_download_events_serialize_progress_and_reasons() {
        let event = AppEvent::ModelDownload {
            id: "qwen2.5-1.5b-instruct".into(),
            status: DownloadStatus::Progress,
            downloaded_bytes: 512,
            total_bytes: 1024,
            message: None,
        };

        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["kind"], json!("ModelDownload"));
        assert_eq!(value["status"], json!("Progress"));
        assert_eq!(value["downloaded_bytes"], json!(512));
        assert!(value["message"].is_null());

        let failed = serde_json::to_value(DownloadStatus::Failed).unwrap();
        assert_eq!(failed, json!("Failed"));
    }

    #[test]
    fn sources_event_carries_numbered_source_metadata() {
        let event = AppEvent::Sources {
            run_id: "r1".into(),
            sources: vec![ResearchSource {
                id: 1,
                title: "Example".into(),
                url: "https://example.com/".into(),
                domain: "example.com".into(),
                snippet: "A source snippet".into(),
                engine: "duckduckgo".into(),
                read: true,
            }],
        };

        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["kind"], json!("Sources"));
        assert_eq!(value["sources"][0]["id"], json!(1));
        assert_eq!(value["sources"][0]["engine"], json!("duckduckgo"));
        assert_eq!(value["sources"][0]["read"], json!(true));
    }
}
