//! Phase 4 (QA Intelligence Memory) — memory → test-planning intelligence.
//!
//! Master plan §6 "Testing", one test per line:
//!
//! | Required case                          | Test |
//! |----------------------------------------|------|
//! | Memory influences planning when evidence exists | `a_module_with_failures_produces_a_planning_signal` and 2 more |
//! | Memory does not influence when no evidence      | `a_project_with_no_history_produces_no_signals_at_all` |
//! | Old irrelevant history does not dominate        | `history_older_than_the_window_produces_no_signal` |
//! | Project isolation                              | `signals_never_cross_a_project_boundary` |
//! | Requirement-specific planning                  | `a_signal_names_the_subject_it_is_about` |
//! | Risk prioritization                            | `the_best_evidenced_signal_is_offered_first` |
//! | Regression behaviour                           | `the_planning_agent_receives_the_signals` + the existing suite |
//!
//! Plus §6 "Explainability": every signal must carry a historical signal, a metric,
//! an evidence reference, a confidence and a time window — and §1.5: no
//! chain-of-thought.

use crate::adaptive_intelligence::AdaptiveIntelligenceService;
use crate::domain::{RunPlan, RunState, SimOutcome, SimSpec, TaskDef};
use crate::qa_memory::{PlanningSignal, QaMemoryReader, PLANNING_SIGNAL_WINDOW_DAYS};
use crate::store::Store;
use chrono::{Duration, Utc};
use uuid::Uuid;

async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("q4.sqlite3"))
        .await
        .expect("connect");
    (store, dir)
}

async fn project(store: &Store, name: &str) -> String {
    let id = Uuid::new_v4();
    store
        .insert_project(id, name, Some("phase 4 fixture"))
        .await
        .expect("insert_project");
    id.to_string()
}

/// One execution, `days_ago` days old, so the window rule can be tested.
async fn execute(
    store: &Store,
    pid: &str,
    case_id: Uuid,
    status: &str,
    fingerprint: Option<&str>,
    days_ago: i64,
) -> Uuid {
    let run_id = Uuid::new_v4();
    let task_id = format!("t-{run_id}");
    store
        .insert_run_for_release(
            run_id,
            &RunPlan {
                name: "p4".into(),
                tasks: vec![TaskDef {
                    id: task_id.clone(),
                    name: "Test Case: p4".into(),
                    prerequisites: Vec::new(),
                    simulate: Some(SimSpec {
                        outcome: SimOutcome::Pass,
                        delay_ms: 1,
                    }),
                    executor: Some("simulated".into()),
                    params: serde_json::json!({}),
                    test_case_id: Some(case_id.to_string()),
                }],
                concurrency: 1,
            },
            Some(pid),
            Some("fixture"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("run");

    let when = Utc::now() - Duration::days(days_ago);
    let status = match status {
        "PASSED" => crate::domain::TaskStatus::Passed,
        "FAILED" => crate::domain::TaskStatus::Failed,
        other => panic!("unknown status {other}"),
    };
    store
        .insert_execution_telemetry(&crate::adaptive_intelligence::ExecutionTelemetry {
            id: Uuid::new_v4(),
            project_id: pid.to_string(),
            run_id,
            task_id,
            task_name: "Test Case: p4".into(),
            test_case_id: Some(case_id),
            execution_id: None,
            executor: "simulated".into(),
            environment: "test".into(),
            start_time: when,
            end_time: Some(when),
            duration_ms: 5u64,
            status,
            retry_count: 0,
            healing_attempt_count: 0,
            failure_category: None,
            failure_fingerprint: fingerprint.map(ToString::to_string),
            error_message: None,
            evidence_refs: Vec::new(),
            assertion_result: None,
            http_status: None,
            selector: None,
            verification_outcome: None,
            created_at: when,
        })
        .await
        .expect("telemetry");
    run_id
}

// ------------------------------------------------- memory influences planning

#[tokio::test]
async fn a_module_with_failures_produces_a_planning_signal() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Influences").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();

    store
        .link_test_case_to_module(
            &pid,
            &case.to_string(),
            "payment",
            "Payment flow",
            "Payment",
        )
        .await
        .expect("link");
    // 5 executions, 3 failing: enough sample to be a measurement.
    for _ in 0..3 {
        execute(&store, &pid, case, "FAILED", Some("fp-timeout"), 2).await;
    }
    for _ in 0..2 {
        execute(&store, &pid, case, "PASSED", None, 2).await;
    }

    let signals = reader.planning_signals(&pid).await;
    let module_signal = signals
        .iter()
        .find(|s| s.kind == "module_risk" && s.subject == "payment")
        .expect("a module_risk signal for payment");

    assert!(
        module_signal.metric.contains("3 of 5"),
        "the metric must carry the real numbers: {}",
        module_signal.metric
    );
    assert_eq!(
        module_signal.evidence_runs.len(),
        5,
        "a metric must cite the runs behind it"
    );
    assert!(
        module_signal.confidence > 0.5,
        "five executions should outrank a hint: {}",
        module_signal.confidence
    );
    assert!(
        module_signal.window.contains("execution"),
        "a signal must state its window: {}",
        module_signal.window
    );
    assert!(!module_signal.basis.is_empty());
}

#[tokio::test]
async fn a_recurring_defect_produces_a_planning_signal() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Defects").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();

    // Two executions of the same fingerprint create a cluster; the existing
    // failure-recording path then drafts a bug for it. This test drives that real
    // path rather than inserting a draft directly.
    let mut runs = Vec::new();
    for _ in 0..2 {
        runs.push(execute(&store, &pid, case, "FAILED", Some("recurring-fp"), 1).await);
    }
    assert_eq!(runs.len(), 2);

    // The signature the recorder normalises is derived from the task's reason, which
    // this path does not set, so drive `record_failure` through the service the way
    // `ingest_run` does.
    record_failure_twice(&store, &pid).await;

    let signals = reader.planning_signals(&pid).await;
    let defect = signals
        .iter()
        .find(|s| s.kind == "recurring_defect")
        .expect("a recurring_defect signal");
    assert!(
        defect.metric.contains("2 recorded occurrence"),
        "recurrence must come from the stored count: {}",
        defect.metric
    );
    assert!(defect.basis.contains("occurrence_count"));
}

