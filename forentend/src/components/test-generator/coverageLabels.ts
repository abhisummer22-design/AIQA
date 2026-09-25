import type { CoverageDepth, CoverageTechnique } from '../../types/contract';

/** How a technique is named on screen. */
export const TECHNIQUE_LABEL: Record<CoverageTechnique, string> = {
  positive: 'Positive',
  negative: 'Negative',
  edge: 'Edge',
  security: 'Security',
  performance: 'Performance',
  accessibility: 'Accessibility',
};

/** The built-in skill that provides each technique — what to select when none
 *  of the selected skills does. */
export const TECHNIQUE_SKILL: Record<CoverageTechnique, string> = {
  positive: 'Functional Testing',
  negative: 'Negative Testing',
  edge: 'Edge Case Analysis',
  security: 'Security Testing',
  performance: 'Performance Testing',
  accessibility: 'UI/UX Testing',
};

export const DEPTH_LABEL: Record<CoverageDepth, string> = {
  SMOKE: 'Smoke',
  STANDARD: 'Standard',
  EXHAUSTIVE: 'Exhaustive',
  CHAOS_EXPLORATORY: 'Chaos / exploratory',
};

/** What a depth asks of each requirement, in plain words. */
export const DEPTH_MEANING: Record<CoverageDepth, string> = {
  SMOKE: 'one case per requirement, of the technique that matters most for it',
  STANDARD: 'every technique that applies, once',
  EXHAUSTIVE: 'every technique that applies, twice',
  CHAOS_EXPLORATORY: 'every technique that applies, three times',
};

/** A technique the backend names that this screen does not know yet still
 *  gets a readable name. */
export const techniqueLabel = (technique: string): string =>
  TECHNIQUE_LABEL[technique as CoverageTechnique] ?? technique;

export const techniqueSkill = (technique: string): string | undefined =>
  TECHNIQUE_SKILL[technique as CoverageTechnique];
