/**
 * FE-1 — the real HTTP transport layer to the AIQA orchestrator
 * (`services/orchestrator`, Rust/Axum). This replaces the previous
 * `AIQAClient` stub, which contained zero `fetch()` calls and returned
 * hardcoded/fabricated values for every method (empty arrays, `PASSED`,
 * `status: 'connected'`, synthetic ids) — exactly the "no integration
 * surface" / "fabricated data throughout" findings the frontend audit
 * flagged (F1, F3).
 *
 * Ported from `apps/web/src/services/apiClient.ts` (Integration Plan
 * §3.1 — "~85% reusable... transport/error machinery"), adapted for:
 *   - base path `/api/v1` instead of `/api` (the contract-shaped surface
 *     BE-8..BE-15 actually implement — see §1.2/§2 of the Integration Plan;
 *     `apps/web` deliberately targets the legacy flat surface instead),
 *   - Contract §2.5's real structured error envelope
 *     (`{error:{code,message,details,request_id}}`), which every REST
 *     handler in this backend now returns on failure (BE-9/BE-10) — the
 *     ported version read plain text, since the backend didn't have a
 *     structured envelope yet when it was written.
 *
 * Design rules, in priority order (unchanged from the ported original):
 *
 *  1. **No fabricated data, ever.** Every function here either returns
 *     what the orchestrator actually sent or throws an `ApiError`. No
 *     fallback dataset, no sample response, no silent substitution.
 *  2. **One backend.** Requests go to the Rust orchestrator's `/api/v1`
 *     surface and nowhere else.
 *  3. **Errors are typed, not stringly.** `ApiError.kind` lets a caller
 *     distinguish "not logged in" from "server is down" from "validation
 *     failed" — distinct states that must never collapse into one string.
 *
 * FE-1 scope note: this file is the transport layer only. Nothing in
 * `forentend` imports it yet — wiring real screens to it is FE-2 (auth UI)
 * through FE-10 (dashboard), not this unit. See
 * `AIQA_INTEGRATION_IMPLEMENTATION_PLAN_v1.0.md` §16.
 */

import type {
  AiProviderInfo,
  AiSettings,
  AiStatusResponse,
  ApiErrorEnvelope,
  AuditRow,
  CancelRunResponse,
  AdHocApiRequest,
  ApiTestExecutionResponse,
  ApiKeyListRow,
  CreateCustomSkillRequest,
  CustomSkillRow,
  FileSkillScan,
  MobileFleet,
  NeuralPipelineResponse,
  ResolvedSkillInstruction,
  OrchestratorStateResponse,
  WhoAmIResponse,
  CreateExecutionResponse,
  CreateProjectRequest,
  CompilationOutcome,
  CustomTestingGenerateRequest,
  CustomTestingGenerateResponse,
  DashboardResponse,
  EvidenceRow,
  Finding,
  FindingAnalysis,
  GenerateAiTestCasesRequest,
  GenerateAiTestCasesResponse,
  StartCrawlRequest,
  CrawlStartedResponse,
  CrawlOutput,
  KpisResponse,
  VerifyApplicationSourceRequest,
  VerifyApplicationSourceResponse,
  ExtractDocumentTextRequest,
  ExtractDocumentTextResponse,
  PerformanceTestConfig,
  PerformanceTestStartResponse,
  PerformanceRunStatusResponse,
  PerformanceTestCancelResponse,
  PerformanceRunRow,
  Project,
  ReviewFindingRequest,
  RunDetail,
  RunEvent,
  RunListItem,
  RunPlan,
  RunSummary,
  TaskState,
  TestCaseHandoffRequest,
  TestCaseRow,
  TestIntelRow,
  UpdateProjectRequest,
  UpdateTestCaseRequest,
  CreateTestCaseRequest,
  OrchestratorLogEntry,
} from '../types/contract';

/** Base path for orchestrator calls. Dev: proxy this to the orchestrator
 *  via `vite.config.ts` (see that file). Prod: point a reverse proxy at
 *  the orchestrator on this same path — same-origin, so no CORS
 *  configuration is required either way. */
export const API_BASE: string = (import.meta as any).env?.VITE_ORCHESTRATOR_BASE ?? '/api/v1';

const API_KEY_STORAGE = 'aiqa.orchestrator.apiKey';

/**
 * Local-dev default orchestrator key, baked in at dev-server start time
 * from the environment (`VITE_AIQA_ORCH_KEY`, loaded by `start.bat` from
 * the repo-root `.env`, which is gitignored). This removes the manual
 * "paste your API key" step for the standard local setup; a key saved via
 * Settings still takes precedence, and when the variable is absent
 * (production build, fresh clone without `.env`) behavior is exactly as
 * before: no key, honest `no_key` auth gate.
 */
