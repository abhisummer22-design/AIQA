use crate::auth::{self, AuthContext, Role};
use crate::domain::*;
use crate::engine::{self, RunRecord};
use crate::errors::{ApiError, ApiJson, ApiPath, ApiQuery};
use crate::log_bus::OrchestratorLogEntry;
use crate::policy::{self, PolicyDecision};
use crate::scheduler::validate_plan;
use crate::state::AppState;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    // BE-10: `Path`/`Query` are kept imported for `project_events_ws_handler`
    // only — its pre-upgrade rejections are deliberately not converted to
    // `ApiError` (see `errors.rs`'s module doc comment; G-WS is out of
    // scope). Every other handler below uses `ApiPath`/`ApiQuery`/`ApiJson`
    // instead.
    extract::{Extension, Path, Query, State},
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use uuid::Uuid;

/// `/health` is intentionally the only route outside the auth layer (see
/// `router` below) — basic liveness checks stay unauthenticated, matching
/// common orchestrator/k8s conventions. The project events WebSocket is
/// also mounted outside the header-based auth layer for a different
/// reason: browsers cannot set an `Authorization` header on a WebSocket
/// handshake, so it authenticates itself via a `?key=` query parameter
/// instead (see `project_events_ws_handler`).
pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        // The caller's own real identity/role/scope.
        .route("/api/v1/auth/whoami", get(whoami))
        // The real coordination surface behind the Orchestrator view:
        // executor capability routing, policy gates, and the live DAG.
        .route("/api/v1/orchestrator/state", get(get_orchestrator_state))
        // Real mobile fleet: real adb devices, real AVDs, real Maestro CLI.
        .route("/api/v1/mobile/fleet", get(get_mobile_fleet))
        .route("/api/v1/mobile/emulators/boot", post(boot_mobile_emulator))
        .route(
            "/api/v1/mobile/simulators/boot",
            post(boot_ios_simulator_handler),
        )
        // Real stage-by-stage pipeline state, including honestly-reported
        // unavailable stages (vector store / RAG / re-planning).
        .route(
            "/api/v1/projects/:project_id/pipeline",
            get(get_neural_pipeline),
        )
        // Real, persisted custom skills for AI test-case generation (see
        // `store::insert_custom_skill`'s doc comment).
        .route(
            "/api/v1/skills",
            post(create_custom_skill).get(list_custom_skills_handler),
        )
        .route("/api/v1/skills/:id", delete(delete_custom_skill_handler))
        // Real, persisted custom AI providers a user adds through the
        // Settings UI (see `store::insert_custom_ai_provider`'s doc
        // comment) — live on the next discovery cycle, no restart needed.
        .route(
            "/api/v1/ai/providers/custom",
            post(create_custom_ai_provider).get(list_custom_ai_providers_handler),
        )
        .route(
            "/api/v1/ai/providers/custom/:id",
            delete(delete_custom_ai_provider_handler),
        )
        // The real instruction each skill resolves to — same function the
        // generator uses, so the UI cannot show a drifted copy.
        .route("/api/v1/skills/resolve", post(resolve_skills))
        // Real SKILL.md files a user has dropped into
        // `skill_files::skills_dir()` on this machine — re-read from disk on
        // every call (see that module's doc comment), never cached, so a
        // file added after the server started shows up without a restart.
        .route("/api/v1/skills/files", get(list_file_skills_handler))
        // BE-8 — API Contract §5.1-5.4.
        .route(
            "/api/v1/projects",
            post(create_project).get(list_projects_handler),
        )
        .route(
            "/api/v1/projects/:id",
            get(get_project_handler)
                .patch(update_project_handler)
                .delete(delete_project_handler),
        )
        // BE-12 — API Contract §23.1-23.2 Dashboard/KPI JSON endpoints,
        // project-scoped the same way as the executions/evidence routes
        // below (`require_scoped_project`).
        .route(
            "/api/v1/projects/:project_id/dashboard",
            get(get_project_dashboard),
        )
        .route("/api/v1/projects/:project_id/kpis", get(get_project_kpis))
        // "Actual BE-9" (Integration Plan's own numbering) — project-scoped
        // routing for the execution/evidence resources. See the module doc
        // comment above `require_scoped_project` for exactly which routes
        // moved here and why the rest (bug-drafts/intelligence/audit-log,
        // admin/api-keys, metrics) did not — full reasoning in
        // AIQA_PHASE2_ACTUAL_BE9_PROJECT_SCOPED_ROUTING_REPORT.md §2.
        .route(
            "/api/v1/projects/:project_id/executions",
            post(create_run_scoped).get(list_runs_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/executions/:run_id",
            get(get_run_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/executions/:run_id/events",
            get(get_events_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/executions/:run_id/approve",
            post(approve_run_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/executions/:run_id/reject",
            post(reject_run_scoped),
        )
        // BE-13 — API Contract §12.3-12.5. Cancel + the standalone task list
        // /detail endpoints (previously only reachable embedded inside
        // `GET .../executions/:run_id`'s `tasks` array).
        .route(
            "/api/v1/projects/:project_id/executions/:run_id/cancel",
            post(cancel_run_scoped),
        )
        // Real re-planning v1 — see `retry_failed_tasks_scoped`'s own doc
        // comment for exactly what "retry" means here.
        .route(
            "/api/v1/projects/:project_id/executions/:run_id/retry-failed",
            post(retry_failed_tasks_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/executions/:run_id/tasks",
            get(list_run_tasks_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/executions/:run_id/tasks/:task_id",
            get(get_run_task_detail_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/executions/:run_id/evidence",
            get(list_evidence_scoped),
        )
        // BE-15 — API Contract §15.2/§16/§25. `/evidence/:evidence_id` now
        // returns metadata (Detail), not raw bytes — see
        // `get_evidence_detail_scoped`'s doc comment; the bytes moved to
        // the new `/content` sibling, not removed.
        .route(
            "/api/v1/projects/:project_id/evidence/:evidence_id",
            get(get_evidence_detail_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/evidence/:evidence_id/content",
            get(get_evidence_content_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/findings",
            get(list_findings_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/findings/:finding_id",
            get(get_finding_detail_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/findings/:finding_id/analysis",
            post(get_finding_analysis_scoped),
        )
        // Build order item 4 — on-demand AI-assisted hypothesis, additive
        // to the real, deterministic `.../analysis` above (never a
        // replacement for it). See `generate_finding_ai_hypothesis_scoped`.
        .route(
            "/api/v1/projects/:project_id/findings/:finding_id/ai-hypothesis",
            post(generate_finding_ai_hypothesis_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/findings/:finding_id/review",
            post(review_finding_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/audit",
            get(get_audit_log_scoped),
        )
        // Phase 4 BE-10 — see `list_test_intelligence_scoped`'s doc comment.
        .route(
            "/api/v1/projects/:project_id/intelligence/tests",
            get(list_test_intelligence_scoped),
        )
        // Phase 4 (requirement graph) — see `get_requirement_impact_scoped`'s
        // doc comment.
        .route(
            "/api/v1/projects/:project_id/requirement-impact",
            get(get_requirement_impact_scoped),
        )
        // Gap-closing — real TestPlanningAgent (Phase 3) wiring, grounded in
        // real Phase 5 healing-strategy calibration and Phase 6 adaptive
        // intelligence risk signals. See `generate_test_plan_scoped`'s doc
        // comment.
        .route(
            "/api/v1/projects/:project_id/planning/test-plan",
            post(generate_test_plan_scoped),
        )
        // Phase 6: Adaptive QA Intelligence & Observability endpoints
        .route(
            "/api/v1/projects/:project_id/intelligence/telemetry",
            get(get_project_telemetry_handler),
        )
        .route(
            "/api/v1/projects/:project_id/intelligence/failures",
            get(get_project_failure_clusters_handler),
        )
        .route(
            "/api/v1/projects/:project_id/intelligence/flakiness",
            get(get_project_flakiness_handler),
        )
        .route(
            "/api/v1/projects/:project_id/intelligence/risk",
            get(get_project_risk_handler),
        )
        .route(
            "/api/v1/projects/:project_id/intelligence/strategies",
            get(get_project_strategies_handler),
        )
        .route(
            "/api/v1/projects/:project_id/intelligence/trends",
            get(get_project_trends_handler),
        )
        .route(
            "/api/v1/projects/:project_id/intelligence/quality-graph",
            get(get_project_quality_graph_handler),
        )
        // Phase 8: Intelligent QA Operations & Cross-Environment Observability
        .route(
            "/api/v1/projects/:project_id/intelligence/investigations",
            get(list_failure_investigations_handler),
        )
        .route(
            "/api/v1/projects/:project_id/intelligence/failures/:cluster_id/investigation",
            get(get_failure_investigation_handler),
        )
        .route(
            "/api/v1/projects/:project_id/intelligence/failures/:cluster_id/environments",
            get(get_cluster_environments_handler),
        )
        .route(
            "/api/v1/projects/:project_id/intelligence/failures/:cluster_id/timeline",
            get(get_cluster_timeline_handler),
        )
        .route(
            "/api/v1/projects/:project_id/intelligence/retention/dry-run",
            post(retention_dry_run_handler),
        )
        .route(
            "/api/v1/projects/:project_id/intelligence/retention/prune",
            post(retention_prune_handler),
        )
        // Phase 6 (crawler) — see `start_crawl_scoped`'s doc comment.
        .route(
            "/api/v1/projects/:project_id/crawl",
            post(start_crawl_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/crawl/:run_id/result",
            get(get_crawl_result_scoped),
        )
        // AUD-005 — real, persisted Test Case CRUD + lifecycle + execution
        // handoff. See the "test cases" section below for every handler.
        .route(
            "/api/v1/projects/:project_id/test-cases",
            post(create_test_case_scoped).get(list_test_cases_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/test-cases/:id",
            get(get_test_case_scoped)
                .patch(update_test_case_scoped)
                .delete(delete_test_case_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/test-cases/:id/validate",
            post(validate_test_case_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/test-cases/:id/approve",
            post(approve_test_case_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/test-cases/:id/reject",
            post(reject_test_case_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/test-cases/:id/handoff",
            post(handoff_test_case_scoped),
        )
        // Phase 2.1 — real, read-only compilation preview (no state mutation).
        .route(
            "/api/v1/projects/:project_id/test-cases/:id/compile",
            post(compile_test_case_scoped),
        )
        // Phase 5 (Output maturity) — plan -> suite -> case hierarchy. See
        // the "test plans / test suites" section for every handler.
        .route(
            "/api/v1/projects/:project_id/test-cases/:id/suite",
            put(set_test_case_suite_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/test-plans",
            post(create_test_plan_scoped).get(list_test_plans_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/test-plans/:id",
            get(get_test_plan_scoped)
                .patch(update_test_plan_scoped)
                .delete(delete_test_plan_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/test-suites",
            post(create_test_suite_scoped).get(list_test_suites_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/test-suites/:id",
            get(get_test_suite_scoped)
                .patch(update_test_suite_scoped)
                .delete(delete_test_suite_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/test-suites/:id/plan",
            put(set_test_suite_plan_scoped),
        )
        // Phase 2 — real, skill-aware AI test case generation. Produces
        // drafts only (never persists) — the caller then uses the existing
        // `POST .../test-cases` (above) to save whichever drafts it wants,
        // exactly like the pre-existing local-template drafts already do.
        .route(
            "/api/v1/projects/:project_id/test-cases/generate-ai",
            post(generate_ai_test_cases_scoped),
        )
        // Phase 3 — Custom Testing: real requirement -> AI strategy ->
        // test cases (`custom_testing.rs`). Produces drafts only, exactly
        // like `generate-ai` above — the caller saves whichever drafts it
        // wants via the same existing `POST .../test-cases`, no second
        // persistence path.
        .route(
            "/api/v1/projects/:project_id/custom-testing/generate",
            post(generate_custom_testing_scoped),
        )
        // Phase 4 — API Testing page: real, ad-hoc HTTP request execution
        // (`api_testing.rs`). Reuses the exact Phase 2.1 execution pipeline
        // (ExecutionPlan -> validate_plan -> materialize_task_def ->
        // create_run_core -> CompiledApiExecutor) — no new executor, no new
        // execution engine.
        .route(
            "/api/v1/projects/:project_id/api-testing/execute",
            post(execute_api_test_scoped),
        )
        // Phase 5 — Governed Self-Healing & Quality Feedback
        .route(
            "/api/v1/projects/:project_id/healing/proposals",
            post(create_healing_proposal_scoped).get(list_healing_proposals_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/healing/proposals/:proposal_id",
            get(get_healing_proposal_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/healing/proposals/:proposal_id/diff",
            get(get_healing_proposal_diff_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/healing/proposals/:proposal_id/approve",
            post(approve_healing_proposal_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/healing/proposals/:proposal_id/reject",
            post(reject_healing_proposal_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/healing/proposals/:proposal_id/execute",
            post(execute_healing_proposal_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/healing/proposals/:proposal_id/verification",
            get(get_healing_verification_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/healing/feedback",
            get(get_healing_feedback_scoped),
        )
        // Truthfulness Cleanup Step 1 — real Application Source connectivity
        // verification (`app_source.rs`). Replaces the Test Case Generator's
        // former fully client-side setTimeout+Math.random() simulation
        // (flagged CRITICAL by the Phase 1-6 Final Consolidated Audit) with
        // a genuine backend check reusing the same, unmodified SSRF guard
        // and transport-error classification every other outbound target in
        // this app already uses.
        .route(
            "/api/v1/projects/:project_id/verify-application-source",
            post(verify_application_source_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/documents/extract-text",
            post(extract_document_text_scoped),
        )
        // Phase 6 — Performance Testing: real Load/Stress/Spike/Volume
        // workload generation (`performance_testing.rs`). Reuses the exact
        // Phase 4 per-request execution core (`executor::execute_plan_once`,
        // extracted from `CompiledApiExecutor` in this same phase) as the
        // repeated worker underneath a new, minimal concurrency-bounded
        // scheduler — no second HTTP execution engine.
        .route(
            "/api/v1/projects/:project_id/performance-testing/start",
            post(start_performance_test_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/performance-testing",
            get(list_performance_runs_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/performance-testing/:run_id/status",
            get(get_performance_run_status_scoped),
        )
        .route(
            "/api/v1/projects/:project_id/performance-testing/:run_id/cancel",
            post(cancel_performance_run_scoped),
        )
        // Central AI Provider & Model Intelligence Layer — platform-wide,
        // not project-scoped (the router itself is one shared instance
        // across the whole deployment, exactly like `/admin/api-keys`
        // below is a platform-wide, not project-scoped, resource).
        .route("/api/v1/ai/status", get(get_ai_status))
        .route(
            "/api/v1/ai/settings",
            get(get_ai_settings_handler).put(put_ai_settings_handler),
        )
        .route("/api/v1/ai/refresh", post(refresh_ai_providers))
        // Phase B cleanup — the legacy flat, unscoped routes (`/runs*`,
        // `/evidence/:id/content`, `/intelligence/*`, `/bug-drafts*`,
        // `/runs/:id/ws`) are removed: every caller moved to the scoped
        // `/api/v1/projects/:project_id/...` contract above long ago and a
        // frontend test asserts the flat paths are never called. Kept from
        // this block: `/metrics` (a live frontend consumer reads its raw
        // Prometheus text body) and `/admin/*` (the only API-key
        // provisioning path in the system — FE-2's scoped admin UI is not
        // built; removing these would leave no way to create or revoke a
        // key).
        .route("/metrics", get(get_metrics))
        .route(
            "/admin/api-keys",
            post(create_api_key).get(list_api_keys_handler),
        )
        .route("/admin/api-keys/:id", delete(revoke_api_key_handler))
        .route("/admin/audit-log", get(get_audit_log))
        // Central Real-time Orchestrator Logs endpoints
        .route("/api/v1/logs", get(list_logs_handler))
        .route(
            "/api/v1/projects/:project_id/logs",
            get(list_project_logs_handler),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    Router::new()
        .route("/health", get(health))
        // BE-14 (API Contract §14) — project-scoped realtime stream.
        // Mounted outside the header-based `require_auth` layer because
        // a WebSocket upgrade can't carry an `Authorization` header,
        // so this authenticates itself via `?key=` instead (see
        // `project_events_ws_handler`).
        .route(
            "/api/v1/projects/:project_id/events",
            get(project_events_ws_handler),
        )
        .route("/api/v1/logs/ws", get(logs_ws_handler))
        .route(
            "/api/v1/projects/:project_id/logs/ws",
            get(project_logs_ws_handler),
        )
        .merge(protected)
        .with_state(state)
}

/// B12 remediation. Was `CorsLayer::new().allow_origin(Any)` in `main.rs` —
/// flagged Medium by the Backend Plan/Deep Audit §15.2: not directly
/// exploitable on its own (every route above is still Bearer-gated by
/// `require_auth`), but a real widening of the browser attack surface
/// (any site could read a response *if* it ever got a caller to hand it a
/// valid key) and explicitly called out as something to fix "before any
/// non-local deployment." A real, explicit allow-list, not a second
/// wildcard dressed up as a list.
///
/// `AIQA_CORS_ALLOWED_ORIGINS` (comma-separated origins) overrides the
/// default entirely when set and non-empty — this is the "ask the user
/// directly rather than guessing" path for any real deployment origin.
/// Unset, it defaults to this project's own documented local dev
/// origin: `forentend`'s Vite dev server (`vite --port=3000`, see
/// `forentend/package.json`'s `dev` script), on both `localhost` and
/// `127.0.0.1` since browsers treat those as distinct origins. `apps/web`
/// (default Vite port, unmaintained since 18 Aug — see
/// AIQA_CONSOLIDATED_DOCUMENTATION_REVIEW.md §6.1) is deliberately not in
/// the default list; add it via the env var if it's ever needed again.
pub fn cors_layer_from_env() -> CorsLayer {
    let origins = std::env::var("AIQA_CORS_ALLOWED_ORIGINS")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            s.split(',')
                .map(|o| o.trim().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![
                "http://localhost:3000".to_string(),
                "http://127.0.0.1:3000".to_string(),
            ]
        });
    cors_layer_for_origins(&origins)
}

/// The actual allow-list construction, split out from `cors_layer_from_env`
/// so a test can exercise real enforcement against a known, fixed origin
/// list without mutating process-wide environment state (which `cargo
/// test`'s default parallelism makes inherently racy across tests).
/// `pub(crate)` rather than private: `api_tests.rs` is a sibling module
/// (declared in `lib.rs`), not a child of this one, so it needs at least
/// crate-visibility to call this directly.
pub(crate) fn cors_layer_for_origins(origins: &[String]) -> CorsLayer {
    let headers: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(headers))
        .allow_methods(Any)
        .allow_headers(Any)
}

async fn health() -> &'static str {
    "ok"
}

fn to_500<E: std::fmt::Display>(e: E) -> ApiError {
    ApiError::internal(e)
}

async fn audit(
    state: &AppState,
    ctx: &AuthContext,
    action: &str,
    target: Option<&str>,
    detail: Option<&str>,
    outcome: &str,
) {
    // A global-scope key (`ctx.project_id` is `None`) that acts inside a project is acting in *that*
    // project, so record the one the request's URL names. Otherwise the row belongs to no project and
    // never shows up in the project's own audit trail.
    let project = ctx.project_id.clone().or_else(auth::request_project);
    audit_in_project(
        state,
        ctx,
        project.as_deref(),
        action,
        target,
        detail,
        outcome,
    )
    .await;
}

/// `audit`, for an action whose project the handler knows but the request's URL does not (`project.create`:
/// the project did not exist until this request).
async fn audit_in_project(
    state: &AppState,
    ctx: &AuthContext,
    project_id: Option<&str>,
    action: &str,
    target: Option<&str>,
    detail: Option<&str>,
    outcome: &str,
) {
    let _ = state
        .store
        .insert_audit(
            Some(&ctx.name),
            Some(ctx.role.as_str()),
            project_id,
            action,
            target,
            detail,
            outcome,
        )
        .await;
}

// ---------------------------------------------------- project-scoped routing
//
// "Actual BE-9" (Integration Plan's own numbering, §15: "`/api/v1` + scoped
// routing | All routes contract-shaped") — every handler below this point
// whose name ends in `_scoped` is reached through a URL that names a
// project. Two checks are mandatory for every one of them (Step 3 of the
// brief this was implemented against):
//
//   1. `require_scoped_project` — the project in the URL must actually
//      exist (`store.get_project`), and the caller must be authorized for
//      it (`auth::require_project_access`, the same gate every existing
//      handler already uses — no new authorization primitive was written).
//   2. `require_resource_in_project` — for any handler that also names a
//      specific resource (a run, a piece of evidence) under that project,
//      the resource's *own* stored `project_id` must match the URL's
//      project id. This is what actually prevents Project A's key —
//      including a *global* admin key, which passes check 1 for any
//      project — from reading Project B's run by requesting it under
//      Project A's path. Checking only #1 would authorize the *namespace*
//      without ever confirming the *resource* is really in it.
//
// A mismatch in #2 is reported as 404, never 403: from this URL's point of
// view the resource simply isn't there, and a 403 would leak that it
// exists somewhere else.

/// Requirement #1 above. Existence is checked before authorization,
/// matching this file's own established convention elsewhere (`get_run`,
/// `get_project_handler`): a project that doesn't exist can't be "yours"
/// either way, so 404 is the more informative answer than 403.
async fn require_scoped_project(
    state: &AppState,
    ctx: &AuthContext,
    project_id: Uuid,
) -> Result<(), ApiError> {
    state
        .store
        .get_project(project_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    auth::require_project_access(ctx, Some(project_id.to_string().as_str()))?;
    Ok(())
}

/// Requirement #2 above.
fn require_resource_in_project(
    resource_project_id: Option<&str>,
    url_project_id: Uuid,
) -> Result<(), ApiError> {
    if resource_project_id == Some(url_project_id.to_string().as_str()) {
        Ok(())
    } else {
        Err(ApiError::not_found("resource not found in this project"))
    }
}

// ------------------------------------------------------------------ mobile

/// GET /api/v1/mobile/fleet — the real mobile device fleet visible to this
/// host right now: every device `adb` can see with its real connection
/// state, every AVD that genuinely exists, and whether the real mobile.dev
/// Maestro CLI is actually installed.
///
/// Host-level, not project-scoped: `adb` sees one set of devices per
/// machine. Any authenticated key may read it — knowing that a phone is
/// plugged in grants nothing on its own, and gating it would hide the fleet
/// from exactly the Viewer who needs to see why a run cannot start.
async fn get_mobile_fleet() -> Json<crate::mobile::MobileFleet> {
    Json(crate::mobile::discover_fleet().await)
}

#[derive(Deserialize)]
struct BootSimulatorBody {
    udid: String,
}

/// POST /api/v1/mobile/simulators/boot — really boots an iOS Simulator via
/// `xcrun simctl`. Operator+, same bar as booting an Android emulator.
///
/// On a non-macOS host this fails with the real platform reason rather than
/// pretending: Apple ships no Simulator runtime off macOS, so there is
/// nothing here that installing a tool would fix.
async fn boot_ios_simulator_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiJson(body): ApiJson<BootSimulatorBody>,
) -> Result<Json<BootAvdResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let message = crate::mobile::boot_ios_simulator(&body.udid)
        .await
        .map_err(ApiError::validation)?;
    state
        .log_bus
        .emit("INFO", "mobile", &message, None, None, None)
        .await;
    audit(
        &state,
        &ctx,
        "mobile.simulator.boot",
        Some(&body.udid),
        None,
        "ok",
    )
    .await;
    Ok(Json(BootAvdResponse { message }))
}

#[derive(Deserialize)]
struct BootAvdBody {
    avd: String,
}

#[derive(Serialize)]
struct BootAvdResponse {
    message: String,
}

/// POST /api/v1/mobile/emulators/boot — really launches an AVD.
///
/// Operator+, matching every other action that starts real work on this
/// host. This deliberately does not claim the emulator is up: booting takes
/// tens of seconds, and only `adb` can prove readiness, so the caller
/// re-reads `/mobile/fleet` and watches the device appear for real.
async fn boot_mobile_emulator(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiJson(body): ApiJson<BootAvdBody>,
) -> Result<Json<BootAvdResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let message = crate::mobile::boot_avd(&body.avd)
        .await
        .map_err(ApiError::validation)?;
    state
        .log_bus
        .emit("INFO", "mobile", &message, None, None, None)
        .await;
    audit(
        &state,
        &ctx,
        "mobile.emulator.boot",
        Some(&body.avd),
        None,
        "ok",
    )
    .await;
    Ok(Json(BootAvdResponse { message }))
}

// --------------------------------------------------------- neural pipeline

/// One stage of the real AIQA execution pipeline. `available` is the honest
/// fact of whether the stage exists in this backend at all — a stage that is
/// not implemented is reported as such rather than omitted (which would hide
/// the gap) or drawn as working (which would fabricate it).
#[derive(Serialize)]
struct PipelineStage {
    id: &'static str,
    label: &'static str,
    /// The real module/route that implements it, or why it does not exist.
    backing: String,
    available: bool,
    /// Real live counter for this stage, when the stage has one.
    metric: Option<i64>,
    metric_label: Option<&'static str>,
}

#[derive(Serialize)]
struct NeuralPipelineResponse {
    stages: Vec<PipelineStage>,
    /// Real quality-graph size for this project (`quality_nodes` /
    /// `quality_relations`) — the only persistent "memory" this system has.
    graph_nodes: i64,
    graph_relations: i64,
    /// Real count of runs this process is currently driving.
    active_runs: usize,
    /// Real terminal-state counts for this project, straight from the store.
    completed_runs: i64,
}

/// GET /api/v1/projects/:project_id/pipeline — the real stage-by-stage state
/// of this project's execution pipeline.
///
/// Deliberately reports an unavailable stage instead of omitting it, should
/// one ever exist again: a view that quietly dropped a real gap would imply
/// the pipeline is complete; one that drew an unimplemented stage as active
/// would invent capability outright. As of Re-planning ("retry-failed") and
/// Vector Store + RAG Retrieval v1 (real Gemini embeddings + brute-force
/// cosine similarity grounding — `ai_router.rs::GeminiEmbedder`'s own doc
/// comment), every stage below is genuinely real; `available: false` is not
/// dead code, it is what a future stage that is not yet built reports.
async fn get_neural_pipeline(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<NeuralPipelineResponse>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let pid = project_id.to_string();

    let (graph_nodes, graph_relations) =
        state.store.quality_graph_size(&pid).await.map_err(to_500)?;

    let run_counts = state
        .store
        .dashboard_run_counts(&pid)
        .await
        .map_err(to_500)?;
    let completed_runs: i64 = run_counts
        .iter()
        .filter(|(k, _)| k == "COMPLETED")
        .map(|(_, v)| *v)
        .sum();

    let records: Vec<Arc<RunRecord>> = state.runs.read().await.values().cloned().collect();
    let mut active_runs = 0usize;
    let mut running_tasks = 0i64;
    for record in records {
        if record.project_id.as_deref() != Some(pid.as_str()) {
            continue;
        }
        active_runs += 1;
        for t in record.engine.read().await.tasks.values() {
            if matches!(t.status, TaskStatus::Running) {
                running_tasks += 1;
            }
        }
    }

    let executor_count = state.registry.registered_kinds().len() as i64;
    let ai_settings = state.ai_infra.settings().await;
    // Vector Store v1 — real count of this project's own embedded cases,
    // for the "vector" stage's metric below.
    let embedded_case_count = state
        .store
        .count_test_case_embeddings(&pid)
        .await
        .map_err(to_500)?;

    let stages = vec![
        PipelineStage {
            id: "brain",
            label: "Model Router",
            backing: format!(
                "ai_router.rs — capability/health scoring, {} mode",
                if ai_settings.mode == crate::ai_router::RoutingMode::Manual { "MANUAL" } else { "AUTO" }
            ),
            available: true,
            metric: None,
            metric_label: None,
        },
        PipelineStage {
            id: "planning",
            label: "Planning",
            backing: "scheduler.rs::validate_plan — DAG build, cycle + unknown-prerequisite rejection".to_string(),
            available: true,
            metric: None,
            metric_label: None,
        },
        PipelineStage {
            id: "tools",
            label: "Tool Routing",
            backing: "executor.rs::ExecutorRegistry — capability routing to real runners".to_string(),
            available: true,
            metric: Some(executor_count),
            metric_label: Some("executors"),
        },
        PipelineStage {
            id: "execution",
            label: "Execution",
            backing: "engine.rs — concurrency-bounded task execution".to_string(),
            available: true,
            metric: Some(running_tasks),
            metric_label: Some("tasks running"),
        },
        PipelineStage {
            id: "observation",
            label: "Observation",
            backing: "engine.rs events + log_bus.rs — real per-task events streamed live".to_string(),
            available: true,
            metric: None,
            metric_label: None,
        },
        PipelineStage {
            id: "memory",
            label: "Quality Graph",
            backing: "quality_nodes + quality_relations — execution history, flakiness, failure rate".to_string(),
            available: true,
            metric: Some(graph_nodes),
            metric_label: Some("nodes"),
        },
        PipelineStage {
            id: "goal",
            label: "Goal State",
            backing: "engine.rs — run reaches a real terminal state".to_string(),
            available: true,
            metric: Some(completed_runs),
            metric_label: Some("completed runs"),
        },
        PipelineStage {
            id: "replanning",
            label: "Re-planning",
            backing: "retry-failed — retries a finished run's real INFRA_ERROR/TIMEOUT tasks only (never a genuine FAILED test) via a new run through the same real execution pipeline. Deliberately scoped: a human triggers it, not an autonomous re-planner.".to_string(),
            available: true,
            metric: None,
            metric_label: None,
        },
        PipelineStage {
            id: "vector",
            label: "Vector Store",
            backing: "test_case_embeddings + documents — one real Gemini embedding (gemini-embedding-001) per APPROVED test case and per uniquely-extracted uploaded requirement document (Phase 2 gap-closing), brute-force cosine similarity at retrieval time (no ANN index at this scale). Gemini-only: no other configured provider exposes a real embeddings endpoint. Requires GEMINI_API_KEY; degrades to no grounding (not a failure) when unset.".to_string(),
            available: true,
            metric: Some(embedded_case_count),
            metric_label: Some("cases embedded"),
        },
        PipelineStage {
            id: "rag",
            label: "RAG Retrieval",
            backing: "Every test-case generation request embeds its own requirement text and retrieves this project's most similar already-APPROVED test cases AND most relevant previously-uploaded requirement documents (Phase 2 gap-closing) as grounding — additive only; a case it inspires still follows the same EVIDENCE RULES as the rest of the prompt.".to_string(),
            available: true,
            metric: None,
            metric_label: None,
        },
    ];

    Ok(Json(NeuralPipelineResponse {
        stages,
        graph_nodes,
        graph_relations,
        active_runs,
        completed_runs,
    }))
}

// ------------------------------------------------------------------ whoami

/// The caller's own real identity, as resolved by `auth::require_auth` from
/// the presented key. Every field is the real stored value for that key —
/// there is no session, profile, or display-name concept in this backend to
/// embellish it with.
#[derive(Serialize)]
struct WhoAmIResponse {
    /// `api_keys.name` — the key's label, not a user account.
    name: String,
    role: &'static str,
    /// `None` = global scope (this key sees every project); `Some(id)` = it
    /// is restricted to that one project (`auth::require_project_access`).
    project_id: Option<String>,
    /// The real role ordering `Role::at_least` enforces, least to most
    /// privileged. Served from here so the frontend never has to hardcode a
    /// second copy of this taxonomy that could silently drift from
    /// `auth.rs`.
    role_hierarchy: [&'static str; 3],
}

/// GET /api/v1/auth/whoami — any authenticated key may ask who it is.
/// Deliberately no role gate: a caller learning its *own* already-known
/// identity grants it nothing, and gating it would make the common case
/// (rendering "you are a viewer") impossible for the least-privileged role.
async fn whoami(Extension(ctx): Extension<AuthContext>) -> Json<WhoAmIResponse> {
    Json(WhoAmIResponse {
        name: ctx.name.clone(),
        role: ctx.role.as_str(),
        project_id: ctx.project_id.clone(),
        role_hierarchy: [
            Role::Viewer.as_str(),
            Role::Operator.as_str(),
            Role::Admin.as_str(),
        ],
    })
}

// -------------------------------------------------------- orchestrator state

/// One real executor kind the scheduler can route a task to, plus the real
/// policy gate that applies to it (`policy.rs::evaluate`). "Capability
/// routing" in this system is exactly this table — a task's `executor`
/// field is matched against `ExecutorRegistry`'s registered kinds.
#[derive(Serialize)]
struct ExecutorRoute {
    kind: String,
    /// `"AUTO"` — schedules immediately; `"REQUIRES_APPROVAL"` — a plan
    /// using it lands in `PENDING_APPROVAL` until a human approves;
    /// `"BLOCKED"` — a plan using it is denied outright. Derived from the
    /// same `PolicyConfig` the real gate evaluates against, not a
    /// separate, drifting copy.
    gate: String,
}

/// A run this process is actively driving, with its real task states —
/// the live DAG. Only in-process runs appear here (same honest limitation
/// `AppState::runs` documents): a run finished by a previous process is
/// history, served by the project-scoped execution endpoints instead.
#[derive(Serialize)]
struct ActiveRunView {
    run_id: String,
    name: String,
    project_id: Option<String>,
    state: String,
    concurrency: usize,
    tasks: Vec<TaskState>,
}

#[derive(Serialize)]
struct OrchestratorStateResponse {
    executors: Vec<ExecutorRoute>,
    /// The real, env-tunable governance limits actually enforced before a
    /// run is scheduled (`policy.rs::PolicyConfig::from_env`).
    max_concurrency: usize,
    active_runs: Vec<ActiveRunView>,
}

/// GET /api/v1/orchestrator/state — the real coordination surface behind
/// the Orchestrator view: which executors tasks can be routed to, what
/// policy gate each carries, and the live DAG of every run this process is
/// currently driving. Platform-wide (not project-scoped), matching
/// `/ai/status` — the executor registry and policy config are process-wide
/// singletons, not per-project. A project-scoped key still only sees its
/// own runs, filtered the same way every other list endpoint filters.
async fn get_orchestrator_state(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<Json<OrchestratorStateResponse>, ApiError> {
    let executors = state
        .registry
        .registered_kinds()
        .into_iter()
        .map(|kind| {
            let gate = if state.policy.blocked_executors.contains(&kind) {
                "BLOCKED"
            } else if state.policy.require_approval_for.contains(&kind) {
                "REQUIRES_APPROVAL"
            } else {
                "AUTO"
            };
            ExecutorRoute {
                kind,
                gate: gate.to_string(),
            }
        })
        .collect();

    let records: Vec<Arc<RunRecord>> = state.runs.read().await.values().cloned().collect();
    let mut active_runs = Vec::new();
    for record in records {
        // Same isolation rule as every other list endpoint: a global key
        // (`project_id: None`) sees every run; a project-scoped key sees
        // only its own.
        if let Some(scope) = ctx.project_id.as_deref() {
            if record.project_id.as_deref() != Some(scope) {
                continue;
            }
        }
        let mut tasks: Vec<TaskState> =
            record.engine.read().await.tasks.values().cloned().collect();
        tasks.sort_by(|a, b| a.id.cmp(&b.id));
        active_runs.push(ActiveRunView {
            run_id: record.run_id.to_string(),
            name: record.name.clone(),
            project_id: record.project_id.clone(),
            state: format!("{:?}", *record.state.read().await).to_uppercase(),
            concurrency: record.plan.concurrency,
            tasks,
        });
    }
    active_runs.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Json(OrchestratorStateResponse {
        executors,
        max_concurrency: state.policy.max_concurrency,
        active_runs,
    }))
}

// -------------------------------------------------------- custom skills

/// A "skill" here (matching the frontend's own `SkillDefinition` model,
/// `forentend/src/types/skills.ts`) is a generation-scope configuration
/// value, not a project-owned resource — same platform-wide scope as
/// `ai_settings`, hence no `project_id` anywhere on this route.
#[derive(Deserialize)]
struct CreateCustomSkillBody {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    source_file: Option<String>,
    /// Real YAML frontmatter of an imported SKILL.md, passed through as the
    /// file wrote it. Absent for a hand-made skill.
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

fn validate_skill_name(name: &str) -> Result<&str, ApiError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ApiError::validation("skill name must not be empty"));
    }
    Ok(trimmed)
}

/// POST /api/v1/skills — persists a real custom skill (see
/// `store::insert_custom_skill`'s doc comment for why its `description` is
/// the entire durable state a skill needs). Operator+, matching the bar for
/// every other real create-a-configuration-value action in this file
/// (`create_ai_bug_hypothesis`-adjacent settings writes), not Admin-only
/// like `create_project` — a skill is a generation input, not a new
/// isolation boundary.
async fn create_custom_skill(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiJson(body): ApiJson<CreateCustomSkillBody>,
) -> Result<(StatusCode, Json<crate::store::CustomSkillRow>), ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let name = validate_skill_name(&body.name)?;
    let id = format!("custom-skill-{}", Uuid::new_v4());

    state
        .store
        .insert_custom_skill(
            &id,
            name,
            body.description.trim(),
            body.source_file.as_deref(),
            body.metadata.as_ref(),
            &ctx.name,
        )
        .await
        .map_err(to_500)?;

    audit(&state, &ctx, "skill.create", Some(&id), Some(name), "ok").await;

    let skills = state.store.list_custom_skills().await.map_err(to_500)?;
    let created = skills
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| to_500("skill vanished immediately after insert"))?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[derive(Deserialize)]
struct ResolveSkillsBody {
    skills: Vec<crate::ai_generation::SkillInput>,
}

/// POST /api/v1/skills/resolve — returns the real instruction each supplied
/// skill resolves to.
///
/// This deliberately calls `ai_generation::resolve_skill_instructions` — the
/// exact function the generator itself uses — rather than re-deriving the
/// text. That is the whole point: what a user reads here is provably the
/// same string that gets embedded in the real prompt, so this view cannot
/// drift from generation behaviour the way a second, display-only copy of
/// the instructions would.
///
/// Viewer+, matching `GET /skills`: reading what a skill instructs is not a
/// privileged action.
async fn resolve_skills(
    ApiJson(body): ApiJson<ResolveSkillsBody>,
) -> Json<Vec<crate::ai_generation::ResolvedSkillInstruction>> {
    Json(crate::ai_generation::resolve_skill_instructions(
        &body.skills,
    ))
}

/// GET /api/v1/skills — any authenticated key (Viewer+) may list; selecting
/// a skill for generation is not a privileged action.
async fn list_custom_skills_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::store::CustomSkillRow>>, ApiError> {
    let rows = state.store.list_custom_skills().await.map_err(to_500)?;
    Ok(Json(rows))
}

/// GET /api/v1/skills/files — Viewer+, matching `GET /skills`: reading the
/// real SKILL.md files currently sitting in `skill_files::skills_dir()` is
/// not a privileged action. No `AppState` needed — this is a direct
/// filesystem read, not a database query.
async fn list_file_skills_handler() -> Json<crate::skill_files::FileSkillScan> {
    Json(crate::skill_files::scan_skills_dir())
}

/// DELETE /api/v1/skills/:id — Operator+, matching `create_custom_skill`.
async fn delete_custom_skill_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let deleted = state.store.delete_custom_skill(&id).await.map_err(to_500)?;
    if !deleted {
        return Err(ApiError::not_found("skill not found"));
    }
    audit(&state, &ctx, "skill.delete", Some(&id), None, "ok").await;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------- custom AI providers

/// The real, public-safe view of a `store::CustomAiProviderRow` — every
/// field except `api_key`. `store::CustomAiProviderRow` deliberately does
/// not derive `Serialize` at all (see its own doc comment), so building
/// this view is the only way a handler here can respond with one; there is
/// no direct path that could accidentally leak the raw key.
#[derive(Serialize)]
struct CustomAiProviderView {
    id: String,
    name: String,
    base_url: String,
    /// USD per 1,000 tokens in / out as the user entered them; both `null` when no price was entered.
    input_usd_per_1k: Option<f64>,
    output_usd_per_1k: Option<f64>,
    /// The environment variable the key is read from when it was given as `env:NAME` (the name is not a secret);
    /// `null` when the key itself is stored.
    api_key_env: Option<String>,
    /// Whether that environment variable is set on the server right now; `null` when the key itself is stored.
    api_key_env_set: Option<bool>,
    created_by: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<&crate::store::CustomAiProviderRow> for CustomAiProviderView {
    fn from(r: &crate::store::CustomAiProviderRow) -> Self {
        let env_name = crate::ai_router::env_key_name(&r.api_key);
        Self {
            id: r.id.clone(),
            name: r.name.clone(),
            base_url: r.base_url.clone(),
            input_usd_per_1k: r.input_usd_per_1k,
            output_usd_per_1k: r.output_usd_per_1k,
            api_key_env: env_name.map(str::to_string),
            api_key_env_set: env_name.map(crate::ai_router::env_key_is_set),
            created_by: r.created_by.clone(),
            created_at: r.created_at,
        }
    }
}

/// Refreshes `AiInfrastructure`'s real, runtime-mutable custom-provider set
/// from the DB's own current full list — the one real source of truth,
/// re-read after every mutation rather than hand-editing the in-memory
/// copy, so the two can never drift.
async fn reload_custom_providers_into_infra(state: &AppState) -> Result<(), ApiError> {
    let rows = state
        .store
        .list_custom_ai_providers()
        .await
        .map_err(to_500)?;
    let configs = rows
        .into_iter()
        .map(|r| crate::ai_router::CustomProviderConfig {
            id: r.id,
            name: r.name,
            base_url: r.base_url,
            api_key: r.api_key,
            pricing: crate::ai_router::ProviderPricing::from_prices(
                r.input_usd_per_1k,
                r.output_usd_per_1k,
            ),
        })
        .collect();
    state.ai_infra.set_custom_providers(configs).await;
    Ok(())
}

#[derive(Deserialize)]
struct CreateCustomAiProviderBody {
    name: String,
    base_url: String,
    /// The key itself, or `env:NAME` to have it read from the environment variable `NAME` instead of stored.
    api_key: String,
    /// What the provider charges, USD per 1,000 tokens. Both or neither.
    #[serde(default)]
    input_usd_per_1k: Option<f64>,
    #[serde(default)]
    output_usd_per_1k: Option<f64>,
}

fn validate_custom_provider_fields(
    name: &str,
    base_url: &str,
    api_key: &str,
    input_usd_per_1k: Option<f64>,
    output_usd_per_1k: Option<f64>,
) -> Result<(), ApiError> {
    if name.trim().is_empty() {
        return Err(ApiError::validation("provider name must not be empty"));
    }
    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        return Err(ApiError::validation(
            "base_url must be a real http:// or https:// endpoint",
        ));
    }
    if api_key.trim().is_empty() {
        return Err(ApiError::validation("api_key must not be empty"));
    }
    if let Some(var) = crate::ai_router::env_key_name(api_key) {
        let mut chars = var.chars();
        let valid = chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            return Err(ApiError::validation(
                "api_key written as env:NAME must name an environment variable (letters, digits and _, not starting with a digit)",
            ));
        }
    }
    if input_usd_per_1k.is_some() != output_usd_per_1k.is_some() {
        return Err(ApiError::validation(
            "enter both the input and the output price per 1K tokens, or neither",
        ));
    }
    if [input_usd_per_1k, output_usd_per_1k]
        .into_iter()
        .flatten()
        .any(|p| !p.is_finite() || p < 0.0)
    {
        return Err(ApiError::validation(
            "prices must be zero or more (USD per 1K tokens)",
        ));
    }
    Ok(())
}

/// POST /api/v1/ai/providers/custom — persists a real, user-added AI
/// provider and makes it live for real routing/generation on the very
/// next discovery cycle (`reload_custom_providers_into_infra` — no
/// process restart needed, unlike the env-var providers). Operator+,
/// matching `create_custom_skill`'s own bar for a real configuration-value
/// write.
async fn create_custom_ai_provider(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiJson(body): ApiJson<CreateCustomAiProviderBody>,
) -> Result<(StatusCode, Json<CustomAiProviderView>), ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let name = body.name.trim();
    let base_url = body.base_url.trim().trim_end_matches('/');
    let api_key = body.api_key.trim();
    validate_custom_provider_fields(
        name,
        base_url,
        api_key,
        body.input_usd_per_1k,
        body.output_usd_per_1k,
    )?;
    let id = format!("custom-ai-{}", Uuid::new_v4());

    state
        .store
        .insert_custom_ai_provider(
            &id,
            name,
            base_url,
            api_key,
            body.input_usd_per_1k.zip(body.output_usd_per_1k),
            &ctx.name,
        )
        .await
        .map_err(to_500)?;
    reload_custom_providers_into_infra(&state).await?;

    audit(
        &state,
        &ctx,
        "ai_provider.create",
        Some(&id),
        Some(name),
        "ok",
    )
    .await;

    let rows = state
        .store
        .list_custom_ai_providers()
        .await
        .map_err(to_500)?;
    let created = rows
        .iter()
        .find(|r| r.id == id)
        .map(CustomAiProviderView::from)
        .ok_or_else(|| to_500("custom AI provider vanished immediately after insert"))?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// GET /api/v1/ai/providers/custom — Viewer+, matching `GET /skills`:
/// reading configured providers is not a privileged action. Never
/// includes a raw `api_key` (see `CustomAiProviderView`).
async fn list_custom_ai_providers_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<CustomAiProviderView>>, ApiError> {
    let rows = state
        .store
        .list_custom_ai_providers()
        .await
        .map_err(to_500)?;
    Ok(Json(rows.iter().map(CustomAiProviderView::from).collect()))
}

/// DELETE /api/v1/ai/providers/custom/:id — Operator+, matching
/// `create_custom_ai_provider`.
async fn delete_custom_ai_provider_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let deleted = state
        .store
        .delete_custom_ai_provider(&id)
        .await
        .map_err(to_500)?;
    if !deleted {
        return Err(ApiError::not_found("custom AI provider not found"));
    }
    reload_custom_providers_into_infra(&state).await?;
    audit(&state, &ctx, "ai_provider.delete", Some(&id), None, "ok").await;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------- projects

#[derive(Deserialize)]
struct CreateProjectBody {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

fn validate_project_name(name: &str) -> Result<&str, ApiError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ApiError::validation("project name must not be empty"));
    }
    Ok(trimmed)
}

/// POST /api/v1/projects — API Contract §5.2. Creating a project is a
/// foundational, cross-cutting action (it defines a new isolation boundary
/// every other resource will scope against), so it follows the same bar as
/// the other action that creates a new access boundary today, minting an
/// API key (`create_api_key`, below): Admin-only, and — matching that
/// handler exactly — not additionally restricted by the creating key's own
/// `project_id` scope, since an admin key (scoped or global) can already
/// mint keys for any project.
async fn create_project(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiJson(body): ApiJson<CreateProjectBody>,
) -> Result<(StatusCode, Json<crate::store::Project>), ApiError> {
    auth::require_role(&ctx, Role::Admin)?;
    let name = validate_project_name(&body.name)?;

    let id = Uuid::new_v4();
    state
        .store
        .insert_project(id, name, body.description.as_deref())
        .await
        .map_err(to_500)?;

    audit_in_project(
        &state,
        &ctx,
        Some(&id.to_string()),
        "project.create",
        Some(&id.to_string()),
        Some(name),
        "ok",
    )
    .await;

    let project = state
        .store
        .get_project(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| to_500("project vanished immediately after insert"))?;
    Ok((StatusCode::CREATED, Json(project)))
}

/// GET /api/v1/projects — API Contract §5.1. Same isolation convention as
/// every other list endpoint in this API (`list_runs`, `list_bug_drafts`,
/// `list_test_intelligence`): a global key (`project_id: None`) sees every
/// project; a project-scoped key sees only its own — which, per BE-8's own
/// scope note, may legitimately be an empty list if that project id was
/// never created via `POST /api/v1/projects`.
async fn list_projects_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<Json<Vec<crate::store::Project>>, ApiError> {
    let rows = state
        .store
        .list_projects(ctx.project_id.as_deref())
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

/// GET /api/v1/projects/:id — API Contract §5.3. Fetch-then-authorize, the
/// same order every other scoped-resource handler in this file already uses
/// (`get_run`, `list_evidence`, `get_evidence_content`): existence is
/// checked first (404 if genuinely absent), then `require_project_access`
/// against the project's own id (403 on a cross-project read).
async fn get_project_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<crate::store::Project>, ApiError> {
    let project = state
        .store
        .get_project(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    auth::require_project_access(&ctx, Some(project.id.to_string().as_str()))?;
    Ok(Json(project))
}

#[derive(Deserialize)]
struct UpdateProjectBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// PATCH /api/v1/projects/:id — API Contract §5.4. Mutating a resource
/// within your own scope follows the same bar as every other in-scope
/// mutation (`create_run`, `approve_run`/`reject_run`): Operator, plus
/// `require_project_access` so a project-scoped operator can only edit its
/// own project. A `None` field is left unchanged (partial update); an
/// absent body updates nothing but still refreshes `updated_at`.
async fn update_project_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<UpdateProjectBody>,
) -> Result<Json<crate::store::Project>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let existing = state
        .store
        .get_project(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    auth::require_project_access(&ctx, Some(existing.id.to_string().as_str()))?;

    let name = body
        .name
        .as_deref()
        .map(validate_project_name)
        .transpose()?;

    let updated = state
        .store
        .update_project(id, name, body.description.as_deref())
        .await
        .map_err(to_500)?;
    if !updated {
        return Err(ApiError::not_found("project not found"));
    }

    audit(
        &state,
        &ctx,
        "project.update",
        Some(&id.to_string()),
        name,
        "ok",
    )
    .await;

    let project = state
        .store
        .get_project(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    Ok(Json(project))
}

/// DELETE /api/v1/projects/:id — API Contract §5.5 (Delete Project).
/// Destructive operation: Admin-only. Checks project existence, verifies
/// caller's project scope (if the admin key is project-scoped), deletes the project
/// and its associated scoped data atomically, and records an audit log entry.
async fn delete_project_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    auth::require_role(&ctx, Role::Admin)?;
    let existing = state
        .store
        .get_project(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    auth::require_project_access(&ctx, Some(existing.id.to_string().as_str()))?;

    let deleted = state.store.delete_project(id).await.map_err(to_500)?;
    if !deleted {
        return Err(ApiError::not_found("project not found"));
    }

    audit(
        &state,
        &ctx,
        "project.delete",
        Some(&id.to_string()),
        Some(&existing.name),
        "ok",
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------- test cases
//
// AUD-005 — real, persisted, project-isolated Test Case CRUD + lifecycle +
// execution handoff. See `domain.rs`'s test-case section and `store.rs`'s
// test-case functions for the entity/persistence design; this section is
// just the HTTP surface over them, following the exact same
// `require_scoped_project` + `require_role` + `audit(...)` pattern every
// other mutating scoped resource in this file already uses.

fn validate_test_case_request(req: &CreateTestCaseRequest) -> Result<(), ApiError> {
    if req.title.trim().is_empty() {
        return Err(ApiError::validation("title must not be empty"));
    }
    if req.description.trim().is_empty() {
        return Err(ApiError::validation("description must not be empty"));
    }
    if req.steps.is_empty() {
        return Err(ApiError::validation("at least one step is required"));
    }
    if req
        .expected_result
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
        && req.assertions.iter().all(|a| a.trim().is_empty())
    {
        return Err(ApiError::validation(
            "expected_result or at least one assertion is required",
        ));
    }
    Ok(())
}

/// POST /api/v1/projects/:project_id/test-cases — persists one generated
/// test case for real. This is deliberately a single-case create, not a
/// batch endpoint: the Test Case Generator produces its cases client-side
/// one at a time already, and a single, uniform create contract is simpler
/// than inventing a second batch shape nothing else in this API uses.
async fn create_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<CreateTestCaseRequest>,
) -> Result<(StatusCode, Json<crate::store::TestCaseRow>), ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    validate_test_case_request(&body)?;

    let id = state
        .store
        .insert_test_case(&project_id.to_string(), &body, Some(&ctx.name))
        .await
        .map_err(to_500)?;

    // Real requirement-graph edge (Phase 4 — see `intelligence.rs`'s own
    // doc comment). A write failure here must never undo an already-real,
    // already-persisted test case — logged honestly, never propagated as a
    // 500 for what is genuinely supplementary graph bookkeeping, not the
    // core feature this endpoint exists for.
    if let Err(e) = crate::intelligence::Intelligence::new(&state.store)
        .link_test_case_to_requirement(
            &project_id.to_string(),
            id,
            &body.title,
            body.requirement_reference.as_deref(),
            body.source_requirement.as_deref(),
        )
        .await
    {
        state
            .log_bus
            .emit(
                "WARN",
                "quality_graph",
                &format!(
                    "Could not link test case {id} to its requirement in the quality graph: {e}"
                ),
                None,
                Some(&project_id.to_string()),
                None,
            )
            .await;
    }

    audit(
        &state,
        &ctx,
        "test_case.create",
        Some(&id.to_string()),
        Some(&body.title),
        "ok",
    )
    .await;

    let row = state
        .store
        .get_test_case(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| to_500("test case vanished immediately after insert"))?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Deserialize)]
struct ListTestCasesQuery {
    #[serde(default)]
    status: Option<String>,
    /// Phase 5 (Output maturity) — `?suite_id=<uuid>` narrows to one suite,
    /// same optional-filter shape `status` already uses.
    #[serde(default)]
    suite_id: Option<Uuid>,
}

/// GET /api/v1/projects/:project_id/test-cases — project-scoped, optionally
/// filtered to one lifecycle status (`?status=APPROVED`) and/or one suite
/// (`?suite_id=<uuid>`).
async fn list_test_cases_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ListTestCasesQuery>,
) -> Result<Json<Vec<crate::store::TestCaseRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let rows = state
        .store
        .list_test_cases(&project_id.to_string(), q.status.as_deref(), q.suite_id)
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

/// Shared fetch-then-authorize for every `:id`-scoped test-case route —
/// same order every other scoped-resource handler in this file uses
/// (`get_project_handler`, `scoped_run_summary`): existence first (404 if
/// genuinely absent, including when it exists but belongs to a different
/// project than the URL says — never leaks a cross-project id's existence),
/// then `require_project_access` against the row's own real `project_id`
/// (403 if the caller's key is scoped to a different project).
async fn scoped_test_case(
    state: &AppState,
    ctx: &AuthContext,
    project_id: Uuid,
    id: Uuid,
) -> Result<crate::store::TestCaseRow, ApiError> {
    let row = state
        .store
        .get_test_case(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("test case not found"))?;
    if row.project_id != project_id.to_string() {
        return Err(ApiError::not_found("test case not found"));
    }
    auth::require_project_access(ctx, Some(row.project_id.as_str()))?;
    Ok(row)
}

async fn get_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::store::TestCaseRow>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    Ok(Json(row))
}

#[derive(Deserialize, Default)]
struct UpdateTestCaseBody {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    preconditions: Option<Vec<String>>,
    #[serde(default)]
    steps: Option<Vec<TestStep>>,
    #[serde(default)]
    expected_result: Option<String>,
}

/// PATCH /api/v1/projects/:project_id/test-cases/:id — partial update of
/// the still-editable descriptive fields (never `status`; that only moves
/// through `/validate`, `/approve`, `/reject`, `/handoff` below).
async fn update_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(body): ApiJson<UpdateTestCaseBody>,
) -> Result<Json<crate::store::TestCaseRow>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_test_case(&state, &ctx, project_id, id).await?;

    if let Some(title) = &body.title {
        if title.trim().is_empty() {
            return Err(ApiError::validation("title must not be empty"));
        }
    }

    let updated = state
        .store
        .update_test_case(
            id,
            body.title.as_deref(),
            body.description.as_deref(),
            body.priority.as_deref(),
            body.preconditions.as_ref(),
            body.steps.as_ref(),
            body.expected_result.as_deref(),
        )
        .await
        .map_err(to_500)?;
    if !updated {
        return Err(ApiError::not_found("test case not found"));
    }
    audit(
        &state,
        &ctx,
        "test_case.update",
        Some(&id.to_string()),
        None,
        "ok",
    )
    .await;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    // Vector Store v1 — an already-APPROVED case just got edited; its
    // stored embedding (if any) is now stale content, so recompute it. A
    // case not yet approved has nothing to keep in sync here (see
    // `embed_approved_test_case`'s own doc comment).
    if row.status == TestCaseStatus::Approved {
        embed_approved_test_case(&state, &row).await;
    }
    Ok(Json(row))
}

async fn delete_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_test_case(&state, &ctx, project_id, id).await?;
    let deleted = state.store.delete_test_case(id).await.map_err(to_500)?;
    if !deleted {
        return Err(ApiError::not_found("test case not found"));
    }
    audit(
        &state,
        &ctx,
        "test_case.delete",
        Some(&id.to_string()),
        None,
        "ok",
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// PUT .../test-cases/:id/suite — Phase 5 (Output maturity). See
/// `domain.rs::SetTestCaseSuiteRequest`'s own doc comment for why this is
/// its own endpoint rather than a `PATCH` field: `suite_id: null` must be
/// able to explicitly detach a case from its suite. When a real `suite_id`
/// is given, it must genuinely belong to this same project — cross-project
/// assignment is rejected the same way `require_scoped_project` already
/// rejects cross-project access everywhere else in this file, not left as
/// a silent dangling reference.
async fn set_test_case_suite_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(body): ApiJson<SetTestCaseSuiteRequest>,
) -> Result<Json<crate::store::TestCaseRow>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_test_case(&state, &ctx, project_id, id).await?;
    if let Some(suite_id) = body.suite_id {
        let suite = state
            .store
            .get_test_suite(suite_id)
            .await
            .map_err(to_500)?
            .ok_or_else(|| ApiError::validation("suite_id does not refer to a real suite"))?;
        if suite.project_id != project_id.to_string() {
            return Err(ApiError::validation(
                "suite_id refers to a suite in a different project",
            ));
        }
    }
    let updated = state
        .store
        .set_test_case_suite(id, body.suite_id)
        .await
        .map_err(to_500)?;
    if !updated {
        return Err(ApiError::not_found("test case not found"));
    }
    audit(
        &state,
        &ctx,
        "test_case.set_suite",
        Some(&id.to_string()),
        body.suite_id.map(|s| s.to_string()).as_deref(),
        "ok",
    )
    .await;
    let row = state
        .store
        .get_test_case(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| to_500("test case vanished immediately after update"))?;
    Ok(Json(row))
}

/// POST .../test-cases/:id/validate — FRD-VAL-02 ("generated cases meet
/// configured quality/benchmark rules"). Honest scope: this checks real,
/// verifiable structural completeness (non-empty title/description/steps/
/// expected_result, and every step has both an action and an expected
/// result) — there is no AI/model-scored quality benchmark anywhere in this
/// codebase (no model-gateway/LLM integration exists in `services/
/// orchestrator`), so this deliberately does not claim to evaluate
/// anything beyond structure. A case that fails stays `GENERATED`, with
/// the failure reason returned as a real 409, never silently advanced.
async fn validate_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::store::TestCaseRow>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;

    let mut problems = Vec::new();
    if row.title.trim().is_empty() {
        problems.push("title is empty");
    }
    if row.description.trim().is_empty() {
        problems.push("description is empty");
    }
    if row.steps.is_empty() {
        problems.push("no steps defined");
    }
    if row
        .steps
        .iter()
        .any(|s| s.action.trim().is_empty() || s.expected_result.trim().is_empty())
    {
        problems.push("a step is missing an action or an expected result");
    }
    if row.expected_result.trim().is_empty() {
        problems.push("expected_result is empty");
    }

    if !problems.is_empty() {
        // Real lifecycle activity belongs on the live log stream alongside
        // generation and execution — a rejected case is exactly the kind of
        // event an operator watching the Validator page needs to see as it
        // happens, not only after re-reading the row.
        state
            .log_bus
            .emit(
                "WARN",
                "validation",
                &format!(
                    "Validation failed for '{}': {}",
                    row.title,
                    problems.join("; ")
                ),
                None,
                Some(&project_id.to_string()),
                None,
            )
            .await;
        return Err(ApiError::conflict(format!(
            "structural validation failed: {}",
            problems.join("; ")
        )));
    }

    let notes =
        "structural validation passed: title, description, steps and expected result are present";
    let ok = state
        .store
        .validate_test_case(id, notes)
        .await
        .map_err(to_500)?;
    if !ok {
        return Err(ApiError::conflict(format!(
            "test case is not in GENERATED state (current: {})",
            row.status.as_db_str()
        )));
    }
    state
        .log_bus
        .emit(
            "SUCCESS",
            "validation",
            &format!("Structural validation passed for '{}'", row.title),
            None,
            Some(&project_id.to_string()),
            None,
        )
        .await;
    audit(
        &state,
        &ctx,
        "test_case.validate",
        Some(&id.to_string()),
        None,
        "ok",
    )
    .await;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    Ok(Json(row))
}

/// POST .../test-cases/:id/approve — FRD-EXE-11: only an *approved* case
/// may later convert into a schedulable task. Requires `VALIDATED` first —
/// generation alone is never enough (FRD-AGT-04).
async fn approve_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::store::TestCaseRow>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    let ok = state.store.approve_test_case(id).await.map_err(to_500)?;
    if !ok {
        return Err(ApiError::conflict(format!(
            "test case must be VALIDATED before approval (current: {})",
            row.status.as_db_str()
        )));
    }
    state
        .log_bus
        .emit(
            "SUCCESS",
            "validation",
            &format!("Test case approved: '{}'", row.title),
            None,
            Some(&project_id.to_string()),
            None,
        )
        .await;
    audit(
        &state,
        &ctx,
        "test_case.approve",
        Some(&id.to_string()),
        None,
        "ok",
    )
    .await;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    // Vector Store v1 — this case just became a real candidate for future
    // similar-case grounding (`find_similar_approved_test_cases`).
    embed_approved_test_case(&state, &row).await;
    Ok(Json(row))
}

/// POST .../test-cases/:id/reject — reachable from `GENERATED` or
/// `VALIDATED`, not from `APPROVED`/`EXECUTED` (see `store.rs::
/// reject_test_case`'s own doc comment for why).
async fn reject_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::store::TestCaseRow>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    let ok = state.store.reject_test_case(id).await.map_err(to_500)?;
    if !ok {
        return Err(ApiError::conflict(format!(
            "test case cannot be rejected from its current state ({})",
            row.status.as_db_str()
        )));
    }
    state
        .log_bus
        .emit(
            "WARN",
            "validation",
            &format!("Test case rejected: '{}'", row.title),
            None,
            Some(&project_id.to_string()),
            None,
        )
        .await;
    audit(
        &state,
        &ctx,
        "test_case.reject",
        Some(&id.to_string()),
        None,
        "ok",
    )
    .await;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    Ok(Json(row))
}

/// POST .../test-cases/:id/compile — Phase 2.1. Real, read-only preview of
/// what `execution_plan::compile` would produce for this exact, real,
/// persisted test case right now — deterministic compilation first, the
/// real central AI router only if that finds nothing. Never mutates
/// anything (no run, no status change) — safe to call before a case is
/// even APPROVED, so the frontend can show READY/NOT_EXECUTABLE/
/// COMPILATION_UNAVAILABLE (and why) before a user commits to approving
/// and handing it off.
async fn compile_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::execution_plan::CompilationOutcome>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    let outcome = crate::execution_plan::compile(&state.ai_infra, &row).await;
    Ok(Json(outcome))
}

/// POST .../test-cases/:id/handoff — the real bridge into the existing
/// execution engine (FRD-EXE-11). Requires `APPROVED`. Builds a real,
/// single-task `RunPlan` — the task's id is this test case's own id, so
/// `tasks.id` doubles as the traceability key back to it, no extra join
/// table needed (Test Case -> Run -> Task -> Result/Evidence, all already
/// queryable via the existing `runs`/`tasks`/`evidence`/`run_events`
/// tables) — and dispatches it through the exact same `create_run_core`
/// the real Execution Hub itself calls. No duplicate scheduler, no shadow
/// execution engine.
///
/// Honest disclosure (see `TestCaseHandoffRequest`'s own doc comment): the
/// caller must explicitly supply either a real `executor`, an explicit
/// `simulate` outcome, or `compile_and_execute: true` (Phase 2.1). There is
/// no default here that would silently report a fabricated "passed" result
/// for steps nothing has actually run.
async fn handoff_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
    body: Option<ApiJson<TestCaseHandoffRequest>>,
) -> Result<(StatusCode, Json<CreateExecutionResponse>), ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;

    if row.status != TestCaseStatus::Approved {
        return Err(ApiError::conflict(format!(
            "test case must be APPROVED before handoff (current: {})",
            row.status.as_db_str()
        )));
    }

    let ApiJson(body) = body.unwrap_or(ApiJson(TestCaseHandoffRequest {
        executor: None,
        params: serde_json::Value::Null,
        simulate: None,
        compile_and_execute: false,
    }));

    let task_id = id.to_string();
    let task = if body.compile_and_execute {
        // Phase 2.1 — the real compiler, not the caller, determines the
        // real executor/params; any `executor`/`params`/`simulate` also
        // sent alongside `compile_and_execute: true` is ignored (see
        // `TestCaseHandoffRequest`'s own doc comment) rather than silently
        // mixed with compiled output.
        match crate::execution_plan::compile(&state.ai_infra, &row).await {
            crate::execution_plan::CompilationOutcome::Ready(plan) => {
                crate::execution_plan::materialize_task_def(
                    task_id.clone(),
                    row.title.clone(),
                    &plan,
                )
                .map_err(ApiError::validation)?
            }
            crate::execution_plan::CompilationOutcome::NotExecutable { reasons } => {
                return Err(ApiError::validation(format!(
                    "NOT_EXECUTABLE: {}",
                    reasons.join("; ")
                )));
            }
            crate::execution_plan::CompilationOutcome::CompilationFailed { reasons } => {
                return Err(ApiError::validation(format!(
                    "COMPILATION_FAILED: {}",
                    reasons.join("; ")
                )));
            }
            crate::execution_plan::CompilationOutcome::CompilationUnavailable { reason } => {
                return Err(ApiError::validation(format!(
                    "COMPILATION_UNAVAILABLE: {reason}"
                )));
            }
        }
    } else {
        if body.executor.is_none() && body.simulate.is_none() {
            return Err(ApiError::validation(
                "handoff requires either a real executor, an explicit simulate outcome, or compile_and_execute: true — there is no default that fabricates a result",
            ));
        }
        TaskDef {
            id: task_id.clone(),
            name: row.title.clone(),
            prerequisites: Vec::new(),
            simulate: body.simulate.clone(),
            executor: body.executor.clone(),
            params: body.params.clone(),
        }
    };
    let run_plan = RunPlan {
        name: format!("Test Case: {}", row.title),
        tasks: vec![task],
        concurrency: 1,
    };

    let (status_code, Json(created)) =
        create_run_core(&state, &ctx, Some(project_id.to_string()), run_plan).await?;

    let marked = state
        .store
        .mark_test_case_executed(id, created.run_id, &task_id)
        .await
        .map_err(to_500)?;
    if !marked {
        // The run was genuinely created either way (real, not fabricated);
        // this only means this test case's own back-link didn't apply —
        // surfaced, not silently swallowed.
        tracing::warn!("test case {id} handed off as run {} but its own status transition to EXECUTED did not apply", created.run_id);
    }

    audit(
        &state,
        &ctx,
        "test_case.handoff",
        Some(&id.to_string()),
        Some(&created.run_id.to_string()),
        "ok",
    )
    .await;

    Ok((
        status_code,
        Json(CreateExecutionResponse {
            run_id: created.run_id,
            status: created.state,
            policy_notes: created.policy_notes,
        }),
    ))
}

#[derive(Debug, Deserialize)]
struct SkillInputBody {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Deserialize)]
struct GenerateAiTestCasesBody {
    requirement_text: String,
    #[serde(default)]
    skills: Vec<SkillInputBody>,
    /// Real, optional grounding: a previously completed crawl run
    /// (`POST .../crawl` -> `GET .../crawl/:run_id/result`) whose real
    /// discovered pages/elements/visibility-matrix should feed the prompt
    /// (`ai_generation::build_crawl_context_block`). Omitted/`null` behaves
    /// exactly as generation always has — this is purely additive.
    #[serde(default)]
    crawl_run_id: Option<Uuid>,
}

/// `generation_method` is always exactly `"ai"` or `"fallback"` — never
/// anything implying success when `ai_router.generate` actually returned
/// `GenerationOutcome::Failed`. `provider`/`model` are only ever `Some`
/// alongside `generation_method: "ai"`; `fallback_reason` only alongside
/// `"fallback"` — a client cannot receive a shape that claims both or
/// neither.
#[derive(Debug, Serialize)]
struct GenerateAiTestCasesResponse {
    test_cases: Vec<crate::ai_generation::GeneratedTestCaseDraft>,
    generation_method: String,
    provider: Option<String>,
    model: Option<String>,
    fallback_reason: Option<String>,
    /// The real, per-wave breakdown (`ai_generation::WaveSummary`, reused
    /// verbatim — no second DTO). Additive: a selection that lands entirely
    /// in one wave still returns exactly the response shape this endpoint
    /// always has, plus this field showing the one real wave that ran.
    waves: Vec<crate::ai_generation::WaveSummary>,
    /// How many real, distinct pages from `crawl_run_id`'s crawl actually
    /// reached the prompt (0 when no `crawl_run_id` was supplied) — an
    /// honest confirmation of real grounding, not a claim the caller has to
    /// take on faith.
    crawl_pages_used: usize,
    /// Vector Store + RAG Retrieval v1 — how many of this project's own
    /// similar, already-APPROVED test cases were genuinely retrieved and
    /// folded into the prompt as grounding examples (0 for a project with
    /// none embedded yet, or when no embedding provider is configured —
    /// never a claim of grounding that did not really happen). See
    /// `ai_generation::rank_similar_test_cases`/`find_similar_approved_test_cases`
    /// below.
    similar_cases_used: usize,
    /// Phase 2 gap-closing — how many of this project's own previously-
    /// uploaded requirement documents were genuinely retrieved and folded
    /// into the prompt as background/source material (0 for a project with
    /// none embedded yet, or when no embedding provider is configured —
    /// never a claim of grounding that did not really happen). See
    /// `ai_generation::rank_similar_documents`/`find_similar_documents`
    /// above for how this number is genuinely computed.
    similar_documents_used: usize,
    /// Real, summed token/cost/latency across every wave that actually ran
    /// (`ai_generation::aggregate_wave_usage`, applied to `waves` above —
    /// `Vec::new()` on the fallback arm sums to the same honest zero/`None`
    /// values that helper already returns for no waves, no special-casing
    /// needed here). Token/cost totals are `None` only when at least one
    /// wave that ran has a genuinely unknown figure of its own.
    total_prompt_tokens: Option<u32>,
    total_completion_tokens: Option<u32>,
    total_estimated_cost_usd: Option<f64>,
    total_latency_ms: u64,
}

/// Loads this project's real feedback signal (risk/flakiness scores +
/// human-CONFIRMED recurring defect patterns — `intelligence::Intelligence::
/// load_feedback_context`) for the two AI-generation endpoints below. A read
/// failure here (a DB hiccup on what is genuinely supplementary context, not
/// the core feature) must never block a user's ability to generate test
/// cases at all — it degrades to the same honest, empty
/// `ai_generation::FeedbackContext::default()` every project with no
/// history yet already gets, logged as a real `WARN` rather than swallowed
/// silently.
async fn load_feedback_context_for_generation(
    state: &AppState,
    project_id: Uuid,
) -> crate::ai_generation::FeedbackContext {
    match crate::intelligence::Intelligence::new(&state.store)
        .load_feedback_context(&project_id.to_string())
        .await
    {
        Ok(feedback) => feedback,
        Err(e) => {
            state
                .log_bus
                .emit(
                    "WARN",
                    "generation",
                    &format!(
                        "Could not load feedback context, generating without historical signal: {e}"
                    ),
                    None,
                    Some(&project_id.to_string()),
                    None,
                )
                .await;
            crate::ai_generation::FeedbackContext::default()
        }
    }
}

/// Vector Store + RAG Retrieval v1 — embeds this request's own real
/// requirement text (`state.embedder` — Gemini in production, see
/// `ai_router.rs::GeminiEmbedder`'s own doc comment), finds this project's
/// most similar already-APPROVED test cases via real brute-force cosine
/// similarity (`ai_generation::rank_similar_test_cases`), and fetches their
/// real content. Fails open at every step, same disclosed contract
/// `load_feedback_context_for_generation` above already holds itself to: no
/// embedding provider configured, a real network failure, a project with
/// nothing embedded yet, or a genuinely corrupt/orphaned embedding row all
/// return an empty `Vec` — generation proceeds exactly as it did before
/// this feature existed, never a hard failure over what is genuinely
/// optional grounding.
async fn find_similar_approved_test_cases(
    state: &AppState,
    project_id: Uuid,
    requirement_text: &str,
) -> Vec<crate::ai_generation::SimilarCaseExample> {
    if !state.embedder.is_configured() {
        return Vec::new();
    }
    let query_embedding = match state.embedder.embed(requirement_text).await {
        Ok(v) => v,
        Err(e) => {
            state
                .log_bus
                .emit(
                    "WARN",
                    "generation",
                    &format!(
                        "Could not compute a query embedding, generating without similar-case grounding: {e}"
                    ),
                    None,
                    Some(&project_id.to_string()),
                    None,
                )
                .await;
            return Vec::new();
        }
    };
    let candidates = match state
        .store
        .list_test_case_embeddings(&project_id.to_string())
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            state
                .log_bus
                .emit(
                    "WARN",
                    "generation",
                    &format!(
                        "Could not load this project's test case embeddings, generating without similar-case grounding: {e}"
                    ),
                    None,
                    Some(&project_id.to_string()),
                    None,
                )
                .await;
            return Vec::new();
        }
    };
    let ids = crate::ai_generation::rank_similar_test_cases(&query_embedding, &candidates);
    let mut examples = Vec::with_capacity(ids.len());
    for id in ids {
        // A real, if unusual, orphaned embedding row (its own test case
        // deleted through a path other than `delete_test_case` — none
        // exists today, but this stays honest rather than unwrapping) is
        // simply skipped, not a hard failure for the rest of generation.
        if let Ok(Some(row)) = state.store.get_test_case(id).await {
            examples.push(crate::ai_generation::SimilarCaseExample {
                title: row.title,
                category: row.category,
                steps: row.steps,
                expected_result: row.expected_result,
            });
        }
    }
    examples
}

/// Vector Store v1 — (re)computes and stores this real APPROVED test case's
/// embedding, so a future generation request can retrieve it as a
/// grounding example (`find_similar_approved_test_cases` above). Called
/// after every successful transition that leaves a case APPROVED
/// (`approve_test_case_scoped`, and `update_test_case_scoped` when the
/// edited case was already approved) — never for a GENERATED/VALIDATED/
/// REJECTED case, matching `store.rs::list_test_case_embeddings`'s own
/// "one row per ever-approved case" contract. Fails open, same reasoning as
/// `find_similar_approved_test_cases`: a real embedding-provider failure
/// must never fail the approval/edit itself — logged honestly, never
/// propagated as a 500 for what is genuinely supplementary grounding
/// infrastructure, not the core feature this endpoint exists for (same
/// real precedent `create_test_case_scoped`'s own graceful requirement-
/// graph-link failure handling already set).
async fn embed_approved_test_case(state: &AppState, row: &crate::store::TestCaseRow) {
    if !state.embedder.is_configured() {
        return;
    }
    let text = format!(
        "{}\n{}\n{}",
        row.title, row.description, row.expected_result
    );
    let vector = match state.embedder.embed(&text).await {
        Ok(v) => v,
        Err(e) => {
            state
                .log_bus
                .emit(
                    "WARN",
                    "generation",
                    &format!(
                        "Could not compute this test case's embedding, similar-case grounding will not include it yet: {e}"
                    ),
                    None,
                    Some(&row.project_id),
                    None,
                )
                .await;
            return;
        }
    };
    if let Err(e) = state
        .store
        .upsert_test_case_embedding(
            row.id,
            &row.project_id,
            &state.embedder.model_name(),
            state.embedder.dimensions(),
            &vector,
        )
        .await
    {
        state
            .log_bus
            .emit(
                "WARN",
                "generation",
                &format!(
                    "Could not store this test case's embedding, similar-case grounding will not include it yet: {e}"
                ),
                None,
                Some(&row.project_id),
                None,
            )
            .await;
    }
}

/// Phase 2 gap-closing — document-level RAG. Persists this document's real
/// extracted text (`doc_extract::extract_text`, already computed by the
/// caller) so a LATER, unrelated-looking generation request can still
/// retrieve it (`find_similar_documents` below) — before this, an uploaded
/// document's real content only ever reached the ONE generation request it
/// happened to be attached to (`TestCaseGeneratorPage.tsx`'s own
/// `buildRequirementText()` folds it directly into that request's
/// `requirement_text`), then vanished from the system entirely the moment
/// that request finished. Called after every successful
/// `extract_document_text_scoped` call.
///
/// Fails open at every step, same contract `embed_approved_test_case` above
/// already holds itself to, but with one real difference: the text itself
/// is always stored (deduplicated by a real SHA-256 of its own content —
/// `store::upsert_document`'s own `ON CONFLICT`), even when no embedding
/// provider is configured, so it becomes retrievable retroactively the next
/// time this same document is re-uploaded after a provider is configured. A
/// real DB or embedding-provider failure here must never fail the
/// extraction response itself — that response is the core feature this
/// endpoint exists for; this persistence is genuinely supplementary.
async fn persist_and_embed_document(
    state: &AppState,
    project_id: Uuid,
    file_name: &str,
    extracted: &crate::doc_extract::ExtractedDocument,
) {
    let mut hasher = Sha256::new();
    hasher.update(extracted.text.as_bytes());
    let content_hash = hex::encode(hasher.finalize());

    let document_id = match state
        .store
        .upsert_document(
            &project_id.to_string(),
            file_name,
            &content_hash,
            &extracted.text,
            extracted.char_count as i64,
            extracted.truncated,
        )
        .await
    {
        Ok(id) => id,
        Err(e) => {
            state
                .log_bus
                .emit(
                    "WARN",
                    "generation",
                    &format!(
                        "Could not store this document's content, it will not be available as grounding for future generation requests: {e}"
                    ),
                    None,
                    Some(&project_id.to_string()),
                    None,
                )
                .await;
            return;
        }
    };

    if !state.embedder.is_configured() {
        return;
    }
    let vector = match state.embedder.embed(&extracted.text).await {
        Ok(v) => v,
        Err(e) => {
            state
                .log_bus
                .emit(
                    "WARN",
                    "generation",
                    &format!(
                        "Could not compute this document's embedding, it will not be available as similarity-based grounding yet: {e}"
                    ),
                    None,
                    Some(&project_id.to_string()),
                    None,
                )
                .await;
            return;
        }
    };
    if let Err(e) = state
        .store
        .set_document_embedding(
            document_id,
            &state.embedder.model_name(),
            state.embedder.dimensions(),
            &vector,
        )
        .await
    {
        state
            .log_bus
            .emit(
                "WARN",
                "generation",
                &format!(
                    "Could not store this document's embedding, it will not be available as similarity-based grounding yet: {e}"
                ),
                None,
                Some(&project_id.to_string()),
                None,
            )
            .await;
    }
}

/// Phase 2 gap-closing — document-level RAG. Same real retrieval shape as
/// `find_similar_approved_test_cases` above (embeds this request's own
/// requirement text, ranks this project's own stored embeddings by real
/// cosine similarity — `ai_generation::rank_similar_documents`), just
/// against `documents` instead of `test_case_embeddings`. Fails open at
/// every step, same disclosed contract: no embedding provider configured, a
/// real network failure, a project with no documents embedded yet, or a
/// genuinely corrupt/orphaned row all return an empty `Vec` — generation
/// proceeds exactly as it did before this feature existed. Kept as its own
/// independent embedding call rather than sharing one with
/// `find_similar_approved_test_cases` — same "each real embedding call site
/// stays self-contained" convention that function and `embed_approved_test_
/// case` already both separately follow; a real embedding call is cheap
/// enough that a v1 does not need to share one across two unrelated
/// retrieval pools.
async fn find_similar_documents(
    state: &AppState,
    project_id: Uuid,
    requirement_text: &str,
) -> Vec<crate::ai_generation::SimilarDocumentExample> {
    if !state.embedder.is_configured() {
        return Vec::new();
    }
    let query_embedding = match state.embedder.embed(requirement_text).await {
        Ok(v) => v,
        Err(e) => {
            state
                .log_bus
                .emit(
                    "WARN",
                    "generation",
                    &format!(
                        "Could not compute a query embedding, generating without document-based grounding: {e}"
                    ),
                    None,
                    Some(&project_id.to_string()),
                    None,
                )
                .await;
            return Vec::new();
        }
    };
    let candidates = match state
        .store
        .list_document_embeddings(&project_id.to_string())
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            state
                .log_bus
                .emit(
                    "WARN",
                    "generation",
                    &format!(
                        "Could not load this project's document embeddings, generating without document-based grounding: {e}"
                    ),
                    None,
                    Some(&project_id.to_string()),
                    None,
                )
                .await;
            return Vec::new();
        }
    };
    let ids = crate::ai_generation::rank_similar_documents(&query_embedding, &candidates);
    let mut examples = Vec::with_capacity(ids.len());
    for id in ids {
        // A real, if unusual, orphaned embedding (deleted through a path
        // other than a real `documents` row delete — none exists today) is
        // simply skipped, not a hard failure for the rest of generation.
        if let Ok(Some(row)) = state.store.get_document(id).await {
            examples.push(crate::ai_generation::SimilarDocumentExample {
                file_name: row.file_name,
                excerpt: row.text,
            });
        }
    }
    examples
}

/// POST .../test-cases/generate-ai — Phase 2. Real, skill-aware AI
/// generation (`ai_generation::AiModelRouter`). Produces drafts only, never
/// persists them — the frontend uses the existing
/// `POST .../test-cases` to save whichever drafts it wants, exactly like a
/// pre-existing local-template draft already does (no second, competing
/// persistence path). A `"fallback"` response is a normal 200, not a 5xx:
/// the endpoint did its real job (attempted AI generation and honestly
/// reports the outcome) — the frontend's own existing local generator is
/// what actually produces a result in that case, honestly labeled as such,
/// never silently presented as AI output.
async fn generate_ai_test_cases_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<GenerateAiTestCasesBody>,
) -> Result<Json<GenerateAiTestCasesResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;

    if body.requirement_text.trim().is_empty() {
        return Err(ApiError::validation("requirement_text must not be empty"));
    }

    let skills: Vec<crate::ai_generation::SkillInput> = body
        .skills
        .into_iter()
        .map(|s| crate::ai_generation::SkillInput {
            id: s.id,
            name: s.name,
            description: s.description,
        })
        .collect();

    state
        .log_bus
        .emit(
            "INFO",
            "generation",
            "Generating functional test cases with AI model",
            None,
            Some(&project_id.to_string()),
            None,
        )
        .await;

    let mut feedback = load_feedback_context_for_generation(&state, project_id).await;
    // Vector Store + RAG Retrieval v1 — additive, fails open (see the
    // function's own doc comment): a project with nothing embedded yet, or
    // no embedding provider configured, generates exactly as it did before
    // this feature existed. Phase 1 gap-closing: `generate_custom_testing_
    // scoped` below now gets this same real grounding too — see its own
    // comment for the one boundary that's still separate (crawl-run
    // grounding, not yet wired there).
    feedback.similar_cases =
        find_similar_approved_test_cases(&state, project_id, &body.requirement_text).await;
    let similar_cases_used = feedback.similar_cases.len();
    // Phase 2 gap-closing — document-level RAG, same fails-open contract:
    // a project with nothing embedded yet, or no embedding provider
    // configured, generates exactly as it did before this feature existed.
    feedback.similar_documents =
        find_similar_documents(&state, project_id, &body.requirement_text).await;
    let similar_documents_used = feedback.similar_documents.len();

    // Real, optional crawl grounding — resolved with the exact same
    // ownership check + task lookup `get_crawl_result_scoped` already uses,
    // so a `crawl_run_id` from another project (or one that never produced
    // a result) is a real, honest error here too, never silently ignored.
    let crawl_output: Option<crate::crawler::CrawlOutput> = match body.crawl_run_id {
        Some(run_id) => {
            scoped_run_summary(&state, run_id, project_id).await?;
            let tasks = scoped_run_tasks(&state, run_id).await?;
            let task = tasks
                .first()
                .ok_or_else(|| ApiError::not_found("crawl run has no tasks"))?;
            let output = crate::crawler::read_crawl_result(&task.id).map_err(|_| {
                ApiError::not_found(
                    "crawl result not found — the crawl may still be running or may not have completed",
                )
            })?;
            Some(output)
        }
        None => None,
    };
    let crawl_pages_used = crate::ai_generation::crawl_pages_used_count(crawl_output.as_ref());

    // Real, in-flight "which wave is running right now" signal for the Test
    // Case Generator's live skill indicator — emitted on the same
    // project-scoped log stream (`GET .../logs/ws`) this project's other
    // real-time consumers already use, distinguished only by message text
    // ("... started"), no new schema. `project_id_str` is bound here so the
    // reference handed to `generate_test_cases` outlives the call.
    let project_id_str = project_id.to_string();
    let outcome = crate::ai_generation::generate_test_cases(
        &state.ai_infra,
        &body.requirement_text,
        &skills,
        &feedback,
        crawl_output.as_ref(),
        Some((state.log_bus.as_ref(), project_id_str.as_str())),
    )
    .await;

    let response = match outcome {
        crate::ai_generation::GenerationOutcome::Ai {
            cases,
            provider,
            model,
            routing_score,
            fallback_used,
            routing_mode,
            waves,
            prompt_tokens: _,
            completion_tokens: _,
            latency_ms: _,
            estimated_cost_usd: _,
        } => {
            // Real totals summed from `waves`, not this variant's own
            // (last-wave-only, per its own doc comment) top-level fields.
            let usage = crate::ai_generation::aggregate_wave_usage(&waves);
            // The real routing decision, emitted on its own `ai_router`
            // source so the Orchestrator view can show *why* this route
            // won (score, mode, whether it was the primary choice) rather
            // than only the generation outcome. `fallback_used` is
            // reported truthfully — a fallback is never presented as the
            // original choice (`ai_router.rs`'s own rule).
            state
                .log_bus
                .emit(
                    if fallback_used { "WARN" } else { "INFO" },
                    "ai_router",
                    &format!(
                        "Routed TestCaseGeneration to {provider}/{model} (score {routing_score:.2}, {routing_mode} mode{})",
                        if fallback_used { ", fallback after primary choice failed" } else { "" }
                    ),
                    None,
                    Some(&project_id.to_string()),
                    None,
                )
                .await;
            // One real line per wave that actually ran (never per skipped
            // wave — a wave nothing was selected for is not news). A
            // partial-wave failure (Base succeeded, Dependent timed out)
            // stays visible here instead of silently reducing coverage with
            // no signal, the same standard `GenerationOutcome::Failed`
            // itself is already held to.
            for wave in &waves {
                if wave.skipped {
                    continue;
                }
                match &wave.failure_reason {
                    Some(reason) => {
                        state
                            .log_bus
                            .emit(
                                "WARN",
                                "generation",
                                &format!(
                                    "Wave '{}' ({}) failed, other waves' coverage is unaffected: {reason}",
                                    wave.wave,
                                    wave.skill_names.join(", ")
                                ),
                                None,
                                Some(&project_id.to_string()),
                                None,
                            )
                            .await;
                    }
                    None => {
                        state
                            .log_bus
                            .emit(
                                "INFO",
                                "generation",
                                &format!(
                                    "Wave '{}' ({}) generated {} case(s) via {}/{}",
                                    wave.wave,
                                    wave.skill_names.join(", "),
                                    wave.case_count,
                                    wave.provider.as_deref().unwrap_or("?"),
                                    wave.model.as_deref().unwrap_or("?"),
                                ),
                                None,
                                Some(&project_id.to_string()),
                                None,
                            )
                            .await;
                    }
                }
            }
            state
                .log_bus
                .emit(
                    "SUCCESS",
                    "generation",
                    &format!(
                        "Generated {} test case(s) via {provider} ({model})",
                        cases.len()
                    ),
                    None,
                    Some(&project_id.to_string()),
                    None,
                )
                .await;
            audit(
                &state,
                &ctx,
                "test_case.generate_ai",
                None,
                Some(&format!("{} case(s) via {provider}/{model}", cases.len())),
                "ok",
            )
            .await;
            GenerateAiTestCasesResponse {
                test_cases: cases,
                generation_method: "ai".to_string(),
                provider: Some(provider),
                model: Some(model),
                fallback_reason: None,
                waves,
                crawl_pages_used,
                similar_cases_used,
                similar_documents_used,
                total_prompt_tokens: usage.prompt_tokens,
                total_completion_tokens: usage.completion_tokens,
                total_estimated_cost_usd: usage.estimated_cost_usd,
                total_latency_ms: usage.latency_ms,
            }
        }
        crate::ai_generation::GenerationOutcome::Failed { reason } => {
            state
                .log_bus
                .emit(
                    "WARN",
                    "generation",
                    &format!("AI generation fallback: {reason}"),
                    None,
                    Some(&project_id.to_string()),
                    None,
                )
                .await;
            audit(
                &state,
                &ctx,
                "test_case.generate_ai",
                None,
                Some(&reason),
                "fallback",
            )
            .await;
            GenerateAiTestCasesResponse {
                test_cases: Vec::new(),
                generation_method: "fallback".to_string(),
                provider: None,
                model: None,
                fallback_reason: Some(reason),
                waves: Vec::new(),
                crawl_pages_used,
                similar_cases_used,
                similar_documents_used,
                total_prompt_tokens: None,
                total_completion_tokens: None,
                total_estimated_cost_usd: None,
                total_latency_ms: 0,
            }
        }
    };

    Ok(Json(response))
}

// ---------------------------------------------------------- Custom Testing

#[derive(Debug, Deserialize)]
struct CustomTestingGenerateBody {
    requirement_text: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    skills: Vec<SkillInputBody>,
}

/// `generation_source` is always exactly `"REAL_AI"` or `"UNAVAILABLE"` —
/// there is no deterministic local-fallback strategy generator in this
/// codebase, so a `"LOCAL_FALLBACK"` value is never produced (brief §6: do
/// not invent a fallback path that does not genuinely exist). `analysis` is
/// present whenever the real strategy analysis call succeeded, even if the
/// subsequent case-generation call then failed — see
/// `custom_testing::CustomTestingOutcome`'s own doc comment.
#[derive(Debug, Serialize)]
struct CustomTestingGenerateResponse {
    generation_source: String,
    analysis: Option<crate::custom_testing::CustomTestingAnalysis>,
    test_cases: Vec<crate::ai_generation::GeneratedTestCaseDraft>,
    provider: Option<String>,
    model: Option<String>,
    failure_reason: Option<String>,
}

/// POST .../custom-testing/generate — Phase 3. Real requirement -> AI
/// strategy -> test cases, reusing the exact existing central AI router
/// (`AiTask::RequirementAnalysis`, `AiTask::TestCaseGeneration` — both
/// already-existing task variants, the first newly wired up here for the
/// first time, the second reused byte-for-byte from `generate_ai_test_
/// cases_scoped` above via `ai_generation::generate_test_cases`) and the
/// same skill-resolution/quality/traceability/dedup gates. Produces drafts
/// only, never persists — the caller uses the existing `POST .../test-cases`
/// to save whichever drafts it wants, exactly like `generate-ai`'s own
/// drafts, so a saved custom-testing case is an ordinary `TestCaseRow`
/// indistinguishable from any other — it automatically gets the existing
/// validate/approve/handoff/compile pipeline with no new backend code.
async fn generate_custom_testing_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<CustomTestingGenerateBody>,
) -> Result<Json<CustomTestingGenerateResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;

    if body.requirement_text.trim().is_empty() {
        return Err(ApiError::validation("requirement_text must not be empty"));
    }

    let skills: Vec<crate::ai_generation::SkillInput> = body
        .skills
        .into_iter()
        .map(|s| crate::ai_generation::SkillInput {
            id: s.id,
            name: s.name,
            description: s.description,
        })
        .collect();

    let mut feedback = load_feedback_context_for_generation(&state, project_id).await;
    // Vector Store + RAG Retrieval v1 (Phase 1 gap-closing) — this endpoint
    // previously had a comment explicitly disclosing it did NOT get this
    // grounding, unlike `generate_ai_test_cases_scoped`. Same real
    // retrieval, same fails-open contract: no embedding provider
    // configured, a network failure, or nothing embedded yet all mean
    // "generate exactly as before." Crawl-run grounding remains a separate,
    // not-yet-wired gap for this endpoint (`CustomTestingGenerateBody` has
    // no `crawl_run_id` field at all yet) — not addressed by this change.
    feedback.similar_cases =
        find_similar_approved_test_cases(&state, project_id, &body.requirement_text).await;
    // Phase 2 gap-closing — document-level RAG, same fails-open contract.
    feedback.similar_documents =
        find_similar_documents(&state, project_id, &body.requirement_text).await;

    let outcome = crate::custom_testing::run(
        &state.ai_infra,
        &body.requirement_text,
        body.scope.as_deref(),
        &skills,
        &feedback,
    )
    .await;

    let response = match outcome {
        crate::custom_testing::CustomTestingOutcome::Ready {
            analysis,
            cases,
            provider,
            model,
        } => {
            audit(
                &state,
                &ctx,
                "custom_testing.generate",
                None,
                Some(&format!("{} case(s) via {provider}/{model}", cases.len())),
                "ok",
            )
            .await;
            CustomTestingGenerateResponse {
                generation_source: "REAL_AI".to_string(),
                analysis: Some(analysis),
                test_cases: cases,
                provider: Some(provider),
                model: Some(model),
                failure_reason: None,
            }
        }
        crate::custom_testing::CustomTestingOutcome::Failed { reason, analysis } => {
            audit(
                &state,
                &ctx,
                "custom_testing.generate",
                None,
                Some(&reason),
                "unavailable",
            )
            .await;
            CustomTestingGenerateResponse {
                generation_source: "UNAVAILABLE".to_string(),
                analysis,
                test_cases: Vec::new(),
                provider: None,
                model: None,
                failure_reason: Some(reason),
            }
        }
    };

    Ok(Json(response))
}

// -------------------------------------------------------------- API Testing

/// `task_status` is `TaskStatus` directly (real `SCREAMING_SNAKE_CASE`
/// serde, e.g. `"PASSED"`/`"FAILED"`/`"INFRA_ERROR"`) — the exact same real
/// vocabulary every other execution result in this app already uses, not a
/// second status string invented for this one page. `result` is `None`
/// only when the task's own evidence log genuinely couldn't be read/parsed
/// (never a placeholder standing in for a real one).
#[derive(Debug, Serialize)]
struct ApiTestExecutionResponse {
    run_id: Uuid,
    task_status: crate::domain::TaskStatus,
    reason: Option<String>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
    result: Option<crate::executor::ApiResultSummary>,
    response_body_preview: Option<String>,
    evidence_id: Option<Uuid>,
}

/// POST .../api-testing/execute — Phase 4. Real, ad-hoc HTTP request
/// execution. Builds an `ExecutionPlan` directly from the caller's own
/// explicit method/URL/headers/query/body/assertions
/// (`api_testing::validate_and_build_plan`), then reuses the exact Phase
/// 2.1 pipeline unmodified: `execution_plan::validate_plan` (real SSRF
/// guard) -> `execution_plan::materialize_task_def` -> `create_run_core`
/// (the same function every other run-creating endpoint calls) ->
/// `engine::spawn_run` -> `CompiledApiExecutor` -> a real, live HTTP
/// request. No new executor, no new execution engine, no new persistence
/// path. Waits (bounded, `API_TEST_WAIT_CEILING_SECS`, default 45s) for the
/// real run this same request just created to reach a terminal state — the
/// endpoint itself is the "poll", server-side, over this process's own
/// in-memory run record; the frontend receives one real, complete result,
/// not a second polling contract to implement.
async fn execute_api_test_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<crate::api_testing::AdHocApiRequest>,
) -> Result<Json<ApiTestExecutionResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;

    let task_id = Uuid::new_v4().to_string();
    let task_name = format!(
        "API Test: {} {}",
        body.method.trim().to_uppercase(),
        body.url
    );

    let plan = crate::api_testing::validate_and_build_plan(task_id.clone(), &body)
        .map_err(|(code, msg)| ApiError::validation(format!("{code}: {msg}")))?;

    // The real SSRF/structural guard — reused unmodified. A disallowed
    // target (private IP, cloud metadata address, non-http(s) scheme that
    // slipped past the earlier scheme check, etc.) is rejected here, before
    // any run is created and before any real network call happens.
    if let Err(reasons) = crate::execution_plan::validate_plan(&plan).await {
        return Err(ApiError::validation(format!(
            "TARGET_NOT_ALLOWED: {}",
            reasons.join("; ")
        )));
    }

    let task =
        crate::execution_plan::materialize_task_def(task_id.clone(), task_name.clone(), &plan)
            .map_err(ApiError::validation)?;
    let run_plan = RunPlan {
        name: task_name,
        tasks: vec![task],
        concurrency: 1,
    };

    let (_, Json(created)) =
        create_run_core(&state, &ctx, Some(project_id.to_string()), run_plan).await?;

    let wait_ceiling_secs: u64 = std::env::var("API_TEST_WAIT_CEILING_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(45);
    let terminal = crate::api_testing::run_to_completion(
        &state,
        created.run_id,
        std::time::Duration::from_secs(wait_ceiling_secs),
    )
    .await;

    let Some(task_state) = terminal else {
        audit(
            &state,
            &ctx,
            "api_testing.execute",
            None,
            Some("did not reach a terminal state within the wait ceiling"),
            "still_running",
        )
        .await;
        return Ok(Json(ApiTestExecutionResponse {
            run_id: created.run_id,
            task_status: crate::domain::TaskStatus::Running,
            reason: Some(format!(
                "Execution is still running past this endpoint's own {wait_ceiling_secs}s wait ceiling — the real run continues; check the Execution Hub (run {}) for its eventual real result.",
                created.run_id
            )),
            started_at: None,
            finished_at: None,
            result: None,
            response_body_preview: None,
            evidence_id: None,
        }));
    };

    let (evidence_id, log) =
        match crate::api_testing::read_task_evidence(&state, created.run_id, &task_id).await {
            Some((id, content)) => (Some(id), Some(content)),
            None => (None, None),
        };
    let result = log
        .as_deref()
        .and_then(crate::api_testing::parse_result_summary);
    let response_body_preview = log
        .as_deref()
        .and_then(crate::api_testing::parse_response_body_preview);

    let level = match task_state.status {
        TaskStatus::Passed => "SUCCESS",
        TaskStatus::Failed => "ERROR",
        _ => "INFO",
    };
    state
        .log_bus
        .emit(
            level,
            "api_testing",
            &format!(
                "Ad-hoc API test {} {} -> {:?}",
                body.method.trim().to_uppercase(),
                body.url,
                task_state.status
            ),
            Some(&created.run_id.to_string()),
            Some(&project_id.to_string()),
            Some(&task_id),
        )
        .await;

    audit(
        &state,
        &ctx,
        "api_testing.execute",
        None,
        Some(&format!(
            "{} {} -> {:?}",
            body.method.trim().to_uppercase(),
            body.url,
            task_state.status
        )),
        "ok",
    )
    .await;

    Ok(Json(ApiTestExecutionResponse {
        run_id: created.run_id,
        task_status: task_state.status,
        reason: task_state.reason,
        started_at: task_state.started_at,
        finished_at: task_state.finished_at,
        result,
        response_body_preview,
        evidence_id,
    }))
}

// ------------------------------ Truthfulness Cleanup Step 1 — Application Source

#[derive(Debug, Deserialize)]
struct VerifyApplicationSourceBody {
    url: String,
}

/// POST /api/v1/projects/:project_id/verify-application-source — real,
/// bounded connectivity verification for the Test Case Generator's
/// "Application Sources" panel (`app_source.rs`). `Role::Operator`: this
/// makes a real outbound network request (bounded, SSRF-guarded, redirects
/// disabled), the same authorization bar every other real-outbound-action
/// endpoint in this app (API Testing's `execute`, Performance Testing's
/// `start`) already requires — never left open to a lower role just because
/// nothing is persisted.
async fn verify_application_source_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<VerifyApplicationSourceBody>,
) -> Result<Json<crate::app_source::VerifyApplicationSourceResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;

    let result = crate::app_source::verify(&body.url).await;
    let level = if result.reachable { "SUCCESS" } else { "WARN" };
    state
        .log_bus
        .emit(
            level,
            "app_source",
            &format!("Target URL check for '{}': {}", body.url, result.status),
            None,
            Some(&project_id.to_string()),
            None,
        )
        .await;
    audit(
        &state,
        &ctx,
        "app_source.verify",
        None,
        Some(&format!("{} -> {}", body.url, result.status)),
        if result.reachable {
            "ok"
        } else {
            "not_reachable"
        },
    )
    .await;
    Ok(Json(result))
}

#[derive(Deserialize)]
struct ExtractDocumentTextBody {
    file_name: String,
    /// Standard base64 (RFC 4648) of the real, unmodified file bytes —
    /// sent as JSON rather than multipart/form-data so this endpoint uses
    /// the exact same `ApiJson` request path every other endpoint in this
    /// API already does, instead of a second, one-off upload mechanism.
    content_base64: String,
}

/// Base64 text length before decoding, not the decoded byte count — a
/// generous ceiling for the real requirement documents (PRD/FRD/SRD/etc.)
/// this endpoint exists for, while still bounding memory use against an
/// oversized/malicious payload before a single byte is decoded.
const MAX_DOCUMENT_BASE64_LEN: usize = 20_000_000;

/// POST /api/v1/projects/:project_id/documents/extract-text — real content
/// extraction for one uploaded Product Documentation file
/// (`doc_extract.rs`). Before this endpoint existed, an uploaded PRD/FRD
/// contributed nothing but its own filename to AI generation; this is what
/// makes that content genuinely available. `Role::Operator`, matching
/// every other endpoint that does real, non-trivial server-side work on
/// caller-supplied content in this project.
async fn extract_document_text_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<ExtractDocumentTextBody>,
) -> Result<Json<crate::doc_extract::ExtractedDocument>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;

    if body.content_base64.len() > MAX_DOCUMENT_BASE64_LEN {
        return Err(ApiError::validation(
            "this file is too large for content extraction (20MB encoded limit).",
        ));
    }

    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(body.content_base64.as_bytes())
        .map_err(|e| ApiError::validation(format!("content_base64 is not valid base64: {e}")))?;

    let result = crate::doc_extract::extract_text(&body.file_name, &bytes);
    audit(
        &state,
        &ctx,
        "document.extract_text",
        None,
        Some(&body.file_name),
        if result.is_ok() {
            "ok"
        } else {
            "extraction_failed"
        },
    )
    .await;

    // Phase 2 gap-closing — document-level RAG: a successful extraction is
    // also persisted (and embedded, when a provider is configured) so this
    // document's real content remains available as grounding for a LATER
    // generation request, not just this one — see
    // `persist_and_embed_document`'s own doc comment. Fails open, same as
    // that function's own contract: this never turns a successful
    // extraction into a failed response.
    if let Ok(extracted) = &result {
        persist_and_embed_document(&state, project_id, &body.file_name, extracted).await;
    }

    result.map(Json).map_err(ApiError::validation)
}

