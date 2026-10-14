//! Phase 9 — Release Readiness Intelligence.
//!
//! The roadmap asks for *"a release/build quality recommendation grounded in actual test and
//! historical evidence"*, with an output of `GO` / `CONDITIONAL_GO` / `NO_GO` / `INSUFFICIENT_DATA`
//! plus contributing signals, blocked criteria, evidence references, affected requirements or
//! modules **where reliably linked**, data-freshness and missing-signal notes, and the rule version
//! used.
//!
//! ## No readiness score
//!
//! Rule 1 forbids using "one arbitrary 0–100 score as the whole decision". There is **no numeric
//! readiness score** in this module. The output is a status plus the evidence that produced it.
//! Where the existing model has a number (`release_risk`'s factors), it is carried through as a
//! *supporting* signal with its formula named — rule 6, risk estimates are supporting evidence,
//! never the decision.
//!
//! ## The rule order is the contract
//!
//! Rules are evaluated in a fixed order and the **first** that applies decides. That ordering is
//! the safety property: an evidence problem always outranks a verdict. A release whose required
//! evidence could not be read is `INSUFFICIENT_DATA`, never `GO` — rule 3's prohibition on
//! calling a release safe because tests passed when required tests were not run.
//!
//! ## No inferred release association
//!
//! `runs.release_id` is nullable and never auto-populated (migration 0029's own note: release
//! intelligence must answer `INSUFFICIENT_DATA` "rather than inventing an association"). So this
//! module never treats a project as having release evidence it lacks. Evidence that is genuinely
//! project-scoped is carried with [`EvidenceScope::Project`] and is never presented as if it were
//! about this release.
//!
//! ## D3 — `failure_severity` (OPEN)
//!
//! **This module never reads a `failure_severity` value.** Readiness is decided by deterministic
//! gates over outcomes, integrity results, and the mandatory floor — never by a severity number
//! whose meaning is unresolved. The `release_risk` factors are carried as supporting evidence with
//! their `measured`/`unmeasured` state intact, so a reader sees a partial model rather than a
//! silently-renormalised one.

use crate::qa_memory::QaMemoryReader;
use crate::qa_memory_risk::Factor;
use crate::store::Store;
use crate::test_quality::{assess_case, QualityInputs, QualityVerdict, TestCaseQuality};
use serde::Serialize;
use std::collections::BTreeSet;
use uuid::Uuid;

/// The recommendation states the roadmap defines (L324). No others are invented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReleaseRecommendation {
    /// Every evaluated gate passed on evidence that was actually read.
    Go,
    /// No gate blocked, but something left the evidence materially incomplete for a reason that
    /// is **not** a product defect -- currently only infrastructure error.
    ConditionalGo,
    /// At least one deterministic gate matched confirmed evidence.
    NoGo,
    /// A required piece of evidence was missing, unreadable, or does not exist in this schema.
    ///
    /// This is **not** a mild `GO`. Per rule 3, a release cannot be called safe because the tests
    /// that ran passed while the tests that did not run are unaccounted for.
    InsufficientData,
}

impl ReleaseRecommendation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Go => "GO",
            Self::ConditionalGo => "CONDITIONAL_GO",
            Self::NoGo => "NO_GO",
            Self::InsufficientData => "INSUFFICIENT_DATA",
        }
    }
}

/// How firmly an input can be tied to *this release*.
///
/// The distinction the brief demands: evidence that merely shares a project id is not evidence
/// about a release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceScope {
    /// Provably about this release: reached through `runs.release_id`.
    Release,
    /// About the project, not this release. Carried for context, never used to block.
    Project,
    /// About the whole deployment (executor availability).
    Deployment,
}

/// Which way a signal points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SignalDirection {
    /// Counts against readiness.
    Against,
    /// Counts for readiness.
    Supporting,
    /// Measured, but neither way -- e.g. a rate that exists and is mid-range.
    Neutral,
}

/// One roadmap signal, with its evidence and how firmly it is tied to the release.
#[derive(Debug, Clone, Serialize)]
pub struct ReadinessSignal {
    /// The roadmap's signal name (L309–319), or an explicit `evidence.` name for a local one.
    pub kind: String,
    pub direction: SignalDirection,
    pub scope: EvidenceScope,
    /// One sentence a human can act on.
    pub summary: String,
    /// The specific ids / counts the statement was read from.
    pub evidence: String,
}

