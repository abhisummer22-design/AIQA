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

// --------------------------------------------------------------- custom skills

/** `store.rs::CustomSkillRow`, `POST/GET /api/v1/skills`. A real, persisted
 *  custom skill — `description` is embedded verbatim into the AI generation
 *  prompt for any skill name `ai_generation.rs` doesn't recognize as
 *  built-in (see that module's `resolve_skill_instructions`). */
export interface CustomSkillRow {
  id: string;
  name: string;
  description: string;
  source_file: string | null;
  /** Real YAML frontmatter of an imported SKILL.md, preserved as the file
   *  wrote it. `null`/absent for a skill created by hand. */
  metadata?: Record<string, unknown> | null;
  created_by: string;
  created_at: string;
}

/** `skill_files.rs::FileSkill` — one real `SKILL.md` file read live from
 *  `services/orchestrator/skills/` on disk (`GET /api/v1/skills/files`).
 *  Unlike `CustomSkillRow`, `description` here is genuinely the file's own
 *  short frontmatter summary; `instruction` is the real text used for
 *  generation (the markdown body). */
export interface FileSkill {
  id: string;
  file_name: string;
  name: string;
  description: string | null;
  instruction: string;
  metadata: Record<string, unknown>;
  had_frontmatter: boolean;
  instruction_bytes: number;
}

/** `skill_files.rs::FileSkillError` — one file in the skills folder that
 *  could not be read as a skill, with the real reason. */
export interface FileSkillError {
  file_name: string;
  error: string;
}

/** `skill_files.rs::FileSkillScan`, `GET /api/v1/skills/files` — the real,
 *  live state of the filesystem skills folder right now. Re-read on every
 *  call, not cached (see that module's doc comment). */
export interface FileSkillScan {
  directory: string;
  skills: FileSkill[];
  errors: FileSkillError[];
}

/** `ai_generation.rs::ResolvedSkillInstruction` — the real instruction a
 *  skill resolves to, returned by `POST /api/v1/skills/resolve`. This is the
 *  exact string the generator embeds in the prompt: the endpoint calls the
 *  same `resolve_skill_instructions` the generator does, so what is shown
 *  cannot drift from what actually runs. */
export interface ResolvedSkillInstruction {
  id: string;
  name: string;
  instruction: string;
}

export interface CreateCustomSkillRequest {
  name: string;
  description?: string;
  source_file?: string | null;
}

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

// -------------------------------------------------------- test intelligence

/** `store.rs::TestIntelRow` (FE-22 — exposed via BE-10's project-scoped
 *  `GET .../intelligence/tests`). Real, deterministic (rule-based, not LLM
 *  — `intelligence.rs::update_test_intelligence`) per-test risk/value/
 *  flakiness scoring, computed after every run completes. Every numeric
 *  field is `Option<f64>`/`Option<i64>` on the backend and stays nullable
 *  here — a test with no scoreable history yet is `null`, never a
 *  fabricated `0`. */
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
  /** FE-26 — `api.rs::KpisResponse.execution_avg_run_duration_ms` (BE-12b).
   *  Average *run* duration in milliseconds (distinct from
   *  `execution_efficiency_ms`, which is per task), over every terminal
   *  run with both `started_at`/`finished_at`. `null` when no such run
   *  exists yet — never a fabricated `0`. */
  execution_avg_run_duration_ms: number | null;
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

// -------------------------------------------------------------- test cases

/** `domain.rs::TestCaseStatus` (AUD-005). Only `APPROVED` cases may be
 *  handed off to a real execution (FRD-EXE-11) — `store.rs`'s conditional
 *  `UPDATE ... WHERE status = ...` enforces every transition below
 *  server-side, not just in this client. */
export type TestCaseStatus = 'GENERATED' | 'VALIDATED' | 'APPROVED' | 'REJECTED' | 'EXECUTED';

/** `domain.rs::TestStep`. */
export interface TestCaseStep {
  step_number: number;
  action: string;
  expected_result: string;
}

