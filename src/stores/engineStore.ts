import { create } from "zustand";
import { describeError } from "../lib/errors";
import {
  getEngineStatus,
  setInferenceDevice,
  type GpuInfo,
  type InferenceDevice,
  type LoadedEngineModel,
} from "../lib/engine";

type EngineState = {
  device: InferenceDevice;
  gpu: GpuInfo | null;
  loaded: LoadedEngineModel | null;
  changing: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  setDevice: (device: InferenceDevice) => Promise<void>;
  clearError: () => void;
};

function applyStatus(
  status: Awaited<ReturnType<typeof getEngineStatus>>,
): Pick<EngineState, "device" | "gpu" | "loaded"> {
  return { device: status.device, gpu: status.gpu, loaded: status.loaded };
}

export const useEngineStore = create<EngineState>((set, get) => ({
  device: "cpu",
  gpu: null,
  loaded: null,
  changing: false,
  error: null,

  refresh: async () => {
    try {
      const status = await getEngineStatus();
      set(applyStatus(status));
    } catch {
      // Status polling is best-effort: a transient failure must not clear the
      // last known device or spam the UI.
    }
  },

  setDevice: async (device) => {
    if (get().changing || get().device === device) return;
    set({ changing: true, error: null });
    try {
      const status = await setInferenceDevice(device);
      set({ ...applyStatus(status), changing: false });
    } catch (error) {
      set({ changing: false, error: describeError(error) });
    }
  },

  clearError: () => set({ error: null }),
}));
