import React, { useState, useEffect, useRef } from "react";
import {
  RoleConfig,
  RoleId,
  RoleActionItem,
  LiveRbacEvent,
  EventResult
} from "./types";
import { RolePanel } from "./RolePanel";
import { RBACEngineNode } from "./RBACEngineNode";
import { ConnectionPipes } from "./ConnectionPipes";
import { LiveEventStream } from "./LiveEventStream";
import { RoleZoomView } from "./RoleZoomView";
import { PermissionMatrixModal } from "./PermissionMatrixModal";
import { EventDetailModal } from "./EventDetailModal";
import { FlowInspectorOverlay } from "./FlowInspectorOverlay";
import {
  Zap,
  Grid,
  Maximize2,
  Table,
  Play,
  RotateCcw,
  Sparkles,
  ShieldCheck,
  Radio,
  Sliders,
  Shield,
  Layers,
  Terminal
} from "lucide-react";
import { probeRbacAuthorization } from "../../services/api";

// 4 Initial Roles configured strictly to requirements
const initialRoles: RoleConfig[] = [
  {
    id: "super_admin",
    name: "Super Admin",
    badgeLabel: "SUPER ADMIN",
    email: "superadmin@aiqa.local",
    sessionToken: "aiqa_jwt_super_admin_sec991048_root_access",
    tenant: "Acme AI Corp (Cluster Root)",
    status: "active",
    avatarInitials: "SA",
    color: {
      primary: "purple-400",
      border: "border-purple-500/40",
      bg: "bg-purple-950/20",
      glow: "shadow-[0_0_20px_rgba(168,85,247,0.3)]",
      text: "text-purple-400",
      chip: "bg-purple-500/10 text-purple-300 border-purple-500/30"
    },
    allowedPermissions: ["*"],
    deniedPermissions: [],
    permissionSummary: {
      users: true,
      projects: true,
      tests: true,
      evidence: true,
      bugs: true,
      reports: true,
      policies: true,
      auditLogs: true,
      aiModels: true,
      system: true
    },
    actions: [
      {
        id: "create_user",
        label: "Create User",
        permission: "users.create",
        resource: "/api/rbac/users",
        method: "POST",
        endpoint: "/api/rbac/users",
        isAllowed: true,
        requiresLock: false,
        category: "User Management",
        description: "Provision new team user account"
      },
      {
        id: "manage_roles",
        label: "Manage Role Policy",
        permission: "roles.update",
        resource: "/api/rbac/roles",
        method: "PUT",
        endpoint: "/api/rbac/roles",
        isAllowed: true,
        requiresLock: false,
        category: "Access Governance",
        description: "Reconfigure global RBAC scopes"
      },
      {
        id: "exec_critical_test",
        label: "Execute Critical Test",
        permission: "tests.execute",
        resource: "/api/tests/run",
        method: "POST",
        endpoint: "/api/tests/run",
        isAllowed: true,
        requiresLock: false,
        category: "Test Operations",
        description: "Run automated security fuzzing suite"
      },
      {
        id: "view_tamper_audit",
        label: "View Tamper Audit",
        permission: "audit_logs.view",
        resource: "/api/audit-logs",
        method: "GET",
        endpoint: "/api/audit-logs",
        isAllowed: true,
        requiresLock: false,
        category: "Audit & Compliance",
        description: "Inspect immutable audit log ledger"
      },
      {
        id: "manage_policy_guardrail",
        label: "Manage Policy Guardrail",
        permission: "policies.update",
        resource: "/api/policies",
        method: "POST",
        endpoint: "/api/policies",
        isAllowed: true,
        requiresLock: false,
        category: "Governance & Policies",
        description: "Update production quarantine rules"
      },
      {
        id: "chaos_injection_probe",
        label: "Chaos Injection Probe",
        permission: "system.manage",
        resource: "/api/chaos-simulate",
        method: "POST",
        endpoint: "/api/chaos-simulate",
        isAllowed: true,
        requiresLock: false,
        category: "System Administration",
        description: "Simulate worker failover scenario"
      }
    ]
  },
  {
    id: "org_admin",
    name: "Admin",
    badgeLabel: "ORGANIZATION ADMIN",
    email: "admin@aiqa.local",
    sessionToken: "aiqa_jwt_org_admin_tok881294_tenant_admin",
    tenant: "Acme AI Corp (Tenant Level)",
    status: "active",
    avatarInitials: "AD",
    color: {
      primary: "sky-400",
      border: "border-sky-500/40",
      bg: "bg-sky-950/20",
      glow: "shadow-[0_0_20px_rgba(56,189,248,0.3)]",
      text: "text-sky-400",
      chip: "bg-sky-500/10 text-sky-300 border-sky-500/30"
    },
    allowedPermissions: [
      "users.view", "users.create", "users.update", "users.delete",
      "roles.view", "roles.create", "roles.update",
      "projects.view", "projects.create", "projects.update", "projects.delete",
      "tests.view", "tests.create", "tests.update", "tests.delete", "tests.execute",
      "test_runs.view", "test_runs.create", "test_runs.cancel",
      "evidence.view", "evidence.create", "evidence.delete",
      "bugs.view", "bugs.create", "bugs.update", "bugs.delete",
      "policies.view", "policies.create", "policies.update",
      "audit_logs.view", "reports.view", "reports.create", "reports.export",
      "ai_models.view", "system.view"
    ],
    deniedPermissions: ["system.manage"],
    permissionSummary: {
      users: true,
      projects: true,
      tests: true,
      evidence: true,
      bugs: true,
      reports: true,
      policies: true,
      auditLogs: true,
      aiModels: true,
      system: false
    },
    actions: [
      {
        id: "create_user",
        label: "Create User",
        permission: "users.create",
        resource: "/api/rbac/users",
        method: "POST",
        endpoint: "/api/rbac/users",
        isAllowed: true,
        requiresLock: false,
        category: "User Management",
        description: "Provision new tenant user account"
      },
      {
        id: "manage_project",
        label: "Manage Project",
        permission: "projects.create",
        resource: "/api/projects",
        method: "POST",
        endpoint: "/api/projects",
        isAllowed: true,
        requiresLock: false,
        category: "Projects",
        description: "Configure staging test repository"
      },
      {
        id: "execute_suite",
        label: "Execute Test Suite",
        permission: "tests.execute",
        resource: "/api/tests/run",
        method: "POST",
        endpoint: "/api/tests/run",
        isAllowed: true,
        requiresLock: false,
        category: "Test Operations",
        description: "Execute regression suite runner"
      },
      {
        id: "export_report",
        label: "Export Compliance Report",
        permission: "reports.export",
        resource: "/api/reports/export",
        method: "GET",
        endpoint: "/api/reports/export",
        isAllowed: true,
        requiresLock: false,
        category: "Reporting",
        description: "Generate SOC2 compliance audit packet"
      },
      {
        id: "update_policy",
        label: "Update Policy",
        permission: "policies.update",
        resource: "/api/policies",
        method: "PUT",
        endpoint: "/api/policies",
        isAllowed: true,
        requiresLock: false,
        category: "Governance & Policies",
        description: "Update coverage gate policy"
      },
      {
        id: "chaos_root_test",
        label: "Host Chaos Reboot 🔒",
        permission: "system.manage",
        resource: "/api/chaos-simulate",
        method: "POST",
        endpoint: "/api/chaos-simulate",
        isAllowed: false,
        requiresLock: true,
        category: "System Administration",
        description: "Requires Super Admin cluster privilege"
      }
    ]
  },
  {
    id: "project_admin",
    name: "Project Manager",
    badgeLabel: "PROJECT MANAGER",
    email: "pm@aiqa.local",
    sessionToken: "aiqa_jwt_proj_mgr_tok774129_scoped_projects",
    tenant: "Acme AI Corp",
    status: "active",
    avatarInitials: "PM",
    color: {
      primary: "indigo-400",
      border: "border-indigo-500/40",
      bg: "bg-indigo-950/20",
      glow: "shadow-[0_0_20px_rgba(99,102,241,0.3)]",
      text: "text-indigo-400",
      chip: "bg-indigo-500/10 text-indigo-300 border-indigo-500/30"
    },
    allowedPermissions: [
      "projects.view", "projects.update",
      "tests.view", "tests.create", "tests.update", "tests.delete", "tests.execute",
      "test_runs.view", "test_runs.create", "test_runs.cancel",
      "evidence.view", "evidence.create", "evidence.delete",
      "bugs.view", "bugs.create", "bugs.update", "bugs.delete",
      "quality.view", "risk.view", "flakiness.view",
      "policies.view",
      "reports.view", "reports.create", "reports.export",
      "ai_models.view",
      "system.view"
    ],
    deniedPermissions: [
      "users.view", "users.create", "users.update", "users.delete",
      "roles.view", "roles.create", "roles.update", "roles.delete",
      "policies.create", "policies.update", "policies.delete",
      "audit_logs.view",
      "system.manage"
    ],
    permissionSummary: {
      users: false,
      projects: true,
      tests: true,
      evidence: true,
      bugs: true,
      reports: true,
      policies: false,
      auditLogs: false,
      aiModels: true,
      system: false
    },
    actions: [
      {
        id: "create_test_suite",
        label: "Create Test Suite",
        permission: "tests.create",
        resource: "/api/tests",
        method: "POST",
        endpoint: "/api/tests",
        isAllowed: true,
        requiresLock: false,
        category: "Test Operations",
        description: "Author project test suite"
      },
      {
        id: "execute_test_run",
        label: "Execute Test Run",
        permission: "tests.execute",
        resource: "/api/tests/run",
        method: "POST",
        endpoint: "/api/tests/run",
        isAllowed: true,
        requiresLock: false,
        category: "Test Operations",
        description: "Start scheduled runner execution"
      },
      {
        id: "approve_bug_candidate",
        label: "Approve Bug Candidate",
        permission: "bugs.create",
        resource: "/api/bugs",
        method: "POST",
        endpoint: "/api/bugs",
        isAllowed: true,
        requiresLock: false,
        category: "Defect Management",
        description: "Publish defect candidate to Jira"
      },
      {
        id: "inspect_evidence",
        label: "View Diagnostics Trace",
        permission: "evidence.view",
        resource: "/api/evidence",
        method: "GET",
        endpoint: "/api/evidence",
        isAllowed: true,
        requiresLock: false,
        category: "Evidence & Diagnostics",
        description: "Inspect network HAR traces"
      },
      {
        id: "delete_user_forbidden",
        label: "Delete User Account 🔒",
        permission: "users.delete",
        resource: "/api/rbac/users/usr-102",
        method: "DELETE",
        endpoint: "/api/rbac/users/usr-102",
        isAllowed: false,
        requiresLock: true,
        category: "User Management",
        description: "Requires Admin or Super Admin privilege"
      },
      {
        id: "override_policy_forbidden",
        label: "Override Global Policy 🔒",
        permission: "policies.update",
        resource: "/api/policies",
        method: "PUT",
        endpoint: "/api/policies",
        isAllowed: false,
        requiresLock: true,
        category: "Governance & Policies",
        description: "Requires Organization Admin privilege"
      }
    ]
  },
  {
    id: "qa_engineer",
    name: "User (QA User)",
    badgeLabel: "QA USER",
    email: "qa.user@aiqa.local",
    sessionToken: "aiqa_jwt_qa_user_tok665123_test_author",
    tenant: "Acme AI Corp",
    status: "active",
    avatarInitials: "QA",
    color: {
      primary: "emerald-400",
      border: "border-emerald-500/40",
      bg: "bg-emerald-950/20",
      glow: "shadow-[0_0_20px_rgba(52,211,153,0.3)]",
      text: "text-emerald-400",
      chip: "bg-emerald-500/10 text-emerald-300 border-emerald-500/30"
    },
    allowedPermissions: [
      "projects.view",
      "tests.view", "tests.create", "tests.update", "tests.execute",
      "test_runs.view", "test_runs.create",
      "evidence.view", "evidence.create",
      "bugs.view", "bugs.create", "bugs.update",
      "quality.view", "risk.view", "flakiness.view",
      "policies.view",
      "reports.view",
      "ai_models.view",
      "system.view"
    ],
    deniedPermissions: [
      "users.view", "users.create", "users.update", "users.delete",
      "roles.view", "roles.create", "roles.update", "roles.delete",
      "projects.create", "projects.update", "projects.delete",
      "tests.delete", "test_runs.cancel", "evidence.delete", "bugs.delete",
      "policies.create", "policies.update", "policies.delete",
      "audit_logs.view",
      "system.manage"
    ],
    permissionSummary: {
      users: false,
      projects: false,
      tests: true,
      evidence: true,
      bugs: true,
      reports: false,
      policies: false,
      auditLogs: false,
      aiModels: false,
      system: false
    },
    actions: [
      {
        id: "view_test_specs",
        label: "View Test Specs",
        permission: "tests.view",
        resource: "/api/tests",
        method: "GET",
        endpoint: "/api/tests",
        isAllowed: true,
        requiresLock: false,
        category: "Test Operations",
        description: "Read active test definitions"
      },
      {
        id: "run_test_suite",
        label: "Run Allowed Test Suite",
        permission: "tests.execute",
        resource: "/api/tests/run",
        method: "POST",
        endpoint: "/api/tests/run",
        isAllowed: true,
        requiresLock: false,
        category: "Test Operations",
        description: "Execute functional test runner"
      },
      {
        id: "draft_bug",
        label: "Draft AI Bug Flaw",
        permission: "bugs.create",
        resource: "/api/ai/draft-bug",
        method: "POST",
        endpoint: "/api/ai/draft-bug",
        isAllowed: true,
        requiresLock: false,
        category: "Defect Management",
        description: "Create AI bug candidate"
      },
      {
        id: "view_traces",
        label: "View Execution Traces",
        permission: "evidence.view",
        resource: "/api/evidence",
        method: "GET",
        endpoint: "/api/evidence",
        isAllowed: true,
        requiresLock: false,
        category: "Evidence & Diagnostics",
        description: "Inspect network HAR & assertions"
      },
      {
        id: "delete_project_forbidden",
        label: "Delete Project 🔒",
        permission: "projects.delete",
        resource: "/api/projects/proj-901",
        method: "DELETE",
        endpoint: "/api/projects/proj-901",
        isAllowed: false,
        requiresLock: true,
        category: "Projects",
        description: "Requires Organization Admin privilege"
      },
      {
        id: "delete_user_forbidden",
        label: "Delete User 🔒",
        permission: "users.delete",
        resource: "/api/rbac/users/usr-202",
        method: "DELETE",
        endpoint: "/api/rbac/users/usr-202",
        isAllowed: false,
        requiresLock: true,
        category: "User Management",
        description: "Requires User Admin privilege"
      }
    ]
  }
];

