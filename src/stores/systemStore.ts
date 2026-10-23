import { create } from "zustand";
import type { TelemetryEvent } from "../lib/events";

type SystemState = {
  telemetry: TelemetryEvent | null;
  bridgeError: string | null;
  setTelemetry: (event: TelemetryEvent) => void;
  setBridgeError: (message: string | null) => void;
};

export const useSystemStore = create<SystemState>((set) => ({
  telemetry: null,
  bridgeError: null,
  setTelemetry: (event) => set({ telemetry: event }),
  setBridgeError: (bridgeError) => set({ bridgeError }),
}));
