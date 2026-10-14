//! The loop that carries an autopilot run through its steps.
//!
//! A run is driven by one task at a time (`AutopilotRegistry` makes sure of that), which goes through the run's steps in
//! order and writes what it does to the database as it does it: a step is marked running before it starts, its
//! checkpoints are saved as it goes, and it is marked done (or failed, or waiting) when it stops. So the database always
//! says where the run is, and a run that was going when the service stopped is picked up again at startup
//! (`resume_runs`) at the step it was in, not from the beginning.
//!
//! What a step does is a `StepRunner`, so the loop can be tested with steps that do exactly what a test says (finish, wait,
//! fail, take a long time); the real ones are in `autopilot_steps.rs`.

use crate::autopilot::{event, AutopilotConfig, RunState, Step, StepError, StepOutcome, StepState};
use crate::autopilot_pipeline::{InternalApi, PipelineApi};
use crate::autopilot_store::AutopilotRunRow;
use crate::state::AppState;
use crate::store::Store;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use uuid::Uuid;

/// How long a run waits between looks at something outside it, and the longest it waits for a crawl.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunnerOptions {
    /// How often a step that is waiting for something (a crawl) looks again; also how often it looks for a cancel.
    pub poll_every: Duration,
    /// The longest a crawl may take before the step gives up on it.
    pub crawl_timeout: Duration,
    /// The longest the run of one test case may take before the step stops waiting for it.
    pub run_timeout: Duration,
}

impl Default for RunnerOptions {
    fn default() -> Self {
        RunnerOptions {
            poll_every: Duration::from_secs(2),
            crawl_timeout: Duration::from_secs(30 * 60),
            run_timeout: Duration::from_secs(20 * 60),
        }
    }
}

/// Which runs this process is working on, and how it goes about it. One of these is in `AppState`.
#[derive(Default)]
pub struct AutopilotRegistry {
    active: Mutex<HashSet<Uuid>>,
    options: RwLock<RunnerOptions>,
}

impl AutopilotRegistry {
    pub fn options(&self) -> RunnerOptions {
        *self.options.read().unwrap()
    }

    pub fn set_options(&self, options: RunnerOptions) {
        *self.options.write().unwrap() = options;
    }

    /// The right to work on `run`; `None` when something is working on it already. Given back when it is dropped.
    pub fn claim(self: &Arc<Self>, run: Uuid) -> Option<RunClaim> {
        self.active.lock().unwrap().insert(run).then(|| RunClaim {
            registry: self.clone(),
            run,
        })
    }

    pub fn is_working_on(&self, run: Uuid) -> bool {
        self.active.lock().unwrap().contains(&run)
    }
}

pub struct RunClaim {
    registry: Arc<AutopilotRegistry>,
    run: Uuid,
}

impl Drop for RunClaim {
    fn drop(&mut self) {
        self.registry.active.lock().unwrap().remove(&self.run);
    }
}

/// What a step is given to work with.
pub struct StepCtx {
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub step: Step,
    pub config: AutopilotConfig,
    /// What the step had saved when it was started: the checkpoints of an attempt that a restart cut short.
    pub detail: Value,
    pub store: Store,
    pub api: Arc<dyn PipelineApi>,
    pub options: RunnerOptions,
}

impl StepCtx {
    /// A path of this run's project: `/api/v1/projects/<id>/<tail>`.
    pub fn path(&self, tail: &str) -> String {
        format!("/api/v1/projects/{}/{tail}", self.project_id)
    }

    /// A line in the run's timeline, under this step.
    pub async fn note(&self, message: &str, data: Option<Value>) {
        let _ = self
            .store
            .add_autopilot_event(
                self.run_id,
                event::NOTE,
                Some(self.step.as_str()),
                message,
                data.as_ref(),
            )
            .await;
    }

    /// Saves what the step has done so far, for an attempt after a restart to carry on from.
    pub async fn checkpoint(&self, detail: &Value) -> Result<(), StepError> {
        self.store
            .save_autopilot_step_detail(self.run_id, self.step.as_str(), detail)
            .await
            .map_err(|e| StepError::failed(format!("could not save the step's progress: {e}")))
    }

    /// `Err(Cancelled)` when the person has asked for the run to stop.
    pub async fn check_cancel(&self) -> Result<(), StepError> {
        match self.store.get_autopilot_run(self.run_id).await {
            Ok(Some(run)) if run.cancel_requested => Err(StepError::Cancelled),
            Ok(Some(_)) => Ok(()),
            Ok(None) => Err(StepError::failed("the run no longer exists")),
            Err(e) => Err(StepError::failed(format!("could not read the run: {e}"))),
        }
    }

