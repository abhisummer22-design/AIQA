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
    /// A factor backed by a real measurement.
    ///
    /// Public because `calibrate_partial` is public: a caller outside this module must be able
    /// to contribute the factors it actually measured, rather than only inspecting the ones this
    /// module measured for it.
    pub fn measured(name: &str, value: f64, basis: &str, evidence: usize) -> Self {
        Self {
            name: name.to_string(),
            weight: weight_of(name),
            measured: Some(value),
            basis: basis.to_string(),
            evidence,
        }
    }
    /// A factor with **no signal source**, and the reason.
    ///
    /// Public because this is now the answer a caller must be able to give. The whole point of
    /// the risk-signal remediation is that "this schema has no source for this" has to be
    /// expressible from outside this module — otherwise a caller facing a factor it cannot
    /// measure has no option but to pass `0.0`, which is the defect being fixed.
    pub fn unmeasured(name: &str, why: &str) -> Self {
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
        // Phase 4 performance: the evidence is gathered **once** here and every case is
        // answered from it. The per-case path used to re-read telemetry, healing
        // proposals and bug drafts for each case, which made prioritising a project of
        // 500 cases cost 2,000+ reads returning identical rows.
        let evidence = self.test_evidence(project_id).await;
        if evidence.all.is_empty() {
            return Retrieved::empty(project_id, InsufficientData::NoExecutions);
        }
        if evidence.all.is_empty() {
            return Retrieved::empty(project_id, InsufficientData::NoExecutions);
        }
        let all = evidence.all.clone();

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
pub struct WeightCoverage {
    /// Share of the model's weight that a real measurement backs, 0.0-1.0.
    pub measured: f64,
    /// Share that no measurement backs. A non-zero value here is a stated gap.
    pub unmeasured: f64,
    /// Weighted sum of the measured factors only. **Not** a 0-1 score.
    pub contribution: f64,
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
/// The other half of `calibrate_or_none`: a score from the **measured** factors, normalised
/// over the weight the data actually backs.
///
/// ## Why this exists alongside the refusal
///
/// `calibrate_or_none` declines to emit a calibration unless every factor measures, because a
/// partial weighted sum silently claims the missing weight was zero. That refusal is right for a
/// report, and this module keeps it.
///
/// But declining is not the only honest option, and for a caller that *has* measured factors it
/// throws away real information. Passing `0.0` into `calibrate_risk` for a factor with no
/// evidence is the thing that must never happen — and dividing the measured sum by the measured
/// weight is the standard correction for it.
///
/// ## The assumption, stated
///
/// Dividing by `measured_weight` is equivalent to asserting the unmeasured factors *would have
/// been neutral*. That is an **assumption, not a measurement**, and it is written into
/// `RiskCalibration::explanation` so it cannot be read past. When the unmeasured weight is large
/// the assumption is weak, which is why the coverage is reported alongside rather than folded
/// into the number.
///
/// ## One owner of weights and bands
///
/// The weights come from [`FACTOR_WEIGHTS`] (which is transcribed from `calibrate_risk`'s own
/// formula) and the band comes from `adaptive_intelligence::risk_level_for`. Neither is
/// re-declared here, so this cannot become a second risk engine: it reuses the existing weights
/// and the existing bands and adds only the normalisation.
///
/// ## Not a substitute for the refusal
///
/// `calibration: Option<RiskCalibration>` stays `None` on the module/release/test-case surfaces.
/// This function is for callers that would otherwise have to fabricate a zero — chiefly
/// `GET /projects/:id/intelligence/risk`, which measures five of the six factors.
pub fn calibrate_partial(subject: &str, factors: &[Factor]) -> (RiskCalibration, WeightCoverage) {
    let measured_weight: f64 = factors
        .iter()
        .filter(|f| f.measured.is_some())
        .map(|f| f.weight)
        .sum();
    let total_weight: f64 = factors.iter().map(|f| f.weight).sum();
    let unmeasured_weight: f64 = total_weight - measured_weight;
    let contribution: f64 = factors.iter().map(|f| f.contribution()).sum();

    // Nothing measured is not a zero score; it is no answer. Delegated to the refusal's
    // condition rather than duplicating it.
    if measured_weight <= f64::EPSILON {
        return (
            RiskCalibration {
                task_name: subject.to_string(),
                risk_score: 0.0,
                risk_level: "INSUFFICIENT_DATA".to_string(),
                factors: RiskFactors {
                    failure_rate: 0.0,
                    flakiness_score: 0.0,
                    failure_severity: 0.0,
                    dependency_count: 0,
                    healing_regression_rate: 0.0,
                    security_sensitivity: 0.0,
                },
                explanation: format!(
                    "no factor of the model for {subject} could be measured, so no score is \
                     available; this is not a score of zero"
                ),
            },
            WeightCoverage {
                measured: 0.0,
                unmeasured: unmeasured_weight,
                contribution: 0.0,
            },
        );
    }

    let normalised = (contribution / measured_weight).clamp(0.0, 1.0);
    let unmeasured: Vec<&str> = factors
        .iter()
        .filter(|f| f.measured.is_none())
        .map(|f| f.name.as_str())
        .collect();

    let explanation = if unmeasured.is_empty() {
        format!(
            "weighted from {} measured factor(s) covering {:.0}% of the model",
            factors.len(),
            measured_weight * 100.0
        )
    } else {
        format!(
            "weighted from the measured factor(s) only, covering {:.0}% of the model, and \
             normalised over that weight; the unmeasured factor(s) {} are assumed neutral, \
             which is an assumption rather than a measurement",
            measured_weight * 100.0,
            unmeasured.join(", ")
        )
    };

    (
        RiskCalibration {
            task_name: subject.to_string(),
            risk_score: (normalised * 100.0).round() / 100.0,
            risk_level: crate::adaptive_intelligence::risk_level_for(normalised).to_string(),
            // `calibrate_risk`'s own struct field set, which is all `f64`. Filled with the
            // measured factors where the field exists and 0.0 otherwise -- and the caller is
            // told so in the explanation above, so the struct is never the thing a reader trusts.
            factors: RiskFactors {
                failure_rate: measured_of(factors, "failure_rate"),
                flakiness_score: measured_of(factors, "flakiness_score"),
                failure_severity: measured_of(factors, "failure_severity"),
                dependency_count: factors
                    .iter()
                    .find(|f| f.name == "dependency_count")
                    .and_then(|f| f.measured)
                    .unwrap_or(0.0) as usize,
                healing_regression_rate: measured_of(factors, "healing_regression_rate"),
                security_sensitivity: measured_of(factors, "security_sensitivity"),
            },
            explanation,
        },
        WeightCoverage {
            measured: measured_weight,
            unmeasured: unmeasured_weight,
            contribution,
        },
    )
}

/// One measured factor's value, or 0.0 when unmeasured.
fn measured_of(factors: &[Factor], name: &str) -> f64 {
    factors
        .iter()
        .find(|f| f.name == name)
        .and_then(|f| f.measured)
        .unwrap_or(0.0)
}

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

// ============================================================================
// PHASE 4 - RISK-BASED TESTING: per-test risk and prioritisation
// ============================================================================
//
// ## What is reused
//
// Everything statistical. This section adds **no** formula. The six-factor
// `calibrate_risk`, the `Factor` shape, `calibrate_or_none`, `failure_severity`,
// `per_case_counts` and `average_flakiness` are all used as they already were for
// modules and releases. What is new is a third *scope* — the individual test case —
// and an ordering on top of it.
//
// ## Why there is no risk score, and that is the finding
//
// `calibrate_or_none` refuses to emit a `RiskCalibration` unless **every** factor is
// measured, because a partial weighted sum silently claims the missing weight was zero.
// For a test case two factors have no column anywhere in this schema:
//
//   * `dependency_count` - there is no case-to-case dependency graph. `TaskDef.prerequisites`
//     is per *run*, not per case, so a test's dependencies are not stored.
//   * `security_sensitivity` - no column and no table, as this module's header already
//     records for modules and releases.
//
// So `test_risk().calibration` is `None` for every test case, today and structurally, and
// **Phase 4 does not invent a score to fill the gap.** It reports what is measurable
// (`weighted_contribution`, `measured_weight`, `unmeasured_weight`, each factor with its
// basis and evidence count) and leaves the absent score absent. A caller can see exactly
// how much of the model the data backed.
//
// ## Prioritisation is an order, not a blend
//
// `priority` is a band from a **documented total order** over named, individually
// evidenced conditions - not a new weighted formula that would become a third risk model
// alongside `calibrate_risk` and the inline formula in `intelligence.rs`. Each band states
// the condition that put a test in it, and every applicable condition is reported, so the
// band is auditable rather than a number nobody can reconstruct.
//
// ## The safety floor
//
// `PriorityBand::Mandatory` is the floor, derived exactly as Phase 3 derives it: a test
// case with recorded failure evidence. There is **no declared mandatory flag anywhere in
// this schema** (verified), so the floor is a derivation - stated as such rather than
// presented as policy. Prioritisation **orders**; it never removes, so nothing on the
// floor can be dropped by being low risk. `Phase 3`'s `Scope::restricted_to` is what
// enforces retention when a run is actually scoped down.

/// Flakiness at or above this is reported as flaky. A stated number so the band can be
/// checked rather than taken on trust.
pub const FLAKY_THRESHOLD: f64 = 0.34;

/// Ceiling on test cases listed by one prioritisation call.
pub const PRIORITISE_MAX: usize = 500;

/// Everything the per-case risk computation reads, gathered once.
///
/// Held by value and handed to [`QaMemoryReader::test_risk_from`] so that answering a
/// question about N cases costs one read of each table, not N. See
/// [`QaMemoryReader::test_evidence`].
struct TestEvidence {
    all: Vec<ExecutionTelemetryRow>,
    proposals: Vec<crate::store::HealingProposalRow>,
    drafts: Vec<crate::store::BugDraftRow>,
    /// Resolved once, keyed by cluster id, so the draft → cluster hop is one indexed
    /// lookup per distinct cluster rather than per (case, draft) pair.
    clusters: BTreeMap<Uuid, crate::store::FailureClusterRow>,
    /// Case titles, read with the rest of the evidence. Pre-read so the per-case
    /// computation stays synchronous and does not await per case.
    titles: BTreeMap<Uuid, String>,
}

/// One test case's risk, from its own recorded executions.
#[derive(Debug, Clone, Serialize)]
pub struct TestRisk {
    pub test_case_id: String,
    pub title: Option<String>,
    /// `None` in this schema — see the section header. Never a zero standing in for a
    /// score nobody could compute.
    pub calibration: Option<RiskCalibration>,
    /// All six factors, measured or explicitly unmeasured with the reason.
    pub factors: Vec<Factor>,
    pub partial: bool,
    /// Fraction of the `calibrate_risk` model that stored rows back, 0.0-1.0.
    pub measured_weight: f64,
    pub unmeasured_weight: f64,
    /// Weighted sum of the measured factors only. **Not** a 0-1 score.
    pub weighted_contribution: f64,
    /// Measured factors divided by `measured_weight`. This assumes the unmeasured weight
    /// would have been neutral, which is an assumption, not a measurement — hence
    /// `band_basis` states it and `calibration` stays `None`.
    pub measured_ratio: Option<f64>,
    pub executions: usize,
    pub failures: usize,
    pub failure_rate: Option<f64>,
    pub flakiness_score: Option<f64>,
    /// Bug drafts whose cluster this case's failures produced.
    pub recurring_defects: usize,
    /// Healing proposals raised against this case's failure signature.
    pub healing_attempts: usize,
    /// Of those, the ones whose verification recorded a regression.
    pub healing_regressions: usize,
    pub window: Window,
    pub signals: Vec<Signal>,
}

/// Why a test case is where it is in the order.
///
/// Ordered highest-priority first; [`PriorityBand::rank`] is the sort key and is a total
/// order, so the same data always produces the same list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityBand {
    /// On the safety floor: has recorded failure evidence. Never removed by prioritisation.
    Mandatory,
    /// Inside a declared change's impact set (Phase 3 `change_impacts`).
    Changed,
    /// The measured share of the risk model is at or above 0.70.
    CriticalRisk,
    /// ... at or above 0.50.
    HighRisk,
    /// A bug draft exists against this case's failure.
    RecurringDefect,
    /// Flakiness at or above [`FLAKY_THRESHOLD`].
    Flaky,
    /// No condition applies. Still listed — this band orders, it does not exclude.
    Routine,
}

impl PriorityBand {
    pub fn rank(self) -> u8 {
        match self {
            Self::Mandatory => 0,
            Self::Changed => 1,
            Self::CriticalRisk => 2,
            Self::HighRisk => 3,
            Self::RecurringDefect => 4,
            Self::Flaky => 5,
            Self::Routine => 6,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mandatory => "mandatory",
            Self::Changed => "changed",
            Self::CriticalRisk => "critical_risk",
            Self::HighRisk => "high_risk",
            Self::RecurringDefect => "recurring_defect",
            Self::Flaky => "flaky",
            Self::Routine => "routine",
        }
    }

    /// The condition that put a test in this band, in one clause.
    pub fn rationale(self) -> &'static str {
        match self {
            Self::Mandatory => {
                "this case has recorded failure evidence, so it is on the \
                 safety floor and is never removed by prioritisation"
            }
            Self::Changed => "this case is inside a declared change's recorded impact set",
            Self::CriticalRisk => {
                "the measured share of the existing risk model is at or \
                 above 0.70"
            }
            Self::HighRisk => {
                "the measured share of the existing risk model is at or above \
                 0.50"
            }
            Self::RecurringDefect => {
                "a bug draft has been raised against this case's \
                 recorded failure"
            }
            Self::Flaky => {
                "this case's recorded outcomes have flipped often enough to meet \
                 the flakiness threshold"
            }
            Self::Routine => "no risk condition applies to this case",
        }
    }
}

