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

// --------------------------------------------------------- custom AI providers

/** `store.rs::CustomAiProviderRow`'s real, public-safe view
 *  (`api.rs::CustomAiProviderView`) — a user-added AI provider, persisted
 *  and live for real routing on the very next discovery cycle (no
 *  restart). Deliberately carries no `api_key` field at all — the real
 *  backend row does hold one, but it is never serialized into this
 *  response (see that struct's own doc comment), so there is nothing here
 *  to accidentally render. */
export interface CustomAiProviderView {
  id: string;
  name: string;
  base_url: string;
  /** USD per 1,000 tokens in / out, as entered when the provider was added.
   *  `null` (or absent, from an older backend) when no price was entered —
   *  its cost then shows as free. */
  input_usd_per_1k?: number | null;
  output_usd_per_1k?: number | null;
  /** When the key was given as `env:NAME`: the variable's name (not a
   *  secret) and whether the server has it set right now. `null` when the
   *  key itself is stored. */
  api_key_env?: string | null;
  api_key_env_set?: boolean | null;
  created_by: string;
  created_at: string;
}

export interface CreateCustomAiProviderRequest {
  name: string;
  base_url: string;
  /** The key itself, or `env:NAME` to read it from an environment variable
   *  on the server instead of storing it. */
  api_key: string;
  /** What the provider charges, USD per 1,000 tokens. Both or neither. */
  input_usd_per_1k?: number;
  output_usd_per_1k?: number;
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

/** `api.rs::RetryFailedTasksResponse` — real re-planning v1. `run_id`/
 *  `state` describe the real NEW run created to retry
 *  `retried_task_ids` — the real, non-empty subset of the original run's
 *  own tasks that ended INFRA_ERROR/TIMEOUT (never a real FAILED test;
 *  see the backend handler's own doc comment for why). */
export interface RetryFailedTasksResponse {
  run_id: string;
  state: RunState;
  retried_task_ids: string[];
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

/** `api.rs::AuditLogResponse` — real pagination envelope for
 *  `GET .../projects/:id/audit`. `total` is a real `COUNT(*)`
 *  (`store.rs::count_audit`), not derived from `rows.length` — a caller
 *  can show a real "X-Y of Z" and correctly disable "next" exactly at the
 *  real end, not one batch-length short or long. The legacy global
 *  `/admin/audit-log` route keeps its original bare-`AuditRow[]` shape,
 *  unrelated to this type. */
export interface AuditLogResponse {
  rows: AuditRow[];
  total: number;
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
  /** Phase 5 — with `compile_and_execute`: compile against this finished
   *  crawl run (see `CompileRequest`). Ignored without it. */
  crawl_run_id?: string;
  /** Phase 5 — with `crawl_run_id`: the crawl's role the case runs as. */
  role?: string;
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
export type CompilationOutcome = (
  | { status: 'READY'; test_case_id: string; execution_type: PlanExecutionType; target: string | null; actions: PlanAction[]; compiler: PlanCompilerMethod; diagnostics: string[]; provider: string | null; model: string | null }
  | { status: 'NOT_EXECUTABLE'; reasons: string[] }
  | { status: 'COMPILATION_FAILED'; reasons: string[] }
  | { status: 'COMPILATION_UNAVAILABLE'; reason: string }
) & {
  /** Phase 5 — present only for a compile that named a crawl. */
  grounding?: CompileGrounding;
};

/** `grounding.rs::GroundingReport` — what a compile against a crawl was
 *  given. `secrets` are the *names* of the environment variables the login
 *  reads when the test runs (never a value). */
export interface CompileGrounding {
  /** The crawl's role the case signs in as; `null` when it runs without signing in. */
  role: string | null;
  pages_in_crawl: number;
  pages_offered: number;
  elements_offered: number;
  login_steps: number;
  secrets: string[];
}

/** `POST .../test-cases/:id/compile` body (Phase 5) — optional. Naming a
 *  finished crawl grounds the compile in it: the plan may use only selectors
 *  that crawl found, and `role`'s login runs first. */
export interface CompileRequest {
  crawl_run_id?: string;
  /** The crawl's role the case runs as; omitted, the crawl's role that never logged in. */
  role?: string;
}

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

/** `api.rs::ExtractDocumentTextBody` — `POST .../documents/extract-text`.
 *  `content_base64` is the real, unmodified file's raw bytes, base64-encoded
 *  client-side (`File.arrayBuffer()` -> base64) — the file's content never
 *  needs to be parsed in the browser; the real parsing happens server-side
 *  in `doc_extract.rs`. */
export interface ExtractDocumentTextRequest {
  file_name: string;
  content_base64: string;
  /** Which upload slot the file came from (BRD / PRD / FRD / SRD /
   *  ADDITIONAL). It only prefixes the ids given to the document's
   *  requirements; omitted, the file name is used instead. */
  document_type?: string;
}

/** `doc_extract.rs::ExtractedDocument` — the real, extracted text of one
 *  uploaded Product Documentation file. `truncated` is only ever true when
 *  the real document exceeded the server's cap (20,000 characters) and was
 *  genuinely cut off — never set for any other reason.
 *
 *  `text`/`char_count`/`truncated` describe that bounded text only. The whole
 *  document is read as well: `requirements_document` says what was found in
 *  all of it (absent when it could not be stored — the bounded `text` above
 *  is then all there is, exactly as before). */
export interface ExtractDocumentTextResponse {
  text: string;
  char_count: number;
  truncated: boolean;
  requirements_document?: RequirementDocumentSummary;
}

/** `api.rs::RequirementKind` (`requirements.rs`, snake_case on the wire). */
export type RequirementKind =
  | 'functional'
  | 'security'
  | 'performance'
  | 'usability'
  | 'interface'
  | 'data'
  | 'context'
  | 'reference';

/** `coverage.rs::Technique` (snake_case on the wire) — one way of testing a
 *  requirement. */
export type CoverageTechnique = 'positive' | 'negative' | 'edge' | 'security' | 'performance' | 'accessibility';

/** How much coverage a request asks for — the page's Exploration Depth. */
export type CoverageDepth = 'SMOKE' | 'STANDARD' | 'EXHAUSTIVE' | 'CHAOS_EXPLORATORY';

/** `api.rs::RequirementDocumentSummary` — what reading one whole document
 *  produced. `full_char_count` is the whole document (the `text` beside it
 *  stops at 20,000); `full_truncated` is true only past a 3,000,000-character
 *  safety ceiling. `skipped_sections` are sections left out on purpose (a
 *  table of contents, a revision history). */
export interface RequirementDocumentSummary {
  document_id: string;
  full_char_count: number;
  full_truncated: boolean;
  sections_count: number;
  requirements_count: number;
  skipped_sections: string[];
  by_kind: Partial<Record<RequirementKind, number>>;
}

/** `store.rs::RequirementDocumentRow` — a document read for its requirements. */
export interface RequirementDocumentRecord {
  id: string;
  file_name: string;
  doc_type: string | null;
  char_count: number;
  requirements_count: number;
  sections_count: number;
  skipped_sections: string[];
  furniture_lines: number;
  created_at: string;
  updated_at: string;
}

/** `store.rs::RequirementRow` — one numbered requirement read out of a
 *  document. `key` is the id people see and a generated test case carries. */
export interface RequirementRecord {
  document_id: string;
  key: string;
  ordinal: number;
  section: string | null;
  kind: RequirementKind | string;
  text: string;
}

/** `GET .../requirements[?document_ids=a,b]`. */
export interface ListRequirementsResponse {
  documents: RequirementDocumentRecord[];
  requirements: RequirementRecord[];
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
 *  request. category/depth/document/app-source context is not a separate
 *  wire field (the backend has none) — a caller that wants those to matter
 *  folds them into `requirement_text` as plain instruction text, same as
 *  any other requirement detail. `crawl_run_id` is the one real, optional
 *  exception: a previously completed crawl run
 *  (`crawl.start` -> `crawl.result` below) whose real discovered pages/
 *  elements/visibility-matrix should ground the prompt
 *  (`ai_generation::build_crawl_context_block`). Omitted/`null` behaves
 *  exactly as generation always has. */
export interface GenerateAiTestCasesRequest {
  requirement_text: string;
  skills: GenerateAiSkillInput[];
  crawl_run_id?: string | null;
  /** Generate from the numbered requirements read out of these documents
   *  (`ExtractDocumentTextResponse.requirements_document.document_id`)
   *  instead of from pasted document text: each skill is shown only the
   *  requirements it tests, in batches that fit the provider, and every case
   *  names the requirement it came from. Omitted or empty, the request is
   *  generated exactly as it always was. */
  requirement_document_ids?: string[];
  /** Phase 4 — with `requirement_document_ids`: the depth asked for. It turns
   *  on the coverage report (which techniques each requirement was tested
   *  with, against that depth's targets). Ignored without documents. */
  depth?: CoverageDepth;
  /** Phase 4 — after the first pass, make extra calls for the requirements
   *  that are short of the depth's targets. Needs `depth`. */
  coverage_top_up?: boolean;
}

/** `POST .../test-cases/generate-ai/plan` request — the same documents and
 *  skills a generation would use; nothing is generated. */
export interface GenerateAiPlanRequest {
  skills: GenerateAiSkillInput[];
  requirement_document_ids: string[];
  /** Phase 4 — with a depth, the plan also says which techniques would be
   *  covered and what a top-up may add. */
  depth?: CoverageDepth;
  coverage_top_up?: boolean;
}

/** `coverage.rs::TechniquePlan` — one technique's line in the plan. */
export interface CoverageTechniquePlan {
  technique: CoverageTechnique;
  /** Requirements it has a target for at this depth. */
  requirements: number;
  /** Cases each of those should get. */
  per_requirement: number;
  /** The selected skill that provides it; `null` when none does (it is then
   *  never attempted). */
  provided_by: string | null;
}

/** `coverage.rs::CoveragePlan` — what a depth would cover, worked out before
 *  any model is called. `top_up_call_limit` is what is left of the request's
 *  call limit after the first pass. */
export interface CoveragePlan {
  depth: CoverageDepth;
  techniques: CoverageTechniquePlan[];
  top_up: boolean;
  top_up_rounds: number;
  top_up_call_limit: number;
}

/** `requirement_generation.rs::SkillPlan` — what one skill would be shown.
 *  `note` is set when it would be skipped (nothing to test). */
export interface GenerateAiPlanSkill {
  skill_name: string;
  wave: string;
  requirements: number;
  batches: number;
  note?: string;
}

/** `api.rs::GeneratePlanResponse` — what a requirement-driven generation
 *  would do, worked out by the same planning the run uses without calling any
 *  model. `calls > max_calls` (`within_limit: false`) is refused by the
 *  generation endpoint before any call is made. The token figures are rough
 *  sizes, not a quote. `provider_name`/`model`/`prompt_window_tokens` are
 *  absent when no provider can serve generation right now. */
export interface GenerateAiPlanResponse {
  requirements_total: number;
  by_kind: Partial<Record<RequirementKind, number>>;
  /** Requirements at least one selected skill will be shown. */
  offered: number;
  /** Of a kind no selected skill tests (at most 100 ids). */
  not_offered_keys: string[];
  calls: number;
  max_calls: number;
  within_limit: boolean;
  skills: GenerateAiPlanSkill[];
  budget_chars: number;
  provider_name: string | null;
  model: string | null;
  prompt_window_tokens: number | null;
  estimated_prompt_tokens: number;
  estimated_completion_tokens: number;
  /** Present only when the request named a depth. */
  coverage?: CoveragePlan;
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
  /** Requirement-driven generation only: the id
   *  (`RequirementRecord.key`) of the requirement this case tests. Absent
   *  when the case could not be tied to one requirement, and always absent
   *  for a request generated from pasted text. */
  requirement_id?: string | null;
  /** Requirement-driven generation only: the technique the case counts for
   *  (`CoverageTechnique`) — the model's own label, else what its skill is
   *  about. Absent for a request generated from pasted text. */
  technique?: CoverageTechnique | string | null;
}

/** `ai_generation::WaveSummary` — one selected skill's real generation call.
 *  `failure_reason` is set (and `case_count` is 0) when that call failed; the
 *  other skills' cases are still returned. */
export interface GenerationWaveSummary {
  wave: string;
  skill_names: string[];
  case_count: number;
  provider: string | null;
  model: string | null;
  routing_score: number | null;
  fallback_used: boolean | null;
  routing_mode: string | null;
  prompt_tokens: number | null;
  completion_tokens: number | null;
  estimated_cost_usd: number | null;
  latency_ms: number | null;
  failure_reason: string | null;
  skipped: boolean;
  /** Requirement-driven generation only: which batch of the skill's
   *  requirements this call carried (`"2/5"`) and how many requirements were
   *  in it. */
  batch?: string;
  requirements?: number;
  /** Coverage top-up calls only (`wave` is then `"top_up"`, and `batch` is the
   *  call's place among its round's calls): the technique the call was for. */
  technique?: string;
}

/** `coverage.rs::TechniqueSummary` — one technique across the whole
 *  requirement set. `provided_by` is `null` when no selected skill provides
 *  it: it is then never attempted, and `short` is what selecting one would
 *  address. */
export interface CoverageTechniqueSummary {
  technique: CoverageTechnique;
  /** Requirements the technique has a target for at this depth. */
  applicable: number;
  met: number;
  short: number;
  provided_by: string | null;
}

export interface CoverageMissingTechnique {
  technique: CoverageTechnique;
  have: number;
  target: number;
}

/** `coverage.rs::RequirementGap` — a requirement below target for a technique
 *  a selected skill provides. */
export interface CoverageRequirementGap {
  key: string;
  kind: string;
  missing: CoverageMissingTechnique[];
}

/** `coverage.rs::TopUpSummary` — what a coverage top-up did. */
export interface CoverageTopUpSummary {
  rounds: number;
  calls: number;
  cases_added: number;
  /** Why it stopped, in words ("every target is met", "the call limit was
   *  reached", ...). */
  stopped: string;
}

/** `coverage.rs::CoverageMatrix` — which techniques each requirement was
 *  tested with, against the depth asked for. `gaps` lists at most 200
 *  requirements; `gap_total` is the real count. */
export interface CoverageMatrix {
  depth: CoverageDepth;
  techniques: CoverageTechniqueSummary[];
  gaps: CoverageRequirementGap[];
  gap_total: number;
  /** Requirements that reached the target for every technique that applies. */
  complete: number;
  /** Requirements no technique applies to (background text, reference). */
  not_applicable: number;
  /** Cases tied to no requirement, which count for none. */
  unlinked_cases: number;
  top_up?: CoverageTopUpSummary;
}

/** `requirement_generation.rs::RequirementCoverage` — how much of the
 *  requirement set a requirement-driven run reached. `uncovered_keys` were
 *  shown to a skill but produced no case (a failed call, or nothing to test);
 *  `not_offered_keys` are of a kind no selected skill tests. Both list at
 *  most 100 ids. */
export interface RequirementCoverage {
  total: number;
  offered: number;
  covered: number;
  uncovered_keys: string[];
  not_offered_keys: string[];
  notes: string[];
  calls: number;
  budget_chars: number;
  /** Present only when the request named a depth. */
  matrix?: CoverageMatrix;
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
  /** The real per-skill breakdown, one entry per skill call that was made. */
  waves?: GenerationWaveSummary[];
  /** How many real, distinct pages from `crawl_run_id`'s crawl actually
   *  reached the prompt (0 when no `crawl_run_id` was supplied) — an honest
   *  confirmation of real grounding, not a claim to take on faith. */
  crawl_pages_used: number;
  /** Vector Store + RAG Retrieval v1 — how many of this project's own
   *  similar, already-APPROVED test cases were genuinely retrieved and
   *  folded into the prompt as grounding examples (0 for a project with
   *  none embedded yet, or when no embedding provider is configured —
   *  never a claim of grounding that did not really happen). */
  similar_cases_used: number;
  /** Phase 2 gap-closing (document-level RAG) — how many of this project's
   *  own previously-uploaded requirement documents were genuinely retrieved
   *  and folded into the prompt as background/source material (0 for a
   *  project with none embedded yet, or when no embedding provider is
   *  configured — never a claim of grounding that did not really happen). */
  similar_documents_used: number;
  /** Real, summed token/cost/latency across every wave that actually ran
   *  (`ai_generation::aggregate_wave_usage`). The token/cost totals are
   *  `null` only when at least one wave that ran has a genuinely unknown
   *  figure of its own — never a silently-undercounted partial sum. */
  total_prompt_tokens: number | null;
  total_completion_tokens: number | null;
  total_estimated_cost_usd: number | null;
  total_latency_ms: number;
  /** Present only when the request named `requirement_document_ids`. */
  requirements?: RequirementCoverage;
}

// ---------------------------------------------------------------- crawler

/** `crawler.rs::CrawlRole`. Empty `login_actions` crawls anonymously.
 *  `login_actions` reuses `PlanAction` (`execution_plan::Action`'s own real
 *  wire shape) — the backend restricts it to NAVIGATE/FILL/CLICK only
 *  (`crawler::validate_crawl_plan`), the same closed vocabulary a compiled
 *  test case's browser actions already use. */
export interface CrawlRole {
  name: string;
  login_actions: PlanAction[];
}

/** `api.rs::CrawlRequestBody` — `POST .../crawl` request. */
export interface StartCrawlRequest {
  base_url: string;
  roles: CrawlRole[];
  /** Visit first the pages the project's requirements are about
   *  (`gap_check::focus_terms`); off unless set. */
  focus_requirements?: boolean;
}

/** `api.rs::CrawlStartedResponse`. The crawl runs through the exact same
 *  run/task pipeline every other browser task does — poll it via
 *  `executions.get(projectId, run_id)` like any other run, then fetch the
 *  real result via `crawl.result()` below once it reaches a terminal
 *  state. */
export interface CrawlStartedResponse {
  run_id: string;
}

/** `crawler.rs::CrawledElement` — one real element a real browser actually
 *  saw on a real crawled page. */
export interface CrawledElement {
  kind: string;
  text: string;
  selector_hint: string;
  href?: string | null;
}

/** `crawler.rs::CrawledPage`. */
export interface CrawledPage {
  url: string;
  title: string;
  elements: CrawledElement[];
}

/** `crawler.rs::RoleCrawlResult` — one role's real crawl outcome.
 *  `login_succeeded: false` means every page after that role's login step
 *  is honestly absent, not fabricated. */
export interface RoleCrawlResult {
  role: string;
  login_succeeded: boolean;
  login_error?: string | null;
  pages: CrawledPage[];
}

/** `crawler.rs::VisibilityDiffEntry` — a real, observed per-URL
 *  access-control boundary: which real elements on this real page were
 *  only ever seen by some of the crawled roles. */
export interface VisibilityDiffEntry {
  url: string;
  roles_with_access: string[];
  elements_only_visible_to: Record<string, string[]>;
}

/** `crawler.rs::CrawlOutput` — `GET .../crawl/:run_id/result`'s real,
 *  structured response, only ever present once the crawl run reaches a
 *  terminal state and genuinely wrote it. */
export interface CrawlOutput {
  roles: RoleCrawlResult[];
  visibility_matrix: VisibilityDiffEntry[];
}

// ------------------------------------------------------------- gap check

/** `gap_check.rs::Status` — how well the application shows a requirement. */
export type GapStatus = 'FOUND' | 'WEAK' | 'MISSING' | 'NOT_CHECKABLE';

/** `gap_check.rs::Basis` — what a page match rests on: words the requirement and
 *  the page share (`text`), a page close in meaning to it (`embedding`), or a page
 *  the AI read and agreed with (`ai`). */
export type GapBasis = 'text' | 'embedding' | 'ai';

/** `gap_check.rs::PageMatch` — one page a requirement points at. */
export interface GapPageMatch {
  url: string;
  title: string;
  /** How much of what the requirement is about the page carries, 0 to 1. */
  score: number;
  basis: GapBasis;
  /** The requirement's words found on the page. */
  terms: string[];
  /** Up to three things on the page that carry them. */
  elements: string[];
  roles: string[];
  /** For an `embedding` match: how close, 0 to 1. */
  similarity?: number;
}

/** `gap_check.rs::CaseCoverage` — a requirement's saved cases (rejected ones count for nothing). */
export interface GapCaseCoverage {
  total: number;
  approved: number;
  executed: number;
  techniques: Record<string, number>;
  /** Techniques that apply to this kind of requirement and have no case. */
  missing_techniques: string[];
}

/** `gap_verify.rs::Verification` — what the AI said about a requirement it was asked to check. */
export interface GapVerification {
  verdict: 'CONFIRMED' | 'NOT_FOUND' | 'UNCLEAR' | 'FAILED';
  /** The addresses of the pages it named (only ones it was shown). */
  pages: string[];
  reason: string;
  /** `provider/model`. */
  by: string;
}

/** `gap_check.rs::RequirementFinding`. */
export interface GapRequirement {
  key: string;
  section: string | null;
  kind: string;
  text: string;
  status: GapStatus;
  basis?: GapBasis;
  matches: GapPageMatch[];
  verification?: GapVerification;
  cases: GapCaseCoverage;
}

/** `gap_check.rs::UndocumentedPage` — a page no requirement points at. */
export interface GapUndocumentedPage {
  url: string;
  title: string;
  roles: string[];
  elements: string[];
  element_count: number;
}

/** `gap_check.rs::GapSummary`. */
export interface GapSummary {
  requirements: number;
  /** Requirements a crawl can show: functional, usability and data ones. */
  checkable: number;
  found: number;
  weak: number;
  missing: number;
  not_checkable: number;
  pages: number;
  undocumented_pages: number;
  cases: number;
  with_cases: number;
  /** Requirements the application shows that no case tests. */
  shown_without_cases: number;
  /** Requirements with cases that the crawl found no sign of. */
  tested_without_evidence: number;
}

/** `api.rs::GapCrawlInfo`. */
export interface GapCrawlInfo {
  run_id: string;
  /** `grounding` (links, buttons and form fields) or `result` (links and buttons only: an older crawl). */
  source: 'grounding' | 'result';
  role: string | null;
  roles: { role: string; login_succeeded: boolean; pages: number }[];
}

/** `gap_verify.rs::EmbeddingReport` — what the optional embedder step did. */
export interface GapEmbeddingReport {
  requested: boolean;
  used: boolean;
  model: string | null;
  /** Texts sent to the embedder, and how many of those failed. */
  texts: number;
  failed: number;
  note: string | null;
}

/** `gap_verify.rs::VerificationReport` — what the optional AI step did. */
export interface GapVerificationReport {
  requested: boolean;
  calls: number;
  checked: number;
  confirmed: number;
  not_found: number;
  unclear: number;
  failed: number;
  /** The `provider/model` that answered. */
  by: string[];
  note: string | null;
}

/** `POST .../gap-check` request — every field optional. */
export interface GapCheckRequest {
  /** A crawl run of this project; omitted, its newest one. */
  crawl_run_id?: string;
  /** Only the pages this role's crawl reached. */
  role?: string;
  /** Only these documents' requirements. */
  document_ids?: string[];
  /** Also compare meaning with the embedder (needs `GEMINI_API_KEY`). */
  use_embeddings?: boolean;
  /** Also ask the AI to check the requirements the words left unclear. */
  verify?: boolean;
  max_verifications?: number;
}

/** `api.rs::GapCheckResponse` — `POST .../gap-check`. */
export interface GapCheckResponse {
  crawl: GapCrawlInfo;
  summary: GapSummary;
  requirements: GapRequirement[];
  undocumented_pages: GapUndocumentedPage[];
  /** What to know when reading the report (a role that could not log in, an older crawl). */
  notes: string[];
  method?: { embeddings: GapEmbeddingReport; verification: GapVerificationReport };
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
  /** Real billing fact (`ai_router.rs::ProviderPricing`) — never a guessed
   *  rate. `UNMETERED` for a provider with genuinely no per-token bill
   *  (both providers wired today run against this deployment's own local
   *  compute); `METERED` only once a real, disclosed rate is configured. */
  pricing: AiProviderPricing;
}

export type AiProviderPricing =
  | { kind: 'UNMETERED' }
  | { kind: 'METERED'; input_usd_per_1k: number; output_usd_per_1k: number };

export type AiRoutingMode = 'AUTO' | 'MANUAL';

/** `ai_router.rs::AiSettings` — the real, persisted routing configuration
 *  (`GET`/`PUT /api/v1/ai/settings`). */
export interface AiSettings {
  mode: AiRoutingMode;
  preferred_provider: string | null;
  preferred_model: string | null;
  fallback_enabled: boolean;
  /** Real, explicit per-task routing rules (`ai_router.rs::AiSettings::
   *  task_overrides`), keyed by the real `AiTask` variant name — e.g.
   *  "always use a real paid API for `TEST_CASE_GENERATION` once one is
   *  configured, leave every other task on the default/global
   *  preference." Optional here (`#[serde(default)]` on the backend) —
   *  absent/empty means no overrides, identical to today's behavior. */
  task_overrides?: Record<string, TaskProviderPreference>;
}

/** `ai_router.rs::TaskProviderPreference` — each field independently
 *  optional: pin just the provider (any of its models) or just the model
 *  (whichever provider serves it) for one specific task. `chain` is an
 *  ordered list of exact provider/model pairs: the first is the primary,
 *  the rest are tried in order if it fails, and nothing outside the list is
 *  ever used for that task. Absent/empty = the usual routing. */
export interface TaskProviderPreference {
  provider: string | null;
  model: string | null;
  chain?: TaskTarget[];
}

/** `ai_router.rs::TaskTarget` — one exact provider + model. */
export interface TaskTarget {
  provider: string;
  model: string;
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

/** Phase 5 gap-closing — `api.rs::CreateApiKeyBody`, `POST /admin/api-keys`
 *  (Admin only). `project_id` omitted/`undefined` means global scope, the
 *  same real meaning `null` carries on `ApiKeyListRow`/`WhoAmIResponse`. */
export interface CreateApiKeyRequest {
  name: string;
  role: Role;
  project_id?: string;
}

/** `api.rs::CreateApiKeyResponse`. `key` is the real plaintext key —
 *  returned exactly once, at creation time; only its SHA-256 hash is ever
 *  persisted, so this value cannot be recovered again after this response. */
export interface CreateApiKeyResponse {
  id: string;
  key: string;
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

// ------------------------------------------------ Phase 6 & 7: Adaptive QA Intelligence

export interface ExecutionTelemetryRow {
  id: string;
  project_id: string;
  run_id: string;
  task_id: string;
  task_name: string;
  test_case_id: string | null;
  execution_id: string | null;
  executor: string;
  environment: string;
  start_time: string;
  end_time: string | null;
  duration_ms: number;
  status: string;
  retry_count: number;
  healing_attempt_count: number;
  failure_category: string | null;
  failure_fingerprint: string | null;
  error_message: string | null;
  evidence_refs: string;
  assertion_result: string | null;
  http_status: number | null;
  selector: string | null;
  verification_outcome: string | null;
  created_at: string;
}

export interface FailureClusterRow {
  id: string;
  project_id: string | null;
  signature: string;
  first_seen_at: string;
  last_seen_at: string;
  occurrence_count: number;
  root_cause_hypothesis: string | null;
  confidence: number | null;
  reproducibility: number | null;
  review_state: string;
}

export interface FlakinessMetricsRow {
  id: string;
  node_id: string;
  flakiness_score: number;
  window_runs: number;
  window_failures: number;
  environment_correlation: string | null;
  computed_at: string;
}

export interface HealingStrategyMetricsRow {
  id: string;
  project_id: string;
  strategy: string;
  failure_category: string;
  attempts_count: number;
  verified_count: number;
  not_verified_count: number;
  regression_count: number;
  calibrated_confidence: number;
  updated_at: string;
}

export type TrendDirection = 'IMPROVING' | 'STABLE' | 'DEGRADING' | 'INSUFFICIENT_DATA';

export interface TrendAnalysis {
  metric_name: string;
  direction: TrendDirection;
  current_value: number;
  previous_value: number;
  sample_count: number;
  notes: string;
}

export interface QualityNodeRow {
  id: string;
  project_id: string | null;
  node_type: string;
  external_ref: string | null;
  name: string;
  metadata: string;
  created_at: string;
}

export interface QualityRelationRow {
  id: string;
  from_node_id: string;
  to_node_id: string;
  relation_type: string;
  metadata: string;
  created_at: string;
}

export interface QualityGraphResponse {
  nodes: QualityNodeRow[];
  relations: QualityRelationRow[];
}

export interface RiskFactors {
  failure_rate: number;
  flakiness_score: number;
  failure_severity: number;
  dependency_count: number;
  healing_regression_rate: number;
  security_sensitivity: number;
}

export interface RiskCalibration {
  task_name: string;
  risk_score: number;
  risk_level: 'CRITICAL' | 'HIGH' | 'MEDIUM' | 'LOW' | 'INSUFFICIENT_DATA' | string;
  factors: RiskFactors;
  explanation: string;
}

// ---------------------------------------------------------- Phase 8: Intelligent QA Operations & Cross-Environment Correlation

export type RecurrenceState = 'NEW' | 'RECURRING' | 'PERSISTENT' | 'RESOLVED';

export type TimelineEventType =
  | 'FAILURE_OBSERVED'
  | 'HEALING_PROPOSED'
  | 'HEALING_APPROVED'
  | 'HEALING_EXECUTED'
  | 'HEALING_VERIFIED'
  | 'REGRESSION_DETECTED';

export interface EnvironmentOccurrence {
  environment: string;
  occurrences: number;
  total_runs: number;
  failure_rate: number;
  first_seen: string;
  last_seen: string;
  affected_tests: string[];
  affected_components: string[];
  trend: string;
}

export interface EnvironmentCorrelationMatrix {
  cluster_id: string;
  signature: string;
  environments: EnvironmentOccurrence[];
  recurrence_state: RecurrenceState;
  cross_environment: boolean;
  total_occurrences: number;
}

export interface InvestigationTimelineEvent {
  id: string;
  timestamp: string;
  event_type: TimelineEventType;
  environment: string;
  description: string;
  evidence_refs: string[];
  metadata?: Record<string, unknown>;
}

export interface HealingHistoryCorrelation {
  proposal_id: string;
  task_id: string;
  strategy: string;
  risk_level: string;
  state: string;
  verification_result?: string | null;
  regression_detected: boolean;
  ast_diff?: string | null;
  created_at: string;
}

export interface FailureInvestigation {
  project_id: string;
  cluster_id: string;
  signature: string;
  failure_category: string;
  root_cause_hypothesis?: string | null;
  confidence?: number | null;
  reproducibility?: number | null;
  environment_matrix: EnvironmentCorrelationMatrix;
  timeline: InvestigationTimelineEvent[];
  affected_tests: string[];
  affected_tasks: string[];
  affected_components: string[];
  healing_history: HealingHistoryCorrelation[];
  regression_status: string;
  non_causal_changes: string[];
  evidence_refs: string[];
  recommendations: string[];
}

export interface RetentionPolicy {
  retention_days: number;
  dry_run: boolean;
}

export interface RetentionReport {
  project_id: string;
  eligible_count: number;
  pruned_count: number;
  dry_run: boolean;
  executed_at: string;
  retention_days: number;
}

// ---------------------------------------------------------- Phase 7: Autopilot

/** `login_suggest.rs::Confidence`. */
export type LoginSuggestionConfidence = 'low' | 'medium' | 'high';

/** `login_suggest.rs::LoginSuggestion` — a role's login steps as proposed from what a crawl saw on the login page.
 *  Every locator is one the crawl recorded; the values are `env:NAME` references, never credentials. */
export interface LoginSuggestion {
  role: string;
  page_url: string;
  page_title: string;
  actions: PlanAction[];
  /** The environment variables the steps read (names only). */
  secrets: string[];
  confidence: LoginSuggestionConfidence;
  /** Why each part was chosen. */
  reasons: string[];
  /** `rules`, or the `provider/model` of the AI that proposed it. */
  by: string;
}

/** `POST .../login-suggestion` request. */
export interface LoginSuggestionRequest {
  role: string;
  /** A crawl of this project; omitted, its newest one that has a page with a password field. */
  crawl_run_id?: string;
  /** Also ask the router's AI when the rules are not sure (Operator role). */
  use_ai?: boolean;
}

/** `api.rs::LoginSuggestionResponse`. */
export interface LoginSuggestionResponse {
  crawl_run_id: string;
  suggestion: LoginSuggestion;
  /** The other answer, when the rules and the AI differ. */
  alternatives: LoginSuggestion[];
  ai: { requested: boolean; used: boolean; note: string | null };
  /** Variables the steps read that are not set in the orchestrator's environment. */
  missing_secrets: string[];
}

export type AutopilotRunState = 'AWAITING_APPROVAL' | 'RUNNING' | 'WAITING' | 'COMPLETED' | 'FAILED' | 'CANCELLED';
export type AutopilotStepState = 'PENDING' | 'RUNNING' | 'WAITING' | 'DONE' | 'SKIPPED' | 'FAILED';
export type AutopilotStepName =
  | 'REQUIREMENTS'
  | 'CRAWL'
  | 'GAP_CHECK'
  | 'GENERATE'
  | 'VALIDATE'
  | 'APPROVE'
  | 'EXECUTE'
  | 'REPORT';
export type AutopilotApprovalMode = 'manual' | 'auto_validated';

/** `autopilot.rs::AutopilotConfig` — what a person asks a run to do. */
export interface AutopilotConfig {
  name?: string | null;
  base_url: string;
  /** The roles to crawl and test as, each with the login steps a person confirmed. `anonymous`, with none, is a role too. */
  roles: CrawlRole[];
  /** A finished crawl of this project to use instead of crawling again. */
  crawl_run_id?: string | null;
  requirement_document_ids?: string[];
  skills?: GenerateAiSkillInput[];
  depth?: string | null;
  coverage_top_up?: boolean;
  focus_requirements?: boolean;
  /** Ask the AI about at most this many requirements the words left unclear. */
  verify_gaps?: number | null;
  /** Whether approved cases are compiled and run (default true). */
  execute?: boolean;
  run_concurrency?: number | null;
  approval?: AutopilotApprovalMode;
  execution_role?: string | null;
}

export interface AutopilotRange<T = number> {
  min: T;
  max: T;
}

export type AutopilotPricing =
  | { kind: 'UNMETERED' }
  | { kind: 'METERED'; input_usd_per_1k: number; output_usd_per_1k: number };

export interface AutopilotProviderProfile {
  id: string;
  name: string;
  model: string;
  kind: 'LOCAL' | 'API' | 'OTHER';
  pricing: AutopilotPricing;
  observed: {
    calls: number;
    avg_ms: number;
    avg_prompt_tokens: number | null;
    avg_completion_tokens: number | null;
  } | null;
}

export type AutopilotLatency =
  | { basis: 'MEASURED'; calls: number; avg_seconds: number }
  | { basis: 'ASSUMED'; tokens_per_second: number }
  | { basis: 'UNKNOWN' };

/** `autopilot_estimate.rs::Estimate` — what a run would cost, as ranges, every assumption written down. */
export interface AutopilotEstimate {
  requirements: number;
  cases: AutopilotRange;
  steps: Array<{ step: AutopilotStepName; ai_calls: AutopilotRange; seconds: AutopilotRange; note: string | null }>;
  tasks: Array<{
    task: string;
    used_for: string;
    provider: AutopilotProviderProfile | null;
    calls: AutopilotRange;
    prompt_tokens: AutopilotRange;
    completion_tokens: AutopilotRange;
    seconds: AutopilotRange;
    /** `null` when no provider serves the task (a cost nobody can state), never a made-up 0. */
    cost_usd: AutopilotRange | null;
    latency: AutopilotLatency;
  }>;
  totals: { ai_calls: AutopilotRange; seconds: AutopilotRange; cost_usd: AutopilotRange | null; paid: boolean };
  assumptions: string[];
  warnings: string[];
}

/** `POST .../autopilot/estimate` body — every field optional. */
export interface AutopilotEstimateRequest {
  requirement_document_ids?: string[];
  skills?: GenerateAiSkillInput[];
  depth?: string;
  coverage_top_up?: boolean;
  roles?: number;
  focus_requirements?: boolean;
  verify_gaps?: number;
  execute?: boolean;
  run_concurrency?: number;
}

/** `autopilot_store.rs::AutopilotRunRow`. */
export interface AutopilotRun {
  id: string;
  project_id: string;
  name: string;
  state: AutopilotRunState;
  config: AutopilotConfig;
  estimate: AutopilotEstimate;
  approved_at: string | null;
  approved_paid: boolean;
  created_by_name: string;
  current_step: AutopilotStepName | null;
  error: string | null;
  cancel_requested: boolean;
  created_at: string;
  started_at: string | null;
  finished_at: string | null;
  updated_at: string;
}

/** `autopilot_store.rs::AutopilotStepRow`. `detail` is what the step found (see `autopilotModel.ts` for how it is read). */
export interface AutopilotStep {
  run_id: string;
  name: AutopilotStepName;
  position: number;
  state: AutopilotStepState;
  attempts: number;
  detail: Record<string, unknown>;
  error: string | null;
  started_at: string | null;
  finished_at: string | null;
}

/** `autopilot_store.rs::AutopilotEventRow`. */
export interface AutopilotEvent {
  id: number;
  at: string;
  kind: string;
  step: AutopilotStepName | null;
  message: string;
  data: unknown;
}

/** `autopilot_api.rs::RunDetail`. */
export interface AutopilotRunDetail {
  run: AutopilotRun;
  steps: AutopilotStep[];
  events: AutopilotEvent[];
  /** Whether the service is working on the run at this moment. */
  working: boolean;
}

/** `POST .../autopilot/runs/:id/start` body. */
export interface AutopilotStartRequest {
  accept_estimate: boolean;
  accept_paid?: boolean;
}

/** `POST .../autopilot/runs/:id/cancel` response. */
export interface AutopilotCancelResponse extends AutopilotRunDetail {
  outcome: 'cancelled' | 'requested';
}

/** `POST .../autopilot/runs/:id/approve-cases` response. */
export interface AutopilotApproveCasesResponse {
  suite_id: string;
  approved: string[];
  refused: Array<{ id: string; title: string; reason: string }>;
}

export type AutopilotVerdict = 'PASSED' | 'FAILED' | 'INCONCLUSIVE' | 'NOT_EXECUTED' | 'NO_CASES';

/** `autopilot_report.rs::build` — the report of a run. */
export interface AutopilotReport {
  final: boolean;
  run: {
    id: string;
    name: string;
    state: AutopilotRunState;
    error: string | null;
    created_by: string;
    created_at: string;
    started_at: string | null;
    finished_at: string | null;
    seconds: number | null;
  };
  application: { base_url: string; roles: string[] | null; execution_role: string | null };
  requirements: { total: number; by_kind: Record<string, number> | null };
  crawl: { crawl_run_id: string | null; reused: boolean | null; pages: number | null; roles: unknown };
  gaps: Record<string, number> | null;
  generation: {
    cases: number | null;
    groups: number;
    providers: Record<string, number> | null;
    fallback_used: boolean | null;
    prompt_tokens: number | null;
    completion_tokens: number | null;
    latency_ms: number | null;
  };
  validation: { validated: number | null; invalid: Array<{ id: string; title?: string; reason?: string }> | null };
  approval: {
    approved: number | null;
    rejected: number | null;
    undecided: number | null;
    automatic: boolean | null;
    released: boolean | null;
  };
  execution: {
    enabled: boolean;
    done: boolean;
    cases: number | null;
    finished: number | null;
    counts: Record<string, number> | null;
    passed: number;
    failed: number;
    role: string | null;
  };
  findings: AutopilotReportCase[];
  not_run: AutopilotReportCase[];
  coverage: Array<{
    key: string;
    kind: string;
    text: string;
    cases: number;
    ran: number;
    passed: number;
    failed: number;
    not_run: number;
    verdict: AutopilotVerdict;
  }>;
  coverage_summary: Partial<Record<AutopilotVerdict, number>>;
  cost: {
    estimated: AutopilotEstimate['totals'] | null;
    actual: {
      ai_prompt_tokens: number | null;
      ai_completion_tokens: number | null;
      ai_seconds: number | null;
      run_seconds: number | null;
    };
  };
  steps: Array<{ name: AutopilotStepName; state: AutopilotStepState; attempts: number; seconds: number | null }>;
  notes: string[];
}

/** A case of the run that failed, or that could not give an answer. */
export interface AutopilotReportCase {
  case_id: string;
  title: string;
  requirement: string | null;
  outcome: string;
  reason: string | null;
  run_id: string | null;
  attempts: Array<{ run_id: string; outcome: string; reason: string | null }>;
}
