use crate::domain::*;
use crate::evidence::LocalEvidenceStore;
use crate::executor::{ExecutionOutcome, ExecutorRegistry};
use crate::intelligence::Intelligence;
use crate::policy::PolicyConfig;
use crate::scheduler::RunEngine;
use crate::store::Store;
use crate::webhook;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, watch, RwLock, Semaphore};
use uuid::Uuid;

/// BE-14 (§14, Integration Plan §11.2) — one project's realtime event
/// fan-out: a broadcast channel every run in that project publishes into
/// (in addition to its own per-run channel), plus a sequence counter that
/// is monotonic *across* that project's runs, not per-run — see
/// `RunEvent::project_sequence`'s doc comment for why a shared counter is
/// necessary (two concurrent runs' own per-run sequences both start at 0
/// and would collide as a merge key on a multiplexed project stream).
/// Owned by `AppState`, created lazily on first use
/// (`AppState::project_stream`), and handed to each `RunRecord` at
/// construction so `emit` can publish into it directly — no polling, no
/// second event-creation path.
pub struct ProjectStream {
    pub tx: broadcast::Sender<RunEvent>,
    seq: AtomicU64,
}

impl Default for ProjectStream {
    fn default() -> Self {
        let (tx, _rx) = broadcast::channel(1024);
        Self {
            tx,
            seq: AtomicU64::new(0),
        }
    }
}

/// All server-side state for one run (SRD §2 Orchestrator State Machine +
/// SRD §6 event stream). `engine` holds the pure scheduler state from
/// `scheduler.rs`; everything else here is I/O plumbing around it —
/// including, as of this session, the durable side of that plumbing
/// (`store.rs`), not just the in-memory/broadcast side.
impl RunRecord {
    /// The in-memory events at or after `since`, or `None` when the answer is no
    /// longer complete.
    ///
    /// `None` is the honest response once the retained window has moved past
    /// `since`: the caller must then read `run_events` from the store, where every
    /// event was durably written. Returning a truncated list instead would make a
    /// client that reconnects with `since=0` silently receive only the tail, with
    /// no indication that anything was dropped -- which is worse than a database
    /// read, because the client cannot tell the difference.
    pub async fn events_after(&self, since: u64) -> Option<Vec<RunEvent>> {
        let events = self.events.read().await;
        let oldest_retained = match events.first() {
            Some(first) => first.sequence,
            // Nothing retained. That is complete only if the caller wanted nothing
            // newer than the very first sequence, which is unknowable here, so it
            // is treated as incomplete and deferred to the store.
            None => return None,
        };
        if since < oldest_retained.saturating_sub(1) {
            return None;
        }
        Some(
            events
                .iter()
                .filter(|e| e.sequence >= since)
                .cloned()
                .collect(),
        )
    }
}

/// How many `RunEvent`s a `RunRecord` keeps in memory.
///
/// A cap, not a budget: `RunRecord::emit` persists every event to the
/// `run_events` table through `Store::insert_event`, so this bounds only how much
/// is held resident per live run. Generous enough that an ordinary run never
/// notices -- `events_after` degrades to a database read rather than anything
/// visible -- while stopping a pathological run from growing without bound.
pub const MAX_IN_MEMORY_EVENTS: usize = 2048;

pub struct RunRecord {
    pub run_id: RunId,
    pub name: String,
    pub plan: RunPlan,
    /// `None` = untenanted/global run. See `intelligence::tenant_key` and
    /// `auth::require_project_access` for how this is used downstream.
    pub project_id: Option<String>,
    pub requested_by: Option<String>,
    pub state: RwLock<RunState>,
    pub engine: RwLock<RunEngine>,
    pub events: RwLock<Vec<RunEvent>>,
    pub tx: broadcast::Sender<RunEvent>,
    /// BE-14 (§14) — the project-scoped event bus this run's events also
    /// fan out to, alongside `tx` above. Resolved once by the caller (from
    /// `AppState::project_stream`, keyed by `project_id`) and handed in at
    /// construction; `None` for a run with no project (nothing to fan out
    /// to) or for records built without an owning `AppState` at all (most
    /// of `engine_tests.rs`) — such a run simply never appears on any
    /// project stream, which is the correct behavior, not a gap.
    project_stream: Option<Arc<ProjectStream>>,
    pub log_bus: Option<Arc<crate::log_bus::LogBus>>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: RwLock<Option<chrono::DateTime<chrono::Utc>>>,
    pub policy_notes: RwLock<Option<String>>,
    pub approved_by: RwLock<Option<String>>,
    pub approved_at: RwLock<Option<chrono::DateTime<chrono::Utc>>>,
    seq: AtomicU64,
    /// B1 — first durable-write failure observed while driving this run.
    ///
    /// The run loop cannot propagate an error upward (it is a detached
    /// `tokio::spawn`), and it must not simply log and carry on: that is
    /// exactly what let a completed run persist as FAILED and passing tasks
    /// persist as RUNNING. So a persistence failure is recorded here, and
    /// finalisation reads it to transition the run into an explicit,
    /// auditable failure state instead of reporting success over a database
    /// that disagrees.
    persistence_error: RwLock<Option<String>>,
    /// BE-13 (§12.3) — cooperative cancellation signal. `false` until
    /// `request_cancel` is called. The dispatch loop in `spawn_run` checks
    /// it before dispatching any new task, and every in-flight task's
    /// executor future races it via `tokio::select!`; when cancellation
    /// wins, the executor future is dropped, which is the exact mechanism
    /// B5's own timeout already relies on (`kill_on_drop`) to make sure a
    /// real subprocess actually dies rather than merely being relabeled
    /// while it keeps running in the background.
    cancel: watch::Sender<bool>,
    cancel_requested_by: RwLock<Option<String>>,
}

