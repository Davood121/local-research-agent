import { create } from "zustand";

import { describeError } from "../lib/errors";
import { getResearchStatus } from "../lib/research";

const MODE_STORAGE_KEY = "lra.researchMode";
const DEPTH_STORAGE_KEY = "lra.researchDepth";
const FOCUS_STORAGE_KEY = "lra.researchFocus";

export type ResearchMode = "chat" | "research";
export type SearchConnection = "checking" | "connected" | "offline";
/** Which backend answers searches: a local SearXNG or the built-in engine. */
export type SearchProvider = "searxng" | "builtin" | "none";
export type ResearchDepth = "quick" | "balanced" | "deep";
export type ResearchFocus = "general" | "academic" | "news";

type ResearchState = {
  connection: SearchConnection;
  provider: SearchProvider;
  checking: boolean;
  error: string | null;
  mode: ResearchMode;
  depth: ResearchDepth;
  focus: ResearchFocus;
  refresh: () => Promise<void>;
  setMode: (mode: ResearchMode) => void;
  setDepth: (depth: ResearchDepth) => void;
  setFocus: (focus: ResearchFocus) => void;
};

function readStoredMode(): ResearchMode {
  try {
    return window.localStorage.getItem(MODE_STORAGE_KEY) === "research" ? "research" : "chat";
  } catch {
    return "chat";
  }
}

function readStoredDepth(): ResearchDepth {
  try {
    const val = window.localStorage.getItem(DEPTH_STORAGE_KEY);
    return val === "quick" || val === "deep" ? val : "balanced";
  } catch {
    return "balanced";
  }
}

function readStoredFocus(): ResearchFocus {
  try {
    const val = window.localStorage.getItem(FOCUS_STORAGE_KEY);
    return val === "academic" || val === "news" ? val : "general";
  } catch {
    return "general";
  }
}

export const useResearchStore = create<ResearchState>((set, get) => ({
  connection: "checking",
  provider: "none",
  checking: false,
  error: null,
  mode: readStoredMode(),
  depth: readStoredDepth(),
  focus: readStoredFocus(),

  refresh: async () => {
    if (get().checking) return;
    set({ checking: true });
    try {
      const status = await getResearchStatus();
      set({
        checking: false,
        connection: status.connected ? "connected" : "offline",
        provider: status.provider === "searxng" ? "searxng" : "builtin",
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
    try {
      window.localStorage.setItem(MODE_STORAGE_KEY, mode);
    } catch {
      // ignore
    }
  },

  setDepth: (depth) => {
    set({ depth });
    try {
      window.localStorage.setItem(DEPTH_STORAGE_KEY, depth);
    } catch {
      // ignore
    }
  },

  setFocus: (focus) => {
    set({ focus });
    try {
      window.localStorage.setItem(FOCUS_STORAGE_KEY, focus);
    } catch {
      // ignore
    }
  },
}));
