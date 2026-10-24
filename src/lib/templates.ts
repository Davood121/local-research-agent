/**
 * Research Intelligence Templates
 *
 * Professional research frameworks engineered to produce publication-grade
 * outputs with dense structure, objective evaluation, and cited sources.
 */

import type { ResearchDepth, ResearchFocus } from "../stores/researchStore";

export type ResearchTemplate = {
  id: string;
  label: string;
  icon: string;
  badge: string;
  description: string;
  recommendedDepth: ResearchDepth;
  recommendedFocus: ResearchFocus;
  directive: string;
  placeholder: string;
};

export const RESEARCH_TEMPLATES: ResearchTemplate[] = [
  {
    id: "academic-review",
    label: "Academic Review",
    icon: "🎓",
    badge: "Formal",
    description: "Literature review with methodology, evidence breakdown, and open debates.",
    recommendedDepth: "deep",
    recommendedFocus: "academic",
    directive:
      "Conduct a formal Academic Literature Review. Structure with: 1. Executive Abstract, 2. Theoretical Foundations & Methodology, 3. Empirical Evidence & Key Findings, 4. Critical Counter-Arguments & Debates, 5. Methodological Limitations & Future Research Vectors.",
    placeholder: "e.g., Quantum error correction breakthroughs in superconducting qubits",
  },
  {
    id: "competitive-matrix",
    label: "Competitive Matrix",
    icon: "🏢",
    badge: "Market",
    description: "Multi-column feature comparison, pricing, moats, and vendor trade-offs.",
    recommendedDepth: "deep",
    recommendedFocus: "general",
    directive:
      "Perform an exhaustive Competitive Landscape & Feature Teardown. Structure with: 1. Market Overview & Problem Space, 2. Comprehensive Multi-Column Feature Matrix (Markdown Table), 3. Pricing & Licensing Models, 4. Core Advantages & Moats, 5. Weaknesses & User Complaints, 6. Strategic Verdict.",
    placeholder: "e.g., Supabase vs Neon vs PlanetScale architecture and pricing",
  },
  {
    id: "fact-check",
    label: "Fact-Check",
    icon: "⚖️",
    badge: "Verify",
    description: "Cross-references claims against primary sources and rates confidence.",
    recommendedDepth: "balanced",
    recommendedFocus: "news",
    directive:
      "Perform a rigorous Fact-Checking & Claim Verification analysis. Structure with: 1. Target Claim & Context, 2. Verified Facts & Primary Sources, 3. Contradictions or Misleading Claims, 4. Source Bias & Credibility Assessment, 5. Factuality Confidence Rating (0-100%) with justification.",
    placeholder: "e.g., Did NASA discover evidence of fossilized Martian microbial life?",
  },
  {
    id: "executive-brief",
    label: "Executive Brief",
    icon: "📈",
    badge: "Actionable",
    description: "High-impact summary for decision makers with risks and next steps.",
    recommendedDepth: "quick",
    recommendedFocus: "general",
    directive:
      "Synthesize an Executive Decision Brief for leadership. Structure with: 1. Bottom Line Up Front (BLUF), 2. Strategic Context & Market Drivers, 3. Key Findings (3-5 high-density bullet points), 4. Risk & Exposure Analysis, 5. Recommended Decision Matrix, 6. Immediate Action Items.",
    placeholder: "e.g., Implications of EU AI Act compliance for local AI desktop apps",
  },
  {
    id: "technical-teardown",
    label: "Tech Architecture",
    icon: "🔬",
    badge: "Deep",
    description: "System architecture, throughput bottlenecks, latency, and code tradeoffs.",
    recommendedDepth: "deep",
    recommendedFocus: "academic",
    directive:
      "Generate an in-depth Technical Architecture & Engineering Teardown. Structure with: 1. Architectural Blueprint & Component Flow, 2. Concurrency & Memory Model, 3. Performance Benchmarks (Throughput vs Latency), 4. Security & Attack Surface, 5. Engineering Trade-offs & Recommendations.",
    placeholder: "e.g., Vulkan vs Metal compute pipeline for GGUF quantization inference",
  },
];
