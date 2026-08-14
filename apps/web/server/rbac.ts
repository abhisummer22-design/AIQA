import { Request, Response, NextFunction } from "express";

export interface RbacPermissionDef {
  id: string;
  resource: string;
  action: string;
  group: string;
  description: string;
}

export interface RbacRoleDef {
  id: string;
  name: string;
  description: string;
  type: "system" | "custom";
  permissions: string[];
  isProtected?: boolean;
}

export interface RbacUserDef {
  id: string;
  email: string;
  name: string;
  tenantId: string;
  tenantName: string;
  roleId: string;
  roleName: string;
  projectIds: string[];
  status: "active" | "disabled";
  lastActive: string;
  createdAt: string;
}

export interface RbacTenantDef {
  id: string;
  name: string;
  slug: string;
  tier: "Enterprise" | "Business" | "Starter";
  status: "active" | "suspended";
}

export interface RbacAuditEventDef {
  id: string;
  timestamp: string;
  actorId: string;
  actorEmail: string;
  tenantId: string;
  action: string;
  resourceType: string;
  resourceId: string;
  oldValue?: string;
  newValue?: string;
  ip: string;
  result: "SUCCESS" | "FAILED" | "BLOCKED" | "DENIED";
  details?: string;
}

// 1. All Granular Permissions (resource.action)
export const RBAC_PERMISSIONS: RbacPermissionDef[] = [
  // Users
  { id: "users.view", resource: "users", action: "view", group: "User Management", description: "View workspace users, profiles, and assignment states" },
  { id: "users.create", resource: "users", action: "create", group: "User Management", description: "Invite and provision new user accounts" },
  { id: "users.update", resource: "users", action: "update", group: "User Management", description: "Edit user profile, roles, projects, and active status" },
  { id: "users.delete", resource: "users", action: "delete", group: "User Management", description: "Deprovision or remove user accounts" },

  // Roles & Permissions
  { id: "roles.view", resource: "roles", action: "view", group: "Access Governance", description: "Inspect defined roles and permission matrices" },
  { id: "roles.create", resource: "roles", action: "create", group: "Access Governance", description: "Create custom roles with tailored permission sets" },
  { id: "roles.update", resource: "roles", action: "update", group: "Access Governance", description: "Modify role permissions and access scopes" },
  { id: "roles.delete", resource: "roles", action: "delete", group: "Access Governance", description: "Delete custom unassigned roles" },

  // Projects
  { id: "projects.view", resource: "projects", action: "view", group: "Project Management", description: "View accessible projects and test suites" },
  { id: "projects.create", resource: "projects", action: "create", group: "Project Management", description: "Create new test projects and repositories" },
  { id: "projects.update", resource: "projects", action: "update", group: "Project Management", description: "Modify project configurations and environment targets" },
  { id: "projects.delete", resource: "projects", action: "delete", group: "Project Management", description: "Archive or delete projects" },

  // Tests & Test Cases
  { id: "tests.view", resource: "tests", action: "view", group: "Test Authoring & Execution", description: "Read test cases, schemas, and specifications" },
  { id: "tests.create", resource: "tests", action: "create", group: "Test Authoring & Execution", description: "Author new manual or AI-generated test cases" },
  { id: "tests.update", resource: "tests", action: "update", group: "Test Authoring & Execution", description: "Edit test steps, assertions, and quarantine status" },
  { id: "tests.delete", resource: "tests", action: "delete", group: "Test Authoring & Execution", description: "Delete test cases and suites" },
  { id: "tests.execute", resource: "tests", action: "execute", group: "Test Authoring & Execution", description: "Trigger test runs and execution runners" },

  // Test Runs
  { id: "test_runs.view", resource: "test_runs", action: "view", group: "Test Execution Operations", description: "Monitor active test executions and live streams" },
  { id: "test_runs.create", resource: "test_runs", action: "create", group: "Test Execution Operations", description: "Schedule or queue test batch executions" },
  { id: "test_runs.cancel", resource: "test_runs", action: "cancel", group: "Test Execution Operations", description: "Abort or cancel running test workers" },

  // Evidence & Artifacts
  { id: "evidence.view", resource: "evidence", action: "view", group: "Evidence & Diagnostics", description: "Inspect execution traces, network HARs, and screenshots" },
  { id: "evidence.create", resource: "evidence", action: "create", group: "Evidence & Diagnostics", description: "Upload or attach diagnostics evidence artifacts" },
  { id: "evidence.delete", resource: "evidence", action: "delete", group: "Evidence & Diagnostics", description: "Purge or delete sensitive evidence files" },

  // Bugs & Defects
  { id: "bugs.view", resource: "bugs", action: "view", group: "Defect Management", description: "View identified flaws, failures, and Jira bug candidates" },
  { id: "bugs.create", resource: "bugs", action: "create", group: "Defect Management", description: "Approve AI bug candidates and publish to trackers" },
  { id: "bugs.update", resource: "bugs", action: "update", group: "Defect Management", description: "Update bug priority, triage status, and root cause notes" },
  { id: "bugs.delete", resource: "bugs", action: "delete", group: "Defect Management", description: "Dismiss or delete bug entries" },

  // Quality & Risk Intelligence
  { id: "quality.view", resource: "quality", action: "view", group: "Quality Intelligence", description: "Inspect quality graph, coverage metrics, and SLAs" },
  { id: "risk.view", resource: "risk", action: "view", group: "Quality Intelligence", description: "Analyze failure risk trends and regression hazards" },
  { id: "flakiness.view", resource: "flakiness", action: "view", group: "Quality Intelligence", description: "Inspect flaky tests and stability analytics" },

  // Policies & Governance
  { id: "policies.view", resource: "policies", action: "view", group: "Governance & Policies", description: "View quality gates and quarantine policies" },
  { id: "policies.create", resource: "policies", action: "create", group: "Governance & Policies", description: "Define new compliance guardrails and thresholds" },
  { id: "policies.update", resource: "policies", action: "update", group: "Governance & Policies", description: "Edit or override governance policies" },
  { id: "policies.delete", resource: "policies", action: "delete", group: "Governance & Policies", description: "Remove governance policies" },

  // Audit Logs
  { id: "audit_logs.view", resource: "audit_logs", action: "view", group: "Audit & Compliance", description: "Inspect tamper-proof audit trails and access events" },

  // Reports
  { id: "reports.view", resource: "reports", action: "view", group: "Reporting", description: "View quality scorecards and release readiness summaries" },
  { id: "reports.create", resource: "reports", action: "create", group: "Reporting", description: "Generate ad-hoc compliance reports" },
  { id: "reports.export", resource: "reports", action: "export", group: "Reporting", description: "Export CSV/PDF audit packets and raw results" },

  // AI Models
  { id: "ai_models.view", resource: "ai_models", action: "view", group: "AI & Model Operations", description: "Monitor model router, Ollama, and Gemini inference latency" },
  { id: "ai_models.manage", resource: "ai_models", action: "manage", group: "AI & Model Operations", description: "Configure model routing, fallbacks, and prompt parameters" },

  // API Keys
  { id: "api_keys.view", resource: "api_keys", action: "view", group: "Security & API Keys", description: "Inspect provisioned API credentials and service tokens" },
  { id: "api_keys.create", resource: "api_keys", action: "create", group: "Security & API Keys", description: "Generate new CI/CD pipeline API tokens" },
  { id: "api_keys.revoke", resource: "api_keys", action: "revoke", group: "Security & API Keys", description: "Revoke or expire API tokens" },

  // System & Diagnostics
  { id: "system.view", resource: "system", action: "view", group: "System Administration", description: "View cluster health, worker queues, and diagnostics" },
  { id: "system.manage", resource: "system", action: "manage", group: "System Administration", description: "Execute chaos injection, restart workers, and configure nodes" }
];

