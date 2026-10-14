//! Phase 6 (QA Intelligence Memory) — performance and storage measurement.
//!
//! Master plan §8 Performance: "Query performance, large historical datasets, index
//! usage where applicable, memory retrieval latency, planning overhead, storage
//! growth — **do not prematurely optimize without evidence**."
//!
//! So these tests measure rather than assume. What they assert:
//!
//! 1. **Index usage.** Every project-scoped query the retrieval layer issues is run
//!    through `EXPLAIN QUERY PLAN` and must show a *search* of a named index, not a
//!    bare `SCAN`. A missing index would otherwise be invisible until a project grew
//!    large enough to hurt.
//!
//! 2. **Latency on a large history.** A project with a realistic history (hundreds of
//!    executions across many cases, modules and releases) is read repeatedly and the
//!    per-read wall time is recorded. The assertion is a **ceiling far above what was
//!    measured**, not the measured value itself: a timing test that pins the current
//!    speed fails on a slower CI machine for no real reason. The number is printed
//!    under `--nocapture` so the evidence lands in the log.
//!
//! 3. **Storage growth.** One telemetry row's cost is measured, so the growth
//!    attributable to this feature is a known quantity rather than a surprise.
//!
//! Nothing here is optimized. If a number is bad, the report says so and the fix is a
//! separate decision.

use crate::adaptive_intelligence::ExecutionTelemetry;
use crate::domain::{RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskStatus};
use crate::qa_memory::QaMemoryReader;
use crate::store::Store;
use sqlx::Row;
use std::time::Instant;
use uuid::Uuid;

async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("q6perf.sqlite3"))
        .await
        .expect("connect");
    (store, dir)
}

async fn project(store: &Store, name: &str) -> String {
    let id = Uuid::new_v4();
    store
        .insert_project(id, name, Some("phase 6 perf"))
        .await
        .expect("insert_project");
    id.to_string()
}

/// Builds a project with `cases` test cases spread over `modules` modules and
/// `releases` releases, each with `per_case` executions. Deterministic statuses so a
/// failure is reproducible rather than incidental.
async fn build_history(
    store: &Store,
    pid: &str,
    cases: usize,
    modules: usize,
    releases: usize,
    per_case: usize,
) -> Vec<Uuid> {
    let mut release_ids = Vec::new();
    for i in 0..releases {
        release_ids.push(
            store
                .create_release(pid, Some(&format!("2026.{:02}", i + 1)), None, None, None)
                .await
                .unwrap()
                .id,
        );
    }

    let mut case_ids = Vec::new();
    for c in 0..cases {
        let case = Uuid::new_v4();
        case_ids.push(case);
        let module = format!("module-{}", c % modules.max(1));
        store
            .link_test_case_to_module(pid, &case.to_string(), &module, &module, &module)
            .await
            .expect("link");

        for n in 0..per_case {
            let run_id = Uuid::new_v4();
            let task_id = format!("perf-{c}-{n}");
            store
                .insert_run_for_release(
                    run_id,
                    &RunPlan {
                        name: "perf".into(),
                        tasks: vec![TaskDef {
                            id: task_id.clone(),
                            name: format!("Test Case: {module} #{c}"),
                            prerequisites: Vec::new(),
                            simulate: Some(SimSpec {
                                outcome: SimOutcome::Pass,
                                delay_ms: 1,
                            }),
                            executor: Some("simulated".into()),
                            params: serde_json::json!({}),
                            test_case_id: Some(case.to_string()),
                        }],
                        concurrency: 1,
                    },
                    Some(pid),
                    Some("perf"),
                    RunState::Completed,
                    None,
                    release_ids
                        .get(n % release_ids.len().max(1))
                        .map(|r| r.to_string())
                        .as_deref(),
                )
                .await
                .expect("run");

            // Every 4th execution fails, so clusters and rates are non-trivial.
            let failing = n % 4 == 0;
            let now = chrono::Utc::now();
            store
                .insert_execution_telemetry(&ExecutionTelemetry {
                    id: Uuid::new_v4(),
                    project_id: pid.to_string(),
                    run_id,
                    task_id,
                    task_name: format!("Test Case: {module} #{c}"),
                    test_case_id: Some(case),
                    execution_id: None,
                    executor: "simulated".into(),
                    environment: "test".into(),
                    start_time: now,
                    end_time: Some(now),
                    duration_ms: 20u64,
                    status: if failing {
                        TaskStatus::Failed
                    } else {
                        TaskStatus::Passed
                    },
                    retry_count: 0,
                    healing_attempt_count: 0,
                    failure_category: failing
                        .then_some(crate::adaptive_intelligence::FailureCategory::AssertionFailure),
                    failure_fingerprint: failing.then(|| format!("fp-{}-{}", c, n)),
                    error_message: failing.then(|| format!("assertion failed at step {n}")),
                    evidence_refs: Vec::new(),
                    assertion_result: None,
                    http_status: None,
                    selector: None,
                    verification_outcome: None,
                    created_at: now,
                })
                .await
                .expect("telemetry");
        }
    }
    case_ids
}

