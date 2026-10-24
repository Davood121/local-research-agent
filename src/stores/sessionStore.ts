import { create } from "zustand";
import type { ChatEntry } from "./chatStore";

export interface SavedSession {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  mode: "chat" | "research";
  messages: ChatEntry[];
}

const STORAGE_KEY = "lra.saved_sessions";

function loadSessionsFromStorage(): SavedSession[] {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function persistSessions(sessions: SavedSession[]): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(sessions));
  } catch {
    // Gracefully ignore storage quota errors
  }
}

interface SessionState {
  sessions: SavedSession[];
  activeSessionId: string | null;
  drawerOpen: boolean;
  exportModalOpen: boolean;
  exportEntryId: string | null;
  searchQuery: string;
  openDrawer: () => void;
  closeDrawer: () => void;
  openExport: (entryId?: string) => void;
  closeExport: () => void;
  setSearchQuery: (q: string) => void;
  saveSession: (messages: ChatEntry[]) => string | null;
  deleteSession: (id: string) => void;
  clearAll: () => void;
  renameSession: (id: string, newTitle: string) => void;
}

export const useSessionStore = create<SessionState>((set, get) => ({
  sessions: loadSessionsFromStorage(),
  activeSessionId: null,
  drawerOpen: false,
  exportModalOpen: false,
  exportEntryId: null,
  searchQuery: "",

  openDrawer: () => set({ drawerOpen: true }),
  closeDrawer: () => set({ drawerOpen: false }),
  openExport: (entryId) => set({ exportModalOpen: true, exportEntryId: entryId ?? null }),
  closeExport: () => set({ exportModalOpen: false, exportEntryId: null }),
  setSearchQuery: (searchQuery) => set({ searchQuery }),

  saveSession: (messages: ChatEntry[]) => {
    if (messages.length === 0) return null;
    const firstUserMsg = messages.find((m) => m.role === "user");
    const rawTitle = firstUserMsg ? firstUserMsg.content.slice(0, 48) : "Session";
    const title = rawTitle.length >= 48 ? `${rawTitle}…` : rawTitle;
    const mode = messages.some((m) => m.mode === "research") ? "research" : "chat";
    const now = Date.now();

    const existingId = get().activeSessionId;
    const sessions = get().sessions;

    if (existingId && sessions.some((s) => s.id === existingId)) {
      const updated = sessions.map((s) =>
        s.id === existingId ? { ...s, messages, updatedAt: now, title } : s,
      );
      set({ sessions: updated });
      persistSessions(updated);
      return existingId;
    }

    const newId = `session-${now}-${Math.random().toString(36).substring(2, 7)}`;
    const newSession: SavedSession = {
      id: newId,
      title,
      createdAt: now,
      updatedAt: now,
      mode,
      messages,
    };

    const updated = [newSession, ...sessions].slice(0, 50); // Keep up to 50 sessions
    set({ sessions: updated, activeSessionId: newId });
    persistSessions(updated);
    return newId;
  },

  deleteSession: (id: string) => {
    const updated = get().sessions.filter((s) => s.id !== id);
    set((state) => ({
      sessions: updated,
      activeSessionId: state.activeSessionId === id ? null : state.activeSessionId,
    }));
    persistSessions(updated);
  },

  clearAll: () => {
    set({ sessions: [], activeSessionId: null });
    persistSessions([]);
  },

  renameSession: (id: string, newTitle: string) => {
    const trimmed = newTitle.trim();
    if (!trimmed) return;
    const updated = get().sessions.map((s) =>
      s.id === id ? { ...s, title: trimmed, updatedAt: Date.now() } : s,
    );
    set({ sessions: updated });
    persistSessions(updated);
  },
}));
