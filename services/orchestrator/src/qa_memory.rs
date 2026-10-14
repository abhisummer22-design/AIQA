//! Phase 3 (QA Intelligence Memory) — deterministic retrieval of historical QA
//! intelligence.
//!
//! ## What this is, and what it is not
//!
//! Every question this module answers was **already asked** somewhere in AIQA:
//! `/intelligence/failures` and `FailureInvestigation` cover failure history,
//! `/intelligence/flakiness` and `flakiness_metrics` cover test stability,
//! `/requirement-impact` covers requirement impact, `/findings` covers bugs.
//! This module does **not** replace any of them. It is the retrieval layer that
//! answers the *cross-cutting* questions those separate endpoints cannot:
//!
//! * module risk history — nothing served it (a module had no identity until Phase 2)
//! * test stability keyed on the **stable case id** rather than on `task_name`
//! * release history — nothing served it (a release had no identity until Phase 2)
//!
//! The stability and failure answers are then attributable to a module and to a
//! release, which is what makes the two new Phase 2 entities worth anything.
//!
//! ## The rules that bind every function here
//!
//! Master plan §3 "Retrieval Rules", enforced rather than assumed:
//!
//! * **Project scoped** — `project_id` is a required argument of every function and
//!   appears in every query. There is no "all projects" variant.
//! * **Deterministic** — no model, no embedding, no similarity. Every number is a
//!   count or a ratio over stored rows, and every list is explicitly ordered.
//! * **Evidence backed** — every figure carries the runs/rows it was computed from.
//!   A count with no evidence is not returned; an empty history yields
//!   `INSUFFICIENT_DATA` with the reason, never a zero dressed up as a finding.
//! * **Time aware** — every result carries an explicit window and the first/last
//!   time anything was observed, so "3 failures" cannot be read as "3 failures
//!   ever" when it means "3 in the last 7 days".
//! * **Explainable** — a `signals` list per result states each contributing fact in
//!   words, with its own numbers. No hidden reasoning is stored or returned.
//! * **Bounded** — every list is capped by a `*_MAX` constant, in the same style as
//!   the existing `store::LIST_*_MAX` bounds.
//!
//! ## What is deliberately absent
//!
//! No chain-of-thought (§1.5). No credential material (§1.5) — nothing here reads
//! `params`, `error_message` or any secret-bearing field. No historical claim that
//! outruns the stored rows (§1.4): every aggregate is computed at read time from the
//! rows that exist, so a pruned history reports less rather than more.

use crate::domain::TaskStatus;
use crate::store::{ExecutionTelemetryRow, ReleaseRow, Store};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use uuid::Uuid;

/// Ceiling on `signals` per result. A module with a thousand distinct failing tests
/// is summarised, not dumped.
const MAX_SIGNALS: usize = 20;
/// Ceiling on telemetry rows one retrieval reads. The store takes an i64 limit and
/// every existing caller passes 100 or 1000; 1000 is the largest in use, so this
/// phase does not silently read less history than the surfaces it sits beside.
pub const TELEMETRY_READ_MAX: i64 = 1000;
/// Ceiling on affected-test lists.
const MAX_AFFECTED_TESTS: usize = 50;
/// Ceiling on releases compared in one historical comparison.
const MAX_RELEASES_COMPARED: usize = 10;

// ---------------------------------------------------------------- domain

/// Why a retrieval produced nothing. Returned as data rather than as an HTTP error,
/// because "this project has no history yet" is a true and useful answer, not a
/// failure — and because it keeps the distinction between *no data* and *all good*
/// visible to the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InsufficientData {
    /// The project has no execution telemetry at all.
    NoExecutions,
    /// Telemetry exists but nothing matches the thing asked about.
    NoMatchingHistory,
    /// The thing asked about does not exist in this project.
    UnknownSubject,
}

impl InsufficientData {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NoExecutions => "NO_EXECUTIONS",
            Self::NoMatchingHistory => "NO_MATCHING_HISTORY",
            Self::UnknownSubject => "UNKNOWN_SUBJECT",
        }
    }

    /// Plain-language reason, safe to show a user. Says what is missing rather
    /// than asserting anything about quality.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::NoExecutions => {
                "this project has no recorded execution telemetry yet, so no history can be reported"
            }
            Self::NoMatchingHistory => {
                "this project has execution history, but none of it matches what was asked about"
            }
            Self::UnknownSubject => {
                "the module, test case or release asked about does not exist in this project"
            }
        }
    }
}

/// One contributed, checkable fact. Every `Signals` entry is a sentence a person can
/// verify against the stored rows — never a hidden inference.
#[derive(Debug, Clone, Serialize)]
pub struct Signal {
    /// Stable machine-readable kind, e.g. `failure_recurrence`.
    pub kind: String,
    /// The fact in one clause, e.g. "failed 4 times in 3 distinct runs".
    pub statement: String,
    /// The rows or runs the statement was computed from. Empty only when the
    /// statement is about the absence of something.
    pub evidence_runs: Vec<Uuid>,
    /// Confidence in this specific signal, 0..1, and what it is based on.
    pub confidence: f64,
    pub basis: String,
}

/// The window a result covers, stated explicitly so no figure can be read as
/// "all time" by accident.
#[derive(Debug, Clone, Serialize)]
pub struct Window {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub executions_considered: usize,
}

/// Shared envelope. `Some(view)` only ever wraps a view built from rows that exist.
#[derive(Debug, Clone, Serialize)]
pub struct Retrieved<T> {
    pub project_id: String,
    /// Present exactly when there is nothing to report, and then `view` is absent.
    pub insufficient_data: Option<InsufficientData>,
    pub view: Option<T>,
}

impl<T> Retrieved<T> {
    pub(crate) fn found(project_id: &str, view: T) -> Self {
        Self {
            project_id: project_id.to_string(),
            insufficient_data: None,
            view: Some(view),
        }
    }
    pub(crate) fn empty(project_id: &str, why: InsufficientData) -> Self {
        Self {
            project_id: project_id.to_string(),
            insufficient_data: Some(why),
            view: None,
        }
    }
    /// Whether this result carries a real view. Lets callers branch without
    /// re-inspecting the enum.
    pub fn is_found(&self) -> bool {
        self.view.is_some()
    }
}

// ------------------------------------------------------------ view models

/// How one test case has behaved over its recorded history.
#[derive(Debug, Clone, Serialize)]
pub struct TestStabilityView {
    pub test_case_id: String,
    /// Present only when the case is known to this project; a stability read for an
    /// unknown id is `UnknownSubject`, not a row of zeroes.
    pub test_name: Option<String>,
    pub module_ref: Option<String>,
    pub executions: usize,
    pub passed: usize,
    pub failed: usize,
    /// Recorded outcomes that are neither a pass nor a failure -- a step that was blocked,
    /// skipped, cancelled, or never started. Reported, never silently folded into the other two.
    pub other_outcomes: usize,
    /// Percentages 0..=100, not 0..1: an earlier version of this doc claimed `0..1`, which the
    /// implementation never did.
    ///
    /// The denominator is the **completed** pass/fail outcomes only, so an outcome where the
    /// step never ran (see `other_outcomes`) can neither create a rate nor dilute one; when
    /// there are no eligible outcomes at all the answer is `None`, not `0.0`.
    ///
    /// `None` when that denominator is zero. It is **not** `None` below two executions: one
    /// genuine pass really does measure 0% failures, and that is an existing tested contract.
    /// (`flakiness_score` is the field with a two-sample minimum, because one sample cannot
    /// show flakiness -- a single pass is not evidence of instability.)
    pub failure_rate: Option<f64>,
    /// `None` below the same threshold. The existing
    /// `adaptive_intelligence::evaluate_flakiness` owns the real calculation; this
    /// is the shape of the answer, not a second flakiness engine.
    pub flakiness_score: Option<f64>,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    /// Distinct failure fingerprints this case has produced.
    pub distinct_failure_fingerprints: usize,
    /// Most recent distinct fingerprint, for correlation.
    pub latest_failure_fingerprint: Option<String>,
    pub window: Window,
    pub signals: Vec<Signal>,
}