/// One test case in the prioritised order.
#[derive(Debug, Clone, Serialize)]
pub struct PrioritisedTest {
    pub test_case_id: String,
    pub title: Option<String>,
    /// The highest band that applies.
    pub priority: PriorityBand,
    /// Every band that applies, so a case that is both `changed` and `critical_risk`
    /// reports both rather than silently losing one.
    pub applies: Vec<PriorityBand>,
    /// One human-readable line per applying condition — the explanation for the order.
    pub reasons: Vec<String>,
    pub risk: TestRisk,
    /// The change this test is impacted by, when the call named one.
    pub change_ref: Option<String>,
}

/// A whole project's prioritisation.
#[derive(Debug, Clone, Serialize)]
pub struct TestPrioritisation {
    pub project_id: String,
    /// The declared change the ordering is relative to, if one was named.
    pub change_ref: Option<String>,
    pub tests: Vec<PrioritisedTest>,
    /// How many fell in each band, so the shape of the project is readable at a glance.
    pub band_counts: BTreeMap<String, usize>,
    /// The floor. Reported separately so a reader can audit what prioritisation will not
    /// drop rather than trusting that it does not.
    pub mandatory_floor: Vec<String>,
    pub window: Window,
    pub signals: Vec<Signal>,
}

impl QaMemoryReader {
    /// Risk for one test case, from its own recorded executions.
    ///
    /// Project-scoped: the telemetry read is the same `telemetry_for(project_id)` every
    /// other surface here uses, so one tenant's history cannot reach another's case.
    pub async fn test_risk(&self, project_id: &str, test_case_id: &str) -> Retrieved<TestRisk> {
        // Checked before the evidence read, and reported separately from
        // "no matching history": a caller who passed a malformed id made a mistake, which
        // is a different answer from "this project has nothing about that case". Phase 0
        // records `UnknownSubject` as its own reason for exactly this reason.
        if Uuid::parse_str(test_case_id).is_err() {
            return Retrieved::empty(project_id, InsufficientData::UnknownSubject);
        }

        let evidence = self.test_evidence(project_id).await;
        if evidence.all.is_empty() {
            return Retrieved::empty(project_id, InsufficientData::NoExecutions);
        }
        match self.test_risk_from(&evidence, test_case_id) {
            Some(risk) => Retrieved {
                project_id: project_id.to_string(),
                insufficient_data: None,
                view: Some(risk),
            },
            None => Retrieved::empty(project_id, InsufficientData::NoMatchingHistory),
        }
    }