const ENV_DEFAULT_API_KEY: string | undefined = ((import.meta as any).env?.VITE_AIQA_ORCH_KEY as string | undefined)?.trim() || undefined;

// ---------------------------------------------------------------- api key

export function getApiKey(): string | null {
  try {
    const stored = localStorage.getItem(API_KEY_STORAGE);
    if (stored && stored.length > 0) return stored;
  } catch {
    /* storage unavailable (private mode); fall through to the env default */
  }
  return ENV_DEFAULT_API_KEY ?? null;
}

export function setApiKey(key: string): void {
  try {
    localStorage.setItem(API_KEY_STORAGE, key.trim());
  } catch {
    /* storage unavailable (private mode); the session stays unauthenticated */
  }
}

export function clearApiKey(): void {
  try {
    localStorage.removeItem(API_KEY_STORAGE);
  } catch {
    /* nothing to do */
  }
}

export function hasApiKey(): boolean {
  const k = getApiKey();
  return typeof k === 'string' && k.length > 0;
}

/** For display only — never renders the secret itself. */
export function maskApiKey(key: string): string {
  if (key.length <= 10) return '•'.repeat(key.length);
  return `${key.slice(0, 6)}…${'•'.repeat(6)}`;
}

// ----------------------------------------------------------------- errors

export type ApiErrorKind =
  /** No API key stored — the user has not authenticated yet. */
  | 'unauthenticated'
  /** Key was rejected (401): unknown, revoked, or malformed. */
  | 'unauthorized'
  /** Authenticated but not permitted (403): role or project scope. */
  | 'forbidden'
  /** 404 from the orchestrator. */
  | 'not_found'
  /** 409 — e.g. cancelling a run that is already terminal. */
  | 'conflict'
  /** 400 — Contract §2.5 `VALIDATION_ERROR`. */
  | 'validation_error'
  /** 4xx that is none of the above. */
  | 'bad_request'
  /** 5xx from the orchestrator. */
  | 'server_error'
  /** Network-level failure: orchestrator not running, DNS, CORS, TLS, timeout. */
  | 'offline';

export class ApiError extends Error {
  readonly kind: ApiErrorKind;
  readonly status: number | null;
  /** Contract §2.5 machine-readable code (`UNAUTHENTICATED`,
   *  `RESOURCE_NOT_FOUND`, ...), when the backend returned the structured
   *  envelope. `null` for a network-level failure or a non-enveloped
   *  response (see `parseErrorBody`). */
  readonly code: string | null;
  /** The backend's own human-readable message — real, from `errors.rs`,
   *  which is deliberately sanitized server-side (Contract §2.5: "MUST
   *  NOT expose secrets, internal stack traces, ..."). */
  readonly detail: string | null;
  /** Contract §2.5 `request_id`, for correlating a report against server
   *  logs/audit records. `null` when unavailable (network failure). */
  readonly requestId: string | null;

  constructor(
    kind: ApiErrorKind,
    message: string,
    status: number | null = null,
    code: string | null = null,
    detail: string | null = null,
    requestId: string | null = null,
  ) {
    super(message);
    this.name = 'ApiError';
    this.kind = kind;
    this.status = status;
    this.code = code;
    this.detail = detail;
    this.requestId = requestId;
  }

  /** Whether retrying the same request could plausibly succeed without
   *  the user changing something first. Drives whether a "Retry" button
   *  shows. */
  get isRetryable(): boolean {
    return this.kind === 'offline' || this.kind === 'server_error';
  }
}

function kindForStatus(status: number): ApiErrorKind {
  if (status === 401) return 'unauthorized';
  if (status === 403) return 'forbidden';
  if (status === 404) return 'not_found';
  if (status === 409) return 'conflict';
  if (status === 400) return 'validation_error';
  // Gateway-level failures (dev proxy's own 502-on-upstream-down, or a prod
  // reverse proxy) mean the request never reached the orchestrator at all.
  if (status === 502 || status === 503 || status === 504) return 'offline';
  if (status >= 500) return 'server_error';
  return 'bad_request';
}

function messageForKind(kind: ApiErrorKind, detail: string | null): string {
  switch (kind) {
    case 'unauthenticated':
      return 'Not authenticated. Enter an orchestrator API key to continue.';
    case 'unauthorized':
      return 'API key was rejected. It may be invalid or revoked.';
    case 'forbidden':
      return detail || "Your API key's role or project scope does not permit this action.";
    case 'not_found':
      return detail || 'The orchestrator has no such resource.';
    case 'conflict':
      return detail || 'The resource is not in a state that allows this action.';
    case 'validation_error':
      return detail || 'The request failed contract validation rules.';
    case 'bad_request':
      return detail || 'The orchestrator rejected the request.';
    case 'server_error':
      return detail || 'The orchestrator returned an internal error.';
    case 'offline':
      return 'Cannot reach the AIQA orchestrator. Is it running?';
  }
}

