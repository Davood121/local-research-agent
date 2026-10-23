import { invoke } from "@tauri-apps/api/core";

/** Mirrors `ollama.rs`. The endpoint itself is never sent from the webview. */
export type OllamaModel = {
  name: string;
  size_bytes: number;
  parameter_size: string;
  quantization: string;
  family: string;
  context_length: number | null;
};

export type OllamaStatus = {
  connected: boolean;
  version: string | null;
  models: OllamaModel[];
  error: string | null;
};

export type ChatRole = "system" | "user" | "assistant";

export type ChatTurn = {
  role: ChatRole;
  content: string;
};

export type StartChatRequest = {
  run_id: string;
  model: string;
  messages: ChatTurn[];
};

export function getOllamaStatus(): Promise<OllamaStatus> {
  return invoke<OllamaStatus>("ollama_status");
}

export function startChat(request: StartChatRequest): Promise<void> {
  return invoke<void>("start_chat", { request });
}

export function cancelChat(): Promise<void> {
  return invoke<void>("cancel_chat");
}

/** A run id that is safe for the backend's ASCII validation. */
export function createRunId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  const suffix = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return `run-${suffix}`;
}