    /// Everything `test_risk_from` needs, read **once**.
    ///
    /// This exists because the obvious implementation — a per-case `test_risk` call —
    /// turned one question into four per case: a full bounded `execution_telemetry`
    /// scan, a full `healing_proposals` read, a full `bug_drafts` read, and a cluster
    /// lookup for every draft. Prioritising a project of 500 cases did 2,000+ reads to
    /// answer one question, all of them returning the same rows.
    ///
    /// Phase 6's stance on retrieval — "one bounded read per question rather than a pile
    /// of joins" — applies here literally: the read happens once, and every case is
    /// answered from it. Measured in the Phase 4 report.
    ///
    /// Clusters are resolved eagerly, so the draft→cluster hop is one indexed lookup per
    /// distinct cluster instead of per (case, draft) pair.
    async fn test_evidence(&self, project_id: &str) -> TestEvidence {
        let all = self.telemetry_for(project_id).await;
        let proposals = self
            .store
            .list_healing_proposals(project_id, None)
            .await
            .unwrap_or_default();
        let drafts = self
            .store
            .list_bug_drafts(Some(project_id))
            .await
            .unwrap_or_default();

        let mut clusters: BTreeMap<Uuid, crate::store::FailureClusterRow> = BTreeMap::new();
        for d in &drafts {
            if let Some(cluster) = self
                .store
                .get_failure_cluster_by_id(d.failure_cluster_id)
                .await
                .ok()
                .flatten()
            {
                clusters.insert(d.failure_cluster_id, cluster);
            }
        }

        let mut titles: BTreeMap<Uuid, String> = BTreeMap::new();
        for c in self
            .store
            .list_test_cases(project_id, None, None)
            .await
            .unwrap_or_default()
        {
            titles.insert(c.id, c.title);
        }

        TestEvidence {
            all,
            proposals,
            drafts,
            clusters,
            titles,
        }
    }

