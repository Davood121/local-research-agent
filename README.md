<div align="center">

<img src="src-tauri/icons/icon.png" alt="Local Research Agent" width="120" />

# 🔍 Local Research Agent

**A privacy-first AI research assistant that runs entirely on your machine.**  
No cloud. No API keys. No telemetry. Just pure local intelligence.

[![Latest Release](https://img.shields.io/github/v/release/Davood121/local-research-agent?style=for-the-badge&logo=github&label=Download&color=2ea44f)](https://github.com/Davood121/local-research-agent/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/Davood121/local-research-agent/ci.yml?style=for-the-badge&logo=githubactions&label=CI&logoColor=white)](https://github.com/Davood121/local-research-agent/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20%7C%20Apache--2.0-blue?style=for-the-badge)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey?style=for-the-badge&logo=windows)](https://github.com/Davood121/local-research-agent/releases/latest)
[![Version](https://img.shields.io/badge/version-0.2.1-orange?style=for-the-badge)](https://github.com/Davood121/local-research-agent/releases/tag/v0.2.1)

<br/>

> Built with **Tauri v2** · **Rust** · **React 18** · **TypeScript** · **llama.cpp** (embedded)

</div>

---

## ✨ What's New in v0.1.9

| Feature | Description |
|---------|-------------|
| 📤 **Research Dossier Export** | Export any research session as **Markdown**, **HTML**, or **JSON** — one click |
| 🗂️ **Session Archive** | Auto-saved session history with search, rename, and restore — up to 50 sessions |
| 🎛️ **Research Depth Controls** | Choose **Quick**, **Balanced**, or **Deep** research intensity |
| 🌐 **Domain Focus** | Target **General**, **Academic**, or **News** sources per query |

---

## 🚀 Features at a Glance

<table>
<tr>
<td width="50%">

### 🧠 Local AI Inference
- Embedded **llama.cpp** engine — no server needed
- Runs **GGUF models** in-process (CPU & GPU)
- In-app model downloader (Qwen2.5 family)
- Drop any `.gguf` file into the models folder
- Vulkan (Windows/Linux) + Metal (macOS) GPU acceleration
- Optional **Ollama** fallback

</td>
<td width="50%">

### 🔎 Intelligent Research
- Deterministic **multi-query planning**
- Intent-based **SearXNG** engine routing
- Source merge, rank & domain diversification
- **BM25** in-memory retrieval
- Bounded concurrent page reads (SSRF-safe)
- Cited answers with numbered source cards

</td>
</tr>
<tr>
<td>

### 📦 New in v0.1.9
- **Export Dossier** — MD / HTML / JSON
- **Session Archive** — search & restore history
- **Depth** (Quick / Balanced / Deep)
- **Focus** (General / Academic / News)

</td>
<td>

### 🔒 Privacy by Design
- Zero cloud inference — 100% local
- No API keys, accounts, or subscriptions
- No telemetry or analytics
- Fixed loopback-only endpoints
- Deny-by-default Tauri capabilities + CSP

</td>
</tr>
</table>

---

## 📥 Download & Install

> **One-click installers — no account, no API key, no cloud.**

| Platform | Download | First-Run Notes |
|----------|----------|-----------------|
| 🪟 **Windows 10/11** (x64) | [`_x64-setup.exe`](https://github.com/Davood121/local-research-agent/releases/latest) or `.msi` | SmartScreen: click **More info → Run anyway** (installer is unsigned) |
| 🍎 **macOS 11+** (Apple Silicon / Intel) | [`.dmg`](https://github.com/Davood121/local-research-agent/releases/latest) | Right-click → **Open** on first launch, or run: `xattr -dr com.apple.quarantine "/Applications/Local Research Agent.app"` |
| 🐧 **Linux** (x64) | [`.AppImage`](https://github.com/Davood121/local-research-agent/releases/latest) or `.deb` | `chmod +x *.AppImage && ./Local*.AppImage` — or `sudo apt install ./*.deb` |

### First-Time Setup (3 steps)

```
1. Launch the app
2. Click the ⬇ download button → pick a model (Qwen2.5 1.5B recommended)
3. Start chatting — research works out of the box with the built-in search engine
```

**Recommended models:**

| Model | Size | Speed | Quality |
|-------|------|-------|---------|
| Qwen2.5 0.5B Q4_K_M | ~400 MB | ⚡ Fastest | Basic |
| **Qwen2.5 1.5B Q4_K_M** ← default | ~1 GB | ✅ Balanced | Good |
| Qwen2.5 3B Q4_K_M | ~2 GB | 🐢 Slower | Best |

---

## 🗂️ Session Archive & Export (New)

The **Archive** button (top-right) opens a searchable history of all your research sessions:

- 🔍 **Search** sessions by title or content
- ✏️ **Rename** any session inline
- ♻️ **Restore** a session back into chat
- 🗑️ **Delete** individual sessions or clear all
- Auto-saves up to **50 sessions** to localStorage

The **Export** button lets you download the current research session as:

```
📄 Markdown   — clean, portable, great for notes
🌐 HTML       — styled, shareable in a browser
📦 JSON       — structured data for further processing
```

---

## 🎛️ Research Depth & Focus (New)

When Research mode is active, the composer shows two control groups:

**Depth:**
| Option | Behaviour |
|--------|-----------|
| ⚡ Quick | 1–2 search passes, fast answer |
| ⚖️ Balanced | 2–3 passes with retry (default) |
| 🔬 Deep | Full multi-pass with coverage check |

**Focus:**
| Option | Sources targeted |
|--------|-----------------|
| 🌐 General | All engines (DuckDuckGo, Brave, Mwmbl…) |
| 🎓 Academic | Wikipedia, Wikidata, academic sources |
| 📰 News | News-biased engines |

---

## ⚙️ Development Setup

### Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| **Node.js** | 22+ | [`nodejs.org`](https://nodejs.org) |
| **Rust** | 1.90 (auto-pinned) | [`rustup.rs`](https://rustup.rs) — version pinned by `rust-toolchain.toml` |
| **Vulkan SDK** | Latest | Windows/Linux GPU builds only — [`vulkan.lunarg.com`](https://vulkan.lunarg.com) |

### Run in Development

```bash
# Option A: Use embedded GGUF (drop a .gguf file into the models folder)
# Option B: Use Ollama
ollama serve
ollama pull qwen3:4b

# Install dependencies and start
npm ci
npm run tauri:dev
```

### Research (no setup required)

Research uses the built-in search engine by default — it reads public result
pages directly, so there is nothing to install. A local **SearXNG** is optional
and, when present at `127.0.0.1:8080`, takes priority for multi-engine routing:

```bash
# Optional: richer multi-engine routing
cd tools/searxng && docker compose up -d

# WSL Ubuntu (no Docker)
~/start-searxng.sh

# Dev search proxy on 127.0.0.1:8080
npm run mock:search
```

---

## 🏗️ Build from Source

```bash
# Build for your current OS
npm run tauri:build

# Platform-specific
npm run tauri:build:windows    # → .msi + .exe (NSIS)
npm run tauri:build:macos      # → .app + .dmg
npm run tauri:build:linux      # → .AppImage + .deb
```

> **Note:** Windows & Linux builds require the Vulkan SDK. macOS uses Metal automatically.

---

## 🧪 Quality Gates

```bash
npm run lint                   # ESLint (0 warnings policy)
npm run format:check           # Prettier
npm run build                  # tsc --noEmit + vite build

cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

All gates are enforced in CI on every push and pull request.

---

## 📁 Models Folder

Drop any `.gguf` file here and it appears in the picker immediately:

| OS | Path |
|----|------|
| 🪟 Windows | `%APPDATA%\dev.localresearch.agent\models\` |
| 🍎 macOS | `~/Library/Application Support/dev.localresearch.agent/models/` |
| 🐧 Linux | `~/.local/share/dev.localresearch.agent/models/` |

**Manual download example:**
```bash
curl -L -o "qwen2.5-1.5b-instruct-q4_k_m.gguf" \
  https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf
```

---

## 🗺️ Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                  Local Research Agent                │
│                                                      │
│  ┌──────────────┐    ┌──────────────────────────┐   │
│  │  React + TS  │◄──►│   Tauri v2 IPC Bridge    │   │
│  │  (Frontend)  │    │   (Rust backend)          │   │
│  └──────────────┘    └──────────┬───────────────┘   │
│                                 │                    │
│              ┌──────────────────┼──────────────┐    │
│              │                  │              │    │
│     ┌────────▼──────┐  ┌────────▼───┐  ┌──────▼──┐ │
│     │ llama.cpp     │  │  SearXNG   │  │  Ollama │ │
│     │ (embedded)    │  │  (local)   │  │(optional)│ │
│     │ CPU / GPU     │  │ :8080      │  │  :11434 │ │
│     └───────────────┘  └────────────┘  └─────────┘ │
└─────────────────────────────────────────────────────┘
```

---

## 📋 Implementation Status

### ✅ Implemented

| Phase | Features |
|-------|---------|
| **Phase 0** | Tauri v2 + React 18 + TS + Vite scaffold, Tailwind, Zustand, ESLint/Prettier, cross-platform CI |
| **Phase 1** | `AppEvent` typed bus, `CancellationToken`, broadcast forwarder, Zustand stores |
| **Phase 2** | Adaptive system telemetry (RAM/disk/GPU), NVML + AMD sysfs + Metal, status bar |
| **Phase 3** | Ollama discovery, streaming NDJSON chat, token/metrics events, run insights panel |
| **Embedded Engine** | llama.cpp static link, GGUF scanner, CPU/GPU switch, in-app model downloader, Hugging Face catalog |
| **Phase 4** | Research mode, multi-query planner, SearXNG routing, BM25 retrieval, cited answers, source cards |
| **v0.1.9** | Export dossier (MD/HTML/JSON), session archive, depth controls, domain focus |

### 🚧 Not Yet Implemented

| Feature | Status |
|---------|--------|
| Persistent retrieval (SQLite page cache) | Planned |
| Embedding indexes | Planned |
| Long-term memory | Planned |
| Agent canvas / orchestration | Planned |
| LoRA fine-tuning | Future |
| Auto-updater | Future |
| Code signing / notarization | Future |

---

## 📚 Documentation

| Doc | Description |
|-----|-------------|
| [📐 Architecture](docs/ARCHITECTURE.md) | System design and component overview |
| [⚡ Events](docs/EVENTS.md) | Typed event bus reference |
| [🔎 Research Setup](docs/RESEARCH.md) | SearXNG setup, limits, and troubleshooting |
| [🔧 Build Notes](docs/BUILD.md) | Platform-specific build instructions |
| [🔒 Security](docs/SECURITY.md) | Security model and threat disclosure |

---

## 🤝 Contributing

1. Fork the repo
2. Create a feature branch: `git checkout -b feat/your-feature`
3. Make changes — all quality gates must pass (see above)
4. Open a pull request against `main`

All CI checks run automatically on every PR.

---

## 📄 License

Dual-licensed under **[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)** — your choice.

---

<div align="center">

Made with ❤️ for privacy-first AI  
**[⬇ Download Latest Release](https://github.com/Davood121/local-research-agent/releases/latest)**

</div>
