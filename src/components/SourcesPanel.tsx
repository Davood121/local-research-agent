import { useState } from "react";
import type { ResearchSource } from "../lib/events";
import { useChatStore } from "../stores/chatStore";
import { useSessionStore } from "../stores/sessionStore";
import { CheckIcon, CopyIcon, ExportIcon, GlobeIcon } from "./Icons";

export function SourcesPanel() {
  const messages = useChatStore((state) => state.messages);
  const openExport = useSessionStore((state) => state.openExport);
  const latestResearch = [...messages]
    .reverse()
    .find((entry) => entry.role === "assistant" && entry.mode === "research");
  const sources = latestResearch?.sources ?? [];
  const [copiedId, setCopiedId] = useState<number | null>(null);

  const copy = async (source: ResearchSource) => {
    if (!navigator.clipboard) return;
    try {
      await navigator.clipboard.writeText(source.url);
      setCopiedId(source.id);
      window.setTimeout(() => setCopiedId(null), 1_600);
    } catch {
      // The URL remains selectable when clipboard access is denied.
    }
  };

  return (
    <aside
      aria-label="Research sources"
      className="hidden w-[286px] shrink-0 flex-col border-r border-white/[0.05] bg-[#070c16]/80 md:flex xl:w-[306px]"
    >
      <header className="flex h-[52px] shrink-0 items-center justify-between border-b border-white/[0.045] px-4">
        <div className="flex items-center gap-2">
          <GlobeIcon className="h-3.5 w-3.5 text-emerald-400/70" />
          <h2 className="text-[11px] font-semibold uppercase tracking-[0.18em] text-slate-400">
            Sources
          </h2>
        </div>
        <div className="flex items-center gap-2">
          <span className="font-mono text-[9.5px] text-slate-600">
            {sources.length} {sources.length === 1 ? "source" : "sources"}
          </span>
          {sources.length > 0 && (
            <button
              type="button"
              onClick={() => openExport()}
              title="Export research dossier"
              className="text-slate-500 hover:text-slate-200 transition"
            >
              <ExportIcon className="h-3.5 w-3.5 text-sky-400/80" />
            </button>
          )}
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-4 pt-3">
        {sources.length === 0 ? (
          <div className="rounded-xl border border-dashed border-white/[0.07] px-3 py-8 text-center">
            <p className="text-[11px] text-slate-500">No sources yet.</p>
            <p className="mt-1 text-[10px] leading-relaxed text-slate-600">
              Run a research prompt and its result pages will appear here instead of inside the
              conversation.
            </p>
          </div>
        ) : (
          <ol className="space-y-2">
            {sources.map((source) => (
              <li
                key={`${source.id}-${source.url}`}
                className="rounded-xl border border-white/[0.06] bg-white/[0.025] p-3"
              >
                <div className="flex items-start justify-between gap-2">
                  <span className="font-mono text-[10.5px] text-emerald-300/70">[{source.id}]</span>
                  <button
                    type="button"
                    onClick={() => void copy(source)}
                    className="inline-flex shrink-0 items-center gap-1 rounded px-1 py-0.5 text-[9.5px] text-slate-600 transition hover:bg-white/[0.04] hover:text-slate-300"
                    title="Copy source URL"
                  >
                    {copiedId === source.id ? (
                      <CheckIcon className="h-2.5 w-2.5" />
                    ) : (
                      <CopyIcon className="h-2.5 w-2.5" />
                    )}
                    {copiedId === source.id ? "Copied" : "Copy"}
                  </button>
                </div>

                <p
                  className="mt-1 line-clamp-2 text-[11.5px] font-medium leading-snug text-slate-300"
                  title={source.title}
                >
                  {source.title}
                </p>

                <div className="mt-1 flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-[9.5px] text-slate-600">
                  <span className="max-w-full truncate" title={source.url}>
                    {source.domain || "source"}
                  </span>
                  <span aria-hidden="true">·</span>
                  <span className={source.read ? "text-emerald-400/60" : "text-amber-400/60"}>
                    {source.read ? "page read" : "search snippet"}
                  </span>
                  {source.engine ? (
                    <>
                      <span aria-hidden="true">·</span>
                      <span
                        className="max-w-full truncate"
                        title={`SearXNG engine: ${source.engine}`}
                      >
                        via {source.engine}
                      </span>
                    </>
                  ) : null}
                </div>

                {source.snippet ? (
                  <p className="mt-1.5 line-clamp-3 text-[10px] leading-relaxed text-slate-500">
                    {source.snippet}
                  </p>
                ) : null}
              </li>
            ))}
          </ol>
        )}
      </div>
    </aside>
  );
}
