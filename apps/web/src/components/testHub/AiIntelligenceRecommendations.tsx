import React from "react";
import { Sparkles, ArrowRight, ShieldAlert, Zap, AlertTriangle, Database } from "lucide-react";
import { TestRecommendation, TestType } from "../../types";

interface AiIntelligenceRecommendationsProps {
  recommendations: TestRecommendation[];
  onGenerateForRecommendation: (rec: TestRecommendation) => void;
}

export const AiIntelligenceRecommendations: React.FC<AiIntelligenceRecommendationsProps> = ({
  recommendations,
  onGenerateForRecommendation
}) => {
  if (!recommendations || recommendations.length === 0) return null;

  const getIcon = (type: TestType) => {
    switch (type) {
      case "PERFORMANCE":
        return <Zap className="w-4 h-4 text-amber-400" />;
      case "SECURITY":
        return <ShieldAlert className="w-4 h-4 text-rose-400" />;
      case "DATABASE":
        return <Database className="w-4 h-4 text-emerald-400" />;
      case "EDGE_CASE":
      default:
        return <AlertTriangle className="w-4 h-4 text-fuchsia-400" />;
    }
  };

  return (
    <div className="bg-gradient-to-r from-cyan-950/30 via-slate-900/60 to-indigo-950/30 border border-cyan-800/60 rounded-xl p-3.5 shadow-sm font-mono space-y-2">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <div className="p-1.5 rounded-lg bg-cyan-900/60 border border-cyan-600 text-cyan-300">
            <Sparkles className="w-3.5 h-3.5" />
          </div>
          <div>
            <h3 className="text-xs font-bold text-slate-100 flex items-center gap-2">
              <span>AI Test Gap Intelligence</span>
              <span className="text-[10px] px-1.5 py-0.2 rounded bg-cyan-950 text-cyan-300 border border-cyan-700">
                {recommendations.length} Recommendations
              </span>
            </h3>
            <p className="text-[10px] text-slate-400">
              Autonomous analyzer detected untested attack surfaces, missing performance benchmarks, and boundary gaps
            </p>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-2 pt-1">
        {recommendations.map((rec) => (
          <div
            key={rec.id}
            className="p-2.5 rounded-lg bg-[#080d16]/90 border border-slate-800 flex items-start justify-between gap-3 text-xs"
          >
            <div className="flex items-start gap-2 min-w-0">
              <div className="mt-0.5 shrink-0">{getIcon(rec.test_type)}</div>
              <div className="min-w-0 space-y-0.5">
                <div className="flex items-center gap-1.5 flex-wrap">
                  <span className="font-bold text-slate-200 truncate">{rec.targetFeature}</span>
                  <span className="text-[9px] uppercase px-1.5 py-0.2 rounded bg-slate-800 text-cyan-300 border border-slate-700">
                    {rec.test_type.replace("_", " ")}
                  </span>
                  <span
                    className={`text-[9px] px-1.5 py-0.2 rounded font-semibold ${
                      rec.priority === "High"
                        ? "bg-rose-950 text-rose-300 border border-rose-800"
                        : "bg-amber-950 text-amber-300 border border-amber-800"
                    }`}
                  >
                    {rec.priority} Priority
                  </span>
                </div>
                <p className="text-[11px] text-slate-400 leading-relaxed line-clamp-2">{rec.reason}</p>
              </div>
            </div>

            <button
              onClick={() => onGenerateForRecommendation(rec)}
              className="shrink-0 flex items-center gap-1 px-2.5 py-1.5 rounded-lg bg-cyan-600 hover:bg-cyan-500 text-white text-[11px] font-bold shadow-sm transition-all whitespace-nowrap"
            >
              <span>Generate ({rec.suggestedCount})</span>
              <ArrowRight className="w-3 h-3" />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
};
