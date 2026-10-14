//! AIQA Phase 10 (Batch 2) — QA Copilot: Project-Scoped Evidence Retrieval Engine.
//!
//! # Roadmap Reference
//! `AIQA_NEXT_ROADMAP_PHASED_PLAN.md` §"PHASE 10 — AI QA COPILOT" (L369–431)
//!
//! ## Core Objective
//! Build a project-scoped, bounded, typed evidence retrieval coordinator that consumes
//! the parsed `CopilotIntent` and `CopilotDispatchPlan` from Batch 1, invoking ONLY the
//! specific existing evidence readers needed for each query.
//!
//! ## Strict Safety & Governance Boundaries
//! 1. **Zero Memory Database:** No parallel database, no vector store, no cache tables.
//! 2. **Zero Schema Migrations:** Strictly read-only queries; 0 new tables or columns.
//! 3. **Zero Dynamic/Arbitrary SQL:** Parameterized readers only (`?` binds).
//! 4. **Strict Multi-Tenant Scoping:** Every reader boundary filters explicitly by `project_id`.
//!    Cross-project or cross-tenant records are never exposed.
//! 5. **Zero External AI / Model Calls:** No LLM calls, no synthesis, no prompt construction.
//! 6. **Bounded Result Sizes:** Hard ceilings on all collections prevent unbounded memory growth.
//! 7. **Honest `InsufficientData`:** Missing or unmeasured evidence produces structured
//!    `InsufficientDataInfo` rather than fabricated values or disguised empty successes.
//! 8. **Error Propagation:** Reader/database failures propagate as `Err` rather than
//!    being masked as empty results.

use crate::change_impact::{ChangeImpactReader, ChangeTarget, ImpactedTest};
use crate::copilot_intent::{
    ClassificationResult, CopilotDispatchPlan, CopilotIntent, EvidenceReaderKind,
};
use crate::integrity::{IntegrityReport, IntegrityVerifier};
use crate::qa_memory::QaMemoryReader;
use crate::qa_memory_risk::{TestPrioritisation, TestRisk};
use crate::release_readiness::{assess_release, ReleaseReadiness, ReleaseRecommendation};
use crate::store::{ChangeImpactRow, FailureClusterRow, Store};
use crate::test_quality::{assess_case, QualityInputs, QualityVerdict, TestCaseQuality};
use serde::Serialize;
use uuid::Uuid;

/// Hard ceilings on retrieved result collections to prevent unbounded memory growth.
pub const MAX_FAILURES_RETRIEVED: usize = 50;
pub const MAX_TESTS_RETRIEVED: usize = 100;
pub const MAX_REQUIREMENTS_RETRIEVED: usize = 100;
pub const MAX_IMPACTS_RETRIEVED: usize = 50;
pub const MAX_QUALITY_CASES_RETRIEVED: usize = 50;

/// Status of the evidence retrieval operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RetrievalStatus {
    /// Required evidence was retrieved completely from stored records.
    Complete,
    /// Evidence was partially available, but some signals or factors were unmeasured.
    Partial,
    /// Required evidence was missing, unmeasured, or empty.
    InsufficientData,
    /// Retrieval was skipped (e.g. classification required clarification or was unsupported).
    Skipped,
}

/// Explicit structured explanation of why evidence was insufficient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InsufficientDataInfo {
    pub reason: String,
    pub missing_entity: Option<String>,
    pub suggested_action: Option<String>,
}

/// Provenance trace recording exactly which reader, table, and entity produced each fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceProvenance {
    pub reader: EvidenceReaderKind,
    pub project_id: String,
    pub entity_type: &'static str,
    pub entity_id: Option<String>,
    pub source: &'static str,
}

/// A complete, typed container for retrieved evidence and its provenance.
#[derive(Debug, Clone, Serialize)]
pub struct RetrievedEvidence {
    pub intent: Option<CopilotIntent>,
    pub project_id: String,
    pub status: RetrievalStatus,
    pub provenance: Vec<EvidenceProvenance>,
    pub data: Option<RetrievedEvidenceData>,
    pub insufficient_data: Option<InsufficientDataInfo>,
    pub record_count: usize,
    pub is_truncated: bool,
}

/// The typed payload of retrieved evidence, segregated by intent kind.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RetrievedEvidenceData {
    ReleaseReadiness(Box<ReleaseReadinessEvidence>),
    TestRecommendations(Box<TestRecommendationsEvidence>),
    RegressionImpact(Box<RegressionImpactEvidence>),
    TestRisk(Box<TestRiskEvidence>),
    FailureClusters(Vec<FailureClusterRow>),
    RequirementCoverage(Box<RequirementCoverageEvidence>),
    InsufficientEvidenceInvestigation(Box<InsufficientEvidenceInvestigationEvidence>),
}

