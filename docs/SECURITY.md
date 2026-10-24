# Security

This document is intentionally short. The rules below apply to the current
milestone and to later work.

## Hard rules

- Zero paid APIs, API keys, cloud inference, analytics, or crash reporting.
- Loopback services are fixed: Ollama at `http://127.0.0.1:11434` and SearXNG at
  `http://127.0.0.1:8080`. There is no user-configurable endpoint.
- The only non-loopback requests are direct page reads selected by Rust from
  SearXNG results during an explicit research run. There is no background
  search, crawl, prefetch, or model-initiated browsing path.
- Cloud-routed Ollama models are excluded. `/api/tags` entries with a
  `remote_host`, a `:cloud` name, or no completion capability are removed.
  `start_chat` and `start_research` re-fetch the list and reject any model that
  is not in the filtered local set, so the UI cannot bypass the filter by
  sending a name.
- Tauri capabilities are deny-by-default. The only capability granted is
  `core:event:allow-listen` + `core:event:allow-unlisten` for the main window.
  No `fs`, `shell`, `http`, or `dialog` permissions exist.
- `withGlobalTauri: false`. No `eval`, no `new Function`, no remote URLs loaded
  in the webview.
- Commands return `Result<T, AppError>` with a human-readable message. No
  command panics. `clippy::unwrap_used`, `expect_used`, and `panic` are denied
  crate-wide; test modules are the only opt-out.
- Long-running tasks are cancellable. Inference uses its own
  `CancellationToken`, research shares it across search, page reads, and
  generation, and app shutdown cancels both.
- No anti-debugging, packers, obfuscation, or hidden network calls.

## Ollama boundary

- The Ollama client is built with `reqwest`, default TLS features disabled, and
  `no_proxy()` so environment/system proxy settings cannot redirect loopback
  traffic. It is not given a base URL from the webview.
- Only three fixed routes are used: `GET /api/tags`, `GET /api/version`, and
  `POST /api/chat`. The model server is never launched, installed, or updated by
  the app.
- IPC arguments are validated before a request is made: run id charset/length,
  model name charset/length, allowed roles, at most 64 messages including the
  app prompt, at most 32,000 characters per message, and 160,000 characters
  total. The last message must be from the user. `start_chat` discards any
  webview-supplied system message and prepends the app-owned chat prompt, so the
  product context cannot be replaced over IPC.
- NDJSON lines are capped at 1 MiB, non-success error bodies are capped and
  sanitised, and malformed lines fail the run with a recoverable error.
- Only one chat or research run may run at a time. The slot has an explicit
  claim/release/cancel lifecycle and is released even after a failed or
  cancelled run.
- Prompts, responses, and model names are not logged by the app, persisted to
  disk by the app, or uploaded. The selected model name is stored in
  `localStorage` for convenience.

## Search and page-read boundary

- Research first tries the loopback SearXNG client, which uses `no_proxy()` and
  only builds `GET /search` from the constant `http://127.0.0.1:8080`. JSON
  output must be enabled by the user. The app does not install or manage
  SearXNG.
- When SearXNG is absent, unreachable, or returns nothing, the built-in engine
  requests `https://html.duckduckgo.com/html/` and falls back to
  `https://www.bing.com/search`. These are the only two outbound search hosts,
  they are hard-coded, and the webview cannot influence them.
- The webview cannot supply a page URL. Rust takes result URLs from the search
  response and validates each one.
- Page URLs must be `http`/`https`, without embedded credentials. `localhost`,
  `.localhost`, `.local`, `.internal`, `.lan`, `.home`, `metadata.google.internal`,
  and literal private addresses are rejected before a request.
- The hostname is resolved immediately before each request and every resolved
  address must be public. Private, loopback, link-local, CGNAT, documentation,
  multicast, and unspecified addresses are rejected, including IPv6 forms.
- The research client has redirects disabled. Rust handles redirects manually
  and runs the same URL and DNS validation on every hop, with a limit of 4.
- Responses are limited to 2.5 MB, extracted text to 180,000 characters per
  page, and page reads to 4 per run. Body reads, DNS, and requests have
  timeouts.
- The reader sends no cookies, credentials, proxy configuration, or API keys.
  It does not execute JavaScript or load subresources.
- Source titles, snippets, URLs, and page text are held only in memory for the
  run. They are not written to disk by the app.
- Residual risk: DNS is validated immediately before the request, but the HTTP
  stack resolves it again. A hostile DNS server could attempt rebinding between
  the two lookups. Pinning the validated address is a later hardening step.

## CSP

Release webview:

```
default-src 'self';
script-src 'self';
style-src 'self' 'unsafe-inline';
img-src 'self' data:;
connect-src ipc: http://ipc.localhost;
object-src 'none';
frame-ancestors 'none';
base-uri 'none'
```

No loopback origin appears in `connect-src`: the webview does not make the
Ollama request. That request happens in Rust. Dev `devCsp` only adds
`ws://localhost:1420` and `http://localhost:1420` for Vite HMR and is never
applied to release builds.

**Deliberate deviation:** `http://ipc.localhost` is added to `connect-src`
because that is the Windows WebView2 IPC origin in Tauri v2. Without it, every
`invoke` and event would be blocked on Windows. No remote origin was added.

`style-src 'unsafe-inline'` is required because Tailwind/React emit inline
styles; it is the only relaxation.

## Webview trust

The webview is untrusted. It has no filesystem or process access. Everything
privileged stays in Rust. Event payloads are validated in
`src/lib/events.ts` before they reach a store. Future shell/HTTP plugins must add
narrow command permissions and must never expose arbitrary URLs or paths.

Assistant Markdown is untrusted model output. It is rendered with
`react-markdown` with raw HTML disabled (`skipHtml`); no `rehype-raw` or
`dangerouslySetInnerHTML` path exists. Link elements are replaced with
non-navigable text, and image elements are replaced with their alt text, so a
reply cannot open a URL or trigger a remote image request. The CSP also omits
remote `img-src` and `connect-src` origins.

## Build surface

- `panic = "abort"`, full LTO, one codegen unit, size optimisation, and symbol
  stripping in release.
- `devtools: false` in release; the dev merge file is only used by
  `npm run tauri:dev`.
- `debug_start_stream` is compiled out of release builds and not registered.
- WebView2 runtime is not downloaded by the installer; the app fails closed when
  it is absent.

## Reporting

The app ships no server component, account system, or telemetry uploader. Report
security issues privately to the maintainer of the distribution you received.
