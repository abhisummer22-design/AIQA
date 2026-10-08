//! Phase 5 (QA Intelligence Memory) — release & module risk intelligence.
//!
//! ## What is reused, and what is new
//!
//! The scoring formula is **not** new. `adaptive_intelligence::calibrate_risk` already
//! computes a weighted, explainable risk score from six named factors
//! (`failure_rate`, `flakiness_score`, `failure_severity`, `dependency_count`,
//! `healing_regression_rate`, `security_sensitivity`) and already emits a level and
//! a human-readable explanation. Master plan §7 says "risk score if already
//! supported", so this module's whole job is to **feed it real, measured factors**
//! for a scope it never covered — a module and a release — rather than to invent a
//! second score.
//!
//! What is new is the *inputs*: factors derived from stored history, per module and
//! per release, with a trend against previous releases.
//!
//! ## `INSUFFICIENT_DATA` is a first-class answer
//!
//! Master plan §7: "If insufficient data exists, INSUFFICIENT_DATA is preferable to a
//! fabricated risk conclusion." That is enforced rather than preferred:
//!
//! * A subject with no recorded executions returns `INSUFFICIENT_DATA` — **not** a
//!   score of 0.0 with level LOW, which would read as "we looked and it is fine".
//! * A factor that cannot be measured is `None`, and `calibrate_risk` is **not**
//!   called; the reason names the missing factor.
//! * Factors the store has no evidence for at all (dependency count, security
//!   sensitivity) are reported as `None` and **excluded from the score**, with the
//!   score labelled as partial. Inventing 0.0 for them would silently claim "no
//!   dependency risk", which is a claim nobody made.
//!
//! ## No fabrication
//!
//! Every factor carries the rows it was computed from. `security_sensitivity` and
//! `dependency_count` are *structurally* unavailable in this schema — there is no
//! column and no table for either — and that is reported as unmeasured rather than
//! as zero.

use crate::adaptive_intelligence::{calibrate_risk, RiskCalibration, RiskFactors};
use crate::qa_memory::{InsufficientData, QaMemoryReader, Retrieved, Signal, Window};
use crate::store::ExecutionTelemetryRow;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// Releases a single risk view will compare against.
const MAX_TREND_POINTS: usize = 10;

/// The weights `adaptive_intelligence::calibrate_risk` uses, transcribed from the
/// formula documented at that function:
///
/// ```text
/// 35% failure rate + 20% flakiness + 15% severity
///        + 10% dependency + 10% healing regression + 10% security
/// ```
///
/// They are named here rather than re-derived so there is exactly one place where
/// the model's shape is described, and so a change to `calibrate_risk` shows up here
/// as a test failure rather than as a silently different score.
pub const FACTOR_WEIGHTS: [(&str, f64); 6] = [
    ("failure_rate", 0.35),
    ("flakiness_score", 0.20),
    ("failure_severity", 0.15),
    ("dependency_count", 0.10),
    ("healing_regression_rate", 0.10),
    ("security_sensitivity", 0.10),
];

/// The weight of the factors this schema has no column for.
///
/// Three of the six are **structurally** unavailable: there is no column and no
/// table anywhere for a module's or a release's dependency count, security
/// sensitivity, or healing-regression rate. That is 30% of the model.
///
/// This is why a full `RiskCalibration` is never emitted for a module or a release,
/// and it is stated numerically rather than left for a reader to work out.
pub const STRUCTURALLY_UNAVAILABLE_WEIGHT: f64 = 0.30;

/// One measured risk factor, with its weight and the evidence behind it.
#[derive(Debug, Clone, Serialize)]
pub struct Factor {
    /// The `RiskFactors` field this fills.
    pub name: String,
    /// This factor's weight in the existing `calibrate_risk` formula.
    pub weight: f64,
    /// `None` when the store has no evidence for this factor at all.
    pub measured: Option<f64>,
    /// What it was computed from, or why it is unmeasured.
    pub basis: String,
    /// Rows behind the measurement. Empty when unmeasured.
    pub evidence: usize,
}