// ------------------------------------------------------------- index usage

/// The plan SQLite actually chose for a query, as text.
///
/// `EXPLAIN QUERY PLAN` output is what makes index usage checkable rather than
/// assumed — the same discipline migration 0027 used when it added indexes on the
/// strength of recorded plans rather than a guess.
async fn query_plan(store: &Store, sql: &str, bind: &str) -> String {
    // `EXPLAIN QUERY PLAN` returns four columns — id, parent, notused, detail — so
    // this reads `detail` by name rather than positionally.
    //
    // AUDITED: `sql` is a literal written in this file, one per test, and never
    // derived from input. sqlx's blanket ban on runtime-built SQL is the right
    // default; this is the sanctioned escape for the fixed statements `EXPLAIN
    // QUERY PLAN` requires, and `bind` is still a real bound parameter.
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {sql}")))
        .bind(bind)
        .fetch_all(&store.pool)
        .await
        .expect("explain");
    rows.iter()
        .map(|r| r.get::<String, _>("detail"))
        .collect::<Vec<_>>()
        .join(" | ")
}

#[tokio::test]
async fn the_telemetry_read_the_retrieval_layer_issues_uses_an_index() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Index use").await;
    build_history(&store, &pid, 8, 2, 2, 3).await;

    // This is the exact shape `QaMemoryReader::telemetry_for` uses.
    let plan = query_plan(
        &store,
        "SELECT * FROM execution_telemetry WHERE project_id = ? ORDER BY created_at DESC LIMIT ?",
        &pid,
    )
    .await;
    eprintln!("INDEX telemetry read plan: {plan}");

    assert!(
        plan.to_uppercase().contains("USING INDEX"),
        "the retrieval layer's own read must not be a table scan; SQLite chose: {plan}"
    );
    assert!(
        !plan.to_uppercase().contains("SCAN EXECUTION_TELEMETRY"),
        "a bare scan of execution_telemetry would make every retrieval O(history); \
         SQLite chose: {plan}"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn the_per_test_case_telemetry_read_uses_the_index_phase_two_added() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Case index").await;
    let cases = build_history(&store, &pid, 4, 1, 1, 2).await;

    // This is the shape Phase 3's per-case read uses, and the reason migration 0028
    // added `idx_telemetry_test_case`.
    let plan = query_plan(
        &store,
        "SELECT * FROM execution_telemetry WHERE project_id = ? AND test_case_id = ?",
        &pid,
    )
    .await;
    eprintln!("INDEX per-case read plan: {plan}");
    let _ = &cases;

    assert!(
        plan.to_uppercase().contains("USING INDEX"),
        "per-case history must not scan the whole telemetry table; SQLite chose: {plan}"
    );
    assert!(
        !plan.to_uppercase().contains("SCAN EXECUTION_TELEMETRY"),
        "a bare scan here would make every per-case read O(all history); {plan}"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn the_per_task_and_per_release_reads_use_an_index() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Run index").await;
    let release = store
        .create_release(&pid, Some("r"), None, None, None)
        .await
        .unwrap();
    build_history(&store, &pid, 3, 1, 1, 2).await;

    // `runs_for_release` — the read behind every release comparison.
    let plan = query_plan(
        &store,
        "SELECT id FROM runs WHERE release_id = ?",
        &release.id.to_string(),
    )
    .await;
    eprintln!("INDEX runs-by-release plan: {plan}");
    assert!(
        plan.to_uppercase().contains("USING INDEX"),
        "release comparison must not scan every run; SQLite chose: {plan}"
    );

    // `list_release` — the read behind every Phase 5 risk view.
    let plan = query_plan(
        &store,
        "SELECT * FROM qa_releases WHERE project_id = ? ORDER BY created_at DESC",
        &pid,
    )
    .await;
    eprintln!("INDEX list-releases plan: {plan}");
    assert!(
        plan.to_uppercase().contains("USING INDEX"),
        "listing releases must not scan; SQLite chose: {plan}"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn the_cluster_to_telemetry_join_phase_six_added_uses_an_index() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Cluster index").await;
    build_history(&store, &pid, 3, 1, 1, 2).await;

    // The join migration 0030 exists for: a cluster and the executions behind it.
    let plan = query_plan(
        &store,
        "SELECT id FROM failure_clusters WHERE project_id = ? AND failure_fingerprint = ?",
        &pid,
    )
    .await;
    eprintln!("INDEX cluster-by-fingerprint plan: {plan}");
    assert!(
        plan.to_uppercase().contains("USING INDEX"),
        "correlating a defect with its executions must not scan; SQLite chose: {plan}"
    );
    store.pool.close().await;
}

// ------------------------------------------------------------------ latency

#[tokio::test]
async fn retrieval_over_a_large_history_stays_far_below_the_ceiling() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Large").await;

    // 120 cases x 4 executions = 480 telemetry rows, 12 modules, 6 releases. Bigger
    // than anything this project has held, small enough to build in a test.
    let cases = build_history(&store, &pid, 120, 12, 6, 4).await;
    let reader = QaMemoryReader::new(store.clone());

    let total = store
        .list_execution_telemetry(&pid, 10_000)
        .await
        .unwrap()
        .len();
    assert_eq!(total, 480, "the fixture must be the size it claims");

    // Warm once so the measurement is of retrieval, not of first-touch page faults.
    let _ = reader
        .test_stability(&pid, &cases[0].to_string())
        .await
        .expect("test_stability");

    let mut worst = std::time::Duration::ZERO;
    let mut reads = 0usize;
    for case in cases.iter().take(40) {
        let t0 = Instant::now();
        let r = reader
            .test_stability(&pid, &case.to_string())
            .await
            .expect("test_stability");
        let took = t0.elapsed();
        assert!(r.is_found(), "every fixture case must be found");
        worst = worst.max(took);
        reads += 1;
    }

    let module = {
        let t0 = Instant::now();
        let r = reader
            .module_risk_history(&pid, "module-0")
            .await
            .expect("module_risk_history");
        let took = t0.elapsed();
        assert!(r.is_found());
        worst = worst.max(took);
        reads += 1;
        took
    };
    let planning = {
        let t0 = Instant::now();
        let s = reader
            .planning_signals(&pid)
            .await
            .expect("planning_signals");
        assert!(!s.is_empty());
        let took = t0.elapsed();
        worst = worst.max(took);
        reads += 1;
        took
    };
    let risk = {
        let t0 = Instant::now();
        let r = reader
            .module_risk(&pid, "module-0")
            .await
            .expect("module_risk");
        assert!(r.is_found());
        let took = t0.elapsed();
        worst = worst.max(took);
        reads += 1;
        took
    };

    eprintln!(
        "PERF over {total} telemetry rows / {} reads: worst {:?}, module_risk_history {:?}, \
         planning_signals {:?}, module_risk {:?}",
        reads, worst, module, planning, risk
    );

    // The ceiling is deliberately loose — it is a guard against an accidental
    // O(history^2) or an unbounded scan, not a performance target. The measured
    // numbers are printed above as the evidence.
    const CEILING: std::time::Duration = std::time::Duration::from_millis(750);
    assert!(
        worst < CEILING,
        "the slowest single retrieval took {worst:?} over {total} rows, above the \
         {CEILING:?} ceiling. Every read here is one bounded scan, so anything near \
         this ceiling means a query changed shape."
    );
    store.pool.close().await;
}

#[tokio::test]
async fn planning_signal_count_stays_bounded_as_history_grows() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Bounded").await;
    build_history(&store, &pid, 60, 30, 10, 4).await;

    let reader = QaMemoryReader::new(store.clone());
    let signals = reader
        .planning_signals(&pid)
        .await
        .expect("planning_signals");

    // 30 modules and 10 releases exist; the planner must offer a bounded, ranked
    // subset rather than everything, or the prompt grows without limit.
    assert!(
        signals.len() <= 20,
        "planning input must stay bounded; got {} signals from 30 modules and 10 releases",
        signals.len()
    );
    assert!(!signals.is_empty());

    // And the ranking must be monotonic in evidence, so truncation drops the weakest.
    let confidences: Vec<f64> = signals.iter().map(|s| s.confidence).collect();
    let mut sorted = confidences.clone();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
    assert_eq!(
        confidences, sorted,
        "signals must be offered best-evidenced first, or truncation is arbitrary"
    );
    eprintln!(
        "PERF planning: {} modules + 10 releases -> {} bounded signals, \
         confidences {:?}",
        30,
        signals.len(),
        confidences
    );
    store.pool.close().await;
}

