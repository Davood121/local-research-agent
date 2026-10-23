import { create } from "zustand";
import type { MetricsEvent } from "../lib/events";

const HISTORY_LIMIT = 60;

type MetricsState = {
  latest: MetricsEvent | null;
  tokPerSecHistory: number[];
  onMetrics: (event: MetricsEvent) => void;
  reset: () => void;
};

export const useMetricsStore = create<MetricsState>((set) => ({
  latest: null,
  tokPerSecHistory: [],
  onMetrics: (event) =>
    set((state) => ({
      latest: event,
      tokPerSecHistory: [...state.tokPerSecHistory, event.tok_per_sec].slice(-HISTORY_LIMIT),
    })),
  reset: () => set({ latest: null, tokPerSecHistory: [] }),
}));