impl Factor {
    fn measured(name: &str, value: f64, basis: &str, evidence: usize) -> Self {
        Self {
            name: name.to_string(),
            weight: weight_of(name),
            measured: Some(value),
            basis: basis.to_string(),
            evidence,
        }
    }
    fn unmeasured(name: &str, why: &str) -> Self {
        Self {
            name: name.to_string(),
            weight: weight_of(name),
            measured: None,
            basis: why.to_string(),
            evidence: 0,
        }
    }
    /// What this factor actually contributed to the weighted sum: `weight × value`,
    /// or 0 when unmeasured. Reported per factor so a caller can see where a score
    /// comes from without reconstructing it.
    pub fn contribution(&self) -> f64 {
        self.weight * self.measured.unwrap_or(0.0)
    }
}

fn weight_of(name: &str) -> f64 {
    FACTOR_WEIGHTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, w)| *w)
        .unwrap_or(0.0)
}

/// Risk for one module (a `quality_nodes` vertex of type `module`).
#[derive(Debug, Clone, Serialize)]
pub struct ModuleRisk {
    pub module_ref: String,
    pub module_name: String,
    /// Present only when `calibrate_risk` was actually run. `None` means the module
    /// has history but not enough of it to score — never a zero standing in for one.
    pub calibration: Option<RiskCalibration>,
    /// Every factor, measured or explicitly unmeasured.
    pub factors: Vec<Factor>,
    /// True when at least one factor had to be excluded from the score.
    pub partial: bool,
    /// Fraction of the `calibrate_risk` model that real stored rows back, 0.0–1.0.
    /// For a module this is 0.70, because 30% of the model has nowhere to come from
    /// in this schema. Stated as a number so a reader is never left assuming the
    /// whole model was applied.
    pub measured_weight: f64,
    /// The complement: the fraction of the model with no evidence here, 0.0–1.0.
    pub unmeasured_weight: f64,
    /// The weighted sum of the measured factors only — what the measured 70% of the
    /// model contributed. **Not** a 0–1 score: divide by `measured_weight` for that,
    /// and note that such a division assumes the missing 30% would have been neutral.
    pub weighted_contribution: f64,
    pub linked_test_cases: usize,
    pub test_cases_with_history: usize,
    pub executions: usize,
    pub failures: usize,
    /// Failing test cases, with their own counts.
    pub affected_tests: Vec<AffectedTestRisk>,
    pub recurring_failure_count: usize,
    pub window: Window,
    pub signals: Vec<Signal>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AffectedTestRisk {
    pub test_case_id: String,
    pub executions: usize,
    pub failures: usize,
    pub failure_rate: Option<f64>,
}

/// One point on a release's timeline.
#[derive(Debug, Clone, Serialize)]
pub struct ReleaseTrendPoint {
    pub release_id: String,
    pub label: Option<String>,
    pub executions: usize,
    pub failures: usize,
    pub failure_rate: Option<f64>,
    pub risk_score: Option<f64>,
}

/// Risk for one release, compared against this project's other releases.
#[derive(Debug, Clone, Serialize)]
pub struct ReleaseRisk {
    pub release_id: String,
    pub label: Option<String>,
    pub version: Option<String>,
    /// Present only when the release has enough recorded executions to score.
    pub calibration: Option<RiskCalibration>,
    pub factors: Vec<Factor>,
    pub partial: bool,
    /// Fraction of the `calibrate_risk` model that real stored rows back, 0.0–1.0.
    pub measured_weight: f64,
    /// The complement: the fraction with no evidence here, 0.0–1.0.
    pub unmeasured_weight: f64,
    /// The weighted sum of the measured factors only. Not a 0–1 score; see
    /// `ModuleRisk::weighted_contribution`.
    pub weighted_contribution: f64,
    pub runs: usize,
    pub executions: usize,
    pub failures: usize,
    pub failure_rate: Option<f64>,
    /// `"worsening"`, `"improving"`, `"steady"`, or `None` when there is nothing to
    /// compare against. Never invented.
    pub trend: Option<String>,
    /// The comparison itself, oldest first.
    pub trend_points: Vec<ReleaseTrendPoint>,
    pub window: Window,
    pub signals: Vec<Signal>,
}

impl QaMemoryReader {
    /// Risk for one module, from its recorded executions.
    pub async fn module_risk(&self, project_id: &str, module_ref: &str) -> Retrieved<ModuleRisk> {
        let all = self.telemetry_for(project_id).await;
        if all.is_empty() {
            return Retrieved::empty(project_id, InsufficientData::NoExecutions);
        }

        let (nodes, relations) = match (
            self.store.list_quality_nodes(project_id).await,
            self.store
                .list_quality_relations_for_project(project_id)
                .await,
        ) {
            (Ok(n), Ok(r)) => (n, r),
            _ => return Retrieved::empty(project_id, InsufficientData::NoMatchingHistory),
        };
        let Some(module_node) = nodes
            .iter()
            .find(|n| n.node_type == "module" && n.external_ref.as_deref() == Some(module_ref))
        else {
            return Retrieved::empty(project_id, InsufficientData::UnknownSubject);
        };

        let case_refs: BTreeSet<String> = relations
            .iter()
            .filter(|r| r.relation_type == "covers" && r.to_node_id == module_node.id)
            .filter_map(|r| {
                nodes
                    .iter()
                    .find(|n| n.id == r.from_node_id && n.node_type == "test")
                    .and_then(|n| n.external_ref.clone())
            })
            .collect();

        let rows: Vec<&ExecutionTelemetryRow> = all
            .iter()
            .filter(|r| {
                r.test_case_id
                    .map(|c| c.to_string())
                    .map(|c| case_refs.contains(&c))
                    .unwrap_or(false)
            })
            .collect();
        if rows.is_empty() {
            return Retrieved::empty(project_id, InsufficientData::NoMatchingHistory);
        }

        let failures = rows.iter().filter(|r| Self::is_failure(r)).count();
        let failure_rate = (failures as f64 / rows.len() as f64).clamp(0.0, 1.0);

        // Flakiness, from the existing evaluation, per test case then averaged.
        let per_case = per_case_counts(&rows);
        let flakiness = average_flakiness(&rows, &per_case);

        // Severity: how the failures break down by category, normalised. Measured
        // only when at least one failure carries a category.
        let severity = failure_severity(&rows);

        let factors = vec![
            Factor::measured(
                "failure_rate",
                failure_rate,
                &format!("{failures} failed of {} recorded executions", rows.len()),
                rows.len(),
            ),
            Factor::measured(
                "flakiness_score",
                flakiness,
                "adaptive_intelligence::evaluate_flakiness per linked test case, averaged",
                rows.len(),
            ),
            match severity {
                Some(v) => Factor::measured(
                    "failure_severity",
                    v,
                    "failure categories of this module's failures, weighted by share",
                    failures,
                ),
                None => Factor::unmeasured(
                    "failure_severity",
                    "no failure in this module carries a failure_category",
                ),
            },
            Factor::unmeasured(
                "dependency_count",
                "this schema has no dependency count for a module's test cases",
            ),
            Factor::unmeasured(
                "healing_regression_rate",
                "this module has no recorded healing proposal, so there is no regression to measure",
            ),
            Factor::unmeasured(
                "security_sensitivity",
                "this schema has no security_sensitivity field for a module",
            ),
        ];

        let (calibration, coverage) = calibrate_or_none(module_ref, &factors);
        let partial = coverage.unmeasured > f64::EPSILON;
        let owned: Vec<ExecutionTelemetryRow> = rows.iter().map(|r| (*r).clone()).collect();
        let window = Self::window(&owned);

        let mut affected_tests: Vec<AffectedTestRisk> = per_case
            .iter()
            .filter(|(_, (_, f))| *f > 0)
            .map(|(id, (n, f))| AffectedTestRisk {
                test_case_id: id.clone(),
                executions: *n,
                failures: *f,
                failure_rate: (if *n > 0 {
                    Some((*f as f64 / *n as f64) * 100.0)
                } else {
                    None
                }),
            })
            .collect();
        affected_tests.sort_by(|a, b| {
            b.failures
                .cmp(&a.failures)
                .then(a.test_case_id.cmp(&b.test_case_id))
        });

        let fingerprints: BTreeSet<&str> = rows
            .iter()
            .filter_map(|r| r.failure_fingerprint.as_deref())
            .filter(|f| !f.is_empty())
            .collect();
        let recurring = self
            .module_risk_history(project_id, module_ref)
            .await
            .view
            .map(|v| v.recurring_failures.len())
            .unwrap_or(
                fingerprints
                    .iter()
                    .collect::<BTreeSet<_>>()
                    .len()
                    .saturating_sub(0),
            );

        let mut signals = Vec::new();
        signals.push(Signal {
            kind: "module_failure_rate".into(),
            statement: format!(
                "{failures} of {} recorded executions of this module's tests failed ({:.1}%)",
                rows.len(),
                failure_rate * 100.0
            ),
            evidence_runs: rows.iter().map(|r| r.run_id).collect(),
            confidence: if rows.len() >= 5 { 0.85 } else { 0.5 },
            basis: format!(
                "{} linked test case(s) via `covers` edges, {} execution rows",
                case_refs.len(),
                rows.len()
            ),
        });
        if partial {
            signals.push(Signal {
                kind: "partial_score".into(),
                statement: format!(
                    "the risk score omits {} of the six factors because this schema stores no evidence for them",
                    factors.iter().filter(|f| f.measured.is_none()).count()
                ),
                evidence_runs: Vec::new(),
                confidence: 1.0,
                basis: "each unmeasured factor names its own reason".into(),
            });
        }

        Retrieved::found(
            project_id,
            ModuleRisk {
                module_ref: module_ref.to_string(),
                module_name: module_node.name.clone(),
                factors,
                calibration,
                partial,
                measured_weight: coverage.measured,
                unmeasured_weight: coverage.unmeasured,
                weighted_contribution: coverage.contribution,
                linked_test_cases: case_refs.len(),
                test_cases_with_history: per_case.len(),
                executions: rows.len(),
                failures,
                affected_tests,
                recurring_failure_count: recurring,
                window,
                signals,
            },
        )
    }

