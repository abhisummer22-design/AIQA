/**
 * Typed client for the AIQA orchestrator's real API surface.
 *
 * This file contains exactly the endpoints that exist in
 * `services/orchestrator/src/api.rs` — 16 HTTP routes and one WebSocket —
 * and nothing else. If a UI feature needs something that is not in this
 * file, the backend capability does not exist; the correct response is a
 * "Not Implemented" state, not a new function here backed by invented data.
 *
 * Endpoint inventory (verified against api.rs::router):
 *   GET    /health                              (unauthenticated, text "ok")
 *   GET    /runs?limit=                         viewer+
 *   POST   /runs?project_id=                    operator+
 *   GET    /runs/:id                            viewer+
 *   GET    /runs/:id/events?since=              viewer+
 *   POST   /runs/:id/approve                    operator+
 *   POST   /runs/:id/reject                     operator+
 *   GET    /runs/:id/evidence                   viewer+
 *   GET    /evidence/:id/content                viewer+
 *   GET    /intelligence/tests                  viewer+
 *   GET    /intelligence/failure-clusters       viewer+
 *   GET    /bug-drafts                          viewer+
 *   POST   /bug-drafts/:id/review               operator+
 *   GET    /metrics                             viewer+ (Prometheus text)
 *   POST   /admin/api-keys                      admin
 *   GET    /admin/api-keys                      admin
 *   DELETE /admin/api-keys/:id                  admin
 *   GET    /admin/audit-log?limit=              admin
 *   WS     /runs/:id/ws?key=                    viewer+
 */

import {
  API_BASE,
  ApiError,
  getApiKey,
  requestBlob,
  requestJson,
  requestText,
  requestVoid,
} from "./apiClient";
import type {
  ApiKeyListRow,
  ApiRole,
  AuditRow,
  BugDraftRow,
  CreateRunResponse,
  EvidenceRow,
  FailureClusterRow,
  OrchestratorMetrics,
  RunDetail,
  RunEvent,
  RunListItem,
  RunPlan,
  RunSummary,
  TestIntelRow,
} from "../types/orchestrator";

// ------------------------------------------------------------------ health

export type HealthStatus = "online" | "offline";

/**
 * `GET /health` returns the literal string "ok" and is the only
 * unauthenticated route. Used to distinguish "orchestrator is down" from
 * "orchestrator is up but my key is bad", which are different screens.
 */
export async function checkHealth(signal?: AbortSignal): Promise<HealthStatus> {
  try {
    const body = await requestText("/health", { skipAuth: true, signal });
    return body.trim() === "ok" ? "online" : "offline";
  } catch (err) {
    if (err instanceof ApiError) return "offline";
    throw err;
  }
}

// -------------------------------------------------------------------- runs

export function listRuns(limit = 50, signal?: AbortSignal): Promise<RunListItem[]> {
  return requestJson<RunListItem[]>(`/runs?limit=${encodeURIComponent(limit)}`, { signal });
}

export function getRun(runId: string, signal?: AbortSignal): Promise<RunDetail> {
  return requestJson<RunDetail>(`/runs/${encodeURIComponent(runId)}`, { signal });
}

/**
 * `POST /runs` — operator+. The plan is validated server-side
 * (`scheduler::validate_plan`) and then passed through the policy engine,
 * which may return the run in `PENDING_APPROVAL` rather than scheduling it.
 * A 403 here means the policy engine *denied* the run outright.
 */
export function createRun(plan: RunPlan, projectId?: string, signal?: AbortSignal): Promise<CreateRunResponse> {
  const q = projectId ? `?project_id=${encodeURIComponent(projectId)}` : "";
  return requestJson<CreateRunResponse>(`/runs${q}`, { method: "POST", body: plan, signal });
}

/** `GET /runs/:id/events?since=` — SRD §18 reconnect path. `since` is the
 *  last sequence number the client already has. */
