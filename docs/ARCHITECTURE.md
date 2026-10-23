# Architecture

## Three layers

```
┌──────────────────────────────────────────────────────────────┐
│ 1. Webview UI — React 18 + TypeScript + Vite                 │
│    Zustand stores · Tailwind · no OS access, no direct net   │
└──────────────────────────────┬───────────────────────────────┘
                               │ Tauri IPC (invoke + app-event)
┌──────────────────────────────┴───────────────────────────────┐
│ 2. Rust core — Tauri v2                                      │
│    commands · AppState · broadcast bus · telemetry poller    │
│    sysinfo · NVML · AMD sysfs · system_profiler              │
│    fixed-loopback Ollama + SearXNG clients (reqwest)         │
│    bounded reader · extraction · BM25 · cited prompt         │
└───────┬──────────────────────┬──────────────────────┬────────┘
        │ HTTP 127.0.0.1:11434 │ HTTP 127.0.0.1:8080 │ HTTPS (user-initiated)
┌───────┴──────────────┐ ┌─────┴─────────────┐ ┌──────┴──────────────┐
│ 3a. Local Ollama     │ │ 3b. Local SearXNG │ │ 3c. Public result   │
│ inference + models   │ │ search + JSON API │ │ pages (bounded read)│
└──────────────────────┘ └───────────────────┘ └─────────────────────┘
```

**Layer 1** is presentation only. It cannot read files, spawn processes, or open
sockets. It invokes a small Rust command surface and receives state over the
single `app-event` channel. `withGlobalTauri` is false, so there is no ambient
`window.__TAURI__` handle.

**Layer 2** owns all privileged work. Commands return `Result<T, AppError>` and
never panic. Long-running work (telemetry, the debug stream, inference streams,
research) runs on Tokio tasks that share one `CancellationToken` and stop when
the app exits. The event bus is `tokio::sync::broadcast`; subscribers are lossy
by design and lag is reported to the UI as a recoverable `Error` event instead
of silently dropping input.

**Layer 3** is external and user-run. The app does not install, start, update, or
manage Ollama, SearXNG, or their content. The Rust core constructs URLs only from
the constants `http://127.0.0.1:11434` and `http://127.0.0.1:8080`. The only
non-loopback requests are page reads in an explicit research run.

## Local chat path

1. The UI generates a `run_id` before invoking `start_chat`, so streamed events
   and the command response cannot race.
2. Rust validates the run id, model name, message roles, message count, and text
   size. The webview cannot supply a host, port, proxy, header, or arbitrary
   endpoint. Any system message sent by the webview is discarded; Rust prepends
   the fixed app-owned chat prompt.
3. Rust reads `GET /api/tags`, removes models with a `remote_host` or a `:cloud`
   name, removes models that do not advertise completion support, and confirms
   the requested model is in that local list. Cloud-routed models are therefore
   rejected even if a malicious UI asks for one.
4. The single chat slot is claimed. A second concurrent chat is refused so two
   models cannot compete for the same GPU/VRAM.
5. Rust sends `POST /api/chat` with `stream: true` and `think: false`, then parses
   newline-delimited JSON incrementally. Token chunks become `Token` events;
   `eval_count`, `eval_duration`, and `prompt_eval_count` become one `Metrics`
   event at completion. A final `Step` marks the run done or failed.
6. `cancel_chat` cancels the active `CancellationToken`. The stream task stops,
   emits a recoverable `Error` with `scope: "ollama"` and a failed `Step`, and
   releases the slot. The UI keeps whatever text already arrived.
7. Chat history, model choice, and UI state never leave the webview except as the
   messages sent to the local Ollama daemon.

## Local research path

1. The UI sends `start_research` with a question, the selected local model, and
   prior conversation. It cannot send a page URL, search endpoint, or prompt
   template.
2. Rust validates the request, re-fetches Ollama tags, rejects non-local models,
   and claims the same single-run slot used by chat.
3. `GET http://127.0.0.1:8080/search?format=json` returns at most 6 normalized
   results. Literal private addresses and blocked hostnames are removed before
   the UI ever sees them.
