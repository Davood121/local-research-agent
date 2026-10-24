import { useChatStore } from "../stores/chatStore";
import { useSessionStore } from "../stores/sessionStore";
import { Composer } from "./Composer";
import { EmptyState } from "./EmptyState";
import { MessageList } from "./MessageList";
import { ExportIcon, PlusIcon, XIcon } from "./Icons";

export function ChatPanel() {
  const messages = useChatStore((state) => state.messages);
  const isStreaming = useChatStore((state) => state.isStreaming);
  const error = useChatStore((state) => state.error);
  const newChat = useChatStore((state) => state.newChat);
  const clearError = useChatStore((state) => state.clearError);
  const openExport = useSessionStore((state) => state.openExport);

  return (
    <section
      aria-label="Local research chat"
      className="relative flex min-w-0 flex-1 flex-col bg-[#060b15]"
    >
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-x-0 top-0 h-48 bg-[radial-gradient(ellipse_at_top,rgba(14,165,233,0.07),transparent_65%)]"
      />

      <header className="relative z-10 flex h-[52px] shrink-0 items-center justify-between border-b border-white/[0.045] px-5 sm:px-7">
        <div className="flex min-w-0 items-center gap-2.5">
          <h2 className="text-[11px] font-semibold uppercase tracking-[0.18em] text-slate-400">
            Conversation
          </h2>
          <span className="h-3 w-px bg-white/10" />
          <span className="truncate text-[10.5px] text-slate-600">
            {messages.length === 0
              ? "New local session"
              : `${messages.filter((entry) => entry.role === "user").length} prompt${
                  messages.filter((entry) => entry.role === "user").length === 1 ? "" : "s"
                }`}
          </span>
        </div>

        <div className="flex items-center gap-2">
          {messages.length > 0 ? (
            <button
              type="button"
              onClick={() => openExport()}
              disabled={isStreaming}
              className="inline-flex items-center gap-1.5 rounded-lg border border-white/[0.07] bg-white/[0.03] px-2.5 py-1.5 text-[10.5px] font-medium text-slate-400 transition hover:border-white/15 hover:bg-white/[0.06] hover:text-slate-200 disabled:opacity-40"
              title="Export research dossier"
            >
              <ExportIcon className="h-3.5 w-3.5" />
              Export
            </button>
          ) : null}

          <button
            type="button"
            onClick={newChat}
            disabled={isStreaming || messages.length === 0}
            className="inline-flex items-center gap-1.5 rounded-lg border border-white/[0.07] bg-white/[0.03] px-2.5 py-1.5 text-[10.5px] font-medium text-slate-400 transition hover:border-white/15 hover:bg-white/[0.06] hover:text-slate-200 disabled:cursor-not-allowed disabled:opacity-40"
          >
            <PlusIcon className="h-3.5 w-3.5" />
            New chat
          </button>
        </div>
      </header>

      {messages.length === 0 ? <EmptyState /> : <MessageList />}

      {error ? (
        <div className="relative z-10 px-4 sm:px-6">
          <div className="mx-auto flex w-full max-w-3xl items-start justify-between gap-3 rounded-t-xl border border-b-0 border-rose-400/15 bg-rose-400/[0.055] px-3.5 py-2.5">
            <p className="text-[11.5px] leading-relaxed text-rose-200/90" role="alert">
              {error}
            </p>
            <button
              type="button"
              onClick={clearError}
              aria-label="Dismiss error"
              className="mt-0.5 shrink-0 rounded p-0.5 text-rose-300/60 transition hover:bg-rose-400/10 hover:text-rose-200"
            >
              <XIcon className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
      ) : null}

      <Composer />
    </section>
  );
}
