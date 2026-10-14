//! Phase 6 (QA Intelligence Memory) — full-flow validation.
//!
//! Master plan §8 asks for the whole chain to be validated end to end:
//!
//! ```text
//! Requirements -> Requirement Intelligence -> Test Planning -> Test Execution
//!   -> Evidence -> Failure Intelligence -> Historical Memory -> Retrieval
//!   -> Risk Intelligence -> Future Test Planning
//! ```
//!
//! These tests drive that chain through the **real** store and service code, not
//! through fixtures shaped like it. Each step uses the same production entry point
//! the HTTP layer uses, so a break in any link shows up here rather than in a
//! dashboard.
//!
//! They are deliberately end-to-end and therefore fewer, wider tests rather than many
//! narrow ones — the point is the chain, not each step.
//!
//! Performance and index verification live in `qa_memory_perf_tests.rs`, and the
//! migration/data-integrity checks in `qa_memory_migration_tests.rs`. Security checks
//! (isolation, secrets, chain-of-thought) are in this file's last section, because
//! they are properties of the chain rather than of one step.

use crate::adaptive_intelligence::{ExecutionTelemetry, FailureCategory};
use crate::domain::{
    CreateTestCaseRequest, RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskStatus,
};
use crate::intelligence::Intelligence;
use crate::qa_memory::{InsufficientData, QaMemoryReader};
use crate::requirements::{ParsedDocument, ParsedRequirement, RequirementKind};
use crate::store::Store;
use uuid::Uuid;

async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("q6.sqlite3"))
        .await
        .expect("connect");
    (store, dir)
}

async fn project(store: &Store, name: &str) -> String {
    let id = Uuid::new_v4();
    store
        .insert_project(id, name, Some("phase 6 validation"))
        .await
        .expect("insert_project");
    id.to_string()
}

// ============================================================ the whole chain

