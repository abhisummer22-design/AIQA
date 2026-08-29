/**
 * AIQA — Skills & MCP selection state.
 *
 * A standalone hook (not inline `useState` calls buried in
 * `TestCaseGeneratorPage.tsx`) so the Test Case Generator's generation
 * engine can import and reuse this exact selection contract without
 * depending on that page's other internals, and so this feature is testable
 * on its own.
 *
 * Custom skills are real, backend-persisted resources
 * (`services/orchestrator/src/api.rs`'s `/api/v1/skills` routes, backed by
 * the `custom_skills` table — see that migration's own doc comment). This
 * hook fetches the real list on mount and creates real rows via
 * `aiqaApi.skills.create`; nothing here fabricates a skill locally.
 * Selection itself (`selectedSkillIds`) stays UI-local state, exactly like
 * this page's existing `selectedCategories`/`depth`/`requirementNotes` —
 * kept across a branch switch via the caller's own `initialSelectedIds`
 * (typically read from `AIQAContext.activeGenerationSession`), never
 * persisted itself.
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import { aiqaApi } from '../services/aiqaApi';
import { BUILT_IN_SKILLS } from '../services/skillRegistry';
import type { CustomSkillDraft, SkillDefinition } from '../types/skills';

function customSkillRowToDefinition(row: { id: string; name: string; description: string }): SkillDefinition {
  return {
    id: row.id,
    name: row.name,
    description: row.description,
    type: 'CUSTOM',
    availability: 'AVAILABLE',
    configurable: true,
  };
}

export interface UseSkillSelectionResult {
  /** Built-in skills plus every real, persisted custom skill. */
  skills: SkillDefinition[];
  selectedSkillIds: string[];
  isSelected: (id: string) => boolean;
  toggleSkill: (id: string) => void;
  /** Creates a real, backend-persisted custom skill and selects it. Throws
   *  `ApiError` on failure — the caller (the Skill Manager) is responsible
   *  for surfacing that, never for treating a failed create as if it
   *  succeeded. */
  addCustomSkill: (draft: CustomSkillDraft) => Promise<SkillDefinition>;
  customSkills: SkillDefinition[];
  /** True while the initial real custom-skill list is loading. */
  customSkillsLoading: boolean;
}

export function useSkillSelection(
  initialSelectedIds: string[] = [],
  initialCustomSkills: SkillDefinition[] = [],
): UseSkillSelectionResult {
  const [customSkills, setCustomSkills] = useState<SkillDefinition[]>(initialCustomSkills);
  const [customSkillsLoading, setCustomSkillsLoading] = useState(true);
  const [selectedSkillIds, setSelectedSkillIds] = useState<string[]>(initialSelectedIds);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const rows = await aiqaApi.skills.list();
        if (cancelled) return;
        setCustomSkills((prev) => {
          const fetched = rows.map(customSkillRowToDefinition);
          const fetchedIds = new Set(fetched.map((s) => s.id));
          // Keep any not-yet-reconciled skill from `initialCustomSkills`
          // (e.g. one just created this render, before the list has
          // caught up) that the real fetch hasn't returned yet.
          const localOnly = prev.filter((s) => !fetchedIds.has(s.id));
          return [...fetched, ...localOnly];
        });
      } catch {
        // Real backend unreachable/unauthenticated — leave whatever local
        // state already exists rather than throwing out of a page-load
        // effect; the Skill Manager's own create flow will surface the
        // same real error the next time it's used.
      } finally {
        if (!cancelled) setCustomSkillsLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const skills = useMemo(() => [...BUILT_IN_SKILLS, ...customSkills], [customSkills]);

  const isSelected = useCallback((id: string) => selectedSkillIds.includes(id), [selectedSkillIds]);

  const toggleSkill = useCallback((id: string) => {
    setSelectedSkillIds((prev) => (prev.includes(id) ? prev.filter((existing) => existing !== id) : [...prev, id]));
  }, []);

  const addCustomSkill = useCallback(async (draft: CustomSkillDraft): Promise<SkillDefinition> => {
    const row = await aiqaApi.skills.create({ name: draft.name.trim(), description: draft.description.trim() });
    const skill = customSkillRowToDefinition(row);
    setCustomSkills((prev) => [...prev, skill]);
    setSelectedSkillIds((prev) => [...prev, skill.id]);
    return skill;
  }, []);

  return { skills, selectedSkillIds, isSelected, toggleSkill, addCustomSkill, customSkills, customSkillsLoading };
}
