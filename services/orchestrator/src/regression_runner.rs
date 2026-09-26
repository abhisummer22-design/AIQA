//! Phase 8C — the loop that starts scheduled regression runs, and the one function that makes a run (`regression.rs` decides what and when).
//!
//! Every half minute the loop asks for the schedules whose time has come. For each it works out which cases can be repeated (the ones a
//! project has approved and run once, each with the task it was last run as), makes one execution of those tasks as the key that made
//! the schedule, and moves the schedule to its next slot. A run is made through `create_run_core`, the function every run is made by, so
//! it meets the policy, the plan checks (a target the service may not reach is refused) and the audit like any other.

use crate::api::{audit, create_run_core, scoped_run_summary};
use crate::auth::{self, AuthContext, Role};
use crate::domain::{RunPlan, TaskDef};
use crate::regression::{
    next_slot_after, replay_of, CaseInfo, RegressionRunRow, Replay, ReplayedCase, ScheduleRow,
    SkippedCase, Trigger,
};
use crate::state::AppState;
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use uuid::Uuid;

/// How often the loop looks for schedules that are due.
pub const TICK: Duration = Duration::from_secs(30);

/// Why a schedule did not make a run this time.
#[derive(Debug)]
pub enum Blocked {
    /// The key it runs as was revoked, or may no longer run tests in the project.
    KeyGone(String),
    ProjectGone,
    /// The run before this one has not ended; another is not piled on it.
    StillRunning(Uuid),
    /// No case in scope has a run to repeat.
    NothingToRepeat(Vec<SkippedCase>),
    /// The service refused the run (its policy, a target it may not reach) or could not keep it.
    Refused(String),
}

impl Blocked {
    /// What is written on the schedule.
    pub fn note(&self) -> String {
        match self {
            Blocked::KeyGone(why) => format!("paused: {why}"),
            Blocked::ProjectGone => "paused: the project is gone".to_string(),
            Blocked::StillRunning(_) => {
                "skipped this time: the run before it has not ended".to_string()
            }
            Blocked::NothingToRepeat(skipped) if skipped.is_empty() => {
                "nothing to run: the project has no approved case".to_string()
            }
            Blocked::NothingToRepeat(skipped) => format!(
                "nothing to run: none of the {} case(s) in scope has a run to repeat (hand each one off once)",
                skipped.len()
            ),
            Blocked::Refused(why) => format!("the run was refused: {why}"),
        }
    }

    /// A schedule that nobody can run is paused, not tried again every time.
    pub fn pauses(&self) -> bool {
        matches!(self, Blocked::KeyGone(_) | Blocked::ProjectGone)
    }
}

/// What a schedule would repeat now: the tasks, each with the case it belongs to, and the cases that cannot be repeated with why.
pub struct Scope {
    pub tasks: Vec<(ReplayedCase, TaskDef)>,
    pub skipped: Vec<SkippedCase>,
}

/// The cases in scope (the project's approved cases, or a suite's), split into those whose last run can be repeated and those that cannot.
pub async fn scope(state: &AppState, project_id: &str, suite_id: Option<&str>) -> Result<Scope> {
    let suite = suite_id.map(Uuid::parse_str).transpose()?;
    let mut cases = state
        .store
        .list_test_cases(project_id, Some("EXECUTED"), suite)
        .await?;
    cases.extend(
        state
            .store
            .list_test_cases(project_id, Some("APPROVED"), suite)
            .await?,
    );
    // The plan of each run a case was last executed in, read once.
    let mut plans: BTreeMap<String, RunPlan> = BTreeMap::new();
    for run in cases.iter().filter_map(|c| c.executed_run_id) {
        if let std::collections::btree_map::Entry::Vacant(slot) = plans.entry(run.to_string()) {
            if let Some(plan) = state.store.get_run_plan(run).await? {
                slot.insert(plan);
            }
        }
    }
    let mut tasks: Vec<(ReplayedCase, TaskDef)> = Vec::new();
    let mut skipped = Vec::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    for case in cases {
        let info = CaseInfo {
            id: case.id.to_string(),
            title: case.title.clone(),
            status: case.status,
            executed_run_id: case.executed_run_id.map(|r| r.to_string()),
            executed_task_id: case.executed_task_id.clone(),
        };
        match replay_of(&info, &plans) {
            // Two cases cannot be one task of a run.
            Replay::Repeat { task, .. } if used.contains(&task.id) => skipped.push(SkippedCase {
                case_id: info.id,
                title: info.title,
                reason: "another case was last run as the same task".to_string(),
            }),
            Replay::Repeat {
                task,
                source_run_id,
            } => {
                used.insert(task.id.clone());
                tasks.push((
                    ReplayedCase {
                        case_id: info.id,
                        task_id: task.id.clone(),
                        title: info.title,
                        source_run_id,
                    },
                    task,
                ));
            }
            Replay::Skip(reason) => skipped.push(SkippedCase {
                case_id: info.id,
                title: info.title,
                reason,
            }),
        }
    }
    Ok(Scope { tasks, skipped })
}

