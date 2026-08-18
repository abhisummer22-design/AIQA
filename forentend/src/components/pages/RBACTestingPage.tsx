/**
 * AIQA RBAC Testing Workspace
 * Dedicated authorization matrix validation architecture:
 * Role -> Permission -> Resource -> Action -> Expected Access -> Actual Access -> Result
 */

import React, { useState } from 'react';
import {
  ShieldCheck,
  Plus,
  Play,
  CheckCircle2,
  XCircle,
  AlertTriangle,
  Lock,
  Unlock,
  KeyRound,
  Filter,
  Layers,
  ChevronRight,
  ShieldAlert
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { SectionHeader } from '../common/SectionHeader';
import { StatusBadge } from '../common/StatusBadge';
import { EmptyState } from '../common/EmptyState';
import { RBACMatrixRule } from '../../types';

export const RBACTestingPage: React.FC = () => {
  const {
    rbacScenarios,
    activeRBACScenarioId,
    setActiveRBACScenarioId,
    addRBACRule,
    evaluateRBACMatrix,
    currentEnvironment,
  } = useAIQA();

  const [isAddRuleOpen, setIsAddRuleOpen] = useState(false);
  const [roleFilter, setRoleFilter] = useState<string>('ALL');

  // Form state for new rule
  const [newRole, setNewRole] = useState('BillingManager');
  const [newResource, setNewResource] = useState('Billing:Invoices');
  const [newAction, setNewAction] = useState<RBACMatrixRule['action']>('READ');
  const [newExpected, setNewExpected] = useState<RBACMatrixRule['expectedAccess']>('ALLOW');
  const [newCondition, setNewCondition] = useState('');

  const activeScenario =
    rbacScenarios.find((s) => s.id === activeRBACScenarioId) || rbacScenarios[0];

  const rules = activeScenario?.rules || [];

  const filteredRules = rules.filter((r) => {
    if (roleFilter === 'ALL') return true;
    return r.role === roleFilter;
  });

  const passedCount = rules.filter((r) => r.status === 'PASSED').length;
  const failedCount = rules.filter((r) => r.status === 'FAILED').length;

  const handleAddRule = (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeScenario) return;

    addRBACRule(activeScenario.id, {
      role: newRole,
      resource: newResource,
      action: newAction,
      expectedAccess: newExpected,
      conditionCriteria: newCondition || undefined,
      status: 'QUEUED',
    });

    setIsAddRuleOpen(false);
    setNewCondition('');
  };

  return (
    <div className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="RBAC Security Testing Matrix"
        subtitle="Multi-tenant privilege boundary auditing and role authorization policy enforcement"
        badge={activeScenario ? `${activeScenario.rolesUnderAudit.length} ROLES UNDER AUDIT` : 'RBAC'}
        icon={ShieldCheck}
        actions={
          <div className="flex items-center gap-2">
            <button
              onClick={() => setIsAddRuleOpen(true)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 text-xs font-medium border border-slate-800 cursor-pointer"
            >
              <Plus className="w-3.5 h-3.5" />
              <span>Define Access Rule</span>
            </button>
            <button
              onClick={() => activeScenario && evaluateRBACMatrix(activeScenario.id)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shadow-sm shadow-indigo-950/60 cursor-pointer"
            >
              <Play className="w-3.5 h-3.5 fill-current" />
              <span>Evaluate Authorization Matrix</span>
            </button>
          </div>
        }
      />

      {/* RBAC Structural Pipeline Visualizer */}
      <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-3">
        <div className="text-[10px] font-mono uppercase tracking-wider text-slate-500 mb-1.5 font-semibold">
          Authorization Validation Chain
        </div>
        <div className="flex items-center gap-1.5 overflow-x-auto text-xs font-mono py-0.5">
          {['Role', 'Permission', 'Resource', 'Action', 'Expected Access', 'Actual Access', 'Result'].map(
            (step, idx, arr) => (
              <React.Fragment key={step}>
                <div className="px-2.5 py-1 rounded bg-[#020617] border border-slate-800 text-indigo-300 shrink-0 font-semibold text-[11px]">
                  {step}
                </div>
                {idx < arr.length - 1 && (
                  <ChevronRight className="w-3.5 h-3.5 text-slate-600 shrink-0" />
                )}
              </React.Fragment>
            )
          )}
        </div>
      </div>

      {activeScenario ? (
        <div className="space-y-3">
          {/* Scenario Overview Card */}
          <div className="p-3.5 rounded-lg border border-slate-800 bg-[#0B1120] flex flex-col md:flex-row md:items-center justify-between gap-3">
            <div>
              <div className="flex items-center gap-2">
                <span className="font-mono text-xs font-bold text-indigo-400">{activeScenario.id}</span>
                <h3 className="text-sm font-bold text-slate-100">{activeScenario.name}</h3>
              </div>
              <p className="text-xs text-slate-400 mt-0.5">{activeScenario.description}</p>
              <div className="flex flex-wrap items-center gap-1.5 mt-2">
                <span className="text-[10px] font-mono text-slate-500 mr-1 uppercase">Audited Roles:</span>
                {activeScenario.rolesUnderAudit.map((r) => (
                  <span
                    key={r}
                    className="px-1.5 py-0.2 rounded bg-[#020617] border border-slate-800 text-slate-300 font-mono text-[10px]"
                  >
                    {r}
                  </span>
                ))}
              </div>
            </div>

            {/* Matrix Status Totals */}
            <div className="flex items-center gap-2 shrink-0">
              <div className="px-3 py-1.5 rounded-md bg-emerald-950/40 border border-emerald-800/60 text-center min-w-[70px]">
                <div className="text-[9px] font-mono uppercase text-emerald-400">Compliant</div>
                <div className="text-base font-bold font-mono text-emerald-300">{passedCount}</div>
              </div>
              <div className="px-3 py-1.5 rounded-md bg-rose-950/40 border border-rose-800/60 text-center min-w-[70px]">
                <div className="text-[9px] font-mono uppercase text-rose-400">Breaches</div>
                <div className="text-base font-bold font-mono text-rose-300">{failedCount}</div>
              </div>
            </div>
          </div>

          {/* Filter Bar */}
          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <Filter className="w-3 h-3 text-slate-400" />
              <span className="text-xs text-slate-400">Filter by Role:</span>
              <select
                value={roleFilter}
                onChange={(e) => setRoleFilter(e.target.value)}
                className="px-2 py-1 rounded bg-[#0B1120] border border-slate-800 text-xs text-slate-200 focus:border-indigo-500 focus:outline-none font-mono"
              >
                <option value="ALL">All Roles ({rules.length} Rules)</option>
                {activeScenario.rolesUnderAudit.map((r) => (
                  <option key={r} value={r}>
                    {r}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {/* RBAC Matrix Table */}
          <div className="overflow-x-auto rounded-lg border border-slate-800 bg-[#020617]">
            <table className="w-full text-left text-xs">
              <thead>
                <tr className="border-b border-slate-800 bg-[#0B1120] text-slate-400 font-mono text-[10px] uppercase">
                  <th className="py-2.5 px-3">Subject Role</th>
                  <th className="py-2.5 px-3">Target Resource</th>
                  <th className="py-2.5 px-3">Action</th>
                  <th className="py-2.5 px-3">Expected Access</th>
                  <th className="py-2.5 px-3">Actual Access</th>
                  <th className="py-2.5 px-3">Audit Status</th>
                  <th className="py-2.5 px-3">Policy Evidence & Log</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/60 font-sans">
                {filteredRules.map((rule) => {
                  const isViolation = rule.actualAccess && rule.actualAccess !== rule.expectedAccess;

                  return (
                    <tr
                      key={rule.id}
                      className={`hover:bg-slate-800/30 transition-colors ${
                        isViolation ? 'bg-rose-950/20' : ''
                      }`}
                    >
                      <td className="py-2.5 px-3 font-bold font-mono text-slate-200 text-xs">
                        {rule.role}
                      </td>
                      <td className="py-2.5 px-3 font-mono text-indigo-300 text-[11px]">
                        {rule.resource}
                      </td>
                      <td className="py-2.5 px-3">
                        <span className="px-1.5 py-0.2 rounded bg-[#0B1120] border border-slate-800 font-mono text-[10px] font-semibold text-slate-300">
                          {rule.action}
                        </span>
                      </td>
                      <td className="py-2.5 px-3 font-mono">
                        <span
                          className={`px-1.5 py-0.2 rounded text-[10px] font-bold ${
                            rule.expectedAccess === 'ALLOW'
                              ? 'bg-emerald-950/80 text-emerald-300 border border-emerald-800'
                              : 'bg-zinc-900 text-zinc-300 border border-zinc-700'
                          }`}
                        >
                          {rule.expectedAccess}
                        </span>
                      </td>
                      <td className="py-2.5 px-3 font-mono">
                        {rule.actualAccess ? (
                          <span
                            className={`px-1.5 py-0.2 rounded text-[10px] font-bold ${
                              rule.actualAccess === rule.expectedAccess
                                ? 'bg-emerald-950/80 text-emerald-300 border border-emerald-800'
                                : 'bg-rose-950 text-rose-300 border border-rose-800 animate-pulse'
                            }`}
                          >
                            {rule.actualAccess}
                          </span>
                        ) : (
                          <span className="text-slate-600 font-mono text-[10px]">Pending</span>
                        )}
                      </td>
                      <td className="py-2.5 px-3">
                        {rule.status ? (
                          <StatusBadge status={rule.status} size="sm" />
                        ) : (
                          <span className="text-slate-500 font-mono text-[10px]">QUEUED</span>
                        )}
                      </td>
                      <td className="py-2.5 px-3 text-xs text-slate-400 font-sans max-w-xs">
                        {rule.evidenceNote ? (
                          <span className={isViolation ? 'text-rose-300 font-medium' : ''}>
                            {rule.evidenceNote}
                          </span>
                        ) : (
                          <span className="text-slate-600 font-mono">—</span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      ) : (
        <EmptyState
          icon={ShieldCheck}
          title="No RBAC Scenarios Configured"
          description="Define role access boundaries and security matrices to audit permission policies."
        />
      )}

      {/* Define Rule Modal */}
      {isAddRuleOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4 animate-in fade-in duration-100">
          <div
            className="w-full max-w-md rounded-lg bg-[#0B1120] border border-slate-800 shadow-2xl p-4 space-y-3.5 text-xs"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between pb-2.5 border-b border-slate-800">
              <h3 className="text-xs font-bold text-slate-100 uppercase font-mono">Define RBAC Rule Assertion</h3>
              <button
                onClick={() => setIsAddRuleOpen(false)}
                className="text-slate-400 hover:text-slate-200 cursor-pointer"
              >
                ✕
              </button>
            </div>

            <form onSubmit={handleAddRule} className="space-y-3">
              <div>
                <label className="block text-slate-300 font-medium mb-1">Subject Role</label>
                <input
                  type="text"
                  value={newRole}
                  onChange={(e) => setNewRole(e.target.value)}
                  className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                  required
                />
              </div>

              <div>
                <label className="block text-slate-300 font-medium mb-1">Target Resource</label>
                <input
                  type="text"
                  value={newResource}
                  onChange={(e) => setNewResource(e.target.value)}
                  placeholder="e.g. Payments:Refunds"
                  className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                  required
                />
              </div>

              <div className="grid grid-cols-2 gap-2.5">
                <div>
                  <label className="block text-slate-300 font-medium mb-1">Action</label>
                  <select
                    value={newAction}
                    onChange={(e) => setNewAction(e.target.value as any)}
                    className="w-full px-2 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                  >
                    <option value="READ">READ</option>
                    <option value="WRITE">WRITE</option>
                    <option value="DELETE">DELETE</option>
                    <option value="EXECUTE">EXECUTE</option>
                    <option value="ADMIN">ADMIN</option>
                    <option value="IMPERSONATE">IMPERSONATE</option>
                  </select>
                </div>

                <div>
                  <label className="block text-slate-300 font-medium mb-1">Expected Policy</label>
                  <select
                    value={newExpected}
                    onChange={(e) => setNewExpected(e.target.value as any)}
                    className="w-full px-2 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                  >
                    <option value="ALLOW">ALLOW</option>
                    <option value="DENY">DENY</option>
                    <option value="CONDITIONAL">CONDITIONAL</option>
                  </select>
                </div>
              </div>

              <div className="pt-2.5 border-t border-slate-800 flex justify-end gap-2">
                <button
                  type="button"
                  onClick={() => setIsAddRuleOpen(false)}
                  className="px-3 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-300 hover:bg-slate-800 cursor-pointer"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="px-3.5 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold cursor-pointer shadow-sm shadow-indigo-950/60"
                >
                  Add Rule
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
};
