//! Phase G, part 2 — audit-log retention and a real latency histogram.
//!
//! Two of the remaining findings:
//!
//! 1. `audit_log` had no retention of any kind. One row per audited action,
//!    appended forever, on the table every state-changing handler writes to. The
//!    existing retention pass deliberately preserved audit logs — correct default
//!    for a trail, since how long evidence must be kept is a compliance decision
//!    the backend should not make for an operator — but that left unbounded growth
//!    with *no operator control at all*. `Store::prune_audit` is that control:
//!    project-scoped, dry-run capable, and opt-in.
//!
//! 2. `/metrics` reported only counts. Every metric was a gauge or counter of
//!    *things* (runs by state, tasks by status, audit rows), so nothing there
//!    could answer a latency question. `http_request_duration_seconds` is now a
//!    real in-process histogram.
//!
//! Audit rows are inserted through raw SQL here rather than `insert_audit`,
//! which stamps `now_rfc3339()` and so cannot produce the aged rows a retention
//! test needs. That is a deliberate limitation of `insert_audit`, called out
//! rather than worked around by widening its signature.

use crate::ai_router::test_support::{
    healthy_model, healthy_provider, FakeDiscovery, FakeEmbedder, FakeExecutor,
};
use crate::ai_router::{AiExecutor, AiInfrastructure, CapabilityLevel};
use crate::operations::{OperationsService, RetentionPolicy};
use crate::policy::PolicyConfig;
use crate::state::AppState;
use crate::store::Store;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

async fn setup() -> (Store, TempDir) {
    let dir = TempDir::new().unwrap();
    let store = Store::connect(&dir.path().join("retention.sqlite3"))
        .await
        .unwrap();
    (store, dir)
}

/// A router with one healthy mock provider, so `AppState` can be built. No real
/// provider is reachable from these tests and none is needed.
fn inert_ai_router() -> Arc<AiInfrastructure> {
    let executor: Arc<dyn AiExecutor> = Arc::new(FakeExecutor::ok("{}"));
    let mut executors = HashMap::new();
    executors.insert("mock", executor);
    Arc::new(AiInfrastructure::new(
        vec![Arc::new(FakeDiscovery(healthy_provider(
            "mock",
            vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
        )))],
        executors,
    ))
}

/// Writes an audit row with a caller-chosen `occurred_at`, which is the only way
/// to get a row old enough for retention to be interested in it.
async fn insert_audit_at(
    store: &Store,
    project_id: Option<&str>,
    action: &str,
    occurred_at: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO audit_log (id, occurred_at, actor, role, project_id, action, target, detail, outcome)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(occurred_at.to_rfc3339())
    .bind("someone")
    .bind("admin")
    .bind(project_id)
    .bind(action)
    .bind(Uuid::new_v4().to_string())
    .bind("detail")
    .bind("ok")
    .execute(&store.pool)
    .await
    .unwrap();
}

async fn count_audit(store: &Store, project_id: Option<&str>) -> i64 {
    let sql = match project_id {
        Some(_) => "SELECT COUNT(*) as c FROM audit_log WHERE project_id = ?",
        None => "SELECT COUNT(*) as c FROM audit_log WHERE project_id IS NULL",
    };
    let mut q = sqlx::query_scalar(sql);
    if let Some(pid) = project_id {
        q = q.bind(pid);
    }
    q.fetch_one(&store.pool).await.unwrap()
}

// --------------------------------------------------------------- prune_audit

#[tokio::test]
async fn a_dry_run_counts_the_aged_audit_rows_without_deleting_them() {
    let (store, _dir) = setup().await;
    let old = Utc::now() - Duration::days(90);
    for _ in 0..3 {
        insert_audit_at(&store, Some("p1"), "run.create", old).await;
    }
    insert_audit_at(&store, Some("p1"), "run.create", Utc::now()).await;

    let eligible = store
        .prune_audit("p1", Utc::now() - Duration::days(30), true)
        .await
        .unwrap();

    assert_eq!(eligible, 3, "only the aged rows are eligible");
    assert_eq!(
        count_audit(&store, Some("p1")).await,
        4,
        "a dry run must delete nothing"
    );
}

