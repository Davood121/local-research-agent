import { invoke } from "@tauri-apps/api/core";

export type CatalogModel = {
  id: string;
  name: string;
  filename: string;
  size_bytes: number;
  description: string;
  installed: boolean;
};

/** Curated allowlist from the backend; the webview never supplies a URL. */
export function listModelCatalog(): Promise<CatalogModel[]> {
  return invoke<CatalogModel[]>("list_model_catalog");
}

/** Resolves when the download finishes, fails, or is cancelled. */
export function downloadModel(id: string): Promise<void> {
  return invoke<void>("download_model", { id });
}

/** Idempotent; returns whether an active download was cancelled. */
export function cancelModelDownload(): Promise<boolean> {
  return invoke<boolean>("cancel_model_download");
}

/** Human-readable size for catalog and progress rows. */
export function formatModelSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "unknown size";
  const gb = bytes / 1_000_000_000;
  if (gb >= 1) return `${gb.toFixed(2)} GB`;
  return `${Math.round(bytes / 1_000_000)} MB`;
}