// Seeded sample initial events for instant rich rendering
const seedEvents: LiveRbacEvent[] = [
  {
    id: "evt-001",
    timestamp: new Date(Date.now() - 32000).toISOString(),
    timeFormatted: new Date(Date.now() - 32000).toLocaleTimeString([], { hour12: false }),
    roleId: "super_admin",
    roleName: "Super Admin",
    actorEmail: "superadmin@aiqa.local",
    action: "Manage Role Policy",
    permission: "roles.update",
    resource: "/api/rbac/roles",
    result: "AUTHORIZED",
    statusCode: 200,
    latencyMs: 14,
    reason: "Matched system wildcard authorization role (*)",
    tenantId: "tenant_acme",
    checks: {
      authentication: true,
      permissionCheck: true,
      tenantCheck: true,
      projectAccess: true,
      policyCheck: true
    }
  },
  {
    id: "evt-002",
    timestamp: new Date(Date.now() - 25000).toISOString(),
    timeFormatted: new Date(Date.now() - 25000).toLocaleTimeString([], { hour12: false }),
    roleId: "org_admin",
    roleName: "Organization Admin",
    actorEmail: "admin@aiqa.local",
    action: "Create User",
    permission: "users.create",
    resource: "/api/rbac/users",
    result: "AUTHORIZED",
    statusCode: 200,
    latencyMs: 18,
    reason: "Granted via Organization Admin scope (users.create)",
    tenantId: "tenant_acme",
    checks: {
      authentication: true,
      permissionCheck: true,
      tenantCheck: true,
      projectAccess: true,
      policyCheck: true
    }
  },
  {
    id: "evt-003",
    timestamp: new Date(Date.now() - 18000).toISOString(),
    timeFormatted: new Date(Date.now() - 18000).toLocaleTimeString([], { hour12: false }),
    roleId: "project_admin",
    roleName: "Project Manager",
    actorEmail: "pm@aiqa.local",
    action: "Delete User Account",
    permission: "users.delete",
    resource: "/api/rbac/users/usr-102",
    result: "FORBIDDEN",
    statusCode: 403,
    latencyMs: 8,
    reason: "Role 'Project Admin' does not possess required permission 'users.delete'",
    tenantId: "tenant_acme",
    checks: {
      authentication: true,
      permissionCheck: false,
      tenantCheck: true,
      projectAccess: true,
      policyCheck: false
    }
  },
  {
    id: "evt-004",
    timestamp: new Date(Date.now() - 9000).toISOString(),
    timeFormatted: new Date(Date.now() - 9000).toLocaleTimeString([], { hour12: false }),
    roleId: "qa_engineer",
    roleName: "QA User",
    actorEmail: "qa.user@aiqa.local",
    action: "Run Allowed Test Suite",
    permission: "tests.execute",
    resource: "/api/tests/run",
    result: "AUTHORIZED",
    statusCode: 200,
    latencyMs: 22,
    reason: "Granted via QA Engineer scope (tests.execute)",
    tenantId: "tenant_acme",
    checks: {
      authentication: true,
      permissionCheck: true,
      tenantCheck: true,
      projectAccess: true,
      policyCheck: true
    }
  }
];