// ---------------------------------------------------- Phase 6 — Performance Testing

#[derive(Debug, Serialize)]
struct PerformanceTestStartResponse {
    run_id: Uuid,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct PerformanceRunStatusResponse {
    run_id: Uuid,
    name: String,
    test_type: String,
    status: String,
    verdict: Option<String>,
    verdict_reasons: Vec<String>,
    /// Set only for a genuine `ERROR` result (§9 of the Phase 6 ERROR-fix
    /// brief) — the same real, safe, sanitized message also present as
    /// `verdict_reasons[0]` for backward-compatible rendering, exposed here
    /// too as its own explicit field rather than requiring a caller to know
    /// that convention. `null` for every other status.
    error_reason: Option<String>,
    elapsed_ms: u64,
    current_concurrency: u32,
    sample_count: u64,
    metrics: Option<crate::performance_testing::PerformanceMetrics>,
    samples: Vec<crate::performance_testing::RequestSample>,
    config: crate::performance_testing::PerformanceTestConfig,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
    evidence_id: Option<Uuid>,
    /// `true` when this reflects the live, in-process workload handle
    /// (real-time samples, recomputed-on-read metrics/verdict); `false`
    /// when falling back to the durable DB row because no live handle
    /// exists in *this* process (a different process ran it, or this
    /// process restarted) — `samples` is honestly empty in that case; the
    /// full raw sample set, if it was persisted, is still fetchable via the
    /// existing `evidence_id`/`.../evidence/:id/content` endpoint.
    live: bool,
}

/// POST /api/v1/projects/:project_id/performance-testing/start — real Load/
/// Stress/Spike/Volume workload kickoff. Validates the target (reused,
/// unmodified `execution_plan::validate_plan` — real SSRF guard) and the
/// schedule against server-enforced `SafetyLimits` *before* creating any
/// persisted row or spawning any real request, then returns immediately
/// (`status: RUNNING`) — the real workload runs in a background task;
/// progress is polled via the `.../status` endpoint below, never faked.
async fn start_performance_test_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<crate::performance_testing::PerformanceTestConfig>,
) -> Result<Json<PerformanceTestStartResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;

    let limits = crate::performance_testing::SafetyLimits::from_env();
    crate::performance_testing::validate_schedule(&body.schedule, &limits)
        .map_err(|(code, msg)| ApiError::validation(format!("{code}: {msg}")))?;

    let run_id = Uuid::new_v4();
    let plan = crate::performance_testing::build_request_plan(
        &run_id.to_string(),
        &body.request,
        body.expected_status,
    )
    .map_err(|(code, msg)| ApiError::validation(format!("{code}: {msg}")))?;

    // The real SSRF/structural guard — reused unmodified, run once before
    // the workload starts (every repeated request inside the workload
    // reuses this same, already-validated plan; see `executor::
    // execute_plan_once`'s doc comment for why it is not re-run per sample).
    if let Err(reasons) = crate::execution_plan::validate_plan(&plan).await {
        return Err(ApiError::validation(format!(
            "TARGET_NOT_ALLOWED: {}",
            reasons.join("; ")
        )));
    }

    let test_type = body.schedule.type_name();
    let name = format!(
        "{test_type} test: {} {}",
        body.request.method.trim().to_uppercase(),
        body.request.url
    );
    let config_json = serde_json::to_string(&body).map_err(to_500)?;

    let handle = crate::performance_testing::PerformanceRunHandle::new(run_id);
    state
        .store
        .insert_performance_run(
            run_id,
            &project_id.to_string(),
            &name,
            test_type,
            &config_json,
            Some(&ctx.name),
            handle.started_at,
        )
        .await
        .map_err(to_500)?;
    state
        .performance_tests
        .write()
        .await
        .insert(run_id, handle.clone());

    state
        .log_bus
        .emit(
            "INFO",
            "performance",
            &format!("Performance test started: {name}"),
            Some(&run_id.to_string()),
            Some(&project_id.to_string()),
            None,
        )
        .await;

    // One real `reqwest::Client` built here and reused for every request
    // this whole workload makes — real HTTP keep-alive connection pooling,
    // not a fresh TCP connection (and fresh local ephemeral port) per
    // single request. Same `API_TEST_TIMEOUT_SECS` convention `CompiledApiExecutor`
    // already uses for its own per-request timeout.
    let timeout = crate::executor::CompiledApiExecutor::from_env().timeout;
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(to_500)?;
    let plan = Arc::new(plan);
    let schedule = body.schedule;
    let config = body.clone();
    let state2 = state.clone();
    let handle2 = handle.clone();
    // Real Tokio task supervision (`performance_testing::supervise_workload_task`)
    // — the fix for a genuine gap this phase's own live verification found: a
    // bare, unobserved `tokio::spawn` here meant a real panic inside the
    // workload task silently vanished, leaving the persisted row stuck at
    // RUNNING until the *next process restart's* recovery sweep eventually
    // noticed. `supervise_workload_task` owns the real `JoinHandle` and
    // reacts immediately: `Ok(())` is the exact same normal-completion path
    // as before; `Err(failure)` is a genuine, unexpected task-level failure,
    // finalized as a real, persisted `ERROR` — not silently orphaned.
    tokio::spawn(async move {
        let handle3 = handle2.clone();
        let outcome = crate::performance_testing::supervise_workload_task(async move {
            crate::performance_testing::run_workload(client, plan, schedule, limits, handle3).await;
        })
        .await;
        match outcome {
            Ok(()) => finalize_performance_run(&state2, run_id, &config, &handle2).await,
            Err(failure) => {
                finalize_performance_run_as_error(&state2, run_id, &handle2, &failure).await
            }
        }
    });

    audit(
        &state,
        &ctx,
        "performance_testing.start",
        Some(&run_id.to_string()),
        Some(&name),
        "ok",
    )
    .await;

    Ok(Json(PerformanceTestStartResponse {
        run_id,
        status: "RUNNING",
    }))
}