impl RunRecord {
    pub fn new(
        run_id: RunId,
        plan: &RunPlan,
        project_id: Option<String>,
        requested_by: Option<String>,
        initial_state: RunState,
        policy_notes: Option<String>,
        project_stream: Option<Arc<ProjectStream>>,
    ) -> Arc<Self> {
        Self::new_with_log_bus(
            run_id,
            plan,
            project_id,
            requested_by,
            initial_state,
            policy_notes,
            project_stream,
            None,
        )
    }

    pub fn new_with_log_bus(
        run_id: RunId,
        plan: &RunPlan,
        project_id: Option<String>,
        requested_by: Option<String>,
        initial_state: RunState,
        policy_notes: Option<String>,
        project_stream: Option<Arc<ProjectStream>>,
        log_bus: Option<Arc<crate::log_bus::LogBus>>,
    ) -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(1024);
        let (cancel, _cancel_rx) = watch::channel(false);
        Arc::new(Self {
            run_id,
            name: plan.name.clone(),
            plan: plan.clone(),
            project_id,
            requested_by,
            state: RwLock::new(initial_state),
            engine: RwLock::new(RunEngine::new(plan)),
            events: RwLock::new(Vec::new()),
            tx,
            project_stream,
            log_bus,
            started_at: chrono::Utc::now(),
            finished_at: RwLock::new(None),
            policy_notes: RwLock::new(policy_notes),
            approved_by: RwLock::new(None),
            approved_at: RwLock::new(None),
            seq: AtomicU64::new(0),
            persistence_error: RwLock::new(None),
            cancel,
            cancel_requested_by: RwLock::new(None),
        })
    }

    pub fn is_cancel_requested(&self) -> bool {
        *self.cancel.borrow()
    }

    /// Signals cancellation to `spawn_run`'s dispatch loop and every
    /// in-flight task future. Safe to call more than once (idempotent —
    /// `watch::Sender::send` just republishes `true`); the API handler is
    /// still responsible for rejecting a second `/cancel` call against an
    /// already-terminal run via the state check, so this alone is not the
    /// idempotency boundary.
    pub async fn request_cancel(&self, actor: &str) {
        *self.cancel_requested_by.write().await = Some(actor.to_string());
        let _ = self.cancel.send(true);
    }

    /// B1 — record a durable-write failure. Keeps the first one (the
    /// root cause; later failures are usually the same contention) and logs
    /// every occurrence so the operator sees the full extent.
    pub async fn note_persistence_failure(&self, context: &str, err: impl std::fmt::Display) {
        let message = format!("{context}: {err}");
        tracing::error!("run {}: durable write failed — {message}", self.run_id);
        let mut slot = self.persistence_error.write().await;
        if slot.is_none() {
            *slot = Some(message);
        }
    }

    pub async fn persistence_error(&self) -> Option<String> {
        self.persistence_error.read().await.clone()
    }

    pub fn task_defs(&self) -> HashMap<TaskId, TaskDef> {
        self.plan
            .tasks
            .iter()
            .map(|t| (t.id.clone(), t.clone()))
            .collect()
    }

    /// Append + broadcast one event (SRD §5: event_id, run_id, task_id,
    /// sequence, timestamp, schema_version on every event; the `seq` counter
    /// here is the single writer that makes ordering well-defined), and
    /// persist it durably. A DB write failure is logged, not fatal — the
    /// event is already in `events`/broadcast for anyone watching live; see
    /// README "Persistence" for the tradeoff this implies (a crash between
    /// the in-memory append and the DB write can, in principle, lose the
    /// last event or two — full write-ahead durability was out of scope for
    /// this pass).
    pub async fn emit(
        &self,
        store: &Store,
        event_type: EventType,
        task_id: Option<TaskId>,
        status: Option<TaskStatus>,
        summary: Option<String>,
    ) {
        let sequence = self.seq.fetch_add(1, Ordering::SeqCst);
        let ev = RunEvent {
            event_id: Uuid::new_v4(),
            run_id: self.run_id,
            task_id,
            sequence,
            timestamp: chrono::Utc::now(),
            schema_version: 1,
            event_type,
            status,
            summary,
            project_id: self.project_id.clone(),
            project_sequence: None,
        };
        {
            let mut events = self.events.write().await;
            events.push(ev.clone());
            // Trim to the most recent `MAX_IN_MEMORY_EVENTS`. This vec was
            // push-only with no cap anywhere in the crate, so a long run kept
            // every event it ever emitted, and the events endpoint cloned the
            // whole thing on every poll.
            //
            // Nothing is lost: `emit` persists every event to `run_events` via
            // `Store::insert_event` a few lines below, and `events_after` makes
            // the in-memory read return `None` once the retained window no longer
            // reaches back to the requested `since`, so the caller falls back to
            // the store rather than silently answering a `since=0` request with
            // only the tail. Trimming silently would have been a quiet data-loss
            // bug dressed as a memory optimisation.
            if events.len() > MAX_IN_MEMORY_EVENTS {
                let excess = events.len() - MAX_IN_MEMORY_EVENTS;
                events.drain(0..excess);
            }
        }
        // A send error just means no WebSocket subscribers are currently
        // connected; the event is already durably in `events` for replay.
        let _ = self.tx.send(ev.clone());
        // BE-14 — same real event, additionally fanned out to this
        // project's realtime stream with its own project-wide sequence
        // number (never the run-local `sequence` above — see
        // `RunEvent::project_sequence`). `ev` itself (persisted below) keeps
        // `project_sequence: None`; only the published copy carries it,
        // since it's meaningless outside a project-stream context.
        if let Some(ps) = &self.project_stream {
            let mut project_ev = ev.clone();
            project_ev.project_sequence = Some(ps.seq.fetch_add(1, Ordering::SeqCst));
            let _ = ps.tx.send(project_ev);
        }
        if let Some(lb) = &self.log_bus {
            let (lvl, src, msg) = match &ev.event_type {
                EventType::RunScheduled => (
                    "INFO",
                    "execution",
                    format!(
                        "Execution run '{}' scheduled with {} task(s)",
                        self.name,
                        self.plan.tasks.len()
                    ),
                ),
                EventType::TaskStarted => (
                    "INFO",
                    "execution",
                    format!(
                        "Task '{}' started on executor",
                        ev.task_id.as_deref().unwrap_or("unknown")
                    ),
                ),
                EventType::TaskPassed => (
                    "SUCCESS",
                    "execution",
                    format!(
                        "Task '{}' passed successfully",
                        ev.task_id.as_deref().unwrap_or("unknown")
                    ),
                ),
                EventType::TaskFailed => (
                    "ERROR",
                    "execution",
                    format!(
                        "Task '{}' failed: {}",
                        ev.task_id.as_deref().unwrap_or("unknown"),
                        ev.summary.as_deref().unwrap_or("task failed")
                    ),
                ),
                EventType::TaskBlocked => (
                    "WARN",
                    "execution",
                    format!(
                        "Task '{}' blocked by dependency",
                        ev.task_id.as_deref().unwrap_or("unknown")
                    ),
                ),
                EventType::TaskSkipped => (
                    "INFO",
                    "execution",
                    format!(
                        "Task '{}' skipped",
                        ev.task_id.as_deref().unwrap_or("unknown")
                    ),
                ),
                EventType::TaskCancelled => (
                    "WARN",
                    "execution",
                    format!(
                        "Task '{}' cancelled",
                        ev.task_id.as_deref().unwrap_or("unknown")
                    ),
                ),
                EventType::TaskTimeout => (
                    "ERROR",
                    "execution",
                    format!(
                        "Task '{}' timed out",
                        ev.task_id.as_deref().unwrap_or("unknown")
                    ),
                ),
                EventType::TaskInfraError => (
                    "ERROR",
                    "execution",
                    format!(
                        "Task '{}' infrastructure error: {}",
                        ev.task_id.as_deref().unwrap_or("unknown"),
                        ev.summary.as_deref().unwrap_or("error")
                    ),
                ),
                EventType::TaskNeedsReview => (
                    "WARN",
                    "execution",
                    format!(
                        "Task '{}' needs review: {}",
                        ev.task_id.as_deref().unwrap_or("unknown"),
                        ev.summary.as_deref().unwrap_or("review required")
                    ),
                ),
                EventType::RunCompleted => {
                    let st = *self.state.read().await;
                    let (lvl, verdict) = match st {
                        RunState::Completed => ("SUCCESS", "completed successfully"),
                        RunState::Failed => ("ERROR", "finished with failures"),
                        RunState::Cancelled => ("WARN", "cancelled"),
                        RunState::Partial => ("WARN", "completed with partial failures"),
                        _ => ("INFO", "completed"),
                    };
                    (
                        lvl,
                        "execution",
                        format!("Execution run '{}' {verdict}", self.name),
                    )
                }
            };
            let run_id_str = self.run_id.to_string();
            let proj_id_str = self.project_id.as_deref();
            let task_id_str = ev.task_id.as_deref();
            lb.emit(lvl, src, &msg, Some(&run_id_str), proj_id_str, task_id_str)
                .await;
        }
        if let Err(e) = store.insert_event(&ev).await {
            // B1 — a lost event leaves a permanent gap in a log SRD §5
            // requires to be complete and gap-free, so this is escalated
            // rather than logged and forgotten.
            self.note_persistence_failure(&format!("insert_event({})", ev.event_id), e)
                .await;
        }
    }

    pub async fn summary(&self) -> RunSummary {
        let engine = self.engine.read().await;
        let counts = engine.counts();
        let get = |s: TaskStatus| counts.get(&s).copied().unwrap_or(0);
        RunSummary {
            run_id: self.run_id,
            name: self.name.clone(),
            state: *self.state.read().await,
            total: engine.tasks.len(),
            queued: get(TaskStatus::Queued),
            running: get(TaskStatus::Running),
            passed: get(TaskStatus::Passed),
            failed: get(TaskStatus::Failed),
            blocked: get(TaskStatus::Blocked),
            skipped: get(TaskStatus::Skipped),
            cancelled: get(TaskStatus::Cancelled),
            timeout: get(TaskStatus::Timeout),
            infra_error: get(TaskStatus::InfraError),
            needs_review: get(TaskStatus::NeedsReview),
            started_at: self.started_at,
            finished_at: *self.finished_at.read().await,
            project_id: self.project_id.clone(),
            requested_by: self.requested_by.clone(),
            policy_notes: self.policy_notes.read().await.clone(),
            approved_by: self.approved_by.read().await.clone(),
            approved_at: *self.approved_at.read().await,
        }
    }
}

