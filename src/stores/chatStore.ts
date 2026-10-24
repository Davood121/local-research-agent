import { create } from "zustand";
import { describeError } from "../lib/errors";
import type {
  AgentErrorEvent,
  ResearchSource,
  SourcesEvent,
  StepEvent,
  TokenEvent,
} from "../lib/events";
import {
  cancelChat,
  createRunId,
  getOllamaStatus,
  startChat,
  type ChatTurn,
  type OllamaModel,
} from "../lib/ollama";
import { startResearch } from "../lib/research";
import { useActivityStore } from "./activityStore";
import { useMetricsStore } from "./metricsStore";
import { useResearchStore } from "./researchStore";
import { useSessionStore } from "./sessionStore";
import { useVaultStore } from "./vaultStore";

const MODEL_STORAGE_KEY = "lra.selectedModel";
const MAX_HISTORY_MESSAGES = 24;
const MAX_RESEARCH_HISTORY_MESSAGES = 16;
const MAX_HISTORY_CHARS = 80_000;

export type ConnectionPhase = "checking" | "connected" | "offline";
export type MessagePhase = "done" | "streaming" | "stopped" | "error";

export type ChatEntry = {
  id: string;
  role: "user" | "assistant";
  content: string;
  createdAt: number;
  state: MessagePhase;
  model?: string;
  runId?: string;
  mode?: "chat" | "research";
  sources?: ResearchSource[];
};

type ChatState = {
  connection: ConnectionPhase;
  checking: boolean;
  version: string | null;
  models: OllamaModel[];
  selectedModel: string | null;
  statusError: string | null;
  messages: ChatEntry[];
  isStreaming: boolean;
  activeRunId: string | null;
  cancelRequested: boolean;
  error: string | null;
  lastDurationMs: number | null;
  refresh: () => Promise<void>;
  selectModel: (name: string) => void;
  send: (content: string, mode?: "chat" | "research") => boolean;
  stop: () => Promise<void>;
  newChat: () => void;
  loadSession: (messages: ChatEntry[]) => void;
  clearError: () => void;
  onToken: (event: TokenEvent) => void;
  onStep: (event: StepEvent) => void;
  onSources: (event: SourcesEvent) => void;
  onError: (event: AgentErrorEvent) => void;
};

function readStoredModel(): string | null {
  try {
    return window.localStorage.getItem(MODEL_STORAGE_KEY);
  } catch {
    return null;
  }
}

function storeModel(name: string): void {
  try {
    window.localStorage.setItem(MODEL_STORAGE_KEY, name);
  } catch {
    // A blocked localStorage only loses the selection preference, not a feature.
  }
}

function buildHistory(
  entries: ChatEntry[],
  prompt?: string,
  maxMessages = MAX_HISTORY_MESSAGES - 1,
): ChatTurn[] {
  const history: ChatTurn[] = [];
  let chars = prompt?.length ?? 0;

  for (let index = entries.length - 1; index >= 0; index -= 1) {
    if (history.length >= maxMessages) break;
    const entry = entries[index];
    if (entry.content.trim().length === 0) continue;
    if (entry.state === "error") continue;
    if (chars + entry.content.length > MAX_HISTORY_CHARS) break;
    history.unshift({ role: entry.role, content: entry.content });
    chars += entry.content.length;
  }

  if (prompt !== undefined) {
    history.push({ role: "user", content: prompt });
  }
  return history;
}

