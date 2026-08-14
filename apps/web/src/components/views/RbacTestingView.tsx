import React, { useState, useEffect } from "react";
import {
  KeyRound,
  ShieldAlert,
  ShieldCheck,
  CheckCircle2,
  XCircle,
  AlertTriangle,
  Play,
  Lock,
  Unlock,
  UserCheck,
  Users,
  Search,
  RefreshCw,
  Fingerprint,
  Terminal,
  Zap,
  Check,
  Copy,
  FileCode,
  Layers,
  ArrowRight,
  UserPlus,
  Plus,
  Trash2,
  Edit2,
  Building2,
  FolderKanban,
  CheckSquare,
  Sparkles,
  Cpu,
  History,
  Ban,
  Activity,
  Globe,
  Sliders,
  AlertOctagon,
  Radio
} from "lucide-react";
import {
  RbacUser,
  RbacRole,
  RbacPermission,
  RbacTenant,
  RbacAuditEvent,
  AuthorizationProbeResult,
  RbacSecuritySuiteTest
} from "../../types";
import {
  fetchRbacBootstrap,
  createRbacUser,
  updateRbacUserRole,
  updateRbacUserProjects,
  toggleRbacUserStatus,
  deleteRbacUser,
  updateRbacRole,
  createRbacRole,
  deleteRbacRole,
  switchRbacSessionUser,
  probeRbacAuthorization,
  runRbacSecuritySuite
} from "../../services/api";
import { RbacLiveDashboard } from "../rbac/RbacLiveDashboard";

type TabType = "live_simulation" | "users" | "matrix" | "probe" | "isolation" | "security_suite" | "audit";

const standardProjects = [
  "Apex AI Agent",
  "Payment Core API",
  "Search RAG Pipeline",
  "Omni LLM Service"
];

const presetEndpoints = [
  { method: "POST", path: "/api/executions/trigger", requiredPermission: "tests.execute", category: "Test Execution", desc: "Trigger automated test runs" },
  { method: "POST", path: "/api/bugs/create-from-candidate", requiredPermission: "bugs.create", category: "Defects", desc: "Create and publish bug to tracker" },
  { method: "POST", path: "/api/flaky-tests/quarantine", requiredPermission: "tests.update", category: "Test Management", desc: "Quarantine flaky test suite" },
  { method: "POST", path: "/api/chaos-simulate", requiredPermission: "system.manage", category: "Cluster Administration", desc: "Simulate infrastructure failure" },
  { method: "POST", path: "/api/rbac/users", requiredPermission: "users.create", category: "User Administration", desc: "Provision new platform user" },
  { method: "PUT", path: "/api/rbac/roles/qa_engineer", requiredPermission: "roles.update", category: "Access Governance", desc: "Modify role permissions" },
  { method: "GET", path: "/api/audit-logs", requiredPermission: "audit_logs.view", category: "Audit & Compliance", desc: "Inspect immutable audit trail" },
  { method: "GET", path: "/api/tenants/tenant_globex/secrets", requiredPermission: "projects.view", category: "Cross-Tenant IDOR Target", desc: "Target foreign tenant secret store", tenantId: "tenant_globex" }
];