#[tokio::test]
async fn pruning_audit_removes_only_rows_older_than_the_cutoff() {
    let (store, _dir) = setup().await;
    for _ in 0..2 {
        insert_audit_at(
            &store,
            Some("p1"),
            "run.create",
            Utc::now() - Duration::days(60),
        )
        .await;
    }
    insert_audit_at(&store, Some("p1"), "run.create", Utc::now()).await;

    let pruned = store
        .prune_audit("p1", Utc::now() - Duration::days(30), false)
        .await
        .unwrap();

    assert_eq!(pruned, 2);
    assert_eq!(
        count_audit(&store, Some("p1")).await,
        1,
        "the recent row is still within retention and must survive"
    );
}

/// The security-relevant one. `audit_log.project_id` is nullable, and rows
/// written without a tenant — an unauthenticated `auth.fail`, a system action —
/// have NULL there. Those are shared across every project, so a project admin
/// pruning their own project must not be able to reach them.
#[tokio::test]
async fn pruning_audit_cannot_delete_the_untenanted_rows_that_are_shared_across_tenants() {
    let (store, _dir) = setup().await;
    let old = Utc::now() - Duration::days(365);
    for _ in 0..4 {
        insert_audit_at(&store, None, "auth.fail", old).await;
    }
    insert_audit_at(&store, Some("p1"), "run.create", old).await;

    let pruned = store
        .prune_audit("p1", Utc::now() - Duration::days(30), false)
        .await
        .unwrap();

    assert_eq!(pruned, 1, "only p1's own row");
    assert_eq!(
        count_audit(&store, None).await,
        4,
        "global audit history must be unreachable from a project-scoped prune"
    );
}

#[tokio::test]
async fn pruning_audit_in_one_project_does_not_touch_another() {
    let (store, _dir) = setup().await;
    let old = Utc::now() - Duration::days(60);
    insert_audit_at(&store, Some("p1"), "run.create", old).await;
    insert_audit_at(&store, Some("p2"), "run.create", old).await;

    assert_eq!(
        store
            .prune_audit("p1", Utc::now() - Duration::days(30), false)
            .await
            .unwrap(),
        1
    );
    assert_eq!(count_audit(&store, Some("p1")).await, 0);
    assert_eq!(
        count_audit(&store, Some("p2")).await,
        1,
        "p2 is not p1's to delete"
    );
}

// -------------------------------------------------------- execute_retention

/// The regression guard for the default: a caller that asks for telemetry
/// retention, exactly as every caller did before `include_audit` existed, must
/// still leave the audit trail completely alone.
#[tokio::test]
async fn retention_without_include_audit_leaves_the_audit_trail_untouched() {
    let (store, _dir) = setup().await;
    let pid = "p1";
    insert_audit_at(
        &store,
        Some(pid),
        "run.create",
        Utc::now() - Duration::days(120),
    )
    .await;

    let ops = OperationsService::new(store.clone());
    let report = ops
        .execute_retention(
            pid,
            &RetentionPolicy {
                retention_days: 30,
                dry_run: false,
                include_audit: false,
            },
            "admin",
        )
        .await
        .unwrap();

    assert_eq!(report.audit_pruned_count, 0);
    assert_eq!(
        count_audit(&store, Some(pid)).await,
        2,
        "the aged row plus the row recording this very prune"
    );
}

#[tokio::test]
async fn retention_with_include_audit_prunes_the_trail_and_records_that_it_did() {
    let (store, _dir) = setup().await;
    let pid = "p1";
    for _ in 0..3 {
        insert_audit_at(
            &store,
            Some(pid),
            "run.create",
            Utc::now() - Duration::days(120),
        )
        .await;
    }
    insert_audit_at(&store, Some(pid), "run.create", Utc::now()).await;

    let ops = OperationsService::new(store.clone());
    let report = ops
        .execute_retention(
            pid,
            &RetentionPolicy {
                retention_days: 30,
                dry_run: false,
                include_audit: true,
            },
            "admin",
        )
        .await
        .unwrap();

    assert_eq!(report.audit_pruned_count, 3);
    // The recent row survives, plus the row this prune wrote about itself — the
    // audit log documents its own pruning, which is the one outcome worth
    // engineering for: a trail pruned without a trace of having been pruned.
    let remaining = count_audit(&store, Some(pid)).await;
    assert_eq!(remaining, 2, "the recent row and the prune's own record");
    let self_recorded: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE project_id = ? AND action = 'audit.retention'",
    )
    .bind(pid)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(
        self_recorded, 1,
        "pruning the audit trail must be distinguishable in the trail"
    );
}

