//! Phase 8: Intelligent QA Operations & Cross-Environment Correlation.
//!
//! Core Architectural Invariants:
//! - Strictly zero OpenJarvis, zero Python runtimes, zero PyO3.
//! - Strictly observe, analyze, correlate, recommend. NEVER authorize or execute without governance.
//! - Tenant & project isolation: all correlation queries are strictly scoped to project_id.
//! - Deterministic cross-environment correlation using normalized categories, fingerprints, selectors, and tasks.
//! - Safe, bounded, project-aware telemetry retention preserving active runs, healing proposals, audit logs, and Quality Graph integrity.
//! - Traceability chain: Test -> Task -> Failure -> Cluster -> Evidence -> Healing Proposal -> AST Diff -> Approval -> Execution -> Verification -> Quality Graph.

use crate::adaptive_intelligence::{calculate_trend, FailureCategory};
use crate::store::{ExecutionTelemetryRow, FailureClusterRow, HealingProposalRow, Store};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

// ============================================================================
// 1. DOMAIN MODELS & ENUMS
// ============================================================================

/// Recurrence intelligence state for failure clusters across runs and environments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecurrenceState {
    /// Newly observed failure with minimal historical occurrences.
    New,
    /// Repeated failure observed across multiple executions or environments.
    Recurring,
    /// Ongoing, unfixed failure persisting across runs without resolution.
    Persistent,
    /// Formerly failing cluster whose recent executions have verified green / passed.
    Resolved,
}

impl RecurrenceState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::New => "NEW",
            Self::Recurring => "RECURRING",
            Self::Persistent => "PERSISTENT",
            Self::Resolved => "RESOLVED",
        }
    }
}

/// Operational timeline event type along the test-to-healing traceability chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TimelineEventType {
    FailureObserved,
    HealingProposed,
    HealingApproved,
    HealingExecuted,
    HealingVerified,
    RegressionDetected,
}

impl TimelineEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FailureObserved => "FAILURE_OBSERVED",
            Self::HealingProposed => "HEALING_PROPOSED",
            Self::HealingApproved => "HEALING_APPROVED",
            Self::HealingExecuted => "HEALING_EXECUTED",
            Self::HealingVerified => "HEALING_VERIFIED",
            Self::RegressionDetected => "REGRESSION_DETECTED",
        }
    }
}

/// Environment-specific occurrence statistics for a failure cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentOccurrence {
    pub environment: String,
    pub occurrences: i64,
    pub total_runs: i64,
    pub failure_rate: f64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub affected_tests: Vec<String>,
    pub affected_components: Vec<String>,
    pub trend: String,
}

/// Cross-environment correlation matrix comparing failure behavior across environments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentCorrelationMatrix {
    pub cluster_id: String,
    pub signature: String,
    pub environments: Vec<EnvironmentOccurrence>,
    pub recurrence_state: RecurrenceState,
    pub cross_environment: bool,
    pub total_occurrences: i64,
}

/// Chronological event on the operational investigation timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvestigationTimelineEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: TimelineEventType,
    pub environment: String,
    pub description: String,
    pub evidence_refs: Vec<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Traceable correlation to a healing proposal and its verification outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealingHistoryCorrelation {
    pub proposal_id: Uuid,
    pub task_id: String,
    pub strategy: String,
    pub risk_level: String,
    pub state: String,
    pub verification_result: Option<String>,
    pub regression_detected: bool,
    pub ast_diff: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Structured, comprehensive Failure Investigation domain object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureInvestigation {
    pub project_id: String,
    pub cluster_id: String,
    pub signature: String,
    pub failure_category: String,
    pub root_cause_hypothesis: Option<String>,
    pub confidence: Option<f64>,
    pub reproducibility: Option<f64>,
    pub environment_matrix: EnvironmentCorrelationMatrix,
    pub timeline: Vec<InvestigationTimelineEvent>,
    pub affected_tests: Vec<String>,
    pub affected_tasks: Vec<String>,
    pub affected_components: Vec<String>,
    pub healing_history: Vec<HealingHistoryCorrelation>,
    pub regression_status: String,
    pub non_causal_changes: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub recommendations: Vec<String>,
}

/// Request policy for telemetry retention pruning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub retention_days: u32,
    pub dry_run: bool,
    /// Whether to also prune this project's `audit_log` rows.
    ///
    /// Defaults to `false`, and that default is the point. Telemetry is
    /// operational data with a natural expiry; an audit trail is evidence of
    /// who did what, and how long it must be kept is a compliance decision the
    /// backend should not make on an operator's behalf. So the existing
    /// behaviour — prune telemetry, preserve audit logs — is unchanged, and
    /// pruning the trail is something an admin has to ask for by name.
    #[serde(default)]
    pub include_audit: bool,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            retention_days: 30,
            dry_run: true,
            include_audit: false,
        }
    }
}

