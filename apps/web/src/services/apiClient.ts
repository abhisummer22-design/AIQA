/**
 * The single HTTP entry point to the AIQA Rust orchestrator.
 *
 * Design rules, in priority order:
 *
 *  1. **No fabricated data, ever.** Every function here either returns what
 *     the orchestrator actually sent or throws an `ApiError`. There is no
 *     fallback dataset, no "sample" response, no silent substitution. A
 *     caller that cannot reach the backend must render an offline/error
 *     state, not a plausible-looking number.
 *  2. **One backend.** Requests go to the Rust orchestrator and nowhere else
 *     (`services/orchestrator`, port 8080 by default, proxied at `/api` in
 *     dev by `vite.config.ts`).
 *  3. **Errors are typed, not stringly.** `ApiError.kind` lets a component
 *     distinguish "you are not logged in" from "the server is down" from
 *     "this returned nothing", which are four different screens.
 *
 * Auth: the orchestrator requires `Authorization: Bearer <key>` on every
 * route except `/health` (see `services/orchestrator/src/auth.rs`). The key
 * is minted by the backend on first boot and pasted in by the operator; it
 * is held in localStorage and never hardcoded, never logged, and never sent
 * anywhere other than the orchestrator.
 */

/** Base path for orchestrator calls. Dev: proxied to :8080 by vite.config.ts.
 *  Prod: point a reverse proxy at the orchestrator on this same path. */
export const API_BASE =
  (import.meta as any).env?.VITE_ORCHESTRATOR_BASE ?? "/api";

const API_KEY_STORAGE = "aiqa.orchestrator.apiKey";

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
  return typeof k === "string" && k.length > 0;
}

/** For display only — never renders the secret itself. */
export function maskApiKey(key: string): string {
  if (key.length <= 10) return "•".repeat(key.length);
  return `${key.slice(0, 6)}…${"•".repeat(6)}`;
}

// ----------------------------------------------------------------- errors

export type ApiErrorKind =
  /** No API key stored — the user has not authenticated yet. */
  | "unauthenticated"
  /** Key was rejected (401): unknown, revoked, or malformed. */
  | "unauthorized"
  /** Authenticated but not permitted (403): role or project scope. */
  | "forbidden"
  /** 404 from the orchestrator. */
  | "not_found"
  /** 409 — e.g. approving a run that is not PENDING_APPROVAL. */
  | "conflict"
  /** 4xx that is none of the above (bad request body, bad plan, ...). */
  | "bad_request"
  /** 5xx from the orchestrator. */
  | "server_error"
  /** Network-level failure: orchestrator not running, DNS, CORS, TLS. */
  | "offline";

export class ApiError extends Error {
  readonly kind: ApiErrorKind;
  readonly status: number | null;
  /** The orchestrator's own error text, which is a safe hand-written string
   *  (see `api.rs` — it never leaks stack traces or internal URLs). */
  readonly detail: string | null;

  constructor(kind: ApiErrorKind, message: string, status: number | null = null, detail: string | null = null) {
    super(message);
    this.name = "ApiError";
    this.kind = kind;
    this.status = status;
    this.detail = detail;
  }

  /** Whether retrying the same request could plausibly succeed without the
   *  user changing something first. Drives whether a "Retry" button shows. */
  get isRetryable(): boolean {
    return this.kind === "offline" || this.kind === "server_error";
  }
}

function kindForStatus(status: number): ApiErrorKind {
  if (status === 401) return "unauthorized";
  if (status === 403) return "forbidden";
  if (status === 404) return "not_found";
  if (status === 409) return "conflict";
  if (status >= 500) return "server_error";
  return "bad_request";
}

function messageForKind(kind: ApiErrorKind, detail: string | null): string {
  switch (kind) {
    case "unauthenticated":
      return "Not authenticated. Enter an orchestrator API key to continue.";
    case "unauthorized":
      return "API key was rejected. It may be invalid or revoked.";
    case "forbidden":
      return detail || "Your API key's role or project scope does not permit this.";
    case "not_found":
      return detail || "The orchestrator has no such resource.";
    case "conflict":
      return detail || "The resource is not in a state that allows this action.";
    case "bad_request":
      return detail || "The orchestrator rejected the request.";
    case "server_error":
      return detail || "The orchestrator returned an internal error.";
    case "offline":
      return "Cannot reach the AIQA orchestrator. Is it running on port 8080?";
  }
}

// ---------------------------------------------------------------- requests

interface RequestOptions {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: unknown;
  signal?: AbortSignal;
  /** Set for `/health`, the one route the orchestrator leaves unauthenticated. */
  skipAuth?: boolean;
}

async function rawRequest(path: string, opts: RequestOptions = {}): Promise<Response> {
  const { method = "GET", body, signal, skipAuth = false } = opts;

  const headers: Record<string, string> = {};
  if (!skipAuth) {
    const key = getApiKey();
    if (!key) {
      throw new ApiError("unauthenticated", messageForKind("unauthenticated", null));
    }
    headers["Authorization"] = `Bearer ${key}`;
  }
  if (body !== undefined) {
    headers["Content-Type"] = "application/json";
  }

  let res: Response;
  try {
    res = await fetch(`${API_BASE}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      signal,
    });
  } catch (err) {
    // An AbortError is the caller cancelling, not the backend failing — let
    // it through untouched so callers can tell the two apart.
    if (err instanceof DOMException && err.name === "AbortError") throw err;
    throw new ApiError("offline", messageForKind("offline", null));
  }

  if (!res.ok) {
    const detail = await res.text().catch(() => "");
    const kind = kindForStatus(res.status);
    throw new ApiError(kind, messageForKind(kind, detail || null), res.status, detail || null);
  }

  return res;
}

/** JSON request. Throws `ApiError` on any non-2xx or network failure. */
export async function requestJson<T>(path: string, opts: RequestOptions = {}): Promise<T> {
  const res = await rawRequest(path, opts);
  return (await res.json()) as T;
}

/** Text request — used by `/health` and `/metrics`, which are not JSON. */
export async function requestText(path: string, opts: RequestOptions = {}): Promise<string> {
  const res = await rawRequest(path, opts);
  return res.text();
}

/** Request with no response body (204). */
export async function requestVoid(path: string, opts: RequestOptions = {}): Promise<void> {
  await rawRequest(path, opts);
}

/** Raw bytes — evidence content, which may be any content type. */
export async function requestBlob(path: string, opts: RequestOptions = {}): Promise<Blob> {
  const res = await rawRequest(path, opts);
  return res.blob();
}
