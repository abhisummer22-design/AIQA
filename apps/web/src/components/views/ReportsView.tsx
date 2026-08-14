import React from "react";
import { FileText, Download, CheckCircle2, TrendingUp, ShieldCheck, Share2 } from "lucide-react";

export const ReportsView: React.FC = () => {
  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold font-mono text-white flex items-center gap-2.5">
            <FileText className="w-5 h-5 text-cyan-400" />
            Executive Quality &amp; SLA Compliance Reports
          </h1>
          <p className="text-xs text-slate-400 font-mono mt-1">
            Automated weekly summaries, release audit certifications, and flakiness burn-down reports
          </p>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        {[
          { title: "Weekly Enterprise QA Intelligence Brief", date: "Generated Today", tag: "PDF / Markdown", size: "2.4 MB" },
          { title: "SOC2 & ISO 27001 Test Evidence Certification", date: "Yesterday", tag: "Audit Signed", size: "8.1 MB" },
          { title: "Q3 AI Model Hallucination & Safety Summary", date: "3 days ago", tag: "Executive Brief", size: "1.2 MB" }
        ].map((r, i) => (
          <div key={i} className="p-5 rounded-xl bg-[#0e1422] border border-slate-800/90 space-y-4 shadow-sm flex flex-col justify-between">
            <div>
              <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-cyan-950 text-cyan-300 border border-cyan-800 font-bold uppercase">
                {r.tag}
              </span>
              <h3 className="font-bold text-sm text-slate-100 mt-2">{r.title}</h3>
              <p className="text-xs font-mono text-slate-500 mt-1">{r.date} &bull; {r.size}</p>
            </div>

            <button className="w-full py-2 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs font-mono text-cyan-300 font-semibold flex items-center justify-center gap-2 transition-colors">
              <Download className="w-3.5 h-3.5" />
              <span>Export Artifact</span>
            </button>
          </div>
        ))}
      </div>
    </div>
  );
};