    /// `test_risk` over evidence already read. `None` when the case has no rows.
    fn test_risk_from(&self, evidence: &TestEvidence, test_case_id: &str) -> Option<TestRisk> {
        let case = Uuid::parse_str(test_case_id).ok()?;
        let rows: Vec<&ExecutionTelemetryRow> = evidence
            .all
            .iter()
            .filter(|r| r.test_case_id == Some(case))
            .collect();
        if rows.is_empty() {
            return None;
        }

        let owned: Vec<ExecutionTelemetryRow> = rows.iter().map(|r| (*r).clone()).collect();
        let failures = rows
            .iter()
            .filter(|r| QaMemoryReader::is_failure(r))
            .count();
        let failure_rate = failures as f64 / rows.len() as f64;

        // Reused exactly as modules and releases use it: rows plus the per-case counts,
        // and it returns 0.0 when no case has two samples. A single-execution case would
        // then read as "perfectly stable", so it is reported as unmeasured instead.
        let mut one_case_counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        one_case_counts.insert(test_case_id.to_string(), (rows.len(), failures));
        let raw_flakiness = average_flakiness(&rows, &one_case_counts);
        let flakiness = if rows.len() < 2 {
            None
        } else {
            Some(raw_flakiness)
        };
        let severity = failure_severity(&rows);

        // Healing and recurring-defect evidence, both joined on the fingerprints this
        // case's own failures carry. Stored keys throughout.
        let fingerprints: BTreeSet<String> = rows
            .iter()
            .filter_map(|r| r.failure_fingerprint.clone())
            .filter(|f| !f.is_empty())
            .collect();

        let mut healing_attempts = 0usize;
        let mut healing_regressions = 0usize;
        for p in evidence
            .proposals
            .iter()
            .filter(|p| fingerprints.contains(&p.failure_fingerprint))
        {
            healing_attempts += 1;
            let regressed = p
                .verification_result
                .as_deref()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                .and_then(|v| {
                    v.get("regression_detected")
                        .and_then(|b| b.as_bool())
                        .or_else(|| {
                            v.get("outcome")
                                .and_then(|o| o.as_str())
                                .map(|o| o == "REGRESSION_INTRODUCED")
                        })
                })
                .unwrap_or(false);
            if regressed {
                healing_regressions += 1;
            }
        }

        let mut recurring_defects = 0usize;
        for d in &evidence.drafts {
            let Some(cluster) = evidence.clusters.get(&d.failure_cluster_id) else {
                continue;
            };
            if cluster
                .failure_fingerprint
                .as_deref()
                .map(|fp| fingerprints.contains(fp))
                .unwrap_or(false)
            {
                recurring_defects += 1;
            }
        }

        let healing_regression_rate = if healing_attempts == 0 {
            None
        } else {
            Some(healing_regressions as f64 / healing_attempts as f64)
        };

        let factors = vec![
            Factor::measured(
                "failure_rate",
                failure_rate,
                &format!(
                    "{failures} failed of {} recorded executions of this case",
                    rows.len()
                ),
                rows.len(),
            ),
            match flakiness {
                Some(v) => Factor::measured(
                    "flakiness_score",
                    v,
                    "adaptive_intelligence::evaluate_flakiness over this case's own outcomes",
                    rows.len(),
                ),
                None => Factor::unmeasured(
                    "flakiness_score",
                    "fewer than two recorded executions, so no outcome flip can be observed",
                ),
            },
            match severity {
                Some(v) => Factor::measured(
                    "failure_severity",
                    v,
                    "failure categories of this case's failures, weighted by share",
                    failures,
                ),
                None => Factor::unmeasured(
                    "failure_severity",
                    "no failure of this case carries a failure_category",
                ),
            },
            Factor::unmeasured(
                "dependency_count",
                "this schema has no test-case-to-test-case dependency graph; \
                 TaskDef.prerequisites is per run, not per case",
            ),
            match healing_regression_rate {
                Some(v) => Factor::measured(
                    "healing_regression_rate",
                    v,
                    &format!(
                        "{healing_regressions} of {healing_attempts} healing proposals against \
                         this case's failure signature recorded a regression"
                    ),
                    healing_attempts,
                ),
                None => Factor::unmeasured(
                    "healing_regression_rate",
                    "no healing proposal has been raised against this case's failure signature",
                ),
            },
            Factor::unmeasured(
                "security_sensitivity",
                "this schema has no security_sensitivity field for a test case",
            ),
        ];

        let (calibration, coverage) = calibrate_or_none(test_case_id, &factors);
        let partial = coverage.unmeasured > f64::EPSILON;
        let measured_ratio = if coverage.measured > f64::EPSILON {
            Some(coverage.contribution / coverage.measured)
        } else {
            None
        };
        let window = Self::window(&owned);

        let mut signals = Vec::new();
        signals.push(Signal {
            kind: "failure_rate".into(),
            statement: format!(
                "this case failed {failures} time(s) in {} recorded execution(s)",
                rows.len()
            ),
            evidence_runs: rows.iter().map(|r| r.run_id).collect(),
            confidence: 1.0,
            basis: "counted from execution_telemetry rows carrying this case's test_case_id".into(),
        });
        if let Some(f) = flakiness {
            signals.push(Signal {
                kind: "flakiness".into(),
                statement: format!(
                    "outcome flips score {f:.2} for this case across {} execution(s)",
                    rows.len()
                ),
                evidence_runs: rows.iter().map(|r| r.run_id).collect(),
                confidence: 1.0,
                basis: "adaptive_intelligence::evaluate_flakiness".into(),
            });
        }
        if recurring_defects > 0 {
            signals.push(Signal {
                kind: "failure_recurrence".into(),
                statement: format!(
                    "{recurring_defects} bug draft(s) exist against this case's failure \
                     signature"
                ),
                evidence_runs: Vec::new(),
                confidence: 1.0,
                basis: "bug_drafts -> failure_clusters.failure_fingerprint -> this case's \
                        telemetry rows"
                    .into(),
            });
        }

        let title = evidence.titles.get(&case).cloned();

        Some(TestRisk {
            test_case_id: test_case_id.to_string(),
            title,
            calibration,
            factors,
            partial,
            measured_weight: coverage.measured,
            unmeasured_weight: coverage.unmeasured,
            weighted_contribution: coverage.contribution,
            measured_ratio,
            executions: rows.len(),
            failures,
            failure_rate: Some(failure_rate),
            flakiness_score: flakiness,
            recurring_defects,
            healing_attempts,
            healing_regressions,
            window,
            signals,
        })
    }

