import React, { useState } from "react";
import {
  X,
  CheckCircle2,
  XCircle,
  Copy,
  Check,
  Download,
  Terminal,
  Zap,
  Shield,
  Layers,
  ArrowRight,
  Code2,
  Server,
  Lock,
  KeyRound,
  RefreshCw,
  Clock,
  Radio,
  FileCode,
  Sliders
} from "lucide-react";
import { RoleConfig, RoleId, LiveRbacEvent } from "./types";

interface FlowInspectorOverlayProps {
  roleId: RoleId | null;
  role: RoleConfig | undefined;
  allRoles: RoleConfig[];
  latestEvent: LiveRbacEvent | null;
  allEvents: LiveRbacEvent[];
  onClose: () => void;
  onSelectRole: (roleId: RoleId) => void;
  onReProbe: (role: RoleConfig) => void;
}

type TabType = "all" | "request" | "response" | "jwt_token" | "headers";

export const FlowInspectorOverlay: React.FC<FlowInspectorOverlayProps> = ({
  roleId,
  role,
  allRoles,
  latestEvent,
  allEvents,
  onClose,
  onSelectRole,
  onReProbe
}) => {
  const [activeTab, setActiveTab] = useState<TabType>("all");
  const [copied, setCopied] = useState<boolean>(false);

  if (!roleId || !role) return null;

  // Filter events for this specific pipe
  const roleEvents = allEvents.filter((e) => e.roleId === roleId);
  const currentEvent = latestEvent && latestEvent.roleId === roleId ? latestEvent : roleEvents[0];

  const isAuth = currentEvent ? currentEvent.result === "AUTHORIZED" : true;
  const statusCode = currentEvent ? currentEvent.statusCode : 200;

  // Synthesize detailed Raw JSON Request & Response Payloads
  const requestPayload = {
    protocol: "HTTP/2.0 (TLS 1.3)",
    method: currentEvent ? (currentEvent.action.includes("Delete") ? "DELETE" : currentEvent.action.includes("View") ? "GET" : "POST") : "POST",
    path: currentEvent ? currentEvent.resource : "/api/rbac/verify",
    endpoint: currentEvent ? currentEvent.resource : "/api/rbac/verify",
    headers: {
      "Host": "api.aiqa.internal",
      "Authorization": `Bearer ${role.sessionToken}`,
      "Content-Type": "application/json",
      "X-AIQA-Tenant-ID": role.tenant,
      "X-Client-Role": role.id,
      "X-Request-ID": currentEvent ? currentEvent.id : `req-${Date.now()}`,
      "X-Forwarded-For": "10.244.3.112",
      "User-Agent": "AIQA-ControlCenter-LiveProbe/2.4 (React/Vite Engine)"
    },
    body: {
      action: currentEvent ? currentEvent.action : "Execute Authorization Check",
      requiredPermission: currentEvent ? currentEvent.permission : "system.access",
      targetResource: currentEvent ? currentEvent.resource : "/api/rbac/verify",
      clientTimestamp: currentEvent ? currentEvent.timestamp : new Date().toISOString(),
      evaluationContext: {
        environment: "production",
        cluster: "us-east-cluster-01",
        riskThreshold: "STRICT_GOVERNANCE",
        activePolicies: ["POL-001-TENANT-ISOLATION", "POL-003-RBAC-ZERO-TRUST"]
      }
    }
  };

  const responsePayload = {
    protocol: "HTTP/2.0",
    statusCode: statusCode,
    statusText: isAuth ? "200 OK (AUTHORIZED)" : "403 Forbidden (RBAC_PERMISSION_DENIED)",
    headers: {
      "Content-Type": "application/json; charset=utf-8",
      "X-RBAC-Engine-Version": "3.4.1-enterprise",
      "X-Decision-Latency-MS": currentEvent ? `${currentEvent.latencyMs}ms` : "14ms",
      "X-Audit-Signature": `sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`,
      "Cache-Control": "no-store, no-cache, must-revalidate",
      "Strict-Transport-Security": "max-age=31536000; includeSubDomains"
    },
    body: {
      decision: {
        allowed: isAuth,
        verdict: isAuth ? "ACCESS_GRANTED" : "ACCESS_DENIED",
        reason: currentEvent ? currentEvent.reason : (isAuth ? "Permission granted" : "Access denied"),
        code: isAuth ? "OK" : "PERMISSION_DENIED_BY_POLICY"
      },
      actor: {
        email: role.email,
        roleId: role.id,
        roleName: role.name,
        tenant: role.tenant
      },
      pipelineChecklist: {
        step1_authentication: { status: "PASS", signature: "RSA-256 Validated" },
        step2_permission_scope: {
          status: isAuth ? "PASS" : "FAIL",
          required: currentEvent ? currentEvent.permission : "system.access",
          assigned: role.allowedPermissions
        },
        step3_tenant_isolation: { status: "PASS", targetTenant: role.tenant, boundary: "ENFORCED" },
        step4_project_boundary: { status: "PASS", project: "Apex AI Agent", state: "AUTHORIZED" },
        step5_governance_guardrail: { status: isAuth ? "PASS" : "FAIL", quarantineStatus: "CLEAN" }
      },
      telemetry: {
        evaluatedAt: currentEvent ? currentEvent.timestamp : new Date().toISOString(),
        engineNode: "rbac-worker-node-primary-04",
        executionTimeMs: currentEvent ? currentEvent.latencyMs : 14
      }
    }
  };

  const jwtTokenPayload = {
    header: {
      alg: "RS256",
      typ: "JWT",
      kid: "aiqa-rbac-key-2026-q3"
    },
    payload: {
      iss: "https://auth.aiqa.internal",
      sub: role.id,
      email: role.email,
      name: role.name,
      tenant_id: role.tenant,
      role: role.id,
      permissions: role.allowedPermissions,
      session_id: `sess-${role.id}-${Date.now().toString(36)}`,
      iat: Math.floor(Date.now() / 1000) - 3600,
      exp: Math.floor(Date.now() / 1000) + 86400
    },
    signature: "SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c..."
  };

  const fullCombinedJson = {
    flowMetadata: {
      pipeIdentifier: `PIPE-${role.id.toUpperCase()}-TO-RBAC-ENGINE`,
      sourceRole: role.name,
      destinationNode: "RBAC Authorization Central Engine",
      evaluatedAt: currentEvent ? currentEvent.timestamp : new Date().toISOString(),
      resultVerdict: isAuth ? "AUTHORIZED" : "FORBIDDEN",
      latencyMs: currentEvent ? currentEvent.latencyMs : 14
    },
    request: requestPayload,
    response: responsePayload,
    jwtSession: jwtTokenPayload
  };

  const handleCopy = () => {
    let textToCopy = "";
    if (activeTab === "all") textToCopy = JSON.stringify(fullCombinedJson, null, 2);
    else if (activeTab === "request") textToCopy = JSON.stringify(requestPayload, null, 2);
    else if (activeTab === "response") textToCopy = JSON.stringify(responsePayload, null, 2);
    else if (activeTab === "jwt_token") textToCopy = JSON.stringify(jwtTokenPayload, null, 2);
    else textToCopy = JSON.stringify({ requestHeaders: requestPayload.headers, responseHeaders: responsePayload.headers }, null, 2);

    navigator.clipboard.writeText(textToCopy);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleDownload = () => {
    const dataStr = "data:text/json;charset=utf-8," + encodeURIComponent(JSON.stringify(fullCombinedJson, null, 2));
    const downloadAnchor = document.createElement("a");
    downloadAnchor.setAttribute("href", dataStr);
    downloadAnchor.setAttribute("download", `rbac_flow_${role.id}_${Date.now()}.json`);
    document.body.appendChild(downloadAnchor);
    downloadAnchor.click();
    downloadAnchor.remove();
  };

  return (
    <div className="fixed inset-0 z-50 bg-black/85 backdrop-blur-md flex items-center justify-center p-3 sm:p-5 animate-in fade-in duration-200">
      <div className="bg-zinc-950 border border-zinc-700/80 rounded-2xl w-full max-w-4xl max-h-[92vh] flex flex-col shadow-2xl overflow-hidden font-mono text-xs">
        {/* Top Title Bar */}
        <div className="flex items-center justify-between px-5 py-3.5 bg-zinc-900 border-b border-zinc-800">
          <div className="flex items-center gap-3">
            <div
              className={`p-2 rounded-xl border ${
                isAuth
                  ? "bg-sky-500/10 text-sky-400 border-sky-500/30 shadow-[0_0_12px_rgba(56,189,248,0.2)]"
                  : "bg-orange-500/10 text-orange-400 border-orange-500/30 shadow-[0_0_12px_rgba(249,115,22,0.2)]"
              }`}
            >
              <Zap className="w-5 h-5" />
            </div>
            <div>
              <div className="flex items-center gap-2 flex-wrap">
                <h2 className="text-sm sm:text-base font-bold text-zinc-100 uppercase tracking-wider">
                  FLOW INSPECTOR OVERLAY
                </h2>
                <span className="px-2 py-0.2 rounded text-[10px] font-bold bg-zinc-800 text-zinc-300 border border-zinc-700">
                  CONNECTION PIPE INSPECTOR
                </span>
                <span
                  className={`px-2 py-0.2 rounded text-[10px] font-bold border ${
                    isAuth
                      ? "bg-sky-500/20 text-sky-300 border-sky-400/40"
                      : "bg-orange-500/20 text-orange-300 border-orange-400/40"
                  }`}
                >
                  {isAuth ? "✓ 200 OK (AUTHORIZED)" : "✕ 403 (FORBIDDEN)"}
                </span>
              </div>
              <p className="text-[11px] text-zinc-400 flex items-center gap-2 mt-0.5">
                <span>Channel:</span>
                <span className="text-sky-400 font-semibold">{role.name}</span>
                <ArrowRight className="w-3 h-3 text-zinc-500" />
                <span className="text-purple-400 font-semibold">RBAC Engine Node</span>
                <span className="text-zinc-600">|</span>
                <span className="text-zinc-400">Latency: {currentEvent ? currentEvent.latencyMs : 14}ms</span>
              </p>
            </div>
          </div>

          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800 transition-colors"
            title="Close Inspector (Esc)"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Role Pipe Selector Ribbon */}
        <div className="px-5 py-2.5 bg-zinc-950 border-b border-zinc-800 flex items-center justify-between gap-2 overflow-x-auto">
          <div className="flex items-center gap-2">
            <span className="text-[11px] text-zinc-500 font-semibold uppercase tracking-wider shrink-0">
              Active Pipe:
            </span>
            <div className="flex items-center gap-1.5">
              {allRoles.map((r) => {
                const isSelected = r.id === roleId;
                return (
                  <button
                    key={r.id}
                    onClick={() => onSelectRole(r.id)}
                    className={`px-2.5 py-1 rounded-lg text-[11px] font-bold transition-all flex items-center gap-1.5 ${
                      isSelected
                        ? "bg-sky-500/20 text-sky-300 border border-sky-400/50 shadow-[0_0_8px_rgba(56,189,248,0.2)]"
                        : "bg-zinc-900 text-zinc-400 hover:text-zinc-200 border border-zinc-800"
                    }`}
                  >
                    <span className={`w-1.5 h-1.5 rounded-full ${isSelected ? "bg-sky-400 animate-pulse" : "bg-zinc-600"}`} />
                    <span>{r.name}</span>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Quick Action: Re-Probe this pipe */}
          <button
            onClick={() => onReProbe(role)}
            className="px-3 py-1 rounded-lg bg-zinc-900 hover:bg-zinc-800 text-sky-400 border border-sky-500/30 hover:border-sky-500/60 transition-all flex items-center gap-1.5 text-[11px] shrink-0 active:scale-95"
          >
            <RefreshCw className="w-3 h-3 text-sky-400 animate-spin-slow" />
            <span>Re-Probe Pipe</span>
          </button>
        </div>

        {/* View Mode Tabs & Action Controls */}
        <div className="flex items-center justify-between px-5 py-2.5 bg-zinc-900/70 border-b border-zinc-800 flex-wrap gap-2">
          {/* Tabs */}
          <div className="flex items-center gap-1">
            <button
              onClick={() => setActiveTab("all")}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-all flex items-center gap-1.5 ${
                activeTab === "all"
                  ? "bg-zinc-800 text-white shadow-sm"
                  : "text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/50"
              }`}
            >
              <FileCode className="w-3.5 h-3.5 text-sky-400" />
              <span>Full Wire Flow</span>
            </button>
            <button
              onClick={() => setActiveTab("request")}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-all flex items-center gap-1.5 ${
                activeTab === "request"
                  ? "bg-zinc-800 text-white shadow-sm"
                  : "text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/50"
              }`}
            >
              <Code2 className="w-3.5 h-3.5 text-amber-400" />
              <span>Request JSON</span>
            </button>
            <button
              onClick={() => setActiveTab("response")}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-all flex items-center gap-1.5 ${
                activeTab === "response"
                  ? "bg-zinc-800 text-white shadow-sm"
                  : "text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/50"
              }`}
            >
              <Server className="w-3.5 h-3.5 text-emerald-400" />
              <span>Response JSON</span>
            </button>
            <button
              onClick={() => setActiveTab("jwt_token")}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-all flex items-center gap-1.5 ${
                activeTab === "jwt_token"
                  ? "bg-zinc-800 text-white shadow-sm"
                  : "text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/50"
              }`}
            >
              <KeyRound className="w-3.5 h-3.5 text-purple-400" />
              <span>JWT Bearer Claims</span>
            </button>
            <button
              onClick={() => setActiveTab("headers")}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-all flex items-center gap-1.5 ${
                activeTab === "headers"
                  ? "bg-zinc-800 text-white shadow-sm"
                  : "text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/50"
              }`}
            >
              <Sliders className="w-3.5 h-3.5 text-indigo-400" />
              <span>HTTP Headers</span>
            </button>
          </div>

          {/* Copy & Download Tools */}
          <div className="flex items-center gap-2">
            <button
              onClick={handleCopy}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-200 hover:text-white border border-zinc-700 transition-all text-xs active:scale-95"
            >
              {copied ? (
                <>
                  <Check className="w-3.5 h-3.5 text-emerald-400" />
                  <span className="text-emerald-400 font-bold">COPIED!</span>
                </>
              ) : (
                <>
                  <Copy className="w-3.5 h-3.5 text-zinc-400" />
                  <span>Copy JSON</span>
                </>
              )}
            </button>

            <button
              onClick={handleDownload}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-200 hover:text-white border border-zinc-700 transition-all text-xs active:scale-95"
              title="Download JSON Payload"
            >
              <Download className="w-3.5 h-3.5 text-zinc-400" />
              <span>Download</span>
            </button>
          </div>
        </div>

        {/* Payload Display Area */}
        <div className="p-4 sm:p-5 overflow-y-auto space-y-4 max-h-[60vh]">
          {/* Quick Flow Visualization Ribbon */}
          <div className="p-3 bg-zinc-900/60 rounded-xl border border-zinc-800 grid grid-cols-1 md:grid-cols-3 gap-3">
            <div className="space-y-1">
              <span className="text-[10px] text-zinc-500 block uppercase">1. REQUEST ORIGIN</span>
              <div className="flex items-center gap-2">
                <span className="font-bold text-zinc-200">{role.email}</span>
                <span className="text-[10px] px-1.5 py-0.2 rounded bg-zinc-800 text-zinc-400">
                  {role.badgeLabel}
                </span>
              </div>
              <p className="text-[10px] text-zinc-500">Method: {requestPayload.method} {requestPayload.path}</p>
            </div>

            <div className="space-y-1">
              <span className="text-[10px] text-zinc-500 block uppercase">2. RBAC ENGINE PIPELINE</span>
              <div className="flex items-center gap-2">
                <Shield className="w-4 h-4 text-purple-400" />
                <span className="text-zinc-200 font-bold">5 Validation Gates</span>
              </div>
              <p className="text-[10px] text-zinc-400">
                Permission Required: <span className="text-purple-300 font-semibold">{currentEvent ? currentEvent.permission : "system.access"}</span>
              </p>
            </div>

            <div className="space-y-1">
              <span className="text-[10px] text-zinc-500 block uppercase">3. EVALUATION OUTCOME</span>
              <div className="flex items-center gap-2">
                {isAuth ? (
                  <CheckCircle2 className="w-4 h-4 text-sky-400" />
                ) : (
                  <XCircle className="w-4 h-4 text-orange-400" />
                )}
                <span className={`font-bold ${isAuth ? "text-sky-300" : "text-orange-300"}`}>
                  {isAuth ? "200 AUTHORIZED" : "403 ACCESS DENIED"}
                </span>
              </div>
              <p className="text-[10px] text-zinc-500 truncate">{currentEvent ? currentEvent.reason : "Decision completed"}</p>
            </div>
          </div>

          {/* Code Viewer */}
          <div className="relative">
            <div className="absolute top-2.5 right-3 text-[10px] text-zinc-500 font-mono">
              JSON • UTF-8
            </div>
            <pre className="p-4 bg-zinc-950 rounded-xl border border-zinc-800 text-[11px] leading-relaxed text-zinc-300 overflow-x-auto shadow-inner">
              {activeTab === "all" && JSON.stringify(fullCombinedJson, null, 2)}
              {activeTab === "request" && JSON.stringify(requestPayload, null, 2)}
              {activeTab === "response" && JSON.stringify(responsePayload, null, 2)}
              {activeTab === "jwt_token" && JSON.stringify(jwtTokenPayload, null, 2)}
              {activeTab === "headers" &&
                JSON.stringify(
                  {
                    requestHeaders: requestPayload.headers,
                    responseHeaders: responsePayload.headers
                  },
                  null,
                  2
                )}
            </pre>
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between px-5 py-3 bg-zinc-900 border-t border-zinc-800 text-[11px] text-zinc-400">
          <div className="flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
            <span>Connection Pipe Status: Live & Monitored</span>
          </div>

          <div className="flex items-center gap-3">
            <span>Tip: Click any connection line in the 2×2 grid to open this Flow Inspector</span>
            <button
              onClick={onClose}
              className="px-4 py-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-200 font-semibold border border-zinc-700 transition-colors"
            >
              Close
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