/// A deterministic rule that produced a `NO_GO`.
#[derive(Debug, Clone, Serialize)]
pub struct BlockedCriterion {
    /// Stable rule id, also recorded in `rule_version` on the result.
    pub rule_id: &'static str,
    pub criterion: &'static str,
    pub why: String,
    pub evidence: String,
}

/// A roadmap signal that had no usable source, and why.
#[derive(Debug, Clone, Serialize)]
pub struct MissingSignal {
    pub kind: String,
    /// `NO_SOURCE_IN_SCHEMA` when the capability does not exist here; `NO_EVIDENCE` when the
    /// source exists but this release produced none.
    pub reason: &'static str,
    pub detail: String,
}

/// Per-case Phase 8 quality, carried whole. **Not aggregated.**
#[derive(Debug, Clone, Serialize)]
pub struct CaseQualityNote {
    pub test_case_id: String,
    /// Phase 8's own overall status. Carried verbatim, never recomputed or summed.
    pub overall: QualityVerdict,
    /// The dimensions with no evidence for this case, so a reader sees the gaps.
    pub unknown_dimensions: Vec<&'static str>,
    pub measured_dimensions: usize,
    pub weak_dimensions: Vec<&'static str>,
}

/// A release's readiness, with everything the decision was read from.
#[derive(Debug, Clone, Serialize)]
pub struct ReleaseReadiness {
    pub release_id: String,
    pub project_id: String,
    pub label: Option<String>,
    pub version: Option<String>,
    pub recommendation: ReleaseRecommendation,
    /// The rule set that decided this, so a verdict can be reproduced.
    pub rule_version: &'static str,
    pub signals: Vec<ReadinessSignal>,
    pub blocked_criteria: Vec<BlockedCriterion>,
    pub evidence_refs: Vec<String>,
    /// Only populated where a real link exists. Never guessed (rule 7).
    pub affected_requirements: Vec<String>,
    /// Only populated where a `covers` edge exists. Never guessed (rule 7).
    pub affected_modules: Vec<String>,
    /// The window `release_risk` actually considered, i.e. how fresh the evidence is.
    pub data_freshness: Option<(String, String)>,
    pub missing_signals: Vec<MissingSignal>,
    /// Phase 8 quality, per case. Informational; never a number, never a factor.
    pub case_quality: Vec<CaseQualityNote>,
    /// True when a required read failed. An assessment with `incomplete == true` must not be
    /// presented as complete, whatever its recommendation says.
    pub incomplete: bool,
}

/// The rule set this module implements. Recorded on every result so a verdict can be reproduced.
pub const RULE_VERSION: &str = "phase9-readiness-v1";

