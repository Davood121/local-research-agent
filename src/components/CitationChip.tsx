import { useEffect, useRef, useState } from "react";

import type { ResearchSource } from "../lib/events";
import { CheckIcon, CopyIcon, GlobeIcon } from "./Icons";

const CARD_WIDTH = 304;
const CARD_MARGIN = 8;

type Coords = { left: number; top: number; above: boolean };

/**
 * Inline citation marker (`[3]`) with a hover/focus preview card.
 *
 * The card is fixed-positioned and clamped to the viewport so it stays legible
 * inside the scrolling transcript. Clicking pins it open, which gives keyboard
 * and touch users the same preview without needing a hover state.
 */
export function CitationChip({ id, source }: { id: number; source: ResearchSource }) {
  const [open, setOpen] = useState(false);
  const [pinned, setPinned] = useState(false);
  const [coords, setCoords] = useState<Coords | null>(null);
  const [copied, setCopied] = useState<"citation" | "url" | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const closeTimer = useRef<number | null>(null);

  const measure = () => {
    const rect = buttonRef.current?.getBoundingClientRect();
    if (!rect) return;
    const above = rect.top > 280;
    const left = Math.min(
      Math.max(CARD_MARGIN, rect.left - 12),
      Math.max(CARD_MARGIN, window.innerWidth - CARD_WIDTH - CARD_MARGIN),
    );
    setCoords({ left, top: above ? rect.top - CARD_MARGIN : rect.bottom + CARD_MARGIN, above });
  };

  const show = () => {
    if (closeTimer.current) window.clearTimeout(closeTimer.current);
    measure();
    setOpen(true);
  };

  const hide = () => {
    if (pinned) return;
    if (closeTimer.current) window.clearTimeout(closeTimer.current);
    closeTimer.current = window.setTimeout(() => setOpen(false), 140);
  };

  useEffect(() => {
    return () => {
      if (closeTimer.current) window.clearTimeout(closeTimer.current);
    };
  }, []);

  useEffect(() => {
    if (!pinned) return;
    const onDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (!target) return;
      if (cardRef.current?.contains(target) || buttonRef.current?.contains(target)) return;
      setPinned(false);
      setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setPinned(false);
      setOpen(false);
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [pinned]);

  const copy = async (kind: "citation" | "url") => {
    if (!navigator.clipboard) return;
    const text =
      kind === "url"
        ? source.url
        : `[${id}] ${source.title} — ${source.domain || "web"}. ${source.url}`;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(kind);
      window.setTimeout(() => setCopied(null), 1_600);
    } catch {
      // Clipboard access can be denied; the card text stays selectable.
    }
  };

  const focusInPanel = () => {
    window.dispatchEvent(new CustomEvent("lra:focus-source", { detail: { id, url: source.url } }));
    setPinned(false);
    setOpen(false);
  };

  const visible = open || pinned;

  return (
    <span className="inline-block align-baseline">
      <button
        ref={buttonRef}
        type="button"
        aria-label={`Citation ${id}: ${source.title}`}
        aria-expanded={visible}
        onMouseEnter={show}
        onMouseLeave={hide}
        onFocus={show}
        onBlur={hide}
        onClick={() => {
          if (pinned) {
            setPinned(false);
            setOpen(false);
            return;
          }
          setPinned(true);
          show();
          window.dispatchEvent(
            new CustomEvent("lra:focus-source", { detail: { id, url: source.url } }),
          );
        }}
        className={`press ml-0.5 inline-flex -translate-y-0.5 items-center rounded border px-1 py-[1px] font-mono text-[10px] leading-none ${
          pinned
            ? "border-sky-400/40 bg-sky-400/20 text-sky-100"
            : "border-sky-400/20 bg-sky-400/[0.08] text-sky-300/90 hover:border-sky-400/40 hover:bg-sky-400/[0.16] hover:text-sky-200"
        }`}
      >
        {id}
      </button>

      {visible && coords ? (
        <div
          ref={cardRef}
          role="dialog"
          aria-label={`Source ${id} preview`}
          onMouseEnter={show}
          onMouseLeave={hide}
          style={{
            position: "fixed",
            left: coords.left,
            top: coords.top,
            width: CARD_WIDTH,
            transform: coords.above ? "translateY(-100%)" : undefined,
          }}
          className="motion-scale-in z-[70] rounded-xl border border-white/10 bg-[#0a1120] p-3 text-left shadow-[0_18px_50px_rgba(0,0,0,0.65)]"
        >
          <div className="flex items-center gap-1.5 text-[9px] uppercase tracking-[0.12em] text-slate-500">
            <span className="font-mono text-emerald-300/80">[{id}]</span>
            <span className="min-w-0 flex-1 truncate normal-case tracking-normal text-slate-400">
              {source.domain || "source"}
            </span>
            <span className={source.read ? "text-emerald-400/60" : "text-amber-400/60"}>
              {source.read ? "page read" : "snippet"}
            </span>
          </div>

          <p className="mt-1.5 text-[12px] font-medium leading-snug text-slate-100">
            {source.title}
          </p>

          {source.snippet ? (
            <p className="mt-1.5 line-clamp-4 text-[10.5px] leading-relaxed text-slate-400">
              {source.snippet}
            </p>
          ) : null}

          <p className="mt-1.5 break-all font-mono text-[9.5px] leading-snug text-slate-600">
            {source.url}
          </p>

          <div className="mt-2.5 flex items-center gap-1.5">
            <button
              type="button"
              onClick={() => void copy("citation")}
              className="press inline-flex items-center gap-1 rounded-md border border-white/10 bg-white/[0.05] px-2 py-1 text-[10px] font-medium text-slate-300 hover:border-white/20 hover:text-slate-100"
            >
              {copied === "citation" ? (
                <CheckIcon className="h-2.5 w-2.5 text-emerald-400" />
              ) : (
                <CopyIcon className="h-2.5 w-2.5" />
              )}
              {copied === "citation" ? "Copied" : "Copy citation"}
            </button>
            <button
              type="button"
              onClick={() => void copy("url")}
              className="press inline-flex items-center gap-1 rounded-md border border-white/10 bg-white/[0.05] px-2 py-1 text-[10px] font-medium text-slate-300 hover:border-white/20 hover:text-slate-100"
            >
              {copied === "url" ? (
                <CheckIcon className="h-2.5 w-2.5 text-emerald-400" />
              ) : (
                <CopyIcon className="h-2.5 w-2.5" />
              )}
              {copied === "url" ? "Copied" : "Copy URL"}
            </button>
            <button
              type="button"
              onClick={focusInPanel}
              title="Highlight this source in the sources panel"
              className="press ml-auto inline-flex items-center gap-1 rounded-md border border-sky-400/25 bg-sky-400/[0.1] px-2 py-1 text-[10px] font-medium text-sky-200 hover:border-sky-400/45 hover:bg-sky-400/[0.18]"
            >
              <GlobeIcon className="h-2.5 w-2.5" />
              Show
            </button>
          </div>
        </div>
      ) : null}
    </span>
  );
}