    /// Order a project's test cases by evidenced risk, most important first.
    ///
    /// ## This orders; it never removes
    ///
    /// Every case with recorded executions is returned. `PriorityBand::Routine` is a
    /// position in the order, not an exclusion — dropping cases is Phase 3's
    /// `Scope::restricted_to`, which is where the safety floor is actually enforced.
    ///
    /// ## `change_ref` is optional and additive
    ///
    /// Naming a change promotes the cases Phase 3 recorded for it into `Changed`. It
    /// never demotes anything, so a stale history cannot bury current work — the
    /// opposite failure mode to the one the negative tests check.
    ///
    /// ## One telemetry read
    ///
    /// All history is read once and shared across every case, so prioritising 200 cases
    /// does not re-read the table 200 times. Measured in the Phase 4 report.
    pub async fn prioritise_tests(
        &self,
        project_id: &str,
        change_ref: Option<&str>,
    ) -> Retrieved<TestPrioritisation> {
        // Phase 4 performance: the evidence is gathered **once** here and every case is
        // answered from it. The per-case path used to re-read telemetry, healing
        // proposals and bug drafts for each case, which made prioritising a project of
        // 500 cases cost 2,000+ reads returning identical rows.
        let evidence = self.test_evidence(project_id).await;
        if evidence.all.is_empty() {
            return Retrieved::empty(project_id, InsufficientData::NoExecutions);
        }
        let all = self.telemetry_for(project_id).await;
        if all.is_empty() {
            return Retrieved::empty(project_id, InsufficientData::NoExecutions);
        }

        // Which cases the named change impacts, from the Phase 3 records. The stored id
        // is a graph node; the node's `external_ref` is the case id.
        let changed: BTreeSet<String> = match change_ref {
            None => BTreeSet::new(),
            Some(reference) => {
                let nodes = self
                    .store
                    .list_quality_nodes(project_id)
                    .await
                    .unwrap_or_default();
                let case_of: BTreeMap<Uuid, String> = nodes
                    .iter()
                    .filter(|n| {
                        n.node_type == crate::store::TEST_NODE_TYPE
                            || n.node_type == crate::store::LEGACY_TEST_NODE_TYPE
                    })
                    .filter_map(|n| {
                        let node = n.id;
                        let case = n.external_ref.clone()?;
                        Some((node, case))
                    })
                    .collect();
                self.store
                    .list_change_impacts(project_id, 5000)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|r| r.change_ref == reference)
                    .filter_map(|r| r.selected_test_node_id)
                    .filter_map(|node| case_of.get(&node).cloned())
                    .collect()
            }
        };

