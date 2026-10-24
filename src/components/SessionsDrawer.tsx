import { useState } from "react";
import { GlobeIcon, PlusIcon, XIcon } from "./Icons";
import { useChatStore } from "../stores/chatStore";
import { useSessionStore, type SavedSession } from "../stores/sessionStore";

export function SessionsDrawer() {
  const drawerOpen = useSessionStore((state) => state.drawerOpen);
  const closeDrawer = useSessionStore((state) => state.closeDrawer);
  const sessions = useSessionStore((state) => state.sessions);
  const activeSessionId = useSessionStore((state) => state.activeSessionId);
  const deleteSession = useSessionStore((state) => state.deleteSession);
  const clearAll = useSessionStore((state) => state.clearAll);
  const renameSession = useSessionStore((state) => state.renameSession);
  const searchQuery = useSessionStore((state) => state.searchQuery);
  const setSearchQuery = useSessionStore((state) => state.setSearchQuery);

  const loadSession = useChatStore((state) => state.loadSession);
  const newChat = useChatStore((state) => state.newChat);

  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState("");

  if (!drawerOpen) return null;

  const filtered = sessions.filter((s) =>
    s.title.toLowerCase().includes(searchQuery.toLowerCase()),
  );

  const handleSelect = (session: SavedSession) => {
    useSessionStore.setState({ activeSessionId: session.id });
    loadSession(session.messages);
    closeDrawer();
  };

  const handleNew = () => {
    newChat();
    closeDrawer();
  };

  const startRename = (session: SavedSession, e: React.MouseEvent) => {
    e.stopPropagation();
    setEditingId(session.id);
    setEditingTitle(session.title);
  };

  const saveRename = (id: string, e: React.FormEvent) => {
    e.preventDefault();
    if (editingTitle.trim()) {
      renameSession(id, editingTitle.trim());
    }
    setEditingId(null);
  };

  return (
    <div
      className="motion-fade-in fixed inset-0 z-50 flex justify-end bg-black/60 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) closeDrawer();
      }}
    >
      <div className="motion-slide-in flex h-full w-full max-w-sm flex-col border-l border-white/10 bg-[#070c16] shadow-2xl">
        <header className="flex h-16 shrink-0 items-center justify-between border-b border-white/[0.06] px-5">
          <div>
            <h2 className="text-sm font-semibold text-slate-100">Research Archive</h2>
            <p className="text-[11px] text-slate-500">
              {sessions.length} local session{sessions.length === 1 ? "" : "s"} stored
            </p>
          </div>
          <div className="flex items-center gap-1.5">
            <button
              type="button"
              onClick={handleNew}
              title="Start fresh session"
              className="flex h-8 w-8 items-center justify-center rounded-lg border border-white/10 bg-white/[0.04] text-slate-400 hover:border-white/20 hover:text-slate-100"
            >
              <PlusIcon className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              onClick={closeDrawer}
              aria-label="Close"
              className="flex h-8 w-8 items-center justify-center rounded-lg border border-white/10 bg-white/[0.04] text-slate-400 hover:border-white/20 hover:text-slate-100"
            >
              <XIcon className="h-3.5 w-3.5" />
            </button>
          </div>
        </header>

        <div className="p-3 border-b border-white/[0.06]">
          <input
            type="text"
            placeholder="Search past research…"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full rounded-lg border border-white/10 bg-black/30 px-3 py-1.5 text-xs text-slate-200 placeholder-slate-500 outline-none focus:border-sky-400/40"
          />
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto p-3 space-y-2">
          {filtered.length === 0 ? (
            <div className="rounded-xl border border-dashed border-white/[0.07] p-6 text-center">
              <p className="text-xs text-slate-500">
                {sessions.length === 0
                  ? "No research sessions saved yet."
                  : "No sessions match your search."}
              </p>
              <p className="mt-1 text-[11px] text-slate-600">
                Sessions are automatically saved locally when generation completes.
              </p>
            </div>
          ) : (
            filtered.map((session) => {
              const isActive = session.id === activeSessionId;
              const isResearch = session.mode === "research";
              const dateStr = new Date(session.updatedAt).toLocaleDateString(undefined, {
                month: "short",
                day: "numeric",
                hour: "2-digit",
                minute: "2-digit",
              });

              return (
                <div
                  key={session.id}
                  onClick={() => handleSelect(session)}
                  className={`group relative cursor-pointer rounded-xl border p-3 transition ${
                    isActive
                      ? "border-sky-400/40 bg-sky-400/[0.08]"
                      : "border-white/[0.06] bg-white/[0.025] hover:border-white/15 hover:bg-white/[0.05]"
                  }`}
                >
                  <div className="flex items-start justify-between gap-2">
                    <span
                      className={`inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[9.5px] font-semibold uppercase tracking-wider ${
                        isResearch
                          ? "bg-emerald-400/15 text-emerald-300"
                          : "bg-sky-400/15 text-sky-300"
                      }`}
                    >
                      {isResearch ? <GlobeIcon className="h-2.5 w-2.5" /> : null}
                      {isResearch ? "Research" : "Chat"}
                    </span>
                    <span className="text-[10px] text-slate-500">{dateStr}</span>
                  </div>

                  {editingId === session.id ? (
                    <form onSubmit={(e) => saveRename(session.id, e)} className="mt-2 flex gap-1">
                      <input
                        type="text"
                        value={editingTitle}
                        onChange={(e) => setEditingTitle(e.target.value)}
                        autoFocus
                        onClick={(e) => e.stopPropagation()}
                        className="min-w-0 flex-1 rounded border border-sky-400/50 bg-black/60 px-2 py-0.5 text-xs text-slate-100 outline-none"
                      />
                      <button
                        type="submit"
                        onClick={(e) => e.stopPropagation()}
                        className="rounded bg-sky-500/20 px-2 py-0.5 text-[11px] text-sky-200 hover:bg-sky-500/40"
                      >
                        Save
                      </button>
                    </form>
                  ) : (
                    <p className="mt-1.5 line-clamp-2 text-xs font-medium text-slate-200">
                      {session.title}
                    </p>
                  )}

                  <div className="mt-2.5 flex items-center justify-between border-t border-white/[0.04] pt-2 text-[10px] text-slate-500">
                    <span>
                      {session.messages.length} message{session.messages.length === 1 ? "" : "s"}
                    </span>

                    <div className="flex items-center gap-2 opacity-0 group-hover:opacity-100 transition">
                      <button
                        type="button"
                        onClick={(e) => startRename(session, e)}
                        className="text-slate-400 hover:text-slate-200"
                        title="Rename"
                      >
                        Rename
                      </button>
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          deleteSession(session.id);
                        }}
                        className="text-rose-400 hover:text-rose-300"
                        title="Delete"
                      >
                        Delete
                      </button>
                    </div>
                  </div>
                </div>
              );
            })
          )}
        </div>

        {sessions.length > 0 && (
          <footer className="border-t border-white/[0.06] p-3">
            <button
              type="button"
              onClick={() => {
                if (window.confirm("Clear all saved research sessions?")) clearAll();
              }}
              className="w-full rounded-lg border border-rose-500/20 bg-rose-500/[0.05] py-1.5 text-xs font-medium text-rose-300 hover:bg-rose-500/[0.1] transition"
            >
              Clear Entire Archive
            </button>
          </footer>
        )}
      </div>
    </div>
  );
}
