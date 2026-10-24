import { useMemo, useState } from "react";
import type { ResearchSource } from "../lib/events";
import { useChatStore } from "../stores/chatStore";
import { GlobeIcon } from "./Icons";

type GraphNodeType = "query" | "source" | "insight";

type GraphNode = {
  id: string;
  type: GraphNodeType;
  title: string;
  subtitle?: string;
  excerpt?: string;
  url?: string;
  domain?: string;
  read?: boolean;
  engine?: string;
  x: number;
  y: number;
};

type GraphEdge = {
  id: string;
  from: string;
  to: string;
  label?: string;
};

export function KnowledgeGraphView() {
  const messages = useChatStore((state) => state.messages);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });

  // Extract research queries, sources, and key cited points from messages
  const { nodes, edges } = useMemo(() => {
    const calculatedNodes: GraphNode[] = [];
    const calculatedEdges: GraphEdge[] = [];

    // Find all research turns
    const userMessages = messages.filter((m) => m.role === "user");
    const assistantMessages = messages.filter((m) => m.role === "assistant");

    if (userMessages.length === 0) {
      return { nodes: [], edges: [] };
    }

    const latestUser = userMessages[userMessages.length - 1];
    const latestAssistant = assistantMessages[assistantMessages.length - 1];

    // 1. Root Query Node (Center Left)
    const rootId = "root-query";
    calculatedNodes.push({
      id: rootId,
      type: "query",
      title: latestUser.content.slice(0, 80) + (latestUser.content.length > 80 ? "…" : ""),
      subtitle: "Research Objective",
      x: 100,
      y: 280,
    });

    // 2. Discovered Sources (Middle Column)
    const sources: ResearchSource[] = latestAssistant?.sources ?? [];
    const sourceCount = sources.length;
    const startY = Math.max(60, 280 - (sourceCount * 70) / 2);

    sources.forEach((src, idx) => {
      const sourceId = `source-${src.id}`;
      const y = startY + idx * 75;
      calculatedNodes.push({
        id: sourceId,
        type: "source",
        title: src.title.slice(0, 45) + (src.title.length > 45 ? "…" : ""),
        subtitle: src.domain || "Web Source",
        excerpt: src.snippet,
        url: src.url,
        domain: src.domain,
        read: src.read,
        engine: src.engine,
        x: 420,
        y,
      });

      calculatedEdges.push({
        id: `edge-${rootId}-${sourceId}`,
        from: rootId,
        to: sourceId,
        label: src.read ? "verified" : "candidate",
      });
    });

    // 3. Extracted Insights / Cited Claims (Right Column)
    if (latestAssistant && latestAssistant.content) {
      // Split assistant content into bullet points or paragraphs with citations
      const paragraphs = latestAssistant.content
        .split(/\n\n+/)
        .map((p) => p.trim())
        .filter((p) => p.length > 20 && !p.startsWith("#") && !p.startsWith("Sources:"));

      const topInsights = paragraphs.slice(0, Math.min(5, Math.max(2, sourceCount)));
      const insightStartY = Math.max(80, 280 - (topInsights.length * 90) / 2);

      topInsights.forEach((para, idx) => {
        const insightId = `insight-${idx + 1}`;
        const y = insightStartY + idx * 105;

        // Check which sources were cited (e.g. [1], [2])
        const citationMatches = para.match(/\[(\d+)\]/g) || [];
        const citedIds = citationMatches
          .map((c) => parseInt(c.replace(/[[\]]/g, ""), 10))
          .filter((n) => !isNaN(n));

        calculatedNodes.push({
          id: insightId,
          type: "insight",
          title: `Key Finding #${idx + 1}`,
          subtitle:
            citationMatches.length > 0 ? `Cited by ${citationMatches.join(" ")}` : "Synthesis",
          excerpt: para.slice(0, 160) + (para.length > 160 ? "…" : ""),
          x: 780,
          y,
        });

        if (citedIds.length > 0) {
          citedIds.forEach((cId) => {
            const matchedSource = sources.find((s) => s.id === cId);
            if (matchedSource) {
              calculatedEdges.push({
                id: `edge-source-${cId}-${insightId}`,
                from: `source-${cId}`,
                to: insightId,
              });
            }
          });
        } else {
          // Default connection to root
          calculatedEdges.push({
            id: `edge-${rootId}-${insightId}`,
            from: rootId,
            to: insightId,
          });
        }
      });
    }

    return { nodes: calculatedNodes, edges: calculatedEdges };
  }, [messages]);

  const selectedNode = nodes.find((n) => n.id === selectedNodeId) ?? null;

  // Mouse pan handlers
  const handleMouseDown = (e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest(".graph-node-card")) return;
    setIsDragging(true);
    setDragStart({ x: e.clientX - pan.x, y: e.clientY - pan.y });
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    if (!isDragging) return;
    setPan({ x: e.clientX - dragStart.x, y: e.clientY - dragStart.y });
  };

  const handleMouseUp = () => setIsDragging(false);

  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const zoomFactor = e.deltaY < 0 ? 1.08 : 0.92;
    setZoom((prev) => Math.min(2.5, Math.max(0.4, prev * zoomFactor)));
  };

  if (nodes.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center p-8 text-center">
        <div className="relative mb-4 flex h-20 w-20 items-center justify-center rounded-2xl border border-sky-400/20 bg-sky-400/[0.05] shadow-[0_0_40px_rgba(14,165,233,0.15)]">
          <GlobeIcon className="h-10 w-10 text-sky-400/80" />
          <div className="absolute inset-0 animate-ping rounded-2xl border border-sky-400/30 opacity-20" />
        </div>
        <h3 className="text-base font-semibold text-slate-200">No Knowledge Graph Yet</h3>
        <p className="mt-2 max-w-sm text-xs leading-relaxed text-slate-400">
          Ask a question in <strong className="text-emerald-300">Research Mode</strong>. As the
          agent retrieves web sources and verifies evidence, an interactive visual mindmap will
          construct itself here.
        </p>
      </div>
    );
  }

  return (
    <div
      className="relative flex flex-1 overflow-hidden bg-[#040810] select-none"
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onWheel={handleWheel}
      style={{ cursor: isDragging ? "grabbing" : "grab" }}
    >
      {/* Background Grid Accent */}
      <div
        className="pointer-events-none absolute inset-0 opacity-[0.04]"
        style={{
          backgroundImage: `radial-gradient(circle, #38bdf8 1px, transparent 1px)`,
          backgroundSize: "28px 28px",
        }}
      />

      {/* SVG Canvas for Bezier Edge Connectors */}
      <svg
        className="absolute inset-0 h-full w-full pointer-events-none"
        style={{
          transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
          transformOrigin: "0 0",
        }}
      >
        <defs>
          <linearGradient id="edge-gradient" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="#10b981" stopOpacity="0.4" />
            <stop offset="100%" stopColor="#38bdf8" stopOpacity="0.5" />
          </linearGradient>
          <linearGradient id="edge-highlight" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="#34d399" stopOpacity="0.9" />
            <stop offset="100%" stopColor="#38bdf8" stopOpacity="1" />
          </linearGradient>
        </defs>

        {edges.map((edge) => {
          const fromNode = nodes.find((n) => n.id === edge.from);
          const toNode = nodes.find((n) => n.id === edge.to);
          if (!fromNode || !toNode) return null;

          const isConnected = selectedNodeId === edge.from || selectedNodeId === edge.to;

          const startX = fromNode.x + 140;
          const startY = fromNode.y + 25;
          const endX = toNode.x;
          const endY = toNode.y + 25;
          const deltaX = (endX - startX) * 0.5;

          const pathData = `M ${startX} ${startY} C ${startX + deltaX} ${startY}, ${
            endX - deltaX
          } ${endY}, ${endX} ${endY}`;

          return (
            <g key={edge.id}>
              <path
                d={pathData}
                fill="none"
                stroke={isConnected ? "url(#edge-highlight)" : "url(#edge-gradient)"}
                strokeWidth={isConnected ? 2.5 : 1.2}
                strokeDasharray={isConnected ? "none" : "4 3"}
                className="transition-all duration-300"
              />
              {/* Pulsing connection bead */}
              {isConnected ? (
                <circle r="3" fill="#38bdf8">
                  <animateMotion path={pathData} dur="3s" repeatCount="indefinite" />
                </circle>
              ) : null}
            </g>
          );
        })}
      </svg>

      {/* Interactive HTML Nodes Container */}
      <div
        className="absolute inset-0"
        style={{
          transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
          transformOrigin: "0 0",
        }}
      >
        {nodes.map((node) => {
          const isSelected = selectedNodeId === node.id;

          let badgeColor = "border-sky-400/30 bg-sky-400/10 text-sky-200";
          let glowColor = "hover:shadow-[0_0_20px_rgba(56,189,248,0.2)]";

          if (node.type === "query") {
            badgeColor = "border-emerald-400/40 bg-emerald-400/15 text-emerald-200";
            glowColor = "shadow-[0_0_25px_rgba(52,211,153,0.25)]";
          } else if (node.type === "insight") {
            badgeColor = "border-purple-400/30 bg-purple-400/10 text-purple-200";
            glowColor = "hover:shadow-[0_0_20px_rgba(192,132,252,0.2)]";
          }

          return (
            <div
              key={node.id}
              onClick={(e) => {
                e.stopPropagation();
                setSelectedNodeId(node.id === selectedNodeId ? null : node.id);
              }}
              className={`graph-node-card absolute w-64 rounded-xl border bg-[#0a1324]/95 p-3 backdrop-blur-md transition-all duration-200 cursor-pointer ${
                isSelected
                  ? "border-sky-400 ring-2 ring-sky-400/30 shadow-[0_0_30px_rgba(56,189,248,0.35)] scale-105 z-20"
                  : `border-white/[0.08] ${glowColor} hover:border-white/20 z-10`
              }`}
              style={{ left: `${node.x}px`, top: `${node.y}px` }}
            >
              <div className="flex items-center justify-between gap-2">
                <span
                  className={`inline-flex items-center rounded-md border px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider ${badgeColor}`}
                >
                  {node.type}
                </span>
                {node.read ? (
                  <span className="flex items-center gap-1 text-[9px] font-medium text-emerald-400">
                    <span className="h-1.5 w-1.5 rounded-full bg-emerald-400 animate-pulse" />
                    Read
                  </span>
                ) : null}
              </div>

              <h4 className="mt-1.5 line-clamp-2 text-xs font-semibold text-slate-100">
                {node.title}
              </h4>

              {node.subtitle ? (
                <p className="mt-0.5 truncate text-[10px] text-slate-400">{node.subtitle}</p>
              ) : null}
            </div>
          );
        })}
      </div>

      {/* Floating View Controls (Bottom Left) */}
      <div className="absolute bottom-4 left-4 z-30 flex items-center gap-1.5 rounded-xl border border-white/[0.08] bg-[#091120]/90 p-1 backdrop-blur-md shadow-xl">
        <button
          type="button"
          onClick={() => setZoom((z) => Math.min(2.5, z + 0.15))}
          className="rounded-lg p-1.5 text-xs text-slate-300 hover:bg-white/[0.06] hover:text-white"
          title="Zoom In"
        >
          ＋
        </button>
        <button
          type="button"
          onClick={() => setZoom((z) => Math.max(0.4, z - 0.15))}
          className="rounded-lg p-1.5 text-xs text-slate-300 hover:bg-white/[0.06] hover:text-white"
          title="Zoom Out"
        >
          －
        </button>
        <button
          type="button"
          onClick={() => {
            setZoom(1);
            setPan({ x: 0, y: 0 });
          }}
          className="rounded-lg px-2 py-1 text-[10px] font-medium text-slate-300 hover:bg-white/[0.06] hover:text-white"
          title="Reset Zoom & Pan"
        >
          Fit
        </button>
      </div>

      {/* Selected Node Details Flyout Panel (Right Side) */}
      {selectedNode ? (
        <div className="absolute right-4 top-4 bottom-4 z-30 w-80 rounded-2xl border border-white/[0.1] bg-[#0a1324]/95 p-4 backdrop-blur-xl shadow-2xl flex flex-col justify-between">
          <div className="overflow-y-auto pr-1">
            <div className="flex items-center justify-between">
              <span className="rounded-md border border-sky-400/30 bg-sky-400/10 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-sky-200">
                {selectedNode.type} Node
              </span>
              <button
                type="button"
                onClick={() => setSelectedNodeId(null)}
                className="rounded-lg p-1 text-slate-400 hover:bg-white/10 hover:text-white text-xs"
              >
                ✕
              </button>
            </div>

            <h3 className="mt-3 text-sm font-semibold text-slate-100 leading-snug">
              {selectedNode.title}
            </h3>

            {selectedNode.domain ? (
              <p className="mt-1 font-mono text-[10.5px] text-sky-400/90">
                🌐 {selectedNode.domain}
              </p>
            ) : null}

            {selectedNode.excerpt ? (
              <div className="mt-3 rounded-lg border border-white/[0.06] bg-black/30 p-2.5">
                <span className="text-[10px] font-semibold uppercase tracking-wider text-slate-500">
                  Extracted Evidence:
                </span>
                <p className="mt-1 text-xs leading-relaxed text-slate-300">
                  {selectedNode.excerpt}
                </p>
              </div>
            ) : null}
          </div>

          {selectedNode.url ? (
            <a
              href={selectedNode.url}
              target="_blank"
              rel="noreferrer"
              className="mt-4 flex items-center justify-center gap-1.5 rounded-xl border border-sky-400/30 bg-sky-500/15 py-2 text-xs font-semibold text-sky-200 transition hover:bg-sky-500/25"
            >
              <span>Open Primary Source</span>
              <span>↗</span>
            </a>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