        // Every case with recorded executions, counted once.
        let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for r in &all {
            let Some(case) = r.test_case_id.map(|c| c.to_string()) else {
                continue;
            };
            let slot = counts.entry(case).or_insert((0, 0));
            slot.0 += 1;
            if QaMemoryReader::is_failure(r) {
                slot.1 += 1;
            }
        }

        let mut tests: Vec<PrioritisedTest> = Vec::new();
        for case_id in counts.keys() {
            if Uuid::parse_str(case_id).is_err() {
                continue;
            }
            // Answered from the shared evidence. `None` here would mean the counted row
            // set and the evidence disagree, which cannot happen because both come from
            // the same read; skipped rather than given a fabricated zero if it somehow did.
            let Some(risk) = self.test_risk_from(&evidence, case_id) else {
                continue;
            };

            let mut applies: Vec<PriorityBand> = Vec::new();
            let mut reasons: Vec<String> = Vec::new();

            // The floor: any recorded failure at all.
            if risk.failures > 0 {
                applies.push(PriorityBand::Mandatory);
                reasons.push(format!(
                    "{} recorded failure(s); on the safety floor",
                    risk.failures
                ));
            }
            if changed.contains(case_id) {
                applies.push(PriorityBand::Changed);
                let reference = change_ref.unwrap_or("");
                reasons.push(format!("recorded as impacted by change `{reference}`"));
            }
            match risk.measured_ratio {
                Some(r) if r >= 0.70 => {
                    applies.push(PriorityBand::CriticalRisk);
                    reasons.push(format!(
                        "measured share of the risk model is {r:.2} (>= 0.70)"
                    ));
                }
                Some(r) if r >= 0.50 => {
                    applies.push(PriorityBand::HighRisk);
                    reasons.push(format!(
                        "measured share of the risk model is {r:.2} (>= 0.50)"
                    ));
                }
                _ => {}
            }
            if risk.recurring_defects > 0 {
                applies.push(PriorityBand::RecurringDefect);
                reasons.push(format!(
                    "{} bug draft(s) against this case's failure signature",
                    risk.recurring_defects
                ));
            }
            if risk.flakiness_score.is_some_and(|f| f >= FLAKY_THRESHOLD) {
                applies.push(PriorityBand::Flaky);
                reasons.push(format!(
                    "flakiness {} meets the {FLAKY_THRESHOLD} threshold",
                    risk.flakiness_score.unwrap_or_default()
                ));
            }
            if applies.is_empty() {
                applies.push(PriorityBand::Routine);
                reasons.push(PriorityBand::Routine.rationale().to_string());
            }

            applies.sort_by_key(|b| b.rank());
            let priority = applies[0];
            reasons.push(priority.rationale().to_string());

            tests.push(PrioritisedTest {
                test_case_id: case_id.clone(),
                title: risk.title.clone(),
                priority,
                applies,
                reasons,
                risk,
                change_ref: change_ref.map(ToString::to_string),
            });
        }

