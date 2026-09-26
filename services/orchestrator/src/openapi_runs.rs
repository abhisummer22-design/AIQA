//! Phase 8B — what an OpenAPI test run was made from (`0024_openapi_runs.sql`), and the report made of it once its tasks have answered.
//!
//! The run itself is an ordinary execution (one task for each case that is run: `openapi_api.rs`). This file keeps the cases as they were
//! derived, each with the task that ran it, and reads them back next to the tasks' own outcomes: for each operation of the document what
//! was expected and what came back, and how much of the document was tested at all. Nothing in the report is worked out twice: the
//! assertions' own words (`ApiResultSummary`) are what say why a case failed.

use crate::domain::TaskStatus;
use crate::executor::{
    ApiAssertionResult, ApiResultSummary, BODY_SCHEMA_DESCRIPTION, STATUS_IN_DESCRIPTION,
};
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

/// Why a case that ran did not pass, in the words of the assertions that did not hold. A body that does not fit the schema of the success
/// answer says nothing when the status already says that was not the answer (an error page is not the success body), so it is left out.
fn failure_reason(outcome: &TaskOutcome) -> Option<String> {
    if let Some(result) = &outcome.result {
        let failed: Vec<&ApiAssertionResult> =
            result.assertions.iter().filter(|a| !a.passed).collect();
        let wrong_status = failed
            .iter()
            .any(|a| a.description.starts_with(STATUS_IN_DESCRIPTION));
        let told: Vec<String> = failed
            .iter()
            .filter(|a| !(wrong_status && a.description == BODY_SCHEMA_DESCRIPTION))
            .map(|a| format!("{}: got {}", a.description, a.actual))
            .collect();
        if !told.is_empty() {
            return Some(shorten(&told.join("; ")));
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
    let not_tested: Vec<String> = coverage["not_tested"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|n| format!("{} ({})", text(&n["operation"]), text(&n["reason"])))
        .collect();
    if !not_tested.is_empty() {
        out.push_str(&format!("Not tested: {}.\n", not_tested.join("; ")));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi_cases::{CaseRequest, Expect};

    fn case(operation: &str, kind: CaseKind, skipped: bool) -> Case {
        let (method, path) = operation.split_once(' ').unwrap();
        Case {
            id: format!("{operation}#{}", kind.name()),
            operation: operation.to_string(),
            kind,
            title: format!("{operation}: {}", kind.name().to_lowercase()),
            request: CaseRequest {
                method: method.to_string(),
                url: format!("https://api.test/v1{path}"),
                headers: BTreeMap::new(),
                body: None,
            },
            expect: Expect {
                statuses: vec![200],
                schema: Some(json!({"type": "object"})),
                why: "the document lists 200 as the success response".to_string(),
            },
            skipped: skipped.then(|| {
                format!("{method} changes data on the server: it is run only when unsafe methods are allowed")
            }),
        }
    }

    /// A case that is run by `task`, or, with none, one that is held back.
    fn stored(operation: &str, kind: CaseKind, task: Option<&str>) -> StoredCase {
        StoredCase {
            case: case(operation, kind, task.is_none()),
            task_id: task.map(str::to_string),
        }
    }

    fn operation(id: &str, selected: bool, skipped: Option<&str>) -> StoredOperation {
        StoredOperation {
            id: id.to_string(),
            summary: None,
            tags: Vec::new(),
            selected,
            skipped: skipped.map(str::to_string),
        }
    }

    fn row(
        operations: Vec<StoredOperation>,
        cases: Vec<StoredCase>,
        notes: &[&str],
    ) -> OpenApiRunRow {
        OpenApiRunRow {
            id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            project_id: "proj".to_string(),
            run_id: Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            title: "Pets".to_string(),
            version: "1".to_string(),
            base_url: "https://api.test/v1".to_string(),
            options: json!({}),
            operations,
            cases,
            notes: notes.iter().map(|n| n.to_string()).collect(),
            created_by: "tester".to_string(),
            created_at: DateTime::parse_from_rfc3339("2026-09-26T10:15:03Z")
                .unwrap()
                .with_timezone(&Utc),
        }
    }

    fn assertion(description: &str, actual: &str, passed: bool) -> ApiAssertionResult {
        ApiAssertionResult {
            description: description.to_string(),
            actual: actual.to_string(),
            passed,
        }
    }

    fn outcome(
        status: TaskStatus,
        reason: Option<&str>,
        result: Option<ApiResultSummary>,
    ) -> TaskOutcome {
        TaskOutcome {
            status,
            reason: reason.map(str::to_string),
            result,
        }
    }

    fn answer(http: u16, assertions: Vec<ApiAssertionResult>) -> ApiResultSummary {
        ApiResultSummary {
            status: http,
            request_duration_ms: 12,
            response_headers: Vec::new(),
            assertions,
        }
    }

    fn tasks(list: Vec<(&str, TaskOutcome)>) -> BTreeMap<String, TaskOutcome> {
        list.into_iter()
            .map(|(id, t)| (id.to_string(), t))
            .collect()
    }

    fn status_assertion(list: &str, actual: &str, passed: bool) -> ApiAssertionResult {
        assertion(&format!("{STATUS_IN_DESCRIPTION} {list}"), actual, passed)
    }

    fn body_assertion(actual: &str, passed: bool) -> ApiAssertionResult {
        assertion(BODY_SCHEMA_DESCRIPTION, actual, passed)
    }

    // ------------------------------------------------------------------------------------------------- outcomes and verdicts

    #[test]
    fn every_status_a_task_can_end_in_is_one_outcome_of_a_case() {
        use TaskStatus::*;
        let table = [
            (Passed, "PASSED"),
            (Failed, "FAILED"),
            (Timeout, "ERROR"),
            (InfraError, "ERROR"),
            (Queued, "PENDING"),
            (Running, "PENDING"),
            (Blocked, "NOT_RUN"),
            (Skipped, "NOT_RUN"),
            (Cancelled, "NOT_RUN"),
            (NeedsReview, "NOT_RUN"),
        ];
        for (status, want) in table {
            assert_eq!(outcome_of(status), want, "{status:?}");
        }
    }

    #[test]
    fn an_operation_comes_to_its_worst_outcome() {
        let counts = |passed, failed, error, pending, not_run| Counts {
            passed,
            failed,
            error,
            pending,
            not_run,
        };
        assert_eq!(counts(9, 1, 1, 1, 1).verdict(), "FAILED");
        assert_eq!(counts(9, 0, 1, 1, 1).verdict(), "ERROR");
        assert_eq!(counts(9, 0, 0, 1, 1).verdict(), "PENDING");
        assert_eq!(counts(1, 0, 0, 0, 9).verdict(), "PASSED");
        assert_eq!(counts(0, 0, 0, 0, 3).verdict(), "NOT_RUN");
        assert_eq!(counts(0, 0, 0, 0, 0).verdict(), "NOT_RUN");
    }

    #[test]
    fn counts_add_each_outcome_to_its_own_and_ran_is_all_that_answered_or_tried() {
        let mut counts = Counts::default();
        for outcome in [
            "PASSED",
            "PASSED",
            "FAILED",
            "ERROR",
            "PENDING",
            "NOT_RUN",
            "anything else",
        ] {
            counts.add(outcome);
        }
        assert_eq!(
            counts.json(),
            json!({"passed": 2, "failed": 1, "error": 1, "pending": 1, "not_run": 2})
        );
        assert_eq!(counts.ran(), 4, "pending and not run have not run");
    }

    // ------------------------------------------------------------------------------------------------- the reason a case gives

    #[test]
    fn a_failed_case_says_which_assertions_did_not_hold_in_their_words() {
        let failed = outcome(
            TaskStatus::Failed,
            Some("the task's own reason"),
            Some(answer(
                200,
                vec![
                    status_assertion("400", "200 OK", false),
                    assertion("Header X is set", "(absent)", false),
                    assertion("Something that held", "fine", true),
                ],
            )),
        );
        assert_eq!(
            failure_reason(&failed).as_deref(),
            Some("Status is one of 400: got 200 OK; Header X is set: got (absent)"),
            "only the assertions that failed, each as it says itself"
        );
    }

    #[test]
    fn a_body_that_does_not_fit_is_left_out_when_the_status_already_says_it_was_not_the_answer() {
        let both = outcome(
            TaskStatus::Failed,
            None,
            Some(answer(
                404,
                vec![
                    status_assertion("200", "404 Not Found", false),
                    body_assertion(
                        "(not valid JSON, content type application/json: expected value)",
                        false,
                    ),
                ],
            )),
        );
        assert_eq!(
            failure_reason(&both).as_deref(),
            Some("Status is one of 200: got 404 Not Found")
        );
        // With the right status, a body that does not fit is the whole story.
        let body_only = outcome(
            TaskStatus::Failed,
            None,
            Some(answer(
                200,
                vec![
                    status_assertion("200", "200 OK", true),
                    body_assertion("$.id: expected integer, got string", false),
                ],
            )),
        );
        assert_eq!(
            failure_reason(&body_only).as_deref(),
            Some("Body fits its schema: got $.id: expected integer, got string")
        );
        // A status that held does not hide a body that did not, and a body that held is not told.
        let status_ok_body_ok_other_bad = outcome(
            TaskStatus::Failed,
            None,
            Some(answer(
                200,
                vec![
                    status_assertion("200", "200 OK", true),
                    body_assertion("fits", true),
                    assertion("Header X is set", "(absent)", false),
                ],
            )),
        );
        assert_eq!(
            failure_reason(&status_ok_body_ok_other_bad).as_deref(),
            Some("Header X is set: got (absent)")
        );
    }

    #[test]
    fn when_the_evidence_says_nothing_the_tasks_own_reason_is_used_and_when_there_is_none_there_is_none(
    ) {
        let passed_assertions_only = outcome(
            TaskStatus::Failed,
            Some("REQUEST_TIMEOUT: took too long"),
            Some(answer(200, vec![status_assertion("200", "200 OK", true)])),
        );
        assert_eq!(
            failure_reason(&passed_assertions_only).as_deref(),
            Some("REQUEST_TIMEOUT: took too long")
        );
        let no_evidence = outcome(
            TaskStatus::InfraError,
            Some("CONNECTION_FAILED: refused"),
            None,
        );
        assert_eq!(
            failure_reason(&no_evidence).as_deref(),
            Some("CONNECTION_FAILED: refused")
        );
        assert_eq!(
            failure_reason(&outcome(TaskStatus::Failed, None, None)),
            None
        );
        assert_eq!(
            failure_reason(&outcome(TaskStatus::Failed, Some("  \n "), None)),
            None,
            "a reason that is only white space is none"
        );
    }

    #[test]
    fn a_reason_is_on_one_line_and_not_longer_than_the_limit() {
        let flat = outcome(TaskStatus::Failed, Some("a\n  b\t\tc\r\nd"), None);
        assert_eq!(failure_reason(&flat).as_deref(), Some("a b c d"));
        let exactly = "x".repeat(MAX_REASON_CHARS);
        assert_eq!(shorten(&exactly), exactly, "at the limit it is whole");
        let over = "x".repeat(MAX_REASON_CHARS + 1);
        assert_eq!(shorten(&over), format!("{}…", "x".repeat(MAX_REASON_CHARS)));
        let words = "word ".repeat(200);
        let cut = shorten(&words);
        assert!(
            cut.ends_with("word…")
                || cut.ends_with("wor…")
                || cut.ends_with("wo…")
                || cut.ends_with("w…"),
            "{cut}"
        );
        assert!(
            !cut.contains(" …"),
            "the white space before the cut is not kept: {cut}"
        );
        assert!(cut.chars().count() <= MAX_REASON_CHARS + 1);
    }

    // ------------------------------------------------------------------------------------------------- the report

    /// Five cases of three operations, one that is run and passes, one that fails on a wrong status, one that could not be run, one
    /// that was cancelled and one that changes data; and three operations that have no case at all.
    fn ended_run() -> (OpenApiRunRow, BTreeMap<String, TaskOutcome>) {
        let row = row(
            vec![
                operation("GET /pets", true, None),
                operation("POST /pets", true, None),
                operation("GET /pets/{id}", true, None),
                operation("GET /old", true, Some("the operation is marked deprecated")),
                operation("GET /other", false, None),
                operation("GET /bare", true, None),
            ],
            vec![
                stored("GET /pets", CaseKind::Happy, Some("api-0001")),
                stored("GET /pets", CaseKind::MissingRequired, Some("api-0002")),
                stored("POST /pets", CaseKind::Happy, None),
                stored("GET /pets/{id}", CaseKind::Happy, Some("api-0003")),
                stored("GET /pets/{id}", CaseKind::NotFound, Some("api-0004")),
            ],
            &["a note from the derivation"],
        );
        let tasks = tasks(vec![
            ("api-0001", outcome(TaskStatus::Passed, None, None)),
            (
                "api-0002",
                outcome(
                    TaskStatus::Failed,
                    None,
                    Some(answer(
                        200,
                        vec![
                            status_assertion("400", "200 OK", false),
                            body_assertion("(not valid JSON, content type text/plain: x)", false),
                        ],
                    )),
                ),
            ),
            (
                "api-0003",
                outcome(TaskStatus::Timeout, Some("REQUEST_TIMEOUT: slow"), None),
            ),
            ("api-0004", outcome(TaskStatus::Cancelled, None, None)),
        ]);
        (row, tasks)
    }

    fn case_of<'a>(report: &'a Value, id: &str) -> &'a Value {
        report["operations"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|op| op["cases"].as_array().unwrap().iter())
            .find(|c| c["id"] == id)
            .unwrap_or_else(|| panic!("no case {id}"))
    }

    fn operation_of<'a>(report: &'a Value, id: &str) -> &'a Value {
        report["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["id"] == id)
            .unwrap_or_else(|| panic!("no operation {id}"))
    }

    #[test]
    fn a_report_names_the_run_the_document_and_where_it_was_tested() {
        let (row, tasks) = ended_run();
        let report = build_report(&row, "FAILED", true, &tasks);
        assert_eq!(report["id"], "11111111-1111-4111-8111-111111111111");
        assert_eq!(report["run_id"], "22222222-2222-4222-8222-222222222222");
        assert_eq!(report["project_id"], "proj");
        assert_eq!(
            report["spec"],
            json!({"title": "Pets", "version": "1", "base_url": "https://api.test/v1"})
        );
        assert_eq!(report["created_by"], "tester");
        assert_eq!(report["created_at"], "2026-09-26T10:15:03Z");
        assert_eq!(report["run_state"], "FAILED");
        assert_eq!(report["final"], true);
    }

    #[test]
    fn a_report_counts_every_case_once() {
        let (row, tasks) = ended_run();
        let report = build_report(&row, "FAILED", true, &tasks);
        assert_eq!(
            report["summary"],
            json!({"cases": 5, "ran": 3, "passed": 1, "failed": 1, "error": 1, "pending": 0, "not_run": 2, "pass_rate": 1.0 / 3.0})
        );
        assert_eq!(
            report["by_kind"]["HAPPY"],
            json!({"passed": 1, "failed": 0, "error": 1, "pending": 0, "not_run": 1})
        );
        assert_eq!(
            report["by_kind"]["MISSING_REQUIRED"],
            json!({"passed": 0, "failed": 1, "error": 0, "pending": 0, "not_run": 0})
        );
        assert_eq!(
            report["by_kind"]["NOT_FOUND"],
            json!({"passed": 0, "failed": 0, "error": 0, "pending": 0, "not_run": 1})
        );
        for quiet in ["NO_AUTH", "WRONG_TYPE"] {
            assert_eq!(
                report["by_kind"][quiet],
                json!({"passed": 0, "failed": 0, "error": 0, "pending": 0, "not_run": 0}),
                "a kind that was not made is there, with nothing in it"
            );
        }
    }

    #[test]
    fn each_case_says_what_it_expected_what_came_back_and_why_it_did_not_pass() {
        let (row, tasks) = ended_run();
        let report = build_report(&row, "FAILED", true, &tasks);

        let passed = case_of(&report, "GET /pets#HAPPY");
        assert_eq!(passed["outcome"], "PASSED");
        assert_eq!(passed["reason"], Value::Null);
        assert_eq!(
            passed["actual"],
            Value::Null,
            "the evidence of a case that passed is not read"
        );
        assert_eq!(passed["task_id"], "api-0001");
        assert_eq!(passed["kind"], "HAPPY");
        assert_eq!(passed["title"], "GET /pets: happy");
        assert_eq!(
            passed["request"],
            json!({"method": "GET", "url": "https://api.test/v1/pets"})
        );
        assert_eq!(
            passed["expect"],
            json!({"statuses": [200], "schema": true, "why": "the document lists 200 as the success response"})
        );

        let failed = case_of(&report, "GET /pets#MISSING_REQUIRED");
        assert_eq!(failed["outcome"], "FAILED");
        assert_eq!(failed["reason"], "Status is one of 400: got 200 OK");
        assert_eq!(failed["actual"]["status"], 200);
        assert_eq!(failed["actual"]["duration_ms"], 12);
        assert_eq!(
            failed["actual"]["assertions"],
            json!([
                {"description": "Status is one of 400", "actual": "200 OK", "passed": false},
                {"description": "Body fits its schema", "actual": "(not valid JSON, content type text/plain: x)", "passed": false},
            ]),
            "the reason leaves the moot body check out, the answer keeps everything that was found"
        );

        let error = case_of(&report, "GET /pets/{id}#HAPPY");
        assert_eq!(error["outcome"], "ERROR");
        assert_eq!(error["reason"], "REQUEST_TIMEOUT: slow");

        let cancelled = case_of(&report, "GET /pets/{id}#NOT_FOUND");
        assert_eq!(cancelled["outcome"], "NOT_RUN");
        assert_eq!(cancelled["reason"], "the task ended cancelled");

        let held = case_of(&report, "POST /pets#HAPPY");
        assert_eq!(held["outcome"], "NOT_RUN");
        assert_eq!(
            held["reason"],
            "POST changes data on the server: it is run only when unsafe methods are allowed"
        );
        assert_eq!(held["skipped"], held["reason"]);
        assert_eq!(held["task_id"], Value::Null);
    }

    #[test]
    fn an_operation_is_what_its_cases_came_to_and_one_with_none_says_why() {
        let (row, tasks) = ended_run();
        let report = build_report(&row, "FAILED", true, &tasks);
        let ids: Vec<&str> = report["operations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            [
                "GET /pets",
                "POST /pets",
                "GET /pets/{id}",
                "GET /old",
                "GET /other",
                "GET /bare"
            ],
            "in the order of the document"
        );
        let verdict = |id: &str| {
            operation_of(&report, id)["verdict"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert_eq!(verdict("GET /pets"), "FAILED");
        assert_eq!(verdict("POST /pets"), "NOT_RUN");
        assert_eq!(verdict("GET /pets/{id}"), "ERROR");
        assert_eq!(verdict("GET /old"), "SKIPPED");
        assert_eq!(verdict("GET /other"), "NOT_SELECTED");
        assert_eq!(verdict("GET /bare"), "SKIPPED");
        assert_eq!(
            operation_of(&report, "GET /pets")["counts"],
            json!({"passed": 1, "failed": 1, "error": 0, "pending": 0, "not_run": 0})
        );
        assert_eq!(
            operation_of(&report, "GET /pets")["cases"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            operation_of(&report, "GET /pets")["why_no_cases"],
            Value::Null
        );
        assert_eq!(
            operation_of(&report, "GET /old")["why_no_cases"],
            "the operation is marked deprecated"
        );
        assert_eq!(
            operation_of(&report, "GET /other")["why_no_cases"],
            "not selected"
        );
        assert_eq!(
            operation_of(&report, "GET /bare")["why_no_cases"],
            Value::Null
        );
        for none in ["GET /old", "GET /other", "GET /bare"] {
            assert_eq!(operation_of(&report, none)["counts"], Value::Null, "{none}");
            assert_eq!(operation_of(&report, none)["cases"], json!([]), "{none}");
        }
    }

    #[test]
    fn coverage_says_how_much_of_the_document_was_tested_and_what_was_not_and_why() {
        let (row, tasks) = ended_run();
        let report = build_report(&row, "FAILED", true, &tasks);
        assert_eq!(report["coverage"]["operations"], 6);
        assert_eq!(
            report["coverage"]["tested"], 2,
            "an operation whose case could not be run to the end was still tried"
        );
        assert_eq!(
            report["coverage"]["not_tested"],
            json!([
                {"operation": "POST /pets", "reason": "none of its cases was run"},
                {"operation": "GET /old", "reason": "the operation is marked deprecated"},
                {"operation": "GET /other", "reason": "not selected"},
                {"operation": "GET /bare", "reason": "no case could be made for it"},
            ])
        );
    }

    #[test]
    fn the_notes_are_the_derivations_and_one_for_what_was_held_back_and_a_run_that_is_still_going_says_so(
    ) {
        let (row, tasks) = ended_run();
        let ended = build_report(&row, "FAILED", true, &tasks);
        assert_eq!(
            ended["notes"],
            json!([
                "a note from the derivation",
                "1 case(s) were not run because their request changes data (POST, PUT, PATCH, DELETE): run again with unsafe methods allowed to run them.",
            ])
        );
        let going = build_report(&row, "RUNNING", false, &tasks);
        assert_eq!(going["final"], false);
        assert_eq!(going["run_state"], "RUNNING");
        assert_eq!(
            going["notes"][1],
            "The run has not ended: this is what it has found so far."
        );
        assert_eq!(going["notes"].as_array().unwrap().len(), 3);
        let nothing_held = build_report(
            &row_with_no_held_case(),
            "COMPLETED",
            true,
            &tasks_for_no_held_case(),
        );
        assert_eq!(
            nothing_held["notes"],
            json!([]),
            "no note when there is nothing to say"
        );
    }

    fn row_with_no_held_case() -> OpenApiRunRow {
        row(
            vec![operation("GET /pets", true, None)],
            vec![stored("GET /pets", CaseKind::Happy, Some("api-0001"))],
            &[],
        )
    }

    fn tasks_for_no_held_case() -> BTreeMap<String, TaskOutcome> {
        tasks(vec![("api-0001", outcome(TaskStatus::Passed, None, None))])
    }

    #[test]
    fn a_case_that_has_no_answer_yet_is_pending_while_the_run_goes_and_not_run_once_it_has_ended() {
        let row = row(
            vec![
                operation("GET /pets", true, None),
                operation("GET /things", true, None),
            ],
            vec![
                stored("GET /pets", CaseKind::Happy, Some("api-0001")),
                stored("GET /things", CaseKind::Happy, Some("api-0002")),
            ],
            &[],
        );
        let so_far = tasks(vec![("api-0001", outcome(TaskStatus::Running, None, None))]);
        let going = build_report(&row, "RUNNING", false, &so_far);
        assert_eq!(
            case_of(&going, "GET /pets#HAPPY")["outcome"],
            "PENDING",
            "its task is running"
        );
        assert_eq!(
            case_of(&going, "GET /things#HAPPY")["outcome"],
            "PENDING",
            "its task is not even listed yet"
        );
        assert_eq!(going["summary"]["pending"], 2);
        assert_eq!(going["summary"]["ran"], 0);
        assert_eq!(operation_of(&going, "GET /pets")["verdict"], "PENDING");
        assert_eq!(
            going["coverage"]["not_tested"],
            json!([
                {"operation": "GET /pets", "reason": "its cases have not run yet"},
                {"operation": "GET /things", "reason": "its cases have not run yet"},
            ])
        );
        let ended = build_report(&row, "COMPLETED", true, &so_far);
        assert_eq!(
            case_of(&ended, "GET /things#HAPPY")["outcome"],
            "NOT_RUN",
            "a task that never appeared and a run that has ended: it did not run"
        );
        assert_eq!(
            case_of(&ended, "GET /pets#HAPPY")["outcome"],
            "PENDING",
            "a task still running is pending, ended run or not"
        );
    }

    #[test]
    fn the_pass_rate_is_of_the_cases_that_ran_and_none_when_none_did() {
        let one = row(
            vec![operation("GET /pets", true, None)],
            vec![
                stored("GET /pets", CaseKind::Happy, Some("api-0001")),
                stored("GET /pets", CaseKind::NoAuth, Some("api-0002")),
                stored("GET /pets", CaseKind::WrongType, None),
            ],
            &[],
        );
        let all_pass = tasks(vec![
            ("api-0001", outcome(TaskStatus::Passed, None, None)),
            ("api-0002", outcome(TaskStatus::Passed, None, None)),
        ]);
        let report = build_report(&one, "COMPLETED", true, &all_pass);
        assert_eq!(
            report["summary"]["pass_rate"], 1.0,
            "one held back does not count against it"
        );
        assert_eq!(report["summary"]["ran"], 2);
        let half = tasks(vec![
            ("api-0001", outcome(TaskStatus::Passed, None, None)),
            ("api-0002", outcome(TaskStatus::Failed, None, None)),
        ]);
        assert_eq!(
            build_report(&one, "FAILED", true, &half)["summary"]["pass_rate"],
            0.5
        );
        let none_ran = build_report(&one, "CANCELLED", true, &BTreeMap::new());
        assert_eq!(none_ran["summary"]["pass_rate"], Value::Null);
        assert_eq!(none_ran["summary"]["ran"], 0);
    }

    // ------------------------------------------------------------------------------------------------- Markdown

    #[test]
    fn the_markdown_says_what_ran_what_did_not_pass_and_what_to_know() {
        let (row, tasks) = ended_run();
        let markdown = to_markdown(&build_report(&row, "FAILED", true, &tasks));
        let want = "# API tests from Pets 1

Against https://api.test/v1. 5 cases: 1 passed, 1 failed, 1 error, 2 not run.

Operations tested: 2 of 6.
Not tested: POST /pets (none of its cases was run); GET /old (the operation is marked deprecated); GET /other (not selected); GET /bare (no case could be made for it).

| Operation | Verdict | Passed | Failed | Error | Not run |
|---|---|---|---|---|---|
| GET /pets | FAILED | 1 | 1 | 0 | 0 |
| POST /pets | NOT_RUN | 0 | 0 | 0 | 1 |
| GET /pets/{id} | ERROR | 0 | 0 | 1 | 1 |
| GET /old | SKIPPED | 0 | 0 | 0 | 0 |
| GET /other | NOT_SELECTED | 0 | 0 | 0 | 0 |
| GET /bare | SKIPPED | 0 | 0 | 0 | 0 |

## What did not pass

- **GET /pets: missing_required** (FAILED): Status is one of 400: got 200 OK
  GET https://api.test/v1/pets
- **GET /pets/{id}: happy** (ERROR): REQUEST_TIMEOUT: slow
  GET https://api.test/v1/pets/{id}

## Notes

- a note from the derivation
- 1 case(s) were not run because their request changes data (POST, PUT, PATCH, DELETE): run again with unsafe methods allowed to run them.
";
        assert_eq!(markdown, want);
    }

    #[test]
    fn a_markdown_report_of_a_run_that_is_going_says_so_and_has_no_sections_it_has_nothing_for() {
        let row = row(
            vec![operation("GET /a|b", true, None)],
            vec![stored("GET /a|b", CaseKind::Happy, Some("api-0001"))],
            &[],
        );
        let markdown = to_markdown(&build_report(&row, "RUNNING", false, &BTreeMap::new()));
        assert!(
            markdown.contains("Against https://api.test/v1. 1 cases: 0 passed, 0 failed, 0 error, 0 not run (the run has not ended).\n"),
            "{markdown}"
        );
        assert!(
            markdown.contains("| GET /a\\|b | PENDING | 0 | 0 | 0 | 0 |\n"),
            "a pipe in a name does not break the table: {markdown}"
        );
        assert!(!markdown.contains("## What did not pass"), "{markdown}");
        assert!(
            markdown.contains(
                "## Notes\n\n- The run has not ended: this is what it has found so far.\n"
            ),
            "{markdown}"
        );
        assert!(
            markdown.contains("Not tested: GET /a|b (its cases have not run yet).\n"),
            "{markdown}"
        );
    }

    #[test]
    fn a_failure_with_no_reason_says_so_and_a_report_with_everything_tested_has_no_not_tested_line()
    {
        let row = row(
            vec![operation("GET /pets", true, None)],
            vec![stored("GET /pets", CaseKind::Happy, Some("api-0001"))],
            &[],
        );
        let tasks = tasks(vec![("api-0001", outcome(TaskStatus::Failed, None, None))]);
        let markdown = to_markdown(&build_report(&row, "FAILED", true, &tasks));
        assert!(
            markdown.contains("(FAILED): no reason was given\n  GET https://api.test/v1/pets\n"),
            "{markdown}"
        );
        assert!(!markdown.contains("Not tested"), "{markdown}");
        assert!(!markdown.contains("## Notes"), "{markdown}");
    }
}