/** `domain.rs::CreateTestCaseRequest` — `POST .../test-cases` body. */
export interface CreateTestCaseRequest {
  title: string;
  description: string;
  category: string;
  priority: string;
  severity: string;
  preconditions?: string[];
  steps: TestCaseStep[];
  expected_result?: string | null;
  assertions?: string[];
  test_data?: string | null;
  source_document?: string | null;
  requirement_reference?: string | null;
  source_requirement?: string | null;
  traceability?: unknown;
  risk_factor?: string | null;
  tags?: string[];
}

/** `store.rs::TestCaseRow` — the persisted, real shape returned by every
 *  `.../test-cases` endpoint. */
export interface TestCaseRow {
  id: string;
  project_id: string;
  title: string;
  description: string;
  category: string;
  priority: string;
  severity: string;
  preconditions: string[];
  steps: TestCaseStep[];
  expected_result: string;
  assertions: string[];
  test_data: string | null;
  source_document: string | null;
  requirement_reference: string | null;
  source_requirement: string | null;
  traceability: unknown | null;
  risk_factor: string | null;
  tags: string[];
  source: string;
  status: TestCaseStatus;
  validation_notes: string | null;
  executed_run_id: string | null;
  executed_task_id: string | null;
  created_by: string | null;
  created_at: string;
  updated_at: string;
}

export interface UpdateTestCaseRequest {
  title?: string;
  description?: string;
  priority?: string;
  preconditions?: string[];
  steps?: TestCaseStep[];
  expected_result?: string;
}

/** `domain.rs::TestCaseHandoffRequest`. Deliberately requires the caller to
 *  choose either a real `executor`, an explicit `simulate` outcome, or
 *  `compile_and_execute: true` (Phase 2.1) — the backend rejects a handoff
 *  with none of the three (400), rather than silently reporting a
 *  fabricated "passed" result for steps nothing has actually run. See
 *  `store.rs`/`api.rs`'s own doc comments on why. */
export interface TestCaseHandoffRequest {
  executor?: string;
  params?: unknown;
  simulate?: { outcome: 'pass' | 'fail' | 'timeout' | 'infra_error'; delay_ms?: number };
  /** Phase 2.1 — when `true`, `executor`/`params`/`simulate` above are
   *  ignored and the real backend compiler determines the real executor
   *  from the test case's own actual content instead. */
  compile_and_execute?: boolean;
}

// ---------------------------------------------------- Phase 2.1: execution plan compiler

/** `execution_plan.rs::ExecutionType`. */
export type PlanExecutionType = 'API' | 'BROWSER' | 'PYTHON';

/** `execution_plan.rs::Action` — the closed, real action vocabulary the
 *  compiler (deterministic or AI-assisted) may ever produce. There is
 *  deliberately no "run arbitrary command" variant. */
export type PlanAction =
  | { type: 'NAVIGATE'; url: string }
  | { type: 'CLICK'; selector: string }
  | { type: 'FILL'; selector: string; value: string }
  | { type: 'ASSERT_VISIBLE'; selector: string }
  | { type: 'ASSERT_TEXT'; selector: string; text: string }
  | { type: 'ASSERT_URL'; url_contains: string }
  | { type: 'REQUEST'; method: string; url: string; headers?: Record<string, string>; body?: unknown }
  | { type: 'ASSERT_STATUS'; status: number }
  | { type: 'ASSERT_HEADER'; name: string; value: string }
  | { type: 'ASSERT_BODY_FIELD'; path: string; equals: unknown };

/** `execution_plan.rs::CompilerMethod`. `'MANUAL'` (Phase 4) — no
 *  compilation happened at all; the user explicitly authored every action
 *  via the API Testing page's own request builder. */
export type PlanCompilerMethod = 'DETERMINISTIC' | 'AI' | 'MANUAL';

/** `execution_plan.rs::ExecutionPlan` — real, validated, and (when
 *  `compiler === 'AI'`) real-provider-attributed, exactly like Phase 1's
 *  `GenerateAiTestCasesResponse`. */
export interface ExecutionPlan {
  test_case_id: string;
  execution_type: PlanExecutionType;
  target: string | null;
  actions: PlanAction[];
  compiler: PlanCompilerMethod;
  diagnostics: string[];
  provider: string | null;
  model: string | null;
}

