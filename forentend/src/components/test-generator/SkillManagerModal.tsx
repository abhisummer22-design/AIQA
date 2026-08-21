/**
 * AIQA — Skills & MCP, Phase 1: Skill Manager.
 *
 * Reuses the exact overlay/card visual pattern already established by
 * `CommandPalette.tsx` (the only existing modal-shaped UI in this
 * repository, confirmed by inspection before writing this file — no
 * reusable `<Modal>`/`<Drawer>` component exists to import instead), so
 * this feels native rather than introducing a second design language.
 */
import React, { useState } from 'react';
import { Search, X, CheckSquare, Square, Plus, Info } from 'lucide-react';
import type { SkillDefinition, SkillSourceType } from '../../types/skills';

interface SkillManagerModalProps {
  skills: SkillDefinition[];
  selectedSkillIds: string[];
  onToggleSkill: (id: string) => void;
  onAddCustomSkill: (draft: { name: string; description: string }) => void;
  onClose: () => void;
}

function typeBadge(type: SkillSourceType): React.ReactNode {
  const styles: Record<SkillSourceType, string> = {
    BUILT_IN: 'bg-indigo-950/80 border-indigo-700 text-indigo-300',
    CUSTOM: 'bg-cyan-950/80 border-cyan-700 text-cyan-300',
    PLUGIN: 'bg-slate-900 border-slate-700 text-slate-400',
    MCP: 'bg-slate-900 border-slate-700 text-slate-400',
  };
  const labels: Record<SkillSourceType, string> = {
    BUILT_IN: 'Built-in',
    CUSTOM: 'Custom',
    PLUGIN: 'Plugin',
    MCP: 'MCP',
  };
  return (
    <span className={`px-1.5 py-0.2 rounded text-[9px] font-mono font-bold uppercase tracking-wider border ${styles[type]}`}>
      {labels[type]}
    </span>
  );
}

