import { useEffect } from "react";
import { BrandMark } from "./BrandMark";
import { ChevronDownIcon, DownloadIcon, RefreshIcon, ShieldIcon } from "./Icons";
import { DebugStreamButton } from "./DebugStreamButton";
import { useChatStore } from "../stores/chatStore";
import { useEngineStore } from "../stores/engineStore";
import { useModelStore } from "../stores/modelStore";
import { useResearchStore } from "../stores/researchStore";

function ModelPicker() {
  const models = useChatStore((state) => state.models);
  const selectedModel = useChatStore((state) => state.selectedModel);
  const connection = useChatStore((state) => state.connection);
  const checking = useChatStore((state) => state.checking);
  const isStreaming = useChatStore((state) => state.isStreaming);
  const selectModel = useChatStore((state) => state.selectModel);
  const refresh = useChatStore((state) => state.refresh);
  const openDownloads = useModelStore((state) => state.openPanel);

  const value = models.some((model) => model.name === selectedModel) ? (selectedModel ?? "") : "";
  const disabled = connection !== "connected" || models.length === 0 || isStreaming;

  return (
    <div className="flex items-center gap-2">
      <div className="relative">
        <select
          aria-label="Local model"
          value={value}
          onChange={(event) => selectModel(event.target.value)}
          disabled={disabled}
          className="h-9 w-[190px] appearance-none truncate rounded-lg border border-white/10 bg-white/[0.04] pl-3 pr-8 text-xs font-medium text-slate-200 shadow-sm outline-none transition hover:border-white/20 hover:bg-white/[0.07] focus:border-sky-400/50 focus:ring-2 focus:ring-sky-400/10 disabled:cursor-not-allowed disabled:text-slate-500"
        >
          {models.length === 0 ? (
            <option value="">
              {connection === "checking" ? "Looking for Ollama…" : "No local model"}
            </option>
          ) : (
            models.map((model) => (
              <option key={model.name} value={model.name} className="bg-slate-900 text-slate-100">
                {model.parameter_size ? `${model.name} · ${model.parameter_size}` : model.name}
              </option>
            ))
          )}
        </select>
        <ChevronDownIcon className="pointer-events-none absolute right-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-slate-500" />
      </div>

      <button
        type="button"
        onClick={() => void refresh()}
        disabled={checking}
        title="Refresh Ollama status and installed models"
        className="flex h-9 w-9 items-center justify-center rounded-lg border border-white/10 bg-white/[0.04] text-slate-400 transition hover:border-white/20 hover:bg-white/[0.07] hover:text-slate-200 disabled:cursor-wait disabled:opacity-70"
      >
        <RefreshIcon className={`h-3.5 w-3.5 ${checking ? "animate-spin" : ""}`} />
      </button>

      <button
        type="button"
        onClick={openDownloads}
        title="Download a local model"
        aria-label="Download a local model"
        className="flex h-9 w-9 items-center justify-center rounded-lg border border-white/10 bg-white/[0.04] text-slate-400 transition hover:border-sky-400/30 hover:bg-sky-400/[0.07] hover:text-sky-200"
      >
        <DownloadIcon className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

function ConnectionPill() {
  const connection = useChatStore((state) => state.connection);
  const version = useChatStore((state) => state.version);
  const count = useChatStore((state) => state.models.length);

  if (connection === "checking") {
    return (
      <span className="hidden items-center gap-2 rounded-full border border-sky-400/20 bg-sky-400/[0.06] px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-sky-300 lg:flex">
        <span className="h-2.5 w-2.5 animate-spin rounded-full border border-sky-300/30 border-t-sky-300" />
        Checking local models
      </span>
    );
  }

  if (connection === "offline") {
    return (
      <span className="hidden items-center gap-2 rounded-full border border-rose-400/20 bg-rose-400/[0.06] px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-rose-300 lg:flex">
        <span className="h-1.5 w-1.5 rounded-full bg-rose-400" />
        No local models
      </span>
    );
  }

  return (
    <span
      title={version ? `Ollama ${version}` : "Running the built-in engine"}
      className="hidden items-center gap-2 rounded-full border border-emerald-400/20 bg-emerald-400/[0.06] px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-emerald-300 lg:flex"
    >
      <span className="h-1.5 w-1.5 rounded-full bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.8)]" />
      {count === 1 ? "1 local model" : `${count} local models`}
    </span>
  );
}

export function TopBar() {
  const refresh = useChatStore((state) => state.refresh);
  const refreshResearch = useResearchStore((state) => state.refresh);
  const refreshEngine = useEngineStore((state) => state.refresh);

  useEffect(() => {
    void refresh();
    void refreshResearch();
    void refreshEngine();
    const timer = window.setInterval(() => {
      void refresh();
      void refreshResearch();
      void refreshEngine();
    }, 30_000);
    return () => window.clearInterval(timer);
  }, [refresh, refreshResearch, refreshEngine]);

  return (
    <header className="relative z-20 flex h-16 shrink-0 items-center justify-between gap-4 border-b border-white/[0.06] bg-[#060b15]/90 px-5 backdrop-blur-xl">
      <div className="flex min-w-0 items-center gap-3">
        <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-sky-400/20 bg-gradient-to-br from-sky-400/15 to-cyan-400/[0.03] shadow-[inset_0_1px_0_rgba(255,255,255,0.08)]">
          <BrandMark />
        </div>
        <div className="min-w-0">
          <h1 className="truncate text-sm font-semibold tracking-[0.01em] text-slate-100">
            Local Research Agent
          </h1>
          <p className="truncate text-[11px] text-slate-500">
            Private workspace · local inference with cited research
          </p>
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-2.5">
        <span className="hidden items-center gap-1.5 rounded-full border border-white/[0.07] bg-white/[0.03] px-2.5 py-1 text-[10px] font-medium uppercase tracking-[0.14em] text-slate-400 md:flex">
          <ShieldIcon className="h-3 w-3 text-emerald-400/80" />
          Local inference
        </span>
        <ConnectionPill />
        <DebugStreamButton />
        <div className="mx-0.5 hidden h-6 w-px bg-white/[0.07] sm:block" />
        <ModelPicker />
      </div>
    </header>
  );
}
