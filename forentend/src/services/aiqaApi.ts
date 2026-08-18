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
  ApiErrorEnvelope,
  AuditRow,
  CancelRunResponse,
  CreateExecutionResponse,
  CreateProjectRequest,
  DashboardResponse,
  EvidenceRow,
  Finding,
  FindingAnalysis,
  KpisResponse,
  Project,
  ReviewFindingRequest,
  RunDetail,
  RunEvent,
  RunListItem,
  RunPlan,
  TaskState,
  UpdateProjectRequest,
} from '../types/contract';

/** Base path for orchestrator calls. Dev: proxy this to the orchestrator
 *  via `vite.config.ts` (see that file). Prod: point a reverse proxy at
 *  the orchestrator on this same path — same-origin, so no CORS
 *  configuration is required either way. */
export const API_BASE: string = (import.meta as any).env?.VITE_ORCHESTRATOR_BASE ?? '/api/v1';

const API_KEY_STORAGE = 'aiqa.orchestrator.apiKey';

// ---------------------------------------------------------------- api key

export function getApiKey(): string | null {
  try {
    return localStorage.getItem(API_KEY_STORAGE);
  } catch {
    return null;
  }
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
}

function buildUrl(path: string, query?: RequestOptions['query']): string {
  const url = `${API_BASE}${path}`;
  if (!query) return url;
  const params = new URLSearchParams();
  for (const [k, v] of Object.entries(query)) {
    if (v !== undefined) params.set(k, String(v));
  }
  const qs = params.toString();
  return qs ? `${url}?${qs}` : url;
}

async function rawRequest(path: string, opts: RequestOptions = {}): Promise<Response> {
  const { method = 'GET', body, query, signal } = opts;

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
    res = await fetch(buildUrl(path, query), {
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
    approve: (projectId: string, runId: string, signal?: AbortSignal) =>
      requestJson<RunDetail>(`/projects/${projectId}/executions/${runId}/approve`, { method: 'POST', signal }),
    reject: (projectId: string, runId: string, reason?: string, signal?: AbortSignal) =>
      requestJson<RunDetail>(`/projects/${projectId}/executions/${runId}/reject`, {
        method: 'POST',
        body: reason !== undefined ? { reason } : undefined,
        signal,
      }),
    tasks: (projectId: string, runId: string, signal?: AbortSignal) =>
      requestJson<TaskState[]>(`/projects/${projectId}/executions/${runId}/tasks`, { signal }),
    task: (projectId: string, runId: string, taskId: string, signal?: AbortSignal) =>
      requestJson<TaskState>(`/projects/${projectId}/executions/${runId}/tasks/${taskId}`, { signal }),
    /** Catch-up/history for one run (Contract §14/ACC-008 reconnect path).
     *  The live realtime stream is FE-9 (`useEventStream`), not this. */
    events: (projectId: string, runId: string, since?: number, signal?: AbortSignal) =>
      requestJson<RunEvent[]>(`/projects/${projectId}/executions/${runId}/events`, { query: { since }, signal }),
    evidence: (projectId: string, runId: string, signal?: AbortSignal) =>
      requestJson<EvidenceRow[]>(`/projects/${projectId}/executions/${runId}/evidence`, { signal }),
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
};
