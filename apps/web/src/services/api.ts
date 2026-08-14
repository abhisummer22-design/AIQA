import {
  SystemHealthState,
  KpiMetrics,
  LiveExecution,
  FailureItem,
  BugCandidate,
  ModelRouterState,
  FlakyTestItem,
  TimelineEvent,
  AuditLogItem,
  GovernancePolicy,
  EvidenceItem,
  QualityGraphData,
  AlertItem,
  DashboardState,
  EnvironmentType,
  TestCase,
  TestType,
  TestCoverageItem,
  TestExecutionAnalytics,
  TestRecommendation
} from "../types";


export const API_BASE = "";

// Initial Fallback state
export const initialDashboardState: DashboardState = {
  systemHealth: {
    backend: "online",
    frontend: "online",
    database: "connected",
    aiModel: "ready",
    ollama: "connected",
    apiConnectivity: 99.98,
    testExecutor: "running",
    queue: { status: "healthy", pending: 8 },
    worker: { active: 4, total: 4, status: "healthy" },
    voiceMic: "ready",
    lastChecked: new Date().toISOString()
  },
  kpiMetrics: {
    environment: "staging",
    totalTests: {
      total: 4892,
      executedToday: 1420,
      passPercentage: 96.8,
      trend: "+1.4%"
    },
    testRuns: {
      running: 4,
      completed: 128,
      failed: 6
    },
    qualityScore: {
      score: 87.4,
      trend: "+4.2%",
      historical: [
        { date: "Mon", score: 81.2 },
        { date: "Tue", score: 82.5 },
        { date: "Wed", score: 84.1 },
        { date: "Thu", score: 83.9 },
        { date: "Fri", score: 85.8 },
        { date: "Sat", score: 86.2 },
        { date: "Sun", score: 87.4 }
      ]
    },
    riskScore: {
      level: "Medium",
      value: 42,
      criticalCount: 2,
      highCount: 5,
      mediumCount: 12,
      lowCount: 28
    },
    flakiness: {
      percentage: 8.7,
      trend: "-1.1%",
      flakyTestCount: 4
    },
    openBugs: {
      critical: 2,
      high: 7,
      medium: 14,
      low: 9,
      total: 32
    }
  },
  liveExecutions: [
    {
      id: "exec-1",
      testName: "Auth & RBAC Security Fuzzing Suite",
      project: "Apex AI Agent",
      environment: "Staging",
      executor: "Schemathesis / OpenAPI",
      status: "running",
      durationSec: 145,
      passed: 124,
      failed: 2,
      total: 150,
      progress: 84,
      startTime: "10:14:20 AM",
      logs: [
        "[INFO] Fuzzing endpoint POST /v1/auth/tokens with 500 mutation strategies",
        "[PASS] Schema check passed for 200 OK responses",
        "[FAIL] HTTP 403 response missing Content-Security-Policy header"
      ]
    },
    {
      id: "exec-2",
      testName: "Payment Gateway Idempotency & Webhooks",
      project: "Payment Core API",
      environment: "Production",
      executor: "Playwright API + PyTest",
      status: "running",
      durationSec: 88,
      passed: 64,
      failed: 1,
      total: 80,
      progress: 72,
      startTime: "10:15:10 AM",
      logs: [
        "[INFO] Spawning 20 concurrent checkout idempotency requests",
        "[PASS] Webhook HMAC signature verification succeeded",
        "[WARN] DB transaction lock time elevated (380ms)"
      ]
    },
    {
      id: "exec-3",
      testName: "RAG Pipeline Grounding & Hallucination Evals",
      project: "Search RAG Pipeline",
      environment: "Staging",
      executor: "DeepEval / LLM-as-Judge",
      status: "running",
      durationSec: 210,
      passed: 48,
      failed: 0,
      total: 50,
      progress: 96,
      startTime: "10:12:05 AM",
      logs: [
        "[INFO] Evaluating faithfulness against ground truth corpus",
        "[PASS] Semantic similarity score: 0.94",
        "[PASS] Context recall verified across 100 documents"
      ]
    }
  ],
  qualityGraph: {
    nodes: [
      { id: "srv-payment", label: "Payment Gateway", type: "service", status: "degraded", risk: "high", count: 8 },
      { id: "srv-auth", label: "Auth & RBAC Service", type: "service", status: "healthy", risk: "low", count: 12 },
      { id: "srv-rag", label: "Search & RAG Engine", type: "service", status: "healthy", risk: "low", count: 6 },
      { id: "api-checkout", label: "POST /v1/checkout/session", type: "api", status: "failing", latency: "420ms" },
      { id: "api-tokens", label: "POST /v1/auth/tokens", type: "api", status: "healthy", latency: "95ms" },
      { id: "test-idempotency", label: "Concurrent Idempotency Test", type: "test", status: "failed", flakiness: "12%" },
      { id: "test-fuzz", label: "Contract Schemathesis Fuzz", type: "test", status: "healthy", flakiness: "0.2%" },
      { id: "bug-deadlock", label: "PostgreSQL Mutex Deadlock", type: "bug", severity: "Critical" }
    ],
    edges: [
      { from: "srv-payment", to: "api-checkout", relationship: "exposes" },
      { from: "api-checkout", to: "test-idempotency", relationship: "verified_by" },
      { from: "test-idempotency", to: "bug-deadlock", relationship: "caused" }
    ]
  },
  recentFailures: [
    {
      id: "fail-1",
      testName: "Payment Gateway Idempotency & Webhooks",
      failureType: "API error",
      httpStatus: 500,
      errorMessage: "PostgreSQL Deadlock detected on table 'payment_transactions' during concurrent update with row locks.",
      environment: "Production",
      timestamp: "10:14:52 AM",
      riskLevel: "Critical",
      suggestedAction: "Implement optimistic locking with retry backoff in payment repository.",
      service: "Payment Gateway",
      relatedTest: "test_concurrent_payment_idempotency"
    },
    {
      id: "fail-2",
      testName: "Auth & RBAC Security Fuzzing Suite",
      failureType: "Schema violation",
      httpStatus: 403,
      errorMessage: "Header validation mismatch: Content-Security-Policy expected but received 'X-Frame-Options: SAMEORIGIN'.",
      environment: "Staging",
      timestamp: "10:12:18 AM",
      riskLevel: "Medium",
      suggestedAction: "Update Nginx reverse-proxy ingress header rules for staging cluster.",
      service: "Auth & RBAC",
      relatedTest: "test_oauth_security_headers"
    }
  ],
  bugCandidates: [
    {
      id: "cand-1",
      title: "Payment API returns 500 during concurrent race condition requests",
      severity: "Critical",
      confidence: 94,
      relatedTest: "Payment Gateway Idempotency",
      relatedService: "Payment Gateway",
      evidenceCount: 17,
      suggestedPriority: "P1 - Blocker",
      description: "Automated analysis identified deadlock exceptions across 14 consecutive runs under 50+ RPS concurrency load. Database logs confirm lock contention.",
      createdAt: "3 mins ago",
      status: "pending_review"
    },
    {
      id: "cand-2",
      title: "Missing CORS preflight OPTIONS header on /v1/search/rag-query",
      severity: "High",
      confidence: 89,
      relatedTest: "Schemathesis API Contract",
      relatedService: "Search & RAG Engine",
      evidenceCount: 8,
      suggestedPriority: "P2 - Normal",
      description: "Options request returns 404 instead of 204 No Content for external origin origins.",
      createdAt: "18 mins ago",
      status: "pending_review"
    }
  ],
  evidence: [
    {
      id: "ev-1",
      title: "Concurrent Deadlock Stack Trace & Query Plan",
      type: "trace",
      testName: "Payment Gateway Idempotency",
      timestamp: "10:14:52 AM",
      environment: "Production",
      source: "PostgreSQL Engine Probe",
      relatedFailure: "fail-1",
      size: "48 KB",
      content: `ERROR: deadlock detected\nDETAIL: Process 4182 waits for ExclusiveLock on tuple (48, 12) of relation 16422; blocked by process 4185.\nProcess 4185 waits for ShareLock on transaction 889211; blocked by process 4182.\nHINT: See server log for query details.\nCONTEXT: while updating tuple (48, 12) in relation "payment_transactions"\nSTATEMENT: UPDATE payment_transactions SET status = 'CAPTURED' WHERE idempotency_key = $1;`
    },
    {
      id: "ev-2",
      title: "HTTP 500 Raw Request & Response Payload",
      type: "payload",
      testName: "Payment Gateway Idempotency",
      timestamp: "10:14:52 AM",
      environment: "Production",
      source: "Express Reverse Proxy Ingress",
      relatedFailure: "fail-1",
      size: "12 KB",
      content: `POST /v1/checkout/session HTTP/1.1\nHost: api.enterprise.internal\nIdempotency-Key: 8c34f2a9-7c6b-4e12-b91c-12f849a9bc21\nContent-Type: application/json\n\n{"amount": 14900, "currency": "USD", "items": [{"id": "sku_premium_annual", "quantity": 1}]}\n\nHTTP/1.1 500 Internal Server Error\nContent-Type: application/json; charset=utf-8\n\n{"error": "InternalServerError", "message": "Database transaction aborted due to concurrency conflict", "code": "DB_MUTEX_DEADLOCK"}`
    },
    {
      id: "ev-3",
      title: "RAG Cosine Distance Vector Search Logs",
      type: "log",
      testName: "RAG Pipeline Grounding",
      timestamp: "10:12:05 AM",
      environment: "Staging",
      source: "VectorDB & Gemma 3 Embedding Service",
      relatedFailure: "",
      size: "84 KB",
      content: `[INFO] Embed query: "refund policy for international orders"\n[INFO] Generated embedding vector dim: 768\n[INFO] Top-5 retrieved cosine scores: [0.942, 0.918, 0.895, 0.881, 0.874]\n[INFO] Evaluator faithfulness test passed with threshold 0.85`
    }
  ],
  modelRouter: {
    primaryModel: {
      name: "Gemma 3 12B",
      runtime: "Ollama (Local AI Runtime)",
      status: "online",
      healthPercent: 99.4,
      memoryUsageMb: 8192,
      cpuPercent: 34.2,
      gpuPercent: 48.0,
      modelLoadTimeMs: 112,
      inferenceLatencyMs: 420,
      requestsPerMin: 148
    },
    fallbackModel: {
      name: "Gemini 3.7 Flash",
      runtime: "Google AI Cloud",
      status: "available",
      healthPercent: 100,
      inferenceLatencyMs: 290
    },
    currentActiveModel: "Gemma 3 12B",
    currentLatencyMs: 420,
    requestsPerMinute: 182,
    averageLatencyMs: 385,
    successRate: 99.1,
    errorRate: 0.9,
    fallbackCountToday: 3,
    routingStrategy: "Local First -> Cloud Fallback on Latency > 1200ms or 5xx"
  },
  flakyTests: [
    {
      id: "flaky-1",
      testName: "Concurrent Checkout Race Condition Idempotency",
      flakinessPercent: 18.4,
      failureFrequency: "14 / 44 runs",
      lastFailure: "6 mins ago",
      retries: 3,
      quarantined: false,
      service: "Payment Gateway",
      trend: "increasing"
    },
    {
      id: "flaky-2",
      testName: "OAuth2 SSO Refresh Token Expiry Race",
      flakinessPercent: 12.1,
      failureFrequency: "8 / 52 runs",
      lastFailure: "45 mins ago",
      retries: 2,
      quarantined: true,
      service: "Auth & RBAC",
      trend: "stable"
    },
    {
      id: "flaky-3",
      testName: "Search Vector Index Cache Invalidation",
      flakinessPercent: 9.8,
      failureFrequency: "5 / 60 runs",
      lastFailure: "2 hours ago",
      retries: 1,
      quarantined: false,
      service: "Search Pipeline",
      trend: "decreasing"
    }
  ],
  timeline: [
    { time: "10:15:32", event: "Test Run Completed", category: "execution", detail: "RAG Evaluation Suite passed (48/50 tests)", level: "info" },
    { time: "10:14:52", event: "Deadlock Exception Caught", category: "failure", detail: "POST /v1/checkout/session failed with HTTP 500", level: "error" },
    { time: "10:13:10", event: "Model Router Health Check", category: "ai", detail: "Ollama Gemma 3 12B latency nominal at 420ms", level: "info" },
    { time: "10:10:44", event: "Auto Quarantine Policy", category: "risk", detail: "OAuth2 Token Expiry test isolated from PR blocking gate", level: "warn" }
  ],
  governance: [
    {
      id: "gov-1",
      name: "Production Destructive API Protection",
      status: "Active",
      violations: 0,
      blockedExecutions: 0,
      severity: "Critical",
      description: "Blocks DELETE/DROP methods on live production endpoints without signed authorization."
    },
    {
      id: "gov-2",
      name: "Flakiness Release Blocker Threshold",
      status: "Active",
      violations: 1,
      blockedExecutions: 3,
      severity: "High",
      description: "Any release candidate with overall flakiness score > 10% requires QA Lead sign-off."
    },
    {
      id: "gov-3",
      name: "AI Model Hallucination SLA Policy",
      status: "Active",
      violations: 0,
      blockedExecutions: 0,
      severity: "High",
      description: "RAG retrieval faithfulness must remain above 90% across 500 benchmark queries."
    }
  ],
  auditLogs: [
    {
      id: "aud-1",
      timestamp: "10:15:32 AM",
      actor: "automated-agent-ci",
      action: "EXECUTE_TEST_SUITE",
      resource: "Apex AI Agent",
      result: "Success",
      ip: "10.128.0.4",
      tenant: "production-eu"
    },
    {
      id: "aud-2",
      timestamp: "10:14:55 AM",
      actor: "ai-auto-triage",
      action: "DRAFT_BUG_CANDIDATE",
      resource: "Payment Gateway / Mutex Deadlock",
      result: "Success",
      ip: "127.0.0.1",
      tenant: "production-eu"
    },
    {
      id: "aud-3",
      timestamp: "10:10:44 AM",
      actor: "sarah.chen@company.com",
      action: "QUARANTINE_TEST_CASE",
      resource: "OAuth2 SSO Refresh Token Expiry Race",
      result: "Success",
      ip: "192.168.1.104",
      tenant: "production-eu"
    }
  ],
  alerts: [
    {
      id: "alt-1",
      level: "CRITICAL",
      title: "Concurrent Deadlock on Payment API",
      message: "PostgreSQL lock contention detected on table 'payment_transactions' during checkout test run.",
      timestamp: "2 mins ago",
      component: "Payment Gateway"
    }
  ]
};