export function getRunEvents(runId: string, since = 0, signal?: AbortSignal): Promise<RunEvent[]> {
  return requestJson<RunEvent[]>(
    `/runs/${encodeURIComponent(runId)}/events?since=${encodeURIComponent(since)}`,
    { signal },
  );
}

export function approveRun(runId: string, signal?: AbortSignal): Promise<RunSummary> {
  return requestJson<RunSummary>(`/runs/${encodeURIComponent(runId)}/approve`, { method: "POST", signal });
}

export function rejectRun(runId: string, reason?: string, signal?: AbortSignal): Promise<RunSummary> {
  return requestJson<RunSummary>(`/runs/${encodeURIComponent(runId)}/reject`, {
    method: "POST",
    body: { reason: reason ?? null },
    signal,
  });
}

// ---------------------------------------------------------------- evidence

export function listRunEvidence(runId: string, signal?: AbortSignal): Promise<EvidenceRow[]> {
  return requestJson<EvidenceRow[]>(`/runs/${encodeURIComponent(runId)}/evidence`, { signal });
}

/** `GET /evidence/:id/content` — returns the stored artifact bytes with the
 *  content type recorded at capture time. */
export function getEvidenceContent(evidenceId: string, signal?: AbortSignal): Promise<Blob> {
  return requestBlob(`/evidence/${encodeURIComponent(evidenceId)}/content`, { signal });
}

/** Convenience for text-ish evidence (logs, traces). */
export async function getEvidenceText(evidenceId: string, signal?: AbortSignal): Promise<string> {
  const blob = await getEvidenceContent(evidenceId, signal);
  return blob.text();
}

// ------------------------------------------------------------ intelligence

export function listTestIntelligence(signal?: AbortSignal): Promise<TestIntelRow[]> {
  return requestJson<TestIntelRow[]>("/intelligence/tests", { signal });
}

export function listFailureClusters(signal?: AbortSignal): Promise<FailureClusterRow[]> {
  return requestJson<FailureClusterRow[]>("/intelligence/failure-clusters", { signal });
}

export function listBugDrafts(signal?: AbortSignal): Promise<BugDraftRow[]> {
  return requestJson<BugDraftRow[]>("/bug-drafts", { signal });
}

/**
 * `POST /bug-drafts/:id/review` — the human-gated anomaly -> confirmed-defect
 * promotion step (PRD §14). Never happens automatically; always audited.
 */
export function reviewBugDraft(
  bugDraftId: string,
  action: "confirm" | "dismiss",
  signal?: AbortSignal,
): Promise<void> {
  return requestVoid(`/bug-drafts/${encodeURIComponent(bugDraftId)}/review`, {
    method: "POST",
    body: { action },
    signal,
  });
}

// ----------------------------------------------------------------- metrics

/**
 * Minimal Prometheus text-exposition parser for `GET /metrics`.
 *
 * The orchestrator emits counters computed on demand from the database
 * (`metrics.rs`) — real numbers, but in Prometheus format rather than JSON,
 * so the UI has to parse rather than invent. Only the metric families the
 * orchestrator actually emits are read here.
 */
export function parsePrometheus(text: string): OrchestratorMetrics {
  const runsByState: Record<string, number> = {};
  const tasksByStatus: Record<string, number> = {};
  const bugDraftsByReviewState: Record<string, number> = {};
  let failureClustersTotal: number | null = null;
  let apiKeysActive: number | null = null;
  let auditLogTotal: number | null = null;

  for (const rawLine of text.split("\n")) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#")) continue;

    const spaceIdx = line.lastIndexOf(" ");
    if (spaceIdx === -1) continue;
    const name = line.slice(0, spaceIdx);
    const value = Number(line.slice(spaceIdx + 1));
    if (!Number.isFinite(value)) continue;

    const labelMatch = name.match(/^([a-z_]+)\{([a-z_]+)="([^"]*)"\}$/);
    if (labelMatch) {
      const [, family, , labelValue] = labelMatch;
      if (family === "aiqa_runs_total") runsByState[labelValue] = value;
      else if (family === "aiqa_tasks_total") tasksByStatus[labelValue] = value;
      else if (family === "aiqa_bug_drafts_total") bugDraftsByReviewState[labelValue] = value;
      continue;
    }

    if (name === "aiqa_failure_clusters_total") failureClustersTotal = value;
    else if (name === "aiqa_api_keys_active") apiKeysActive = value;
    else if (name === "aiqa_audit_log_total") auditLogTotal = value;
  }

  return {
    runsByState,
    tasksByStatus,
    bugDraftsByReviewState,
    failureClustersTotal,
    apiKeysActive,
    auditLogTotal,
  };
}

