//! Phase 8C — scheduled regression: the cases a project has approved and run once are run again on a schedule, and each run is compared
//! with the last (`0025_regression.sql`).
//!
//! Nothing is compiled again and no AI is asked. A case that has been handed off carries the run it was last executed in
//! (`executed_run_id`), and that run's plan holds the task the case ran as, with its compiled steps. A regression run is a new execution
//! made of those same tasks, so it has the evidence, the audit trail and the place in the Execution Hub of any other run, and what a case
//! does now can be set beside what it did before: passed and now fails (`REGRESSED`), failed and now passes (`FIXED`), and so on.
//!
//! This file is the part that decides: when a schedule is next due, which cases can be repeated and which cannot (and why), how a result
//! compares with the one before it, and what the report says. The loop that starts runs is `regression_runner.rs`, the routes are
//! `regression_api.rs`.

use crate::domain::{RunPlan, TaskDef, TestCaseStatus};
use crate::openapi_runs::{outcome_of, shorten, TaskOutcome};
use crate::store::{parse_ts, Store};
use anyhow::Result;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::BTreeMap;
use uuid::Uuid;

/// The shortest and longest time between two runs of a schedule.
pub const MIN_EVERY_MINUTES: i64 = 5;
pub const MAX_EVERY_MINUTES: i64 = 60 * 24 * 31;
pub const MAX_SCHEDULES_PER_PROJECT: i64 = 20;
pub const DEFAULT_CONCURRENCY: i64 = 2;
pub const MAX_CONCURRENCY: i64 = 8;
pub const MAX_NAME_CHARS: usize = 100;
/// Runs in a schedule's history.
pub const LIST_LIMIT: i64 = 50;

// ---------------------------------------------------------------------------------------------------------------------- what is stored

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleRow {
    pub id: Uuid,
    pub project_id: String,
    pub name: String,
    /// A suite, or every case of the project when none.
    pub suite_id: Option<String>,
    pub every_minutes: i64,
    /// The first time it is due, and the grid the later ones fall on.
    pub anchor_at: DateTime<Utc>,
    pub next_run_at: DateTime<Utc>,
    pub concurrency: i64,
    pub enabled: bool,
    pub last_started_at: Option<DateTime<Utc>>,
    /// Why the last time it was due did not make a run, or what to know about the one it made.
    pub last_note: Option<String>,
    /// The key it runs as: looked up again each time, so revoking it stops the schedule.
    pub created_by_key: Uuid,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

/// A case a regression run repeated, and the run its task was copied from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayedCase {
    pub case_id: String,
    /// The id of the task in the run (it is the case's own id when the case was handed off in the usual way).
    pub task_id: String,
    pub title: String,
    pub source_run_id: String,
}

/// A case that was in scope and could not be repeated, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkippedCase {
    pub case_id: String,
    pub title: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Schedule,
    Manual,
}

