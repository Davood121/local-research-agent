import { create } from "zustand";
import type { AgentErrorEvent, StepEvent, TokenEvent, ToolEvent } from "../lib/events";

const PREVIEW_LIMIT = 800;
const LIST_LIMIT = 25;

type ActivityState = {
  tokenCount: number;
  preview: string;
  steps: StepEvent[];
  tools: ToolEvent[];
  errors: AgentErrorEvent[];
  onToken: (event: TokenEvent) => void;
  onStep: (event: StepEvent) => void;
  onTool: (event: ToolEvent) => void;
  onError: (event: AgentErrorEvent) => void;
  reset: () => void;
};

/**
 * The trace panel only needs a bounded window of recent activity; keeping every
 * token in memory would grow without limit during a long generation.
 */
export const useActivityStore = create<ActivityState>((set) => ({
  tokenCount: 0,
  preview: "",
  steps: [],
  tools: [],
  errors: [],
  onToken: (event) =>
    set((state) => ({
      tokenCount: state.tokenCount + 1,
      preview: (state.preview + event.delta).slice(-PREVIEW_LIMIT),
    })),
  onStep: (event) => set((state) => ({ steps: [event, ...state.steps].slice(0, LIST_LIMIT) })),
  onTool: (event) => set((state) => ({ tools: [event, ...state.tools].slice(0, LIST_LIMIT) })),
  onError: (event) => set((state) => ({ errors: [event, ...state.errors].slice(0, LIST_LIMIT) })),
  reset: () => set({ tokenCount: 0, preview: "", steps: [], tools: [], errors: [] }),
}));
