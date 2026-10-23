import { invoke } from "@tauri-apps/api/core";

export type InferenceDevice = "cpu" | "gpu";

export type GpuInfo = {
  name: string;
  backend: string;
  memory_total_bytes: number;
  memory_free_bytes: number;
};

export type LoadedEngineModel = {
  name: string;
  path: string;
  context_length: number;
  device: InferenceDevice;
};

export type EngineStatus = {
  loaded: LoadedEngineModel | null;
  files: { name: string; size_bytes: number }[];
  models_dir: string | null;
  device: InferenceDevice;
  gpu: GpuInfo | null;
};

export function getEngineStatus(): Promise<EngineStatus> {
  return invoke<EngineStatus>("engine_status");
}

/** Persists the choice and unloads resident weights so the next run reloads. */
export function setInferenceDevice(device: InferenceDevice): Promise<EngineStatus> {
  return invoke<EngineStatus>("set_inference_device", { device });
}
