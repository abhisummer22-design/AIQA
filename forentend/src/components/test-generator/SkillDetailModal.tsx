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
import { Modal } from '../common/Modal';
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


  const isCustom = skill.type === 'CUSTOM';
  const isFile = skill.type === 'FILE';
  const badgeLabel = isFile ? 'File' : isCustom ? 'Custom' : 'Built-in';
  const badgeClass = isFile
    ? 'bg-amber-950/80 border-amber-700 text-amber-300'
    : isCustom
      ? 'bg-yellow-950/80 border-yellow-700 text-yellow-300'
      : 'bg-violet-950/80 border-violet-700 text-violet-300';
  const metadataEntries = Object.entries(skill.fileSource?.metadata ?? {}).filter(([, v]) => v !== null && v !== undefined);

  return (
    // Migrated to the shared `Modal`. This one had its own Escape listener but
    // no focus trap, no focus move on open and no focus restore, so with the
    // dialog up, Tab still walked the page behind it -- WCAG 2.1.2 No Keyboard
    // Trap (A) and 2.4.3 Focus Order (A). The local Escape handler is gone: the
    // primitive owns that now, and two owners would close this twice.
    <Modal
      id="skill-detail-modal"
      ariaLabel={`Skill: ${skill.name}`}
      onClose={onClose}
      panelClassName="w-full max-w-lg max-h-[80vh]"
      overlayClassName="items-start justify-center pt-16"
    >
        <div className="flex items-start justify-between gap-3 px-4 py-3 border-b border-slate-800 bg-[#020617]">
          <div className="min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <h2 className="text-sm font-bold text-slate-100 font-sans truncate">{skill.name}</h2>
              <span
                className={`px-1.5 py-0.5 rounded text-[9px] font-mono font-bold uppercase tracking-wide border ${badgeClass}`}
              >
                {badgeLabel}
              </span>
            </div>
            <p className="text-[10px] font-mono text-content-muted mt-0.5 truncate">{skill.id}</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close skill detail"
            className="p-1.5 text-content-muted hover:text-slate-300 rounded cursor-pointer shrink-0"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="p-4 space-y-3 overflow-y-auto">
          {isFile && skill.fileSource && (
            <div id="skill-detail-file-source" className="space-y-1.5 rounded-md border border-amber-900/60 bg-amber-950/20 p-2.5">
              <p className="text-[10px] font-mono text-amber-300 truncate">Read from {skill.fileSource.fileName}</p>
              {skill.fileSource.summary && <p className="text-[11px] text-slate-300">{skill.fileSource.summary}</p>}
              {metadataEntries.length > 0 && (
                <dl className="grid grid-cols-[auto_1fr] gap-x-2 gap-y-0.5 pt-1 text-[10px] font-mono">
                  {metadataEntries.map(([key, value]) => (
                    <React.Fragment key={key}>
                      <dt className="text-content-muted">{key}</dt>
                      <dd className="text-slate-300 truncate">{Array.isArray(value) ? value.join(', ') : String(value)}</dd>
                    </React.Fragment>
                  ))}
                </dl>
              )}
            </div>
          )}

          <div className="flex items-center gap-1.5">
            <FileText className="w-3.5 h-3.5 text-violet-400 shrink-0" />
            <h3 className="text-[11px] font-mono font-semibold text-slate-300 uppercase tracking-wider">
              Instruction sent to the model
            </h3>
          </div>

          {loading ? (
            <div className="flex items-center gap-2 text-xs font-mono text-slate-400 py-4">
              <Loader2 className="w-3.5 h-3.5 animate-spin text-violet-400" />
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
                className="px-2.5 py-1 rounded bg-[#020617] border border-slate-700 hover:border-violet-600 text-slate-300 hover:text-violet-300 font-mono text-[11px] cursor-pointer"
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
            <p className="text-[10px] font-mono text-content-muted flex items-start gap-1.5">
              <Info className="w-3 h-3 shrink-0 mt-0.5" />
              <span>
                {isFile
                  ? 'This is the real markdown body of the file above, passed to the model verbatim — its YAML frontmatter is never sent.'
                  : isCustom
                    ? 'This is your own saved description, passed to the model verbatim — exactly the text above.'
                    : 'This is the backend’s built-in instruction for this skill.'}{' '}
                Read live from the orchestrator’s own resolver, so it is the same text the generator embeds in
                its prompt.
              </span>
            </p>
          )}
        </div>
    </Modal>
  );
};