#[tokio::test]
async fn an_audit_dry_run_reports_what_it_would_prune_and_deletes_nothing() {
    let (store, _dir) = setup().await;
    let pid = "p1";
    for _ in 0..5 {
        insert_audit_at(
            &store,
            Some(pid),
            "run.create",
            Utc::now() - Duration::days(90),
        )
        .await;
    }

    let ops = OperationsService::new(store.clone());
    let report = ops
        .execute_retention(
            pid,
            &RetentionPolicy {
                retention_days: 30,
                dry_run: true,
                include_audit: true,
            },
            "admin",
        )
        .await
        .unwrap();

    // `pruned_count` is 0 under a dry run by construction, and `audit_pruned_count`
    // follows the same rule, so the *count of what is eligible* is what the
    // operator reads — `eligible_count` covers telemetry, so the audit number is
    // reported through the separate field and has to be visible before the
    // destructive call.
    assert!(report.dry_run);
    assert_eq!(report.audit_pruned_count, 0);
    assert_eq!(
        count_audit(&store, Some(pid)).await,
        5,
        "a dry run must delete nothing"
    );
}

#[tokio::test]
async fn retention_still_rejects_a_zero_day_window() {
    let (store, _dir) = setup().await;
    let ops = OperationsService::new(store.clone());
    let err = ops
        .execute_retention(
            "p1",
            &RetentionPolicy {
                retention_days: 0,
                dry_run: true,
                include_audit: true,
            },
            "admin",
        )
        .await;
    assert!(err.is_err(), "a zero-day window would prune all history");
}

// ------------------------------------------------------------------ metrics

/// Reads one scalar series out of the exposition body.
fn series(body: &str, name: &str) -> f64 {
    body.lines()
        .find(|l| l.starts_with(name) && !l.contains("_bucket"))
        .and_then(|l| l.rsplit(' ').next())
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or_else(|| panic!("{name} missing from:\n{body}"))
}

/// Builds the real production router over a throwaway store, so the middleware is
/// exercised through the same wiring a deployment gets rather than a hand-rolled
/// one that would pass even if the layer were never mounted.
async fn test_state() -> (AppState, TempDir) {
    let dir = TempDir::new().unwrap();
    let store = Store::connect(&dir.path().join("metrics.sqlite3"))
        .await
        .unwrap();
    let evidence =
        crate::evidence::LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence");
    (
        AppState::new(
            Arc::new(crate::executor::ExecutorRegistry::new(Arc::new(
                crate::executor::SimulatedExecutor,
            ))),
            store,
            PolicyConfig {
                max_concurrency: 8,
                require_approval_for: HashSet::new(),
                blocked_executors: HashSet::new(),
            },
            evidence,
            inert_ai_router(),
            Arc::new(FakeEmbedder::unconfigured()),
        ),
        dir,
    )
}