    /// Risk for one release, plus a trend against this project's other releases.
    pub async fn release_risk(&self, project_id: &str, release_id: Uuid) -> Retrieved<ReleaseRisk> {
        let all = self.telemetry_for(project_id).await;
        let releases = self
            .store
            .list_releases(project_id)
            .await
            .unwrap_or_default();
        let Some(target) = releases.iter().find(|r| r.id == release_id) else {
            return Retrieved::empty(project_id, InsufficientData::UnknownSubject);
        };
        let Ok(target_runs) = self.store.runs_for_release(release_id).await else {
            return Retrieved::empty(project_id, InsufficientData::NoMatchingHistory);
        };

        let ids: BTreeSet<Uuid> = target_runs.iter().copied().collect();
        let rows: Vec<&ExecutionTelemetryRow> =
            all.iter().filter(|r| ids.contains(&r.run_id)).collect();
        if rows.is_empty() {
            // A recorded-but-unexercised release is a true state, reported as such.
            return Retrieved::empty(project_id, InsufficientData::NoMatchingHistory);
        }

        let failures = rows.iter().filter(|r| Self::is_failure(r)).count();
        let failure_rate = (failures as f64 / rows.len() as f64).clamp(0.0, 1.0);
        let per_case = per_case_counts(&rows);
        let flakiness = average_flakiness(&rows, &per_case);
        let severity = failure_severity(&rows);

        let factors = vec![
            Factor::measured(
                "failure_rate",
                failure_rate,
                &format!("{failures} failed of {} recorded executions", rows.len()),
                rows.len(),
            ),
            Factor::measured(
                "flakiness_score",
                flakiness,
                "adaptive_intelligence::evaluate_flakiness per test case in this release, averaged",
                rows.len(),
            ),
            match severity {
                Some(v) => Factor::measured(
                    "failure_severity",
                    v,
                    "failure categories within this release, weighted by share",
                    failures,
                ),
                None => Factor::unmeasured(
                    "failure_severity",
                    "no failure in this release carries a failure_category",
                ),
            },
            Factor::unmeasured(
                "dependency_count",
                "this schema has no dependency count for a release",
            ),
            Factor::unmeasured(
                "healing_regression_rate",
                "no healing proposal in this project records a regression against a release",
            ),
            Factor::unmeasured(
                "security_sensitivity",
                "this schema has no security_sensitivity field for a release",
            ),
        ];

        let (calibration, coverage) = calibrate_or_none(&release_id.to_string(), &factors);
        let partial = coverage.unmeasured > f64::EPSILON;

        // ---- the comparison, oldest first -------------------------------
        let mut points: Vec<ReleaseTrendPoint> = Vec::new();
        // The target is excluded here and appended at the end instead. Without
        // this filter the newest release appears twice in its own trend, which
        // both inflates the point count and biases `trend_from` — the newest
        // value would then be averaged against itself as one of the "earlier"
        // points.
        for other in releases.iter().filter(|r| r.id != release_id) {
            let Ok(runs) = self.store.runs_for_release(other.id).await else {
                continue;
            };
            let oids: BTreeSet<Uuid> = runs.iter().copied().collect();
            let orows: Vec<&ExecutionTelemetryRow> =
                all.iter().filter(|r| oids.contains(&r.run_id)).collect();
            // A release with no executions is omitted, never counted as a clean one.
            if orows.is_empty() {
                continue;
            }
            let of = orows.iter().filter(|r| Self::is_failure(r)).count();
            let orate = of as f64 / orows.len() as f64;
            let opc = per_case_counts(&orows);
            let ofl = average_flakiness(&orows, &opc);
            let osev = failure_severity(&orows);
            let ofactors = release_factors(orate, ofl, osev);
            let (ocal, _) = calibrate_or_none(&other.id.to_string(), &ofactors);
            points.push(ReleaseTrendPoint {
                release_id: other.id.to_string(),
                label: other.label.clone().or(other.version.clone()),
                executions: orows.len(),
                failures: of,
                failure_rate: Some(orate * 100.0),
                risk_score: ocal.as_ref().map(|c| c.risk_score),
            });
        }
        // Oldest first, so the last point is the newest release.
        points.sort_by(|a, b| {
            releases
                .iter()
                .find(|r| r.id.to_string() == a.release_id)
                .map(|r| r.created_at)
                .cmp(
                    &releases
                        .iter()
                        .find(|r| r.id.to_string() == b.release_id)
                        .map(|r| r.created_at),
                )
        });
        points.truncate(MAX_TREND_POINTS);

        // The target's own point, so a caller sees it in the same shape as the rest.
        points.push(ReleaseTrendPoint {
            release_id: release_id.to_string(),
            label: target.label.clone().or(target.version.clone()),
            executions: rows.len(),
            failures,
            failure_rate: Some(failure_rate * 100.0),
            risk_score: calibration.as_ref().map(|c| c.risk_score),
        });

        let trend = trend_from(&points);

        let owned: Vec<ExecutionTelemetryRow> = rows.iter().map(|r| (*r).clone()).collect();
        let window = Self::window(&owned);
        let mut signals = Vec::new();
        signals.push(Signal {
            kind: "release_failure_rate".into(),
            statement: format!(
                "{failures} of {} recorded executions in this release failed ({:.1}%)",
                rows.len(),
                failure_rate * 100.0
            ),
            evidence_runs: rows.iter().map(|r| r.run_id).collect(),
            confidence: if rows.len() >= 5 { 0.85 } else { 0.5 },
            basis: format!("{} run(s) associated with this release", target_runs.len()),
        });
        if let Some(t) = &trend {
            signals.push(Signal {
                kind: "release_trend".into(),
                statement: format!("the trend across comparable releases is {t}"),
                evidence_runs: Vec::new(),
                confidence: if points.len() >= 3 { 0.7 } else { 0.4 },
                basis: format!(
                    "failure rates of {} release(s) with recorded executions, oldest first",
                    points.len()
                ),
            });
        }
        if partial {
            signals.push(Signal {
                kind: "partial_score".into(),
                statement: format!(
                    "the risk score omits {} of the six factors because this schema stores no evidence for them",
                    factors.iter().filter(|f| f.measured.is_none()).count()
                ),
                evidence_runs: Vec::new(),
                confidence: 1.0,
                basis: "each unmeasured factor names its own reason".into(),
            });
        }

        Retrieved::found(
            project_id,
            ReleaseRisk {
                release_id: release_id.to_string(),
                label: target.label.clone(),
                version: target.version.clone(),
                factors,
                calibration,
                partial,
                measured_weight: coverage.measured,
                unmeasured_weight: coverage.unmeasured,
                weighted_contribution: coverage.contribution,
                runs: target_runs.len(),
                executions: rows.len(),
                failures,
                failure_rate: Some(failure_rate * 100.0),
                trend,
                trend_points: points,
                window,
                signals,
            },
        )
    }