/// B2 — the single place a run's terminal state is derived from its tasks.
///
/// Used by recovery so a restarted process reaches the same verdict the
/// live driver would have. Vocabulary is Phase 1's existing one (no status
/// renames in this phase):
///
///   * any `CANCELLED` task (BE-13) -> `Cancelled`, checked first — see the
///     inline comment below for why this outranks every other case;
///   * any infrastructure outcome (`INFRA_ERROR` / `TIMEOUT`) -> `Partial`,
///     because part of the run never produced a real product verdict;
///   * otherwise any product failure (`FAILED`) -> `Failed`;
///   * otherwise any `BLOCKED` task -> `Partial`, since some declared work
///     never ran even though nothing failed outright;
///   * otherwise -> `Completed`.
///
/// `Blocked` is deliberately checked *after* `Failed`: a blocked task is
/// almost always the consequence of a failed prerequisite, and the failure
/// is the more informative verdict.
fn derive_run_state(tasks: &[TaskState]) -> RunState {
    let has = |s: TaskStatus| tasks.iter().any(|t| t.status == s);

    // BE-13 — a cancelled task only ever exists because a human explicitly
    // requested cancellation (nothing else produces this status), so it
    // takes priority over every other classification: a run interrupted
    // mid-flight should report CANCELLED, not PARTIAL/FAILED, even if the
    // interrupted task's executor happened to be mid-timeout or some other
    // task had already failed for unrelated reasons. Checked first (not
    // just before the `else`) so it also wins recovery's read of this same
    // function after a restart — see `RunRecord::cancel`'s doc comment for
    // why that in-memory signal alone isn't durable, but a persisted
    // CANCELLED task status is.
    if has(TaskStatus::Cancelled) {
        RunState::Cancelled
    } else if has(TaskStatus::InfraError) || has(TaskStatus::Timeout) {
        RunState::Partial
    } else if has(TaskStatus::Failed) {
        RunState::Failed
    } else if has(TaskStatus::Blocked) {
        RunState::Partial
    } else {
        RunState::Completed
    }
}

