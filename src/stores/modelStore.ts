import { create } from "zustand";
import { describeError } from "../lib/errors";
import type { DownloadStatus, ModelDownloadEvent } from "../lib/events";
import {
  cancelModelDownload,
  downloadModel,
  listModelCatalog,
  type CatalogModel,
} from "../lib/modelDownloads";
import { useChatStore } from "./chatStore";

export type DownloadState = {
  status: DownloadStatus;
  downloaded: number;
  total: number;
  message: string | null;
};

type ModelStore = {
  open: boolean;
  catalog: CatalogModel[];
  loading: boolean;
  error: string | null;
  downloads: Record<string, DownloadState>;
  downloading: string | null;
  openPanel: () => void;
  closePanel: () => void;
  refresh: () => Promise<void>;
  start: (id: string) => Promise<void>;
  cancel: () => Promise<void>;
  onEvent: (event: ModelDownloadEvent) => void;
};

export const useModelStore = create<ModelStore>((set, get) => ({
  open: false,
  catalog: [],
  loading: false,
  error: null,
  downloads: {},
  downloading: null,

  openPanel: () => {
    set({ open: true });
    void get().refresh();
  },

  closePanel: () => set({ open: false }),

  refresh: async () => {
    set({ loading: true, error: null });
    try {
      const catalog = await listModelCatalog();
      set({ catalog, loading: false });
    } catch (error) {
      set({ loading: false, error: describeError(error) });
    }
  },

  start: async (id) => {
    if (get().downloading) return;
    const row = get().catalog.find((model) => model.id === id);
    set((state) => ({
      downloading: id,
      error: null,
      downloads: {
        ...state.downloads,
        [id]: {
          status: "Started",
          downloaded: 0,
          total: row?.size_bytes ?? 0,
          message: null,
        },
      },
    }));

    try {
      await downloadModel(id);
    } catch (error) {
      const message = describeError(error);
      set({ error: message });
      set((state) => {
        const current = state.downloads[id];
        return {
          downloads: {
            ...state.downloads,
            [id]: {
              status: current?.status === "Cancelled" ? "Cancelled" : "Failed",
              downloaded: current?.downloaded ?? 0,
              total: current?.total ?? row?.size_bytes ?? 0,
              message,
            },
          },
        };
      });
    } finally {
      if (get().downloading === id) {
        set({ downloading: null });
      }
      void get().refresh();
    }
  },

  cancel: async () => {
    try {
      await cancelModelDownload();
    } catch (error) {
      set({ error: describeError(error) });
    }
  },

  onEvent: (event) => {
    set((state) => ({
      downloads: {
        ...state.downloads,
        [event.id]: {
          status: event.status,
          downloaded: event.downloaded_bytes,
          total: event.total_bytes,
          message: event.message,
        },
      },
      downloading:
        event.status === "Started" || event.status === "Progress"
          ? event.id
          : state.downloading === event.id
            ? null
            : state.downloading,
      error:
        event.status === "Failed" ? (event.message ?? "The model download failed.") : state.error,
    }));

    if (event.status === "Done") {
      void get().refresh();
      void useChatStore.getState().refresh();
    }
  },
}));
