/**
 * AIQA — Skills & MCP, Phase 1 registry.
 *
 * Fresh inspection before writing this file: no plugin registry, MCP
 * connector, model-gateway, or provider-configuration code exists anywhere
 * in `services/orchestrator` (backend) or `forentend/src` (frontend) —
 * confirmed by a repository-wide grep for `mcp|plugin|skill` before any
 * code in this unit was written. This registry is therefore a genuinely
 * new, standalone module, not a wrapper around an existing one.
 *
 * `BUILT_IN_SKILLS` are real, selectable generation-scope categories —
 * configurable capability *labels*, not running AI engines. Selecting one
 * does not start, call, or simulate any external service.
 */
import type { SkillDefinition } from '../types/skills';

/** The 8 initial built-in skills (brief §5). Two are pre-selected by
 *  default in `useSkillSelection` (Functional Testing, Edge Case Analysis)
 *  — the same pair the brief's own layout mock shows checked; the rest
 *  start deselected, not because they are unavailable, but because a
 *  narrower default is easier for a user to deliberately widen than to
 *  narrow. */
export const BUILT_IN_SKILLS: SkillDefinition[] = [
  {
    id: 'skill-functional-testing',
    name: 'Functional Testing',
    description: 'Core requirement and acceptance-criteria behavior coverage.',
    type: 'BUILT_IN',
    availability: 'AVAILABLE',
    configurable: true,
  },
  {
    id: 'skill-edge-case-analysis',
    name: 'Edge Case Analysis',
    description: 'Boundary values, unusual sequences, and rare-but-real conditions.',
    type: 'BUILT_IN',
    availability: 'AVAILABLE',
    configurable: true,
  },
  {
    id: 'skill-security-testing',
    name: 'Security Testing',
    description: 'Authentication, authorization, and input-handling risk coverage.',
    type: 'BUILT_IN',
    availability: 'AVAILABLE',
    configurable: true,
  },
  {
    id: 'skill-performance-testing',
    name: 'Performance Testing',
    description: 'Latency, throughput, and resource-behavior scenarios.',
    type: 'BUILT_IN',
    availability: 'AVAILABLE',
    configurable: true,
  },
  {
    id: 'skill-api-testing',
    name: 'API Testing',
    description: 'Request/response contract and status-code coverage.',
    type: 'BUILT_IN',
    availability: 'AVAILABLE',
    configurable: true,
  },
  {
    id: 'skill-database-testing',
    name: 'Database Testing',
    description: 'Data integrity, constraint, and query-behavior coverage.',
    type: 'BUILT_IN',
    availability: 'AVAILABLE',
    configurable: true,
  },
  {
    id: 'skill-rbac-testing',
    name: 'RBAC Testing',
    description: 'Role and permission-boundary scenario coverage.',
    type: 'BUILT_IN',
    availability: 'AVAILABLE',
    configurable: true,
  },
  {
    id: 'skill-business-case-testing',
    name: 'Business Case Testing',
    description: 'End-to-end business-rule and workflow scenario coverage.',
    type: 'BUILT_IN',
    availability: 'AVAILABLE',
    configurable: true,
  },
];

export const DEFAULT_SELECTED_SKILL_IDS: string[] = ['skill-functional-testing', 'skill-edge-case-analysis'];

/** A skill's name with its generic testing-verb suffix removed, so it can
 *  be compared to a test-category id — "Performance Testing" ->
 *  "performance", matching category id "Performance". Only a fixed,
 *  short suffix list is stripped; nothing else about the name is guessed
 *  at or fuzzy-matched.
 *
 *  Exported (not just used by `categoryMatchedSkillIds` below) because
 *  `GeneratedTestCasesView` reuses the exact same stemming rule to resolve
 *  a generated test case's real `category` text (freeform — the AI model
 *  is not constrained to the 14 canonical category ids, and demonstrably
 *  sometimes writes the skill's own display name, e.g. "Functional
 *  Testing", into it) back to its real canonical category id. Same rule,
 *  same regex, one source of truth for "strip the generic testing-verb
 *  suffix" either direction. */
export function skillStem(name: string): string {
  return name.replace(/\s+(Testing|Analysis)$/i, '').trim().toLowerCase();
}

/** A small, explicit, human-reviewed table of the real cases where one of
 *  the 14 canonical test-category names and a built-in skill's own stemmed
 *  name are the same underlying concept, just worded differently — e.g.
 *  category "Business Logic" and skill "Business Case Testing" both mean
 *  "identify business-rule/workflow/decision-path scenarios". Each entry
 *  is a deliberate, individually-justified synonym, never a generic
 *  fuzzy/edit-distance guess: `categoryMatchedSkillIds` below still does
 *  exact stem equality — this table only widens what counts as "the same
 *  stem" for these specific, reviewed pairs. Add to this table only when a
 *  category and a skill are genuinely the same real concept; do not use it
 *  to paper over a category (like "Senior Base") that has no real skill
 *  counterpart at all. */
const CATEGORY_STEM_ALIASES: Record<string, string> = {
  'business logic': 'business case',
};

/**
 * Skills genuinely equivalent to a currently-selected test category — e.g.
 * category `"Performance"` matches skill `"Performance Testing"` — so that
 * skill's real instruction is included in generation automatically, without
 * requiring the user to also check its own box. Returns a map from matched
 * skill id to the real category name it matched, so a caller can say *why*
 * a skill is included rather than just *that* it is.
 *
 * Matching is exact stem equality only, never a general fuzzy/partial
 * guess: a skill is included here only when it unambiguously names the
 * same concept as a selected category, either directly or via the small,
 * curated `CATEGORY_STEM_ALIASES` table above. This deliberately leaves
 * most categories unmatched (no built-in skill named "Load Testing" or
 * "UI/UX Testing" exists today, and no alias papers over that) and most
 * non-built-in skills unmatched too (a custom or SKILL.md-file skill only
 * participates if its own name happens to equal a category, e.g. a file
 * literally named "Performance Testing") — those stay exactly as
 * selectable-by-hand as before. This is why a skill with no category
 * counterpart (Database Testing, RBAC Testing, most real SKILL.md files)
 * is never silently added or dropped by this function.
 *
 * Works identically for built-in, custom, and filesystem skills alike,
 * since it only ever reads `SkillDefinition.name` — never `type` — so a
 * newly-dropped SKILL.md file participates with zero extra wiring the
 * moment its name matches a category (directly or via an alias).
 */
export function categoryMatchedSkillIds(
  selectedCategories: readonly string[],
  skills: readonly SkillDefinition[],
): Map<string, string> {
  const stemToCategory = new Map(
    selectedCategories.map((c) => {
      const normalized = c.trim().toLowerCase();
      return [CATEGORY_STEM_ALIASES[normalized] ?? normalized, c];
    }),
  );
  const matched = new Map<string, string>();
  for (const skill of skills) {
    const category = stemToCategory.get(skillStem(skill.name));
    if (category) {
      matched.set(skill.id, category);
    }
  }
  return matched;
}

/** Real, evidence-based MCP status for this phase — re-stated here (not
 *  just in the UI copy) so any future code importing this module gets the
 *  same true fact from one place: no MCP connector backend exists yet. */
export const MCP_BACKEND_CONFIGURED = false as const;