/// Drive the real failure-recording path so a cluster and a bug draft genuinely
/// exist, rather than fabricating one or adding a test-only accessor to production
/// code.
///
/// `IntelligenceService::ingest_run` is the public entry point that
/// `engine.rs` calls on `RunCompleted`, and it is what calls `record_failure`,
/// which is what upserts the cluster and drafts the bug. Two calls with the same
/// failing task is therefore exactly what two real runs of the same failure do.
async fn record_failure_twice(store: &Store, pid: &str) {
    for _ in 0..2 {
        let svc = crate::intelligence::Intelligence::new(store);
        let plan = RunPlan {
            name: "p4 defect".into(),
            tasks: vec![TaskDef {
                id: "planning-signal-task".into(),
                name: "Test Case: payment decline".into(),
                prerequisites: Vec::new(),
                simulate: Some(SimSpec {
                    outcome: SimOutcome::Fail,
                    delay_ms: 1,
                }),
                executor: Some("simulated".into()),
                params: serde_json::json!({}),
                test_case_id: None,
            }],
            concurrency: 1,
        };
        let tasks = vec![crate::domain::TaskState {
            id: "planning-signal-task".into(),
            name: "Test Case: payment decline".into(),
            prerequisites: Vec::new(),
            status: crate::domain::TaskStatus::Failed,
            reason: Some("REQUEST GET /payment returned 500".into()),
            started_at: None,
            finished_at: None,
            test_case_id: None,
        }];
        svc.ingest_run(None, Some(pid), &plan, &tasks)
            .await
            .expect("ingest_run records the failure");
    }
}

#[tokio::test]
async fn a_release_with_repeated_failures_produces_a_planning_signal() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Release signals").await;
    let reader = QaMemoryReader::new(store.clone());
    let release = store
        .create_release(&pid, Some("2026.10"), None, None, None)
        .await
        .unwrap();
    let case = Uuid::new_v4();

    for _ in 0..4 {
        let run = execute(&store, &pid, case, "FAILED", Some("fp-r"), 3).await;
        sqlx::query("UPDATE runs SET release_id = ? WHERE id = ?")
            .bind(release.id.to_string())
            .bind(run.to_string())
            .execute(&store.pool)
            .await
            .expect("attach release");
    }

    let signals = reader.planning_signals(&pid).await;
    let rel = signals
        .iter()
        .find(|s| s.kind == "release_regression")
        .expect("a release_regression signal");
    assert_eq!(rel.subject, "2026.10");
    assert!(rel.metric.contains("4 of 4"), "{}", rel.metric);
}

// -------------------------------------- memory does NOT influence when no evidence

#[tokio::test]
async fn a_project_with_no_history_produces_no_signals_at_all() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "No history").await;
    let reader = QaMemoryReader::new(store.clone());

    // A module and a release exist, so nothing can be missing for want of a subject.
    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "empty", "Empty", "Empty")
        .await
        .expect("link");
    store
        .create_release(&pid, Some("2026.10"), None, None, None)
        .await
        .unwrap();

    assert!(
        reader.planning_signals(&pid).await.is_empty(),
        "with no executions there is nothing to plan from, so nothing may be offered"
    );
}

