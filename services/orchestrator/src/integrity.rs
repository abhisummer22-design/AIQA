//! Phase 6 — Data Integrity + Cross-Layer Intelligence.
//!
//! ## What this is
//!
//! A **read-only** verifier that compares facts independently recorded by different parts of
//! this system, and reports where those recordings disagree.
//!
//! | Layer | Evidence | Real writer |
//! |---|---|---|
//! | `UI` | `crawler::CrawlOutput` — element text, selector, role visibility | the TypeScript crawler, read via `read_crawl_result` |
//! | `API` | `execution_telemetry` — status, `http_status`, `selector`, `assertion_result`, `failure_fingerprint`, `duration_ms` | `Intelligence::ingest_run` |
//! | `BACKEND` | `tasks` — status, reason, `started_at`, `finished_at`, `test_case_id` | `store::set_task_status`, from the engine |
//! | `DATABASE` | `runs`, `failure_clusters`, `evidence`, `risk_assessments` | `set_run_state`, `record_failure`, `insert_evidence`, `upsert_risk_assessment` |
//! | `AUDIT` | `audit_log`, `run_events` | `insert_audit`, `insert_event` |
//!
//! ## Read-only by construction, not by policy
//!
//! `Store` has **no arbitrary-SQL path at all** — every statement in it is fixed text with
//! bound parameters, and there is no query builder and no string-concatenated statement. So
//! "reuse the existing governed read mechanism" resolves to "use the existing readers", and
//! [`IntegrityVerifier`] holds a `&Store` and exposes only `&self` methods that call them.
//!
//! **No new store reader and no new table were needed.** Every observation comes from a reader
//! that already existed: `list_tasks_for_run`, `get_run_summary`, `list_events_since`,
//! `list_execution_telemetry`, `list_failure_clusters`, `list_audit`,
//! `list_evidence_for_run`.
//!
//! ## Identity, never similarity
//!
//! Values are never correlated because they look alike. An entity is identified only by a
//! **stored key** both sides recorded independently:
//!
//! | Source | Key | Corroborated by |
//! |---|---|---|
//! | [`IdentitySource::RunTaskKey`] | `(run_id, task_id)` | the primary key of `tasks`, and independently present on telemetry and events |
//! | [`IdentitySource::RunKey`] | `run_id` | `runs.id`, `run_events.run_id`, `evidence.run_id` |
//! | [`IdentitySource::FailureFingerprint`] | the Phase 2 canonical fingerprint | `failure_clusters.failure_fingerprint` |
//! | [`IdentitySource::TestCaseKey`] | `test_case_id` | `tasks.test_case_id`, `execution_telemetry.test_case_id` |
//! | [`IdentitySource::ObjectKey`] | `evidence.object_key` | the artifact readable from the evidence store |
//!
//! When a key resolves to **more than one** candidate the answer is
//! [`IntegrityOutcome::InsufficientData`] with the candidates named — never a guess. That is
//! the "two entities that happen to share a visible value" case.
//!
//! ## The four result states
//!
//! [`IntegrityOutcome::Inconsistent`] is the rare answer. Reaching it needs a resolved
//! identity, a real value on **both** sides, and timestamps inside the comparison window. A
//! mismatch on any weaker footing becomes `INSUFFICIENT_DATA` with the reason stated, because
//! a stale layer, an eventual-consistency gap and a fabricated constant all *look* like
//! disagreements.
//!
//! `defect_established` is stricter again: it additionally requires **both observations to be
//! measurements**. A constant is not a measurement, so a disagreement involving one can never
//! establish a product defect. That is not theoretical — see
//! [`IntegrityVerifier::fabricated_constant_check`], which exists because of a real finding in
//! `intelligence.rs`.
//!
//! ## No new subsystem
//!
//! No second evidence system, no second database abstraction, no second assertion engine, no
//! second execution engine, no second memory system.

use crate::domain::{RunId, RunState, TaskState, TaskStatus};
use crate::evidence::LocalEvidenceStore;
use crate::store::{ExecutionTelemetryRow, Store};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

// ============================================================================
// 1. THE MODEL
// ============================================================================

/// A system layer that independently records facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Layer {
    /// What the browser showed: crawled element text and role-restricted visibility.
    Ui,
    /// What the executor/HTTP boundary reported: `execution_telemetry`.
    Api,
    /// What the scheduler recorded in the domain: `tasks`.
    Backend,
    /// What the durable store holds: `runs`, `failure_clusters`, `evidence`, `risk_assessments`.
    Database,
    /// What the audit trail and event log recorded: `audit_log`, `run_events`.
    Audit,
}

impl Layer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ui => "UI",
            Self::Api => "API",
            Self::Backend => "BACKEND",
            Self::Database => "DATABASE",
            Self::Audit => "AUDIT",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "UI" => Some(Self::Ui),
            "API" => Some(Self::Api),
            "BACKEND" => Some(Self::Backend),
            "DATABASE" => Some(Self::Database),
            "AUDIT" => Some(Self::Audit),
            _ => None,
        }
    }

    /// Every layer, in the order the phase brief lists them.
    pub fn all() -> &'static [Layer] {
        &[
            Self::Ui,
            Self::Api,
            Self::Backend,
            Self::Database,
            Self::Audit,
        ]
    }
}

/// What a comparison concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegrityOutcome {
    /// Every layer that answered recorded the same value.
    Consistent,
    /// Two layers that both recorded a real value recorded **different** ones, inside the
    /// comparison window.
    Inconsistent,
    /// Not enough was recorded to answer. **The default, and by far the most common answer.**
    InsufficientData,
    /// The layers exist but cannot be compared for this entity or field — different subject
    /// matter, not missing evidence.
    NotComparable,
}

impl IntegrityOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Consistent => "CONSISTENT",
            Self::Inconsistent => "INCONSISTENT",
            Self::InsufficientData => "INSUFFICIENT_DATA",
            Self::NotComparable => "NOT_COMPARABLE",
        }
    }
}

/// Which kind of agreement was tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ComparisonKind {
    /// The same task's status, as the backend recorded it and as telemetry recorded it.
    StatusConsistency,
    /// The same quantity measured by two independent clocks.
    NumericConsistency,
    /// The same field's value, as two layers recorded it.
    FieldValueConsistency,
    /// The same entity across two executions.
    BeforeAfterState,
    /// What the browser showed against what the API reported.
    ApiUi,
    /// What the API reported against what the durable store holds.
    ApiDb,
    /// What the audit trail recorded against what actually ran.
    AuditTrailConsistency,
    /// The run's recorded state against the state its tasks imply.
    BusinessStateConsistency,
    /// A stored field that is a constant rather than a measurement.
    ///
    /// Distinct from the rest because it does not compare two *layers*: it compares what a
    /// column claims against how much the column actually varies. See
    /// [`IntegrityVerifier::fabricated_constant_check`].
    FabricatedConstant,
}

impl ComparisonKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StatusConsistency => "STATUS_CONSISTENCY",
            Self::NumericConsistency => "NUMERIC_CONSISTENCY",
            Self::FieldValueConsistency => "FIELD_VALUE_CONSISTENCY",
            Self::BeforeAfterState => "BEFORE_AFTER_STATE",
            Self::ApiUi => "API_UI",
            Self::ApiDb => "API_DB",
            Self::AuditTrailConsistency => "AUDIT_TRAIL_CONSISTENCY",
            Self::BusinessStateConsistency => "BUSINESS_STATE_CONSISTENCY",
            Self::FabricatedConstant => "FABRICATED_CONSTANT",
        }
    }
}

/// Where an identity came from. Only ever a stored key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IdentitySource {
    /// `(run_id, task_id)` — the primary key of `tasks`.
    RunTaskKey { run_id: Uuid, task_id: String },
    /// `run_id`.
    RunKey { run_id: Uuid },
    /// The Phase 2 canonical failure fingerprint.
    FailureFingerprint { fingerprint: String },
    /// `test_case_id`, as recorded on both `tasks` and telemetry.
    TestCaseKey { test_case_id: String },
    /// `evidence.object_key`.
    ObjectKey { object_key: String },
    /// A synthetic key for a project-scoped finding that is about the whole project rather
    /// than one entity — used only by [`ComparisonKind::FabricatedConstant`], which has no
    /// entity to point at.
    ProjectScope { project_id: String },
}

