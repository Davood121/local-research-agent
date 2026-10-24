import { useRef, useState } from "react";
import { useVaultStore } from "../stores/vaultStore";

export function VaultDropzone() {
  const documents = useVaultStore((state) => state.documents);
  const addDocument = useVaultStore((state) => state.addDocument);
  const removeDocument = useVaultStore((state) => state.removeDocument);
  const clearVault = useVaultStore((state) => state.clearVault);

  const [isDragging, setIsDragging] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const processFile = (file: File) => {
    const reader = new FileReader();
    reader.onload = (e) => {
      const content = e.target?.result;
      if (typeof content === "string") {
        addDocument(file.name, content, file.type || "text/plain");
      }
    };
    reader.readAsText(file);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    const files = Array.from(e.dataTransfer.files);
    files.forEach(processFile);
  };

  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files || []);
    files.forEach(processFile);
    if (fileInputRef.current) fileInputRef.current.value = "";
  };

  return (
    <div className="space-y-3">
      {/* Drag & Drop Area */}
      <div
        onDragOver={(e) => {
          e.preventDefault();
          setIsDragging(true);
        }}
        onDragLeave={() => setIsDragging(false)}
        onDrop={handleDrop}
        onClick={() => fileInputRef.current?.click()}
        className={`group relative flex cursor-pointer flex-col items-center justify-center rounded-xl border border-dashed p-4 text-center transition ${
          isDragging
            ? "border-sky-400 bg-sky-400/10 shadow-[0_0_20px_rgba(56,189,248,0.2)]"
            : "border-white/[0.08] bg-black/20 hover:border-white/20 hover:bg-white/[0.03]"
        }`}
      >
        <input
          ref={fileInputRef}
          type="file"
          multiple
          accept=".txt,.md,.markdown,.json,.csv,.js,.ts,.py,.html"
          onChange={handleFileSelect}
          className="hidden"
        />

        <div className="flex h-8 w-8 items-center justify-center rounded-lg border border-sky-400/20 bg-sky-400/10 text-sky-400 transition group-hover:scale-110">
          📁
        </div>
        <p className="mt-2 text-[11px] font-medium text-slate-300">
          Drop research files or <span className="text-sky-400 underline">browse</span>
        </p>
        <p className="mt-0.5 text-[9.5px] text-slate-500">
          .txt, .md, .pdf, .json, .csv (100% private & local)
        </p>
      </div>

      {/* Active Vault Documents List */}
      {documents.length > 0 ? (
        <div className="space-y-2">
          <div className="flex items-center justify-between px-1">
            <span className="text-[10px] font-semibold uppercase tracking-wider text-slate-400">
              Vault Documents ({documents.length})
            </span>
            <button
              type="button"
              onClick={clearVault}
              className="text-[9.5px] text-slate-500 hover:text-rose-300 transition"
            >
              Clear all
            </button>
          </div>

          <div className="space-y-1.5 max-h-48 overflow-y-auto pr-0.5">
            {documents.map((doc) => (
              <div
                key={doc.id}
                className="flex items-center justify-between gap-2 rounded-lg border border-white/[0.06] bg-white/[0.02] p-2 text-left"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[11px] font-medium text-slate-200" title={doc.name}>
                    📄 {doc.name}
                  </p>
                  <p className="mt-0.5 text-[9px] text-slate-500">
                    {doc.wordCount.toLocaleString()} words · {(doc.size / 1024).toFixed(1)} KB
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => removeDocument(doc.id)}
                  className="rounded p-1 text-slate-500 hover:bg-white/[0.06] hover:text-rose-300 text-xs transition"
                  title="Remove document from research context"
                >
                  ✕
                </button>
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}