/// How one module (a `quality_nodes` vertex of type `module`) has behaved.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleRiskHistory {
    pub module_ref: String,
    pub module_name: String,
    pub linked_test_cases: usize,
    /// Test cases that have at least one recorded execution. Can be fewer than
    /// `linked_test_cases` — a case can be linked before it is ever run.
    pub test_cases_with_history: usize,
    pub executions: usize,
    pub failures: usize,
    /// **All-recorded-outcome rate** (B3, Q1): failures over *every* recorded execution row for
    /// this module's cases, so a blocked or unexecuted step is in the denominator.
    ///
    /// Unchanged in value from before Phase 7.3, deliberately: renaming or redefining it would
    /// silently repoint every existing consumer. Read [`Self::completed_failure_rate`] for the
    /// rate over executions that actually ran.
    pub failure_rate: Option<f64>,
    /// **Completed-execution failure rate, as a PERCENTAGE (0..100).**
    ///
    /// Renamed from `completed_failure_rate` (pre-Phase-8 follow-up): the API response carried
    /// a key of that name holding a 0..1 *fraction*, so one name meant two units. The API key
    /// keeps its fraction value and its name for compatibility; this field now says which of
    /// the two it is.
    ///
    /// Numerator and denominator count only executions
    /// that are a genuine `PASSED`, `FAILED` or `TIMEOUT`. Blocked, skipped and cancelled steps
    /// are excluded from both halves. `None` when nothing completed.
    ///
    /// `INFRA_ERROR` rows are excluded here too and reported as
    /// [`Self::infrastructure_errors`] (Q2).
    pub completed_failure_rate_percent: Option<f64>,
    /// Recorded outcomes where the **executor** failed rather than the test (Q2). A count, not a
    /// rate: it has no denominator and is never folded into either rate.
    pub infrastructure_errors: usize,
    pub affected_tests: Vec<AffectedTest>,
    /// Fingerprints seen more than once here, i.e. genuinely recurring rather than
    /// one-off.
    pub recurring_failures: Vec<RecurringFailure>,
    pub affected_releases: Vec<String>,
    pub window: Window,
    pub signals: Vec<Signal>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AffectedTest {
    pub test_case_id: String,
    pub executions: usize,
    pub failures: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecurringFailure {
    pub fingerprint: String,
    pub occurrences: usize,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    pub failure_category: Option<String>,
    pub affected_test_count: usize,
}

/// How one release's executions went.
#[derive(Debug, Clone, Serialize)]
pub struct ReleaseHistory {
    pub release_id: String,
    pub label: Option<String>,
    pub version: Option<String>,
    pub runs: usize,
    pub executions: usize,
    pub failures: usize,
    /// **All-recorded-outcome rate** (B3, Q1): failures over every recorded execution row for
    /// this release. Unchanged in value from before Phase 7.3, so existing consumers keep their
    /// current answer; read [`Self::completed_failure_rate`] for the completed-execution rate.
    pub failure_rate: Option<f64>,
    /// **Completed-execution failure rate, as a PERCENTAGE (0..100).**
    ///
    /// Renamed from `completed_failure_rate` (pre-Phase-8 follow-up): the API response carried
    /// a key of that name holding a 0..1 *fraction*, so one name meant two units. The API key
    /// keeps its fraction value and its name for compatibility; this field now says which of
    /// the two it is.
    /// Numerator and denominator count only `PASSED` / `FAILED` /
    /// `TIMEOUT` rows, so a blocked or unexecuted step cannot dilute it. `None` when nothing
    /// completed.
    pub completed_failure_rate_percent: Option<f64>,
    /// Executor-failure count (Q2). A count, not a rate.
    pub infrastructure_errors: usize,
    pub distinct_fingerprints: usize,
    /// Every other release of this project that has runs, newest first, with its own
    /// totals, so a comparison needs no second call.
    pub compared_against: Vec<ReleaseComparison>,
    pub window: Window,
    pub signals: Vec<Signal>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseComparison {
    pub release_id: String,
    pub label: Option<String>,
    pub runs: usize,
    pub executions: usize,
    pub failures: usize,
    /// **All-recorded-outcome rate** (B3, Q1). Unchanged from before Phase 7.3.
    pub failure_rate: Option<f64>,
    /// **Completed-execution failure rate, as a PERCENTAGE (0..100).**
    ///
    /// Renamed from `completed_failure_rate` (pre-Phase-8 follow-up): the API response carried
    /// a key of that name holding a 0..1 *fraction*, so one name meant two units. The API key
    /// keeps its fraction value and its name for compatibility; this field now says which of
    /// the two it is.
    ///
    /// Numerator and denominator count only `PASSED` / `FAILED` / `TIMEOUT` rows.
    /// `None` when nothing completed.
    pub completed_failure_rate_percent: Option<f64>,
    /// Executor-failure count (Q2). A count, not a rate.
    pub infrastructure_errors: usize,
}

/// How one requirement's coverage and execution history stand.
#[derive(Debug, Clone, Serialize)]
pub struct RequirementHistory {
    pub requirement_key: String,
    pub requirement_text: Option<String>,
    /// Test cases linked by a `validates` edge.
    pub validating_test_cases: usize,
    /// Of those, how many have recorded executions.
    pub with_execution_history: usize,
    pub executions: usize,
    pub failures: usize,
    /// **All-recorded-outcome rate** (B3, Q1). Unchanged from before Phase 7.3.
    pub failure_rate: Option<f64>,
    /// **Completed-execution failure rate, as a PERCENTAGE (0..100).**
    ///
    /// Renamed from `completed_failure_rate` (pre-Phase-8 follow-up): the API response carried
    /// a key of that name holding a 0..1 *fraction*, so one name meant two units. The API key
    /// keeps its fraction value and its name for compatibility; this field now says which of
    /// the two it is.
    /// Numerator and denominator count only `PASSED` / `FAILED` / `TIMEOUT`,
    /// so a case whose step was never executed reports `None` rather than a diluted figure.
    pub completed_failure_rate_percent: Option<f64>,
    /// Executor-failure count (Q2). A count, not a rate.
    pub infrastructure_errors: usize,
    pub distinct_failure_fingerprints: usize,
    pub window: Window,
    pub signals: Vec<Signal>,
}

/// One defect that has recurred, with what it touched.
#[derive(Debug, Clone, Serialize)]
pub struct RecurringDefect {
    pub bug_draft_id: String,
    pub failure_cluster_id: String,
    pub title: String,
    pub review_state: String,
    pub severity: Option<String>,
    /// From the cluster, not the draft: this is the recurrence evidence.
    pub occurrences: i64,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    pub related_test_cases: Vec<String>,
    pub related_modules: Vec<String>,
}

// ------------------------------------------------------------- service

/// Read-only retrieval over stored history. Holds a `Store` clone and never writes.
#[derive(Clone)]
pub struct QaMemoryReader {
    pub(crate) store: Store,
}

impl QaMemoryReader {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Every stored telemetry row for a project, newest first, bounded.
    ///
    /// Retrieval here is deliberately one bounded read per question rather than a
    /// pile of joins: the master plan's Phase 3 test list requires correct answers
    /// on "large history", and a single bounded scan is what keeps those answers
    /// consistent with each other. Phase 6 records the query cost.
    ///
    /// ## Why this returns a `Result`
    ///
    /// It used to return `Vec<ExecutionTelemetryRow>` and swallow the read with
    /// `unwrap_or_default()`. A closed or failing store therefore produced an **empty** vector,
    /// which every caller then read as "this project has no execution history" — so a storage
    /// fault and a genuinely empty project became indistinguishable, and a project with history
    /// was reported as `NO_EXECUTIONS`.
    ///
    /// The distinction this return type now forces:
    ///
    /// * `Ok(vec![])` — the store answered, and there is genuinely no history. A real answer, and
    ///   `NO_EXECUTIONS` is the correct thing to report for it.
    /// * `Err(e)` — the store could not answer. No caller may turn this into an empty history.
    ///
    /// An empty `Ok` is deliberately still allowed to produce `NO_EXECUTIONS`; that behaviour is
    /// correct and unchanged.
    pub(crate) async fn telemetry_for(
        &self,
        project_id: &str,
    ) -> anyhow::Result<Vec<ExecutionTelemetryRow>> {
        self.store
            .list_execution_telemetry(project_id, TELEMETRY_READ_MAX)
            .await
    }

    pub(crate) fn has_history(rows: &[ExecutionTelemetryRow]) -> bool {
        !rows.is_empty()
    }

    pub(crate) fn window(rows: &[ExecutionTelemetryRow]) -> Window {
        let mut from: Option<DateTime<Utc>> = None;
        let mut to: Option<DateTime<Utc>> = None;
        for r in rows {
            from = Some(match from {
                Some(f) if f <= r.created_at => f,
                _ => r.created_at,
            });
            to = Some(match to {
                Some(t) if t >= r.created_at => t,
                _ => r.created_at,
            });
        }
        Window {
            from,
            to,
            executions_considered: rows.len(),
        }
    }

    /// A **completed** execution that failed. `TIMEOUT` counts: the step ran and did not
    /// succeed, which is what a failure rate is about.
    pub(crate) fn is_failure(row: &ExecutionTelemetryRow) -> bool {
        matches!(row.status.as_str(), "FAILED" | "TIMEOUT")
    }

    /// A **completed** execution that passed. Together with [`Self::is_failure`] this is the
    /// set of rows that qualify as evidence about a test's reliability; every other row is a
    /// record that something did not run, and is reported separately rather than counted here.
    pub(crate) fn is_pass(row: &ExecutionTelemetryRow) -> bool {
        row.status == "PASSED"
    }

    /// **B3 (Phase 7.3): the completed-execution failure rate.** `failed / (passed + failed) * 100`.
    ///
    /// This is the *other* rate from [`Self::rate`], and the two answer different questions:
    ///
    /// | helper | numerator | denominator | question it answers |
    /// |---|---|---|---|
    /// | `rate(f, n)` | `f` failures | `n` **all recorded rows** | "how many recorded attempts failed?" |
    /// | `completed_failure_rate(rows)` | failures | **completed pass/fail rows only** | "when this actually runs, how often does it fail?" |
    ///
    /// `None` when nothing completed -- a set of only blocked/skipped/cancelled rows has no
    /// completed execution to have failed, so there is no rate to report. Never `0.0`: a
    /// fabricated zero is a claim, and this codebase refuses those elsewhere.
    ///
    /// Q2 (Phase 7.3): `INFRA_ERROR` is **not** a completed pass/fail outcome. It is neither a
    /// pass nor a product failure -- it records that the executor itself broke -- so it is
    /// excluded from both halves of this rate. It is counted and reported separately by
    /// [`Self::infra_errors`] and is never silently turned into a pass.
    pub(crate) fn completed_failure_rate(rows: &[&ExecutionTelemetryRow]) -> Option<f64> {
        let completed = rows
            .iter()
            .filter(|r| Self::is_pass(r) || Self::is_failure(r))
            .count();
        let failures = rows.iter().filter(|r| Self::is_failure(r)).count();
        Self::rate(failures, completed)
    }

    /// **The completed-execution failure rate as a 0..1 fraction** — the *same* measurement as
    /// [`Self::completed_failure_rate`], without the x100 percentage scaling.
    ///
    /// **Phase 7.4 (Decision 1).** The risk model consumes factors in **0..1**
    /// (`Factor.contribution` multiplies a weight by the value and the result feeds a 0..1 score),
    /// while `completed_failure_rate` returns 0..100 because it shares `rate()` with the
    /// all-recorded metric. Two functions rather than a magic division at each call site, so the
    /// scaling lives in exactly one place and cannot be forgotten.
    ///
    /// This is the **only** shape the risk `failure_rate` factor may now use: it excludes every
    /// outcome that is not a genuine pass or failure, so a blocked step cannot dilute the factor
    /// and an `INFRA_ERROR` cannot inflate it. `None` when nothing completed — a caller must
    /// represent that as unmeasured, never as a measured `0.0`.
    pub(crate) fn completed_failure_rate_fraction(rows: &[&ExecutionTelemetryRow]) -> Option<f64> {
        let completed = Self::completed_count(rows);
        let failures = rows.iter().filter(|r| Self::is_failure(r)).count();
        Self::rate(failures, completed).map(|pct| pct / 100.0)
    }

    /// **How many of these rows are a completed pass or failure** (`PASSED` / `FAILED` /
    /// `TIMEOUT`).
    ///
    /// The denominator of the completed-execution rate, exposed separately because a risk
    /// `Factor` must state how many observations its value was taken over. Exposing it here means
    /// the eligibility rule is written once and every consumer quotes the same count.
    pub(crate) fn completed_count(rows: &[&ExecutionTelemetryRow]) -> usize {
        rows.iter()
            .filter(|r| Self::is_pass(r) || Self::is_failure(r))
            .count()
    }

    /// **B3 (Phase 7.3): how many recorded outcomes were infrastructure errors.**
    ///
    /// Q2: `INFRA_ERROR` means the executor failed (no browser, a dead subprocess), which is
    /// neither a test passing nor a test failing. Counting it as a failure inflates a project's
    /// risk for conditions the application under test never caused; dropping it entirely would
    /// erase it from observability. Both are wrong, so it is counted here and reported next to
    /// the rate rather than inside it.
    ///
    /// This is Q2's "track them separately", and it is a **count, not a rate**: it has no
    /// denominator, so it cannot be mistaken for a failure share.
    pub(crate) fn infra_errors(rows: &[&ExecutionTelemetryRow]) -> usize {
        rows.iter().filter(|r| r.status == "INFRA_ERROR").count()
    }

    /// A rate, or `None` when there is nothing to take a rate of.
    ///
    /// This is the **all-recorded-outcome** rate. It divides by every stored row, so a blocked or
    /// unexecuted step is in the denominator. That is a legitimate question ("what share of
    /// recorded attempts failed?") and it is what `failure_rate` on the history views means --
    /// which is why those fields are *not* silently redefined here. For the rate over completed
    /// executions only, see [`Self::completed_failure_rate`].
    pub(crate) fn rate(part: usize, whole: usize) -> Option<f64> {
        if whole == 0 {
            None
        } else {
            Some((part as f64 / whole as f64) * 100.0)
        }
    }

    // -------------------------------------------------- test stability

    /// Test stability for one test case, keyed on the **stable case id**.
    ///
    /// Returns `UnknownSubject` when the project has executions but this id is not
    /// one of them, and `NoMatchingHistory` is never used here — the two would say
    /// the same thing. Before Phase 2 most history had no case id at all, so those
    /// rows simply do not appear; that is stated rather than papered over.
    pub async fn test_stability(
        &self,
        project_id: &str,
        test_case_id: &str,
    ) -> anyhow::Result<Retrieved<TestStabilityView>> {
        let all = self.telemetry_for(project_id).await?;
        if !Self::has_history(&all) {
            return Ok(Retrieved::empty(project_id, InsufficientData::NoExecutions));
        }

        let mine: Vec<&ExecutionTelemetryRow> = all
            .iter()
            .filter(|r| r.test_case_id.map(|c| c.to_string()).as_deref() == Some(test_case_id))
            .collect();
        if mine.is_empty() {
            return Ok(Retrieved::empty(
                project_id,
                InsufficientData::UnknownSubject,
            ));
        }

        let passed = mine.iter().filter(|r| Self::is_pass(r)).count();
        let failed = mine.iter().filter(|r| Self::is_failure(r)).count();
        let other = mine.len() - passed - failed;
        // Phase 7.1 (P7-D2): the outcomes that qualify as a *completed* execution, and therefore
        // the only ones a failure rate may be computed over. Computed once here because this view
        // reports the same rate twice -- as the `failure_rate` field and as a `failure_rate`
        // signal -- and when they were computed separately they disagreed: the field used the
        // eligible count while the signal still divided by every recorded row, so one struct
        // could say 100% in one place and 50% in the other for the same case.
        let completed = passed + failed;

        let fingerprints: BTreeSet<String> = mine
            .iter()
            .filter_map(|r| r.failure_fingerprint.clone())
            .filter(|f| !f.is_empty())
            .collect();
        let latest = mine
            .iter()
            .rev()
            .find(|r| r.failure_fingerprint.is_some())
            .and_then(|r| r.failure_fingerprint.clone());

        // Flakiness only from `adaptive_intelligence`'s own evaluation, and only
        // once there are enough samples for it to mean anything.
        // Phase 3 delegates to the existing `adaptive_intelligence::evaluate_flakiness`
        // rather than inventing a second flakiness formula. It takes task outcomes,
        // so the row statuses are mapped onto the real `TaskStatus` vocabulary.
        let flakiness_score = if mine.len() >= 2 {
            let outcomes: Vec<TaskStatus> = mine
                .iter()
                .map(|r| match r.status.as_str() {
                    "PASSED" => TaskStatus::Passed,
                    "FAILED" => TaskStatus::Failed,
                    "TIMEOUT" => TaskStatus::Timeout,
                    "SKIPPED" => TaskStatus::Skipped,
                    "BLOCKED" => TaskStatus::Blocked,
                    "CANCELLED" => TaskStatus::Cancelled,
                    _ => TaskStatus::InfraError,
                })
                .collect();
            let analysis = crate::adaptive_intelligence::evaluate_flakiness(
                test_case_id,
                &mine
                    .first()
                    .map(|r| r.environment.clone())
                    .unwrap_or_default(),
                &outcomes,
            );
            Some(analysis.flakiness_score.clamp(0.0, 1.0))
        } else {
            None
        };

        let runs: Vec<Uuid> = mine.iter().map(|r| r.run_id).collect();
        let owned: Vec<ExecutionTelemetryRow> = mine.iter().map(|r| (*r).clone()).collect();
        let window = Self::window(&owned);
        let test_name = mine.first().map(|r| r.task_name.clone());

        // One rate, two consumers. `None` when nothing completed, in which case neither the signal
        // nor the field claims a rate -- an unmeasured outcome stays unmeasured.
        let rate = Self::rate(failed, completed);
        let mut signals = Vec::new();
        if let Some(f) = rate {
            signals.push(Signal {
                kind: "failure_rate".into(),
                // The numerator and denominator are the completed executions, matching the
                // `failure_rate` field above. The excluded rows are named rather than dropped, so
                // a reader can still see that other outcomes exist.
                statement: format!(
                    "{} of {completed} completed executions failed ({f:.1}%)",
                    failed
                ),
                evidence_runs: runs.clone(),
                confidence: if completed >= 5 { 0.9 } else { 0.5 },
                basis: format!(
                    "{completed} completed execution_telemetry row(s) (PASSED/FAILED/TIMEOUT) out of \
                     {} recorded for this test case; the {} other outcome(s) were never \
                     executions and are excluded from the rate",
                    mine.len(),
                    other
                ),
            });
        }
        if fingerprints.len() > 1 {
            signals.push(Signal {
                kind: "distinct_failure_modes".into(),
                statement: format!(
                    "{} distinct failure fingerprints recorded, so these are not one repeated fault",
                    fingerprints.len()
                ),
                evidence_runs: runs.clone(),
                confidence: 0.85,
                basis: "distinct non-empty failure_fingerprint values".into(),
            });
        }

        Ok(Retrieved::found(
            project_id,
            TestStabilityView {
                test_case_id: test_case_id.to_string(),
                test_name,
                module_ref: None, // filled by `module_risk_history`'s caller if needed
                executions: mine.len(),
                passed,
                failed,
                other_outcomes: other,
                // Phase 7.1 (P7-D2). The denominator used to be `mine.len()` -- *every*
                // recorded outcome, including ones where the step never ran at all. A step
                // blocked by a failed prerequisite produces one `BLOCKED` row, so it read as
                // `failed = 0` over `1`: a measured **0% failure rate**, for a case that had
                // never executed. The denominator is now the outcomes that qualify as a
                // completed execution -- the same rows `is_pass` and `is_failure` count -- so a
                // non-execution outcome can neither create a rate nor dilute one. With only a
                // blocked row the denominator is zero and `rate` answers `None`.
                //
                // What deliberately did NOT change: a single genuine `PASSED` execution still
                // reports `Some(0.0)`. That is arithmetically true and is an existing tested
                // contract ("a 0% failure rate over one execution is arithmetically true",
                // `qa_memory_retrieval_tests`), so no minimum-sample rule was invented here.
                //
                // The same `rate` the `failure_rate` signal above was built from, so the two can
                // never disagree.
                failure_rate: rate,
                flakiness_score,
                first_seen: window.from,
                last_seen: window.to,
                distinct_failure_fingerprints: fingerprints.len(),
                latest_failure_fingerprint: latest,
                window,
                signals,
            },
        ))
    }

    // -------------------------------------------------- module risk history

    /// Risk history for one module, assembled from the Phase 2 `covers` edges.
    ///
    /// A module with no linked cases, or with linked cases that have never run,
    /// returns `NoMatchingHistory` — not an all-clear.
    pub async fn module_risk_history(
        &self,
        project_id: &str,
        module_ref: &str,
    ) -> anyhow::Result<Retrieved<ModuleRiskHistory>> {
        let all = self.telemetry_for(project_id).await?;
        if !Self::has_history(&all) {
            return Ok(Retrieved::empty(project_id, InsufficientData::NoExecutions));
        }

        // module vertex -> its `covers` edges -> the case vertices on the other end.
        let (nodes, relations) = match (
            self.store.list_quality_nodes(project_id).await,
            self.store
                .list_quality_relations_for_project(project_id)
                .await,
        ) {
            (Ok(n), Ok(r)) => (n, r),
            _ => {
                return Ok(Retrieved::empty(
                    project_id,
                    InsufficientData::NoMatchingHistory,
                ))
            }
        };

        let Some(module_node) = nodes
            .iter()
            .find(|n| n.node_type == "module" && n.external_ref.as_deref() == Some(module_ref))
        else {
            return Ok(Retrieved::empty(
                project_id,
                InsufficientData::UnknownSubject,
            ));
        };

        let linked_cases: BTreeSet<Uuid> = relations
            .iter()
            .filter(|r| r.relation_type == "covers" && r.to_node_id == module_node.id)
            .map(|r| r.from_node_id)
            .collect();

        // The case vertices carry `external_ref` = the case id used on `covers`.
        let case_ref_by_node: HashMap<Uuid, String> = nodes
            .iter()
            .filter(|n| n.node_type == "test")
            .filter_map(|n| n.external_ref.clone().map(|e| (n.id, e)))
            .collect();
        let linked_case_refs: BTreeSet<String> = linked_cases
            .iter()
            .filter_map(|id| case_ref_by_node.get(id).cloned())
            .collect();

        let mine: Vec<&ExecutionTelemetryRow> = all
            .iter()
            .filter(|r| {
                r.test_case_id
                    .map(|c| c.to_string())
                    .map(|c| linked_case_refs.contains(&c))
                    .unwrap_or(false)
            })
            .collect();
        if mine.is_empty() {
            return Ok(Retrieved::empty(
                project_id,
                InsufficientData::NoMatchingHistory,
            ));
        }

        let failed_rows: Vec<&&ExecutionTelemetryRow> =
            mine.iter().filter(|r| Self::is_failure(r)).collect();
        let failures = failed_rows.len();

        // per-case rollup
        let mut per_case: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for r in &mine {
            let Some(case) = r.test_case_id.map(|c| c.to_string()) else {
                continue;
            };
            let e = per_case.entry(case.to_string()).or_insert((0, 0));
            e.0 += 1;
            if Self::is_failure(r) {
                e.1 += 1;
            }
        }
        let mut affected_tests: Vec<AffectedTest> = per_case
            .iter()
            .filter(|(_, (_, f))| *f > 0)
            .map(|(id, (n, f))| AffectedTest {
                test_case_id: id.clone(),
                executions: *n,
                failures: *f,
            })
            .collect();
        affected_tests.sort_by(|a, b| {
            b.failures
                .cmp(&a.failures)
                .then(a.test_case_id.cmp(&b.test_case_id))
        });
        affected_tests.truncate(MAX_AFFECTED_TESTS);

        // recurrence: a fingerprint seen more than once here
        let mut fp_counts: BTreeMap<&str, (usize, Vec<&ExecutionTelemetryRow>)> = BTreeMap::new();
        for r in &failed_rows {
            if let Some(fp) = r.failure_fingerprint.as_deref().filter(|f| !f.is_empty()) {
                fp_counts.entry(fp).or_insert((0, Vec::new())).1.push(r);
            }
        }
        let mut recurring: Vec<RecurringFailure> = fp_counts
            .iter()
            .filter(|(_, (_, rows))| rows.len() > 1)
            .map(|(fp, (_, rows))| {
                let first = rows.iter().map(|r| r.created_at).min();
                let last = rows.iter().map(|r| r.created_at).max();
                let affected = rows
                    .iter()
                    .map(|r| r.test_case_id.map(|c| c.to_string()))
                    .collect::<BTreeSet<_>>()
                    .len();
                RecurringFailure {
                    fingerprint: (*fp).to_string(),
                    occurrences: rows.len(),
                    first_seen: first,
                    last_seen: last,
                    failure_category: rows.first().and_then(|r| r.failure_category.clone()),
                    affected_test_count: affected,
                }
            })
            .collect();
        recurring.sort_by(|a, b| {
            b.occurrences
                .cmp(&a.occurrences)
                .then(a.fingerprint.cmp(&b.fingerprint))
        });

        // which releases the affected executions belong to
        let run_ids: BTreeSet<Uuid> = mine.iter().map(|r| r.run_id).collect();
        let mut affected_releases: Vec<String> = Vec::new();
        // Both reads used to be `if let Ok(..)`, so a storage fault produced an empty
        // `affected_releases` -- "this module's failures touch no release" -- which is a claim about
        // the data that the store never answered. Both propagate now.
        for rel in self.store.list_releases(project_id).await? {
            let runs = self.store.runs_for_release(rel.id).await?;
            if runs.iter().any(|r| run_ids.contains(r)) {
                if let Some(label) = rel.label.clone().or(rel.version.clone()) {
                    affected_releases.push(label);
                }
            }
        }

        let owned: Vec<ExecutionTelemetryRow> = mine.iter().map(|r| (*r).clone()).collect();
        let window = Self::window(&owned);
        let mut signals = Vec::new();
        if let Some(f) = Self::rate(failures, mine.len()) {
            signals.push(Signal {
                kind: "module_failure_rate".into(),
                statement: format!(
                    "{} of {} recorded executions across this module's test cases failed ({f:.1}%)",
                    failures,
                    mine.len()
                ),
                evidence_runs: mine.iter().map(|r| r.run_id).collect(),
                confidence: if mine.len() >= 5 { 0.85 } else { 0.5 },
                basis: format!(
                    "{} linked test case(s), {} execution_telemetry rows",
                    linked_case_refs.len(),
                    mine.len()
                ),
            });
        }
        if !recurring.is_empty() {
            signals.push(Signal {
                kind: "recurring_failure".into(),
                statement: format!(
                    "{} failure fingerprint(s) occurred more than once in this module",
                    recurring.len()
                ),
                evidence_runs: Vec::new(),
                confidence: 0.9,
                basis: "grouped by non-empty failure_fingerprint across the module's failures"
                    .into(),
            });
        }
        if per_case.len() < linked_case_refs.len() {
            signals.push(Signal {
                kind: "partial_coverage".into(),
                statement: format!(
                    "{} of this module's {} linked test cases have no recorded execution",
                    linked_case_refs.len() - per_case.len(),
                    linked_case_refs.len()
                ),
                evidence_runs: Vec::new(),
                confidence: 1.0,
                basis: "linked `covers` edges vs test cases found in execution_telemetry".into(),
            });
        }
        signals.truncate(MAX_SIGNALS);

        Ok(Retrieved::found(
            project_id,
            ModuleRiskHistory {
                module_ref: module_ref.to_string(),
                module_name: module_node.name.clone(),
                linked_test_cases: linked_case_refs.len(),
                test_cases_with_history: per_case.len(),
                executions: mine.len(),
                failures,
                failure_rate: Self::rate(failures, mine.len()),
                // B3/Q1: the second, explicitly named rate. A caller reads either one knowingly.
                completed_failure_rate_percent: Self::completed_failure_rate(&mine),
                // Q2: counted and reported, never inside either rate.
                infrastructure_errors: Self::infra_errors(&mine),
                affected_tests,
                recurring_failures: recurring,
                affected_releases,
                window,
                signals,
            },
        ))
    }

    // -------------------------------------------------- release history

    /// Execution history for one release, plus a same-project comparison against its
    /// other releases that have runs.
    ///
    /// Comparison is only ever made against releases **that have runs**; a release
    /// with none is omitted rather than counted as a zero-failure release, which
    /// would read as "that release was clean".
    pub async fn release_history(
        &self,
        project_id: &str,
        release_id: Uuid,
    ) -> anyhow::Result<Retrieved<ReleaseHistory>> {
        let all = self.telemetry_for(project_id).await?;
        let releases = self.store.list_releases(project_id).await?;

        let Some(target) = releases.iter().find(|r| r.id == release_id) else {
            return Ok(Retrieved::empty(
                project_id,
                InsufficientData::UnknownSubject,
            ));
        };

        // A failed `runs_for_release` used to be reported as `NO_MATCHING_HISTORY`, i.e. "this release
        // has never been run" -- indistinguishable from a genuine empty. Only the genuine empty is
        // still that answer; a failed read propagates.
        let target_runs = self.store.runs_for_release(release_id).await?;
        if target_runs.is_empty() {
            return Ok(Retrieved::empty(
                project_id,
                InsufficientData::NoMatchingHistory,
            ));
        }
        let target_run_ids: BTreeSet<Uuid> = target_runs.iter().copied().collect();
        let mine: Vec<&ExecutionTelemetryRow> = all
            .iter()
            .filter(|r| target_run_ids.contains(&r.run_id))
            .collect();

        if mine.is_empty() {
            // A release that exists with no executions is a real, reportable state:
            // it has been recorded but not yet exercised.
            return Ok(Retrieved::empty(
                project_id,
                InsufficientData::NoMatchingHistory,
            ));
        }

        let failures = mine.iter().filter(|r| Self::is_failure(r)).count();
        let fingerprints: BTreeSet<String> = mine
            .iter()
            .filter_map(|r| r.failure_fingerprint.clone())
            .filter(|f| !f.is_empty())
            .collect();

        let mut compared_against = Vec::new();
        for other in releases.iter().filter(|r| r.id != release_id) {
            // A failure here used to `continue`, silently dropping that release from the
            // comparison -- which biases the trend this view exists to produce. It propagates.
            let runs = self.store.runs_for_release(other.id).await?;
            let ids: BTreeSet<Uuid> = runs.iter().copied().collect();
            let rows: Vec<&ExecutionTelemetryRow> =
                all.iter().filter(|r| ids.contains(&r.run_id)).collect();
            if rows.is_empty() {
                continue; // no runs recorded against it: omitted, not counted as clean
            }
            let f = rows.iter().filter(|r| Self::is_failure(r)).count();
            compared_against.push(ReleaseComparison {
                release_id: other.id.to_string(),
                label: other.label.clone().or(other.version.clone()),
                runs: runs.len(),
                executions: rows.len(),
                failures: f,
                failure_rate: Self::rate(f, rows.len()),
                // B3/Q1: both rates, so a comparison cannot silently mix the two definitions.
                completed_failure_rate_percent: Self::completed_failure_rate(&rows),
                infrastructure_errors: Self::infra_errors(&rows),
            });
        }
        compared_against.sort_by(|a, b| b.label.cmp(&a.label));
        compared_against.truncate(MAX_RELEASES_COMPARED);

        let owned: Vec<ExecutionTelemetryRow> = mine.iter().map(|r| (*r).clone()).collect();
        let window = Self::window(&owned);
        let mut signals = Vec::new();
        if let Some(f) = Self::rate(failures, mine.len()) {
            signals.push(Signal {
                kind: "release_failure_rate".into(),
                statement: format!(
                    "{} of {} recorded executions in this release failed ({f:.1}%)",
                    failures,
                    mine.len()
                ),
                evidence_runs: mine.iter().map(|r| r.run_id).collect(),
                confidence: if mine.len() >= 5 { 0.85 } else { 0.5 },
                basis: format!("{} run(s) associated with this release", target_runs.len()),
            });
        }
        if compared_against.is_empty() {
            signals.push(Signal {
                kind: "no_comparable_release".into(),
                statement:
                    "no other release of this project has recorded executions, so no comparison is possible"
                        .into(),
                evidence_runs: Vec::new(),
                confidence: 1.0,
                basis: "every other release had zero runs with executions".into(),
            });
        }
        signals.truncate(MAX_SIGNALS);

        Ok(Retrieved::found(
            project_id,
            ReleaseHistory {
                release_id: release_id.to_string(),
                label: target.label.clone(),
                version: target.version.clone(),
                runs: target_runs.len(),
                executions: mine.len(),
                failures,
                failure_rate: Self::rate(failures, mine.len()),
                // B3/Q1 + Q2: the completed-execution rate and the executor-error count sit
                // beside the all-recorded rate rather than replacing it.
                completed_failure_rate_percent: Self::completed_failure_rate(&mine),
                infrastructure_errors: Self::infra_errors(&mine),
                distinct_fingerprints: fingerprints.len(),
                compared_against,
                window,
                signals,
            },
        ))
    }

    // -------------------------------------------------- requirement history

    /// Execution history for one requirement, through its `validates` edges.
    pub async fn requirement_history(
        &self,
        project_id: &str,
        requirement_key: &str,
    ) -> anyhow::Result<Retrieved<RequirementHistory>> {
        let all = self.telemetry_for(project_id).await?;
        if !Self::has_history(&all) {
            return Ok(Retrieved::empty(project_id, InsufficientData::NoExecutions));
        }

        // A failed read used to answer `NO_MATCHING_HISTORY`, i.e. "this requirement was never
        // exercised" -- which is exactly what a genuine empty says. A failure now propagates; the
        // genuine empty falls through to the `find` below and is answered there, as it always was.
        let requirements = self.store.list_requirements(project_id, &[]).await?;
        let Some(req) = requirements.iter().find(|r| r.key == requirement_key) else {
            return Ok(Retrieved::empty(
                project_id,
                InsufficientData::UnknownSubject,
            ));
        };

        // The requirement's own node on the graph is linked to its cases by `validates`.
        let (nodes, relations) = match (
            self.store.list_quality_nodes(project_id).await,
            self.store
                .list_quality_relations_for_project(project_id)
                .await,
        ) {
            (Ok(n), Ok(r)) => (n, r),
            _ => {
                return Ok(Retrieved::empty(
                    project_id,
                    InsufficientData::NoMatchingHistory,
                ))
            }
        };

        let req_node = nodes.iter().find(|n| {
            n.node_type != "test"
                && n.node_type != "module"
                && n.external_ref.as_deref() == Some(requirement_key)
        });
        let case_refs: BTreeSet<String> = match req_node {
            Some(node) => relations
                .iter()
                .filter(|r| r.relation_type == "validates" && r.to_node_id == node.id)
                .filter_map(|r| {
                    nodes
                        .iter()
                        .find(|n| n.id == r.from_node_id)
                        .and_then(|n| n.external_ref.clone())
                })
                .collect(),
            None => BTreeSet::new(),
        };

        let mine: Vec<&ExecutionTelemetryRow> = all
            .iter()
            .filter(|r| {
                r.test_case_id
                    .map(|c| c.to_string())
                    .map(|c| case_refs.contains(&c))
                    .unwrap_or(false)
            })
            .collect();

        if mine.is_empty() {
            return Ok(Retrieved::empty(
                project_id,
                InsufficientData::NoMatchingHistory,
            ));
        }

        let failures = mine.iter().filter(|r| Self::is_failure(r)).count();
        let fingerprints: BTreeSet<String> = mine
            .iter()
            .filter_map(|r| r.failure_fingerprint.clone())
            .filter(|f| !f.is_empty())
            .collect();
        let cases_with_history: BTreeSet<String> = mine
            .iter()
            .filter_map(|r| r.test_case_id)
            .map(|c| c.to_string())
            .collect();
        let owned: Vec<ExecutionTelemetryRow> = mine.iter().map(|r| (*r).clone()).collect();
        let window = Self::window(&owned);

        let mut signals = Vec::new();
        if let Some(f) = Self::rate(failures, mine.len()) {
            signals.push(Signal {
                kind: "requirement_failure_rate".into(),
                statement: format!(
                    "{} of {} recorded executions of tests validating this requirement failed ({f:.1}%)",
                    failures,
                    mine.len()
                ),
                evidence_runs: mine.iter().map(|r| r.run_id).collect(),
                confidence: if mine.len() >= 5 { 0.85 } else { 0.5 },
                basis: format!(
                    "{} `validates` edge(s) to test cases, {} execution_telemetry rows",
                    case_refs.len(),
                    mine.len()
                ),
            });
        }
        if case_refs.len() > cases_with_history.len() {
            signals.push(Signal {
                kind: "uncovered_in_practice".into(),
                statement: format!(
                    "{} of the {} test cases linked to this requirement have never run",
                    case_refs.len() - cases_with_history.len(),
                    case_refs.len()
                ),
                evidence_runs: Vec::new(),
                confidence: 1.0,
                basis: "`validates` edges vs test cases present in execution_telemetry".into(),
            });
        }
        signals.truncate(MAX_SIGNALS);

        Ok(Retrieved::found(
            project_id,
            RequirementHistory {
                requirement_key: requirement_key.to_string(),
                requirement_text: Some(req.text.clone()),
                validating_test_cases: case_refs.len(),
                with_execution_history: cases_with_history.len(),
                executions: mine.len(),
                failures,
                failure_rate: Self::rate(failures, mine.len()),
                // B3/Q1 + Q2: same pair, for the same reason.
                completed_failure_rate_percent: Self::completed_failure_rate(&mine),
                infrastructure_errors: Self::infra_errors(&mine),
                distinct_failure_fingerprints: fingerprints.len(),
                window,
                signals,
            },
        ))
    }

    // -------------------------------------------------- bug history

    /// Recurring defects of a project, with the tests and modules each touched.
    ///
    /// Recurrence is read from the **cluster** (`occurrence_count`), not from the
    /// draft, because the draft is a review artefact and the count is the evidence.
    pub async fn bug_history(&self, project_id: &str) -> anyhow::Result<Vec<RecurringDefect>> {
        // All three reads below used to fail silently: a failed `list_bug_drafts` returned an
        // empty list, and the node and relation reads used `unwrap_or_default()`. A storage fault
        // therefore produced "this project has no recurring defects" - a confident claim about
        // the project that the store never answered. This function already returned `Result` for
        // its telemetry read, so the error channel existed and these three were not using it.
        let drafts = self.store.list_bug_drafts(Some(project_id)).await?;

        let nodes = self.store.list_quality_nodes(project_id).await?;
        let relations = self
            .store
            .list_quality_relations_for_project(project_id)
            .await?;
        let telemetry = self.telemetry_for(project_id).await?;

        let mut out = Vec::new();
        for draft in drafts {
            // Phase 3 bug, caught by the Phase 4 planning tests: this looked the cluster
            // up with `get_failure_cluster_by_signature(.., failure_cluster_id)`,
            // passing an id where the query wanted a signature. It never matched,
            // so `bug_history` returned an empty list for *every* project — which
            // reads as "no recurring defects anywhere", the most dangerous possible
            // wrong answer. The project-scoped by-id lookup is what was meant.
            let cluster = match self
                .store
                .get_failure_cluster_by_id_scoped(project_id, draft.failure_cluster_id)
                .await
            {
                Ok(Some(c)) => c,
                _ => continue,
            };

            // Phase 3 bug #2, caught by the Phase 6 end-to-end validation: this matched
            // telemetry to the cluster by comparing `cluster.signature` against
            // `execution_telemetry.failure_fingerprint`. Those are **different
            // identifier schemes** and can never be equal:
            //
            //   failure_clusters.signature            = normalize_signature(task.reason)
            //                                                  -> human text
            //   execution_telemetry.failure_fingerprint = compute_telemetry_fingerprint
            //                                                  -> 64-char SHA-256 hex
            //
            // So the correlation silently produced nothing, and every defect reported
            // an empty `related_test_cases` — i.e. "this defect touched nothing",
            // which is the most misleading answer available.
            //
            // Phase 6 fixed it at the source: migration 0030 added
            // `failure_clusters.failure_fingerprint`, written by `record_failure`
            // with the same computation telemetry uses, so the two rows now join by
            // equality.
            //
            // The `fails_with` graph is the fallback for clusters recorded before
            // that migration: it is keyed by the same fingerprint on the node side,
            // so a cluster node found by fingerprint still yields its test nodes
            // without inventing a link.
            let fingerprint: Option<&str> = cluster
                .failure_fingerprint
                .as_deref()
                .filter(|f| !f.is_empty());

            let related: Vec<&ExecutionTelemetryRow> = match fingerprint {
                Some(fp) => telemetry
                    .iter()
                    .filter(|r| r.failure_fingerprint.as_deref() == Some(fp))
                    .collect(),
                None => Vec::new(),
            };
            let related_cases: BTreeSet<String> = related
                .iter()
                .filter_map(|r| r.test_case_id)
                .map(|c| c.to_string())
                .collect();

            // Test names behind those executions, and — for a cluster too old to
            // carry a fingerprint — the names the graph recorded anyway.
            let related_task_names: BTreeSet<String> =
                related.iter().map(|r| r.task_name.clone()).collect();

            let related_tests: Vec<String> = if !related_cases.is_empty() {
                related_cases.iter().cloned().collect()
            } else {
                let cluster_node = nodes.iter().find(|n| {
                    n.node_type == "failure_cluster"
                        && fingerprint
                            .map(|fp| n.external_ref.as_deref() == Some(fp))
                            .unwrap_or(false)
                });
                match cluster_node {
                    Some(cn) => relations
                        .iter()
                        .filter(|r| r.relation_type == "fails_with" && r.to_node_id == cn.id)
                        .filter_map(|r| nodes.iter().find(|n| n.id == r.from_node_id))
                        .filter_map(|n| n.external_ref.clone())
                        .collect(),
                    None => Vec::new(),
                }
            };

            // Modules those cases cover.
            let case_node_ids: BTreeSet<Uuid> = nodes
                .iter()
                .filter(|n| {
                    n.node_type == "test"
                        && n.external_ref
                            .as_deref()
                            .map(|e| related_cases.contains(e) || related_task_names.contains(e))
                            .unwrap_or(false)
                })
                .map(|n| n.id)
                .collect();
            let mut related_modules: BTreeSet<String> = BTreeSet::new();
            for rel in relations.iter().filter(|r| r.relation_type == "covers") {
                if case_node_ids.contains(&rel.from_node_id) {
                    if let Some(m) = nodes
                        .iter()
                        .find(|n| n.id == rel.to_node_id && n.node_type == "module")
                        .and_then(|n| n.external_ref.clone())
                    {
                        related_modules.insert(m);
                    }
                }
            }

            out.push(RecurringDefect {
                bug_draft_id: draft.id.to_string(),
                failure_cluster_id: draft.failure_cluster_id.to_string(),
                title: draft.title.clone(),
                review_state: draft.review_state.clone(),
                severity: draft.severity.clone(),
                occurrences: cluster.occurrence_count,
                first_seen: Some(cluster.first_seen_at),
                last_seen: Some(cluster.last_seen_at),
                related_test_cases: related_tests
                    .iter()
                    .take(MAX_AFFECTED_TESTS)
                    .cloned()
                    .collect(),
                related_modules: related_modules
                    .into_iter()
                    .take(MAX_AFFECTED_TESTS)
                    .collect(),
            });
        }

        // Most-recurring first, then newest.
        out.sort_by(|a, b| {
            b.occurrences
                .cmp(&a.occurrences)
                .then(b.last_seen.cmp(&a.last_seen))
        });
        out.truncate(MAX_SIGNALS);
        Ok(out)
    }

    /// The whole picture for one project, in a single call.
    ///
    /// This is a convenience over the individual reads above, not a fourth way of
    /// asking the same question: each field is exactly what the matching function
    /// returns, and each reports its own `insufficient_data` reason when it has
    /// nothing.
    pub async fn project_memory_summary(
        &self,
        project_id: &str,
    ) -> anyhow::Result<ProjectMemorySummary> {
        let telemetry = self.telemetry_for(project_id).await?;
        let releases = self.store.list_releases(project_id).await?;

        let module_refs: Vec<String> = self
            .store
            .list_quality_nodes(project_id)
            .await?
            .into_iter()
            .filter(|n| n.node_type == "module")
            .filter_map(|n| n.external_ref)
            .collect();

        Ok(ProjectMemorySummary {
            project_id: project_id.to_string(),
            has_history: !telemetry.is_empty(),
            executions: telemetry.len(),
            linked_test_cases: telemetry
                .iter()
                .map(|r| r.test_case_id.map(|c| c.to_string()))
                .collect::<BTreeSet<_>>()
                .len(),
            distinct_fingerprints: telemetry
                .iter()
                .filter_map(|r| r.failure_fingerprint.clone())
                .filter(|f| !f.is_empty())
                .collect::<BTreeSet<String>>()
                .len(),
            releases: releases.len(),
            modules: module_refs.len(),
            recurring_defects: self.bug_history(project_id).await?.len(),
            window: Self::window(&telemetry),
        })
    }
}

/// What a project actually has. Every field is a count of stored rows.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectMemorySummary {
    pub project_id: String,
    pub has_history: bool,
    pub executions: usize,
    /// Test cases with at least one *linked* execution. Lower than the number of
    /// distinct task names whenever history predates Phase 2.
    pub linked_test_cases: usize,
    pub distinct_fingerprints: usize,
    pub releases: usize,
    pub modules: usize,
    pub recurring_defects: usize,
    pub window: Window,
}

/// Releases of a project, for callers that need the ids `release_history` takes.
/// Thin on purpose: the query itself is `store::list_releases`.
pub async fn list_releases_for_retrieval(store: &Store, project_id: &str) -> Vec<ReleaseRow> {
    store.list_releases(project_id).await.unwrap_or_default()
}

// ============================================================================
// Phase 4 — history as structured planning signals
// ============================================================================

/// How far back a planning signal may look.
///
/// Master plan §6 requires that "old irrelevant history does not dominate current
/// signals". A failure from a year ago is not the same planning input as one from
/// last week, so the window is explicit rather than implied by whatever rows happen
/// to exist: **90 days**. Older history is still retrievable through
/// `module_risk_history` and friends; it simply does not drive planning on its own.
pub const PLANNING_SIGNAL_WINDOW_DAYS: i64 = 90;

/// A historical fact offered to the planning agent, with everything needed to check
/// it.
///
/// Every field is filled from stored rows. `basis` says which rows; `metric` gives
/// the measured number; `window` says over what period; `confidence` says how much
/// the sample supports it. Nothing here is an inference about *why* — there is no
/// reasoning text, by master plan §1.5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanningSignal {
    /// Machine-readable kind: `module_risk`, `recurring_defect`, `release_regression`,
    /// `unstable_test`, `requirement_gap`.
    pub kind: String,
    /// What the signal is about — a module ref, a release label, a case id, a
    /// requirement key.
    pub subject: String,
    /// The measured fact, with its numbers: `"7 of 20 executions failed (35.0%)"`.
    pub metric: String,
    /// The runs the metric was computed from.
    pub evidence_runs: Vec<Uuid>,
    /// 0..1, and always derived from the sample size, never asserted.
    pub confidence: f64,
    /// `"2026-09-01 .. 2026-10-01 (20 executions)"`.
    pub window: String,
    /// What the signal was computed from, in one clause.
    pub basis: String,
}

