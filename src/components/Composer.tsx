import { useEffect, useRef, useState } from "react";
import { useChatStore } from "../stores/chatStore";
import { useResearchStore } from "../stores/researchStore";
import { ChatIcon, GlobeIcon, RefreshIcon, SendIcon, StopIcon } from "./Icons";

export function Composer() {
  const [draft, setDraft] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const connection = useChatStore((state) => state.connection);
  const models = useChatStore((state) => state.models);
  const selectedModel = useChatStore((state) => state.selectedModel);
  const isStreaming = useChatStore((state) => state.isStreaming);
  const send = useChatStore((state) => state.send);
  const stop = useChatStore((state) => state.stop);
  const mode = useResearchStore((state) => state.mode);
  const setMode = useResearchStore((state) => state.setMode);
  const searchConnection = useResearchStore((state) => state.connection);
  const searchChecking = useResearchStore((state) => state.checking);
  const searchError = useResearchStore((state) => state.error);
  const refreshSearch = useResearchStore((state) => state.refresh);

  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = "auto";
    textarea.style.height = `${Math.min(textarea.scrollHeight, 168)}px`;
  }, [draft]);

  const ready = connection === "connected" && models.length > 0 && Boolean(selectedModel);
  const researchReady = searchConnection === "connected";
  const canSend =
    ready && (mode === "chat" || researchReady) && draft.trim().length > 0 && !isStreaming;

  const chooseMode = (next: "chat" | "research") => {
    if (isStreaming || next === mode) return;
    setMode(next);
    if (next === "research" && searchConnection === "offline" && !searchChecking) {
      void refreshSearch();
    }
  };

  const submit = () => {
    if (!canSend) return;
    if (send(draft, mode)) {
      setDraft("");
      requestAnimationFrame(() => textareaRef.current?.focus());
    }
  };

  const placeholder =
    connection === "checking"
      ? "Checking Ollama on this device…"
      : !ready
        ? connection === "offline"
          ? "Start Ollama, then refresh the model list to chat…"
          : "A local chat model is required to begin…"
        : isStreaming
          ? mode === "research"
            ? "Local research is running…"
            : "The local model is writing…"
          : mode === "research"
            ? searchChecking
              ? "Checking local search…"
              : searchConnection === "offline"
                ? "Start SearXNG at 127.0.0.1:8080, then retry…"
                : "Ask a research question; result pages are read and cited…"
            : "Ask a question, paste notes, or describe a task…";

  const researchOffline = mode === "research" && searchConnection === "offline" && !searchChecking;

  return (
    <div className="shrink-0 border-t border-white/[0.05] bg-gradient-to-t from-[#050a13] via-[#050a13] to-transparent px-4 pb-4 pt-3 sm:px-6">
      <div className="mx-auto w-full max-w-3xl">
        <div className="rounded-2xl border border-white/[0.08] bg-[#0a1220]/90 p-2.5 shadow-[0_16px_50px_-20px_rgba(0,0,0,0.9)] transition focus-within:border-sky-400/25 focus-within:ring-1 focus-within:ring-sky-400/10">
          <div className="flex flex-wrap items-center justify-between gap-2 px-1 pb-1.5">
            <div
              role="group"
              aria-label="Response mode"
              className="inline-flex rounded-lg border border-white/[0.07] bg-black/20 p-0.5"
            >
              <button
                type="button"
                aria-pressed={mode === "research"}
                onClick={() => chooseMode("research")}
                disabled={isStreaming}
                className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-[10.5px] font-semibold transition ${
                  mode === "research"
                    ? "bg-emerald-400/15 text-emerald-200 shadow-[inset_0_1px_0_rgba(255,255,255,0.06)]"
                    : "text-slate-500 hover:text-slate-300"
                } disabled:cursor-not-allowed disabled:opacity-50`}
              >
                <GlobeIcon className="h-3 w-3" />
                Research
              </button>
              <button
                type="button"
                aria-pressed={mode === "chat"}
                onClick={() => chooseMode("chat")}
                disabled={isStreaming}
                className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-[10.5px] font-semibold transition ${
                  mode === "chat"
                    ? "bg-sky-400/15 text-sky-200 shadow-[inset_0_1px_0_rgba(255,255,255,0.06)]"
                    : "text-slate-500 hover:text-slate-300"
                } disabled:cursor-not-allowed disabled:opacity-50`}
              >
                <ChatIcon className="h-3 w-3" />
                Chat
              </button>
            </div>

            <span
              title={
                searchError ??
                (mode === "research"
                  ? "SearXNG runs locally on 127.0.0.1:8080"
                  : "Prompts stay with the local Ollama instance")
              }
              className="flex min-w-0 items-center gap-1.5 pr-1 text-[10px] text-slate-600"
            >
              <span
                className={`h-1.5 w-1.5 shrink-0 rounded-full ${
                  mode === "research"
                    ? searchChecking
                      ? "animate-pulse bg-sky-400"
                      : searchConnection === "connected"
                        ? "bg-emerald-400"
                        : searchConnection === "checking"
                          ? "animate-pulse bg-sky-400"
                          : "bg-amber-400"
                    : ready
                      ? "bg-emerald-400"
                      : "bg-slate-600"
                }`}
              />
              <span className="truncate">
                {mode === "research"
                  ? searchChecking || searchConnection === "checking"
                    ? "Checking search"
                    : searchConnection === "connected"
                      ? "SearXNG ready"
                      : "Search offline"
                  : "Local inference"}
              </span>
            </span>
          </div>

          {researchOffline ? (
            <div className="mx-1 mb-1 flex items-start justify-between gap-3 rounded-lg border border-amber-400/20 bg-amber-400/[0.05] px-2.5 py-2">
              <p className="text-[10.5px] leading-relaxed text-amber-100/80">
                Local search is offline. Start SearXNG at{" "}
                <code className="font-mono text-amber-200/90">127.0.0.1:8080</code>, then retry.
              </p>
              <button
                type="button"
                onClick={() => void refreshSearch()}
                disabled={searchChecking}
                className="inline-flex shrink-0 items-center gap-1 rounded-md border border-amber-300/20 px-2 py-1 text-[10px] font-semibold text-amber-200/90 transition hover:bg-amber-300/10 disabled:cursor-not-allowed disabled:opacity-50"
              >
                <RefreshIcon className={`h-3 w-3 ${searchChecking ? "animate-spin" : ""}`} />
                {searchChecking ? "Checking" : "Retry"}
              </button>
            </div>
          ) : null}

          <textarea
            ref={textareaRef}
            aria-label="Message composer"
            value={draft}
            rows={1}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
                event.preventDefault();
                submit();
              }
            }}
            disabled={!ready}
            placeholder={placeholder}
            className="min-h-[52px] w-full resize-none bg-transparent px-2.5 py-1.5 text-[13.5px] leading-relaxed text-slate-100 outline-none placeholder:text-slate-600 disabled:cursor-not-allowed disabled:text-slate-500"
          />

          <div className="flex items-center justify-between gap-3 px-1 pt-1">
            <div className="flex min-w-0 items-center gap-2">
              <span
                className={`h-1.5 w-1.5 shrink-0 rounded-full ${
                  ready && (mode === "chat" || researchReady)
                    ? isStreaming
                      ? "animate-pulse bg-sky-400"
                      : "bg-emerald-400"
                    : mode === "research" && (searchChecking || searchConnection === "checking")
                      ? "animate-pulse bg-sky-400"
                      : researchOffline
                        ? "bg-amber-400"
                        : "bg-slate-600"
                }`}
              />
              <span className="truncate font-mono text-[10px] text-slate-500">
                {selectedModel ?? "No model selected"}
              </span>
              <span className="hidden text-[10px] text-slate-700 sm:inline">·</span>
              <span className="hidden text-[10px] text-slate-600 sm:inline">
                {isStreaming
                  ? mode === "research"
                    ? "Researching locally"
                    : "Generating locally"
                  : mode === "research"
                    ? "Reads result pages"
                    : "Stays on this device"}
              </span>
            </div>

            <div className="flex shrink-0 items-center gap-2.5">
              <span className="hidden text-[10px] text-slate-600 md:inline">
                Enter to send · Shift+Enter for a new line
              </span>
              {isStreaming ? (
                <button
                  type="button"
                  onClick={() => void stop()}
                  className="inline-flex h-9 items-center gap-2 rounded-lg border border-rose-400/25 bg-rose-400/[0.08] px-3.5 text-xs font-semibold text-rose-200 transition hover:border-rose-400/40 hover:bg-rose-400/[0.13]"
                >
                  <StopIcon className="h-3.5 w-3.5" />
                  Stop
                </button>
              ) : (
                <button
                  type="button"
                  onClick={submit}
                  disabled={!canSend}
                  className="inline-flex h-9 items-center gap-2 rounded-lg bg-gradient-to-r from-sky-500 to-cyan-400 px-3.5 text-xs font-semibold text-slate-950 shadow-[0_6px_20px_-8px_rgba(56,189,248,0.8)] transition hover:brightness-110 disabled:cursor-not-allowed disabled:bg-none disabled:bg-white/[0.04] disabled:text-slate-600 disabled:shadow-none"
                >
                  {mode === "research" ? "Research" : "Send"}
                  <SendIcon className="h-3.5 w-3.5" />
                </button>
              )}
            </div>
          </div>
        </div>

        <p className="mt-2 px-1 text-center text-[10px] leading-relaxed text-slate-700">
          {mode === "research"
            ? "Research reads result pages directly from the public web. Prompts stay with your local model; no accounts, cookies, analytics, or cloud inference."
            : "Local inference only. Prompts and responses never leave this computer."}
        </p>
      </div>
    </div>
  );
}
