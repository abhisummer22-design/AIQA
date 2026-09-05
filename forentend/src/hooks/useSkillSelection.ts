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
import type { FileSkill } from '../types/contract';
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

/** A `FileSkill`'s `instruction` — not its short `description` — becomes
 *  `SkillDefinition.description`: that field is what every consumer
 *  (generation, `SkillDetailModal`'s resolve call) already treats as "the
 *  real text this skill applies", exactly like a `CUSTOM` skill's own
 *  stored description. The file's own short summary and remaining
 *  frontmatter survive separately in `fileSource`, not discarded. */
function fileSkillToDefinition(row: FileSkill): SkillDefinition {
  return {
    id: row.id,
    name: row.name,
    description: row.instruction,
    type: 'FILE',
    availability: 'AVAILABLE',
    configurable: true,
    fileSource: {
      fileName: row.file_name,
      summary: row.description,
      metadata: row.metadata,
    },
  };
}

export interface UseSkillSelectionResult {
  /** Built-in skills, every real persisted custom skill, and every real
   *  SKILL.md file currently sitting in the backend's skills folder. */
  skills: SkillDefinition[];
  selectedSkillIds: string[];
  isSelected: (id: string) => boolean;
  toggleSkill: (id: string) => void;
  /** Selects every real, currently-selectable skill at once — the same
   *  `configurable && availability !== 'NOT_CONFIGURED'` eligibility every
   *  individual skill's own checkbox already enforces (`SkillsAndMcpPanel`'s
   *  `isDisabled` check), so this can never select a skill an individual
   *  click couldn't. A skill only selected via its matching category
   *  (`categoryMatchedSkillIds`) is untouched either way — that is driven
   *  by category selection, not `selectedSkillIds`, exactly like a single
   *  click on a locked/auto-matched skill is already a no-op. */
  selectAllSkills: () => void;
  /** Clears every manual selection — an auto-matched-via-category skill
   *  stays exactly as selected as it was (see `selectAllSkills`'s own doc
   *  comment); uncheck the category itself to actually remove one of
   *  those. */
  deselectAllSkills: () => void;
  /** Creates a real, backend-persisted custom skill and selects it. Throws
   *  `ApiError` on failure — the caller (the Skill Manager) is responsible
   *  for surfacing that, never for treating a failed create as if it
   *  succeeded. */
  addCustomSkill: (draft: CustomSkillDraft) => Promise<SkillDefinition>;
  customSkills: SkillDefinition[];
  /** True while the initial real custom-skill list is loading. */
  customSkillsLoading: boolean;
  /** Every real SKILL.md file currently readable from the backend's skills
   *  folder, mapped to a selectable `FILE` skill. */
  fileSkills: SkillDefinition[];
  /** True while the initial real file-skill scan is loading. */
  fileSkillsLoading: boolean;
  /** Real per-file parse failures from the backend's last scan (e.g. a
   *  `.md` file using block-style YAML lists) — surfaced verbatim so a
   *  broken file is a visible, actionable state rather than one that
   *  silently vanishes from the list. */
  fileSkillErrors: Array<{ fileName: string; error: string }>;
  /** The real backend folder path files should be dropped into, once the
   *  first scan has returned it. */
  fileSkillsDirectory: string | null;
}

export function useSkillSelection(
  initialSelectedIds: string[] = [],
  initialCustomSkills: SkillDefinition[] = [],
): UseSkillSelectionResult {
  const [customSkills, setCustomSkills] = useState<SkillDefinition[]>(initialCustomSkills);
  const [customSkillsLoading, setCustomSkillsLoading] = useState(true);
  const [fileSkills, setFileSkills] = useState<SkillDefinition[]>([]);
  const [fileSkillsLoading, setFileSkillsLoading] = useState(true);
  const [fileSkillErrors, setFileSkillErrors] = useState<Array<{ fileName: string; error: string }>>([]);
  const [fileSkillsDirectory, setFileSkillsDirectory] = useState<string | null>(null);
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

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const scan = await aiqaApi.skills.files();
        if (cancelled) return;
        setFileSkills(scan.skills.map(fileSkillToDefinition));
        setFileSkillErrors(scan.errors.map((e) => ({ fileName: e.file_name, error: e.error })));
        setFileSkillsDirectory(scan.directory);
      } catch {
        // Real backend unreachable/unauthenticated — same honest
        // do-nothing-locally behaviour as the custom-skill fetch above.
      } finally {
        if (!cancelled) setFileSkillsLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const skills = useMemo(
    () => [...BUILT_IN_SKILLS, ...customSkills, ...fileSkills],
    [customSkills, fileSkills],
  );

  const isSelected = useCallback((id: string) => selectedSkillIds.includes(id), [selectedSkillIds]);

  const toggleSkill = useCallback((id: string) => {
    setSelectedSkillIds((prev) => (prev.includes(id) ? prev.filter((existing) => existing !== id) : [...prev, id]));
  }, []);

  const selectAllSkills = useCallback(() => {
    setSelectedSkillIds(
      skills.filter((s) => s.configurable && s.availability !== 'NOT_CONFIGURED').map((s) => s.id),
    );
  }, [skills]);

  const deselectAllSkills = useCallback(() => {
    setSelectedSkillIds([]);
  }, []);

  const addCustomSkill = useCallback(async (draft: CustomSkillDraft): Promise<SkillDefinition> => {
    const row = await aiqaApi.skills.create({ name: draft.name.trim(), description: draft.description.trim() });
    const skill = customSkillRowToDefinition(row);
    setCustomSkills((prev) => [...prev, skill]);
    setSelectedSkillIds((prev) => [...prev, skill.id]);
    return skill;
  }, []);

  return {
    skills,
    selectedSkillIds,
    isSelected,
    toggleSkill,
    selectAllSkills,
    deselectAllSkills,
    addCustomSkill,
    customSkills,
    customSkillsLoading,
    fileSkills,
    fileSkillsLoading,
    fileSkillErrors,
    fileSkillsDirectory,
  };
}