/// Persists the final status of every task `complete_task` just made
/// terminal and emits its event — the shared tail end of "a task reached a
/// terminal state," used both by a real executor completion (`spawn_run`'s
/// `done_rx` branch) and by BE-13's cancellation path, which drives tasks
/// straight to `Cancelled` without ever dispatching them. Factored out so
/// cancellation reuses exactly the same persist/emit code a normal
/// completion already uses, rather than a second copy of it.
async fn persist_and_emit_terminal(
    record: &Arc<RunRecord>,
    store: &Store,
    newly_terminal: Vec<TaskId>,
) {
    for tid in newly_terminal {
        let (status, reason) = {
            let engine = record.engine.read().await;
            let t = &engine.tasks[&tid];
            (t.status, t.reason.clone())
        };
        if let Err(e) = store
            .set_task_status(record.run_id, &tid, status, reason.as_deref(), false, true)
            .await
        {
            record
                .note_persistence_failure(&format!("set_task_status(final, {tid})"), e)
                .await;
        }
        record
            .emit(
                store,
                event_type_for(status),
                Some(tid),
                Some(status),
                reason,
            )
            .await;
    }
}

fn event_type_for(status: TaskStatus) -> EventType {
    match status {
        TaskStatus::Passed => EventType::TaskPassed,
        TaskStatus::Failed => EventType::TaskFailed,
        TaskStatus::Blocked => EventType::TaskBlocked,
        TaskStatus::Skipped => EventType::TaskSkipped,
        TaskStatus::Cancelled => EventType::TaskCancelled,
        TaskStatus::Timeout => EventType::TaskTimeout,
        TaskStatus::InfraError => EventType::TaskInfraError,
        TaskStatus::NeedsReview => EventType::TaskNeedsReview,
        TaskStatus::Queued | TaskStatus::Running => {
            unreachable!("event_type_for is only called with terminal statuses")
        }
    }
}

/// A run held by the policy engine (`policy.rs`) for human approval is
/// released here: mark it approved (durably + in-memory) and hand it to
/// `spawn_run` exactly as an auto-scheduled run would be. Plain `async fn`
/// (not spawned) so callers in `api.rs` can await the approval bookkeeping
/// itself and return an accurate post-approval summary; `spawn_run` inside
/// still runs the actual execution loop in the background as usual.
pub async fn approve_and_spawn(
    record: Arc<RunRecord>,
    registry: Arc<ExecutorRegistry>,
    concurrency: usize,
    store: Store,
    evidence: Arc<LocalEvidenceStore>,
    policy: Arc<PolicyConfig>,
    actor: &str,
    task_timeout: std::time::Duration,
) -> anyhow::Result<()> {
    if let Err(e) = store.approve_run(record.run_id, actor).await {
        record.note_persistence_failure("approve_run", &e).await;
        return Err(e);
    }
    *record.approved_by.write().await = Some(actor.to_string());
    *record.approved_at.write().await = Some(chrono::Utc::now());
    // Synchronous, awaited here -- so the caller (the approve HTTP handler)
    // can never observe a stale PENDING_APPROVAL, nor a state the dispatch
    // loop already raced past, when it reads the run back right after this
    // returns. See `mark_run_running`'s doc comment.
    mark_run_running(&record, &store).await;
    spawn_run(
        record,
        registry,
        concurrency,
        store,
        evidence,
        policy,
        task_timeout,
    );
    Ok(())
}