/** `execution_plan.rs::CompilationOutcome` — every truthful state the real
 *  compiler can report. Never collapse `NOT_EXECUTABLE` and
 *  `COMPILATION_UNAVAILABLE` into one generic "can't run" message — they
 *  mean different real things (the case genuinely lacks information, vs.
 *  no AI provider was reachable to even try). */
export type CompilationOutcome =
  | { status: 'READY'; test_case_id: string; execution_type: PlanExecutionType; target: string | null; actions: PlanAction[]; compiler: PlanCompilerMethod; diagnostics: string[]; provider: string | null; model: string | null }
  | { status: 'NOT_EXECUTABLE'; reasons: string[] }
  | { status: 'COMPILATION_FAILED'; reasons: string[] }
  | { status: 'COMPILATION_UNAVAILABLE'; reason: string };

// ---------------------------------------------------------- Phase 4: API Testing

/** `api_testing.rs::AdHocApiRequest` — `POST .../api-testing/execute`
 *  request. `assertions` reuses `PlanAction`'s own real, closed wire shape
 *  directly (only the three `ASSERT_*` variants are valid here — a
 *  `REQUEST`/browser action sent here is a real, honest
 *  `INVALID_REQUEST_CONFIGURATION` rejection server-side, not silently
 *  dropped). */
export interface AdHocApiRequest {
  method: string;
  url: string;
  query_params?: [string, string][];
  headers?: Record<string, string>;
  body?: unknown;
  assertions?: PlanAction[];
}

/** `executor.rs::ApiAssertionResult` — one real, actually-evaluated
 *  assertion outcome from `CompiledApiExecutor`'s own real HTTP call. */
export interface ApiAssertionResult {
  description: string;
  actual: string;
  passed: boolean;
}

/** `executor.rs::ApiResultSummary` — real, measured data from the one real
 *  HTTP call `CompiledApiExecutor` made. `request_duration_ms` is the real
 *  network-request-only duration (`Instant` around `.send().await` alone)
 *  — see `ApiTestExecutionResponse`'s own doc comment for how this differs
 *  from `started_at`/`finished_at`. `response_headers` has known-sensitive
 *  names (Authorization/Cookie/Set-Cookie/...) already redacted server-side
 *  — never sent to this client unredacted. */
export interface ApiResultSummary {
  status: number;
  request_duration_ms: number;
  response_headers: [string, string][];
  assertions: ApiAssertionResult[];
}

/** `api.rs::ApiTestExecutionResponse`. `task_status` is the exact same real
 *  `TaskStatus` vocabulary every other execution result in this app already
 *  uses (`PASSED`/`FAILED`/`INFRA_ERROR`/...) — `RUNNING` only when the
 *  real run genuinely didn't reach a terminal state within the backend's
 *  own wait ceiling (a real, ongoing execution, not a failure). `started_at`/
 *  `finished_at` are the task's real, persisted lifecycle timestamps —
 *  "end-to-end operation duration" (includes real scheduling/dispatch
 *  overhead), distinct from `result.request_duration_ms`'s pure network
 *  time. `result`/`response_body_preview`/`evidence_id` are only ever
 *  `null`/absent when the real evidence log genuinely couldn't be read —
 *  never a placeholder standing in for real data. */
export interface ApiTestExecutionResponse {
  run_id: string;
  task_status: 'QUEUED' | 'RUNNING' | 'PASSED' | 'FAILED' | 'BLOCKED' | 'SKIPPED' | 'CANCELLED' | 'TIMEOUT' | 'INFRA_ERROR' | 'NEEDS_REVIEW';
  reason: string | null;
  started_at: string | null;
  finished_at: string | null;
  result: ApiResultSummary | null;
  response_body_preview: string | null;
  evidence_id: string | null;
}

// ---------------------------------------------------- Phase 6: Performance Testing

/** `performance_testing.rs::WorkloadSchedule` — internally tagged on
 *  `test_type`, mirroring `PlanAction`'s own wire shape. Each variant is a
 *  genuinely distinct real schedule (see the Rust doc comment on this same
 *  type for exactly how) — never render all four with the same form fields
 *  relabeled. */