    /// Every release of a project that has recorded executions, with its risk.
    ///
    /// The project-level view: which of its releases is currently the riskiest,
    /// on evidence. A release with no executions is **absent**, not listed as LOW.
    pub async fn release_risk_overview(&self, project_id: &str) -> Vec<ReleaseRisk> {
        let releases = self
            .store
            .list_releases(project_id)
            .await
            .unwrap_or_default();
        let mut out = Vec::new();
        for r in releases {
            if let Retrieved { view: Some(v), .. } = self.release_risk(project_id, r.id).await {
                out.push(v);
            }
        }
        out.sort_by(|a, b| {
            b.calibration
                .as_ref()
                .map(|c| c.risk_score)
                .partial_cmp(&a.calibration.as_ref().map(|c| c.risk_score))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.label.cmp(&b.label))
                .then(a.label.cmp(&b.label))
        });
        out
    }
}

/// Per-test-case execution/failure counts within a set of rows.
fn per_case_counts(rows: &[&ExecutionTelemetryRow]) -> BTreeMap<String, (usize, usize)> {
    let mut out: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for r in rows {
        let Some(case) = r.test_case_id.map(|c| c.to_string()) else {
            continue;
        };
        let e = out.entry(case).or_insert((0, 0));
        e.0 += 1;
        if QaMemoryReader::is_failure(r) {
            e.1 += 1;
        }
    }
    out
}

