/**
 * TypeScript mirrors of the Rust orchestrator's wire types.
 *
 * Every type here corresponds 1:1 to a `#[derive(Serialize)]` struct or enum
 * in `services/orchestrator/src/`. The source of truth is the Rust code, not
 * this file — if they disagree, the Rust side is right.
 *
 *   TaskStatus / RunState / EventType / RunEvent / TaskState / RunSummary
 *     -> services/orchestrator/src/domain.rs
 *   EvidenceRow / TestIntelRow / FailureClusterRow / BugDraftRow /
 *   ApiKeyListRow / AuditRow
 *     -> services/orchestrator/src/store.rs
 *
 * Rust enums are serialized SCREAMING_SNAKE_CASE (`#[serde(rename_all =
 * "SCREAMING_SNAKE_CASE")]`), so the string unions below use that casing
 * verbatim. Timestamps are chrono `DateTime<Utc>`, which serde renders as
 * RFC 3339 strings.
 */

// ---------------------------------------------------------------- domain.rs

/** FRD §2 Test Status Model. Product failure (FAILED) and infrastructure
 *  failure (INFRA_ERROR) are deliberately distinct classifications. */
export type TaskStatus =
  | "QUEUED"
  | "RUNNING"
  | "PASSED"
  | "FAILED"
  | "BLOCKED"
  | "SKIPPED"
  | "CANCELLED"
  | "TIMEOUT"
  | "INFRA_ERROR"
  | "NEEDS_REVIEW";

/** SRD §2 Orchestrator State Machine. Independent of individual task states. */
export type RunState =
  | "CREATED"
  | "PENDING_APPROVAL"
  | "REJECTED"
  | "SCHEDULED"
  | "RUNNING"
  | "PARTIAL"
  | "COMPLETED"
  | "CANCELLED"
  | "FAILED";

/** SRD §4 event types. */
export type EventType =
  | "RUN_SCHEDULED"
  | "TASK_STARTED"
  | "TASK_PASSED"
  | "TASK_FAILED"
  | "TASK_BLOCKED"
  | "TASK_SKIPPED"
  | "TASK_CANCELLED"
  | "TASK_TIMEOUT"
  | "TASK_INFRA_ERROR"
  | "TASK_NEEDS_REVIEW"
  | "RUN_COMPLETED";

export interface TaskState {
  id: string;
  name: string;
  prerequisites: string[];
  status: TaskStatus;
  reason: string | null;
  started_at: string | null;
  finished_at: string | null;
}

export interface RunSummary {
  run_id: string;
  name: string;
  state: RunState;
  total: number;
  queued: number;
  running: number;
  passed: number;
  failed: number;
  blocked: number;
  skipped: number;
  cancelled: number;
  timeout: number;
  infra_error: number;
  needs_review: number;
  started_at: string;
  finished_at: string | null;
  project_id: string | null;
  requested_by: string | null;
  policy_notes: string | null;
  approved_by: string | null;
  approved_at: string | null;
}

/** `GET /runs/:id` — RunSummary flattened, plus the task list. */
export interface RunDetail extends RunSummary {
  tasks: TaskState[];
}

/** `GET /runs` list item — a deliberately narrower projection than RunSummary. */
export interface RunListItem {
  run_id: string;
  name: string;
  state: RunState;
  total: number;
  passed: number;
  failed: number;
  project_id: string | null;
}

/**
 * SRD §5 Event Ordering & Idempotency. `sequence` is a per-run monotonic
 * counter assigned by the single writer, which is what makes the reconnect
 * path (`?since=`) and consumer-side dedupe possible.
 */
export interface RunEvent {
  event_id: string;
  run_id: string;
  task_id: string | null;
  sequence: number;
  timestamp: string;
  schema_version: number;
  type: EventType;
  status: TaskStatus | null;
  summary: string | null;
}

// -------------------------------------------------------- run plan (POST /runs)

export type SimOutcome = "pass" | "fail" | "timeout" | "infra_error";

export interface SimSpec {
  outcome: SimOutcome;
  delay_ms?: number;
}

export interface TaskDef {
  id: string;
  name: string;
  prerequisites?: string[];
  simulate?: SimSpec | null;
  /** Capability-routing key: "playwright" | "newman" | "pytest" | "maestro"
   *  | "schemathesis". Absent falls back to the SimulatedExecutor. */
  executor?: string | null;
  params?: unknown;
}

export interface RunPlan {
  name: string;
  tasks: TaskDef[];
  concurrency?: number;
}

export interface CreateRunResponse {
  run_id: string;
  state: RunState;
  policy_notes: string | null;
}

// ----------------------------------------------------------------- store.rs

export interface EvidenceRow {
  id: string;
  task_id: string | null;
  kind: string;
  object_key: string;
  content_type: string | null;
  size_bytes: number;
  created_at: string;
}

export interface TestIntelRow {
  id: string;
  name: string;
  risk_score: number | null;
  failure_rate: number | null;
  dependency_count: number | null;
  execution_cost_ms: number | null;
  value_score: number | null;
  flakiness_score: number | null;
  window_runs: number | null;
  window_failures: number | null;
}

export interface FailureClusterRow {
  id: string;
  signature: string;
  first_seen_at: string;
  last_seen_at: string;
  occurrence_count: number;
  root_cause_hypothesis: string | null;
  confidence: number | null;
  reproducibility: number | null;
  review_state: string;
}

export interface BugDraftRow {
  id: string;
  failure_cluster_id: string;
  title: string;
  description: string;
  severity: string | null;
  confidence: number | null;
  review_state: string;
  created_at: string;
  promoted_at: string | null;
  promoted_by: string | null;
}

export interface ApiKeyListRow {
  id: string;
  name: string;
  role: ApiRole;
  project_id: string | null;
  created_at: string;
  revoked_at: string | null;
}

export interface AuditRow {
  occurred_at: string;
  actor: string | null;
  role: string | null;
  project_id: string | null;
  action: string;
  target: string | null;
  detail: string | null;
  outcome: string;
}

// --------------------------------------------------------------- auth.rs

export type ApiRole = "viewer" | "operator" | "admin";

// --------------------------------------------------------------- metrics.rs

/**
 * Parsed form of `GET /metrics`, which the orchestrator emits as Prometheus
 * text exposition format (not JSON) — see `metrics.rs`. Every field here is
 * a real counter read from the database; there is no derived "quality score"
 * or "pass percentage" because the backend does not compute one.
 */
export interface OrchestratorMetrics {
  /** `aiqa_runs_total{state="..."}` */
  runsByState: Record<string, number>;
  /** `aiqa_tasks_total{status="..."}` */
  tasksByStatus: Record<string, number>;
  /** `aiqa_bug_drafts_total{review_state="..."}` */
  bugDraftsByReviewState: Record<string, number>;
  failureClustersTotal: number | null;
  apiKeysActive: number | null;
  auditLogTotal: number | null;
}