/// Evidence retrieved for ReleaseFailureReason and ReleaseReadinessBlockers intents.
#[derive(Debug, Clone, Serialize)]
pub struct ReleaseReadinessEvidence {
    pub release_id: Uuid,
    pub project_id: String,
    pub label: Option<String>,
    pub version: Option<String>,
    pub recommendation: ReleaseRecommendation,
    pub readiness: ReleaseReadiness,
}

/// Evidence retrieved for NextTestRecommendation intent.
#[derive(Debug, Clone, Serialize)]
pub struct TestRecommendationsEvidence {
    pub project_id: String,
    pub prioritisation: Option<TestPrioritisation>,
    pub safety_floor_test_ids: Vec<Uuid>,
    pub safety_floor_count: usize,
    pub high_risk_test_count: usize,
}

/// Evidence retrieved for RegressionImpact intent.
#[derive(Debug, Clone, Serialize)]
pub struct RegressionImpactEvidence {
    pub project_id: String,
    pub change_ref: Option<String>,
    pub recorded_impacts: Vec<ChangeImpactRow>,
    pub direct_impacts: Vec<ImpactedTest>,
    pub safety_floor_test_ids: Vec<Uuid>,
}

/// Evidence retrieved for TestRiskExplanation intent.
#[derive(Debug, Clone, Serialize)]
pub struct TestRiskEvidence {
    pub project_id: String,
    pub test_case_id: Option<String>,
    pub test_title: Option<String>,
    pub risk_view: Option<TestRisk>,
    pub quality: Option<TestCaseQuality>,
    pub project_high_risk_cases: Vec<(Uuid, f64)>,
}

/// One requirement item with its linked test cases.
#[derive(Debug, Clone, Serialize)]
pub struct RequirementCoverageItem {
    pub key: String,
    pub document_id: Uuid,
    pub section: Option<String>,
    pub text: String,
    pub test_case_count: usize,
    pub test_case_ids: Vec<Uuid>,
}

/// Evidence retrieved for RequirementCoverageGaps intent.
#[derive(Debug, Clone, Serialize)]
pub struct RequirementCoverageEvidence {
    pub project_id: String,
    pub total_requirements: usize,
    pub uncovered_count: usize,
    pub covered_count: usize,
    pub uncovered_requirements: Vec<RequirementCoverageItem>,
    pub covered_requirements: Vec<RequirementCoverageItem>,
}

/// Summary of a test case evaluated for quality gaps.
#[derive(Debug, Clone, Serialize)]
pub struct TestCaseQualitySummary {
    pub test_case_id: String,
    pub title: Option<String>,
    pub overall_verdict: QualityVerdict,
    pub unknown_dimensions: Vec<&'static str>,
    pub weak_dimensions: Vec<&'static str>,
}

/// Evidence retrieved for InsufficientEvidenceReason intent.
#[derive(Debug, Clone, Serialize)]
pub struct InsufficientEvidenceInvestigationEvidence {
    pub project_id: String,
    pub run_id: Option<Uuid>,
    pub integrity_report: Option<IntegrityReport>,
    pub insufficient_quality_cases: Vec<TestCaseQualitySummary>,
}

/// Validates that a project identifier does not contain malicious injection patterns.
fn validate_project_id(pid: &str) -> anyhow::Result<&str> {
    let trimmed = pid.trim();
    if trimmed.is_empty() {
        anyhow::bail!("project_id cannot be empty or whitespace only");
    }
    if trimmed.contains('\'')
        || trimmed.contains('"')
        || trimmed.contains(';')
        || trimmed.contains("/*")
        || trimmed.contains("--")
    {
        anyhow::bail!("project_id contains invalid or disallowed characters");
    }
    Ok(trimmed)
}

/// Coordinator for project-scoped evidence retrieval.
///
/// Dispatches intent-specific queries strictly over existing readers and models without
/// introducing any new tables, duplicate stores, or external model calls.
#[derive(Clone)]
pub struct CopilotEvidenceCoordinator {
    store: Store,
    executor_kinds: Vec<String>,
}

impl CopilotEvidenceCoordinator {
    /// Creates a new evidence coordinator with default executor kinds.
    pub fn new(store: Store) -> Self {
        Self {
            store,
            executor_kinds: vec![
                "manual".to_string(),
                "automated".to_string(),
                "simulated".to_string(),
            ],
        }
    }

