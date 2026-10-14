//! AIQA Phase 10 (Batch 4) — QA Copilot: API Endpoint, RBAC Governance & Pipeline Integration.
//!
//! # Roadmap Reference
//! `AIQA_NEXT_ROADMAP_PHASED_PLAN.md` §"PHASE 10 — AI QA COPILOT" (L369–431)
//!
//! ## Core Objective
//! Expose the conversational AI QA Copilot via a project-scoped, authenticated,
//! RBAC-governed REST API endpoint:
//!
//! `POST /api/v1/projects/:project_id/copilot/ask`
//!
//! ## Governance Pipeline
//! 1. **Authentication:** Bearer token validation through existing `require_auth` middleware.
//! 2. **Project Scoping:** Project existence and tenant isolation checked via `require_scoped_project`.
//! 3. **RBAC Guard:** `RequireViewer` extractor ensures minimum `Role::Viewer` before request processing.
//! 4. **Input Validation:** Non-empty, whitespace-trimmed, bounded to 1,000 characters.
//! 5. **Intent Classification:** Deterministic classification via Batch 1 (`classify_query`).
//! 6. **Fast-path Bypass:** Unsupported and clarification-needed queries bypass retrieval and synthesis.
//! 7. **Project-Scoped Evidence Retrieval:** Bounded coordinator reads via Batch 2 (`CopilotEvidenceCoordinator`).
//! 8. **Grounded Answer Synthesis:** Fact/inference separation and citation validation via Batch 3 (`CopilotSynthesizer`).
//! 9. **Execution Timeout:** 30-second bounded cancellation prevents long-hanging model calls.

use crate::api::require_scoped_project;
use crate::auth::{AuthContext, RequireViewer};
use crate::copilot_intent::{classify_question, ClassificationResult};
use crate::copilot_retrieval::CopilotEvidenceCoordinator;
use crate::copilot_synthesizer::{CopilotResponse, CopilotResponseStatus, CopilotSynthesizer};
use crate::errors::{ApiError, ApiJson, ApiPath};
use crate::state::AppState;
use axum::{extract::State, response::Json, Extension};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

/// Maximum allowable length for incoming question text.
pub const MAX_QUESTION_LENGTH: usize = 1000;

/// Default timeout duration for complete copilot request processing.
pub const COPILOT_REQUEST_TIMEOUT_SECS: u64 = 30;

/// Request payload for the Copilot endpoint.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AskCopilotRequest {
    /// The user's natural language question regarding QA evidence or intelligence.
    pub question: String,
    /// Optional context window parameter (for future multi-turn tracking).
    #[serde(default)]
    pub context_window: Option<String>,
}

/// Handler for `POST /api/v1/projects/:project_id/copilot/ask`.
///
/// Implements full RBAC, project-level multi-tenant isolation, query classification,
/// evidence retrieval, and grounded synthesis with strict timeout containment.
pub async fn ask_copilot_handler(
    _authz: RequireViewer,
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(payload): ApiJson<AskCopilotRequest>,
) -> Result<Json<CopilotResponse>, ApiError> {
    // 1. Verify project existence and enforce project-scoped access boundary
    require_scoped_project(&state, &ctx, project_id).await?;

    // 2. Validate input question
    let query_text = payload.question.trim();
    if query_text.is_empty() {
        return Err(ApiError::validation("question cannot be empty"));
    }
    if query_text.len() > MAX_QUESTION_LENGTH {
        return Err(ApiError::validation(format!(
            "question exceeds maximum length of {MAX_QUESTION_LENGTH} characters"
        )));
    }

    let project_id_str = project_id.to_string();

    // 3. Execute bounded pipeline within cancellation timeout
    let response = timeout(Duration::from_secs(COPILOT_REQUEST_TIMEOUT_SECS), async {
        // Step A: Intent classification (pure in-memory, deterministic)
        let classification = classify_question(query_text);

        match &classification {
            // Short-circuit: clarification needed requires 0 DB queries and 0 model calls
            ClassificationResult::ClarificationRequired {
                reason,
                suggested_clarification,
                ..
            } => Ok(CopilotResponse {
                status: CopilotResponseStatus::ClarificationNeeded,
                project_id: project_id_str,
                summary: format!("Please clarify: {suggested_clarification}"),
                facts: Vec::new(),
                inferences: Vec::new(),
                citations: Vec::new(),
                freshness_note: None,
                insufficient_reason: Some(format!(
                    "{reason}. Suggestion: {suggested_clarification}"
                )),
            }),

            // Short-circuit: unsupported query requires 0 DB queries and 0 model calls
            ClassificationResult::UnsupportedQuery { reason, .. } => Ok(CopilotResponse {
                status: CopilotResponseStatus::Unsupported,
                project_id: project_id_str,
                summary: format!("Unsupported question: {reason}"),
                facts: Vec::new(),
                inferences: Vec::new(),
                citations: Vec::new(),
                freshness_note: None,
                insufficient_reason: Some(reason.clone()),
            }),

            // Valid classified intent: proceed to project-scoped evidence retrieval & synthesis
            ClassificationResult::Classified { intent, .. } => {
                let coordinator = CopilotEvidenceCoordinator::new(state.store.clone());
                let evidence = coordinator
                    .retrieve_for_classification(&project_id_str, &classification)
                    .await
                    .map_err(|e| ApiError::internal(format!("evidence retrieval failed: {e}")))?;

                let synthesizer = CopilotSynthesizer::with_infra(&state.ai_infra);
                let synthesized = synthesizer.synthesize(intent, &evidence).await;
                Ok(synthesized)
            }
        }
    })
    .await
    .map_err(|_| {
        ApiError::new(
            axum::http::StatusCode::GATEWAY_TIMEOUT,
            "TIMEOUT",
            format!("Copilot request processing exceeded {COPILOT_REQUEST_TIMEOUT_SECS} seconds"),
        )
    })??;

    Ok(Json(response))
}