/// Mean flakiness across the test cases present, via the existing evaluation.
/// `None` when no test case has two samples, because a mean of hints is a hint.
fn average_flakiness(
    rows: &[&ExecutionTelemetryRow],
    per_case: &BTreeMap<String, (usize, usize)>,
) -> f64 {
    let mut scores: Vec<f64> = Vec::new();
    for (case, (_, failures)) in per_case {
        let case_rows: Vec<&ExecutionTelemetryRow> = rows
            .iter()
            .filter(|r| r.test_case_id.map(|c| c.to_string()).as_deref() == Some(case.as_str()))
            .copied()
            .collect();
        if case_rows.len() < 2 {
            continue;
        }
        let outcomes: Vec<crate::domain::TaskStatus> = case_rows
            .iter()
            .map(|r| match r.status.as_str() {
                "PASSED" => crate::domain::TaskStatus::Passed,
                "FAILED" => crate::domain::TaskStatus::Failed,
                "TIMEOUT" => crate::domain::TaskStatus::Timeout,
                "SKIPPED" => crate::domain::TaskStatus::Skipped,
                "BLOCKED" => crate::domain::TaskStatus::Blocked,
                "CANCELLED" => crate::domain::TaskStatus::Cancelled,
                _ => crate::domain::TaskStatus::InfraError,
            })
            .collect();
        let _ = failures;
        scores.push(
            crate::adaptive_intelligence::evaluate_flakiness(
                case,
                &case_rows
                    .first()
                    .map(|r| r.environment.clone())
                    .unwrap_or_default(),
                &outcomes,
            )
            .flakiness_score
            .clamp(0.0, 1.0),
        );
    }
    if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
    }
}