export const useChatStore = create<ChatState>((set, get) => ({
  connection: "checking",
  checking: false,
  version: null,
  models: [],
  selectedModel: readStoredModel(),
  statusError: null,
  messages: [],
  isStreaming: false,
  activeRunId: null,
  cancelRequested: false,
  error: null,
  lastDurationMs: null,

  refresh: async () => {
    if (get().checking) return;
    set({ checking: true });

    try {
      const status = await getOllamaStatus();
      const models = (status.models ?? []).filter((model) => !model.name.endsWith(":cloud"));
      const previous = get().selectedModel;
      const selected =
        models.find((model) => model.name === previous)?.name ??
        models[0]?.name ??
        (status.connected ? null : previous);

      if (selected) storeModel(selected);
      set({
        connection: status.connected ? "connected" : "offline",
        checking: false,
        version: status.version,
        models,
        selectedModel: selected,
        statusError: status.error,
      });
    } catch (error) {
      set({
        connection: "offline",
        checking: false,
        version: null,
        models: [],
        statusError: describeError(error),
      });
    }
  },

  selectModel: (name) => {
    if (get().isStreaming) return;
    set({ selectedModel: name });
    storeModel(name);
  },

  send: (rawContent, mode = "chat") => {
    const prompt = rawContent.trim();
    const state = get();
    if (!prompt || state.isStreaming || state.connection !== "connected" || !state.selectedModel) {
      return false;
    }
    if (mode === "research" && useResearchStore.getState().connection !== "connected") {
      return false;
    }

    const runId = createRunId();
    const now = Date.now();
    const model = state.selectedModel;
    const userEntry: ChatEntry = {
      id: `${runId}-user`,
      role: "user",
      content: prompt,
      createdAt: now,
      state: "done",
      mode,
    };
    const assistantEntry: ChatEntry = {
      id: `${runId}-assistant`,
      role: "assistant",
      content: "",
      createdAt: now,
      state: "streaming",
      model,
      runId,
      mode,
      sources: mode === "research" ? [] : undefined,
    };
    const history =
      mode === "research"
        ? buildHistory(state.messages, undefined, MAX_RESEARCH_HISTORY_MESSAGES)
        : buildHistory(state.messages, prompt);

    useActivityStore.getState().reset();
    useMetricsStore.getState().reset();
    set({
      messages: [...state.messages, userEntry, assistantEntry],
      isStreaming: true,
      activeRunId: runId,
      cancelRequested: false,
      error: null,
      lastDurationMs: null,
    });

    void (async () => {
      try {
        if (mode === "research") {
          const vaultContext = useVaultStore.getState().getVaultContext();
          const enrichedQuery = vaultContext
            ? `${vaultContext}\n\n---\nResearch query: ${prompt}`
            : prompt;
          await startResearch({
            run_id: runId,
            model,
            query: enrichedQuery,
            history,
          });
        } else {
          await startChat({
            run_id: runId,
            model,
            messages: history,
          });
        }
      } catch (error) {
        const message = describeError(error);
        set((current) => ({
          isStreaming: false,
          activeRunId: null,
          cancelRequested: false,
          error: message,
          messages: current.messages.map((entry) =>
            entry.id === assistantEntry.id
              ? {
                  ...entry,
                  state: "error",
                  content:
                    entry.content ||
                    (mode === "research"
                      ? "The local research run could not start."
                      : "The local model could not start this response."),
                }
              : entry,
          ),
        }));
      }
    })();

    return true;
  },

  stop: async () => {
    if (!get().isStreaming || get().cancelRequested) return;
    set({ cancelRequested: true });
    try {
      await cancelChat();
    } catch (error) {
      if (get().isStreaming) {
        set({ cancelRequested: false, error: describeError(error) });
      }
    }
  },

  newChat: () => {
    if (get().isStreaming) return;
    useSessionStore.setState({ activeSessionId: null });
    set({
      messages: [],
      error: null,
      lastDurationMs: null,
      cancelRequested: false,
    });
    useActivityStore.getState().reset();
    useMetricsStore.getState().reset();
  },

  loadSession: (messages) => {
    if (get().isStreaming) return;
    set({
      messages,
      error: null,
      lastDurationMs: null,
      cancelRequested: false,
      activeRunId: null,
      isStreaming: false,
    });
    useActivityStore.getState().reset();
    useMetricsStore.getState().reset();
  },

  clearError: () => set({ error: null }),

  onToken: (event) => {
    const { activeRunId } = get();
    if (!activeRunId || event.run_id !== activeRunId) return;
    set((state) => ({
      messages: state.messages.map((entry) =>
        entry.role === "assistant" && entry.runId === event.run_id
          ? { ...entry, content: entry.content + event.delta }
          : entry,
      ),
    }));
  },

  onStep: (event) => {
    const state = get();
    if (!state.activeRunId || event.run_id !== state.activeRunId) return;
    if (event.status === "Started") {
      set({ isStreaming: true });
      return;
    }
    // The research step closes before chat starts. Only a failed research step
    // ends the run; a successful one hands off to the chat step.
    if (event.node === "research" && event.status === "Done") return;
    if (event.node !== "chat" && event.node !== "research") return;

    const stopped = state.cancelRequested || state.error === "Generation stopped.";
    const failed = event.status === "Failed";
    const fallback =
      event.node === "research"
        ? "The local research run stopped before completing."
        : "The local model stopped before completing.";
    const finalState: MessagePhase = failed ? (stopped ? "stopped" : "error") : "done";
    const updatedMessages: ChatEntry[] = state.messages.map((entry) =>
      entry.role === "assistant" && entry.runId === event.run_id
        ? {
            ...entry,
            state: finalState,
          }
        : entry,
    );
    set({
      isStreaming: false,
      activeRunId: null,
      cancelRequested: false,
      lastDurationMs: event.duration_ms,
      error: stopped || !failed ? null : (state.error ?? fallback),
      messages: updatedMessages,
    });
    if (!failed && !stopped) {
      useSessionStore.getState().saveSession(updatedMessages);
    }
  },

  onSources: (event) => {
    const { activeRunId } = get();
    if (!activeRunId || event.run_id !== activeRunId) return;
    set((state) => ({
      messages: state.messages.map((entry) =>
        entry.role === "assistant" && entry.runId === event.run_id
          ? { ...entry, sources: event.sources }
          : entry,
      ),
    }));
  },

  onError: (event) => {
    if (
      !get().activeRunId ||
      (event.scope !== "ollama" && event.scope !== "chat" && event.scope !== "research")
    )
      return;
    if (event.message === "Generation stopped.") {
      set({ cancelRequested: true });
      return;
    }
    set({ error: event.message });
  },
}));