impl IdentitySource {
    /// The key rendered for storage and for a reader.
    pub fn key(&self) -> String {
        match self {
            Self::RunTaskKey { run_id, task_id } => format!("{run_id}/{task_id}"),
            Self::RunKey { run_id } => run_id.to_string(),
            Self::FailureFingerprint { fingerprint } => fingerprint.clone(),
            Self::TestCaseKey { test_case_id } => test_case_id.clone(),
            Self::ObjectKey { object_key } => object_key.clone(),
            Self::ProjectScope { project_id } => format!("project:{project_id}"),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::RunTaskKey { .. } => "run_task_key",
            Self::RunKey { .. } => "run_key",
            Self::FailureFingerprint { .. } => "failure_fingerprint",
            Self::TestCaseKey { .. } => "test_case_key",
            Self::ObjectKey { .. } => "object_key",
            Self::ProjectScope { .. } => "project_scope",
        }
    }
}

/// One layer's recording of one field of one entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub layer: Layer,
    pub entity: String,
    pub field: String,
    /// `None` when the layer recorded no value for this field, which is itself an answer.
    pub value: Option<Value>,
    pub observed_at: chrono::DateTime<chrono::Utc>,
    /// The execution context, when the layer recorded one.
    pub run_id: Option<Uuid>,
    /// A key into the evidence store, when this observation has an artifact.
    pub evidence_ref: Option<String>,
    /// What produced this observation, named so it can be checked.
    pub basis: String,
    /// **False when the value is a constant, a default, or otherwise not a measurement.**
    ///
    /// This is the flag that stops a fabricated zero from becoming a defect, and the reason
    /// `defect_established` is stricter than `INCONSISTENT`.
    pub is_measurement: bool,
    /// Why this value is not a measurement, when it is not one.
    pub measurement_caveat: Option<String>,
}

impl Observation {
    /// A real reading of a real field.
    ///
    /// `value` is optional because "this layer recorded no value here" is itself an answer,
    /// and forcing a placeholder for it would be the invented value this module refuses to
    /// produce.
    #[allow(clippy::too_many_arguments)]
    fn measured(
        layer: Layer,
        entity: &str,
        field: &str,
        value: Option<Value>,
        observed_at: chrono::DateTime<chrono::Utc>,
        run_id: Option<Uuid>,
        basis: &str,
    ) -> Self {
        Self {
            layer,
            entity: entity.to_string(),
            field: field.to_string(),
            value,
            observed_at,
            run_id,
            evidence_ref: None,
            basis: basis.to_string(),
            is_measurement: true,
            measurement_caveat: None,
        }
    }

    /// A value that exists but carries no information.
    #[allow(clippy::too_many_arguments)]
    fn not_measured(
        layer: Layer,
        entity: &str,
        field: &str,
        value: Option<Value>,
        observed_at: chrono::DateTime<chrono::Utc>,
        run_id: Option<Uuid>,
        basis: &str,
        caveat: &str,
    ) -> Self {
        Self {
            layer,
            entity: entity.to_string(),
            field: field.to_string(),
            value,
            observed_at,
            run_id,
            evidence_ref: None,
            basis: basis.to_string(),
            is_measurement: false,
            measurement_caveat: Some(caveat.to_string()),
        }
    }

    fn with_evidence(mut self, key: &str) -> Self {
        self.evidence_ref = Some(key.to_string());
        self
    }
}

/// Relative tolerance between two independently recorded durations.
///
/// Kept at the original 10 % so a genuine proportional divergence on a long-running test is still
/// reported.
pub const DURATION_RELATIVE_TOLERANCE: f64 = 0.10;

/// Absolute tolerance floor, in **milliseconds**.
///
/// ## Why this exists
///
/// The two durations compared here are **independently measured**, and neither is a copy of the
/// other:
///
/// * the backend's is `(tasks.finished_at - tasks.started_at)`, taken by `store::set_task_status`
///   stamping each end at the moment it performs that write;
/// * the API's is `execution_telemetry.duration_ms`, derived by
///   `ExecutionTelemetry::from_task_state` from the `TaskState` timestamps.
///
/// Two `now()` calls microseconds apart are therefore two different instants, and the difference
/// between them is scheduling noise rather than a cross-layer defect.
///
/// ## Why a *relative* tolerance alone is not enough
///
/// Both sides are integers in **milliseconds** — `num_milliseconds()` on one side, a `u64` column
/// on the other — so 1 ms is the smallest non-zero difference the data can express. Under a
/// relative tolerance alone, a 0 ms span is treated as *infinitely* sensitive: any non-zero
/// reading on the other side counts as a 100 % divergence. That made the verdict a function of
/// whether two writes happened to land in the same millisecond — precisely the scheduling
/// question a cross-layer verifier must not be answering.
///
/// ## Why 10 ms
///
/// 10 ms is ten times the 1 ms quantum, so it absorbs the inter-write gap without reaching any
/// duration a reader would call different: the existing genuine-disagreement case is 0 ms against
/// 5 000 ms, leaving a 500x margin. It is a stated policy constant, not a value fitted to any
/// observed dataset, and both components are reported in each comparison's detail.
pub const DURATION_ABSOLUTE_FLOOR_MS: f64 = 10.0;

/// What comparing two independently recorded durations concluded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DurationAgreement {
    /// The difference is within the effective tolerance — measurement noise.
    WithinTolerance {
        difference_ms: f64,
        tolerance_ms: f64,
    },
    /// The difference exceeds the effective tolerance — a real disagreement.
    BeyondTolerance {
        difference_ms: f64,
        tolerance_ms: f64,
    },
}

/// The effective tolerance for a pair of durations: the larger of the absolute floor and the
/// relative allowance.
///
/// The absolute floor governs while the measurements are small, which is exactly where a relative
/// fraction stops meaning anything. The relative allowance governs on long durations, where a flat
/// 10 ms would be finer than either clock resolves.
pub fn duration_tolerance_ms(backend_ms: f64, api_ms: f64) -> f64 {
    let relative_ms = DURATION_RELATIVE_TOLERANCE * backend_ms.abs().max(api_ms.abs());
    DURATION_ABSOLUTE_FLOOR_MS.max(relative_ms)
}

/// Compare two independently recorded durations.
///
/// The boundary is deliberate and inclusive: a difference **equal** to the effective tolerance is
/// treated as agreeing, so a boundary test can sit exactly on it and get a stated answer.
pub fn compare_durations(backend_ms: f64, api_ms: f64) -> DurationAgreement {
    let difference_ms = (backend_ms - api_ms).abs();
    let tolerance_ms = duration_tolerance_ms(backend_ms, api_ms);
    if difference_ms <= tolerance_ms {
        DurationAgreement::WithinTolerance {
            difference_ms,
            tolerance_ms,
        }
    } else {
        DurationAgreement::BeyondTolerance {
            difference_ms,
            tolerance_ms,
        }
    }
}

/// How far apart two observations may be in time and still be comparable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparisonWindow {
    /// The largest skew permitted between the two observations.
    pub max_skew: chrono::Duration,
}

impl Default for ComparisonWindow {
    fn default() -> Self {
        Self {
            // One minute: wide enough to absorb two writers committing independently, narrow
            // enough that "recorded an hour apart" is not read as a disagreement.
            max_skew: chrono::Duration::seconds(60),
        }
    }
}

/// The result of one comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityComparison {
    pub kind: ComparisonKind,
    pub identity: Option<IdentitySource>,
    pub entity: String,
    pub field: String,
    pub outcome: IntegrityOutcome,
    /// The observations compared, including any that declined to answer.
    pub observations: Vec<Observation>,
    /// One clause naming the evidence for the outcome.
    pub detail: String,
    /// The window applied, and the skew actually observed, when time was relevant.
    pub window: Option<ComparisonWindow>,
    pub observed_skew_ms: Option<i64>,
    /// Fraction of the layers that could have answered and did, 0.0–1.0.
    pub data_sufficiency: f64,
    /// Whether the evidence establishes a **product defect** — strictly harder than
    /// `INCONSISTENT`.
    pub defect_established: bool,
    /// Why `defect_established` is false, when it is. Never empty when false.
    pub defect_caveat: Option<String>,
    /// A Phase 1 trace chain key, when one links this finding to Requirement → Test →
    /// Execution. Absent rather than fabricated when no chain exists.
    pub trace_key: Option<String>,
}

