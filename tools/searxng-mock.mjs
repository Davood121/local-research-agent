/**
 * Local Research Agent – Free Search Proxy
 *
 * Implements the SearXNG JSON API (`GET /search?q=...&format=json`, `/healthz`, `/`)
 * so the Tauri backend needs zero changes.
 *
 * Uses 100% FREE, no-API-key search sources:
 *   1. DuckDuckGo Instant Answer API  (ddg-ia)
 *   2. Wikipedia Search API           (wikipedia)
 *   3. Wikipedia OpenSearch           (wikipedia-suggest)
 *
 * No API keys. No accounts. No cost. Runs fully locally at 127.0.0.1:8080.
 *
 * Start:  node tools/searxng-mock.mjs
 * Or:     npm run mock:search
 */

import http from "node:http";
import https from "node:https";
import { URL } from "node:url";

const HOST = "127.0.0.1";
const PORT = Number(process.env.MOCK_PORT ?? 8080);
const MAX_RESULTS = 8;
const FETCH_TIMEOUT_MS = 8000;

// ─── HTTP helpers ────────────────────────────────────────────────────────────

function getJson(url) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(
      () => reject(new Error(`Timeout fetching ${url}`)),
      FETCH_TIMEOUT_MS,
    );
    const lib = url.startsWith("https") ? https : http;
    lib
      .get(
        url,
        {
          headers: {
            "User-Agent":
              "LocalResearchAgent/0.1.9 (free search proxy; +https://github.com/Davood121/local-research-agent)",
            Accept: "application/json",
          },
        },
        (res) => {
          let data = "";
          res.on("data", (chunk) => (data += chunk));
          res.on("end", () => {
            clearTimeout(timeout);
            try {
              resolve(JSON.parse(data));
            } catch {
              reject(new Error(`JSON parse error from ${url}`));
            }
          });
        },
      )
      .on("error", (err) => {
        clearTimeout(timeout);
        reject(err);
      });
  });
}

function sendJson(response, status, payload) {
  const body = JSON.stringify(payload, null, 2);
  response.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "cache-control": "no-store",
    "content-length": Buffer.byteLength(body),
  });
  response.end(body);
}

// ─── Search sources ──────────────────────────────────────────────────────────

/**
 * DuckDuckGo Instant Answer API (free, no key).
 * Returns abstract + related topics as search hits.
 */
async function searchDuckDuckGo(query) {
  try {
    const url = `https://api.duckduckgo.com/?q=${encodeURIComponent(query)}&format=json&no_redirect=1&no_html=1&skip_disambig=1`;
    const data = await getJson(url);
    const results = [];

    // Main abstract
    if (data.AbstractURL && data.Abstract) {
      results.push({
        url: data.AbstractURL,
        title: data.Heading || query,
        content: data.Abstract,
        engine: "duckduckgo",
      });
    }

    // Related topics
    for (const topic of data.RelatedTopics ?? []) {
      if (results.length >= MAX_RESULTS) break;
      if (topic.FirstURL && topic.Text) {
        results.push({
          url: topic.FirstURL,
          title: topic.Text.split(" - ")[0] || topic.FirstURL,
          content: topic.Text,
          engine: "duckduckgo",
        });
      }
      // Handle nested topic groups
      for (const sub of topic.Topics ?? []) {
        if (results.length >= MAX_RESULTS) break;
        if (sub.FirstURL && sub.Text) {
          results.push({
            url: sub.FirstURL,
            title: sub.Text.split(" - ")[0] || sub.FirstURL,
            content: sub.Text,
            engine: "duckduckgo",
          });
        }
      }
    }

    // Infobox entity
    if (data.Infobox?.content) {
      for (const item of data.Infobox.content) {
        if (item.label && item.value && typeof item.value === "string") {
          if (results[0]) {
            results[0].content += ` | ${item.label}: ${item.value}`;
          }
        }
      }
    }

    return results;
  } catch (err) {
    console.error("[ddg]", err.message);
    return [];
  }
}

/**
 * Wikipedia full-text search API (free, no key).
 * Returns real article titles, URLs, and snippets.
 */
async function searchWikipedia(query) {
  try {
    const url =
      `https://en.wikipedia.org/w/api.php?action=query&list=search` +
      `&srsearch=${encodeURIComponent(query)}&srlimit=${MAX_RESULTS}` +
      `&srprop=snippet|titlesnippet&format=json&origin=*`;
    const data = await getJson(url);
    const items = data?.query?.search ?? [];
    return items.map((item) => ({
      url: `https://en.wikipedia.org/wiki/${encodeURIComponent(item.title.replace(/ /g, "_"))}`,
      title: item.title,
      content: item.snippet.replace(/<[^>]+>/g, ""), // strip highlight tags
      engine: "wikipedia",
    }));
  } catch (err) {
    console.error("[wikipedia]", err.message);
    return [];
  }
}

/**
 * Wikipedia OpenSearch (autocomplete-style, returns page extracts).
 */