// Fetch full dashboard state
export async function fetchDashboardState(): Promise<DashboardState> {
  try {
    const [healthRes, metricsRes, execRes, graphRes, failRes, bugsRes, modRes, flakyRes, evidRes, timeRes, polRes, audRes] =
      await Promise.allSettled([
        fetch(`${API_BASE}/api/health`).then((r) => r.json()),
        fetch(`${API_BASE}/api/metrics`).then((r) => r.json()),
        fetch(`${API_BASE}/api/executions`).then((r) => r.json()),
        fetch(`${API_BASE}/api/quality-graph`).then((r) => r.json()),
        fetch(`${API_BASE}/api/failures`).then((r) => r.json()),
        fetch(`${API_BASE}/api/bugs`).then((r) => r.json()),
        fetch(`${API_BASE}/api/models`).then((r) => r.json()),
        fetch(`${API_BASE}/api/flaky-tests`).then((r) => r.json()),
        fetch(`${API_BASE}/api/evidence`).then((r) => r.json()),
        fetch(`${API_BASE}/api/timeline`).then((r) => r.json()),
        fetch(`${API_BASE}/api/policies`).then((r) => r.json()),
        fetch(`${API_BASE}/api/audit-logs`).then((r) => r.json())
      ]);

    return {
      systemHealth: healthRes.status === "fulfilled" && healthRes.value.systemHealth ? healthRes.value.systemHealth : initialDashboardState.systemHealth,
      kpiMetrics: metricsRes.status === "fulfilled" ? metricsRes.value : initialDashboardState.kpiMetrics,
      liveExecutions: execRes.status === "fulfilled" && execRes.value.executions ? execRes.value.executions : initialDashboardState.liveExecutions,
      qualityGraph: graphRes.status === "fulfilled" ? graphRes.value : initialDashboardState.qualityGraph,
      recentFailures: failRes.status === "fulfilled" && failRes.value.failures ? failRes.value.failures : initialDashboardState.recentFailures,
      bugCandidates: bugsRes.status === "fulfilled" && bugsRes.value.candidates ? bugsRes.value.candidates : initialDashboardState.bugCandidates,
      evidence: evidRes.status === "fulfilled" && evidRes.value.evidence ? evidRes.value.evidence : initialDashboardState.evidence,
      modelRouter: modRes.status === "fulfilled" && modRes.value.modelRouter ? modRes.value.modelRouter : initialDashboardState.modelRouter,
      flakyTests: flakyRes.status === "fulfilled" && flakyRes.value.flakyTests ? flakyRes.value.flakyTests : initialDashboardState.flakyTests,
      timeline: timeRes.status === "fulfilled" && timeRes.value.timeline ? timeRes.value.timeline : initialDashboardState.timeline,
      governance: polRes.status === "fulfilled" && polRes.value.policies ? polRes.value.policies : initialDashboardState.governance,
      auditLogs: audRes.status === "fulfilled" && audRes.value.auditLogs ? audRes.value.auditLogs : initialDashboardState.auditLogs,
      alerts: initialDashboardState.alerts
    };
  } catch (err) {
    console.warn("Using fallback state:", err);
    return initialDashboardState;
  }
}

