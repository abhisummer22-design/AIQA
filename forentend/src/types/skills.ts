/**
 * AIQA — Skills & MCP, Phase 1 (evidence-based unit).
 *
 * Fresh repository inspection before writing this file confirmed there is
 * no existing skill/plugin/provider/MCP concept anywhere in this codebase
 * (`grep -ri "skill|mcp|plugin"` across `forentend/src` and
 * `services/orchestrator/src` returned no matches outside this unit's own
 * new files) and no model-gateway/AI-provider architecture at all. This is
 * a genuinely new, standalone conceptual model — not an extension of an
 * existing one.
 *
 * A "skill" here is a **configurable generation capability/category**, not
 * a running AI engine, external plugin process, or live MCP tool. Nothing
 * in this file, or anything that consumes it, may claim a skill is
 * "Connected", "Running", or backed by a live external service unless a
 * real implementation actually proves that state — see `availability`
 * below, which is the one field allowed to communicate that distinction.
 */

/** Where a skill definition came from.
 *  - `BUILT_IN` — shipped with this app, always available for selection.
 *  - `CUSTOM` — created by hand through the Skill Manager and persisted as a
 *    real `custom_skills` row (`services/orchestrator/src/store.rs`).
 *  - `FILE` — read live from a real `SKILL.md` file the user dropped into
 *    `services/orchestrator/skills/` on disk (`skill_files.rs`,
 *    `GET /api/v1/skills/files`). Re-scanned from disk on every fetch, so a
 *    file added while the app is open shows up without a restart — see
 *    `fileSource` below for the real facts the file itself stated.
 *  - `PLUGIN` — reserved for a future, real plugin registry. No plugin
 *    registry exists in this codebase today, so no skill of this type is
 *    ever produced by the current registry — the type exists so a future
 *    unit can introduce real plugin-sourced skills without changing this
 *    model or any of its consumers.
 *  - `MCP` — reserved for a future, real MCP capability discovered from a
 *    connected MCP server. No MCP connector backend exists in this
 *    codebase today (confirmed by inspection — see
 *    `services/skillRegistry.ts`'s own module doc comment), so no skill of
 *    this type is ever produced today either. */
export type SkillSourceType = 'BUILT_IN' | 'CUSTOM' | 'FILE' | 'PLUGIN' | 'MCP';

/** Truthful availability label (brief §5A). `AVAILABLE` means "this skill
 *  can be selected as a real generation-scope configuration value" — it
 *  never means "an AI model/plugin/MCP tool is actively running".
 *  `NOT_CONFIGURED` means selecting it would require an external
 *  integration (a plugin install, an MCP connection) that does not exist
 *  yet — such a skill is shown, but its checkbox is disabled, and it is
 *  never silently treated as selected. */
export type SkillAvailability = 'AVAILABLE' | 'NOT_CONFIGURED';

/** The standardized skill model every part of this feature (registry,
 *  selection state, the Skill Manager, and any future generation engine or
 *  plugin/MCP layer) shares. Adding a new `SkillSourceType` producer (a
 *  real plugin registry, a real MCP connector) means adding
 *  `SkillDefinition` values to a registry — it does not require changing
 *  this type or rewriting the Test Case Generator page. */
export interface SkillDefinition {
  id: string;
  name: string;
  description: string;
  type: SkillSourceType;
  availability: SkillAvailability;
  /** Whether a user may toggle this skill's selection at all. `false` is
   *  reserved for a future skill that exists (e.g. surfaced from a
   *  connected MCP server) but cannot yet be selected for another real
   *  reason — every skill produced by the current registry is `true`. */
  configurable: boolean;
  /** Real facts about a `FILE` skill's source file — absent for every other
   *  `type`. Kept separate from `description` (which already carries the
   *  real instruction for a `FILE` skill, exactly like `CUSTOM`) so the
   *  file's own short frontmatter summary and remaining frontmatter keys
   *  are still shown, not discarded. */
  fileSource?: {
    fileName: string;
    /** The file's own frontmatter `description` — a short summary, not the
     *  instruction. `null` when the file declared none. */
    summary: string | null;
    /** Every other real frontmatter key (version, author, frameworks, ...)
     *  as the file wrote it. */
    metadata: Record<string, unknown>;
  };
}

/** A user-submitted custom skill. Persisted for real via `POST
 *  /api/v1/skills` (`useSkillSelection.ts::addCustomSkill`) — every
 *  `CustomSkillDraft` becomes a real `custom_skills` row (see that
 *  migration's doc comment) and a `CUSTOM` `SkillDefinition` returned from
 *  the backend, not a local-only fabrication. */
export interface CustomSkillDraft {
  name: string;
  description: string;
}

/** Truthful MCP connection record. `status` is the one field a consumer of
 *  this type may show as "Connected" — and only `'CONNECTED'` may ever be
 *  rendered with a positive/green treatment, and only once a real backend
 *  connector exists and reports that state (none does today — see
 *  `services/skillRegistry.ts`). `'DRAFT'` is the only status this Phase 1
 *  unit's own UI can ever produce. No secret/credential field exists on
 *  this type on purpose — Phase 1 collects no credentials at all (brief
 *  §6: "Do NOT store secrets insecurely in frontend local state"). */
export interface McpConnectionDraft {
  name: string;
  status: 'DRAFT' | 'CONNECTED' | 'FAILED';
}
