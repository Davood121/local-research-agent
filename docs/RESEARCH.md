# Local research

Research is an explicit mode in the composer. It runs a local SearXNG search,
reads a bounded number of the returned public pages, ranks excerpts locally with
BM25, and asks the selected local Ollama model for an answer with numbered
citations. It is the only path that makes non-loopback requests, and it only
runs after you press **Research**.

## Prerequisites

- Ollama at `http://127.0.0.1:11434` with at least one local chat model.
- SearXNG at `http://127.0.0.1:8080` with **JSON output enabled**.
- The app does not install, start, or configure either service. It never accepts
  an alternate endpoint from the webview.

## SearXNG with Docker (recommended)

The example configuration is in [`tools/searxng`](../tools/searxng). It keeps the
published port on loopback only.

```bash
cd tools/searxng
# Change server.secret_key in settings.yml before sharing the instance.
docker compose up -d
```

Check that the JSON API is enabled before opening the app:

```bash
curl "http://127.0.0.1:8080/search?q=local-first&format=json"
```

A JSON object with a `results` array means the app is ready. If you see HTML,
the instance rejected the `format=json` request; add `json` to
`search.formats` in its `settings.yml` and restart it:

```yaml
search:
  formats:
    - html
    - json
```

The app probes `/healthz` and `/` every 30 seconds. It does not probe the search
API in the background, because that would create search traffic without a user
request. Press **Retry** in the composer, or wait for the status refresh, after
starting the service.

### Native SearXNG

