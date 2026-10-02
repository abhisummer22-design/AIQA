/**
 * AIQA — Skills & MCP, Phase 1: Skill Manager.
 *
 * Reuses the exact overlay/card visual pattern already established by
 * `CommandPalette.tsx` (the only existing modal-shaped UI in this
 * repository, confirmed by inspection before writing this file — no
 * reusable `<Modal>`/`<Drawer>` component exists to import instead), so
 * this feels native rather than introducing a second design language.
 */
import React, { useRef, useState } from 'react';
import { Search, X, CheckSquare, Square, Plus, Info, Upload, Loader2, AlertTriangle } from 'lucide-react';
import { Modal, ModalHeader } from '../common/Modal';
import type { SkillDefinition, SkillSourceType } from '../../types/skills';

interface SkillManagerModalProps {
  skills: SkillDefinition[];
  selectedSkillIds: string[];
  onToggleSkill: (id: string) => void;
  onAddCustomSkill: (draft: { name: string; description: string }) => Promise<SkillDefinition>;
  onClose: () => void;
}

/** Read a text file client-side for the "attach a skill file" flow. Only
 *  plain-text formats are accepted (`.txt`/`.md`/`.json`/`.yaml`/`.yml`) —
 *  its content becomes the skill's real `description`, the exact text
 *  `ai_generation.rs::resolve_skill_instructions` embeds into the
 *  generation prompt, so anything that isn't readable text would silently
 *  become a useless instruction. */
function readTextFile(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ''));
    reader.onerror = () => reject(reader.error ?? new Error('Failed to read file'));
    reader.readAsText(file);
  });
}

