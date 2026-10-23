# Local Research Agent

A local-first AI research agent desktop app (Tauri v2 + Rust +
React/TypeScript). All inference is local. Chat is offline-capable; research
reads result pages from the public web only when you explicitly start it. No
cloud inference, API keys, analytics, or telemetry uploads.

**Status: local chat + intelligent local research (Phases 0–4) + embedded
engine.** The app shell, typed live event bus, system telemetry, streaming
local chat, deterministic multi-query planning, intent-based SearXNG engine
routing, source merge/rank/diversification, bounded page reads, evidence
coverage checks with one retry, in-memory BM25 retrieval, cited research
answers, and a **built-in llama.cpp engine** (no separate server needed) are
implemented. Persistent retrieval, embeddings, memory, and the agent canvas
are **not** implemented yet.

## Requirements

- Rust 1.90 (pinned by `rust-toolchain.toml`, installed automatically by rustup)
- Node.js 22+
- Per-OS native dependencies — see [docs/BUILD.md](docs/BUILD.md)
- **Nothing else is required for chat.** The app embeds a llama.cpp engine and
  runs GGUF models in-process, on the **CPU or a discrete GPU**. Use the download
  button in the header (or the prompt on an empty chat screen) to fetch a curated
  model in-app, or drop a `.gguf` file into the app's models folder and it
  appears in the model picker:

  - Windows: `%APPDATA%\dev.localresearch.agent\models\`
  - macOS: `~/Library/Application Support/dev.localresearch.agent/models/`
  - Linux: `~/.local/share/dev.localresearch.agent/models/`

  The in-app catalog offers Qwen2.5 0.5B, 1.5B, and 3B Instruct (Q4_K_M) from
  Hugging Face over HTTPS. Downloads show progress, can be cancelled, and are
  written to a `.part` file until the size is verified. The equivalent manual
  download is:

  ```bash
  curl -L -o "$APPDATA/dev.localresearch.agent/models/qwen2.5-1.5b-instruct-q4_k_m.gguf" \
    https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf
  ```

  Any model whose file stem matches a chat request takes priority inside the
  app; a separately installed **Ollama** (fixed at `http://127.0.0.1:11434`) is
  used afterwards as a fallback for names it serves. Ollama stays optional:
  the app does not bundle, download, start, or update it. Pick **CPU** or **GPU**
  under **Run insights → Compute**; the switch takes effect on the next message
  and is remembered across restarts. GPU is only offered when the build and
  driver expose a discrete device. If a model cannot be loaded on the GPU (for
  example, it does not fit in VRAM), the error is shown and the CPU choice stays
  one click away.
- **SearXNG** for research, run separately at `http://127.0.0.1:8080` with JSON
  output enabled. Chat works without it; research shows a setup state until it
  is reachable. The bundled settings aggregate DuckDuckGo, Brave, Wikipedia,
  Wikidata, Mwmbl, Wiby, and Yep without API keys. See
  [docs/RESEARCH.md](docs/RESEARCH.md).

The app runs chat inference either in-process (embedded engine, preferred when
the selected model is one of the GGUF files above) or through Ollama at the
fixed loopback address. It never accepts a different endpoint, API key, or
cloud-routed model. Models with a `remote_host` or a `:cloud` name are filtered
out of the picker and rejected by the backend. SearXNG is likewise fixed at
`127.0.0.1:8080`; the webview cannot choose a search or inference endpoint.

## Installers

The cross-platform CI workflow builds Windows MSI/EXE installers, a macOS DMG,
and Linux AppImage/DEB packages. Successful workflow runs provide downloadable
artifacts; version tags publish the installers as a GitHub Release. See
[docs/BUILD.md](docs/BUILD.md#installers-and-releases) for download and release
details.

## Run in development

Either install a GGUF file into the models folder (see above) or start Ollama
and pull one local chat model (the Ollama desktop app starts the server
automatically; use `ollama serve` for a CLI-only install):

```bash
ollama serve
ollama pull qwen3:4b
```

Then install the app dependencies and start Tauri:

```bash
npm ci
npm run tauri:dev
```

The header shows the number of local chat models (embedded GGUFs and Ollama
models appear together; embedded files are marked `embedded gguf` in the
backend model list). Select
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

# No Docker? Native install inside WSL Ubuntu (see tools/searxng/README.md)
~/start-searxng.sh

# Or the development mock, which returns real public test pages
npm run mock:search
```

Then switch the composer to **Research**, ask a question, and watch the
left-hand **Sources** panel fill with numbered result pages. The answer keeps
its citations; if the model omits inline labels, the app appends a short numbered
`Sources:` footer for the excerpts it was given instead of repeating the full
panel list.
Research reads a small number of result pages directly from the public web; see
[docs/RESEARCH.md](docs/RESEARCH.md) for the exact limits, disclosures, and
troubleshooting.

If neither a GGUF in the models folder nor Ollama is available, the chat screen
explains the two ways to install a model. Use the
refresh button or wait for the 30-second health check after starting Ollama.
Search
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

Ollama is an optional runtime dependency, not a build dependency. The embedded
engine links llama.cpp statically into the app binary. The installers remain
offline and do not download a model server or a model. Windows/Linux builds
enable the Vulkan backend, so the Vulkan SDK must be installed on the build
machine; macOS uses Metal. See [docs/BUILD.md](docs/BUILD.md#gpu-backend).

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
- **Embedded engine:** llama.cpp compiled statically into the binary. GGUF files
  placed in the app's models folder are scanned by file stem and selectable in
  the picker without Ollama; chatted models load in-process and stream
  deterministic-sampled tokens with the same Step/Token/Metrics events, UTF-8
  safe partial emission, prompt-chunked prefill, and cooperative cancellation
  as the Ollama path. A curated Hugging Face catalog downloads GGUF models
  in-app with progress, cancellation, size verification, and an HTTPS/host
  allowlist; finished files appear in the picker without a restart. A
  **CPU/GPU switch** in Run insights picks the device per model load: CPU keeps
  all layers on the host, GPU offloads every layer to the best discrete Vulkan
  (Windows/Linux) or Metal (macOS) device, ignoring integrated GPUs. The choice
  persists in `<app data>/settings.json`; switching unloads resident weights so
  the next message reloads them on the chosen device.
- **Phase 4:** explicit Research mode, deterministic multi-query planning,
  intent-based SearXNG engine routing, merged/deduplicated/ranked source
  selection with domain and engine diversity, bounded concurrent page reads with
  SSRF/private-address checks, manual redirect validation, HTML/plain-text
  extraction, local evidence coverage checks with one bounded retry, in-memory
  BM25 ranking, numbered `Sources` events, source cards, citation validation in
  the UI, offline/setup states, and end-to-end cancellation across planning,
  search, reads, retries, and generation. A deterministic SearXNG mock and setup
  assets are included.

## What is explicitly NOT implemented

Persistent retrieval (SQLite/page cache), embedding indexes, long-term memory,
agent orchestration, LoRA, export,
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