#[tokio::test]
async fn the_planning_agent_receives_no_signals_for_a_project_with_no_history() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "No history agent").await;
    let svc = AdaptiveIntelligenceService::new(store.clone());

    let feedback = svc
        .generate_planning_feedback(&pid)
        .await
        .expect("feedback");
    assert!(
        feedback.planning_signals.is_empty(),
        "a brand-new project must not receive invented planning signals"
    );
    // The pre-existing string fields keep their old behaviour.
    assert!(feedback.fragile_tests.is_empty());
}

#[tokio::test]
async fn the_planning_agent_receives_the_signals_when_evidence_exists() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "With history").await;
    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "refund", "Refund", "Refund")
        .await
        .expect("link");
    for _ in 0..4 {
        execute(&store, &pid, case, "FAILED", Some("fp-refund"), 1).await;
    }

    let svc = AdaptiveIntelligenceService::new(store.clone());
    let feedback = svc
        .generate_planning_feedback(&pid)
        .await
        .expect("feedback");

    assert!(
        feedback
            .planning_signals
            .iter()
            .any(|s| s.subject == "refund"),
        "the agent must receive what memory knows: {:?}",
        feedback.planning_signals
    );
}

// ------------------------------------------------- old history does not dominate

#[tokio::test]
async fn history_older_than_the_window_produces_no_signal() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Old").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "legacy", "Legacy", "Legacy")
        .await
        .expect("link");

    // Comfortably outside the window.
    for _ in 0..5 {
        execute(
            &store,
            &pid,
            case,
            "FAILED",
            Some("fp-ancient"),
            PLANNING_SIGNAL_WINDOW_DAYS + 30,
        )
        .await;
    }

    assert!(
        reader.planning_signals(&pid).await.is_empty(),
        "history older than the {PLANNING_SIGNAL_WINDOW_DAYS}-day window must not drive planning, \
         however many rows it has"
    );
}

#[tokio::test]
async fn a_recent_failure_outranks_old_history_in_the_same_module() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Mixed ages").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "mixed", "Mixed", "Mixed")
        .await
        .expect("link");

    // Ancient: outside the window, so it does not enter the signal at all.
    for _ in 0..10 {
        execute(
            &store,
            &pid,
            case,
            "FAILED",
            Some("fp-ancient"),
            PLANNING_SIGNAL_WINDOW_DAYS + 10,
        )
        .await;
    }
    // One recent pass and one recent failure: inside the window.
    execute(&store, &pid, case, "PASSED", None, 1).await;
    execute(&store, &pid, case, "FAILED", Some("fp-recent"), 1).await;

    let signals = reader.planning_signals(&pid).await;
    let mixed = signals
        .iter()
        .find(|s| s.subject == "mixed")
        .expect("a signal for the module");
    assert!(
        mixed.metric.contains("1 of 2"),
        "only the in-window executions may be counted: {}",
        mixed.metric
    );
    assert_eq!(
        mixed.evidence_runs.len(),
        2,
        "ten ancient rows must not be cited as evidence for a current signal"
    );
}

// ------------------------------------------------------------------ isolation

#[tokio::test]
async fn signals_never_cross_a_project_boundary() {
    let (store, _d) = temp_store().await;
    let mine = project(&store, "Mine").await;
    let theirs = project(&store, "Theirs").await;
    let reader = QaMemoryReader::new(store.clone());

    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&mine, &case.to_string(), "secret", "Secret", "Secret")
        .await
        .expect("link");
    for _ in 0..3 {
        execute(&store, &mine, case, "FAILED", Some("fp-secret"), 1).await;
    }

    // `mine` owns three failures and must produce a signal; `theirs` owns
    // nothing and must produce none. (An earlier draft of this test asserted
    // that `theirs` was NON-empty, which would have meant the very leak it
    // exists to catch.)
    assert!(
        !reader.planning_signals(&mine).await.is_empty(),
        "the project that owns the history must see its own signals"
    );
    assert!(
        reader.planning_signals(&theirs).await.is_empty(),
        "another project's failures must not become this project's planning input"
    );
}

// ---------------------------------------------------- requirement-specific

