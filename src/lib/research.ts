import { invoke } from "@tauri-apps/api/core";

import type { ChatTurn } from "./ollama";

/** Mirrors `commands/research.rs`. The endpoint is fixed in Rust. */
export type ResearchStatus = {
  connected: boolean;
  /** `searxng` when the local instance answers, `builtin` when the app searches directly. */
  provider: string;
  error: string | null;
};

export type StartResearchRequest = {
  run_id: string;
  model: string;
  query: string;
  history: ChatTurn[];
};

export function getResearchStatus(): Promise<ResearchStatus> {
  return invoke<ResearchStatus>("research_status");
}

export function startResearch(request: StartResearchRequest): Promise<void> {
  return invoke<void>("start_research", { request });
}
