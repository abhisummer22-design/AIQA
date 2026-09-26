//! Phase 8B — what an OpenAPI test run was made from (`0024_openapi_runs.sql`), and the report made of it once its tasks have answered.
//!
//! The run itself is an ordinary execution (one task for each case that is run: `openapi_api.rs`). This file keeps the cases as they were
//! derived, each with the task that ran it, and reads them back next to the tasks' own outcomes: for each operation of the document what
//! was expected and what came back, and how much of the document was tested at all. Nothing in the report is worked out twice: the
//! assertions' own words (`ApiResultSummary`) are what say why a case failed.

use crate::domain::TaskStatus;
use crate::executor::ApiResultSummary;
use crate::openapi_cases::{Case, CaseKind};
use crate::store::{now_rfc3339, parse_ts, Store};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::BTreeMap;
use uuid::Uuid;

/// The most failed cases whose evidence is read for one report: the rest say only that they failed.
pub const MAX_DETAILED: usize = 200;
/// The longest reason a case carries.
const MAX_REASON_CHARS: usize = 400;

/// A derived case with the task that runs it (none when it is not run).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredCase {
    #[serde(flatten)]
    pub case: Case,
    pub task_id: Option<String>,
}

/// An operation of the document, and what became of it when the cases were derived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredOperation {
    pub id: String,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    /// Asked for (all of them are, unless the person picked some).
    pub selected: bool,
    /// Why no case was made for it, when none was.
    pub skipped: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OpenApiRunRow {
    pub id: Uuid,
    pub project_id: String,
    pub run_id: Uuid,
    pub title: String,
    pub version: String,
    pub base_url: String,
    pub options: Value,
    pub operations: Vec<StoredOperation>,
    pub cases: Vec<StoredCase>,
    pub notes: Vec<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

/// A run in a project's list: what it was made from, not the cases.
#[derive(Debug, Clone, Serialize)]
pub struct OpenApiRunListRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub title: String,
    pub version: String,
    pub base_url: String,
    pub cases: i64,
    pub cases_run: i64,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

fn json_column<T: serde::de::DeserializeOwned>(text: &str) -> Result<T> {
    Ok(serde_json::from_str(text)?)
}

impl Store {
    pub async fn insert_openapi_run(&self, row: &OpenApiRunRow) -> Result<()> {
        sqlx::query(
            "INSERT INTO openapi_runs (id, project_id, run_id, title, version, base_url, options, operations, cases, notes, created_by, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(row.id.to_string())
        .bind(&row.project_id)
        .bind(row.run_id.to_string())
        .bind(&row.title)
        .bind(&row.version)
        .bind(&row.base_url)
        .bind(serde_json::to_string(&row.options)?)
        .bind(serde_json::to_string(&row.operations)?)
        .bind(serde_json::to_string(&row.cases)?)
        .bind(serde_json::to_string(&row.notes)?)
        .bind(&row.created_by)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_openapi_run(&self, id: Uuid) -> Result<Option<OpenApiRunRow>> {
        let Some(r) = sqlx::query(
            "SELECT id, project_id, run_id, title, version, base_url, options, operations, cases, notes, created_by, created_at
             FROM openapi_runs WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(OpenApiRunRow {
            id,
            project_id: r.get("project_id"),
            run_id: Uuid::parse_str(&r.get::<String, _>("run_id"))?,
            title: r.get("title"),
            version: r.get("version"),
            base_url: r.get("base_url"),
            options: json_column(&r.get::<String, _>("options"))?,
            operations: json_column(&r.get::<String, _>("operations"))?,
            cases: json_column(&r.get::<String, _>("cases"))?,
            notes: json_column(&r.get::<String, _>("notes"))?,
            created_by: r.get("created_by"),
            created_at: parse_ts(&r.get::<String, _>("created_at")),
        }))
    }

    /// The project's runs, newest first.
    pub async fn list_openapi_runs(
        &self,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<OpenApiRunListRow>> {
        let rows = sqlx::query(
            "SELECT id, run_id, title, version, base_url, created_by, created_at,
                    json_array_length(cases) AS cases,
                    (SELECT count(*) FROM json_each(cases) WHERE json_extract(value, '$.task_id') IS NOT NULL) AS cases_run
             FROM openapi_runs WHERE project_id = ? ORDER BY created_at DESC, id LIMIT ?",
        )
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(OpenApiRunListRow {
                    id: Uuid::parse_str(&r.get::<String, _>("id"))?,
                    run_id: Uuid::parse_str(&r.get::<String, _>("run_id"))?,
                    title: r.get("title"),
                    version: r.get("version"),
                    base_url: r.get("base_url"),
                    cases: r.get("cases"),
                    cases_run: r.get("cases_run"),
                    created_by: r.get("created_by"),
                    created_at: parse_ts(&r.get::<String, _>("created_at")),
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------------------------------------------- the report

/// What one task of the run came to, and what it said about the answer it got.
#[derive(Debug, Clone)]
pub struct TaskOutcome {
    pub status: TaskStatus,
    /// The task's own reason (the tail of its log): used when the evidence could not be read.
    pub reason: Option<String>,
    pub result: Option<ApiResultSummary>,
}

/// What a case came to: `PASSED`, `FAILED` (an assertion did not hold), `ERROR` (it could not be run to the end: no answer, a timeout,
/// a variable not set), `PENDING` (not yet) or `NOT_RUN`.
pub fn outcome_of(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Passed => "PASSED",
        TaskStatus::Failed => "FAILED",
        TaskStatus::Timeout | TaskStatus::InfraError => "ERROR",
        TaskStatus::Queued | TaskStatus::Running => "PENDING",
        TaskStatus::Blocked
        | TaskStatus::Skipped
        | TaskStatus::Cancelled
        | TaskStatus::NeedsReview => "NOT_RUN",
    }
}

fn shorten(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > MAX_REASON_CHARS {
        format!(
            "{}…",
            flat.chars()
                .take(MAX_REASON_CHARS)
                .collect::<String>()
                .trim_end()
        )
    } else {
        flat
    }
}

/// Why a case that ran did not pass, in the words of the assertions that did not hold.
fn failure_reason(outcome: &TaskOutcome) -> Option<String> {
    if let Some(result) = &outcome.result {
        let failed: Vec<String> = result
            .assertions
            .iter()
            .filter(|a| !a.passed)
            .map(|a| format!("{}: got {}", a.description, a.actual))
            .collect();
        if !failed.is_empty() {
            return Some(shorten(&failed.join("; ")));
        }
    }
    outcome
        .reason
        .as_deref()
        .map(shorten)
        .filter(|r| !r.is_empty())
}

#[derive(Default)]
struct Counts {
    passed: u64,
    failed: u64,
    error: u64,
    pending: u64,
    not_run: u64,
}

impl Counts {
    fn add(&mut self, outcome: &str) {
        match outcome {
            "PASSED" => self.passed += 1,
            "FAILED" => self.failed += 1,
            "ERROR" => self.error += 1,
            "PENDING" => self.pending += 1,
            _ => self.not_run += 1,
        }
    }

    fn ran(&self) -> u64 {
        self.passed + self.failed + self.error
    }

    fn json(&self) -> Value {
        json!({"passed": self.passed, "failed": self.failed, "error": self.error, "pending": self.pending, "not_run": self.not_run})
    }

    /// What an operation as a whole came to.
    fn verdict(&self) -> &'static str {
        if self.failed > 0 {
            "FAILED"
        } else if self.error > 0 {
            "ERROR"
        } else if self.pending > 0 {
            "PENDING"
        } else if self.passed > 0 {
            "PASSED"
        } else {
            "NOT_RUN"
        }
    }
}

/// The report of a run: `tasks` are its tasks' outcomes by task id, `final_run` whether the run has ended.
pub fn build_report(
    row: &OpenApiRunRow,
    run_state: &str,
    final_run: bool,
    tasks: &BTreeMap<String, TaskOutcome>,
) -> Value {
    let mut total = Counts::default();
    let mut by_kind: BTreeMap<&'static str, Counts> = CaseKind::ALL
        .iter()
        .map(|k| (k.name(), Counts::default()))
        .collect();
    let mut by_operation: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    let mut operation_counts: BTreeMap<&str, Counts> = BTreeMap::new();
    let mut unsafe_not_run = 0u64;

    for stored in &row.cases {
        let case = &stored.case;
        let task = stored.task_id.as_ref().and_then(|id| tasks.get(id));
        let (outcome, reason) = match (&case.skipped, task) {
            (Some(why), _) => ("NOT_RUN", Some(why.clone())),
            (None, None) => (if final_run { "NOT_RUN" } else { "PENDING" }, None),
            (None, Some(task)) => {
                let outcome = outcome_of(task.status);
                let reason = match outcome {
                    "FAILED" | "ERROR" => failure_reason(task),
                    "NOT_RUN" => Some(format!(
                        "the task ended {}",
                        task.status.as_str().to_lowercase().replace('_', " ")
                    )),
                    _ => None,
                };
                (outcome, reason)
            }
        };
        if case.skipped.is_some() {
            unsafe_not_run += 1;
        }
        total.add(outcome);
        by_kind.entry(case.kind.name()).or_default().add(outcome);
        operation_counts
            .entry(case.operation.as_str())
            .or_default()
            .add(outcome);
        let actual = task.and_then(|t| t.result.as_ref()).map(|r| {
            json!({
                "status": r.status,
                "duration_ms": r.request_duration_ms,
                "assertions": r.assertions.iter().map(|a| json!({"description": a.description, "actual": a.actual, "passed": a.passed})).collect::<Vec<_>>(),
            })
        });
        by_operation.entry(case.operation.as_str()).or_default().push(json!({
            "id": case.id,
            "kind": case.kind.name(),
            "title": case.title,
            "outcome": outcome,
            "reason": reason,
            "request": {"method": case.request.method, "url": case.request.url},
            "expect": {"statuses": case.expect.statuses, "schema": case.expect.schema.is_some(), "why": case.expect.why},
            "actual": actual,
            "skipped": case.skipped,
            "task_id": stored.task_id,
        }));
    }

    let operations: Vec<Value> = row
        .operations
        .iter()
        .map(|op| {
            let counts = operation_counts.get(op.id.as_str());
            let verdict = match counts {
                Some(c) => c.verdict(),
                None if !op.selected => "NOT_SELECTED",
                None => "SKIPPED",
            };
            let why_no_cases = match counts {
                Some(_) => Value::Null,
                None if !op.selected => json!("not selected"),
                None => json!(op.skipped),
            };
            json!({
                "id": op.id,
                "summary": op.summary,
                "tags": op.tags,
                "verdict": verdict,
                "counts": counts.map(Counts::json),
                "why_no_cases": why_no_cases,
                "cases": by_operation.get(op.id.as_str()).cloned().unwrap_or_default(),
            })
        })
        .collect();

    let tested = operation_counts.values().filter(|c| c.ran() > 0).count();
    let not_tested: Vec<Value> = row
        .operations
        .iter()
        .filter(|op| {
            operation_counts
                .get(op.id.as_str())
                .map_or(true, |c| c.ran() == 0)
        })
        .map(|op| {
            let why = match operation_counts.get(op.id.as_str()) {
                Some(c) if c.pending > 0 => "its cases have not run yet".to_string(),
                Some(_) => "none of its cases was run".to_string(),
                None if !op.selected => "not selected".to_string(),
                None => op
                    .skipped
                    .clone()
                    .unwrap_or_else(|| "no case could be made for it".to_string()),
            };
            json!({"operation": op.id, "reason": why})
        })
        .collect();

    let mut notes = row.notes.clone();
    if !final_run {
        notes.push("The run has not ended: this is what it has found so far.".to_string());
    }
    if unsafe_not_run > 0 {
        notes.push(format!(
            "{unsafe_not_run} case(s) were not run because their request changes data (POST, PUT, PATCH, DELETE): run again with unsafe methods allowed to run them."
        ));
    }

    let pass_rate = if total.ran() == 0 {
        Value::Null
    } else {
        json!(total.passed as f64 / total.ran() as f64)
    };
    let kinds: serde_json::Map<String, Value> = by_kind
        .iter()
        .map(|(k, c)| (k.to_string(), c.json()))
        .collect();
    json!({
        "id": row.id,
        "run_id": row.run_id,
        "project_id": row.project_id,
        "spec": {"title": row.title, "version": row.version, "base_url": row.base_url},
        "created_at": row.created_at,
        "created_by": row.created_by,
        "run_state": run_state,
        "final": final_run,
        "summary": {
            "cases": row.cases.len(),
            "ran": total.ran(),
            "passed": total.passed,
            "failed": total.failed,
            "error": total.error,
            "pending": total.pending,
            "not_run": total.not_run,
            "pass_rate": pass_rate,
        },
        "by_kind": kinds,
        "operations": operations,
        "coverage": {
            "operations": row.operations.len(),
            "tested": tested,
            "not_tested": not_tested,
        },
        "notes": notes,
    })
}

/// The report as Markdown, to paste into a ticket or a pull request.
pub fn to_markdown(report: &Value) -> String {
    let text = |v: &Value| v.as_str().unwrap_or("").to_string();
    let num = |v: &Value| v.as_u64().unwrap_or(0);
    let s = &report["summary"];
    let mut out = format!(
        "# API tests from {} {}\n\nAgainst {}. {} cases: {} passed, {} failed, {} error, {} not run{}.\n",
        text(&report["spec"]["title"]),
        text(&report["spec"]["version"]),
        text(&report["spec"]["base_url"]),
        num(&s["cases"]),
        num(&s["passed"]),
        num(&s["failed"]),
        num(&s["error"]),
        num(&s["not_run"]),
        if report["final"] == true { "" } else { " (the run has not ended)" },
    );
    let coverage = &report["coverage"];
    out.push_str(&format!(
        "\nOperations tested: {} of {}.\n",
        num(&coverage["tested"]),
        num(&coverage["operations"])
    ));
    out.push_str("\n| Operation | Verdict | Passed | Failed | Error | Not run |\n|---|---|---|---|---|---|\n");
    for op in report["operations"].as_array().into_iter().flatten() {
        let c = &op["counts"];
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            text(&op["id"]).replace('|', "\\|"),
            text(&op["verdict"]),
            num(&c["passed"]),
            num(&c["failed"]),
            num(&c["error"]),
            num(&c["not_run"]),
        ));
    }
    let mut failures = Vec::new();
    for op in report["operations"].as_array().into_iter().flatten() {
        for case in op["cases"].as_array().into_iter().flatten() {
            if matches!(case["outcome"].as_str(), Some("FAILED") | Some("ERROR")) {
                failures.push(format!(
                    "- **{}** ({}): {}\n  {} {}",
                    text(&case["title"]),
                    text(&case["outcome"]),
                    case["reason"].as_str().unwrap_or("no reason was given"),
                    text(&case["request"]["method"]),
                    text(&case["request"]["url"]),
                ));
            }
        }
    }
    if !failures.is_empty() {
        out.push_str("\n## What did not pass\n\n");
        out.push_str(&failures.join("\n"));
        out.push('\n');
    }
    let notes: Vec<String> = report["notes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|n| format!("- {}", text(n)))
        .collect();
    if !notes.is_empty() {
        out.push_str("\n## Notes\n\n");
        out.push_str(&notes.join("\n"));
        out.push('\n');
    }
    out
}
