/**
 * Development-only SearXNG mock.
 *
 * It implements just enough of SearXNG's JSON API for the desktop app's local
 * research flow: `GET /search?q=...&format=json`, `/healthz`, and `/`.
 * Results point at real public pages so the app still exercises its own
 * bounded page reader, extraction, ranking and citation UI.
 *
 * This is not a search engine and must never be exposed beyond 127.0.0.1.
 */
import http from "node:http";

const HOST = "127.0.0.1";
const PORT = Number(process.env.MOCK_PORT ?? 8080);
const DELAY_MS = Number(process.env.MOCK_DELAY_MS ?? 0);

function resultsFor(query) {
  const normalized = query.toLowerCase();
  if (normalized.includes("local-first") || normalized.includes("local first")) {
    return [
      {
        url: "https://en.wikipedia.org/wiki/Local-first_software",
        title: "Local-first software - Wikipedia",
        content:
          "Local-first software is a set of principles for software that stores primary data on the user's device while still supporting collaboration.",
        engine: "mock-wikipedia",
      },
      {
        url: "https://www.inkandswitch.com/essay/local-first/",
        title: "Local-first software: You own your data, in spite of the cloud",
        content:
          "The local-first essay argues that collaborative software should keep primary copies of data on local devices, with the cloud as an optional sync layer.",
        engine: "mock-inkandswitch",
      },
    ];
  }

  if (
    normalized.includes("rag") ||
    normalized.includes("retrieval") ||
    normalized.includes("fine-tun")
  ) {
    return [
      {
        url: "https://en.wikipedia.org/wiki/Retrieval-augmented_generation",
        title: "Retrieval-augmented generation - Wikipedia",
        content:
          "Retrieval-augmented generation combines an information retrieval component with a language model to ground answers in external documents.",
        engine: "mock-wikipedia",
      },
      {
        url: "https://www.pinecone.io/learn/retrieval-augmented-generation/",
        title: "Retrieval-augmented generation explained",
        content:
          "RAG retrieves relevant passages from a knowledge base and supplies them to a model as context, reducing the need to retrain the model.",
        engine: "mock-pinecone",
      },
    ];
  }

  if (normalized.includes("earth") || normalized.includes("planet")) {
    return [
      {
        url: "https://en.wikipedia.org/wiki/Earth",
        title: "Earth - Wikipedia",
        content:
          "Earth is the third planet from the Sun and the only astronomical object known to harbor life. About 71% of Earth's surface is covered by water.",
        engine: "mock-wikipedia",
      },
      {
        url: "https://science.nasa.gov/earth/facts/",
        title: "Facts About Earth - NASA Science",
        content:
          "Earth is the third planet from the Sun, and the only place we know of so far that is inhabited by living things. NASA studies Earth as a system of land, water, air, and life.",
        engine: "mock-nasa",
      },
    ];
  }

  return [
    {
      url: "https://example.com/",
      title: "Example Domain",
      content:
        "This domain is for use in illustrative examples in documents. It is a simple, stable page for testing.",
      engine: "mock-example",
    },
    {
      url: "https://www.iana.org/help/example-domains",
      title: "IANA example domains",
      content:
        "The example domains are reserved for documentation and testing. They are not intended for real services.",
      engine: "mock-iana",
    },
  ];
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

const server = http.createServer((request, response) => {
  const url = new URL(request.url ?? "/", `http://${HOST}:${PORT}`);

  if (request.method !== "GET") {
    sendJson(response, 405, { error: "mock only supports GET" });
    return;
  }

  if (url.pathname === "/healthz") {
    response.writeHead(200, { "content-type": "text/plain" });
    response.end("OK");
    return;
  }

  if (url.pathname === "/") {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end("<!doctype html><title>SearXNG mock</title><h1>Local SearXNG mock</h1>");
    return;
  }

  if (url.pathname !== "/search") {
    sendJson(response, 404, { error: "not found" });
    return;
  }

  if (url.searchParams.get("format") !== "json") {
    sendJson(response, 400, { error: "this mock requires format=json" });
    return;
  }

  const query = (url.searchParams.get("q") ?? "").trim();
  if (!query) {
    sendJson(response, 400, { error: "missing q" });
    return;
  }

  const results = resultsFor(query);
  const payload = {
    query,
    number_of_results: results.length,
    results: results.map((result, index) => ({
      ...result,
      score: 1 - index * 0.01,
      category: "general",
      pretty_url: result.url,
    })),
    answers: [],
    corrections: [],
    infoboxes: [],
    suggestions: [],
    unresponsive_engines: [],
  };

  const finish = () => {
    sendJson(response, 200, payload);
    console.log(`mock search q=${JSON.stringify(query)} results=${results.length}`);
  };

  if (DELAY_MS > 0) {
    setTimeout(finish, DELAY_MS);
  } else {
    finish();
  }
});

server.listen(PORT, HOST, () => {
  console.log(`SearXNG mock listening on http://${HOST}:${PORT}`);
  console.log("Use the Research toggle in the app. Ctrl+C to stop.");
});