/// Confidence from a sample size: 1 execution is a hint, 20 is a measurement.
///
/// Not a statistical model — a stated, testable convention, so that a caller can
/// see exactly why one signal outweighs another. Capped at 0.9 because even a large
/// sample is history, not proof.
fn confidence_from_sample(n: usize) -> f64 {
    match n {
        0 => 0.0,
        1..=2 => 0.3,
        3..=9 => 0.6,
        _ => 0.9,
    }
}

impl QaMemoryReader {
    /// Historical signals for planning, newest-evidence-weighted.
    ///
    /// Returns an **empty vector** when the project has no history inside
    /// [`PLANNING_SIGNAL_WINDOW_DAYS`]. That is the important property: memory must
    /// not influence planning when there is no evidence to influence it, so an empty
    /// project produces no signals rather than neutral-sounding ones.
    pub async fn planning_signals(&self, project_id: &str) -> anyhow::Result<Vec<PlanningSignal>> {
        let all = self.telemetry_for(project_id).await?;
        if all.is_empty() {
            return Ok(Vec::new());
        }

        let cutoff = Utc::now() - Duration::days(PLANNING_SIGNAL_WINDOW_DAYS);
        let recent: Vec<&ExecutionTelemetryRow> =
            all.iter().filter(|r| r.created_at >= cutoff).collect();
        if recent.is_empty() {
            // History exists but none of it is recent enough to plan from. Saying
            // nothing is correct; re-surfacing year-old failures is not.
            return Ok(Vec::new());
        }

        let mut signals: Vec<PlanningSignal> = Vec::new();

        // ---- module risk -------------------------------------------------
        let nodes = self.store.list_quality_nodes(project_id).await?;
        let relations = self
            .store
            .list_quality_relations_for_project(project_id)
            .await?;
        let case_ref_by_node: HashMap<Uuid, String> = nodes
            .iter()
            .filter(|n| n.node_type == "test")
            .filter_map(|n| n.external_ref.clone().map(|e| (n.id, e)))
            .collect();

        for module in nodes.iter().filter(|n| n.node_type == "module") {
            let Some(ref_name) = module.external_ref.clone() else {
                continue;
            };
            let cases: BTreeSet<String> = relations
                .iter()
                .filter(|r| r.relation_type == "covers" && r.to_node_id == module.id)
                .filter_map(|r| case_ref_by_node.get(&r.from_node_id).cloned())
                .collect();

            let rows: Vec<&ExecutionTelemetryRow> = recent
                .iter()
                .filter(|r| {
                    r.test_case_id
                        .map(|c| c.to_string())
                        .map(|c| cases.contains(&c))
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            if rows.is_empty() {
                continue;
            }
            let failures = rows.iter().filter(|r| Self::is_failure(r)).count();
            // B3/Q1: both rates, so the sentence says which definition it is quoting instead of
            // leaving the reader to assume. `all_rate` keeps this signal's existing value and
            // therefore every existing consumer of it; `completed_rate` is the rate over
            // executions that actually ran.
            let all_rate = Self::rate(failures, rows.len());
            let completed_rate = Self::completed_failure_rate(&rows);
            let infra = Self::infra_errors(&rows);
            let Some(all_rate) = all_rate else { continue };

            let from = rows.iter().map(|r| r.created_at).min();
            let to = rows.iter().map(|r| r.created_at).max();
            // Q2: an executor failure is named in its own right, never inside a failure count.
            let infra_note = if infra > 0 {
                format!(
                    "; {infra} recorded outcome(s) were infrastructure errors, counted \
                          separately and excluded from both rates"
                )
            } else {
                String::new()
            };
            signals.push(PlanningSignal {
                kind: "module_risk".into(),
                subject: ref_name.clone(),
                metric: match completed_rate {
                    Some(c) => format!(
                        "{failures} of {} completed executions of this module's tests failed \
                         ({c:.1}%); of all {} recorded execution rows the all-outcome rate is \
                         {all_rate:.1}%{infra_note}",
                        failures + rows.iter().filter(|r| Self::is_pass(r)).count(),
                        rows.len()
                    ),
                    None => format!(
                        "{failures} of {} recorded executions of this module's tests failed \
                         ({all_rate:.1}%); no execution completed, so there is no \
                         completed-execution rate{infra_note}",
                        rows.len()
                    ),
                },
                evidence_runs: rows.iter().map(|r| r.run_id).collect(),
                confidence: confidence_from_sample(rows.len()),
                window: format_window(from, to, rows.len()),
                basis: format!(
                    "{} linked test case(s) via `covers` edges, {} recent execution rows, of \
                     which {} completed as a pass or failure",
                    cases.len(),
                    rows.len(),
                    rows.iter()
                        .filter(|r| Self::is_pass(r) || Self::is_failure(r))
                        .count()
                ),
            });
        }

        // ---- recurring defects -------------------------------------------
        for defect in self.bug_history(project_id).await? {
            if defect.occurrences < 2 {
                continue;
            }
            // Recency gate: a defect not seen inside the window does not plan.
            if defect.last_seen.map(|t| t < cutoff).unwrap_or(true) {
                continue;
            }
            signals.push(PlanningSignal {
                kind: "recurring_defect".into(),
                subject: defect.title.clone(),
                metric: format!(
                    "{} recorded occurrence(s), review state {}",
                    defect.occurrences, defect.review_state
                ),
                evidence_runs: Vec::new(),
                confidence: confidence_from_sample(defect.occurrences as usize),
                window: format_window(defect.first_seen, defect.last_seen, 0),
                basis: format!(
                    "failure cluster {} with occurrence_count {}, plus telemetry rows carrying its fingerprint",
                    defect.failure_cluster_id, defect.occurrences
                ),
            });
        }

        // ---- release regression -------------------------------------------
        let releases = self.store.list_releases(project_id).await?;
        for release in releases.iter().take(MAX_RELEASES_COMPARED) {
            // Used to `continue` on failure, silently dropping a release from the regression
            // signal this section exists to compute. It propagates.
            let runs = self.store.runs_for_release(release.id).await?;
            if runs.is_empty() {
                continue;
            }
            let ids: BTreeSet<Uuid> = runs.iter().copied().collect();
            let rows: Vec<&ExecutionTelemetryRow> = recent
                .iter()
                .filter(|r| ids.contains(&r.run_id))
                .copied()
                .collect();
            if rows.len() < 2 {
                continue; // one execution cannot show a regression
            }
            let failures = rows.iter().filter(|r| Self::is_failure(r)).count();
            // B3/Q1 + Q2: both rates and the executor-error count, all named. This signal exists
            // to say "this release got worse", and an all-outcome denominator lets blocked rows
            // pull it toward zero -- so the completed-execution rate leads, and the all-outcome
            // figure is quoted alongside rather than instead.
            let all_rate = Self::rate(failures, rows.len());
            let completed_rate = Self::completed_failure_rate(&rows);
            let infra = Self::infra_errors(&rows);
            let Some(all_rate) = all_rate else {
                continue;
            };
            let from = rows.iter().map(|r| r.created_at).min();
            let to = rows.iter().map(|r| r.created_at).max();
            let infra_note = if infra > 0 {
                format!(
                    "; {infra} recorded outcome(s) were infrastructure errors, counted separately \
                     and excluded from both rates"
                )
            } else {
                String::new()
            };
            signals.push(PlanningSignal {
                kind: "release_regression".into(),
                subject: release
                    .label
                    .clone()
                    .or_else(|| release.version.clone())
                    .unwrap_or_else(|| release.id.to_string()),
                metric: match completed_rate {
                    Some(c) => format!(
                        "{failures} of {} completed executions in this release failed ({c:.1}%); \
                         across all {} recorded execution rows the all-outcome rate is \
                         {all_rate:.1}%{infra_note}",
                        failures + rows.iter().filter(|r| Self::is_pass(r)).count(),
                        rows.len()
                    ),
                    None => format!(
                        "{failures} of {} recorded executions in this release failed \
                         ({all_rate:.1}%); nothing completed, so no completed-execution rate \
                         exists{infra_note}",
                        rows.len()
                    ),
                },
                evidence_runs: rows.iter().map(|r| r.run_id).collect(),
                confidence: confidence_from_sample(rows.len()),
                window: format_window(from, to, rows.len()),
                basis: format!(
                    "{} run(s) associated with release {}, of which {} recorded outcome(s) \
                     completed as a pass or failure",
                    runs.len(),
                    release.id,
                    rows.iter()
                        .filter(|r| Self::is_pass(r) || Self::is_failure(r))
                        .count()
                ),
            });
        }

        // Most-evidenced first, so a weak signal cannot outrank a strong one.
        signals.sort_by(|a, b| {
            let by_confidence = b
                .confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal);
            let by_evidence = b.evidence_runs.len().cmp(&a.evidence_runs.len());
            let by_kind = a.kind.cmp(&b.kind);
            let by_subject = a.subject.cmp(&b.subject);
            by_confidence
                .then(by_evidence)
                .then(by_kind)
                .then(by_subject)
        });
        signals.truncate(MAX_SIGNALS);
        Ok(signals)
    }
}

/// `"2026-09-01 .. 2026-10-01 (20 executions)"`, or a clear absence.
fn format_window(
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    executions: usize,
) -> String {
    match (from, to) {
        (Some(f), Some(t)) => format!(
            "{} .. {} ({executions} execution(s))",
            f.format("%Y-%m-%d"),
            t.format("%Y-%m-%d")
        ),
        _ => "no dated executions (0 executions)".to_string(),
    }
}