/// Runs exactly once, right after `run_workload` returns — computes real
/// metrics/verdict from the handle's final sample set, persists the raw
/// samples via the *existing* evidence store (same convention
/// `CompiledApiExecutor`'s own evidence already uses), and writes the one
/// terminal `performance_runs` row update.
async fn finalize_performance_run(
    state: &AppState,
    run_id: Uuid,
    config: &crate::performance_testing::PerformanceTestConfig,
    handle: &crate::performance_testing::PerformanceRunHandle,
) {
    let samples = handle.samples.read().await.clone();
    // `run_workload` freezes this the instant it actually finishes — use
    // it in preference to a fresh `now - started_at` read, which would
    // include whatever real time has passed since (scheduling delay
    // between `run_workload` returning and this function actually running).
    let actual_duration_ms = match *handle.finished_elapsed_ms.read().await {
        Some(ms) => ms,
        None => (chrono::Utc::now() - handle.started_at)
            .num_milliseconds()
            .max(0) as u64,
    };
    let metrics = crate::performance_testing::compute_metrics(&samples, actual_duration_ms);
    let cancelled = handle
        .user_cancelled
        .load(std::sync::atomic::Ordering::SeqCst);
    let (verdict, reasons) =
        crate::performance_testing::evaluate_verdict(config, &metrics, cancelled);

    let evidence_id = match serde_json::to_string(&samples) {
        Ok(samples_json) => match state.evidence.put(
            &run_id.to_string(),
            "performance-test",
            "raw-samples",
            &samples_json,
        ) {
            Ok((object_key, size)) => state
                .store
                .insert_evidence(
                    run_id,
                    "performance-test",
                    "raw-samples",
                    &object_key,
                    "application/json",
                    size,
                )
                .await
                .ok(),
            Err(e) => {
                tracing::error!(
                    "failed to persist performance test raw samples for run {run_id}: {e}"
                );
                None
            }
        },
        Err(e) => {
            tracing::error!(
                "failed to serialize performance test raw samples for run {run_id}: {e}"
            );
            None
        }
    };

    let status_str = if cancelled { "CANCELLED" } else { "COMPLETED" };
    let metrics_json = serde_json::to_string(&metrics).ok();
    let reasons_json = serde_json::to_string(&reasons).ok();
    if let Err(e) = state
        .store
        .finalize_performance_run(
            run_id,
            status_str,
            Some(verdict.as_str()),
            metrics_json.as_deref(),
            reasons_json.as_deref(),
            None,
            evidence_id,
        )
        .await
    {
        tracing::error!("failed to persist final performance test result for run {run_id}: {e}");
    }
}