/** Parses Contract §2.5's structured envelope. Falls back to the raw
 *  response text if the body isn't (yet) that shape — e.g. a WebSocket
 *  pre-upgrade rejection, or an unexpected proxy/gateway error page —
 *  rather than throwing a second, confusing parse error on top of the
 *  original one. */
async function parseErrorBody(res: Response): Promise<{ code: string | null; detail: string | null; requestId: string | null }> {
  const text = await res.text().catch(() => '');
  if (!text) return { code: null, detail: null, requestId: null };
  try {
    const body = JSON.parse(text) as Partial<ApiErrorEnvelope>;
    if (body.error && typeof body.error.message === 'string') {
      return {
        code: typeof body.error.code === 'string' ? body.error.code : null,
        detail: body.error.message,
        requestId: typeof body.error.request_id === 'string' ? body.error.request_id : null,
      };
    }
  } catch {
    /* not the structured envelope — fall through to raw text */
  }
  return { code: null, detail: text, requestId: null };
}

// ---------------------------------------------------------------- requests

interface RequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  body?: unknown;
  query?: Record<string, string | number | boolean | undefined>;
  signal?: AbortSignal;
  /** Build order item 2 (22 Aug 2026) — skips the `API_BASE` (`/api/v1`)
   *  prefix. Only `/metrics` needs this today: a real, flat, legacy-style
   *  route (`main.rs`'s un-prefixed router block, alongside
   *  `/admin/api-keys`), not one of the `/api/v1/...` contract routes
   *  every other call in this file targets. */
  skipBase?: boolean;
}

function buildUrl(path: string, query?: RequestOptions['query'], skipBase?: boolean): string {
  const url = skipBase ? path : `${API_BASE}${path}`;
  if (!query) return url;
  const params = new URLSearchParams();
  for (const [k, v] of Object.entries(query)) {
    if (v !== undefined) params.set(k, String(v));
  }
  const qs = params.toString();
  return qs ? `${url}?${qs}` : url;
}

async function rawRequest(path: string, opts: RequestOptions = {}): Promise<Response> {
  const { method = 'GET', body, query, signal, skipBase } = opts;

  const key = getApiKey();
  if (!key) {
    throw new ApiError('unauthenticated', messageForKind('unauthenticated', null));
  }
  const headers: Record<string, string> = { Authorization: `Bearer ${key}` };
  if (body !== undefined) {
    headers['Content-Type'] = 'application/json';
  }

  let res: Response;
  try {
    res = await fetch(buildUrl(path, query, skipBase), {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      signal,
    });
  } catch (err) {
    // An AbortError is the caller cancelling, not the backend failing — let
    // it through untouched so callers can tell the two apart.
    if (err instanceof DOMException && err.name === 'AbortError') throw err;
    throw new ApiError('offline', messageForKind('offline', null));
  }

  if (!res.ok) {
    const { code, detail, requestId } = await parseErrorBody(res);
    const kind = kindForStatus(res.status);
    throw new ApiError(kind, messageForKind(kind, detail), res.status, code, detail, requestId);
  }

  return res;
}