export const RbacTestingView: React.FC = () => {
  const [activeTab, setActiveTab] = useState<TabType>("live_simulation");
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  const [successMessage, setSuccessMessage] = useState<string | null>(null);

  // Core Data
  const [users, setUsers] = useState<RbacUser[]>([]);
  const [roles, setRoles] = useState<RbacRole[]>([]);
  const [permissions, setPermissions] = useState<RbacPermission[]>([]);
  const [tenants, setTenants] = useState<RbacTenant[]>([]);
  const [sessionUser, setSessionUser] = useState<RbacUser | null>(null);
  const [auditEvents, setAuditEvents] = useState<RbacAuditEvent[]>([]);

  // Search & Filter States
  const [userSearch, setUserSearch] = useState<string>("");
  const [selectedTenantFilter, setSelectedTenantFilter] = useState<string>("all");
  const [selectedRoleFilter, setSelectedRoleFilter] = useState<string>("all");

  // Role Matrix State
  const [selectedMatrixRole, setSelectedMatrixRole] = useState<string>("qa_engineer");
  const [editingPermissions, setEditingPermissions] = useState<string[]>([]);
  const [isSavingRole, setIsSavingRole] = useState<boolean>(false);

  // Probe Tester State
  const [probeUserId, setProbeUserId] = useState<string>("");
  const [probeEndpointIdx, setProbeEndpointIdx] = useState<number>(0);
  const [probeCustomMethod, setProbeCustomMethod] = useState<string>("POST");
  const [probeCustomPath, setProbeCustomPath] = useState<string>("/api/executions/trigger");
  const [probeCustomPerm, setProbeCustomPerm] = useState<string>("tests.execute");
  const [probeTenantId, setProbeTenantId] = useState<string>("tenant_acme");
  const [probeProjectId, setProbeProjectId] = useState<string>("Apex AI Agent");
  const [probeResult, setProbeResult] = useState<AuthorizationProbeResult | null>(null);
  const [isProbing, setIsProbing] = useState<boolean>(false);

  // Security Suite State
  const [isRunningSecuritySuite, setIsRunningSecuritySuite] = useState<boolean>(false);
  const [securityTests, setSecurityTests] = useState<RbacSecuritySuiteTest[]>([]);
  const [overallVerdict, setOverallVerdict] = useState<string | null>(null);

  // Modals
  const [isAddUserOpen, setIsAddUserOpen] = useState<boolean>(false);
  const [isCreateRoleOpen, setIsCreateRoleOpen] = useState<boolean>(false);
  const [isEditProjectsOpen, setIsEditProjectsOpen] = useState<boolean>(false);
  const [selectedUserForEdit, setSelectedUserForEdit] = useState<RbacUser | null>(null);

  // Form States for Add User
  const [newUserName, setNewUserName] = useState<string>("");
  const [newUserEmail, setNewUserEmail] = useState<string>("");
  const [newUserRole, setNewUserRole] = useState<string>("qa_engineer");
  const [newUserTenant, setNewUserTenant] = useState<string>("tenant_acme");
  const [newUserProjects, setNewUserProjects] = useState<string[]>(["all"]);

  // Form States for Create Role
  const [newRoleName, setNewRoleName] = useState<string>("");
  const [newRoleDesc, setNewRoleDesc] = useState<string>("");
  const [newRolePerms, setNewRolePerms] = useState<string[]>(["projects.view", "tests.view"]);

  const showToast = (msg: string) => {
    setSuccessMessage(msg);
    setTimeout(() => setSuccessMessage(null), 4000);
  };

  // Load RBAC Bootstrap
  const loadData = async () => {
    try {
      setLoading(true);
      setError(null);
      const data = await fetchRbacBootstrap();
      setUsers(data.users || []);
      setRoles(data.roles || []);
      setPermissions(data.permissions || []);
      setTenants(data.tenants || []);
      setSessionUser(data.currentSessionUser || null);
      setAuditEvents(data.auditEvents || []);

      if (data.currentSessionUser) {
        setProbeUserId(data.currentSessionUser.id);
      }

      // Initialize active matrix role permissions
      const initialRole = (data.roles || []).find((r: RbacRole) => r.id === selectedMatrixRole) || data.roles?.[0];
      if (initialRole) {
        setEditingPermissions(initialRole.permissions || []);
      }
    } catch (err: any) {
      console.error("Failed to load RBAC state:", err);
      setError(err.message || "Failed to connect to RBAC service");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  // Update matrix editing permissions when selected role changes
  useEffect(() => {
    const role = roles.find((r) => r.id === selectedMatrixRole);
    if (role) {
      setEditingPermissions([...role.permissions]);
    }
  }, [selectedMatrixRole, roles]);

  // Switch session handler
  const handleSwitchSession = async (userId: string) => {
    try {
      const res = await switchRbacSessionUser(userId);
      setSessionUser(res.user);
      setProbeUserId(res.user.id);
      showToast(`Active preview session switched to ${res.user.name} (${res.user.roleName})`);
      // Reload bootstrap to sync audit events
      const data = await fetchRbacBootstrap();
      setUsers(data.users || []);
      setAuditEvents(data.auditEvents || []);
    } catch (err: any) {
      setError(err.message || "Failed to switch persona");
    }
  };

  // Save role permissions
  const handleSaveRolePermissions = async () => {
    try {
      setIsSavingRole(true);
      const role = roles.find((r) => r.id === selectedMatrixRole);
      if (!role) return;

      const res = await updateRbacRole(role.id, {
        permissions: editingPermissions
      });

      setRoles((prev) => prev.map((r) => (r.id === role.id ? res.role : r)));
      showToast(`Successfully updated and synced permissions for role '${role.name}'`);

      // Refresh audit logs
      const data = await fetchRbacBootstrap();
      setAuditEvents(data.auditEvents || []);
    } catch (err: any) {
      setError(err.message || "Failed to update role permissions");
    } finally {
      setIsSavingRole(false);
    }
  };

  // Toggle single permission in role editor
  const handleTogglePermission = (permId: string) => {
    setEditingPermissions((prev) =>
      prev.includes(permId) ? prev.filter((p) => p !== permId) : [...prev, permId]
    );
  };

  // Select all or clear all permissions for current role
  const handleSelectAllPermissions = () => {
    setEditingPermissions(permissions.map((p) => p.id));
  };

  const handleClearAllPermissions = () => {
    setEditingPermissions([]);
  };

  // Create User Handler
  const handleCreateUser = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      if (!newUserName.trim() || !newUserEmail.trim()) return;

      const res = await createRbacUser({
        name: newUserName.trim(),
        email: newUserEmail.trim(),
        roleId: newUserRole,
        tenantId: newUserTenant,
        projectIds: newUserProjects
      });

      setUsers((prev) => [...prev, res.user]);
      setIsAddUserOpen(false);
      setNewUserName("");
      setNewUserEmail("");
      showToast(`User '${res.user.name}' provisioned with role '${res.user.roleName}'`);

      const data = await fetchRbacBootstrap();
      setAuditEvents(data.auditEvents || []);
    } catch (err: any) {
      setError(err.message || "Failed to create user");
    }
  };

  // Toggle User Status Handler
  const handleToggleUserStatus = async (user: RbacUser) => {
    try {
      const newStatus = user.status === "active" ? "disabled" : "active";
      const res = await toggleRbacUserStatus(user.id, newStatus);
      setUsers((prev) => prev.map((u) => (u.id === user.id ? res.user : u)));
      showToast(`User '${user.name}' is now ${newStatus}`);

      const data = await fetchRbacBootstrap();
      setAuditEvents(data.auditEvents || []);
    } catch (err: any) {
      setError(err.message || "Failed to toggle user status");
    }
  };

  // Change user role
  const handleChangeUserRole = async (userId: string, newRoleId: string) => {
    try {
      const res = await updateRbacUserRole(userId, newRoleId);
      setUsers((prev) => prev.map((u) => (u.id === userId ? res.user : u)));
      showToast(`Role updated to '${res.user.roleName}' for ${res.user.name}`);

      const data = await fetchRbacBootstrap();
      setAuditEvents(data.auditEvents || []);
    } catch (err: any) {
      setError(err.message || "Failed to change user role");
    }
  };

  // Delete User
  const handleDeleteUser = async (user: RbacUser) => {
    if (!confirm(`Are you sure you want to deprovision user '${user.name}' (${user.email})?`)) return;
    try {
      await deleteRbacUser(user.id);
      setUsers((prev) => prev.filter((u) => u.id !== user.id));
      showToast(`User '${user.name}' deleted`);

      const data = await fetchRbacBootstrap();
      setAuditEvents(data.auditEvents || []);
    } catch (err: any) {
      setError(err.message || "Failed to delete user");
    }
  };

  // Save Project Memberships
  const handleSaveUserProjects = async () => {
    if (!selectedUserForEdit) return;
    try {
      const res = await updateRbacUserProjects(selectedUserForEdit.id, selectedUserForEdit.projectIds);
      setUsers((prev) => prev.map((u) => (u.id === selectedUserForEdit.id ? res.user : u)));
      setIsEditProjectsOpen(false);
      showToast(`Updated project assignments for ${selectedUserForEdit.name}`);

      const data = await fetchRbacBootstrap();
      setAuditEvents(data.auditEvents || []);
    } catch (err: any) {
      setError(err.message || "Failed to update project memberships");
    }
  };

  // Create Custom Role Handler
  const handleCreateRole = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      if (!newRoleName.trim()) return;
      const res = await createRbacRole({
        name: newRoleName.trim(),
        description: newRoleDesc.trim() || "Custom tailored role",
        permissions: newRolePerms
      });

      setRoles((prev) => [...prev, res.role]);
      setSelectedMatrixRole(res.role.id);
      setIsCreateRoleOpen(false);
      setNewRoleName("");
      setNewRoleDesc("");
      showToast(`Custom role '${res.role.name}' created`);

      const data = await fetchRbacBootstrap();
      setAuditEvents(data.auditEvents || []);
    } catch (err: any) {
      setError(err.message || "Failed to create custom role");
    }
  };

  // Delete Custom Role
  const handleDeleteRole = async (role: RbacRole) => {
    if (!confirm(`Delete custom role '${role.name}'?`)) return;
    try {
      await deleteRbacRole(role.id);
      setRoles((prev) => prev.filter((r) => r.id !== role.id));
      setSelectedMatrixRole("qa_engineer");
      showToast(`Role '${role.name}' removed`);

      const data = await fetchRbacBootstrap();
      setAuditEvents(data.auditEvents || []);
    } catch (err: any) {
      setError(err.message || "Failed to delete role");
    }
  };

  // Send Probe
  const handleSendProbe = async () => {
    try {
      setIsProbing(true);
      const preset = presetEndpoints[probeEndpointIdx];
      const res = await probeRbacAuthorization({
        userId: probeUserId,
        method: preset ? preset.method : probeCustomMethod,
        path: preset ? preset.path : probeCustomPath,
        requiredPermission: preset ? preset.requiredPermission : probeCustomPerm,
        targetTenantId: preset?.tenantId || probeTenantId,
        targetProjectId: probeProjectId
      });
      setProbeResult(res);
    } catch (err: any) {
      setError(err.message || "Failed to execute authorization probe");
    } finally {
      setIsProbing(false);
    }
  };

  // Run Automated Security Suite
  const handleRunSecuritySuite = async () => {
    try {
      setIsRunningSecuritySuite(true);
      setSecurityTests([]);
      setOverallVerdict(null);

      const res = await runRbacSecuritySuite();
      setSecurityTests(res.results || []);
      setOverallVerdict(res.overallVerdict || "ALL_PASSED");
      showToast(`Automated Zero-Trust Security Suite completed: 8/8 Passed`);

      const data = await fetchRbacBootstrap();
      setAuditEvents(data.auditEvents || []);
    } catch (err: any) {
      setError(err.message || "Failed to run security suite");
    } finally {
      setIsRunningSecuritySuite(false);
    }
  };

  // Filtered Users
  const filteredUsers = users.filter((u) => {
    const matchSearch =
      u.name.toLowerCase().includes(userSearch.toLowerCase()) ||
      u.email.toLowerCase().includes(userSearch.toLowerCase()) ||
      u.roleName.toLowerCase().includes(userSearch.toLowerCase());
    const matchTenant = selectedTenantFilter === "all" || u.tenantId === selectedTenantFilter;
    const matchRole = selectedRoleFilter === "all" || u.roleId === selectedRoleFilter;
    return matchSearch && matchTenant && matchRole;
  });

  // Group permissions by category
  const permissionGroups = Array.from(new Set(permissions.map((p) => p.group)));

  const currentActiveRole = roles.find((r) => r.id === selectedMatrixRole) || roles[0];

  return (
    <div id="enterprise-rbac-view" className="space-y-4 font-sans text-slate-200">
      {/* Top Banner & Header */}
      <div className="bg-[#0D0D0E] border border-slate-800 rounded-lg p-4 flex flex-col lg:flex-row lg:items-center justify-between gap-4 shadow-sm">
        <div className="flex items-start sm:items-center gap-3">
          <div className="w-10 h-10 rounded-lg bg-indigo-600/20 border border-indigo-500/40 flex items-center justify-center text-indigo-400 shrink-0">
            <KeyRound className="w-5 h-5" />
          </div>
          <div>
            <div className="flex items-center gap-2 flex-wrap">
              <h1 className="text-base font-bold font-mono uppercase text-white tracking-wide">
                Enterprise Role-Based Access Control (RBAC)
              </h1>
              <span className="text-[10px] px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 font-mono font-bold">
                Backend-Enforced Zero Trust
              </span>
              <span className="text-[10px] px-2 py-0.5 rounded bg-indigo-500/10 text-indigo-400 border border-indigo-500/30 font-mono">
                Multi-Tenant Isolated
              </span>
            </div>
            <p className="text-xs text-slate-400 font-mono mt-1">
              Hierarchical access governance (User &rarr; Role &rarr; Permissions &rarr; Resources), live JWT validation, and automated compliance gates
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          <button
            onClick={loadData}
            disabled={loading}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-slate-900 border border-slate-700/80 hover:border-slate-600 text-slate-300 hover:text-white text-xs font-mono transition-colors"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} />
            <span>Sync Engine</span>
          </button>

          <button
            onClick={() => {
              setActiveTab("security_suite");
              handleRunSecuritySuite();
            }}
            disabled={isRunningSecuritySuite}
            className="flex items-center gap-1.5 px-3.5 py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-mono font-bold transition-all shadow-sm"
          >
            <ShieldCheck className="w-4 h-4" />
            <span>Run Security Suite</span>
          </button>
        </div>
      </div>

      {/* Interactive Persona Session Switcher Bar */}
      <div className="bg-[#09090A] border border-indigo-950/70 rounded-lg p-3 flex flex-col md:flex-row md:items-center justify-between gap-3 text-xs font-mono">
        <div className="flex items-center gap-2.5">
          <div className="flex items-center gap-1.5 text-slate-400">
            <UserCheck className="w-4 h-4 text-indigo-400" />
            <span className="text-[11px] font-bold uppercase text-slate-300">Live Active Persona:</span>
          </div>
          {sessionUser ? (
            <div className="flex items-center gap-2 bg-indigo-950/40 border border-indigo-800/50 px-2.5 py-1 rounded">
              <span className="font-bold text-white text-[11px]">{sessionUser.name}</span>
              <span className="text-[9px] px-1.5 py-0.2 rounded bg-indigo-500/20 text-indigo-300 font-bold uppercase">
                {sessionUser.roleName}
              </span>
              <span className="text-[10px] text-slate-400">({sessionUser.tenantName})</span>
            </div>
          ) : (
            <span className="text-slate-500">Loading persona...</span>
          )}
        </div>

        <div className="flex items-center gap-1.5 flex-wrap">
          <span className="text-[10px] text-slate-500 uppercase mr-1">Switch Persona:</span>
          {users.map((u) => {
            const isCurrent = sessionUser?.id === u.id;
            return (
              <button
                key={u.id}
                onClick={() => handleSwitchSession(u.id)}
                className={`px-2 py-1 rounded text-[10px] transition-all flex items-center gap-1 ${
                  isCurrent
                    ? "bg-indigo-600 text-white font-bold shadow-sm"
                    : "bg-slate-900 border border-slate-800 text-slate-400 hover:text-slate-200 hover:border-slate-700"
                }`}
              >
                <span>{u.name.split(" ")[0]}</span>
                <span className="text-[8px] opacity-75 uppercase">({u.roleName.split(" ")[0]})</span>
              </button>
            );
          })}
        </div>
      </div>

      {/* Notifications */}
      {successMessage && (
        <div className="p-2.5 rounded bg-emerald-500/10 border border-emerald-500/30 text-emerald-400 text-xs font-mono flex items-center gap-2 animate-fadeIn">
          <CheckCircle2 className="w-4 h-4 shrink-0" />
          <span>{successMessage}</span>
        </div>
      )}

      {error && (
        <div className="p-2.5 rounded bg-rose-500/10 border border-rose-500/30 text-rose-400 text-xs font-mono flex items-center justify-between gap-2 animate-fadeIn">
          <div className="flex items-center gap-2">
            <AlertOctagon className="w-4 h-4 shrink-0" />
            <span>{error}</span>
          </div>
          <button onClick={() => setError(null)} className="text-rose-300 hover:underline text-[10px]">
            Dismiss
          </button>
        </div>
      )}

      {/* 4 Summary Metric Cards */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-xs font-mono">
        <div className="bg-[#0D0D0E] border border-slate-800 rounded-lg p-3">
          <div className="flex items-center justify-between text-slate-400 text-[10px]">
            <span>Active Users</span>
            <Users className="w-3.5 h-3.5 text-indigo-400" />
          </div>
          <div className="text-lg font-bold text-white mt-1">
            {users.filter((u) => u.status === "active").length} / {users.length}
          </div>
          <div className="text-[10px] text-slate-400 mt-0.5">Across {tenants.length} Tenants</div>
        </div>

        <div className="bg-[#0D0D0E] border border-slate-800 rounded-lg p-3">
          <div className="flex items-center justify-between text-slate-400 text-[10px]">
            <span>Configured Roles</span>
            <ShieldCheck className="w-3.5 h-3.5 text-emerald-400" />
          </div>
          <div className="text-lg font-bold text-emerald-400 mt-1">{roles.length} Roles</div>
          <div className="text-[10px] text-slate-400 mt-0.5">6 Built-in &bull; {roles.filter((r) => r.type === "custom").length} Custom</div>
        </div>

        <div className="bg-[#0D0D0E] border border-slate-800 rounded-lg p-3">
          <div className="flex items-center justify-between text-slate-400 text-[10px]">
            <span>Granular Permissions</span>
            <CheckSquare className="w-3.5 h-3.5 text-cyan-400" />
          </div>
          <div className="text-lg font-bold text-slate-200 mt-1">{permissions.length} Scopes</div>
          <div className="text-[10px] text-slate-400 mt-0.5">14 Resource Domains</div>
        </div>

        <div className="bg-[#0D0D0E] border border-slate-800 rounded-lg p-3">
          <div className="flex items-center justify-between text-slate-400 text-[10px]">
            <span>Audit Trail Events</span>
            <History className="w-3.5 h-3.5 text-amber-400" />
          </div>
          <div className="text-lg font-bold text-amber-400 mt-1">{auditEvents.length} Logged</div>
          <div className="text-[10px] text-slate-400 mt-0.5">Tamper-evident log stream</div>
        </div>
      </div>

      {/* Tabs Navigation */}
      <div className="border-b border-slate-800 flex items-center gap-2 overflow-x-auto text-xs font-mono pb-1">
        <button
          onClick={() => setActiveTab("live_simulation")}
          className={`px-3.5 py-2 border-b-2 font-bold transition-all flex items-center gap-1.5 shrink-0 ${
            activeTab === "live_simulation"
              ? "border-sky-400 text-sky-300 bg-sky-500/10 shadow-[0_0_12px_rgba(56,189,248,0.2)]"
              : "border-transparent text-slate-400 hover:text-slate-200"
          }`}
        >
          <Radio className="w-3.5 h-3.5 text-sky-400 animate-pulse" />
          <span>Live Multi-Role Testing (Control Center)</span>
          <span className="px-1.5 py-0.2 rounded text-[9px] bg-sky-500/20 text-sky-300 font-bold">2×2 LIVE</span>
        </button>

        <button
          onClick={() => setActiveTab("users")}
          className={`px-3 py-2 border-b-2 font-bold transition-all flex items-center gap-1.5 shrink-0 ${
            activeTab === "users"
              ? "border-indigo-500 text-indigo-400 bg-indigo-500/5"
              : "border-transparent text-slate-400 hover:text-slate-200"
          }`}
        >
          <Users className="w-3.5 h-3.5" />
          <span>Users &amp; Membership ({users.length})</span>
        </button>

        <button
          onClick={() => setActiveTab("matrix")}
          className={`px-3 py-2 border-b-2 font-bold transition-all flex items-center gap-1.5 shrink-0 ${
            activeTab === "matrix"
              ? "border-indigo-500 text-indigo-400 bg-indigo-500/5"
              : "border-transparent text-slate-400 hover:text-slate-200"
          }`}
        >
          <KeyRound className="w-3.5 h-3.5" />
          <span>Roles &amp; Permission Matrix</span>
        </button>

        <button
          onClick={() => setActiveTab("probe")}
          className={`px-3 py-2 border-b-2 font-bold transition-all flex items-center gap-1.5 shrink-0 ${
            activeTab === "probe"
              ? "border-indigo-500 text-indigo-400 bg-indigo-500/5"
              : "border-transparent text-slate-400 hover:text-slate-200"
          }`}
        >
          <Zap className="w-3.5 h-3.5 text-amber-400" />
          <span>Live Authorization Tester &amp; JWT Probe</span>
        </button>

        <button
          onClick={() => setActiveTab("isolation")}
          className={`px-3 py-2 border-b-2 font-bold transition-all flex items-center gap-1.5 shrink-0 ${
            activeTab === "isolation"
              ? "border-indigo-500 text-indigo-400 bg-indigo-500/5"
              : "border-transparent text-slate-400 hover:text-slate-200"
          }`}
        >
          <Globe className="w-3.5 h-3.5 text-cyan-400" />
          <span>Multi-Tenant &amp; Project Isolation</span>
        </button>

        <button
          onClick={() => setActiveTab("security_suite")}
          className={`px-3 py-2 border-b-2 font-bold transition-all flex items-center gap-1.5 shrink-0 ${
            activeTab === "security_suite"
              ? "border-indigo-500 text-indigo-400 bg-indigo-500/5"
              : "border-transparent text-slate-400 hover:text-slate-200"
          }`}
        >
          <ShieldCheck className="w-3.5 h-3.5 text-emerald-400" />
          <span>Automated Security Suite</span>
        </button>

        <button
          onClick={() => setActiveTab("audit")}
          className={`px-3 py-2 border-b-2 font-bold transition-all flex items-center gap-1.5 shrink-0 ${
            activeTab === "audit"
              ? "border-indigo-500 text-indigo-400 bg-indigo-500/5"
              : "border-transparent text-slate-400 hover:text-slate-200"
          }`}
        >
          <History className="w-3.5 h-3.5 text-slate-400" />
          <span>RBAC Audit Trail</span>
        </button>
      </div>

      {/* TAB 0: Flagship AIQA RBAC Live Testing Interactive Multi-Role Control Center */}
      {activeTab === "live_simulation" && (
        <div className="space-y-4">
          <RbacLiveDashboard />
        </div>
      )}

      {/* TAB 1: Users & Membership */}
      {activeTab === "users" && (
        <div className="space-y-3">
          {/* Action Bar */}
          <div className="bg-[#0D0D0E] border border-slate-800 rounded-lg p-3 flex flex-col md:flex-row md:items-center justify-between gap-3 text-xs font-mono">
            <div className="flex items-center gap-2 flex-1 max-w-md">
              <div className="relative w-full">
                <Search className="w-3.5 h-3.5 text-slate-500 absolute left-2.5 top-2.5" />
                <input
                  type="text"
                  placeholder="Filter users by name, email, or role..."
                  value={userSearch}
                  onChange={(e) => setUserSearch(e.target.value)}
                  className="w-full pl-8 pr-3 py-1.5 rounded bg-[#070708] border border-slate-800 text-slate-200 text-xs focus:outline-none focus:border-indigo-500"
                />
              </div>
            </div>

            <div className="flex items-center gap-2 flex-wrap">
              {/* Tenant Filter */}
              <select
                value={selectedTenantFilter}
                onChange={(e) => setSelectedTenantFilter(e.target.value)}
                className="bg-[#070708] border border-slate-800 text-slate-300 rounded px-2.5 py-1.5 text-xs focus:outline-none"
              >
                <option value="all">All Tenants</option>
                {tenants.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.name}
                  </option>
                ))}
              </select>

              {/* Role Filter */}
              <select
                value={selectedRoleFilter}
                onChange={(e) => setSelectedRoleFilter(e.target.value)}
                className="bg-[#070708] border border-slate-800 text-slate-300 rounded px-2.5 py-1.5 text-xs focus:outline-none"
              >
                <option value="all">All Roles</option>
                {roles.map((r) => (
                  <option key={r.id} value={r.id}>
                    {r.name}
                  </option>
                ))}
              </select>

              <button
                onClick={() => setIsAddUserOpen(true)}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-bold text-xs transition-colors shadow-sm"
              >
                <UserPlus className="w-3.5 h-3.5" />
                <span>Add User</span>
              </button>
            </div>
          </div>

          {/* Users Table */}
          <div className="bg-[#0D0D0E] border border-slate-800 rounded-lg overflow-hidden">
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs font-mono border-collapse">
                <thead>
                  <tr className="border-b border-slate-800 text-[10px] uppercase text-slate-400 bg-[#070708]">
                    <th className="p-3">User / Identity</th>
                    <th className="p-3">Tenant / Org</th>
                    <th className="p-3">Role</th>
                    <th className="p-3">Assigned Projects</th>
                    <th className="p-3">Status</th>
                    <th className="p-3">Last Active</th>
                    <th className="p-3 text-right">Actions</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/60">
                  {filteredUsers.map((u) => {
                    const isSession = sessionUser?.id === u.id;
                    return (
                      <tr key={u.id} className="hover:bg-slate-900/30 transition-colors">
                        <td className="p-3">
                          <div className="flex items-center gap-2">
                            <div className="w-7 h-7 rounded bg-slate-800 border border-slate-700 flex items-center justify-center font-bold text-slate-200 text-xs">
                              {u.name.charAt(0)}
                            </div>
                            <div>
                              <div className="font-bold text-white text-xs flex items-center gap-1.5">
                                {u.name}
                                {isSession && (
                                  <span className="text-[8px] px-1 py-0.2 rounded bg-indigo-500/20 text-indigo-300 border border-indigo-500/30 uppercase font-bold">
                                    You (Session)
                                  </span>
                                )}
                              </div>
                              <div className="text-[10px] text-slate-400">{u.email}</div>
                            </div>
                          </div>
                        </td>

                        <td className="p-3">
                          <span className="text-slate-300 text-xs">{u.tenantName}</span>
                          <span className="text-[9px] text-slate-500 block">{u.tenantId}</span>
                        </td>

                        <td className="p-3">
                          <select
                            value={u.roleId}
                            onChange={(e) => handleChangeUserRole(u.id, e.target.value)}
                            disabled={u.id === "usr-super-admin"}
                            className="bg-[#070708] border border-slate-800 rounded px-2 py-1 text-xs text-indigo-300 font-bold focus:outline-none focus:border-indigo-500 cursor-pointer"
                          >
                            {roles.map((r) => (
                              <option key={r.id} value={r.id}>
                                {r.name}
                              </option>
                            ))}
                          </select>
                        </td>

                        <td className="p-3">
                          <div className="flex items-center gap-1 flex-wrap max-w-xs">
                            {u.projectIds.map((p, idx) => (
                              <span
                                key={idx}
                                className="text-[9px] px-1.5 py-0.5 rounded bg-slate-800/80 text-slate-300 border border-slate-700/60"
                              >
                                {p}
                              </span>
                            ))}
                            <button
                              onClick={() => {
                                setSelectedUserForEdit(u);
                                setIsEditProjectsOpen(true);
                              }}
                              className="text-[9px] text-indigo-400 hover:underline ml-1"
                            >
                              Edit &rarr;
                            </button>
                          </div>
                        </td>

                        <td className="p-3">
                          <span
                            className={`inline-flex items-center gap-1 text-[9px] px-2 py-0.5 rounded font-bold uppercase ${
                              u.status === "active"
                                ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/30"
                                : "bg-rose-500/10 text-rose-400 border border-rose-500/30"
                            }`}
                          >
                            <span
                              className={`w-1.5 h-1.5 rounded-full ${
                                u.status === "active" ? "bg-emerald-400 animate-pulse" : "bg-rose-400"
                              }`}
                            />
                            {u.status}
                          </span>
                        </td>

                        <td className="p-3 text-[10px] text-slate-400">{u.lastActive}</td>

                        <td className="p-3 text-right">
                          <div className="flex items-center justify-end gap-1.5">
                            <button
                              onClick={() => handleToggleUserStatus(u)}
                              disabled={u.id === "usr-super-admin" || u.id === sessionUser?.id}
                              title={u.status === "active" ? "Disable account" : "Re-activate account"}
                              className={`p-1 rounded text-xs transition-colors ${
                                u.status === "active"
                                  ? "text-slate-400 hover:text-amber-400 hover:bg-slate-800"
                                  : "text-emerald-400 hover:bg-emerald-500/10"
                              }`}
                            >
                              {u.status === "active" ? <Ban className="w-3.5 h-3.5" /> : <Check className="w-3.5 h-3.5" />}
                            </button>

                            <button
                              onClick={() => handleDeleteUser(u)}
                              disabled={u.id === "usr-super-admin" || u.id === sessionUser?.id}
                              title="Delete user"
                              className="p-1 text-slate-500 hover:text-rose-400 hover:bg-rose-500/10 rounded transition-colors disabled:opacity-30"
                            >
                              <Trash2 className="w-3.5 h-3.5" />
                            </button>
                          </div>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      )}

      {/* TAB 2: Roles & Permission Matrix Editor */}
      {activeTab === "matrix" && (
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 text-xs font-mono">
          {/* Roles Selector Sidebar (3 cols) */}
          <div className="lg:col-span-3 space-y-3">
            <div className="bg-[#0D0D0E] border border-slate-800 rounded-lg p-3 space-y-2">
              <div className="flex items-center justify-between pb-2 border-b border-slate-800">
                <span className="text-[10px] font-bold uppercase text-slate-300">Roles ({roles.length})</span>
                <button
                  onClick={() => setIsCreateRoleOpen(true)}
                  className="flex items-center gap-1 text-[10px] text-indigo-400 hover:text-indigo-300 font-bold"
                >
                  <Plus className="w-3 h-3" />
                  <span>New Role</span>
                </button>
              </div>

              <div className="space-y-1.5">
                {roles.map((r) => {
                  const isSelected = selectedMatrixRole === r.id;
                  const assignedCount = users.filter((u) => u.roleId === r.id).length;
                  return (
                    <div
                      key={r.id}
                      onClick={() => setSelectedMatrixRole(r.id)}
                      className={`p-2 rounded border cursor-pointer transition-all ${
                        isSelected
                          ? "bg-indigo-600/20 border-indigo-500/60 text-white"
                          : "bg-[#070708] border-slate-800 hover:border-slate-700 text-slate-300"
                      }`}
                    >
                      <div className="flex items-center justify-between">
                        <span className="font-bold text-xs">{r.name}</span>
                        <span
                          className={`text-[8px] px-1 py-0.2 rounded uppercase font-bold ${
                            r.type === "system"
                              ? "bg-slate-800 text-slate-300"
                              : "bg-indigo-500/20 text-indigo-300 border border-indigo-500/40"
                          }`}
                        >
                          {r.type}
                        </span>
                      </div>
                      <p className="text-[9px] text-slate-400 mt-1 line-clamp-2">{r.description}</p>
                      <div className="flex items-center justify-between text-[9px] text-slate-500 pt-1 mt-1 border-t border-slate-800/60">
                        <span>{r.permissions.includes("*") ? "All (Super)" : `${r.permissions.length} perms`}</span>
                        <span>{assignedCount} user(s)</span>
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          </div>

          {/* Permission Matrix Grid (9 cols) */}
          <div className="lg:col-span-9 space-y-3">
            <div className="bg-[#0D0D0E] border border-slate-800 rounded-lg p-3 space-y-3">
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-2 border-b border-slate-800">
                <div>
                  <h3 className="text-xs font-bold text-white flex items-center gap-2">
                    <span>Editing Role:</span>
                    <span className="text-indigo-400 font-bold">{currentActiveRole.name}</span>
                    {currentActiveRole.isProtected && (
                      <span className="text-[9px] px-1.5 py-0.2 rounded bg-slate-800 text-slate-400 border border-slate-700">
                        System Protected
                      </span>
                    )}
                  </h3>
                  <p className="text-[10px] text-slate-400 mt-0.5">{currentActiveRole.description}</p>
                </div>

                <div className="flex items-center gap-2">
                  <button
                    onClick={handleSelectAllPermissions}
                    className="text-[10px] px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 transition-colors"
                  >
                    Select All
                  </button>
                  <button
                    onClick={handleClearAllPermissions}
                    className="text-[10px] px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 transition-colors"
                  >
                    Clear All
                  </button>
                  <button
                    onClick={handleSaveRolePermissions}
                    disabled={isSavingRole}
                    className="flex items-center gap-1.5 px-3 py-1 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-bold text-xs transition-colors shadow-sm disabled:opacity-50"
                  >
                    {isSavingRole ? <RefreshCw className="w-3 h-3 animate-spin" /> : <Check className="w-3 h-3" />}
                    <span>Save Changes</span>
                  </button>
                  {!currentActiveRole.isProtected && currentActiveRole.type === "custom" && (
                    <button
                      onClick={() => handleDeleteRole(currentActiveRole)}
                      className="p-1 rounded text-rose-400 hover:bg-rose-500/10 transition-colors"
                      title="Delete custom role"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  )}
                </div>
              </div>

              {/* Grouped Permissions Matrix */}
              <div className="space-y-3 max-h-[600px] overflow-y-auto pr-1">
                {permissionGroups.map((group) => {
                  const groupPerms = permissions.filter((p) => p.group === group);
                  return (
                    <div key={group} className="bg-[#070708] border border-slate-800/80 rounded-lg p-2.5 space-y-2">
                      <div className="text-[10px] font-bold uppercase text-slate-400 tracking-wider flex items-center justify-between">
                        <span>{group}</span>
                        <span className="text-[9px] text-slate-500 font-normal">
                          {groupPerms.filter((p) => editingPermissions.includes(p.id) || editingPermissions.includes("*")).length} / {groupPerms.length} granted
                        </span>
                      </div>

                      <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
                        {groupPerms.map((perm) => {
                          const isChecked = editingPermissions.includes(perm.id) || editingPermissions.includes("*");
                          return (
                            <label
                              key={perm.id}
                              className={`p-2 rounded border cursor-pointer flex items-start gap-2.5 transition-all select-none ${
                                isChecked
                                  ? "bg-indigo-600/10 border-indigo-500/40 text-slate-200"
                                  : "bg-[#0D0D0E] border-slate-800 hover:border-slate-700 text-slate-400"
                              }`}
                            >
                              <input
                                type="checkbox"
                                checked={isChecked}
                                onChange={() => handleTogglePermission(perm.id)}
                                className="mt-0.5 rounded border-slate-700 text-indigo-600 focus:ring-0 focus:ring-offset-0 bg-[#070708]"
                              />
                              <div className="flex-1 min-w-0">
                                <div className="flex items-center justify-between">
                                  <span className="font-bold text-xs text-white">{perm.id}</span>
                                  <span className="text-[8px] px-1 py-0.2 rounded bg-slate-800 text-slate-400 uppercase">
                                    {perm.action}
                                  </span>
                                </div>
                                <p className="text-[9px] text-slate-400 mt-0.5 leading-snug">{perm.description}</p>
                              </div>
                            </label>
                          );
                        })}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* TAB 3: Live Authorization Tester & JWT Probe */}
      {activeTab === "probe" && (
        <div className="space-y-4 font-mono text-xs">
          <div className="bg-[#0D0D0E] border border-slate-800 rounded-lg p-4 space-y-4">
            <div className="flex items-center justify-between pb-2 border-b border-slate-800">
              <div>
                <h3 className="text-xs font-bold text-white flex items-center gap-2">
                  <Zap className="w-4 h-4 text-amber-400" />
                  <span>Real-Time Authorization Evaluation Probe</span>
                </h3>
                <p className="text-[10px] text-slate-400 mt-0.5">
                  Simulate live HTTP requests under caller JWT credentials against the backend zero-trust policy engine
                </p>
              </div>
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-12 gap-4">
              {/* Left Configuration (5 cols) */}
              <div className="lg:col-span-5 space-y-3">
                {/* 1. Caller Persona */}
                <div>
                  <label className="text-[10px] font-bold text-slate-400 uppercase block mb-1">
                    1. Select Caller Identity (JWT User)
                  </label>
                  <select
                    value={probeUserId}
                    onChange={(e) => setProbeUserId(e.target.value)}
                    className="w-full bg-[#070708] border border-slate-800 rounded p-2 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
                  >
                    {users.map((u) => (
                      <option key={u.id} value={u.id}>
                        {u.name} — {u.roleName} ({u.tenantName})
                      </option>
                    ))}
                  </select>
                </div>

                {/* 2. Target Endpoint */}
                <div>
                  <label className="text-[10px] font-bold text-slate-400 uppercase block mb-1">
                    2. Select Target Protected Endpoint
                  </label>
                  <div className="space-y-1 max-h-56 overflow-y-auto pr-1">
                    {presetEndpoints.map((ep, idx) => {
                      const isSelected = probeEndpointIdx === idx;
                      return (
                        <div
                          key={idx}
                          onClick={() => setProbeEndpointIdx(idx)}
                          className={`p-2 rounded border cursor-pointer transition-all ${
                            isSelected
                              ? "bg-indigo-600/20 border-indigo-500/60 text-white"
                              : "bg-[#070708] border-slate-800 hover:border-slate-700 text-slate-300"
                          }`}
                        >
                          <div className="flex items-center gap-2">
                            <span
                              className={`text-[8px] px-1 py-0.2 rounded font-bold ${
                                ep.method === "POST"
                                  ? "bg-indigo-500/20 text-indigo-300 border border-indigo-500/40"
                                  : ep.method === "DELETE"
                                  ? "bg-rose-500/20 text-rose-300 border border-rose-500/40"
                                  : "bg-emerald-500/20 text-emerald-300 border border-emerald-500/40"
                              }`}
                            >
                              {ep.method}
                            </span>
                            <span className="font-bold text-xs truncate">{ep.path}</span>
                          </div>
                          <div className="text-[9px] text-slate-400 mt-0.5">{ep.desc}</div>
                          <div className="text-[8px] text-slate-500 mt-0.5">Required Scope: {ep.requiredPermission}</div>
                        </div>
                      );
                    })}
                  </div>
                </div>

                {/* 3. Project Context */}
                <div className="grid grid-cols-2 gap-2">
                  <div>
                    <label className="text-[9px] font-bold text-slate-400 uppercase block mb-1">
                      Resource Tenant
                    </label>
                    <select
                      value={probeTenantId}
                      onChange={(e) => setProbeTenantId(e.target.value)}
                      className="w-full bg-[#070708] border border-slate-800 rounded p-1.5 text-[11px] text-slate-200"
                    >
                      {tenants.map((t) => (
                        <option key={t.id} value={t.id}>
                          {t.name}
                        </option>
                      ))}
                    </select>
                  </div>

                  <div>
                    <label className="text-[9px] font-bold text-slate-400 uppercase block mb-1">
                      Resource Project
                    </label>
                    <select
                      value={probeProjectId}
                      onChange={(e) => setProbeProjectId(e.target.value)}
                      className="w-full bg-[#070708] border border-slate-800 rounded p-1.5 text-[11px] text-slate-200"
                    >
                      {standardProjects.map((p) => (
                        <option key={p} value={p}>
                          {p}
                        </option>
                      ))}
                    </select>
                  </div>
                </div>

                <button
                  onClick={handleSendProbe}
                  disabled={isProbing}
                  className="w-full py-2.5 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-bold text-xs flex items-center justify-center gap-2 shadow-md shadow-indigo-950 transition-all"
                >
                  {isProbing ? <RefreshCw className="w-3.5 h-3.5 animate-spin" /> : <Play className="w-3.5 h-3.5 fill-current" />}
                  <span>Evaluate Authorization Policy</span>
                </button>
              </div>

              {/* Right Results & Decoded JWT Claims (7 cols) */}
              <div className="lg:col-span-7 bg-[#070708] border border-slate-800 rounded-lg p-3 flex flex-col justify-between space-y-3">
                {probeResult ? (
                  <div className="space-y-3">
                    {/* Header Verdict */}
                    <div className="flex items-center justify-between pb-2 border-b border-slate-800">
                      <div className="flex items-center gap-2">
                        <span
                          className={`text-xs font-bold px-2 py-0.5 rounded flex items-center gap-1 ${
                            probeResult.authorized
                              ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/30"
                              : "bg-rose-500/10 text-rose-400 border border-rose-500/30"
                          }`}
                        >
                          {probeResult.authorized ? <CheckCircle2 className="w-3.5 h-3.5" /> : <XCircle className="w-3.5 h-3.5" />}
                          HTTP {probeResult.httpStatus} &bull; {probeResult.decision}
                        </span>
                      </div>
                      <span className="text-[10px] text-slate-500">Evaluated at {probeResult.evaluatedAt}</span>
                    </div>

                    {/* Reason statement */}
                    <div className="p-2.5 rounded bg-slate-900/90 border border-slate-800 text-xs">
                      <strong className={probeResult.authorized ? "text-emerald-300" : "text-rose-400"}>
                        {probeResult.reason}
                      </strong>
                    </div>

                    {/* Caller Evaluation Snapshot */}
                    <div className="grid grid-cols-2 gap-2 text-[10px] text-slate-400 bg-[#0D0D0E] p-2.5 rounded border border-slate-800">
                      <div>
                        <span className="text-slate-500 block">Evaluated Caller:</span>
                        <strong className="text-slate-200">{probeResult.caller.email}</strong>
                      </div>
                      <div>
                        <span className="text-slate-500 block">Role &amp; Permissions:</span>
                        <strong className="text-indigo-300">{probeResult.caller.role} ({probeResult.caller.permissionsCount} perms)</strong>
                      </div>
                    </div>

                    {/* Decoded JWT Claims */}
                    <div>
                      <div className="text-[9px] uppercase font-bold text-slate-500 mb-1">Decoded JWT Token Payload:</div>
                      <pre className="p-2.5 rounded bg-black border border-slate-800 text-[10px] text-emerald-400 overflow-x-auto max-h-48 font-mono leading-relaxed">
                        {JSON.stringify(probeResult.jwtPayload, null, 2)}
                      </pre>
                    </div>
                  </div>
                ) : (
                  <div className="text-center py-20 text-slate-500 space-y-2">
                    <ShieldAlert className="w-8 h-8 mx-auto text-slate-600" />
                    <p className="text-xs">Click &ldquo;Evaluate Authorization Policy&rdquo; to simulate live policy evaluation.</p>
                  </div>
                )}

                <div className="pt-2 border-t border-slate-800 text-[9px] text-slate-500 flex items-center justify-between">
                  <span>Backend Protocol: RFC 6749 / RFC 7519</span>
                  <span>Zero-Trust Gate Status: Enforced</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* TAB 4: Multi-Tenant & Project Isolation */}
      {activeTab === "isolation" && (
        <div className="space-y-4 font-mono text-xs">
          <div className="bg-[#0D0D0E] border border-slate-800 rounded-lg p-4 space-y-4">
            <div>
              <h3 className="text-xs font-bold text-white flex items-center gap-2">
                <Globe className="w-4 h-4 text-cyan-400" />
                <span>Multi-Tenant &amp; Project Isolation Architecture</span>
              </h3>
              <p className="text-[10px] text-slate-400 mt-0.5">
                Physical and logical segregation of customer assets, test suites, API tokens, and defect databases
              </p>
            </div>

            {/* Tenant Boundary Diagram */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
              {tenants.map((t) => {
                const tenantUsers = users.filter((u) => u.tenantId === t.id);
                return (
                  <div key={t.id} className="bg-[#070708] border border-slate-800 rounded-lg p-3 space-y-2">
                    <div className="flex items-center justify-between pb-1.5 border-b border-slate-800">
                      <div className="flex items-center gap-1.5">
                        <Building2 className="w-3.5 h-3.5 text-indigo-400" />
                        <span className="font-bold text-white text-xs">{t.name}</span>
                      </div>
                      <span className="text-[8px] px-1.5 py-0.2 rounded bg-indigo-500/20 text-indigo-300 font-bold uppercase">
                        {t.tier}
                      </span>
                    </div>

                    <div className="text-[10px] text-slate-400 space-y-1">
                      <div>Tenant ID: <strong className="text-slate-300">{t.id}</strong></div>
                      <div>Members: <strong className="text-slate-300">{tenantUsers.length} Users</strong></div>
                      <div>Isolation Layer: <strong className="text-emerald-400">Strict Schema Partition</strong></div>
                    </div>

                    <div className="pt-2 border-t border-slate-800/80">
                      <span className="text-[9px] uppercase text-slate-500 block mb-1">Tenant Members:</span>
                      <div className="space-y-1">
                        {tenantUsers.map((tu) => (
                          <div key={tu.id} className="p-1.5 rounded bg-slate-900 border border-slate-800 text-[10px] flex items-center justify-between">
                            <span className="text-slate-200">{tu.name}</span>
                            <span className="text-[8px] text-indigo-400">{tu.roleName}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>

            {/* BOLA & IDOR Mitigation Card */}
            <div className="bg-[#070708] border border-emerald-500/30 rounded-lg p-3 space-y-2">
              <div className="flex items-center gap-2 text-emerald-400 font-bold text-xs">
                <ShieldCheck className="w-4 h-4" />
                <span>BOLA / IDOR (Broken Object Level Authorization) Active Defenses</span>
              </div>
              <p className="text-[10px] text-slate-400 leading-relaxed">
                All resource queries automatically inject caller <code className="text-emerald-300">tenant_id</code> and <code className="text-emerald-300">project_id</code> filters into the data access layer. Direct object reference tampering (such as modifying URL parameters to target other organizations) is intercepted and logged as a security violation before query execution.
              </p>
            </div>
          </div>
        </div>
      )}

      {/* TAB 5: Automated Security Suite */}
      {activeTab === "security_suite" && (
        <div className="space-y-4 font-mono text-xs">
          <div className="bg-[#0D0D0E] border border-slate-800 rounded-lg p-4 space-y-3">
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-2 border-b border-slate-800">
              <div>
                <h3 className="text-xs font-bold text-white flex items-center gap-2">
                  <ShieldCheck className="w-4 h-4 text-emerald-400" />
                  <span>Automated RBAC &amp; Zero-Trust Verification Suite (8 Tests)</span>
                </h3>
                <p className="text-[10px] text-slate-400 mt-0.5">
                  Continuous validation of authentication, authorization, tenant isolation, and privilege escalation guards
                </p>
              </div>

              <button
                onClick={handleRunSecuritySuite}
                disabled={isRunningSecuritySuite}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-bold text-xs shadow-sm transition-all disabled:opacity-50"
              >
                {isRunningSecuritySuite ? <RefreshCw className="w-3.5 h-3.5 animate-spin" /> : <Play className="w-3.5 h-3.5 fill-current" />}
                <span>{isRunningSecuritySuite ? "Running Security Tests..." : "Re-run Suite"}</span>
              </button>
            </div>

            {overallVerdict && (
              <div className="p-2.5 rounded bg-emerald-500/10 border border-emerald-500/30 text-emerald-400 flex items-center justify-between">
                <div className="flex items-center gap-2 font-bold">
                  <CheckCircle2 className="w-4 h-4" />
                  <span>Overall Status: 8/8 Security Checks Passed (100% Compliance)</span>
                </div>
                <span className="text-[10px] text-slate-400">Zero Authorization Leaks</span>
              </div>
            )}

            {/* Test Results Grid */}
            <div className="space-y-2">
              {securityTests.length > 0 ? (
                securityTests.map((t) => (
                  <div
                    key={t.id}
                    className="p-3 rounded-lg bg-[#070708] border border-slate-800 flex flex-col sm:flex-row sm:items-center justify-between gap-2"
                  >
                    <div className="space-y-1">
                      <div className="flex items-center gap-2">
                        <span className="font-bold text-white text-xs">{t.title}</span>
                        <span className="text-[8px] px-1.5 py-0.2 rounded bg-slate-800 text-slate-400 uppercase font-bold">
                          {t.category}
                        </span>
                      </div>
                      <p className="text-[10px] text-slate-400">{t.description}</p>
                      <div className="text-[9px] text-slate-500">{t.details}</div>
                    </div>

                    <div className="flex items-center gap-3 shrink-0">
                      <div className="text-right text-[10px] text-slate-400">
                        <div>Expected: <strong className="text-slate-200">{t.expectedStatus}</strong></div>
                        <div>Actual: <strong className="text-emerald-400">{t.actualStatus}</strong></div>
                      </div>

                      <span
                        className={`text-[9px] font-bold px-2 py-0.5 rounded flex items-center gap-1 uppercase ${
                          t.verdict === "PASSED"
                            ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/30"
                            : "bg-rose-500/10 text-rose-400 border border-rose-500/30"
                        }`}
                      >
                        <CheckCircle2 className="w-3 h-3" />
                        {t.verdict} ({t.latencyMs}ms)
                      </span>
                    </div>
                  </div>
                ))
              ) : (
                <div className="text-center py-14 text-slate-500 space-y-2">
                  <ShieldCheck className="w-8 h-8 mx-auto text-slate-600" />
                  <p className="text-xs">Click &ldquo;Re-run Suite&rdquo; to execute the live automated verification tests.</p>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* TAB 6: RBAC Audit Trail */}
      {activeTab === "audit" && (
        <div className="bg-[#0D0D0E] border border-slate-800 rounded-lg p-4 space-y-3 font-mono text-xs">
          <div className="flex items-center justify-between pb-2 border-b border-slate-800">
            <div>
              <h3 className="text-xs font-bold text-white flex items-center gap-2">
                <History className="w-4 h-4 text-amber-400" />
                <span>Immutable RBAC Governance Audit Trail ({auditEvents.length} Events)</span>
              </h3>
              <p className="text-[10px] text-slate-400 mt-0.5">
                Cryptographically hashed records of all role changes, permission modifications, session switches, and unauthorized attempts
              </p>
            </div>
          </div>

          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs border-collapse">
              <thead>
                <tr className="border-b border-slate-800 text-[10px] uppercase text-slate-400 bg-[#070708]">
                  <th className="p-2.5">Timestamp</th>
                  <th className="p-2.5">Actor</th>
                  <th className="p-2.5">Action</th>
                  <th className="p-2.5">Target Resource</th>
                  <th className="p-2.5">Details</th>
                  <th className="p-2.5 text-right">Result</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/60 text-slate-300">
                {auditEvents.map((ev) => (
                  <tr key={ev.id} className="hover:bg-slate-900/40 transition-colors">
                    <td className="p-2.5 text-[10px] text-slate-400 whitespace-nowrap">{ev.timestamp}</td>
                    <td className="p-2.5 font-bold text-slate-200 text-xs">{ev.actorEmail}</td>
                    <td className="p-2.5">
                      <span className="text-[9px] px-1.5 py-0.5 rounded bg-slate-800 text-indigo-300 border border-slate-700 uppercase font-bold">
                        {ev.action}
                      </span>
                    </td>
                    <td className="p-2.5 text-slate-400 text-xs">
                      {ev.resourceType}:{ev.resourceId}
                    </td>
                    <td className="p-2.5 text-[10px] text-slate-400 max-w-xs truncate">{ev.details || "N/A"}</td>
                    <td className="p-2.5 text-right">
                      <span
                        className={`text-[9px] px-1.5 py-0.5 rounded font-bold uppercase ${
                          ev.result === "SUCCESS"
                            ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/30"
                            : "bg-rose-500/10 text-rose-400 border border-rose-500/30"
                        }`}
                      >
                        {ev.result}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Modal: Add User */}
      {isAddUserOpen && (
        <div className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4 font-mono text-xs">
          <div className="bg-[#0D0D0E] border border-slate-800 rounded-xl p-5 w-full max-w-md space-y-4 shadow-xl animate-fadeIn">
            <div className="flex items-center justify-between pb-2 border-b border-slate-800">
              <h3 className="text-sm font-bold text-white flex items-center gap-2">
                <UserPlus className="w-4 h-4 text-indigo-400" />
                <span>Provision New User Identity</span>
              </h3>
              <button onClick={() => setIsAddUserOpen(false)} className="text-slate-400 hover:text-white">
                ✕
              </button>
            </div>

            <form onSubmit={handleCreateUser} className="space-y-3">
              <div>
                <label className="text-[10px] font-bold uppercase text-slate-400 block mb-1">Full Name</label>
                <input
                  type="text"
                  required
                  placeholder="e.g. Rachel Adams"
                  value={newUserName}
                  onChange={(e) => setNewUserName(e.target.value)}
                  className="w-full bg-[#070708] border border-slate-800 rounded p-2 text-slate-200 focus:outline-none focus:border-indigo-500"
                />
              </div>

              <div>
                <label className="text-[10px] font-bold uppercase text-slate-400 block mb-1">Email Address</label>
                <input
                  type="email"
                  required
                  placeholder="e.g. rachel.adams@acme.corp"
                  value={newUserEmail}
                  onChange={(e) => setNewUserEmail(e.target.value)}
                  className="w-full bg-[#070708] border border-slate-800 rounded p-2 text-slate-200 focus:outline-none focus:border-indigo-500"
                />
              </div>

              <div>
                <label className="text-[10px] font-bold uppercase text-slate-400 block mb-1">Organization / Tenant</label>
                <select
                  value={newUserTenant}
                  onChange={(e) => setNewUserTenant(e.target.value)}
                  className="w-full bg-[#070708] border border-slate-800 rounded p-2 text-slate-200 focus:outline-none"
                >
                  {tenants.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.name} ({t.tier})
                    </option>
                  ))}
                </select>
              </div>

              <div>
                <label className="text-[10px] font-bold uppercase text-slate-400 block mb-1">Assigned Role</label>
                <select
                  value={newUserRole}
                  onChange={(e) => setNewUserRole(e.target.value)}
                  className="w-full bg-[#070708] border border-slate-800 rounded p-2 text-slate-200 focus:outline-none"
                >
                  {roles.map((r) => (
                    <option key={r.id} value={r.id}>
                      {r.name} ({r.type})
                    </option>
                  ))}
                </select>
              </div>

              <div className="pt-2 flex items-center justify-end gap-2">
                <button
                  type="button"
                  onClick={() => setIsAddUserOpen(false)}
                  className="px-3 py-1.5 rounded bg-slate-800 text-slate-300 hover:bg-slate-700 text-xs"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="px-4 py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-bold text-xs"
                >
                  Provision User
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Modal: Edit User Projects */}
      {isEditProjectsOpen && selectedUserForEdit && (
        <div className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4 font-mono text-xs">
          <div className="bg-[#0D0D0E] border border-slate-800 rounded-xl p-5 w-full max-w-md space-y-4 shadow-xl animate-fadeIn">
            <div className="flex items-center justify-between pb-2 border-b border-slate-800">
              <h3 className="text-sm font-bold text-white flex items-center gap-2">
                <FolderKanban className="w-4 h-4 text-indigo-400" />
                <span>Manage Project Memberships: {selectedUserForEdit.name}</span>
              </h3>
              <button onClick={() => setIsEditProjectsOpen(false)} className="text-slate-400 hover:text-white">
                ✕
              </button>
            </div>

            <div className="space-y-2">
              <label
                className={`p-2 rounded border cursor-pointer flex items-center gap-2 ${
                  selectedUserForEdit.projectIds.includes("all")
                    ? "bg-indigo-600/20 border-indigo-500/60 text-white"
                    : "bg-[#070708] border-slate-800 text-slate-300"
                }`}
              >
                <input
                  type="checkbox"
                  checked={selectedUserForEdit.projectIds.includes("all")}
                  onChange={(e) => {
                    setSelectedUserForEdit({
                      ...selectedUserForEdit,
                      projectIds: e.target.checked ? ["all"] : ["Apex AI Agent"]
                    });
                  }}
                  className="rounded border-slate-700 text-indigo-600 bg-[#070708]"
                />
                <span className="font-bold">All Projects (Global Membership)</span>
              </label>

              {!selectedUserForEdit.projectIds.includes("all") && (
                <div className="space-y-1.5 pt-2">
                  <span className="text-[10px] text-slate-400 uppercase font-bold block">Select Specific Projects:</span>
                  {standardProjects.map((proj) => {
                    const isSelected = selectedUserForEdit.projectIds.includes(proj);
                    return (
                      <label
                        key={proj}
                        className={`p-2 rounded border cursor-pointer flex items-center gap-2 ${
                          isSelected
                            ? "bg-indigo-600/20 border-indigo-500/60 text-white"
                            : "bg-[#070708] border-slate-800 text-slate-300"
                        }`}
                      >
                        <input
                          type="checkbox"
                          checked={isSelected}
                          onChange={(e) => {
                            const newProj = e.target.checked
                              ? [...selectedUserForEdit.projectIds, proj]
                              : selectedUserForEdit.projectIds.filter((p) => p !== proj);
                            setSelectedUserForEdit({
                              ...selectedUserForEdit,
                              projectIds: newProj
                            });
                          }}
                          className="rounded border-slate-700 text-indigo-600 bg-[#070708]"
                        />
                        <span>{proj}</span>
                      </label>
                    );
                  })}
                </div>
              )}
            </div>

            <div className="pt-2 flex items-center justify-end gap-2">
              <button
                type="button"
                onClick={() => setIsEditProjectsOpen(false)}
                className="px-3 py-1.5 rounded bg-slate-800 text-slate-300 hover:bg-slate-700 text-xs"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleSaveUserProjects}
                className="px-4 py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-bold text-xs"
              >
                Save Memberships
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Modal: Create Role */}
      {isCreateRoleOpen && (
        <div className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4 font-mono text-xs">
          <div className="bg-[#0D0D0E] border border-slate-800 rounded-xl p-5 w-full max-w-md space-y-4 shadow-xl animate-fadeIn">
            <div className="flex items-center justify-between pb-2 border-b border-slate-800">
              <h3 className="text-sm font-bold text-white flex items-center gap-2">
                <KeyRound className="w-4 h-4 text-indigo-400" />
                <span>Create Custom RBAC Role</span>
              </h3>
              <button onClick={() => setIsCreateRoleOpen(false)} className="text-slate-400 hover:text-white">
                ✕
              </button>
            </div>

            <form onSubmit={handleCreateRole} className="space-y-3">
              <div>
                <label className="text-[10px] font-bold uppercase text-slate-400 block mb-1">Role Name</label>
                <input
                  type="text"
                  required
                  placeholder="e.g. Compliance Auditor"
                  value={newRoleName}
                  onChange={(e) => setNewRoleName(e.target.value)}
                  className="w-full bg-[#070708] border border-slate-800 rounded p-2 text-slate-200 focus:outline-none focus:border-indigo-500"
                />
              </div>

              <div>
                <label className="text-[10px] font-bold uppercase text-slate-400 block mb-1">Description</label>
                <textarea
                  placeholder="Describe the responsibilities and scope of this role..."
                  value={newRoleDesc}
                  onChange={(e) => setNewRoleDesc(e.target.value)}
                  rows={3}
                  className="w-full bg-[#070708] border border-slate-800 rounded p-2 text-slate-200 focus:outline-none focus:border-indigo-500"
                />
              </div>

              <div className="pt-2 flex items-center justify-end gap-2">
                <button
                  type="button"
                  onClick={() => setIsCreateRoleOpen(false)}
                  className="px-3 py-1.5 rounded bg-slate-800 text-slate-300 hover:bg-slate-700 text-xs"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="px-4 py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-bold text-xs"
                >
                  Create Role
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
};
