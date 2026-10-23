import { useActivityStore } from "../stores/activityStore";
import { useChatStore } from "../stores/chatStore";
import { useEngineStore } from "../stores/engineStore";
import { useMetricsStore } from "../stores/metricsStore";
import { ClockIcon, PulseIcon, ServerIcon, ShieldIcon } from "./Icons";

function formatDuration(ms: number | null): string {
  if (ms === null) return "—";
  if (ms < 1_000) return `${ms} ms`;
  return `${(ms / 1_000).toFixed(1)} s`;
}

function formatNumber(value: number): string {
  return value.toLocaleString("en-US");
}

function formatGigabytes(bytes: number): string {
  return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-xl border border-white/[0.055] bg-white/[0.025] px-3 py-2.5">
      <p className="text-[9px] font-medium uppercase tracking-[0.16em] text-slate-600">{label}</p>
      <p className="tabular mt-1 text-[15px] font-semibold text-slate-200">{value}</p>
    </div>
  );
}

export function ActivityPanel() {
  const tokenCount = useActivityStore((state) => state.tokenCount);
  const preview = useActivityStore((state) => state.preview);
  const steps = useActivityStore((state) => state.steps);
  const tools = useActivityStore((state) => state.tools);
  const errors = useActivityStore((state) => state.errors);
  const metrics = useMetricsStore((state) => state.latest);
  const isStreaming = useChatStore((state) => state.isStreaming);
  const selectedModel = useChatStore((state) => state.selectedModel);
  const models = useChatStore((state) => state.models);
  const connection = useChatStore((state) => state.connection);
  const embeddedModel =
    selectedModel !== null &&
    models.some((model) => model.name === selectedModel && model.family === "embedded gguf");
  const lastDurationMs = useChatStore((state) => state.lastDurationMs);
  const messages = useChatStore((state) => state.messages);
  const device = useEngineStore((state) => state.device);
  const gpu = useEngineStore((state) => state.gpu);
  const changingDevice = useEngineStore((state) => state.changing);
  const deviceError = useEngineStore((state) => state.error);
  const setDevice = useEngineStore((state) => state.setDevice);
  const clearDeviceError = useEngineStore((state) => state.clearError);
  const lastRunHadOutput = tokenCount > 0;
  const visibleErrors = errors.filter((error) => error.message !== "Generation stopped.");
  const lastEntry = messages[messages.length - 1];
  const lastAssistantState = lastEntry?.role === "assistant" ? lastEntry.state : null;
  // The store is newest-first, so the first occurrence of a node is its latest
  // state. Keep one row per node instead of showing both Started and Done.
  const visibleSteps = steps.filter(
    (step, index, all) =>
      all.findIndex(
        (candidate) => candidate.run_id === step.run_id && candidate.node === step.node,
      ) === index,
  );
  const visibleTools = tools.filter(
    (tool, index, all) =>
      all.findIndex(
        (candidate) => candidate.run_id === tool.run_id && candidate.name === tool.name,
      ) === index,
  );
  const generated = metrics ? metrics.gen_tokens : tokenCount;

  const stepLabel = (node: string): string => {
    if (node === "chat") return "Local inference";
    if (node === "research") return "Local research";
    if (node === "plan") return "Query planning";
    if (node === "search") return "Engine search";
    if (node === "read") return "Page reading";
    if (node === "verify") return "Evidence check";
    return node;
  };

  const toolLabel = (name: string): string => {
    if (name === "plan_research") return "Research plan";
    if (name === "web_search") return "Web search";
    if (name === "read_pages") return "Read pages";
    if (name === "verify_evidence") return "Evidence check";
    return name;
  };

  const stateLabel = isStreaming
    ? "Generating"
    : lastAssistantState === "stopped"
      ? "Stopped"
      : lastAssistantState === "error"
        ? "Interrupted"
        : lastRunHadOutput
          ? "Complete"
          : connection === "connected"
            ? "Ready"
            : "Idle";

  const stateDot = isStreaming
    ? "animate-pulse bg-sky-400 shadow-[0_0_8px_rgba(56,189,248,0.9)]"
    : lastAssistantState === "stopped"
      ? "bg-amber-400"
      : lastAssistantState === "error"
        ? "bg-rose-400"
        : lastRunHadOutput
          ? "bg-emerald-400"
          : connection === "connected"
            ? "bg-emerald-400/60"
            : "bg-slate-600";

  return (
    <aside
      aria-label="Local run insights"
      className="hidden w-[326px] shrink-0 flex-col border-l border-white/[0.05] bg-[#070c16]/80 lg:flex xl:w-[344px]"
    >
      <header className="flex h-[52px] shrink-0 items-center justify-between border-b border-white/[0.045] px-4">
        <div className="flex items-center gap-2">
          <PulseIcon className="h-3.5 w-3.5 text-sky-400/70" />
          <h2 className="text-[11px] font-semibold uppercase tracking-[0.18em] text-slate-400">
            Run insights
          </h2>
        </div>
        <span className="flex items-center gap-1.5 text-[9.5px] font-semibold uppercase tracking-[0.13em] text-slate-500">
          <span className={`h-1.5 w-1.5 rounded-full ${stateDot}`} />
          {stateLabel}
        </span>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-5 pt-4">
        <div className="rounded-xl border border-white/[0.06] bg-gradient-to-br from-white/[0.045] to-transparent p-3">
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <p className="text-[9px] font-medium uppercase tracking-[0.16em] text-slate-600">
                Active model
              </p>
              <p className="mt-1 truncate font-mono text-[12px] text-slate-200">
                {selectedModel ?? "No local model"}
              </p>
            </div>
            <span
              className={`mt-1 h-1.5 w-1.5 shrink-0 rounded-full ${
                connection === "connected" ? "bg-emerald-400" : "bg-rose-400"
              }`}
            />
          </div>
          <div className="mt-3 flex items-center gap-1.5 text-[9.5px] text-slate-600">
            <ServerIcon className="h-3 w-3" />
            <span className={embeddedModel ? undefined : "font-mono"}>
              {embeddedModel ? "Built-in engine · no server" : "127.0.0.1:11434"}
            </span>
          </div>
          {embeddedModel ? (
            <div className="mt-3 border-t border-white/[0.05] pt-3">
              <div className="flex items-center justify-between gap-2">
                <span className="text-[9px] font-medium uppercase tracking-[0.16em] text-slate-600">
                  Compute
                </span>
                <div
                  role="group"
                  aria-label="Inference device"
                  className="flex rounded-md border border-white/[0.08] bg-black/20 p-0.5"
                >
                  <button
                    type="button"
                    onClick={() => {
                      clearDeviceError();
                      void setDevice("cpu");
                    }}
                    disabled={isStreaming || changingDevice}
                    aria-pressed={device === "cpu"}
                    className={`rounded px-2 py-0.5 text-[9.5px] font-semibold uppercase tracking-[0.1em] transition disabled:cursor-not-allowed disabled:opacity-50 ${
                      device === "cpu"
                        ? "bg-sky-400/15 text-sky-200"
                        : "text-slate-500 hover:text-slate-300"
                    }`}
                  >
                    CPU
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      clearDeviceError();
                      void setDevice("gpu");
                    }}
                    disabled={isStreaming || changingDevice || gpu === null}
                    aria-pressed={device === "gpu"}
                    title={
                      gpu
                        ? `${gpu.name} · ${gpu.backend} · ${formatGigabytes(gpu.memory_free_bytes)} free`
                        : "No discrete Vulkan GPU detected"
                    }
                    className={`rounded px-2 py-0.5 text-[9.5px] font-semibold uppercase tracking-[0.1em] transition disabled:cursor-not-allowed disabled:opacity-50 ${
                      device === "gpu"
                        ? "bg-emerald-400/15 text-emerald-200"
                        : "text-slate-500 hover:text-slate-300"
                    }`}
                  >
                    GPU
                  </button>
                </div>
              </div>
              <p className="mt-1.5 truncate text-[9.5px] text-slate-600">
                {device === "gpu"
                  ? gpu
                    ? `${gpu.name} · ${gpu.backend}`
                    : "GPU unavailable"
                  : gpu
                    ? "CPU · GPU available"
                    : "CPU only"}
              </p>
              {deviceError && (
                <p className="mt-1 text-[10px] leading-snug text-rose-300">{deviceError}</p>
              )}
            </div>
          ) : null}
        </div>

        <div className="mt-3 grid grid-cols-2 gap-2">
          <Metric label="Generated" value={formatNumber(generated)} />
          <Metric label="Speed" value={metrics ? `${metrics.tok_per_sec.toFixed(1)} t/s` : "—"} />
          <Metric
            label="First token"
            value={metrics ? `${formatNumber(metrics.ttft_ms)} ms` : "—"}
          />
          <Metric
            label="Duration"
            value={formatDuration(lastDurationMs ?? visibleSteps[0]?.duration_ms ?? null)}
          />
        </div>

        <div className="mt-5">
          <div className="mb-2 flex items-center justify-between">
            <p className="text-[9px] font-medium uppercase tracking-[0.16em] text-slate-600">
              This run
            </p>
            <span className="flex items-center gap-1 text-[9.5px] text-slate-600">
              <ClockIcon className="h-3 w-3" />
              {metrics
                ? `${formatNumber(metrics.prompt_tokens)} prompt · ${formatNumber(
                    metrics.gen_tokens,
                  )} generated`
                : "No metrics yet"}
            </span>
          </div>

          {visibleSteps.length === 0 ? (
            <div className="rounded-xl border border-dashed border-white/[0.06] px-3 py-4 text-center">
              <p className="text-[11px] text-slate-600">Send a prompt to start a run.</p>
            </div>
          ) : (
            <ol className="space-y-1.5">
              {visibleSteps.map((step, index) => (
                <li
                  key={`${step.run_id}-${step.node}-${index}`}
                  className="flex items-center justify-between gap-2 rounded-lg border border-white/[0.05] bg-white/[0.02] px-2.5 py-2"
                >
                  <span className="flex min-w-0 items-center gap-2">
                    <span
                      className={`h-1.5 w-1.5 shrink-0 rounded-full ${
                        step.status === "Started"
                          ? "animate-pulse bg-sky-400"
                          : step.status === "Done"
                            ? "bg-emerald-400"
                            : "bg-rose-400"
                      }`}
                    />
                    <span className="truncate text-[11px] text-slate-400">
                      {stepLabel(step.node)}
                    </span>
                  </span>
                  <span className="shrink-0 font-mono text-[9.5px] text-slate-600">
                    {step.duration_ms === null ? step.status : formatDuration(step.duration_ms)}
                  </span>
                </li>
              ))}
            </ol>
          )}
        </div>

        {visibleTools.length > 0 ? (
          <div className="mt-5">
            <p className="mb-2 text-[9px] font-medium uppercase tracking-[0.16em] text-slate-600">
              Tools
            </p>
            <ul className="space-y-1.5">
              {visibleTools.map((tool, index) => (
                <li
                  key={`${tool.run_id}-${tool.name}-${index}`}
                  className="rounded-lg border border-white/[0.05] px-2.5 py-2 text-[11px] text-slate-400"
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate font-mono text-[10.5px]">{toolLabel(tool.name)}</span>
                    <span className="text-[9.5px] uppercase text-slate-600">{tool.status}</span>
                  </div>
                  {tool.summary ? (
                    <p className="mt-1 line-clamp-2 text-[10px] leading-relaxed text-slate-600">
                      {tool.summary}
                    </p>
                  ) : null}
                </li>
              ))}
            </ul>
          </div>
        ) : null}

        {preview ? (
          <details className="group mt-5 rounded-xl border border-white/[0.055] bg-black/20">
            <summary className="cursor-pointer list-none px-3 py-2 text-[9px] font-medium uppercase tracking-[0.16em] text-slate-600 transition hover:text-slate-400">
              Raw stream
              <span className="float-right normal-case tracking-normal text-slate-700 group-open:hidden">
                show
              </span>
            </summary>
            <pre className="max-h-32 overflow-y-auto whitespace-pre-wrap break-words border-t border-white/[0.04] p-2.5 font-mono text-[10px] leading-relaxed text-slate-500">
              {preview}
            </pre>
          </details>
        ) : null}

        {visibleErrors.length > 0 ? (
          <div className="mt-4 rounded-xl border border-rose-400/15 bg-rose-400/[0.045] p-2.5">
            <p className="mb-1.5 text-[9px] font-medium uppercase tracking-[0.16em] text-rose-300/80">
              Errors
            </p>
            <ul className="space-y-1">
              {visibleErrors.map((error, index) => (
                <li
                  key={`${error.scope}-${index}`}
                  className="text-[10.5px] leading-relaxed text-rose-200/80"
                >
                  {error.scope}: {error.message}
                </li>
              ))}
            </ul>
          </div>
        ) : null}

        <div className="mt-5 flex gap-2.5 rounded-xl border border-emerald-400/10 bg-emerald-400/[0.025] p-3">
          <ShieldIcon className="mt-0.5 h-3.5 w-3.5 shrink-0 text-emerald-400/60" />
          <p className="text-[10px] leading-relaxed text-slate-500">
            No cloud inference, accounts, API keys, or usage telemetry. Research reads result pages
            only when you start a run; prompts and retrieval stay on this device.
          </p>
        </div>
      </div>
    </aside>
  );
}