/// Execution outcome report for telemetry retention operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionReport {
    pub project_id: String,
    pub eligible_count: u64,
    pub pruned_count: u64,
    pub dry_run: bool,
    pub executed_at: DateTime<Utc>,
    pub retention_days: u32,
    /// Audit rows this pass pruned, or would prune under `dry_run`.
    ///
    /// Separate from `pruned_count` rather than folded into it because the two
    /// are different things and an operator needs to be able to see which one
    /// moved: `pruned_count` is telemetry, this is the audit trail. Always `0`
    /// unless the request set `include_audit`.
    #[serde(default)]
    pub audit_pruned_count: u64,
}

// ============================================================================
// 2. OPERATIONS SERVICE & CORRELATION ENGINE
// ============================================================================

/// Coordinator for Phase 8 Intelligent QA Operations & Cross-Environment Correlation.
pub struct OperationsService {
    pub store: Store,
}

impl OperationsService {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Fetches and synthesizes a full Failure Investigation for a given cluster.
    pub async fn get_investigation(
        &self,
        project_id: &str,
        cluster_id_or_sig: &str,
    ) -> Result<Option<FailureInvestigation>, String> {
        // 1. Resolve cluster record from store
        let cluster = self.resolve_cluster(project_id, cluster_id_or_sig).await?;
        let cluster = match cluster {
            Some(c) => c,
            None => return Ok(None),
        };

        // 2. Fetch correlated telemetry records
        let cluster_id_str = cluster.id.to_string();
        let telemetry_records = self
            .store
            .list_telemetry_for_cluster(project_id, &cluster.signature, Some(&cluster_id_str), 500)
            .await
            .map_err(|e| format!("failed to list telemetry for cluster: {e}"))?;

        // 3. Fetch all project telemetry to calculate baseline runs per environment
        let all_telemetry = self
            .store
            .list_execution_telemetry(project_id, 1000)
            .await
            .map_err(|e| format!("failed to list project telemetry: {e}"))?;

        // 4. Extract affected tests, tasks, selectors/components
        let mut affected_tests_set = HashSet::new();
        let mut affected_tasks_set = HashSet::new();
        let mut affected_components_set = HashSet::new();
        let mut evidence_refs_set = HashSet::new();

        for t in &telemetry_records {
            affected_tests_set.insert(t.task_name.clone());
            affected_tasks_set.insert(t.task_id.clone());
            if let Some(ref sel) = t.selector {
                affected_components_set.insert(sel.clone());
            }
            if let Ok(refs) = serde_json::from_str::<Vec<String>>(&t.evidence_refs) {
                for r in refs {
                    evidence_refs_set.insert(r);
                }
            }
        }

        let mut affected_tests: Vec<String> = affected_tests_set.into_iter().collect();
        affected_tests.sort();
        let mut affected_tasks: Vec<String> = affected_tasks_set.into_iter().collect();
        affected_tasks.sort();
        let mut affected_components: Vec<String> = affected_components_set.into_iter().collect();
        affected_components.sort();
        let mut evidence_refs: Vec<String> = evidence_refs_set.into_iter().collect();
        evidence_refs.sort();

        // 5. Fetch healing proposals correlating with this cluster or affected tasks
        let healing_proposals = self
            .store
            .list_healing_proposals_for_cluster(
                project_id,
                Some(&cluster.signature),
                &affected_tasks,
            )
            .await
            .map_err(|e| format!("failed to list healing proposals: {e}"))?;

        // 6. Build Cross-Environment Correlation Matrix
        let environment_matrix = self.build_environment_matrix(
            &cluster_id_str,
            &cluster.signature,
            &telemetry_records,
            &all_telemetry,
            &healing_proposals,
            &affected_tests,
        );

        // 7. Build Operational Timeline
        let timeline = self.build_investigation_timeline(&telemetry_records, &healing_proposals);

        // 8. Build Healing History
        let healing_history = self.build_healing_history(&healing_proposals);

        // 9. Conservative Regression Correlation
        let regression_status = self.correlate_regression_status(&healing_history, &timeline);

        // 10. Non-Causal Change-Time Correlation
        let non_causal_changes =
            self.correlate_non_causal_changes(&healing_history, &telemetry_records);

        // 11. Derive primary failure category
        let failure_category = telemetry_records
            .iter()
            .find_map(|t| t.failure_category.clone())
            .unwrap_or_else(|| {
                FailureCategory::from_str(&cluster.signature)
                    .as_str()
                    .to_string()
            });

        // 12. Prioritized QA Recommendations
        let recommendations = self.generate_operational_recommendations(
            &failure_category,
            &environment_matrix,
            &regression_status,
            &healing_history,
            &affected_components,
        );

        Ok(Some(FailureInvestigation {
            project_id: project_id.to_string(),
            cluster_id: cluster_id_str,
            signature: cluster.signature,
            failure_category,
            root_cause_hypothesis: cluster.root_cause_hypothesis,
            confidence: cluster.confidence,
            reproducibility: cluster.reproducibility,
            environment_matrix,
            timeline,
            affected_tests,
            affected_tasks,
            affected_components,
            healing_history,
            regression_status,
            non_causal_changes,
            evidence_refs,
            recommendations,
        }))
    }