export type PerformanceWorkloadSchedule =
  | { test_type: 'LOAD'; concurrency: number; duration_secs: number; max_requests?: number | null }
  | { test_type: 'STRESS'; start_concurrency: number; max_concurrency: number; step_size: number; step_interval_secs: number; max_duration_secs: number }
  | { test_type: 'SPIKE'; baseline_concurrency: number; spike_concurrency: number; baseline_secs: number; spike_secs: number }
  | { test_type: 'VOLUME'; concurrency: number; total_requests: number };

/** `performance_testing.rs::PerformanceTestConfig` — the real `POST
 *  .../performance-testing/start` request body. `request` reuses
 *  `AdHocApiRequest`'s exact shape, but its own `assertions` field must be
 *  left empty here — a performance test's real per-request correctness
 *  check is `expected_status` alone (the backend rejects a non-empty
 *  `assertions` with `INVALID_REQUEST_CONFIGURATION`). Thresholds are all
 *  optional; when none are set the real verdict is `COMPLETED` (metrics
 *  only, no pass/fail judgement) — never a false `PASSED`. */
export interface PerformanceTestConfig {
  request: Omit<AdHocApiRequest, 'assertions'>;
  expected_status?: number | null;
  schedule: PerformanceWorkloadSchedule;
  max_p95_ms?: number | null;
  max_error_rate_percent?: number | null;
  min_throughput_rps?: number | null;
}

export type PerformanceSampleOutcome = 'SUCCESS' | 'FAILED' | 'ERROR';

/** `performance_testing.rs::RequestSample` — one real, completed HTTP
 *  round-trip. `concurrency_at_start` is the real, live worker count in
 *  effect at that moment — the truthful basis for a "concurrency over time"
 *  chart overlay, not inferred from the schedule's own configured values. */
export interface PerformanceRequestSample {
  sequence: number;
  offset_ms: number;
  duration_ms: number;
  outcome: PerformanceSampleOutcome;
  http_status: number | null;
  error_reason: string | null;
  concurrency_at_start: number;
}

/** `performance_testing.rs::PerformanceMetrics` — every field computed
 *  directly from the real `samples` gathered so far (or, once terminal, the
 *  complete real set). `throughput_rps` divides by the real wall-clock
 *  `actual_duration_ms`, never by a configured/expected duration. */
export interface PerformanceMetrics {
  total_requests: number;
  successful: number;
  failed: number;
  errored: number;
  error_rate_percent: number;
  success_rate_percent: number;
  min_ms: number;
  avg_ms: number;
  max_ms: number;
  p50_ms: number;
  p95_ms: number;
  p99_ms: number;
  throughput_rps: number;
  actual_duration_ms: number;
}

export type PerformanceRunLifecycleStatus = 'RUNNING' | 'COMPLETED' | 'CANCELLED' | 'ERROR' | 'INTERRUPTED';
export type PerformanceVerdict = 'COMPLETED' | 'PASSED' | 'FAILED' | 'ERROR' | 'CANCELLED';

/** `api.rs::PerformanceTestStartResponse`. */
export interface PerformanceTestStartResponse {
  run_id: string;
  status: 'RUNNING';
}

/** `api.rs::PerformanceRunStatusResponse` — real, live polling shape.
 *  `live: true` means this came from the actual in-process running/
 *  just-finished workload handle (`samples`/`metrics`/`current_concurrency`
 *  are real-time); `live: false` means no such handle exists in this
 *  backend process (a restart, or another process ran it) and this is the
 *  durable row instead — `samples` is honestly `[]` in that case, not a
 *  silently-stale copy; the full raw set, if persisted, is still fetchable
 *  via `evidence_id` through the existing evidence-content endpoint.
 *  `verdict`/`verdict_reasons` are only populated once `status !== 'RUNNING'`
 *  — never a provisional pass/fail while still in flight. */
export interface PerformanceRunStatusResponse {
  run_id: string;
  name: string;
  test_type: string;
  status: PerformanceRunLifecycleStatus;
  verdict: PerformanceVerdict | null;
  verdict_reasons: string[];
  /** Set only for a genuine `ERROR` result — the workload task itself
   *  failed unexpectedly (a real Tokio panic caught by the backend's own
   *  supervisor), not a per-request failure. The same safe message is also
   *  present as `verdict_reasons[0]` for convenience; this field exists so
   *  a caller doesn't need to know that convention to render it. `null` for
   *  every other status. */
  error_reason: string | null;
  elapsed_ms: number;
  current_concurrency: number;
  sample_count: number;
  metrics: PerformanceMetrics | null;
  samples: PerformanceRequestSample[];
  config: PerformanceTestConfig;
  started_at: string;
  finished_at: string | null;
  evidence_id: string | null;
  live: boolean;
}

