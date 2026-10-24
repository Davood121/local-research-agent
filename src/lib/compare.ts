import type { ResearchSource } from "./events";

/**
 * Cross-source comparison.
 *
 * There is no language model in this loop: "agreement" is measured as the
 * number of significant terms two pages share. That is a coarse signal, so the
 * UI labels it as shared terms rather than claiming the pages agree.
 */

const STOP_WORDS = new Set([
  "about",
  "after",
  "against",
  "all",
  "also",
  "among",
  "and",
  "any",
  "are",
  "because",
  "been",
  "before",
  "being",
  "between",
  "both",
  "but",
  "can",
  "could",
  "did",
  "does",
  "each",
  "for",
  "from",
  "had",
  "has",
  "have",
  "here",
  "how",
  "into",
  "its",
  "may",
  "might",
  "more",
  "most",
  "must",
  "not",
  "now",
  "off",
  "only",
  "other",
  "our",
  "out",
  "over",
  "same",
  "should",
  "some",
  "such",
  "than",
  "that",
  "the",
  "their",
  "them",
  "then",
  "there",
  "these",
  "they",
  "this",
  "those",
  "through",
  "under",
  "use",
  "used",
  "using",
  "was",
  "were",
  "what",
  "when",
  "where",
  "which",
  "while",
  "who",
  "why",
  "will",
  "with",
  "would",
  "you",
  "your",
]);

/** Two sources need at least this many shared terms to be called related. */
export const MIN_SHARED_TERMS = 3;

export type SourceComparison = {
  source: ResearchSource;
  /** Number of significant terms extracted from title + snippet. */
  termCount: number;
  /** Largest number of shared terms with any other source. */
  bestShared: number;
  /** Ids of sources sharing at least MIN_SHARED_TERMS terms. */
  peers: number[];
};

function significantTerms(source: ResearchSource): Set<string> {
  const words = `${source.title} ${source.snippet}`
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((word) => word.length >= 4 && !STOP_WORDS.has(word) && !/^\d+$/.test(word));
  return new Set(words);
}

export function compareSources(sources: ResearchSource[]): SourceComparison[] {
  const terms = sources.map(significantTerms);

  return sources.map((source, index) => {
    let bestShared = 0;
    const peers = new Set<number>();

    for (let other = 0; other < sources.length; other += 1) {
      if (other === index) continue;
      let shared = 0;
      for (const term of terms[index]) {
        if (terms[other].has(term)) shared += 1;
      }
      if (shared > bestShared) bestShared = shared;
      if (shared >= MIN_SHARED_TERMS) peers.add(sources[other].id);
    }

    return {
      source,
      termCount: terms[index].size,
      bestShared,
      peers: Array.from(peers).sort((a, b) => a - b),
    };
  });
}

function csvCell(value: string | number | boolean): string {
  const text = String(value);
  return /[",\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}

/** CSV for spreadsheet users; one row per source. */
export function compareToCsv(rows: SourceComparison[]): string {
  const header = [
    "id",
    "title",
    "domain",
    "url",
    "access",
    "engine",
    "significant_terms",
    "max_shared_terms",
    "related_ids",
  ];
  const body = rows.map((row) =>
    [
      row.source.id,
      row.source.title,
      row.source.domain || "web",
      row.source.url,
      row.source.read ? "page read" : "search snippet",
      row.source.engine || "builtin",
      row.termCount,
      row.bestShared,
      row.peers.join(" "),
    ]
      .map(csvCell)
      .join(","),
  );
  return [header.join(","), ...body].join("\n");
}

/** Markdown table, so the comparison can be pasted into notes or a report. */
export function compareToMarkdown(rows: SourceComparison[]): string {
  const lines = [
    "| # | Source | Domain | Access | Max shared terms | Related sources |",
    "|---|--------|--------|--------|------------------|-----------------|",
  ];
  for (const row of rows) {
    const title = row.source.title.replace(/\|/g, "-");
    lines.push(
      `| [${row.source.id}] | ${title} | \`${row.source.domain || "web"}\` | ${
        row.source.read ? "read" : "snippet"
      } | ${row.bestShared} | ${row.peers.length ? row.peers.map((id) => `[${id}]`).join(" ") : "—"} |`,
    );
  }
  return lines.join("\n");
}