A native install works too, provided it listens on `127.0.0.1:8080` and returns
JSON. Follow the official [SearXNG installation guide](https://docs.searxng.org/admin/installation.html),
add the `json` format to your settings file, and start it. The app does not
depend on Docker specifically.

Do not expose this instance to other machines. It is intended as a single-user
local service.

## Search engines

The app talks to one local SearXNG instance; SearXNG is the open-source
metasearch layer that fans a query out to several engines. The bundled
`tools/searxng/settings.yml` keeps SearXNG's broad defaults and arranges a
keyless, independent mix:

| Engine | Role |
| --- | --- |
| DuckDuckGo | Broad web coverage (SearXNG default) |
| Brave | Broad web coverage (SearXNG default) |
| Wikipedia | Open encyclopedia; now returns article results, not only infoboxes |
| Wikidata | Open knowledge base; now returns article results |
| Mwmbl | Free-libre, non-profit search index |
| Wiby | Independent index of the small/old web |
| Yep | Independent general index |

No API key is required for this set. The engines are merged with the shipped
SearXNG defaults by name, so the two broad defaults stay enabled. Mwmbl and Wiby
are weighted down (`weight: 0.2`) because they return many results for broad
queries; without that, they crowd the six-result window. SearXNG itself is open
source; DuckDuckGo, Brave, and Yep use their own indexes and terms, so “open
source” describes the aggregator and the open knowledge/index engines, not every
upstream service.

Some engines are intentionally **not** enabled:

- **Marginalia** requires an API key for its public API.
- **Mojeek** and **Startpage** currently use proof-of-work CAPTCHAs in SearXNG.
- **Swisscows** is marked inactive upstream.
- **Bing** and **Google web** are proprietary and are frequently rate-limited
  or blocked when queried by a metasearch instance. SearXNG's default
  `google cse` engine is explicitly disabled because it dominated broad queries
  with Google Earth and map results.
- **Qwant** returned `access denied` from the test network, so the bundled
  settings leave it disabled. Re-enable it if it works from your network.

If one engine is blocked or slow, SearXNG records it as unresponsive and still
returns results from the others; the app treats that as a normal partial result.
Source cards show `via <engine>` so you can see which engine supplied each page.
To add or remove engines, edit `tools/searxng/settings.yml` and restart
SearXNG, then check the instance's **Preferences → Engines** page and logs.

## Using research

1. Select **Research** in the composer.
2. Ask a question. The status row shows **SearXNG ready** when search is
   reachable. Leading commands such as `find` or `tell me about` and trailing
   punctuation are removed from the search query only; the full prompt still
   goes to the model.
3. The run emits a **Web search** step, then **Read pages**, then fills the
   left-hand **Sources** panel. The local model streams the answer with `[n]`
   citations, but the source list is not repeated under the chat message. The
   prompt tells the model not to append a separate bibliography; the panel owns
   that list.
4. **Stop** cancels search, page reads, or generation. Search and page tasks are
   aborted, not just hidden.
5. Source cards show the title, domain, snippet, whether the page was actually
   read, and a **Copy** action. The app never opens an external browser or
   webview.

Citations are bounded to the sources returned in that run. If a small model
writes an unknown number such as `[7]` when only two sources exist, the UI drops
the invalid label and keeps valid labels from the same group; the prompt also
states the exact valid label set. This is a display safety net, not a guarantee
about a model's reasoning.

## What the reader does

Only after a research request:

- Queries `GET http://127.0.0.1:8080/search` with JSON, general category, and
  safe search on. At most 6 results are normalized.
- Selects up to 4 result URLs. The webview never supplies or overrides a page
  URL.
- Reads each URL over HTTP or HTTPS with redirects disabled at the HTTP client.
  Redirects are followed manually so every hop is revalidated.
- Rejects non-HTTP(S), embedded credentials, `localhost`, `.local`, `.internal`,
  and other blocked hostnames. Literal private addresses are rejected before
  DNS. The hostname is resolved immediately before the request and every
  resolved address must be public.
- Rejects private, loopback, link-local, CGNAT, documentation, multicast, and
  unspecified addresses, including IPv6 and IPv4-mapped forms.
- Sends no cookies, credentials, proxy configuration, or API keys. It does not
  execute JavaScript, load images/subresources, or use a browser engine.
- Caps each response at 2.5 MB, extracted text at 180,000 characters, redirects
  at 4, and page reads at 4. DNS and body reads have timeouts.
- Extracts readable HTML or plain text locally. JavaScript-only pages fall back
  to the search-engine snippet instead of running page scripts.
- Ranks chunks with in-memory BM25, limits how much one source can dominate the
  context, and asks the local model to answer only from those excerpts.

The residual risk is DNS rebinding: the address is checked immediately before
the request, but the HTTP stack resolves the name again. A hostile DNS server
could in principle return a public address to the check and a private address to
the connection. This is documented rather than hidden; pinning the validated
address would require a custom connector and is a later hardening step.

## Limits

| Limit | Value |
| --- | --- |
| Search query sent to SearXNG | 500 characters (after command/punctuation cleanup) |
| Research prompt | 4,000 characters |
| Search results kept | 6 |
| Pages read per run | 4 |
| Page body | 2.5 MB |
| Extracted text per page | 180,000 characters |
| Context chunks | 6 |
| Redirects | 4 |
| Research history messages | 16 |

Retrieval is per-run and in-memory. There is no SQLite library, persistent page
cache, saved project, or embedding index yet. Reloading the app discards the
conversation and source text. Those are the next Phase 4B slice if persistent
retrieval is needed.

SearXNG itself contacts its configured search engines. That traffic is outside
this app's process and is governed by the SearXNG instance you run. The app makes
no analytics, crash-reporting, or cloud-inference request.

## Mock SearXNG

For UI and reader testing without Docker:

```bash
npm run mock:search
```

The mock listens on `127.0.0.1:8080`, returns deterministic results that point to
real public pages, and implements `/healthz`, `/`, and JSON search. It is a test
fixture, not a search engine; never expose it beyond loopback.

Useful variants:

```bash
# Slow every search by 6 seconds to exercise Stop during search
$env:MOCK_DELAY_MS=6000; npm run mock:search   # PowerShell
MOCK_DELAY_MS=6000 npm run mock:search         # bash

# Run on another local port (the app still expects 8080, so this is for API tests)
$env:MOCK_PORT=8888; npm run mock:search
```

The mock's results cover “local-first software”, Earth/planet queries,
RAG/fine-tuning queries, and a generic example fallback.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| **Search offline** | Is SearXNG listening on `127.0.0.1:8080`? `curl http://127.0.0.1:8080/healthz` |
| **SearXNG did not return JSON** | Add `json` to `search.formats`, restart SearXNG |
| **SearXNG rejected the search** | Inspect the instance logs; a limiter or upstream engine failure can return an error page |
| **No usable results** | Try a shorter query or check the instance's engines in its web UI |
| **Only snippets, no page read** | The page may block non-browser clients, use JavaScript, or return non-text content. The answer uses the search snippet instead |
| **Readable text is empty** | The page is likely a shell that requires JavaScript; the app does not execute scripts |
| **Research is slow** | First token includes model load time. Page reads are concurrent but bounded; **Stop** cancels the run |
| **Ollama offline** | Start Ollama and refresh the model list; research reuses the same local model checks as chat |

If the app rejects a source, it is intentionally protecting the local network.
Do not weaken the private-address checks to make one page work.