/// Makes one regression run of a schedule, as `actor` when a person asked for it and as the key that made the schedule when its time came.
/// It writes nothing on the schedule: the caller does, since a run asked for by hand does not move the next slot.
pub async fn run_schedule(
    state: &AppState,
    schedule: &ScheduleRow,
    trigger: Trigger,
    now: DateTime<Utc>,
    actor: Option<&AuthContext>,
) -> Result<RegressionRunRow, Blocked> {
    let refused = |e: anyhow::Error| Blocked::Refused(e.to_string());
    let project = Uuid::parse_str(&schedule.project_id).map_err(|_| Blocked::ProjectGone)?;
    let ctx = match actor {
        Some(actor) => actor.clone(),
        None => {
            // Looked up each time, so that revoking the key stops the schedule.
            let key = state
                .store
                .find_active_api_key(schedule.created_by_key)
                .await
                .map_err(refused)?
                .ok_or_else(|| {
                    Blocked::KeyGone("the key this schedule runs as was revoked".to_string())
                })?;
            AuthContext {
                key_id: key.id,
                name: key.name,
                role: Role::parse(&key.role),
                project_id: key.project_id,
            }
        }
    };
    if auth::require_role(&ctx, Role::Operator).is_err()
        || auth::require_project_access(&ctx, Some(&schedule.project_id)).is_err()
    {
        return Err(Blocked::KeyGone(
            "the key this schedule runs as may no longer run tests in this project".to_string(),
        ));
    }
    if state
        .store
        .get_project(project)
        .await
        .map_err(refused)?
        .is_none()
    {
        return Err(Blocked::ProjectGone);
    }
    if let Some(previous) = state
        .store
        .list_regression_runs(schedule.id, 1)
        .await
        .map_err(refused)?
        .into_iter()
        .next()
    {
        // A run that is no longer there is not one that is going.
        if let Ok(summary) = scoped_run_summary(state, previous.run_id, project).await {
            if !summary.state.is_terminal() {
                return Err(Blocked::StillRunning(previous.run_id));
            }
        }
    }

    let found = scope(state, &schedule.project_id, schedule.suite_id.as_deref())
        .await
        .map_err(refused)?;
    if found.tasks.is_empty() {
        return Err(Blocked::NothingToRepeat(found.skipped));
    }
    let plan = RunPlan {
        name: format!(
            "Regression: {} ({})",
            schedule.name,
            now.format("%Y-%m-%d %H:%M UTC")
        ),
        tasks: found.tasks.iter().map(|(_, task)| task.clone()).collect(),
        concurrency: schedule.concurrency.max(1) as usize,
    };
    let (_, created) = create_run_core(state, &ctx, Some(schedule.project_id.clone()), plan)
        .await
        .map_err(|e| Blocked::Refused(e.message().to_string()))?;
    let row = RegressionRunRow {
        id: Uuid::new_v4(),
        schedule_id: schedule.id,
        project_id: schedule.project_id.clone(),
        run_id: created.0.run_id,
        trigger,
        cases: found.tasks.into_iter().map(|(case, _)| case).collect(),
        skipped: found.skipped,
        created_by: ctx.name.clone(),
        created_at: now,
    };
    state
        .store
        .insert_regression_run(&row)
        .await
        .map_err(refused)?;
    audit(
        state,
        &ctx,
        "regression.run",
        Some(&row.id.to_string()),
        Some(&format!(
            "{} case(s), {}, schedule {}",
            row.cases.len(),
            trigger.as_str().to_lowercase(),
            schedule.name
        )),
        "ok",
    )
    .await;
    Ok(row)
}

/// A run asked for by a person: the schedule keeps its own slots, only its last start is written.
pub async fn run_now(
    state: &AppState,
    schedule: &ScheduleRow,
    actor: &AuthContext,
    now: DateTime<Utc>,
) -> Result<RegressionRunRow, Blocked> {
    let row = run_schedule(state, schedule, Trigger::Manual, now, Some(actor)).await?;
    let mut updated = schedule.clone();
    updated.last_started_at = Some(now);
    updated.last_note = None;
    if let Err(e) = state.store.update_regression_schedule(&updated).await {
        tracing::warn!("regression: could not write the last start of a schedule: {e}");
    }
    Ok(row)
}

/// One pass: the schedules that are due each get a run (or a note on why not) and move to their next slot. Returns how many runs it made.
pub async fn tick(state: &AppState, now: DateTime<Utc>) -> Result<usize> {
    let mut made = 0;
    for schedule in state.store.due_regression_schedules(now).await? {
        let mut updated = schedule.clone();
        updated.next_run_at = next_slot_after(schedule.anchor_at, schedule.every_minutes, now);
        match run_schedule(state, &schedule, Trigger::Schedule, now, None).await {
            Ok(row) => {
                made += 1;
                updated.last_started_at = Some(now);
                updated.last_note = (!row.skipped.is_empty())
                    .then(|| format!("{} case(s) could not be repeated", row.skipped.len()));
            }
            Err(blocked) => {
                updated.last_note = Some(blocked.note());
                if blocked.pauses() {
                    tracing::warn!(
                        "regression schedule {} paused: {}",
                        schedule.name,
                        blocked.note()
                    );
                    updated.enabled = false;
                    updated.next_run_at = schedule.next_run_at;
                }
            }
        }
        state.store.update_regression_schedule(&updated).await?;
    }
    Ok(made)
}

/// Starts the loop. The first look is one tick after the service starts, so that the start is not busy with it.
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(TICK).await;
            if let Err(e) = tick(&state, Utc::now()).await {
                tracing::error!("regression scheduler pass failed: {e}");
            }
        }
    });
}