impl Trigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Trigger::Schedule => "SCHEDULE",
            Trigger::Manual => "MANUAL",
        }
    }

    fn parse(text: &str) -> Self {
        if text == "SCHEDULE" {
            Trigger::Schedule
        } else {
            Trigger::Manual
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegressionRunRow {
    pub id: Uuid,
    pub schedule_id: Uuid,
    pub project_id: String,
    /// The execution.
    pub run_id: Uuid,
    pub trigger: Trigger,
    pub cases: Vec<ReplayedCase>,
    pub skipped: Vec<SkippedCase>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

/// Stored times are written one way, so that comparing them as text in a query is comparing them as times.
fn stamp(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, true)
}

// The column lists are macros, so that every query below is one string the compiler has seen whole.
macro_rules! schedule_columns {
    () => {
        "id, project_id, name, suite_id, every_minutes, anchor_at, next_run_at, concurrency, enabled, last_started_at, last_note, created_by_key, created_by, created_at"
    };
}
macro_rules! run_columns {
    () => {
        "id, schedule_id, project_id, run_id, trigger_kind, cases, skipped, created_by, created_at"
    };
}

fn schedule_from(r: sqlx::sqlite::SqliteRow) -> Result<ScheduleRow> {
    Ok(ScheduleRow {
        id: Uuid::parse_str(&r.get::<String, _>("id"))?,
        project_id: r.get("project_id"),
        name: r.get("name"),
        suite_id: r.get("suite_id"),
        every_minutes: r.get("every_minutes"),
        anchor_at: parse_ts(&r.get::<String, _>("anchor_at")),
        next_run_at: parse_ts(&r.get::<String, _>("next_run_at")),
        concurrency: r.get("concurrency"),
        enabled: r.get::<i64, _>("enabled") != 0,
        last_started_at: r
            .get::<Option<String>, _>("last_started_at")
            .map(|t| parse_ts(&t)),
        last_note: r.get("last_note"),
        created_by_key: Uuid::parse_str(&r.get::<String, _>("created_by_key"))?,
        created_by: r.get("created_by"),
        created_at: parse_ts(&r.get::<String, _>("created_at")),
    })
}

fn run_from(r: sqlx::sqlite::SqliteRow) -> Result<RegressionRunRow> {
    Ok(RegressionRunRow {
        id: Uuid::parse_str(&r.get::<String, _>("id"))?,
        schedule_id: Uuid::parse_str(&r.get::<String, _>("schedule_id"))?,
        project_id: r.get("project_id"),
        run_id: Uuid::parse_str(&r.get::<String, _>("run_id"))?,
        trigger: Trigger::parse(&r.get::<String, _>("trigger_kind")),
        cases: serde_json::from_str(&r.get::<String, _>("cases"))?,
        skipped: serde_json::from_str(&r.get::<String, _>("skipped"))?,
        created_by: r.get("created_by"),
        created_at: parse_ts(&r.get::<String, _>("created_at")),
    })
}

impl Store {
    pub async fn insert_regression_schedule(&self, s: &ScheduleRow) -> Result<()> {
        sqlx::query(concat!(
            "INSERT INTO regression_schedules (",
            schedule_columns!(),
            ") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(s.id.to_string())
        .bind(&s.project_id)
        .bind(&s.name)
        .bind(&s.suite_id)
        .bind(s.every_minutes)
        .bind(stamp(s.anchor_at))
        .bind(stamp(s.next_run_at))
        .bind(s.concurrency)
        .bind(i64::from(s.enabled))
        .bind(s.last_started_at.map(stamp))
        .bind(&s.last_note)
        .bind(s.created_by_key.to_string())
        .bind(&s.created_by)
        .bind(stamp(s.created_at))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_regression_schedule(&self, id: Uuid) -> Result<Option<ScheduleRow>> {
        sqlx::query(concat!(
            "SELECT ",
            schedule_columns!(),
            " FROM regression_schedules WHERE id = ?"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .map(schedule_from)
        .transpose()
    }

    /// The project's schedules, oldest first.
    pub async fn list_regression_schedules(&self, project_id: &str) -> Result<Vec<ScheduleRow>> {
        sqlx::query(concat!(
            "SELECT ",
            schedule_columns!(),
            " FROM regression_schedules WHERE project_id = ? ORDER BY created_at, id"
        ))
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(schedule_from)
        .collect()
    }

    pub async fn count_regression_schedules(&self, project_id: &str) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM regression_schedules WHERE project_id = ?")
                .bind(project_id)
                .fetch_one(&self.pool)
                .await?,
        )
    }

    /// Writes what a person can change (and what the runner keeps): everything but the id, the project, who made it and when.
    pub async fn update_regression_schedule(&self, s: &ScheduleRow) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE regression_schedules SET name = ?, suite_id = ?, every_minutes = ?, anchor_at = ?, next_run_at = ?, concurrency = ?, enabled = ?, last_started_at = ?, last_note = ? WHERE id = ?",
        )
        .bind(&s.name)
        .bind(&s.suite_id)
        .bind(s.every_minutes)
        .bind(stamp(s.anchor_at))
        .bind(stamp(s.next_run_at))
        .bind(s.concurrency)
        .bind(i64::from(s.enabled))
        .bind(s.last_started_at.map(stamp))
        .bind(&s.last_note)
        .bind(s.id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// A schedule and its history (the executions themselves stay: they are runs like any other).
    pub async fn delete_regression_schedule(&self, id: Uuid) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM regression_runs WHERE schedule_id = ?")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
        let result = sqlx::query("DELETE FROM regression_schedules WHERE id = ?")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    /// The enabled schedules whose time has come, the longest waiting first.
    pub async fn due_regression_schedules(&self, now: DateTime<Utc>) -> Result<Vec<ScheduleRow>> {
        sqlx::query(concat!(
            "SELECT ",
            schedule_columns!(),
            " FROM regression_schedules WHERE enabled = 1 AND next_run_at <= ? ORDER BY next_run_at, id"
        ))
        .bind(stamp(now))
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(schedule_from)
        .collect()
    }

    pub async fn insert_regression_run(&self, r: &RegressionRunRow) -> Result<()> {
        sqlx::query(concat!(
            "INSERT INTO regression_runs (",
            run_columns!(),
            ") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(r.id.to_string())
        .bind(r.schedule_id.to_string())
        .bind(&r.project_id)
        .bind(r.run_id.to_string())
        .bind(r.trigger.as_str())
        .bind(serde_json::to_string(&r.cases)?)
        .bind(serde_json::to_string(&r.skipped)?)
        .bind(&r.created_by)
        .bind(stamp(r.created_at))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_regression_run(&self, id: Uuid) -> Result<Option<RegressionRunRow>> {
        sqlx::query(concat!(
            "SELECT ",
            run_columns!(),
            " FROM regression_runs WHERE id = ?"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .map(run_from)
        .transpose()
    }

    /// A schedule's runs, newest first.
    pub async fn list_regression_runs(
        &self,
        schedule_id: Uuid,
        limit: i64,
    ) -> Result<Vec<RegressionRunRow>> {
        sqlx::query(concat!(
            "SELECT ",
            run_columns!(),
            " FROM regression_runs WHERE schedule_id = ? ORDER BY created_at DESC, id DESC LIMIT ?"
        ))
        .bind(schedule_id.to_string())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(run_from)
        .collect()
    }

    /// The run of the same schedule just before this one.
    pub async fn previous_regression_run(
        &self,
        run: &RegressionRunRow,
    ) -> Result<Option<RegressionRunRow>> {
        sqlx::query(concat!(
            "SELECT ",
            run_columns!(),
            " FROM regression_runs WHERE schedule_id = ? AND (created_at < ? OR (created_at = ? AND id < ?)) ORDER BY created_at DESC, id DESC LIMIT 1"
        ))
        .bind(run.schedule_id.to_string())
        .bind(stamp(run.created_at))
        .bind(stamp(run.created_at))
        .bind(run.id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .map(run_from)
        .transpose()
    }
}

// ---------------------------------------------------------------------------------------------------------------------- the schedule

/// A name for a schedule: what is left of it once trimmed, which must be something and not too long.
pub fn check_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("the name must not be empty".to_string());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!(
            "the name must be at most {MAX_NAME_CHARS} characters"
        ));
    }
    Ok(name.to_string())
}

pub fn check_every(minutes: i64) -> Result<(), String> {
    if !(MIN_EVERY_MINUTES..=MAX_EVERY_MINUTES).contains(&minutes) {
        return Err(format!(
            "every_minutes must be from {MIN_EVERY_MINUTES} to {MAX_EVERY_MINUTES}"
        ));
    }
    Ok(())
}

pub fn check_concurrency(n: i64) -> Result<(), String> {
    if !(1..=MAX_CONCURRENCY).contains(&n) {
        return Err(format!("concurrency must be from 1 to {MAX_CONCURRENCY}"));
    }
    Ok(())
}

/// The first time on the schedule's grid (`anchor` and a whole number of `every_minutes` after it) that is later than `now`; the anchor
/// itself while that has not come. A run that took a long time, or a service that was down for days, gives one next time and not a
/// queue of missed ones, and the hour of the day stays where it was put.
pub fn next_slot_after(
    anchor: DateTime<Utc>,
    every_minutes: i64,
    now: DateTime<Utc>,
) -> DateTime<Utc> {
    if now < anchor {
        return anchor;
    }
    let step = every_minutes.max(1) * 60;
    let gone = (now - anchor).num_seconds() / step;
    anchor + Duration::seconds((gone + 1) * step)
}

// ---------------------------------------------------------------------------------------------------------------------- what can be repeated

/// A case in the scope of a schedule, as far as repeating it goes.
#[derive(Debug, Clone)]
pub struct CaseInfo {
    pub id: String,
    pub title: String,
    pub status: TestCaseStatus,
    pub executed_run_id: Option<String>,
    pub executed_task_id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Replay {
    /// The task the case was last run as, to run again, and the run it came from.
    Repeat {
        task: TaskDef,
        source_run_id: String,
    },
    Skip(String),
}

/// Whether a case can be repeated, given the plans of the runs cases were last executed in.
pub fn replay_of(case: &CaseInfo, plans: &BTreeMap<String, RunPlan>) -> Replay {
    match case.status {
        TestCaseStatus::Executed => {}
        TestCaseStatus::Approved => {
            return Replay::Skip(
                "approved, but not run yet: hand it off once, so that there is something to repeat"
                    .to_string(),
            )
        }
        other => {
            return Replay::Skip(format!(
                "its status is {}, not one a run can be repeated of",
                other.as_db_str()
            ))
        }
    }
    let Some(source) = &case.executed_run_id else {
        return Replay::Skip("it has no earlier run to repeat".to_string());
    };
    let Some(plan) = plans.get(source) else {
        return Replay::Skip("the run it was last executed in is no longer there".to_string());
    };
    let task_id = case.executed_task_id.as_deref().unwrap_or(&case.id);
    let Some(task) = plan.tasks.iter().find(|t| t.id == task_id) else {
        return Replay::Skip("its task is not in the run it was last executed in".to_string());
    };
    if task.simulate.is_some() {
        return Replay::Skip(
            "its last run was simulated, not executed, so there is nothing real to repeat"
                .to_string(),
        );
    }
    if task.executor.is_none() {
        return Replay::Skip("its task names no executor".to_string());
    }
    Replay::Repeat {
        task: TaskDef {
            // A regression run is one flat set of tasks: whatever the task waited for in its own run is not in this one.
            prerequisites: Vec::new(),
            ..task.clone()
        },
        source_run_id: source.clone(),
    }
}

// ---------------------------------------------------------------------------------------------------------------------- comparing

/// Where the result a case is compared with came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeforeFrom {
    /// The run of this schedule before this one.
    PreviousRun,
    /// The run the case's task was copied from (this schedule has no earlier run of it).
    SourceRun,
}

impl BeforeFrom {
    fn as_str(self) -> &'static str {
        match self {
            BeforeFrom::PreviousRun => "PREVIOUS_RUN",
            BeforeFrom::SourceRun => "SOURCE_RUN",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Before {
    pub outcome: String,
    pub from: BeforeFrom,
}

fn answered(outcome: &str) -> bool {
    matches!(outcome, "PASSED" | "FAILED" | "ERROR")
}

/// What became of a case between the last time and now. `before` is what it came to then (only an outcome that answered counts).
pub fn change_of(before: Option<&str>, now: &str) -> &'static str {
    if !answered(now) {
        return "UNCOMPARED";
    }
    match before.filter(|b| answered(b)) {
        None => "NEW",
        Some(b) => match (b == "PASSED", now == "PASSED") {
            (true, true) => "STILL_PASSING",
            (true, false) => "REGRESSED",
            (false, true) => "FIXED",
            (false, false) => "STILL_FAILING",
        },
    }
}

/// Regressions first, then what still fails, what is new, what could not be compared, what was fixed, and what passes.
fn rank(change: &str, outcome: &str) -> u8 {
    match (change, outcome) {
        ("REGRESSED", _) => 0,
        ("STILL_FAILING", _) => 1,
        ("NEW", "FAILED" | "ERROR") => 2,
        ("UNCOMPARED", _) => 3,
        ("FIXED", _) => 4,
        ("NEW", _) => 5,
        _ => 6,
    }
}

#[derive(Default)]
struct Tally {
    passed: u64,
    failed: u64,
    error: u64,
    pending: u64,
    not_run: u64,
    regressed: u64,
    fixed: u64,
    still_failing: u64,
    still_passing: u64,
    new: u64,
    uncompared: u64,
}

impl Tally {
    fn add(&mut self, outcome: &str, change: &str) {
        match outcome {
            "PASSED" => self.passed += 1,
            "FAILED" => self.failed += 1,
            "ERROR" => self.error += 1,
            "PENDING" => self.pending += 1,
            _ => self.not_run += 1,
        }
        match change {
            "REGRESSED" => self.regressed += 1,
            "FIXED" => self.fixed += 1,
            "STILL_FAILING" => self.still_failing += 1,
            "STILL_PASSING" => self.still_passing += 1,
            "NEW" => self.new += 1,
            _ => self.uncompared += 1,
        }
    }

    fn ran(&self) -> u64 {
        self.passed + self.failed + self.error
    }
}

/// The report of a regression run: `tasks` are its execution's tasks by task id, `before` what each case came to the last time by case
/// id, `final_run` whether the execution has ended.
pub fn build_report(
    row: &RegressionRunRow,
    schedule_name: &str,
    run_state: &str,
    final_run: bool,
    tasks: &BTreeMap<String, TaskOutcome>,
    before: &BTreeMap<String, Before>,
) -> Value {
    let mut tally = Tally::default();
    let mut entries: Vec<(u8, String, Value)> = Vec::new();
    let mut from_source = 0usize;
    for replayed in &row.cases {
        let task = tasks.get(&replayed.task_id);
        let outcome = match task {
            Some(t) => outcome_of(t.status),
            None if final_run => "NOT_RUN",
            None => "PENDING",
        };
        let reason = match (outcome, task) {
            ("FAILED" | "ERROR", Some(t)) => {
                t.reason.as_deref().map(shorten).filter(|r| !r.is_empty())
            }
            ("NOT_RUN", Some(t)) => Some(format!(
                "the task ended {}",
                t.status.as_str().to_lowercase().replace('_', " ")
            )),
            _ => None,
        };
        let prior = before.get(&replayed.case_id);
        let change = change_of(prior.map(|b| b.outcome.as_str()), outcome);
        if change != "NEW"
            && change != "UNCOMPARED"
            && prior.map(|b| b.from) == Some(BeforeFrom::SourceRun)
        {
            from_source += 1;
        }
        tally.add(outcome, change);
        entries.push((
            rank(change, outcome),
            replayed.title.clone(),
            json!({
                "case_id": replayed.case_id,
                "task_id": replayed.task_id,
                "title": replayed.title,
                "outcome": outcome,
                "before": prior.filter(|b| answered(&b.outcome)).map(|b| b.outcome.as_str()),
                "before_from": prior.filter(|b| answered(&b.outcome)).map(|b| b.from.as_str()),
                "change": change,
                "reason": reason,
                "source_run_id": replayed.source_run_id,
            }),
        ));
    }
    entries.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    let verdict = if !final_run {
        "RUNNING"
    } else if tally.regressed > 0 {
        "REGRESSED"
    } else if tally.failed + tally.error > 0 {
        "FAILING"
    } else if tally.ran() > 0 {
        "PASSING"
    } else {
        "NOTHING_RAN"
    };
    let mut notes = Vec::new();
    if !final_run {
        notes.push("The run has not ended: this is what it has found so far.".to_string());
    }
    if from_source > 0 {
        notes.push(format!(
            "{from_source} case(s) are compared with the run they were copied from: this schedule has no earlier run of them."
        ));
    }
    if !row.skipped.is_empty() {
        notes.push(format!(
            "{} case(s) in scope were not repeated: see what could not be repeated.",
            row.skipped.len()
        ));
    }
    json!({
        "id": row.id,
        "schedule_id": row.schedule_id,
        "schedule_name": schedule_name,
        "project_id": row.project_id,
        "run_id": row.run_id,
        "trigger": row.trigger.as_str(),
        "created_at": row.created_at,
        "created_by": row.created_by,
        "run_state": run_state,
        "final": final_run,
        "verdict": verdict,
        "summary": {
            "cases": row.cases.len(),
            "ran": tally.ran(),
            "passed": tally.passed,
            "failed": tally.failed,
            "error": tally.error,
            "pending": tally.pending,
            "not_run": tally.not_run,
            "regressed": tally.regressed,
            "fixed": tally.fixed,
            "still_failing": tally.still_failing,
            "still_passing": tally.still_passing,
            "new": tally.new,
            "uncompared": tally.uncompared,
        },
        "cases": entries.into_iter().map(|(_, _, v)| v).collect::<Vec<_>>(),
        "skipped": row.skipped,
        "notes": notes,
    })
}

/// The report as Markdown, to paste into a ticket or a message.
pub fn to_markdown(report: &Value) -> String {
    let text = |v: &Value| v.as_str().unwrap_or("").to_string();
    let num = |v: &Value| v.as_u64().unwrap_or(0);
    let s = &report["summary"];
    let mut out = format!(
        "# Regression run: {} ({})\n\n{}. {} cases: {} passed, {} failed, {} error; {} regressed, {} fixed, {} still failing.\n",
        text(&report["schedule_name"]),
        text(&report["created_at"]),
        match report["verdict"].as_str() {
            Some("REGRESSED") => "Regressions found",
            Some("FAILING") => "Failing, nothing new",
            Some("PASSING") => "All passing",
            Some("RUNNING") => "Still running",
            _ => "Nothing ran",
        },
        num(&s["cases"]),
        num(&s["passed"]),
        num(&s["failed"]),
        num(&s["error"]),
        num(&s["regressed"]),
        num(&s["fixed"]),
        num(&s["still_failing"]),
    );
    let cases = report["cases"].as_array().cloned().unwrap_or_default();
    let listed: Vec<&Value> = cases
        .iter()
        .filter(|c| {
            matches!(
                c["change"].as_str(),
                Some("REGRESSED" | "STILL_FAILING" | "FIXED")
            ) || matches!(c["outcome"].as_str(), Some("FAILED" | "ERROR"))
        })
        .collect();
    if !listed.is_empty() {
        out.push_str("\n## What changed or fails\n\n");
        for case in listed {
            out.push_str(&format!(
                "- **{}** ({}, {}){}\n",
                text(&case["title"]),
                text(&case["outcome"]),
                text(&case["change"]).to_lowercase().replace('_', " "),
                case["reason"]
                    .as_str()
                    .map(|r| format!(": {r}"))
                    .unwrap_or_default(),
            ));
        }
    }
    let skipped = report["skipped"].as_array().cloned().unwrap_or_default();
    if !skipped.is_empty() {
        out.push_str("\n## Not repeated\n\n");
        for case in skipped {
            out.push_str(&format!(
                "- {}: {}\n",
                text(&case["title"]),
                text(&case["reason"])
            ));
        }
    }
    let notes = report["notes"].as_array().cloned().unwrap_or_default();
    if !notes.is_empty() {
        out.push_str("\n## Notes\n\n");
        for note in notes {
            out.push_str(&format!("- {}\n", text(&note)));
        }
    }
    out
}

/// A schedule as the routes show it; `last_run` is a short account of its latest run, when it has one.
pub fn schedule_view(s: &ScheduleRow, last_run: Option<Value>) -> Value {
    json!({
        "id": s.id,
        "project_id": s.project_id,
        "name": s.name,
        "suite_id": s.suite_id,
        "every_minutes": s.every_minutes,
        "anchor_at": s.anchor_at,
        "next_run_at": s.next_run_at,
        "concurrency": s.concurrency,
        "enabled": s.enabled,
        "last_started_at": s.last_started_at,
        "last_note": s.last_note,
        "created_by": s.created_by,
        "created_at": s.created_at,
        "last_run": last_run,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{SimSpec, TaskStatus};
    use chrono::TimeZone;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, s).unwrap()
    }

    // ------------------------------------------------------------------------------------------------------ the schedule

    #[test]
    fn a_name_is_trimmed_and_must_be_something_and_not_too_long() {
        assert_eq!(check_name("  Nightly  ").unwrap(), "Nightly");
        assert!(check_name("   ").unwrap_err().contains("must not be empty"));
        assert!(check_name(&"x".repeat(MAX_NAME_CHARS)).is_ok());
        assert!(check_name(&"x".repeat(MAX_NAME_CHARS + 1))
            .unwrap_err()
            .contains("at most 100"));
        assert!(
            check_name(&"é".repeat(MAX_NAME_CHARS)).is_ok(),
            "characters are counted, not bytes"
        );
    }

    #[test]
    fn the_time_between_runs_and_the_concurrency_have_their_limits() {
        assert!(check_every(MIN_EVERY_MINUTES).is_ok());
        assert!(check_every(MAX_EVERY_MINUTES).is_ok());
        assert!(check_every(MIN_EVERY_MINUTES - 1)
            .unwrap_err()
            .contains("from 5 to 44640"));
        assert!(check_every(MAX_EVERY_MINUTES + 1).is_err());
        assert!(check_every(0).is_err() && check_every(-60).is_err());
        assert!(check_concurrency(1).is_ok() && check_concurrency(MAX_CONCURRENCY).is_ok());
        assert!(check_concurrency(0).unwrap_err().contains("from 1 to 8"));
        assert!(check_concurrency(MAX_CONCURRENCY + 1).is_err());
    }

    #[test]
    fn the_next_slot_is_the_first_on_the_grid_after_now_and_the_anchor_while_that_has_not_come() {
        let anchor = at(2026, 9, 1, 2, 0, 0);
        let day = 24 * 60;
        // Before the anchor: the anchor.
        assert_eq!(
            next_slot_after(anchor, day, at(2026, 8, 31, 12, 0, 0)),
            anchor
        );
        // At the anchor the run of that slot is being made: the next is one interval on.
        assert_eq!(
            next_slot_after(anchor, day, anchor),
            at(2026, 9, 2, 2, 0, 0)
        );
        assert_eq!(
            next_slot_after(anchor, day, at(2026, 9, 1, 2, 0, 1)),
            at(2026, 9, 2, 2, 0, 0)
        );
        assert_eq!(
            next_slot_after(anchor, day, at(2026, 9, 1, 23, 59, 59)),
            at(2026, 9, 2, 2, 0, 0)
        );
        // A moment before a slot: that slot; on the slot: the one after.
        assert_eq!(
            next_slot_after(anchor, day, at(2026, 9, 2, 1, 59, 59)),
            at(2026, 9, 2, 2, 0, 0)
        );
        assert_eq!(
            next_slot_after(anchor, day, at(2026, 9, 2, 2, 0, 0)),
            at(2026, 9, 3, 2, 0, 0)
        );
        // Ten days of downtime give one next run, on the same hour, not a queue.
        assert_eq!(
            next_slot_after(anchor, day, at(2026, 9, 11, 9, 30, 0)),
            at(2026, 9, 12, 2, 0, 0)
        );
        // Short intervals keep their grid.
        assert_eq!(
            next_slot_after(anchor, 15, at(2026, 9, 1, 2, 44, 59)),
            at(2026, 9, 1, 2, 45, 0)
        );
        assert_eq!(
            next_slot_after(anchor, 15, at(2026, 9, 1, 2, 45, 0)),
            at(2026, 9, 1, 3, 0, 0)
        );
        // An interval that is not one still moves forward.
        assert!(next_slot_after(anchor, 0, at(2026, 9, 1, 2, 0, 30)) > at(2026, 9, 1, 2, 0, 30));
    }

    // ------------------------------------------------------------------------------------------------------ what can be repeated

    fn task(id: &str) -> TaskDef {
        TaskDef {
            id: id.to_string(),
            name: format!("task {id}"),
            prerequisites: vec!["something-before".to_string()],
            simulate: None,
            executor: Some("compiled-api".to_string()),
            params: json!({"plan": id}),
            test_case_id: None,
        }
    }

    fn plan_of(tasks: Vec<TaskDef>) -> RunPlan {
        RunPlan {
            name: "the run".to_string(),
            tasks,
            concurrency: 4,
        }
    }

    fn case(status: TestCaseStatus, run: Option<&str>, task_id: Option<&str>) -> CaseInfo {
        CaseInfo {
            id: "case-1".to_string(),
            title: "A case".to_string(),
            status,
            executed_run_id: run.map(str::to_string),
            executed_task_id: task_id.map(str::to_string),
        }
    }

    fn skipped(replay: Replay) -> String {
        match replay {
            Replay::Skip(why) => why,
            Replay::Repeat { .. } => panic!("expected the case to be skipped"),
        }
    }

    #[test]
    fn an_executed_case_is_repeated_as_the_task_it_was_last_run_as_with_nothing_it_waited_for() {
        let plans = BTreeMap::from([(
            "run-1".to_string(),
            plan_of(vec![task("other"), task("case-1")]),
        )]);
        match replay_of(
            &case(TestCaseStatus::Executed, Some("run-1"), Some("case-1")),
            &plans,
        ) {
            Replay::Repeat {
                task,
                source_run_id,
            } => {
                assert_eq!(task.id, "case-1");
                assert_eq!(
                    task.params,
                    json!({"plan": "case-1"}),
                    "the steps it was compiled to, as they were"
                );
                assert!(
                    task.prerequisites.is_empty(),
                    "a flat set of tasks: what it waited for is not in this run"
                );
                assert_eq!(source_run_id, "run-1");
            }
            Replay::Skip(why) => panic!("skipped: {why}"),
        }
        // Without a recorded task id the case's own id is the task.
        assert!(matches!(
            replay_of(&case(TestCaseStatus::Executed, Some("run-1"), None), &plans),
            Replay::Repeat { .. }
        ));
    }

    #[test]
    fn a_case_that_cannot_be_repeated_says_why() {
        let plans = BTreeMap::from([("run-1".to_string(), plan_of(vec![task("case-1")]))]);
        assert!(skipped(replay_of(
            &case(TestCaseStatus::Approved, None, None),
            &plans
        ))
        .starts_with("approved, but not run yet"));
        for status in [
            TestCaseStatus::Generated,
            TestCaseStatus::Validated,
            TestCaseStatus::Rejected,
        ] {
            let why = skipped(replay_of(&case(status, Some("run-1"), None), &plans));
            assert!(
                why.starts_with(&format!("its status is {}", status.as_db_str())),
                "{why}"
            );
        }
        assert_eq!(
            skipped(replay_of(
                &case(TestCaseStatus::Executed, None, None),
                &plans
            )),
            "it has no earlier run to repeat"
        );
        assert_eq!(
            skipped(replay_of(
                &case(TestCaseStatus::Executed, Some("gone"), None),
                &plans
            )),
            "the run it was last executed in is no longer there"
        );
        assert_eq!(
            skipped(replay_of(
                &case(TestCaseStatus::Executed, Some("run-1"), Some("not-there")),
                &plans
            )),
            "its task is not in the run it was last executed in"
        );
        let mut simulated = task("case-1");
        simulated.simulate = Some(SimSpec {
            outcome: crate::domain::SimOutcome::Pass,
            delay_ms: 0,
        });
        let plans = BTreeMap::from([("run-1".to_string(), plan_of(vec![simulated]))]);
        assert!(skipped(replay_of(
            &case(TestCaseStatus::Executed, Some("run-1"), None),
            &plans
        ))
        .contains("simulated, not executed"));
        let mut bare = task("case-1");
        bare.executor = None;
        let plans = BTreeMap::from([("run-1".to_string(), plan_of(vec![bare]))]);
        assert_eq!(
            skipped(replay_of(
                &case(TestCaseStatus::Executed, Some("run-1"), None),
                &plans
            )),
            "its task names no executor"
        );
    }

    // ------------------------------------------------------------------------------------------------------ comparing

    #[test]
    fn what_became_of_a_case_since_the_last_time() {
        let table = [
            (Some("PASSED"), "PASSED", "STILL_PASSING"),
            (Some("PASSED"), "FAILED", "REGRESSED"),
            (Some("PASSED"), "ERROR", "REGRESSED"),
            (Some("FAILED"), "PASSED", "FIXED"),
            (Some("ERROR"), "PASSED", "FIXED"),
            (Some("FAILED"), "FAILED", "STILL_FAILING"),
            (Some("FAILED"), "ERROR", "STILL_FAILING"),
            (Some("ERROR"), "FAILED", "STILL_FAILING"),
            (None, "PASSED", "NEW"),
            (None, "FAILED", "NEW"),
            // Something that never answered is no result to compare with.
            (Some("PENDING"), "PASSED", "NEW"),
            (Some("NOT_RUN"), "FAILED", "NEW"),
            // Now has no answer: nothing to say about it.
            (Some("PASSED"), "PENDING", "UNCOMPARED"),
            (Some("FAILED"), "NOT_RUN", "UNCOMPARED"),
            (None, "PENDING", "UNCOMPARED"),
        ];
        for (before, now, want) in table {
            assert_eq!(change_of(before, now), want, "{before:?} -> {now}");
        }
    }

    #[test]
    fn regressions_come_first_and_what_passes_last() {
        let order = [
            rank("REGRESSED", "FAILED"),
            rank("STILL_FAILING", "FAILED"),
            rank("NEW", "FAILED"),
            rank("NEW", "ERROR"),
            rank("UNCOMPARED", "PENDING"),
            rank("FIXED", "PASSED"),
            rank("NEW", "PASSED"),
            rank("STILL_PASSING", "PASSED"),
        ];
        assert_eq!(order, [0, 1, 2, 2, 3, 4, 5, 6]);
    }

    // ------------------------------------------------------------------------------------------------------ the report

    fn replayed(n: u32) -> ReplayedCase {
        ReplayedCase {
            case_id: format!("case-{n}"),
            task_id: format!("task-{n}"),
            title: format!("Case {n}"),
            source_run_id: "source".to_string(),
        }
    }

    fn row(cases: Vec<ReplayedCase>, skipped: Vec<SkippedCase>) -> RegressionRunRow {
        RegressionRunRow {
            id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            schedule_id: Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            project_id: "proj".to_string(),
            run_id: Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
            trigger: Trigger::Schedule,
            cases,
            skipped,
            created_by: "tester".to_string(),
            created_at: at(2026, 9, 26, 2, 0, 0),
        }
    }

    fn row_with(n: u32) -> RegressionRunRow {
        row((1..=n).map(replayed).collect(), Vec::new())
    }

    fn outcome(status: TaskStatus, reason: Option<&str>) -> TaskOutcome {
        TaskOutcome {
            status,
            reason: reason.map(str::to_string),
            result: None,
        }
    }

    fn before(outcome: &str, from: BeforeFrom) -> Before {
        Before {
            outcome: outcome.to_string(),
            from,
        }
    }

    /// Five cases: one that regressed, one that still fails, one fixed, one still passing, one new.
    #[allow(clippy::type_complexity)]
    fn scenario() -> (
        RegressionRunRow,
        BTreeMap<String, TaskOutcome>,
        BTreeMap<String, Before>,
    ) {
        let row = row(
            (1..=5).map(replayed).collect(),
            vec![SkippedCase {
                case_id: "case-9".to_string(),
                title: "Case 9".to_string(),
                reason: "approved, but not run yet".to_string(),
            }],
        );
        let tasks = BTreeMap::from([
            ("task-1".to_string(), outcome(TaskStatus::Passed, None)),
            (
                "task-2".to_string(),
                outcome(TaskStatus::Failed, Some("expected 200, got 500")),
            ),
            (
                "task-3".to_string(),
                outcome(TaskStatus::Failed, Some("still broken")),
            ),
            ("task-4".to_string(), outcome(TaskStatus::Passed, None)),
            (
                "task-5".to_string(),
                outcome(TaskStatus::Timeout, Some("took too long")),
            ),
        ]);
        let before_map = BTreeMap::from([
            (
                "case-1".to_string(),
                before("PASSED", BeforeFrom::PreviousRun),
            ),
            (
                "case-2".to_string(),
                before("PASSED", BeforeFrom::PreviousRun),
            ),
            (
                "case-3".to_string(),
                before("FAILED", BeforeFrom::PreviousRun),
            ),
            (
                "case-4".to_string(),
                before("FAILED", BeforeFrom::SourceRun),
            ),
        ]);
        (row, tasks, before_map)
    }

    #[test]
    fn the_report_says_what_each_case_came_to_beside_what_it_came_to_before_regressions_first() {
        let (row, tasks, before_map) = scenario();
        let report = build_report(&row, "Nightly", "FAILED", true, &tasks, &before_map);
        assert_eq!(report["schedule_name"], "Nightly");
        assert_eq!(report["trigger"], "SCHEDULE");
        assert_eq!(report["verdict"], "REGRESSED");
        assert_eq!(report["final"], true);
        assert_eq!(
            report["summary"],
            json!({"cases": 5, "ran": 5, "passed": 2, "failed": 2, "error": 1, "pending": 0, "not_run": 0,
                   "regressed": 1, "fixed": 1, "still_failing": 1, "still_passing": 1, "new": 1, "uncompared": 0})
        );
        let all = report["cases"].as_array().unwrap();
        let order: Vec<&str> = all.iter().map(|c| c["case_id"].as_str().unwrap()).collect();
        assert_eq!(
            order,
            ["case-2", "case-3", "case-5", "case-4", "case-1"],
            "regressed, still failing, new and failing, fixed, still passing"
        );
        let first = &all[0];
        assert_eq!(first["change"], "REGRESSED");
        assert_eq!(first["before"], "PASSED");
        assert_eq!(first["before_from"], "PREVIOUS_RUN");
        assert_eq!(first["outcome"], "FAILED");
        assert_eq!(first["reason"], "expected 200, got 500");
        let fixed = all.iter().find(|c| c["case_id"] == "case-4").unwrap();
        assert_eq!(
            (fixed["change"].as_str(), fixed["before_from"].as_str()),
            (Some("FIXED"), Some("SOURCE_RUN"))
        );
        assert_eq!(fixed["reason"], Value::Null);
        let new = all.iter().find(|c| c["case_id"] == "case-5").unwrap();
        assert_eq!(
            (new["change"].as_str(), new["outcome"].as_str()),
            (Some("NEW"), Some("ERROR"))
        );
        assert_eq!(new["before"], Value::Null);
        assert_eq!(report["skipped"][0]["reason"], "approved, but not run yet");
        assert_eq!(
            report["notes"],
            json!([
                "1 case(s) are compared with the run they were copied from: this schedule has no earlier run of them.",
                "1 case(s) in scope were not repeated: see what could not be repeated."
            ])
        );
    }

    #[test]
    fn the_verdict_is_running_until_the_run_ends_then_the_worst_thing_found() {
        let (row, tasks, before_map) = scenario();
        let going = build_report(&row, "N", "RUNNING", false, &tasks, &before_map);
        assert_eq!(going["verdict"], "RUNNING");
        assert!(going["notes"][0]
            .as_str()
            .unwrap()
            .starts_with("The run has not ended"));

        let one = row_with(1);
        let ok = BTreeMap::from([("task-1".to_string(), outcome(TaskStatus::Passed, None))]);
        let bad = BTreeMap::from([("task-1".to_string(), outcome(TaskStatus::Failed, None))]);
        let none = BTreeMap::new();
        assert_eq!(
            build_report(&one, "N", "COMPLETED", true, &ok, &none)["verdict"],
            "PASSING"
        );
        assert_eq!(
            build_report(&one, "N", "FAILED", true, &bad, &none)["verdict"],
            "FAILING",
            "it fails and it did not pass before either"
        );
        let passed_before = BTreeMap::from([(
            "case-1".to_string(),
            before("PASSED", BeforeFrom::PreviousRun),
        )]);
        assert_eq!(
            build_report(&one, "N", "FAILED", true, &bad, &passed_before)["verdict"],
            "REGRESSED"
        );
        assert_eq!(
            build_report(&one, "N", "CANCELLED", true, &BTreeMap::new(), &none)["verdict"],
            "NOTHING_RAN"
        );
        // A case with no task yet is pending while the run goes and not run once it has ended.
        let pending = build_report(&one, "N", "RUNNING", false, &BTreeMap::new(), &none);
        assert_eq!(pending["cases"][0]["outcome"], "PENDING");
        assert_eq!(pending["summary"]["pending"], 1);
        let never = build_report(&one, "N", "CANCELLED", true, &BTreeMap::new(), &none);
        assert_eq!(never["cases"][0]["outcome"], "NOT_RUN");
        let cancelled =
            BTreeMap::from([("task-1".to_string(), outcome(TaskStatus::Cancelled, None))]);
        assert_eq!(
            build_report(&one, "N", "CANCELLED", true, &cancelled, &none)["cases"][0]["reason"],
            "the task ended cancelled"
        );
    }

    #[test]
    fn the_markdown_lists_what_changed_or_fails_what_was_not_repeated_and_the_notes() {
        let (row, tasks, before_map) = scenario();
        let markdown = to_markdown(&build_report(
            &row,
            "Nightly",
            "FAILED",
            true,
            &tasks,
            &before_map,
        ));
        let want = "# Regression run: Nightly (2026-09-26T02:00:00Z)

Regressions found. 5 cases: 2 passed, 2 failed, 1 error; 1 regressed, 1 fixed, 1 still failing.

## What changed or fails

- **Case 2** (FAILED, regressed): expected 200, got 500
- **Case 3** (FAILED, still failing): still broken
- **Case 5** (ERROR, new): took too long
- **Case 4** (PASSED, fixed)

## Not repeated

- Case 9: approved, but not run yet

## Notes

- 1 case(s) are compared with the run they were copied from: this schedule has no earlier run of them.
- 1 case(s) in scope were not repeated: see what could not be repeated.
";
        assert_eq!(markdown, want);
        let quiet = to_markdown(&build_report(
            &row_with(1),
            "N",
            "COMPLETED",
            true,
            &BTreeMap::from([("task-1".to_string(), outcome(TaskStatus::Passed, None))]),
            &BTreeMap::from([(
                "case-1".to_string(),
                before("PASSED", BeforeFrom::PreviousRun),
            )]),
        ));
        assert!(
            quiet.contains("All passing. 1 cases: 1 passed, 0 failed, 0 error; 0 regressed, 0 fixed, 0 still failing."),
            "{quiet}"
        );
        assert!(
            !quiet.contains("## "),
            "nothing to list when nothing changed or fails: {quiet}"
        );
    }
}
