/**
 * Skill detail — shows what a skill actually instructs the generator to do.
 *
 * The panel row only had room for a skill's *name*, and the local
 * `SkillDefinition.description` is a one-line label, not the thing that runs.
 * The text that genuinely shapes generation lives in the backend
 * (`ai_generation.rs::resolve_skill_instructions`), so showing the local
 * description here and calling it "the skill" would be showing a summary and
 * implying it was the source.
 *
 * This fetches `POST /api/v1/skills/resolve` instead, which calls that exact
 * same resolver. What is displayed is therefore provably the string embedded
 * in the real prompt — it cannot drift from generation behaviour, because
 * there is only one copy of it and the backend owns it.
 *
 * The two real resolution paths are labelled honestly, because they are
 * genuinely different:
 *   - a **built-in** skill resolves to the specific instruction the backend
 *     defines for that name;
 *   - a **custom** skill resolves to its own stored description, verbatim —
 *     so what a user typed is literally what the model is told.
 */

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { AlertTriangle, FileText, Info, Loader2, X } from 'lucide-react';
import { aiqaApi } from '../../services/aiqaApi';
import type { SkillDefinition } from '../../types/skills';

interface SkillDetailModalProps {
  skill: SkillDefinition;
  onClose: () => void;
}

export const SkillDetailModal: React.FC<SkillDetailModalProps> = ({ skill, onClose }) => {
  const [instruction, setInstruction] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const isUnmounted = useRef(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const resolved = await aiqaApi.skills.resolve([
        { id: skill.id, name: skill.name, description: skill.description },
      ]);
      if (isUnmounted.current) return;
      const match = Array.isArray(resolved) ? resolved.find((r) => r.id === skill.id) : undefined;
      if (!match) {
        // The backend answered but not about this skill — say so rather than
        // falling back to the local description and passing it off as real.
        setInstruction(null);
        setError('The orchestrator did not return an instruction for this skill.');
        return;
      }
      setInstruction(match.instruction);
    } catch (err) {
      if (isUnmounted.current) return;
      setInstruction(null);
      setError(err instanceof Error ? err.message : 'Could not read this skill from the orchestrator.');
    } finally {
      if (!isUnmounted.current) setLoading(false);
    }
  }, [skill.id, skill.name, skill.description]);

  useEffect(() => {
    isUnmounted.current = false;
    void load();
    return () => {
      isUnmounted.current = true;
    };
  }, [load]);

  // Escape closes, matching the other modals in this feature.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const isCustom = skill.type === 'CUSTOM';

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-16 bg-slate-950/80 backdrop-blur-sm p-4 animate-in fade-in duration-100"
      onClick={onClose}
    >
      <div
        id="skill-detail-modal"
        role="dialog"
        aria-modal="true"
        aria-label={`Skill: ${skill.name}`}
        className="w-full max-w-lg rounded-lg bg-[#0B1120] border border-slate-700/80 shadow-2xl overflow-hidden flex flex-col max-h-[80vh]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-3 px-4 py-3 border-b border-slate-800 bg-[#020617]">
          <div className="min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <h2 className="text-sm font-bold text-slate-100 font-sans truncate">{skill.name}</h2>
              <span
                className={`px-1.5 py-0.5 rounded text-[9px] font-mono font-bold uppercase tracking-wide border ${
                  isCustom
                    ? 'bg-cyan-950/80 border-cyan-700 text-cyan-300'
                    : 'bg-indigo-950/80 border-indigo-700 text-indigo-300'
                }`}
              >
                {isCustom ? 'Custom' : 'Built-in'}
              </span>
            </div>
            <p className="text-[10px] font-mono text-slate-500 mt-0.5 truncate">{skill.id}</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close skill detail"
            className="p-1.5 text-slate-500 hover:text-slate-300 rounded cursor-pointer shrink-0"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="p-4 space-y-3 overflow-y-auto">
          <div className="flex items-center gap-1.5">
            <FileText className="w-3.5 h-3.5 text-indigo-400 shrink-0" />
            <h3 className="text-[11px] font-mono font-semibold text-slate-300 uppercase tracking-wider">
              Instruction sent to the model
            </h3>
          </div>

          {loading ? (
            <div className="flex items-center gap-2 text-xs font-mono text-slate-400 py-4">
              <Loader2 className="w-3.5 h-3.5 animate-spin text-indigo-400" />
              Reading this skill from the orchestrator…
            </div>
          ) : error ? (
            <div className="space-y-2">
              <p className="text-[11px] text-rose-300 flex items-start gap-1.5">
                <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
                <span>{error}</span>
              </p>
              <button
                type="button"
                id="btn-skill-detail-retry"
                onClick={() => void load()}
                className="px-2.5 py-1 rounded bg-[#020617] border border-slate-700 hover:border-indigo-600 text-slate-300 hover:text-indigo-300 font-mono text-[11px] cursor-pointer"
              >
                Retry
              </button>
            </div>
          ) : (
            <pre
              id="skill-detail-instruction"
              className="whitespace-pre-wrap break-words text-[11px] font-mono text-slate-200 bg-[#020617] border border-slate-800 rounded-md p-3 leading-relaxed"
            >
              {instruction}
            </pre>
          )}

          {!loading && !error && (
            <p className="text-[10px] font-mono text-slate-500 flex items-start gap-1.5">
              <Info className="w-3 h-3 shrink-0 mt-0.5" />
              <span>
                {isCustom
                  ? 'This is your own saved description, passed to the model verbatim — exactly the text above.'
                  : 'This is the backend’s built-in instruction for this skill.'}{' '}
                Read live from the orchestrator’s own resolver, so it is the same text the generator embeds in
                its prompt.
              </span>
            </p>
          )}
        </div>
      </div>
    </div>
  );
};
