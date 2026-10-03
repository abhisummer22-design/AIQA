/**
 * FE-8 — the real Execution Hub. FE-9 — event-driven, not polled.
 *
 * Backend contract (re-verified at the start of this sprint, `services/
 * orchestrator/src/api.rs`, `domain.rs`, `policy.rs`, `scheduler.rs`):
 *
 *   POST/GET /api/v1/projects/:project_id/executions            (create_run_scoped / list_runs_scoped)
 *   GET      /api/v1/projects/:project_id/executions/:run_id    (get_run_scoped — RunDetail = RunSummary + tasks)
 *   POST     .../executions/:run_id/cancel                      (cancel_run_scoped)
 *   POST     .../executions/:run_id/approve                     (approve_run_scoped)
 *   POST     .../executions/:run_id/reject                      (reject_run_scoped)
 *
 * All already exposed by FE-1 (`aiqaApi.executions.*`) before this sprint —
 * zero new transport code. One real, pre-existing type inaccuracy was fixed
 * in `aiqaApi.ts` as part of this sprint: `approve`/`reject` were typed as
 * returning `RunDetail`, but `approve_run_scoped`/`reject_run_scoped` both
 * actually return `RunSummary` (no `tasks` field) — confirmed by reading the
 * handlers directly. Fixed to the real return type; not otherwise changed.
 *
 * Policy/approval gate (`policy.rs::evaluate`, real, already implemented):
 * `POST .../executions` can return `RunState::PendingApproval` instead of
 * scheduling immediately, when a task names an executor configured in
 * `AIQA_REQUIRE_APPROVAL_FOR` (default: `maestro` — the one adapter that can
 * touch a real mobile device). A `PendingApproval` run only leaves that
 * state via a real `POST .../approve` or `.../reject` call — this page
 * surfaces both, never a client-side approximation.
 *
 * Status vocabulary (`domain.rs::RunState`/`TaskStatus`) is used exactly as
 * the backend returns it — `CREATED/PENDING_APPROVAL/REJECTED/SCHEDULED/
 * RUNNING/PARTIAL/COMPLETED/CANCELLED/FAILED` for runs, `QUEUED/RUNNING/
 * PASSED/FAILED/BLOCKED/SKIPPED/CANCELLED/TIMEOUT/INFRA_ERROR/NEEDS_REVIEW`
 * for tasks. `types/contract.ts`'s own `TaskStatus` doc comment already
 * flags one real, pre-existing vocabulary gap against product documentation
 * ("still `INFRA_ERROR`, not the contract's `INFRASTRUCTURE_ERROR`... that
 * rename is FE-12... do not silently alias it here") — this sprint follows
 * that same rule and performs no vocabulary translation of its own.
 *
 * Monitoring mechanism (FE-9, replacing FE-8's original always-on 3-second
 * poll): the centralized `RealtimeContext` (`useRealtime().subscribe`) is
 * now primary. A real event for the selected run triggers a debounced
 * resync of its detail; a real event for a run already in the list (or a
 * brand-new run's first `RUN_SCHEDULED`) triggers a debounced resync of the
 * list — never a hand-patch from the event payload alone (`RunEvent`
 * structurally cannot carry a task's `started_at`/`finished_at`, nor a
 * run's final `RunState` on `RUN_COMPLETED` — see `RealtimeContext.tsx`'s
 * own doc comment). There is **no fallback timer** — brief §36's own "do
 * not leave uncontrolled duplicate polling + realtime running indefinitely"
 * is taken literally: when the connection is not `connected`, the existing
 * real "Refresh" control (already extended to also resync the selected
 * run's detail, not just the list) is this page's disclosed fallback, and
 * `connectionState` is surfaced as a small, honest indicator next to it —
 * never fabricated as "live" when it isn't. See the FE-9 report §15.
 *
 * Test Generator handoff (brief §11): deliberately **not changed**. Its
 * `handleExportToExecution` still creates a fabricated `AIQAContext` run,
 * exactly as FE-4 left it. This is a disclosed, considered decision, not an
 * oversight — see this file's own report §8 for the full reasoning: an
 * AI-generated test case has no real mapping to this backend's actual
 * `TaskDef` (`executor` must be one of `pytest|playwright|newman|maestro`,
 * or `simulate`, with executor-specific `params`) without inventing that
 * mapping, which brief §11 itself forbids ("do not invent missing test/task
 * fields... do not force this flow if the actual backend contract does not
 * support it").
 *
 * No FRD/SRD/PRD/flow-diagram document existed in this repository at the
 * time FE-4 through FE-7 were built; three were added afterward and are
 * now present (`AIQA_Consolidated_PRD_FRD_SRD_v6.0...`,
 * `AIQA_Business_Logic_Architecture_v2.0...`, `AIQA_Complete_End_to_End_
 * Flow_Diagram_v6.0.pdf`, all re-read for this sprint). Their Execution
 * Orchestrator requirements (dependency-aware DAG, distinct product vs.
 * infrastructure failure, policy/approval gates, evidence-first execution)
 * describe exactly the real backend behavior above — no contradiction was
 * found for the Execution Hub specifically (unlike Audit/Evidence's
 * navigation placement, tracked separately in
 * `AIQA_PHASE4_DOCUMENTATION_COMPLIANCE_GAP_ANALYSIS.md`).
 */

import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  Ban,
  CheckCircle2,
  ExternalLink,
  FolderOpen,
  Inbox,
  Layers,
  Play,
  Plus,
  RotateCcw,
  Square,
  Trash2,
  Wifi,
  WifiOff,
  XCircle,
} from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { ApiError, aiqaApi, getApiKey } from '../../services/aiqaApi';
import { useProject } from '../../context/ProjectContext';
import { useRealtime } from '../../context/RealtimeContext';
import type { RealtimeConnectionState } from '../../context/RealtimeContext';
import { StateCard, Cta, renderResourceErrorState } from '../state/StateViews';
import { EmptyState } from '../common/EmptyState';
import { formatStatusLabel } from '../../utils/statusLabel';
import type { RunDetail, RunListItem, RunPlan, RunState, TaskStatus } from '../../types/contract';