    /// Creates an evidence coordinator with specific executor kinds for quality checks.
    pub fn with_executor_kinds(store: Store, executor_kinds: Vec<String>) -> Self {
        Self {
            store,
            executor_kinds,
        }
    }

    /// Dispatches evidence retrieval directly from a `ClassificationResult`.
    ///
    /// If the classification result is `ClarificationRequired` or `UnsupportedQuery`,
    /// evidence retrieval is completely skipped (zero reader invocations, zero DB queries).
    pub async fn retrieve_for_classification(
        &self,
        project_id: &str,
        classification: &ClassificationResult,
    ) -> anyhow::Result<RetrievedEvidence> {
        let valid_pid = validate_project_id(project_id)?;

        match classification {
            ClassificationResult::Classified { intent, .. } => {
                let plan = intent.dispatch_plan();
                self.retrieve(valid_pid, &plan).await
            }
            ClassificationResult::ClarificationRequired {
                reason,
                suggested_clarification,
                ..
            } => Ok(RetrievedEvidence {
                intent: None,
                project_id: valid_pid.to_string(),
                status: RetrievalStatus::Skipped,
                provenance: Vec::new(),
                data: None,
                insufficient_data: Some(InsufficientDataInfo {
                    reason: format!("Classification requires clarification: {reason}"),
                    missing_entity: None,
                    suggested_action: Some(suggested_clarification.clone()),
                }),
                record_count: 0,
                is_truncated: false,
            }),
            ClassificationResult::UnsupportedQuery { reason, .. } => Ok(RetrievedEvidence {
                intent: None,
                project_id: valid_pid.to_string(),
                status: RetrievalStatus::Skipped,
                provenance: Vec::new(),
                data: None,
                insufficient_data: Some(InsufficientDataInfo {
                    reason: format!("Classification unsupported: {reason}"),
                    missing_entity: None,
                    suggested_action: Some(
                        "Ask one of the eight canonical QA Copilot questions.".to_string(),
                    ),
                }),
                record_count: 0,
                is_truncated: false,
            }),
        }
    }

    /// Executes evidence retrieval for a given `CopilotDispatchPlan`.
    pub async fn retrieve(
        &self,
        project_id: &str,
        plan: &CopilotDispatchPlan,
    ) -> anyhow::Result<RetrievedEvidence> {
        let valid_pid = validate_project_id(project_id)?;

        match &plan.intent {
            CopilotIntent::ReleaseFailureReason { release_id }
            | CopilotIntent::ReleaseReadinessBlockers { release_id } => {
                self.retrieve_release_readiness(valid_pid, &plan.intent, release_id.as_deref())
                    .await
            }
            CopilotIntent::NextTestRecommendation => {
                self.retrieve_test_recommendations(valid_pid, &plan.intent)
                    .await
            }
            CopilotIntent::RegressionImpact { change_ref } => {
                self.retrieve_regression_impact(valid_pid, &plan.intent, change_ref.as_deref())
                    .await
            }
            CopilotIntent::TestRiskExplanation { test_case_id } => {
                self.retrieve_test_risk(valid_pid, &plan.intent, test_case_id.as_deref())
                    .await
            }
            CopilotIntent::RecurringFailures => {
                self.retrieve_recurring_failures(valid_pid, &plan.intent)
                    .await
            }
            CopilotIntent::RequirementCoverageGaps => {
                self.retrieve_requirement_coverage(valid_pid, &plan.intent)
                    .await
            }
            CopilotIntent::InsufficientEvidenceReason { finding_id, run_id } => {
                self.retrieve_insufficient_evidence_reason(
                    valid_pid,
                    &plan.intent,
                    finding_id.as_deref(),
                    run_id.as_deref(),
                )
                .await
            }
        }
    }

    // ------------------------------------------------------------------------
    // Intent-Specific Retrieval Implementations
    // ------------------------------------------------------------------------

