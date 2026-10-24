import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";

import { useChatStore } from "../stores/chatStore";
import { useResearchStore, type ResearchDepth } from "../stores/researchStore";
import { useSessionStore } from "../stores/sessionStore";
import { ChatIcon, GlobeIcon, HistoryIcon, RefreshIcon, SendIcon } from "./Icons";

type Command = {
  id: string;
  group: "Actions" | "Models" | "Sessions";
  label: string;
  hint?: string;
  keywords?: string;
  run: () => void;
};

/**
 * Match `query` against `text` as an ordered subsequence, so "nch" finds
 * "New chat". Exact substrings rank above scattered letters.
 */
function score(query: string, text: string): number {
  const q = query.trim().toLowerCase();
  if (!q) return 1;
  const t = text.toLowerCase();
  const direct = t.indexOf(q);
  if (direct >= 0) return 1000 - direct;
  let index = 0;
  let gaps = 0;
  for (const char of q) {
    const found = t.indexOf(char, index);
    if (found < 0) return 0;
    gaps += found - index;
    index = found + 1;
  }
  return Math.max(1, 500 - gaps);
}

export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const models = useChatStore((state) => state.models);
  const selectedModel = useChatStore((state) => state.selectedModel);
  const selectModel = useChatStore((state) => state.selectModel);
  const newChat = useChatStore((state) => state.newChat);
  const loadSession = useChatStore((state) => state.loadSession);
  const refreshModels = useChatStore((state) => state.refresh);

  const mode = useResearchStore((state) => state.mode);
  const setMode = useResearchStore((state) => state.setMode);
  const setDepth = useResearchStore((state) => state.setDepth);
  const refreshSearch = useResearchStore((state) => state.refresh);

  const sessions = useSessionStore((state) => state.sessions);
  const openDrawer = useSessionStore((state) => state.openDrawer);
  const openExport = useSessionStore((state) => state.openExport);

  const close = () => setOpen(false);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const combo = (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k";
      if (combo) {
        event.preventDefault();
        setOpen((value) => !value);
        return;
      }
      if (event.key === "Escape") setOpen(false);
    };
    const onRequest = () => setOpen(true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("lra:command-palette", onRequest);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("lra:command-palette", onRequest);
    };
  }, []);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActive(0);
    const timer = window.setTimeout(() => inputRef.current?.focus(), 20);
    return () => window.clearTimeout(timer);
  }, [open]);

  const commands = useMemo<Command[]>(() => {
    const list: Command[] = [
      {
        id: "new-chat",
        group: "Actions",
        label: "New conversation",
        hint: "Clear the current thread",
        keywords: "reset clear thread",
        run: () => {
          newChat();
          close();
        },
      },
      {
        id: "mode-chat",
        group: "Actions",
        label: "Switch to chat mode",
        hint: mode === "chat" ? "Active" : undefined,
        keywords: "answer ask local",
        run: () => {
          setMode("chat");
          close();
        },
      },
      {
        id: "mode-research",
        group: "Actions",
        label: "Switch to research mode",
        hint: mode === "research" ? "Active" : undefined,
        keywords: "web search cite sources",
        run: () => {
          setMode("research");
          close();
        },
      },
      ...(["quick", "balanced", "deep"] as ResearchDepth[]).map<Command>((depth) => ({
        id: `depth-${depth}`,
        group: "Actions",
        label: `Research depth: ${depth}`,
        keywords: "effort thorough",
        run: () => {
          setDepth(depth);
          setMode("research");
          close();
        },
      })),
      {
        id: "archive",
        group: "Actions",
        label: "Open research archive",
        hint: `${sessions.length} saved`,
        keywords: "sessions history restore",
        run: () => {
          openDrawer();
          close();
        },
      },
      {
        id: "export",
        group: "Actions",
        label: "Export this session",
        hint: "Markdown, HTML or JSON",
        keywords: "save download share",
        run: () => {
          openExport();
          close();
        },
      },
      {
        id: "refresh",
        group: "Actions",
        label: "Refresh models and search",
        keywords: "reload rescan status",
        run: () => {
          void refreshModels();
          void refreshSearch();
          close();
        },
      },
    ];

    for (const model of models) {
      list.push({
        id: `model-${model.name}`,
        group: "Models",
        label: model.name,
        hint: model.name === selectedModel ? "Active" : (model.parameter_size ?? undefined),
        keywords: model.family ?? "",
        run: () => {
          selectModel(model.name);
          close();
        },
      });
    }

    for (const session of sessions) {
      list.push({
        id: `session-${session.id}`,
        group: "Sessions",
        label: session.title || "Untitled session",
        hint: session.mode === "research" ? "Research" : "Chat",
        keywords: "restore load open",
        run: () => {
          loadSession(session.messages);
          setMode(session.mode);
          close();
        },
      });
    }

    return list;
  }, [
    loadSession,
    mode,
    models,
    newChat,
    openDrawer,
    openExport,
    refreshModels,
    refreshSearch,
    selectedModel,
    selectModel,
    sessions,
    setDepth,
    setMode,
  ]);

  const results = useMemo(() => {
    return commands
      .map((command) => ({
        command,
        rank: score(query, `${command.label} ${command.keywords ?? ""}`),
      }))
      .filter((entry) => entry.rank > 0)
      .sort((a, b) => b.rank - a.rank)
      .slice(0, 40)
      .map((entry) => entry.command);
  }, [commands, query]);

  useEffect(() => {
    setActive(0);
  }, [query]);

  useEffect(() => {
    const node = listRef.current?.querySelector<HTMLElement>(`[data-index="${active}"]`);
    node?.scrollIntoView({ block: "nearest" });
  }, [active]);

  if (!open) return null;

  const onKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActive((value) => (results.length ? (value + 1) % results.length : 0));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive((value) => (results.length ? (value - 1 + results.length) % results.length : 0));
    } else if (event.key === "Enter") {
      event.preventDefault();
      results[active]?.run();
    }
  };

  const groupIcon = (group: Command["group"]) => {
    if (group === "Models") return <SendIcon className="h-3.5 w-3.5" />;
    if (group === "Sessions") return <HistoryIcon className="h-3.5 w-3.5" />;
    return <GlobeIcon className="h-3.5 w-3.5" />;
  };

  return (
    <div
      className="motion-fade-in fixed inset-0 z-[60] flex items-start justify-center bg-[#02060f]/70 p-4 pt-[12vh] backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <div className="motion-scale-in flex max-h-[60vh] w-full max-w-xl flex-col overflow-hidden rounded-xl border border-white/10 bg-[#0a1120] shadow-[0_30px_80px_rgba(0,0,0,0.6)]">
        <div className="flex items-center gap-2 border-b border-white/[0.06] px-3.5">
          <ChatIcon className="h-4 w-4 shrink-0 text-slate-500" />
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Search actions, models and sessions…"
            className="w-full bg-transparent py-3 text-[13px] text-slate-100 placeholder:text-slate-600 focus:outline-none"
            aria-label="Command search"
          />
          <kbd className="shrink-0 rounded border border-white/10 px-1.5 py-0.5 font-mono text-[9px] text-slate-500">
            esc
          </kbd>
        </div>

        <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto p-1.5">
          {results.length === 0 ? (
            <p className="px-3 py-6 text-center text-xs text-slate-500">No matching commands.</p>
          ) : (
            results.map((command, index) => (
              <button
                key={command.id}
                data-index={index}
                type="button"
                onMouseEnter={() => setActive(index)}
                onClick={command.run}
                className={`flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left ${
                  index === active ? "bg-sky-400/10 text-slate-100" : "text-slate-300"
                }`}
              >
                <span className={index === active ? "text-sky-300" : "text-slate-500"}>
                  {groupIcon(command.group)}
                </span>
                <span className="min-w-0 flex-1 truncate text-[12.5px]">{command.label}</span>
                {command.hint ? (
                  <span className="shrink-0 text-[10px] text-slate-500">{command.hint}</span>
                ) : null}
                <span className="shrink-0 text-[9px] uppercase tracking-wider text-slate-600">
                  {command.group}
                </span>
              </button>
            ))
          )}
        </div>

        <div className="flex shrink-0 items-center gap-3 border-t border-white/[0.06] px-3.5 py-2 text-[10px] text-slate-600">
          <span>↑↓ navigate</span>
          <span>↵ run</span>
          <span className="ml-auto flex items-center gap-1">
            <RefreshIcon className="h-3 w-3" /> Ctrl K
          </span>
        </div>
      </div>
    </div>
  );
}