export const RbacLiveDashboard: React.FC = () => {
  const [roles, setRoles] = useState<RoleConfig[]>(initialRoles);
  const [events, setEvents] = useState<LiveRbacEvent[]>(seedEvents);
  const [zoomedRoleId, setZoomedRoleId] = useState<RoleId | null>(null);

  // Active Flow Animation State
  const [activeRoleId, setActiveRoleId] = useState<RoleId | null>(null);
  const [activeActionId, setActiveActionId] = useState<string | null>(null);
  const [activeResult, setActiveResult] = useState<EventResult | null>(null);
  const [isEvaluating, setIsEvaluating] = useState<boolean>(false);
  const [activeEngineEvent, setActiveEngineEvent] = useState<LiveRbacEvent | null>(seedEvents[0]);

  // Modals
  const [isMatrixOpen, setIsMatrixOpen] = useState<boolean>(false);
  const [inspectedEvent, setInspectedEvent] = useState<LiveRbacEvent | null>(null);
  const [selectedPipeRoleId, setSelectedPipeRoleId] = useState<RoleId | null>(null);

  // Auto-simulation
  const [isAutoSimulating, setIsAutoSimulating] = useState<boolean>(false);
  const autoSimTimerRef = useRef<NodeJS.Timeout | null>(null);

  // Execute Action Probe
  const handleExecuteAction = async (role: RoleConfig, action: RoleActionItem) => {
    const actionKey = `${role.id}:${action.id}`;
    setActiveActionId(actionKey);
    setActiveRoleId(role.id);
    setIsEvaluating(true);

    const startTime = performance.now();

    // Map roleId to backend test probe
    let probeUserId = "usr-super-admin";
    if (role.id === "org_admin") probeUserId = "usr-org-admin";
    if (role.id === "project_admin") probeUserId = "usr-proj-admin";
    if (role.id === "qa_engineer") probeUserId = "usr-qa-eng";

    let backendResult: any = null;

    try {
      // Probe backend endpoint
      const response = await probeRbacAuthorization({
        userId: probeUserId,
        method: action.method || "POST",
        path: action.endpoint || action.resource,
        requiredPermission: action.permission,
        targetProjectId: "Apex AI Agent"
      });
      if (response && response.decision) {
        backendResult = response;
      }
    } catch (err) {
      // Client-side fallback if server fails
      console.warn("Probe API error, using deterministic client fallback:", err);
    }

    const isAuthorized = backendResult ? backendResult.decision.allowed : action.isAllowed;
    const latencyMs = Math.max(8, Math.round(performance.now() - startTime));

    const newEvent: LiveRbacEvent = {
      id: `evt-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
      timestamp: new Date().toISOString(),
      timeFormatted: new Date().toLocaleTimeString([], { hour12: false }),
      roleId: role.id,
      roleName: role.name,
      actorEmail: role.email,
      action: action.label.replace(" 🔒", ""),
      permission: action.permission,
      resource: action.resource,
      result: isAuthorized ? "AUTHORIZED" : "FORBIDDEN",
      statusCode: isAuthorized ? 200 : 403,
      latencyMs: latencyMs,
      reason: isAuthorized
        ? `Granted: Role '${role.name}' possesses required permission '${action.permission}'`
        : `Denied: Role '${role.name}' does not possess required permission '${action.permission}'`,
      tenantId: role.tenant,
      checks: {
        authentication: true,
        permissionCheck: isAuthorized,
        tenantCheck: true,
        projectAccess: true,
        policyCheck: isAuthorized
      }
    };

    setActiveResult(newEvent.result);
    setActiveEngineEvent(newEvent);
    setEvents((prev) => [newEvent, ...prev.slice(0, 49)]);

    // Hold the active glowing state for 1.8s then smoothly reset
    setTimeout(() => {
      setIsEvaluating(false);
      setActiveActionId(null);
      setTimeout(() => {
        setActiveRoleId(null);
        setActiveResult(null);
      }, 600);
    }, 1800);
  };

  // Multi-Role Concurrency Test
  const handleRunConcurrencyTest = async () => {
    // Fire actions concurrently across all 4 roles
    const actionsToRun: { role: RoleConfig; action: RoleActionItem }[] = [
      { role: roles[0], action: roles[0].actions[2] }, // Super Admin -> Execute Critical Test (200)
      { role: roles[1], action: roles[1].actions[0] }, // Admin -> Create User (200)
      { role: roles[2], action: roles[2].actions[4] }, // PM -> Delete User 🔒 (403)
      { role: roles[3], action: roles[3].actions[1] }  // QA User -> Run Test (200)
    ];

    actionsToRun.forEach((item, index) => {
      setTimeout(() => {
        handleExecuteAction(item.role, item.action);
      }, index * 250);
    });
  };

  // Probe permission from Matrix modal
  const handleProbeFromMatrix = (permissionId: string, roleId: RoleId) => {
    const targetRole = roles.find((r) => r.id === roleId);
    if (!targetRole) return;

    const isAllowed = targetRole.allowedPermissions.includes("*") || targetRole.allowedPermissions.includes(permissionId);
    const mockAction: RoleActionItem = {
      id: `matrix-probe-${permissionId}`,
      label: `Matrix Probe (${permissionId})`,
      permission: permissionId,
      resource: `/api/${permissionId.split(".")[0]}`,
      method: "POST",
      endpoint: `/api/${permissionId.split(".")[0]}`,
      isAllowed,
      requiresLock: !isAllowed,
      category: "Permission Matrix Probe",
      description: "Triggered from permission comparison matrix"
    };

    handleExecuteAction(targetRole, mockAction);
  };

  // Auto-simulation loop
  useEffect(() => {
    if (isAutoSimulating) {
      autoSimTimerRef.current = setInterval(() => {
        const randomRole = roles[Math.floor(Math.random() * roles.length)];
        const randomAction = randomRole.actions[Math.floor(Math.random() * randomRole.actions.length)];
        handleExecuteAction(randomRole, randomAction);
      }, 3500);
    } else {
      if (autoSimTimerRef.current) clearInterval(autoSimTimerRef.current);
    }

    return () => {
      if (autoSimTimerRef.current) clearInterval(autoSimTimerRef.current);
    };
  }, [isAutoSimulating, roles]);

  const zoomedRole = roles.find((r) => r.id === zoomedRoleId);

  return (
    <div className="relative min-h-screen bg-zinc-950 text-zinc-100 p-4 sm:p-6 flex flex-col justify-between space-y-6">
      {/* Top Header / Control Toolbar */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 pb-4 border-b border-zinc-800">
        <div>
          <div className="flex items-center gap-2.5">
            <div className="p-2 rounded-xl bg-sky-500/10 text-sky-400 border border-sky-500/30">
              <ShieldCheck className="w-6 h-6" />
            </div>
            <div>
              <div className="flex items-center gap-2 flex-wrap">
                <h1 className="text-xl sm:text-2xl font-bold font-mono text-zinc-100 uppercase tracking-wider">
                  AIQA RBAC Live Testing
                </h1>
                <span className="px-2.5 py-0.5 rounded-full text-xs font-mono font-bold bg-sky-500/10 text-sky-400 border border-sky-500/30 flex items-center gap-1.5">
                  <Radio className="w-3 h-3 text-sky-400 animate-pulse" />
                  INTERACTIVE MULTI-ROLE CONTROL CENTER
                </span>
              </div>
              <p className="text-xs text-zinc-400 font-mono mt-0.5">
                Real-time authorization simulation & multi-role permission pipeline inspection
              </p>
            </div>
          </div>
        </div>

        {/* Global Action Toolbar */}
        <div className="flex items-center gap-2.5 flex-wrap font-mono text-xs">
          {/* Flow Inspector Trigger */}
          <button
            onClick={() => setSelectedPipeRoleId(activeRoleId || "super_admin")}
            className="flex items-center gap-1.5 px-3 py-2 rounded-lg bg-zinc-900 hover:bg-zinc-800 text-sky-300 hover:text-white border border-sky-500/40 hover:border-sky-400 transition-all shadow-[0_0_10px_rgba(56,189,248,0.15)] active:scale-95"
            title="Inspect Raw JSON Wire Payloads for Connection Pipes"
          >
            <Zap className="w-4 h-4 text-sky-400 animate-pulse" />
            <span>FLOW INSPECTOR</span>
          </button>

          {/* Permission Matrix Button */}
          <button
            onClick={() => setIsMatrixOpen(true)}
            className="flex items-center gap-1.5 px-3 py-2 rounded-lg bg-zinc-900 hover:bg-zinc-800 text-zinc-300 hover:text-white border border-zinc-700 transition-all shadow-sm"
          >
            <Table className="w-4 h-4 text-purple-400" />
            <span>PERMISSION MATRIX</span>
          </button>

          {/* Concurrency Probe Button */}
          <button
            onClick={handleRunConcurrencyTest}
            className="flex items-center gap-1.5 px-3.5 py-2 rounded-lg bg-sky-500 hover:bg-sky-400 text-zinc-950 font-bold transition-all shadow-[0_0_15px_rgba(56,189,248,0.3)] active:scale-95"
          >
            <Zap className="w-4 h-4 fill-zinc-950" />
            <span>CONCURRENCY TEST</span>
          </button>

          {/* Auto Simulation Toggle */}
          <button
            onClick={() => setIsAutoSimulating(!isAutoSimulating)}
            className={`flex items-center gap-1.5 px-3 py-2 rounded-lg border transition-all ${
              isAutoSimulating
                ? "bg-amber-500/20 text-amber-300 border-amber-500/40 shadow-[0_0_15px_rgba(245,158,11,0.3)]"
                : "bg-zinc-900 text-zinc-400 hover:text-zinc-200 border-zinc-700"
            }`}
          >
            <Play className={`w-3.5 h-3.5 ${isAutoSimulating ? "animate-spin text-amber-400" : ""}`} />
            <span>AUTO SIM: {isAutoSimulating ? "ON" : "OFF"}</span>
          </button>
        </div>
      </div>

      {/* Main Canvas Area */}
      <div className="relative w-full flex-1 flex flex-col justify-center items-center my-2">
        {zoomedRole ? (
          /* Zoomed Role View */
          <RoleZoomView
            zoomedRole={zoomedRole}
            allRoles={roles}
            onClose={() => setZoomedRoleId(null)}
            onSelectRole={(rId) => setZoomedRoleId(rId)}
            onExecuteAction={handleExecuteAction}
            activeActionId={activeActionId}
            events={events}
            onInspectEvent={(evt) => setInspectedEvent(evt)}
          />
        ) : (
          /* 2x2 Layout with Central RBAC Engine Node and SVG Connection Pipes */
          <div className="relative w-full">
            {/* SVG Connection Pipes Overlay */}
            <ConnectionPipes
              activeRoleId={activeRoleId}
              activeResult={activeResult}
              isEvaluating={isEvaluating}
              onSelectPipe={(pipeRoleId) => setSelectedPipeRoleId(pipeRoleId)}
            />

            {/* 2x2 Grid container */}
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 relative z-0">
              {/* Role 1: Top-Left (Super Admin) */}
              <div className="relative">
                <RolePanel
                  role={roles[0]}
                  onZoom={(rId) => setZoomedRoleId(rId)}
                  onExecuteAction={handleExecuteAction}
                  activeActionId={activeActionId}
                  activeEvent={events.find((e) => e.roleId === "super_admin")}
                  isEvaluating={isEvaluating && activeRoleId === "super_admin"}
                  evaluationResult={activeRoleId === "super_admin" ? activeResult : null}
                />
              </div>

              {/* Role 2: Top-Right (Admin) */}
              <div className="relative">
                <RolePanel
                  role={roles[1]}
                  onZoom={(rId) => setZoomedRoleId(rId)}
                  onExecuteAction={handleExecuteAction}
                  activeActionId={activeActionId}
                  activeEvent={events.find((e) => e.roleId === "org_admin")}
                  isEvaluating={isEvaluating && activeRoleId === "org_admin"}
                  evaluationResult={activeRoleId === "org_admin" ? activeResult : null}
                />
              </div>

              {/* Role 3: Bottom-Left (Project Manager) */}
              <div className="relative">
                <RolePanel
                  role={roles[2]}
                  onZoom={(rId) => setZoomedRoleId(rId)}
                  onExecuteAction={handleExecuteAction}
                  activeActionId={activeActionId}
                  activeEvent={events.find((e) => e.roleId === "project_admin")}
                  isEvaluating={isEvaluating && activeRoleId === "project_admin"}
                  evaluationResult={activeRoleId === "project_admin" ? activeResult : null}
                />
              </div>

              {/* Role 4: Bottom-Right (User / QA User) */}
              <div className="relative">
                <RolePanel
                  role={roles[3]}
                  onZoom={(rId) => setZoomedRoleId(rId)}
                  onExecuteAction={handleExecuteAction}
                  activeActionId={activeActionId}
                  activeEvent={events.find((e) => e.roleId === "qa_engineer")}
                  isEvaluating={isEvaluating && activeRoleId === "qa_engineer"}
                  evaluationResult={activeRoleId === "qa_engineer" ? activeResult : null}
                />
              </div>
            </div>

            {/* Centered RBAC Authorization Engine Node */}
            <div className="hidden lg:flex absolute inset-0 items-center justify-center pointer-events-none z-20">
              <div className="pointer-events-auto">
                <RBACEngineNode
                  activeEvent={activeEngineEvent}
                  isEvaluating={isEvaluating}
                  totalEvaluated={events.length}
                />
              </div>
            </div>
          </div>
        )}
      </div>

      {/* Real-time RBAC Event Stream Console */}
      <LiveEventStream
        events={events}
        onSelectEvent={(evt) => {
          setZoomedRoleId(evt.roleId);
          setActiveEngineEvent(evt);
        }}
        onClearEvents={() => setEvents([])}
        onInspectEvent={(evt) => setInspectedEvent(evt)}
      />

      {/* Permission Matrix Modal */}
      <PermissionMatrixModal
        isOpen={isMatrixOpen}
        onClose={() => setIsMatrixOpen(false)}
        roles={roles}
        onProbePermission={handleProbeFromMatrix}
      />

      {/* Event Detail Audit Inspector Modal */}
      <EventDetailModal
        event={inspectedEvent}
        onClose={() => setInspectedEvent(null)}
      />

      {/* Flow Inspector Overlay for Connection Pipes (Raw JSON Request/Response) */}
      {selectedPipeRoleId && (
        <FlowInspectorOverlay
          roleId={selectedPipeRoleId}
          role={roles.find((r) => r.id === selectedPipeRoleId)}
          allRoles={roles}
          latestEvent={activeEngineEvent}
          allEvents={events}
          onClose={() => setSelectedPipeRoleId(null)}
          onSelectRole={(rId) => setSelectedPipeRoleId(rId)}
          onReProbe={(targetRole) => {
            const probeAction = targetRole.actions[0];
            handleExecuteAction(targetRole, probeAction);
          }}
        />
      )}
    </div>
  );
};