    async fn retrieve_release_readiness(
        &self,
        project_id: &str,
        intent: &CopilotIntent,
        release_ref: Option<&str>,
    ) -> anyhow::Result<RetrievedEvidence> {
        let mut provenance = Vec::new();
        let releases = self.store.list_releases(project_id).await?;
        provenance.push(EvidenceProvenance {
            reader: EvidenceReaderKind::ReleaseReadiness,
            project_id: project_id.to_string(),
            entity_type: "releases",
            entity_id: None,
            source: "store::list_releases",
        });

        let target_release = match release_ref {
            Some(ref_str) => {
                let by_uuid = Uuid::parse_str(ref_str).ok();
                let found = releases.iter().find(|r| {
                    if let Some(uid) = by_uuid {
                        if r.id == uid {
                            return true;
                        }
                    }
                    r.version.as_deref() == Some(ref_str) || r.label.as_deref() == Some(ref_str)
                });
                match found {
                    Some(r) => Some(r.clone()),
                    None => {
                        return Ok(RetrievedEvidence {
                            intent: Some(intent.clone()),
                            project_id: project_id.to_string(),
                            status: RetrievalStatus::InsufficientData,
                            provenance,
                            data: None,
                            insufficient_data: Some(InsufficientDataInfo {
                                reason: format!(
                                    "No release matching '{ref_str}' exists in project '{project_id}'. \
                                     Release evidence is project-scoped."
                                ),
                                missing_entity: Some(ref_str.to_string()),
                                suggested_action: Some(
                                    "Verify the release UUID, tag, or version exists within this project.".to_string(),
                                ),
                            }),
                            record_count: 0,
                            is_truncated: false,
                        });
                    }
                }
            }
            None => releases.first().cloned(),
        };

        let Some(release) = target_release else {
            return Ok(RetrievedEvidence {
                intent: Some(intent.clone()),
                project_id: project_id.to_string(),
                status: RetrievalStatus::InsufficientData,
                provenance,
                data: None,
                insufficient_data: Some(InsufficientDataInfo {
                    reason: format!("No releases found in project '{project_id}'."),
                    missing_entity: None,
                    suggested_action: Some(
                        "Create a release before assessing release readiness.".to_string(),
                    ),
                }),
                record_count: 0,
                is_truncated: false,
            });
        };

        let inputs = QualityInputs {
            executor_kinds: &self.executor_kinds,
        };
        let readiness = assess_release(&self.store, project_id, release.id, &inputs).await?;
        provenance.push(EvidenceProvenance {
            reader: EvidenceReaderKind::ReleaseReadiness,
            project_id: project_id.to_string(),
            entity_type: "release_readiness",
            entity_id: Some(release.id.to_string()),
            source: "release_readiness::assess_release",
        });

        let status = match readiness.recommendation {
            ReleaseRecommendation::Go | ReleaseRecommendation::NoGo => RetrievalStatus::Complete,
            ReleaseRecommendation::ConditionalGo => RetrievalStatus::Partial,
            ReleaseRecommendation::InsufficientData => RetrievalStatus::InsufficientData,
        };

        let insufficient_data = if status == RetrievalStatus::InsufficientData {
            Some(InsufficientDataInfo {
                reason: format!(
                    "Release {} readiness evaluated to INSUFFICIENT_DATA: {}",
                    release.id,
                    readiness
                        .missing_signals
                        .first()
                        .map(|m| m.detail.as_str())
                        .unwrap_or("required release evidence incomplete")
                ),
                missing_entity: Some(release.id.to_string()),
                suggested_action: Some("Run required tests for this release.".to_string()),
            })
        } else {
            None
        };

        let data = RetrievedEvidenceData::ReleaseReadiness(Box::new(ReleaseReadinessEvidence {
            release_id: release.id,
            project_id: project_id.to_string(),
            label: release.label,
            version: release.version,
            recommendation: readiness.recommendation,
            readiness,
        }));

        Ok(RetrievedEvidence {
            intent: Some(intent.clone()),
            project_id: project_id.to_string(),
            status,
            provenance,
            data: Some(data),
            insufficient_data,
            record_count: 1,
            is_truncated: false,
        })
    }