export const SkillManagerModal: React.FC<SkillManagerModalProps> = ({
  skills,
  selectedSkillIds,
  onToggleSkill,
  onAddCustomSkill,
  onClose,
}) => {
  const [search, setSearch] = useState('');
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [draftName, setDraftName] = useState('');
  const [draftDescription, setDraftDescription] = useState('');
  const [justAddedId, setJustAddedId] = useState<string | null>(null);

  const filtered = skills.filter((s) => {
    const q = search.trim().toLowerCase();
    if (!q) return true;
    return s.name.toLowerCase().includes(q) || s.description.toLowerCase().includes(q);
  });

  const handleCreate = () => {
    if (!draftName.trim()) return;
    onAddCustomSkill({ name: draftName.trim(), description: draftDescription.trim() || 'No description provided.' });
    setJustAddedId(draftName.trim());
    setDraftName('');
    setDraftDescription('');
    setShowCreateForm(false);
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-16 bg-slate-950/80 backdrop-blur-sm p-4 animate-in fade-in duration-100"
      onClick={onClose}
    >
      <div
        id="skill-manager-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Skill Manager"
        className="w-full max-w-2xl rounded-lg bg-[#0B1120] border border-slate-700/80 shadow-2xl overflow-hidden flex flex-col max-h-[80vh]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-slate-800 bg-[#020617]">
          <div>
            <h2 className="text-sm font-bold text-slate-100 font-sans">Skill Manager</h2>
            <p className="text-[11px] text-slate-500 font-sans">Search, select, and create generation-scope skills.</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close Skill Manager"
            className="p-1.5 text-slate-500 hover:text-slate-300 rounded cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Search */}
        <div className="flex items-center gap-2.5 px-4 py-2.5 border-b border-slate-800">
          <Search className="w-4 h-4 text-indigo-400 shrink-0" />
          <input
            autoFocus
            type="text"
            aria-label="Search skills"
            placeholder="Search skills by name or description..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="flex-1 bg-transparent border-none outline-none text-xs text-slate-100 placeholder-slate-500 font-sans"
          />
        </div>

        {/* Skill list */}
        <div className="flex-1 overflow-y-auto p-3 space-y-1.5">
          {filtered.length === 0 ? (
            <div className="py-8 text-center text-xs text-slate-500 font-mono">No matching skills found for "{search}"</div>
          ) : (
            filtered.map((skill) => {
              const selected = selectedSkillIds.includes(skill.id);
              const disabled = !skill.configurable || skill.availability === 'NOT_CONFIGURED';
              return (
                <button
                  key={skill.id}
                  type="button"
                  id={`skill-manager-item-${skill.id}`}
                  disabled={disabled}
                  onClick={() => onToggleSkill(skill.id)}
                  aria-pressed={selected}
                  className={`w-full flex items-start gap-3 p-2.5 rounded-md text-left transition-colors cursor-pointer disabled:cursor-not-allowed disabled:opacity-50 ${
                    selected ? 'bg-indigo-950/50 border border-indigo-700/70' : 'bg-[#020617] border border-slate-800 hover:border-slate-700'
                  }`}
                >
                  {selected ? (
                    <CheckSquare className="w-4 h-4 text-indigo-400 shrink-0 mt-0.5" />
                  ) : (
                    <Square className="w-4 h-4 text-slate-600 shrink-0 mt-0.5" />
                  )}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="text-xs font-bold text-slate-100 font-sans">{skill.name}</span>
                      {typeBadge(skill.type)}
                      <span className="text-[9px] font-mono text-slate-500">
                        {skill.availability === 'AVAILABLE' ? 'Available' : 'Not configured for external execution'}
                      </span>
                    </div>
                    <p className="text-[11px] text-slate-400 font-sans mt-0.5">{skill.description}</p>
                  </div>
                </button>
              );
            })
          )}
        </div>

        {/* Create custom skill */}
        <div className="border-t border-slate-800 p-3 space-y-2 bg-[#020617]">
          {justAddedId && !showCreateForm && (
            <p className="text-[11px] font-mono text-cyan-300 flex items-center gap-1.5">
              <Info className="w-3.5 h-3.5 shrink-0" />
              "{justAddedId}" added for this session only — not saved anywhere, and will be gone on reload.
            </p>
          )}
          {showCreateForm ? (
            <div className="space-y-2">
              <input
                type="text"
                aria-label="New skill name"
                placeholder="Skill name"
                value={draftName}
                onChange={(e) => setDraftName(e.target.value)}
                className="w-full px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-mono text-slate-100 placeholder-slate-500 focus:outline-none focus:border-indigo-500"
              />
              <textarea
                aria-label="New skill description"
                placeholder="Description (optional)"
                rows={2}
                value={draftDescription}
                onChange={(e) => setDraftDescription(e.target.value)}
                className="w-full px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-mono text-slate-100 placeholder-slate-500 focus:outline-none focus:border-indigo-500 resize-none"
              />
              <p className="text-[10px] font-mono text-amber-300 flex items-start gap-1.5">
                <Info className="w-3.5 h-3.5 shrink-0 mt-0.5" />
                <span>Custom-skill persistence is not implemented yet. This skill will exist only for this browser session.</span>
              </p>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  id="btn-confirm-add-custom-skill"
                  onClick={handleCreate}
                  disabled={!draftName.trim()}
                  className="px-3.5 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed text-white font-mono text-xs font-bold cursor-pointer"
                >
                  Add to this session
                </button>
                <button
                  type="button"
                  onClick={() => setShowCreateForm(false)}
                  className="px-3.5 py-1.5 rounded-md bg-[#0B1120] border border-slate-700 text-slate-300 font-mono text-xs cursor-pointer"
                >
                  Cancel
                </button>
              </div>
            </div>
          ) : (
            <button
              type="button"
              id="btn-create-custom-skill"
              onClick={() => {
                setShowCreateForm(true);
                setJustAddedId(null);
              }}
              className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-md bg-[#0B1120] border border-slate-700 hover:border-indigo-600 text-slate-300 hover:text-indigo-300 font-mono text-xs font-semibold cursor-pointer transition-colors"
            >
              <Plus className="w-3.5 h-3.5" />
              <span>Create Custom Skill</span>
            </button>
          )}
        </div>
      </div>
    </div>
  );
};