    /// How many clusters `list_investigations` builds a full investigation for.
    ///
    /// Each iteration is not cheap: `get_investigation` issues ~4 queries, two
    /// of which (`list_execution_telemetry(project_id, 1000)` and the
    /// project-wide read behind `list_healing_proposals_for_cluster`) are
    /// *identical for every cluster* and re-fetched each time. So this endpoint
    /// used to cost `4 x N_clusters` queries and grew without bound.
    ///
    /// The obvious fix — hoist those two reads out of the loop and pass them
    /// into `get_investigation` — would mean changing the signature of a public
    /// method that four other handlers call directly (and that its own tests
    /// call), for a saving of exactly 2 queries per cluster. Capping the loop
    /// instead removes the unbounded growth outright, costs nothing to the
    /// single-cluster path, and keeps the response a size a UI can render.
    /// `list_failure_clusters` orders by occurrence_count DESC and the results
    /// are re-sorted by total occurrences DESC, so this keeps the clusters the
    /// caller most likely wanted; the full set stays one call away on
    /// `GET /failure-clusters`.
    const INVESTIGATIONS_MAX_CLUSTERS: usize = 25;

    /// Lists all failure investigations for a project.
    pub async fn list_investigations(
        &self,
        project_id: &str,
    ) -> Result<Vec<FailureInvestigation>, String> {
        let clusters = self
            .store
            .list_failure_clusters(Some(project_id))
            .await
            .map_err(|e| format!("failed to list clusters for investigations: {e}"))?;

        let mut investigations = Vec::new();
        for c in clusters.into_iter().take(Self::INVESTIGATIONS_MAX_CLUSTERS) {
            if let Ok(Some(inv)) = self.get_investigation(project_id, &c.id.to_string()).await {
                investigations.push(inv);
            }
        }

        // Sort by total occurrences descending
        investigations.sort_by(|a, b| {
            b.environment_matrix
                .total_occurrences
                .cmp(&a.environment_matrix.total_occurrences)
        });

        Ok(investigations)
    }

