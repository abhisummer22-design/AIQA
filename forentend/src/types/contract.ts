/**
 * Wire types for the AIQA orchestrator (`services/orchestrator`), mirrored
 * 1:1 from the actual Rust response/request shapes (`domain.rs`, `store.rs`,
 * `api.rs`, `errors.rs`) — not from `AIQA_API_CONTRACT_v1.0.md`'s
 * illustrative examples, which several real endpoints deliberately diverge
 * from where the contract's own example was unimplementable or ambiguous
 * (see the BE-8..BE-15 implementation reports for each such decision).
 *
 * FE-1 scope: these are the types the transport layer (`services/aiqaApi.ts`)
 * needs to be real. They are deliberately NOT wired into any page/component
 * in this sprint — `forentend/src/types.ts` (the existing, page-consumed
 * vocabulary) is untouched, so no currently-rendering screen is affected.
 */

// ------------------------------------------------------------- canonical enums

/** `domain.rs::RunState::as_db_str` — every value this run-level state
 *  machine actually produces. */
export type RunState =
  | 'CREATED'
  | 'PENDING_APPROVAL'
  | 'REJECTED'
  | 'SCHEDULED'
  | 'RUNNING'
  | 'PARTIAL'
  | 'COMPLETED'
  | 'CANCELLED'
  | 'FAILED';

/** `domain.rs::TaskStatus::as_db_str`. Note: still `INFRA_ERROR`, not the
 *  contract's `INFRASTRUCTURE_ERROR` — that rename is FE-12/a separate
 *  backend unit, not yet done; do not silently alias it here. */
export type TaskStatus =
  | 'QUEUED'
  | 'RUNNING'
  | 'PASSED'
  | 'FAILED'
  | 'BLOCKED'
  | 'SKIPPED'
  | 'CANCELLED'
  | 'TIMEOUT'
  | 'INFRA_ERROR'
  | 'NEEDS_REVIEW';

/** `domain.rs::EventType::as_db_str` — this backend's real event
 *  vocabulary. Deliberately not remapped to the contract's
 *  (`TASK_STATUS_CHANGED`, `MODEL_*`, ...) — Integration Plan §11.2 item 2
 *  calls that remapping "a §31 decision, not an implementation choice". */
export type EventType =
  | 'RUN_SCHEDULED'
  | 'TASK_STARTED'
  | 'TASK_PASSED'
  | 'TASK_FAILED'
  | 'TASK_BLOCKED'
  | 'TASK_SKIPPED'
  | 'TASK_CANCELLED'
  | 'TASK_TIMEOUT'
  | 'TASK_INFRA_ERROR'
  | 'TASK_NEEDS_REVIEW'
  | 'RUN_COMPLETED';

export type ReviewState = 'NEEDS_REVIEW' | 'CONFIRMED' | 'DISMISSED';

export type Role = 'admin' | 'operator' | 'viewer';

// ------------------------------------------------------------------- projects

/** `store.rs::Project` (Contract §5). */
export interface Project {
  id: string;
  name: string;
  description: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateProjectRequest {
  name: string;
  description?: string | null;
}

export interface UpdateProjectRequest {
  name?: string | null;
  description?: string | null;
}

// ----------------------------------------------------------------- executions

/** `domain.rs::TaskDef` — one task in a submitted plan.
 *
 *  FE-8 fix: `simulate.outcome` was `'Pass' | 'Fail' | 'Timeout' |
 *  'InfraError'` (the Rust enum's own variant names) — but `domain.rs::
 *  SimOutcome` carries `#[serde(rename_all = "snake_case")]`, so the real
 *  wire values are lowercase `snake_case` (`pass`/`fail`/`timeout`/
 *  `infra_error`). Confirmed live: the real backend rejected the old
 *  values with a 422 ("unknown variant `Pass`, expected one of `pass`,
 *  `fail`, `timeout`, `infra_error`") the first time anything actually
 *  sent one — FE-1 defined this type but nothing exercised it for real
 *  until FE-8's execution-creation form did. */
export interface TaskDef {
  id: string;
  name: string;
  prerequisites?: string[];
  simulate?: { outcome: 'pass' | 'fail' | 'timeout' | 'infra_error'; delay_ms?: number };
  executor?: string;
  params?: unknown;
}

/** `domain.rs::RunPlan` — the real (non-contract-`test_case_ids`) request
 *  shape `POST .../executions` actually accepts; see BE-13 report §3 for
 *  why `test_case_ids`/`test_suite_id`/`execution_mode` are not supported —
 *  no real test-case/suite entity exists in this backend to validate them
 *  against, and this client does not pretend otherwise. */
export interface RunPlan {
  name: string;
  tasks: TaskDef[];
  concurrency?: number;
}

/** `api.rs::CreateExecutionResponse` — the scoped `POST .../executions`
 *  response (BE-13 §5: field is `status`, not the legacy route's `state`). */
export interface CreateExecutionResponse {
  run_id: string;
  status: RunState;
  policy_notes: string | null;
}

/** `domain.rs::TaskState`. */
export interface TaskState {
  id: string;
  name: string;
  prerequisites: string[];
  status: TaskStatus;
  reason: string | null;
  started_at: string | null;
  finished_at: string | null;
}

/** `domain.rs::RunSummary`, flattened into `GET .../executions/:run_id`
 *  together with `tasks` (`api.rs::RunDetail`). */
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

export interface RunDetail extends RunSummary {
  tasks: TaskState[];
}

/** `api.rs::RunListItem` — `GET .../executions` list item (a summary
 *  subset, not the full `RunSummary`). */
export interface RunListItem {
  run_id: string;
  name: string;
  state: RunState;
  total: number;
  passed: number;
  failed: number;
  project_id: string | null;
}

/** `api.rs::CancelRunResponse` (BE-13 §7). `status` is the run's state at
 *  the instant cancellation was accepted — execution is asynchronous, so
 *  this does not itself guarantee `CANCELLED` yet; poll/subscribe for the
 *  final state. */
export interface CancelRunResponse {
  run_id: string;
  status: RunState;
}

// ---------------------------------------------------------------------- events

/** `domain.rs::RunEvent`, as actually serialized (`type` is the renamed
 *  `event_type` field). `project_id`/`project_sequence` are only ever
 *  populated on the BE-14 project realtime stream and are `null` on events
 *  read back via the per-run REST catch-up endpoint (see that field's own
 *  doc comment in `domain.rs` for exactly why). */
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
  project_id: string | null;
  project_sequence: number | null;
}