/** FE-9: a burst of real events (several tasks starting/finishing within
 *  the same tick of a highly-parallel run) collapses into one trailing
 *  fetch, never one request per event (brief §15/§16/§37: "do not create
 *  request storms"). Not a poll interval — nothing re-schedules itself; a
 *  fetch only ever happens because a real event, a reconnect, or the user's
 *  own Refresh click asked for one. */
const EVENT_RESYNC_DEBOUNCE_MS = 250;

/** Mirrors `domain.rs::RunState::is_terminal()` exactly, for client-side
 *  "stop polling" / "is Cancel still valid" decisions only — never used as
 *  an authorization decision (the backend re-checks this itself on every
 *  cancel/approve/reject call and is the only authority that matters). */
const TERMINAL_RUN_STATES = new Set<RunState>(['REJECTED', 'PARTIAL', 'COMPLETED', 'CANCELLED', 'FAILED']);

// Phase 2 (Execution Handoff) — exported so the Test Case Generator's own
// handoff UI (`GeneratedTestCasesView.tsx`) offers the exact same real
// executor choices as this page, rather than a second, possibly-diverging
// list. `''` still means "Simulate (no real executor)" everywhere it's used.
export const EXECUTOR_OPTIONS: Array<{ value: string; label: string }> = [
  { value: '', label: 'Simulate (no real executor)' },
  { value: 'pytest', label: 'pytest — API' },
  { value: 'playwright', label: 'Playwright — Browser' },
  { value: 'newman', label: 'Newman — API collection' },
  { value: 'maestro', label: 'Maestro — Mobile (requires approval)' },
];
// Real wire values (`domain.rs::SimOutcome`'s `#[serde(rename_all =
// "snake_case")]`) — confirmed live: a real create request with the Rust
// enum's own variant names (`'Pass'`, ...) was genuinely rejected by the
// backend with a 422 before this was fixed (see `types/contract.ts`'s own
// note on this same finding). `label` is display-only.
export const SIMULATE_OUTCOMES: Array<{ value: 'pass' | 'fail' | 'timeout' | 'infra_error'; label: string }> = [
  { value: 'pass', label: 'Pass' },
  { value: 'fail', label: 'Fail' },
  { value: 'timeout', label: 'Timeout' },
  { value: 'infra_error', label: 'Infra Error' },
];

interface DraftTask {
  key: string;
  id: string;
  name: string;
  executor: string;
  simulateOutcome: (typeof SIMULATE_OUTCOMES)[number]['value'];
  // FE-17 — real `TaskDef.prerequisites` exposure. Stores the *other* draft
  // tasks' stable `key` (not their editable `id` text), so a prerequisite
  // selection survives the referenced task's id being retyped later — it's
  // resolved to that task's current `id` only at submission time (see
  // `handleCreate`). Never contains this task's own `key` (the UI never
  // offers a task as its own candidate — see `renderCreatePanel` below), so
  // a self-dependency is structurally unrepresentable, not just blocked.
  prerequisites: string[];
}

let draftTaskCounter = 0;
function newDraftTask(): DraftTask {
  draftTaskCounter += 1;
  return { key: `draft-${draftTaskCounter}`, id: `task-${draftTaskCounter}`, name: '', executor: '', simulateOutcome: 'pass', prerequisites: [] };
}

const RUN_STATE_STYLE: Record<RunState, string> = {
  CREATED: 'text-slate-300 bg-slate-800/60 border-slate-700',
  PENDING_APPROVAL: 'text-amber-300 bg-amber-950/60 border-amber-800',
  REJECTED: 'text-rose-300 bg-rose-950/60 border-rose-800',
  SCHEDULED: 'text-violet-300 bg-violet-950/60 border-violet-800',
  RUNNING: 'text-yellow-300 bg-yellow-950/60 border-yellow-800',
  PARTIAL: 'text-amber-300 bg-amber-950/60 border-amber-800',
  COMPLETED: 'text-emerald-300 bg-emerald-950/60 border-emerald-800',
  CANCELLED: 'text-slate-300 bg-slate-800/60 border-slate-700',
  FAILED: 'text-rose-300 bg-rose-950/60 border-rose-800',
};

/** FE-9 — small, honest connection indicator (brief §23: "Do not make
 *  connection state more visually prominent than the actual product
 *  workflow requires"). Reuses the same badge component/style language as
 *  `RUN_STATE_STYLE`/`TASK_STATUS_STYLE` above, not a new visual system. */
const REALTIME_STATE_STYLE: Record<RealtimeConnectionState, string> = {
  disconnected: 'text-slate-400 bg-slate-800/60 border-slate-700',
  connecting: 'text-violet-300 bg-violet-950/60 border-violet-800',
  connected: 'text-emerald-300 bg-emerald-950/60 border-emerald-800',
  reconnecting: 'text-amber-300 bg-amber-950/60 border-amber-800',
  error: 'text-rose-300 bg-rose-950/60 border-rose-800',
};
const REALTIME_STATE_LABEL: Record<RealtimeConnectionState, string> = {
  disconnected: 'Realtime Off',
  connecting: 'Connecting…',
  connected: 'Live',
  reconnecting: 'Reconnecting…',
  error: 'Realtime Error',
};

const TASK_STATUS_STYLE: Record<TaskStatus, string> = {
  QUEUED: 'text-slate-300 bg-slate-800/60 border-slate-700',
  RUNNING: 'text-yellow-300 bg-yellow-950/60 border-yellow-800',
  PASSED: 'text-emerald-300 bg-emerald-950/60 border-emerald-800',
  FAILED: 'text-rose-300 bg-rose-950/60 border-rose-800',
  BLOCKED: 'text-purple-300 bg-purple-950/60 border-purple-800',
  SKIPPED: 'text-slate-300 bg-slate-800/60 border-slate-700',
  CANCELLED: 'text-slate-300 bg-slate-800/60 border-slate-700',
  TIMEOUT: 'text-orange-300 bg-orange-950/60 border-orange-800',
  INFRA_ERROR: 'text-orange-300 bg-orange-950/60 border-orange-800',
  NEEDS_REVIEW: 'text-amber-300 bg-amber-950/60 border-amber-800',
};

/** Every badge always pairs a color with the literal status text (brief
 *  §22: "Do not use color alone to distinguish statuses"). */
const Badge: React.FC<{ label: string; className: string }> = ({ label, className }) => (
  <span className={`inline-flex items-center px-2 py-0.5 rounded border text-[10px] font-mono font-bold uppercase tracking-wide ${className}`}>
    {label}
  </span>
);