/** `api.rs::PerformanceTestCancelResponse`. `reason` is only present when
 *  `cancelled === false` — a genuinely honest explanation (no live handle
 *  in this process, or the run was already terminal), never a silent
 *  false-success. */
export interface PerformanceTestCancelResponse {
  run_id: string;
  cancelled: boolean;
  reason: string | null;
}

/** `store.rs::PerformanceRunRow` — one row of real, persisted run history
 *  (`GET .../performance-testing`). `config_json`/`metrics_json`/
 *  `verdict_reasons_json` are opaque JSON text exactly as the backend
 *  stores them — parse with `JSON.parse` when displaying, same convention
 *  as every other `..._json` field this API already returns. */
export interface PerformanceRunRow {
  id: string;
  project_id: string;
  name: string;
  test_type: string;
  status: PerformanceRunLifecycleStatus;
  verdict: PerformanceVerdict | null;
  config_json: string;
  metrics_json: string | null;
  verdict_reasons_json: string | null;
  error_reason: string | null;
  evidence_id: string | null;
  created_by: string | null;
  started_at: string;
  finished_at: string | null;
}

// -------------------------- Truthfulness Cleanup Step 1: Application Source

/** `app_source.rs::AppSourceStatus`. Every value here corresponds to a real,
 *  distinct backend outcome — never collapsed into a generic success/failure. */
export type AppSourceVerifyStatus = 'CONNECTED' | 'UNREACHABLE' | 'REJECTED' | 'INVALID_URL' | 'ERROR';

/** `api.rs::VerifyApplicationSourceBody`. */
export interface VerifyApplicationSourceRequest {
  url: string;
}

/** `app_source.rs::VerifyApplicationSourceResponse` — the real, honest
 *  result of a real, bounded, SSRF-guarded outbound request
 *  (`POST .../verify-application-source`). `reachable` and `http_status`
 *  are deliberately distinct: a real server answering 401/403/404 is
 *  `reachable: true` with that real `http_status`, never conflated with
 *  `UNREACHABLE`. `latency_ms` is only ever a real, measured duration —
 *  `null` exactly when no request was ever attempted (`REJECTED`/
 *  `INVALID_URL`), never a fabricated/randomized value. */
export interface VerifyApplicationSourceResponse {
  status: AppSourceVerifyStatus;
  reachable: boolean;
  http_status: number | null;
  latency_ms: number | null;
  message: string;
}

// ---------------------------------------------------- AI-powered test generation

/** `api.rs::SkillInputBody` / `ai_generation::SkillInput` — one selected
 *  skill sent to the real generation endpoint. `id`/`name` come from the
 *  frontend's own skill registry (`services/skillRegistry.ts`); `description`
 *  is only meaningful for a custom skill the backend doesn't recognize by
 *  name (`ai_generation.rs::resolve_skill_instructions` falls back to it
 *  verbatim) — sending it for a built-in skill too is harmless (ignored). */
export interface GenerateAiSkillInput {
  id: string;
  name: string;
  description?: string;
}

/** `api.rs::GenerateAiTestCasesBody` — `POST .../test-cases/generate-ai`
 *  request. No other field exists on this real endpoint; category/depth/
 *  document/app-source context is not a separate wire field (the backend
 *  has none) — a caller that wants those to matter folds them into
 *  `requirement_text` as plain instruction text, same as any other
 *  requirement detail. */
export interface GenerateAiTestCasesRequest {
  requirement_text: string;
  skills: GenerateAiSkillInput[];
}

/** `ai_generation.rs::DraftStep`. */
export interface GeneratedTestCaseDraftStep {
  action: string;
  expected_result: string;
}

/** `ai_generation.rs::EvidenceKind` (`#[serde(rename_all = "lowercase")]`). */
export type GenerationEvidenceKind = 'evidenced' | 'inferred' | 'exploratory';