// SSE Connection
export function connectSSE(onData: (data: Partial<DashboardState>) => void): () => void {
  let eventSource: EventSource | null = null;
  let pollInterval: any = null;

  try {
    eventSource = new EventSource(`${API_BASE}/api/stream`);
    eventSource.onmessage = (event) => {
      try {
        const payload = JSON.parse(event.data);
        if (payload.type === "health") {
          onData({ systemHealth: payload.data });
        } else if (payload.type === "executions") {
          onData({ liveExecutions: payload.data });
        } else if (payload.type === "metrics") {
          onData({ kpiMetrics: payload.data });
        } else if (payload.type === "modelRouter") {
          onData({ modelRouter: payload.data });
        }
      } catch (e) {
        console.error("SSE parse error", e);
      }
    };
  } catch (err) {
    console.warn("SSE not available, using periodic polling fallback.");
  }

  // Backup timer to keep counters animated smoothly
  pollInterval = setInterval(async () => {
    try {
      const res = await fetch(`${API_BASE}/api/executions`);
      if (res.ok) {
        const data = await res.json();
        onData({ liveExecutions: data.executions });
      }
    } catch {}
  }, 4000);

  return () => {
    if (eventSource) eventSource.close();
    if (pollInterval) clearInterval(pollInterval);
  };
}

