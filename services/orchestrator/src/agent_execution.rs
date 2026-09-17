//! Agent Execution Dispatcher — Phase 4: Controlled Execution Infrastructure.
//!
//! Provides the authoritative orchestration layer between validated Agent plans
//! and the existing AIQA execution pipeline (engine, scheduler, executor registry,
//! policy, evidence store, and Quality Graph).
//!
//! Architectural safety rules:
//! - Closed Action AST and PlanValidator enforced before execution
//! - Multi-tenancy and RBAC verified on every dispatch/approval/rejection
//! - SSRF target validation enforced via `executor::validate_target_url`
//! - Existing `policy::evaluate` determines approval requirements
//! - Human approval gate holds dangerous/production runs in `PENDING_APPROVAL`
//! - Explicit execution budgets prevent infinite loops or task storms
//! - Quality Graph traceability links requirements -> test cases -> runs -> findings
//! - Structured observations return sanitized metrics without private CoT or secrets
//! - Idempotency key protection avoids accidental duplicate runs

use crate::auth::{AuthContext, Role};
use crate::domain::{RunState, TaskStatus};
use crate::engine::{self, RunRecord};
use crate::evidence::LocalEvidenceStore;
use crate::executor::ExecutorRegistry;
use crate::plan_validator::{AgentExecutionPlan, PlanValidationError, PlanValidator};
use crate::policy::{self, PolicyConfig, PolicyDecision};
use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Explicit resource and execution limits for Agent-initiated operations.
#[derive(Debug, Clone)]
pub struct AgentExecutionBudget {
    /// Maximum number of tasks permitted in a single plan (default: 50).
    pub max_tasks_per_plan: usize,
    /// Maximum duration permitted for any individual task in seconds (default: 300s / 5m).
    pub max_task_duration_secs: u64,
    /// Maximum total duration for an entire execution in seconds (default: 1800s / 30m).
    pub max_total_duration_secs: u64,
    /// Maximum concurrency allowed for an agent run (default: 8).
    pub max_concurrency: usize,
    /// Maximum evidence size in bytes (default: 10 MB).
    pub max_evidence_bytes: usize,
    /// Maximum retries allowed (strictly 0 for Phase 4; autonomous loops forbidden).
    pub max_retries: usize,
}

impl Default for AgentExecutionBudget {
    fn default() -> Self {
        Self {
            max_tasks_per_plan: 50,
            max_task_duration_secs: 300,
            max_total_duration_secs: 1800,
            max_concurrency: 8,
            max_evidence_bytes: 10 * 1024 * 1024,
            max_retries: 0,
        }
    }
}

/// Request to dispatch an Agent plan for execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecutionRequest {
    pub plan: AgentExecutionPlan,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub auto_approve_safe: bool,
}

/// Execution lifecycle states for an Agent plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentExecutionLifecycle {
    Planned,
    Validated,
    PolicyCheck,
    PendingApproval,
    Approved,
    Executing,
    Completed,
    Failed,
    Rejected,
    Cancelled,
}

impl AgentExecutionLifecycle {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planned => "PLANNED",
            Self::Validated => "VALIDATED",
            Self::PolicyCheck => "POLICY_CHECK",
            Self::PendingApproval => "PENDING_APPROVAL",
            Self::Approved => "APPROVED",
            Self::Executing => "EXECUTING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Rejected => "REJECTED",
            Self::Cancelled => "CANCELLED",
        }
    }
}

/// Response returned when an Agent plan is dispatched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecutionResponse {
    pub run_id: Uuid,
    pub project_id: String,
    pub lifecycle_state: AgentExecutionLifecycle,
    pub policy_decision: String,
    pub requires_human_approval: bool,
    pub policy_notes: Option<String>,
    pub tasks_count: usize,
    pub message: String,
}

/// Operational summary of a finding derived from task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFindingSummary {
    pub task_id: String,
    pub task_name: String,
    pub status: String,
    pub reason: Option<String>,
    pub requirement_reference: Option<String>,
}