const allPermissionIds = RBAC_PERMISSIONS.map((p) => p.id);

// 2. Default Built-in Roles
export const RBAC_ROLES: RbacRoleDef[] = [
  {
    id: "super_admin",
    name: "Super Admin",
    description: "Full global access across all tenants, users, roles, system controls, and policy overrides.",
    type: "system",
    isProtected: true,
    permissions: ["*"] // Grants all permissions
  },
  {
    id: "org_admin",
    name: "Organization Admin",
    description: "Full administration inside organization/tenant. Manages users, roles, projects, policies, audit logs, and test suites.",
    type: "system",
    isProtected: true,
    permissions: allPermissionIds.filter((p) => !p.startsWith("system.manage"))
  },
  {
    id: "project_admin",
    name: "Project Admin",
    description: "Full operational access within assigned projects. Manages test cases, test runs, evidence, bugs, quality metrics, and reports.",
    type: "system",
    isProtected: true,
    permissions: [
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
    ]
  },
  {
    id: "qa_engineer",
    name: "QA Engineer",
    description: "Author and execute tests, inspect failures and evidence, review AI bug candidates, and view quality metrics.",
    type: "system",
    isProtected: true,
    permissions: [
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
    ]
  },
  {
    id: "developer",
    name: "Developer",
    description: "View assigned projects, test runs, failure traces, evidence, bugs, and execute ad-hoc validation tests.",
    type: "system",
    isProtected: true,
    permissions: [
      "projects.view",
      "tests.view", "tests.execute",
      "test_runs.view",
      "evidence.view",
      "bugs.view", "bugs.create",
      "quality.view", "risk.view", "flakiness.view",
      "reports.view",
      "ai_models.view",
      "system.view"
    ]
  },
  {
    id: "viewer",
    name: "Viewer",
    description: "Read-only access to published dashboards, test results, quality analytics, evidence, and reports.",
    type: "system",
    isProtected: true,
    permissions: [
      "projects.view",
      "tests.view",
      "test_runs.view",
      "evidence.view",
      "bugs.view",
      "quality.view", "risk.view", "flakiness.view",
      "policies.view",
      "reports.view",
      "system.view"
    ]
  }
];