    async fn retrieve_test_recommendations(
        &self,
        project_id: &str,
        intent: &CopilotIntent,
    ) -> anyhow::Result<RetrievedEvidence> {
        let mut provenance = Vec::new();
        let qa_reader = QaMemoryReader::new(self.store.clone());
        let prioritisation = qa_reader.prioritise_tests(project_id, None).await?;
        provenance.push(EvidenceProvenance {
            reader: EvidenceReaderKind::QaMemoryRisk,
            project_id: project_id.to_string(),
            entity_type: "test_prioritisation",
            entity_id: None,
            source: "qa_memory_risk::prioritise_tests",
        });

        let impact_reader = ChangeImpactReader::new(self.store.clone());
        let floor = impact_reader.safety_floor(project_id).await?;
        provenance.push(EvidenceProvenance {
            reader: EvidenceReaderKind::ChangeImpact,
            project_id: project_id.to_string(),
            entity_type: "safety_floor",
            entity_id: None,
            source: "change_impact::safety_floor",
        });

        let risks = impact_reader.risk_scores(project_id).await?;
        let high_risk_count = risks
            .values()
            .filter(|&&r| r >= crate::change_impact::HIGH_RISK_THRESHOLD)
            .count();

        let floor_ids: Vec<Uuid> = floor.keys().copied().collect();
        let floor_count = floor_ids.len();

        let (status, insufficient_data, prio_view, rec_count) = match prioritisation.view {
            Some(p) => {
                let count = p.tests.len();
                (RetrievalStatus::Complete, None, Some(p), count)
            }
            None => {
                let reason = prioritisation
                    .insufficient_data
                    .map(|i| format!("{i:?}"))
                    .unwrap_or_else(|| "No historical execution telemetry in project".to_string());
                (
                    RetrievalStatus::InsufficientData,
                    Some(InsufficientDataInfo {
                        reason: format!("Test prioritisation has insufficient data: {reason}"),
                        missing_entity: None,
                        suggested_action: Some(
                            "Execute test runs to generate telemetry for risk prioritisation."
                                .to_string(),
                        ),
                    }),
                    None,
                    0,
                )
            }
        };

        let data =
            RetrievedEvidenceData::TestRecommendations(Box::new(TestRecommendationsEvidence {
                project_id: project_id.to_string(),
                prioritisation: prio_view,
                safety_floor_test_ids: floor_ids,
                safety_floor_count: floor_count,
                high_risk_test_count: high_risk_count,
            }));

        Ok(RetrievedEvidence {
            intent: Some(intent.clone()),
            project_id: project_id.to_string(),
            status,
            provenance,
            data: Some(data),
            insufficient_data,
            record_count: rec_count,
            is_truncated: false,
        })
    }

    async fn retrieve_regression_impact(
        &self,
        project_id: &str,
        intent: &CopilotIntent,
        change_ref: Option<&str>,
    ) -> anyhow::Result<RetrievedEvidence> {
        let mut provenance = Vec::new();
        let limit = MAX_IMPACTS_RETRIEVED as i64;
        let recorded_impacts = self.store.list_change_impacts(project_id, limit).await?;
        provenance.push(EvidenceProvenance {
            reader: EvidenceReaderKind::ChangeImpact,
            project_id: project_id.to_string(),
            entity_type: "change_impacts",
            entity_id: change_ref.map(|s| s.to_string()),
            source: "store::list_change_impacts",
        });

        let impact_reader = ChangeImpactReader::new(self.store.clone());
        let safety_floor = impact_reader.safety_floor(project_id).await?;
        let safety_floor_ids: Vec<Uuid> = safety_floor.keys().copied().collect();

        let mut direct_impacts = Vec::new();

        let filtered_impacts: Vec<ChangeImpactRow> = match change_ref {
            Some(cr) => {
                let exact: Vec<ChangeImpactRow> = recorded_impacts
                    .into_iter()
                    .filter(|r| r.change_ref == cr)
                    .collect();
                if exact.is_empty() {
                    let target = ChangeTarget {
                        kind: "requirement".to_string(),
                        ref_id: cr.to_string(),
                    };
                    if let Ok(crate::change_impact::Resolution::Resolved { node_id, .. }) =
                        impact_reader.resolve(project_id, &target).await
                    {
                        if let Ok(impacted) =
                            impact_reader.impacted_from_node(project_id, node_id).await
                        {
                            direct_impacts = impacted;
                        }
                    }
                }
                exact
            }
            None => recorded_impacts,
        };

        let is_empty = filtered_impacts.is_empty() && direct_impacts.is_empty();
        let (status, insufficient_data) = if is_empty {
            (
                RetrievalStatus::InsufficientData,
                Some(InsufficientDataInfo {
                    reason: match change_ref {
                        Some(cr) => format!(
                            "No change impact records or graph vertices found for change '{cr}' in project '{project_id}'."
                        ),
                        None => format!("No change impact history found in project '{project_id}'."),
                    },
                    missing_entity: change_ref.map(|s| s.to_string()),
                    suggested_action: Some(
                        "Declare change targets or run impact analysis before querying regression impact."
                            .to_string(),
                    ),
                }),
            )
        } else {
            (RetrievalStatus::Complete, None)
        };

        let record_count = filtered_impacts.len() + direct_impacts.len();
        let data = RetrievedEvidenceData::RegressionImpact(Box::new(RegressionImpactEvidence {
            project_id: project_id.to_string(),
            change_ref: change_ref.map(|s| s.to_string()),
            recorded_impacts: filtered_impacts,
            direct_impacts,
            safety_floor_test_ids: safety_floor_ids,
        }));

        Ok(RetrievedEvidence {
            intent: Some(intent.clone()),
            project_id: project_id.to_string(),
            status,
            provenance,
            data: Some(data),
            insufficient_data,
            record_count,
            is_truncated: false,
        })
    }

