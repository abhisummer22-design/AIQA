/**
 * Test Case Validator — the real validation lifecycle, on its own page.
 *
 * History: FE-11 replaced this page's original fabricated UI (5 hardcoded
 * `ValidationRuleCheck` records, an invented traceability/quality-score/
 * duplicate-similarity/risk-coverage model, a client-side "AI Auto-Fix"
 * button) with a "not implemented" state, then ACC-005 turned that into a
 * redirect pointing at the Test Generator, where the real validate/approve/
 * reject lifecycle was already wired. The redirect was accurate but left
 * this page with nothing real on it.
 *
 * This version puts the real capability here directly. Every value shown is
 * a real backend response; there is no local, simulated, or sample data
 * anywhere in this file:
 *
 *   - **The queue** is `GET /api/v1/projects/:id/test-cases` — the real,
 *     persisted rows, with their real `status` and real `validation_notes`.
 *   - **Validate / Approve / Reject** call the real
 *     `POST .../test-cases/:id/validate|approve|reject` endpoints. A real
 *     409 (`structural validation failed: …`, or a real state-machine
 *     rejection) is surfaced verbatim against the row that produced it —
 *     never swallowed, and never converted into a fake success.
 *   - **Realtime**: the shared `RealtimeContext` socket refetches the queue
 *     when real run events arrive, and the live `OrchestratorLogsPanel`
 *     streams the real `validation`-source log entries these actions now
 *     emit (`api.rs::validate/approve/reject_test_case_scoped`).
 *
 * Honest scope, stated on the page itself rather than only here: the real
 * backend validator is a **structural** check — non-empty title,
 * description, steps and expected result, and every step having both an
 * action and an expected result. It does not implement duplicate/overlap
 * detection, quality-benchmark scoring, or business-logic/RBAC/security/
 * risk coverage. Those remain genuinely unimplemented, and this page does
 * not imply otherwise.
 *
 * The Test Generator's own validate/approve/reject controls are untouched
 * and keep working — both surfaces call the same real endpoints.
 */

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  AlertTriangle,
  CheckCheck,
  CheckCircle2,
  FolderOpen,
  Info,
  Loader2,
  RefreshCw,
  ShieldCheck,
  XCircle,
} from 'lucide-react';
import { SectionHeader } from '../common/SectionHeader';
import { EmptyState } from '../common/EmptyState';
import { OrchestratorLogsPanel } from '../dashboard/OrchestratorLogsPanel';
import { aiqaApi, ApiError } from '../../services/aiqaApi';
import { useAuth } from '../../context/AuthContext';
import { useProject } from '../../context/ProjectContext';
import { useRealtime } from '../../context/RealtimeContext';
import { renderAuthState, renderResourceErrorState } from '../state/StateViews';
import type { TestCaseRow, TestCaseStatus } from '../../types/contract';

const statusStyles: Record<TestCaseStatus, string> = {
  GENERATED: 'text-slate-300 bg-slate-800/70 border-slate-700',
  VALIDATED: 'text-yellow-300 bg-yellow-950/70 border-yellow-800',
  APPROVED: 'text-emerald-300 bg-emerald-950/70 border-emerald-800',
  REJECTED: 'text-rose-300 bg-rose-950/70 border-rose-800',
  EXECUTED: 'text-violet-300 bg-violet-950/70 border-violet-800',
};

const Pill: React.FC<{ label: string; className?: string }> = ({ label, className }) => (
  <span
    className={`inline-flex items-center px-1.5 py-0.5 rounded border text-[9px] font-mono font-bold uppercase tracking-wide shrink-0 ${
      className ?? 'text-slate-400 bg-slate-800/60 border-slate-700'
    }`}
  >
    {label}
  </span>
);