/// The other side of the approval gate: a held run that a human declines.
/// It never enters the scheduler loop — its tasks stay QUEUED in the DB
/// forever, which is fine, they were never real (SRD §25 semantics only
/// apply to scheduled runs).
pub async fn reject_run(
    record: Arc<RunRecord>,
    store: &Store,
    actor: &str,
    reason: &str,
) -> anyhow::Result<()> {
    if let Err(e) = store.reject_run(record.run_id, actor, reason).await {
        record.note_persistence_failure("reject_run", &e).await;
        return Err(e);
    }
    *record.state.write().await = RunState::Rejected;
    *record.finished_at.write().await = Some(chrono::Utc::now());
    *record.approved_by.write().await = Some(actor.to_string());
    *record.approved_at.write().await = Some(chrono::Utc::now());
    *record.policy_notes.write().await = Some(reason.to_string());
    record
        .emit(
            store,
            EventType::RunCompleted,
            None,
            None,
            Some(format!("run rejected by {actor}: {reason}")),
        )
        .await;
    Ok(())
}

/// Run recovery on startup — build report: "Observability, audit log,
/// notifications, run recovery: None implemented," and separately: "a
/// restart loses every run" under the old in-memory-only design. Now that
/// runs/tasks persist (`store.rs`), a process restart can *see* what was
/// left mid-flight, but this slice does not attempt to resume execution
/// (no durable work queue exists to redeliver an in-flight subprocess
/// invocation to — that's the "Redis task coordination: Not integrated"
/// gap, still open, see README). What it does instead is the honest thing:
/// any run/task the DB shows as non-terminal when the process starts *must*
/// have been orphaned by an unclean shutdown (a clean shutdown has no other
/// way to leave one non-terminal), so it's marked INFRA_ERROR/FAILED with a
/// reason that says exactly that, rather than silently staying QUEUED
/// forever or being re-run blind.
///
/// B11/L4 remediation, and the more severe half of it: a `PENDING_APPROVAL`
/// run's tasks sit `QUEUED` *by design* — they were never spawned, because
/// the run is waiting on a human decision, not because a shutdown
/// interrupted them. `list_incomplete_runs` (correctly) surfaces these runs
/// too, since `QUEUED` tasks make them look non-terminal — but this loop
/// used to treat that exactly like an orphaned in-flight run and force
/// every one of those tasks to `INFRA_ERROR`, which flipped the whole run
/// FAILED. That meant *every* clean restart silently destroyed *every*
/// pending approval, which is a strictly worse outcome than the documented
/// finding ("a restart voids pending approvals," implying merely
/// inaccessible, not actively converted to a failure). Skipped here,
/// entirely untouched; `api.rs::rehydrate_run_record` is what makes a
/// still-`PENDING_APPROVAL` run reachable again after a restart.
pub async fn recover_incomplete_runs(store: &Store) -> anyhow::Result<usize> {
    let ids = store.list_incomplete_runs().await?;
    let reason = "orchestrator process restarted while this task was queued/running; execution state was not durable enough to resume, so it is marked INFRA_ERROR rather than left QUEUED forever";
    for run_id in &ids {
        if let Some(summary) = store.get_run_summary(*run_id).await? {
            if summary.state == RunState::PendingApproval {
                continue;
            }
        }
        let tasks = store.list_tasks_for_run(*run_id).await?;
        let mut touched_any = false;
        for t in &tasks {
            if matches!(t.status, TaskStatus::Queued | TaskStatus::Running) {
                store
                    .set_task_status(
                        *run_id,
                        &t.id,
                        TaskStatus::InfraError,
                        Some(reason),
                        false,
                        true,
                    )
                    .await?;
                touched_any = true;
            }
        }
        // B2 — derive the run's final state from what the tasks actually
        // say, re-read after any orphan was forced terminal.
        //
        // The old rule was `if touched_any { Partial } else { Failed }`,
        // which is inverted for the most important case: when recovery had
        // to force *nothing*, every task was already terminal and only the
        // run-level completion write was lost (the B1 scenario) — that is a
        // run that finished, not one that failed. It was being recorded as
        // FAILED, and a 50-task run in which all 50 tasks passed was
        // permanently stored as a failure.
        let tasks_after = store.list_tasks_for_run(*run_id).await?;
        let final_state = derive_run_state(&tasks_after);
        let _ = touched_any; // retained only for the log line below
        store.set_run_finished(*run_id, final_state, None).await?;
        let sequence = tasks.len() as u64 + 1000; // best-effort tail marker; exact ordering isn't load-bearing here
        let ev = RunEvent {
            event_id: Uuid::new_v4(),
            run_id: *run_id,
            task_id: None,
            sequence,
            timestamp: chrono::Utc::now(),
            schema_version: 1,
            event_type: EventType::RunCompleted,
            status: None,
            summary: Some(format!("run recovered at startup: {reason}")),
            // No live RunRecord/project stream exists at startup recovery
            // time to resolve or publish to — see `RunEvent`'s doc comment.
            project_id: None,
            project_sequence: None,
        };
        store.insert_event(&ev).await?;
    }
    Ok(ids.len())
}