function redactOwnKey(value: string | null): string | null {
  if (!value) return value;
  const key = getApiKey();
  if (!key) return value;
  return value.split(key).join('••••••••');
}

function formatTimestamp(iso: string | null): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, { month: 'short', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

export const ExecutionHubPage: React.FC = () => {
  const { selectedProjectId, selectedProject } = useProject();
  const realtime = useRealtime();
  const navigate = useNavigate();

  const [runs, setRuns] = useState<RunListItem[] | null>(null);
  const [runsLoading, setRunsLoading] = useState(false);
  const [runsRefreshing, setRunsRefreshing] = useState(false);
  const [runsError, setRunsError] = useState<ApiError | null>(null);
  const runsAbortRef = useRef<AbortController | null>(null);
  const runsRef = useRef<RunListItem[] | null>(null);
  useEffect(() => {
    runsRef.current = runs;
  }, [runs]);

  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const selectedRunIdRef = useRef<string | null>(null);
  useEffect(() => {
    selectedRunIdRef.current = selectedRunId;
  }, [selectedRunId]);
  const [detail, setDetail] = useState<RunDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState<ApiError | null>(null);
  const detailAbortRef = useRef<AbortController | null>(null);

  // Whether the selected run has any real evidence — checked once per
  // selection (not polled) so "View Evidence" is only ever shown when it's
  // genuinely true, never a blind navigation link (brief §20: "do not
  // fabricate evidence counts"). `null` = not yet confirmed either way,
  // which renders the same as "hidden" — a fetch failure here must never
  // be read as "evidence exists".
  const [hasEvidence, setHasEvidence] = useState<boolean | null>(null);
  const evidenceCheckAbortRef = useRef<AbortController | null>(null);

  const [showCreate, setShowCreate] = useState(false);
  const [runName, setRunName] = useState('');
  const [draftTasks, setDraftTasks] = useState<DraftTask[]>([newDraftTask()]);
  const [concurrency, setConcurrency] = useState('');
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  // Live-discovered FE-8 bug fix: two native click events fired within the
  // same tick (a real fast double-click, or a repeated Enter keypress) both
  // read the same stale `creating` closure before React commits the first
  // click's `setCreating(true)` — the `disabled` attribute alone does not
  // stop this, and it produced two real `201 Created` executions against
  // the live backend during Scenario O manual verification. A ref is
  // mutated synchronously, so the second call sees the lock immediately.
  const creatingLockRef = useRef(false);

  const [actionPending, setActionPending] = useState(false);
  const [actionError, setActionError] = useState<ApiError | null>(null);

  // ------------------------------------------------------------- run list

  const fetchRuns = useCallback(async (projectId: string, isRefresh: boolean) => {
    runsAbortRef.current?.abort();
    const controller = new AbortController();
    runsAbortRef.current = controller;
    if (isRefresh) setRunsRefreshing(true);
    else setRunsLoading(true);
    setRunsError(null);
    try {
      const list = await aiqaApi.executions.list(projectId, {}, controller.signal);
      if (controller.signal.aborted) return;
      setRuns(list);
      // Refresh (brief §25): clear a selection that no longer exists.
      setSelectedRunId((current) => (current && !list.some((r) => r.run_id === current) ? null : current));
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return;
      const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
      setRunsError(apiErr);
      setRuns(null);
    } finally {
      if (!controller.signal.aborted) {
        setRunsLoading(false);
        setRunsRefreshing(false);
      }
    }
  }, []);

  useEffect(() => {
    setSelectedRunId(null);
    setShowCreate(false);
    setCreateError(null);
    if (!selectedProjectId) {
      runsAbortRef.current?.abort();
      setRuns(null);
      setRunsError(null);
      setRunsLoading(false);
      return;
    }
    setRuns(null);
    void fetchRuns(selectedProjectId, false);
  }, [selectedProjectId, fetchRuns]);

  const handleRefreshRuns = useCallback(() => {
    if (!selectedProjectId || runsLoading || runsRefreshing) return;
    void fetchRuns(selectedProjectId, true);
    // FE-9: "Refresh" now also resyncs whatever is currently on screen, not
    // just the list — the same real detail endpoint the initial load and
    // every event-triggered resync already use, not a second refresh path.
    if (selectedRunIdRef.current) void fetchDetail(selectedProjectId, selectedRunIdRef.current);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- fetchDetail is declared below; both are stable ([]) callbacks
  }, [selectedProjectId, runsLoading, runsRefreshing, fetchRuns]);

  // -------------------------------------------------------- selected run

  /** FE-9: no longer self-reschedules (FE-8's `setTimeout(POLL_INTERVAL_MS)`
   *  loop is gone). This is now called exactly three ways: once when a run
   *  is first selected (effect below), once per debounced real event for
   *  this run (subscription effect further down), and once on reconnect/
   *  manual Refresh — never on a timer. */
  const fetchDetail = useCallback(async (projectId: string, runId: string) => {
    detailAbortRef.current?.abort();
    const controller = new AbortController();
    detailAbortRef.current = controller;
    setDetailError(null);
    try {
      const d = await aiqaApi.executions.get(projectId, runId, controller.signal);
      if (controller.signal.aborted) return;
      setDetail(d);
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return;
      const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
      setDetailError(apiErr);
      setDetail(null);
    } finally {
      if (!controller.signal.aborted) setDetailLoading(false);
    }
  }, []);

  useEffect(() => {
    detailAbortRef.current?.abort();
    setDetail(null);
    setDetailError(null);
    setActionError(null);
    if (!selectedProjectId || !selectedRunId) {
      setDetailLoading(false);
      return;
    }
    setDetailLoading(true);
    void fetchDetail(selectedProjectId, selectedRunId);
    return () => {
      detailAbortRef.current?.abort();
    };
  }, [selectedProjectId, selectedRunId, fetchDetail]);

  // ------------------------------------------------------- realtime events
  //
  // FE-9: one subscription to the centralized `RealtimeContext` (never a
  // second/page-local connection — brief §8). `RealtimeContext` has already
  // validated shape, enforced project isolation, and deduplicated by
  // `event_id`; this page's own job is purely "which of *my* two resources
  // (the selected run's detail, the run list) does this event concern".
  //
  // Both branches below fetch the real resource rather than hand-patching
  // fields from the event payload — `RunEvent` structurally cannot carry
  // everything a full resync needs (no `started_at`/`finished_at` on a
  // TASK_* event, no final `RunState` at all on `RUN_COMPLETED` — see
  // `RealtimeContext.tsx`'s own module doc comment). Debounced
  // (`EVENT_RESYNC_DEBOUNCE_MS`) so a burst of events for a highly parallel
  // run collapses into one request each, not one per event.
  const detailResyncTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const listResyncTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!selectedProjectId) return;
    const unsubscribe = realtime.subscribe((event) => {
      if (selectedRunIdRef.current && event.run_id === selectedRunIdRef.current) {
        if (detailResyncTimeoutRef.current) clearTimeout(detailResyncTimeoutRef.current);
        detailResyncTimeoutRef.current = setTimeout(() => {
          detailResyncTimeoutRef.current = null;
          void fetchDetail(selectedProjectId, event.run_id);
        }, EVENT_RESYNC_DEBOUNCE_MS);
      }

      // List consistency (brief §16): only resync the list for an event
      // that could actually change what it shows — a run already listed
      // (its counts/state may have changed), or `RUN_SCHEDULED` (a brand
      // new run's first-ever event, the signal it should now appear at
      // all). Anything else (e.g. a `TASK_*` event for a run this page has
      // never listed, because it belongs to a different, unselected
      // project — already filtered by `RealtimeContext` before this
      // listener ever runs) is correctly ignored rather than forcing a
      // request for every event in the whole project.
      const knownRun = runsRef.current?.some((r) => r.run_id === event.run_id) ?? false;
      if (knownRun || event.type === 'RUN_SCHEDULED') {
        if (listResyncTimeoutRef.current) clearTimeout(listResyncTimeoutRef.current);
        listResyncTimeoutRef.current = setTimeout(() => {
          listResyncTimeoutRef.current = null;
          void fetchRuns(selectedProjectId, true);
        }, EVENT_RESYNC_DEBOUNCE_MS);
      }
    });
    return () => {
      unsubscribe();
      if (detailResyncTimeoutRef.current) clearTimeout(detailResyncTimeoutRef.current);
      if (listResyncTimeoutRef.current) clearTimeout(listResyncTimeoutRef.current);
    };
    // Live-discovered FE-9 bug fix: depending on the whole `realtime`
    // object here (instead of just its stable `subscribe` function) meant
    // this effect re-ran on *every single event* — `RealtimeContextValue`
    // is a new object each time `lastEventAt` changes, which is every
    // event, including ones for a *different* run. The resulting
    // unsubscribe-then-resubscribe cycle cancelled the debounce timer this
    // same listener had just scheduled a moment earlier (its cleanup
    // clears `detailResyncTimeoutRef`) — so a run's *last* event
    // (`RUN_COMPLETED`, after which nothing else ever arrives to
    // accidentally retrigger a working resync) could be scheduled and then
    // immediately cancelled, leaving the UI stuck on its last-known
    // non-terminal state indefinitely. Confirmed live: a real run finished
    // (FAILED) server-side and stayed displayed as RUNNING until a manual
    // Refresh, over a minute later. `realtime.subscribe` is memoized with
    // `useCallback(..., [])` in `RealtimeContext.tsx` — depending on it
    // directly keeps this effect (and its debounce timers) alive for the
    // page's entire mounted lifetime, not just between events.
  }, [selectedProjectId, realtime.subscribe, fetchDetail, fetchRuns]);

  // Reconcile authoritative state after reconnect (real, documented
  // requirement — PRD/FRD-RT-03: "Reconcile authoritative state after
  // reconnect"; Business Logic §10: "reconnect reconciles authoritative
  // state"). One real REST resync, not a fabricated "nothing was missed"
  // assumption — there is no project-wide catch-up protocol to trust
  // instead (see `RealtimeContext.tsx`'s own doc comment on this gap).
  const prevConnectionStateRef = useRef(realtime.connectionState);
  useEffect(() => {
    const prev = prevConnectionStateRef.current;
    prevConnectionStateRef.current = realtime.connectionState;
    if (realtime.connectionState === 'connected' && prev !== 'connected' && selectedProjectId) {
      void fetchRuns(selectedProjectId, true);
      if (selectedRunIdRef.current) void fetchDetail(selectedProjectId, selectedRunIdRef.current);
    }
  }, [realtime.connectionState, selectedProjectId, fetchRuns, fetchDetail]);

  // One-off (not polled) real check for whether this run has any evidence
  // at all — reuses FE-7's own real endpoint, never a second evidence
  // system (brief §20).
  useEffect(() => {
    evidenceCheckAbortRef.current?.abort();
    setHasEvidence(null);
    if (!selectedProjectId || !selectedRunId) return;
    const controller = new AbortController();
    evidenceCheckAbortRef.current = controller;
    void (async () => {
      try {
        const rows = await aiqaApi.executions.evidence(selectedProjectId, selectedRunId, controller.signal);
        if (controller.signal.aborted) return;
        setHasEvidence(rows.length > 0);
      } catch {
        if (controller.signal.aborted) return;
        setHasEvidence(null);
      }
    })();
    return () => controller.abort();
  }, [selectedProjectId, selectedRunId]);

  // -------------------------------------------------------------- create

  const addDraftTask = () => setDraftTasks((prev) => [...prev, newDraftTask()]);
  // FE-17 — dependency integrity on removal (brief §8, Option A): deleting a
  // task also strips its `key` from every remaining task's `prerequisites`,
  // so no stale hidden dependency reference survives the delete.
  const removeDraftTask = (key: string) =>
    setDraftTasks((prev) =>
      prev.length > 1
        ? prev.filter((t) => t.key !== key).map((t) => ({ ...t, prerequisites: t.prerequisites.filter((p) => p !== key) }))
        : prev,
    );
  const updateDraftTask = (key: string, patch: Partial<DraftTask>) =>
    setDraftTasks((prev) => prev.map((t) => (t.key === key ? { ...t, ...patch } : t)));
  const toggleDraftPrerequisite = (key: string, prereqKey: string) =>
    setDraftTasks((prev) =>
      prev.map((t) =>
        t.key === key
          ? {
              ...t,
              prerequisites: t.prerequisites.includes(prereqKey)
                ? t.prerequisites.filter((p) => p !== prereqKey)
                : [...t.prerequisites, prereqKey],
            }
          : t,
      ),
    );

  const handleCreate = useCallback(async () => {
    if (!selectedProjectId || creating || creatingLockRef.current) return;
    creatingLockRef.current = true;
    setCreateError(null);

    const name = runName.trim();
    if (!name) {
      setCreateError('Execution name is required.');
      creatingLockRef.current = false;
      return;
    }
    const ids = draftTasks.map((t) => t.id.trim());
    if (ids.some((id) => !id)) {
      setCreateError('Every task needs an ID.');
      creatingLockRef.current = false;
      return;
    }
    // Mirrors `scheduler.rs::validate_plan`'s cheapest real check (unique
    // ids) for UX only — the backend re-validates this and every other rule
    // (cycle detection, unknown prerequisites) authoritatively; see
    // `scheduler.rs::validate_plan`, confirmed real and tested (FE-17
    // report §3/§9). This form does not duplicate that engine client-side —
    // a submitted cycle is rejected by the real backend and its exact
    // message is shown via `createError` below, same as any other real
    // validation failure.
    if (new Set(ids).size !== ids.length) {
      setCreateError('Task IDs must be unique.');
      creatingLockRef.current = false;
      return;
    }

    // FE-17 — `prerequisites` resolves each selected dependency's stable
    // `key` to that task's *current* `id` text at submission time (not the
    // id at selection time), so an edit to a referenced task's id never
    // leaves a stale/dangling reference in the real payload.
    const idByKey = new Map(draftTasks.map((t) => [t.key, t.id.trim()]));
    const plan: RunPlan = {
      name,
      tasks: draftTasks.map((t) => ({
        id: t.id.trim(),
        name: t.name.trim() || t.id.trim(),
        prerequisites: t.prerequisites.map((k) => idByKey.get(k)).filter((v): v is string => !!v),
        ...(t.executor ? { executor: t.executor } : { simulate: { outcome: t.simulateOutcome } }),
      })),
      ...(concurrency.trim() ? { concurrency: Number(concurrency) } : {}),
    };

    setCreating(true);
    try {
      const res = await aiqaApi.executions.create(selectedProjectId, plan);
      setShowCreate(false);
      setRunName('');
      setDraftTasks([newDraftTask()]);
      setConcurrency('');
      setSelectedRunId(res.run_id);
      void fetchRuns(selectedProjectId, true);
    } catch (err) {
      const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
      setCreateError(apiErr.message);
    } finally {
      setCreating(false);
      creatingLockRef.current = false;
    }
  }, [selectedProjectId, creating, runName, draftTasks, concurrency, fetchRuns]);

  // ------------------------------------------------------------- actions

  const handleCancel = useCallback(async () => {
    if (!selectedProjectId || !selectedRunId || actionPending) return;
    setActionPending(true);
    setActionError(null);
    try {
      await aiqaApi.executions.cancel(selectedProjectId, selectedRunId);
      await fetchDetail(selectedProjectId, selectedRunId);
    } catch (err) {
      setActionError(err instanceof ApiError ? err : new ApiError('server_error', String(err)));
    } finally {
      setActionPending(false);
    }
  }, [selectedProjectId, selectedRunId, actionPending, fetchDetail]);

  const handleApprove = useCallback(async () => {
    if (!selectedProjectId || !selectedRunId || actionPending) return;
    setActionPending(true);
    setActionError(null);
    try {
      await aiqaApi.executions.approve(selectedProjectId, selectedRunId);
      await fetchDetail(selectedProjectId, selectedRunId);
    } catch (err) {
      setActionError(err instanceof ApiError ? err : new ApiError('server_error', String(err)));
    } finally {
      setActionPending(false);
    }
  }, [selectedProjectId, selectedRunId, actionPending, fetchDetail]);

  const handleReject = useCallback(async () => {
    if (!selectedProjectId || !selectedRunId || actionPending) return;
    setActionPending(true);
    setActionError(null);
    try {
      await aiqaApi.executions.reject(selectedProjectId, selectedRunId);
      await fetchDetail(selectedProjectId, selectedRunId);
    } catch (err) {
      setActionError(err instanceof ApiError ? err : new ApiError('server_error', String(err)));
    } finally {
      setActionPending(false);
    }
  }, [selectedProjectId, selectedRunId, actionPending, fetchDetail]);

  /** Real re-planning v1 — creates a real new run retrying only this run's
   *  real INFRA_ERROR/TIMEOUT tasks (never a real FAILED test), then
   *  switches straight to watching that new run, same as `handleCreate`
   *  already does for a freshly-created one. */
  const handleRetryFailed = useCallback(async () => {
    if (!selectedProjectId || !selectedRunId || actionPending) return;
    setActionPending(true);
    setActionError(null);
    try {
      const retried = await aiqaApi.executions.retryFailed(selectedProjectId, selectedRunId);
      setSelectedRunId(retried.run_id);
      await fetchRuns(selectedProjectId, true);
    } catch (err) {
      setActionError(err instanceof ApiError ? err : new ApiError('server_error', String(err)));
    } finally {
      setActionPending(false);
    }
  }, [selectedProjectId, selectedRunId, actionPending, fetchRuns]);

  const canCancel = detail ? detail.state !== 'PENDING_APPROVAL' && !TERMINAL_RUN_STATES.has(detail.state) : false;
  const canApproveReject = detail?.state === 'PENDING_APPROVAL';
  const canRetryFailed = detail
    ? TERMINAL_RUN_STATES.has(detail.state) && (detail.infra_error > 0 || detail.timeout > 0)
    : false;

  // -------------------------------------------------------------- render

  const errorStateCard = (err: ApiError, onRetry: () => void) =>
    renderResourceErrorState(err, onRetry, {
      forbiddenTitle: 'Not permitted',
      forbiddenDescription: "This key's role or project scope does not permit this action. This is a permission problem, not a bad credential.",
    });

  const renderRunList = () => {
    if (runsLoading) {
      return (
        <StateCard
          icon={Layers}
          iconClassName="text-violet-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
          title="Loading executions"
          description={`Fetching executions for ${selectedProject?.name ?? 'this project'}…`}
        />
      );
    }
    if (runsError) return errorStateCard(runsError, () => selectedProjectId && void fetchRuns(selectedProjectId, false));
    if (runs && runs.length === 0) {
      return (
        <EmptyState
          icon={Inbox}
          title="No executions yet"
          description={`${selectedProject?.name ?? 'This project'} has no executions recorded. Start one below.`}
        />
      );
    }
    return (
      <ul className="divide-y divide-slate-800/80">
        {runs?.map((r) => (
          <li key={r.run_id}>
            <button
              type="button"
              onClick={() => setSelectedRunId(r.run_id)}
              className={`w-full flex items-center gap-3 py-2.5 px-1 text-left hover:bg-slate-900/40 transition-colors cursor-pointer rounded focus:outline-none focus-visible:ring-1 focus-visible:ring-violet-400 ${
                r.run_id === selectedRunId ? 'bg-slate-800/60' : ''
              }`}
            >
              <Badge label={formatStatusLabel(r.state)} className={RUN_STATE_STYLE[r.state]} />
              <span className="font-mono text-xs font-semibold text-slate-100 truncate">{r.name}</span>
              <span className="ml-auto shrink-0 text-[10px] font-mono text-content-muted">
                {r.passed}/{r.total} passed
              </span>
            </button>
          </li>
        ))}
      </ul>
    );
  };

  const renderDetail = () => {
    if (!selectedRunId) {
      return (
        <EmptyState icon={Layers} title="No execution selected" description="Choose an execution from the list, or start a new one." />
      );
    }
    if (detailLoading) {
      return (
        <StateCard
          icon={Layers}
          iconClassName="text-violet-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
          title="Loading execution"
          description="Fetching the real execution detail…"
        />
      );
    }
    if (detailError) return errorStateCard(detailError, () => selectedProjectId && selectedRunId && void fetchDetail(selectedProjectId, selectedRunId));
    if (!detail) return null;

    return (
      <div className="space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-2 pb-2.5 border-b border-slate-800/80">
          <div className="space-y-1">
            <div className="flex items-center gap-2 flex-wrap">
              <h2 className="text-sm font-bold text-slate-100 font-sans">{detail.name}</h2>
              <Badge label={formatStatusLabel(detail.state)} className={RUN_STATE_STYLE[detail.state]} />
            </div>
            <div className="flex items-center gap-3 text-[11px] font-mono text-slate-400 flex-wrap">
              <span>Requested by: <strong className="text-slate-200">{detail.requested_by ?? '—'}</strong></span>
              <span>Started: <strong className="text-slate-200">{formatTimestamp(detail.started_at)}</strong></span>
              <span>Finished: <strong className="text-slate-200">{formatTimestamp(detail.finished_at)}</strong></span>
              {/* FE-18 — real `RunSummary.approved_by`/`.approved_at` (already
                  fetched by every `detail` request; `store.rs::approve_run`/
                  `reject_run` populate both with the real acting API key's
                  name, on *either* a real approval or a real rejection). Only
                  ever shown for a run that genuinely passed through the real
                  policy approval gate — a run that was auto-scheduled has
                  both fields `null` and this line stays hidden, never a
                  fabricated "—" placeholder. */}
              {detail.approved_by && (
                <span id="run-approval-attribution">
                  {detail.state === 'REJECTED' ? 'Rejected by' : 'Approved by'}:{' '}
                  <strong className="text-slate-200">{detail.approved_by}</strong> at{' '}
                  <strong className="text-slate-200">{formatTimestamp(detail.approved_at)}</strong>
                </span>
              )}
            </div>
          </div>
          <div className="flex items-center gap-2">
            {canApproveReject && (
              <>
                <Cta label="Approve" icon={CheckCircle2} onClick={handleApprove} disabled={actionPending} primary />
                <Cta label="Reject" icon={Ban} onClick={handleReject} disabled={actionPending} />
              </>
            )}
            {canCancel && <Cta label="Cancel" icon={Square} onClick={handleCancel} disabled={actionPending} />}
            {detail && canRetryFailed && (
              <Cta
                label={`Retry Failed (${detail.infra_error + detail.timeout})`}
                icon={RotateCcw}
                onClick={handleRetryFailed}
                disabled={actionPending}
              />
            )}
          </div>
        </div>

        {detail.policy_notes && (
          <div className="px-2.5 py-2 rounded-md bg-amber-950/30 border border-amber-900/60 text-amber-300 text-[11px]">
            {redactOwnKey(detail.policy_notes)}
          </div>
        )}
        {actionError && <div className="px-2.5 py-2 rounded-md bg-rose-950/30 border border-rose-900/60 text-rose-300 text-[11px]">{actionError.message}</div>}

        <div id="run-detail-count-grid" className="grid grid-cols-3 sm:grid-cols-4 lg:grid-cols-6 gap-2 text-[10px] font-mono">
          <div className="p-1.5 rounded bg-[#020617] border border-slate-800 text-center">
            <div className="text-content-muted">TOTAL</div>
            <div className="text-slate-200 font-bold">{detail.total}</div>
          </div>
          <div className="p-1.5 rounded bg-[#020617] border border-slate-800 text-center">
            <div className="text-content-muted">{formatStatusLabel('QUEUED')}</div>
            <div className="text-slate-400 font-bold">{detail.queued}</div>
          </div>
          <div className="p-1.5 rounded bg-[#020617] border border-slate-800 text-center">
            <div className="text-content-muted">{formatStatusLabel('RUNNING')}</div>
            <div className="text-yellow-400 font-bold">{detail.running}</div>
          </div>
          <div className="p-1.5 rounded bg-[#020617] border border-slate-800 text-center">
            <div className="text-content-muted">{formatStatusLabel('PASSED')}</div>
            <div className="text-emerald-400 font-bold">{detail.passed}</div>
          </div>
          <div className="p-1.5 rounded bg-[#020617] border border-slate-800 text-center">
            <div className="text-content-muted">{formatStatusLabel('FAILED')}</div>
            <div className="text-rose-400 font-bold">{detail.failed}</div>
          </div>
          <div className="p-1.5 rounded bg-[#020617] border border-slate-800 text-center">
            <div className="text-content-muted">{formatStatusLabel('BLOCKED')}</div>
            <div className="text-purple-400 font-bold">{detail.blocked}</div>
          </div>
          <div className="p-1.5 rounded bg-[#020617] border border-slate-800 text-center">
            <div className="text-content-muted">{formatStatusLabel('SKIPPED')}</div>
            <div className="text-slate-400 font-bold">{detail.skipped}</div>
          </div>
          <div className="p-1.5 rounded bg-[#020617] border border-slate-800 text-center">
            <div className="text-content-muted">{formatStatusLabel('CANCELLED')}</div>
            <div className="text-slate-400 font-bold">{detail.cancelled}</div>
          </div>
          <div className="p-1.5 rounded bg-[#020617] border border-slate-800 text-center">
            <div className="text-content-muted">{formatStatusLabel('TIMEOUT')}</div>
            <div className="text-orange-400 font-bold">{detail.timeout}</div>
          </div>
          <div className="p-1.5 rounded bg-[#020617] border border-slate-800 text-center">
            <div className="text-content-muted">{formatStatusLabel('INFRA_ERROR')}</div>
            <div className="text-orange-400 font-bold">{detail.infra_error}</div>
          </div>
          <div className="p-1.5 rounded bg-[#020617] border border-slate-800 text-center">
            <div className="text-content-muted">{formatStatusLabel('NEEDS_REVIEW')}</div>
            <div className="text-amber-400 font-bold">{detail.needs_review}</div>
          </div>
        </div>

        <div className="rounded-lg border border-slate-800 overflow-hidden">
          <table className="w-full text-left text-xs">
            <thead>
              <tr className="font-mono text-[10px] uppercase tracking-wider text-content-muted bg-[#020617] border-b border-slate-800">
                <th scope="col" className="py-2 px-2.5 font-normal">Task</th>
                <th scope="col" className="py-2 px-2.5 font-normal">Status</th>
                <th scope="col" className="py-2 px-2.5 font-normal">Reason</th>
                <th scope="col" className="py-2 px-2.5 font-normal">Started</th>
                <th scope="col" className="py-2 px-2.5 font-normal">Finished</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/60">
              {detail.tasks.map((t) => (
                <tr key={t.id}>
                  <td className="py-2 px-2.5">
                    <div className="font-mono text-xs font-semibold text-slate-100">{t.name}</div>
                    <div className="font-mono text-[10px] text-content-muted">{t.id}</div>
                  </td>
                  <td className="py-2 px-2.5">
                    <Badge label={formatStatusLabel(t.status)} className={TASK_STATUS_STYLE[t.status]} />
                  </td>
                  <td className="py-2 px-2.5 text-slate-400 max-w-xs truncate" title={redactOwnKey(t.reason) ?? undefined}>
                    {redactOwnKey(t.reason) ?? '—'}
                  </td>
                  <td className="py-2 px-2.5 font-mono text-[10px] text-slate-400 whitespace-nowrap">{formatTimestamp(t.started_at)}</td>
                  <td className="py-2 px-2.5 font-mono text-[10px] text-slate-400 whitespace-nowrap">{formatTimestamp(t.finished_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <div className="flex items-center gap-2">
          {hasEvidence && (
            <button
              type="button"
              onClick={() => navigate('/evidence')}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 font-medium text-xs border border-slate-700 transition-colors cursor-pointer"
            >
              <ExternalLink className="w-3.5 h-3.5" />
              View Evidence
            </button>
          )}
          <button
            type="button"
            onClick={() => navigate('/audit')}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 font-medium text-xs border border-slate-700 transition-colors cursor-pointer"
          >
            <ExternalLink className="w-3.5 h-3.5" />
            View Audit Log
          </button>
        </div>
      </div>
    );
  };

  const renderCreatePanel = () => (
    <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-3">
      <div>
        <label htmlFor="exec-run-name" className="text-[10px] font-mono uppercase text-content-muted">
          Execution name
        </label>
        <input
          id="exec-run-name"
          type="text"
          value={runName}
          onChange={(e) => setRunName(e.target.value)}
          placeholder="e.g. Regression Suite — Checkout"
          className="mt-1 w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-xs text-slate-200 outline-none focus:ring-1 focus:ring-violet-500 font-sans"
        />
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <span className="text-[10px] font-mono uppercase text-content-muted">Tasks</span>
          <button
            type="button"
            onClick={addDraftTask}
            className="flex items-center gap-1 text-[11px] text-violet-400 hover:text-violet-300 cursor-pointer"
          >
            <Plus className="w-3.5 h-3.5" />
            Add task
          </button>
        </div>
        {draftTasks.map((t) => (
          <div key={t.key} className="p-2 rounded-md bg-[#020617] border border-slate-800 space-y-1.5">
          <div className="flex flex-wrap items-center gap-2">
            <input
              type="text"
              aria-label="Task id"
              value={t.id}
              onChange={(e) => updateDraftTask(t.key, { id: e.target.value })}
              placeholder="task id"
              className="w-28 px-2 py-1 rounded bg-[#0B1120] border border-slate-800 text-[11px] font-mono text-slate-200 outline-none focus:ring-1 focus:ring-violet-500"
            />
            <input
              type="text"
              aria-label="Task name"
              value={t.name}
              onChange={(e) => updateDraftTask(t.key, { name: e.target.value })}
              placeholder="task name"
              className="flex-1 min-w-[120px] px-2 py-1 rounded bg-[#0B1120] border border-slate-800 text-[11px] text-slate-200 outline-none focus:ring-1 focus:ring-violet-500"
            />
            <select
              aria-label="Task executor"
              value={t.executor}
              onChange={(e) => updateDraftTask(t.key, { executor: e.target.value })}
              className="px-2 py-1 rounded bg-[#0B1120] border border-slate-800 text-[11px] text-slate-200 outline-none focus:ring-1 focus:ring-violet-500 cursor-pointer"
            >
              {EXECUTOR_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
            {!t.executor && (
              <select
                aria-label="Simulated outcome"
                value={t.simulateOutcome}
                onChange={(e) => updateDraftTask(t.key, { simulateOutcome: e.target.value as DraftTask['simulateOutcome'] })}
                className="px-2 py-1 rounded bg-[#0B1120] border border-slate-800 text-[11px] text-slate-200 outline-none focus:ring-1 focus:ring-violet-500 cursor-pointer"
              >
                {SIMULATE_OUTCOMES.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            )}
            <button
              type="button"
              onClick={() => removeDraftTask(t.key)}
              disabled={draftTasks.length === 1}
              className="p-1 rounded text-content-muted hover:text-rose-400 disabled:opacity-30 disabled:cursor-not-allowed cursor-pointer"
              title="Remove task"
            >
              <Trash2 className="w-3.5 h-3.5" />
            </button>
          </div>
          {/* FE-17 — real `TaskDef.prerequisites` exposure. Only shown once a
              second task exists to depend on; a task is never offered as its
              own candidate, so a self-dependency cannot be selected. */}
          {draftTasks.length > 1 && (
            <div id={`task-prereqs-${t.key}`} className="flex flex-wrap items-center gap-1.5 pl-1">
              <span className="text-[10px] font-mono text-content-muted uppercase">Depends on:</span>
              {draftTasks
                .filter((other) => other.key !== t.key)
                .map((other) => {
                  const selected = t.prerequisites.includes(other.key);
                  return (
                    <button
                      key={other.key}
                      type="button"
                      onClick={() => toggleDraftPrerequisite(t.key, other.key)}
                      aria-pressed={selected}
                      className={`px-2 py-0.5 rounded text-[10px] font-mono cursor-pointer border transition-colors ${
                        selected
                          ? 'bg-violet-950/80 border-violet-600 text-violet-200'
                          : 'bg-[#0B1120] border-slate-800 text-slate-400 hover:border-slate-700 hover:text-slate-200'
                      }`}
                    >
                      {other.id.trim() || other.key}
                    </button>
                  );
                })}
              {t.prerequisites.length === 0 && (
                <span className="text-[10px] text-content-muted">none — click a task above to add a dependency</span>
              )}
            </div>
          )}
          </div>
        ))}
        <p className="text-[10px] text-content-muted">
          Prerequisites map directly to the real backend <code>TaskDef.prerequisites</code>. The backend authoritatively
          validates unique task ids, unknown prerequisite references, and dependency cycles — this form does not duplicate
          that logic.
        </p>
      </div>

      <div>
        <label htmlFor="exec-concurrency" className="text-[10px] font-mono uppercase text-content-muted">
          Concurrency (optional — backend default applies if left blank)
        </label>
        <input
          id="exec-concurrency"
          type="number"
          min={1}
          value={concurrency}
          onChange={(e) => setConcurrency(e.target.value)}
          className="mt-1 w-32 px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-xs text-slate-200 outline-none focus:ring-1 focus:ring-violet-500 font-mono"
        />
      </div>

      {createError && <div className="px-2.5 py-2 rounded-md bg-rose-950/30 border border-rose-900/60 text-rose-300 text-[11px]">{createError}</div>}

      <div className="flex items-center gap-2 pt-1">
        <Cta label={creating ? 'Creating…' : 'Start Execution'} icon={Play} onClick={handleCreate} disabled={creating} primary />
        <Cta label="Cancel" icon={XCircle} onClick={() => setShowCreate(false)} disabled={creating} />
      </div>
    </div>
  );

  return (
    <div id="aiqa-execution-hub-page" className="space-y-4 animate-in fade-in duration-200 text-slate-200">
      <header
        id="execution-hub-header"
        className="flex flex-col md:flex-row md:items-center justify-between gap-3 p-4 rounded-lg bg-[#0B1120] border border-slate-800/90 shadow-sm"
      >
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <span className="p-1.5 rounded bg-violet-950/80 border border-violet-700/60 text-violet-400">
              <Play className="w-4 h-4" />
            </span>
            <h1 className="text-base font-bold text-slate-100 font-sans tracking-tight">Execution Hub</h1>
            <span className="px-2 py-0.5 rounded bg-violet-950/60 border border-violet-800/60 text-[10px] font-mono text-violet-300 uppercase">
              EXECUTION
            </span>
          </div>
          <p className="text-xs text-slate-400 font-sans">
            {selectedProject ? `Real executions for ${selectedProject.name}` : 'Real executions for the selected project'}
          </p>
        </div>
        <div className="flex items-center gap-2 self-start md:self-center">
          {selectedProjectId && (
            <span id="execution-realtime-indicator" className="flex items-center gap-1.5" title="Realtime connection state">
              {realtime.connectionState === 'connected' ? (
                <Wifi className="w-3.5 h-3.5 text-emerald-400" />
              ) : (
                <WifiOff className="w-3.5 h-3.5 text-content-muted" />
              )}
              <Badge label={REALTIME_STATE_LABEL[realtime.connectionState]} className={REALTIME_STATE_STYLE[realtime.connectionState]} />
              {realtime.connectionState === 'error' && (
                <button
                  type="button"
                  id="execution-realtime-reconnect-button"
                  onClick={() => realtime.reconnect()}
                  className="text-[10px] font-mono uppercase text-violet-300 hover:text-violet-200 underline cursor-pointer"
                >
                  Reconnect
                </button>
              )}
            </span>
          )}
          <button
            type="button"
            id="execution-refresh-button"
            onClick={handleRefreshRuns}
            disabled={!selectedProjectId || runsLoading || runsRefreshing}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 font-medium text-xs border border-slate-700 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <RotateCcw className={`w-3.5 h-3.5 ${runsRefreshing ? 'animate-spin' : ''}`} />
            <span>Refresh</span>
          </button>
          <button
            type="button"
            id="execution-new-button"
            onClick={() => setShowCreate((v) => !v)}
            disabled={!selectedProjectId}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 text-white font-semibold text-xs shadow-sm shadow-violet-950/60 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <Play className="w-3.5 h-3.5 fill-current" />
            <span>New Execution</span>
          </button>
        </div>
      </header>

      {!selectedProjectId ? (
        <EmptyState
          icon={FolderOpen}
          title="No project selected"
          description="Choose a project from the selector in the header to view and start real executions."
        />
      ) : (
        <>
          {showCreate && renderCreatePanel()}
          <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-start">
            <section id="execution-list" className="lg:col-span-5 p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm">
              {renderRunList()}
            </section>
            <section id="execution-detail" className="lg:col-span-7 p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm">
              {renderDetail()}
            </section>
          </div>
        </>
      )}
    </div>
  );
};