/** JSON request. Throws `ApiError` on any non-2xx or network failure. */
async function requestJson<T>(path: string, opts: RequestOptions = {}): Promise<T> {
  const res = await rawRequest(path, opts);
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

/** Request with no response body (204). */
async function requestVoid(path: string, opts: RequestOptions = {}): Promise<void> {
  await rawRequest(path, opts);
}

/** Raw bytes — evidence content, which may be any content type. */
async function requestBlob(path: string, opts: RequestOptions = {}): Promise<Blob> {
  const res = await rawRequest(path, opts);
  return res.blob();
}

/** Raw text — `/metrics`'s real Prometheus text-exposition body
 *  (`Content-Type: text/plain; version=0.0.4`, `services/orchestrator/
 *  src/metrics.rs::render`), not JSON. Parsed client-side by
 *  `parsePrometheusText` in `TelemetryPage.tsx` — no backend JSON adapter
 *  exists or is needed; the real format is simple enough to parse directly
 *  (build order item 2, 22 Aug 2026). */
async function requestText(path: string, opts: RequestOptions = {}): Promise<string> {
  const res = await rawRequest(path, opts);
  return res.text();
}

// ------------------------------------------------------------- realtime (WS)

/**
 * FE-9 — real URL for `services/orchestrator`'s project-scoped realtime
 * stream (`GET /api/v1/projects/:project_id/events`, `project_events_ws_handler`
 * in `api.rs`, BE-14/Contract §14). Confirmed by reading that handler
 * directly, not assumed from the route name:
 *
 *  - **Transport is a real WebSocket** (axum's `WebSocketUpgrade`), not SSE
 *    or any other streaming mechanism.
 *  - **Auth is a `?key=` query parameter**, not the `Authorization` header
 *    every REST call in this file uses — the backend's own doc comment on
 *    `ws_handler` explains why: "browsers cannot set custom headers on a
 *    WebSocket handshake (`new WebSocket(url)` takes no headers argument)".
 *    This is not a new, weaker auth mechanism invented here — it is the one
 *    real mechanism the backend actually implements for this endpoint, and
 *    reuses the exact same raw key `getApiKey()` already returns (no second
 *    credential store).
 *  - No backlog/replay on this endpoint (unlike the legacy per-run
 *    `/runs/:id/ws`) — catch-up after a gap is `aiqaApi.executions.events`
 *    (per-run, `?since=`), which already existed before this sprint.
 *
 * Returns `null` when no key is stored — callers must not attempt to open a
 * socket with an empty/missing key (the backend would reject it with a real
 * 401 anyway, but there is no reason to make that round trip).
 */
export function projectEventsWsUrl(projectId: string): string | null {
  const key = getApiKey();
  if (!key) return null;
  const base = new URL(API_BASE, window.location.origin);
  const wsProtocol = base.protocol === 'https:' ? 'wss:' : 'ws:';
  const path = `${base.pathname.replace(/\/+$/, '')}/projects/${encodeURIComponent(projectId)}/events`;
  return `${wsProtocol}//${base.host}${path}?key=${encodeURIComponent(key)}`;
}

/** Real-time WebSocket URL for AIQA orchestrator structured log streaming. */
export function orchestratorLogsWsUrl(projectId?: string | null): string | null {
  const key = getApiKey();
  if (!key) return null;
  const base = new URL(API_BASE, window.location.origin);
  const wsProtocol = base.protocol === 'https:' ? 'wss:' : 'ws:';
  if (projectId) {
    const path = `${base.pathname.replace(/\/+$/, '')}/projects/${encodeURIComponent(projectId)}/logs/ws`;
    return `${wsProtocol}//${base.host}${path}?key=${encodeURIComponent(key)}`;
  }
  const path = `${base.pathname.replace(/\/+$/, '')}/logs/ws`;
  return `${wsProtocol}//${base.host}${path}?key=${encodeURIComponent(key)}`;
}

// ------------------------------------------------------------- typed calls
//
// One function per real, implemented backend endpoint (BE-8..BE-15). Every
// path is the exact route `services/orchestrator/src/api.rs` mounts under
// `/api/v1/projects/...` — no legacy flat path (`/runs`, `/bug-drafts`,
// `/admin/...`) is called here, per this sprint's explicit instruction.
// Admin/API-key management is deliberately not covered — that is FE-2's
// (Auth UI) concern, not the transport layer's.

export const aiqaApi = {
  projects: {
    list: (signal?: AbortSignal) => requestJson<Project[]>('/projects', { signal }),
    get: (projectId: string, signal?: AbortSignal) => requestJson<Project>(`/projects/${projectId}`, { signal }),
    create: (body: CreateProjectRequest, signal?: AbortSignal) =>
      requestJson<Project>('/projects', { method: 'POST', body, signal }),
    update: (projectId: string, body: UpdateProjectRequest, signal?: AbortSignal) =>
      requestJson<Project>(`/projects/${projectId}`, { method: 'PATCH', body, signal }),
    delete: (projectId: string, signal?: AbortSignal) =>
      requestVoid(`/projects/${projectId}`, { method: 'DELETE', signal }),
  },

  executions: {
    list: (projectId: string, opts: { limit?: number } = {}, signal?: AbortSignal) =>
      requestJson<RunListItem[]>(`/projects/${projectId}/executions`, { query: { limit: opts.limit }, signal }),
    create: (projectId: string, plan: RunPlan, signal?: AbortSignal) =>
      requestJson<CreateExecutionResponse>(`/projects/${projectId}/executions`, { method: 'POST', body: plan, signal }),
    get: (projectId: string, runId: string, signal?: AbortSignal) =>
      requestJson<RunDetail>(`/projects/${projectId}/executions/${runId}`, { signal }),
    cancel: (projectId: string, runId: string, signal?: AbortSignal) =>
      requestJson<CancelRunResponse>(`/projects/${projectId}/executions/${runId}/cancel`, { method: 'POST', signal }),
    // FE-8: both real handlers (`approve_run_scoped`/`reject_run_scoped`)
    // return `Json<RunSummary>` — confirmed by reading `api.rs` directly —
    // not `RunDetail` (which additionally claims a `tasks` field neither
    // response actually carries). Fixed to the real shape; call sites that
    // need the task list re-fetch it via `.get()`/`.tasks()`, same as any
    // other real ordering the backend enforces.
    approve: (projectId: string, runId: string, signal?: AbortSignal) =>
      requestJson<RunSummary>(`/projects/${projectId}/executions/${runId}/approve`, { method: 'POST', signal }),
    reject: (projectId: string, runId: string, reason?: string, signal?: AbortSignal) =>
      requestJson<RunSummary>(`/projects/${projectId}/executions/${runId}/reject`, {
        method: 'POST',
        body: reason !== undefined ? { reason } : undefined,
        signal,
      }),
    tasks: (projectId: string, runId: string, signal?: AbortSignal) =>
      requestJson<TaskState[]>(`/projects/${projectId}/executions/${runId}/tasks`, { signal }),
    task: (projectId: string, runId: string, taskId: string, signal?: AbortSignal) =>
      requestJson<TaskState>(`/projects/${projectId}/executions/${runId}/tasks/${taskId}`, { signal }),
    /** Catch-up/history for one run (Contract §14/ACC-008 reconnect path).
     *  The live realtime stream is FE-9's `RealtimeContext`
     *  (`projectEventsWsUrl` below), not this. */
    events: (projectId: string, runId: string, since?: number, signal?: AbortSignal) =>
      requestJson<RunEvent[]>(`/projects/${projectId}/executions/${runId}/events`, { query: { since }, signal }),
    evidence: (projectId: string, runId: string, signal?: AbortSignal) =>
      requestJson<EvidenceRow[]>(`/projects/${projectId}/executions/${runId}/evidence`, { signal }),
  },

  /** Phase 6/8 — real crawler + per-role visibility matrix. A crawl runs
   *  through the exact same run/task pipeline every other browser task
   *  does (`executions.get(projectId, run_id)` polls it like any other
   *  run); `result()` is only ever non-empty once that run reaches a
   *  terminal state and genuinely wrote one. */
  crawl: {
    start: (projectId: string, body: StartCrawlRequest, signal?: AbortSignal) =>
      requestJson<CrawlStartedResponse>(`/projects/${projectId}/crawl`, { method: 'POST', body, signal }),
    result: (projectId: string, runId: string, signal?: AbortSignal) =>
      requestJson<CrawlOutput>(`/projects/${projectId}/crawl/${runId}/result`, { signal }),
  },

  evidence: {
    /** §15.2 "Evidence Detail" — metadata only. Use `content()` for the
     *  actual artifact bytes (BE-15 §4: intentionally split). */
    get: (projectId: string, evidenceId: string, signal?: AbortSignal) =>
      requestJson<EvidenceRow>(`/projects/${projectId}/evidence/${evidenceId}`, { signal }),
    content: (projectId: string, evidenceId: string, signal?: AbortSignal) =>
      requestBlob(`/projects/${projectId}/evidence/${evidenceId}/content`, { signal }),
  },

  findings: {
    list: (projectId: string, signal?: AbortSignal) => requestJson<Finding[]>(`/projects/${projectId}/findings`, { signal }),
    get: (projectId: string, findingId: string, signal?: AbortSignal) =>
      requestJson<Finding>(`/projects/${projectId}/findings/${findingId}`, { signal }),
    analysis: (projectId: string, findingId: string, signal?: AbortSignal) =>
      requestJson<FindingAnalysis>(`/projects/${projectId}/findings/${findingId}/analysis`, { method: 'POST', signal }),
    review: (projectId: string, findingId: string, body: ReviewFindingRequest, signal?: AbortSignal) =>
      requestVoid(`/projects/${projectId}/findings/${findingId}/review`, { method: 'POST', body, signal }),
  },

  dashboard: {
    get: (projectId: string, signal?: AbortSignal) =>
      requestJson<DashboardResponse>(`/projects/${projectId}/dashboard`, { signal }),
    kpis: (projectId: string, signal?: AbortSignal) => requestJson<KpisResponse>(`/projects/${projectId}/kpis`, { signal }),
  },

  audit: {
    list: (projectId: string, opts: { limit?: number } = {}, signal?: AbortSignal) =>
      requestJson<AuditRow[]>(`/projects/${projectId}/audit`, { query: { limit: opts.limit }, signal }),
  },

  /** FE-22 — BE-10's project-scoped `TestIntelRow[]` route. The legacy,
   *  unscoped `/intelligence/tests` (key-bound `ctx.project_id`, no URL
   *  parameter) is deliberately never called from here — see BE-10's
   *  report for why that endpoint is unsafe for this app's multi-project
   *  key/project-switcher model. No fallback to it exists. */
  intelligence: {
    tests: (projectId: string, signal?: AbortSignal) =>
      requestJson<TestIntelRow[]>(`/projects/${projectId}/intelligence/tests`, { signal }),
  },

  /** AUD-005 — real, persisted, project-isolated Test Case CRUD + lifecycle
   *  + execution handoff (`services/orchestrator/src/api.rs`'s
   *  `*_test_case_scoped` handlers). Replaces the Test Case Generator's
   *  previous local-only, React-state-only output. */
  testCases: {
    list: (projectId: string, opts: { status?: TestCaseRow['status'] } = {}, signal?: AbortSignal) =>
      requestJson<TestCaseRow[]>(`/projects/${projectId}/test-cases`, { query: { status: opts.status }, signal }),
    create: (projectId: string, body: CreateTestCaseRequest, signal?: AbortSignal) =>
      requestJson<TestCaseRow>(`/projects/${projectId}/test-cases`, { method: 'POST', body, signal }),
    get: (projectId: string, id: string, signal?: AbortSignal) =>
      requestJson<TestCaseRow>(`/projects/${projectId}/test-cases/${id}`, { signal }),
    update: (projectId: string, id: string, body: UpdateTestCaseRequest, signal?: AbortSignal) =>
      requestJson<TestCaseRow>(`/projects/${projectId}/test-cases/${id}`, { method: 'PATCH', body, signal }),
    delete: (projectId: string, id: string, signal?: AbortSignal) =>
      requestVoid(`/projects/${projectId}/test-cases/${id}`, { method: 'DELETE', signal }),
    validate: (projectId: string, id: string, signal?: AbortSignal) =>
      requestJson<TestCaseRow>(`/projects/${projectId}/test-cases/${id}/validate`, { method: 'POST', signal }),
    approve: (projectId: string, id: string, signal?: AbortSignal) =>
      requestJson<TestCaseRow>(`/projects/${projectId}/test-cases/${id}/approve`, { method: 'POST', signal }),
    reject: (projectId: string, id: string, signal?: AbortSignal) =>
      requestJson<TestCaseRow>(`/projects/${projectId}/test-cases/${id}/reject`, { method: 'POST', signal }),
    /** `TestCaseHandoffRequest.executor`/`.simulate` — at least one is
     *  required (the backend 400s otherwise); see that type's own doc
     *  comment for why there is no default. */
    handoff: (projectId: string, id: string, body: TestCaseHandoffRequest, signal?: AbortSignal) =>
      requestJson<CreateExecutionResponse>(`/projects/${projectId}/test-cases/${id}/handoff`, { method: 'POST', body, signal }),
    /** Phase 1 (AI Test Case Generator integration) — the real, skill-aware
     *  AI generation endpoint (`api.rs::generate_ai_test_cases_scoped`,
     *  `ai_generation.rs`/`ai_router.rs` behind it). Produces drafts only
     *  (never persists) — save whichever drafts the user keeps via the
     *  existing `.create()` above, exactly like any other draft. Requires
     *  `Role::Operator` and a real, scoped project id, same as `.create()`. */
    generateAi: (projectId: string, body: GenerateAiTestCasesRequest, signal?: AbortSignal) =>
      requestJson<GenerateAiTestCasesResponse>(`/projects/${projectId}/test-cases/generate-ai`, { method: 'POST', body, signal }),
    /** Phase 2.1 — real, read-only compilation preview
     *  (`api.rs::compile_test_case_scoped`). Never mutates anything; safe
     *  to call to preview READY/NOT_EXECUTABLE/COMPILATION_FAILED/
     *  COMPILATION_UNAVAILABLE before committing to a handoff. */
    compile: (projectId: string, id: string, signal?: AbortSignal) =>
      requestJson<CompilationOutcome>(`/projects/${projectId}/test-cases/${id}/compile`, { method: 'POST', signal }),
  },

  /** Phase 3 (Custom Testing) — real requirement -> AI strategy -> test
   *  cases (`api.rs::generate_custom_testing_scoped`, `custom_testing.rs`
   *  behind it). Produces drafts only (never persists) — save whichever
   *  drafts are kept via the existing `testCases.create()` above, exactly
   *  like a Test Case Generator draft. Requires `Role::Operator` and a
   *  real, scoped project id, same as `testCases.generateAi()`. */
  customTesting: {
    generate: (projectId: string, body: CustomTestingGenerateRequest, signal?: AbortSignal) =>
      requestJson<CustomTestingGenerateResponse>(`/projects/${projectId}/custom-testing/generate`, { method: 'POST', body, signal }),
  },

  /** Phase 4 (API Testing page) — real, ad-hoc HTTP request execution
   *  (`api.rs::execute_api_test_scoped`, `api_testing.rs`/`executor.rs::
   *  CompiledApiExecutor` behind it). This call genuinely waits for the
   *  real run it creates to finish (server-side, bounded) — it is not a
   *  fire-and-forget dispatch, so no separate client-side polling loop is
   *  needed for the common case. Requires `Role::Operator` and a real,
   *  scoped project id, same as every other execution-triggering call. */
  apiTesting: {
    execute: (projectId: string, body: AdHocApiRequest, signal?: AbortSignal) =>
      requestJson<ApiTestExecutionResponse>(`/projects/${projectId}/api-testing/execute`, { method: 'POST', body, signal }),
  },

  /** Phase 6 (Performance Testing page) — real Load/Stress/Spike/Volume
   *  workload generation (`api.rs`'s `.../performance-testing/*` handlers,
   *  `performance_testing.rs` behind them). `start` returns immediately
   *  (`status: 'RUNNING'`); poll `status` for real, live progress — never
   *  synthesize a percentage client-side. `cancel`/`status` are safe to
   *  call from any component; `start` requires `Role::Operator` and a real,
   *  scoped project id, same as every other execution-triggering call. */
  performanceTesting: {
    start: (projectId: string, body: PerformanceTestConfig, signal?: AbortSignal) =>
      requestJson<PerformanceTestStartResponse>(`/projects/${projectId}/performance-testing/start`, { method: 'POST', body, signal }),
    status: (projectId: string, runId: string, signal?: AbortSignal) =>
      requestJson<PerformanceRunStatusResponse>(`/projects/${projectId}/performance-testing/${runId}/status`, { signal }),
    cancel: (projectId: string, runId: string, signal?: AbortSignal) =>
      requestJson<PerformanceTestCancelResponse>(`/projects/${projectId}/performance-testing/${runId}/cancel`, { method: 'POST', signal }),
    list: (projectId: string, signal?: AbortSignal) =>
      requestJson<PerformanceRunRow[]>(`/projects/${projectId}/performance-testing`, { signal }),
  },

  /** Truthfulness Cleanup Step 1 — real Application Source connectivity
   *  verification (`api.rs::verify_application_source_scoped`,
   *  `app_source.rs` behind it). Replaces the Test Case Generator's former
   *  fully client-side `setTimeout`+`Math.random()` simulation with a
   *  genuine, bounded, SSRF-guarded outbound request. Requires
   *  `Role::Operator` and a real, scoped project id, same as every other
   *  real-outbound-action call (API Testing's `execute`, Performance
   *  Testing's `start`). */
  appSource: {
    verify: (projectId: string, body: VerifyApplicationSourceRequest, signal?: AbortSignal) =>
      requestJson<VerifyApplicationSourceResponse>(`/projects/${projectId}/verify-application-source`, { method: 'POST', body, signal }),
  },

  /** Real Product Documentation content extraction
   *  (`services/orchestrator/src/doc_extract.rs`) — makes an uploaded
   *  PRD/FRD/SRD/Additional document's real text genuinely available to
   *  AI generation, instead of the document being referenced by filename
   *  only. */
  documents: {
    extractText: (projectId: string, body: ExtractDocumentTextRequest, signal?: AbortSignal) =>
      requestJson<ExtractDocumentTextResponse>(`/projects/${projectId}/documents/extract-text`, { method: 'POST', body, signal }),
  },

  /** Central AI Provider & Model Intelligence Layer
   *  (`services/orchestrator/src/ai_router.rs`) — real, platform-wide (not
   *  project-scoped) discovery/settings, matching that backend module's
   *  own doc comment. */
  ai: {
    status: (signal?: AbortSignal) => requestJson<AiStatusResponse>('/ai/status', { signal }),
    getSettings: (signal?: AbortSignal) => requestJson<AiSettings>('/ai/settings', { signal }),
    updateSettings: (body: AiSettings, signal?: AbortSignal) => requestJson<AiSettings>('/ai/settings', { method: 'PUT', body, signal }),
    refresh: (signal?: AbortSignal) => requestJson<AiProviderInfo[]>('/ai/refresh', { method: 'POST', signal }),
  },

  /** Build order item 2 (22 Aug 2026, System Telemetry) — `GET /metrics`
   *  (`services/orchestrator/src/metrics.rs`, `api.rs::get_metrics`), a
   *  real, flat, platform-wide (not project-scoped) route, requiring only
   *  authentication (no specific role). Returns real Prometheus text, not
   *  JSON — `requestText` + `skipBase` (this route is not under
   *  `/api/v1/...`). */
  metrics: {
    get: (signal?: AbortSignal) => requestText('/metrics', { signal, skipBase: true }),
  },

  /** Real RBAC surface. `whoami` is the caller's own identity/role/scope
   *  (`api.rs::whoami`); `apiKeys` is the real key inventory with each key's
   *  real role and project scope (`GET /admin/api-keys`, Admin only — a
   *  non-admin caller gets a real 403, which this app surfaces rather than
   *  hiding the control). `/admin/...` is a real flat route, not under
   *  `/api/v1`, hence `skipBase`. */
  rbac: {
    whoami: (signal?: AbortSignal) => requestJson<WhoAmIResponse>('/auth/whoami', { signal }),
    apiKeys: (signal?: AbortSignal) => requestJson<ApiKeyListRow[]>('/admin/api-keys', { signal, skipBase: true }),
  },

  /** Real mobile device fleet (`mobile.rs`) — real `adb devices -l`, real
   *  AVDs, and the real Maestro CLI's real version. Host-level, not
   *  project-scoped: adb sees one set of devices per machine. */
  mobile: {
    fleet: (signal?: AbortSignal) => requestJson<MobileFleet>('/mobile/fleet', { signal }),
    bootEmulator: (avd: string, signal?: AbortSignal) =>
      requestJson<{ message: string }>('/mobile/emulators/boot', { method: 'POST', body: { avd }, signal }),
    bootSimulator: (udid: string, signal?: AbortSignal) =>
      requestJson<{ message: string }>('/mobile/simulators/boot', { method: 'POST', body: { udid }, signal }),
  },

  /** Real stage-by-stage pipeline state for one project
   *  (`api.rs::get_neural_pipeline`), including stages that genuinely do
   *  not exist, reported as unavailable with the real reason. */
  pipeline: {
    get: (projectId: string, signal?: AbortSignal) =>
      requestJson<NeuralPipelineResponse>(`/projects/${projectId}/pipeline`, { signal }),
  },

  /** The real coordination surface behind the Orchestrator view
   *  (`api.rs::get_orchestrator_state`) — executor capability routing,
   *  enforced policy gates, and the live DAG of in-process runs.
   *  Platform-wide, not project-scoped (same as `ai`). */
  orchestrator: {
    state: (signal?: AbortSignal) => requestJson<OrchestratorStateResponse>('/orchestrator/state', { signal }),
  },

  /** Real, persisted custom skills for AI test-case generation
   *  (`services/orchestrator/src/api.rs`'s `create_custom_skill` /
   *  `list_custom_skills_handler` / `delete_custom_skill_handler`).
   *  Platform-wide, not project-scoped — same as `ai`. */
  skills: {
    list: (signal?: AbortSignal) => requestJson<CustomSkillRow[]>('/skills', { signal }),
    create: (body: CreateCustomSkillRequest, signal?: AbortSignal) =>
      requestJson<CustomSkillRow>('/skills', { method: 'POST', body, signal }),
    delete: (id: string, signal?: AbortSignal) => requestVoid(`/skills/${encodeURIComponent(id)}`, { method: 'DELETE', signal }),
    /** The real instruction each skill resolves to — the same text the
     *  generator embeds in its prompt (`api.rs::resolve_skills`). */
    resolve: (skills: Array<{ id: string; name: string; description: string }>, signal?: AbortSignal) =>
      requestJson<ResolvedSkillInstruction[]>('/skills/resolve', { method: 'POST', body: { skills }, signal }),
    /** Real `SKILL.md` files read live from `services/orchestrator/skills/`
     *  on disk (`api.rs::list_file_skills_handler`) — re-scanned on every
     *  call, so a file dropped in after the app loaded still shows up. */
    files: (signal?: AbortSignal) => requestJson<FileSkillScan>('/skills/files', { signal }),
  },

  /** Real-time Orchestrator Live Activity Logs (`api.rs::list_logs_handler`). */
  logs: {
    list: (opts: { projectId?: string | null; limit?: number } = {}, signal?: AbortSignal) => {
      if (opts.projectId) {
        return requestJson<OrchestratorLogEntry[]>(`/projects/${encodeURIComponent(opts.projectId)}/logs`, {
          query: { limit: opts.limit },
          signal,
        });
      }
      return requestJson<OrchestratorLogEntry[]>('/logs', {
        query: { limit: opts.limit },
        signal,
      });
    },
  },
};
