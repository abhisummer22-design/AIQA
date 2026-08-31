/**
 * AIQA Neural Intelligence Graph — structural configuration.
 *
 * This file holds layout/shape/palette constants only — never a data
 * value. `MAJOR_STAGES` and `CATEGORY_NODES` are real backend vocabulary
 * (the six real pipeline stages this app actually has, and the real
 * 14-category set `GenerationConfigSection.tsx::TEST_CATEGORIES` already
 * defines for test-case generation) — not an invented "14 engines" list.
 */
import {
  Activity,
  AlertTriangle,
  Briefcase,
  BrainCircuit,
  Bug,
  Camera,
  CheckSquare,
  CircleCheck,
  CircleX,
  ClipboardList,
  FlaskConical,
  Gauge,
  GitBranch,
  Layers,
  Palette,
  PlayCircle,
  ScrollText,
  Shield,
  SlidersHorizontal,
  TrendingUp,
  Webhook,
  type LucideIcon,
} from 'lucide-react';
import type { NeuralNodeStatus, NeuralNodeTier } from './neuralGraph.types';

export const VIEWBOX_SIZE = 900;
export const CENTER = VIEWBOX_SIZE / 2;
export const MAJOR_RING_RADIUS = 190;
export const SATELLITE_RING_RADIUS = 360;
export const MAJOR_NODE_R = 34;
export const SATELLITE_NODE_R = 16;
export const CORE_NODE_R = 46;

/** The six real pipeline stages, in the real order a project's data
 *  actually moves through this backend (Contract: requirement text -> real
 *  persisted test cases -> real executions -> real evidence/risk signal ->
 *  real audit trail). Matches this app's real routes, not the spec's
 *  invented per-skill "engine" names. */
export const MAJOR_STAGES: { id: string; label: string; shortLabel: string }[] = [
  { id: 'requirement', label: 'Requirement Coverage', shortLabel: 'Requirement' },
  { id: 'test-case', label: 'Test Case Intelligence', shortLabel: 'Test Cases' },
  { id: 'execution', label: 'Execution Intelligence', shortLabel: 'Execution' },
  { id: 'evidence', label: 'Evidence Engine', shortLabel: 'Evidence' },
  { id: 'risk', label: 'Risk Analysis', shortLabel: 'Risk' },
  { id: 'audit', label: 'Audit Intelligence', shortLabel: 'Audit' },
];

/** The real 14 test-generation categories (`GenerationConfigSection.tsx`'s
 *  own `TEST_CATEGORIES`, restated as plain id/label pairs here rather than
 *  imported, since that module also carries icon/React-component fields
 *  this config file has no reason to depend on). Every `TestCaseRow` in
 *  this backend already carries one of these exact strings as its real
 *  `category` field. */
export const CATEGORY_NODES: { id: string; label: string }[] = [
  { id: 'Functional', label: 'Functional' },
  { id: 'Non-Functional', label: 'Non-Functional' },
  { id: 'Positive', label: 'Positive' },
  { id: 'Negative', label: 'Negative' },
  { id: 'UI/UX', label: 'UI/UX' },
  { id: 'Spike', label: 'Spike' },
  { id: 'Load', label: 'Load' },
  { id: 'Edge Case', label: 'Edge Case' },
  { id: 'Senior Base', label: 'Senior Base' },
  { id: 'Business Logic', label: 'Business Logic' },
  { id: 'API', label: 'API' },
  { id: 'Performance', label: 'Performance' },
  { id: 'Security', label: 'Security' },
  { id: 'Penetration', label: 'Penetration' },
];

export const STATUS_COLOR: Record<NeuralNodeStatus, string> = {
  active: '#22d3ee',
  processing: '#818cf8',
  completed: '#34d399',
  warning: '#fbbf24',
  failed: '#fb7185',
  idle: '#475569',
};

/** Warning/failed are never told apart by colour alone (spec §4) — each
 *  status also gets a distinct real glyph the node/legend renders. */
export const STATUS_GLYPH: Record<NeuralNodeStatus, string> = {
  active: '●',
  processing: '◐',
  completed: '✓',
  warning: '▲',
  failed: '✕',
  idle: '○',
};

export const TIER_RADIUS: Record<NeuralNodeTier, number> = {
  core: CORE_NODE_R,
  major: MAJOR_NODE_R,
  satellite: SATELLITE_NODE_R,
};

/** Which real pipeline stage (or the core hub) a node stands for — keyed by
 *  the exact node `id` `useNeuralGraphData.ts` assigns (`'core'`, then each
 *  `MAJOR_STAGES[i].id`). Purely a visual label for what the node already
 *  is; carries no data of its own. */
export const CORE_AND_STAGE_ICON: Record<string, LucideIcon> = {
  core: BrainCircuit,
  requirement: ClipboardList,
  'test-case': FlaskConical,
  execution: PlayCircle,
  evidence: Camera,
  risk: AlertTriangle,
  audit: ScrollText,
};

/** Which real test-generation category a satellite node stands for — keyed
 *  by the plain `CATEGORY_NODES[i].id` (the `cat-` id prefix is stripped
 *  before this lookup, since that prefix only exists to keep a category's
 *  graph-node id from colliding with a same-named stage/status id). */
export const CATEGORY_ICON: Record<string, LucideIcon> = {
  Functional: CheckSquare,
  'Non-Functional': SlidersHorizontal,
  Positive: CircleCheck,
  Negative: CircleX,
  'UI/UX': Palette,
  Spike: TrendingUp,
  Load: Gauge,
  'Edge Case': GitBranch,
  'Senior Base': Layers,
  'Business Logic': Briefcase,
  API: Webhook,
  Performance: Activity,
  Security: Shield,
  Penetration: Bug,
};