function typeBadge(type: SkillSourceType): React.ReactNode {
  const styles: Record<SkillSourceType, string> = {
    BUILT_IN: 'bg-violet-950/80 border-violet-700 text-violet-300',
    CUSTOM: 'bg-yellow-950/80 border-yellow-700 text-yellow-300',
    FILE: 'bg-amber-950/80 border-amber-700 text-amber-300',
    PLUGIN: 'bg-slate-900 border-slate-700 text-slate-400',
    MCP: 'bg-slate-900 border-slate-700 text-slate-400',
  };
  const labels: Record<SkillSourceType, string> = {
    BUILT_IN: 'Built-in',
    CUSTOM: 'Custom',
    FILE: 'File',
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
  const [attachedFileName, setAttachedFileName] = useState<string | null>(null);
  const [justAddedId, setJustAddedId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const filtered = skills.filter((s) => {
    const q = search.trim().toLowerCase();
    if (!q) return true;
    return s.name.toLowerCase().includes(q) || s.description.toLowerCase().includes(q);
  });

  const handleFileSelected = async (file: File | undefined) => {
    if (!file) return;
    setSaveError(null);
    try {
      const text = await readTextFile(file);
      setDraftDescription((prev) => (prev.trim() ? `${prev.trim()}\n\n${text}` : text));
      setAttachedFileName(file.name);
    } catch {
      setSaveError(`Could not read "${file.name}" as text.`);
    }
  };

  const handleCreate = async () => {
    if (!draftName.trim() || saving) return;
    setSaving(true);
    setSaveError(null);
    try {
      const created = await onAddCustomSkill({
        name: draftName.trim(),
        description: draftDescription.trim() || 'No description provided.',
      });
      setJustAddedId(created.name);
      setDraftName('');
      setDraftDescription('');
      setAttachedFileName(null);
      setShowCreateForm(false);
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : 'Failed to save this skill.');
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      id="skill-manager-modal"
      ariaLabel="Skill Manager"
      onClose={onClose}
      panelClassName="w-full max-w-2xl max-h-[80vh]"
      overlayClassName="items-start justify-center pt-16"
    >
      <ModalHeader
        title="Skill Manager"
        subtitle="Search, select, and create generation-scope skills."
        onClose={onClose}
        closeLabel="Close Skill Manager"
      />

      {/* Search */}
        <div className="flex items-center gap-2.5 px-4 py-2.5 border-b border-slate-800">
          <Search className="w-4 h-4 text-violet-400 shrink-0" />
          <input
            autoFocus
            type="text"
            aria-label="Search skills"
            placeholder="Search skills by name or description..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="flex-1 bg-transparent border-none outline-none text-xs text-slate-100 placeholder-slate-500 font-sans focus:ring-1 focus:ring-violet-400 focus:ring-inset"
          />
        </div>

        {/* Skill list */}
        <div className="flex-1 overflow-y-auto p-3 space-y-1.5">
          {filtered.length === 0 ? (
            <div className="py-8 text-center text-xs text-content-muted font-mono">No matching skills found for "{search}"</div>
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
                    selected ? 'bg-violet-950/50 border border-violet-700/70' : 'bg-[#020617] border border-slate-800 hover:border-slate-700'
                  }`}
                >
                  {selected ? (
                    <CheckSquare className="w-4 h-4 text-violet-400 shrink-0 mt-0.5" />
                  ) : (
                    <Square className="w-4 h-4 text-content-muted shrink-0 mt-0.5" />
                  )}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="text-xs font-bold text-slate-100 font-sans">{skill.name}</span>
                      {typeBadge(skill.type)}
                      <span className="text-[9px] font-mono text-content-muted">
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
            <p className="text-[11px] font-mono text-yellow-300 flex items-center gap-1.5">
              <Info className="w-3.5 h-3.5 shrink-0" />
              "{justAddedId}" saved — available to every project from now on.
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
                disabled={saving}
                className="w-full px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-mono text-slate-100 placeholder-slate-500 focus:outline-none focus:border-violet-500 disabled:opacity-50"
              />
              <textarea
                aria-label="New skill description"
                placeholder="Description — the exact instruction the AI generator will follow for this skill (optional if you attach a file below)"
                rows={3}
                value={draftDescription}
                onChange={(e) => setDraftDescription(e.target.value)}
                disabled={saving}
                className="w-full px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-mono text-slate-100 placeholder-slate-500 focus:outline-none focus:border-violet-500 resize-none disabled:opacity-50"
              />
              <input
                ref={fileInputRef}
                type="file"
                accept=".txt,.md,.markdown,.json,.yaml,.yml"
                className="hidden"
                onChange={(e) => {
                  void handleFileSelected(e.target.files?.[0]);
                  e.target.value = '';
                }}
              />
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => fileInputRef.current?.click()}
                  disabled={saving}
                  className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-md bg-[#0B1120] border border-slate-700 hover:border-yellow-600 text-slate-300 hover:text-yellow-300 font-mono text-[11px] font-semibold cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  <Upload className="w-3.5 h-3.5" />
                  Attach file (.txt/.md/.json/.yaml)
                </button>
                {attachedFileName && <span className="text-[10px] font-mono text-content-muted truncate">{attachedFileName}</span>}
              </div>
              <p className="text-[10px] font-mono text-content-muted flex items-start gap-1.5">
                <Info className="w-3.5 h-3.5 shrink-0 mt-0.5" />
                <span>
                  Saved to the real orchestrator backend — persists across reloads and is available to every project. The
                  description text (attached file content included) is what the AI generator actually reads for this skill.
                </span>
              </p>
              {saveError && (
                <p className="text-[10px] font-mono text-rose-300 flex items-start gap-1.5">
                  <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
                  <span>{saveError}</span>
                </p>
              )}
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  id="btn-confirm-add-custom-skill"
                  onClick={() => void handleCreate()}
                  disabled={!draftName.trim() || saving}
                  className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 disabled:opacity-50 disabled:cursor-not-allowed text-white font-mono text-xs font-bold cursor-pointer"
                >
                  {saving && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
                  {saving ? 'Saving…' : 'Save Skill'}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setShowCreateForm(false);
                    setSaveError(null);
                  }}
                  disabled={saving}
                  className="px-3.5 py-1.5 rounded-md bg-[#0B1120] border border-slate-700 text-slate-300 font-mono text-xs cursor-pointer disabled:opacity-50"
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
              className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-md bg-[#0B1120] border border-slate-700 hover:border-violet-600 text-slate-300 hover:text-violet-300 font-mono text-xs font-semibold cursor-pointer transition-colors"
            >
              <Plus className="w-3.5 h-3.5" />
              <span>Create Custom Skill</span>
            </button>
          )}
        </div>
  </Modal>
  );
};