    /// Fetches only the EnvironmentCorrelationMatrix for a given cluster.
    pub async fn get_environment_matrix(
        &self,
        project_id: &str,
        cluster_id: &str,
    ) -> Result<Option<EnvironmentCorrelationMatrix>, String> {
        self.get_investigation(project_id, cluster_id)
            .await
            .map(|opt| opt.map(|inv| inv.environment_matrix))
    }

    /// Fetches only the chronological timeline for a given cluster.
    pub async fn get_investigation_timeline(
        &self,
        project_id: &str,
        cluster_id: &str,
    ) -> Result<Option<Vec<InvestigationTimelineEvent>>, String> {
        self.get_investigation(project_id, cluster_id)
            .await
            .map(|opt| opt.map(|inv| inv.timeline))
    }

    /// Executes or previews bounded telemetry retention pruning with safety guards.
    pub async fn execute_retention(
        &self,
        project_id: &str,
        policy: &RetentionPolicy,
        actor: &str,
    ) -> Result<RetentionReport, String> {
        if policy.retention_days == 0 {
            return Err("retention_days must be at least 1 day".to_string());
        }

        let older_than = Utc::now() - Duration::days(policy.retention_days as i64);

        let count = self
            .store
            .prune_execution_telemetry(project_id, older_than, policy.dry_run)
            .await
            .map_err(|e| format!("retention execution failed: {e}"))?;

        // Opt-in, and after telemetry, so a failure here cannot leave telemetry
        // pruned with no audit record of it.
        let audit_count = if policy.include_audit {
            self.store
                .prune_audit(project_id, older_than, policy.dry_run)
                .await
                .map_err(|e| format!("audit retention execution failed: {e}"))?
        } else {
            0
        };

        let now = Utc::now();

        if !policy.dry_run {
            let detail = if policy.include_audit {
                format!(
                    "Retention prune executed: pruned {} telemetry records and {} audit rows older than {} days",
                    count, audit_count, policy.retention_days
                )
            } else {
                format!(
                    "Retention prune executed: pruned {} records older than {} days",
                    count, policy.retention_days
                )
            };
            // Recorded *after* the prune, and scoped to this project, so the row
            // documenting the prune is itself newer than the cutoff and survives
            // it. The alternative — pruning the trail without leaving a trace of
            // having pruned it — is the one outcome worth engineering against.
            let _ = self
                .store
                .insert_audit(
                    Some(actor),
                    Some("admin"),
                    Some(project_id),
                    if policy.include_audit {
                        "audit.retention"
                    } else {
                        "telemetry.retention"
                    },
                    None,
                    Some(&detail),
                    "allowed",
                )
                .await;
        }

        Ok(RetentionReport {
            project_id: project_id.to_string(),
            eligible_count: count,
            pruned_count: if policy.dry_run { 0 } else { count },
            dry_run: policy.dry_run,
            executed_at: now,
            retention_days: policy.retention_days,
            audit_pruned_count: if policy.dry_run { 0 } else { audit_count },
        })
    }

    // ========================================================================
    // PRIVATE CORRELATION HELPERS
    // ========================================================================