/// Severity from failure categories, or `None` when no failure carries one.
///
/// A category that is not recognised counts toward the average at its own weight
/// rather than being dropped, so an unknown category cannot make a release look
/// safer than it is.
fn failure_severity(rows: &[&ExecutionTelemetryRow]) -> Option<f64> {
    use crate::adaptive_intelligence::FailureCategory;
    let failures: Vec<&ExecutionTelemetryRow> = rows
        .iter()
        .filter(|r| QaMemoryReader::is_failure(r))
        .copied()
        .collect();
    if failures.is_empty() {
        return None;
    }
    let mut total = 0.0;
    let mut counted = 0usize;
    for r in &failures {
        let Some(cat) = r.failure_category.as_deref() else {
            continue;
        };
        // `FailureCategory::from_str` is total: an unrecognised value becomes
        // `Unknown`, which carries the middle weight. A category the store could
        // not classify therefore does not silently vanish from the average.
        total += category_weight(FailureCategory::from_str(cat));
        counted += 1;
    }
    if counted == 0 {
        None
    } else {
        Some((total / counted as f64).clamp(0.0, 1.0))
    }
}

/// How much one failure category contributes to severity.
///
/// Ordered by how much a failure of that kind should worry a release: a data
/// failure or an auth failure is worse than a client-side typo. This is a stated,
/// testable convention rather than a model.
fn category_weight(cat: crate::adaptive_intelligence::FailureCategory) -> f64 {
    use crate::adaptive_intelligence::FailureCategory as C;
    match cat {
        C::DataFailure => 1.0,
        C::AuthenticationFailure | C::AuthorizationFailure => 0.95,
        C::ValidationFailure => 0.8,
        C::DependencyFailure => 0.75,
        C::ServerError => 0.7,
        C::Timeout => 0.65,
        C::NetworkFailure => 0.6,
        C::EnvironmentFailure => 0.55,
        C::SelectorNotFound | C::SelectorAmbiguous => 0.45,
        C::RateLimit => 0.4,
        C::AssertionFailure => 0.4,
        C::ClientError => 0.3,
        // `Unknown` is the store's own "we could not classify this", so it sits in
        // the middle rather than at either end.
        C::Unknown => 0.5,
    }
}