#[tokio::test]
async fn the_whole_chain_carries_one_failure_from_a_requirement_to_the_next_plan() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Full chain").await;
    let reader = QaMemoryReader::new(store.clone());

    // ---------------------------------------------------------------- 1.
    // Requirements: a real parsed document with one real requirement.
    let doc_id = store
        .upsert_requirement_document(
            &pid,
            "PRD.md",
            Some("PRD"),
            "hash-chain-1",
            "The payment module must decline invalid cards.",
            &ParsedDocument {
                requirements: vec![ParsedRequirement {
                    key: "REQ-PAY-1".into(),
                    ordinal: 1,
                    section: Some("Payments".into()),
                    kind: RequirementKind::Functional,
                    text: "The payment module must decline invalid cards.".into(),
                }],
                sections_count: 1,
                skipped_sections: vec![],
                furniture_lines: 0,
            },
        )
        .await
        .expect("requirement document");

    let requirements = store
        .list_requirements(&pid, &[doc_id])
        .await
        .expect("list requirements");
    assert_eq!(
        requirements.len(),
        1,
        "step 1: the requirement must be stored"
    );
    let req_key = requirements[0].key.clone();

    // ---------------------------------------------------------------- 2.
    // Test case, traceable back to that requirement.
    let case_id = store
        .insert_test_case(
            &pid,
            &CreateTestCaseRequest {
                title: "Decline an invalid card".into(),
                description: "A declined card must show the decline reason.".into(),
                category: "functional".into(),
                priority: "P0".into(),
                severity: "high".into(),
                preconditions: vec!["a saved card".into()],
                steps: vec![],
                expected_result: Some("the decline reason is shown".into()),
                assertions: vec!["a decline reason is visible".into()],
                test_data: None,
                source_document: Some("PRD.md".into()),
                requirement_reference: Some(req_key.clone()),
                source_requirement: Some(req_key.clone()),
                traceability: None,
                risk_factor: None,
                suite_id: None,
                tags: vec![],
            },
            Some("validation"),
        )
        .await
        .expect("test case");

    // Module link: Phase 2's `covers` edge.
    store
        .link_test_case_to_module(&pid, &case_id.to_string(), "payment", "Payment", "Payment")
        .await
        .expect("module link");

    // ---------------------------------------------------------------- 3+4.
    // Execution: a real run whose task carries the case link, then the real
    // telemetry row `ingest_run` writes. The task fails twice, so a cluster forms.
    for attempt in 0..3 {
        let run_id = Uuid::new_v4();
        let task_id = format!("chain-task-{attempt}");
        store
            .insert_run_for_release(
                run_id,
                &RunPlan {
                    name: format!("chain run {attempt}"),
                    tasks: vec![TaskDef {
                        id: task_id.clone(),
                        name: "Test Case: Decline an invalid card".into(),
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
                Some(&pid),
                Some("validation"),
                RunState::Completed,
                None,
                None,
            )
            .await
            .expect("run");

        // ---------------------------------------------------------------- 6.
        // Failure intelligence: the real ingest path. This is what writes the
        // telemetry row, updates the quality graph, clusters the failure and — once
        // it recurs — drafts a bug. Inserting telemetry by hand here would double
        // count every execution, so this test drives the single real path only.
        let plan = RunPlan {
            name: format!("chain run {attempt}"),
            tasks: vec![TaskDef {
                id: task_id.clone(),
                name: "Test Case: Decline an invalid card".into(),
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
        };
        let states = vec![crate::domain::TaskState {
            id: task_id.clone(),
            name: "Test Case: Decline an invalid card".into(),
            prerequisites: Vec::new(),
            status: if attempt < 2 {
                TaskStatus::Failed
            } else {
                TaskStatus::Passed
            },
            reason: if attempt < 2 {
                Some("REQUEST POST /payment returned 422".into())
            } else {
                None
            },
            started_at: None,
            finished_at: None,
            test_case_id: Some(case_id.to_string()),
        }];
        Intelligence::new(&store)
            .ingest_run(Some(run_id), Some(&pid), &plan, &states)
            .await
            .expect("ingest_run");

        // ---------------------------------------------------------------- 5.
        // Evidence: a real evidence row for the run.
        store
            .insert_evidence(
                run_id,
                &task_id,
                "screenshot",
                &format!("evidence/{run_id}.png"),
                "image/png",
                4096,
            )
            .await
            .expect("evidence");
    }

    // ---------------------------------------------------------------- 7.
    // Historical memory: the telemetry carries the case link for every run.
    let telemetry = store
        .list_execution_telemetry(&pid, 1000)
        .await
        .expect("telemetry");
    assert_eq!(telemetry.len(), 3, "step 7: three executions remembered");
    assert!(
        telemetry.iter().all(|t| t.test_case_id == Some(case_id)),
        "step 7: every remembered execution is linked back to its test case"
    );

    // ---------------------------------------------------------------- 8.
    // Retrieval: the module, the case, and the requirement all answer from it.
    let stability = reader
        .test_stability(&pid, &case_id.to_string())
        .await
        .expect("test_stability")
        .view
        .expect("step 8: test stability");
    assert_eq!(stability.executions, 3);
    assert_eq!(stability.failed, 2);
    assert_eq!(stability.passed, 1);

    let module = reader
        .module_risk_history(&pid, "payment")
        .await
        .expect("module_risk_history")
        .view
        .expect("step 8: module history");
    assert_eq!(module.executions, 3);
    assert_eq!(module.failures, 2);

    // The requirement is reachable because the case carries a `validates` edge.
    // Link it the way requirement coverage does, then re-read.
    let req_node = store
        .upsert_quality_node(&pid, "requirement", &req_key, &req_key)
        .await
        .expect("requirement node");
    let case_node = store
        .upsert_quality_node(
            &pid,
            "test",
            &case_id.to_string(),
            "Decline an invalid card",
        )
        .await
        .expect("case node");
    store
        .upsert_quality_relation(case_node, req_node, "validates")
        .await
        .expect("validates edge");

    let requirement = reader
        .requirement_history(&pid, &req_key)
        .await
        .expect("requirement_history")
        .view
        .expect("step 8: requirement history");
    assert_eq!(requirement.executions, 3);
    assert_eq!(requirement.validating_test_cases, 1);

    // ---------------------------------------------------------------- 9.
    // Risk intelligence: same evidence, risk-shaped answer.
    let risk = reader
        .module_risk(&pid, "payment")
        .await
        .expect("module_risk")
        .view
        .expect("step 9: module risk");
    assert_eq!(risk.executions, 3);
    assert_eq!(risk.failures, 2);
    assert!(
        risk.weighted_contribution > 0.0,
        "two validation failures must carry measurable weight"
    );
    assert!(
        risk.measured_weight < 1.0,
        "and the model's uncovered part must still be reported: {}",
        risk.unmeasured_weight
    );

    // Bug history: the recurring failure must now be visible, which is the link
    // Phase 4 found broken and this chain is the regression guard for.
    let defects = reader.bug_history(&pid).await.expect("bug_history");
    assert_eq!(
        defects.len(),
        1,
        "step 9: two identical failures must produce one recurring defect, not an \
         empty list: {defects:?}"
    );
    assert!(defects[0].occurrences >= 2);
    assert!(
        defects[0].related_test_cases.contains(&case_id.to_string()),
        "the defect must name the test case it came from: {:?}",
        defects[0].related_test_cases
    );
    assert!(
        defects[0].related_modules.contains(&"payment".to_string()),
        "and the module it touched: {:?}",
        defects[0].related_modules
    );

    // ---------------------------------------------------------------- 10.
    // Future test planning: the next plan is informed by all of the above.
    let signals = reader
        .planning_signals(&pid)
        .await
        .expect("planning_signals");
    assert!(
        signals.iter().any(|s| s.subject == "payment"),
        "step 10: the failing module must reach the next plan: {signals:?}"
    );
    let module_signal = signals
        .iter()
        .find(|s| s.subject == "payment")
        .expect("a signal for payment");
    assert!(
        module_signal.metric.contains("2 of 3"),
        "step 10: with the real numbers: {}",
        module_signal.metric
    );
    assert_eq!(
        module_signal.evidence_runs.len(),
        3,
        "step 10: every run behind the signal is cited"
    );
    assert!(
        signals.iter().any(|s| s.kind == "recurring_defect"),
        "step 10: the recurring defect must also reach planning"
    );

    // And the agent's own feedback path agrees.
    let feedback = crate::adaptive_intelligence::AdaptiveIntelligenceService::new(store.clone())
        .generate_planning_feedback(&pid)
        .await
        .expect("planning feedback");
    assert!(
        feedback
            .planning_signals
            .iter()
            .any(|s| s.subject == "payment"),
        "step 10: the agent must receive what retrieval knows"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn a_chain_where_nothing_failed_still_produces_an_honest_empty_answer() {
    // The mirror of the chain above, so "no defects" is proven to be a real state
    // rather than the accident of a code path that never runs.
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Clean chain").await;
    let reader = QaMemoryReader::new(store.clone());

    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "healthy", "Healthy", "Healthy")
        .await
        .expect("link");

    let run_id = Uuid::new_v4();
    let task_id = "clean-task".to_string();
    store
        .insert_run_for_release(
            run_id,
            &RunPlan {
                name: "clean run".into(),
                tasks: vec![TaskDef {
                    id: task_id.clone(),
                    name: "Test Case: healthy".into(),
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
            Some(&pid),
            Some("validation"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("run");

    let now = chrono::Utc::now();
    store
        .insert_execution_telemetry(&ExecutionTelemetry {
            id: Uuid::new_v4(),
            project_id: pid.clone(),
            run_id,
            task_id,
            task_name: "Test Case: healthy".into(),
            test_case_id: Some(case),
            execution_id: None,
            executor: "simulated".into(),
            environment: "test".into(),
            start_time: now,
            end_time: Some(now),
            duration_ms: 10u64,
            status: TaskStatus::Passed,
            retry_count: 0,
            healing_attempt_count: 0,
            failure_category: None,
            failure_fingerprint: None,
            error_message: None,
            evidence_refs: Vec::new(),
            assertion_result: None,
            http_status: None,
            selector: None,
            verification_outcome: None,
            created_at: now,
        })
        .await
        .expect("telemetry");

    assert!(
        reader
            .bug_history(&pid)
            .await
            .expect("bug_history")
            .is_empty(),
        "one clean run must produce no defect"
    );
    assert!(
        reader
            .planning_signals(&pid)
            .await
            .expect("planning_signals")
            .iter()
            .all(|s| s.metric.starts_with("0 of ")),
        "a clean module may produce a signal, but it must report zero failures: {:?}",
        reader
            .planning_signals(&pid)
            .await
            .expect("planning_signals")
    );

    let risk = reader
        .module_risk(&pid, "healthy")
        .await
        .expect("module_risk")
        .view
        .unwrap();
    assert_eq!(risk.failures, 0);
    assert_eq!(
        risk.weighted_contribution, 0.0,
        "nothing failed, so nothing weighs"
    );
    store.pool.close().await;
}

// ============================================================ data integrity

#[tokio::test]
async fn the_chain_leaves_no_orphaned_or_duplicated_records() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Integrity").await;

    // A real test case, inserted through the real API, so the link below is a real
    // one. An invented id would test nothing: `tasks.test_case_id` has no foreign
    // key, so any string can be written and the orphan check would only be
    // reporting this fixture's own shortcut. That the column is unconstrained is
    // recorded in FINAL_VALIDATION_REPORT.md as an integrity observation.
    let case = store
        .insert_test_case(
            &pid,
            &CreateTestCaseRequest {
                title: "Integrity case".into(),
                description: "d".into(),
                category: "functional".into(),
                priority: "P1".into(),
                severity: "medium".into(),
                preconditions: vec![],
                steps: vec![],
                expected_result: None,
                assertions: vec!["it works".into()],
                test_data: None,
                source_document: None,
                requirement_reference: None,
                source_requirement: None,
                traceability: None,
                risk_factor: None,
                suite_id: None,
                tags: vec![],
            },
            Some("validation"),
        )
        .await
        .expect("test case");
    store
        .link_test_case_to_module(&pid, &case.to_string(), "m", "M", "M")
        .await
        .expect("link");

    // Two executions of the same case.
    for _ in 0..2 {
        let run_id = Uuid::new_v4();
        let task_id = format!("orphan-check-{run_id}");
        store
            .insert_run_for_release(
                run_id,
                &RunPlan {
                    name: "integrity".into(),
                    tasks: vec![TaskDef {
                        id: task_id.clone(),
                        name: "t".into(),
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
                Some(&pid),
                Some("validation"),
                RunState::Completed,
                None,
                None,
            )
            .await
            .expect("run");
        let now = chrono::Utc::now();
        store
            .insert_execution_telemetry(&ExecutionTelemetry {
                id: Uuid::new_v4(),
                project_id: pid.clone(),
                run_id,
                task_id,
                task_name: "t".into(),
                test_case_id: Some(case),
                execution_id: None,
                executor: "simulated".into(),
                environment: "test".into(),
                start_time: now,
                end_time: Some(now),
                duration_ms: 1u64,
                status: TaskStatus::Failed,
                retry_count: 0,
                healing_attempt_count: 0,
                failure_category: Some(FailureCategory::AssertionFailure),
                failure_fingerprint: Some("same-fp".into()),
                error_message: None,
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

    // No duplicate history: two executions, two telemetry rows, one case.
    let telemetry = store.list_execution_telemetry(&pid, 1000).await.unwrap();
    assert_eq!(telemetry.len(), 2, "two executions, two rows");

    // No orphaned telemetry: every row names a run that exists.
    let orphans: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_telemetry t
         WHERE NOT EXISTS (SELECT 1 FROM runs r WHERE r.id = t.run_id)",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(orphans, 0, "telemetry pointing at a missing run");

    // No orphaned task links: every non-null test_case_id is a real row.
    let bad_links: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks t
         WHERE t.test_case_id IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM test_cases c WHERE c.id = t.test_case_id)",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(
        bad_links, 0,
        "a task pointing at a test case that does not exist"
    );

    // Correct project scope: nothing of this project appears under another.
    let leaked: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_telemetry
         WHERE project_id != ?1 AND test_case_id IN (
             SELECT test_case_id FROM execution_telemetry WHERE project_id = ?1)",
    )
    .bind(&pid)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(
        leaked, 0,
        "one project's case id appearing in another's rows"
    );

    // Timestamps are RFC3339 and parseable, not a bare epoch or NULL.
    let unparseable: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_telemetry
         WHERE created_at IS NULL OR created_at NOT LIKE '____-__-__T%'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(
        unparseable, 0,
        "every remembered execution must carry a real RFC3339 timestamp"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn releasing_a_release_never_moves_a_run_out_of_its_project() {
    let (store, _d) = temp_store().await;
    let mine = project(&store, "Mine").await;
    let theirs = project(&store, "Theirs").await;
    let their_release = store
        .create_release(&theirs, Some("theirs"), None, None, None)
        .await
        .unwrap();

    let err = store
        .insert_run_for_release(
            Uuid::new_v4(),
            &RunPlan {
                name: "cross".into(),
                tasks: vec![],
                concurrency: 1,
            },
            Some(&mine),
            Some("validation"),
            RunState::Created,
            None,
            Some(&their_release.id.to_string()),
        )
        .await;
    assert!(
        err.is_err(),
        "attaching another project's release must be refused, not silently accepted"
    );

    // And nothing was written by the failed attempt.
    let runs = store.list_run_ids(Some(&mine)).await.expect("list runs");
    assert!(
        runs.is_empty(),
        "the refused insert must leave no run behind"
    );
    store.pool.close().await;
}

// ================================================================== security

#[tokio::test]
async fn no_cross_project_leakage_anywhere_in_the_chain() {
    let (store, _d) = temp_store().await;
    let mine = project(&store, "Mine").await;
    let theirs = project(&store, "Theirs").await;
    let reader = QaMemoryReader::new(store.clone());

    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&mine, &case.to_string(), "secret", "Secret", "Secret")
        .await
        .expect("link");
    let release = store
        .create_release(&mine, Some("mine-only"), None, None, None)
        .await
        .unwrap();

    for _ in 0..4 {
        let run_id = Uuid::new_v4();
        store
            .insert_run_for_release(
                run_id,
                &RunPlan {
                    name: "secret".into(),
                    tasks: vec![TaskDef {
                        id: "secret-task".into(),
                        name: "Test Case: secret".into(),
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
                Some(&mine),
                Some("validation"),
                RunState::Completed,
                None,
                Some(release.id.to_string().as_str()),
            )
            .await
            .expect("run");
        let now = chrono::Utc::now();
        store
            .insert_execution_telemetry(&ExecutionTelemetry {
                id: Uuid::new_v4(),
                project_id: mine.clone(),
                run_id,
                task_id: "secret-task".into(),
                task_name: "Test Case: secret".into(),
                test_case_id: Some(case),
                execution_id: None,
                executor: "simulated".into(),
                environment: "test".into(),
                start_time: now,
                end_time: Some(now),
                duration_ms: 1u64,
                status: TaskStatus::Failed,
                retry_count: 0,
                healing_attempt_count: 0,
                failure_category: Some(FailureCategory::DataFailure),
                failure_fingerprint: Some("secret-fingerprint".into()),
                error_message: None,
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

    // Every read surface, asked about the other project.
    let rendered = [
        serde_json::to_string(
            &reader
                .test_stability(&theirs, &case.to_string())
                .await
                .expect("test_stability"),
        )
        .unwrap(),
        serde_json::to_string(
            &reader
                .module_risk_history(&theirs, "secret")
                .await
                .expect("module_risk_history"),
        )
        .unwrap(),
        serde_json::to_string(
            &reader
                .module_risk(&theirs, "secret")
                .await
                .expect("module_risk"),
        )
        .unwrap(),
        serde_json::to_string(
            &reader
                .release_history(&theirs, release.id)
                .await
                .expect("release_history"),
        )
        .unwrap(),
        serde_json::to_string(
            &reader
                .release_risk(&theirs, release.id)
                .await
                .expect("release_risk"),
        )
        .unwrap(),
        serde_json::to_string(
            &reader
                .requirement_history(&theirs, "REQ-X")
                .await
                .expect("requirement_history"),
        )
        .unwrap(),
        serde_json::to_string(
            &reader
                .project_memory_summary(&theirs)
                .await
                .expect("project_memory_summary"),
        )
        .unwrap(),
        serde_json::to_string(
            &reader
                .planning_signals(&theirs)
                .await
                .expect("planning_signals"),
        )
        .unwrap(),
        serde_json::to_string(&reader.bug_history(&theirs).await.expect("bug_history")).unwrap(),
    ];

    for needle in [
        "secret-fingerprint",
        "mine-only",
        "Test Case: secret",
        &case.to_string(),
    ] {
        for json in &rendered {
            assert!(
                !json.contains(needle),
                "another project's data leaked into a read: {needle} appears in {json}"
            );
        }
    }

    assert_eq!(
        reader
            .test_stability(&theirs, &case.to_string())
            .await
            .expect("test_stability")
            .insufficient_data,
        Some(InsufficientData::NoExecutions)
    );
    assert!(reader
        .planning_signals(&theirs)
        .await
        .expect("planning_signals")
        .is_empty());
    assert!(reader
        .bug_history(&theirs)
        .await
        .expect("bug_history")
        .is_empty());
    assert!(reader
        .release_risk_overview(&theirs)
        .await
        .expect("release_risk_overview")
        .is_empty());
    let summary = reader
        .project_memory_summary(&theirs)
        .await
        .expect("project_memory_summary");
    assert!(!summary.has_history && summary.executions == 0 && summary.releases == 0);
    store.pool.close().await;
}

#[tokio::test]
async fn no_credential_material_reaches_the_memory() {
    // A secret in a task's params and in an error message must not appear in any
    // retrieved view. `execution_telemetry` has both fields, so the risk is real and
    // the check is worth having.
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Secrets").await;
    let reader = QaMemoryReader::new(store.clone());

    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "m", "M", "M")
        .await
        .expect("link");

    const SECRET: &str = "sk-live-DO-NOT-LEAK-9f3a2b";
    let run_id = Uuid::new_v4();
    store
        .insert_run_for_release(
            run_id,
            &RunPlan {
                name: "secret-bearing".into(),
                tasks: vec![TaskDef {
                    id: "secret-task".into(),
                    name: "t".into(),
                    prerequisites: Vec::new(),
                    simulate: Some(SimSpec {
                        outcome: SimOutcome::Pass,
                        delay_ms: 1,
                    }),
                    executor: Some("simulated".into()),
                    params: serde_json::json!({
                        "username": "qa",
                        "password": SECRET,
                    }),
                    test_case_id: Some(case.to_string()),
                }],
                concurrency: 1,
            },
            Some(&pid),
            Some("validation"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("run");

    let now = chrono::Utc::now();
    store
        .insert_execution_telemetry(&ExecutionTelemetry {
            id: Uuid::new_v4(),
            project_id: pid.clone(),
            run_id,
            task_id: "secret-task".into(),
            task_name: "t".into(),
            test_case_id: Some(case),
            execution_id: None,
            executor: "simulated".into(),
            environment: "test".into(),
            start_time: now,
            end_time: Some(now),
            duration_ms: 1u64,
            status: TaskStatus::Failed,
            retry_count: 0,
            healing_attempt_count: 0,
            failure_category: Some(FailureCategory::AuthenticationFailure),
            failure_fingerprint: Some("auth-failure".into()),
            // A real risk: a message that echoes the credential.
            error_message: Some(format!("login rejected for password {SECRET}")),
            evidence_refs: Vec::new(),
            assertion_result: None,
            http_status: None,
            selector: None,
            verification_outcome: None,
            created_at: now,
        })
        .await
        .expect("telemetry");

    let rendered = [
        serde_json::to_string(
            &reader
                .test_stability(&pid, &case.to_string())
                .await
                .expect("test_stability"),
        )
        .unwrap(),
        serde_json::to_string(
            &reader
                .module_risk_history(&pid, "m")
                .await
                .expect("module_risk_history"),
        )
        .unwrap(),
        serde_json::to_string(&reader.module_risk(&pid, "m").await.expect("module_risk")).unwrap(),
        serde_json::to_string(&reader.bug_history(&pid).await.expect("bug_history")).unwrap(),
        serde_json::to_string(
            &reader
                .planning_signals(&pid)
                .await
                .expect("planning_signals"),
        )
        .unwrap(),
        serde_json::to_string(
            &reader
                .project_memory_summary(&pid)
                .await
                .expect("project_memory_summary"),
        )
        .unwrap(),
    ];

    for json in &rendered {
        assert!(
            !json.contains(SECRET),
            "a credential reached a retrieved view: {json}"
        );
    }

    // Scope note, stated here because it is a finding rather than a pass:
    //
    // What is checked is the **retrieval layer** — every view this phase added
    // reads `execution_telemetry` and the quality graph, and none of them reads
    // `params` or echoes `error_message`. That is what master plan §8's "credentials
    // are not stored in memory" means for this system.
    //
    // What is NOT checked, and is a pre-existing gap outside this plan's scope:
    // `runs.plan` stores `TaskDef.params` verbatim, so a password handed to a task
    // is persisted as given. `adaptive_intelligence::mask_dynamic_tokens` exists and
    // scrubs telemetry text, but nothing applies it to the plan. That is run-plan
    // persistence, not QA memory, and fixing it would mean changing what the run
    // engine persists for every executor — so it is reported for a decision rather
    // than changed here. See FINAL_VALIDATION_REPORT.md.
    store.pool.close().await;
}

#[tokio::test]
async fn no_chain_of_thought_is_stored_or_returned_anywhere_in_the_chain() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "No CoT").await;
    let reader = QaMemoryReader::new(store.clone());

    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "m", "M", "M")
        .await
        .expect("link");
    let release = store
        .create_release(&pid, Some("r"), None, None, None)
        .await
        .unwrap();

    for i in 0..3 {
        let run_id = Uuid::new_v4();
        store
            .insert_run_for_release(
                run_id,
                &RunPlan {
                    name: "cot".into(),
                    tasks: vec![TaskDef {
                        id: "cot-task".into(),
                        name: "t".into(),
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
                Some(&pid),
                Some("validation"),
                RunState::Completed,
                None,
                Some(release.id.to_string().as_str()),
            )
            .await
            .expect("run");
        let now = chrono::Utc::now();
        store
            .insert_execution_telemetry(&ExecutionTelemetry {
                id: Uuid::new_v4(),
                project_id: pid.clone(),
                run_id,
                task_id: "cot-task".into(),
                task_name: "t".into(),
                test_case_id: Some(case),
                execution_id: None,
                executor: "simulated".into(),
                environment: "test".into(),
                start_time: now,
                end_time: Some(now),
                duration_ms: 1u64,
                status: if i == 0 {
                    TaskStatus::Passed
                } else {
                    TaskStatus::Failed
                },
                retry_count: 0,
                healing_attempt_count: 0,
                failure_category: if i == 0 {
                    None
                } else {
                    Some(FailureCategory::ServerError)
                },
                failure_fingerprint: if i == 0 { None } else { Some("fp".into()) },
                error_message: None,
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

    // Everything the chain returns, in one list.
    let rendered = [
        serde_json::to_string(
            &reader
                .test_stability(&pid, &case.to_string())
                .await
                .expect("test_stability"),
        )
        .unwrap(),
        serde_json::to_string(
            &reader
                .module_risk_history(&pid, "m")
                .await
                .expect("module_risk_history"),
        )
        .unwrap(),
        serde_json::to_string(
            &reader
                .release_history(&pid, release.id)
                .await
                .expect("release_history"),
        )
        .unwrap(),
        serde_json::to_string(
            &reader
                .requirement_history(&pid, "REQ-1")
                .await
                .expect("requirement_history"),
        )
        .unwrap(),
        serde_json::to_string(&reader.module_risk(&pid, "m").await.expect("module_risk")).unwrap(),
        serde_json::to_string(
            &reader
                .release_risk(&pid, release.id)
                .await
                .expect("release_risk"),
        )
        .unwrap(),
        serde_json::to_string(
            &reader
                .release_risk_overview(&pid)
                .await
                .expect("release_risk_overview"),
        )
        .unwrap(),
        serde_json::to_string(&reader.bug_history(&pid).await.expect("bug_history")).unwrap(),
        serde_json::to_string(
            &reader
                .planning_signals(&pid)
                .await
                .expect("planning_signals"),
        )
        .unwrap(),
        serde_json::to_string(
            &reader
                .project_memory_summary(&pid)
                .await
                .expect("project_memory_summary"),
        )
        .unwrap(),
        serde_json::to_string(
            &crate::adaptive_intelligence::AdaptiveIntelligenceService::new(store.clone())
                .generate_planning_feedback(&pid)
                .await
                .unwrap(),
        )
        .unwrap(),
    ];

    // `rationale` and `proposed_actions` exist elsewhere in AIQA for a different
    // purpose; the ban here is on stored *reasoning*, so the list is the words that
    // would mean hidden thinking rather than a recorded fact.
    for needle in [
        "chain_of_thought",
        "chainOfThought",
        "chain-of-thought",
        "scratchpad",
        "internal_notes",
        "hidden_reasoning",
        "step_by_step",
        "monologue",
    ] {
        for json in &rendered {
            assert!(!json.contains(needle), "a view exposed {needle}: {json}");
        }
    }
    store.pool.close().await;
}