    async fn retrieve_test_risk(
        &self,
        project_id: &str,
        intent: &CopilotIntent,
        test_case_id: Option<&str>,
    ) -> anyhow::Result<RetrievedEvidence> {
        let mut provenance = Vec::new();
        let qa_reader = QaMemoryReader::new(self.store.clone());
        let impact_reader = ChangeImpactReader::new(self.store.clone());

        let (test_case, risk_view, quality_view) = match test_case_id {
            Some(tc_ref) => {
                let case = if let Ok(uid) = Uuid::parse_str(tc_ref) {
                    let found = self.store.get_test_case(uid).await?;
                    found.filter(|c| c.project_id == project_id)
                } else {
                    let cases = self.store.list_test_cases(project_id, None, None).await?;
                    cases.into_iter().find(|c| {
                        c.id.to_string() == tc_ref
                            || c.title.contains(tc_ref)
                            || c.requirement_reference.as_deref() == Some(tc_ref)
                    })
                };

                let Some(case) = case else {
                    provenance.push(EvidenceProvenance {
                        reader: EvidenceReaderKind::QaMemoryRisk,
                        project_id: project_id.to_string(),
                        entity_type: "test_case",
                        entity_id: Some(tc_ref.to_string()),
                        source: "store::get_test_case",
                    });
                    return Ok(RetrievedEvidence {
                        intent: Some(intent.clone()),
                        project_id: project_id.to_string(),
                        status: RetrievalStatus::InsufficientData,
                        provenance,
                        data: None,
                        insufficient_data: Some(InsufficientDataInfo {
                            reason: format!(
                                "Test case '{tc_ref}' does not exist in project '{project_id}'. \
                                 Cross-project or non-existent cases are not retrievable."
                            ),
                            missing_entity: Some(tc_ref.to_string()),
                            suggested_action: Some(
                                "Check the test case ID or title in project test cases."
                                    .to_string(),
                            ),
                        }),
                        record_count: 0,
                        is_truncated: false,
                    });
                };

                provenance.push(EvidenceProvenance {
                    reader: EvidenceReaderKind::QaMemoryRisk,
                    project_id: project_id.to_string(),
                    entity_type: "test_case",
                    entity_id: Some(case.id.to_string()),
                    source: "store::get_test_case",
                });

                let risk_res = qa_reader
                    .test_risk(project_id, &case.id.to_string())
                    .await?;
                provenance.push(EvidenceProvenance {
                    reader: EvidenceReaderKind::QaMemoryRisk,
                    project_id: project_id.to_string(),
                    entity_type: "test_risk",
                    entity_id: Some(case.id.to_string()),
                    source: "qa_memory_risk::test_risk",
                });

                let inputs = QualityInputs {
                    executor_kinds: &self.executor_kinds,
                };
                let quality = assess_case(&self.store, &case, project_id, &inputs).await;
                provenance.push(EvidenceProvenance {
                    reader: EvidenceReaderKind::TestQuality,
                    project_id: project_id.to_string(),
                    entity_type: "test_quality",
                    entity_id: Some(case.id.to_string()),
                    source: "test_quality::assess_case",
                });

                (Some(case), risk_res.view, Some(quality))
            }
            None => (None, None, None),
        };

        let all_risks = impact_reader.risk_scores(project_id).await?;
        let mut risk_ranking: Vec<(Uuid, f64)> = all_risks.into_iter().collect();
        risk_ranking.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        risk_ranking.truncate(MAX_TESTS_RETRIEVED);

        let is_empty = test_case.is_none() && risk_ranking.is_empty();
        let (status, insufficient_data) = if is_empty {
            (
                RetrievalStatus::InsufficientData,
                Some(InsufficientDataInfo {
                    reason: format!(
                        "No test case or risk intelligence records found in project '{project_id}'."
                    ),
                    missing_entity: test_case_id.map(|s| s.to_string()),
                    suggested_action: Some(
                        "Ensure tests are defined and executions have been recorded.".to_string(),
                    ),
                }),
            )
        } else {
            (RetrievalStatus::Complete, None)
        };

        let record_count = if test_case.is_some() {
            1
        } else {
            risk_ranking.len()
        };
        let data = RetrievedEvidenceData::TestRisk(Box::new(TestRiskEvidence {
            project_id: project_id.to_string(),
            test_case_id: test_case.as_ref().map(|c| c.id.to_string()),
            test_title: test_case.as_ref().map(|c| c.title.clone()),
            risk_view,
            quality: quality_view,
            project_high_risk_cases: risk_ranking,
        }));

        Ok(RetrievedEvidence {
            intent: Some(intent.clone()),
            project_id: project_id.to_string(),
            status,
            provenance,
            data: Some(data),
            insufficient_data,
            record_count,
            is_truncated: false,
        })
    }

