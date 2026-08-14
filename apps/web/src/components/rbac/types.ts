export type RoleId = "super_admin" | "org_admin" | "project_admin" | "qa_engineer";

export type EventResult = "AUTHORIZED" | "FORBIDDEN";

export interface RoleConfig {
  id: RoleId;
  name: string;
  badgeLabel: string;
  email: string;
  sessionToken: string;
  tenant: string;
  status: "active" | "connected" | "reconnecting" | "disconnected";
  avatarInitials: string;
  color: {
    primary: string;
    border: string;
    bg: string;
    glow: string;
    text: string;
    chip: string;
  };
  allowedPermissions: string[];
  deniedPermissions: string[];
  permissionSummary: {
    users: boolean;
    projects: boolean;
    tests: boolean;
    evidence: boolean;
    bugs: boolean;
    reports: boolean;
    policies: boolean;
    auditLogs: boolean;
    aiModels: boolean;
    system: boolean;
  };
  actions: RoleActionItem[];
}

export interface RoleActionItem {
  id: string;
  label: string;
  permission: string;
  resource: string;
  method: "GET" | "POST" | "PUT" | "DELETE" | "PATCH";
  endpoint: string;
  isAllowed: boolean;
  requiresLock: boolean;
  category: string;
  description: string;
}

export interface LiveRbacEvent {
  id: string;
  timestamp: string;
  timeFormatted: string;
  roleId: RoleId;
  roleName: string;
  actorEmail: string;
  action: string;
  permission: string;
  resource: string;
  result: EventResult;
  statusCode: number;
  latencyMs: number;
  reason: string;
  tenantId: string;
  checks: {
    authentication: boolean;
    permissionCheck: boolean;
    tenantCheck: boolean;
    projectAccess: boolean;
    policyCheck: boolean;
  };
  payloadPreview?: string;
}

export interface ActiveFlowAnimation {
  id: string;
  roleId: RoleId;
  result: EventResult;
  timestamp: number;
  actionLabel: string;
  permission: string;
  step: "request" | "engine" | "decision" | "resource" | "completed";
}
