import { create } from "zustand";

export type VaultDocument = {
  id: string;
  name: string;
  size: number;
  type: string;
  content: string;
  wordCount: number;
  createdAt: number;
};

type VaultState = {
  documents: VaultDocument[];
  addDocument: (name: string, content: string, type?: string) => void;
  removeDocument: (id: string) => void;
  clearVault: () => void;
  getVaultContext: () => string;
};

const VAULT_STORAGE_KEY = "lra.localVault";

function loadStoredVault(): VaultDocument[] {
  try {
    const raw = localStorage.getItem(VAULT_STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function persistVault(docs: VaultDocument[]) {
  try {
    localStorage.setItem(VAULT_STORAGE_KEY, JSON.stringify(docs));
  } catch {
    // Ignore storage quota limits
  }
}

export const useVaultStore = create<VaultState>((set, get) => ({
  documents: loadStoredVault(),

  addDocument: (name, content, type = "text/plain") => {
    const wordCount = (content.match(/\S+/g) || []).length;
    const newDoc: VaultDocument = {
      id: `doc-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
      name,
      size: content.length,
      type,
      content,
      wordCount,
      createdAt: Date.now(),
    };

    const updated = [newDoc, ...get().documents].slice(0, 20); // Keep up to 20 documents
    set({ documents: updated });
    persistVault(updated);
  },

  removeDocument: (id) => {
    const updated = get().documents.filter((doc) => doc.id !== id);
    set({ documents: updated });
    persistVault(updated);
  },

  clearVault: () => {
    set({ documents: [] });
    persistVault([]);
  },

  getVaultContext: () => {
    const docs = get().documents;
    if (docs.length === 0) return "";

    return docs
      .map(
        (doc, index) =>
          `[LOCAL VAULT DOCUMENT #${index + 1}: ${doc.name}]\n${doc.content.slice(0, 3000)}\n[/LOCAL VAULT DOCUMENT #${index + 1}]`,
      )
      .join("\n\n");
  },
}));