/** `ai_generation.rs::GeneratedTestCaseDraft` — one real, un-persisted AI
 *  output case. `source_reference`, when present, is a real, backend-
 *  verified substring of the actual `requirement_text` sent (`ai_generation.
 *  rs::enforce_traceability` strips anything that is not) — never trust a
 *  claim of "evidenced" without also checking this field is non-null. */
export interface GeneratedTestCaseDraft {
  title: string;
  category: string;
  skill_id: string;
  preconditions: string[];
  steps: GeneratedTestCaseDraftStep[];
  expected_result: string;
  priority: string;
  severity: string;
  evidence: GenerationEvidenceKind;
  source_reference: string | null;
}

/** `api.rs::GenerateAiTestCasesResponse`. `generation_method` is always
 *  exactly `"ai"` or `"fallback"` — `provider`/`model` are only ever
 *  present alongside `"ai"`; `fallback_reason` only alongside `"fallback"`.
 *  A `"fallback"` response is a normal `200`, not an error — it is the
 *  endpoint honestly reporting that no compatible AI model produced a
 *  result (`test_cases` is then always empty). */
export interface GenerateAiTestCasesResponse {
  test_cases: GeneratedTestCaseDraft[];
  generation_method: 'ai' | 'fallback';
  provider: string | null;
  model: string | null;
  fallback_reason: string | null;
}

// ---------------------------------------------------------- Phase 3: Custom Testing

/** `api.rs::CustomTestingGenerateBody` — `POST .../custom-testing/generate`
 *  request. `scope` is real, user-provided free text (the `Project`
 *  resource has no module/feature metadata — confirmed in `store.rs`, see
 *  the Phase 3 report §13) — never a fabricated project module. */
export interface CustomTestingGenerateRequest {
  requirement_text: string;
  scope?: string;
  skills: GenerateAiSkillInput[];
}

/** `custom_testing.rs::CustomTestingAnalysis` — a real, structured test
 *  strategy produced by `AiTask::RequirementAnalysis` (the existing router
 *  task, newly wired up this phase — not a new router). Every field is
 *  real model output, schema-validated server-side; never a fabricated or
 *  hardcoded strategy. */
export interface CustomTestingAnalysis {
  objective: string;
  normalized_scope: string;
  recommended_skills: string[];
  recommended_testing_types: string[];
  risk_areas: string[];
  assumptions: string[];
  prerequisites: string[];
  limitations: string[];
  generation_approach: string;
  diagnostics: string[];
}

/** `api.rs::CustomTestingGenerateResponse`. `generation_source` is always
 *  exactly `"REAL_AI"` or `"UNAVAILABLE"` — there is no deterministic
 *  local-fallback strategy generator in this codebase, so a
 *  `"LOCAL_FALLBACK"` value is never sent (see the Phase 3 report §6/§19).
 *  `analysis` is present whenever the real strategy analysis call
 *  succeeded, even if the subsequent case-generation call then failed — a
 *  genuinely-produced strategy is never discarded just because case
 *  generation didn't also succeed. */
export interface CustomTestingGenerateResponse {
  generation_source: 'REAL_AI' | 'UNAVAILABLE';
  analysis: CustomTestingAnalysis | null;
  test_cases: GeneratedTestCaseDraft[];
  provider: string | null;
  model: string | null;
  failure_reason: string | null;
}

// ---------------------------------------------------- AI provider intelligence

/** `ai_router.rs::ProviderType`. Extensible on the backend beyond these 4
 *  values without a frontend change (this type is only ever read, never
 *  matched exhaustively as a discriminated union here). */
export type AiProviderType = 'LOCAL' | 'API' | 'OPENAI_COMPATIBLE' | 'CUSTOM';

/** `ai_router.rs::ProviderStatus` — every value the real discovery pass
 *  can honestly produce. Never collapse this to a single "Connected"
 *  boolean in the UI (brief §13). */
export type AiProviderStatus = 'NOT_CONFIGURED' | 'DISCOVERING' | 'AVAILABLE' | 'HEALTHY' | 'DEGRADED' | 'UNAVAILABLE' | 'ERROR';

/** `ai_router.rs::ModelStatus`. `UNKNOWN` is the honest default before a
 *  model has ever actually been health-checked — never rendered as
 *  "Healthy" (brief §14). */
