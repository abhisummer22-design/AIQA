import React, { useState } from "react";
import { X, Sparkles, Bug, CheckCircle2, ShieldAlert, ArrowRight, RefreshCw } from "lucide-react";
import { BugCandidate } from "../../types";

interface CreateBugModalProps {
  isOpen: boolean;
  onClose: () => void;
  candidate: BugCandidate | null;
  onConfirmCreate: (data: {
    candidateId: string;
    title: string;
    severity: string;
    priority: string;
    notes?: string;
  }) => void;
}

export const CreateBugModal: React.FC<CreateBugModalProps> = ({
  isOpen,
  onClose,
  candidate,
  onConfirmCreate
}) => {
  const [title, setTitle] = useState(candidate?.title || "");
  const [severity, setSeverity] = useState(candidate?.severity || "High");
  const [priority, setPriority] = useState(candidate?.suggestedPriority || "P1 - Blocker");
  const [notes, setNotes] = useState("");
  const [syncJira, setSyncJira] = useState(true);

  React.useEffect(() => {
    if (candidate) {
      setTitle(candidate.title);
      setSeverity(candidate.severity);
      setPriority(candidate.suggestedPriority);
    }
  }, [candidate]);

  if (!isOpen || !candidate) return null;

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onConfirmCreate({
      candidateId: candidate.id,
      title,
      severity,
      priority,
      notes
    });
    onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm">
      <div className="bg-[#0D0D0E] border border-slate-800 rounded w-full max-w-xl overflow-hidden shadow-2xl flex flex-col">
        {/* Header */}
        <div className="px-4 py-3 border-b border-slate-800 flex items-center justify-between bg-slate-900/40">
          <div className="flex items-center gap-2">
            <Sparkles className="w-4 h-4 text-indigo-400" />
            <div>
              <h3 className="text-xs font-bold font-mono uppercase text-white tracking-wider">Confirm AI-Drafted Bug</h3>
              <p className="text-[10px] text-slate-400 font-mono">AI proposes candidate &bull; Engineer confirmation</p>
            </div>
          </div>
          <button onClick={onClose} aria-label="Close" className="p-1 rounded text-slate-400 hover:text-white">
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Form Body */}
        <form onSubmit={handleSubmit} className="p-4 space-y-3 text-xs font-mono">
          <div>
            <label className="text-slate-400 uppercase text-[9px] font-bold block mb-1">
              Bug Title
            </label>
            <input
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              className="w-full p-2 rounded bg-[#070708] border border-slate-800 text-slate-100 text-xs font-mono focus:outline-none focus:border-indigo-500"
              required
            />
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="text-slate-400 uppercase text-[9px] font-bold block mb-1">
                Severity
              </label>
              <select
                value={severity}
                onChange={(e) => setSeverity(e.target.value as any)}
                className="w-full p-2 rounded bg-[#070708] border border-slate-800 text-slate-100 text-xs font-mono focus:outline-none focus:border-indigo-500"
              >
                <option value="Critical">Critical (P0 / Outage)</option>
                <option value="High">High (P1 / Major Failure)</option>
                <option value="Medium">Medium (P2 / Functional Bug)</option>
                <option value="Low">Low (P3 / Minor)</option>
              </select>
            </div>

            <div>
              <label className="text-slate-400 uppercase text-[9px] font-bold block mb-1">
                Suggested Priority
              </label>
              <select
                value={priority}
                onChange={(e) => setPriority(e.target.value as any)}
                className="w-full p-2 rounded bg-[#070708] border border-slate-800 text-slate-100 text-xs font-mono focus:outline-none focus:border-indigo-500"
              >
                <option value="P0 - Urgent">P0 - Urgent (Stop the line)</option>
                <option value="P1 - Blocker">P1 - Blocker (Release blocker)</option>
                <option value="P2 - Normal">P2 - Normal</option>
                <option value="P3 - Minor">P3 - Minor</option>
              </select>
            </div>
          </div>

          <div className="p-2.5 rounded bg-[#070708] border border-slate-800 space-y-1 text-slate-300">
            <div className="text-[9px] text-indigo-400 uppercase font-bold">AI Diagnostics & Description</div>
            <p className="text-[11px] font-sans leading-relaxed text-slate-300">{candidate.description}</p>
            <div className="text-[10px] text-slate-500 pt-0.5 flex items-center gap-2.5 font-mono">
              <span>Related Test: {candidate.relatedTest}</span>
              <span>&bull;</span>
              <span>Confidence: {candidate.confidence}%</span>
            </div>
          </div>

          <div>
            <label className="text-slate-400 uppercase text-[9px] font-bold block mb-1">
              Engineer Notes / Remediation Target
            </label>
            <textarea
              rows={2}
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              placeholder="Add optional reproduction notes, JIRA project tag, or squad assignment..."
              className="w-full p-2 rounded bg-[#070708] border border-slate-800 text-slate-100 text-xs font-mono focus:outline-none focus:border-indigo-500"
            />
          </div>

          <label className="flex items-center gap-2 text-slate-300 cursor-pointer pt-0.5 font-mono">
            <input
              type="checkbox"
              checked={syncJira}
              onChange={(e) => setSyncJira(e.target.checked)}
              className="rounded bg-slate-900 border-slate-700 text-indigo-500"
            />
            <span className="text-[11px]">Sync to Jira / Issue Tracker</span>
          </label>

          {/* Buttons */}
          <div className="pt-2.5 border-t border-slate-800 flex items-center justify-end gap-2">
            <button
              type="button"
              onClick={onClose}
              className="px-3 py-1.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 font-mono text-[11px]"
            >
              Cancel
            </button>
            <button
              type="submit"
              className="px-3.5 py-1.5 rounded bg-emerald-600 hover:bg-emerald-500 text-white font-mono text-[11px] font-bold flex items-center gap-1.5 shadow-md shadow-emerald-950"
            >
              <CheckCircle2 className="w-3.5 h-3.5" />
              <span>Confirm &amp; Create Bug</span>
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};