// 3. Tenants
export const RBAC_TENANTS: RbacTenantDef[] = [
  { id: "tenant_acme", name: "Acme AI Corp", slug: "acme-corp", tier: "Enterprise", status: "active" },
  { id: "tenant_globex", name: "Globex Cloud Systems", slug: "globex-cloud", tier: "Business", status: "active" },
  { id: "tenant_cyberdyne", name: "Cyberdyne Systems", slug: "cyberdyne", tier: "Enterprise", status: "active" }
];

// 4. Initial Users
export let rbacUsers: RbacUserDef[] = [
  {
    id: "usr-super-admin",
    email: "alex.vance@aiqa-core.internal",
    name: "Alex Vance",
    tenantId: "tenant_acme",
    tenantName: "Acme AI Corp (Global)",
    roleId: "super_admin",
    roleName: "Super Admin",
    projectIds: ["all", "Apex AI Agent", "Payment Core API", "Search RAG Pipeline", "Omni LLM Service"],
    status: "active",
    lastActive: "Just now",
    createdAt: "2026-01-10T08:00:00Z"
  },
  {
    id: "usr-org-admin",
    email: "sarah.connor@acme.corp",
    name: "Sarah Connor",
    tenantId: "tenant_acme",
    tenantName: "Acme AI Corp",
    roleId: "org_admin",
    roleName: "Organization Admin",
    projectIds: ["all", "Apex AI Agent", "Payment Core API", "Search RAG Pipeline", "Omni LLM Service"],
    status: "active",
    lastActive: "5 mins ago",
    createdAt: "2026-01-15T09:30:00Z"
  },
  {
    id: "usr-proj-admin",
    email: "marcus.wright@acme.corp",
    name: "Marcus Wright",
    tenantId: "tenant_acme",
    tenantName: "Acme AI Corp",
    roleId: "project_admin",
    roleName: "Project Admin",
    projectIds: ["Apex AI Agent", "Payment Core API"],
    status: "active",
    lastActive: "12 mins ago",
    createdAt: "2026-02-01T11:20:00Z"
  },
  {
    id: "usr-qa-eng",
    email: "devon.miles@acme.corp",
    name: "Devon Miles",
    tenantId: "tenant_acme",
    tenantName: "Acme AI Corp",
    roleId: "qa_engineer",
    roleName: "QA Engineer",
    projectIds: ["Apex AI Agent", "Payment Core API", "Search RAG Pipeline"],
    status: "active",
    lastActive: "2 mins ago",
    createdAt: "2026-02-10T14:15:00Z"
  },
  {
    id: "usr-dev",
    email: "elena.rostova@acme.corp",
    name: "Elena Rostova",
    tenantId: "tenant_acme",
    tenantName: "Acme AI Corp",
    roleId: "developer",
    roleName: "Developer",
    projectIds: ["Apex AI Agent"],
    status: "active",
    lastActive: "35 mins ago",
    createdAt: "2026-02-18T16:45:00Z"
  },
  {
    id: "usr-viewer",
    email: "david.bowman@acme.corp",
    name: "David Bowman",
    tenantId: "tenant_acme",
    tenantName: "Acme AI Corp",
    roleId: "viewer",
    roleName: "Viewer",
    projectIds: ["all"],
    status: "active",
    lastActive: "2 hours ago",
    createdAt: "2026-03-01T10:00:00Z"
  },
  {
    id: "usr-globex-admin",
    email: "viktor.stone@globex.internal",
    name: "Viktor Stone (Tenant B)",
    tenantId: "tenant_globex",
    tenantName: "Globex Cloud Systems",
    roleId: "org_admin",
    roleName: "Organization Admin",
    projectIds: ["Globex Core Storage"],
    status: "active",
    lastActive: "1 hour ago",
    createdAt: "2026-03-05T12:00:00Z"
  },
  {
    id: "usr-disabled",
    email: "thomas.anderson@acme.corp",
    name: "Thomas Anderson (Locked)",
    tenantId: "tenant_acme",
    tenantName: "Acme AI Corp",
    roleId: "qa_engineer",
    roleName: "QA Engineer",
    projectIds: ["Apex AI Agent"],
    status: "disabled",
    lastActive: "3 days ago",
    createdAt: "2026-01-20T09:00:00Z"
  }
];