/// Runs when `supervise_workload_task` reports that the workload task
/// itself failed unexpectedly (§ the fix this function exists for) — a
/// genuine, immediate ERROR finalization, never a silently orphaned
/// `RUNNING` row left for the next restart's recovery sweep to eventually
/// notice. Preserves whatever real samples the handle had already collected
/// before the failure (never invents the rest — §6 of the fix brief: "Never
/// invent missing samples"), freezes a real elapsed-time snapshot at the
/// moment of detection (the task is *provably* no longer running here —
/// `supervise_workload_task`'s own `join.await` only returns after the
/// spawned task has fully exited, so `now - started_at` at this exact point
/// is the real, true elapsed duration, not an approximation), and persists
/// only `failure`'s already-sanitized `safe_message` — never a raw panic
/// payload.
///
/// `pub(crate)`, not part of the public HTTP surface: this exists so
/// `api_tests.rs` can exercise the real persistence round-trip of a genuine,
/// deterministic supervision failure (obtained via a real panic through the
/// real `supervise_workload_task`) without needing to make the real
/// Load/Stress/Spike/Volume workload logic itself panic, which this fix
/// deliberately leaves untouched — see the fix report's own test-design
/// section for the full reasoning.
pub(crate) async fn finalize_performance_run_as_error(
    state: &AppState,
    run_id: Uuid,
    handle: &crate::performance_testing::PerformanceRunHandle,
    failure: &crate::performance_testing::SupervisionFailure,
) {
    let samples = handle.samples.read().await.clone();
    let elapsed_ms = (chrono::Utc::now() - handle.started_at)
        .num_milliseconds()
        .max(0) as u64;
    *handle.finished_elapsed_ms.write().await = Some(elapsed_ms);
    let metrics = crate::performance_testing::compute_metrics(&samples, elapsed_ms);

    *handle.error_reason.write().await = Some(failure.safe_message.clone());
    *handle.state.write().await = crate::performance_testing::PerfRunState::Error;

    let evidence_id = match serde_json::to_string(&samples) {
        Ok(samples_json) => match state.evidence.put(
            &run_id.to_string(),
            "performance-test",
            "raw-samples",
            &samples_json,
        ) {
            Ok((object_key, size)) => state
                .store
                .insert_evidence(
                    run_id,
                    "performance-test",
                    "raw-samples",
                    &object_key,
                    "application/json",
                    size,
                )
                .await
                .ok(),
            Err(e) => {
                tracing::error!("failed to persist partial performance test raw samples for run {run_id} after an internal execution failure: {e}");
                None
            }
        },
        Err(e) => {
            tracing::error!("failed to serialize partial performance test raw samples for run {run_id} after an internal execution failure: {e}");
            None
        }
    };

    let metrics_json = serde_json::to_string(&metrics).ok();
    // Also mirrored into `verdict_reasons_json` (not just the dedicated
    // `error_reason` column) so a reload after the live handle is gone
    // (`get_performance_run_status_scoped`'s durable-row fallback, which
    // only ever reads `verdict_reasons_json` — the same real field every
    // other terminal verdict already uses) still shows the real reason,
    // not an empty list. A real gap this fix's own tests caught: an earlier
    // version left this `None`, and a durable-fallback read of an ERROR run
    // showed no reason at all even though `error_reason` was separately
    // persisted.
    let reasons_json = serde_json::to_string(std::slice::from_ref(&failure.safe_message)).ok();
    tracing::error!(
        "performance test run {run_id} failed with a genuine internal execution error: {}",
        failure.safe_message
    );
    if let Err(e) = state
        .store
        .finalize_performance_run(
            run_id,
            "ERROR",
            Some(crate::performance_testing::PerfVerdict::Error.as_str()),
            metrics_json.as_deref(),
            reasons_json.as_deref(),
            Some(&failure.safe_message),
            evidence_id,
        )
        .await
    {
        tracing::error!("failed to persist ERROR result for performance test run {run_id}: {e}");
    }
}