#[tokio::test]
async fn a_signal_names_the_subject_it_is_about() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Subjects").await;
    let reader = QaMemoryReader::new(store.clone());

    for module in ["alpha", "beta"] {
        let case = Uuid::new_v4();
        store
            .link_test_case_to_module(&pid, &case.to_string(), module, module, module)
            .await
            .expect("link");
        for _ in 0..3 {
            execute(&store, &pid, case, "FAILED", Some("fp"), 1).await;
        }
    }

    let signals = reader.planning_signals(&pid).await;
    let subjects: std::collections::BTreeSet<&str> =
        signals.iter().map(|s| s.subject.as_str()).collect();
    assert!(
        subjects.contains("alpha") && subjects.contains("beta"),
        "each module must be reported under its own name: {subjects:?}"
    );
}

// ------------------------------------------------------ risk prioritization

#[tokio::test]
async fn the_best_evidenced_signal_is_offered_first() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Ordering").await;
    let reader = QaMemoryReader::new(store.clone());

    // "thin": one execution -> confidence 0.3. "thick": twenty -> 0.9.
    let thin = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &thin.to_string(), "thin", "Thin", "Thin")
        .await
        .expect("link thin");
    execute(&store, &pid, thin, "FAILED", Some("fp"), 1).await;

    let thick = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &thick.to_string(), "thick", "Thick", "Thick")
        .await
        .expect("link thick");
    for _ in 0..20 {
        execute(&store, &pid, thick, "FAILED", Some("fp"), 1).await;
    }

    let signals = reader.planning_signals(&pid).await;
    let first = signals
        .iter()
        .find(|s| s.subject == "thick")
        .expect("thick signal");
    let second = signals
        .iter()
        .find(|s| s.subject == "thin")
        .expect("thin signal");
    let pos = |s: &PlanningSignal| signals.iter().position(|x| x.subject == s.subject).unwrap();
    assert!(
        pos(first) < pos(second),
        "the better-evidenced module must be offered first: {:?}",
        signals
            .iter()
            .map(|s| (&s.subject, s.confidence))
            .collect::<Vec<_>>()
    );
    assert!(first.confidence > second.confidence);
}

// --------------------------------------------------------------- explainability

#[tokio::test]
async fn every_signal_carries_the_five_fields_the_plan_requires() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Explainable").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "x", "X", "X")
        .await
        .expect("link");
    for _ in 0..6 {
        execute(&store, &pid, case, "FAILED", Some("fp"), 2).await;
    }

    let signals = reader.planning_signals(&pid).await;
    assert!(!signals.is_empty());
    for s in &signals {
        // historical signal (kind + subject)
        assert!(!s.kind.is_empty(), "{s:?}");
        assert!(!s.subject.is_empty(), "{s:?}");
        // metric
        assert!(!s.metric.is_empty(), "{s:?}");
        assert!(
            s.metric.chars().any(|c| c.is_ascii_digit()),
            "a metric must carry numbers, not adjectives: {}",
            s.metric
        );
        // evidence reference
        assert!(!s.evidence_runs.is_empty(), "{s:?}");
        // confidence
        assert!((0.0..=1.0).contains(&s.confidence), "{s:?}");
        // time window
        assert!(!s.window.is_empty(), "{s:?}");
        assert!(
            s.window.contains(".."),
            "a window must have a range: {}",
            s.window
        );
        // and what it was computed from
        assert!(!s.basis.is_empty(), "{s:?}");
    }
}

#[tokio::test]
async fn no_signal_carries_a_field_that_looks_like_hidden_reasoning() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "No CoT").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "x", "X", "X")
        .await
        .expect("link");
    for _ in 0..4 {
        execute(&store, &pid, case, "FAILED", Some("fp"), 1).await;
    }

    let json = serde_json::to_string(&reader.planning_signals(&pid).await).unwrap();
    for needle in [
        "chain_of_thought",
        "chainOfThought",
        "reasoning",
        "thinking",
        "scratchpad",
        "internal_notes",
    ] {
        assert!(
            !json.contains(needle),
            "a planning signal exposed {needle}, which §1.5 forbids: {json}"
        );
    }
}

#[tokio::test]
async fn signals_are_deterministic_for_the_same_history() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Deterministic").await;
    let reader = QaMemoryReader::new(store.clone());
    for module in ["a", "b", "c"] {
        let case = Uuid::new_v4();
        store
            .link_test_case_to_module(&pid, &case.to_string(), module, module, module)
            .await
            .expect("link");
        for _ in 0..3 {
            execute(&store, &pid, case, "FAILED", Some("fp"), 1).await;
        }
    }

    let first = serde_json::to_string(&reader.planning_signals(&pid).await).unwrap();
    let second = serde_json::to_string(&reader.planning_signals(&pid).await).unwrap();
    assert_eq!(
        first, second,
        "planning input must be reproducible, or two identical plans could differ"
    );
}
