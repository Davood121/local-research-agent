import { useEffect } from "react";
import { CheckIcon, DownloadIcon, XIcon } from "./Icons";
import { formatModelSize } from "../lib/modelDownloads";
import { useModelStore } from "../stores/modelStore";

export function ModelDownloads() {
  const open = useModelStore((state) => state.open);
  const catalog = useModelStore((state) => state.catalog);
  const loading = useModelStore((state) => state.loading);
  const error = useModelStore((state) => state.error);
  const downloads = useModelStore((state) => state.downloads);
  const downloading = useModelStore((state) => state.downloading);
  const closePanel = useModelStore((state) => state.closePanel);
  const start = useModelStore((state) => state.start);
  const cancel = useModelStore((state) => state.cancel);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") closePanel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, closePanel]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-[#02060f]/75 p-4 backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      aria-label="Download a local model"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) closePanel();
      }}
    >
      <div className="w-full max-w-xl overflow-hidden rounded-2xl border border-white/10 bg-[#0a1120] shadow-[0_30px_80px_rgba(0,0,0,0.6)]">
        <header className="flex items-start justify-between gap-4 border-b border-white/[0.06] px-5 py-4">
          <div>
            <h2 className="text-sm font-semibold text-slate-100">Download a local model</h2>
            <p className="mt-1 text-xs leading-relaxed text-slate-400">
              GGUF files from Hugging Face, saved to the models folder and run by the built-in
              engine. No account, API key, or cloud inference.
            </p>
          </div>
          <button
            type="button"
            onClick={closePanel}
            aria-label="Close"
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-white/10 bg-white/[0.04] text-slate-400 transition hover:border-white/20 hover:bg-white/[0.07] hover:text-slate-200"
          >
            <XIcon className="h-3.5 w-3.5" />
          </button>
        </header>

        <div className="max-h-[58vh] overflow-y-auto">
          {catalog.length === 0 ? (
            <p className="px-5 py-6 text-xs text-slate-400">
              {loading ? "Reading the catalog…" : "No models are available in the catalog."}
            </p>
          ) : (
            <ul className="divide-y divide-white/[0.06]">
              {catalog.map((model) => {
                const progress = downloads[model.id];
                const active =
                  downloading === model.id ||
                  progress?.status === "Started" ||
                  progress?.status === "Progress";
                const percent =
                  progress && progress.total > 0
                    ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
                    : 0;
                const installed = model.installed || progress?.status === "Done";

                return (
                  <li key={model.id} className="px-5 py-4">
                    <div className="flex items-start justify-between gap-4">
                      <div className="min-w-0">
                        <p className="truncate text-xs font-semibold text-slate-100">
                          {model.name}
                        </p>
                        <p className="mt-0.5 text-[11px] leading-relaxed text-slate-400">
                          {model.description}
                        </p>
                        <p className="mt-1 font-mono text-[10px] uppercase tracking-[0.12em] text-slate-500">
                          {formatModelSize(model.size_bytes)} · {model.filename}
                        </p>
                      </div>

                      {installed ? (
                        <span className="flex shrink-0 items-center gap-1.5 rounded-full border border-emerald-400/20 bg-emerald-400/[0.07] px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.12em] text-emerald-300">
                          <CheckIcon className="h-3 w-3" />
                          Installed
                        </span>
                      ) : active ? (
                        <button
                          type="button"
                          onClick={() => void cancel()}
                          className="shrink-0 rounded-lg border border-white/10 bg-white/[0.04] px-3 py-1.5 text-[11px] font-medium text-slate-300 transition hover:border-rose-400/30 hover:bg-rose-400/[0.07] hover:text-rose-200"
                        >
                          Cancel
                        </button>
                      ) : (
                        <button
                          type="button"
                          onClick={() => void start(model.id)}
                          disabled={downloading !== null}
                          className="flex shrink-0 items-center gap-1.5 rounded-lg border border-sky-400/25 bg-sky-400/[0.09] px-3 py-1.5 text-[11px] font-semibold text-sky-200 transition hover:border-sky-400/40 hover:bg-sky-400/[0.15] disabled:cursor-not-allowed disabled:opacity-40"
                        >
                          <DownloadIcon className="h-3.5 w-3.5" />
                          Download
                        </button>
                      )}
                    </div>

                    {active ? (
                      <div className="mt-3">
                        <div className="h-1.5 overflow-hidden rounded-full bg-white/[0.07]">
                          <div
                            className="h-full rounded-full bg-gradient-to-r from-sky-400 to-cyan-300 transition-[width] duration-300"
                            style={{ width: `${percent}%` }}
                          />
                        </div>
                        <p className="mt-1.5 font-mono text-[10px] text-slate-400">
                          {formatModelSize(progress?.downloaded ?? 0)} of{" "}
                          {formatModelSize(progress?.total ?? model.size_bytes)} · {percent}%
                        </p>
                      </div>
                    ) : null}

                    {progress?.status === "Failed" && progress.message ? (
                      <p className="mt-2 text-[11px] text-rose-300">{progress.message}</p>
                    ) : null}
                    {progress?.status === "Cancelled" ? (
                      <p className="mt-2 text-[11px] text-slate-400">
                        Download cancelled; the partial file was removed.
                      </p>
                    ) : null}
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        <footer className="border-t border-white/[0.06] px-5 py-3">
          {error ? (
            <p className="text-[11px] text-rose-300">{error}</p>
          ) : (
            <p className="text-[11px] text-slate-500">
              Downloads use HTTPS to huggingface.co and are verified against the response size
              before the file is added to the model picker.
            </p>
          )}
        </footer>
      </div>
    </div>
  );
}
