import { create } from "zustand";

import { describeError } from "../lib/errors";
import { getResearchStatus } from "../lib/research";

const MODE_STORAGE_KEY = "lra.researchMode";

export type ResearchMode = "chat" | "research";
export type SearchConnection = "checking" | "connected" | "offline";

type ResearchState = {
  connection: SearchConnection;
  checking: boolean;
  error: string | null;
  mode: ResearchMode;
  refresh: () => Promise<void>;
  setMode: (mode: ResearchMode) => void;
};

function readStoredMode(): ResearchMode {
  try {
    return window.localStorage.getItem(MODE_STORAGE_KEY) === "research" ? "research" : "chat";
  } catch {
    return "chat";
  }
}

function storeMode(mode: ResearchMode): void {
  try {
    window.localStorage.setItem(MODE_STORAGE_KEY, mode);
  } catch {
    // The selected mode is a convenience; a blocked storage API loses only that.
  }
}

export const useResearchStore = create<ResearchState>((set, get) => ({
  connection: "checking",
  checking: false,
  error: null,
  mode: readStoredMode(),

  refresh: async () => {
    if (get().checking) return;
    set({ checking: true });
    try {
      const status = await getResearchStatus();
      set({
        checking: false,
        connection: status.connected ? "connected" : "offline",
        error: status.error,
      });
    } catch (error) {
      set({
        checking: false,
        connection: "offline",
        error: describeError(error),
      });
    }
  },

  setMode: (mode) => {
    set({ mode });
    storeMode(mode);
  },
}));
