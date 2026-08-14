import React, { useState } from "react";
import { X, Check, Search, Shield, Play, Filter, KeyRound } from "lucide-react";
import { RoleConfig, RoleId } from "./types";

interface PermissionMatrixModalProps {
  isOpen: boolean;
  onClose: () => void;
  roles: RoleConfig[];
  onProbePermission: (permissionId: string, roleId: RoleId) => void;
}

interface MatrixItem {
  id: string;
  name: string;
  category: string;
  superAdmin: boolean;
  admin: boolean;
  pm: boolean;
  user: boolean;
}

const matrixData: MatrixItem[] = [
  // Users
  { id: "users.view", name: "View Users & Profiles", category: "User Management", superAdmin: true, admin: true, pm: false, user: false },
  { id: "users.create", name: "Provision New Users", category: "User Management", superAdmin: true, admin: true, pm: false, user: false },
  { id: "users.update", name: "Update User Roles / Projects", category: "User Management", superAdmin: true, admin: true, pm: false, user: false },
  { id: "users.delete", name: "Deprovision / Delete Users", category: "User Management", superAdmin: true, admin: true, pm: false, user: false },

  // Roles
  { id: "roles.view", name: "Inspect Role Definitions", category: "Access Governance", superAdmin: true, admin: true, pm: false, user: false },
  { id: "roles.create", name: "Create Custom RBAC Roles", category: "Access Governance", superAdmin: true, admin: true, pm: false, user: false },
  { id: "roles.update", name: "Modify Role Permission Matrix", category: "Access Governance", superAdmin: true, admin: true, pm: false, user: false },
  { id: "roles.delete", name: "Delete Custom Roles", category: "Access Governance", superAdmin: true, admin: true, pm: false, user: false },

  // Projects
  { id: "projects.view", name: "View Test Projects", category: "Projects", superAdmin: true, admin: true, pm: true, user: true },
  { id: "projects.create", name: "Create New Projects", category: "Projects", superAdmin: true, admin: true, pm: true, user: false },
  { id: "projects.update", name: "Configure Project Targets", category: "Projects", superAdmin: true, admin: true, pm: true, user: false },
  { id: "projects.delete", name: "Archive / Delete Projects", category: "Projects", superAdmin: true, admin: true, pm: false, user: false },

  // Tests
  { id: "tests.view", name: "Read Test Suites & Specs", category: "Test Operations", superAdmin: true, admin: true, pm: true, user: true },
  { id: "tests.create", name: "Author New Test Cases", category: "Test Operations", superAdmin: true, admin: true, pm: true, user: true },
  { id: "tests.update", name: "Edit Assertions & Steps", category: "Test Operations", superAdmin: true, admin: true, pm: true, user: true },
  { id: "tests.execute", name: "Trigger Automated Test Execution", category: "Test Operations", superAdmin: true, admin: true, pm: true, user: true },
  { id: "tests.delete", name: "Purge Test Suites", category: "Test Operations", superAdmin: true, admin: true, pm: true, user: false },

  // Test Runs
  { id: "test_runs.view", name: "Monitor Realtime Test Stream", category: "Test Operations", superAdmin: true, admin: true, pm: true, user: true },
  { id: "test_runs.cancel", name: "Abort / Cancel Active Worker", category: "Test Operations", superAdmin: true, admin: true, pm: true, user: false },

  // Evidence
  { id: "evidence.view", name: "Inspect Network HAR & DB Logs", category: "Evidence & Diagnostics", superAdmin: true, admin: true, pm: true, user: true },
  { id: "evidence.create", name: "Attach Diagnostics Artifacts", category: "Evidence & Diagnostics", superAdmin: true, admin: true, pm: true, user: false },
  { id: "evidence.delete", name: "Purge Forensic Artifacts", category: "Evidence & Diagnostics", superAdmin: true, admin: true, pm: false, user: false },

  // Bugs
  { id: "bugs.view", name: "View Defect Candidates", category: "Defect Management", superAdmin: true, admin: true, pm: true, user: true },
  { id: "bugs.create", name: "Approve & Sync AI Bugs", category: "Defect Management", superAdmin: true, admin: true, pm: true, user: true },
  { id: "bugs.update", name: "Update Triage & Severity", category: "Defect Management", superAdmin: true, admin: true, pm: true, user: true },
  { id: "bugs.delete", name: "Dismiss / Delete Bug Candidate", category: "Defect Management", superAdmin: true, admin: true, pm: true, user: false },

  // Policies
  { id: "policies.view", name: "View Guardrails & Quality Gates", category: "Governance & Policies", superAdmin: true, admin: true, pm: true, user: false },
  { id: "policies.create", name: "Define New Quality Guardrail", category: "Governance & Policies", superAdmin: true, admin: true, pm: false, user: false },
  { id: "policies.update", name: "Override Governance Policy", category: "Governance & Policies", superAdmin: true, admin: true, pm: false, user: false },
  { id: "policies.delete", name: "Remove Governance Guardrail", category: "Governance & Policies", superAdmin: true, admin: true, pm: false, user: false },

  // Audit Logs
  { id: "audit_logs.view", name: "Inspect Tamper-Proof Audit Trail", category: "Audit & Compliance", superAdmin: true, admin: true, pm: false, user: false },

  // AI Models
  { id: "ai_models.view", name: "View Model Router & Health", category: "AI & Model Operations", superAdmin: true, admin: true, pm: true, user: false },
  { id: "ai_models.manage", name: "Tune Fallbacks & Ollama Params", category: "AI & Model Operations", superAdmin: true, admin: true, pm: false, user: false },

  // System
  { id: "system.view", name: "View Cluster Nodes & Queues", category: "System Administration", superAdmin: true, admin: true, pm: true, user: true },
  { id: "system.manage", name: "Execute Chaos Injection / Reboots", category: "System Administration", superAdmin: true, admin: false, pm: false, user: false }
];