4. Up to 4 pages are read concurrently. The HTTP client has redirects disabled
   and proxies disabled; Rust follows redirects manually, revalidating every
   hop. Each hostname is resolved immediately before the request and all
   resolved addresses must be public. Bodies are capped at 2.5 MB, with timeouts.
5. Readable HTML or plain text is extracted without executing scripts. Up to
   180,000 characters per page are kept, chunked with overlap, and ranked by
   in-memory BM25 with a per-source diversity cap.
6. Rust emits one `Sources` event with the numbered metadata, then builds a
   system prompt containing only those sources and their excerpts. The prompt
   names the exact valid citation labels.
7. The same Ollama streaming path as chat produces `Token`/`Metrics` events. The
   research and chat steps are separate in the activity panel, but **Stop**
   cancels the entire pipeline.
8. The UI fills the left **Sources** panel and filters out citation numbers that
   do not exist in that run. Assistant Markdown is rendered without raw HTML,
   navigable links, or remote image loads. Page text and sources live only in
   memory for the run.

Research is deliberately not an implicit tool: the model cannot choose to browse
in ordinary chat. Only the explicit Research composer mode starts the pipeline.

## Event flow

```
sysinfo / NVML / sysfs / system_profiler    Ollama NDJSON stream
              │                                      │
              ▼                                      ▼
      telemetry task                         chat/research task
              └──────────────┬───────────────────────┘
                             ▼
                broadcast::Sender<AppEvent>
                             │
                             ▼
                 subscribe_events forwarder
                             │ window.emit("app-event", event)
                             ▼
                 @tauri-apps/api listen → routeAppEvent
                             ▼
        Zustand stores → React panels (throttled resource rendering)
```

The research task emits tool events for search/page reads and a `Sources` event
before it hands the prompt to the same streaming chat helper.

Telemetry starts with a short delay so the webview can install the forwarder.
GPU backend detection is cached at startup: NVML initialisation is expensive and
`system_profiler` can take seconds, so probing it on every tick would break the
idle CPU budget. A GPU hot-plug therefore requires an app restart; the UI shows
`GPU unavailable` in the meantime rather than failing.

## Why Tauri instead of Electron

- No bundled Chromium or Node runtime. The app uses the OS webview (WebView2,
  WKWebView, WebKitGTK), which keeps installers and idle memory far below an
  Electron equivalent.
- The privileged surface is explicit Rust code exposed through commands, with a
  deny-by-default capability file. Electron's main process has a much larger
  default surface and relies on discipline alone.
- Rust gives us a small, auditable telemetry layer (`sysinfo`, runtime-loaded
  NVML, plain sysfs reads) with no JS/native boundary in the hot path.
- Tauri's CSP injection and IPC scoping give the frontend a browser-grade
  sandbox without an extra framework.

The trade-off is webview variance: Chromium on Windows, WebKit on macOS/Linux.
We keep the UI to stable web-platform features and verify all three OSes in CI.

## Trust boundaries

- The webview is untrusted. It gets no fs/shell/http/dialog permissions.
- Rust treats IPC arguments as untrusted and keeps command bodies panic-free.
- The command surface is `subscribe_events`, `ollama_status`, `start_chat`,
  `cancel_chat`, `research_status`, and `start_research` in both profiles, plus
  `debug_start_stream` in debug builds only. The release invoke handler does not
  contain the debug command.
- The Ollama and SearXNG endpoints are compile-time loopback constants. There is
  no user-configurable URL.
- The only non-loopback requests are direct page reads selected by Rust from
  SearXNG results during an explicit research run. They are bounded, revalidated
  on every redirect, and reject private address space. There are no background
  searches or page crawls.
- No analytics, crash upload, or cloud-inference request exists.

## Deliberate limits of this milestone

Chat, telemetry, the event bus, local research, retrieval, and the shell are
real. Retrieval is per-run BM25 in memory: there is no SQLite page library,
embedding index, long-term memory, agent orchestration, or in-app model
management. The UI does not fake those capabilities. Persistent retrieval is the
next Phase 4B slice.