    /// What another step of this run saved (`null` when there is nothing): a step reads what an earlier one found.
    pub async fn detail_of(&self, step: Step) -> Value {
        self.store
            .list_autopilot_steps(self.run_id)
            .await
            .ok()
            .and_then(|rows| rows.into_iter().find(|row| row.name == step.as_str()))
            .map_or(Value::Null, |row| row.detail)
    }

    /// Waits one polling interval, then looks for a cancel.
    pub async fn pause(&self) -> Result<(), StepError> {
        tokio::time::sleep(self.options.poll_every).await;
        self.check_cancel().await
    }
}

/// What a step does.
#[async_trait]
pub trait StepRunner: Send + Sync {
    async fn run(&self, step: Step, ctx: &StepCtx) -> Result<StepOutcome, StepError>;
}

/// Everything the loop needs, and cheap to copy.
#[derive(Clone)]
pub struct Worker {
    pub store: Store,
    pub api: Arc<dyn PipelineApi>,
    pub steps: Arc<dyn StepRunner>,
    pub options: RunnerOptions,
}

async fn record(
    store: &Store,
    run: Uuid,
    kind: &str,
    step: Option<Step>,
    message: &str,
    data: Option<Value>,
) {
    let _ = store
        .add_autopilot_event(run, kind, step.map(Step::as_str), message, data.as_ref())
        .await;
}

/// Ends a run that has not ended, as failed, and says so in its timeline.
async fn fail_run(store: &Store, run_id: Uuid, why: &str) {
    if store
        .finish_autopilot_run(run_id, RunState::Failed, Some(why))
        .await
        .unwrap_or(false)
    {
        record(store, run_id, event::FAILED, None, why, None).await;
    }
}

async fn cancel_run(store: &Store, run_id: Uuid) {
    if store
        .finish_autopilot_run(run_id, RunState::Cancelled, None)
        .await
        .unwrap_or(false)
    {
        let _ = store
            .skip_unfinished_autopilot_steps(run_id, "the run was cancelled")
            .await;
        record(store, run_id, event::CANCELLED, None, "Run cancelled", None).await;
    }
}