export type AiModelStatus = 'AVAILABLE' | 'HEALTHY' | 'DEGRADED' | 'UNAVAILABLE' | 'UNKNOWN';

/** `ai_router.rs::CapabilityLevel` — a real, evidenced level, never
 *  invented. `UNKNOWN` means the discovery source gave no signal either
 *  way (brief §5: "UNKNOWN is preferable to guessing"). */
export type AiCapabilityLevel = 'UNKNOWN' | 'NONE' | 'BASIC' | 'STRONG';

export interface AiModelCapabilities {
  text_generation: AiCapabilityLevel | null;
  structured_output: AiCapabilityLevel | null;
  reasoning: AiCapabilityLevel | null;
  vision: AiCapabilityLevel | null;
  embedding: AiCapabilityLevel | null;
  tool_use: AiCapabilityLevel | null;
  long_context: AiCapabilityLevel | null;
}

/** `ai_router.rs::ModelDefinition`. `family`/`parameter_size`/
 *  `context_window` are `null` whenever the real discovery source gave no
 *  evidence for them — never guessed. */
export interface AiModelDefinition {
  id: string;
  provider_id: string;
  display_name: string;
  family: string | null;
  parameter_size: string | null;
  context_window: number | null;
  capabilities: AiModelCapabilities;
  status: AiModelStatus;
}

/** `ai_router.rs::AiProviderInfo` — the real, current discovery snapshot
 *  for one provider. Deliberately has no credential-shaped field at all
 *  (brief §6/§22): `configured` is a boolean fact, never the secret
 *  itself. */
export interface AiProviderInfo {
  id: string;
  name: string;
  provider_type: AiProviderType;
  endpoint: string | null;
  configured: boolean;
  reachable: boolean;
  status: AiProviderStatus;
  models: AiModelDefinition[];
}

export type AiRoutingMode = 'AUTO' | 'MANUAL';

/** `ai_router.rs::AiSettings` — the real, persisted routing configuration
 *  (`GET`/`PUT /api/v1/ai/settings`). */
export interface AiSettings {
  mode: AiRoutingMode;
  preferred_provider: string | null;
  preferred_model: string | null;
  fallback_enabled: boolean;
}

/** `ai_router.rs::RoutingDecision` — a documented heuristic score, not a
 *  benchmarked accuracy metric (brief §10). */
export interface AiRoutingDecision {
  provider_id: string;
  model_id: string;
  routing_score: number;
  reason: string;
}

/** `GET /api/v1/ai/status` response shape. `routing_preview` is `null`
 *  when no compatible model currently exists for TEST_CASE_GENERATION —
 *  never fabricated. */
export interface AiStatusResponse {
  providers: AiProviderInfo[];
  settings: AiSettings;
  routing_preview: AiRoutingDecision | null;
}

// ------------------------------------------------------------------ rbac

/** `api.rs::WhoAmIResponse` — the caller's own real identity as resolved
 *  from the presented key. `name` is the API key's label, not a user
 *  account: this backend has no user/profile concept. */
export interface WhoAmIResponse {
  name: string;
  role: Role;
  /** `null` = global scope (sees every project). */
  project_id: string | null;
  /** The real role ordering `auth.rs::Role::at_least` enforces, least to
   *  most privileged. Served by the backend so this taxonomy is never
   *  duplicated (and never drifts) on the client. */
  role_hierarchy: Role[];
}

/** `store.rs::ApiKeyListRow` — `GET /admin/api-keys` (Admin only). Only the
 *  key's metadata is ever returned; the key itself is shown exactly once at
 *  creation and only its SHA-256 hash is stored. */
export interface ApiKeyListRow {
  id: string;
  name: string;
  role: Role;
  project_id: string | null;
  created_at: string;
  revoked_at: string | null;
}

// --------------------------------------------------------------- mobile

/** `mobile.rs::MobileDevice` — a real device or emulator `adb` can see. */
export interface MobileDevice {
  serial: string;
  /** Real adb state verbatim: `device` | `offline` | `unauthorized` | ... */
  state: string;
  model: string | null;
  /** Only `state === 'device'` can actually run a test. */
  ready: boolean;
  emulator: boolean;
}

