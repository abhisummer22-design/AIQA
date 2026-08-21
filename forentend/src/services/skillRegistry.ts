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

/** Real, evidence-based MCP status for this phase — re-stated here (not
 *  just in the UI copy) so any future code importing this module gets the
 *  same true fact from one place: no MCP connector backend exists yet. */
export const MCP_BACKEND_CONFIGURED = false as const;