    async fn resolve_cluster(
        &self,
        project_id: &str,
        cluster_id_or_sig: &str,
    ) -> Result<Option<FailureClusterRow>, String> {
        // 1. Try resolving by UUID (strictly scoped to project_id!)
        if let Ok(uid) = Uuid::parse_str(cluster_id_or_sig) {
            if let Ok(Some(row)) = self
                .store
                .get_failure_cluster_by_id_scoped(project_id, uid)
                .await
            {
                return Ok(Some(row));
            }
            // If it's a valid UUID but not found in this project, do not fall back to telemetry
            return Ok(None);
        }

        // 2. Try resolving by exact signature
        if let Ok(Some(row)) = self
            .store
            .get_failure_cluster_by_signature(project_id, cluster_id_or_sig)
            .await
        {
            return Ok(Some(row));
        }

        // 3. Synthesize from existing telemetry if cluster row was not explicitly registered
        let telemetry = self
            .store
            .list_telemetry_for_cluster(project_id, cluster_id_or_sig, None, 10)
            .await
            .map_err(|e| format!("failed to inspect fallback telemetry: {e}"))?;

        if let Some(first) = telemetry.first() {
            let sig = first
                .failure_fingerprint
                .clone()
                .unwrap_or_else(|| cluster_id_or_sig.to_string());
            let count = telemetry.len() as i64;
            let rep = (count as f64 / 5.0).min(1.0);
            return Ok(Some(FailureClusterRow {
                id: Uuid::new_v4(),
                signature: sig,
                first_seen_at: first.created_at,
                last_seen_at: telemetry
                    .last()
                    .map(|t| t.created_at)
                    .unwrap_or(first.created_at),
                occurrence_count: count,
                root_cause_hypothesis: first.error_message.clone(),
                confidence: Some(0.75),
                reproducibility: Some(rep),
                review_state: "NEEDS_REVIEW".to_string(),
                ai_hypothesis: None,
                ai_hypothesis_provider: None,
                ai_hypothesis_model: None,
                ai_hypothesis_generated_at: None,
            }));
        }

        Ok(None)
    }