impl IntegrityComparison {
    /// A result that declines to answer.
    fn unanswered(
        kind: ComparisonKind,
        entity: &str,
        field: &str,
        detail: String,
        identity: Option<IdentitySource>,
        observations: Vec<Observation>,
        expected_layers: usize,
    ) -> Self {
        let answered = observations.iter().filter(|o| o.value.is_some()).count();
        Self {
            kind,
            identity,
            entity: entity.to_string(),
            field: field.to_string(),
            outcome: IntegrityOutcome::InsufficientData,
            observations,
            detail,
            window: None,
            observed_skew_ms: None,
            data_sufficiency: if expected_layers == 0 {
                0.0
            } else {
                answered as f64 / expected_layers as f64
            },
            defect_established: false,
            defect_caveat: Some(
                "no product defect can be established from a comparison that did not answer"
                    .to_string(),
            ),
            trace_key: None,
        }
    }

    /// Whether this result claims a product defect. Read by the API and by any bug-forcing
    /// caller, so a bug record can never be created from an `INCONSISTENT` alone.
    pub fn establishes_defect(&self) -> bool {
        self.outcome == IntegrityOutcome::Inconsistent && self.defect_established
    }
}

/// A whole run's integrity report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    pub project_id: String,
    pub run_id: Uuid,
    pub comparisons: Vec<IntegrityComparison>,
    /// Counts per outcome, so a reader sees the shape without reading every entry.
    pub counts: BTreeMap<String, usize>,
    pub inconsistent: usize,
    /// Inconsistencies that also establish a product defect. Almost always fewer than
    /// `inconsistent`, and that gap is the point of the design.
    pub defects_established: usize,
    /// Layers that had evidence for this run.
    pub layers_observed: Vec<Layer>,
    /// Layers with no evidence at all, named rather than silently absent.
    pub layers_unavailable: Vec<Layer>,
    pub window: ComparisonWindow,
    /// Checks that could not be completed, for the one comparator that is deliberately
    /// best-effort.
    ///
    /// Empty in the normal case, because every *required* read now propagates as `Err` from
    /// [`IntegrityVerifier::verify_run`] instead. A non-empty list means the report is
    /// **incomplete**: the absence of that comparison is a failed read, not a clean layer.
    /// [`IntegrityReport::is_complete`] is the single check for it.
    #[serde(default)]
    pub read_failures: Vec<String>,
}

impl IntegrityReport {
    /// `true` when every check this report needed actually ran.
    ///
    /// This is the distinction P7-D1 removed by accident: with a closed pool the verifier
    /// returned a *successful* report with zero comparisons and zero defects, which a reader
    /// checking `defects_established == 0` could not tell from a run that agreed with itself.
    /// Required reads now fail loudly instead; this covers the one best-effort check that
    /// deliberately does not.
    pub fn is_complete(&self) -> bool {
        self.read_failures.is_empty()
    }
}

// ============================================================================
// 2. THE VERIFIER
// ============================================================================

/// Compares independently recorded facts across layers.
///
/// Holds a `&Store`, so it cannot write: there is no `&mut self` method, and every reader it
/// calls is an existing `&self` query with bound parameters.
pub struct IntegrityVerifier<'a> {
    store: &'a Store,
    evidence: Option<&'a LocalEvidenceStore>,
    window: ComparisonWindow,
}