    async fn retrieve_recurring_failures(
        &self,
        project_id: &str,
        intent: &CopilotIntent,
    ) -> anyhow::Result<RetrievedEvidence> {
        let mut provenance = Vec::new();
        let clusters = self.store.list_failure_clusters(Some(project_id)).await?;
        provenance.push(EvidenceProvenance {
            reader: EvidenceReaderKind::FailureClusters,
            project_id: project_id.to_string(),
            entity_type: "failure_clusters",
            entity_id: None,
            source: "store::list_failure_clusters",
        });

        let is_empty = clusters.is_empty();
        let truncated = clusters.len() > MAX_FAILURES_RETRIEVED;
        let mut bounded_clusters = clusters;
        bounded_clusters.truncate(MAX_FAILURES_RETRIEVED);
        let count = bounded_clusters.len();

        let (status, insufficient_data) = if is_empty {
            (
                RetrievalStatus::InsufficientData,
                Some(InsufficientDataInfo {
                    reason: format!("No failure clusters recorded for project '{project_id}'."),
                    missing_entity: None,
                    suggested_action: Some(
                        "Run tests that exhibit failure signatures to populate failure clusters."
                            .to_string(),
                    ),
                }),
            )
        } else {
            (RetrievalStatus::Complete, None)
        };

        let data = RetrievedEvidenceData::FailureClusters(bounded_clusters);

        Ok(RetrievedEvidence {
            intent: Some(intent.clone()),
            project_id: project_id.to_string(),
            status,
            provenance,
            data: Some(data),
            insufficient_data,
            record_count: count,
            is_truncated: truncated,
        })
    }

    async fn retrieve_requirement_coverage(
        &self,
        project_id: &str,
        intent: &CopilotIntent,
    ) -> anyhow::Result<RetrievedEvidence> {
        let mut provenance = Vec::new();
        let reqs = self.store.list_requirements(project_id, &[]).await?;
        provenance.push(EvidenceProvenance {
            reader: EvidenceReaderKind::RequirementsCoverage,
            project_id: project_id.to_string(),
            entity_type: "requirements",
            entity_id: None,
            source: "store::list_requirements",
        });

        if reqs.is_empty() {
            return Ok(RetrievedEvidence {
                intent: Some(intent.clone()),
                project_id: project_id.to_string(),
                status: RetrievalStatus::InsufficientData,
                provenance,
                data: None,
                insufficient_data: Some(InsufficientDataInfo {
                    reason: format!(
                        "No requirements uploaded or extracted for project '{project_id}'."
                    ),
                    missing_entity: None,
                    suggested_action: Some(
                        "Upload requirement documents before auditing test coverage.".to_string(),
                    ),
                }),
                record_count: 0,
                is_truncated: false,
            });
        }

        let is_truncated = reqs.len() > MAX_REQUIREMENTS_RETRIEVED;
        let mut bounded_reqs = reqs;
        bounded_reqs.truncate(MAX_REQUIREMENTS_RETRIEVED);

        let mut uncovered = Vec::new();
        let mut covered = Vec::new();

        for r in bounded_reqs {
            let cases = self
                .store
                .list_test_cases_for_requirement(project_id, &r.key)
                .await?;
            let case_ids: Vec<Uuid> = cases.iter().map(|c| c.id).collect();
            let item = RequirementCoverageItem {
                key: r.key,
                document_id: r.document_id,
                section: r.section,
                text: r.text,
                test_case_count: cases.len(),
                test_case_ids: case_ids,
            };
            if cases.is_empty() {
                uncovered.push(item);
            } else {
                covered.push(item);
            }
        }

        provenance.push(EvidenceProvenance {
            reader: EvidenceReaderKind::RequirementsCoverage,
            project_id: project_id.to_string(),
            entity_type: "requirement_coverage_edges",
            entity_id: None,
            source: "store::list_test_cases_for_requirement",
        });

        let total = uncovered.len() + covered.len();
        let data =
            RetrievedEvidenceData::RequirementCoverage(Box::new(RequirementCoverageEvidence {
                project_id: project_id.to_string(),
                total_requirements: total,
                uncovered_count: uncovered.len(),
                covered_count: covered.len(),
                uncovered_requirements: uncovered,
                covered_requirements: covered,
            }));

        Ok(RetrievedEvidence {
            intent: Some(intent.clone()),
            project_id: project_id.to_string(),
            status: RetrievalStatus::Complete,
            provenance,
            data: Some(data),
            insufficient_data: None,
            record_count: total,
            is_truncated,
        })
    }