/// Goes through the run's steps, in order, from the first that is not settled. Returns when the run has ended, when it
/// has stopped at a gate (`WAITING`), or when it is not `RUNNING` (another actor moved it: it is not this loop's to work
/// on). `Err` only for a failure of the database.
pub async fn drive(worker: &Worker, run_id: Uuid) -> anyhow::Result<()> {
    let store = &worker.store;
    let Some(run) = store.get_autopilot_run(run_id).await? else {
        return Ok(());
    };
    if run.state != RunState::Running.as_str() {
        return Ok(());
    }
    let config: AutopilotConfig = match serde_json::from_value(run.config.clone()) {
        Ok(config) => config,
        Err(e) => {
            fail_run(
                store,
                run_id,
                &format!("the run's settings could not be read: {e}"),
            )
            .await;
            return Ok(());
        }
    };
    let Ok(project_id) = Uuid::parse_str(&run.project_id) else {
        fail_run(store, run_id, "the run's project is not a valid id").await;
        return Ok(());
    };

    let rows = store.list_autopilot_steps(run_id).await?;
    if let Some(cut_short) = rows
        .iter()
        .find(|row| row.state == StepState::Running.as_str())
    {
        record(
            store,
            run_id,
            event::RESUMED,
            Step::parse(&cut_short.name),
            &format!(
                "Picked up again after the service was restarted, at {}",
                Step::parse(&cut_short.name).map_or("a step", Step::label)
            ),
            None,
        )
        .await;
    }

    for row in rows {
        // Looked at before every step: a cancel, or a run someone else has already moved on.
        let Some(current) = store.get_autopilot_run(run_id).await? else {
            return Ok(());
        };
        if current.state != RunState::Running.as_str() {
            return Ok(());
        }
        if current.cancel_requested {
            cancel_run(store, run_id).await;
            return Ok(());
        }
        if StepState::parse(&row.state).is_some_and(StepState::is_settled) {
            continue;
        }
        let Some(step) = Step::parse(&row.name) else {
            fail_run(
                store,
                run_id,
                &format!(
                    "the run has a step this version does not know: {}",
                    row.name
                ),
            )
            .await;
            return Ok(());
        };

        store.begin_autopilot_step(run_id, &row.name).await?;
        record(
            store,
            run_id,
            event::STEP_STARTED,
            Some(step),
            &format!("Started {}", step.label()),
            None,
        )
        .await;
        let ctx = StepCtx {
            run_id,
            project_id,
            step,
            config: config.clone(),
            detail: row.detail.clone(),
            store: store.clone(),
            api: worker.api.clone(),
            options: worker.options,
        };
        match worker.steps.run(step, &ctx).await {
            Ok(StepOutcome::Done { detail, skipped }) => {
                let (state, kind, message) = if skipped {
                    (
                        StepState::Skipped,
                        event::STEP_SKIPPED,
                        format!("Nothing to do for {}", step.label()),
                    )
                } else {
                    (
                        StepState::Done,
                        event::STEP_DONE,
                        format!("Finished {}", step.label()),
                    )
                };
                store
                    .settle_autopilot_step(run_id, &row.name, state, Some(&detail), None)
                    .await?;
                record(store, run_id, kind, Some(step), &message, Some(detail)).await;
            }
            Ok(StepOutcome::Wait { detail, why }) => {
                store
                    .settle_autopilot_step(
                        run_id,
                        &row.name,
                        StepState::Waiting,
                        Some(&detail),
                        None,
                    )
                    .await?;
                if store.wait_autopilot_run(run_id).await? {
                    record(
                        store,
                        run_id,
                        event::STEP_WAITING,
                        Some(step),
                        &why,
                        Some(detail),
                    )
                    .await;
                }
                return Ok(());
            }
            Err(StepError::Cancelled) => {
                store
                    .settle_autopilot_step(
                        run_id,
                        &row.name,
                        StepState::Skipped,
                        None,
                        Some("the run was cancelled"),
                    )
                    .await?;
                cancel_run(store, run_id).await;
                return Ok(());
            }
            Err(StepError::Failed(why)) => {
                store
                    .settle_autopilot_step(run_id, &row.name, StepState::Failed, None, Some(&why))
                    .await?;
                record(store, run_id, event::STEP_FAILED, Some(step), &why, None).await;
                fail_run(
                    store,
                    run_id,
                    &format!("{}: {why}", capitalised(step.label())),
                )
                .await;
                return Ok(());
            }
        }
    }

    if store
        .finish_autopilot_run(run_id, RunState::Completed, None)
        .await?
    {
        record(store, run_id, event::COMPLETED, None, "Run completed", None).await;
    }
    Ok(())
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// `drive`, on a task of its own: a step that panics ends the run as failed, where it would otherwise leave it running
/// for ever with nothing working on it.
pub async fn drive_guarded(worker: &Worker, run_id: Uuid) {
    let task = {
        let worker = worker.clone();
        tokio::spawn(async move { drive(&worker, run_id).await })
    };
    let why = match task.await {
        Ok(Ok(())) => return,
        Ok(Err(e)) => format!("the run stopped because of an internal error: {e}"),
        Err(_) => "the run stopped because of an internal error (a step panicked)".to_string(),
    };
    fail_run(&worker.store, run_id, &why).await;
}

/// Starts working on a run in the background, as its creator. `false`: something is already working on it.
pub fn spawn_run(state: &AppState, run: &AutopilotRunRow) -> bool {
    let Some(claim) = state.autopilot.claim(run.id) else {
        return false;
    };
    let worker = Worker {
        store: state.store.clone(),
        api: Arc::new(InternalApi::new(state, run.created_by_key)),
        steps: Arc::new(crate::autopilot_steps::ApiSteps),
        options: state.autopilot.options(),
    };
    let run_id = run.id;
    tokio::spawn(async move {
        let _claim = claim;
        drive_guarded(&worker, run_id).await;
    });
    true
}

/// At startup: picks up the runs that were going when the process stopped. Runs waiting at a gate or for their estimate
/// to be accepted are waiting for a person, and stay so. Returns how many were picked up.
pub async fn resume_runs(state: &AppState) -> anyhow::Result<usize> {
    let mut resumed = 0;
    for id in state
        .store
        .autopilot_run_ids_in_state(RunState::Running)
        .await?
    {
        if let Some(run) = state.store.get_autopilot_run(id).await? {
            if spawn_run(state, &run) {
                resumed += 1;
            }
        }
    }
    Ok(resumed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopilot_pipeline::scripted::ScriptedApi;
    use crate::autopilot_store::NewAutopilotRun;
    use serde_json::json;
    use std::collections::{HashMap, VecDeque};

    /// Steps that do what the test says: the outcomes given per step (in turn), else a plain finish.
    #[derive(Default)]
    struct Scripted {
        outcomes: Mutex<HashMap<Step, VecDeque<Result<StepOutcome, StepError>>>>,
        ran: Mutex<Vec<Step>>,
        details: Mutex<Vec<(Step, Value)>>,
    }

    impl Scripted {
        fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }

        fn with(self: Arc<Self>, step: Step, outcome: Result<StepOutcome, StepError>) -> Arc<Self> {
            self.outcomes
                .lock()
                .unwrap()
                .entry(step)
                .or_default()
                .push_back(outcome);
            self
        }

        fn ran(&self) -> Vec<Step> {
            self.ran.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl StepRunner for Scripted {
        async fn run(&self, step: Step, ctx: &StepCtx) -> Result<StepOutcome, StepError> {
            self.ran.lock().unwrap().push(step);
            self.details
                .lock()
                .unwrap()
                .push((step, ctx.detail.clone()));
            self.outcomes
                .lock()
                .unwrap()
                .get_mut(&step)
                .and_then(VecDeque::pop_front)
                .unwrap_or(Ok(StepOutcome::Done {
                    detail: json!({"ran": step.as_str()}),
                    skipped: false,
                }))
        }
    }

    fn finished(detail: Value) -> Result<StepOutcome, StepError> {
        Ok(StepOutcome::Done {
            detail,
            skipped: false,
        })
    }

    struct World {
        store: Store,
        run: Uuid,
        _dir: tempfile::TempDir,
    }

    impl World {
        fn worker(&self, steps: Arc<dyn StepRunner>) -> Worker {
            Worker {
                store: self.store.clone(),
                api: Arc::new(ScriptedApi::new()),
                steps,
                options: RunnerOptions {
                    poll_every: Duration::from_millis(1),
                    crawl_timeout: Duration::from_secs(1),
                    run_timeout: Duration::from_secs(1),
                },
            }
        }

        async fn run_row(&self) -> AutopilotRunRow {
            self.store
                .get_autopilot_run(self.run)
                .await
                .unwrap()
                .unwrap()
        }

        async fn step_states(&self) -> Vec<(String, String)> {
            self.store
                .list_autopilot_steps(self.run)
                .await
                .unwrap()
                .into_iter()
                .map(|s| (s.name, s.state))
                .collect()
        }

        async fn events(&self) -> Vec<(String, Option<String>)> {
            self.store
                .list_autopilot_events(self.run, None, 200)
                .await
                .unwrap()
                .into_iter()
                .map(|e| (e.kind, e.step))
                .collect()
        }

        /// Teardown. `World` owns the SQLite pool and the temp directory it lives
        /// in, so the pool is closed here and `_dir` becomes removable. The fields
        /// stay private: tests reach the pool through this method rather than by
        /// reaching into the struct. Safe to call more than once.
        async fn close(&self) {
            self.store.pool.close().await;
        }
    }

    /// `first` for the steps it names, then `rest` for every step after them.
    fn states(first: &[&str], rest: &str) -> Vec<String> {
        let mut states: Vec<String> = first.iter().map(|s| s.to_string()).collect();
        states.resize(Step::ALL.len(), rest.to_string());
        states
    }

    fn valid_config() -> Value {
        json!({"base_url": "https://shop.example.test", "roles": [{"name": "anonymous"}]})
    }

    async fn made_run(config: &Value) -> World {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::connect(&dir.path().join("t.sqlite3")).await.unwrap();
        let run = Uuid::new_v4();
        assert!(store
            .insert_autopilot_run(
                &NewAutopilotRun {
                    id: run,
                    project_id: &Uuid::new_v4().to_string(),
                    name: "t",
                    config,
                    estimate: &json!({}),
                    created_by_key: Uuid::new_v4(),
                    created_by_name: "k",
                },
                &Step::ALL.map(Step::as_str),
            )
            .await
            .unwrap());
        World {
            store,
            run,
            _dir: dir,
        }
    }

    /// A run in a real temporary database, started (`RUNNING`), its three steps pending.
    async fn world_with(config: Value) -> World {
        let w = made_run(&config).await;
        assert!(w.store.start_autopilot_run(w.run, false).await.unwrap());
        w
    }

    async fn world() -> World {
        world_with(valid_config()).await
    }

    #[tokio::test]
    async fn a_run_goes_through_its_steps_in_order_and_completes() {
        let w = world().await;
        let steps = Scripted::new().with(Step::Crawl, finished(json!({"pages": 12})));
        drive(&w.worker(steps.clone()), w.run).await.unwrap();

        assert_eq!(
            steps.ran(),
            Step::ALL.to_vec(),
            "in the order the run has them"
        );
        let run = w.run_row().await;
        assert_eq!(run.state, "COMPLETED");
        assert!(run.error.is_none() && run.finished_at.is_some());
        let done = w.store.list_autopilot_steps(w.run).await.unwrap();
        assert!(done
            .iter()
            .all(|s| s.state == "DONE" && s.attempts == 1 && s.finished_at.is_some()));
        assert_eq!(
            done[1].detail,
            json!({"pages": 12}),
            "what a step reports is kept with it"
        );
        assert_eq!(done[0].detail, json!({"ran": "REQUIREMENTS"}));
        // The timeline reads like the run: created, each step started and done, completed.
        let kinds: Vec<String> = w.events().await.into_iter().map(|(k, _)| k).collect();
        let mut expected = vec!["run.created"];
        for _ in Step::ALL {
            expected.extend(["step.started", "step.done"]);
        }
        expected.push("run.completed");
        assert_eq!(kinds, expected);
        w.close().await;
    }

    #[tokio::test]
    async fn a_step_that_had_nothing_to_do_is_marked_skipped_and_the_run_goes_on() {
        let w = world().await;
        let steps = Scripted::new().with(
            Step::Crawl,
            Ok(StepOutcome::Done {
                detail: json!({"reused": true}),
                skipped: true,
            }),
        );
        drive(&w.worker(steps.clone()), w.run).await.unwrap();
        let states = w.step_states().await;
        assert_eq!(states[1], ("CRAWL".to_string(), "SKIPPED".to_string()));
        assert_eq!(states[2].1, "DONE", "the next step ran");
        assert_eq!(w.run_row().await.state, "COMPLETED");
        assert!(w
            .events()
            .await
            .contains(&("step.skipped".to_string(), Some("CRAWL".to_string()))));
            w.close().await;
    }

    #[tokio::test]
    async fn a_failing_step_ends_the_run_failed_with_the_reason_and_leaves_the_later_steps_pending()
    {
        let w = world().await;
        let steps = Scripted::new().with(
            Step::Crawl,
            Err(StepError::failed("the crawl reached no page")),
        );
        drive(&w.worker(steps.clone()), w.run).await.unwrap();

        assert_eq!(
            steps.ran(),
            [Step::Requirements, Step::Crawl],
            "nothing after the failure ran"
        );
        let run = w.run_row().await;
        assert_eq!(run.state, "FAILED");
        assert_eq!(
            run.error.as_deref(),
            Some("Crawling the application: the crawl reached no page")
        );
        assert_eq!(
            run.current_step.as_deref(),
            Some("CRAWL"),
            "which step it was in stays"
        );
        let found: Vec<String> = w.step_states().await.into_iter().map(|(_, s)| s).collect();
        assert_eq!(found, states(&["DONE", "FAILED"], "PENDING"));
        let crawl = &w.store.list_autopilot_steps(w.run).await.unwrap()[1];
        assert_eq!(crawl.error.as_deref(), Some("the crawl reached no page"));
        let kinds: Vec<String> = w.events().await.into_iter().map(|(k, _)| k).collect();
        assert_eq!(&kinds[kinds.len() - 2..], ["step.failed", "run.failed"]);
        w.close().await;
    }

    #[tokio::test]
    async fn a_step_that_waits_leaves_the_run_waiting_and_a_run_that_is_waiting_is_not_driven() {
        let w = world().await;
        let steps = Scripted::new().with(
            Step::Crawl,
            Ok(StepOutcome::Wait {
                detail: json!({"cases": 40}),
                why: "40 cases wait for approval".into(),
            }),
        );
        drive(&w.worker(steps.clone()), w.run).await.unwrap();

        assert_eq!(steps.ran(), [Step::Requirements, Step::Crawl]);
        let run = w.run_row().await;
        assert_eq!(run.state, "WAITING");
        assert!(run.finished_at.is_none(), "a run at a gate has not ended");
        let found: Vec<String> = w.step_states().await.into_iter().map(|(_, s)| s).collect();
        assert_eq!(found, states(&["DONE", "WAITING"], "PENDING"));
        let waiting = &w.store.list_autopilot_steps(w.run).await.unwrap()[1];
        assert_eq!(waiting.detail, json!({"cases": 40}));
        let events = w
            .store
            .list_autopilot_events(w.run, None, 100)
            .await
            .unwrap();
        let last = events.last().unwrap();
        assert_eq!(
            (last.kind.as_str(), last.message.as_str()),
            ("step.waiting", "40 cases wait for approval")
        );

        // Driven again while it waits: nothing happens, whoever asks.
        drive(&w.worker(steps.clone()), w.run).await.unwrap();
        assert_eq!(steps.ran().len(), 2);
        assert_eq!(w.run_row().await.state, "WAITING");
        w.close().await;
    }

    #[tokio::test]
    async fn only_a_running_run_is_driven() {
        let w = made_run(&valid_config()).await;
        let run = w.run;
        let steps = Scripted::new();
        // Awaiting approval: not started by the person yet.
        drive(&w.worker(steps.clone()), run).await.unwrap();
        assert!(steps.ran().is_empty());
        assert_eq!(w.run_row().await.state, "AWAITING_APPROVAL");
        // Ended.
        w.store.cancel_autopilot_run(run).await.unwrap();
        drive(&w.worker(steps.clone()), run).await.unwrap();
        assert!(steps.ran().is_empty());
        assert_eq!(w.run_row().await.state, "CANCELLED");
        // A run that does not exist is nothing to do either.
        drive(&w.worker(steps.clone()), Uuid::new_v4())
            .await
            .unwrap();
        assert!(steps.ran().is_empty());
        w.close().await;
    }

    #[tokio::test]
    async fn a_cancel_asked_for_before_a_step_ends_the_run_cancelled_and_skips_what_is_left() {
        let w = world().await;
        assert_eq!(
            w.store.cancel_autopilot_run(w.run).await.unwrap(),
            crate::autopilot_store::CancelOutcome::Requested
        );
        let steps = Scripted::new();
        drive(&w.worker(steps.clone()), w.run).await.unwrap();
        assert!(steps.ran().is_empty(), "not even the first step started");
        let run = w.run_row().await;
        assert_eq!(run.state, "CANCELLED");
        assert!(run.finished_at.is_some() && run.error.is_none());
        assert!(w.step_states().await.iter().all(|(_, s)| s == "SKIPPED"));
        assert_eq!(w.events().await.last().unwrap().0, "run.cancelled");
        w.close().await;
    }

    /// Asks for the run to be cancelled while its first step is working; every step finishes normally.
    struct CancelsDuringFirstStep;

    #[async_trait]
    impl StepRunner for CancelsDuringFirstStep {
        async fn run(&self, step: Step, ctx: &StepCtx) -> Result<StepOutcome, StepError> {
            if step == Step::Requirements {
                ctx.store.cancel_autopilot_run(ctx.run_id).await.unwrap();
            }
            finished(json!({}))
        }
    }

    #[tokio::test]
    async fn a_cancel_asked_for_while_a_step_works_lets_that_step_finish_and_stops_before_the_next()
    {
        let w = world().await;
        drive(&w.worker(Arc::new(CancelsDuringFirstStep)), w.run)
            .await
            .unwrap();
        assert_eq!(w.run_row().await.state, "CANCELLED");
        let found: Vec<String> = w.step_states().await.into_iter().map(|(_, s)| s).collect();
        assert_eq!(found, states(&["DONE"], "SKIPPED"));
        w.close().await;
    }

    #[tokio::test]
    async fn a_step_that_notices_the_cancel_itself_is_skipped_and_the_run_ends_cancelled() {
        let w = world().await;
        let steps = Scripted::new().with(Step::Crawl, Err(StepError::Cancelled));
        drive(&w.worker(steps.clone()), w.run).await.unwrap();
        assert_eq!(steps.ran(), [Step::Requirements, Step::Crawl]);
        let run = w.run_row().await;
        assert_eq!(run.state, "CANCELLED");
        assert!(run.error.is_none(), "a cancel is not a failure");
        let found: Vec<String> = w.step_states().await.into_iter().map(|(_, s)| s).collect();
        assert_eq!(found, states(&["DONE"], "SKIPPED"));
        assert_eq!(w.events().await.last().unwrap().0, "run.cancelled");
        w.close().await;
    }

    #[tokio::test]
    async fn a_run_picked_up_after_a_restart_passes_the_steps_that_are_done_and_repeats_the_one_cut_short(
    ) {
        let w = world().await;
        // The state a restart leaves: the first step done, the second running with a checkpoint saved.
        w.store
            .begin_autopilot_step(w.run, "REQUIREMENTS")
            .await
            .unwrap();
        w.store
            .settle_autopilot_step(
                w.run,
                "REQUIREMENTS",
                StepState::Done,
                Some(&json!({"requirements": 6})),
                None,
            )
            .await
            .unwrap();
        w.store.begin_autopilot_step(w.run, "CRAWL").await.unwrap();
        w.store
            .save_autopilot_step_detail(w.run, "CRAWL", &json!({"crawl_run_id": "abc"}))
            .await
            .unwrap();

        let steps = Scripted::new();
        drive(&w.worker(steps.clone()), w.run).await.unwrap();

        assert_eq!(
            steps.ran(),
            Step::ALL[1..].to_vec(),
            "the finished step is not done again"
        );
        let seen = steps.details.lock().unwrap().clone();
        assert_eq!(
            seen[0],
            (Step::Crawl, json!({"crawl_run_id": "abc"})),
            "the step gets its checkpoint back"
        );
        let rows = w.store.list_autopilot_steps(w.run).await.unwrap();
        assert_eq!(rows[0].attempts, 1);
        assert_eq!(
            rows[0].detail,
            json!({"requirements": 6}),
            "what it found is untouched"
        );
        assert_eq!(
            rows[1].attempts, 2,
            "the crawl step was started a second time"
        );
        assert_eq!(w.run_row().await.state, "COMPLETED");
        let resumed: Vec<_> = w
            .events()
            .await
            .into_iter()
            .filter(|(k, _)| k == "run.resumed")
            .collect();
        assert_eq!(
            resumed,
            [("run.resumed".to_string(), Some("CRAWL".to_string()))]
        );
        let message = w
            .store
            .list_autopilot_events(w.run, None, 100)
            .await
            .unwrap()
            .into_iter()
            .find(|e| e.kind == "run.resumed")
            .unwrap()
            .message;
        assert_eq!(
            message,
            "Picked up again after the service was restarted, at crawling the application"
        );
        w.close().await;
    }

    #[tokio::test]
    async fn a_run_that_was_not_cut_short_says_nothing_about_a_restart() {
        let w = world().await;
        drive(&w.worker(Scripted::new()), w.run).await.unwrap();
        assert!(w.events().await.iter().all(|(k, _)| k != "run.resumed"));
        w.close().await;
    }

    #[tokio::test]
    async fn settings_that_cannot_be_read_or_a_step_this_version_does_not_know_fail_the_run() {
        let w = world_with(json!({"roles": "not a list"})).await;
        let steps = Scripted::new();
        drive(&w.worker(steps.clone()), w.run).await.unwrap();
        assert!(steps.ran().is_empty());
        let run = w.run_row().await;
        assert_eq!(run.state, "FAILED");
        assert!(run
            .error
            .unwrap()
            .starts_with("the run's settings could not be read"));

w.close().await;
        let w = world().await;
        sqlx::query(
            "UPDATE autopilot_steps SET name = 'TELEPORT' WHERE run_id = ? AND name = 'CRAWL'",
        )
        .bind(w.run.to_string())
        .execute(&w.store.pool)
        .await
        .unwrap();
        let steps = Scripted::new();
        drive(&w.worker(steps.clone()), w.run).await.unwrap();
        assert_eq!(steps.ran(), [Step::Requirements], "the steps before it ran");
        let run = w.run_row().await;
        assert_eq!(run.state, "FAILED");
        assert_eq!(
            run.error.as_deref(),
            Some("the run has a step this version does not know: TELEPORT")
        );
        w.close().await;
    }

    struct Panics;

    #[async_trait]
    impl StepRunner for Panics {
        async fn run(&self, _step: Step, _ctx: &StepCtx) -> Result<StepOutcome, StepError> {
            panic!("a step went wrong in a way nobody thought of");
        }
    }

    #[tokio::test]
    async fn a_step_that_panics_ends_the_run_failed_instead_of_leaving_it_running() {
        let w = world().await;
        drive_guarded(&w.worker(Arc::new(Panics)), w.run).await;
        let run = w.run_row().await;
        assert_eq!(run.state, "FAILED");
        assert_eq!(
            run.error.as_deref(),
            Some("the run stopped because of an internal error (a step panicked)")
        );
        assert_eq!(w.events().await.last().unwrap().0, "run.failed");
        // A run that ends normally is left as the loop left it.
        w.close().await;
        let w = world().await;
        drive_guarded(&w.worker(Scripted::new()), w.run).await;
        assert_eq!(w.run_row().await.state, "COMPLETED");
        w.close().await;
    }

    #[tokio::test]
    async fn only_one_task_may_work_on_a_run_at_a_time() {
        let registry = Arc::new(AutopilotRegistry::default());
        let run = Uuid::new_v4();
        assert!(!registry.is_working_on(run));
        let claim = registry.claim(run).expect("nothing works on it yet");
        assert!(registry.is_working_on(run));
        assert!(registry.claim(run).is_none(), "a second claim is refused");
        assert!(
            registry.claim(Uuid::new_v4()).is_some(),
            "other runs are not affected"
        );
        drop(claim);
        assert!(!registry.is_working_on(run));
        assert!(registry.claim(run).is_some(), "given back when dropped");
    }

    #[test]
    fn the_registry_keeps_the_options_a_test_or_a_deployment_sets() {
        let registry = AutopilotRegistry::default();
        assert_eq!(registry.options(), RunnerOptions::default());
        assert_eq!(RunnerOptions::default().poll_every, Duration::from_secs(2));
        assert_eq!(
            RunnerOptions::default().crawl_timeout,
            Duration::from_secs(1800)
        );
        let fast = RunnerOptions {
            poll_every: Duration::from_millis(5),
            crawl_timeout: Duration::from_secs(9),
            run_timeout: Duration::from_secs(8),
        };
        registry.set_options(fast);
        assert_eq!(registry.options(), fast);
    }

    #[tokio::test]
    async fn a_step_can_read_what_another_step_saved_and_build_a_path_in_its_project() {
        let w = world().await;
        w.store
            .save_autopilot_step_detail(w.run, "CRAWL", &json!({"crawl_run_id": "c1"}))
            .await
            .unwrap();
        let run = w.run_row().await;
        let ctx = StepCtx {
            run_id: w.run,
            project_id: Uuid::parse_str(&run.project_id).unwrap(),
            step: Step::GapCheck,
            config: serde_json::from_value(valid_config()).unwrap(),
            detail: json!({}),
            store: w.store.clone(),
            api: Arc::new(ScriptedApi::new()),
            options: RunnerOptions::default(),
        };
        assert_eq!(
            ctx.detail_of(Step::Crawl).await,
            json!({"crawl_run_id": "c1"})
        );
        assert_eq!(
            ctx.detail_of(Step::Requirements).await,
            json!({}),
            "a step that saved nothing has an empty detail"
        );
        assert_eq!(
            ctx.path("gap-check"),
            format!("/api/v1/projects/{}/gap-check", run.project_id)
        );
        assert_eq!(ctx.check_cancel().await, Ok(()));
        w.store.cancel_autopilot_run(w.run).await.unwrap();
        assert_eq!(ctx.check_cancel().await, Err(StepError::Cancelled));
        assert_eq!(
            ctx.pause().await,
            Err(StepError::Cancelled),
            "waiting looks for a cancel too"
        );
        // A checkpoint is saved under the step that made it.
        ctx.checkpoint(&json!({"n": 1})).await.unwrap();
        assert_eq!(ctx.detail_of(Step::GapCheck).await, json!({"n": 1}));
        ctx.note("hello", Some(json!({"k": "v"}))).await;
        let last = w
            .store
            .list_autopilot_events(w.run, None, 100)
            .await
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(
            (
                last.kind.as_str(),
                last.step.as_deref(),
                last.message.as_str()
            ),
            ("note", Some("GAP_CHECK"), "hello")
        );
        assert_eq!(last.data, Some(json!({"k": "v"})));
        w.close().await;
    }

    #[test]
    fn a_step_is_named_in_a_sentence_with_a_capital() {
        assert_eq!(
            capitalised("crawling the application"),
            "Crawling the application"
        );
        assert_eq!(capitalised("\u{e9}"), "\u{c9}");
        assert_eq!(capitalised(""), "");
    }
}
