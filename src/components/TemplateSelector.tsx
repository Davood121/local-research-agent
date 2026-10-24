import { RESEARCH_TEMPLATES, type ResearchTemplate } from "../lib/templates";
import { useResearchStore } from "../stores/researchStore";

type TemplateSelectorProps = {
  activeTemplateId: string | null;
  onSelectTemplate: (template: ResearchTemplate | null) => void;
  disabled?: boolean;
};

export function TemplateSelector({
  activeTemplateId,
  onSelectTemplate,
  disabled,
}: TemplateSelectorProps) {
  const setDepth = useResearchStore((state) => state.setDepth);
  const setFocus = useResearchStore((state) => state.setFocus);

  const activeTemplate = RESEARCH_TEMPLATES.find((t) => t.id === activeTemplateId) ?? null;

  const handleSelect = (template: ResearchTemplate) => {
    if (activeTemplateId === template.id) {
      onSelectTemplate(null);
    } else {
      setDepth(template.recommendedDepth);
      setFocus(template.recommendedFocus);
      onSelectTemplate(template);
    }
  };

  return (
    <div className="relative mb-2">
      {/* Quick Pills Row */}
      <div className="flex flex-wrap items-center gap-1.5 px-0.5">
        <span className="text-[10px] font-semibold uppercase tracking-wider text-slate-500">
          Intelligence Templates:
        </span>

        {RESEARCH_TEMPLATES.map((template) => {
          const isSelected = activeTemplateId === template.id;
          return (
            <button
              key={template.id}
              type="button"
              disabled={disabled}
              onClick={() => handleSelect(template)}
              title={`${template.label}: ${template.description}`}
              className={`group inline-flex items-center gap-1.5 rounded-lg border px-2 py-0.5 text-[10.5px] font-medium transition ${
                isSelected
                  ? "border-emerald-400/40 bg-emerald-400/15 text-emerald-200 shadow-[0_0_12px_rgba(52,211,153,0.15)]"
                  : "border-white/[0.08] bg-black/20 text-slate-400 hover:border-white/20 hover:bg-white/[0.04] hover:text-slate-200"
              } disabled:cursor-not-allowed disabled:opacity-40`}
            >
              <span>{template.icon}</span>
              <span>{template.label}</span>
              {isSelected ? (
                <span
                  role="button"
                  tabIndex={0}
                  onClick={(e) => {
                    e.stopPropagation();
                    onSelectTemplate(null);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.stopPropagation();
                      onSelectTemplate(null);
                    }
                  }}
                  className="ml-0.5 inline-flex h-3 w-3 items-center justify-center rounded-full bg-emerald-400/30 text-[9px] hover:bg-emerald-400/50"
                  title="Remove template"
                >
                  ✕
                </span>
              ) : null}
            </button>
          );
        })}
      </div>

      {/* Active Template Notice Banner */}
      {activeTemplate ? (
        <div className="mt-1.5 flex items-center justify-between rounded-lg border border-emerald-400/20 bg-emerald-400/[0.06] px-2.5 py-1 text-[10px] text-emerald-200/90">
          <div className="flex items-center gap-1.5">
            <span className="font-semibold text-emerald-300">
              {activeTemplate.icon} {activeTemplate.label} Active:
            </span>
            <span className="text-emerald-100/70">{activeTemplate.description}</span>
          </div>
          <span className="font-mono text-[9.5px] text-emerald-300/80">
            Depth: {activeTemplate.recommendedDepth.toUpperCase()} · Focus:{" "}
            {activeTemplate.recommendedFocus.toUpperCase()}
          </span>
        </div>
      ) : null}
    </div>
  );
}