export let rbacRoles: RbacRoleDef[] = [...RBAC_ROLES];

// 5. Active Session User (defaults to Devon Miles QA Engineer or Alex Vance Super Admin)
export let currentSessionUserId = "usr-super-admin";

// 6. Immutable RBAC Audit Events
export let rbacAuditEvents: RbacAuditEventDef[] = [
  {
    id: "rbac-aud-108",
    timestamp: new Date(Date.now() - 120000).toLocaleTimeString(),
    actorId: "usr-dev",
    actorEmail: "marcus.brody@acme.corp",
    tenantId: "tenant_acme",
    action: "PRIVILEGE_ESCALATION_ATTEMPT_BLOCKED",
    resourceType: "user",
    resourceId: "usr-dev",
    oldValue: "Developer",
    newValue: "Super Admin",
    ip: "192.168.4.112",
    result: "BLOCKED",
    details: "Unauthorized privilege escalation: developer account attempted self-promotion to Super Admin"
  },
  {
    id: "rbac-aud-107",
    timestamp: new Date(Date.now() - 480000).toLocaleTimeString(),
    actorId: "usr-org-admin",
    actorEmail: "sarah.connor@acme.corp",
    tenantId: "tenant_acme",
    action: "ROLE_PERMISSIONS_UPDATE",
    resourceType: "role",
    resourceId: "developer",
    oldValue: "11 permissions",
    newValue: "13 permissions (+ bugs.create, tests.execute)",
    ip: "10.0.1.45",
    result: "SUCCESS",
    details: "Updated permission matrix for role 'Developer' to allow ad-hoc test validation and bug creation"
  },
  {
    id: "rbac-aud-106",
    timestamp: new Date(Date.now() - 960000).toLocaleTimeString(),
    actorId: "usr-org-admin",
    actorEmail: "sarah.connor@acme.corp",
    tenantId: "tenant_acme",
    action: "USER_ROLE_CHANGE",
    resourceType: "user",
    resourceId: "usr-qa-eng",
    oldValue: "Developer",
    newValue: "QA Engineer",
    ip: "10.0.1.45",
    result: "SUCCESS",
    details: "Promoted user 'Devon Miles' from Developer to QA Engineer with test management permissions"
  },
  {
    id: "rbac-aud-105",
    timestamp: new Date(Date.now() - 1440000).toLocaleTimeString(),
    actorId: "usr-globex-admin",
    actorEmail: "viktor.stone@globex.internal",
    tenantId: "tenant_globex",
    action: "CROSS_TENANT_IDOR_BLOCKED",
    resourceType: "tenant_secret",
    resourceId: "tenant_acme/api_keys",
    ip: "172.16.88.9",
    result: "BLOCKED",
    details: "Cross-tenant boundary protection: blocked unauthorized access to Acme AI Corp secret store from Globex Cloud"
  },
  {
    id: "rbac-aud-104",
    timestamp: new Date(Date.now() - 2100000).toLocaleTimeString(),
    actorId: "usr-super-admin",
    actorEmail: "alex.vance@aiqa-core.internal",
    tenantId: "tenant_acme",
    action: "ROLE_CREATE",
    resourceType: "role",
    resourceId: "compliance_auditor",
    newValue: "Created role 'Compliance Auditor' with 6 view-only scopes",
    ip: "10.0.1.2",
    result: "SUCCESS",
    details: "Provisioned custom role for third-party SOC2 compliance inspectors"
  },
  {
    id: "rbac-aud-103",
    timestamp: new Date(Date.now() - 2800000).toLocaleTimeString(),
    actorId: "usr-org-admin",
    actorEmail: "sarah.connor@acme.corp",
    tenantId: "tenant_acme",
    action: "USER_DISABLE",
    resourceType: "user",
    resourceId: "usr-disabled",
    oldValue: "active",
    newValue: "disabled",
    ip: "10.0.1.45",
    result: "SUCCESS",
    details: "Temporarily suspended account 'Thomas Anderson' pending annual credential re-certification"
  },
  {
    id: "rbac-aud-102",
    timestamp: new Date(Date.now() - 3600000).toLocaleTimeString(),
    actorId: "usr-org-admin",
    actorEmail: "sarah.connor@acme.corp",
    tenantId: "tenant_acme",
    action: "USER_PROJECT_GRANT",
    resourceType: "user",
    resourceId: "usr-qa-eng",
    oldValue: "2 projects",
    newValue: "3 projects (+ Search RAG Pipeline)",
    ip: "10.0.1.45",
    result: "SUCCESS",
    details: "Assigned project membership for automated RAG quality audits"
  },
  {
    id: "rbac-aud-101",
    timestamp: new Date(Date.now() - 4500000).toLocaleTimeString(),
    actorId: "usr-super-admin",
    actorEmail: "alex.vance@aiqa-core.internal",
    tenantId: "tenant_acme",
    action: "ROLE_PERMISSION_SYNC",
    resourceType: "role",
    resourceId: "qa_engineer",
    oldValue: "18 permissions",
    newValue: "19 permissions",
    ip: "10.0.1.2",
    result: "SUCCESS",
    details: "Enforced zero-trust RBAC policies across cluster nodes"
  }
];

