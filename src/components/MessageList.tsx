import { useLayoutEffect, useRef, useState } from "react";
import Markdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import type { ChatEntry } from "../stores/chatStore";
import { useChatStore } from "../stores/chatStore";
import { BrandMark } from "./BrandMark";
import { CheckIcon, CopyIcon } from "./Icons";

const timeFormatter = new Intl.DateTimeFormat("en-US", {
  hour: "2-digit",
  minute: "2-digit",
});

// Links and images are deliberately rendered without an href/src. The model is
// untrusted text: opening a URL or fetching a remote image must stay a user
// action, never a side effect of rendering an answer.
const MARKDOWN_COMPONENTS: Components = {
  h1: ({ children }) => (
    <h1 className="mb-2 mt-4 text-[15px] font-semibold text-slate-100 first:mt-0">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="mb-2 mt-4 text-[14px] font-semibold text-slate-100 first:mt-0">{children}</h2>
  ),
  h3: ({ children }) => (
    <h3 className="mb-1.5 mt-3 text-[13.5px] font-semibold text-slate-100 first:mt-0">
      {children}
    </h3>
  ),
  h4: ({ children }) => (
    <h4 className="mb-1.5 mt-3 text-[13px] font-semibold text-slate-200 first:mt-0">{children}</h4>
  ),
  h5: ({ children }) => (
    <h5 className="mb-1 mt-2 text-[12.5px] font-semibold text-slate-200 first:mt-0">{children}</h5>
  ),
  h6: ({ children }) => (
    <h6 className="mb-1 mt-2 text-[12px] font-semibold text-slate-300 first:mt-0">{children}</h6>
  ),
  p: ({ children }) => <p className="my-2 break-words first:mt-0 last:mb-0">{children}</p>,
  ul: ({ children }) => (
    <ul className="my-2 list-disc space-y-1 pl-5 marker:text-slate-500">{children}</ul>
  ),
  ol: ({ children }) => (
    <ol className="my-2 list-decimal space-y-1 pl-5 marker:text-slate-500">{children}</ol>
  ),
  li: ({ children }) => <li className="pl-0.5 [&>p]:my-0">{children}</li>,
  strong: ({ children }) => <strong className="font-semibold text-slate-100">{children}</strong>,
  em: ({ children }) => <em className="italic text-slate-300">{children}</em>,
  del: ({ children }) => <del className="text-slate-500 line-through">{children}</del>,
  blockquote: ({ children }) => (
    <blockquote className="my-3 border-l-2 border-sky-400/30 pl-3 text-slate-400">
      {children}
    </blockquote>
  ),
  hr: () => <hr className="my-4 border-white/[0.07]" />,
  a: ({ children, href }) => (
    <span className="text-sky-300 underline decoration-sky-400/30 underline-offset-2" title={href}>
      {children}
    </span>
  ),
  img: ({ alt }) => <span className="text-slate-500">[image{alt ? `: ${alt}` : ""} omitted]</span>,
  pre: ({ children }) => (
    <pre className="my-3 overflow-x-auto rounded-lg border border-white/[0.08] bg-black/35 p-3 font-mono text-[11.5px] leading-relaxed text-slate-200">
      {children}
    </pre>
  ),
  code: ({ className, children }) => {
    const text = String(children).replace(/\n$/, "");
    if (className?.startsWith("language-") || text.includes("\n")) {
      return <code className="block font-mono">{text}</code>;
    }
    return (
      <code className="rounded border border-white/[0.06] bg-white/[0.06] px-1 py-0.5 font-mono text-[0.9em] text-sky-200">
        {children}
      </code>
    );
  },
  table: ({ children }) => (
    <div className="my-3 overflow-x-auto">
      <table className="w-full border-collapse text-[12px]">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border border-white/[0.08] bg-white/[0.04] px-2 py-1 text-left font-semibold text-slate-200">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border border-white/[0.06] px-2 py-1 align-top text-slate-300">{children}</td>
  ),
};

function MessageBody({ content, markdown }: { content: string; markdown: boolean }) {
  if (!markdown) {
    return <p className="whitespace-pre-wrap break-words">{content}</p>;
  }

  return (
    <div className="min-w-0">
      <Markdown remarkPlugins={[remarkGfm]} skipHtml components={MARKDOWN_COMPONENTS}>
        {content}
      </Markdown>
    </div>
  );
}

function sanitizeCitations(content: string, sourceCount: number): string {
  if (sourceCount < 1) return content;
  return content
    .replace(/\[(\s*\d+(?:\s*,\s*\d+)*\s*)\]/g, (_match, group: string) => {
      const numbers = group
        .split(",")
        .map((part) => Number(part.trim()))
        .filter((value) => Number.isInteger(value) && value >= 1 && value <= sourceCount);
      if (numbers.length === 0) return "";
      return `[${Array.from(new Set(numbers)).join(", ")}]`;
    })
    .replace(/\s+([.,;:!?])/g, "$1")
    .replace(/[ \t]{2,}/g, " ");
}

