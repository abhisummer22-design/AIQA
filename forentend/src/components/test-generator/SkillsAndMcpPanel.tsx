/**
 * AIQA — Skills & MCP, Phase 1 panel.
 *
 * Sits beside the existing input/source area (brief §4). Visual language —
 * card background, border, spacing, font-mono labels, checkbox styling —
 * is lifted directly from the sibling `GenerationConfigSection.tsx`'s own
 * category grid (`CheckSquare`/`Square` icons, the same
 * `bg-[#0B1120] border border-slate-800` card treatment), not a new design
 * system.
 */
import React, { useState } from 'react';
import { CheckSquare, Square, Sparkles, Plus, Network, Info, Eye, AlertTriangle, FolderOpen } from 'lucide-react';
import type { SkillDefinition } from '../../types/skills';
import { SkillManagerModal } from './SkillManagerModal';
import { McpConnectModal } from './McpConnectModal';
import { SkillDetailModal } from './SkillDetailModal';

interface SkillsAndMcpPanelProps {
  skills: SkillDefinition[];
  selectedSkillIds: string[];
  onToggleSkill: (id: string) => void;
  onAddCustomSkill: (draft: { name: string; description: string }) => Promise<SkillDefinition>;
  disabled?: boolean;
  /** Real per-file parse failures from the backend's last skills-folder
   *  scan (`useSkillSelection().fileSkillErrors`) — shown verbatim rather
   *  than silently dropping the broken file from the list. */
  fileSkillErrors?: Array<{ fileName: string; error: string }>;
  /** The real backend folder path SKILL.md files are read from
   *  (`useSkillSelection().fileSkillsDirectory`), shown so a user knows
   *  exactly where "drop a file" means. */
  fileSkillsDirectory?: string | null;
  /** Skill id -> the real selected category it matches
   *  (`categoryMatchedSkillIds`, e.g. "Performance Testing" once
   *  "Performance" is checked). Such a skill is genuinely sent for
   *  generation even when its own checkbox is off, so its row must say so
   *  rather than leaving the unchecked box looking like "not used". */
  autoMatchedSkills?: Map<string, string>;
  /** Real, live names of the skills in whichever generation wave is
   *  running right now (`TestCaseGeneratorPage`'s WS-parsed
   *  `ai_generation.rs::run_wave` "started" events) — empty whenever no
   *  generation is in flight. Matched by skill *name*, the one real value
   *  both the backend's wave-start message and this skill list share.
   *  Multiple names can glow at once because a wave genuinely runs every
   *  skill assigned to it in one combined AI call, not one after another —
   *  the glow's own real granularity, not a simplification. */
  activeSkillNames?: string[];
}

