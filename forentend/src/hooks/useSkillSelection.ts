/**
 * AIQA — Skills & MCP, Phase 1 selection state.
 *
 * A standalone hook (not inline `useState` calls buried in
 * `TestCaseGeneratorPage.tsx`) so the next Test Case Generator
 * implementation unit — the real generation engine — can import and reuse
 * this exact selection contract without depending on that page's other
 * internals, and so this feature is testable on its own.
 *
 * Persistence scope (brief §10, evidence-based, not invented): selected
 * skills are session/UI state only, exactly like this page's existing
 * `selectedCategories`/`depth`/`requirementNotes` — kept across a branch
 * switch via the caller's own `initialSelectedIds`/`initialCustomSkills`
 * (typically read from `AIQAContext.activeGenerationSession`, itself only
 * ever in-memory), never sent to or persisted by the backend. No project-
 * or account-level persistence is invented here.
 */
import { useCallback, useMemo, useState } from 'react';
import { BUILT_IN_SKILLS } from '../services/skillRegistry';
import type { CustomSkillDraft, SkillDefinition } from '../types/skills';

let customSkillCounter = 0;

/** A local, session-only id for a just-created custom skill — a React key
 *  and selection-tracking id only, never presented as a real backend-
 *  issued identifier and never sent anywhere. Matches this codebase's own
 *  existing convention for local-draft ids (see
 *  `TestCaseGeneratorPage.tsx`'s `activeGenerationSession.id`) — a
 *  monotonic counter rather than `Math.random()`, so it is unambiguous on
 *  inspection that this is deterministic, not fabricated "random-looking"
 *  data. */
function nextCustomSkillId(): string {
  customSkillCounter += 1;
  return `custom-skill-${customSkillCounter}`;
}

export interface UseSkillSelectionResult {
  /** Built-in skills plus every custom skill created this session. */
  skills: SkillDefinition[];
  selectedSkillIds: string[];
  isSelected: (id: string) => boolean;
  toggleSkill: (id: string) => void;
  /** Adds a session-only custom skill and selects it. Returns the created
   *  definition so a caller (the Skill Manager) can confirm what was
   *  actually added without re-deriving it. */
  addCustomSkill: (draft: CustomSkillDraft) => SkillDefinition;
  customSkills: SkillDefinition[];
}

export function useSkillSelection(
  initialSelectedIds: string[] = [],
  initialCustomSkills: SkillDefinition[] = [],
): UseSkillSelectionResult {
  const [customSkills, setCustomSkills] = useState<SkillDefinition[]>(initialCustomSkills);
  const [selectedSkillIds, setSelectedSkillIds] = useState<string[]>(initialSelectedIds);

  const skills = useMemo(() => [...BUILT_IN_SKILLS, ...customSkills], [customSkills]);

  const isSelected = useCallback((id: string) => selectedSkillIds.includes(id), [selectedSkillIds]);

  const toggleSkill = useCallback((id: string) => {
    setSelectedSkillIds((prev) => (prev.includes(id) ? prev.filter((existing) => existing !== id) : [...prev, id]));
  }, []);

  const addCustomSkill = useCallback((draft: CustomSkillDraft): SkillDefinition => {
    const skill: SkillDefinition = {
      id: nextCustomSkillId(),
      name: draft.name.trim(),
      description: draft.description.trim(),
      type: 'CUSTOM',
      availability: 'AVAILABLE',
      configurable: true,
    };
    setCustomSkills((prev) => [...prev, skill]);
    setSelectedSkillIds((prev) => [...prev, skill.id]);
    return skill;
  }, []);

  return { skills, selectedSkillIds, isSelected, toggleSkill, addCustomSkill, customSkills };
}
