# Events

The backend and frontend share one event name and one schema.

## Wire envelope

- Event name: **`app-event`** (constant `APP_EVENT_NAME` in `src-tauri/src/config.rs`)
- Payload: one JSON object per event, internally tagged by `kind`.
- Source of truth: `src-tauri/src/events.rs`
- Mirror: `src/lib/events.ts`

Any schema change must update `events.rs`, `events.ts`, and this document in the
same commit. The TypeScript listener also validates the `kind` discriminator and
drops malformed payloads with a local console warning.

## Rust enum ↔ TypeScript union

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum AppEvent {
    Telemetry { ram_used_gb: f32, ram_total_gb: f32, disk_free_gb: f32,
                gpu: GpuStat, ts: u64 },
    Token { run_id: String, delta: String, index: u32 },
    Metrics { run_id: String, ttft_ms: u32, tok_per_sec: f32,
              prompt_tokens: u32, gen_tokens: u32, ctx_used_pct: f32 },
    Step { run_id: String, node: String, status: StepStatus,
           duration_ms: Option<u32> },
    Tool { run_id: String, name: String, args: serde_json::Value,
           status: StepStatus, duration_ms: Option<u32>,
           summary: Option<String> },
    Sources { run_id: String, sources: Vec<ResearchSource> },
    Error { scope: String, message: String, recoverable: bool },
}

pub struct ResearchSource {
    pub id: u32,
    pub title: String,
    pub url: String,
    pub domain: String,
    pub snippet: String,
    pub engine: String,
    pub read: bool,
}

pub enum StepStatus { Started, Done, Failed }

pub enum GpuStat {
    Nvidia { vram_used_gb: f32, vram_total_gb: f32, util_pct: u8 },
    Amd { vram_used_gb: f32, vram_total_gb: f32 },
    AppleUnified { mem_used_gb: f32, mem_total_gb: f32 },
    Unavailable,
}
```

Field names stay `snake_case` on purpose: the TypeScript union is a literal
mirror, not a translation layer. `Option<T>` serialises as `T | null`. Unit
variant `Unavailable` serialises as the plain string `"Unavailable"`.

### JSON examples

```json
{
  "kind": "Telemetry",
  "ram_used_gb": 7.2,
  "ram_total_gb": 16.0,
  "disk_free_gb": 45.3,
  "gpu": { "Nvidia": { "vram_used_gb": 4.1, "vram_total_gb": 8.0, "util_pct": 38 } },
  "ts": 1761177600000
}
```

```json
{ "kind": "Token", "run_id": "run-1", "delta": "Local ", "index": 42 }
```

```json
{
  "kind": "Metrics",
  "run_id": "run-1",
  "ttft_ms": 210,
  "tok_per_sec": 38.6,
  "prompt_tokens": 128,
  "gen_tokens": 118,
  "ctx_used_pct": 0.1
}
```

```json
{
  "kind": "Step",
  "run_id": "run-1",
  "node": "chat",
  "status": "Done",
  "duration_ms": 4210
}
```

```json
{ "kind": "Error", "scope": "ollama", "message": "Generation stopped.", "recoverable": true }
```

```json
{ "kind": "Tool", "run_id": "run-1", "name": "search", "args": { "q": "x" },
  "status": "Failed", "duration_ms": 512, "summary": null }
```

```json
{
  "kind": "Sources",
  "run_id": "run-1",
  "sources": [
    {
      "id": 1,
      "title": "Local-first software - Wikipedia",
      "url": "https://en.wikipedia.org/wiki/Local-first_software",
      "domain": "en.wikipedia.org",
      "snippet": "Local-first software stores primary data on the user's device…",
      "engine": "wikipedia",
      "read": true
    }
  ]
}
```

## Chat run lifecycle

The UI generates the `run_id` before calling `start_chat`, so it can accept
events before the command's promise resolves. Run ids are limited to ASCII
letters, digits, `-`, and `_`.

A successful local run emits:

1. `Step { node: "chat", status: Started, duration_ms: null }`
2. zero or more `Token` events. `index` starts at 0 and counts stream chunks, not
   tokenizer tokens.
3. one `Metrics` event calculated from Ollama's final `done` object. `ttft_ms`
   includes model load/queue time, `tok_per_sec` uses `eval_duration`, and
   `prompt_tokens`/`gen_tokens` come from `prompt_eval_count`/`eval_count`.
   `ctx_used_pct` uses the selected model's advertised context length when
   available, otherwise it is `0`.
4. `Step { node: "chat", status: Done, duration_ms: <run duration> }`

On cancel, the stream task emits `Error { scope: "ollama", message: "Generation
stopped.", recoverable: true }` followed by `Step { status: Failed }`. The chat
store treats that error as a user-requested stop and keeps the partial answer.

On a stream/protocol failure, `Error { scope: "ollama", recoverable: true }` is
followed by a failed `Step`. Validation and setup failures happen before the task
starts and are returned as command errors instead of events. `Error` has no
`run_id`; only one chat or research run can run at a time.

`Metrics` and `Step` data feed the `Run insights` panel. `Token` data feeds both
the conversation and the bounded raw-stream preview.

## Research run lifecycle

A successful research run emits:

1. `Step { node: "research", status: Started }`
2. `Tool { name: "web_search", status: Started }`, then Done or Failed with a
   summary.
3. `Tool { name: "read_pages", status: Started }`, then Done or Failed. A Done
   summary reports how many of the selected pages were read; the rest fall back
   to search snippets.
4. one `Sources` event with the numbered source metadata used by the UI.
5. `Step { node: "research", status: Done }`.
6. `Step { node: "chat", status: Started }`, zero or more `Token` events, one
   final `Metrics`, and `Step { node: "chat", status: Done }`.

The chat store deliberately ignores a successful `research` Done event so it
does not end the run before chat starts. A failed research step finalizes the
assistant message. Search, page-read, and generation errors use
`Error { scope: "research" }`; Ollama generation failures use
`Error { scope: "ollama" }`. Both are routed to the chat store.

Cancellation emits a failed active tool/step followed by
`Error { scope: "research" | "ollama", message: "Generation stopped." }` as
appropriate. The UI keeps any partial answer and marks the entry stopped.

Research is a separate command from chat. The webview never supplies a page URL;
only Rust selects and validates URLs from search results.

## Delivery guarantees

- One broadcast channel (capacity 256) feeds one forwarder per app process.
- `subscribe_events` is idempotent: repeated calls do not duplicate events.
- No replay. A client that subscribes late misses earlier events; telemetry
  re-emits on the next poll.
- If the UI cannot keep up, slow subscribers receive a lag error and the
  forwarder emits a recoverable `Error` event with `scope: "event_bus"` and the
  number of skipped events.
- Telemetry begins ~500 ms after startup so the forwarder can attach.
- Only one chat or research run is allowed at a time. A second `start_chat` or
  `start_research` is rejected while the slot is claimed.

## Debug stream

`debug_start_stream` exists only in debug builds (`#[cfg(debug_assertions)]`)
and is not registered in release. It emits:

- one `Step { node: "debug.stream", status: "Started" }`
- one `Token` and (every 10th tick) one `Metrics` at 10 Hz for 10 seconds
- one final `Step { status: "Done", duration_ms: 10000 }` if not cancelled

The button in the header is also compiled out of production bundles.
