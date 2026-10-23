import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Phase = "idle" | "running" | "error";

/**
 * Dev-only probe for the event pipe. The command itself only exists in debug
 * builds, so this button is compiled out of production bundles as well.
 */
export function DebugStreamButton() {
  const [phase, setPhase] = useState<Phase>("idle");

  if (!import.meta.env.DEV) return null;

  const start = async () => {
    setPhase("running");
    try {
      await invoke("debug_start_stream");
      window.setTimeout(() => setPhase("idle"), 10_500);
    } catch (error) {
      console.error("debug_start_stream failed", error);
      setPhase("error");
    }
  };

  return (
    <button
      type="button"
      onClick={() => void start()}
      disabled={phase === "running"}
      title="Emit fake Token and Metrics events at 10 Hz for 10 seconds"
      className="rounded-md border border-sky-800 bg-sky-950/50 px-2.5 py-1 text-[11px] font-medium text-sky-200 transition hover:bg-sky-900/60 disabled:cursor-wait disabled:opacity-60"
    >
      {phase === "running" ? "Streaming…" : phase === "error" ? "Stream failed" : "Debug Stream"}
    </button>
  );
}
