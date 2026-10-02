//! The tables of `0022_autopilot.sql`: autopilot runs, their steps and their timeline. See `autopilot.rs` for what they
//! mean; this file only moves them in and out, one guarded change at a time, so that two things acting on the same run at
//! once (a person cancelling while the run finishes, two clicks on Start) cannot both win.

use crate::autopilot::{RunState, StepState};
use crate::store::{now_rfc3339, parse_ts, parse_ts_opt, Store};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

// The states in which a run still counts (a project has at most one such run) and the columns of a run, as macros so that
// the statements built from them are string literals: sqlx takes no others.
macro_rules! active_states {
    () => {
        "('AWAITING_APPROVAL', 'RUNNING', 'WAITING')"
    };
}

macro_rules! run_columns {
    () => {
        "id, project_id, name, state, config, estimate, approved_at, approved_paid, created_by_key, created_by_name, current_step, error, cancel_requested, created_at, started_at, finished_at, updated_at"
    };
}

#[derive(Debug, Clone, Serialize)]
pub struct AutopilotRunRow {
    pub id: Uuid,
    pub project_id: String,
    pub name: String,
    pub state: String,
    pub config: Value,
    pub estimate: Value,
    pub approved_at: Option<DateTime<Utc>>,
    pub approved_paid: bool,
    /// The key whose identity the run's calls are made with; its id is kept out of what the API returns.
    #[serde(skip_serializing)]
    pub created_by_key: Uuid,
    pub created_by_name: String,
    pub current_step: Option<String>,
    pub error: Option<String>,
    pub cancel_requested: bool,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutopilotStepRow {
    pub run_id: Uuid,
    pub name: String,
    pub position: i64,
    pub state: String,
    pub attempts: i64,
    pub detail: Value,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutopilotEventRow {
    pub id: i64,
    pub at: DateTime<Utc>,
    pub kind: String,
    pub step: Option<String>,
    pub message: String,
    pub data: Option<Value>,
}

/// A run to be made. It is created `AWAITING_APPROVAL`, with every step `PENDING`.
pub struct NewAutopilotRun<'a> {
    pub id: Uuid,
    pub project_id: &'a str,
    pub name: &'a str,
    pub config: &'a Value,
    pub estimate: &'a Value,
    pub created_by_key: Uuid,
    pub created_by_name: &'a str,
}

/// What asking for a run to be cancelled did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    /// The run was not being worked on (waiting for approval, or stopped at a gate): it is cancelled now.
    Cancelled,
    /// The run is being worked on: it stops at the next point it can, which it looks for every few seconds.
    Requested,
    /// The run had ended already, in this state.
    AlreadyEnded(RunState),
    NotFound,
}

/// What taking a failed run over to start it again did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeOutcome {
    Resumed,
    /// The run had not failed: it is in this state.
    NotFailed(RunState),
    /// The project has another run that has not ended, so this one may not start again beside it.
    Blocked,
    NotFound,
}

fn id_of(r: &sqlx::sqlite::SqliteRow, column: &str) -> Uuid {
    Uuid::parse_str(&r.get::<String, _>(column)).unwrap_or_default()
}

/// A JSON column read back; one that does not parse (it always did when written) reads as `null`.
fn json_of(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or(Value::Null)
}

fn run_from_row(r: sqlx::sqlite::SqliteRow) -> AutopilotRunRow {
    AutopilotRunRow {
        id: id_of(&r, "id"),
        project_id: r.get("project_id"),
        name: r.get("name"),
        state: r.get("state"),
        config: json_of(&r.get::<String, _>("config")),
        estimate: json_of(&r.get::<String, _>("estimate")),
        approved_at: parse_ts_opt(r.get("approved_at")),
        approved_paid: r.get::<i64, _>("approved_paid") != 0,
        created_by_key: id_of(&r, "created_by_key"),
        created_by_name: r.get("created_by_name"),
        current_step: r.get("current_step"),
        error: r.get("error"),
        cancel_requested: r.get::<i64, _>("cancel_requested") != 0,
        created_at: parse_ts(&r.get::<String, _>("created_at")),
        started_at: parse_ts_opt(r.get("started_at")),
        finished_at: parse_ts_opt(r.get("finished_at")),
        updated_at: parse_ts(&r.get::<String, _>("updated_at")),
    }
}

impl Store {
    /// Makes the run and its steps, unless the project already has a run that has not ended. `false`: it has one.
    /// Decided by the one statement that inserts, so two requests at once cannot both make a run.
    pub async fn insert_autopilot_run(
        &self,
        new: &NewAutopilotRun<'_>,
        steps: &[&str],
    ) -> Result<bool> {
        let now = now_rfc3339();
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query(concat!(
            "INSERT INTO autopilot_runs (id, project_id, name, state, config, estimate, created_by_key, created_by_name, created_at, updated_at)
             SELECT ?, ?, ?, 'AWAITING_APPROVAL', ?, ?, ?, ?, ?, ?
             WHERE NOT EXISTS (SELECT 1 FROM autopilot_runs WHERE project_id = ? AND state IN ",
            active_states!(),
            ")"
        ))
        .bind(new.id.to_string())
        .bind(new.project_id)
        .bind(new.name)
        .bind(new.config.to_string())
        .bind(new.estimate.to_string())
        .bind(new.created_by_key.to_string())
        .bind(new.created_by_name)
        .bind(&now)
        .bind(&now)
        .bind(new.project_id)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            > 0;
        if !inserted {
            return Ok(false);
        }
        for (position, name) in steps.iter().enumerate() {
            sqlx::query(
                "INSERT INTO autopilot_steps (run_id, name, position, state) VALUES (?, ?, ?, 'PENDING')",
            )
            .bind(new.id.to_string())
            .bind(name)
            .bind(position as i64)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO autopilot_events (run_id, at, kind, step, message, data) VALUES (?, ?, ?, NULL, ?, NULL)",
        )
        .bind(new.id.to_string())
        .bind(&now)
        .bind(crate::autopilot::event::CREATED)
        .bind(format!(
            "Run created by {}; it starts when the estimate is accepted",
            new.created_by_name
        ))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn get_autopilot_run(&self, id: Uuid) -> Result<Option<AutopilotRunRow>> {
        let row = sqlx::query(concat!(
            "SELECT ",
            run_columns!(),
            " FROM autopilot_runs WHERE id = ?"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(run_from_row))
    }

    /// A project's runs, newest first.
    pub async fn list_autopilot_runs(
        &self,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<AutopilotRunRow>> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            run_columns!(),
            " FROM autopilot_runs WHERE project_id = ? ORDER BY created_at DESC, id DESC LIMIT ?"
        ))
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(run_from_row).collect())
    }