export async function triggerTestRun(data: {
  testName: string;
  project: string;
  environment: string;
  executor: string;
}): Promise<{ success: boolean; execution: LiveExecution }> {
  const res = await fetch(`${API_BASE}/api/executions/trigger`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data)
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function toggleQuarantineTest(testId: string): Promise<{ success: boolean; quarantined: boolean }> {
  const res = await fetch(`${API_BASE}/api/flaky-tests/quarantine`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ testId })
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function createBugConfirmed(data: {
  candidateId: string;
  title: string;
  severity: string;
  priority: string;
  notes?: string;
}): Promise<{ success: boolean; bug: any }> {
  const res = await fetch(`${API_BASE}/api/bugs/create-from-candidate`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data)
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function simulateChaosComponent(component: string, status: string): Promise<{ success: boolean }> {
  const res = await fetch(`${API_BASE}/api/chaos-simulate`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ component, status })
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function dismissAlertItem(id: string): Promise<{ success: boolean }> {
  return { success: true };
}

// ==========================================
// --- RBAC API Client Methods ---
// ==========================================

export async function fetchRbacBootstrap(): Promise<any> {
  const res = await fetch(`${API_BASE}/api/rbac/bootstrap`);
  if (!res.ok) throw new Error(`Failed to fetch RBAC bootstrap: HTTP ${res.status}`);
  return res.json();
}

export async function fetchRbacUsers(): Promise<{ users: any[] }> {
  const res = await fetch(`${API_BASE}/api/rbac/users`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function createRbacUser(data: {
  email: string;
  name: string;
  roleId: string;
  tenantId?: string;
  projectIds?: string[];
}): Promise<{ success: boolean; user: any }> {
  const res = await fetch(`${API_BASE}/api/rbac/users`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data)
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: `HTTP ${res.status}` }));
    throw new Error(err.error || `HTTP ${res.status}`);
  }
  return res.json();
}