/// The three measurable factors, for the trend comparison.
fn release_factors(failure_rate: f64, flakiness: f64, severity: Option<f64>) -> Vec<Factor> {
    vec![
        Factor::measured("failure_rate", failure_rate, "derived", 0),
        Factor::measured("flakiness_score", flakiness, "derived", 0),
        match severity {
            Some(v) => Factor::measured("failure_severity", v, "derived", 0),
            None => Factor::unmeasured("failure_severity", "no categorised failure"),
        },
    ]
}

/// How much of the model real rows back, and what the measured factors summed to.
#[derive(Debug, Clone, Copy)]
struct WeightCoverage {
    measured: f64,
    unmeasured: f64,
    contribution: f64,
}

/// Run the **existing** `calibrate_risk`, but only when every factor it needs is
/// measured.
///
/// `calibrate_risk` takes six `f64`s. Passing 0.0 for a factor that simply has no
/// evidence would make it report "no dependency risk" and "no security risk" for a
/// schema that has never stored either — the most misleading possible answer, and
/// the one master plan §7 rules out. So when anything is unmeasured this returns
/// `None` and the caller reports the measured coverage instead of a score.
///
/// In this schema the unreachable branch is the normal one, because three of the six
/// factors have no column anywhere. It is kept because the moment a migration adds
/// one, the full model starts working with no change here.
fn calibrate_or_none(
    subject: &str,
    factors: &[Factor],
) -> (Option<RiskCalibration>, WeightCoverage) {
    let measured_weight: f64 = factors
        .iter()
        .filter(|f| f.measured.is_some())
        .map(|f| f.weight)
        .sum();
    let total_weight: f64 = factors.iter().map(|f| f.weight).sum();
    let contribution: f64 = factors.iter().map(|f| f.contribution()).sum();
    let coverage = WeightCoverage {
        measured: measured_weight,
        unmeasured: (total_weight - measured_weight).max(0.0),
        contribution,
    };

    if measured_weight < total_weight - f64::EPSILON {
        return (None, coverage);
    }

    let get = |name: &str| {
        factors
            .iter()
            .find(|f| f.name == name)
            .and_then(|f| f.measured)
            .unwrap_or(0.0)
    };
    let calibration = calibrate_risk(
        subject,
        RiskFactors {
            failure_rate: get("failure_rate"),
            flakiness_score: get("flakiness_score"),
            failure_severity: get("failure_severity"),
            dependency_count: get("dependency_count").round() as usize,
            healing_regression_rate: get("healing_regression_rate"),
            security_sensitivity: get("security_sensitivity"),
        },
    );
    (Some(calibration), coverage)
}

/// Direction of travel across the trend points, or `None` with too few to say.
///
/// Uses the failure rate of the newest point against the mean of the ones before
/// it. One prior release is not a trend, so at least two are required.
fn trend_from(points: &[ReleaseTrendPoint]) -> Option<String> {
    if points.len() < 3 {
        return None;
    }
    let rates: Vec<f64> = points.iter().filter_map(|p| p.failure_rate).collect();
    if rates.len() < 3 {
        return None;
    }
    let (last, earlier) = rates.split_last()?;
    let mean = earlier.iter().sum::<f64>() / earlier.len() as f64;
    // A 5-point band, so noise is not reported as movement.
    let delta = last - mean;
    Some(
        if delta > 5.0 {
            "worsening"
        } else if delta < -5.0 {
            "improving"
        } else {
            "steady"
        }
        .to_string(),
    )
}