async function searchWikipediaOpen(query) {
  try {
    const url =
      `https://en.wikipedia.org/w/api.php?action=opensearch` +
      `&search=${encodeURIComponent(query)}&limit=${MAX_RESULTS}&format=json`;
    const data = await getJson(url);
    // OpenSearch format: [query, [titles], [descriptions], [urls]]
    const titles = data[1] ?? [];
    const descs = data[2] ?? [];
    const urls = data[3] ?? [];
    return titles.map((title, i) => ({
      url: urls[i] || `https://en.wikipedia.org/wiki/${encodeURIComponent(title)}`,
      title,
      content: descs[i] || title,
      engine: "wikipedia-suggest",
    }));
  } catch (err) {
    console.error("[wikipedia-opensearch]", err.message);
    return [];
  }
}

/**
 * Wikipedia page summary via REST API — richer content for top result.
 */
async function getWikipediaSummary(title) {
  try {
    const slug = encodeURIComponent(title.replace(/ /g, "_"));
    const url = `https://en.wikipedia.org/api/rest_v1/page/summary/${slug}`;
    const data = await getJson(url);
    if (data.extract && data.content_urls?.desktop?.page) {
      return {
        url: data.content_urls.desktop.page,
        title: data.title,
        content: data.extract,
        engine: "wikipedia-summary",
      };
    }
  } catch {
    // ignore
  }
  return null;
}

// ─── Result merging ──────────────────────────────────────────────────────────

function dedupeByUrl(results) {
  const seen = new Set();
  return results.filter((r) => {
    const key = r.url.replace(/\/$/, "").toLowerCase();
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

async function search(query) {
  // Run all sources in parallel
  const [ddgResults, wikiResults, wikiOpenResults] = await Promise.all([
    searchDuckDuckGo(query),
    searchWikipedia(query),
    searchWikipediaOpen(query),
  ]);

  // Fetch rich summary for the top Wikipedia hit
  let wikiSummary = null;
  if (wikiResults[0]) {
    wikiSummary = await getWikipediaSummary(wikiResults[0].title);
  }

  // Merge: put Wikipedia summary first (most reliable), then DDG, then wiki search
  const merged = [
    ...(wikiSummary ? [wikiSummary] : []),
    ...ddgResults,
    ...wikiResults,
    ...wikiOpenResults,
  ];

  return dedupeByUrl(merged).slice(0, MAX_RESULTS);
}

// ─── HTTP server ─────────────────────────────────────────────────────────────

const server = http.createServer(async (request, response) => {
  const url = new URL(request.url ?? "/", `http://${HOST}:${PORT}`);

  if (request.method !== "GET") {
    sendJson(response, 405, { error: "only GET is supported" });
    return;
  }

  // Health check — Rust backend probes this
  if (url.pathname === "/healthz") {
    response.writeHead(200, { "content-type": "text/plain" });
    response.end("OK");
    return;
  }

  // Root page
  if (url.pathname === "/") {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(
      `<!doctype html><title>Local Research Agent – Free Search</title>
       <h1>✅ Free Search Proxy Running</h1>
       <p>Sources: DuckDuckGo Instant Answer, Wikipedia. No API keys needed.</p>
       <p>Used by Local Research Agent on port ${PORT}.</p>`,
    );
    return;
  }

  // Search endpoint — must match SearXNG's JSON API
  if (url.pathname !== "/search") {
    sendJson(response, 404, { error: "not found" });
    return;
  }

  if (url.searchParams.get("format") !== "json") {
    sendJson(response, 400, { error: "format=json required" });
    return;
  }

  const query = (url.searchParams.get("q") ?? "").trim();
  if (!query) {
    sendJson(response, 400, { error: "missing q" });
    return;
  }

  try {
    const results = await search(query);
    console.log(
      `[search] q=${JSON.stringify(query)} → ${results.length} results` +
        ` (${[...new Set(results.map((r) => r.engine))].join(", ")})`,
    );

    // Return in SearXNG JSON format so Rust needs no changes
    sendJson(response, 200, {
      query,
      number_of_results: results.length,
      results: results.map((r, i) => ({
        url: r.url,
        title: r.title,
        content: r.content,
        engine: r.engine,
        score: 1 - i * 0.05,
        category: "general",
        pretty_url: r.url,
      })),
      answers: [],
      corrections: [],
      infoboxes: [],
      suggestions: [],
      unresponsive_engines: [],
    });
  } catch (err) {
    console.error("[search error]", err);
    sendJson(response, 500, { error: err.message });
  }
});

server.listen(PORT, HOST, () => {
  console.log("");
  console.log("╔══════════════════════════════════════════════════════╗");
  console.log("║   Local Research Agent – Free Search Proxy           ║");
  console.log("║   No API keys. No accounts. 100% free.               ║");
  console.log("╠══════════════════════════════════════════════════════╣");
  console.log(`║   Listening on  http://${HOST}:${PORT}                    ║`);
  console.log("║   Sources:  DuckDuckGo  ·  Wikipedia                 ║");
  console.log("║   Press Ctrl+C to stop                               ║");
  console.log("╚══════════════════════════════════════════════════════╝");
  console.log("");
  console.log("Switch the app composer to Research mode, then ask anything.");
  console.log("");
});