export async function updateRbacUserRole(userId: string, roleId: string): Promise<{ success: boolean; user: any }> {
  const res = await fetch(`${API_BASE}/api/rbac/users/${userId}/role`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ roleId })
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: `HTTP ${res.status}` }));
    throw new Error(err.error || `HTTP ${res.status}`);
  }
  return res.json();
}

export async function updateRbacUserProjects(userId: string, projectIds: string[]): Promise<{ success: boolean; user: any }> {
  const res = await fetch(`${API_BASE}/api/rbac/users/${userId}/projects`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ projectIds })
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: `HTTP ${res.status}` }));
    throw new Error(err.error || `HTTP ${res.status}`);
  }
  return res.json();
}

export async function toggleRbacUserStatus(userId: string, status: "active" | "disabled"): Promise<{ success: boolean; user: any }> {
  const res = await fetch(`${API_BASE}/api/rbac/users/${userId}/status`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ status })
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: `HTTP ${res.status}` }));
    throw new Error(err.error || `HTTP ${res.status}`);
  }
  return res.json();
}

export async function deleteRbacUser(userId: string): Promise<{ success: boolean; deletedUserId: string }> {
  const res = await fetch(`${API_BASE}/api/rbac/users/${userId}`, {
    method: "DELETE"
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: `HTTP ${res.status}` }));
    throw new Error(err.error || `HTTP ${res.status}`);
  }
  return res.json();
}

