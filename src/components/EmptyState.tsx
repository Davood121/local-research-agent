import { useState } from "react";
import { useChatStore } from "../stores/chatStore";
import { useModelStore } from "../stores/modelStore";
import { useResearchStore } from "../stores/researchStore";
import { BrandMark } from "./BrandMark";
import { CheckIcon, CopyIcon, DownloadIcon, RefreshIcon, ServerIcon, SparkIcon } from "./Icons";

const SUGGESTIONS = [
  "Explain a difficult idea in simple terms",
  "Help me draft a step-by-step plan",
  "Compare two approaches and their trade-offs",
  "Turn rough notes into a clear summary",
];

const RESEARCH_SUGGESTIONS = [
  "What is local-first software?",
  "Find primary sources on retrieval-augmented generation",
  "Compare RAG with fine-tuning for private data",
  "Summarize current guidance on local AI privacy",
];

function CommandLine({ children }: { children: string }) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    if (!navigator.clipboard) return;
    try {
      await navigator.clipboard.writeText(children);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1_600);
    } catch {
      // Commands remain visible and selectable if clipboard access is denied.
    }
  };

  return (
    <div className="flex items-center justify-between gap-3 rounded-lg border border-white/[0.07] bg-black/30 px-3 py-2">
      <code className="overflow-x-auto whitespace-nowrap font-mono text-[11.5px] text-sky-200">
        {children}
      </code>
      <button
        type="button"
        onClick={() => void copy()}
        title="Copy command"
        className="shrink-0 rounded p-1 text-slate-500 transition hover:bg-white/[0.05] hover:text-slate-200"
      >
        {copied ? (
          <CheckIcon className="h-3.5 w-3.5 text-emerald-400" />
        ) : (
          <CopyIcon className="h-3.5 w-3.5" />
        )}
      </button>
    </div>
  );
}

function CheckingState() {
  return (
    <div className="flex flex-col items-center text-center">
      <div className="mb-5 flex h-14 w-14 items-center justify-center rounded-2xl border border-sky-400/15 bg-sky-400/[0.05]">
        <span className="h-6 w-6 animate-spin rounded-full border-2 border-sky-400/20 border-t-sky-300" />
      </div>
      <h2 className="text-lg font-semibold text-slate-100">Looking for local models</h2>
      <p className="mt-2 max-w-sm text-sm leading-relaxed text-slate-500">
        Checking the built-in model folder and any local Ollama server. Nothing leaves this machine.
      </p>
    </div>
  );
}

function OfflineState() {
  const statusError = useChatStore((state) => state.statusError);
  const checking = useChatStore((state) => state.checking);
  const refresh = useChatStore((state) => state.refresh);
  const openDownloads = useModelStore((state) => state.openPanel);

  return (
    <div className="w-full max-w-xl">
      <div className="mb-5 flex h-14 w-14 items-center justify-center rounded-2xl border border-amber-400/15 bg-amber-400/[0.05] text-amber-300/80">
        <ServerIcon className="h-6 w-6" />
      </div>
      <h2 className="text-xl font-semibold tracking-[-0.01em] text-slate-100">
        Connect your local model
      </h2>
      <p className="mt-2 max-w-lg text-sm leading-relaxed text-slate-400">
        No local inference is available yet: install a GGUF model or start Ollama once, and this
        screen will update automatically.
      </p>

      <button
        type="button"
        onClick={openDownloads}
        className="mt-5 inline-flex items-center gap-2 rounded-lg border border-sky-400/25 bg-sky-400/[0.09] px-3.5 py-2 text-xs font-semibold text-sky-200 transition hover:border-sky-400/40 hover:bg-sky-400/[0.15]"
      >
        <DownloadIcon className="h-3.5 w-3.5" />
        Download a model
      </button>

      <div className="mt-6 grid gap-2.5">
        <div className="flex gap-3 rounded-xl border border-white/[0.06] bg-white/[0.025] p-3.5">
          <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-white/10 bg-white/[0.04] text-[10px] font-semibold text-slate-400">
            1
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-xs font-medium text-slate-300">
              Drop a GGUF file into the models folder (no server needed)
            </p>
            <div className="mt-2">
              <CommandLine>
                C:\Users\…\AppData\Roaming\dev.localresearch.agent\models\model-q4_k_m.gguf
              </CommandLine>
            </div>
          </div>
        </div>

        <div className="flex gap-3 rounded-xl border border-white/[0.06] bg-white/[0.025] p-3.5">
          <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-white/10 bg-white/[0.04] text-[10px] font-semibold text-slate-400">
            2
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-xs font-medium text-slate-300">Or start the Ollama server</p>
            <div className="mt-2">
              <CommandLine>ollama serve</CommandLine>
            </div>
          </div>
        </div>

        <div className="flex gap-3 rounded-xl border border-white/[0.06] bg-white/[0.025] p-3.5">
          <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-white/10 bg-white/[0.04] text-[10px] font-semibold text-slate-400">
            3
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-xs font-medium text-slate-300">Or pull a chat model with Ollama</p>
            <div className="mt-2">
              <CommandLine>ollama pull qwen3:4b</CommandLine>
            </div>
          </div>
        </div>
      </div>

      {statusError ? (
        <p className="mt-4 rounded-lg border border-rose-400/10 bg-rose-400/[0.04] px-3 py-2 font-mono text-[10.5px] leading-relaxed text-rose-300/80">
          {statusError}
        </p>
      ) : null}

      <div className="mt-5 flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={() => void refresh()}
          disabled={checking}
          className="inline-flex items-center gap-2 rounded-lg border border-sky-400/25 bg-sky-400/[0.08] px-3.5 py-2 text-xs font-semibold text-sky-200 transition hover:border-sky-400/40 hover:bg-sky-400/[0.12] disabled:cursor-wait disabled:opacity-70"
        >
          <RefreshIcon className={`h-3.5 w-3.5 ${checking ? "animate-spin" : ""}`} />
          Check again
        </button>
        <span className="text-[11px] text-slate-600">Only 127.0.0.1 is contacted.</span>
      </div>
    </div>
  );
}