export const SkillsAndMcpPanel: React.FC<SkillsAndMcpPanelProps> = ({
  skills,
  selectedSkillIds,
  onToggleSkill,
  onAddCustomSkill,
  disabled = false,
  fileSkillErrors = [],
  fileSkillsDirectory = null,
  autoMatchedSkills = new Map(),
  activeSkillNames = [],
}) => {
  const [managerOpen, setManagerOpen] = useState(false);
  const [mcpOpen, setMcpOpen] = useState(false);
  /** The skill whose real instruction is being viewed, if any. Kept separate
   *  from selection: viewing what a skill says must never change whether it
   *  is applied. */
  const [detailSkill, setDetailSkill] = useState<SkillDefinition | null>(null);

  return (
    <div id="skills-and-mcp-panel" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-3 h-full flex flex-col">
      <div className="flex items-center gap-2 pb-2 border-b border-slate-800/80">
        <Sparkles className="w-4 h-4 text-cyan-400" />
        <h2 className="text-sm font-bold text-slate-100 font-sans tracking-tight">Skills & MCP</h2>
      </div>

      <div className="space-y-1.5">
        <h3 className="text-[11px] font-mono font-semibold text-slate-400 uppercase tracking-wider">Selected Skills</h3>
        <ul className="space-y-1.5" role="group" aria-label="Selected skills">
          {skills.map((skill) => {
            const manuallySelected = selectedSkillIds.includes(skill.id);
            const autoCategory = autoMatchedSkills.get(skill.id);
            // A skill matching a checked category is genuinely sent for
            // generation whether or not its own box was ever clicked — the
            // box must show that real state, not just the manual flag.
            const selected = manuallySelected || autoCategory !== undefined;
            const isDisabled = disabled || !skill.configurable || skill.availability === 'NOT_CONFIGURED';
            // Locked on, not off: while its category matches, unchecking
            // this box would have no effect (the category re-adds it), so a
            // click here is a no-op rather than a misleading toggle —
            // uncheck the category itself to actually remove it.
            const lockedByCategory = autoCategory !== undefined;
            // Real, live: this skill's own name is in the wave the backend
            // just reported starting (`TestCaseGeneratorPage`'s WS listener)
            // — matched by name, not merely "selected", since only a
            // selected skill can ever appear here in the first place.
            const isActiveNow = activeSkillNames.includes(skill.name);
            return (
              <li key={skill.id}>
                {/* Two separate controls on purpose: the toggle changes real
                    applied configuration, the view button only reads. Nesting
                    a button inside the checkbox would make "show me what this
                    says" also flip whether the skill is used. */}
                <div
                  id={`skill-row-${skill.id}`}
                  data-active-now={isActiveNow}
                  className={`flex items-center gap-1 rounded-md transition-all duration-300 ${
                    isActiveNow
                      ? 'bg-cyan-950/60 border border-cyan-400 shadow-[0_0_14px_2px_rgba(34,211,238,0.45)]'
                      : selected
                        ? 'bg-indigo-950/50 border border-indigo-700/70'
                        : 'bg-[#020617] border border-slate-800 hover:border-slate-700'
                  }`}
                >
                  <button
                    type="button"
                    id={`skill-toggle-${skill.id}`}
                    role="checkbox"
                    aria-checked={selected}
                    aria-label={`${skill.name} — ${selected ? 'selected' : 'not selected'}${autoCategory ? ` (auto, via ${autoCategory} category)` : ''}${isActiveNow ? ' (running now)' : ''}`}
                    title={lockedByCategory ? `Included automatically because the "${autoCategory}" category is selected — uncheck that category to remove it` : undefined}
                    disabled={isDisabled}
                    onClick={() => {
                      if (!lockedByCategory) onToggleSkill(skill.id);
                    }}
                    className={`flex-1 min-w-0 flex items-center gap-2 px-2.5 py-1.5 text-left disabled:cursor-not-allowed disabled:opacity-50 ${
                      lockedByCategory ? 'cursor-default' : 'cursor-pointer'
                    }`}
                  >
                    {selected ? (
                      <CheckSquare className={`w-3.5 h-3.5 shrink-0 ${isActiveNow ? 'text-cyan-300' : 'text-indigo-400'}`} />
                    ) : (
                      <Square className="w-3.5 h-3.5 text-slate-600 shrink-0" />
                    )}
                    <span className={`text-xs font-sans truncate flex-1 ${isActiveNow ? 'text-cyan-100 font-semibold' : 'text-slate-200'}`}>
                      {skill.name}
                    </span>
                    {isActiveNow && (
                      <span
                        id={`skill-active-badge-${skill.id}`}
                        title="This skill's wave is running right now"
                        className="flex items-center gap-1 text-[9px] font-mono font-bold px-1.5 py-0.2 rounded bg-cyan-950/80 border border-cyan-500 text-cyan-300 shrink-0"
                      >
                        <span className="relative flex h-1.5 w-1.5">
                          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-cyan-400 opacity-75"></span>
                          <span className="relative inline-flex rounded-full h-1.5 w-1.5 bg-cyan-400"></span>
                        </span>
                        LIVE
                      </span>
                    )}
                    {autoCategory && (
                      <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-emerald-950/80 border border-emerald-700 text-emerald-300 shrink-0">
                        Auto
                      </span>
                    )}
                    {skill.type === 'CUSTOM' && (
                      <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-cyan-950/80 border border-cyan-700 text-cyan-300 shrink-0">
                        Custom
                      </span>
                    )}
                    {skill.type === 'FILE' && (
                      <span
                        title={skill.fileSource?.fileName}
                        className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-amber-950/80 border border-amber-700 text-amber-300 shrink-0"
                      >
                        File
                      </span>
                    )}
                  </button>
                  <button
                    type="button"
                    id={`skill-view-${skill.id}`}
                    aria-label={`View what ${skill.name} instructs`}
                    title="View this skill's real instruction"
                    onClick={() => setDetailSkill(skill)}
                    className="p-1.5 mr-1 rounded text-slate-500 hover:text-indigo-300 hover:bg-slate-800/60 cursor-pointer shrink-0"
                  >
                    <Eye className="w-3.5 h-3.5" />
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      </div>

      <p className="text-[10px] font-mono text-slate-500 flex items-start gap-1.5">
        <Info className="w-3 h-3 shrink-0 mt-0.5" />
        <span>Skill selection is real, applied configuration state — external skill/MCP execution is not yet implemented.</span>
      </p>

      {fileSkillsDirectory && (
        <p className="text-[10px] font-mono text-slate-500 flex items-start gap-1.5">
          <FolderOpen className="w-3 h-3 shrink-0 mt-0.5 text-amber-500" />
          <span>
            Drop real SKILL.md files into <span className="text-slate-400 break-all">{fileSkillsDirectory}</span> to add
            them here — no restart needed.
          </span>
        </p>
      )}

      {fileSkillErrors.length > 0 && (
        <div id="file-skill-errors" className="space-y-1">
          {fileSkillErrors.map((e) => (
            <p key={e.fileName} className="text-[10px] font-mono text-rose-300 flex items-start gap-1.5">
              <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
              <span>
                <span className="text-rose-200">{e.fileName}</span> could not be read: {e.error}
              </span>
            </p>
          ))}
        </div>
      )}

      <div className="border-t border-slate-800/80 pt-3 mt-auto space-y-2">
        <button
          type="button"
          id="btn-add-skill"
          onClick={() => setManagerOpen(true)}
          disabled={disabled}
          className="w-full flex items-center justify-center gap-1.5 px-3 py-2 rounded-md bg-[#020617] hover:bg-slate-900 border border-slate-700 hover:border-indigo-600 text-slate-300 hover:text-indigo-300 font-mono text-xs font-semibold cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          <Plus className="w-3.5 h-3.5" />
          <span>Add Skill</span>
        </button>
        <button
          type="button"
          id="btn-connect-mcp"
          onClick={() => setMcpOpen(true)}
          disabled={disabled}
          className="w-full flex items-center justify-center gap-1.5 px-3 py-2 rounded-md bg-[#020617] hover:bg-slate-900 border border-slate-700 hover:border-cyan-600 text-slate-300 hover:text-cyan-300 font-mono text-xs font-semibold cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          <Network className="w-3.5 h-3.5" />
          <span>Connect MCP</span>
        </button>
      </div>

      {managerOpen && (
        <SkillManagerModal
          skills={skills}
          selectedSkillIds={selectedSkillIds}
          onToggleSkill={onToggleSkill}
          onAddCustomSkill={onAddCustomSkill}
          onClose={() => setManagerOpen(false)}
        />
      )}
      {mcpOpen && <McpConnectModal onClose={() => setMcpOpen(false)} />}
      {detailSkill && <SkillDetailModal skill={detailSkill} onClose={() => setDetailSkill(null)} />}
    </div>
  );
};
