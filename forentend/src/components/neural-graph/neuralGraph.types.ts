/**
 * AIQA Neural Intelligence Graph — shared types.
 *
 * Every field here is meant to hold a real value derived from this app's
 * actual backend state (`useNeuralGraphData.ts` is the one place that
 * builds a `NeuralGraphData`). `metricValue` is deliberately a pre-
 * formatted string rather than a bare number so "N/A"/"Awaiting Data" are
 * first-class, not a special case bolted on afterward — a node with no
 * real backing metric renders that string, never an invented number.
 */

export type NeuralNodeStatus = 'active' | 'processing' | 'completed' | 'warning' | 'failed' | 'idle';

/** `major` = one of the six real pipeline stages (the big spokes). `core`
 *  = the single center hub. `satellite` = one real test-case category
 *  (the many small outer nodes). */
export type NeuralNodeTier = 'core' | 'major' | 'satellite';

export interface NeuralNodeSummaryRow {
  label: string;
  value: string;
}

/** The real detail shown when a node is clicked — every row here must
 *  already exist in `value` form before the node is built; this type has
 *  no field for "guess if missing". */
export interface NeuralNodeDetail {
  rows: NeuralNodeSummaryRow[];
  /** Real recent items relevant to this node (e.g. the newest test cases
   *  in a category), already formatted as short display strings. Empty
   *  array — not omitted — when the query genuinely returned nothing. */
  recent: string[];
}

export interface NeuralNodeData {
  id: string;
  /** Full real name, shown in the tooltip and detail panel. */
  label: string;
  /** Short on-graph label — the SVG canvas is not wide enough for the
   *  full name at 21 nodes; the full name is never hidden, only deferred
   *  to hover/click. */
  shortLabel: string;
  tier: NeuralNodeTier;
  status: NeuralNodeStatus;
  metricLabel: string;
  metricValue: string;
  detail: NeuralNodeDetail;
}

export interface NeuralEdgeData {
  id: string;
  from: string;
  to: string;
  /** An edge only animates while both endpoints report real, currently-
   *  active work — see `NeuralEdge.tsx`'s own doc comment. */
  active: boolean;
}

export interface NeuralGraphSummary {
  nodesActive: number;
  nodesTotal: number;
  testsGenerated: number;
  testsExecuted: number;
  /** `null` when this project has no test case carrying a real
   *  `risk_factor` yet — never a fabricated zero. */
  risksDetected: number | null;
  criticalIssues: number | null;
  /** Real share of test cases that have reached `APPROVED`, as a 0-100
   *  number, or `null` with zero test cases (nothing to compute a ratio
   *  over). */
  coveragePercent: number | null;
  /** The real routing score of the model `GET /api/v1/ai/status` would
   *  currently route to, or `null` when no route exists. Not a 0-100
   *  "confidence" — that concept does not exist in this backend; the
   *  label calls it what it really is (see `NeuralGraphStats.tsx`). */
  routingScore: number | null;
}

export interface NeuralGraphData {
  projectId: string;
  projectName: string;
  nodes: NeuralNodeData[];
  edges: NeuralEdgeData[];
  summary: NeuralGraphSummary;
}
