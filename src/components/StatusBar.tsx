import type { GpuStat } from "../lib/events";
import { formatGb } from "../lib/format";
import { useThrottledValue } from "../lib/useThrottledValue";
import { useChatStore } from "../stores/chatStore";
import { useResearchStore } from "../stores/researchStore";
import { useSystemStore } from "../stores/systemStore";

function gpuLabel(gpu: GpuStat): string {
  if (gpu === "Unavailable") return "GPU unavailable";
  if ("Nvidia" in gpu) {
    const { vram_used_gb, vram_total_gb, util_pct } = gpu.Nvidia;
    return `GPU ${formatGb(vram_used_gb)}/${formatGb(vram_total_gb)} GB · util ${util_pct}%`;
  }
  if ("Amd" in gpu) {
    const { vram_used_gb, vram_total_gb } = gpu.Amd;
    return `GPU ${formatGb(vram_used_gb)}/${formatGb(vram_total_gb)} GB`;
  }
  const { mem_used_gb, mem_total_gb } = gpu.AppleUnified;
  return `Unified memory ${formatGb(mem_used_gb)}/${formatGb(mem_total_gb)} GB`;
}

function Separator() {
  return (
    <span aria-hidden="true" className="text-slate-700/80">
      ·
    </span>
  );
}

export function StatusBar() {
  const telemetry = useSystemStore((state) => state.telemetry);
  const bridgeError = useSystemStore((state) => state.bridgeError);
  const connection = useChatStore((state) => state.connection);
  const version = useChatStore((state) => state.version);
  const searchConnection = useResearchStore((state) => state.connection);
  const searchProvider = useResearchStore((state) => state.provider);
  const searchChecking = useResearchStore((state) => state.checking);
  const searchError = useResearchStore((state) => state.error);

  // Telemetry is emitted at most every second; the throttle still guarantees a
  // 10 Hz ceiling if a future poller runs faster.
  const view = useThrottledValue(telemetry, 100);

  return (
    <footer
      aria-label="System status"
      className="tabular flex flex-wrap items-center justify-between gap-2 border-t border-white/[0.05] bg-[#070c16] px-4 py-1.5 text-[10.5px] text-slate-500"
    >
      <div className="flex flex-wrap items-center gap-2">
        {view ? (
          <>
            <span title="Physical memory in use">
              RAM {formatGb(view.ram_used_gb)} / {formatGb(view.ram_total_gb)} GB
            </span>
            <Separator />
            <span title="Free space on the app data volume">
              Disk {formatGb(view.disk_free_gb)} GB free
            </span>
            <Separator />
            <span
              className={view.gpu === "Unavailable" ? "text-slate-600" : undefined}
              title="Local GPU telemetry"
            >
              {gpuLabel(view.gpu)}
            </span>
          </>
        ) : (
          <span className="text-slate-600">Waiting for telemetry…</span>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <span className="flex items-center gap-1.5" title="Local inference server">
          <span
            className={`h-1.5 w-1.5 rounded-full ${
              connection === "connected"
                ? "bg-emerald-400"
                : connection === "checking"
                  ? "animate-pulse bg-sky-400"
                  : "bg-rose-400"
            }`}
          />
          <span className={connection === "offline" ? "text-slate-500" : "text-slate-400"}>
            {connection === "connected"
              ? version
                ? `Ollama ${version}`
                : "Built-in engine"
              : connection === "checking"
                ? "Checking local models"
                : "No local models"}
          </span>
        </span>
        <Separator />
        <span
          className="flex items-center gap-1.5"
          title={
            searchError ??
            (searchProvider === "searxng" ? "Local SearXNG search" : "Built-in search")
          }
        >
          <span
            className={`h-1.5 w-1.5 rounded-full ${
              searchChecking || searchConnection === "checking"
                ? "animate-pulse bg-sky-400"
                : searchConnection === "connected"
                  ? "bg-emerald-400"
                  : "bg-amber-400/80"
            }`}
          />
          <span className={searchConnection === "offline" ? "text-slate-500" : "text-slate-400"}>
            {searchChecking || searchConnection === "checking"
              ? "Checking search"
              : searchConnection === "connected"
                ? searchProvider === "searxng"
                  ? "SearXNG ready"
                  : "Built-in search"
                : "Search offline"}
          </span>
        </span>
        <Separator />
        <span className="flex items-center gap-1.5">
          <span
            className={`h-1.5 w-1.5 rounded-full ${bridgeError ? "bg-rose-500" : "bg-emerald-400"}`}
          />
          <span className={bridgeError ? "text-rose-300" : "text-slate-500"}>
            {bridgeError ?? "Local backend connected"}
          </span>
        </span>
      </div>
    </footer>
  );
}
