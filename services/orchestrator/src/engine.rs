use crate::domain::*;
use crate::evidence::LocalEvidenceStore;
use crate::executor::ExecutorRegistry;
use crate::intelligence::Intelligence;
use crate::scheduler::RunEngine;
use crate::store::Store;
use crate::webhook;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock, Semaphore};
use uuid::Uuid;

/// All server-side state for one run (SRD §2 Orchestrator State Machine +
/// SRD §6 event stream). `engine` holds the pure scheduler state from
/// `scheduler.rs`; everything else here is I/O plumbing around it —
/// including, as of this session, the durable side of that plumbing
/// (`store.rs`), not just the in-memory/broadcast side.
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
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: RwLock<Option<chrono::DateTime<chrono::Utc>>>,
    pub policy_notes: RwLock<Option<String>>,
    pub approved_by: RwLock<Option<String>>,
    pub approved_at: RwLock<Option<chrono::DateTime<chrono::Utc>>>,
    seq: AtomicU64,
}

impl RunRecord {
    pub fn new(
        run_id: RunId,
        plan: &RunPlan,
        project_id: Option<String>,
        requested_by: Option<String>,
        initial_state: RunState,
        policy_notes: Option<String>,
    ) -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(1024);
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
            started_at: chrono::Utc::now(),
            finished_at: RwLock::new(None),
            policy_notes: RwLock::new(policy_notes),
            approved_by: RwLock::new(None),
            approved_at: RwLock::new(None),
            seq: AtomicU64::new(0),
        })
    }

    pub fn task_defs(&self) -> HashMap<TaskId, TaskDef> {
        self.plan.tasks.iter().map(|t| (t.id.clone(), t.clone())).collect()
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
        };
        self.events.write().await.push(ev.clone());
        // A send error just means no WebSocket subscribers are currently
        // connected; the event is already durably in `events` for replay.
        let _ = self.tx.send(ev.clone());
        if let Err(e) = store.insert_event(&ev).await {
            tracing::warn!("failed to persist event {} for run {}: {e}", ev.event_id, self.run_id);
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
    actor: &str,
) {
    if let Err(e) = store.approve_run(record.run_id, actor).await {
        tracing::error!("failed to persist approval for run {}: {e}", record.run_id);
    }
    *record.approved_by.write().await = Some(actor.to_string());
    *record.approved_at.write().await = Some(chrono::Utc::now());
    spawn_run(record, registry, concurrency, store, evidence);
}

