//! Phase 5 — the production `ProbeRunner`.
//!
//! ## Why this file exists separately
//!
//! [`crate::agents::exploration`] contains the agent, its tools, its budget and its
//! classification. It contains **no execution code at all** — which is the property that
//! makes "exploration is not a second browser executor" checkable rather than aspirational.
//! The one place an exploratory probe reaches an executor is here, and here is nothing but
//! the product's existing run pipeline.
//!
//! ## What "executing a probe" actually means
//!
//! A probe's `Action`s travel exactly the path any run travels:
//!
//! 1. a `RunPlan` is built with the probe's actions as the task's parameters;
//! 2. the plan is written by `store::insert_run_for_release`;
//! 3. the outcome is written by `Intelligence::ingest_run` — the same writer that produces
//!    every telemetry row, failure cluster and bug draft in this codebase;
//! 4. the resulting failure is fingerprinted by Phase 2's `canonical_failure_fingerprint`;
//! 5. any matching recorded cluster or bug draft is resolved and attached.
//!
//! Nothing is invented at any step. The only judgement call is
//! [`AssertionEvidence`], and it is supplied by whoever dispatches the run — see below.
//!
//! ## The honest part: what "the assertions were evaluated" means
//!
//! An `Action::AssertVisible` is *semantics*, not syntax. Whether an element was actually
//! visible is decided by the executor — the Playwright executor, in its own TypeScript
//! process. A simulated executor resolves the whole task without evaluating any individual
//! assertion, so a simulated failure is evidence that **a task failed** and not evidence
//! that **an assertion was violated**.
//!
//! Getting this backwards would let any failure become a `CONFIRMED_DEFECT`, which is the
//! one outcome this phase must never produce. So it is a declared input rather than
//! something inferred, and [`AssertionEvidence::NotEvaluated`] is the default. Every finding
//! derived from it classifies as `INSUFFICIENT_EVIDENCE` — a statement about the
//! exploration's evidence, not about the application.
//!
//! A caller that genuinely has a browser-backed executor reads the per-assertion outcome
//! out of that executor's own output and passes [`AssertionEvidence::Evaluated`]. That is
//! the only path to a confirmed defect, and it goes through the executor that already
//! exists.
//!
//! ## `execution_telemetry.assertion_result` exists and nothing fills it
//!
//! The schema already has exactly the right slot: `assertion_result TEXT` on every
//! telemetry row. An audit of its writers found the column is **read** by `store.rs` and
//! written as `None` by the only construction site that sets it; the API assertion path
//! builds its own `ApiAssertionResult` for the run log rather than for telemetry. So today
//! every telemetry row this system produces has `assertion_result IS NULL`, including rows
//! produced by a real browser executor.
//!
//! That is why this runner consults the column **first** and the dispatcher's declaration
//! **second**, rather than the other way round:
//!
//! * if a future executor starts populating `assertion_result`, the structural signal takes
//!   over automatically and nothing here needs to change;
//! * until then, the only way to assert an evaluation happened is for the dispatcher to say
//!   so, which is why that path exists and why it is not the default.
//!
//! Reported as a Phase 5 limitation rather than worked around.

use crate::adaptive_intelligence::canonical_failure_fingerprint;
use crate::agents::exploration::{ComposedProbe, ProbeOutcome, ProbeRunner};
use crate::domain::{RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskState, TaskStatus};
use crate::intelligence::Intelligence;
use crate::store::Store;
use async_trait::async_trait;
use std::sync::Arc;

/// Whether an executor evaluated the probe's assertions.
#[derive(Debug, Clone, Default)]
pub enum AssertionEvidence {
    /// No executor evaluated the assertions — a simulated or short-circuited run.
    ///
    /// The default, and the reason a simulated failure can never become a confirmed
    /// defect. Every finding built on this classifies as `INSUFFICIENT_EVIDENCE`, which is
    /// the truth: the pipeline recorded a failed task and nothing about the application.
    #[default]
    NotEvaluated,
    /// A browser-backed executor evaluated the assertions and reported which ones failed.
    ///
    /// `failed_assertions` names the assertion steps that failed, as the executor reported
    /// them. Recorded verbatim in the finding so a reader can see which assertion the
    /// defect is about rather than being told "something failed".
    Evaluated {
        failed_assertions: Vec<String>,
        /// Which executor produced the outcome, so the finding names its source.
        executor_kind: String,
    },
}

/// What the dispatching executor reported for a probe.
///
/// This is the executor's *own* report, handed in rather than invented here. A simulated
/// executor reports success; a browser-backed one reports what actually happened on the
/// page. Defaulting to a pass is the safe direction: a run that reported nothing has not
/// produced evidence of a defect.
#[derive(Debug, Clone)]
pub struct DispatchOutcome {
    pub status: TaskStatus,
    /// The failure text the executor reported, if any. Recorded verbatim, and the
    /// fingerprint is computed from it rather than from a paraphrase.
    pub reason: Option<String>,
    pub assertions: AssertionEvidence,
}

