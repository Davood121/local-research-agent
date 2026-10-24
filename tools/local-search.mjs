#!/usr/bin/env node
// Minimal local search service that speaks the small subset of the SearXNG JSON
// API the desktop app consumes. It exists so research works on a machine with
// no Docker, no Python and no SearXNG: the app probes 127.0.0.1:8080, this
// process answers, and the results are real web results.
//
//   node tools/local-search.mjs            # listens on 127.0.0.1:8080
//   node tools/local-search.mjs --port 8081
//
// It performs one outbound HTTPS request per query to DuckDuckGo (falling back
// to Bing when DuckDuckGo throttles), extracts the organic results, and returns
// them in SearXNG's shape: { results: [{ title, url, content, engine }] }.
// Nothing is logged to disk and no key or account is involved.

import http from "node:http";

const PORT = Number(
  process.argv.includes("--port")
    ? process.argv[process.argv.indexOf("--port") + 1]
    : process.env.PORT || 8080,
);
const HOST = "127.0.0.1";

const UA =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 " +
  "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const MAX_RESULTS = 12;

function decodeEntities(text) {
  return text
    .replace(/&#(\d+);/g, (_, code) => String.fromCharCode(Number(code)))
    .replace(/&#x([0-9a-f]+);/gi, (_, code) => String.fromCharCode(parseInt(code, 16)))
    .replace(/&quot;/g, '"')
    .replace(/&#39;|&apos;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&nbsp;/g, " ")
    .replace(/&amp;/g, "&");
}

function stripTags(html) {
  return decodeEntities(html.replace(/<[^>]*>/g, " "))
    .replace(/\s+/g, " ")
    .trim();
}

/** DuckDuckGo wraps results in //duckduckgo.com/l/?uddg=<encoded target>. */
function unwrapDuckDuckGo(href) {
  const absolute = href.startsWith("//") ? `https:${href}` : href;
  try {
    const url = new URL(absolute);
    const target = url.searchParams.get("uddg");
    return target ? decodeURIComponent(target) : absolute;
  } catch {
    return "";
  }
}

function parseDuckDuckGo(html) {
  const results = [];
  const blocks = html.split('class="result results_links');
  for (const block of blocks.slice(1)) {
    const link = block.match(/class="result__a"[^>]*href="([^"]+)"[^>]*>([\s\S]*?)<\/a>/);
    if (!link) continue;
    const url = unwrapDuckDuckGo(decodeEntities(link[1]));
    if (!/^https?:/i.test(url)) continue;
    const snippet =
      block.match(/class="result__snippet"[^>]*>([\s\S]*?)<\/a>/) ||
      block.match(/class="result__snippet"[^>]*>([\s\S]*?)<\/div>/);
    results.push({
      title: stripTags(link[2]),
      url,
      content: snippet ? stripTags(snippet[1]) : "",
      engine: "duckduckgo",
    });
  }
  return results;
}

function parseBing(html) {
  const results = [];
  const blocks = html.split('<li class="b_algo"');
  for (const block of blocks.slice(1)) {
    const link = block.match(/<h2[^>]*>\s*<a[^>]*href="([^"]+)"[^>]*>([\s\S]*?)<\/a>/);
    if (!link) continue;
    const url = decodeEntities(link[1]);
    if (!/^https?:/i.test(url)) continue;
    const snippet = block.match(/<p[^>]*>([\s\S]*?)<\/p>/);
    results.push({
      title: stripTags(link[2]),
      url,
      content: snippet ? stripTags(snippet[1]) : "",
      engine: "bing",
    });
  }
  return results;
}

async function fetchText(url) {
  const response = await fetch(url, {
    headers: {
      "User-Agent": UA,
      Accept: "text/html,application/xhtml+xml",
      "Accept-Language": "en-US,en;q=0.9",
    },
    redirect: "follow",
    signal: AbortSignal.timeout(15000),
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response.text();
}

async function search(query) {
  const encoded = encodeURIComponent(query);
  try {
    const results = parseDuckDuckGo(
      await fetchText(`https://html.duckduckgo.com/html/?q=${encoded}`),
    );
    if (results.length > 0) return results.slice(0, MAX_RESULTS);
  } catch {
    // Fall through to the second engine.
  }
  const results = parseBing(await fetchText(`https://www.bing.com/search?q=${encoded}&count=20`));
  return results.slice(0, MAX_RESULTS);
}

const server = http.createServer(async (request, response) => {
  const url = new URL(request.url ?? "/", `http://${HOST}:${PORT}`);

  if (url.pathname === "/healthz") {
    response.writeHead(200, { "Content-Type": "text/plain" });
    response.end("OK");
    return;
  }

  if (url.pathname === "/search") {
    const query = (url.searchParams.get("q") ?? "").trim();
    if (!query) {
      response.writeHead(400, { "Content-Type": "application/json" });
      response.end(JSON.stringify({ error: "missing q" }));
      return;
    }
    try {
      const results = await search(query);
      const body = JSON.stringify({
        query,
        number_of_results: results.length,
        results,
        answers: [],
        infoboxes: [],
        suggestions: [],
        unresponsive_engines: [],
      });
      response.writeHead(200, {
        "Content-Type": "application/json; charset=utf-8",
        "Content-Length": Buffer.byteLength(body),
      });
      response.end(body);
      console.log(`[search] ${query} -> ${results.length} results`);
    } catch (error) {
      response.writeHead(502, { "Content-Type": "application/json" });
      response.end(JSON.stringify({ error: String(error?.message ?? error) }));
      console.error(`[search] ${query} failed: ${error?.message ?? error}`);
    }
    return;
  }

  response.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
  response.end("<!doctype html><title>local search</title><p>local search is running");
});

server.listen(PORT, HOST, () => {
  console.log(`local search listening on http://${HOST}:${PORT}`);
});
