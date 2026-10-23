import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * Mirrors `src-tauri/src/events.rs`.
 *
 * Every schema change must update both files in the same commit (docs/EVENTS.md).
 */

export type StepStatus = "Started" | "Done" | "Failed";

export type GpuStat =
  | { Nvidia: { vram_used_gb: number; vram_total_gb: number; util_pct: number } }
  | { Amd: { vram_used_gb: number; vram_total_gb: number } }
  | { AppleUnified: { mem_used_gb: number; mem_total_gb: number } }
  | "Unavailable";

export type TelemetryEvent = {
  kind: "Telemetry";
  ram_used_gb: number;
  ram_total_gb: number;
  disk_free_gb: number;
  gpu: GpuStat;
  ts: number;
};

export type TokenEvent = {
  kind: "Token";
  run_id: string;
  delta: string;
  index: number;
};

export type MetricsEvent = {
  kind: "Metrics";
  run_id: string;
  ttft_ms: number;
  tok_per_sec: number;
  prompt_tokens: number;
  gen_tokens: number;
  ctx_used_pct: number;
};

export type StepEvent = {
  kind: "Step";
  run_id: string;
  node: string;
  status: StepStatus;
  duration_ms: number | null;
};

export type ToolEvent = {
  kind: "Tool";
  run_id: string;
  name: string;
  args: unknown;
  status: StepStatus;
  duration_ms: number | null;
  summary: string | null;
};

export type ResearchSource = {
  id: number;
  title: string;
  url: string;
  domain: string;
  snippet: string;
  engine: string;
  read: boolean;
};

export type SourcesEvent = {
  kind: "Sources";
  run_id: string;
  sources: ResearchSource[];
};

export type AgentErrorEvent = {
  kind: "Error";
  scope: string;
  message: string;
  recoverable: boolean;
};

export type DownloadStatus = "Started" | "Progress" | "Done" | "Failed" | "Cancelled";

export type ModelDownloadEvent = {
  kind: "ModelDownload";
  id: string;
  status: DownloadStatus;
  downloaded_bytes: number;
  total_bytes: number;
  message: string | null;
};

export type AppEvent =
  | TelemetryEvent
  | TokenEvent
  | MetricsEvent
  | StepEvent
  | ToolEvent
  | SourcesEvent
  | AgentErrorEvent
  | ModelDownloadEvent;

const EVENT_KINDS = new Set<AppEvent["kind"]>([
  "Telemetry",
  "Token",
  "Metrics",
  "Step",
  "Tool",
  "Sources",
  "Error",
  "ModelDownload",
]);

/**
 * Wire payloads arrive from Rust. Validate the discriminator before the payload
 * reaches a store so a schema drift shows up as a warning, not a broken render.
 */
export function isAppEvent(payload: unknown): payload is AppEvent {
  if (typeof payload !== "object" || payload === null) return false;
  const kind = (payload as { kind?: unknown }).kind;
  return typeof kind === "string" && EVENT_KINDS.has(kind as AppEvent["kind"]);
}

export function listenAppEvents(handler: (event: AppEvent) => void): Promise<UnlistenFn> {
  return listen<unknown>("app-event", (event) => {
    if (isAppEvent(event.payload)) {
      handler(event.payload);
    } else {
      console.warn("Dropped app-event with an unknown payload shape", event.payload);
    }
  });
}