/** `mobile.rs::AvdDefinition` — an AVD that exists and could be booted. A
 *  definition, not a running device. */
export interface AvdDefinition {
  name: string;
  running: boolean;
}

/** `mobile.rs::IosDevice` — a real iOS Simulator (macOS only) or a real
 *  physically attached iPhone/iPad (via libimobiledevice). Kept separate
 *  from `MobileDevice` because the two platforms have genuinely different
 *  identifiers, states and tooling. */
export interface IosDevice {
  udid: string;
  name: string;
  /** Real runtime/OS version when the tool reports one. */
  runtime: string | null;
  /** Real state verbatim: simctl's `Booted`/`Shutdown`, or `Connected`. */
  state: string;
  ready: boolean;
  simulator: boolean;
}

/** `mobile.rs::MobileToolStatus` — real presence/version of one CLI. */
export interface MobileToolStatus {
  path: string | null;
  available: boolean;
  /** Real version string, or the real reason it is unusable. */
  detail: string;
}

/** `GET /api/v1/mobile/fleet`. */
export interface MobileFleet {
  devices: MobileDevice[];
  avds: AvdDefinition[];
  adb: MobileToolStatus;
  emulator_tool: MobileToolStatus;
  maestro: MobileToolStatus;
  /** Real iOS devices — Simulators and/or attached hardware. */
  ios_devices: IosDevice[];
  /** `xcrun simctl`. Unavailable off macOS, with that as the real reason. */
  simctl: MobileToolStatus;
  /** libimobiledevice — the only real path to a physical iPhone off macOS. */
  libimobiledevice: MobileToolStatus;
  /** Real host OS, so platform limits can be stated truthfully. */
  host_os: string;
}

// ------------------------------------------------------------- pipeline

/** `api.rs::PipelineStage` — one stage of the real execution pipeline.
 *  `available: false` is a first-class, honestly-reported state: the stage
 *  is listed with the real reason it does not exist, rather than hidden
 *  (which would imply completeness) or animated (which would fabricate it). */
export interface PipelineStage {
  id: string;
  label: string;
  /** The real module/route implementing it, or why it is absent. */
  backing: string;
  available: boolean;
  metric: number | null;
  metric_label: string | null;
}

/** `GET /api/v1/projects/:id/pipeline`. */
export interface NeuralPipelineResponse {
  stages: PipelineStage[];
  graph_nodes: number;
  graph_relations: number;
  active_runs: number;
  completed_runs: number;
}

// ---------------------------------------------------------------- orchestrator

/** `api.rs::ExecutorRoute` — one real executor kind the scheduler can route
 *  a task to, plus the real policy gate applied to it. */
export interface ExecutorRoute {
  kind: string;
  gate: 'AUTO' | 'REQUIRES_APPROVAL' | 'BLOCKED';
}

/** `api.rs::ActiveRunView` — a run this backend process is actively
 *  driving, with its real live task states (the live DAG). Only in-process
 *  runs appear; finished runs from a previous process are history, served
 *  by the project-scoped execution endpoints instead. */
export interface ActiveRunView {
  run_id: string;
  name: string;
  project_id: string | null;
  state: string;
  concurrency: number;
  tasks: TaskState[];
}

/** `GET /api/v1/orchestrator/state` — the real coordination surface:
 *  capability routing table, enforced policy limits, and the live DAG. */
export interface OrchestratorStateResponse {
  executors: ExecutorRoute[];
  max_concurrency: number;
  active_runs: ActiveRunView[];
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

// ------------------------------------------------------------- orchestrator logs

/** `log_bus.rs::OrchestratorLogEntry` — structured live log entry emitted by
 *  the real AIQA Orchestrator runtime. */
export interface OrchestratorLogEntry {
  id: string;
  timestamp: string; // ISO 8601
  level: string; // 'INFO' | 'WARN' | 'ERROR' | 'DEBUG' | 'SUCCESS'
  source: string; // 'ai_router' | 'generation' | 'validation' | 'execution' | 'executor' | 'policy' | 'store' | 'engine' | 'auth' | etc.
  message: string;
  run_id?: string;
  project_id?: string;
  task_id?: string;
}

