import { useEffect, useMemo, useState } from "react";

import { compareSources, compareToCsv, compareToMarkdown } from "../lib/compare";
import { downloadFile } from "../lib/export";
import { useChatStore } from "../stores/chatStore";
import { CheckIcon, CopyIcon, DownloadIcon, GraphIcon, GlobeIcon, XIcon } from "./Icons";

const timeFormatter = new Intl.DateTimeFormat("en-US", {
  hour: "2-digit",
  minute: "2-digit",
});

/**
 * Side-by-side view of every page read for a research run.
 *
 * The "shared terms" column is a local text-overlap measure (see lib/compare),
 * not a model judgement, and the table is honest about that in its subtitle.
 */
export function SourceCompareTable() {
  const [open, setOpen] = useState(false);
  const [selectedRun, setSelectedRun] = useState<string | null>(null);
  const [sort, setSort] = useState<"id" | "shared">("id");
  const [copied, setCopied] = useState(false);
  const messages = useChatStore((state) => state.messages);

  useEffect(() => {
    const onOpen = () => setOpen(true);
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("lra:source-compare", onOpen);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("lra:source-compare", onOpen);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  const runs = useMemo(
    () =>
      messages.filter(
        (entry) =>
          entry.role === "assistant" &&
          entry.mode === "research" &&
          (entry.sources?.length ?? 0) > 0,
      ),
    [messages],
  );

  const activeRun = runs.find((run) => run.id === selectedRun) ?? runs[runs.length - 1];
  const sources = useMemo(() => activeRun?.sources ?? [], [activeRun]);

  const rows = useMemo(() => {
    const computed = compareSources(sources);
    return sort === "shared"
      ? [...computed].sort((a, b) => b.bestShared - a.bestShared || a.source.id - b.source.id)
      : computed;
  }, [sources, sort]);

  const domains = new Set(sources.map((source) => source.domain || "web")).size;
  const readCount = sources.filter((source) => source.read).length;
  const relatedCount = rows.filter((row) => row.peers.length > 0).length;

  const copyMarkdown = async () => {
    if (!navigator.clipboard || rows.length === 0) return;
    try {
      await navigator.clipboard.writeText(compareToMarkdown(rows));
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1_600);
    } catch {
      // Clipboard access can be denied; the CSV download still works.
    }
  };

  const downloadCsv = () => {
    if (rows.length === 0) return;
    downloadFile(compareToCsv(rows), `source-compare-${Date.now()}.csv`, "text/csv");
  };

  const reveal = (url: string) => {
    window.dispatchEvent(new CustomEvent("lra:focus-source", { detail: { url } }));
    setOpen(false);
  };

  if (!open) return null;

  return (
    <div
      className="motion-fade-in fixed inset-0 z-[65] flex items-center justify-center bg-[#02060f]/75 p-4 backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      aria-label="Source comparison"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) setOpen(false);
      }}
    >
      <div className="motion-scale-in flex max-h-[85vh] w-full max-w-4xl flex-col overflow-hidden rounded-2xl border border-white/10 bg-[#0a1120] shadow-[0_30px_80px_rgba(0,0,0,0.6)]">
        <header className="flex shrink-0 items-start justify-between gap-3 border-b border-white/[0.06] px-5 py-4">
          <div className="min-w-0">
            <h2 className="flex items-center gap-2 text-sm font-semibold text-slate-100">
              <GraphIcon className="h-4 w-4 text-sky-400" />
              Source comparison
            </h2>
            <p className="mt-0.5 text-xs text-slate-400">
              Overlap is counted from significant shared terms in titles and snippets — a reading
              aid, not a verdict on which page is right.
            </p>
          </div>
          <button
            type="button"
            onClick={() => setOpen(false)}
            className="press shrink-0 rounded-lg border border-white/10 p-1.5 text-slate-400 hover:border-white/20 hover:text-slate-100"
            aria-label="Close source comparison"
          >
            <XIcon className="h-3.5 w-3.5" />
          </button>
        </header>

        <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-white/[0.06] bg-black/20 px-5 py-2.5">
          {runs.length > 1 ? (
            <select
              value={activeRun?.id ?? ""}
              onChange={(event) => setSelectedRun(event.target.value)}
              className="h-7 max-w-[260px] rounded-lg border border-white/10 bg-[#0a1120] px-2 text-[11px] text-slate-300 focus:border-sky-400/40 focus:outline-none"
              aria-label="Research run"
            >
              {runs.map((run) => (
                <option key={run.id} value={run.id}>
                  {timeFormatter.format(run.createdAt)} · {run.sources?.length ?? 0} sources
                </option>
              ))}
            </select>
          ) : (
            <span className="text-[11px] text-slate-500">
              {activeRun
                ? `Run at ${timeFormatter.format(activeRun.createdAt)}`
                : "No research run yet"}
            </span>
          )}

          <div className="flex items-center gap-1 rounded-lg bg-white/[0.04] p-0.5">
            {(["id", "shared"] as const).map((value) => (
              <button
                key={value}
                type="button"
                onClick={() => setSort(value)}
                className={`rounded-md px-2 py-1 text-[10px] font-semibold uppercase tracking-wider ${
                  sort === value
                    ? "bg-white/[0.08] text-slate-200"
                    : "text-slate-500 hover:text-slate-300"
                }`}
              >
                {value === "id" ? "Citation order" : "Most overlap"}
              </button>
            ))}
          </div>

          <div className="ml-auto flex items-center gap-1.5">
            <button
              type="button"
              onClick={() => void copyMarkdown()}
              disabled={rows.length === 0}
              className="press inline-flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1 text-[11px] font-medium text-slate-300 hover:border-white/20 hover:text-slate-100 disabled:opacity-40"
            >
              {copied ? (
                <CheckIcon className="h-3 w-3 text-emerald-400" />
              ) : (
                <CopyIcon className="h-3 w-3" />
              )}
              {copied ? "Copied" : "Copy table"}
            </button>
            <button
              type="button"
              onClick={downloadCsv}
              disabled={rows.length === 0}
              className="press inline-flex items-center gap-1.5 rounded-lg border border-sky-400/25 bg-sky-400/[0.1] px-2.5 py-1 text-[11px] font-medium text-sky-200 hover:border-sky-400/45 hover:bg-sky-400/[0.18] disabled:opacity-40"
            >
              <DownloadIcon className="h-3 w-3" />
              CSV
            </button>
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-auto">
          {rows.length === 0 ? (
            <div className="px-5 py-14 text-center">
              <p className="text-sm text-slate-400">No sources to compare yet.</p>
              <p className="mt-1 text-xs text-slate-600">
                Run a research prompt and every page it reads will be compared here.
              </p>
            </div>
          ) : (
            <table className="w-full border-collapse text-[11.5px]">
              <thead className="sticky top-0 z-10 bg-[#0c1424] text-[9.5px] uppercase tracking-[0.1em] text-slate-500">
                <tr>
                  <th className="w-10 border-b border-white/[0.07] px-3 py-2 text-left font-semibold">
                    #
                  </th>
                  <th className="border-b border-white/[0.07] px-3 py-2 text-left font-semibold">
                    Source
                  </th>
                  <th className="w-24 border-b border-white/[0.07] px-3 py-2 text-left font-semibold">
                    Access
                  </th>
                  <th className="w-16 border-b border-white/[0.07] px-3 py-2 text-right font-semibold">
                    Terms
                  </th>
                  <th className="w-24 border-b border-white/[0.07] px-3 py-2 text-right font-semibold">
                    Max shared
                  </th>
                  <th className="w-32 border-b border-white/[0.07] px-3 py-2 text-left font-semibold">
                    Related
                  </th>
                  <th className="w-14 border-b border-white/[0.07] px-3 py-2" />
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <tr key={`${row.source.id}-${row.source.url}`} className="hover:bg-white/[0.02]">
                    <td className="border-b border-white/[0.04] px-3 py-2 align-top font-mono text-[10.5px] text-emerald-300/70">
                      {row.source.id}
                    </td>
                    <td className="border-b border-white/[0.04] px-3 py-2 align-top">
                      <p className="line-clamp-2 font-medium leading-snug text-slate-200">
                        {row.source.title}
                      </p>
                      <p
                        className="mt-0.5 truncate font-mono text-[9.5px] text-slate-600"
                        title={row.source.url}
                      >
                        {row.source.domain || "source"}
                        {row.source.engine ? ` · via ${row.source.engine}` : ""}
                      </p>
                    </td>
                    <td className="border-b border-white/[0.04] px-3 py-2 align-top">
                      <span
                        className={`rounded-full border px-1.5 py-0.5 text-[9px] font-medium ${
                          row.source.read
                            ? "border-emerald-400/20 bg-emerald-400/[0.07] text-emerald-300/80"
                            : "border-amber-400/20 bg-amber-400/[0.07] text-amber-300/80"
                        }`}
                      >
                        {row.source.read ? "page read" : "snippet"}
                      </span>
                    </td>
                    <td className="border-b border-white/[0.04] px-3 py-2 text-right align-top font-mono text-[10.5px] text-slate-400">
                      {row.termCount}
                    </td>
                    <td className="border-b border-white/[0.04] px-3 py-2 text-right align-top font-mono text-[10.5px]">
                      <span
                        className={
                          row.bestShared >= 6
                            ? "text-emerald-300/90"
                            : row.bestShared >= 3
                              ? "text-sky-300/90"
                              : "text-slate-500"
                        }
                      >
                        {row.bestShared}
                      </span>
                    </td>
                    <td className="border-b border-white/[0.04] px-3 py-2 align-top">
                      {row.peers.length ? (
                        <span className="font-mono text-[10px] text-slate-400">
                          {row.peers.map((id) => `[${id}]`).join(" ")}
                        </span>
                      ) : (
                        <span className="text-[10px] text-slate-600">unique</span>
                      )}
                    </td>
                    <td className="border-b border-white/[0.04] px-3 py-2 align-top">
                      <button
                        type="button"
                        onClick={() => reveal(row.source.url)}
                        title="Show this source in the sources panel"
                        className="press rounded p-1 text-slate-600 hover:bg-white/[0.05] hover:text-sky-300"
                        aria-label={`Show source ${row.source.id}`}
                      >
                        <GlobeIcon className="h-3 w-3" />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        <footer className="flex shrink-0 items-center gap-3 border-t border-white/[0.06] px-5 py-2 text-[10px] text-slate-600">
          <span>{sources.length} sources</span>
          <span>·</span>
          <span>{domains} domains</span>
          <span>·</span>
          <span>{readCount} pages read</span>
          <span>·</span>
          <span>{relatedCount} with related pages</span>
        </footer>
      </div>
    </div>
  );
}