// -------------------------------------------------------------------- evidence

/** `store.rs::EvidenceRow` — also the BE-15 §15.2 "Evidence Detail" shape.
 *  The raw artifact bytes are a separate fetch: `GET .../evidence/:id/content`. */
export interface EvidenceRow {
  id: string;
  task_id: string | null;
  kind: string;
  object_key: string;
  content_type: string | null;
  size_bytes: number;
  created_at: string;
}

// ------------------------------------------------------------------- findings

/** `store.rs::BugDraftRow` — this backend's real "Finding" entity (BE-15
 *  §3: Contract §16's vocabulary, this backend's `bug_drafts` data). */
export interface Finding {
  id: string;
  failure_cluster_id: string;
  title: string;
  description: string;
  severity: string;
  confidence: number;
  review_state: ReviewState;
  created_at: string;
  promoted_at: string | null;
  promoted_by: string | null;
}

/** `store.rs::FailureClusterRow`. */
export interface FailureCluster {
  id: string;
  signature: string;
  first_seen_at: string;
  last_seen_at: string;
  occurrence_count: number;
  root_cause_hypothesis: string | null;
  confidence: number | null;
  reproducibility: number | null;
  review_state: ReviewState;
}

/** `api.rs::FindingAnalysisResponse` (BE-15 §6 — `POST .../analysis`). */
export interface FindingAnalysis {
  finding: Finding;
  failure_cluster: FailureCluster;
}

export interface ReviewFindingRequest {
  action: 'confirm' | 'dismiss';
}

// ------------------------------------------------------------ dashboard / kpis

/** `api.rs::DashboardResponse` (BE-12 §4). Only the concepts genuinely
 *  derivable from real persisted state are present — see that report §3
 *  for the full list of §23 concepts intentionally omitted rather than
 *  fabricated (report counts, a computed health status, API latency,
 *  model/worker health, ...). */
export interface DashboardResponse {
  project_id: string;
  project_name: string;
  execution_counts: Record<string, number>;
  task_counts: Record<string, number>;
  pass_fail: { passed: number; failed: number };
  test_count: number;
  generated_at: string;
}

/** `api.rs::KpisResponse` (BE-12 §4). `null` means "no qualifying data
 *  yet" — never render it as `0`. */
export interface KpisResponse {
  project_id: string;
  execution_reliability: number | null;
  execution_efficiency_ms: number | null;
  generated_at: string;
}

// ---------------------------------------------------------------------- audit

/** `store.rs::AuditRow` (BE-15 §8, project-scoped `GET .../audit`). */
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

// ----------------------------------------------------------------------- errors

/** `errors.rs::ErrorBody` — API Contract §2.5's structured envelope, which
 *  every REST handler in this backend actually returns on failure
 *  (BE-9/BE-10). WebSocket pre-upgrade rejections are the one deliberate
 *  exception (plain status + text) — see `services/aiqaApi.ts`. */
export interface ApiErrorEnvelope {
  error: {
    code: string;
    message: string;
    details: unknown;
    request_id: string;
  };
}