export function recordRbacAuditEvent(event: Omit<RbacAuditEventDef, "id" | "timestamp">) {
  const newEvent: RbacAuditEventDef = {
    id: `rbac-aud-${Date.now().toString().slice(-6)}`,
    timestamp: new Date().toLocaleTimeString(),
    ...event
  };
  rbacAuditEvents.unshift(newEvent);
  if (rbacAuditEvents.length > 200) {
    rbacAuditEvents.pop();
  }
  return newEvent;
}

// 7. Core Authorization Engine
export interface AuthCheckOptions {
  tenantId?: string;
  projectId?: string;
  resourceType?: string;
  resourceId?: string;
}

export interface AuthDecision {
  authorized: boolean;
  status: number;
  decision: "ALLOWED" | "DENIED";
  reason: string;
  user: RbacUserDef | null;
  role: RbacRoleDef | null;
  grantedPermissions: string[];
}

export function authorize(
  user: RbacUserDef | null,
  requiredPermission: string,
  options?: AuthCheckOptions
): AuthDecision {
  // Step 1: Authentication Check
  if (!user) {
    return {
      authorized: false,
      status: 401,
      decision: "DENIED",
      reason: "Authentication Required: No valid session or authorization header provided",
      user: null,
      role: null,
      grantedPermissions: []
    };
  }

  // Step 2: Account Status Check
  if (user.status !== "active") {
    return {
      authorized: false,
      status: 403,
      decision: "DENIED",
      reason: `Forbidden: User account '${user.email}' is disabled/suspended`,
      user,
      role: null,
      grantedPermissions: []
    };
  }

  // Step 3: Resolve Role & Permissions
  const role = rbacRoles.find((r) => r.id === user.roleId);
  if (!role) {
    return {
      authorized: false,
      status: 403,
      decision: "DENIED",
      reason: `Forbidden: Role '${user.roleId}' not recognized or was removed`,
      user,
      role: null,
      grantedPermissions: []
    };
  }

  const hasWildcard = role.permissions.includes("*");
  const hasSpecificPerm = role.permissions.includes(requiredPermission);
  const isPermitted = hasWildcard || hasSpecificPerm;

  if (!isPermitted) {
    return {
      authorized: false,
      status: 403,
      decision: "DENIED",
      reason: `Access Denied: Role '${role.name}' lacks required permission '${requiredPermission}'`,
      user,
      role,
      grantedPermissions: role.permissions
    };
  }

  // Step 4: Multi-Tenant Boundary Enforcement
  if (options?.tenantId && role.id !== "super_admin") {
    if (user.tenantId !== options.tenantId) {
      return {
        authorized: false,
        status: 403,
        decision: "DENIED",
        reason: `Cross-Tenant Access Violation: User is bound to tenant '${user.tenantId}', requested resource belongs to '${options.tenantId}'`,
        user,
        role,
        grantedPermissions: role.permissions
      };
    }
  }

  // Step 5: Project-Level Access Control Enforcement
  if (options?.projectId && role.id !== "super_admin" && role.id !== "org_admin") {
    const hasAllProjects = user.projectIds.includes("all");
    const hasAssignedProject = user.projectIds.includes(options.projectId);
    if (!hasAllProjects && !hasAssignedProject) {
      return {
        authorized: false,
        status: 403,
        decision: "DENIED",
        reason: `Project Access Denied: User is not assigned to project '${options.projectId}'`,
        user,
        role,
        grantedPermissions: role.permissions
      };
    }
  }

  return {
    authorized: true,
    status: 200,
    decision: "ALLOWED",
    reason: `Authorized: Role '${role.name}' holds permission '${requiredPermission}'`,
    user,
    role,
    grantedPermissions: role.permissions
  };
}