    fn build_environment_matrix(
        &self,
        cluster_id: &str,
        signature: &str,
        telemetry_records: &[ExecutionTelemetryRow],
        all_telemetry: &[ExecutionTelemetryRow],
        healing_proposals: &[HealingProposalRow],
        affected_tests: &[String],
    ) -> EnvironmentCorrelationMatrix {
        let mut env_map: HashMap<String, Vec<&ExecutionTelemetryRow>> = HashMap::new();

        for t in telemetry_records {
            env_map.entry(t.environment.clone()).or_default().push(t);
        }

        let mut environments = Vec::new();
        let mut total_occurrences = 0;

        for (env, recs) in env_map {
            let occurrences = recs.len() as i64;
            total_occurrences += occurrences;

            let first_seen = recs
                .iter()
                .map(|r| r.created_at)
                .min()
                .unwrap_or_else(Utc::now);
            let last_seen = recs
                .iter()
                .map(|r| r.created_at)
                .max()
                .unwrap_or_else(Utc::now);

            // Calculate total runs for affected tests in this environment
            let total_env_runs = all_telemetry
                .iter()
                .filter(|t| t.environment == env && affected_tests.contains(&t.task_name))
                .count() as i64;
            let total_runs = total_env_runs.max(occurrences);

            let failure_rate = if total_runs > 0 {
                (occurrences as f64 / total_runs as f64 * 100.0).round() / 100.0
            } else {
                1.0
            };

            let mut env_affected_tests: Vec<String> = recs
                .iter()
                .map(|r| r.task_name.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            env_affected_tests.sort();

            let mut env_affected_components: Vec<String> = recs
                .iter()
                .filter_map(|r| r.selector.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            env_affected_components.sort();

            // Calculate environment trend
            let mut failure_values: Vec<f64> = recs
                .iter()
                .map(|r| {
                    if matches!(r.status.as_str(), "FAILED" | "TIMEOUT" | "INFRA_ERROR") {
                        1.0
                    } else {
                        0.0
                    }
                })
                .collect();
            if failure_values.len() < 3 {
                failure_values.push(1.0);
                failure_values.push(1.0);
                failure_values.push(1.0);
            }
            let trend_analysis = calculate_trend("failure_frequency", &failure_values);
            let trend = trend_analysis.direction.as_str().to_string();

            environments.push(EnvironmentOccurrence {
                environment: env,
                occurrences,
                total_runs,
                failure_rate,
                first_seen,
                last_seen,
                affected_tests: env_affected_tests,
                affected_components: env_affected_components,
                trend,
            });
        }

        environments.sort_by(|a, b| a.environment.cmp(&b.environment));

        let cross_environment = environments.len() > 1;

        // Recurrence intelligence determination
        let has_verified_heal = healing_proposals
            .iter()
            .any(|p| p.state == "VERIFIED" || p.verification_result.as_deref() == Some("PASSED"));

        let recurrence_state = if has_verified_heal {
            RecurrenceState::Resolved
        } else if total_occurrences >= 5 || environments.len() >= 3 {
            RecurrenceState::Persistent
        } else if total_occurrences >= 2 || cross_environment {
            RecurrenceState::Recurring
        } else {
            RecurrenceState::New
        };

        EnvironmentCorrelationMatrix {
            cluster_id: cluster_id.to_string(),
            signature: signature.to_string(),
            environments,
            recurrence_state,
            cross_environment,
            total_occurrences,
        }
    }

    fn build_investigation_timeline(
        &self,
        telemetry_records: &[ExecutionTelemetryRow],
        healing_proposals: &[HealingProposalRow],
    ) -> Vec<InvestigationTimelineEvent> {
        let mut events = Vec::new();

        // 1. Failure occurrences from telemetry
        for t in telemetry_records {
            let desc = format!(
                "Failure observed in task '{}' ({})",
                t.task_name,
                t.error_message.as_deref().unwrap_or("no error details")
            );

            let evidence_refs: Vec<String> =
                serde_json::from_str(&t.evidence_refs).unwrap_or_default();

            events.push(InvestigationTimelineEvent {
                id: format!("failure-{}", t.id),
                timestamp: t.created_at,
                event_type: TimelineEventType::FailureObserved,
                environment: t.environment.clone(),
                description: desc,
                evidence_refs,
                metadata: Some(serde_json::json!({
                    "run_id": t.run_id,
                    "task_id": t.task_id,
                    "selector": t.selector,
                    "http_status": t.http_status,
                })),
            });
        }

        // 2. Healing proposal lifecycle events
        for p in healing_proposals {
            let evidence_refs: Vec<String> =
                serde_json::from_str(&p.evidence_refs).unwrap_or_default();

            // Healing proposed
            events.push(InvestigationTimelineEvent {
                id: format!("healing-proposed-{}", p.id),
                timestamp: p.created_at,
                event_type: TimelineEventType::HealingProposed,
                environment: "orchestrator".to_string(),
                description: format!(
                    "Healing proposed for task '{}' (risk: {}, strategy: {})",
                    p.task_id, p.risk_level, p.proposed_actions
                ),
                evidence_refs: evidence_refs.clone(),
                metadata: Some(serde_json::json!({
                    "proposal_id": p.id,
                    "risk_level": p.risk_level,
                    "requires_approval": p.requires_approval,
                })),
            });

            // Healing approved
            if let Some(approved_at) = p.approved_at {
                events.push(InvestigationTimelineEvent {
                    id: format!("healing-approved-{}", p.id),
                    timestamp: approved_at,
                    event_type: TimelineEventType::HealingApproved,
                    environment: "governance".to_string(),
                    description: format!(
                        "Healing proposal approved by {}",
                        p.approved_by.as_deref().unwrap_or("authorized_reviewer")
                    ),
                    evidence_refs: evidence_refs.clone(),
                    metadata: Some(serde_json::json!({
                        "proposal_id": p.id,
                        "approved_by": p.approved_by,
                    })),
                });
            }

            // Healing executed / verified / regression
            if matches!(
                p.state.as_str(),
                "EXECUTING" | "EXECUTED" | "VERIFIED" | "FAILED"
            ) {
                events.push(InvestigationTimelineEvent {
                    id: format!("healing-executed-{}", p.id),
                    timestamp: p.updated_at,
                    event_type: TimelineEventType::HealingExecuted,
                    environment: "executor".to_string(),
                    description: format!(
                        "Healing proposal execution state transitioned to '{}'",
                        p.state
                    ),
                    evidence_refs: evidence_refs.clone(),
                    metadata: Some(serde_json::json!({
                        "proposal_id": p.id,
                        "state": p.state,
                        "healing_run_id": p.healing_run_id,
                    })),
                });
            }

            if let Some(ref ver_result) = p.verification_result {
                if ver_result.to_uppercase() == "REGRESSION" {
                    events.push(InvestigationTimelineEvent {
                        id: format!("healing-regression-{}", p.id),
                        timestamp: p.updated_at,
                        event_type: TimelineEventType::RegressionDetected,
                        environment: "verification".to_string(),
                        description: format!(
                            "Regression detected during healing verification: proposal {}",
                            p.id
                        ),
                        evidence_refs: evidence_refs.clone(),
                        metadata: Some(serde_json::json!({
                            "proposal_id": p.id,
                            "verification_result": ver_result,
                        })),
                    });
                } else {
                    events.push(InvestigationTimelineEvent {
                        id: format!("healing-verified-{}", p.id),
                        timestamp: p.updated_at,
                        event_type: TimelineEventType::HealingVerified,
                        environment: "verification".to_string(),
                        description: format!(
                            "Healing verification completed with outcome '{}'",
                            ver_result
                        ),
                        evidence_refs: evidence_refs.clone(),
                        metadata: Some(serde_json::json!({
                            "proposal_id": p.id,
                            "verification_result": ver_result,
                        })),
                    });
                }
            }
        }

        events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        events
    }

    fn build_healing_history(
        &self,
        healing_proposals: &[HealingProposalRow],
    ) -> Vec<HealingHistoryCorrelation> {
        let mut history: Vec<HealingHistoryCorrelation> = healing_proposals
            .iter()
            .map(|p| {
                let regression_detected = p
                    .verification_result
                    .as_deref()
                    .map(|v| v.to_uppercase() == "REGRESSION")
                    .unwrap_or(false);

                HealingHistoryCorrelation {
                    proposal_id: p.id,
                    task_id: p.task_id.clone(),
                    strategy: p.proposed_actions.clone(),
                    risk_level: p.risk_level.clone(),
                    state: p.state.clone(),
                    verification_result: p.verification_result.clone(),
                    regression_detected,
                    ast_diff: Some(p.ast_diff.clone()),
                    created_at: p.created_at,
                }
            })
            .collect();

        history.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        history
    }

    fn correlate_regression_status(
        &self,
        healing_history: &[HealingHistoryCorrelation],
        timeline: &[InvestigationTimelineEvent],
    ) -> String {
        // 1. Confirmed regression: explicit verification result = "REGRESSION"
        if healing_history.iter().any(|h| h.regression_detected) {
            return "Confirmed regression".to_string();
        }

        // 2. Possible related regression: a failure observed shortly after a healing execution
        let has_executed_heal = timeline
            .iter()
            .any(|e| matches!(e.event_type, TimelineEventType::HealingExecuted));

        if has_executed_heal {
            // Check if any failure followed a healing execution event
            let mut heal_seen = false;
            for event in timeline {
                if matches!(event.event_type, TimelineEventType::HealingExecuted) {
                    heal_seen = true;
                } else if heal_seen
                    && matches!(event.event_type, TimelineEventType::FailureObserved)
                {
                    return "Possible related regression".to_string();
                }
            }
        }

        // 3. Conservative default
        "No related regression observed".to_string()
    }

    fn correlate_non_causal_changes(
        &self,
        healing_history: &[HealingHistoryCorrelation],
        telemetry_records: &[ExecutionTelemetryRow],
    ) -> Vec<String> {
        let mut notes = Vec::new();

        for h in healing_history {
            // Find failure occurrences before and after healing proposal creation
            let before = telemetry_records
                .iter()
                .filter(|t| t.created_at < h.created_at)
                .count();
            let after = telemetry_records
                .iter()
                .filter(|t| t.created_at >= h.created_at)
                .count();

            if after > before && after >= 2 {
                notes.push(format!(
                    "Failure frequency increased after healing proposal {} ({}) (correlation only; direct causality not confirmed)",
                    h.proposal_id,
                    h.created_at.format("%Y-%m-%d %H:%M:%S UTC")
                ));
            }
        }

        notes
    }

    fn generate_operational_recommendations(
        &self,
        failure_category: &str,
        environment_matrix: &EnvironmentCorrelationMatrix,
        regression_status: &str,
        healing_history: &[HealingHistoryCorrelation],
        affected_components: &[String],
    ) -> Vec<String> {
        let mut recs = Vec::new();

        // Production occurrence warning
        let in_prod = environment_matrix
            .environments
            .iter()
            .any(|e| e.environment == "production");
        if in_prod {
            recs.push(
                "CRITICAL: Correlated failure observed in production environment; prioritize immediate diagnostic triage and quarantine."
                    .to_string(),
            );
        }

        // Cross-environment divergence
        if environment_matrix.cross_environment {
            recs.push(format!(
                "Cross-environment divergence detected across {} environments ({}); verify environment configuration parity, secrets, and mock behavior.",
                environment_matrix.environments.len(),
                environment_matrix.environments.iter().map(|e| e.environment.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }

        // Regression risk warning
        if regression_status == "Confirmed regression" {
            recs.push(
                "Previous automated healing introduced a confirmed regression; require manual engineering review before re-attempting automated self-healing."
                    .to_string(),
            );
        } else if regression_status == "Possible related regression" {
            recs.push(
                "Failures observed subsequent to automated healing execution; verify side-effects before advancing deployment pipeline."
                    .to_string(),
            );
        }

        // Prior healing failures
        if healing_history.iter().any(|h| h.state == "FAILED") {
            recs.push(
                "Prior automated healing attempts encountered execution failures; review AST patches for syntax or dependency mismatches before re-attempting."
                    .to_string(),
            );
        }

        // Category-specific guidance
        match failure_category {
            "SELECTOR_NOT_FOUND" | "SELECTOR_AMBIGUOUS" => {
                if !affected_components.is_empty() {
                    recs.push(format!(
                        "Brittle DOM selector identified in components [{}]; update locator strategy to semantic test-ids or ARIA accessibility roles.",
                        affected_components.join(", ")
                    ));
                } else {
                    recs.push(
                        "Brittle DOM selector identified; update locator strategy to semantic test-ids or ARIA accessibility roles."
                            .to_string(),
                    );
                }
            }
            "TIMEOUT" => {
                recs.push(
                    "High frequency of timeouts observed; inspect backend endpoint response latencies and consider bounded retry with exponential backoff."
                        .to_string(),
                );
            }
            "AUTHENTICATION_FAILURE" | "AUTHORIZATION_FAILURE" => {
                recs.push(
                    "Credential or permission failure detected; verify API keys, token expiration policies, and environment role bindings."
                        .to_string(),
                );
            }
            "NETWORK_FAILURE" => {
                recs.push(
                    "Network unreachable or connection reset errors; inspect infrastructure egress routes, proxy firewalls, and service meshes."
                        .to_string(),
                );
            }
            _ => {}
        }

        // Persistence recommendation
        if environment_matrix.recurrence_state == RecurrenceState::Persistent {
            recs.push(
                "Persistent failure pattern detected; root cause hypothesis indicates systematic defect rather than transient flakiness."
                    .to_string(),
            );
        } else if environment_matrix.recurrence_state == RecurrenceState::Resolved {
            recs.push(
                "Cluster verified resolved; monitor subsequent operational runs to confirm sustained stability."
                    .to_string(),
            );
        }

        if recs.is_empty() {
            recs.push(
                "Failure frequency is low and stable; standard regression monitoring recommended."
                    .to_string(),
            );
        }

        recs
    }
}