/// GET /api/v1/projects/:project_id/performance-testing/:run_id/status —
/// real, live progress while running (real elapsed time, real sample count,
/// real current concurrency, metrics recomputed from the real samples
/// gathered so far — never a fabricated percentage), falling back to the
/// durable row once no live handle exists in this process.
async fn get_performance_run_status_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<PerformanceRunStatusResponse>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = state
        .store
        .get_performance_run(run_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("performance test run not found"))?;
    require_resource_in_project(Some(&row.project_id), project_id)?;

    if let Some(handle) = state.performance_tests.read().await.get(&run_id).cloned() {
        let config: crate::performance_testing::PerformanceTestConfig =
            serde_json::from_str(&row.config_json).map_err(to_500)?;
        let samples = handle.samples.read().await.clone();
        let sample_count = samples.len() as u64;
        // While RUNNING, real, live elapsed time (`now - started_at`) is
        // exactly right for a live progress read. Once terminal, the run's
        // own `finished_elapsed_ms` (frozen by `run_workload` the instant
        // it actually finished) must be used instead — otherwise every
        // later poll of an already-completed run would recompute
        // `metrics`/`throughput_rps` against a denominator that keeps
        // growing with wall-clock time even though nothing is still
        // happening, silently shrinking the reported throughput on repeat
        // reads (a real bug caught during this phase's own live
        // verification — see the report's bug log).
        let frozen_elapsed = *handle.finished_elapsed_ms.read().await;
        let elapsed_ms = match frozen_elapsed {
            Some(ms) => ms,
            None => (chrono::Utc::now() - handle.started_at)
                .num_milliseconds()
                .max(0) as u64,
        };
        let metrics = crate::performance_testing::compute_metrics(&samples, elapsed_ms);
        let run_state = *handle.state.read().await;
        // `PerfRunState::Error` is deliberately never routed through
        // `evaluate_verdict` — that function has no genuine ERROR-producing
        // path of its own (it only ever compares real metrics against
        // configured thresholds) and would misclassify a genuine task-level
        // failure as COMPLETED/PASSED/FAILED/CANCELLED from whatever partial
        // samples happened to exist. The real, safe reason
        // `finalize_performance_run_as_error` recorded is used verbatim.
        let (verdict, reasons, error_reason) = match run_state {
            crate::performance_testing::PerfRunState::Running => (None, Vec::new(), None),
            crate::performance_testing::PerfRunState::Error => {
                let reason =
                    handle.error_reason.read().await.clone().unwrap_or_else(|| {
                        "performance execution terminated unexpectedly".to_string()
                    });
                (
                    Some(
                        crate::performance_testing::PerfVerdict::Error
                            .as_str()
                            .to_string(),
                    ),
                    vec![reason.clone()],
                    Some(reason),
                )
            }
            _ => {
                let cancelled = handle
                    .user_cancelled
                    .load(std::sync::atomic::Ordering::SeqCst);
                let (v, r) =
                    crate::performance_testing::evaluate_verdict(&config, &metrics, cancelled);
                (Some(v.as_str().to_string()), r, None)
            }
        };
        return Ok(Json(PerformanceRunStatusResponse {
            run_id,
            name: row.name,
            test_type: row.test_type,
            status: run_state.as_str().to_string(),
            verdict,
            verdict_reasons: reasons,
            error_reason,
            elapsed_ms,
            current_concurrency: handle
                .current_concurrency
                .load(std::sync::atomic::Ordering::SeqCst),
            sample_count,
            metrics: Some(metrics),
            samples,
            config,
            started_at: handle.started_at,
            finished_at: row.finished_at,
            evidence_id: row.evidence_id,
            live: true,
        }));
    }

    let config: crate::performance_testing::PerformanceTestConfig =
        serde_json::from_str(&row.config_json).map_err(to_500)?;
    let metrics = row
        .metrics_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());
    let reasons: Vec<String> = row
        .verdict_reasons_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let elapsed_ms = row
        .finished_at
        .unwrap_or_else(chrono::Utc::now)
        .signed_duration_since(row.started_at)
        .num_milliseconds()
        .max(0) as u64;
    Ok(Json(PerformanceRunStatusResponse {
        run_id,
        name: row.name,
        test_type: row.test_type,
        status: row.status,
        verdict: row.verdict,
        verdict_reasons: reasons,
        error_reason: row.error_reason,
        elapsed_ms,
        current_concurrency: 0,
        sample_count: 0,
        metrics,
        samples: Vec::new(),
        config,
        started_at: row.started_at,
        finished_at: row.finished_at,
        evidence_id: row.evidence_id,
        live: false,
    }))
}

#[derive(Debug, Serialize)]
struct PerformanceTestCancelResponse {
    run_id: Uuid,
    cancelled: bool,
    reason: Option<&'static str>,
}

/// POST /api/v1/projects/:project_id/performance-testing/:run_id/cancel —
/// real cancellation (`PerformanceRunHandle::request_cancel`, races and
/// interrupts in-flight requests, not just "stop starting new ones"). Only
/// reachable when a live handle exists in *this* process — an honest
/// limitation disclosed via `reason`, not silently pretended to succeed,
/// same class of in-memory-only limitation `engine.rs`'s own run
/// cancellation already has.
async fn cancel_performance_run_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<PerformanceTestCancelResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = state
        .store
        .get_performance_run(run_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("performance test run not found"))?;
    require_resource_in_project(Some(&row.project_id), project_id)?;

    let Some(handle) = state.performance_tests.read().await.get(&run_id).cloned() else {
        audit(
            &state,
            &ctx,
            "performance_testing.cancel",
            Some(&run_id.to_string()),
            Some("no live handle in this process"),
            "unavailable",
        )
        .await;
        return Ok(Json(PerformanceTestCancelResponse {
            run_id,
            cancelled: false,
            reason: Some("no live handle for this run exists in this process — it may have already finished, or this process restarted since it started"),
        }));
    };
    let cancelled = handle.request_cancel().await;
    audit(
        &state,
        &ctx,
        "performance_testing.cancel",
        Some(&run_id.to_string()),
        None,
        if cancelled { "ok" } else { "already_terminal" },
    )
    .await;
    Ok(Json(PerformanceTestCancelResponse {
        run_id,
        cancelled,
        reason: if cancelled {
            None
        } else {
            Some("run was already terminal")
        },
    }))
}

/// GET /api/v1/projects/:project_id/performance-testing — real, persisted
/// run history for this project, newest first.
async fn list_performance_runs_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<Vec<crate::store::PerformanceRunRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let rows = state
        .store
        .list_performance_runs(&project_id.to_string())
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

// ------------------------------------------------------ AI Intelligence Layer

/// GET /api/v1/ai/status — real, current discovery snapshot + active
/// settings. Never triggers a fresh discovery pass itself (that's
/// `POST /api/v1/ai/refresh`, an explicit action) — this returns whatever
/// is already cached, honestly, including an empty provider list if
/// nothing has been discovered yet.
/// GET /api/v1/ai/status — real discovery snapshot + settings, plus a
/// real `routing_preview`: what `AiInfrastructure::route_among` would
/// actually select right now for this app's primary AI task
/// (`TEST_CASE_GENERATION`), computed with the exact same scoring
/// function real generation uses — never a separate, hand-picked "here's
/// what looks good" summary. `null` when no compatible model currently
/// exists, exactly like a real generation attempt would honestly fail.
async fn get_ai_status(
    State(state): State<AppState>,
    Extension(_ctx): Extension<AuthContext>,
) -> Json<serde_json::Value> {
    let providers = state.ai_infra.providers_snapshot().await;
    let settings = state.ai_infra.settings().await;
    let routing_preview = crate::ai_router::AiInfrastructure::preview_route(
        &providers,
        crate::ai_router::AiTask::TestCaseGeneration,
        &settings,
    );
    Json(
        serde_json::json!({ "providers": providers, "settings": settings, "routing_preview": routing_preview }),
    )
}

async fn get_ai_settings_handler(
    State(state): State<AppState>,
    Extension(_ctx): Extension<AuthContext>,
) -> Json<crate::ai_router::AiSettings> {
    Json(state.ai_infra.settings().await)
}

/// PUT /api/v1/ai/settings — the only mutating route in this layer.
/// Persists for real (`ai_settings`, a real singleton row) and updates the
/// in-memory router in the same request, so the very next generation call
/// already reflects the new mode/preference — never a setting that "looks"
/// saved but doesn't actually change routing behavior.
async fn put_ai_settings_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiJson(body): ApiJson<crate::ai_router::AiSettings>,
) -> Result<Json<crate::ai_router::AiSettings>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    const MAX_TASK_CHAIN: usize = 8;
    for (task, preference) in &body.task_overrides {
        if preference.chain.len() > MAX_TASK_CHAIN {
            return Err(ApiError::validation(format!(
                "the chain for {task:?} may have at most {MAX_TASK_CHAIN} entries"
            )));
        }
        if preference
            .chain
            .iter()
            .any(|t| t.provider.trim().is_empty() || t.model.trim().is_empty())
        {
            return Err(ApiError::validation(format!(
                "every chain entry for {task:?} needs a provider and a model"
            )));
        }
    }
    let mode_str = match body.mode {
        crate::ai_router::RoutingMode::Auto => "AUTO",
        crate::ai_router::RoutingMode::Manual => "MANUAL",
    };
    // `task_overrides` round-trips as real JSON text — `None` (not `"{}"`)
    // when the map is empty, so a settings row that never used this field
    // stores the same NULL it always has, not a needless empty object.
    let task_overrides_json = if body.task_overrides.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&body.task_overrides).map_err(to_500)?)
    };
    state
        .store
        .upsert_ai_settings(
            mode_str,
            body.preferred_provider.as_deref(),
            body.preferred_model.as_deref(),
            body.fallback_enabled,
            task_overrides_json.as_deref(),
        )
        .await
        .map_err(to_500)?;
    state.ai_infra.set_settings(body.clone()).await;
    audit(
        &state,
        &ctx,
        "ai.settings.update",
        None,
        Some(mode_str),
        "ok",
    )
    .await;
    Ok(Json(body))
}

/// POST /api/v1/ai/refresh — the explicit, on-demand re-discovery brief
/// §15 calls for ("Manual refresh"), distinct from the TTL-bounded lazy
/// refresh `generate` itself uses.
async fn refresh_ai_providers(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<Json<Vec<crate::ai_router::AiProviderInfo>>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let providers = state.ai_infra.discover_all().await;
    audit(
        &state,
        &ctx,
        "ai.providers.refresh",
        None,
        Some(&format!("{} provider(s)", providers.len())),
        "ok",
    )
    .await;
    Ok(Json(providers))
}

// -------------------------------------------------------------- run create

#[derive(Serialize)]
struct CreateRunResponse {
    run_id: Uuid,
    state: RunState,
    policy_notes: Option<String>,
}

/// The actual run-creation logic (FRD §1 Execution Lifecycle: plan
/// validation -> dependency graph -> policy gate -> scheduling ->
/// persistence), shared verbatim by the project-scoped
/// `POST /api/v1/projects/:project_id/executions` handler (Phase B cleanup:
/// the legacy flat `POST /runs` handler that also used to share it is
/// removed). `project_id` here is
/// already resolved and, for the scoped caller, already verified against a
/// real project by `require_scoped_project`; this function does not decide
/// it, only uses it.
async fn create_run_core(
    state: &AppState,
    ctx: &AuthContext,
    project_id: Option<String>,
    plan: RunPlan,
) -> Result<(StatusCode, Json<CreateRunResponse>), ApiError> {
    auth::require_role(ctx, Role::Operator)?;
    validate_plan(&plan).map_err(ApiError::validation)?;

    let decision = policy::evaluate(&state.policy, &plan);

    let (initial_state, policy_notes, spawn_now) = match &decision {
        PolicyDecision::Allow => (RunState::Scheduled, None, true),
        PolicyDecision::RequireApproval(reason) => {
            (RunState::PendingApproval, Some(reason.clone()), false)
        }
        PolicyDecision::Deny(reason) => {
            audit(state, ctx, "run.create", None, Some(reason), "denied").await;
            return Err(ApiError::forbidden(format!("policy denied: {reason}")));
        }
    };

    let run_id = Uuid::new_v4();
    state
        .store
        .insert_run(
            run_id,
            &plan,
            project_id.as_deref(),
            Some(&ctx.name),
            initial_state,
            policy_notes.as_deref(),
        )
        .await
        .map_err(to_500)?;

    // BE-14 — resolve this run's project realtime stream (if any) up front,
    // so `emit()` can fan every event into it without needing `AppState`
    // itself (keeps `engine.rs` decoupled from `state.rs`).
    let project_stream = match &project_id {
        Some(pid) => Some(state.project_stream(pid).await),
        None => None,
    };
    let record = RunRecord::new_with_log_bus(
        run_id,
        &plan,
        project_id.clone(),
        Some(ctx.name.clone()),
        initial_state,
        policy_notes.clone(),
        project_stream,
        Some(state.log_bus.clone()),
    );
    state.runs.write().await.insert(run_id, record.clone());

    audit(
        state,
        ctx,
        "run.create",
        Some(&run_id.to_string()),
        policy_notes.as_deref(),
        "ok",
    )
    .await;

    if spawn_now {
        engine::spawn_run(
            record,
            state.registry.clone(),
            plan.concurrency,
            state.store.clone(),
            state.evidence.clone(),
            state.policy.clone(),
            engine::default_task_timeout(),
        );
    }

    Ok((
        StatusCode::CREATED,
        Json(CreateRunResponse {
            run_id,
            state: initial_state,
            policy_notes,
        }),
    ))
}