    async fn retrieve_insufficient_evidence_reason(
        &self,
        project_id: &str,
        intent: &CopilotIntent,
        finding_id: Option<&str>,
        run_id: Option<&str>,
    ) -> anyhow::Result<RetrievedEvidence> {
        let mut provenance = Vec::new();
        let run_ids = self.store.list_run_ids(Some(project_id)).await?;
        provenance.push(EvidenceProvenance {
            reader: EvidenceReaderKind::IntegrityVerifier,
            project_id: project_id.to_string(),
            entity_type: "run_ids",
            entity_id: None,
            source: "store::list_run_ids",
        });

        let target_run_id = match run_id {
            Some(r_str) => Uuid::parse_str(r_str).ok().filter(|u| run_ids.contains(u)),
            None => run_ids.first().copied(),
        };

        let integrity_report = match target_run_id {
            Some(rid) => {
                let verifier = IntegrityVerifier::new(&self.store);
                let rep = verifier.verify_run(project_id, rid, None).await?;
                provenance.push(EvidenceProvenance {
                    reader: EvidenceReaderKind::IntegrityVerifier,
                    project_id: project_id.to_string(),
                    entity_type: "integrity_report",
                    entity_id: Some(rid.to_string()),
                    source: "integrity::verify_run",
                });
                Some(rep)
            }
            None => None,
        };

        let cases = self.store.list_test_cases(project_id, None, None).await?;
        provenance.push(EvidenceProvenance {
            reader: EvidenceReaderKind::TestQuality,
            project_id: project_id.to_string(),
            entity_type: "test_cases",
            entity_id: None,
            source: "store::list_test_cases",
        });

        let inputs = QualityInputs {
            executor_kinds: &self.executor_kinds,
        };
        let mut quality_summaries = Vec::new();
        for c in cases.iter().take(MAX_QUALITY_CASES_RETRIEVED) {
            let q = assess_case(&self.store, c, project_id, &inputs).await;
            let overall = q.overall();
            if overall != QualityVerdict::Adequate {
                quality_summaries.push(TestCaseQualitySummary {
                    test_case_id: c.id.to_string(),
                    title: Some(c.title.clone()),
                    overall_verdict: overall,
                    unknown_dimensions: q.unknown_keys(),
                    weak_dimensions: q
                        .dimensions
                        .iter()
                        .filter(|d| d.verdict == QualityVerdict::Weak)
                        .map(|d| d.key)
                        .collect(),
                });
            }
        }

        let is_empty = integrity_report.is_none() && quality_summaries.is_empty();
        let (status, insufficient_data) = if is_empty {
            (
                RetrievalStatus::InsufficientData,
                Some(InsufficientDataInfo {
                    reason: format!(
                        "No runs or test cases found for project '{project_id}' to investigate insufficient evidence."
                    ),
                    missing_entity: run_id.or(finding_id).map(|s| s.to_string()),
                    suggested_action: Some(
                        "Execute runs or add test cases to investigate evidence integrity."
                            .to_string(),
                    ),
                }),
            )
        } else {
            (RetrievalStatus::Complete, None)
        };

        let record_count = if integrity_report.is_some() { 1 } else { 0 } + quality_summaries.len();
        let data = RetrievedEvidenceData::InsufficientEvidenceInvestigation(Box::new(
            InsufficientEvidenceInvestigationEvidence {
                project_id: project_id.to_string(),
                run_id: target_run_id,
                integrity_report,
                insufficient_quality_cases: quality_summaries,
            },
        ));

        Ok(RetrievedEvidence {
            intent: Some(intent.clone()),
            project_id: project_id.to_string(),
            status,
            provenance,
            data: Some(data),
            insufficient_data,
            record_count,
            is_truncated: false,
        })
    }
}