/// Safe, sanitized observation returned to the Agent after execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecutionObservation {
    pub run_id: String,
    pub project_id: String,
    pub status: String,
    pub tasks_total: usize,
    pub tasks_passed: usize,
    pub tasks_failed: usize,
    pub tasks_blocked: usize,
    pub duration_ms: u64,
    pub findings: Vec<AgentFindingSummary>,
    pub evidence_refs: Vec<String>,
}

/// Error returned when dispatch or execution fails.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentExecutionError {
    ValidationError(Vec<PlanValidationError>),
    Unauthorized(String),
    CrossProjectForbidden(String),
    PolicyDenied(String),
    NotFound(String),
    Conflict(String),
    BudgetExceeded(String),
    StorageError(String),
    Internal(String),
}

impl std::fmt::Display for AgentExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ValidationError(errs) => write!(f, "plan validation failed: {errs:?}"),
            Self::Unauthorized(msg) => write!(f, "unauthorized: {msg}"),
            Self::CrossProjectForbidden(msg) => write!(f, "cross-project forbidden: {msg}"),
            Self::PolicyDenied(msg) => write!(f, "policy denied: {msg}"),
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::Conflict(msg) => write!(f, "conflict: {msg}"),
            Self::BudgetExceeded(msg) => write!(f, "budget exceeded: {msg}"),
            Self::StorageError(msg) => write!(f, "storage error: {msg}"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for AgentExecutionError {}

/// Authoritative dispatcher for Agent execution plans.
#[derive(Clone)]
pub struct AgentExecutionDispatcher {
    pub store: Store,
    pub policy: PolicyConfig,
    pub registry: Arc<ExecutorRegistry>,
    pub evidence: Arc<LocalEvidenceStore>,
    pub budget: AgentExecutionBudget,
    pub runs: Arc<RwLock<HashMap<Uuid, Arc<RunRecord>>>>,
    idempotency_cache: Arc<RwLock<HashMap<String, (Uuid, AgentExecutionResponse)>>>,
}

impl AgentExecutionDispatcher {
    pub fn new(
        store: Store,
        policy: PolicyConfig,
        registry: Arc<ExecutorRegistry>,
        evidence: Arc<LocalEvidenceStore>,
        budget: Option<AgentExecutionBudget>,
    ) -> Self {
        Self {
            store,
            policy,
            registry,
            evidence,
            budget: budget.unwrap_or_default(),
            runs: Arc::new(RwLock::new(HashMap::new())),
            idempotency_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Dispatches an Agent plan through validation, policy evaluation, approval gate,
    /// Quality Graph persistence, and execution scheduling.
    pub async fn dispatch(
        &self,
        ctx: &AuthContext,
        request: AgentExecutionRequest,
    ) -> Result<AgentExecutionResponse, AgentExecutionError> {
        let plan = &request.plan;
        let project_id = plan.project_id.trim();

        // 1. Multi-Tenant Authorization & RBAC Enforcement
        if !ctx.role.at_least(Role::Operator) {
            return Err(AgentExecutionError::Unauthorized(format!(
                "role '{}' is not authorized to dispatch execution plans; requires operator or admin",
                ctx.role.as_str()
            )));
        }

        if let Some(ref auth_pid) = ctx.project_id {
            if auth_pid != project_id {
                let _ = self
                    .store
                    .insert_audit(
                        Some(&ctx.name),
                        Some(ctx.role.as_str()),
                        Some(project_id),
                        "agent_execution.cross_project_deny",
                        None,
                        Some("cross-project access violation blocked"),
                        "denied",
                    )
                    .await;
                return Err(AgentExecutionError::CrossProjectForbidden(format!(
                    "cross-project access violation: key scoped to '{auth_pid}' cannot execute plan on '{project_id}'"
                )));
            }
        }

        // 2. Idempotency Check
        if let Some(ref ikey) = request.idempotency_key {
            let cache_key = format!("{project_id}:{ikey}");
            let cache = self.idempotency_cache.read().await;
            if let Some((_cached_id, cached_resp)) = cache.get(&cache_key) {
                return Ok(cached_resp.clone());
            }
        }

        // 3. Plan Validation Boundary
        let validated = PlanValidator::validate(
            plan,
            self.budget.max_tasks_per_plan,
            self.budget.max_task_duration_secs,
            self.budget.max_concurrency,
        )
        .await
        .map_err(AgentExecutionError::ValidationError)?;

        // 4. Convert to Authoritative RunPlan
        let run_plan = validated
            .to_run_plan()
            .map_err(AgentExecutionError::Internal)?;

        // 5. Policy Gate Evaluation
        let policy_decision = policy::evaluate(&self.policy, &run_plan);

        // Production environment safety rule: agent executions in production always require approval
        let is_prod = validated.environment == "production" || validated.environment == "prod";
        let (initial_state, policy_notes, requires_human_approval, decision_label) = match policy_decision {
            PolicyDecision::Deny(reason) => {
                let _ = self
                    .store
                    .insert_audit(
                        Some(&ctx.name),
                        Some(ctx.role.as_str()),
                        Some(project_id),
                        "agent_execution.policy_deny",
                        None,
                        Some(&reason),
                        "denied",
                    )
                    .await;
                return Err(AgentExecutionError::PolicyDenied(format!(
                    "policy engine rejected agent execution plan: {reason}"
                )));
            }
            PolicyDecision::RequireApproval(reason) => (
                RunState::PendingApproval,
                Some(reason.clone()),
                true,
                format!("REQUIRE_APPROVAL: {reason}"),
            ),
            PolicyDecision::Allow if is_prod => (
                RunState::PendingApproval,
                Some("agent-triggered execution in production environment requires human approval".to_string()),
                true,
                "REQUIRE_APPROVAL: production environment protection".to_string(),
            ),
            PolicyDecision::Allow => (RunState::Scheduled, None, false, "ALLOW".to_string()),
        };

        // 6. Quality Graph Integration & Traceability
        for task in &validated.tasks {
            if let Some(ref req_ref) = task.requirement_reference {
                if let Ok(req_node_id) = self
                    .store
                    .upsert_quality_node(project_id, "requirement", req_ref, req_ref)
                    .await
                {
                    if let Ok(case_node_id) = self
                        .store
                        .upsert_quality_node(project_id, "test_case", &task.name, &task.name)
                        .await
                    {
                        let _ = self
                            .store
                            .upsert_quality_relation(case_node_id, req_node_id, "validates")
                            .await;
                    }
                }
            }
        }

        // 7. Persist Run & Tasks
        let run_id = Uuid::new_v4();
        self.store
            .insert_run(
                run_id,
                &run_plan,
                Some(project_id),
                Some(&ctx.name),
                initial_state,
                policy_notes.as_deref(),
            )
            .await
            .map_err(|e| AgentExecutionError::StorageError(format!("failed to insert run: {e}")))?;

        // 8. Construct RunRecord
        let record = RunRecord::new(
            run_id,
            &run_plan,
            Some(project_id.to_string()),
            Some(ctx.name.clone()),
            initial_state,
            policy_notes.clone(),
            None,
        );

        self.runs.write().await.insert(run_id, record.clone());

        // 9. Dispatch Execution or Hold for Approval
        let (lifecycle_state, message) = if requires_human_approval {
            let _ = self
                .store
                .insert_audit(
                    Some(&ctx.name),
                    Some(ctx.role.as_str()),
                    Some(project_id),
                    "agent_execution.held_for_approval",
                    Some(&run_id.to_string()),
                    policy_notes.as_deref(),
                    "ok",
                )
                .await;

            (
                AgentExecutionLifecycle::PendingApproval,
                format!(
                    "Plan validated and held in PENDING_APPROVAL: {}",
                    policy_notes.as_deref().unwrap_or("human approval required")
                ),
            )
        } else {
            let task_timeout = std::time::Duration::from_secs(self.budget.max_task_duration_secs);
            engine::spawn_run(
                record.clone(),
                self.registry.clone(),
                run_plan.concurrency,
                self.store.clone(),
                self.evidence.clone(),
                task_timeout,
            );

            let _ = self
                .store
                .insert_audit(
                    Some(&ctx.name),
                    Some(ctx.role.as_str()),
                    Some(project_id),
                    "agent_execution.dispatched",
                    Some(&run_id.to_string()),
                    None,
                    "ok",
                )
                .await;

            (
                AgentExecutionLifecycle::Executing,
                format!(
                    "Plan validated and scheduled for execution ({} tasks)",
                    run_plan.tasks.len()
                ),
            )
        };

        let response = AgentExecutionResponse {
            run_id,
            project_id: project_id.to_string(),
            lifecycle_state,
            policy_decision: decision_label,
            requires_human_approval,
            policy_notes,
            tasks_count: run_plan.tasks.len(),
            message,
        };

        // 10. Cache for Idempotency
        if let Some(ref ikey) = request.idempotency_key {
            let cache_key = format!("{project_id}:{ikey}");
            self.idempotency_cache
                .write()
                .await
                .insert(cache_key, (run_id, response.clone()));
        }

        Ok(response)
    }

    /// Releases a run held in `PENDING_APPROVAL` after human authorization.
    pub async fn approve(
        &self,
        ctx: &AuthContext,
        run_id: Uuid,
    ) -> Result<AgentExecutionResponse, AgentExecutionError> {
        if !ctx.role.at_least(Role::Operator) {
            return Err(AgentExecutionError::Unauthorized(
                "only operators and admins can approve held executions".to_string(),
            ));
        }

        let record = self.resolve_or_rehydrate_run(run_id).await?;
        let project_id = record
            .project_id
            .clone()
            .unwrap_or_else(|| "default".to_string());

        if let Some(ref auth_pid) = ctx.project_id {
            if auth_pid != &project_id {
                return Err(AgentExecutionError::CrossProjectForbidden(
                    "cannot approve execution belonging to a different project".to_string(),
                ));
            }
        }

        {
            let state = *record.state.read().await;
            if state != RunState::PendingApproval {
                return Err(AgentExecutionError::Conflict(format!(
                    "run is in state {:?}, not PENDING_APPROVAL",
                    state
                )));
            }
        }

        let task_timeout = std::time::Duration::from_secs(self.budget.max_task_duration_secs);
        engine::approve_and_spawn(
            record.clone(),
            self.registry.clone(),
            record.plan.concurrency,
            self.store.clone(),
            self.evidence.clone(),
            &ctx.name,
            task_timeout,
        )
        .await
        .map_err(|e| AgentExecutionError::Internal(format!("approval failed: {e}")))?;

        let _ = self
            .store
            .insert_audit(
                Some(&ctx.name),
                Some(ctx.role.as_str()),
                Some(&project_id),
                "agent_execution.approve",
                Some(&run_id.to_string()),
                None,
                "ok",
            )
            .await;

        Ok(AgentExecutionResponse {
            run_id,
            project_id,
            lifecycle_state: AgentExecutionLifecycle::Executing,
            policy_decision: "APPROVED_BY_HUMAN".to_string(),
            requires_human_approval: false,
            policy_notes: None,
            tasks_count: record.plan.tasks.len(),
            message: format!("Execution approved by {} and dispatched", ctx.name),
        })
    }

    /// Rejects a run held in `PENDING_APPROVAL`.
    pub async fn reject(
        &self,
        ctx: &AuthContext,
        run_id: Uuid,
        reason: &str,
    ) -> Result<AgentExecutionResponse, AgentExecutionError> {
        if !ctx.role.at_least(Role::Operator) {
            return Err(AgentExecutionError::Unauthorized(
                "only operators and admins can reject held executions".to_string(),
            ));
        }

        let record = self.resolve_or_rehydrate_run(run_id).await?;
        let project_id = record
            .project_id
            .clone()
            .unwrap_or_else(|| "default".to_string());

        if let Some(ref auth_pid) = ctx.project_id {
            if auth_pid != &project_id {
                return Err(AgentExecutionError::CrossProjectForbidden(
                    "cannot reject execution belonging to a different project".to_string(),
                ));
            }
        }

        {
            let state = *record.state.read().await;
            if state != RunState::PendingApproval {
                return Err(AgentExecutionError::Conflict(format!(
                    "run is in state {:?}, not PENDING_APPROVAL",
                    state
                )));
            }
        }

        engine::reject_run(record.clone(), &self.store, &ctx.name, reason)
            .await
            .map_err(|e| AgentExecutionError::Internal(format!("rejection failed: {e}")))?;

        let _ = self
            .store
            .insert_audit(
                Some(&ctx.name),
                Some(ctx.role.as_str()),
                Some(&project_id),
                "agent_execution.reject",
                Some(&run_id.to_string()),
                Some(reason),
                "ok",
            )
            .await;

        Ok(AgentExecutionResponse {
            run_id,
            project_id,
            lifecycle_state: AgentExecutionLifecycle::Rejected,
            policy_decision: format!("REJECTED: {reason}"),
            requires_human_approval: false,
            policy_notes: Some(reason.to_string()),
            tasks_count: record.plan.tasks.len(),
            message: format!("Execution rejected by {}: {}", ctx.name, reason),
        })
    }

    /// Constructs a safe, sanitized observation from execution outcomes and evidence.
    pub async fn get_observation(
        &self,
        ctx: &AuthContext,
        run_id: Uuid,
    ) -> Result<AgentExecutionObservation, AgentExecutionError> {
        let summary = self
            .store
            .get_run_summary(run_id)
            .await
            .map_err(|e| AgentExecutionError::StorageError(e.to_string()))?
            .ok_or_else(|| AgentExecutionError::NotFound(format!("run '{run_id}' not found")))?;

        let project_id = summary
            .project_id
            .clone()
            .unwrap_or_else(|| "default".to_string());

        if let Some(ref auth_pid) = ctx.project_id {
            if auth_pid != &project_id {
                return Err(AgentExecutionError::CrossProjectForbidden(
                    "cannot observe execution belonging to a different project".to_string(),
                ));
            }
        }

        let tasks = self
            .store
            .list_tasks_for_run(run_id)
            .await
            .map_err(|e| AgentExecutionError::StorageError(e.to_string()))?;

        let mut findings = Vec::new();
        for t in &tasks {
            if matches!(
                t.status,
                TaskStatus::Failed | TaskStatus::Timeout | TaskStatus::InfraError
            ) {
                findings.push(AgentFindingSummary {
                    task_id: t.id.clone(),
                    task_name: t.name.clone(),
                    status: t.status.as_db_str().to_string(),
                    reason: t.reason.clone(),
                    requirement_reference: None,
                });
            }
        }

        let evidence_rows = self
            .store
            .list_evidence_for_run(run_id)
            .await
            .unwrap_or_default();

        let evidence_refs: Vec<String> = evidence_rows.into_iter().map(|e| e.object_key).collect();

        let duration_ms = match (summary.started_at, summary.finished_at) {
            (start, Some(end)) if end >= start => (end - start).num_milliseconds().max(0) as u64,
            _ => 0,
        };

        let mut observation = AgentExecutionObservation {
            run_id: run_id.to_string(),
            project_id,
            status: summary.state.as_db_str().to_string(),
            tasks_total: summary.total,
            tasks_passed: summary.passed,
            tasks_failed: summary.failed,
            tasks_blocked: summary.blocked,
            duration_ms,
            findings,
            evidence_refs,
        };

        // Redact any leaked tokens or sensitive keys
        if let Ok(mut obs_val) = serde_json::to_value(&observation) {
            crate::tools::security::sanitize_secrets(&mut obs_val);
            if let Ok(sanitized) = serde_json::from_value(obs_val) {
                observation = sanitized;
            }
        }

        Ok(observation)
    }

    /// Resolves an in-memory `RunRecord` or rehydrates it from persistent storage.
    async fn resolve_or_rehydrate_run(
        &self,
        run_id: Uuid,
    ) -> Result<Arc<RunRecord>, AgentExecutionError> {
        if let Some(record) = self.runs.read().await.get(&run_id).cloned() {
            return Ok(record);
        }

        let rehydration = self
            .store
            .get_run_for_rehydration(run_id)
            .await
            .map_err(|e| AgentExecutionError::StorageError(e.to_string()))?;

        if let Some((plan, project_id, requested_by, policy_notes, state)) = rehydration {
            let record = RunRecord::new(
                run_id,
                &plan,
                project_id,
                requested_by,
                state,
                policy_notes,
                None,
            );
            self.runs.write().await.insert(run_id, record.clone());
            Ok(record)
        } else {
            Err(AgentExecutionError::NotFound(format!(
                "run '{run_id}' not found"
            )))
        }
    }
}