function Entry({ entry }: { entry: ChatEntry }) {
  const [copied, setCopied] = useState(false);
  const isAssistant = entry.role === "assistant";
  const sourceCount = entry.sources?.length ?? 0;
  const displayContent =
    isAssistant && entry.mode === "research"
      ? sanitizeCitations(entry.content, sourceCount)
      : entry.content;
  const isEmptyStreaming = isAssistant && entry.state === "streaming" && !displayContent;
  const isEmptyAnswer = isAssistant && entry.state !== "streaming" && !displayContent;
  const isError = entry.state === "error";

  const copy = async () => {
    if (!navigator.clipboard || !displayContent) return;
    try {
      await navigator.clipboard.writeText(displayContent);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1_600);
    } catch {
      // Clipboard access can be denied by the OS; the message stays selectable.
    }
  };

  return (
    <article
      className={`group flex gap-3 ${isAssistant ? "items-start" : "flex-row-reverse items-start"}`}
    >
      <div
        className={`mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border text-[10px] font-semibold tracking-wide ${
          isAssistant
            ? "border-sky-400/15 bg-sky-400/[0.08] text-sky-300"
            : "border-white/10 bg-white/[0.05] text-slate-400"
        }`}
      >
        {isAssistant ? <BrandMark /> : "YOU"}
      </div>

      <div
        className={`flex min-w-0 max-w-[88%] flex-col ${isAssistant ? "items-start" : "items-end"}`}
      >
        <div className="mb-1.5 flex items-center gap-2 px-0.5">
          <span className="text-[10px] font-semibold uppercase tracking-[0.14em] text-slate-500">
            {isAssistant ? (entry.mode === "research" ? "Local research" : "Local model") : "You"}
          </span>
          {isAssistant && entry.model ? (
            <span className="max-w-[180px] truncate rounded border border-white/[0.07] bg-white/[0.03] px-1.5 py-0.5 font-mono text-[9px] text-slate-500">
              {entry.model}
            </span>
          ) : null}
          <span className="text-[10px] text-slate-600">
            {timeFormatter.format(entry.createdAt)}
          </span>
          {entry.state === "stopped" ? (
            <span className="rounded-full border border-amber-400/20 bg-amber-400/[0.06] px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wider text-amber-300/90">
              stopped
            </span>
          ) : null}
          {isError ? (
            <span className="rounded-full border border-rose-400/20 bg-rose-400/[0.06] px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wider text-rose-300/90">
              interrupted
            </span>
          ) : null}
        </div>

        <div
          className={`relative rounded-2xl border px-4 py-3 text-[13.5px] leading-[1.75] shadow-[0_1px_0_rgba(255,255,255,0.03)_inset] ${
            isAssistant
              ? "rounded-tl-md border-white/[0.07] bg-white/[0.035] text-slate-200"
              : "rounded-tr-md border-sky-400/15 bg-sky-500/[0.10] text-slate-100"
          }`}
        >
          {isEmptyStreaming ? (
            <span className="flex items-center gap-2 py-0.5 text-xs text-slate-500">
              <span className="flex gap-1">
                <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-sky-400/80 [animation-delay:-0.2s]" />
                <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-sky-400/80 [animation-delay:-0.1s]" />
                <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-sky-400/80" />
              </span>
              {entry.mode === "research"
                ? entry.sources?.length
                  ? "Reading sources and writing…"
                  : "Searching and reading public pages…"
                : "Thinking locally…"}
            </span>
          ) : isEmptyAnswer ? (
            <span className="text-xs italic text-slate-500">
              {entry.state === "stopped"
                ? "Stopped before an answer was returned."
                : entry.state === "error"
                  ? "The run stopped before an answer was returned."
                  : "No answer was returned."}
            </span>
          ) : (
            <MessageBody content={displayContent} markdown={isAssistant} />
          )}
          {entry.state === "streaming" && displayContent ? (
            <span className="ml-0.5 inline-block h-3.5 w-1.5 animate-pulse rounded-[1px] bg-sky-300 align-text-bottom" />
          ) : null}
        </div>

        {isAssistant && displayContent && entry.state !== "streaming" ? (
          <button
            type="button"
            onClick={() => void copy()}
            className="mt-1 flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] text-slate-600 opacity-0 transition hover:bg-white/[0.04] hover:text-slate-300 focus:opacity-100 group-hover:opacity-100"
          >
            {copied ? <CheckIcon className="h-3 w-3" /> : <CopyIcon className="h-3 w-3" />}
            {copied ? "Copied" : "Copy"}
          </button>
        ) : null}
      </div>
    </article>
  );
}

export function MessageList() {
  const messages = useChatStore((state) => state.messages);
  const isStreaming = useChatStore((state) => state.isStreaming);
  const scrollRef = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);
  const lastMessageCount = useRef(messages.length);

  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    if (messages.length > lastMessageCount.current) {
      stickToBottom.current = true;
    }
    lastMessageCount.current = messages.length;
    if (stickToBottom.current) {
      element.scrollTop = element.scrollHeight;
    }
  }, [messages, isStreaming]);

  return (
    <div
      ref={scrollRef}
      onScroll={() => {
        const element = scrollRef.current;
        if (!element) return;
        stickToBottom.current =
          element.scrollHeight - element.scrollTop - element.clientHeight < 96;
      }}
      className="min-h-0 flex-1 overflow-y-auto"
    >
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-7 px-5 py-7 sm:px-8">
        {messages.map((entry) => (
          <Entry key={entry.id} entry={entry} />
        ))}
      </div>
    </div>
  );
}
