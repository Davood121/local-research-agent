import { useEffect, useState } from "react";
import type { ResearchSource } from "../lib/events";
import { useChatStore } from "../stores/chatStore";
import { useSessionStore } from "../stores/sessionStore";
import { useVaultStore } from "../stores/vaultStore";
import { CheckIcon, CopyIcon, ExportIcon, GlobeIcon } from "./Icons";
import { VaultDropzone } from "./VaultDropzone";

export function SourcesPanel() {
  const [activeTab, setActiveTab] = useState<"sources" | "vault">("sources");
  const messages = useChatStore((state) => state.messages);
  const openExport = useSessionStore((state) => state.openExport);
  const vaultDocs = useVaultStore((state) => state.documents);
  const latestResearch = [...messages]
    .reverse()
    .find((entry) => entry.role === "assistant" && entry.mode === "research");
  const sources = latestResearch?.sources ?? [];
  const [copiedId, setCopiedId] = useState<number | null>(null);

  // A citation chip in the transcript can ask the panel to reveal its source.
  useEffect(() => {
    const onFocusSource = (event: Event) => {
      const detail = (event as CustomEvent<{ id: number; url: string }>).detail;
      if (!detail) return;
      setActiveTab("sources");
      window.setTimeout(() => {
        const node = Array.from(document.querySelectorAll<HTMLElement>("[data-source-url]")).find(
          (element) => element.dataset.sourceUrl === detail.url,
        );
        if (!node) return;
        const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
        node.scrollIntoView({ block: "nearest", behavior: reduce ? "auto" : "smooth" });
        node.classList.add("ring-1", "ring-sky-400/50", "border-sky-400/30");
        window.setTimeout(() => {
          node.classList.remove("ring-1", "ring-sky-400/50", "border-sky-400/30");
        }, 2_200);
      }, 60);
    };
    window.addEventListener("lra:focus-source", onFocusSource);
    return () => window.removeEventListener("lra:focus-source", onFocusSource);
  }, []);

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
      {/* Header with tab switcher */}
      <header className="flex h-[52px] shrink-0 items-center justify-between border-b border-white/[0.045] px-3">
        <div className="flex items-center gap-1 rounded-lg bg-white/[0.04] p-0.5">
          <button
            type="button"
            onClick={() => setActiveTab("sources")}
            className={`flex items-center gap-1.5 rounded-md px-2.5 py-1 text-[10.5px] font-semibold transition ${
              activeTab === "sources"
                ? "bg-white/[0.08] text-slate-200 shadow"
                : "text-slate-500 hover:text-slate-300"
            }`}
          >
            <GlobeIcon className="h-3 w-3" />
            Sources
          </button>
          <button
            type="button"
            onClick={() => setActiveTab("vault")}
            className={`flex items-center gap-1.5 rounded-md px-2.5 py-1 text-[10.5px] font-semibold transition ${
              activeTab === "vault"
                ? "bg-white/[0.08] text-slate-200 shadow"
                : "text-slate-500 hover:text-slate-300"
            }`}
          >
            📁 Vault
            {vaultDocs.length > 0 && (
              <span className="rounded-full bg-sky-500/20 px-1 py-0.5 font-mono text-[8px] text-sky-400">
                {vaultDocs.length}
              </span>
            )}
          </button>
        </div>
        {activeTab === "sources" && sources.length > 0 && (
          <button
            type="button"
            onClick={() => openExport()}
            title="Export research dossier"
            className="text-slate-500 hover:text-slate-200 transition"
          >
            <ExportIcon className="h-3.5 w-3.5 text-sky-400/80" />
          </button>
        )}
      </header>

      {/* Sources tab */}
      {activeTab === "sources" && (
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
              {sources.map((source, index) => (
                <li
                  key={`${source.id}-${source.url}`}
                  data-source-id={source.id}
                  data-source-url={source.url}
                  style={{ animationDelay: `${Math.min(index, 8) * 28}ms` }}
                  className="motion-fade-up rounded-xl border border-white/[0.06] bg-white/[0.025] p-3 transition hover:border-white/[0.13] hover:bg-white/[0.04]"
                >
                  <div className="flex items-start justify-between gap-2">
                    <span className="font-mono text-[10.5px] text-emerald-300/70">
                      [{source.id}]
                    </span>
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
      )}

      {/* Vault tab */}
      {activeTab === "vault" && (
        <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-4 pt-3">
          <VaultDropzone />
        </div>
      )}
    </aside>
  );
}