export const PermissionMatrixModal: React.FC<PermissionMatrixModalProps> = ({
  isOpen,
  onClose,
  roles,
  onProbePermission
}) => {
  const [searchTerm, setSearchTerm] = useState("");
  const [selectedCategory, setSelectedCategory] = useState("all");

  if (!isOpen) return null;

  const categories = ["all", ...Array.from(new Set(matrixData.map((m) => m.category)))];

  const filteredData = matrixData.filter((item) => {
    if (selectedCategory !== "all" && item.category !== selectedCategory) return false;
    if (searchTerm.trim()) {
      const q = searchTerm.toLowerCase();
      return (
        item.name.toLowerCase().includes(q) ||
        item.id.toLowerCase().includes(q) ||
        item.category.toLowerCase().includes(q)
      );
    }
    return true;
  });

  return (
    <div className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4">
      <div className="bg-zinc-950 border border-zinc-700/80 rounded-2xl w-full max-w-5xl max-h-[85vh] flex flex-col shadow-2xl overflow-hidden animate-in fade-in zoom-in-95 duration-200">
        {/* Modal Header */}
        <div className="flex items-center justify-between px-5 py-3.5 bg-zinc-900 border-b border-zinc-800">
          <div className="flex items-center gap-2.5">
            <div className="p-2 rounded-lg bg-sky-500/10 text-sky-400 border border-sky-500/20">
              <KeyRound className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-base font-bold font-mono text-zinc-100 uppercase tracking-wider">
                RBAC MULTI-ROLE PERMISSION MATRIX
              </h2>
              <p className="text-xs text-zinc-400 font-mono">
                Comparative access control schema across Super Admin, Admin, Project Manager, and User
              </p>
            </div>
          </div>

          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800 transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Filter Controls */}
        <div className="flex items-center justify-between gap-3 px-5 py-2.5 bg-zinc-900/60 border-b border-zinc-800 font-mono text-xs">
          <div className="flex items-center gap-2 flex-1 max-w-md bg-zinc-950 border border-zinc-800 rounded-lg px-2.5 py-1.5">
            <Search className="w-3.5 h-3.5 text-zinc-500" />
            <input
              type="text"
              placeholder="Search permission by name or scope (e.g. tests.execute, users.delete)..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="bg-transparent text-zinc-200 placeholder-zinc-500 text-xs w-full focus:outline-none"
            />
          </div>

          <div className="flex items-center gap-2">
            <span className="text-zinc-500 text-[11px]">CATEGORY:</span>
            <select
              value={selectedCategory}
              onChange={(e) => setSelectedCategory(e.target.value)}
              className="bg-zinc-950 border border-zinc-800 text-zinc-300 rounded-lg px-2 py-1 text-xs focus:outline-none cursor-pointer"
            >
              {categories.map((cat) => (
                <option key={cat} value={cat}>
                  {cat === "all" ? "All Categories" : cat}
                </option>
              ))}
            </select>
          </div>
        </div>

        {/* Matrix Table */}
        <div className="flex-1 overflow-y-auto font-mono text-xs">
          <table className="w-full text-left border-collapse">
            <thead className="bg-zinc-900 sticky top-0 border-b border-zinc-800 z-10">
              <tr>
                <th className="py-2.5 px-4 text-zinc-400 font-semibold text-[11px] uppercase tracking-wider">
                  Permission Scope
                </th>
                <th className="py-2.5 px-3 text-center text-purple-300 font-semibold text-[11px] uppercase tracking-wider bg-purple-950/20">
                  Super Admin
                </th>
                <th className="py-2.5 px-3 text-center text-sky-300 font-semibold text-[11px] uppercase tracking-wider bg-sky-950/20">
                  Admin
                </th>
                <th className="py-2.5 px-3 text-center text-indigo-300 font-semibold text-[11px] uppercase tracking-wider bg-indigo-950/20">
                  Project Mgr
                </th>
                <th className="py-2.5 px-3 text-center text-emerald-300 font-semibold text-[11px] uppercase tracking-wider bg-emerald-950/20">
                  QA User
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-zinc-800/80">
              {filteredData.map((item) => (
                <tr key={item.id} className="hover:bg-zinc-900/50 transition-colors">
                  {/* Permission Info */}
                  <td className="py-2 px-4">
                    <div className="font-semibold text-zinc-200">{item.name}</div>
                    <div className="text-[10px] text-zinc-500 flex items-center gap-2">
                      <span className="text-sky-400">{item.id}</span>
                      <span>•</span>
                      <span>{item.category}</span>
                    </div>
                  </td>

                  {/* Super Admin */}
                  <td className="py-2 px-3 text-center bg-purple-950/10">
                    <button
                      onClick={() => {
                        onProbePermission(item.id, "super_admin");
                        onClose();
                      }}
                      className="group inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-bold text-purple-300 bg-purple-500/10 hover:bg-purple-500/20 border border-purple-500/30 transition-all"
                      title="Test probe from Super Admin"
                    >
                      <Check className="w-3.5 h-3.5 stroke-[3] text-purple-400" />
                      <Play className="w-2.5 h-2.5 opacity-0 group-hover:opacity-100 transition-opacity" />
                    </button>
                  </td>

                  {/* Admin */}
                  <td className="py-2 px-3 text-center bg-sky-950/10">
                    <button
                      onClick={() => {
                        onProbePermission(item.id, "org_admin");
                        onClose();
                      }}
                      className={`group inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-bold border transition-all ${
                        item.admin
                          ? "text-sky-300 bg-sky-500/10 hover:bg-sky-500/20 border-sky-500/30"
                          : "text-zinc-500 bg-zinc-900 hover:bg-zinc-800 border-zinc-800 hover:border-orange-500/40 hover:text-orange-300"
                      }`}
                      title={`Test probe from Admin (${item.admin ? "Authorized" : "Forbidden 403"})`}
                    >
                      {item.admin ? (
                        <Check className="w-3.5 h-3.5 stroke-[3] text-sky-400" />
                      ) : (
                        <X className="w-3.5 h-3.5 text-zinc-500" />
                      )}
                      <Play className="w-2.5 h-2.5 opacity-0 group-hover:opacity-100 transition-opacity" />
                    </button>
                  </td>

                  {/* PM */}
                  <td className="py-2 px-3 text-center bg-indigo-950/10">
                    <button
                      onClick={() => {
                        onProbePermission(item.id, "project_admin");
                        onClose();
                      }}
                      className={`group inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-bold border transition-all ${
                        item.pm
                          ? "text-indigo-300 bg-indigo-500/10 hover:bg-indigo-500/20 border-indigo-500/30"
                          : "text-zinc-500 bg-zinc-900 hover:bg-zinc-800 border-zinc-800 hover:border-orange-500/40 hover:text-orange-300"
                      }`}
                      title={`Test probe from PM (${item.pm ? "Authorized" : "Forbidden 403"})`}
                    >
                      {item.pm ? (
                        <Check className="w-3.5 h-3.5 stroke-[3] text-indigo-400" />
                      ) : (
                        <X className="w-3.5 h-3.5 text-zinc-500" />
                      )}
                      <Play className="w-2.5 h-2.5 opacity-0 group-hover:opacity-100 transition-opacity" />
                    </button>
                  </td>

                  {/* User */}
                  <td className="py-2 px-3 text-center bg-emerald-950/10">
                    <button
                      onClick={() => {
                        onProbePermission(item.id, "qa_engineer");
                        onClose();
                      }}
                      className={`group inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-bold border transition-all ${
                        item.user
                          ? "text-emerald-300 bg-emerald-500/10 hover:bg-emerald-500/20 border-emerald-500/30"
                          : "text-zinc-500 bg-zinc-900 hover:bg-zinc-800 border-zinc-800 hover:border-orange-500/40 hover:text-orange-300"
                      }`}
                      title={`Test probe from User (${item.user ? "Authorized" : "Forbidden 403"})`}
                    >
                      {item.user ? (
                        <Check className="w-3.5 h-3.5 stroke-[3] text-emerald-400" />
                      ) : (
                        <X className="w-3.5 h-3.5 text-zinc-500" />
                      )}
                      <Play className="w-2.5 h-2.5 opacity-0 group-hover:opacity-100 transition-opacity" />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        {/* Footer */}
        <div className="px-5 py-3 bg-zinc-900 border-t border-zinc-800 flex items-center justify-between text-xs font-mono text-zinc-400">
          <span>
            Showing <strong className="text-zinc-200">{filteredData.length}</strong> of {matrixData.length} permissions
          </span>
          <span className="text-[11px] text-zinc-500">
            Tip: Click any cell checkmark/cross to trigger a live authorization test probe
          </span>
        </div>
      </div>
    </div>
  );
};