    /// The project's run that has not ended, if it has one.
    pub async fn find_active_autopilot_run(
        &self,
        project_id: &str,
    ) -> Result<Option<AutopilotRunRow>> {
        let row = sqlx::query(concat!(
            "SELECT ",
            run_columns!(),
            " FROM autopilot_runs WHERE project_id = ? AND state IN ",
            active_states!(),
            " ORDER BY created_at DESC LIMIT 1"
        ))
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(run_from_row))
    }

    /// The ids of the runs in `state` (what a starting service picks its unfinished runs up by).
    pub async fn autopilot_run_ids_in_state(&self, state: RunState) -> Result<Vec<Uuid>> {
        let rows = sqlx::query("SELECT id FROM autopilot_runs WHERE state = ? ORDER BY created_at")
            .bind(state.as_str())
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(|r| id_of(r, "id")).collect())
    }

    /// The person accepted the estimate: the run goes from awaiting approval to running. `false`: it was not awaiting
    /// approval (already started, cancelled, ...), and nothing changed.
    pub async fn start_autopilot_run(&self, id: Uuid, paid: bool) -> Result<bool> {
        let now = now_rfc3339();
        let changed = sqlx::query(
            "UPDATE autopilot_runs SET state = 'RUNNING', approved_at = ?, approved_paid = ?, started_at = ?, updated_at = ?
             WHERE id = ? AND state = 'AWAITING_APPROVAL'",
        )
        .bind(&now)
        .bind(i64::from(paid))
        .bind(&now)
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(changed > 0)
    }

    /// A running run stops at a gate. `false`: it was not running.
    pub async fn wait_autopilot_run(&self, id: Uuid) -> Result<bool> {
        let changed = sqlx::query(
            "UPDATE autopilot_runs SET state = 'WAITING', updated_at = ? WHERE id = ? AND state = 'RUNNING'",
        )
        .bind(now_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(changed > 0)
    }

    /// A person lets a run that waits at a gate go on: it runs again, and the step that waits is told it was let through
    /// (`released` in its detail). `false`: the run was not waiting, and nothing changed.
    pub async fn continue_autopilot_run(&self, id: Uuid) -> Result<bool> {
        let now = now_rfc3339();
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query(
            "UPDATE autopilot_runs SET state = 'RUNNING', updated_at = ? WHERE id = ? AND state = 'WAITING'",
        )
        .bind(&now)
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed == 0 {
            return Ok(false);
        }
        let waiting = sqlx::query(
            "SELECT name, detail FROM autopilot_steps WHERE run_id = ? AND state = 'WAITING'",
        )
        .bind(id.to_string())
        .fetch_all(&mut *tx)
        .await?;
        for step in waiting {
            let mut detail = json_of(&step.get::<String, _>("detail"));
            if !detail.is_object() {
                detail = Value::Object(Default::default());
            }
            detail["released"] = Value::Bool(true);
            sqlx::query("UPDATE autopilot_steps SET detail = ? WHERE run_id = ? AND name = ?")
                .bind(detail.to_string())
                .bind(id.to_string())
                .bind(step.get::<String, _>("name"))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    /// A person takes a run that failed over and starts it again, at the step that failed: what was done before it is kept.
    /// From then on the run acts as `key`, the person's own (the key that made it may be gone). Refused while the project
    /// has another run that has not ended.
    pub async fn resume_failed_autopilot_run(
        &self,
        id: Uuid,
        key: Uuid,
        name: &str,
    ) -> Result<ResumeOutcome> {
        let changed = sqlx::query(concat!(
            "UPDATE autopilot_runs SET state = 'RUNNING', error = NULL, finished_at = NULL, cancel_requested = 0,
             created_by_key = ?, created_by_name = ?, updated_at = ?
             WHERE id = ? AND state = 'FAILED'
             AND NOT EXISTS (SELECT 1 FROM autopilot_runs other WHERE other.project_id = autopilot_runs.project_id
                             AND other.id <> autopilot_runs.id AND other.state IN ",
            active_states!(),
            ")"
        ))
        .bind(key.to_string())
        .bind(name)
        .bind(now_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await?
        .rows_affected();
        if changed > 0 {
            return Ok(ResumeOutcome::Resumed);
        }
        Ok(match self.get_autopilot_run(id).await? {
            None => ResumeOutcome::NotFound,
            Some(run) => match RunState::parse(&run.state) {
                Some(RunState::Failed) => ResumeOutcome::Blocked,
                Some(state) => ResumeOutcome::NotFailed(state),
                None => ResumeOutcome::NotFailed(RunState::Failed),
            },
        })
    }

    /// A run that has not ended ends, in `to` (which must be an ending state), with the reason it failed if it did.
    /// `false`: it had ended already, and nothing changed.
    pub async fn finish_autopilot_run(
        &self,
        id: Uuid,
        to: RunState,
        error: Option<&str>,
    ) -> Result<bool> {
        debug_assert!(to.is_terminal(), "{to:?} is not an end");
        let now = now_rfc3339();
        let changed = sqlx::query(concat!(
            "UPDATE autopilot_runs SET state = ?, error = ?, finished_at = ?, updated_at = ? WHERE id = ? AND state IN ",
            active_states!()
        ))
        .bind(to.as_str())
        .bind(error)
        .bind(&now)
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(changed > 0)
    }

    /// A person asks for the run to stop. A run nobody is working on stops now, and its steps that were not done are
    /// marked skipped; a run being worked on is asked to, and stops at the next point it can.
    pub async fn cancel_autopilot_run(&self, id: Uuid) -> Result<CancelOutcome> {
        let now = now_rfc3339();
        // Reads the run before it writes, so the write lock is taken at BEGIN (see `Store::begin_write`).
        let mut tx = self.begin_write().await?;
        let Some(row) = sqlx::query("SELECT state FROM autopilot_runs WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&mut *tx)
            .await?
        else {
            return Ok(CancelOutcome::NotFound);
        };
        let state = RunState::parse(&row.get::<String, _>("state")).unwrap_or(RunState::Failed);
        let outcome = match state {
            state if state.is_terminal() => return Ok(CancelOutcome::AlreadyEnded(state)),
            RunState::Running => {
                sqlx::query(
                    "UPDATE autopilot_runs SET cancel_requested = 1, updated_at = ? WHERE id = ?",
                )
                .bind(&now)
                .bind(id.to_string())
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "INSERT INTO autopilot_events (run_id, at, kind, step, message, data) VALUES (?, ?, ?, NULL, ?, NULL)",
                )
                .bind(id.to_string())
                .bind(&now)
                .bind(crate::autopilot::event::NOTE)
                .bind("Cancel requested: the run stops at the next point it can")
                .execute(&mut *tx)
                .await?;
                CancelOutcome::Requested
            }
            _ => {
                sqlx::query(
                    "UPDATE autopilot_runs SET state = 'CANCELLED', finished_at = ?, updated_at = ? WHERE id = ?",
                )
                .bind(&now)
                .bind(&now)
                .bind(id.to_string())
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "UPDATE autopilot_steps SET state = 'SKIPPED', error = 'the run was cancelled', finished_at = ?
                     WHERE run_id = ? AND state IN ('PENDING', 'WAITING')",
                )
                .bind(&now)
                .bind(id.to_string())
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "INSERT INTO autopilot_events (run_id, at, kind, step, message, data) VALUES (?, ?, ?, NULL, ?, NULL)",
                )
                .bind(id.to_string())
                .bind(&now)
                .bind(crate::autopilot::event::CANCELLED)
                .bind("Run cancelled")
                .execute(&mut *tx)
                .await?;
                CancelOutcome::Cancelled
            }
        };
        tx.commit().await?;
        Ok(outcome)
    }

    // ------------------------------------------------------------------------------------------ steps

    /// A run's steps, in the order they are done.
    pub async fn list_autopilot_steps(&self, run_id: Uuid) -> Result<Vec<AutopilotStepRow>> {
        let rows = sqlx::query(
            "SELECT run_id, name, position, state, attempts, detail, error, started_at, finished_at
             FROM autopilot_steps WHERE run_id = ? ORDER BY position",
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| AutopilotStepRow {
                run_id: id_of(&r, "run_id"),
                name: r.get("name"),
                position: r.get("position"),
                state: r.get("state"),
                attempts: r.get("attempts"),
                detail: json_of(&r.get::<String, _>("detail")),
                error: r.get("error"),
                started_at: parse_ts_opt(r.get("started_at")),
                finished_at: parse_ts_opt(r.get("finished_at")),
            })
            .collect())
    }

    /// A step is started (again, after a restart): running, one more attempt, and it is the run's current step.
    pub async fn begin_autopilot_step(&self, run_id: Uuid, name: &str) -> Result<()> {
        let now = now_rfc3339();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE autopilot_steps SET state = 'RUNNING', attempts = attempts + 1, error = NULL, finished_at = NULL,
             started_at = COALESCE(started_at, ?) WHERE run_id = ? AND name = ?",
        )
        .bind(&now)
        .bind(run_id.to_string())
        .bind(name)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE autopilot_runs SET current_step = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(&now)
            .bind(run_id.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// What a step has done so far, kept so that a step started again after a restart carries on from it.
    pub async fn save_autopilot_step_detail(
        &self,
        run_id: Uuid,
        name: &str,
        detail: &Value,
    ) -> Result<()> {
        sqlx::query("UPDATE autopilot_steps SET detail = ? WHERE run_id = ? AND name = ?")
            .bind(detail.to_string())
            .bind(run_id.to_string())
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// A step stops being worked on, in `state`: with what it found (`detail`, when it has something to say) and why it
    /// failed (`error`) if it did. A step that waits is not finished, so it has no finishing time yet.
    pub async fn settle_autopilot_step(
        &self,
        run_id: Uuid,
        name: &str,
        state: StepState,
        detail: Option<&Value>,
        error: Option<&str>,
    ) -> Result<()> {
        let finished_at = (state != StepState::Waiting).then(now_rfc3339);
        sqlx::query(
            "UPDATE autopilot_steps SET state = ?, detail = COALESCE(?, detail), error = ?, finished_at = ?
             WHERE run_id = ? AND name = ?",
        )
        .bind(state.as_str())
        .bind(detail.map(Value::to_string))
        .bind(error)
        .bind(finished_at)
        .bind(run_id.to_string())
        .bind(name)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The steps of a run that has ended that were never finished (not started, waiting, or cut off) are marked skipped,
    /// with the reason.
    pub async fn skip_unfinished_autopilot_steps(&self, run_id: Uuid, why: &str) -> Result<()> {
        sqlx::query(
            "UPDATE autopilot_steps SET state = 'SKIPPED', error = ?, finished_at = ?
             WHERE run_id = ? AND state IN ('PENDING', 'WAITING', 'RUNNING')",
        )
        .bind(why)
        .bind(now_rfc3339())
        .bind(run_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ------------------------------------------------------------------------------------------ timeline

    pub async fn add_autopilot_event(
        &self,
        run_id: Uuid,
        kind: &str,
        step: Option<&str>,
        message: &str,
        data: Option<&Value>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO autopilot_events (run_id, at, kind, step, message, data) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(run_id.to_string())
        .bind(now_rfc3339())
        .bind(kind)
        .bind(step)
        .bind(message)
        .bind(data.map(Value::to_string))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// A run's timeline, oldest first: the events after `after` (an event id) when it is given, otherwise the latest
    /// `limit` of them.
    pub async fn list_autopilot_events(
        &self,
        run_id: Uuid,
        after: Option<i64>,
        limit: i64,
    ) -> Result<Vec<AutopilotEventRow>> {
        let rows = match after {
            Some(after) => {
                sqlx::query(
                    "SELECT id, at, kind, step, message, data FROM autopilot_events
                     WHERE run_id = ? AND id > ? ORDER BY id LIMIT ?",
                )
                .bind(run_id.to_string())
                .bind(after)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT id, at, kind, step, message, data FROM (
                         SELECT id, at, kind, step, message, data FROM autopilot_events WHERE run_id = ? ORDER BY id DESC LIMIT ?
                     ) ORDER BY id",
                )
                .bind(run_id.to_string())
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(rows
            .into_iter()
            .map(|r| AutopilotEventRow {
                id: r.get("id"),
                at: parse_ts(&r.get::<String, _>("at")),
                kind: r.get("kind"),
                step: r.get("step"),
                message: r.get("message"),
                data: r
                    .get::<Option<String>, _>("data")
                    .map(|text| json_of(&text)),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn temp_store() -> (Store, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::connect(&dir.path().join("test.sqlite3"))
            .await
            .expect("connect");
        (store, dir)
    }

    const STEPS: [&str; 3] = ["REQUIREMENTS", "CRAWL", "GAP_CHECK"];

    async fn make_run(store: &Store, project: &str) -> Uuid {
        let id = Uuid::new_v4();
        let config = serde_json::json!({"base_url": "https://shop.example.test"});
        let estimate = serde_json::json!({"totals": {"paid": false}});
        let made = store
            .insert_autopilot_run(
                &NewAutopilotRun {
                    id,
                    project_id: project,
                    name: "Release 4",
                    config: &config,
                    estimate: &estimate,
                    created_by_key: Uuid::new_v4(),
                    created_by_name: "ci-key",
                },
                &STEPS,
            )
            .await
            .unwrap();
        assert!(made, "the project had no run yet");
        id
    }

    #[tokio::test]
    async fn a_run_is_made_awaiting_approval_with_its_steps_pending_and_a_first_event() {
        let (store, _dir) = temp_store().await;
        let id = make_run(&store, "p1").await;

        let run = store.get_autopilot_run(id).await.unwrap().unwrap();
        assert_eq!(run.state, "AWAITING_APPROVAL");
        assert_eq!(
            (run.project_id.as_str(), run.name.as_str()),
            ("p1", "Release 4")
        );
        assert_eq!(run.config["base_url"], "https://shop.example.test");
        assert_eq!(run.estimate["totals"]["paid"], false);
        assert_eq!(run.created_by_name, "ci-key");
        assert!(run.approved_at.is_none() && !run.approved_paid);
        assert!(run.started_at.is_none() && run.finished_at.is_none());
        assert!(run.current_step.is_none() && run.error.is_none() && !run.cancel_requested);

        let steps = store.list_autopilot_steps(id).await.unwrap();
        let names: Vec<(&str, i64, &str)> = steps
            .iter()
            .map(|s| (s.name.as_str(), s.position, s.state.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("REQUIREMENTS", 0, "PENDING"),
                ("CRAWL", 1, "PENDING"),
                ("GAP_CHECK", 2, "PENDING")
            ]
        );
        assert!(steps
            .iter()
            .all(|s| s.attempts == 0 && s.detail == serde_json::json!({})));

        let events = store.list_autopilot_events(id, None, 10).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "run.created");
        assert!(
            events[0].message.contains("ci-key"),
            "{}",
            events[0].message
        );
        assert!(store
            .get_autopilot_run(Uuid::new_v4())
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn a_project_has_one_run_that_has_not_ended_and_another_may_be_made_once_it_has() {
        let (store, _dir) = temp_store().await;
        let first = make_run(&store, "p1").await;
        let config = serde_json::json!({});
        let estimate = serde_json::json!({});
        let second = |project: &'static str| NewAutopilotRun {
            id: Uuid::new_v4(),
            project_id: project,
            name: "again",
            config: &config,
            estimate: &estimate,
            created_by_key: Uuid::new_v4(),
            created_by_name: "k",
        };
        // Awaiting approval counts, and so does every other state that has not ended.
        assert!(!store
            .insert_autopilot_run(&second("p1"), &STEPS)
            .await
            .unwrap());
        assert!(store.start_autopilot_run(first, false).await.unwrap());
        assert!(!store
            .insert_autopilot_run(&second("p1"), &STEPS)
            .await
            .unwrap());
        assert!(store.wait_autopilot_run(first).await.unwrap());
        assert!(!store
            .insert_autopilot_run(&second("p1"), &STEPS)
            .await
            .unwrap());
        // A refused run leaves nothing behind.
        assert_eq!(store.list_autopilot_runs("p1", 10).await.unwrap().len(), 1);
        // Another project is another matter.
        assert!(store
            .insert_autopilot_run(&second("p2"), &STEPS)
            .await
            .unwrap());
        // Once it has ended (in any way), a new run may be made.
        assert!(store
            .finish_autopilot_run(first, RunState::Failed, Some("boom"))
            .await
            .unwrap());
        assert!(store
            .insert_autopilot_run(&second("p1"), &STEPS)
            .await
            .unwrap());
        assert_eq!(store.list_autopilot_runs("p1", 10).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn the_active_run_of_a_project_is_the_one_that_has_not_ended() {
        let (store, _dir) = temp_store().await;
        assert!(store
            .find_active_autopilot_run("p1")
            .await
            .unwrap()
            .is_none());
        let id = make_run(&store, "p1").await;
        assert_eq!(
            store
                .find_active_autopilot_run("p1")
                .await
                .unwrap()
                .unwrap()
                .id,
            id
        );
        assert!(store
            .find_active_autopilot_run("p2")
            .await
            .unwrap()
            .is_none());
        store
            .finish_autopilot_run(id, RunState::Completed, None)
            .await
            .unwrap();
        assert!(store
            .find_active_autopilot_run("p1")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn runs_are_listed_newest_first_and_only_the_projects_own() {
        let (store, _dir) = temp_store().await;
        let old = make_run(&store, "p1").await;
        store
            .finish_autopilot_run(old, RunState::Completed, None)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let new = make_run(&store, "p1").await;
        let other = make_run(&store, "p2").await;
        let ids: Vec<Uuid> = store
            .list_autopilot_runs("p1", 10)
            .await
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, [new, old]);
        let ids: Vec<Uuid> = store
            .list_autopilot_runs("p2", 10)
            .await
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, [other]);
        assert_eq!(
            store.list_autopilot_runs("p1", 1).await.unwrap().len(),
            1,
            "limited"
        );
        assert!(store
            .list_autopilot_runs("nobody", 10)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn starting_a_run_records_the_approval_once_and_only_from_awaiting_approval() {
        let (store, _dir) = temp_store().await;
        let id = make_run(&store, "p1").await;
        assert!(store.start_autopilot_run(id, true).await.unwrap());
        let run = store.get_autopilot_run(id).await.unwrap().unwrap();
        assert_eq!(run.state, "RUNNING");
        assert!(run.approved_at.is_some() && run.started_at.is_some());
        assert!(run.approved_paid, "what was accepted is kept");
        let approved_at = run.approved_at;
        // A second start changes nothing.
        assert!(!store.start_autopilot_run(id, false).await.unwrap());
        let again = store.get_autopilot_run(id).await.unwrap().unwrap();
        assert!(again.approved_paid);
        assert_eq!(again.approved_at, approved_at);
        assert!(!store
            .start_autopilot_run(Uuid::new_v4(), false)
            .await
            .unwrap());
        // A free run records that too.
        let (store2, _dir2) = temp_store().await;
        let free = make_run(&store2, "p1").await;
        store2.start_autopilot_run(free, false).await.unwrap();
        assert!(
            !store2
                .get_autopilot_run(free)
                .await
                .unwrap()
                .unwrap()
                .approved_paid
        );
    }

    #[tokio::test]
    async fn a_run_waits_only_from_running_and_ends_only_once() {
        let (store, _dir) = temp_store().await;
        let id = make_run(&store, "p1").await;
        assert!(
            !store.wait_autopilot_run(id).await.unwrap(),
            "not running yet"
        );
        store.start_autopilot_run(id, false).await.unwrap();
        assert!(store.wait_autopilot_run(id).await.unwrap());
        assert_eq!(
            store.get_autopilot_run(id).await.unwrap().unwrap().state,
            "WAITING"
        );
        assert!(
            !store.wait_autopilot_run(id).await.unwrap(),
            "already waiting"
        );

        assert!(store
            .finish_autopilot_run(id, RunState::Failed, Some("the crawl failed"))
            .await
            .unwrap());
        let run = store.get_autopilot_run(id).await.unwrap().unwrap();
        assert_eq!(
            (run.state.as_str(), run.error.as_deref()),
            ("FAILED", Some("the crawl failed"))
        );
        assert!(run.finished_at.is_some());
        // An ended run stays as it ended.
        assert!(!store
            .finish_autopilot_run(id, RunState::Completed, None)
            .await
            .unwrap());
        assert!(!store.wait_autopilot_run(id).await.unwrap());
        assert!(!store.start_autopilot_run(id, false).await.unwrap());
        let run = store.get_autopilot_run(id).await.unwrap().unwrap();
        assert_eq!(
            (run.state.as_str(), run.error.as_deref()),
            ("FAILED", Some("the crawl failed"))
        );
        // Completing leaves no error.
        let (store2, _dir2) = temp_store().await;
        let done = make_run(&store2, "p1").await;
        store2.start_autopilot_run(done, false).await.unwrap();
        assert!(store2
            .finish_autopilot_run(done, RunState::Completed, None)
            .await
            .unwrap());
        let run = store2.get_autopilot_run(done).await.unwrap().unwrap();
        assert_eq!(run.state, "COMPLETED");
        assert!(run.error.is_none() && run.finished_at.is_some());
    }

    async fn step_detail(store: &Store, run: Uuid, name: &str) -> Value {
        store
            .list_autopilot_steps(run)
            .await
            .unwrap()
            .into_iter()
            .find(|s| s.name == name)
            .unwrap()
            .detail
    }

    #[tokio::test]
    async fn a_run_at_a_gate_goes_on_when_a_person_lets_it_and_the_step_that_waited_is_told_so() {
        let (store, _dir) = temp_store().await;
        let id = make_run(&store, "p1").await;
        assert!(
            !store.continue_autopilot_run(id).await.unwrap(),
            "not waiting: awaiting approval"
        );
        store.start_autopilot_run(id, false).await.unwrap();
        assert!(
            !store.continue_autopilot_run(id).await.unwrap(),
            "not waiting: running"
        );

        store
            .begin_autopilot_step(id, "REQUIREMENTS")
            .await
            .unwrap();
        store
            .settle_autopilot_step(
                id,
                "REQUIREMENTS",
                StepState::Done,
                Some(&serde_json::json!({"requirements": 6})),
                None,
            )
            .await
            .unwrap();
        store.begin_autopilot_step(id, "CRAWL").await.unwrap();
        store
            .settle_autopilot_step(
                id,
                "CRAWL",
                StepState::Waiting,
                Some(&serde_json::json!({"cases": 40})),
                None,
            )
            .await
            .unwrap();
        store.wait_autopilot_run(id).await.unwrap();

        assert!(store.continue_autopilot_run(id).await.unwrap());
        assert_eq!(
            store.get_autopilot_run(id).await.unwrap().unwrap().state,
            "RUNNING"
        );
        assert_eq!(
            step_detail(&store, id, "CRAWL").await,
            serde_json::json!({"cases": 40, "released": true}),
            "what it saved is kept"
        );
        assert_eq!(
            step_detail(&store, id, "REQUIREMENTS").await,
            serde_json::json!({"requirements": 6}),
            "another step is left alone"
        );
        assert!(!store.continue_autopilot_run(id).await.unwrap(), "once");

        // A step whose saved detail is not an object still gets the flag.
        let (store2, _dir2) = temp_store().await;
        let other = make_run(&store2, "p1").await;
        store2.start_autopilot_run(other, false).await.unwrap();
        store2.begin_autopilot_step(other, "CRAWL").await.unwrap();
        sqlx::query("UPDATE autopilot_steps SET state = 'WAITING', detail = '[1]' WHERE run_id = ? AND name = 'CRAWL'")
            .bind(other.to_string())
            .execute(&store2.pool)
            .await
            .unwrap();
        store2.wait_autopilot_run(other).await.unwrap();
        assert!(store2.continue_autopilot_run(other).await.unwrap());
        assert_eq!(
            step_detail(&store2, other, "CRAWL").await,
            serde_json::json!({"released": true})
        );
    }

    #[tokio::test]
    async fn a_failed_run_is_taken_over_and_started_again_only_when_the_project_has_no_other_run_going(
    ) {
        let (store, _dir) = temp_store().await;
        let id = make_run(&store, "p1").await;
        let key = Uuid::new_v4();
        assert_eq!(
            store
                .resume_failed_autopilot_run(id, key, "second")
                .await
                .unwrap(),
            ResumeOutcome::NotFailed(RunState::AwaitingApproval)
        );
        store.start_autopilot_run(id, false).await.unwrap();
        assert_eq!(
            store
                .resume_failed_autopilot_run(id, key, "second")
                .await
                .unwrap(),
            ResumeOutcome::NotFailed(RunState::Running)
        );
        assert_eq!(
            store
                .resume_failed_autopilot_run(Uuid::new_v4(), key, "second")
                .await
                .unwrap(),
            ResumeOutcome::NotFound
        );

        store.begin_autopilot_step(id, "CRAWL").await.unwrap();
        store
            .settle_autopilot_step(id, "CRAWL", StepState::Failed, None, Some("no page"))
            .await
            .unwrap();
        store
            .finish_autopilot_run(
                id,
                RunState::Failed,
                Some("Crawling the application: no page"),
            )
            .await
            .unwrap();
        sqlx::query("UPDATE autopilot_runs SET cancel_requested = 1 WHERE id = ?")
            .bind(id.to_string())
            .execute(&store.pool)
            .await
            .unwrap();

        // Another run of the project has been made since: this one may not start beside it.
        let config = serde_json::json!({});
        let estimate = serde_json::json!({});
        let blocker = Uuid::new_v4();
        assert!(store
            .insert_autopilot_run(
                &NewAutopilotRun {
                    id: blocker,
                    project_id: "p1",
                    name: "b",
                    config: &config,
                    estimate: &estimate,
                    created_by_key: Uuid::new_v4(),
                    created_by_name: "k"
                },
                &STEPS,
            )
            .await
            .unwrap());
        assert_eq!(
            store
                .resume_failed_autopilot_run(id, key, "second")
                .await
                .unwrap(),
            ResumeOutcome::Blocked
        );
        assert_eq!(
            store.get_autopilot_run(id).await.unwrap().unwrap().state,
            "FAILED"
        );
        store.cancel_autopilot_run(blocker).await.unwrap();

        assert_eq!(
            store
                .resume_failed_autopilot_run(id, key, "second")
                .await
                .unwrap(),
            ResumeOutcome::Resumed
        );
        let run = store.get_autopilot_run(id).await.unwrap().unwrap();
        assert_eq!(run.state, "RUNNING");
        assert!(run.error.is_none() && run.finished_at.is_none() && !run.cancel_requested);
        assert_eq!(
            (run.created_by_key, run.created_by_name.as_str()),
            (key, "second"),
            "the run acts as the person who took it over"
        );
        // The step that failed is started again, at that step, with the others as they were.
        store.begin_autopilot_step(id, "CRAWL").await.unwrap();
        let steps = store.list_autopilot_steps(id).await.unwrap();
        assert_eq!(steps[1].attempts, 2);
        assert!(steps[1].error.is_none());
        // Once, and not from any other state.
        assert_eq!(
            store
                .resume_failed_autopilot_run(id, key, "third")
                .await
                .unwrap(),
            ResumeOutcome::NotFailed(RunState::Running)
        );
    }

    #[tokio::test]
    async fn runs_are_found_by_state_oldest_first() {
        let (store, _dir) = temp_store().await;
        let a = make_run(&store, "p1").await;
        let b = make_run(&store, "p2").await;
        let c = make_run(&store, "p3").await;
        store.start_autopilot_run(a, false).await.unwrap();
        store.start_autopilot_run(c, false).await.unwrap();
        assert_eq!(
            store
                .autopilot_run_ids_in_state(RunState::Running)
                .await
                .unwrap(),
            [a, c]
        );
        assert_eq!(
            store
                .autopilot_run_ids_in_state(RunState::AwaitingApproval)
                .await
                .unwrap(),
            [b]
        );
        assert!(store
            .autopilot_run_ids_in_state(RunState::Waiting)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn cancelling_a_run_nobody_is_working_on_ends_it_and_skips_its_unfinished_steps() {
        let (store, _dir) = temp_store().await;
        // Awaiting approval.
        let id = make_run(&store, "p1").await;
        assert_eq!(
            store.cancel_autopilot_run(id).await.unwrap(),
            CancelOutcome::Cancelled
        );
        let run = store.get_autopilot_run(id).await.unwrap().unwrap();
        assert_eq!(run.state, "CANCELLED");
        assert!(run.finished_at.is_some() && !run.cancel_requested);
        let steps = store.list_autopilot_steps(id).await.unwrap();
        assert!(steps.iter().all(|s| s.state == "SKIPPED"), "{steps:?}");
        assert_eq!(steps[0].error.as_deref(), Some("the run was cancelled"));
        let events = store.list_autopilot_events(id, None, 10).await.unwrap();
        assert_eq!(events.last().unwrap().kind, "run.cancelled");
        // Twice: it has ended.
        assert_eq!(
            store.cancel_autopilot_run(id).await.unwrap(),
            CancelOutcome::AlreadyEnded(RunState::Cancelled)
        );
        // Waiting at a gate: the finished steps stay as they were, the one that waits is skipped.
        let (store2, _dir2) = temp_store().await;
        let waiting = make_run(&store2, "p1").await;
        store2.start_autopilot_run(waiting, false).await.unwrap();
        store2
            .begin_autopilot_step(waiting, "REQUIREMENTS")
            .await
            .unwrap();
        store2
            .settle_autopilot_step(waiting, "REQUIREMENTS", StepState::Done, None, None)
            .await
            .unwrap();
        store2.begin_autopilot_step(waiting, "CRAWL").await.unwrap();
        store2
            .settle_autopilot_step(waiting, "CRAWL", StepState::Waiting, None, None)
            .await
            .unwrap();
        store2.wait_autopilot_run(waiting).await.unwrap();
        assert_eq!(
            store2.cancel_autopilot_run(waiting).await.unwrap(),
            CancelOutcome::Cancelled
        );
        let states: Vec<String> = store2
            .list_autopilot_steps(waiting)
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.state)
            .collect();
        assert_eq!(states, ["DONE", "SKIPPED", "SKIPPED"]);
    }

    #[tokio::test]
    async fn cancelling_a_running_run_only_asks_it_to_stop() {
        let (store, _dir) = temp_store().await;
        let id = make_run(&store, "p1").await;
        store.start_autopilot_run(id, false).await.unwrap();
        assert_eq!(
            store.cancel_autopilot_run(id).await.unwrap(),
            CancelOutcome::Requested
        );
        let run = store.get_autopilot_run(id).await.unwrap().unwrap();
        assert_eq!(
            run.state, "RUNNING",
            "the run ends when it notices, not before"
        );
        assert!(run.cancel_requested && run.finished_at.is_none());
        let steps = store.list_autopilot_steps(id).await.unwrap();
        assert!(steps.iter().all(|s| s.state == "PENDING"));
        let events = store.list_autopilot_events(id, None, 10).await.unwrap();
        assert_eq!(events.last().unwrap().kind, "note");
        assert!(events.last().unwrap().message.contains("Cancel requested"));
        assert_eq!(
            store.cancel_autopilot_run(Uuid::new_v4()).await.unwrap(),
            CancelOutcome::NotFound
        );
    }

    /// A person cancels a run while its own writers are busy: another connection holds SQLite's write lock. A cancel that reads
    /// the run first and asks for the lock after is refused at once (`database is locked`, and the busy timeout does not apply);
    /// one that takes the lock before it reads waits its turn.
    #[tokio::test]
    async fn cancelling_a_run_waits_for_another_writer_instead_of_failing_with_database_is_locked()
    {
        let (store, _dir) = temp_store().await;
        let id = make_run(&store, "p1").await;
        store.start_autopilot_run(id, false).await.unwrap();

        let writer = store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        let cancelling = tokio::spawn({
            let store = store.clone();
            async move { store.cancel_autopilot_run(id).await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if cancelling.is_finished() {
            panic!(
                "the cancel did not wait for the write lock: {:?}",
                cancelling.await
            );
        }
        writer.commit().await.unwrap();

        assert_eq!(cancelling.await.unwrap().unwrap(), CancelOutcome::Requested);
        let run = store.get_autopilot_run(id).await.unwrap().unwrap();
        assert!(run.cancel_requested);
    }

    #[tokio::test]
    async fn a_step_is_started_counted_checkpointed_and_settled() {
        let (store, _dir) = temp_store().await;
        let id = make_run(&store, "p1").await;
        store.begin_autopilot_step(id, "CRAWL").await.unwrap();
        let step =
            |steps: Vec<AutopilotStepRow>| steps.into_iter().find(|s| s.name == "CRAWL").unwrap();
        let s = step(store.list_autopilot_steps(id).await.unwrap());
        assert_eq!((s.state.as_str(), s.attempts), ("RUNNING", 1));
        assert!(s.started_at.is_some() && s.finished_at.is_none());
        assert_eq!(
            store
                .get_autopilot_run(id)
                .await
                .unwrap()
                .unwrap()
                .current_step
                .as_deref(),
            Some("CRAWL")
        );
        let started = s.started_at;

        store
            .save_autopilot_step_detail(id, "CRAWL", &serde_json::json!({"crawl_run_id": "abc"}))
            .await
            .unwrap();
        let s = step(store.list_autopilot_steps(id).await.unwrap());
        assert_eq!(s.detail["crawl_run_id"], "abc");
        assert_eq!(s.state, "RUNNING", "a checkpoint does not settle the step");

        // Started again (after a restart): one more attempt, the first start time and the checkpoint kept.
        store.begin_autopilot_step(id, "CRAWL").await.unwrap();
        let s = step(store.list_autopilot_steps(id).await.unwrap());
        assert_eq!(s.attempts, 2);
        assert_eq!(s.started_at, started);
        assert_eq!(s.detail["crawl_run_id"], "abc");

        // Settled with what it found: the detail is replaced.
        store
            .settle_autopilot_step(
                id,
                "CRAWL",
                StepState::Done,
                Some(&serde_json::json!({"pages": 12})),
                None,
            )
            .await
            .unwrap();
        let s = step(store.list_autopilot_steps(id).await.unwrap());
        assert_eq!(s.state, "DONE");
        assert_eq!(s.detail, serde_json::json!({"pages": 12}));
        assert!(s.finished_at.is_some() && s.error.is_none());

        // Settled with nothing to add: what it had is kept. Failing gives the reason.
        store.begin_autopilot_step(id, "CRAWL").await.unwrap();
        let s = step(store.list_autopilot_steps(id).await.unwrap());
        assert!(
            s.finished_at.is_none(),
            "a step started again is not finished"
        );
        store
            .settle_autopilot_step(id, "CRAWL", StepState::Failed, None, Some("no login page"))
            .await
            .unwrap();
        let s = step(store.list_autopilot_steps(id).await.unwrap());
        assert_eq!(
            (s.state.as_str(), s.error.as_deref()),
            ("FAILED", Some("no login page"))
        );
        assert_eq!(s.detail, serde_json::json!({"pages": 12}));
        assert!(s.finished_at.is_some());
        // Starting it again clears the old failure.
        store.begin_autopilot_step(id, "CRAWL").await.unwrap();
        assert!(step(store.list_autopilot_steps(id).await.unwrap())
            .error
            .is_none());
        // A step that waits has not finished.
        store
            .settle_autopilot_step(id, "CRAWL", StepState::Waiting, None, None)
            .await
            .unwrap();
        let s = step(store.list_autopilot_steps(id).await.unwrap());
        assert_eq!(s.state, "WAITING");
        assert!(s.finished_at.is_none());
        // The other steps were left alone.
        let others: Vec<String> = store
            .list_autopilot_steps(id)
            .await
            .unwrap()
            .into_iter()
            .filter(|s| s.name != "CRAWL")
            .map(|s| s.state)
            .collect();
        assert_eq!(others, ["PENDING", "PENDING"]);
    }

    #[tokio::test]
    async fn the_steps_a_run_never_finished_are_skipped_with_the_reason_and_the_finished_ones_stay()
    {
        let (store, _dir) = temp_store().await;
        let id = make_run(&store, "p1").await;
        store
            .begin_autopilot_step(id, "REQUIREMENTS")
            .await
            .unwrap();
        store
            .settle_autopilot_step(id, "REQUIREMENTS", StepState::Done, None, None)
            .await
            .unwrap();
        store.begin_autopilot_step(id, "CRAWL").await.unwrap(); // still running when the run ends
        store
            .skip_unfinished_autopilot_steps(id, "the run was cancelled")
            .await
            .unwrap();
        let steps = store.list_autopilot_steps(id).await.unwrap();
        let states: Vec<&str> = steps.iter().map(|s| s.state.as_str()).collect();
        assert_eq!(states, ["DONE", "SKIPPED", "SKIPPED"]);
        assert!(steps[0].error.is_none(), "a finished step keeps its record");
        assert_eq!(steps[1].error.as_deref(), Some("the run was cancelled"));
        assert!(steps[1].finished_at.is_some() && steps[2].finished_at.is_some());
        // Another run's steps are not touched.
        let other = make_run(&store, "p2").await;
        assert!(store
            .list_autopilot_steps(other)
            .await
            .unwrap()
            .iter()
            .all(|s| s.state == "PENDING"));
    }

    #[tokio::test]
    async fn a_step_waiting_at_a_gate_is_skipped_when_its_run_ends_and_a_step_that_failed_keeps_its_own_reason(
    ) {
        let (store, _dir) = temp_store().await;
        let id = make_run(&store, "p1").await;
        store
            .begin_autopilot_step(id, "REQUIREMENTS")
            .await
            .unwrap();
        store
            .settle_autopilot_step(
                id,
                "REQUIREMENTS",
                StepState::Failed,
                None,
                Some("no requirements"),
            )
            .await
            .unwrap();
        store.begin_autopilot_step(id, "CRAWL").await.unwrap();
        store
            .settle_autopilot_step(id, "CRAWL", StepState::Waiting, None, None)
            .await
            .unwrap();
        // GAP_CHECK was never started.
        store
            .skip_unfinished_autopilot_steps(id, "the run was cancelled")
            .await
            .unwrap();
        let steps = store.list_autopilot_steps(id).await.unwrap();
        let states: Vec<&str> = steps.iter().map(|s| s.state.as_str()).collect();
        assert_eq!(states, ["FAILED", "SKIPPED", "SKIPPED"]);
        assert_eq!(
            steps[0].error.as_deref(),
            Some("no requirements"),
            "a step that failed keeps its reason"
        );
        assert_eq!(steps[1].error.as_deref(), Some("the run was cancelled"));
        assert!(
            steps[1].finished_at.is_some(),
            "the run's end finishes a step that waited"
        );
    }

    #[tokio::test]
    async fn the_timeline_is_read_oldest_first_from_the_latest_or_from_where_the_reader_was() {
        let (store, _dir) = temp_store().await;
        let id = make_run(&store, "p1").await;
        let other = make_run(&store, "p2").await;
        for n in 1..=5 {
            store
                .add_autopilot_event(
                    id,
                    "note",
                    Some("CRAWL"),
                    &format!("event {n}"),
                    Some(&serde_json::json!({"n": n})),
                )
                .await
                .unwrap();
        }
        store
            .add_autopilot_event(other, "note", None, "not this run's", None)
            .await
            .unwrap();

        let all = store.list_autopilot_events(id, None, 100).await.unwrap();
        let messages: Vec<&str> = all.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(messages.len(), 6, "the creation event and five more");
        assert_eq!(
            &messages[1..],
            ["event 1", "event 2", "event 3", "event 4", "event 5"]
        );
        assert_eq!(all[1].step.as_deref(), Some("CRAWL"));
        assert_eq!(all[1].data, Some(serde_json::json!({"n": 1})));
        assert!(all[0].data.is_none() && all[0].step.is_none());

        // The latest three, still oldest first.
        let latest = store.list_autopilot_events(id, None, 3).await.unwrap();
        let messages: Vec<&str> = latest.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(messages, ["event 3", "event 4", "event 5"]);

        // Everything after an event the reader has, oldest first, limited.
        let after = store
            .list_autopilot_events(id, Some(all[2].id), 100)
            .await
            .unwrap();
        let messages: Vec<&str> = after.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(messages, ["event 3", "event 4", "event 5"]);
        let limited = store
            .list_autopilot_events(id, Some(all[2].id), 2)
            .await
            .unwrap();
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].message, "event 3");
        assert!(store
            .list_autopilot_events(id, Some(all[5].id), 100)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn deleting_a_project_deletes_its_runs_steps_and_events_and_no_other_projects() {
        let (store, _dir) = temp_store().await;
        let project = Uuid::new_v4();
        store
            .insert_project(project, "Autopilot cleanup", None)
            .await
            .unwrap();
        let mine = make_run(&store, &project.to_string()).await;
        let theirs = make_run(&store, "another-project").await;
        store
            .add_autopilot_event(mine, "note", None, "hello", None)
            .await
            .unwrap();

        assert!(store.delete_project(project).await.unwrap());
        assert!(store.get_autopilot_run(mine).await.unwrap().is_none());
        assert!(store.list_autopilot_steps(mine).await.unwrap().is_empty());
        assert!(store
            .list_autopilot_events(mine, None, 100)
            .await
            .unwrap()
            .is_empty());
        assert!(store.get_autopilot_run(theirs).await.unwrap().is_some());
        assert_eq!(store.list_autopilot_steps(theirs).await.unwrap().len(), 3);
        assert_eq!(
            store
                .list_autopilot_events(theirs, None, 100)
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
