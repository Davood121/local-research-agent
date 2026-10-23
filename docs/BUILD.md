# Build & platform notes

## Toolchain

- Rust **1.90.0**, pinned by `rust-toolchain.toml` (rustfmt + clippy included)
- Node.js **22 LTS** or newer (CI uses 22)
- npm 10+ (lockfile v3)
- Tauri v2 CLI is installed via `npm ci` (`@tauri-apps/cli`)

The first `cargo` invocation in the repo automatically installs the pinned
toolchain through rustup.

## Runtime dependency: Ollama

Ollama is not bundled, installed, started, or updated by this app. Install it
separately (https://ollama.com), keep it on its default loopback listener, and
pull at least one local completion model:

```bash
ollama serve                # not needed with the desktop app
ollama pull qwen3:4b
ollama list
```

The app uses only `http://127.0.0.1:11434`, filters out `remote_host` and
`:cloud` models, and never downloads a model. Ollama's `OLLAMA_HOST` setting is
not read by the app; changing it would move the daemon away from the expected
listener and the app would correctly fail closed.

## Runtime dependency: SearXNG (research only)

Research also needs a separately installed SearXNG instance at
`http://127.0.0.1:8080` with JSON output enabled. Chat does not need it. The
repository includes a loopback-only Docker example and a deterministic mock:

```bash
cd tools/searxng && docker compose up -d   # real local instance
npm run mock:search                        # mock on 127.0.0.1:8080
```

SearXNG is not bundled, installed, started, or updated by the app. See
[docs/RESEARCH.md](RESEARCH.md) for configuration and limits.

## Per-OS prerequisites

### Windows

1. Install the **Visual Studio 2022 Build Tools** with the *Desktop development
   with C++* workload (MSVC v143 + Windows SDK). The Rust MSVC target needs
   `link.exe`.
2. The Windows installer downloads and installs the **WebView2 Runtime** when
   it is missing. Windows 10/11 systems normally already include it.
3. `rustup` (https://rustup.rs) and Node 22.

### macOS

1. `xcode-select --install` (Command Line Tools provide the SDK and linker).
2. `rustup` and Node 22.
3. Builds are unsigned and not notarized in CI.

### Linux (Ubuntu 24.04+)

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf \
  glslc \
  spirv-headers \
  libvulkan-dev \
  pkg-config \
  libssl-dev \
  build-essential
```

Install `rustup` and Node 22. AppImage/deb builds are unsigned.

## GPU backend

The embedded engine is compiled with a GPU backend so the app can switch between
CPU and GPU inference at runtime. The CPU is always available in the same
binary; the GPU half of the switch is offered only when llama.cpp enumerates a
**discrete** device (integrated GPUs are ignored).

| OS      | Backend | Build prerequisite                                                                                   |
| ------- | ------- | ---------------------------------------------------------------------------------------------------- |
| Windows | Vulkan  | [Vulkan SDK](https://vulkan.lunarg.com/sdk/home), e.g. `winget install KhronosGroup.VulkanSDK`        |
| Linux   | Vulkan  | Vulkan SDK/loader plus `glslc` (e.g. `vulkan-sdk` packages or the LunarG SDK)                        |
| macOS   | Metal   | None beyond the Xcode Command Line Tools                                                             |

llama.cpp compiles its Vulkan shaders during the build, so `glslc` and the Vulkan
headers must be discoverable. On Windows the SDK installer sets `VULKAN_SDK`; a
shell started before the install must be restarted (or the variable exported)
before `cargo`/`tauri` runs:

```powershell
$env:VULKAN_SDK = "C:\VulkanSDK\1.4.363.0"
$env:PATH = "$env:VULKAN_SDK\Bin;$env:PATH"
```

A missing SDK fails the build in `llama-cpp-sys-2` with
`Please install Vulkan SDK and ensure that VULKAN_SDK env variable is set`.
The GPU choice is stored in `<app data>/settings.json`; it is ignored at startup
if no discrete device is present, so a driver change cannot leave every chat
failing.

## Installers and releases

The CI workflow builds Windows **MSI and NSIS EXE** installers, a macOS **DMG**,
and Linux **AppImage and DEB** packages. Each workflow run uploads the generated
installers as per-platform artifacts; download them from the run's **Summary**
page. macOS builds are unsigned and not notarized, so macOS may require the user
to approve the app before opening it. Windows installers are unsigned and may
show a SmartScreen warning. The installer downloads WebView2 if it is not
already installed.

For normal app downloads, open the
[latest GitHub Release](https://github.com/Davood121/local-research-agent/releases/latest)
and download the installer for your operating system:

- **Windows:** run the `*-x64-setup.exe` installer (recommended) or the `.msi`.
- **macOS:** open the `.dmg`, then drag **Local Research Agent** into
  **Applications**. Since the build is unsigned, macOS may ask you to approve it
  in **Privacy & Security** before its first launch.
- **Linux:** install the `.deb` with `sudo apt install ./<package>.deb`, or make
  the `.AppImage` executable with `chmod +x <package>.AppImage` and launch it.
  The AppImage requires a compatible FUSE runtime.

To publish a GitHub Release, update the app version consistently in
`package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, and
`src-tauri/tauri.conf.json`, then push a matching version tag (for example,
`v0.1.4`). CI builds all three platforms and attaches their installers to the
release, making them available from the latest-release link above. A regular
workflow run or manual dispatch builds and uploads artifacts without publishing
a release.

## Commands

```bash
npm ci                 # install exact frontend deps
npm run tauri:dev      # dev app (devtools enabled via tauri.dev.conf.json)
npm run tauri:build    # release bundles for the current OS
npm run lint           # ESLint, zero warnings allowed
npm run format:check   # Prettier check (code, tools, root configs)
npm run build          # tsc --noEmit && vite build
npx tsc --noEmit       # type check only

cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

### Unit-test harness

`cargo test` is headless on every OS. Under `cfg(test)`, the crate root
compiles out the Tauri runtime, the command layer, app state, and the live
telemetry task; only pure logic (including Ollama request validation, NDJSON
parsing, URL/SSRF checks, HTML extraction, chunking, BM25 ranking, and prompt
building) is linked. That keeps tests independent of a display server, WebView2,
and the native window stack.
`cargo clippy --all-targets` still checks the normal desktop target as well as
the test harness, and `tauri build` remains the end-to-end compile check for the
GUI path. The test harness is additionally exempt from dead-code analysis
because it omits the desktop shell, and individual test modules may opt out of
the panic lints for fixtures. The binary target's test harness is disabled
(`test = false`) because it is a thin entry point; all tests live in the
library. The library target is `rlib` only because the app targets desktop
platforms exclusively.

### Bundles per OS

| OS      | Command                              | Output (under `src-tauri/target/release/bundle/`) |
| ------- | ------------------------------------ | ------------------------------------------------- |
| Windows | `npm run tauri:build:windows`        | `msi/*.msi`, `nsis/*.exe`                          |
| macOS   | `npm run tauri:build:macos`          | `macos/*.app`, `dmg/*.dmg`                         |
| Linux   | `npm run tauri:build:linux`          | `appimage/*.AppImage`, `deb/*.deb`                 |

`tauri.conf.json` lists all desktop bundle targets; Tauri ignores targets that
do not belong to the host platform. The per-OS scripts (and CI) pass explicit
`--bundles` values so no cross-platform target is attempted.

## DevTools policy

Release windows set `devtools: false`. For local debugging, `npm run tauri:dev`
merges `src-tauri/tauri.dev.conf.json`, which re-enables devtools. Do not ship
that merge in a release build.

## CI

`.github/workflows/ci.yml` runs on `windows-latest`, `macos-latest`, and
`ubuntu-22.04`:

1. `rustup show` installs the pinned toolchain
2. `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`
3. `npm ci`, `npm run lint`, `npm run format:check`, `npm run build`
4. `tauri build --ci --bundles …` (full **release** build, not debug)

Full release bundles are built on purpose: this exercises the LTO/strip profile
and the installer-size criterion. It makes CI slower (typically 15–25 min per
OS) but catches release-only failures. All actions are pinned to full commit
SHAs and the workflow has `permissions: contents: read`. CI never runs the GUI
or uploads crash data.

## Fail-closed behaviour

| Capability missing | Result |
| ------------------ | ------ |
| WebView2 runtime (Windows) | App cannot render. Installer does **not** contact Microsoft. Manual runtime install required. |
| NVIDIA driver / NVML | NVML init fails → backend falls through to AMD (Linux) or `Unavailable`. |
| AMD `mem_info_vram_*` unreadable | `Unavailable`, disk/RAM continue. |
| `system_profiler` missing, slow, or malformed | Probe times out (3 s) → `Unavailable`; it is never retried per tick. |
| App data directory cannot be created | Disk free shows `0.0` and telemetry continues. |
| Ollama not installed/running | Chat shows the setup screen with `ollama serve` / `ollama pull` guidance; refresh retries. No request leaves the machine. |
| Ollama has no local chat models | "Ollama is running" setup screen; cloud/incompatible models stay hidden. |
| Requested model is not installed locally | `start_chat` returns a command error; no inference request is made. |
| Ollama returns malformed NDJSON or closes early | Recoverable `Error` + failed `Step`; already-streamed text is kept. |
| A second chat is started while one is active | Backend rejects it with a command error; the active run continues. |
| Broadcast subscriber lags | Recoverable `Error` event with dropped count; UI keeps working. |
| Tauri fails to start | Error printed to local stderr; process exits non-zero, nothing is uploaded. |

## Acceptance verification

```bash
# Phase 0 — build, title, CI status
npm run tauri:build:windows   # or :macos / :linux
# Launch the binary; window title must be "Local Research Agent".

# Phase 1 — event pipe
npm run tauri:dev
# Click "Debug Stream". Token counter and tok/s must update for 10 s.
cd src-tauri
cargo clippy --all-targets -- -D warnings
cargo test
cd ..
npx tsc --noEmit

# Phase 2 — telemetry
# Observe the footer: "RAM x/y GB · Disk z GB free · GPU …" or "GPU unavailable".
# No GPU machine: the GPU segment is "GPU unavailable" and no error dialog appears.

# Phase 3 — local chat
ollama serve
ollama list                 # at least one local completion model
# Launch the app. The header must show "N local models" and no :cloud model.
# Select a model, send a prompt, and verify streaming tokens, Stop, first-token
# time, generated count, speed, and the Local inference trace.
# With Ollama stopped, the setup screen must appear and a refresh must recover.
# With a browser-only `npm run dev`, the setup screen is expected (no Tauri IPC).

# Idle resources (60 s averages)
# Windows: Task Manager → Details, sum the app + msedgewebview2 processes.
# Linux:   pidstat -p "$(pgrep -f local-research-agent)" 1 60
# macOS:   top -pid "$(pgrep -f 'Local Research Agent')" -l 60 -s 1

# Installer size (must be < 30 MB)
Get-ChildItem src-tauri/target/release/bundle/msi,src-tauri/target/release/bundle/nsis |
  Select-Object Name, @{n='MB';e={[math]::Round($_.Length/1MB,2)}}
du -h src-tauri/target/release/bundle/appimage/*.AppImage \
      src-tauri/target/release/bundle/deb/*.deb
du -h src-tauri/target/release/bundle/dmg/*.dmg
```

CI verifies builds and tests, not runtime behaviour or installer size. The
resource and status-bar checks are manual by design.

## Known limits

- GPU backend is detected once at startup; hot-plugged GPUs need a restart.
- Apple Silicon is reported as `AppleUnified` using system memory, never as VRAM.
- Ollama and its models are external; the app does not install, pull, start, or
  update them. Cloud-routed models are intentionally unsupported.
- Only one chat or research run runs at a time.
- SearXNG and public result pages are external. The app does not install or
  manage SearXNG; research is explicit and bounded.
- No code signing, notarization, or auto-update in this milestone.
- The installer does not bundle WebView2 (size/offline trade-off above).