export async function getMetrics(signal?: AbortSignal): Promise<OrchestratorMetrics> {
  return parsePrometheus(await requestText("/metrics", { signal }));
}

// ------------------------------------------------------------------- admin

export function listApiKeys(signal?: AbortSignal): Promise<ApiKeyListRow[]> {
  return requestJson<ApiKeyListRow[]>("/admin/api-keys", { signal });
}

/** Returns the plaintext key exactly once, at creation (same convention as
 *  GitHub/Stripe tokens) — only its hash is persisted server-side. */
export function createApiKey(
  body: { name: string; role: ApiRole; project_id?: string },
  signal?: AbortSignal,
): Promise<{ id: string; key: string }> {
  return requestJson<{ id: string; key: string }>("/admin/api-keys", { method: "POST", body, signal });
}

export function revokeApiKey(id: string, signal?: AbortSignal): Promise<void> {
  return requestVoid(`/admin/api-keys/${encodeURIComponent(id)}`, { method: "DELETE", signal });
}

export function getAuditLog(limit = 100, signal?: AbortSignal): Promise<AuditRow[]> {
  return requestJson<AuditRow[]>(`/admin/audit-log?limit=${encodeURIComponent(limit)}`, { signal });
}

// --------------------------------------------------------------- websocket

/**
 * `WS /runs/:id/ws?key=` — the orchestrator authenticates the socket via a
 * query parameter because browsers cannot set an `Authorization` header on a
 * WebSocket handshake (`new WebSocket(url)` takes no headers argument). This
 * is the backend's own documented design (see `api.rs::ws_handler`), not a
 * shortcut taken here.
 *
 * On connect the server replays the run's durable event log first, then
 * streams new events, so a client that connects late still sees everything.
 * Every frame is a `RunEvent`.
 *
 * Returns `null` when there is no API key, so the caller renders an
 * unauthenticated state rather than opening a socket that will be closed.
 */
export function openRunEventSocket(
  runId: string,
  handlers: {
    onEvent: (event: RunEvent) => void;
    onOpen?: () => void;
    onClose?: (code: number) => void;
    onError?: () => void;
  },
): WebSocket | null {
  const key = getApiKey();
  if (!key) return null;

  const base = API_BASE.startsWith("http")
    ? API_BASE.replace(/^http/, "ws")
    : `${window.location.protocol === "https:" ? "wss:" : "ws:"}//${window.location.host}${API_BASE}`;

  const url = `${base}/runs/${encodeURIComponent(runId)}/ws?key=${encodeURIComponent(key)}`;

  let socket: WebSocket;
  try {
    socket = new WebSocket(url);
  } catch {
    handlers.onError?.();
    return null;
  }

  socket.onopen = () => handlers.onOpen?.();
  socket.onerror = () => handlers.onError?.();
  socket.onclose = (ev) => handlers.onClose?.(ev.code);
  socket.onmessage = (message) => {
    try {
      handlers.onEvent(JSON.parse(message.data) as RunEvent);
    } catch {
      // A malformed frame is dropped rather than taking the stream down.
      // It is never replaced with a synthesized event.
    }
  };

  return socket;
}