impl<'a> IntegrityVerifier<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self {
            store,
            evidence: None,
            window: ComparisonWindow::default(),
        }
    }

    /// Attaches the evidence store, enabling the artifact-readability comparison.
    pub fn with_evidence(mut self, evidence: &'a LocalEvidenceStore) -> Self {
        self.evidence = Some(evidence);
        self
    }

    pub fn with_window(mut self, window: ComparisonWindow) -> Self {
        self.window = window;
        self
    }

    // ---------------------------------------------------------------- helpers

    /// Telemetry for one run, project-scoped.
    ///
    /// `list_execution_telemetry` is project-scoped, so a run id belonging to another tenant
    /// resolves to nothing rather than to another tenant's rows.
    /// Telemetry rows for one run.
    ///
    /// **Phase 7.1 (P7-D1).** This read used to end in `unwrap_or_default()`, so a storage
    /// fault produced an empty vector -- indistinguishable from a run that recorded no
    /// telemetry, and from a run that therefore had nothing to criticise. Five comparators
    /// route through here, so a single fault silenced five of the ten checks in
    /// [`Self::verify_run`] at once. The error now propagates, and the caller decides what an
    /// unreadable run means.
    async fn telemetry_for_run(
        &self,
        project_id: &str,
        run_id: RunId,
    ) -> anyhow::Result<Vec<ExecutionTelemetryRow>> {
        Ok(self
            .store
            .list_execution_telemetry(project_id, 20_000)
            .await?
            .into_iter()
            .filter(|r| r.run_id == run_id)
            .collect())
    }

    /// Skew between two observations, in milliseconds.
    fn skew(a: &Observation, b: &Observation) -> i64 {
        (a.observed_at - b.observed_at).num_milliseconds().abs()
    }

    /// Decide a two-observation comparison, applying the window rule in one place.
    ///
    /// Centralised so the window rule cannot be applied inconsistently between comparisons.
    fn decide_pair(
        &self,
        kind: ComparisonKind,
        identity: &IdentitySource,
        field: &str,
        a: Observation,
        b: Observation,
    ) -> IntegrityComparison {
        let entity_key = identity.key();
        let observations = vec![a, b];
        let skew = Self::skew(&observations[0], &observations[1]);

        // Time rule first: a disagreement outside the window is two readings taken too far
        // apart to compare, which is staleness rather than inconsistency.
        if skew > self.window.max_skew.num_milliseconds() {
            let mut c = IntegrityComparison::unanswered(
                kind,
                &entity_key,
                field,
                format!(
                    "the two layers recorded this {skew} ms apart, beyond the {} ms comparison \
                     window, so a differing value would be staleness rather than disagreement",
                    self.window.max_skew.num_milliseconds()
                ),
                Some(identity.clone()),
                observations,
                2,
            );
            c.window = Some(self.window);
            c.observed_skew_ms = Some(skew);
            return c;
        }

        let (Some(av), Some(bv)) = (observations[0].value.clone(), observations[1].value.clone())
        else {
            let missing = observations
                .iter()
                .find(|o| o.value.is_none())
                .map(|o| format!("{} did not record this field", o.layer.as_str()))
                .unwrap_or_else(|| "a layer did not record this field".to_string());
            return IntegrityComparison::unanswered(
                kind,
                &entity_key,
                field,
                format!("{missing}, so there is nothing to compare against {entity_key}"),
                Some(identity.clone()),
                observations,
                2,
            );
        };

        let agree = av == bv;
        let both_measurements = observations.iter().all(|o| o.is_measurement);

        // A defect needs both sides to be measurements. A constant, a default or a value with
        // no signal source cannot establish anything about the application under test.
        let (defect_established, defect_caveat) = if agree {
            (
                false,
                Some("the layers agree, so there is nothing to establish".to_string()),
            )
        } else if both_measurements {
            (true, None)
        } else {
            let who = observations
                .iter()
                .find(|o| !o.is_measurement)
                .and_then(|o| o.measurement_caveat.clone())
                .unwrap_or_else(|| "one side is not a measurement".to_string());
            (
                false,
                Some(format!(
                    "the layers disagree, but not both recorded a measurement ({who}), so this \
                     cannot establish a product defect"
                )),
            )
        };

        IntegrityComparison {
            kind,
            identity: Some(identity.clone()),
            entity: entity_key,
            field: field.to_string(),
            outcome: if agree {
                IntegrityOutcome::Consistent
            } else {
                IntegrityOutcome::Inconsistent
            },
            detail: if agree {
                format!(
                    "{} and {} both recorded {field} as {av}",
                    observations[0].layer.as_str(),
                    observations[1].layer.as_str()
                )
            } else {
                format!(
                    "{} recorded {field} as {av} while {} recorded it as {bv}",
                    observations[0].layer.as_str(),
                    observations[1].layer.as_str()
                )
            },
            observations,
            window: Some(self.window),
            observed_skew_ms: Some(skew),
            data_sufficiency: 1.0,
            defect_established,
            defect_caveat,
            trace_key: None,
        }
    }

    // ------------------------------------------------- 1. status: backend vs api

    /// The same task's status, as the backend recorded it and as telemetry recorded it.
    ///
    /// Written by different code paths at different times — `set_task_status` by the engine,
    /// `ingest_run` by the intelligence layer — so agreement is a real fact and disagreement
    /// is a real inconsistency rather than a tautology.
    pub async fn task_status_consistency(
        &self,
        project_id: &str,
        run_id: RunId,
    ) -> anyhow::Result<Vec<IntegrityComparison>> {
        // P7-D1: `.ok().flatten()` conflated "no such run" with "the read failed". Only the
        // first is an empty answer; the second is now an error the caller cannot mistake for one.
        let Some(run) = self.store.get_run_summary(run_id).await? else {
            return Ok(Vec::new());
        };
        if run.project_id.as_deref() != Some(project_id) {
            // Another tenant's run: no observations at all, rather than another tenant's rows.
            return Ok(Vec::new());
        }

        let tasks = self.store.list_tasks_for_run(run_id).await?;
        let telemetry = self.telemetry_for_run(project_id, run_id).await?;
        let mut out = Vec::new();

        for task in &tasks {
            let identity = IdentitySource::RunTaskKey {
                run_id,
                task_id: task.id.clone(),
            };
            let backend = Observation::measured(
                Layer::Backend,
                &identity.key(),
                "status",
                Some(json!(task.status.as_db_str())),
                task.finished_at
                    .or(task.started_at)
                    .unwrap_or_else(chrono::Utc::now),
                Some(run_id),
                "store::list_tasks_for_run <- tasks.status, written by store::set_task_status",
            );

            let rows: Vec<&ExecutionTelemetryRow> =
                telemetry.iter().filter(|r| r.task_id == task.id).collect();

            match rows.as_slice() {
                [] => out.push(IntegrityComparison::unanswered(
                    ComparisonKind::StatusConsistency,
                    &identity.key(),
                    "status",
                    "the backend recorded a status for this task but the API layer recorded no \
                     telemetry for it, so there is nothing to compare"
                        .to_string(),
                    Some(identity),
                    vec![backend],
                    2,
                )),
                // More than one telemetry row means the task was retried. That is a legitimate
                // state, not a mismatch, but it must not be silently collapsed to the first.
                [row] => {
                    let api = Observation::measured(
                        Layer::Api,
                        &identity.key(),
                        "status",
                        Some(json!(row.status.clone())),
                        row.created_at,
                        Some(run_id),
                        "store::list_execution_telemetry <- execution_telemetry.status, written \
                         by Intelligence::ingest_run",
                    );
                    out.push(self.decide_pair(
                        ComparisonKind::StatusConsistency,
                        &identity,
                        "status",
                        backend,
                        api,
                    ));
                }
                many => out.push(IntegrityComparison::unanswered(
                    ComparisonKind::StatusConsistency,
                    &identity.key(),
                    "status",
                    format!(
                        "the API layer recorded {} telemetry rows for this task (a retry), so \
                         there is no single status to compare against the backend's",
                        many.len()
                    ),
                    Some(IdentitySource::RunTaskKey {
                        run_id,
                        task_id: task.id.clone(),
                    }),
                    vec![backend],
                    2,
                )),
            }
        }
        Ok(out)
    }

    // ------------------------------------------------- 2. duration: two clocks

    /// The same task's duration, as the backend's clock and as telemetry recorded it.
    ///
    /// Two independent measurements of one quantity. A stated tolerance applies, because two
    /// clocks never agree exactly and demanding that they do would report every execution as a
    /// mismatch.
    pub async fn task_duration_consistency(
        &self,
        project_id: &str,
        run_id: RunId,
    ) -> anyhow::Result<Vec<IntegrityComparison>> {
        let tasks = self.store.list_tasks_for_run(run_id).await?;
        let telemetry = self.telemetry_for_run(project_id, run_id).await?;

        let mut out = Vec::new();
        for task in &tasks {
            let identity = IdentitySource::RunTaskKey {
                run_id,
                task_id: task.id.clone(),
            };
            let Some(row) = telemetry.iter().find(|r| r.task_id == task.id) else {
                continue;
            };
            let (Some(start), Some(finish)) = (task.started_at, task.finished_at) else {
                out.push(IntegrityComparison::unanswered(
                    ComparisonKind::NumericConsistency,
                    &identity.key(),
                    "duration_ms",
                    "the backend recorded no start/finish pair for this task, so its clock has \
                     no duration to contribute"
                        .to_string(),
                    Some(identity),
                    Vec::new(),
                    2,
                ));
                continue;
            };
            let backend_ms = (finish - start).num_milliseconds().max(0) as f64;

            let backend = Observation::measured(
                Layer::Backend,
                &identity.key(),
                "duration_ms",
                Some(json!(backend_ms)),
                finish,
                Some(run_id),
                "store::list_tasks_for_run <- tasks.started_at/finished_at",
            );
            let api = Observation::measured(
                Layer::Api,
                &identity.key(),
                "duration_ms",
                Some(json!(row.duration_ms as f64)),
                row.created_at,
                Some(run_id),
                "store::list_execution_telemetry <- execution_telemetry.duration_ms",
            );

            let mut c = self.decide_pair(
                ComparisonKind::NumericConsistency,
                &identity,
                "duration_ms",
                backend,
                api,
            );
            if c.outcome == IntegrityOutcome::Inconsistent {
                let a = c.observations[0].value.as_ref().and_then(Value::as_f64);
                let b = c.observations[1].value.as_ref().and_then(Value::as_f64);
                if let (Some(a), Some(b)) = (a, b) {
                    // `Unanswered` never reaches here, so the staleness rule in `decide_pair`
                    // keeps its precedence: a pair recorded too far apart to compare is reported
                    // as staleness and is never rescued by the tolerance below.
                    match compare_durations(a, b) {
                        DurationAgreement::WithinTolerance {
                            difference_ms,
                            tolerance_ms,
                        } => {
                            c.outcome = IntegrityOutcome::Consistent;
                            c.defect_established = false;
                            c.detail = format!(
                                "the two clocks differ by {difference_ms:.1} ms ({a} ms vs \
                                 {b} ms), within the {tolerance_ms:.1} ms effective tolerance \
                                 for two independently recorded measurements"
                            );
                            c.defect_caveat = Some(
                                "the difference is within the stated clock tolerance, so it is \
                                 measurement noise rather than a cross-layer disagreement"
                                    .to_string(),
                            );
                        }
                        DurationAgreement::BeyondTolerance {
                            difference_ms,
                            tolerance_ms,
                        } => {
                            c.detail = format!(
                                "the two clocks differ by {difference_ms:.1} ms ({a} ms vs \
                                 {b} ms), beyond the {tolerance_ms:.1} ms effective tolerance, \
                                 so this is a disagreement between two recorded measurements \
                                 rather than clock noise"
                            );
                        }
                    }
                }
            }
            out.push(c);
        }
        Ok(out)
    }

    // ------------------------------------------------- 3. reason: backend vs api

    /// The failure text for one task, as the backend recorded it and as telemetry did.
    ///
    /// Compared for **presence**, not equality: the two writers mask and truncate differently,
    /// so demanding character-identical text would report every failure as a mismatch. Both
    /// recording a reason, or both recording none, is the agreement that means something.
    pub async fn task_reason_consistency(
        &self,
        project_id: &str,
        run_id: RunId,
    ) -> anyhow::Result<Vec<IntegrityComparison>> {
        let tasks = self.store.list_tasks_for_run(run_id).await?;
        let telemetry = self.telemetry_for_run(project_id, run_id).await?;

        let mut out = Vec::new();
        for task in &tasks {
            let identity = IdentitySource::RunTaskKey {
                run_id,
                task_id: task.id.clone(),
            };
            let Some(row) = telemetry.iter().find(|r| r.task_id == task.id) else {
                continue;
            };
            let backend_reason = task.reason.as_deref().filter(|s| !s.is_empty());
            let api_reason = row.error_message.as_deref().filter(|s| !s.is_empty());
            let both = backend_reason.is_some() && api_reason.is_some();
            let neither = backend_reason.is_none() && api_reason.is_none();
            let failed = matches!(
                TaskStatus::from_db_str(&row.status),
                TaskStatus::Failed | TaskStatus::Timeout
            );

            let backend = Observation::measured(
                Layer::Backend,
                &identity.key(),
                "failure_reason",
                backend_reason.map(|s| json!(s)),
                task.finished_at.unwrap_or_else(chrono::Utc::now),
                Some(run_id),
                "store::list_tasks_for_run <- tasks.reason",
            );
            let api = Observation::measured(
                Layer::Api,
                &identity.key(),
                "failure_reason",
                api_reason.map(|s| json!(s)),
                row.created_at,
                Some(run_id),
                "store::list_execution_telemetry <- execution_telemetry.error_message",
            );
            let skew = Self::skew(&backend, &api);

            let outcome = if both || neither {
                IntegrityOutcome::Consistent
            } else if failed {
                // One side captured a reason and the other did not, for a task that failed.
                IntegrityOutcome::Inconsistent
            } else {
                IntegrityOutcome::NotComparable
            };
            let detail = match outcome {
                IntegrityOutcome::Consistent if both => {
                    "both layers recorded a failure reason for this task".to_string()
                }
                IntegrityOutcome::Consistent => {
                    "neither layer recorded a failure reason, which agrees".to_string()
                }
                IntegrityOutcome::Inconsistent => format!(
                    "the task failed but only one layer recorded why: backend={} api={}",
                    backend_reason.is_some(),
                    api_reason.is_some()
                ),
                _ => format!(
                    "the task did not fail (status {}), so the presence of a failure reason is \
                     not a comparable fact",
                    row.status
                ),
            };

            out.push(IntegrityComparison {
                kind: ComparisonKind::FieldValueConsistency,
                identity: Some(identity.clone()),
                entity: identity.key(),
                field: "failure_reason".to_string(),
                outcome,
                detail,
                observations: vec![backend, api],
                window: Some(self.window),
                observed_skew_ms: Some(skew),
                data_sufficiency: 1.0,
                // Presence disagreement says the two writers captured different amounts of
                // detail -- not that the application misbehaved.
                defect_established: false,
                defect_caveat: Some(
                    "a difference in how much failure detail two layers captured does not \
                     establish a product defect"
                        .to_string(),
                ),
                trace_key: None,
            });
        }
        Ok(out)
    }

    // ------------------------------------------------- 4. run state vs its tasks

    /// The run's recorded state against the state its tasks imply.
    ///
    /// A run recorded terminal while tasks are still queued or running is a durable-record
    /// inconsistency, derivable from stored rows alone.
    pub async fn run_state_consistency(
        &self,
        project_id: &str,
        run_id: RunId,
    ) -> anyhow::Result<Option<IntegrityComparison>> {
        // P7-D1: a failed read used to become `None` -- the same value as "not this tenant's
        // run" -- so the derived run state was silently never compared.
        let Some(run) = self.store.get_run_summary(run_id).await? else {
            return Ok(None);
        };
        if run.project_id.as_deref() != Some(project_id) {
            return Ok(None);
        }
        let tasks = self.store.list_tasks_for_run(run_id).await?;
        let identity = IdentitySource::RunKey { run_id };

        let open: Vec<&TaskState> = tasks
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    TaskStatus::Queued | TaskStatus::Running | TaskStatus::NeedsReview
                )
            })
            .collect();

        let recorded = Observation::measured(
            Layer::Database,
            &identity.key(),
            "state",
            Some(json!(run.state.as_db_str())),
            run.finished_at.unwrap_or(run.started_at),
            Some(run_id),
            "store::get_run_summary <- runs.state, written by store::set_run_state",
        );
        let implied_state = if open.is_empty() {
            "TERMINAL"
        } else {
            "IN_PROGRESS"
        };
        let implied = Observation::measured(
            Layer::Backend,
            &identity.key(),
            "state",
            Some(json!(implied_state)),
            run.finished_at.unwrap_or(run.started_at),
            Some(run_id),
            "derived from tasks.status via store::list_tasks_for_run",
        );

        // `RunState`'s terminal set taken from the enum rather than assumed: `AwaitingApproval`,
        // `Running` and `Waiting` are all still open.
        let terminal = matches!(
            run.state,
            RunState::Completed | RunState::Failed | RunState::Cancelled
        );
        let consistent = !(terminal && !open.is_empty());

        Ok(Some(IntegrityComparison {
            kind: ComparisonKind::BusinessStateConsistency,
            identity: Some(identity.clone()),
            entity: identity.key(),
            field: "state".to_string(),
            outcome: if consistent {
                IntegrityOutcome::Consistent
            } else {
                IntegrityOutcome::Inconsistent
            },
            detail: if consistent {
                format!(
                    "the run is recorded {:?} and {} task(s) remain open",
                    run.state,
                    open.len()
                )
            } else {
                format!(
                    "the run is recorded {:?} but {} task(s) are still queued or running: {:?}",
                    run.state,
                    open.len(),
                    open.iter().map(|t| t.id.clone()).collect::<Vec<_>>()
                )
            },
            observations: vec![recorded, implied],
            window: Some(self.window),
            observed_skew_ms: Some(0),
            data_sufficiency: 1.0,
            // Two views of the same durable record. A disagreement is a bookkeeping
            // inconsistency; it says nothing about the application under test.
            defect_established: false,
            defect_caveat: Some(
                "a run/task bookkeeping disagreement is a records inconsistency, not evidence \
                 about the application under test"
                    .to_string(),
            ),
            trace_key: None,
        }))
    }

    // ------------------------------------------------- 5. fingerprint: api vs db

    /// Every fingerprint telemetry recorded, against the clusters the database holds.
    ///
    /// The fingerprint is the identity, so this is also the duplicate-control surface Phase 2
    /// built: telemetry claiming a failure signature the database has never seen registered is
    /// a genuine cross-layer inconsistency.
    pub async fn fingerprint_registration(
        &self,
        project_id: &str,
        run_id: RunId,
    ) -> anyhow::Result<Vec<IntegrityComparison>> {
        let telemetry = self.telemetry_for_run(project_id, run_id).await?;
        let clusters = self.store.list_failure_clusters(Some(project_id)).await?;
        let known: BTreeMap<&str, &crate::store::FailureClusterRow> = clusters
            .iter()
            .filter_map(|c| c.failure_fingerprint.as_deref().map(|fp| (fp, c)))
            .collect();

        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for row in &telemetry {
            let Some(fp) = row.failure_fingerprint.as_deref().filter(|s| !s.is_empty()) else {
                continue;
            };
            let identity = IdentitySource::FailureFingerprint {
                fingerprint: fp.to_string(),
            };
            // The API layer's proposition is "a failure with this signature occurred" --
            // which is what it recorded. The database's proposition is "a cluster exists for
            // it". Those are *different questions*, so they are not compared for equality:
            // asking whether "a failure occurred" equals "a cluster exists" would report a
            // mismatch for every fingerprint in the system.
            //
            // What is compared is whether **both** layers know about the fingerprint. When the
            // database holds a cluster, both propositions hold and the answer is CONSISTENT.
            // When it holds none, the database is silent -- which is reported as
            // INCONSISTENT with the silence stated explicitly and only half the evidence
            // counted, never as a disagreement between two recorded values.
            let api = Observation::measured(
                Layer::Api,
                &identity.key(),
                "fingerprint_known",
                Some(json!(true)),
                row.created_at,
                Some(row.run_id),
                "execution_telemetry.failure_fingerprint, written by Intelligence::ingest_run \
                 via canonical_failure_fingerprint",
            );

            match known.get(fp) {
                None => out.push(IntegrityComparison {
                    kind: ComparisonKind::ApiDb,
                    identity: Some(identity.clone()),
                    entity: identity.key(),
                    field: "fingerprint_known".to_string(),
                    outcome: IntegrityOutcome::Inconsistent,
                    detail:
                        "telemetry recorded this failure fingerprint but the database holds no \
                         failure cluster for it, so the database is silent about a failure the \
                         API layer recorded; clustering depends on the cluster row existing"
                            .to_string(),
                    observations: vec![api],
                    window: None,
                    observed_skew_ms: None,
                    // One of the two layers has evidence. Stated rather than rounded up.
                    data_sufficiency: 0.5,
                    defect_established: false,
                    defect_caveat: Some(
                        "only one layer has evidence here, so this is an absence in the \
                         database rather than a disagreement between two observations"
                            .to_string(),
                    ),
                    trace_key: None,
                }),
                Some(cluster) => {
                    // One comparison per distinct fingerprint, however many telemetry rows
                    // carried it -- otherwise a common failure would dominate the report.
                    if !seen.insert(fp.to_string()) {
                        continue;
                    }
                    let db = Observation::measured(
                        Layer::Database,
                        &identity.key(),
                        "fingerprint_known",
                        Some(json!(true)),
                        cluster.last_seen_at,
                        Some(row.run_id),
                        &format!(
                            "failure_clusters row {} via store::list_failure_clusters",
                            cluster.id
                        ),
                    );
                    out.push(self.decide_pair(
                        ComparisonKind::ApiDb,
                        &identity,
                        "fingerprint_known",
                        api,
                        db,
                    ));
                }
            }
        }
        Ok(out)
    }

    // ------------------------------------------------- 6. event log vs backend

    /// The event log's record of a task's terminal status against the backend's.
    ///
    /// `run_events` and `tasks` are written independently, so agreement is a real cross-check
    /// of the audit trail.
    pub async fn event_log_consistency(
        &self,
        project_id: &str,
        run_id: RunId,
    ) -> anyhow::Result<Vec<IntegrityComparison>> {
        let Some(run) = self.store.get_run_summary(run_id).await? else {
            return Ok(Vec::new());
        };
        if run.project_id.as_deref() != Some(project_id) {
            return Ok(Vec::new());
        }
        let events = self.store.list_events_since(run_id, 0).await?;
        let tasks = self.store.list_tasks_for_run(run_id).await?;

        let mut out = Vec::new();
        for event in &events {
            let Some(task_id) = event.task_id.as_ref() else {
                continue;
            };
            let Some(event_status) = event.status else {
                continue;
            };
            let Some(task) = tasks.iter().find(|t| &t.id == task_id) else {
                continue;
            };
            // Only the last event per task decides the outcome; intermediate transitions are
            // not supposed to equal the final status.
            let is_last = !events
                .iter()
                .any(|e| e.task_id.as_ref() == Some(task_id) && e.sequence > event.sequence);
            if !is_last {
                continue;
            }

            let identity = IdentitySource::RunTaskKey {
                run_id,
                task_id: task_id.clone(),
            };
            let audit = Observation::measured(
                Layer::Audit,
                &identity.key(),
                "status",
                Some(json!(event_status.as_db_str())),
                event.timestamp,
                Some(run_id),
                "store::list_events_since <- run_events.status, written by store::insert_event",
            );
            let backend = Observation::measured(
                Layer::Backend,
                &identity.key(),
                "status",
                Some(json!(task.status.as_db_str())),
                task.finished_at.unwrap_or_else(chrono::Utc::now),
                Some(run_id),
                "store::list_tasks_for_run <- tasks.status",
            );
            out.push(self.decide_pair(
                ComparisonKind::AuditTrailConsistency,
                &identity,
                "status",
                audit,
                backend,
            ));
        }
        Ok(out)
    }

    // ------------------------------------------------- 7. audit trail vs reality

    /// Audit records naming a run, against runs that actually produced executions.
    ///
    /// Identity is the target string the audit row recorded, matched against telemetry's run
    /// ids — both written by real code. A target that is not a run id is skipped rather than
    /// compared, because the audit log covers actions of many kinds.
    pub async fn audit_trail_consistency(
        &self,
        project_id: &str,
        limit: i64,
    ) -> anyhow::Result<Vec<IntegrityComparison>> {
        let audit = self.store.list_audit(Some(project_id), limit, 0).await?;
        let telemetry = self
            .store
            .list_execution_telemetry(project_id, 20_000)
            .await?;

        let mut out = Vec::new();
        for row in &audit {
            let Some(target) = row.target.as_deref().filter(|s| !s.is_empty()) else {
                continue;
            };
            let Ok(run_id) = Uuid::parse_str(target) else {
                continue;
            };
            let identity = IdentitySource::RunKey { run_id };

            let observed = Observation::measured(
                Layer::Database,
                &identity.key(),
                "executions_recorded",
                Some(json!(telemetry.iter().any(|t| t.run_id == run_id))),
                row.occurred_at,
                Some(run_id),
                "store::list_execution_telemetry filtered to this run",
            );
            let audit_obs = Observation::measured(
                Layer::Audit,
                &identity.key(),
                "executions_recorded",
                Some(json!(true)),
                row.occurred_at,
                Some(run_id),
                &format!(
                    "audit_log row action={:?} outcome={:?} via store::list_audit",
                    row.action, row.outcome
                ),
            );

            let mut c = self.decide_pair(
                ComparisonKind::AuditTrailConsistency,
                &identity,
                "executions_recorded",
                audit_obs,
                observed,
            );
            // The audit layer recording something is not a measurement of the application's
            // behaviour, so this can never escalate to a product defect.
            c.defect_established = false;
            c.defect_caveat = Some(
                "an audit-trail gap is a records inconsistency; it says nothing about whether \
                 the application under test behaved correctly"
                    .to_string(),
            );
            out.push(c);
        }
        Ok(out)
    }

    // ------------------------------------------------- 8. evidence index vs store

    /// Evidence rows whose artifact is not actually readable.
    ///
    /// A genuine cross-layer check: the database claims an artifact exists, and the evidence
    /// store is asked. A row pointing at nothing means a later reader would follow the key and
    /// find silence.
    pub async fn evidence_index_consistency(
        &self,
        run_id: RunId,
    ) -> anyhow::Result<Vec<IntegrityComparison>> {
        // An unconfigured evidence store is genuine insufficient evidence (this verifier was
        // built without one), not a failed read, so it remains an empty answer.
        let Some(evidence_store) = self.evidence else {
            return Ok(Vec::new());
        };
        let rows = self.store.list_evidence_for_run(run_id).await?;
        let mut out = Vec::new();
        for row in rows {
            let identity = IdentitySource::ObjectKey {
                object_key: row.object_key.clone(),
            };
            let readable = evidence_store.read(&row.object_key).is_ok();
            let db = Observation::measured(
                Layer::Database,
                &identity.key(),
                "artifact_readable",
                Some(json!(true)),
                row.created_at,
                Some(run_id),
                &format!(
                    "evidence row {} claims object_key {:?} via store::list_evidence_for_run",
                    row.id, row.object_key
                ),
            );
            let store_obs = Observation::measured(
                Layer::Audit,
                &identity.key(),
                "artifact_readable",
                Some(json!(readable)),
                chrono::Utc::now(),
                Some(run_id),
                "crate::evidence::LocalEvidenceStore::read",
            );
            let mut c = self.decide_pair(
                ComparisonKind::FieldValueConsistency,
                &identity,
                "artifact_readable",
                db,
                store_obs,
            );
            c.defect_established = false;
            c.defect_caveat = Some(
                "a missing artifact is an evidence-retention problem, not a product defect"
                    .to_string(),
            );
            out.push(c);
        }
        Ok(out)
    }

    // ------------------------------------------------- 9. UI vs API

    /// What the browser showed against what the API reported, for one selector on one route.
    ///
    /// The comparison most likely to be wrong, and the one where identity matters most.
    /// Identity is the pair `(route, selector)` both writers recorded independently. If the
    /// crawl recorded that selector on **more than one** element for that route, the identity
    /// is ambiguous and the answer is `INSUFFICIENT_DATA` — two elements that happen to share
    /// a selector are not one entity, and correlating them would be the string-matching this
    /// module exists to avoid.
    pub async fn ui_api_consistency(
        &self,
        project_id: &str,
        run_id: RunId,
        crawl: &crate::crawler::CrawlOutput,
    ) -> anyhow::Result<Vec<IntegrityComparison>> {
        let telemetry = self.telemetry_for_run(project_id, run_id).await?;
        let surface = crate::agents::exploration::ExplorationSurface::from_crawl(crawl);

        let mut out = Vec::new();
        for route in &surface.routes {
            for selector in &route.elements {
                let Some(row) = telemetry
                    .iter()
                    .find(|r| r.selector.as_deref() == Some(selector.selector_hint.as_str()))
                else {
                    continue;
                };
                let entity = format!("{}/{}", route.url, selector.selector_hint);
                let candidates = route
                    .elements
                    .iter()
                    .filter(|e| e.selector_hint == selector.selector_hint)
                    .count();

                if candidates > 1 {
                    out.push(IntegrityComparison::unanswered(
                        ComparisonKind::ApiUi,
                        &entity,
                        "observed_text",
                        format!(
                            "the crawl recorded {candidates} elements with selector {:?} on \
                             {}, so the UI entity cannot be identified; correlating them would \
                             be matching on a value that merely looks the same",
                            selector.selector_hint, route.url
                        ),
                        None,
                        vec![Observation::not_measured(
                            Layer::Ui,
                            &entity,
                            "observed_text",
                            Some(json!(selector.text.clone())),
                            chrono::Utc::now(),
                            Some(run_id),
                            "crawler::CrawlOutput::CrawledElement",
                            "the UI entity is ambiguous: more than one crawled element shares \
                             this selector on this route",
                        )],
                        2,
                    ));
                    continue;
                }

                let identity = IdentitySource::RunTaskKey {
                    run_id,
                    task_id: row.task_id.clone(),
                };
                let ui = Observation::measured(
                    Layer::Ui,
                    &entity,
                    "observed_text",
                    Some(json!(selector.text.clone())),
                    chrono::Utc::now(),
                    Some(run_id),
                    "crawler::CrawlOutput::CrawledElement.text, written by the TypeScript \
                     crawler",
                )
                .with_evidence(&format!("crawl/{}", route.url));
                let api = Observation::measured(
                    Layer::Api,
                    &identity.key(),
                    "observed_text",
                    row.assertion_result
                        .as_ref()
                        .map(|a| json!(a.clone()))
                        .or_else(|| row.error_message.as_ref().map(|m| json!(m.clone()))),
                    row.created_at,
                    Some(run_id),
                    "execution_telemetry.assertion_result / error_message",
                );
                let mut c =
                    self.decide_pair(ComparisonKind::ApiUi, &identity, "observed_text", ui, api);
                // The crawl records no per-element timestamp, so the two sides are genuinely
                // not known to be contemporaneous. A mismatch here cannot be a defect: it may
                // simply be two different moments.
                if c.outcome == IntegrityOutcome::Inconsistent {
                    c.defect_established = false;
                    c.defect_caveat = Some(
                        "the crawl records no per-element timestamp, so the two observations \
                         cannot be shown to be contemporaneous; a mismatch here is not evidence \
                         of a product defect"
                            .to_string(),
                    );
                }
                out.push(c);
            }
        }
        Ok(out)
    }

    // ------------------------------------------------- 10. before / after

    /// One test case's recorded selector across two executions.
    ///
    /// The comparison is on **identity stability**: the same `test_case_id` exercised by the
    /// same recorded selector in both runs. A change is reported as an observation rather than
    /// a defect, because a test legitimately changing its selector between runs is normal test
    /// evolution.
    pub async fn before_after_state(
        &self,
        project_id: &str,
        test_case_id: &str,
    ) -> anyhow::Result<Option<IntegrityComparison>> {
        let all = self
            .store
            .list_execution_telemetry(project_id, 20_000)
            .await?;
        let mut rows: Vec<&ExecutionTelemetryRow> = all
            .iter()
            .filter(|r| r.test_case_id.map(|c| c.to_string()).as_deref() == Some(test_case_id))
            .collect();
        if rows.len() < 2 {
            return Ok(None);
        }
        rows.sort_by_key(|r| r.created_at);
        let before = rows[0];
        let after = rows[rows.len() - 1];
        if before.run_id == after.run_id {
            return Ok(None);
        }

        let identity = IdentitySource::TestCaseKey {
            test_case_id: test_case_id.to_string(),
        };
        let b = Observation::measured(
            Layer::Api,
            &identity.key(),
            "selector",
            before.selector.clone().map(|s| json!(s)),
            before.created_at,
            Some(before.run_id),
            "the earlier execution_telemetry row for this test_case_id",
        );
        let a = Observation::measured(
            Layer::Api,
            &identity.key(),
            "selector",
            after.selector.clone().map(|s| json!(s)),
            after.created_at,
            Some(after.run_id),
            "the later execution_telemetry row for this test_case_id",
        );
        let mut c = self.decide_pair(
            ComparisonKind::BeforeAfterState,
            &identity,
            "selector",
            b,
            a,
        );
        // Two runs of a test are not expected to be simultaneous, so the window does not apply;
        // the skew is still reported.
        c.window = None;
        c.defect_established = false;
        c.defect_caveat = Some(
            "a test changing its selector between runs is normal test evolution, not evidence \
             of a product defect"
                .to_string(),
        );
        Ok(Some(c))
    }

    // ------------------------------------------------- 11. the audit finding

    /// Stored columns that carry no information — and, crucially, **which kind** of
    /// nothing they carry.
    ///
    /// Two conditions look similar and mean opposite things:
    ///
    /// | Condition | Outcome | Meaning |
    /// |---|---|---|
    /// | every row holds the same value, e.g. `0.0` | `INCONSISTENT` | a number is being written that no measurement produced |
    /// | every row is `NULL` | `INSUFFICIENT_DATA` | the signal is honestly unmeasured — a stated gap, not a defect |
    ///
    /// This function existed because of a real finding: `intelligence.rs` used to write
    /// `risk_assessments.security_sensitivity`, `.business_criticality` and `.change_frequency`
    /// as the literal `0.0`, and used the first of them in the risk formula at weight 0.10 — so
    /// every stored score was understated by a fabricated zero. That was remediated; the column
    /// is now `NULL`, because no signal source exists for it.
    ///
    /// Both branches therefore remain reachable and both remain checked:
    ///
    /// * `business_criticality` and `security_sensitivity` have **no signal source**, so `NULL`
    ///   is the correct state and this reports it as a coverage gap — with the weight the
    ///   formula places on it, which is the actionable part.
    /// * `change_frequency` **does** have a source since Phase 3, so it must vary across nodes.
    ///   If it ever becomes constant again, that is a fabrication and is reported as
    ///   `INCONSISTENT`.
    ///
    /// Phase 4's report described `security_sensitivity` as having "no column and no table
    /// anywhere". That is not correct — the column exists and is written. What was missing was a
    /// **signal source**, which is a different problem with a different fix. See
    /// `docs/next-intelligence/PHASE_4_RISK_SIGNAL_REMEDIATION.md`; the Phase 4 report is left
    /// as written, because it records what was known at the time.
    pub async fn fabricated_constant_check(
        &self,
        project_id: &str,
    ) -> Result<Vec<IntegrityComparison>, String> {
        /// The columns, the weight `intelligence.rs`'s formula places on each, and whether a
        /// signal source exists. Transcribed so a change to any of the three is visible here.
        const COLUMNS: [(&str, &str, f64, bool, usize); 3] = [
            // (column, table, weight in the risk formula, has a signal source, tuple index)
            ("security_sensitivity", "risk_assessments", 0.10, false, 0),
            ("business_criticality", "risk_assessments", 0.00, false, 1),
            ("change_frequency", "risk_assessments", 0.00, true, 2),
        ];

        // Through the store's reader, not a query of our own: `Store` is the governed read
        // surface, and this module holding SQL would break the property it is built on.
        let rows = self
            .store
            .list_risk_assessment_constants(project_id)
            .await
            .map_err(|e| e.to_string())?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        for (column, table, weight, has_source, index) in COLUMNS {
            let values: Vec<Option<f64>> = rows.iter().map(|r| [r.0, r.1, r.2][index]).collect();

            // Two distinct "no information" conditions.
            let measured_count = values.iter().filter(|v| v.is_some()).count();
            let distinct: BTreeSet<u64> = values.iter().flatten().map(|v| v.to_bits()).collect();

            let (outcome, detail, caveat): (IntegrityOutcome, String, &str) = if measured_count == 0
            {
                if has_source {
                    (
                        IntegrityOutcome::Inconsistent,
                        format!(
                            "{table}.{column} has a signal source but is NULL on all {} \
                                 assessment(s), so a measurement that should exist is missing",
                            values.len()
                        ),
                        "a column with a source cannot be unmeasured on every row without \
                             something being wrong with its writer",
                    )
                } else {
                    (
                        IntegrityOutcome::InsufficientData,
                        format!(
                            "{table}.{column} is NULL on all {} assessment(s) because no \
                                 signal source for it exists in this schema; the risk formula \
                                 weights it at {weight:.2}, so that share of the model is \
                                 unbacked and the score is normalised over the remaining weight",
                            values.len()
                        ),
                        "an unmeasured signal is a stated gap, not a defect, and no bug \
                             record should be raised against the application from it",
                    )
                }
            } else if !has_source && distinct.len() == 1 {
                // A number, in a column **nothing measures**. That is fabrication by
                // definition, however plausible the number looks.
                //
                // The `!has_source` guard is what makes this branch correct rather than merely
                // suspicious. A column that *does* have a source can legitimately read `0.0`
                // on every row — "no declared change touched this node" is a real measurement,
                // and flagging it would be precisely the conflation of measured-zero with
                // unmeasured that this remediation exists to remove.
                (
                    IntegrityOutcome::Inconsistent,
                    format!(
                        "{table}.{column} holds exactly one distinct value across all {} \
                         measured assessment(s) and **no signal source exists for it**, so the \
                         number was supplied rather than measured; a formula weighting it at \
                         {weight:.2} is being fed a fabricated input",
                        measured_count
                    ),
                    "written as a constant rather than measured from any signal source, so it \
                     must not be treated as a measurement",
                )
            } else {
                // Either it varies across rows, or it is a real measurement that legitimately
                // reads zero. Either way it carries information, and nothing is reported.
                continue;
            };

            let entity = format!("{table}.{column}");
            let identity = IdentitySource::ProjectScope {
                project_id: project_id.to_string(),
            };
            let only = values.iter().flatten().copied().next();
            out.push(IntegrityComparison {
                kind: ComparisonKind::FabricatedConstant,
                identity: Some(identity),
                entity: entity.clone(),
                field: column.to_string(),
                outcome,
                detail,
                observations: vec![Observation::not_measured(
                    Layer::Database,
                    &entity,
                    column,
                    only.map(|v| json!(v)),
                    chrono::Utc::now(),
                    None,
                    "values counted across risk_assessments for this project",
                    caveat,
                )],
                window: None,
                observed_skew_ms: None,
                data_sufficiency: measured_count as f64 / values.len() as f64,
                // A constant and a NULL are both not-measurements, so by construction this can
                // never establish a product defect. The finding is about AIQA's own scoring.
                defect_established: false,
                defect_caveat: Some(
                    "this is a finding about AIQA's own scoring, not about the application under \
                     test; no bug record should be raised against the application from it"
                        .to_string(),
                ),
                trace_key: None,
            });
        }
        Ok(out)
    }

    // ------------------------------------------------- the report

    /// Verify everything comparable for one run.
    pub async fn verify_run(
        &self,
        project_id: &str,
        run_id: RunId,
        crawl: Option<&crate::crawler::CrawlOutput>,
    ) -> anyhow::Result<IntegrityReport> {
        // P7-D1. Every read below used to end in `unwrap_or_default()` / `.ok().flatten()`,
        // so a storage fault produced an *empty* comparison list. An empty list is also what a
        // genuinely clean run produces, and `defects_established == 0` is true of both -- so a
        // failed verification was indistinguishable from a passing one, and a closed pool
        // produced a confident all-clear. Required reads now propagate: if storage cannot be
        // read, the caller learns that instead of receiving a report that says nothing is wrong.
        let mut comparisons = Vec::new();
        comparisons.extend(self.task_status_consistency(project_id, run_id).await?);
        comparisons.extend(self.task_duration_consistency(project_id, run_id).await?);
        comparisons.extend(self.task_reason_consistency(project_id, run_id).await?);
        comparisons.extend(self.fingerprint_registration(project_id, run_id).await?);
        comparisons.extend(self.event_log_consistency(project_id, run_id).await?);
        comparisons.extend(self.evidence_index_consistency(run_id).await?);
        comparisons.extend(self.audit_trail_consistency(project_id, 200).await?);

        // The one deliberately best-effort check, and the reason it stays that way: the eight
        // comparisons above are independent of it, and discarding all of them because an optional
        // reader hiccupped would lose more information than it protects. But "independent" is not
        // the same as "invisible" -- a failed read is recorded in the report rather than dropped,
        // which is exactly the vanishing P7-D1 was about.
        let mut read_failures: Vec<String> = Vec::new();
        match self.fabricated_constant_check(project_id).await {
            Ok(found) => comparisons.extend(found),
            Err(e) => read_failures.push(format!("fabricated_constant_check: {e}")),
        }

        if let Some(c) = self.run_state_consistency(project_id, run_id).await? {
            comparisons.push(c);
        }
        if let Some(c) = crawl {
            comparisons.extend(self.ui_api_consistency(project_id, run_id, c).await?);
        }

        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for c in &comparisons {
            *counts.entry(c.outcome.as_str().to_string()).or_insert(0) += 1;
        }
        let inconsistent = comparisons
            .iter()
            .filter(|c| c.outcome == IntegrityOutcome::Inconsistent)
            .count();
        let defects_established = comparisons
            .iter()
            .filter(|c| c.establishes_defect())
            .count();

        let mut layers_observed: Vec<Layer> = Vec::new();
        let mut layers_unavailable: Vec<Layer> = Vec::new();
        for layer in Layer::all() {
            if comparisons
                .iter()
                .any(|c| c.observations.iter().any(|o| o.layer == *layer))
            {
                layers_observed.push(*layer);
            } else {
                layers_unavailable.push(*layer);
            }
        }

        Ok(IntegrityReport {
            project_id: project_id.to_string(),
            run_id,
            comparisons,
            counts,
            inconsistent,
            defects_established,
            layers_observed,
            layers_unavailable,
            window: self.window,
            read_failures,
        })
    }
}