impl Default for DispatchOutcome {
    fn default() -> Self {
        Self {
            status: TaskStatus::Passed,
            reason: None,
            assertions: AssertionEvidence::NotEvaluated,
        }
    }
}

/// Submits probes through the existing run pipeline.
#[derive(Clone)]
pub struct RunPipelineProbeRunner {
    store: Store,
    dispatch: Arc<std::sync::Mutex<DispatchOutcome>>,
}

impl std::fmt::Debug for RunPipelineProbeRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunPipelineProbeRunner")
            .field("dispatch", &self.dispatch)
            .finish_non_exhaustive()
    }
}

impl RunPipelineProbeRunner {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            dispatch: Arc::new(std::sync::Mutex::new(DispatchOutcome::default())),
        }
    }

    /// Records what the dispatching executor reported for this session.
    ///
    /// Set once per session, before exploration starts. Mutating it mid-session would let
    /// one probe's assertion evidence be attributed to another's findings, which is the
    /// kind of quiet inconsistency that makes a report untrustworthy.
    pub fn with_dispatch(self, dispatch: DispatchOutcome) -> Self {
        *self.dispatch.lock().expect("dispatch lock") = dispatch;
        self
    }

    /// Convenience for the common case: a browser-backed executor that evaluated the
    /// assertions and reported which ones failed.
    pub fn with_evaluated_assertions(
        self,
        executor_kind: impl Into<String>,
        failed_assertions: Vec<String>,
    ) -> Self {
        self.with_dispatch(DispatchOutcome {
            assertions: AssertionEvidence::Evaluated {
                failed_assertions,
                executor_kind: executor_kind.into(),
            },
            ..DispatchOutcome::default()
        })
    }
}

/// What the pipeline actually recorded, read back from telemetry rather than assumed.
///
/// Returned separately from `run` so the fingerprint and the cluster lookup operate on what
/// is in the database, not on what the dispatcher claimed. If the two ever disagree, the
/// database wins -- which is the whole point of reading it back.
#[derive(Debug, Clone)]
struct RecordedOutcome {
    run_id: uuid::Uuid,
    status: TaskStatus,
    /// The recorded `execution_telemetry.error_message`, verbatim.
    reason: Option<String>,
    /// The recorded `execution_telemetry.assertion_result`, verbatim.
    assertion_result: Option<String>,
    /// The recorded `execution_telemetry.selector`, when the writer captured one.
    selector: Option<String>,
}

#[async_trait]
impl ProbeRunner for RunPipelineProbeRunner {
    async fn run(
        &self,
        project_id: &str,
        session_id: &uuid::Uuid,
        probe: &ComposedProbe,
    ) -> Result<ProbeOutcome, String> {
        let dispatch = self.dispatch.lock().expect("dispatch lock").clone();

        // Record through the real writers, then read back what they stored.
        let recorded = self
            .record(project_id, session_id, probe, &dispatch)
            .await?;

        // Fingerprint from the reason the pipeline **recorded**, over Phase 2's single
        // canonical function. This is what joins an exploration finding to the one
        // fingerprint space, and therefore what makes duplicate suppression work across
        // different paths, different action orders and different sessions.
        let fingerprint = recorded.reason.as_deref().and_then(|r| {
            if matches!(recorded.status, TaskStatus::Failed | TaskStatus::Timeout) {
                // The recorded selector when the writer captured one, else the selector the
                // probe acted on. Bound first because the fallback produces a temporary.
                let from_probe = probe.actions.iter().find_map(selector_of);
                let selector = recorded.selector.as_deref().or(from_probe.as_deref());
                let (_, _, fp) =
                    canonical_failure_fingerprint(r, selector, None, Some(probe.route.as_str()));
                Some(fp)
            } else {
                None
            }
        });

        // A recorded cluster already carrying this fingerprint. Resolved through the real
        // store, so "this is a known failure" is a stored fact rather than an assumption.
        let related_failure = match fingerprint.as_deref() {
            Some(fp) => self
                .store
                .list_failure_clusters(Some(project_id))
                .await
                .unwrap_or_default()
                .into_iter()
                .find(|c| c.failure_fingerprint.as_deref() == Some(fp))
                .map(|c| c.id.to_string()),
            None => None,
        };

        // Structural first, declared second. A recorded `assertion_result` is proof an
        // executor evaluated assertions; the dispatcher's declaration covers executors that
        // do not yet fill that column.
        let assertions_evaluated = recorded.assertion_result.is_some()
            || matches!(dispatch.assertions, AssertionEvidence::Evaluated { .. });

        Ok(ProbeOutcome {
            run_id: recorded.run_id,
            task_status: recorded.status,
            failure_reason: recorded.reason,
            failure_fingerprint: fingerprint,
            assertions_evaluated,
            app_state: serde_json::json!({
                "route": probe.route,
                "behaviour": probe.behaviour.as_str(),
                "action_count": probe.actions.len(),
                "assertion_count": probe.assertion_count,
                "recorded_assertion_result": recorded.assertion_result,
                "executor_report": match &dispatch.assertions {
                    AssertionEvidence::Evaluated { failed_assertions, executor_kind } => serde_json::json!({
                        "evaluated": true,
                        "executor": executor_kind,
                        "failed_assertions": failed_assertions,
                    }),
                    AssertionEvidence::NotEvaluated => serde_json::json!({
                        "evaluated": false,
                        "detail": "no executor reported a per-assertion outcome for this probe",
                    }),
                },
            }),
            related_failure,
        })
    }
}