/// Drives one run to completion in the background (SRD §3 Scheduler Design +
/// FRD §1 Execution Lifecycle): dispatch every currently-eligible task,
/// react to completions by updating the DAG (which may cascade BLOCKED
/// statuses per FRD §4), then dispatch whatever just became newly eligible —
/// repeating until every task is terminal. A failing task never stops this
/// loop for unrelated tasks (FRD §3 Continue-on-Failure Requirement).
///
/// Beyond the original slice, this now also: persists every state
/// transition (`store`), captures full executor stdout/stderr as evidence
/// (`evidence`), and — on completion — feeds the run into the heuristic
/// intelligence layer and fires the completion webhook.
/// Per-task execution ceiling (B5). Configurable so a slow real suite is not
/// cut off, while a genuinely hung tool still cannot stall a run forever.
pub fn default_task_timeout() -> std::time::Duration {
    let secs = std::env::var("AIQA_TASK_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|s| *s > 0)
        .unwrap_or(600);
    std::time::Duration::from_secs(secs)
}

/// The transition a run makes the instant it starts executing: in-memory
/// state, durable state, the started timestamp, and the real `RunScheduled`
/// event. Idempotent (checks the current state first) because it is called
/// from two places that can legitimately race to be first: a caller that
/// wants this to have *already happened* before it reads the run back (see
/// `approve_and_spawn`, whose HTTP response must never race the dispatch
/// loop below to report a stale `PENDING_APPROVAL` or an already-progressed
/// terminal state instead of the `RUNNING` it just caused), and `spawn_run`
/// itself for callers that don't need that guarantee up front.
async fn mark_run_running(record: &Arc<RunRecord>, store: &Store) {
    if *record.state.read().await == RunState::Running {
        return;
    }
    *record.state.write().await = RunState::Running;
    if let Err(e) = store.set_run_state(record.run_id, RunState::Running).await {
        record
            .note_persistence_failure("set_run_state(Running)", e)
            .await;
    }
    if let Err(e) = store.set_run_started(record.run_id).await {
        record.note_persistence_failure("set_run_started", e).await;
    }
    record
        .emit(
            store,
            EventType::RunScheduled,
            None,
            None,
            Some(format!("run '{}' scheduled", record.name)),
        )
        .await;
}

pub fn spawn_run(
    record: Arc<RunRecord>,
    registry: Arc<ExecutorRegistry>,
    concurrency: usize,
    store: Store,
    evidence: Arc<LocalEvidenceStore>,
    policy: Arc<PolicyConfig>,
    task_timeout: std::time::Duration,
) {
    let task_defs = record.task_defs();
    tokio::spawn(async move {
        mark_run_running(&record, &store).await;

        let semaphore = Arc::new(Semaphore::new(concurrency.max(1)));
        let (done_tx, mut done_rx) =
            tokio::sync::mpsc::unbounded_channel::<(TaskId, crate::executor::ExecutionOutcome)>();
        let mut in_flight = 0usize;
        // Phase 10: the real selector each failed browser task ended on, read from its own
        // captured output as it completes (see `executor::unresolved_healing_selector`).
        // Held in memory for the autonomous-healing pass after the run; nothing else reads it.
        let mut failed_selectors: HashMap<TaskId, String> = HashMap::new();

        loop {
            // B4 — a task is marked started only once it actually holds a
            // concurrency permit and is about to execute.
            //
            // Previously every eligible task was marked started in one batch
            // *before* any permit was acquired, so with concurrency below the
            // eligible count they all recorded the same `started_at` and
            // their durations silently included queue wait: five 600ms tasks
            // at concurrency 1 reported identical start instants and 2.5-3.1s
            // durations. That timing also feeds `execution_cost_ms`, which is
            // 15% of the risk score, so the corruption reached the
            // intelligence layer.
            //
            // `try_acquire_owned` (rather than awaiting a permit) keeps the
            // loop responsive: we dispatch only what can genuinely start now
            // and immediately return to draining completions, so TASK_PASSED
            // events are not held back behind a blocked dispatch.
            let eligible = { record.engine.read().await.eligible() };

            if record.is_cancel_requested() {
                // BE-13 — nothing further is dispatched once cancellation is
                // requested. Every still-QUEUED task (never acquired a
                // permit, never ran) goes straight to CANCELLED through the
                // same terminal-completion path a real outcome would use, so
                // dependents cascade-BLOCK exactly like a failed prerequisite
                // already would (`complete_task`'s existing cascade — see
                // `TaskStatus::satisfies_dependents`). In-flight tasks are
                // handled below, not here — they still hold a permit and are
                // racing their own executor future against the same signal.
                for id in eligible {
                    let newly_terminal = {
                        let mut engine = record.engine.write().await;
                        engine.complete_task(
                            &id,
                            TaskStatus::Cancelled,
                            Some("run cancelled before this task started".to_string()),
                        )
                    };
                    persist_and_emit_terminal(&record, &store, newly_terminal).await;
                }
            } else {
                for id in eligible {
                    let Ok(permit) = semaphore.clone().try_acquire_owned() else {
                        break; // at capacity — the rest wait for a completion
                    };

                    {
                        let mut engine = record.engine.write().await;
                        engine.mark_started(&id);
                    }
                    record
                        .emit(
                            &store,
                            EventType::TaskStarted,
                            Some(id.clone()),
                            Some(TaskStatus::Running),
                            None,
                        )
                        .await;
                    if let Err(e) = store
                        .set_task_status(record.run_id, &id, TaskStatus::Running, None, true, false)
                        .await
                    {
                        record
                            .note_persistence_failure(&format!("set_task_status(started, {id})"), e)
                            .await;
                    }

                    in_flight += 1;
                    let task_def = task_defs[&id].clone();
                    // Phase 2.1 (brief §13) — `resolve` now returns a real
                    // `Err` for an unregistered `executor` name instead of
                    // silently substituting `SimulatedExecutor`; that error
                    // is captured here and turned into a real, honest
                    // `InfraError` outcome below, without ever touching a
                    // real (or simulated) executor.
                    let resolved = registry.resolve(&task_def);
                    let done_tx = done_tx.clone();
                    // BE-13 — races the executor invocation against
                    // cancellation, the same way B5 already races it against
                    // a timeout (both rely on dropping this future to kill a
                    // real subprocess via `kill_on_drop`; see
                    // `RunRecord::cancel`'s doc comment).
                    let mut cancel_rx = record.cancel.subscribe();
                    tokio::spawn(async move {
                        let outcome = match resolved {
                            Err(msg) => ExecutionOutcome {
                                status: TaskStatus::InfraError,
                                reason: Some(format!("NO_COMPATIBLE_EXECUTOR: {msg}")),
                                evidence: None,
                            },
                            Ok(exec) => tokio::select! {
                                _ = cancel_rx.wait_for(|v| *v) => ExecutionOutcome {
                                    status: TaskStatus::Cancelled,
                                    reason: Some("run cancelled while this task was executing".to_string()),
                                    evidence: None,
                                },
                                // B5 — bound every executor invocation. Without this a
                                // hung tool (a browser waiting on a device, a fuzzer
                                // against an unresponsive target) held its permit
                                // forever, `in_flight` never decremented, and the run
                                // stalled indefinitely. TaskStatus::Timeout was
                                // unreachable for real executors.
                                result = tokio::time::timeout(task_timeout, exec.execute(&task_def)) => match result {
                                    Ok(outcome) => outcome,
                                    Err(_) => ExecutionOutcome {
                                        status: TaskStatus::Timeout,
                                        reason: Some(format!(
                                            "executor exceeded the {}s task timeout and was cancelled",
                                            task_timeout.as_secs()
                                        )),
                                        evidence: None,
                                    },
                                },
                            },
                        };
                        let _ = done_tx.send((task_def.id, outcome));
                        drop(permit); // released on cancel/timeout too, so the run continues
                    });
                }
            }

            if in_flight == 0 {
                // Nothing running and nothing left to dispatch this pass.
                break;
            }

            match done_rx.recv().await {
                Some((id, outcome)) => {
                    in_flight -= 1;

                    if let Some(ev_text) = outcome.evidence.as_deref() {
                        if !ev_text.trim().is_empty() {
                            match evidence.put(&record.run_id.to_string(), &id, "log", ev_text) {
                                Ok((object_key, size)) => {
                                    if let Err(e) = store
                                        .insert_evidence(
                                            record.run_id,
                                            &id,
                                            "log",
                                            &object_key,
                                            "text/plain",
                                            size,
                                        )
                                        .await
                                    {
                                        record
                                            .note_persistence_failure(
                                                &format!("insert_evidence({id})"),
                                                e,
                                            )
                                            .await;
                                    }
                                }
                                Err(e) => {
                                    record
                                        .note_persistence_failure(&format!("evidence.put({id})"), e)
                                        .await;
                                }
                            }
                        }
                    }

                    if matches!(outcome.status, TaskStatus::Failed | TaskStatus::Timeout) {
                        if let Some(selector) = outcome
                            .evidence
                            .as_deref()
                            .and_then(crate::executor::unresolved_healing_selector)
                        {
                            failed_selectors.insert(id.clone(), selector);
                        }
                    }

                    let newly_terminal = {
                        let mut engine = record.engine.write().await;
                        engine.complete_task(&id, outcome.status, outcome.reason.clone())
                    };
                    persist_and_emit_terminal(&record, &store, newly_terminal).await;
                }
                None => break, // all senders dropped; nothing more can complete
            }

            if record.engine.read().await.is_finished() {
                break;
            }
        }

        // SRD §9 Environment Health Gate (pause/quarantine on environment-wide
        // failure) is not implemented in this slice — see README. Final run
        // state here does not yet pre-empt in-flight tasks; by this point
        // `is_finished()` has already guaranteed every task is terminal.
        //
        // D4 — this used to be its own inline check (InfraError -> Partial,
        // else Completed), a second copy of the rule `derive_run_state`
        // already encodes for recovery. That let a run containing FAILED,
        // BLOCKED, or TIMEOUT tasks report COMPLETED live, while the exact
        // same task snapshot would derive FAILED/PARTIAL had the process
        // restarted first — the same tasks, two different verdicts depending
        // on an accident of timing. Reusing the one canonical function here
        // makes that impossible: live and recovery now always agree, because
        // they are the same code path over the same kind of snapshot.
        let tasks_snapshot: Vec<TaskState> =
            { record.engine.read().await.tasks.values().cloned().collect() };
        let mut final_state = derive_run_state(&tasks_snapshot);

        // B1 — if any durable write failed while driving this run, the
        // database no longer agrees with what actually happened. Reporting
        // COMPLETED over a store we know to be wrong is precisely the
        // behaviour that produced silent corruption, so the run is
        // transitioned into an explicit failure state and the cause is
        // recorded in policy_notes (which surfaces on the run summary) and
        // in a RunCompleted summary line, making it auditable rather than a
        // log entry nobody reads.
        let persistence_error = record.persistence_error().await;
        // BE-13 — a cancelled task only exists because cancellation was
        // requested (see `derive_run_state`), so its presence in this
        // snapshot is exactly the "was this run cancelled" signal; there is
        // no separate boolean to fall out of sync with the persisted task
        // states.
        let was_cancelled = final_state == RunState::Cancelled;
        let completion_summary = match &persistence_error {
            None if was_cancelled => {
                let note = match record.cancel_requested_by.read().await.clone() {
                    Some(actor) => format!("run cancelled by {actor}"),
                    None => "run cancelled".to_string(),
                };
                *record.policy_notes.write().await = Some(note.clone());
                note
            }
            None => "run finished".to_string(),
            Some(cause) => {
                final_state = RunState::Failed;
                let note = format!(
                    "durable persistence failed during this run; recorded state may be incomplete — first failure: {cause}"
                );
                *record.policy_notes.write().await = Some(note.clone());
                tracing::error!("run {} marked FAILED: {note}", record.run_id);
                format!("run finished with persistence failure: {cause}")
            }
        };

        let policy_notes_to_persist = record.policy_notes.read().await.clone();

        *record.state.write().await = final_state;
        *record.finished_at.write().await = Some(chrono::Utc::now());
        if let Err(e) = store
            .set_run_finished(
                record.run_id,
                final_state,
                policy_notes_to_persist.as_deref(),
            )
            .await
        {
            record.note_persistence_failure("set_run_finished", e).await;
        }
        record
            .emit(
                &store,
                EventType::RunCompleted,
                None,
                None,
                Some(completion_summary),
            )
            .await;

        // Same snapshot already used to derive `final_state` above — no
        // task can change between then and now, since the dispatch loop has
        // already exited and nothing is still in flight.
        let intel = Intelligence::new(&store);
        if let Err(e) = intel
            .ingest_run(
                Some(record.run_id),
                record.project_id.as_deref(),
                &record.plan,
                &tasks_snapshot,
            )
            .await
        {
            tracing::warn!("intelligence ingest failed for run {}: {e}", record.run_id);
        }

        // Gap-closing — Phase 10: autonomous healing-proposal orchestration.
        // Only meaningful with a real project scope (an untenanted/default
        // run has nothing for GovernedHealingService's RBAC/project checks
        // to scope against); every other gate below is the same live one a
        // human-submitted proposal goes through. An operator can switch the
        // whole pass off with `AIQA_AUTONOMOUS_HEALING=off`.
        let autonomous_healing = crate::policy::autonomous_healing_enabled();
        if let Some(project_id) = record.project_id.as_deref().filter(|_| autonomous_healing) {
            for t in &tasks_snapshot {
                if !matches!(t.status, TaskStatus::Failed | TaskStatus::Timeout) {
                    continue;
                }
                let Some(def) = task_defs.get(&t.id) else {
                    continue;
                };
                // Prefer the selector the task's own runtime self-healing gave up on, read from
                // its output as it completed. `params["selector"]` is only an override for a
                // hand-built plan that names one up front: no compiled plan sets it, so relying
                // on it alone would never fire for a real run.
                let Some(selector) = failed_selectors.get(&t.id).cloned().or_else(|| {
                    def.params
                        .get("selector")
                        .and_then(|v| v.as_str())
                        .map(ToString::to_string)
                }) else {
                    continue;
                };
                let executor_name = def.executor.as_deref().unwrap_or("simulated");

                let Some(req) = intel
                    .detect_autonomous_healing_candidate(
                        project_id,
                        record.run_id,
                        t,
                        executor_name,
                        &selector,
                    )
                    .await
                else {
                    continue;
                };

                let dispatcher = crate::agent_execution::AgentExecutionDispatcher::new(
                    store.clone(),
                    (*policy).clone(),
                    registry.clone(),
                    evidence.clone(),
                    None,
                );
                let healing_service = crate::governed_healing::GovernedHealingService::new(
                    store.clone(),
                    dispatcher,
                    evidence.clone(),
                    policy.clone(),
                    None,
                );
                // Clearly attributable in every audit log entry as
                // system-generated, never impersonating a real API key.
                let system_ctx = crate::auth::AuthContext {
                    key_id: Uuid::nil(),
                    name: "system:phase10-autonomous-orchestrator".to_string(),
                    role: crate::auth::Role::Operator,
                    project_id: Some(project_id.to_string()),
                };
                // With a readable crawl for the project, let the real SelfHealingAgent probe it
                // for a replacement selector; if that diagnosis cannot be completed, fall back
                // to the advisory proposal so a recurring failure is never silently dropped. A
                // spent attempt is not a diagnosis failure and would fail the fallback the same
                // way, so it is passed straight through.
                let outcome = match intel.ground_in_latest_crawl(project_id, &req).await {
                    Some(grounded) => match healing_service
                        .propose_healing(&system_ctx, grounded)
                        .await
                    {
                        Err(e)
                            if !matches!(
                                e,
                                crate::governed_healing::GovernedHealingError::MaxHealingAttemptsExceeded(_)
                            ) =>
                        {
                            tracing::warn!(
                                "Phase 10: crawl-grounded diagnosis for task '{}' in project {project_id} produced no proposal ({e}); proposing without a diagnosed fix instead",
                                t.name
                            );
                            healing_service.propose_healing(&system_ctx, req).await
                        }
                        other => other,
                    },
                    None => healing_service.propose_healing(&system_ctx, req).await,
                };
                match outcome {
                    Ok(proposal) => {
                        tracing::info!(
                            "Phase 10: autonomously proposed healing {} for task '{}' in project {project_id} (run {})",
                            proposal.id,
                            t.name,
                            record.run_id
                        );
                    }
                    // This task/run already used its one bounded healing attempt (e.g. a person
                    // proposed one first): the expected, harmless case.
                    Err(e @ crate::governed_healing::GovernedHealingError::MaxHealingAttemptsExceeded(_)) => {
                        tracing::debug!(
                            "Phase 10: autonomous healing proposal skipped for task '{}' in project {project_id}: {e}",
                            t.name
                        );
                    }
                    // Anything else means a candidate that qualified was refused (validation,
                    // security boundary, storage): surface it rather than lose it silently.
                    // Never fatal to run completion.
                    Err(e) => {
                        tracing::warn!(
                            "Phase 10: autonomous healing proposal refused for task '{}' in project {project_id}: {e}",
                            t.name
                        );
                    }
                }
            }
        }

        webhook::notify_run_completed(record.summary().await);
    });
}
