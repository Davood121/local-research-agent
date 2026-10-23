# Local Research Agent

A local-first AI research agent desktop app (Tauri v2 + Rust +
React/TypeScript). All inference is local. Chat is offline-capable; research
reads result pages from the public web only when you explicitly start it. No
cloud inference, API keys, analytics, or telemetry uploads.

**Status: local chat + local research (Phases 0–4).** The app shell, typed live
event bus, system telemetry, streaming Ollama chat, SearXNG search, bounded page
reads, in-memory BM25 retrieval, and cited research answers are implemented.
Persistent retrieval, embeddings, memory, and the agent canvas are **not**
implemented yet.

## Requirements

- Rust 1.90 (pinned by `rust-toolchain.toml`, installed automatically by rustup)
- Node.js 22+
- Per-OS native dependencies — see [docs/BUILD.md](docs/BUILD.md)
- **Ollama**, installed and running separately on the same machine. The app does
  not bundle, download, start, or update Ollama or its models.
- **SearXNG** for research, run separately at `http://127.0.0.1:8080` with JSON
  output enabled. Chat works without it; research shows a setup state until it
  is reachable. The bundled settings aggregate DuckDuckGo, Brave, Wikipedia,
  Wikidata, Mwmbl, Wiby, and Yep without API keys. See
  [docs/RESEARCH.md](docs/RESEARCH.md).

The app talks to Ollama at the fixed loopback address `http://127.0.0.1:11434`.
It never accepts a different endpoint, API key, or cloud-routed model. Models
with a `remote_host` or a `:cloud` name are filtered out of the picker and
rejected by the backend. SearXNG is likewise fixed at `127.0.0.1:8080`; the
webview cannot choose a search or inference endpoint.

## Run in development

Start Ollama and install one local chat model (the Ollama desktop app starts the
server automatically; use `ollama serve` for a CLI-only install):

```bash
ollama serve
ollama pull qwen3:4b
```

Then install the app dependencies and start Tauri:

```bash
npm ci
npm run tauri:dev
```

The header shows the Ollama version and the number of local chat models. Select
a model, send a prompt, and watch tokens arrive in the conversation and run
metrics in the right-hand **Run insights** panel. **Stop** cancels the active
generation. The model selection and chat/research mode are remembered locally
with `localStorage`; no other state is persisted.

Assistant replies render standard Markdown headings, lists, emphasis, tables,
and code. Raw HTML is ignored, links are not navigable, and remote images are
never loaded while rendering an answer. Chat is primed with a short description
of this app, so asking **what can you do?** describes Local Research Agent rather
than a generic language model.

For research, start SearXNG (or the deterministic mock) at
`127.0.0.1:8080`:

```bash
# Real local instance (Docker example in tools/searxng)
cd tools/searxng && docker compose up -d

# Or the development mock, which returns real public test pages
npm run mock:search
```

Then switch the composer to **Research**, ask a question, and watch the
left-hand **Sources** panel fill with numbered result pages. The answer keeps
its citations, but the source list is no longer repeated under the chat message.
Research reads a small number of result pages directly from the public web; see
[docs/RESEARCH.md](docs/RESEARCH.md) for the exact limits, disclosures, and
troubleshooting.

If Ollama is not running, the chat screen explains how to start it. Use the
refresh button or wait for the 30-second health check after starting it. Search
status is refreshed on the same interval.

Use the **Debug Stream** button (dev builds only) to emit fake token/metric
events at 10 Hz for 10 seconds.

Frontend-only preview (`npm run dev`) works in a browser but has no Tauri IPC, so
the app shows the offline screen; this is expected.

## Build

```bash
npm run tauri:build:windows   # MSI + NSIS
npm run tauri:build:macos     # .app + .dmg
npm run tauri:build:linux     # AppImage + .deb
```

Generic: `npm run tauri:build` (all bundles valid for the host OS).

Ollama is a runtime dependency, not a build dependency. The installers remain
offline and do not download a model server or a model.

## Quality gates

```bash
npm run lint
npm run format:check
npm run build                 # tsc --noEmit + vite build

cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## What is implemented

- **Phase 0:** Tauri v2 + React 18 + TS + Vite scaffold, Tailwind, Zustand,
  ESLint/Prettier/rustfmt/clippy, pinned toolchain, cross-platform bundles,
  hardened release profile, SHA-pinned CI on three OSes, deny-by-default
  capabilities, CSP, docs.
- **Phase 1:** `AppEvent` enum mirrored in TypeScript, broadcast bus +
  `CancellationToken`, idempotent `subscribe_events` forwarder, debug-only
  10 Hz stream, Zustand stores, throttled activity panel.
- **Phase 2:** adaptive telemetry task (5 s idle / 2 s loaded / 1 s generating),
  RAM + disk via `sysinfo`, GPU dispatch across NVML, AMD sysfs, Apple unified
  memory, `Unavailable` fallback, status bar with exact GB formatting, unit
  tests for formatting, AMD sysfs parsing, and event serialization.
- **Phase 3:** fixed-loopback Ollama status/model discovery, local-only model
  filtering, validated chat requests, streaming NDJSON parsing, token/metrics
  events, first-token and tokens-per-second metrics, single-chat concurrency
  guard, cooperative cancellation, and a redesigned conversation UI with model
  selection, setup states, error handling, and run insights.
- **Phase 4:** explicit Research mode, fixed-loopback SearXNG JSON search,
  bounded concurrent page reads with SSRF/private-address checks, manual
  redirect validation, HTML/plain-text extraction, in-memory BM25 ranking,
  numbered `Sources` events, source cards, citation validation in the UI,
  offline/setup states, and end-to-end cancellation across search, reads, and
  generation. A deterministic SearXNG mock and setup assets are included.

## What is explicitly NOT implemented

Persistent retrieval (SQLite/page cache), embedding indexes, long-term memory,
agent orchestration, model download/management inside the app, LoRA, export,
updater, code signing, and later hardening phases. Retrieval is per-run and
in-memory only. Ollama and SearXNG must be installed and managed by the user.

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Events](docs/EVENTS.md)
- [Research setup and limits](docs/RESEARCH.md)
- [Build & platform notes](docs/BUILD.md)
- [Security](docs/SECURITY.md)

## License

Dual-licensed under MIT OR Apache-2.0.