// ---------------------------------------------------------------- listing

#[derive(Serialize)]
struct RunListItem {
    run_id: Uuid,
    name: String,
    state: RunState,
    total: usize,
    passed: usize,
    failed: usize,
    project_id: Option<String>,
}

#[derive(Deserialize)]
struct ListRunsQuery {
    limit: Option<i64>,
}

#[derive(Serialize)]
struct RunDetail {
    #[serde(flatten)]
    summary: RunSummary,
    tasks: Vec<TaskState>,
}

#[derive(Deserialize)]
struct EventsQuery {
    since: Option<u64>,
}

// ------------------------------------------------------------- governance

/// B11/L4 remediation. Rebuilds a live `Arc<RunRecord>` for a run that
/// exists durably in the store (its full `RunPlan` is persisted verbatim —
/// `store.rs::insert_run`) but is not held in *this* process's memory —
/// exactly the situation a restart leaves a still-`PENDING_APPROVAL` run
/// in, now that `recover_incomplete_runs` no longer destroys it (see that
/// function's doc comment). Inserts the rebuilt record into `state.runs`
/// so every subsequent lookup — this request's own state check, a later
/// `GET`, cancellation, WS subscription once it starts running — sees it
/// exactly as if this process had created it. `Ok(None)` (not an error)
/// when the run simply doesn't exist in the store at all.
async fn rehydrate_run_record(
    state: &AppState,
    run_id: Uuid,
) -> Result<Option<Arc<RunRecord>>, ApiError> {
    let Some((plan, project_id, requested_by, policy_notes, run_state)) = state
        .store
        .get_run_for_rehydration(run_id)
        .await
        .map_err(to_500)?
    else {
        return Ok(None);
    };
    let project_stream = match &project_id {
        Some(pid) => Some(state.project_stream(pid).await),
        None => None,
    };
    let record = RunRecord::new_with_log_bus(
        run_id,
        &plan,
        project_id,
        requested_by,
        run_state,
        policy_notes,
        project_stream,
        Some(state.log_bus.clone()),
    );
    state.runs.write().await.insert(run_id, record.clone());
    Ok(Some(record))
}

/// Shared by both approve/reject handlers (scoped): in-memory
/// lookup first (the common, same-process case), falling back to
/// rehydration from the store (B11/L4). A 404 here means the run genuinely
/// does not exist anywhere, not merely that this process hasn't seen it.
async fn find_or_rehydrate_run(state: &AppState, run_id: Uuid) -> Result<Arc<RunRecord>, ApiError> {
    if let Some(record) = state.runs.read().await.get(&run_id).cloned() {
        return Ok(record);
    }
    rehydrate_run_record(state, run_id)
        .await?
        .ok_or_else(|| ApiError::not_found("run not found"))
}

#[derive(Deserialize)]
struct RejectBody {
    #[serde(default)]
    reason: Option<String>,
}

// --------------------------------------------- project-scoped executions
//
// API Contract §12 names this resource "executions" (`POST
// /api/v1/projects/{project_id}/executions`); §15.1/§15.2 name evidence
// under the same project scope. BE-13 (§12.1-12.5) aligned the *response*
// field name (`run_id`/`status`, see `CreateExecutionResponse`) and added
// the cancel/tasks endpoints below, but deliberately left the *request*
// schema alone: full alignment to Contract §12's own request concept
// (`test_case_ids`, `test_suite_id`, `execution_mode`) presupposes real
// test-case/suite entities that do not exist in this backend at all (§8
// Test Generation and §10 Suites/Plans are MISSING/PARTIAL-without-
// persistence per the Integration Plan's own gap matrix) — accepting those
// fields without real validation behind them would be exactly the fake-ID
// behavior this unit is told not to build. See
// AIQA_PHASE2_BE13_EXECUTION_ALIGNMENT_IMPLEMENTATION_REPORT.md §3 for the
// full reasoning.
//
// `/approve`, `/reject`, and `/events` are not enumerated in Contract §12
// at all, but are real, necessary, pre-existing functionality (the
// governance gate and the realtime catch-up path) with nowhere else to go
// under the new scheme; they are mounted as sub-resources of the same
// `executions/{run_id}` the contract does define, which Contract §2.1
// permits ("non-breaking additions may remain within the current
// version").

/// BE-13 (§12.1) response alignment: the contract's own field name is
/// `status`, not this backend's internal `state` — renamed here for the
/// contract-defined scoped endpoint only. The *value* is still this run's
/// real initial `RunState` (`SCHEDULED` or `PENDING_APPROVAL`, whichever the
/// policy gate actually produced) — the contract's own illustrative example
/// (`"status": "QUEUED"`) is not reproduced literally, because `QUEUED` is
/// not a value this backend's run-level state machine ever produces (§13's
/// canonical status list is task-level, not run-level; see the BE-13
/// report). Fabricating it would misrepresent the run's actual state. The
/// legacy `POST /runs` response (`CreateRunResponse`, `state` field) is
/// left completely unchanged.
#[derive(Serialize)]
struct CreateExecutionResponse {
    run_id: Uuid,
    status: RunState,
    policy_notes: Option<String>,
}

/// POST /api/v1/projects/:project_id/executions — project_id is canonical
/// from the URL (Step 3: never trust a project id elsewhere when the route
/// already carries one) — the legacy `?project_id=`/key-scope resolution
/// this route's sibling `create_run` uses does not apply here at all.
async fn create_run_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(plan): ApiJson<RunPlan>,
) -> Result<(StatusCode, Json<CreateExecutionResponse>), ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let (status_code, Json(body)) =
        create_run_core(&state, &ctx, Some(project_id.to_string()), plan).await?;
    Ok((
        status_code,
        Json(CreateExecutionResponse {
            run_id: body.run_id,
            status: body.state,
            policy_notes: body.policy_notes,
        }),
    ))
}

/// GET /api/v1/projects/:project_id/executions — filtered to exactly the
/// URL's project, never the caller's own key scope (a global admin
/// listing project X's executions must see only X's, not everything).
async fn list_runs_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ListRunsQuery>,
) -> Result<Json<Vec<RunListItem>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let pid_str = project_id.to_string();
    let ids = state
        .store
        .list_run_ids(Some(&pid_str))
        .await
        .map_err(to_500)?;
    let limit = q.limit.unwrap_or(50).max(1) as usize;

    let mut items = Vec::new();
    for id in ids.into_iter().take(limit) {
        let summary = if let Some(record) = state.runs.read().await.get(&id).cloned() {
            record.summary().await
        } else {
            match state.store.get_run_summary(id).await.map_err(to_500)? {
                Some(s) => s,
                None => continue,
            }
        };
        items.push(RunListItem {
            run_id: summary.run_id,
            name: summary.name,
            state: summary.state,
            total: summary.total,
            passed: summary.passed,
            failed: summary.failed,
            project_id: summary.project_id,
        });
    }
    Ok(Json(items))
}

/// Fetches a run's durable summary (falling back from the in-memory record
/// to the DB, same as every legacy handler) and confirms it belongs to
/// `project_id` — the shared core every `_scoped` run-detail handler below
/// uses, so that check is written exactly once.
async fn scoped_run_summary(
    state: &AppState,
    run_id: Uuid,
    project_id: Uuid,
) -> Result<RunSummary, ApiError> {
    let summary = if let Some(record) = state.runs.read().await.get(&run_id).cloned() {
        record.summary().await
    } else {
        state
            .store
            .get_run_summary(run_id)
            .await
            .map_err(to_500)?
            .ok_or_else(|| ApiError::not_found("run not found"))?
    };
    require_resource_in_project(summary.project_id.as_deref(), project_id)?;
    Ok(summary)
}

/// GET /api/v1/projects/:project_id/executions/:run_id
/// Fetches a run's tasks (in-memory record first, falling back to the DB —
/// same pattern `scoped_run_summary` uses), sorted by task id. Shared by
/// `get_run_scoped` (embeds them in `RunDetail`) and BE-13's standalone
/// §12.4/§12.5 task endpoints below, so there is exactly one place that
/// knows how to fetch a run's tasks.
async fn scoped_run_tasks(state: &AppState, run_id: Uuid) -> Result<Vec<TaskState>, ApiError> {
    let mut tasks = if let Some(record) = state.runs.read().await.get(&run_id).cloned() {
        record.engine.read().await.tasks.values().cloned().collect()
    } else {
        state
            .store
            .list_tasks_for_run(run_id)
            .await
            .map_err(to_500)?
    };
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(tasks)
}

async fn get_run_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<RunDetail>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let summary = scoped_run_summary(&state, run_id, project_id).await?;
    let tasks = scoped_run_tasks(&state, run_id).await?;
    Ok(Json(RunDetail { summary, tasks }))
}

// ---------------------------------------------------- BE-13 — run tasks

/// GET /api/v1/projects/:project_id/executions/:run_id/tasks (§12.4)
async fn list_run_tasks_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<Vec<TaskState>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_run_summary(&state, run_id, project_id).await?;
    Ok(Json(scoped_run_tasks(&state, run_id).await?))
}

/// GET /api/v1/projects/:project_id/executions/:run_id/tasks/:task_id (§12.5)
async fn get_run_task_detail_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id, task_id)): ApiPath<(Uuid, Uuid, String)>,
) -> Result<Json<TaskState>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_run_summary(&state, run_id, project_id).await?;
    let tasks = scoped_run_tasks(&state, run_id).await?;
    tasks
        .into_iter()
        .find(|t| t.id == task_id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found("task not found in this run"))
}

// -------------------------------------------------------- BE-13 — cancel

#[derive(Serialize)]
struct CancelRunResponse {
    run_id: Uuid,
    /// The run's state at the instant cancellation was accepted — execution
    /// is asynchronous (Contract §12 "All execution is asynchronous"), so
    /// this is not guaranteed to already be `CANCELLED`: an in-flight task's
    /// subprocess needs a moment to actually be killed. Poll `GET
    /// .../executions/:run_id` (or the event stream) to observe the final
    /// state — this response only confirms the request was accepted, it
    /// does not claim execution has already stopped.
    status: RunState,
}

/// POST /api/v1/projects/:project_id/executions/:run_id/cancel (§12.3).
///
/// "Cancellation must be policy-aware and auditable" (contract's entire
/// spec for this endpoint — no request/response schema is given). Resolved
/// here as: policy-aware = the same authorization gate every other mutating
/// execution route already uses (Operator role + project ownership — there
/// is no separate "cancel policy" concept anywhere in `policy.rs`, and
/// inventing one would be exactly the kind of undefined-semantics guess
/// this unit is told not to make); auditable = the existing generic
/// `audit_log` mechanism already used for run.create/approve/reject.
///
/// Only `Created`/`Scheduled`/`Running` runs are cancellable —
/// `PendingApproval` already has its own dedicated stop action (`/reject`),
/// and every other state is already terminal. See `RunState::is_terminal`'s
/// doc comment for the full reasoning.
async fn cancel_run_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<CancelRunResponse>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    auth::require_role(&ctx, Role::Operator)?;
    let summary = scoped_run_summary(&state, run_id, project_id).await?;

    if summary.state == RunState::PendingApproval {
        return Err(ApiError::conflict(
            "run is PENDING_APPROVAL; use POST .../reject to stop a run that has not started executing",
        ));
    }
    if summary.state.is_terminal() {
        return Err(ApiError::conflict(format!(
            "run is already in a terminal state ({:?}), cannot be cancelled",
            summary.state
        )));
    }

    // A run whose persisted state is non-terminal but that is *not* held in
    // this process's memory would mean recovery (B2) has not yet run for it
    // — recovery forces every non-terminal run/task to a terminal state at
    // startup, so this should not happen. Fail safely rather than silently
    // no-op a cancel request that cannot actually reach anything.
    let record = state
        .runs
        .read()
        .await
        .get(&run_id)
        .cloned()
        .ok_or_else(|| {
            ApiError::internal(
                "run reports a live state but is not tracked in this process; cannot safely cancel",
            )
        })?;

    record.request_cancel(&ctx.name).await;
    audit(
        &state,
        &ctx,
        "run.cancel",
        Some(&run_id.to_string()),
        None,
        "ok",
    )
    .await;

    let current_state = *record.state.read().await;
    Ok(Json(CancelRunResponse {
        run_id,
        status: current_state,
    }))
}

// --------------------------------------------------------- retry-failed

#[derive(Serialize)]
struct RetryFailedTasksResponse {
    run_id: Uuid,
    state: RunState,
    retried_task_ids: Vec<String>,
}

/// POST /api/v1/projects/:project_id/executions/:run_id/retry-failed —
/// real re-planning v1: creates a new run containing only this run's real
/// INFRA_ERROR/TIMEOUT tasks. Never a real FAILED task — retrying a
/// genuine product failure would not fix it, and risks silently masking a
/// real defect behind a lucky second attempt; that distinction is the
/// exact one `domain.rs::TaskStatus`'s own doc comment already draws
/// ("Product failure and infrastructure failure are kept as distinct
/// classifications... per FRD §2 / SRD §2"), reused here rather than
/// inventing a second one.
///
/// Each retried task keeps its own real executor/params/simulate config
/// from the original plan (`store.rs::get_run_plan`); real dependency
/// edges are preserved only where BOTH ends are being retried — an edge to
/// a task that is not in the retry set is dropped, since that task's own
/// real prior result already stands and needs no re-run. Reuses
/// `create_run_core` verbatim: the retry run goes through the exact same
/// real policy-gate/scheduling/executor pipeline every other run already
/// does, no new dispatch machinery.
async fn retry_failed_tasks_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<RetryFailedTasksResponse>), ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    auth::require_role(&ctx, Role::Operator)?;
    let summary = scoped_run_summary(&state, run_id, project_id).await?;

    if !summary.state.is_terminal() {
        return Err(ApiError::conflict(
            "run has not reached a terminal state yet; retry-failed is only available once a run finishes",
        ));
    }

    let plan = state
        .store
        .get_run_plan(run_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("this run's original plan could not be found"))?;

    let tasks = scoped_run_tasks(&state, run_id).await?;
    let retry_ids: std::collections::HashSet<String> = tasks
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::InfraError | TaskStatus::Timeout))
        .map(|t| t.id.clone())
        .collect();

    if retry_ids.is_empty() {
        return Err(ApiError::validation(
            "no real infrastructure-classified failures (INFRA_ERROR/TIMEOUT) on this run to retry — a genuine FAILED test is never auto-retried",
        ));
    }

    let retry_tasks: Vec<crate::domain::TaskDef> = plan
        .tasks
        .into_iter()
        .filter(|t| retry_ids.contains(&t.id))
        .map(|mut t| {
            t.prerequisites.retain(|p| retry_ids.contains(p));
            t
        })
        .collect();

    let retry_plan = RunPlan {
        name: format!("Retry (infra failures): {}", summary.name),
        tasks: retry_tasks,
        concurrency: plan.concurrency,
    };

    let (status, created) =
        create_run_core(&state, &ctx, Some(project_id.to_string()), retry_plan).await?;
    let created = created.0;

    audit(
        &state,
        &ctx,
        "run.retry_failed",
        Some(&run_id.to_string()),
        Some(&format!(
            "new run {} retrying {} real infra-classified task(s)",
            created.run_id,
            retry_ids.len()
        )),
        "ok",
    )
    .await;

    Ok((
        status,
        Json(RetryFailedTasksResponse {
            run_id: created.run_id,
            state: created.state,
            retried_task_ids: retry_ids.into_iter().collect(),
        }),
    ))
}

// ------------------------------------------------------- dashboard / kpis

/// BE-12 (API Contract §23.1/§23.2). Only concepts this backend can
/// honestly compute from persisted state are included. §23's "Dashboard may
/// expose"/"Supported KPI concepts include" wording (reinforced by
/// Acceptance Matrix ACC-017's "KPI Coverage (Where implemented)") is a
/// permissive list, not a required schema — several listed concepts (report
/// counts, a computed project-health status, API latency, UI stability,
/// model/worker health, requirement/edge/business-logic/RBAC/security
/// coverage, test-generation accuracy) have no backing data model anywhere
/// in this codebase (no reports table, no test-generation/requirements
/// entities, no latency telemetry, no model gateway). They are
/// intentionally omitted rather than faked; see
/// AIQA_PHASE2_BE12_DASHBOARD_KPI_IMPLEMENTATION_REPORT.md for the full
/// list and reasoning.
#[derive(Serialize)]
struct DashboardResponse {
    project_id: Uuid,
    project_name: String,
    /// Run counts by state (§23.1 "execution counts").
    execution_counts: std::collections::BTreeMap<String, i64>,
    /// Task counts by status, across every run in this project.
    task_counts: std::collections::BTreeMap<String, i64>,
    /// §23.1 "pass/fail ratios" — PASSED vs FAILED task counts.
    pass_fail: PassFailCounts,
    /// §23.1 "test counts" — distinct known tests (`quality_nodes`,
    /// node_type='test') for this project.
    test_count: i64,
    generated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct PassFailCounts {
    passed: i64,
    failed: i64,
}

/// GET /api/v1/projects/:project_id/dashboard
async fn get_project_dashboard(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<DashboardResponse>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let pid = project_id.to_string();

    let project = state
        .store
        .get_project(project_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;

    let execution_counts: std::collections::BTreeMap<String, i64> = state
        .store
        .dashboard_run_counts(&pid)
        .await
        .map_err(to_500)?
        .into_iter()
        .collect();
    let task_counts: std::collections::BTreeMap<String, i64> = state
        .store
        .dashboard_task_counts(&pid)
        .await
        .map_err(to_500)?
        .into_iter()
        .collect();
    let passed = *task_counts.get("PASSED").unwrap_or(&0);
    let failed = *task_counts.get("FAILED").unwrap_or(&0);
    let test_count = state
        .store
        .dashboard_test_count(&pid)
        .await
        .map_err(to_500)?;

    Ok(Json(DashboardResponse {
        project_id,
        project_name: project.name,
        execution_counts,
        task_counts,
        pass_fail: PassFailCounts { passed, failed },
        test_count,
        generated_at: chrono::Utc::now(),
    }))
}

/// §23.2 KPI concepts this backend can honestly derive today: `execution
/// reliability` (product pass rate — PASSED / (PASSED+FAILED+TIMEOUT),
/// mirroring `intelligence.rs`'s own product-vs-infrastructure split, which
/// deliberately excludes INFRA_ERROR/BLOCKED/SKIPPED/CANCELLED as not being
/// evidence the thing under test is broken), `execution efficiency`
/// (average task duration), and (BE-12b) average *run* duration. Every
/// other listed KPI concept has no backing data model — see
/// `DashboardResponse`'s doc comment.
#[derive(Serialize)]
struct KpisResponse {
    project_id: Uuid,
    /// PASSED / (PASSED+FAILED+TIMEOUT) among this project's tasks, or
    /// `null` when no task has reached one of those outcomes yet.
    execution_reliability: Option<f64>,
    /// Average task duration in milliseconds, or `null` when no task has
    /// both a recorded start and finish yet.
    execution_efficiency_ms: Option<f64>,
    /// BE-12b — average *run* duration in milliseconds, over every
    /// terminal run (any final `RunState`) that has both `started_at` and
    /// `finished_at`, or `null` when no such run exists yet. See
    /// `store.rs::dashboard_avg_run_duration_ms`'s own doc comment for the
    /// exact inclusion rule.
    execution_avg_run_duration_ms: Option<f64>,
    generated_at: chrono::DateTime<chrono::Utc>,
}

/// GET /api/v1/projects/:project_id/kpis
async fn get_project_kpis(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<KpisResponse>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let pid = project_id.to_string();

    let task_counts: std::collections::BTreeMap<String, i64> = state
        .store
        .dashboard_task_counts(&pid)
        .await
        .map_err(to_500)?
        .into_iter()
        .collect();
    let passed = *task_counts.get("PASSED").unwrap_or(&0);
    let failed = *task_counts.get("FAILED").unwrap_or(&0);
    let timeout = *task_counts.get("TIMEOUT").unwrap_or(&0);
    let terminal_product_outcomes = passed + failed + timeout;
    let execution_reliability = if terminal_product_outcomes > 0 {
        Some(passed as f64 / terminal_product_outcomes as f64)
    } else {
        None
    };

    let execution_efficiency_ms = state
        .store
        .dashboard_avg_task_duration_ms(&pid)
        .await
        .map_err(to_500)?;
    let execution_avg_run_duration_ms = state
        .store
        .dashboard_avg_run_duration_ms(&pid)
        .await
        .map_err(to_500)?;

    Ok(Json(KpisResponse {
        project_id,
        execution_reliability,
        execution_efficiency_ms,
        execution_avg_run_duration_ms,
        generated_at: chrono::Utc::now(),
    }))
}

/// GET /api/v1/projects/:project_id/executions/:run_id/events?since=N
async fn get_events_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
    ApiQuery(q): ApiQuery<EventsQuery>,
) -> Result<Json<Vec<RunEvent>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_run_summary(&state, run_id, project_id).await?;
    let since = q.since.unwrap_or(0);

    if let Some(record) = state.runs.read().await.get(&run_id).cloned() {
        let events: Vec<RunEvent> = record
            .events
            .read()
            .await
            .iter()
            .filter(|e| e.sequence >= since)
            .cloned()
            .collect();
        return Ok(Json(events));
    }
    let events = state
        .store
        .list_events_since(run_id, since)
        .await
        .map_err(to_500)?;
    Ok(Json(events))
}

/// POST /api/v1/projects/:project_id/executions/:run_id/approve
async fn approve_run_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<RunSummary>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let record = find_or_rehydrate_run(&state, run_id).await?;
    require_resource_in_project(record.project_id.as_deref(), project_id)?;
    {
        let cur = *record.state.read().await;
        if cur != RunState::PendingApproval {
            return Err(ApiError::conflict(format!(
                "run is in state {cur:?}, not PENDING_APPROVAL"
            )));
        }
    }
    let concurrency = record.plan.concurrency;
    engine::approve_and_spawn(
        record.clone(),
        state.registry.clone(),
        concurrency,
        state.store.clone(),
        state.evidence.clone(),
        state.policy.clone(),
        &ctx.name,
        engine::default_task_timeout(),
    )
    .await
    .map_err(to_500)?;
    audit(
        &state,
        &ctx,
        "run.approve",
        Some(&run_id.to_string()),
        None,
        "ok",
    )
    .await;
    Ok(Json(record.summary().await))
}

/// POST /api/v1/projects/:project_id/executions/:run_id/reject
async fn reject_run_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
    body: Option<ApiJson<RejectBody>>,
) -> Result<Json<RunSummary>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let record = find_or_rehydrate_run(&state, run_id).await?;
    require_resource_in_project(record.project_id.as_deref(), project_id)?;
    {
        let cur = *record.state.read().await;
        if cur != RunState::PendingApproval {
            return Err(ApiError::conflict(format!(
                "run is in state {cur:?}, not PENDING_APPROVAL"
            )));
        }
    }
    let reason = body
        .and_then(|b| b.0.reason.clone())
        .unwrap_or_else(|| "no reason given".to_string());
    engine::reject_run(record.clone(), &state.store, &ctx.name, &reason)
        .await
        .map_err(to_500)?;
    audit(
        &state,
        &ctx,
        "run.reject",
        Some(&run_id.to_string()),
        Some(&reason),
        "ok",
    )
    .await;
    Ok(Json(record.summary().await))
}

/// GET /api/v1/projects/:project_id/executions/:run_id/evidence — API
/// Contract §15.1, exact path match.
async fn list_evidence_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<Vec<crate::store::EvidenceRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_run_summary(&state, run_id, project_id).await?;
    let rows = state
        .store
        .list_evidence_for_run(run_id)
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

/// GET /api/v1/projects/:project_id/evidence/:evidence_id — API Contract
/// §15.2 "Evidence Detail". BE-15: this used to serve the raw binary
/// content directly at this exact path (the same behavior as the legacy
/// `/evidence/:id/content`) — a genuine path match but not a shape match:
/// the contract names this "Detail" (metadata, like every other "X Detail"
/// endpoint in it, e.g. §8.4) and separately says "Large binary artifacts
/// SHOULD be accessed through secure artifact references rather than
/// embedded in normal API responses." The raw bytes are still fully
/// available — moved to the sibling `/content` path below, not removed —
/// and this path now returns the same `EvidenceRow` metadata shape §15.1's
/// list endpoint already uses per item.
async fn get_evidence_detail_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, evidence_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::store::EvidenceRow>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let (row, run_id) = state
        .store
        .get_evidence_by_id(evidence_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("evidence not found"))?;
    let run_id = run_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| ApiError::not_found("evidence not found"))?;
    scoped_run_summary(&state, run_id, project_id).await?;
    Ok(Json(row))
}

/// GET /api/v1/projects/:project_id/evidence/:evidence_id/content — the
/// raw binary artifact `get_evidence_detail_scoped` above points at (BE-15;
/// same body this route served before the split). The evidence's *owning
/// run's* project is what must match the URL, since evidence itself
/// carries no `project_id` of its own (same resolve-through-the-run
/// pattern the legacy `get_evidence_content` already uses).
async fn get_evidence_content_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, evidence_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let (object_key, content_type, run_id) = state
        .store
        .get_evidence_object_key(evidence_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("evidence not found"))?;
    let run_id = run_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| ApiError::not_found("evidence not found"))?;
    scoped_run_summary(&state, run_id, project_id).await?;

    let bytes = state
        .evidence
        .read(&object_key)
        .map_err(|e| ApiError::not_found(format!("evidence file unavailable: {e}")))?;
    Ok((
        StatusCode::OK,
        [(
            "Content-Type",
            content_type.unwrap_or_else(|| "text/plain".to_string()),
        )],
        bytes,
    )
        .into_response())
}

// ------------------------------------------------------------- findings