/// Attach a Phase 1 trace key to a comparison, when a chain links it.
///
/// The link is by `test_case_id` — the one key both the trace graph and this module's
/// observations hold — and the value is a readable summary of the entry rather than an
/// identifier, because `TraceIndexEntry` describes which hops have evidence and which do not.
/// A comparison with no chain, or a chain with no case behind it, is returned **unchanged**:
/// the absence of a chain is a fact and must not be filled in.
pub fn with_trace_key(
    mut comparison: IntegrityComparison,
    index: &crate::trace::TraceIndex,
    test_case_id: &str,
) -> IntegrityComparison {
    if let Some(entry) = index
        .test_cases
        .iter()
        .find(|e| e.test_case_id.as_deref() == Some(test_case_id))
    {
        comparison.trace_key = Some(format!(
            "{}{}",
            test_case_id,
            if entry.resolved {
                ""
            } else {
                " (unresolved in the graph)"
            }
        ));
    }
    comparison
}

/// A one-line human summary, for a log or a CLI.
pub fn summarise(c: &IntegrityComparison) -> String {
    format!(
        "[{}] {} {} :: {} — {}",
        c.outcome.as_str(),
        c.kind.as_str(),
        c.entity,
        c.field,
        c.detail
    )
}

/// The evidence keys a comparison rests on.
pub fn evidence_refs(c: &IntegrityComparison) -> Vec<&str> {
    c.observations
        .iter()
        .filter_map(|o| o.evidence_ref.as_deref())
        .collect()
}