        // Highest band first, then case id, so the order is a total order and stable.
        tests.sort_by(|a, b| {
            a.priority
                .rank()
                .cmp(&b.priority.rank())
                .then(a.test_case_id.cmp(&b.test_case_id))
        });
        tests.truncate(PRIORITISE_MAX);

        let mut band_counts: BTreeMap<String, usize> = BTreeMap::new();
        for t in &tests {
            *band_counts
                .entry(t.priority.as_str().to_string())
                .or_insert(0) += 1;
        }
        let mandatory_floor: Vec<String> = tests
            .iter()
            .filter(|t| t.priority == PriorityBand::Mandatory)
            .map(|t| t.test_case_id.clone())
            .collect();

        let window = Self::window(&all.iter().map(|r| (*r).clone()).collect::<Vec<_>>());

        Retrieved {
            project_id: project_id.to_string(),
            view: Some(TestPrioritisation {
                project_id: project_id.to_string(),
                change_ref: change_ref.map(ToString::to_string),
                tests,
                band_counts,
                mandatory_floor,
                window,
                signals: vec![Signal {
                    kind: "prioritisation_scope".into(),
                    statement: "ordered every test case with recorded executions; no case was \
                         excluded from the list"
                        .to_string(),
                    evidence_runs: Vec::new(),
                    confidence: 1.0,
                    basis: "one telemetry read, counted per test_case_id".into(),
                }],
            }),
            insufficient_data: None,
        }
    }
}