/// GET /api/v1/projects/:project_id/findings — API Contract §16.1.
/// "Finding" is this backend's real `bug_drafts` entity (title,
/// description, severity, confidence, `review_state` — including
/// `NEEDS_REVIEW`, the contract's own required distinction between an AI
/// hypothesis and a confirmed fact), reused verbatim rather than
/// reinvented under a new name; see the BE-15 report §2 for the full
/// reasoning.
async fn list_findings_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<Vec<crate::store::BugDraftRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let rows = state
        .store
        .list_bug_drafts(Some(project_id.to_string().as_str()))
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

/// A finding (`bug_drafts` row) carries no `project_id` of its own —
/// ownership resolves through its failure cluster, exactly like
/// `review_bug_draft`'s existing project check. Shared so every findings
/// route below checks it identically.
async fn require_finding_in_project(
    state: &AppState,
    finding_id: Uuid,
    project_id: Uuid,
) -> Result<(), ApiError> {
    let owner = state
        .store
        .bug_draft_project(finding_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("finding not found"))?;
    require_resource_in_project(Some(owner.as_str()), project_id)
}

/// GET /api/v1/projects/:project_id/findings/:finding_id — API Contract
/// §16.2.
async fn get_finding_detail_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, finding_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::store::BugDraftRow>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    require_finding_in_project(&state, finding_id, project_id).await?;
    let row = state
        .store
        .get_bug_draft_by_id(finding_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("finding not found"))?;
    Ok(Json(row))
}

/// POST /api/v1/projects/:project_id/findings/:finding_id/analysis — API
/// Contract §16.3. "Failure analysis may correlate: execution evidence,
/// environment information, historical failures, logs." This backend's
/// real, already-computed correlation for a finding is its underlying
/// failure cluster (root-cause hypothesis, signature, occurrence count,
/// reproducibility — all real, deterministic output of
/// `intelligence::classify_failure`, never an LLM call; see that module's
/// doc comment). No separate "environment"/"log" linkage exists anywhere
/// in this schema to correlate against, so none is fabricated — see the
/// BE-15 report for exactly what was and wasn't included and why. No
/// request body: the contract gives none, and there is nothing to
/// validate.
#[derive(Serialize)]
struct FindingAnalysisResponse {
    finding: crate::store::BugDraftRow,
    failure_cluster: crate::store::FailureClusterRow,
}

async fn get_finding_analysis_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, finding_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<FindingAnalysisResponse>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    require_finding_in_project(&state, finding_id, project_id).await?;
    let finding = state
        .store
        .get_bug_draft_by_id(finding_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("finding not found"))?;
    let failure_cluster = state
        .store
        .get_failure_cluster_by_id(finding.failure_cluster_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| {
            ApiError::internal("finding references a failure cluster that no longer exists")
        })?;
    audit(
        &state,
        &ctx,
        "finding.analysis",
        Some(&finding_id.to_string()),
        None,
        "ok",
    )
    .await;
    Ok(Json(FindingAnalysisResponse {
        finding,
        failure_cluster,
    }))
}

/// POST /api/v1/projects/:project_id/findings/:finding_id/ai-hypothesis —
/// build order item 4 (22 August 2026). Not a Contract §16 endpoint (same
/// footing as `.../review` above) — real, on-demand AI-assisted root-cause
/// analysis, layered on top of the real, already-computed failure cluster
/// `.../analysis` above already returns. Reuses the exact `generation_method`/
/// `provider`/`model`/`fallback_reason` response shape
/// `generate_ai_test_cases_scoped` established for `AiTask::TestCaseGeneration`
/// — a `"fallback"` outcome is a normal 200 here too, per ACC-018's own
/// Failure Behavior criteria ("controlled fallback, or NEEDS_REVIEW — never
/// fabricated output"): the endpoint did its real job (attempted the AI call
/// and honestly reports what happened), and the failure cluster's real,
/// pre-existing deterministic data is untouched and still returned. Gated on
/// `Role::Operator`, same bar as `.../review` and `generate-ai` — this
/// triggers a real, potentially slow model call, not a cheap read.
#[derive(Serialize)]
struct FindingAiHypothesisResponse {
    finding: crate::store::BugDraftRow,
    failure_cluster: crate::store::FailureClusterRow,
    generation_method: String,
    provider: Option<String>,
    model: Option<String>,
    fallback_reason: Option<String>,
}

async fn generate_finding_ai_hypothesis_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, finding_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<FindingAiHypothesisResponse>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    auth::require_role(&ctx, Role::Operator)?;
    require_finding_in_project(&state, finding_id, project_id).await?;

    let finding = state
        .store
        .get_bug_draft_by_id(finding_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("finding not found"))?;
    let failure_cluster = state
        .store
        .get_failure_cluster_by_id(finding.failure_cluster_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| {
            ApiError::internal("finding references a failure cluster that no longer exists")
        })?;

    let rule_based_hypothesis = failure_cluster
        .root_cause_hypothesis
        .clone()
        .unwrap_or_default();
    let outcome = crate::intelligence::generate_ai_hypothesis(
        &state.ai_infra,
        &failure_cluster.signature,
        &rule_based_hypothesis,
        failure_cluster.occurrence_count,
    )
    .await;

    let response = match outcome {
        crate::intelligence::AiHypothesisOutcome::Ai {
            hypothesis,
            provider,
            model,
        } => {
            state
                .store
                .set_ai_hypothesis(failure_cluster.id, &hypothesis, &provider, &model)
                .await
                .map_err(to_500)?;
            let refreshed = state
                .store
                .get_failure_cluster_by_id(failure_cluster.id)
                .await
                .map_err(to_500)?
                .ok_or_else(|| {
                    ApiError::internal("failure cluster vanished immediately after being updated")
                })?;
            audit(
                &state,
                &ctx,
                "finding.ai_hypothesis",
                Some(&finding_id.to_string()),
                Some(&format!("via {provider}/{model}")),
                "ok",
            )
            .await;
            FindingAiHypothesisResponse {
                finding,
                failure_cluster: refreshed,
                generation_method: "ai".to_string(),
                provider: Some(provider),
                model: Some(model),
                fallback_reason: None,
            }
        }
        crate::intelligence::AiHypothesisOutcome::Failed { reason } => {
            audit(
                &state,
                &ctx,
                "finding.ai_hypothesis",
                Some(&finding_id.to_string()),
                Some(&reason),
                "fallback",
            )
            .await;
            FindingAiHypothesisResponse {
                finding,
                failure_cluster,
                generation_method: "fallback".to_string(),
                provider: None,
                model: None,
                fallback_reason: Some(reason),
            }
        }
    };

    Ok(Json(response))
}

/// POST /api/v1/projects/:project_id/findings/:finding_id/review — not a
/// Contract §16 endpoint (the contract defines no review/promotion
/// action), but real, necessary, pre-existing functionality
/// (`review_bug_draft`/legacy `POST /bug-drafts/:id/review`) with nowhere
/// else to go under the new scheme; mounted as a sub-resource of the
/// contract-defined `findings/{finding_id}` the same way Actual BE-9
/// mounted `/approve`/`/reject`/`/events` under `executions/{run_id}`
/// (Contract §2.1: "non-breaking additions may remain within the current
/// version"). Reuses `review_bug_draft`'s exact logic, not a second copy.
async fn review_finding_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, finding_id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(body): ApiJson<ReviewBody>,
) -> Result<StatusCode, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    auth::require_role(&ctx, Role::Operator)?;
    require_finding_in_project(&state, finding_id, project_id).await?;
    let new_state = match body.action.as_str() {
        "confirm" => "CONFIRMED",
        "dismiss" => "DISMISSED",
        other => {
            return Err(ApiError::validation(format!(
                "unknown action '{other}', expected 'confirm' or 'dismiss'"
            )))
        }
    };
    state
        .store
        .review_bug_draft(finding_id, new_state, &ctx.name)
        .await
        .map_err(to_500)?;
    audit(
        &state,
        &ctx,
        "finding.review",
        Some(&finding_id.to_string()),
        Some(new_state),
        "ok",
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------ audit (§25)

/// Real pagination envelope for the project-scoped audit list (added
/// alongside real `offset` support — `AuditPage.tsx`'s own pre-existing doc
/// comment on this file's frontend counterpart called the previous
/// limit-only story "not an invented multi-page contract", i.e. honestly
/// not real pagination at all). `total` is a real `COUNT(*)`
/// (`store.rs::count_audit`), not derived from this response's own `rows`
/// length, so a caller can show a real "X-Y of Z" and correctly disable
/// "next" exactly at the real end. The legacy global `/admin/audit-log`
/// route (`get_audit_log` above) deliberately keeps its original bare-array
/// shape — out of scope here, and still a real consumer, `apps/web`'s own
/// reference `AuditLogView.tsx`, elsewhere in this repo (unmaintained,
/// confirmed not part of this repo's CI, but a bare-array response is still
/// the honest choice not to break it on faith alone).
#[derive(Serialize)]
struct AuditLogResponse {
    rows: Vec<crate::store::AuditRow>,
    total: i64,
}

/// GET /api/v1/projects/:project_id/audit — API Contract §25.1. Unlike the
/// legacy `/admin/audit-log` (global, `Role::Admin`-only), this is a
/// project's own audit trail — visibility follows the same bar every other
/// scoped resource in this project uses (`Role::Operator` + project
/// access), not the global-admin bar, since seeing your own project's
/// history is not a cross-tenant action. The contract specifies no
/// per-endpoint role, so this is a resolved (documented, not blocking)
/// scope decision — see the BE-15 report.
async fn get_audit_log_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<AuditQuery>,
) -> Result<Json<AuditLogResponse>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    auth::require_role(&ctx, Role::Operator)?;
    let pid = project_id.to_string();
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let offset = q.offset.unwrap_or(0).max(0);
    let rows = state
        .store
        .list_audit(Some(pid.as_str()), limit, offset)
        .await
        .map_err(to_500)?;
    let total = state
        .store
        .count_audit(Some(pid.as_str()))
        .await
        .map_err(to_500)?;
    Ok(Json(AuditLogResponse { rows, total }))
}

// ------------------------------------------- test plans / test suites (Phase 5)
//
// Output maturity — a real plan -> suite -> case hierarchy above the flat
// `test_cases` table. Every handler below follows the exact same
// require_scoped_project -> require_role -> fetch+ownership-check ->
// mutate -> audit shape the test-case handlers above already established;
// no new authorization pattern was invented for this section.

async fn scoped_test_plan(
    state: &AppState,
    ctx: &AuthContext,
    project_id: Uuid,
    id: Uuid,
) -> Result<crate::store::TestPlanRow, ApiError> {
    let row = state
        .store
        .get_test_plan(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("test plan not found"))?;
    if row.project_id != project_id.to_string() {
        return Err(ApiError::not_found("test plan not found"));
    }
    auth::require_project_access(ctx, Some(row.project_id.as_str()))?;
    Ok(row)
}

async fn scoped_test_suite(
    state: &AppState,
    ctx: &AuthContext,
    project_id: Uuid,
    id: Uuid,
) -> Result<crate::store::TestSuiteRow, ApiError> {
    let row = state
        .store
        .get_test_suite(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("test suite not found"))?;
    if row.project_id != project_id.to_string() {
        return Err(ApiError::not_found("test suite not found"));
    }
    auth::require_project_access(ctx, Some(row.project_id.as_str()))?;
    Ok(row)
}

fn validate_test_plan_status(status: &Option<String>) -> Result<(), ApiError> {
    if let Some(s) = status {
        if !matches!(s.as_str(), "DRAFT" | "ACTIVE" | "ARCHIVED") {
            return Err(ApiError::validation(
                "status must be one of DRAFT, ACTIVE, ARCHIVED",
            ));
        }
    }
    Ok(())
}

async fn create_test_plan_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<CreateTestPlanRequest>,
) -> Result<(StatusCode, Json<crate::store::TestPlanRow>), ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    if body.name.trim().is_empty() {
        return Err(ApiError::validation("name must not be empty"));
    }

    let id = state
        .store
        .insert_test_plan(&project_id.to_string(), &body, Some(&ctx.name))
        .await
        .map_err(to_500)?;
    audit(
        &state,
        &ctx,
        "test_plan.create",
        Some(&id.to_string()),
        Some(&body.name),
        "ok",
    )
    .await;
    let row = state
        .store
        .get_test_plan(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| to_500("test plan vanished immediately after insert"))?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn list_test_plans_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<Vec<crate::store::TestPlanRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let rows = state
        .store
        .list_test_plans(&project_id.to_string())
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

async fn get_test_plan_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::store::TestPlanRow>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_test_plan(&state, &ctx, project_id, id).await?;
    Ok(Json(row))
}

async fn update_test_plan_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(body): ApiJson<UpdateTestPlanRequest>,
) -> Result<Json<crate::store::TestPlanRow>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_test_plan(&state, &ctx, project_id, id).await?;
    if let Some(name) = &body.name {
        if name.trim().is_empty() {
            return Err(ApiError::validation("name must not be empty"));
        }
    }
    validate_test_plan_status(&body.status)?;

    let updated = state
        .store
        .update_test_plan(
            id,
            body.name.as_deref(),
            body.description.as_deref(),
            body.status.as_deref(),
            body.entry_criteria.as_deref(),
            body.exit_criteria.as_deref(),
        )
        .await
        .map_err(to_500)?;
    if !updated {
        return Err(ApiError::not_found("test plan not found"));
    }
    audit(
        &state,
        &ctx,
        "test_plan.update",
        Some(&id.to_string()),
        None,
        "ok",
    )
    .await;
    let row = state
        .store
        .get_test_plan(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| to_500("test plan vanished immediately after update"))?;
    Ok(Json(row))
}

async fn delete_test_plan_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_test_plan(&state, &ctx, project_id, id).await?;
    let deleted = state.store.delete_test_plan(id).await.map_err(to_500)?;
    if !deleted {
        return Err(ApiError::not_found("test plan not found"));
    }
    audit(
        &state,
        &ctx,
        "test_plan.delete",
        Some(&id.to_string()),
        None,
        "ok",
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_test_suite_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<CreateTestSuiteRequest>,
) -> Result<(StatusCode, Json<crate::store::TestSuiteRow>), ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    if body.name.trim().is_empty() {
        return Err(ApiError::validation("name must not be empty"));
    }
    if let Some(plan_id) = body.plan_id {
        scoped_test_plan(&state, &ctx, project_id, plan_id).await?;
    }

    let id = state
        .store
        .insert_test_suite(&project_id.to_string(), &body, Some(&ctx.name))
        .await
        .map_err(to_500)?;
    audit(
        &state,
        &ctx,
        "test_suite.create",
        Some(&id.to_string()),
        Some(&body.name),
        "ok",
    )
    .await;
    let row = state
        .store
        .get_test_suite(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| to_500("test suite vanished immediately after insert"))?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Deserialize)]
struct ListTestSuitesQuery {
    #[serde(default)]
    plan_id: Option<Uuid>,
}

/// GET /api/v1/projects/:project_id/test-suites — optionally filtered to
/// one plan (`?plan_id=<uuid>`).
async fn list_test_suites_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ListTestSuitesQuery>,
) -> Result<Json<Vec<crate::store::TestSuiteRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let rows = state
        .store
        .list_test_suites(&project_id.to_string(), q.plan_id)
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

async fn get_test_suite_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::store::TestSuiteRow>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_test_suite(&state, &ctx, project_id, id).await?;
    Ok(Json(row))
}

async fn update_test_suite_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(body): ApiJson<UpdateTestSuiteRequest>,
) -> Result<Json<crate::store::TestSuiteRow>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_test_suite(&state, &ctx, project_id, id).await?;
    if let Some(name) = &body.name {
        if name.trim().is_empty() {
            return Err(ApiError::validation("name must not be empty"));
        }
    }

    let updated = state
        .store
        .update_test_suite(id, body.name.as_deref(), body.description.as_deref())
        .await
        .map_err(to_500)?;
    if !updated {
        return Err(ApiError::not_found("test suite not found"));
    }
    audit(
        &state,
        &ctx,
        "test_suite.update",
        Some(&id.to_string()),
        None,
        "ok",
    )
    .await;
    let row = state
        .store
        .get_test_suite(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| to_500("test suite vanished immediately after update"))?;
    Ok(Json(row))
}

/// PUT .../test-suites/:id/plan — see `SetTestCaseSuiteRequest`'s own doc
/// comment (`domain.rs`) for why this is a dedicated endpoint; `plan_id`
/// belonging to a different project is rejected the same way
/// `set_test_case_suite_scoped` rejects a cross-project suite.
async fn set_test_suite_plan_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(body): ApiJson<SetTestSuitePlanRequest>,
) -> Result<Json<crate::store::TestSuiteRow>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_test_suite(&state, &ctx, project_id, id).await?;
    if let Some(plan_id) = body.plan_id {
        scoped_test_plan(&state, &ctx, project_id, plan_id).await?;
    }
    let updated = state
        .store
        .set_test_suite_plan(id, body.plan_id)
        .await
        .map_err(to_500)?;
    if !updated {
        return Err(ApiError::not_found("test suite not found"));
    }
    audit(
        &state,
        &ctx,
        "test_suite.set_plan",
        Some(&id.to_string()),
        body.plan_id.map(|p| p.to_string()).as_deref(),
        "ok",
    )
    .await;
    let row = state
        .store
        .get_test_suite(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| to_500("test suite vanished immediately after update"))?;
    Ok(Json(row))
}

async fn delete_test_suite_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_test_suite(&state, &ctx, project_id, id).await?;
    let deleted = state.store.delete_test_suite(id).await.map_err(to_500)?;
    if !deleted {
        return Err(ApiError::not_found("test suite not found"));
    }
    audit(
        &state,
        &ctx,
        "test_suite.delete",
        Some(&id.to_string()),
        None,
        "ok",
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------- crawler (Phase 6)
//
// Real crawler + per-role visibility matrix. See `crawler.rs`'s own module
// doc comment for exactly what this covers and what it deliberately does
// not (targeted deep crawl, doc-vs-reality gap check).

#[derive(Debug, Deserialize)]
struct CrawlRequestBody {
    base_url: String,
    roles: Vec<crate::crawler::CrawlRole>,
}

#[derive(Debug, Serialize)]
struct CrawlStartedResponse {
    run_id: Uuid,
}

/// POST /api/v1/projects/:project_id/crawl — a real crawl, materialized
/// into a Playwright spec and dispatched through the exact same run/task
/// pipeline every other browser task already uses (`create_run_core` ->
/// `engine::spawn_run` -> the existing `PlaywrightExecutor`) — no new
/// subprocess machinery. The caller polls the returned `run_id` through
/// the existing `GET .../executions/:run_id` the same way any other run is
/// polled, then fetches the real result via
/// `GET .../crawl/:run_id/result` below once it completes.
async fn start_crawl_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<CrawlRequestBody>,
) -> Result<(StatusCode, Json<CrawlStartedResponse>), ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;

    let plan = crate::crawler::CrawlPlan {
        base_url: body.base_url,
        roles: body.roles,
    };
    crate::crawler::validate_crawl_plan(&plan)
        .await
        .map_err(|reasons| ApiError::validation(reasons.join("; ")))?;

    // The task's own id doubles as the real key `crawler::read_crawl_result`
    // looks the eventual result up by — no separate id needed.
    let task_id = Uuid::new_v4().to_string();
    let spec_path =
        crate::crawler::materialize_crawl_spec(&task_id, &plan).map_err(ApiError::validation)?;

    let task = TaskDef {
        id: task_id,
        name: format!("Crawl: {}", plan.base_url),
        prerequisites: Vec::new(),
        simulate: None,
        executor: Some("playwright".to_string()),
        params: serde_json::json!({ "spec": spec_path }),
    };
    let run_plan = RunPlan {
        name: format!("Crawl: {}", plan.base_url),
        tasks: vec![task],
        concurrency: 1,
    };
    let (status_code, Json(created)) =
        create_run_core(&state, &ctx, Some(project_id.to_string()), run_plan).await?;

    audit(
        &state,
        &ctx,
        "crawl.start",
        Some(&created.run_id.to_string()),
        Some(&plan.base_url),
        "ok",
    )
    .await;

    Ok((
        status_code,
        Json(CrawlStartedResponse {
            run_id: created.run_id,
        }),
    ))
}

/// GET /api/v1/projects/:project_id/crawl/:run_id/result — the real,
/// structured output (`crawler::CrawlOutput`) a completed crawl run wrote
/// to disk, keyed by that run's own (single) task id. Reuses the exact
/// same real ownership check (`scoped_run_summary`) and task-fetching
/// (`scoped_run_tasks`) every other run-scoped endpoint already uses —
/// this result is never reachable just by knowing a run id, the same real
/// cross-project guarantee every other resource in this file already
/// holds. A crawl that has not finished yet (or never produced a result)
/// is a real, honest 404 — never a fabricated empty result.
async fn get_crawl_result_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::crawler::CrawlOutput>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_run_summary(&state, run_id, project_id).await?;
    let tasks = scoped_run_tasks(&state, run_id).await?;
    let task = tasks
        .first()
        .ok_or_else(|| ApiError::not_found("crawl run has no tasks"))?;
    let output = crate::crawler::read_crawl_result(&task.id).map_err(|_| {
        ApiError::not_found(
            "crawl result not found — the crawl may still be running or may not have completed",
        )
    })?;
    Ok(Json(output))
}

// ----------------------------------------------------------- intelligence

/// GET /api/v1/projects/:project_id/intelligence/tests — Phase 4 BE-10
/// (distinct from the pre-existing Phase 2 "BE-10" in `errors.rs`/
/// `api_tests.rs`, which is about extractor-rejection error envelopes and
/// is unrelated to and untouched by this route). Phase B cleanup: the
/// legacy unscoped `/intelligence/tests` route this one used to coexist
/// with is removed — it scoped itself to the caller's own key-bound
/// `ctx.project_id` (no URL parameter), which was silently wrong (or
/// empty, via the `"default"` tenant fallback in
/// `store.rs::list_test_intelligence`) for an unscoped/multi-project key
/// browsing a specific project, exactly the key pattern this frontend's
/// project switcher relies on (see FE-21's report §3/§9). This route does
/// it the same way every other resource under
/// `/api/v1/projects/:project_id/...` already does: the project is
/// explicit in the URL, `require_scoped_project` confirms both that it
/// exists and that the caller's key may access it, and the query is always
/// given that real project id — never the caller's own (possibly
/// different, possibly absent) bound project, never the `"default"`
/// tenant. No new intelligence computation, no new table, no new response
/// shape — `store.rs::list_test_intelligence`/`TestIntelRow` are reused
/// exactly as they already exist.
async fn list_test_intelligence_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<Vec<crate::store::TestIntelRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let rows = state
        .store
        .list_test_intelligence(Some(project_id.to_string().as_str()))
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct RequirementImpactQuery {
    key: String,
}

/// GET /api/v1/projects/:project_id/requirement-impact?key=<requirement_key>
/// — Phase 4 (requirement graph). Real impact analysis over the real
/// `quality_relations` edges `intelligence::Intelligence::
/// link_test_case_to_requirement` writes at test-case creation time (see
/// that function's own doc comment): every real, persisted test case that
/// was actually created citing this exact requirement — by its real
/// `requirement_reference` string if that's what `key` is, or by
/// `"text:<sha256>"` of a real `source_requirement` excerpt otherwise (see
/// `intelligence::requirement_identity` for how a caller derives the right
/// `key` to ask for). An unknown/never-linked key returns a real, honest
/// empty list — never an error, and never a fabricated guess at what might
/// be affected.
async fn get_requirement_impact_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<RequirementImpactQuery>,
) -> Result<Json<Vec<crate::store::TestCaseRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    if q.key.trim().is_empty() {
        return Err(ApiError::validation("key must not be empty"));
    }
    let rows = state
        .store
        .list_test_cases_for_requirement(&project_id.to_string(), q.key.trim())
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct ReviewBody {
    action: String, // "confirm" | "dismiss"
}

// ------------------------------------------------------------- governance admin

#[derive(Deserialize)]
struct CreateApiKeyBody {
    name: String,
    role: String, // "admin" | "operator" | "viewer"
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Serialize)]
struct CreateApiKeyResponse {
    id: Uuid,
    key: String,
}

/// POST /admin/api-keys — Admin-only. The plaintext key is returned exactly
/// once, at creation time (same convention as GitHub/Stripe tokens); only
/// its hash is ever persisted (`auth::hash_key`).
async fn create_api_key(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiJson(body): ApiJson<CreateApiKeyBody>,
) -> Result<Json<CreateApiKeyResponse>, ApiError> {
    auth::require_role(&ctx, Role::Admin)?;
    let role = match body.role.as_str() {
        "admin" | "operator" | "viewer" => body.role.as_str(),
        other => return Err(ApiError::validation(format!("unknown role '{other}'"))),
    };
    let id = Uuid::new_v4();
    let raw = auth::generate_key();
    state
        .store
        .insert_api_key(
            id,
            &body.name,
            &auth::hash_key(&raw),
            role,
            body.project_id.as_deref(),
        )
        .await
        .map_err(to_500)?;
    audit(
        &state,
        &ctx,
        "apikey.create",
        Some(&id.to_string()),
        Some(&format!("name={} role={role}", body.name)),
        "ok",
    )
    .await;
    Ok(Json(CreateApiKeyResponse { id, key: raw }))
}

async fn list_api_keys_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<Json<Vec<crate::store::ApiKeyListRow>>, ApiError> {
    auth::require_role(&ctx, Role::Admin)?;
    let rows = state.store.list_api_keys().await.map_err(to_500)?;
    Ok(Json(rows))
}