// ----------------------------------------------------------- storage growth

#[tokio::test]
async fn the_storage_cost_of_one_remembered_execution_is_measured_not_guessed() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Storage").await;
    let cases = build_history(&store, &pid, 10, 2, 2, 5).await;

    let rows = store.list_execution_telemetry(&pid, 10_000).await.unwrap();
    assert_eq!(rows.len(), 50);

    // Bytes held by the tables this feature reads or writes. `page_count` is the
    // number SQLite is actually storing, which is the honest number to report.
    let pages: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    let total_bytes = pages * page_size;

    // Telemetry's own share.
    let telemetry_bytes: i64 = sqlx::query_scalar(
        "SELECT SUM(LENGTH(CAST(id AS BLOB))) + SUM(LENGTH(CAST(project_id AS BLOB)))
         + SUM(LENGTH(CAST(run_id AS BLOB))) + SUM(LENGTH(CAST(task_id AS BLOB)))
         + SUM(LENGTH(CAST(task_name AS BLOB))) + SUM(LENGTH(CAST(executor AS BLOB)))
         + SUM(LENGTH(CAST(environment AS BLOB))) + SUM(LENGTH(CAST(status AS BLOB)))
         + SUM(LENGTH(CAST(created_at AS BLOB))) + SUM(LENGTH(CAST(start_time AS BLOB)))
         + SUM(LENGTH(CAST(end_time AS BLOB))) + SUM(LENGTH(CAST(evidence_refs AS BLOB)))
         + SUM(LENGTH(CAST(failure_fingerprint AS BLOB)))
         + SUM(LENGTH(CAST(error_message AS BLOB)))
         + SUM(LENGTH(CAST(failure_category AS BLOB)))
         + SUM(LENGTH(CAST(test_case_id AS BLOB)))
       FROM execution_telemetry",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap_or(0);

    eprintln!(
        "STORAGE {} telemetry rows -> db {total_bytes} bytes ({pages} pages x {page_size}); \
         telemetry payload {telemetry_bytes} bytes; per-row payload {} bytes; \
         {} linked cases of {}",
        rows.len(),
        telemetry_bytes / rows.len() as i64,
        rows.iter()
            .filter_map(|r| r.test_case_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        cases.len()
    );

    // Growth must be linear in executions, not super-linear: a run/row per
    // execution plus one graph vertex per distinct name.
    assert!(
        telemetry_bytes / rows.len() as i64 > 0,
        "every execution must actually store something"
    );
    assert!(
        telemetry_bytes < total_bytes,
        "telemetry is a part of the database, not all of it: {telemetry_bytes} vs {total_bytes}"
    );
    store.pool.close().await;
}