function NoModelsState() {
  const checking = useChatStore((state) => state.checking);
  const refresh = useChatStore((state) => state.refresh);

  return (
    <div className="w-full max-w-xl">
      <div className="mb-5 flex h-14 w-14 items-center justify-center rounded-2xl border border-sky-400/15 bg-sky-400/[0.05] text-sky-300/80">
        <ServerIcon className="h-6 w-6" />
      </div>
      <h2 className="text-xl font-semibold tracking-[-0.01em] text-slate-100">Ollama is running</h2>
      <p className="mt-2 max-w-lg text-sm leading-relaxed text-slate-400">
        No local chat models are installed yet. Pull one below, then refresh the model list.
        Cloud-routed models are intentionally hidden.
      </p>
      <div className="mt-5">
        <CommandLine>ollama pull qwen3:4b</CommandLine>
      </div>
      <button
        type="button"
        onClick={() => void refresh()}
        disabled={checking}
        className="mt-5 inline-flex items-center gap-2 rounded-lg border border-sky-400/25 bg-sky-400/[0.08] px-3.5 py-2 text-xs font-semibold text-sky-200 transition hover:border-sky-400/40 hover:bg-sky-400/[0.12] disabled:cursor-wait disabled:opacity-70"
      >
        <RefreshIcon className={`h-3.5 w-3.5 ${checking ? "animate-spin" : ""}`} />
        Refresh models
      </button>
    </div>
  );
}

function ReadyState() {
  const model = useChatStore((state) => state.selectedModel);
  const models = useChatStore((state) => state.models);
  const send = useChatStore((state) => state.send);
  const mode = useResearchStore((state) => state.mode);
  const searchConnection = useResearchStore((state) => state.connection);
  const info = models.find((entry) => entry.name === model);
  const isResearch = mode === "research";
  const researchReady = searchConnection === "connected";
  const suggestions = isResearch ? RESEARCH_SUGGESTIONS : SUGGESTIONS;

  return (
    <div className="w-full max-w-2xl">
      <div className="mb-5 flex h-14 w-14 items-center justify-center rounded-2xl border border-sky-400/20 bg-gradient-to-br from-sky-400/15 to-emerald-400/[0.04] text-sky-300 shadow-[inset_0_1px_0_rgba(255,255,255,0.08)]">
        <BrandMark />
      </div>
      <p className="mb-1 text-[10px] font-semibold uppercase tracking-[0.2em] text-sky-300/70">
        Private by architecture
      </p>
      <h2 className="text-2xl font-semibold tracking-[-0.025em] text-slate-100">
        {isResearch ? "Ask the web. Keep the reasoning local." : "Ask anything. Keep it local."}
      </h2>
      <p className="mt-2 max-w-lg text-sm leading-relaxed text-slate-400">
        {isResearch
          ? `Your question is searched through SearXNG and answered by ${
              model ?? "a local model"
            } with numbered citations. Prompts stay local; page reads happen only when you start a run.`
          : `Your conversation is generated by ${
              model ?? "a local model"
            } on this device. No account, API key, or cloud inference is involved.`}
      </p>

      <div className="mt-7 grid gap-2.5 sm:grid-cols-2">
        {suggestions.map((suggestion) => {
          const disabled = isResearch && !researchReady;
          return (
            <button
              key={suggestion}
              type="button"
              disabled={disabled}
              onClick={() => send(suggestion, mode)}
              className="group flex items-start gap-3 rounded-xl border border-white/[0.06] bg-white/[0.025] p-3.5 text-left transition hover:border-sky-400/20 hover:bg-sky-400/[0.045] disabled:cursor-not-allowed disabled:opacity-45 disabled:hover:border-white/[0.06] disabled:hover:bg-white/[0.025]"
            >
              <span className="mt-0.5 text-sky-400/60 transition group-hover:text-sky-300">
                <SparkIcon className="h-4 w-4" />
              </span>
              <span className="text-xs leading-relaxed text-slate-400 transition group-hover:text-slate-200">
                {suggestion}
              </span>
            </button>
          );
        })}
      </div>

      {isResearch && !researchReady ? (
        <p className="mt-3 text-[10.5px] text-amber-200/70">
          Research needs a local SearXNG instance at 127.0.0.1:8080.
        </p>
      ) : null}

      {info ? (
        <div className="mt-5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[10.5px] text-slate-600">
          <span className="font-mono text-slate-500">{info.name}</span>
          {info.parameter_size ? <span>{info.parameter_size}</span> : null}
          {info.quantization ? <span>{info.quantization}</span> : null}
          {info.context_length ? (
            <span>{Math.round(info.context_length / 1024)}K context</span>
          ) : null}
          <span>{info.family === "embedded gguf" ? "built-in engine" : "127.0.0.1:11434"}</span>
          {isResearch ? <span>SearXNG 127.0.0.1:8080</span> : null}
        </div>
      ) : null}
    </div>
  );
}

export function EmptyState() {
  const connection = useChatStore((state) => state.connection);
  const models = useChatStore((state) => state.models);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="flex min-h-full items-center justify-center px-6 py-10 sm:px-10">
        {connection === "checking" ? (
          <CheckingState />
        ) : connection === "offline" ? (
          <OfflineState />
        ) : models.length === 0 ? (
          <NoModelsState />
        ) : (
          <ReadyState />
        )}
      </div>
    </div>
  );
}