const ActionButton: React.FC<{
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  onClick: () => void;
  busy?: boolean;
  disabled?: boolean;
  tone?: 'primary' | 'danger' | 'neutral';
  id?: string;
}> = ({ label, icon: Icon, onClick, busy, disabled, tone = 'neutral', id }) => {
  const tones = {
    primary: 'bg-violet-600 hover:bg-violet-500 text-white border-transparent',
    danger: 'bg-[#020617] hover:bg-rose-950/60 text-rose-300 border-rose-800/70',
    neutral: 'bg-[#020617] hover:bg-slate-800 text-slate-300 border-slate-700',
  } as const;
  return (
    <button
      type="button"
      id={id}
      onClick={onClick}
      disabled={disabled || busy}
      className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md border font-mono text-[11px] font-semibold cursor-pointer transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${tones[tone]}`}
    >
      {busy ? <Loader2 className="w-3 h-3 animate-spin" /> : <Icon className="w-3 h-3" />}
      {label}
    </button>
  );
};

export const TestCaseValidatorPage: React.FC = () => {
  const navigate = useNavigate();
  const auth = useAuth();
  const realtime = useRealtime();
  const { selectedProjectId } = useProject();

  const [cases, setCases] = useState<TestCaseRow[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  /** Per-row, real backend failure (e.g. a 409 "structural validation
   *  failed: …"), keyed by test case id. Shown against the exact row that
   *  produced it rather than as one page-level banner. */
  const [rowErrors, setRowErrors] = useState<Record<string, string>>({});
  const [busyId, setBusyId] = useState<string | null>(null);

  const isUnmounted = useRef(false);

  const load = useCallback(
    async (projectId: string, signal?: AbortSignal) => {
      setLoading(true);
      try {
        const rows = await aiqaApi.testCases.list(projectId, {}, signal);
        if (signal?.aborted || isUnmounted.current) return;
        setCases(rows);
        setError(null);
      } catch (err) {
        if (signal?.aborted || isUnmounted.current) return;
        // A failed load must not leave the previous project's rows on screen
        // looking current.
        setCases(null);
        if (err instanceof ApiError) setError(err);
      } finally {
        if (!signal?.aborted && !isUnmounted.current) setLoading(false);
      }
    },
    [],
  );

  useEffect(() => {
    isUnmounted.current = false;
    if (auth.status !== 'authenticated' || !selectedProjectId) {
      setCases(null);
      setError(null);
      return;
    }
    const controller = new AbortController();
    void load(selectedProjectId, controller.signal);
    return () => {
      isUnmounted.current = true;
      controller.abort();
    };
  }, [auth.status, selectedProjectId, load]);

  // Real events (execution/handoff) can change a case's status server-side;
  // refetch on the shared socket rather than polling.
  useEffect(() => {
    if (auth.status !== 'authenticated' || !selectedProjectId) return;
    return realtime.subscribe(() => {
      void load(selectedProjectId);
    });
  }, [auth.status, selectedProjectId, realtime, load]);

  const runAction = useCallback(
    async (row: TestCaseRow, action: 'validate' | 'approve' | 'reject') => {
      if (!selectedProjectId) return;
      setBusyId(row.id);
      setRowErrors((prev) => {
        const next = { ...prev };
        delete next[row.id];
        return next;
      });
      try {
        const updated = await aiqaApi.testCases[action](selectedProjectId, row.id);
        if (isUnmounted.current) return;
        // Replace with the real returned row — never optimistically assume
        // the new status.
        setCases((prev) => (prev ? prev.map((c) => (c.id === updated.id ? updated : c)) : prev));
      } catch (err) {
        if (isUnmounted.current) return;
        const message = err instanceof Error ? err.message : `Failed to ${action} this test case.`;
        setRowErrors((prev) => ({ ...prev, [row.id]: message }));
      } finally {
        if (!isUnmounted.current) setBusyId(null);
      }
    },
    [selectedProjectId],
  );

  if (auth.status !== 'authenticated') {
    return (
      <div id="test-case-validator-page" className="space-y-4 animate-in fade-in duration-150">
        <SectionHeader
          title="Test Case Validator"
          subtitle="Real structural validation, approval, and rejection"
          badge="VALIDATION"
          icon={CheckCheck}
        />
        {renderAuthState(auth)}
      </div>
    );
  }

  const counts = cases
    ? cases.reduce<Record<string, number>>((acc, c) => {
        acc[c.status] = (acc[c.status] ?? 0) + 1;
        return acc;
      }, {})
    : {};

  const renderQueue = () => {
    if (!selectedProjectId) {
      return (
        <EmptyState
          icon={FolderOpen}
          title="No project selected"
          description="Choose a project from the header selector to load its real, persisted test cases."
        />
      );
    }

    if (error) {
      return renderResourceErrorState(error, () => void load(selectedProjectId), {
        forbiddenTitle: 'Not permitted to view these test cases',
        forbiddenDescription:
          'This key is valid but is not permitted to read test cases for this project. Validation requires Operator role or above.',
        offlineDescriptionSuffix: 'to load real test cases.',
        genericTitle: "Couldn't load test cases",
      });
    }

    if (!cases) {
      return (
        <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-6 flex items-center justify-center gap-2 text-slate-400 text-xs font-mono">
          <Loader2 className="w-4 h-4 animate-spin text-violet-400" />
          Loading real test cases…
        </div>
      );
    }

    if (cases.length === 0) {
      return (
        <EmptyState
          icon={CheckCheck}
          title="No test cases to validate"
          description="This project has no saved test cases yet. Generate and save some on the Test Generator page — they will appear here for validation."
          actionLabel="Go to Test Generator"
          onAction={() => navigate('/test-generator')}
        />
      );
    }

    return (
      <div className="space-y-2">
        {cases.map((row) => {
          const rowError = rowErrors[row.id];
          const busy = busyId === row.id;
          return (
            <div
              key={row.id}
              id={`validator-case-${row.id}`}
              className="rounded-lg border border-slate-800 bg-[#0B1120] p-3 space-y-2"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0 space-y-0.5">
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className="text-xs font-semibold text-slate-100">{row.title}</span>
                    <Pill label={row.status} className={statusStyles[row.status]} />
                  </div>
                  <p className="text-[11px] text-slate-400">{row.description}</p>
                  <div className="text-[10px] font-mono text-content-muted flex flex-wrap gap-x-3">
                    <span>{row.category}</span>
                    <span>{row.priority}</span>
                    <span>
                      {row.steps.length} step{row.steps.length === 1 ? '' : 's'}
                    </span>
                    <span>source: {row.source}</span>
                  </div>
                </div>

                <div className="flex items-center gap-1.5 shrink-0">
                  {/* Controls follow the real backend state machine: validate
                      only from GENERATED, approve only from VALIDATED,
                      reject only while not APPROVED/EXECUTED. Rendering a
                      control the backend would reject with a 409 would be a
                      fake affordance. */}
                  {row.status === 'GENERATED' && (
                    <ActionButton
                      id={`btn-validate-${row.id}`}
                      label="Validate"
                      icon={ShieldCheck}
                      tone="primary"
                      busy={busy}
                      onClick={() => void runAction(row, 'validate')}
                    />
                  )}
                  {row.status === 'VALIDATED' && (
                    <ActionButton
                      id={`btn-approve-${row.id}`}
                      label="Approve"
                      icon={CheckCircle2}
                      tone="primary"
                      busy={busy}
                      onClick={() => void runAction(row, 'approve')}
                    />
                  )}
                  {(row.status === 'GENERATED' || row.status === 'VALIDATED') && (
                    <ActionButton
                      id={`btn-reject-${row.id}`}
                      label="Reject"
                      icon={XCircle}
                      tone="danger"
                      busy={busy}
                      onClick={() => void runAction(row, 'reject')}
                    />
                  )}
                </div>
              </div>

              {/* The backend's own real note, not a client-side restatement. */}
              {row.validation_notes && (
                <p className="text-[10px] font-mono text-emerald-300/90 flex items-start gap-1.5">
                  <CheckCircle2 className="w-3 h-3 shrink-0 mt-0.5" />
                  <span>{row.validation_notes}</span>
                </p>
              )}

              {rowError && (
                <p className="text-[10px] font-mono text-rose-300 flex items-start gap-1.5">
                  <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
                  <span>{rowError}</span>
                </p>
              )}
            </div>
          );
        })}
      </div>
    );
  };

  return (
    <div id="test-case-validator-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Test Case Validator"
        subtitle="Real structural validation, approval, and rejection"
        badge="VALIDATION"
        icon={CheckCheck}
        actions={
          <div className="flex items-center gap-2">
            {cases && (
              <span className="text-[10px] font-mono text-content-muted">
                {cases.length} case{cases.length === 1 ? '' : 's'}
                {Object.entries(counts).map(([status, n]) => ` · ${n} ${status.toLowerCase()}`)}
              </span>
            )}
            <button
              type="button"
              id="btn-validator-refresh"
              onClick={() => selectedProjectId && void load(selectedProjectId)}
              disabled={!selectedProjectId || loading}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] border border-slate-700 hover:border-violet-600 text-slate-300 hover:text-violet-300 font-mono text-xs font-semibold cursor-pointer transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
              Refresh
            </button>
          </div>
        }
      />

      {/* Same two-column pattern as the Dashboard: existing content left,
          the real live stream right; stacks below `xl`. */}
      <div className="grid grid-cols-1 xl:grid-cols-[minmax(0,1fr)_400px] gap-4 items-start">
        <div className="space-y-3 min-w-0">
          <p className="text-[10px] font-mono text-content-muted flex items-start gap-1.5 px-1">
            <Info className="w-3 h-3 shrink-0 mt-0.5" />
            <span>
              Validation is structural only — it checks that title, description, steps and expected result are
              present and that every step has an action and an expected result. Duplicate detection, quality
              scoring, and business-logic/RBAC/security/risk coverage are not implemented.
            </span>
          </p>
          {renderQueue()}
        </div>

        <div className="min-w-0 xl:sticky xl:top-4">
          <OrchestratorLogsPanel projectId={selectedProjectId} />
        </div>
      </div>
    </div>
  );
};