export async function fetchRbacRoles(): Promise<{ roles: any[] }> {
  const res = await fetch(`${API_BASE}/api/rbac/roles`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function createRbacRole(data: {
  name: string;
  description: string;
  permissions: string[];
}): Promise<{ success: boolean; role: any }> {
  const res = await fetch(`${API_BASE}/api/rbac/roles`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data)
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: `HTTP ${res.status}` }));
    throw new Error(err.error || `HTTP ${res.status}`);
  }
  return res.json();
}

export async function updateRbacRole(
  roleId: string,
  data: { name?: string; description?: string; permissions: string[] }
): Promise<{ success: boolean; role: any }> {
  const res = await fetch(`${API_BASE}/api/rbac/roles/${roleId}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data)
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: `HTTP ${res.status}` }));
    throw new Error(err.error || `HTTP ${res.status}`);
  }
  return res.json();
}

export async function deleteRbacRole(roleId: string): Promise<{ success: boolean; deletedRoleId: string }> {
  const res = await fetch(`${API_BASE}/api/rbac/roles/${roleId}`, {
    method: "DELETE"
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: `HTTP ${res.status}` }));
    throw new Error(err.error || `HTTP ${res.status}`);
  }
  return res.json();
}

export async function switchRbacSessionUser(userId: string): Promise<{ success: boolean; user: any; role: any; grantedPermissions: string[] }> {
  const res = await fetch(`${API_BASE}/api/rbac/switch-session`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ userId })
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function probeRbacAuthorization(payload: {
  roleId?: string;
  userId?: string;
  method: string;
  path: string;
  requiredPermission: string;
  targetTenantId?: string;
  targetProjectId?: string;
}): Promise<any> {
  const res = await fetch(`${API_BASE}/api/rbac/probe-authorization`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload)
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function runRbacSecuritySuite(): Promise<any> {
  const res = await fetch(`${API_BASE}/api/rbac/run-security-suite`, {
    method: "POST",
    headers: { "Content-Type": "application/json" }
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function fetchRbacAuditEvents(params?: {
  action?: string;
  result?: string;
  actor?: string;
  tenantId?: string;
}): Promise<{ events: any[]; total: number; filteredCount: number }> {
  const url = new URL(`${API_BASE}/api/rbac/audit-events`);
  if (params) {
    if (params.action) url.searchParams.set("action", params.action);
    if (params.result) url.searchParams.set("result", params.result);
    if (params.actor) url.searchParams.set("actor", params.actor);
    if (params.tenantId) url.searchParams.set("tenantId", params.tenantId);
  }
  const res = await fetch(url.toString());
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function simulateRbacAuditEvent(scenario: string): Promise<{ success: boolean; event: any }> {
  const res = await fetch(`${API_BASE}/api/rbac/audit-events/simulate`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ scenario })
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

// ==========================================
// --- AI BRAIN & ENGINE SERVICE CLIENT ---
// ==========================================

export async function fetchBrainState(): Promise<{
  routingMode: any;
  ollama: any;
  localModels: any[];
  cloudProviders: any[];
  taskRoutings: any[];
  telemetry: any;
  benchmarkHistory: any[];
}> {
  const res = await fetch(`${API_BASE}/api/brain/state`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function updateBrainRoutingMode(mode: string): Promise<{ success: boolean; routingMode: string }> {
  const res = await fetch(`${API_BASE}/api/brain/routing-mode`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ mode })
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function updateOllamaDaemonConfig(payload: {
  baseUrl?: string;
  hardware?: Record<string, any>;
}): Promise<any> {
  const res = await fetch(`${API_BASE}/api/brain/ollama/config`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload)
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function setPrimaryOllamaModel(modelId: string): Promise<any> {
  const res = await fetch(`${API_BASE}/api/brain/ollama/set-primary`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ modelId })
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function pullOllamaModel(modelName: string): Promise<any> {
  const res = await fetch(`${API_BASE}/api/brain/ollama/pull`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ modelName })
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function updateProviderApiKey(payload: {
  providerId: string;
  apiKey?: string;
  customBaseUrl?: string;
  selectedModel?: string;
}): Promise<any> {
  const res = await fetch(`${API_BASE}/api/brain/providers/key`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload)
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function testProviderConnection(providerId: string): Promise<any> {
  const res = await fetch(`${API_BASE}/api/brain/providers/test`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ providerId })
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function updateTaskRoutingMatrix(routings: any[]): Promise<any> {
  const res = await fetch(`${API_BASE}/api/brain/task-routing`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ routings })
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function runBrainBenchmark(payload: {
  engine?: string;
  model?: string;
  prompt?: string;
}): Promise<{ success: boolean; result: any; telemetry: any }> {
  const res = await fetch(`${API_BASE}/api/brain/benchmark`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload)
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function runAbBrainBenchmark(payload: {
  prompt: string;
  modelA: { engine: string; model: string; endpointUrl?: string };
  modelB: { engine: string; model: string; endpointUrl?: string };
  scenarioTitle?: string;
}): Promise<{ success: boolean; comparison: any; telemetry: any }> {
  const res = await fetch(`${API_BASE}/api/brain/benchmark/ab-test`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload)
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

// ==========================================
// --- TEST CASE EXECUTION HUB API CALLS ---
// ==========================================

export async function fetchTestCases(filters?: {
  test_type?: string;
  status?: string;
  priority?: string;
  environment?: string;
  owner_id?: string;
  search?: string;
  project_id?: string;
}): Promise<{ testCases: TestCase[]; total: number }> {
  const params = new URLSearchParams();
  if (filters?.test_type && filters.test_type !== "ALL") params.append("test_type", filters.test_type);
  if (filters?.status && filters.status !== "all") params.append("status", filters.status);
  if (filters?.priority && filters.priority !== "all") params.append("priority", filters.priority);
  if (filters?.environment && filters.environment !== "all") params.append("environment", filters.environment);
  if (filters?.owner_id && filters.owner_id !== "all") params.append("owner_id", filters.owner_id);
  if (filters?.project_id && filters.project_id !== "all") params.append("project_id", filters.project_id);
  if (filters?.search) params.append("search", filters.search);

  const query = params.toString() ? `?${params.toString()}` : "";
  const res = await fetch(`${API_BASE}/api/test-cases${query}`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function fetchTestCaseById(id: string): Promise<{ testCase: TestCase }> {
  const res = await fetch(`${API_BASE}/api/test-cases/${id}`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function createTestCaseApi(data: Partial<TestCase>): Promise<{ success: boolean; testCase: TestCase }> {
  const res = await fetch(`${API_BASE}/api/test-cases`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data)
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function updateTestCaseApi(id: string, updates: Partial<TestCase>): Promise<{ success: boolean; testCase: TestCase }> {
  const res = await fetch(`${API_BASE}/api/test-cases/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(updates)
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function deleteTestCaseApi(id: string): Promise<{ success: boolean; id: string }> {
  const res = await fetch(`${API_BASE}/api/test-cases/${id}`, {
    method: "DELETE"
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function executeTestCasesApi(options: {
  testIds?: string[];
  testType?: TestType | "ALL";
  environment?: string;
}): Promise<{
  success: boolean;
  executionId: string;
  totalExecuted: number;
  passed: number;
  failed: number;
  blocked: number;
  skipped: number;
  durationMs: number;
  testCases: TestCase[];
}> {
  const res = await fetch(`${API_BASE}/api/test-cases/execute`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(options)
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function fetchTestCoverage(): Promise<{ coverage: TestCoverageItem[] }> {
  const res = await fetch(`${API_BASE}/api/test-cases/coverage`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function fetchTestAnalytics(testType?: string): Promise<{ analytics: TestExecutionAnalytics }> {
  const query = testType && testType !== "ALL" ? `?test_type=${testType}` : "";
  const res = await fetch(`${API_BASE}/api/test-cases/analytics${query}`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function fetchTestRecommendations(): Promise<{ recommendations: TestRecommendation[] }> {
  const res = await fetch(`${API_BASE}/api/test-cases/recommendations`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function generateAiTestCasesApi(params: {
  testType: TestType;
  targetFeature: string;
  contextPrompt?: string;
  count?: number;
}): Promise<{ success: boolean; generated: TestCase[]; count: number }> {
  const res = await fetch(`${API_BASE}/api/test-cases/generate`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(params)
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}



