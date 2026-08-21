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
import { CheckSquare, Square, Sparkles, Plus, Network, Info } from 'lucide-react';
import type { SkillDefinition } from '../../types/skills';
import { SkillManagerModal } from './SkillManagerModal';
import { McpConnectModal } from './McpConnectModal';

interface SkillsAndMcpPanelProps {
  skills: SkillDefinition[];
  selectedSkillIds: string[];
  onToggleSkill: (id: string) => void;
  onAddCustomSkill: (draft: { name: string; description: string }) => void;
  disabled?: boolean;
}

export const SkillsAndMcpPanel: React.FC<SkillsAndMcpPanelProps> = ({
  skills,
  selectedSkillIds,
  onToggleSkill,
  onAddCustomSkill,
  disabled = false,
}) => {
  const [managerOpen, setManagerOpen] = useState(false);
  const [mcpOpen, setMcpOpen] = useState(false);

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
            const selected = selectedSkillIds.includes(skill.id);
            const isDisabled = disabled || !skill.configurable || skill.availability === 'NOT_CONFIGURED';
            return (
              <li key={skill.id}>
                <button
                  type="button"
                  id={`skill-toggle-${skill.id}`}
                  role="checkbox"
                  aria-checked={selected}
                  aria-label={`${skill.name} — ${selected ? 'selected' : 'not selected'}`}
                  disabled={isDisabled}
                  onClick={() => onToggleSkill(skill.id)}
                  className={`w-full flex items-center gap-2 px-2.5 py-1.5 rounded-md text-left transition-colors cursor-pointer disabled:cursor-not-allowed disabled:opacity-50 ${
                    selected ? 'bg-indigo-950/50 border border-indigo-700/70' : 'bg-[#020617] border border-slate-800 hover:border-slate-700'
                  }`}
                >
                  {selected ? (
                    <CheckSquare className="w-3.5 h-3.5 text-indigo-400 shrink-0" />
                  ) : (
                    <Square className="w-3.5 h-3.5 text-slate-600 shrink-0" />
                  )}
                  <span className="text-xs font-sans text-slate-200 truncate flex-1">{skill.name}</span>
                  {skill.type === 'CUSTOM' && (
                    <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-cyan-950/80 border border-cyan-700 text-cyan-300 shrink-0">
                      Custom
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      </div>

      <p className="text-[10px] font-mono text-slate-500 flex items-start gap-1.5">
        <Info className="w-3 h-3 shrink-0 mt-0.5" />
        <span>Skill selection is real, applied configuration state — external skill/MCP execution is not yet implemented.</span>
      </p>

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
    </div>
  );
};