async fn revoke_api_key_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    auth::require_role(&ctx, Role::Admin)?;
    state.store.revoke_api_key(id).await.map_err(to_500)?;
    audit(
        &state,
        &ctx,
        "apikey.revoke",
        Some(&id.to_string()),
        None,
        "ok",
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct AuditQuery {
    limit: Option<i64>,
    /// Real pagination (previously `limit` alone was this sprint's entire
    /// pagination story — see `AuditLogResponse`'s own doc comment). `< 0`
    /// is clamped to 0, same defensive floor `limit`'s own `.clamp` already
    /// applies to its side.
    offset: Option<i64>,
}

async fn get_audit_log(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiQuery(q): ApiQuery<AuditQuery>,
) -> Result<Json<Vec<crate::store::AuditRow>>, ApiError> {
    auth::require_role(&ctx, Role::Admin)?;
    let rows = state
        .store
        .list_audit(
            None,
            q.limit.unwrap_or(100).clamp(1, 1000),
            q.offset.unwrap_or(0).max(0),
        )
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

// ----------------------------------------------------------- observability

async fn get_metrics(
    State(state): State<AppState>,
    Extension(_ctx): Extension<AuthContext>,
) -> Response {
    let body = crate::metrics::render(&state.store.pool).await;
    (
        StatusCode::OK,
        [("Content-Type", "text/plain; version=0.0.4")],
        body,
    )
        .into_response()
}

// --------------------------------------------------------------------- ws

#[derive(Deserialize)]
struct WsAuthQuery {
    key: Option<String>,
}

/// BE-14 — API Contract §14. The project-scoped realtime stream. Phase B
/// cleanup removed the former per-run `/runs/:id/ws` socket this one used
/// to be documented as a counterpart of; this is now the only WebSocket
/// endpoint. Auth uses a `?key=` query parameter instead of the
/// `Authorization` header the rest of the API uses, because browsers
/// cannot set custom headers on a WebSocket handshake (`new
/// WebSocket(url)` takes no headers argument) — this is the standard
/// workaround, same as e.g. many streaming APIs' `?access_token=` (and
/// G-WS in the Integration Plan is the deliberately-not-resolved-here
/// question of whether that technique should eventually change). Project
/// access reuses the same model every REST scoped route already uses
/// (`auth::require_project_access`) — the project id in the URL does not
/// itself grant access.
async fn project_events_ws_handler(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(q): Query<WsAuthQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let ctx = match q.key.as_deref() {
        Some(key) => match state.store.find_api_key_by_hash(&auth::hash_key(key)).await {
            Ok(Some(row)) => AuthContext {
                key_id: row.id,
                name: row.name,
                role: Role::parse(&row.role),
                project_id: row.project_id,
            },
            _ => return (StatusCode::UNAUTHORIZED, "invalid or revoked API key").into_response(),
        },
        None => return (StatusCode::UNAUTHORIZED, "missing ?key= query parameter").into_response(),
    };

    match state.store.get_project(project_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return (StatusCode::NOT_FOUND, "project not found").into_response(),
        Err(e) => {
            tracing::error!("project_events_ws_handler: get_project failed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    }
    if auth::require_project_access(&ctx, Some(project_id.to_string().as_str())).is_err() {
        return (
            StatusCode::FORBIDDEN,
            "resource belongs to a different project",
        )
            .into_response();
    }

    // No backlog replay here: unlike a single run's small, bounded event
    // list, "every event from every run this project has ever had" is not
    // something to hold in memory or dump on connect. Contract §14/ACC-008
    // catch-up is `GET .../executions/:run_id/events?since=`, per-run, and
    // continues to work unchanged (Step 7's honest ordering/delivery
    // semantics: this stream delivers events occurring *after* a
    // successful subscription, nothing before it).
    let stream = state.project_stream(&project_id.to_string()).await;
    let rx = stream.tx.subscribe();
    ws.on_upgrade(move |socket| drive_socket(socket, rx))
}

/// Shared consumption loop for the project-scoped socket: forward broadcast
/// events to the client, watch for the client
/// closing, and stop cleanly either way so no subscriber/task/channel is
/// leaked (dropping `rx` on return unsubscribes it from the `broadcast`
/// channel automatically).
async fn drive_socket(mut socket: WebSocket, mut rx: broadcast::Receiver<RunEvent>) {
    loop {
        tokio::select! {
            ev = rx.recv() => {
                match ev {
                    Ok(ev) => {
                        if socket.send(Message::Text(serde_json::to_string(&ev).unwrap())).await.is_err() {
                            break;
                        }
                    }
                    // BE-14 (Integration Plan §11.2 item 5 / audit M5) — a
                    // slow consumer previously `continue`d silently here,
                    // meaning missed events left no trace at all: a client
                    // trusting a gap-free stream would be silently wrong.
                    // Closing with a distinct code makes the gap visible
                    // and tells the client to reconnect and re-catch-up via
                    // the REST endpoint, rather than inventing an in-band
                    // "resync marker" event that never came from real
                    // execution state.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        let _ = socket
                            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                                code: 4001,
                                reason: format!("lagged behind by {skipped} events; reconnect and catch up via REST").into(),
                            })))
                            .await;
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

// ------------------------------------------------------------- orchestrator logs
//
// Real-time, centralized structured log history and WebSocket streaming for
// dashboard live activity. Supports global and project-scoped logs with bounded
// memory and client auto-reconnection.

#[derive(Debug, Deserialize)]
pub struct ListLogsQuery {
    pub limit: Option<usize>,
    pub project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LogsWsQuery {
    pub key: Option<String>,
    pub project_id: Option<String>,
}

/// GET /api/v1/logs — List recent orchestrator log events (authenticated, role >= Viewer).
async fn list_logs_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiQuery(q): ApiQuery<ListLogsQuery>,
) -> Result<Json<Vec<OrchestratorLogEntry>>, ApiError> {
    auth::require_role(&ctx, Role::Viewer)?;

    let effective_project = match (&ctx.project_id, &q.project_id) {
        (Some(scoped), Some(requested)) => {
            if scoped != requested {
                return Err(ApiError::forbidden(
                    "resource belongs to a different project",
                ));
            }
            Some(scoped.as_str())
        }
        (Some(scoped), None) => Some(scoped.as_str()),
        (None, Some(requested)) => Some(requested.as_str()),
        (None, None) => None,
    };

    let limit = q.limit.unwrap_or(100).min(300);
    let logs = state.log_bus.recent_logs(effective_project, limit).await;
    Ok(Json(logs))
}

/// GET /api/v1/projects/:project_id/logs — List recent orchestrator log events for a project.
async fn list_project_logs_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ListLogsQuery>,
) -> Result<Json<Vec<OrchestratorLogEntry>>, ApiError> {
    auth::require_role(&ctx, Role::Viewer)?;
    auth::require_project_access(&ctx, Some(project_id.to_string().as_str()))?;

    let limit = q.limit.unwrap_or(100).min(300);
    let logs = state
        .log_bus
        .recent_logs(Some(&project_id.to_string()), limit)
        .await;
    Ok(Json(logs))
}

/// GET /api/v1/logs/ws?key=...[&project_id=...] — Real-time WebSocket log stream.
async fn logs_ws_handler(
    State(state): State<AppState>,
    Query(q): Query<LogsWsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let ctx = match q.key.as_deref() {
        Some(key) => match state.store.find_api_key_by_hash(&auth::hash_key(key)).await {
            Ok(Some(row)) => AuthContext {
                key_id: row.id,
                name: row.name,
                role: Role::parse(&row.role),
                project_id: row.project_id,
            },
            _ => return (StatusCode::UNAUTHORIZED, "invalid or revoked API key").into_response(),
        },
        None => return (StatusCode::UNAUTHORIZED, "missing ?key= query parameter").into_response(),
    };

    let effective_project = match (&ctx.project_id, &q.project_id) {
        (Some(scoped), Some(requested)) => {
            if scoped != requested {
                return (
                    StatusCode::FORBIDDEN,
                    "resource belongs to a different project",
                )
                    .into_response();
            }
            Some(scoped.clone())
        }
        (Some(scoped), None) => Some(scoped.clone()),
        (None, Some(requested)) => Some(requested.clone()),
        (None, None) => None,
    };

    let rx = state.log_bus.subscribe();
    ws.on_upgrade(move |socket| drive_log_socket(socket, rx, effective_project))
}

/// GET /api/v1/projects/:project_id/logs/ws?key=... — Real-time project-scoped WebSocket log stream.
async fn project_logs_ws_handler(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(q): Query<WsAuthQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let ctx = match q.key.as_deref() {
        Some(key) => match state.store.find_api_key_by_hash(&auth::hash_key(key)).await {
            Ok(Some(row)) => AuthContext {
                key_id: row.id,
                name: row.name,
                role: Role::parse(&row.role),
                project_id: row.project_id,
            },
            _ => return (StatusCode::UNAUTHORIZED, "invalid or revoked API key").into_response(),
        },
        None => return (StatusCode::UNAUTHORIZED, "missing ?key= query parameter").into_response(),
    };

    if auth::require_project_access(&ctx, Some(project_id.to_string().as_str())).is_err() {
        return (
            StatusCode::FORBIDDEN,
            "resource belongs to a different project",
        )
            .into_response();
    }

    let rx = state.log_bus.subscribe();
    let pid_str = project_id.to_string();
    ws.on_upgrade(move |socket| drive_log_socket(socket, rx, Some(pid_str)))
}

/// Consumption loop for the real-time orchestrator log WebSocket stream.
async fn drive_log_socket(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<OrchestratorLogEntry>,
    filter_project_id: Option<String>,
) {
    loop {
        tokio::select! {
            entry = rx.recv() => {
                match entry {
                    Ok(log) => {
                        let should_send = match (&filter_project_id, &log.project_id) {
                            (Some(target_pid), Some(log_pid)) => target_pid == log_pid,
                            (Some(_), None) => true, // Global logs are visible
                            (None, _) => true,
                        };
                        if should_send {
                            if let Ok(json) = serde_json::to_string(&log) {
                                if socket.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

// ------------------------------------------------ Phase 5: Governed Self-Healing

#[derive(Debug, serde::Deserialize)]
pub struct HealingListQuery {
    pub state: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct HealingRejectRequest {
    pub reason: String,
}

fn healing_err_to_api_err(e: crate::governed_healing::GovernedHealingError) -> ApiError {
    match e {
        crate::governed_healing::GovernedHealingError::ValidationError(errs) => {
            ApiError::validation(format!("validation failed: {errs:?}"))
        }
        crate::governed_healing::GovernedHealingError::SecurityViolation(msg) => {
            ApiError::validation(format!("security violation: {msg}"))
        }
        crate::governed_healing::GovernedHealingError::Unauthorized(msg) => {
            ApiError::forbidden(msg)
        }
        crate::governed_healing::GovernedHealingError::CrossProjectForbidden(msg) => {
            ApiError::forbidden(msg)
        }
        crate::governed_healing::GovernedHealingError::MaxHealingAttemptsExceeded(msg) => {
            ApiError::conflict(msg)
        }
        crate::governed_healing::GovernedHealingError::InvalidStateTransition(msg) => {
            ApiError::conflict(msg)
        }
        crate::governed_healing::GovernedHealingError::ProposalExpired(msg) => {
            ApiError::conflict(msg)
        }
        crate::governed_healing::GovernedHealingError::NotFound(msg) => ApiError::not_found(msg),
        crate::governed_healing::GovernedHealingError::Conflict(msg) => ApiError::conflict(msg),
        crate::governed_healing::GovernedHealingError::ExecutionError(msg) => {
            ApiError::internal(msg)
        }
        crate::governed_healing::GovernedHealingError::StorageError(msg) => ApiError::internal(msg),
        crate::governed_healing::GovernedHealingError::Internal(msg) => ApiError::internal(msg),
    }
}

fn make_healing_service(state: &AppState) -> crate::governed_healing::GovernedHealingService {
    let dispatcher = crate::agent_execution::AgentExecutionDispatcher::new(
        state.store.clone(),
        (*state.policy).clone(),
        state.registry.clone(),
        state.evidence.clone(),
        None,
    );
    crate::governed_healing::GovernedHealingService::new(
        state.store.clone(),
        dispatcher,
        state.evidence.clone(),
        state.policy.clone(),
        None,
    )
}

async fn create_healing_proposal_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(mut req): ApiJson<crate::governed_healing::HealingProposalRequest>,
) -> Result<(StatusCode, Json<crate::governed_healing::HealingProposal>), ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    req.project_id = project_id.to_string();
    let service = make_healing_service(&state);
    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .map_err(healing_err_to_api_err)?;
    Ok((StatusCode::CREATED, Json(proposal)))
}

/// POST /api/v1/projects/:project_id/planning/test-plan — runs the real
/// `TestPlanningAgent` (Phase 3) against this project's real requirements/
/// schema, grounded in its real healing-strategy calibration
/// (`GovernedHealingService::get_feedback_for_planning`, Phase 5) and real
/// fragile-test/risk signals (`AdaptiveIntelligenceService::generate_planning_feedback`,
/// Phase 6) — both already built expressly to feed this agent, but never
/// previously reachable outside their own tests. Missing history in either
/// (a brand-new project) is a legitimate, honest state, not an error, so
/// both are fetched best-effort rather than failing the whole request.
///
/// `schema_file_path` is the one field that names something on the server's
/// own disk, so it is confined to `AIQA_SCHEMA_ROOT` before anything else
/// runs — see `confine_schema_file_path`.
async fn generate_test_plan_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(mut req): ApiJson<crate::agents::planning::PlanningRequest>,
) -> Result<Json<crate::agents::planning::TestPlanOutput>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    confine_schema_file_path(&mut req, schema_root_from_env().as_deref())?;

    let project_id_str = project_id.to_string();
    let healing_service = make_healing_service(&state);
    req.healing_feedback = healing_service
        .get_feedback_for_planning(&project_id_str)
        .await
        .unwrap_or_default();
    let qa_service =
        crate::adaptive_intelligence::AdaptiveIntelligenceService::new(state.store.clone());
    req.qa_feedback = qa_service
        .generate_planning_feedback(&project_id_str)
        .await
        .ok();

    let plan =
        crate::agents::planning::generate_test_plan(state.store.clone(), &project_id_str, req)
            .await
            .map_err(ApiError::internal)?;
    Ok(Json(plan))
}

/// `AIQA_SCHEMA_ROOT` — the one directory a planning request's
/// `schema_file_path` may point into. Unset or blank means server-side
/// schema files are off and callers send the schema inline. Read per
/// request, and only here: `confine_schema_file_path` takes the root as a
/// parameter so tests never mutate process-wide environment state.
fn schema_root_from_env() -> Option<std::path::PathBuf> {
    std::env::var("AIQA_SCHEMA_ROOT")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| std::path::PathBuf::from(s.trim()))
}

/// Untrusted-input boundary for `PlanningRequest::schema_file_path`. The
/// schema inspector reads whatever path it is handed, and its lexical
/// denylist doesn't stop absolute paths, UNC shares or junctions, so without
/// this an Operator key could point the server at any file on its own disk.
/// The path is resolved by the OS and must land on a regular file inside
/// `root`; the request is then rewritten to carry that canonical path, so the
/// tool reads exactly what was checked (a relative path would otherwise be
/// re-resolved against the repo root, not `root`).
///
/// An absent or blank path is left alone, so inline schemas are unaffected.
/// A refused path always gets the same 400, with no path in it: telling
/// "doesn't exist" from "exists but outside the root" would make the endpoint
/// a file-existence oracle. The precise reason goes to the server log instead.
pub(crate) fn confine_schema_file_path(
    req: &mut crate::agents::planning::PlanningRequest,
    root: Option<&std::path::Path>,
) -> Result<(), ApiError> {
    use crate::tools::security::{resolve_path_within_root, PathContainmentError};

    const UNAVAILABLE: &str = "schema_file_path is not available: this server has no usable \
        schema directory configured. Send the schema inline as schema_content, or ask an \
        administrator to configure AIQA_SCHEMA_ROOT.";
    const REFUSED: &str = "schema_file_path must name an existing regular file inside the \
        server's schema directory (AIQA_SCHEMA_ROOT). Send the schema inline as \
        schema_content instead.";

    let Some(raw) = req
        .schema_file_path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    else {
        return Ok(());
    };
    let Some(root) = root else {
        return Err(ApiError::validation(UNAVAILABLE));
    };

    // The request carries the path as text; a lossy conversion could name a
    // different file, so a non-UTF-8 result is refused instead.
    let resolved = resolve_path_within_root(raw, root).and_then(|p| {
        p.into_os_string()
            .into_string()
            .map_err(|_| PathContainmentError::Unresolvable)
    });
    match resolved {
        Ok(canonical) => {
            req.schema_file_path = Some(canonical);
            Ok(())
        }
        Err(PathContainmentError::RootUnavailable) => {
            tracing::warn!("AIQA_SCHEMA_ROOT {root:?} does not resolve to a directory");
            Err(ApiError::validation(UNAVAILABLE))
        }
        Err(reason) => {
            tracing::warn!("planning: schema_file_path refused ({reason:?}): {raw:?}");
            Err(ApiError::validation(REFUSED))
        }
    }
}

async fn list_healing_proposals_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<HealingListQuery>,
) -> Result<Json<Vec<crate::governed_healing::HealingProposal>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let service = make_healing_service(&state);
    let proposals = service
        .list_proposals(&project_id.to_string(), query.state.as_deref())
        .await
        .map_err(healing_err_to_api_err)?;
    Ok(Json(proposals))
}

async fn get_healing_proposal_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, proposal_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::governed_healing::HealingProposal>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let service = make_healing_service(&state);
    let proposal = service
        .get_proposal(proposal_id)
        .await
        .map_err(healing_err_to_api_err)?;
    require_resource_in_project(Some(&proposal.project_id), project_id)?;
    Ok(Json(proposal))
}

async fn get_healing_proposal_diff_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, proposal_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::governed_healing::AstDiff>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let service = make_healing_service(&state);
    let proposal = service
        .get_proposal(proposal_id)
        .await
        .map_err(healing_err_to_api_err)?;
    require_resource_in_project(Some(&proposal.project_id), project_id)?;
    Ok(Json(proposal.ast_diff))
}

async fn approve_healing_proposal_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, proposal_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::governed_healing::HealingProposal>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let service = make_healing_service(&state);
    let proposal = service
        .approve_proposal(&ctx, proposal_id)
        .await
        .map_err(healing_err_to_api_err)?;
    Ok(Json(proposal))
}

async fn reject_healing_proposal_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, proposal_id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<HealingRejectRequest>,
) -> Result<Json<crate::governed_healing::HealingProposal>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let service = make_healing_service(&state);
    let proposal = service
        .reject_proposal(&ctx, proposal_id, &req.reason)
        .await
        .map_err(healing_err_to_api_err)?;
    Ok(Json(proposal))
}

async fn execute_healing_proposal_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, proposal_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::governed_healing::HealingProposal>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let service = make_healing_service(&state);
    let proposal = service
        .execute_approved(&ctx, proposal_id)
        .await
        .map_err(healing_err_to_api_err)?;
    Ok(Json(proposal))
}

async fn get_healing_verification_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, proposal_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::governed_healing::HealingVerification>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let service = make_healing_service(&state);
    let verification = service
        .verify_healing(&ctx, proposal_id)
        .await
        .map_err(healing_err_to_api_err)?;
    Ok(Json(verification))
}

async fn get_healing_feedback_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<Vec<crate::governed_healing::HealingFeedback>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let service = make_healing_service(&state);
    let feedback = service
        .get_feedback_for_planning(&project_id.to_string())
        .await
        .map_err(healing_err_to_api_err)?;
    Ok(Json(feedback))
}

// ============================================================================
// Phase 6: Adaptive QA Intelligence Read-Only Endpoints
// ============================================================================

async fn get_project_telemetry_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<Vec<crate::store::ExecutionTelemetryRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let list = state
        .store
        .list_execution_telemetry(&project_id.to_string(), 100)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(list))
}

async fn get_project_failure_clusters_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<Vec<crate::store::FailureClusterRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let pid = project_id.to_string();
    let clusters = state
        .store
        .list_failure_clusters(Some(&pid))
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(clusters))
}

async fn get_project_flakiness_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<Vec<crate::store::FlakinessMetricsRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let metrics = state
        .store
        .list_flakiness_metrics(&project_id.to_string())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(metrics))
}

async fn get_project_risk_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<crate::adaptive_intelligence::RiskCalibration>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let pid = project_id.to_string();
    let telemetry = state
        .store
        .list_execution_telemetry(&pid, 100)
        .await
        .map_err(ApiError::internal)?;
    let flakiness = state
        .store
        .list_flakiness_metrics(&pid)
        .await
        .map_err(ApiError::internal)?;
    let strategies = state
        .store
        .list_healing_strategy_metrics(&pid)
        .await
        .map_err(ApiError::internal)?;

    let total_runs = telemetry.len();
    let failed_runs = telemetry
        .iter()
        .filter(|t| matches!(t.status.as_str(), "FAILED" | "TIMEOUT" | "INFRA_ERROR"))
        .count();
    let failure_rate = if total_runs > 0 {
        failed_runs as f64 / total_runs as f64
    } else {
        0.0
    };

    let flakiness_score = if !flakiness.is_empty() {
        let sum: f64 = flakiness.iter().map(|f| f.flakiness_score).sum();
        sum / flakiness.len() as f64
    } else {
        0.0
    };

    let failure_severity = if failed_runs > 0 {
        let infra_fails = telemetry
            .iter()
            .filter(|t| matches!(t.status.as_str(), "INFRA_ERROR" | "TIMEOUT"))
            .count();
        if infra_fails > 0 {
            0.8
        } else {
            0.4
        }
    } else {
        0.0
    };

    let relations = state
        .store
        .list_quality_relations_for_project(&pid)
        .await
        .unwrap_or_default();
    let dependency_count = relations.len();

    let total_attempts: i64 = strategies.iter().map(|s| s.attempts_count).sum();
    let total_regressions: i64 = strategies.iter().map(|s| s.regression_count).sum();
    let healing_regression_rate = if total_attempts > 0 {
        total_regressions as f64 / total_attempts as f64
    } else {
        0.0
    };

    let security_sensitivity = 0.0;

    let factors = crate::adaptive_intelligence::RiskFactors {
        failure_rate,
        flakiness_score,
        failure_severity,
        dependency_count,
        healing_regression_rate,
        security_sensitivity,
    };

    if total_runs == 0 && flakiness.is_empty() && strategies.is_empty() {
        Ok(Json(crate::adaptive_intelligence::RiskCalibration {
            task_name: pid,
            risk_score: 0.0,
            risk_level: "INSUFFICIENT_DATA".to_string(),
            factors,
            explanation: "Insufficient execution telemetry or metrics to calibrate quality risk"
                .to_string(),
        }))
    } else {
        let cal = crate::adaptive_intelligence::calibrate_risk(&pid, factors);
        Ok(Json(cal))
    }
}

async fn get_project_strategies_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<Vec<crate::store::HealingStrategyMetricsRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let metrics = state
        .store
        .list_healing_strategy_metrics(&project_id.to_string())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(metrics))
}

async fn get_project_trends_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<crate::adaptive_intelligence::TrendAnalysis>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let telemetry = state
        .store
        .list_execution_telemetry(&project_id.to_string(), 100)
        .await
        .map_err(ApiError::internal)?;
    let durations: Vec<f64> = telemetry.iter().map(|t| t.duration_ms as f64).collect();
    let trend = crate::adaptive_intelligence::calculate_trend("execution_duration", &durations);
    Ok(Json(trend))
}

async fn get_project_quality_graph_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let nodes = state
        .store
        .list_quality_nodes(&project_id.to_string())
        .await
        .map_err(ApiError::internal)?;
    let relations = state
        .store
        .list_quality_relations_for_project(&project_id.to_string())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(serde_json::json!({
        "nodes": nodes,
        "relations": relations,
    })))
}

// ============================================================================
// Phase 8: Intelligent QA Operations & Cross-Environment Handlers
// ============================================================================

async fn list_failure_investigations_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<Vec<crate::operations::FailureInvestigation>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let ops = crate::operations::OperationsService::new(state.store.clone());
    let list = ops
        .list_investigations(&project_id.to_string())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(list))
}

async fn get_failure_investigation_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, cluster_id)): ApiPath<(Uuid, String)>,
) -> Result<Json<crate::operations::FailureInvestigation>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let ops = crate::operations::OperationsService::new(state.store.clone());
    let inv = ops
        .get_investigation(&project_id.to_string(), &cluster_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("failure investigation not found"))?;
    Ok(Json(inv))
}

async fn get_cluster_environments_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, cluster_id)): ApiPath<(Uuid, String)>,
) -> Result<Json<crate::operations::EnvironmentCorrelationMatrix>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let ops = crate::operations::OperationsService::new(state.store.clone());
    let matrix = ops
        .get_environment_matrix(&project_id.to_string(), &cluster_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("environment correlation matrix not found"))?;
    Ok(Json(matrix))
}

async fn get_cluster_timeline_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, cluster_id)): ApiPath<(Uuid, String)>,
) -> Result<Json<Vec<crate::operations::InvestigationTimelineEvent>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let ops = crate::operations::OperationsService::new(state.store.clone());
    let timeline = ops
        .get_investigation_timeline(&project_id.to_string(), &cluster_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("investigation timeline not found"))?;
    Ok(Json(timeline))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RetentionRequestPayload {
    pub retention_days: Option<u32>,
}

async fn retention_dry_run_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(payload): ApiJson<RetentionRequestPayload>,
) -> Result<Json<crate::operations::RetentionReport>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    auth::require_role(&ctx, Role::Admin)?;

    let retention_days = payload.retention_days.unwrap_or(30);
    let policy = crate::operations::RetentionPolicy {
        retention_days,
        dry_run: true,
    };

    let ops = crate::operations::OperationsService::new(state.store.clone());
    let report = ops
        .execute_retention(&project_id.to_string(), &policy, &ctx.name)
        .await
        .map_err(ApiError::validation)?;
    Ok(Json(report))
}

async fn retention_prune_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(payload): ApiJson<RetentionRequestPayload>,
) -> Result<Json<crate::operations::RetentionReport>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    auth::require_role(&ctx, Role::Admin)?;

    let retention_days = payload.retention_days.unwrap_or(30);
    let policy = crate::operations::RetentionPolicy {
        retention_days,
        dry_run: false,
    };

    let ops = crate::operations::OperationsService::new(state.store.clone());
    let report = ops
        .execute_retention(&project_id.to_string(), &policy, &ctx.name)
        .await
        .map_err(ApiError::validation)?;
    Ok(Json(report))
}