/// The other side of the approval gate: a held run that a human declines.
/// It never enters the scheduler loop — its tasks stay QUEUED in the DB
/// forever, which is fine, they were never real (SRD §25 semantics only
/// apply to scheduled runs).
pub async fn reject_run(record: Arc<RunRecord>, store: &Store, actor: &str, reason: &str) {
    if let Err(e) = store.reject_run(record.run_id, actor, reason).await {
        tracing::error!("failed to persist rejection for run {}: {e}", record.run_id);
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
pub async fn recover_incomplete_runs(store: &Store) -> anyhow::Result<usize> {
    let ids = store.list_incomplete_runs().await?;
    let reason = "orchestrator process restarted while this task was queued/running; execution state was not durable enough to resume, so it is marked INFRA_ERROR rather than left QUEUED forever";
    for run_id in &ids {
        let tasks = store.list_tasks_for_run(*run_id).await?;
        let mut touched_any = false;
        for t in &tasks {
            if matches!(t.status, TaskStatus::Queued | TaskStatus::Running) {
                store
                    .set_task_status(*run_id, &t.id, TaskStatus::InfraError, Some(reason), false, true)
                    .await?;
                touched_any = true;
            }
        }
        let final_state = if touched_any { RunState::Partial } else { RunState::Failed };
        store.set_run_finished(*run_id, final_state).await?;
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
pub fn spawn_run(
    record: Arc<RunRecord>,
    registry: Arc<ExecutorRegistry>,
    concurrency: usize,
    store: Store,
    evidence: Arc<LocalEvidenceStore>,
) {
    let task_defs = record.task_defs();
    tokio::spawn(async move {
        *record.state.write().await = RunState::Running;
        if let Err(e) = store.set_run_state(record.run_id, RunState::Running).await {
            tracing::error!("failed to persist run state for {}: {e}", record.run_id);
        }
        if let Err(e) = store.set_run_started(record.run_id).await {
            tracing::error!("failed to persist run start time for {}: {e}", record.run_id);
        }
        record
            .emit(
                &store,
                EventType::RunScheduled,
                None,
                None,
                Some(format!("run '{}' scheduled", record.name)),
            )
            .await;

        let semaphore = Arc::new(Semaphore::new(concurrency.max(1)));
        let (done_tx, mut done_rx) = tokio::sync::mpsc::unbounded_channel::<(TaskId, crate::executor::ExecutionOutcome)>();
        let mut in_flight = 0usize;

        loop {
            let newly_eligible = {
                let mut engine = record.engine.write().await;
                let ids = engine.eligible();
                for id in &ids {
                    engine.mark_started(id);
                }
                ids
            };

            for id in newly_eligible {
                record
                    .emit(&store, EventType::TaskStarted, Some(id.clone()), Some(TaskStatus::Running), None)
                    .await;
                if let Err(e) = store
                    .set_task_status(record.run_id, &id, TaskStatus::Running, None, true, false)
                    .await
                {
                    tracing::warn!("failed to persist task-started for {}/{id}: {e}", record.run_id);
                }
                let permit = semaphore.clone().acquire_owned().await.expect("semaphore open");
                in_flight += 1;
                let task_def = task_defs[&id].clone();
                let exec = registry.resolve(&task_def);
                let done_tx = done_tx.clone();
                tokio::spawn(async move {
                    let outcome = exec.execute(&task_def).await;
                    let _ = done_tx.send((task_def.id, outcome));
                    drop(permit);
                });
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
                                        .insert_evidence(record.run_id, &id, "log", &object_key, "text/plain", size)
                                        .await
                                    {
                                        tracing::warn!("failed to record evidence row for {}/{id}: {e}", record.run_id);
                                    }
                                }
                                Err(e) => tracing::warn!("failed to write evidence file for {}/{id}: {e}", record.run_id),
                            }
                        }
                    }

                    let newly_terminal = {
                        let mut engine = record.engine.write().await;
                        engine.complete_task(&id, outcome.status, outcome.reason.clone())
                    };
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
                            tracing::warn!("failed to persist task completion for {}/{tid}: {e}", record.run_id);
                        }
                        record
                            .emit(&store, event_type_for(status), Some(tid), Some(status), reason)
                            .await;
                    }
                }
                None => break, // all senders dropped; nothing more can complete
            }

            if record.engine.read().await.is_finished() {
                break;
            }
        }

        // SRD §9 Environment Health Gate (pause/quarantine on environment-wide
        // failure) is not implemented in this slice — see README. Final run
        // state here only distinguishes "some tasks hit infra errors" from a
        // clean completion; it does not yet pre-empt in-flight tasks.
        let final_state = {
            let engine = record.engine.read().await;
            if engine.tasks.values().any(|t| t.status == TaskStatus::InfraError) {
                RunState::Partial
            } else {
                RunState::Completed
            }
        };
        *record.state.write().await = final_state;
        *record.finished_at.write().await = Some(chrono::Utc::now());
        if let Err(e) = store.set_run_finished(record.run_id, final_state).await {
            tracing::error!("failed to persist run completion for {}: {e}", record.run_id);
        }
        record
            .emit(&store, EventType::RunCompleted, None, None, Some("run finished".into()))
            .await;

        let tasks_snapshot: Vec<TaskState> = { record.engine.read().await.tasks.values().cloned().collect() };
        let intel = Intelligence::new(&store);
        if let Err(e) = intel.ingest_run(record.project_id.as_deref(), &record.plan, &tasks_snapshot).await {
            tracing::warn!("intelligence ingest failed for run {}: {e}", record.run_id);
        }

        webhook::notify_run_completed(record.summary().await);
    });
}