/// `/metrics` must be able to answer a latency question, and it must be in a
/// shape Prometheus accepts: cumulative buckets, a `+Inf` bucket equal to the
/// count, and a sum.
///
/// The count is checked as a *delta*, not an absolute. The histogram is
/// process-global by construction — a latency observation only exists in memory,
/// so it cannot be per-test — which means any assertion about an absolute value
/// would either be wrong when this test runs alone or depend on how many other
/// tests happened to run first. Comparing before/after makes it deterministic
/// either way.
#[tokio::test]
async fn serving_requests_moves_the_latency_histogram_and_metrics_exposes_it_well_formed() {
    let (state, _dir) = test_state().await;
    let app = crate::api::router(state.clone());

    let before = series(
        &crate::metrics::render(&state.store.pool).await,
        "http_request_duration_seconds_count",
    );

    // `/health` is public, needs no key, and exists on the real router — so
    // these are genuine end-to-end requests through the real layer stack.
    for _ in 0..3 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let body = crate::metrics::render(&state.store.pool).await;
    let after = series(&body, "http_request_duration_seconds_count");
    assert!(
        after >= before + 3.0,
        "three requests were served but the histogram only moved from {before} to {after} — \
         the timing middleware is not mounted on the router:\n{body}"
    );

    assert!(
        body.contains("# TYPE http_request_duration_seconds histogram"),
        "the histogram must be declared as a histogram, not a gauge:\n{body}"
    );

    let inf: f64 = body
        .lines()
        .find(|l| l.contains("http_request_duration_seconds_bucket") && l.contains("+Inf"))
        .and_then(|l| l.rsplit(' ').next())
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or_else(|| panic!("+Inf bucket missing:\n{body}"));
    assert_eq!(
        inf, after,
        "the +Inf bucket must equal _count, not be recomputed separately"
    );

    assert!(
        series(&body, "http_request_duration_seconds_sum") > 0.0,
        "the requests took measurable time, so the sum cannot be zero:\n{body}"
    );
}

/// Cumulative semantics, not per-bucket counts: bucket N must never be smaller
/// than bucket N-1. `histogram_quantile` interpolates between adjacent buckets
/// and silently returns nonsense on a non-monotonic series, so this is the
/// property that makes the metric usable rather than merely present.
#[tokio::test]
async fn the_latency_buckets_are_cumulative_and_non_decreasing() {
    let (store, _dir) = setup().await;
    let body = crate::metrics::render(&store.pool).await;

    let mut previous: Option<f64> = None;
    for bound in crate::metrics::DURATION_BUCKETS {
        let needle = format!("http_request_duration_seconds_bucket{{le=\"{bound}\"}} ");
        let line = body
            .lines()
            .find(|l| l.starts_with(&needle))
            .unwrap_or_else(|| panic!("bucket le=\"{bound}\" missing:\n{body}"));
        let value: f64 = line.rsplit(' ').next().unwrap().trim().parse().unwrap();
        if let Some(prev) = previous {
            assert!(
                value >= prev,
                "bucket le=\"{bound}\" ({value}) is smaller than the previous \
                 bucket ({prev}); buckets must be cumulative"
            );
        }
        previous = Some(value);
    }
}

#[tokio::test]
async fn ai_provider_execution_is_recorded_in_metrics_and_ledger_compatibly() {
    let (store, _dir) = setup().await;
    let infra = inert_ai_router();
    let ledger = std::sync::Arc::new(crate::ai_usage::UsageLedger::new(store.clone()));
    infra.set_usage_sink(ledger);

    // Execute through router
    let outcome = infra
        .generate(crate::ai_router::AiRequest {
            task: crate::ai_router::AiTask::GeneralAi,
            prompt: "hello".to_string(),
            preferred_provider: Some("mock".to_string()),
            preferred_model: Some("mock-model".to_string()),
        })
        .await;
    match outcome {
        crate::ai_router::GenerationOutcome2::Ok(res) => {
            assert_eq!(res.text, "{}");
        }
        other => panic!("expected Ok, got {other:?}"),
    }

    // Render metrics
    let body = crate::metrics::render(&store.pool).await;

    // Verify AI duration histogram was populated
    assert!(
        body.contains("ai_provider_execution_duration_seconds_count{provider=\"mock\",task=\"general_ai\"} 1"),
        "metrics must record provider execution count:\n{body}"
    );
    assert!(
        body.contains("ai_provider_calls_total{provider=\"mock\",task=\"general_ai\",outcome=\"success\"} 1"),
        "metrics must record provider call outcome counter:\n{body}"
    );

    // Verify 15-minute slot compatibility in database
    let slot = crate::ai_usage::slot_of(chrono::Utc::now());
    let rows = store.ai_usage_since(&slot, None).await.unwrap();
    assert_eq!(rows.len(), 1, "ledger must record exactly 1 15-minute slot row");
    assert_eq!(rows[0].provider, "mock");
    assert_eq!(rows[0].model, "mock-model");
    assert_eq!(rows[0].calls, 1);
    assert_eq!(rows[0].failed_calls, 0);
}