/// Hard ceiling on telemetry rows read per release. A release with more executions than this is
/// reported as truncated rather than silently summarised.
const MAX_TELEMETRY_ROWS: i64 = 5_000;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Assess one release's readiness.
///
/// `project_id` must be the caller's resolved tenant key. The release is looked up **within that
/// project** (`Store::list_releases` is project-scoped), so a release id from another project is
/// reported as unknown rather than answered from that project's data.
pub async fn assess_release(
    store: &Store,
    project_id: &str,
    release_id: Uuid,
    inputs: &QualityInputs<'_>,
) -> anyhow::Result<ReleaseReadiness> {
    let mut b = Builder::default();
    b.refs.push(format!("release:{release_id}"));

    // ---- R1: is this release even in this project? -------------------------------
    let releases = store.list_releases(project_id).await?;
    let Some(release) = releases.iter().find(|r| r.id == release_id) else {
        b.signals.push(ReadinessSignal {
            kind: "evidence.release_identity".into(),
            direction: SignalDirection::Against,
            scope: EvidenceScope::Release,
            summary: format!(
                "no release with this id exists in project {project_id}. Release ids are \
                 project-scoped, so another project's release is not answered from its data."
            ),
            evidence: format!("list_releases({project_id}) did not contain {release_id}"),
        });
        b.missing.push(MissingSignal {
            kind: "evidence.release_identity".into(),
            reason: "NO_EVIDENCE",
            detail: "the release id is not present in this project".into(),
        });
        return Ok(b.build(
            release_id,
            project_id,
            None,
            None,
            ReleaseRecommendation::InsufficientData,
        ));
    };
    let label = release.label.clone();
    let version = release.version.clone();

    // ---- R2: are there any runs at all for this release? -------------------------
    let run_ids = store.runs_for_release(release_id).await?;
    b.refs.push(format!("runs:{}", run_ids.len()));

    if run_ids.is_empty() {
        b.signals.push(ReadinessSignal {
            kind: "evidence.execution_history".into(),
            direction: SignalDirection::Against,
            scope: EvidenceScope::Release,
            summary:
                "no run is associated with this release. `runs.release_id` is nullable and is \
                 never auto-populated, so an unassociated release has no history to judge -- it \
                 is not a release that passed."
                    .to_string(),
            evidence: format!("runs_for_release({release_id}) = 0 runs"),
        });
        b.missing.push(MissingSignal {
            kind: "evidence.execution_history".into(),
            reason: "NO_EVIDENCE",
            detail: "no run carries this release_id".into(),
        });
        return Ok(b.build(
            release_id,
            project_id,
            label,
            version,
            ReleaseRecommendation::InsufficientData,
        ));
    }

    // ---- Release-scoped outcomes, read through the Phase 7.4 helpers --------------
    let telemetry = store
        .list_execution_telemetry(project_id, MAX_TELEMETRY_ROWS)
        .await?;
    let release_set: BTreeSet<Uuid> = run_ids.iter().copied().collect();
    let rows: Vec<&crate::store::ExecutionTelemetryRow> = telemetry
        .iter()
        .filter(|r| release_set.contains(&r.run_id))
        .collect();

    let completed = QaMemoryReader::completed_count(&rows);
    let failures = rows
        .iter()
        .filter(|r| QaMemoryReader::is_failure(r))
        .count();
    let infra = QaMemoryReader::infra_errors(&rows);
    let completed_rate = QaMemoryReader::completed_failure_rate_fraction(&rows);

    // Phase 8 quality, for the cases this release actually touched. Collected before the R3 gate
    // so even an evidence-less release reports what is known about its cases.
    let (notes, reqs, modules) = quality_for_release(store, project_id, &rows, inputs).await;
    b.quality = notes;
    b.requirements = reqs;
    b.modules = modules;

    // ---- R3: required evidence exists at all -------------------------------------
    // Roadmap rule 3: passing tests say nothing about tests that never ran. This gate stops that
    // mistake, and it outranks every verdict below it.
    if completed == 0 {
        b.signals.push(ReadinessSignal {
            kind: "evidence.execution_history".into(),
            direction: SignalDirection::Against,
            scope: EvidenceScope::Release,
            summary: format!(
                "{} run(s) are associated with this release but none produced a completed \
                 pass/fail outcome, so there is no test result to read. This is NOT a pass.",
                run_ids.len()
            ),
            evidence: format!(
                "recorded rows={}, completed(PASSED|FAILED|TIMEOUT)=0, infrastructure_errors={infra}",
                rows.len()
            ),
        });
        b.missing.push(MissingSignal {
            kind: "evidence.execution_history".into(),
            reason: "NO_EVIDENCE",
            detail: format!("{} telemetry row(s), none completed", rows.len()),
        });
        return Ok(b.build(
            release_id,
            project_id,
            label,
            version,
            ReleaseRecommendation::InsufficientData,
        ));
    }

    // ---- Supporting evidence: outcomes and infrastructure --------------------------
    b.signals.push(ReadinessSignal {
        kind: "release.completed_failure_rate".into(),
        direction: if failures == 0 {
            SignalDirection::Supporting
        } else {
            SignalDirection::Against
        },
        scope: EvidenceScope::Release,
        summary: if failures == 0 {
            format!("every one of {completed} completed executions of this release passed")
        } else {
            format!("{failures} of {completed} completed executions failed")
        },
        evidence: format!(
            "completed_failure_rate_fraction={completed_rate:?} (0..1), failures={failures}, \
             completed={completed}, recorded rows={}",
            rows.len()
        ),
    });

    if infra > 0 {
        b.signals.push(ReadinessSignal {
            kind: "infrastructure.errors".into(),
            // Infrastructure trouble is neither a pass nor a product defect.
            direction: SignalDirection::Neutral,
            scope: EvidenceScope::Release,
            summary: format!(
                "{infra} infrastructure error(s) occurred in this release. They are counted in \
                 neither the completed rate's numerator nor its denominator, so product \
                 reliability below is unaffected by them."
            ),
            evidence: format!("infra_errors={infra}"),
        });
    }

    // ---- R5: the mandatory floor ---------------------------------------------------
    let (gaps, floor_size, floor_note) = mandatory_floor_gaps(store, project_id, &rows).await;
    if floor_size == 0 {
        b.missing.push(MissingSignal {
            kind: "regression.mandatory_floor".into(),
            reason: "NO_SOURCE_IN_SCHEMA",
            detail: floor_note,
        });
    }
    if !gaps.is_empty() {
        b.blocked.push(BlockedCriterion {
            rule_id: "R5",
            criterion: "a mandatory-floor test has no completed outcome in this release",
            why: "rule 3/4: a required test that produced no verdict is not ready. It did not \
                  pass anything -- it did not run."
                .to_string(),
            evidence: format!(
                "{} mandatory-floor test(s) with no completed outcome: {gaps:?} (floor derived \
                 from {floor_size} recorded-failure test(s) in this project)",
                gaps.len()
            ),
        });
        b.signals.push(ReadinessSignal {
            kind: "regression.mandatory_floor".into(),
            direction: SignalDirection::Against,
            scope: EvidenceScope::Project,
            summary: format!(
                "{} mandatory-floor test(s) have no completed outcome in this release",
                gaps.len()
            ),
            evidence: format!("{gaps:?}"),
        });
    }

    // ---- R6: confirmed product failures --------------------------------------------
    if failures > 0 {
        b.blocked.push(BlockedCriterion {
            rule_id: "R6",
            criterion: "this release has a confirmed product-test failure",
            why: "rule 4: a completed FAILED or TIMEOUT outcome is a verdict the product \
                  reached. It is not an infrastructure problem."
                .to_string(),
            evidence: format!(
                "{failures} FAILED/TIMEOUT of {completed} completed executions; \
                 completed_failure_rate_fraction={completed_rate:?}"
            ),
        });
        b.signals.push(ReadinessSignal {
            kind: "release.product_failures".into(),
            direction: SignalDirection::Against,
            scope: EvidenceScope::Release,
            summary: format!("{failures} confirmed product-test failure(s) in this release"),
            evidence: format!("failures={failures} completed={completed}"),
        });
    }

    // ---- R7: data integrity, per run of this release -------------------------------
    let (defective_runs, incomplete_runs) =
        integrity_for_release(store, project_id, &run_ids).await;
    if defective_runs.is_empty() {
        b.missing.push(MissingSignal {
            kind: "integrity.findings".into(),
            reason: "NO_EVIDENCE",
            detail: format!(
                "{} run(s) verified with no established defect",
                run_ids.len()
            ),
        });
    }
    for run in &defective_runs {
        b.blocked.push(BlockedCriterion {
            rule_id: "R7",
            criterion: "a data-integrity comparison established a defect on a run of this release",
            why: "an established inconsistency is a confirmed cross-layer defect, not an estimate."
                .to_string(),
            evidence: format!("run {run} reported an established integrity defect"),
        });
    }
    if incomplete_runs > 0 {
        b.incomplete = true;
        b.missing.push(MissingSignal {
            kind: "integrity.findings".into(),
            reason: "NO_EVIDENCE",
            detail: format!(
                "{incomplete_runs} run(s) could not be fully verified -- a required read failed. \
                 This assessment is NOT complete."
            ),
        });
    }

    // ---- Supporting: the existing release-risk model, carried with its own honesty ---
    match QaMemoryReader::new(store.clone())
        .release_risk(project_id, release_id)
        .await
    {
        Ok(retrieved) => match &retrieved.view {
            Some(v) => {
                let measured: Vec<&str> = v
                    .factors
                    .iter()
                    .filter(|f| f.measured.is_some())
                    .map(|f| f.name.as_str())
                    .collect();
                let unmeasured: Vec<&str> = v
                    .factors
                    .iter()
                    .filter(|f| f.measured.is_none())
                    .map(|f| f.name.as_str())
                    .collect();
                b.signals.push(ReadinessSignal {
                    kind: "risk.release_model".into(),
                    direction: SignalDirection::Neutral,
                    scope: EvidenceScope::Release,
                    summary: format!(
                        "the existing release-risk model backs {}/{} of its weight here \
                         ({:.2} measured). Supporting evidence only -- it is not the decision \
                         (rule 6), and it reads no `failure_severity`.",
                        measured.len(),
                        measured.len() + unmeasured.len(),
                        v.measured_weight
                    ),
                    evidence: format!(
                        "measured={measured:?}; unmeasured={unmeasured:?}; calibration={}",
                        v.calibration
                            .as_ref()
                            .map(|c| format!("{} ({})", c.risk_score, c.risk_level))
                            .unwrap_or_else(|| "None (the model refused a score)".into())
                    ),
                });
                match &v.calibration {
                    Some(c) => b.signals.push(ReadinessSignal {
                        kind: "risk.release_score".into(),
                        direction: SignalDirection::Neutral,
                        scope: EvidenceScope::Release,
                        summary: format!(
                            "the release-risk model returned {} ({}). Rule 2: formula is \
                             `calibrate_risk`, weights unchanged.",
                            c.risk_score, c.risk_level
                        ),
                        evidence: c.explanation.clone(),
                    }),
                    None => b.missing.push(MissingSignal {
                        kind: "risk.release_score".into(),
                        reason: "NO_EVIDENCE",
                        detail: "the model refused a score; some factors are unmeasured".into(),
                    }),
                }

                // Historical comparison, only where real points exist.
                let historical: Vec<&crate::qa_memory_risk::ReleaseTrendPoint> = v
                    .trend_points
                    .iter()
                    .filter(|p| p.release_id != release_id.to_string())
                    .collect();
                if historical.is_empty() {
                    b.missing.push(MissingSignal {
                        kind: "historical.release_comparison".into(),
                        reason: "NO_EVIDENCE",
                        detail: "no other release has recorded executions".into(),
                    });
                } else {
                    b.signals.push(ReadinessSignal {
                        kind: "historical.release_comparison".into(),
                        direction: SignalDirection::Neutral,
                        scope: EvidenceScope::Release,
                        summary: format!(
                            "{} earlier release(s) have recorded executions to compare against",
                            historical.len()
                        ),
                        evidence: historical
                            .iter()
                            .map(|p| {
                                format!(
                                    "{}: {}% failed",
                                    p.release_id,
                                    p.failure_rate.unwrap_or(0.0)
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(", "),
                    });
                }
                // Freshness, only from a window that actually exists.
                match (v.window.from, v.window.to) {
                    (Some(f), Some(t)) => {
                        b.freshness = Some((f.to_rfc3339(), t.to_rfc3339()))
                    }
                    _ => b.missing.push(MissingSignal {
                        kind: "evidence.data_freshness".into(),
                        reason: "NO_EVIDENCE",
                        detail: "the release-risk window has no bounds, so no freshness can be                                  stated"
                            .into(),
                    }),
                }
                b.refs.push(format!("release_risk:{release_id}"));
            }
            None => b.missing.push(MissingSignal {
                kind: "risk.release_model".into(),
                reason: "NO_EVIDENCE",
                detail: format!(
                    "release_risk returned no view (insufficient_data={:?})",
                    retrieved.insufficient_data
                ),
            }),
        },
        Err(e) => {
            b.incomplete = true;
            b.missing.push(MissingSignal {
                kind: "risk.release_model".into(),
                reason: "NO_EVIDENCE",
                detail: format!("release_risk could not be read: {e}"),
            });
        }
    }

    // ---- Roadmap signals this schema cannot release-scope --------------------------
    // Carried as missing, never invented (rule 7) and never used to block.
    for (kind, detail) in [
        (
            "defects.critical_high",
            "bug_drafts carry no release_id, so defect severity cannot be attributed to a release",
        ),
        (
            "failure.new_clusters",
            "failure_clusters carry no release_id; a cluster is project-level",
        ),
        (
            "change.impact_scope",
            "change_impacts are keyed to quality_nodes, not to releases",
        ),
        (
            "exploration.findings",
            "exploration_sessions carry no release_id",
        ),
        (
            "journey.outcomes",
            "journeys are executable tests, not persisted release-scoped records",
        ),
    ] {
        b.missing.push(MissingSignal {
            kind: kind.into(),
            reason: "NO_SOURCE_IN_SCHEMA",
            detail: detail.into(),
        });
    }

    let recommendation = decide(&b.blocked, infra, failures);
    Ok(b.build(release_id, project_id, label, version, recommendation))
}

// ---------------------------------------------------------------------------
// The decision
// ---------------------------------------------------------------------------

/// Rule order. Evidence problems were already handled upstream and returned early; what remains
/// is: any confirmed blocker beats everything, then infrastructure, then a clean pass.
fn decide(blocked: &[BlockedCriterion], infra: usize, failures: usize) -> ReleaseRecommendation {
    if !blocked.is_empty() {
        return ReleaseRecommendation::NoGo;
    }
    if infra > 0 && failures == 0 {
        // No product defect, but the evidence is materially incomplete for a reason that is
        // not the product's fault. Conditional, not GO.
        return ReleaseRecommendation::ConditionalGo;
    }
    ReleaseRecommendation::Go
}

/// Accumulates a result. Built once per assessment so no call site can pass the wrong field in
/// the wrong position -- the first draft used positional `finish(...)` helpers and every call site
/// had to be arity-matched by hand, which is exactly the kind of thing that silently swaps a
/// signal list for a blocked-criteria list.
#[derive(Default)]
struct Builder {
    signals: Vec<ReadinessSignal>,
    blocked: Vec<BlockedCriterion>,
    missing: Vec<MissingSignal>,
    refs: Vec<String>,
    quality: Vec<CaseQualityNote>,
    requirements: Vec<String>,
    modules: Vec<String>,
    freshness: Option<(String, String)>,
    incomplete: bool,
}

impl Builder {
    fn build(
        self,
        release_id: Uuid,
        project_id: &str,
        label: Option<String>,
        version: Option<String>,
        recommendation: ReleaseRecommendation,
    ) -> ReleaseReadiness {
        ReleaseReadiness {
            release_id: release_id.to_string(),
            project_id: project_id.to_string(),
            label,
            version,
            recommendation,
            rule_version: RULE_VERSION,
            signals: self.signals,
            blocked_criteria: self.blocked,
            evidence_refs: self.refs,
            // Only populated where a real link exists. Never guessed (rule 7).
            affected_requirements: self.requirements,
            affected_modules: self.modules,
            data_freshness: self.freshness,
            missing_signals: self.missing,
            case_quality: self.quality,
            incomplete: self.incomplete,
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 8 quality, carried whole
// ---------------------------------------------------------------------------

/// Assess every test case that actually appears in this release's telemetry.
///
/// Phase 8's assessment is reused as-is. It is **not** reimplemented, **not** aggregated into a
/// number, and **not** turned into a risk factor -- each case keeps its own `overall` status and
/// the list of dimensions with no evidence, which is the honest way to show the gaps.
async fn quality_for_release(
    store: &Store,
    project_id: &str,
    rows: &[&crate::store::ExecutionTelemetryRow],
    inputs: &QualityInputs<'_>,
) -> (Vec<CaseQualityNote>, Vec<String>, Vec<String>) {
    let case_ids: BTreeSet<Uuid> = rows.iter().filter_map(|r| r.test_case_id).collect();
    let mut notes = Vec::new();
    let mut requirements: BTreeSet<String> = BTreeSet::new();
    let mut modules: BTreeSet<String> = BTreeSet::new();

    for case_id in case_ids {
        let Ok(Some(row)) = store.get_test_case(case_id).await else {
            continue;
        };
        let q: TestCaseQuality = assess_case(store, &row, project_id, inputs).await;

        // Affected requirements / modules ONLY where a real link exists (rule 7).
        if let Some(r) = &row.requirement_reference {
            requirements.insert(r.clone());
        }
        if let Some(r) = &row.source_requirement {
            requirements.insert(r.clone());
        }
        modules.extend(linked_modules(store, project_id, case_id).await);

        notes.push(CaseQualityNote {
            test_case_id: case_id.to_string(),
            overall: q.overall(),
            unknown_dimensions: q.unknown_keys(),
            measured_dimensions: q.measured_count(),
            weak_dimensions: q
                .dimensions
                .iter()
                .filter(|d| d.verdict == QualityVerdict::Weak)
                .map(|d| d.key)
                .collect(),
        });
    }
    (
        notes,
        requirements.into_iter().collect(),
        modules.into_iter().collect(),
    )
}

/// Modules reachable from a case through a real `covers` edge. Never inferred from a name.
async fn linked_modules(store: &Store, project_id: &str, case_id: Uuid) -> Vec<String> {
    let Ok(nodes) = store.list_quality_nodes(project_id).await else {
        return Vec::new();
    };
    let Some(node) = nodes.iter().find(|n| {
        n.node_type == crate::store::TEST_NODE_TYPE
            && n.external_ref.as_deref() == Some(case_id.to_string().as_str())
    }) else {
        return Vec::new();
    };
    let Ok(relations) = store.list_quality_relations_for_project(project_id).await else {
        return Vec::new();
    };
    let Ok(all_nodes) = store.list_quality_nodes(project_id).await else {
        return Vec::new();
    };
    relations
        .iter()
        .filter(|r| r.relation_type == "covers" && r.from_node_id == node.id)
        .filter_map(|r| {
            all_nodes
                .iter()
                .find(|n| n.id == r.to_node_id && n.node_type == "module")
                .map(|n| n.external_ref.clone().unwrap_or_else(|| n.name.clone()))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The mandatory floor (derived, not declared)
// ---------------------------------------------------------------------------

/// Which derived-mandatory-floor tests have no completed outcome in this release's rows.
///
/// The floor is derived because there is **no declared mandatory flag in this schema**. The
/// derivation is the existing one `qa_memory_risk` already uses: a test with recorded failure
/// evidence. It is stated as a derivation here too, rather than presented as user policy.
async fn mandatory_floor_gaps(
    store: &Store,
    project_id: &str,
    rows: &[&crate::store::ExecutionTelemetryRow],
) -> (Vec<String>, usize, String) {
    // Every case that has *any* recorded failure in this project is on the derived floor.
    let Ok(all) = store
        .list_execution_telemetry(project_id, MAX_TELEMETRY_ROWS)
        .await
    else {
        return (
            Vec::new(),
            0,
            "the floor could not be derived: telemetry read failed".into(),
        );
    };
    let mut failed_cases: BTreeSet<Uuid> = BTreeSet::new();
    for r in all.iter() {
        if QaMemoryReader::is_failure(r) {
            if let Some(c) = r.test_case_id {
                failed_cases.insert(c);
            }
        }
    }
    if failed_cases.is_empty() {
        return (
            Vec::new(),
            0,
            "no test case in this project has recorded failure evidence, so the derived floor is \
             empty. There is NO declared mandatory-test flag anywhere in this schema, so 'the \
             required tests' cannot be read as policy -- only as this derivation."
                .into(),
        );
    }

    let mut completed_cases: BTreeSet<Uuid> = BTreeSet::new();
    for r in rows {
        if QaMemoryReader::is_pass(r) || QaMemoryReader::is_failure(r) {
            if let Some(c) = r.test_case_id {
                completed_cases.insert(c);
            }
        }
    }

    let gaps: Vec<String> = failed_cases
        .iter()
        .filter(|c| !completed_cases.contains(c))
        .map(|c| c.to_string())
        .collect();

    let note = format!(
        "derived floor of {} test(s) from recorded failure evidence; there is no declared \
         mandatory flag in this schema",
        failed_cases.len()
    );
    (gaps, failed_cases.len(), note)
}

// ---------------------------------------------------------------------------
// Data integrity, per run of this release
// ---------------------------------------------------------------------------

/// Verify each of the release's own runs. Returns `(runs with an established defect, runs that
/// could not be fully verified)`.
///
/// `integrity::verify_run` is run-scoped, so composing it over the release's runs is what makes
/// the result genuinely release-scoped rather than inferred.
async fn integrity_for_release(
    store: &Store,
    project_id: &str,
    run_ids: &[Uuid],
) -> (Vec<Uuid>, usize) {
    let mut defective = Vec::new();
    let mut incomplete = 0usize;
    for run in run_ids {
        let report = crate::integrity::IntegrityVerifier::new(store)
            .verify_run(project_id, *run, None)
            .await;
        match report {
            Ok(r) => {
                // Phase 7.1's own contract: a report with read failures is not complete, and must
                // not be presented as if it were.
                if !r.is_complete() {
                    incomplete += 1;
                }
                if r.comparisons.iter().any(|c| c.establishes_defect()) {
                    defective.push(*run);
                }
            }
            Err(_) => incomplete += 1,
        }
    }
    (defective, incomplete)
}

/// Exposed so a caller can render the factor table without re-deriving it.
pub fn factor_lines(factors: &[Factor]) -> Vec<String> {
    factors
        .iter()
        .map(|f| {
            format!(
                "{}: weight={} measured={:?} evidence={}",
                f.name, f.weight, f.measured, f.evidence
            )
        })
        .collect()
}