// Helper to resolve user from request
export function resolveRequestUser(req: Request): RbacUserDef | null {
  const authHeader = req.headers["authorization"];
  const customUserHeader = req.headers["x-user-id"] as string;

  let userIdToFind = currentSessionUserId;

  if (customUserHeader) {
    userIdToFind = customUserHeader;
  } else if (authHeader && authHeader.startsWith("Bearer ")) {
    const token = authHeader.substring(7).trim();
    if (token && token !== "null" && token !== "undefined") {
      userIdToFind = token;
    }
  }

  const user = rbacUsers.find((u) => u.id === userIdToFind || u.email === userIdToFind);
  return user || null;
}

// 8. Express Authorization Middleware Generator
export function requireRbacPermission(
  requiredPermission: string,
  getContext?: (req: Request) => AuthCheckOptions
) {
  return (req: Request, res: Response, next: NextFunction) => {
    const user = resolveRequestUser(req);
    const context = getContext ? getContext(req) : {};
    
    // Default context from params/body if available
    if (!context.tenantId && req.body?.tenantId) context.tenantId = req.body.tenantId;
    if (!context.projectId && (req.body?.project || req.params?.projectId)) {
      context.projectId = req.body?.project || req.params?.projectId;
    }

    const decision = authorize(user, requiredPermission, context);

    if (!decision.authorized) {
      // Record denied audit log for security telemetry
      recordRbacAuditEvent({
        actorId: user?.id || "anonymous",
        actorEmail: user?.email || "unauthenticated@ip",
        tenantId: user?.tenantId || context.tenantId || "unknown",
        action: `BLOCKED_${requiredPermission.toUpperCase().replace(".", "_")}`,
        resourceType: context.resourceType || requiredPermission.split(".")[0],
        resourceId: context.resourceId || req.path,
        ip: req.ip || "127.0.0.1",
        result: decision.status === 401 ? "FAILED" : "DENIED",
        details: decision.reason
      });

      return res.status(decision.status).json({
        error: decision.status === 401 ? "Unauthorized" : "Forbidden",
        message: decision.reason,
        requiredPermission,
        decision: decision.decision
      });
    }

    // Attach user and role to request for downstream handlers
    (req as any).rbacUser = decision.user;
    (req as any).rbacRole = decision.role;
    next();
  };
}

export function getCurrentSessionUser(): RbacUserDef {
  const user = rbacUsers.find((u) => u.id === currentSessionUserId) || rbacUsers[0];
  return user;
}

export function setCurrentSessionUser(userId: string): RbacUserDef | null {
  const user = rbacUsers.find((u) => u.id === userId);
  if (user) {
    currentSessionUserId = user.id;
    recordRbacAuditEvent({
      actorId: user.id,
      actorEmail: user.email,
      tenantId: user.tenantId,
      action: "SWITCH_SESSION_PERSONA",
      resourceType: "session",
      resourceId: user.id,
      ip: "127.0.0.1",
      result: "SUCCESS",
      details: `Switched active preview persona to '${user.name}' (${user.roleName})`
    });
    return user;
  }
  return null;
}