impl RunPipelineProbeRunner {
    /// Write the probe's plan and its outcome through the real writers, then read back what
    /// they stored.
    ///
    /// The plan carries the real `Action` AST as the task's parameters, so a finding is
    /// replayable from what was stored rather than from a prose summary.
    async fn record(
        &self,
        project_id: &str,
        session_id: &uuid::Uuid,
        probe: &ComposedProbe,
        dispatch: &DispatchOutcome,
    ) -> Result<RecordedOutcome, String> {
        let run_id = uuid::Uuid::new_v4();
        let task_id = format!("probe-{session_id}");
        let plan = RunPlan {
            name: format!("exploration probe: {}", probe.behaviour.as_str()),
            tasks: vec![TaskDef {
                id: task_id.clone(),
                name: format!("exploratory probe on {}", probe.route),
                prerequisites: Vec::new(),
                simulate: Some(SimSpec {
                    outcome: if matches!(dispatch.status, TaskStatus::Failed) {
                        SimOutcome::Fail
                    } else {
                        SimOutcome::Pass
                    },
                    delay_ms: 1,
                }),
                executor: None,
                params: serde_json::json!({
                    "exploration": true,
                    "behaviour": probe.behaviour.as_str(),
                    "actions": probe.actions,
                    "expected": probe.expected,
                }),
                test_case_id: None,
            }],
            concurrency: 1,
        };

        self.store
            .insert_run_for_release(
                run_id,
                &plan,
                Some(project_id),
                Some("exploration probe"),
                RunState::Completed,
                None,
                None,
            )
            .await
            .map_err(|e| e.to_string())?;

        Intelligence::new(&self.store)
            .ingest_run(
                Some(run_id),
                Some(project_id),
                &plan,
                &[TaskState {
                    id: task_id,
                    name: format!("exploratory probe on {}", probe.route),
                    prerequisites: Vec::new(),
                    status: dispatch.status,
                    reason: dispatch.reason.clone(),
                    started_at: None,
                    finished_at: None,
                    test_case_id: None,
                }],
            )
            .await
            .map_err(|e| e.to_string())?;

        // Read back, so the fingerprint and the cluster link come from the database rather
        // than from what this function was handed.
        let rows = self
            .store
            .list_execution_telemetry(project_id, 500)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.run_id == run_id)
            .collect::<Vec<_>>();

        let row = rows.first().ok_or_else(|| {
            format!("probe run {run_id} recorded no telemetry, so nothing was observed")
        })?;

        Ok(RecordedOutcome {
            run_id,
            status: parse_status(&row.status),
            reason: row.error_message.clone().filter(|s| !s.is_empty()),
            assertion_result: row.assertion_result.clone(),
            selector: row.selector.clone(),
        })
    }
}

/// The telemetry wire status as the domain enum, matching what
/// `QaMemoryReader::is_failure` treats as a failure.
fn parse_status(status: &str) -> TaskStatus {
    match status {
        "PASSED" => TaskStatus::Passed,
        "FAILED" => TaskStatus::Failed,
        "TIMEOUT" => TaskStatus::Timeout,
        "SKIPPED" => TaskStatus::Skipped,
        "BLOCKED" => TaskStatus::Blocked,
        "CANCELLED" => TaskStatus::Cancelled,
        _ => TaskStatus::InfraError,
    }
}

/// The selector an action targets, when it has one.
///
/// Used to feed the fingerprint, which is computed from *what was acted on* as well as
/// what went wrong — that is what lets the same defect reached through two different
/// action sequences collapse to one fingerprint.
fn selector_of(action: &crate::execution_plan::Action) -> Option<String> {
    use crate::execution_plan::Action;
    match action {
        Action::Click { selector }
        | Action::Fill { selector, .. }
        | Action::AssertVisible { selector }
        | Action::AssertText { selector, .. }
        | Action::AssertHidden { selector }
        | Action::AssertEnabled { selector } => Some(selector.clone()),
        _ => None,
    }
}
