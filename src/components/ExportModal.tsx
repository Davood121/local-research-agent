import { useEffect, useState } from "react";
import { CheckIcon, CopyIcon, DownloadIcon, XIcon } from "./Icons";
import {
  downloadFile,
  generateHtmlReport,
  generateJsonExport,
  generateMarkdownReport,
  type ExportFormat,
  type ResearchReportData,
} from "../lib/export";
import { useChatStore } from "../stores/chatStore";

interface ExportModalProps {
  isOpen: boolean;
  onClose: () => void;
  targetEntryId?: string | null;
}

export function ExportModal({ isOpen, onClose, targetEntryId }: ExportModalProps) {
  const messages = useChatStore((state) => state.messages);
  const selectedModel = useChatStore((state) => state.selectedModel);
  const lastDurationMs = useChatStore((state) => state.lastDurationMs);

  const [format, setFormat] = useState<ExportFormat>("markdown");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!isOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  // Determine what to export: specific assistant entry or whole conversation
  const targetEntry = targetEntryId
    ? messages.find((m) => m.id === targetEntryId)
    : [...messages].reverse().find((m) => m.role === "assistant");

  const lastUserMsg = messages.find((m) => m.role === "user");
  const title = lastUserMsg?.content.slice(0, 60) || "Research Session";

  const reportData: ResearchReportData = {
    title,
    query: lastUserMsg?.content,
    createdAt: targetEntry?.createdAt ?? Date.now(),
    model: targetEntry?.model ?? selectedModel ?? "Local Model",
    content: targetEntry?.content ?? "No content generated yet.",
    sources: targetEntry?.sources ?? [],
    metrics: {
      durationMs: lastDurationMs,
    },
  };

  let previewContent = "";
  let filename = "";
  let mimeType = "";

  if (format === "markdown") {
    previewContent = generateMarkdownReport(reportData);
    filename = `research-report-${Date.now()}.md`;
    mimeType = "text/markdown";
  } else if (format === "html") {
    previewContent = generateHtmlReport(reportData);
    filename = `research-report-${Date.now()}.html`;
    mimeType = "text/html";
  } else {
    previewContent = generateJsonExport(messages);
    filename = `research-session-${Date.now()}.json`;
    mimeType = "application/json";
  }

  const handleCopy = async () => {
    if (!navigator.clipboard) return;
    try {
      await navigator.clipboard.writeText(previewContent);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      // Ignore
    }
  };

  const handleDownload = () => {
    downloadFile(previewContent, filename, mimeType);
  };

  return (
    <div
      className="motion-fade-in fixed inset-0 z-50 flex items-center justify-center bg-[#02060f]/75 p-4 backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      aria-label="Export Research"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="motion-scale-in flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-2xl border border-white/10 bg-[#0a1120] shadow-[0_30px_80px_rgba(0,0,0,0.6)]">
        <header className="flex shrink-0 items-center justify-between border-b border-white/[0.06] px-5 py-4">
          <div>
            <h2 className="text-sm font-semibold text-slate-100">Export Research Dossier</h2>
            <p className="mt-0.5 text-xs text-slate-400">
              Save or share findings, citations, and metadata with no cloud dependency.
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="flex h-8 w-8 items-center justify-center rounded-lg border border-white/10 bg-white/[0.04] text-slate-400 transition hover:border-white/20 hover:text-slate-200"
          >
            <XIcon className="h-3.5 w-3.5" />
          </button>
        </header>

        <div className="flex shrink-0 items-center justify-between gap-3 border-b border-white/[0.06] bg-black/20 px-5 py-2.5">
          <div className="flex gap-1.5">
            {(["markdown", "html", "json"] as const).map((fmt) => (
              <button
                key={fmt}
                type="button"
                onClick={() => setFormat(fmt)}
                className={`rounded-lg px-3 py-1.5 text-xs font-semibold uppercase tracking-wider transition ${
                  format === fmt
                    ? "bg-sky-400/20 text-sky-200 border border-sky-400/30"
                    : "text-slate-400 hover:text-slate-200 hover:bg-white/[0.04]"
                }`}
              >
                {fmt === "markdown"
                  ? "Markdown (.md)"
                  : fmt === "html"
                    ? "Printable HTML"
                    : "Raw JSON"}
              </button>
            ))}
          </div>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => void handleCopy()}
              className="inline-flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/[0.04] px-3 py-1.5 text-xs font-medium text-slate-300 transition hover:border-white/20 hover:text-slate-100"
            >
              {copied ? (
                <>
                  <CheckIcon className="h-3.5 w-3.5 text-emerald-400" />
                  Copied
                </>
              ) : (
                <>
                  <CopyIcon className="h-3.5 w-3.5" />
                  Copy Text
                </>
              )}
            </button>

            <button
              type="button"
              onClick={handleDownload}
              className="inline-flex items-center gap-1.5 rounded-lg border border-sky-400/30 bg-sky-400/[0.1] px-3.5 py-1.5 text-xs font-semibold text-sky-200 transition hover:border-sky-400/50 hover:bg-sky-400/[0.18]"
            >
              <DownloadIcon className="h-3.5 w-3.5" />
              Download
            </button>
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto p-5">
          <div className="rounded-xl border border-white/[0.07] bg-black/40 p-3.5 font-mono text-[11px] leading-relaxed text-slate-300">
            <pre className="max-h-[42vh] overflow-x-auto whitespace-pre-wrap font-mono">
              {previewContent}
            </pre>
          </div>
        </div>
      </div>
    </div>
  );
}
