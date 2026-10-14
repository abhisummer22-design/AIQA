//! Phase 8 — Test Case Quality Intelligence.
//!
//! The roadmap asks: *"measure whether generated and saved test cases are useful, traceable,
//! unambiguous, non-duplicative, and executable"*, and then constrains the answer hard —
//! *"only score dimensions that can be measured honestly"*, *"distinguish `SCORE_AVAILABLE` from
//! `INSUFFICIENT_DATA`"*, *"explain every score and deduction"*, and *"do not create a second
//! test-generation or validation engine"*.
//!
//! ## What this module is, and what it deliberately is not
//!
//! It produces a **per-dimension assessment with its evidence**, not a number.
//!
//! There is **no aggregate quality score**, and that is a decision, not an omission. The roadmap
//! defines eight dimensions and eight scoring rules but defines **no weights and no aggregation
//! formula** for them. Inventing one would be exactly the fabricated numerical score the brief
//! forbids, and its weights would have no authority behind them. Scoring rule 3 asks for every
//! score to be explainable; a made-up weighted mean is not.
//!
//! ## Missing evidence
//!
//! Absence of evidence is never a defect and never a perfect score. A case with no execution
//! history, no embeddings and no requirement link reads `InsufficientData`, not `Adequate`. A
//! dimension with no authoritative source in this schema reads `NotAssessed` and names the source
//! that is missing.
//!
//! ## Reuse (roadmap rule 8)
//!
//! Every dimension reads through machinery that already exists. Nothing here re-implements a
//! validator, a similarity function, or a trace lookup:
//!
//! | dimension | reused source |
//! |---|---|
//! | requirement linkage | `quality_relations` (`covers`), `requirements`, the case's own columns |
//! | expected result | `api::validate_test_case_request`'s creation contract |
//! | concrete steps | `execution_plan::compile_deterministic` |
//! | duplicate similarity | `ai_generation`'s cosine over `test_case_embeddings` |
//! | automation feasibility | `ExecutorRegistry::registered_kinds` |
//! | change relevance | `Store::count_distinct_changes_for_node` |
//! | execution reliability | `QaMemoryReader`'s Phase 7.4 helpers |
//!
//! ## D3 — `failure_severity` (OPEN)
//!
//! **This module never reads a `failure_severity` value, from either implementation.** Quality is
//! a property of the case *definition* plus its traceability and duplicates; product-failure
//! severity is a property of an execution outcome. Conflating them is precisely the error D3's
//! unresolved divergence invites, so the dependency is avoided rather than guessed at.
//!
//! Execution outcomes are read only through `QaMemoryReader`'s established helpers, which exclude
//! `INFRA_ERROR` from both halves of the completed rate and report it separately. Infrastructure
//! trouble is therefore visible without ever being read as a product-test defect.

use crate::store::{Store, TestCaseRow};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Cosine similarity at or above which two embedded cases are reported as near-duplicates.
///
/// Matches the band `ai_generation::rank_similar_test_cases` already treats as a strong match at
/// generation time, so Phase 8 introduces no second, contradictory idea of "similar". It is a
/// reporting threshold over an existing measurement, not a risk weight.
pub const NEAR_DUPLICATE_SIMILARITY: f64 = 0.9;

/// How a single quality dimension came out.
///
/// `InsufficientData` and `NotAssessed` are deliberately distinct, because they are different
/// facts:
///
/// * **`InsufficientData`** — the source *exists*, but this case has not produced enough of it.
///   A project with no embeddings genuinely cannot be assessed for duplication.
/// * **`NotAssessed`** — the source *does not exist in this schema*. Negative/boundary coverage
///   "where the requirement supports it" needs to read meaning out of prose; no field carries it,
///   so it is reported as unanswerable rather than guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QualityVerdict {
    /// Measured, and it meets the rule.
    Adequate,
    /// Measured, and it does not meet the rule. Only ever used when a real source was read.
    Weak,
    /// The source exists but this case has not produced enough of it.
    InsufficientData,
    /// No authoritative source exists for this dimension in this schema.
    NotAssessed,
}

impl QualityVerdict {
    /// Whether this verdict came from reading a real source, as opposed to not having one.
    pub fn is_measurement(self) -> bool {
        matches!(self, Self::Adequate | Self::Weak)
    }
}

/// One dimension's assessment, with the evidence it was read from.
///
/// `Serialize` only, deliberately: `key` and `dimension` are `&'static str`, which is what makes
/// them cheap to match against and impossible to mistype. These values are produced by
/// [`assess_case`], never read back from an untrusted source, so `Deserialize` would buy nothing
/// and would force the keys to become owned `String`s.
#[derive(Debug, Clone, Serialize)]
pub struct QualityDimension {
    /// Stable machine-readable key. Never a display string.
    pub key: &'static str,
    /// The roadmap dimension this answers.
    pub dimension: &'static str,
    pub verdict: QualityVerdict,
    /// A one-line explanation a human can act on. Never a bare number.
    pub summary: String,
    /// The specific evidence the verdict was read from — ids, counts, or the compiler's own words.
    pub evidence: String,
    /// True when the rule was deterministic. The roadmap requires any heuristic to be labelled as
    /// such with a confidence; Phase 8 ships none, and this field would be where one was labelled.
    pub deterministic: bool,
}

/// A case's quality assessment. Deliberately contains **no** aggregate score.
///
/// `Serialize` only, for the same reason as [`QualityDimension`].
#[derive(Debug, Clone, Serialize)]
pub struct TestCaseQuality {
    pub test_case_id: String,
    pub project_id: String,
    pub title: Option<String>,
    pub status: String,
    pub dimensions: Vec<QualityDimension>,
    /// Why there is no aggregate number. Stated on the object itself, so a consumer never has to
    /// discover the absence by wondering where the number went.
    pub aggregate_basis: String,
}

impl TestCaseQuality {
    /// The honest combination, for callers that want a single status.
    ///
    /// `Weak` if any dimension is measurably weak. Otherwise `Adequate` only if every dimension
    /// was *measured* and adequate. If anything was unmeasured or unassessable the answer is
    /// `InsufficientData` — "nothing was found wrong" is not the claim "everything was checked
    /// and is fine".
    pub fn overall(&self) -> QualityVerdict {
        if self
            .dimensions
            .iter()
            .any(|d| d.verdict == QualityVerdict::Weak)
        {
            return QualityVerdict::Weak;
        }
        if self
            .dimensions
            .iter()
            .all(|d| d.verdict == QualityVerdict::Adequate)
        {
            return QualityVerdict::Adequate;
        }
        QualityVerdict::InsufficientData
    }

    pub fn get(&self, key: &str) -> Option<&QualityDimension> {
        self.dimensions.iter().find(|d| d.key == key)
    }

    pub fn measured_count(&self) -> usize {
        self.dimensions
            .iter()
            .filter(|d| d.verdict.is_measurement())
            .count()
    }

    pub fn weak_count(&self) -> usize {
        self.dimensions
            .iter()
            .filter(|d| d.verdict == QualityVerdict::Weak)
            .count()
    }

    /// Dimensions with no usable evidence, for a caller's "what is still unknown" view.
    pub fn unknown_keys(&self) -> Vec<&'static str> {
        self.dimensions
            .iter()
            .filter(|d| !d.verdict.is_measurement())
            .map(|d| d.key)
            .collect()
    }
}

/// Inputs beyond the store.
///
/// `executor_kinds` is what `ExecutorRegistry::registered_kinds()` returns — the executors this
/// deployment actually has. Passing it in keeps the assessment free of hidden global state and
/// keeps project isolation a matter of the caller.
pub struct QualityInputs<'a> {
    pub executor_kinds: &'a [String],
}

/// Assess one case, reading every source inside the caller's already-resolved tenant.
///
/// `project_id` must be the resolved tenant key. Every store read below is scoped by it, so a
/// case can never be assessed against another project's evidence.
pub async fn assess_case(
    store: &Store,
    case: &TestCaseRow,
    project_id: &str,
    inputs: &QualityInputs<'_>,
) -> TestCaseQuality {
    let dims = vec![
        requirement_linkage(store, case, project_id).await,
        expected_result_present(case),
        concrete_steps(case),
        duplicate_similarity(store, case, project_id).await,
        automation_feasibility(inputs),
        change_relevance(store, case, project_id).await,
        negative_boundary_coverage(),
        execution_reliability(store, case, project_id).await,
    ];

    TestCaseQuality {
        test_case_id: case.id.to_string(),
        project_id: project_id.to_string(),
        title: Some(case.title.clone()),
        status: format!("{:?}", case.status),
        dimensions: dims,
        aggregate_basis:
            "No aggregate quality score is produced. The Phase 8 roadmap defines eight dimensions \
             and eight scoring rules but no weights and no aggregation formula, so any single \
             number would have no authority behind it. Use `overall()` for a status and `dimensions` \
             for why."
                .to_string(),
    }
}

/// Assess every case in a project, in the store's own order.
pub async fn assess_project(
    store: &Store,
    project_id: &str,
    inputs: &QualityInputs<'_>,
) -> Vec<TestCaseQuality> {
    match store.list_test_cases(project_id, None, None).await {
        Ok(cases) => {
            let mut out: Vec<TestCaseQuality> = Vec::with_capacity(cases.len());
            for case in &cases {
                out.push(assess_case(store, case, project_id, inputs).await);
            }
            out
        }
        Err(_) => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// The `test` vertex for a case, and its graph edges
// ---------------------------------------------------------------------------

async fn test_node_id(
    store: &Store,
    project_id: &str,
    case_id: Uuid,
) -> Result<Option<Uuid>, String> {
    let nodes = store
        .list_quality_nodes(project_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(nodes
        .into_iter()
        .find(|n| {
            n.node_type == crate::store::TEST_NODE_TYPE
                && n.external_ref.as_deref() == Some(case_id.to_string().as_str())
        })
        .map(|n| n.id))
}

// ---------------------------------------------------------------------------
// Dimension 1 — requirement linkage / coverage
// ---------------------------------------------------------------------------

/// Is this case traceable to a real requirement?
///
/// Sources, all authoritative: the `covers` edge in the quality graph, the case's own
/// `requirement_reference` / `source_requirement` columns, and the `requirements` table a declared
/// reference must resolve to.
///
/// The distinction that matters: a reference that resolves to **no** requirement row is a broken
/// link — a measured defect. A case with **no reference at all** is also measured (the row
/// definitively lacks traceability), so it is `Weak` for that reason rather than
/// `InsufficientData`: the absence is a fact about the case, not missing evidence about the world.
async fn requirement_linkage(
    store: &Store,
    case: &TestCaseRow,
    project_id: &str,
) -> QualityDimension {
    let key = "requirement_linkage";
    let dimension = "requirement linkage / coverage";

    let node = match test_node_id(store, project_id, case.id).await {
        Ok(n) => n,
        Err(e) => {
            return QualityDimension {
                key,
                dimension,
                verdict: QualityVerdict::InsufficientData,
                summary: "the quality graph could not be read, so linkage is unknown".to_string(),
                evidence: format!("store read failed: {e}"),
                deterministic: true,
            }
        }
    };

    let has_covers_edge = match (
        node,
        store.list_quality_relations_for_project(project_id).await,
    ) {
        (Some(n), Ok(rels)) => rels
            .iter()
            .any(|r| r.relation_type == "covers" && (r.from_node_id == n || r.to_node_id == n)),
        _ => false,
    };

    let declared: Vec<String> = [
        case.requirement_reference.as_deref(),
        case.source_requirement.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(str::to_string)
    .collect();

    let evidence = format!(
        "requirement_reference={:?}, source_requirement={:?}, covers_edge={has_covers_edge}",
        case.requirement_reference, case.source_requirement
    );

    if declared.is_empty() && !has_covers_edge {
        return QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::Weak,
            summary:
                "the case declares no requirement and has no `covers` edge in the quality graph, \
                 so nothing traces it to a requirement"
                    .to_string(),
            evidence,
            deterministic: true,
        };
    }

    if has_covers_edge {
        return QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::Adequate,
            summary:
                "the case traces to a requirement through a `covers` edge in the quality graph"
                    .to_string(),
            evidence,
            deterministic: true,
        };
    }

    // Declared references must resolve against the project's own requirements. The document ids
    // are discovered from the store rather than assumed, so a reference into any of the project's
    // real documents can resolve.
    let documents: Vec<Uuid> = match store.list_requirement_documents(project_id).await {
        Ok(docs) => docs.into_iter().map(|d| d.id).collect(),
        Err(e) => {
            return QualityDimension {
                key,
                dimension,
                verdict: QualityVerdict::InsufficientData,
                summary: "requirement documents could not be read, so the link cannot be resolved"
                    .to_string(),
                evidence: format!("store read failed: {e}"),
                deterministic: true,
            }
        }
    };

    let requirements = match store.list_requirements(project_id, &documents).await {
        Ok(r) => r,
        Err(e) => {
            return QualityDimension {
                key,
                dimension,
                verdict: QualityVerdict::InsufficientData,
                summary: "requirements could not be read, so the link cannot be resolved"
                    .to_string(),
                evidence: format!("store read failed: {e}"),
                deterministic: true,
            }
        }
    };

    if requirements.is_empty() {
        return QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::InsufficientData,
            summary:
                "the case declares a requirement reference, but this project has no requirement \
                 rows to resolve it against, so the link cannot be checked either way"
                    .to_string(),
            evidence: format!("{evidence}; requirements=0"),
            deterministic: true,
        };
    }

    let resolved: Vec<&String> = declared
        .iter()
        .filter(|d| requirements.iter().any(|r| r.key.eq_ignore_ascii_case(d)))
        .collect();
    let unresolved: Vec<&String> = declared
        .iter()
        .filter(|d| !requirements.iter().any(|r| r.key.eq_ignore_ascii_case(d)))
        .collect();

    if !resolved.is_empty() {
        QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::Adequate,
            summary: format!(
                "{} of the case's {} declared requirement reference(s) resolve to a real \
                 requirement row",
                resolved.len(),
                declared.len()
            ),
            evidence: format!("{evidence}; resolved={resolved:?}; unresolved={unresolved:?}"),
            deterministic: true,
        }
    } else {
        QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::Weak,
            summary: format!(
                "the case declares {} requirement reference(s) and none resolves to a row in \
                 `requirements`, and it has no `covers` edge -- the link is broken, not missing",
                declared.len()
            ),
            evidence: format!("{evidence}; unresolved={unresolved:?}"),
            deterministic: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Dimension 2 — observable expected result
// ---------------------------------------------------------------------------

/// Reuses the creation contract enforced by `api::validate_test_case_request`: a case needs
/// `expected_result` or at least one non-empty `assertion`.
///
/// That validator is not called here because it takes a request and returns an `ApiError`; the
/// rule it encodes is restated, and the test suite pins both to the same cases so they cannot
/// drift apart unnoticed.
fn expected_result_present(case: &TestCaseRow) -> QualityDimension {
    let key = "expected_result_present";
    let dimension = "observable expected result";

    let has_expected = !case.expected_result.trim().is_empty();
    let assertions: Vec<&String> = case
        .assertions
        .iter()
        .filter(|a| !a.trim().is_empty())
        .collect();

    if has_expected {
        QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::Adequate,
            summary:
                "the case states an expected result, so it declares what an observer should see"
                    .to_string(),
            evidence: format!(
                "expected_result ({} chars), {} non-empty assertion(s)",
                case.expected_result.trim().len(),
                assertions.len()
            ),
            deterministic: true,
        }
    } else if !assertions.is_empty() {
        QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::Adequate,
            summary:
                "no free-text expected result, but the case declares assertions, which is the \
                 other way the existing creation validator accepts an oracle"
                    .to_string(),
            evidence: format!("expected_result empty, {} assertion(s)", assertions.len()),
            deterministic: true,
        }
    } else {
        QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::Weak,
            summary:
                "the case states no expected result and no assertion, so nothing declares what an \
                 observer should see. This is the same rule `validate_test_case_request` enforces \
                 at creation."
                    .to_string(),
            evidence: format!(
                "expected_result={:?}, assertions={:?}",
                case.expected_result, case.assertions
            ),
            deterministic: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Dimension 3 — concrete, reproducible steps
// ---------------------------------------------------------------------------

/// Whether the **existing** deterministic compiler can build a real plan from this case.
///
/// This reuses `execution_plan::compile_deterministic` rather than inventing a "do the steps look
/// concrete" heuristic: that function is the codebase's existing answer, and roadmap rule 8
/// requires reusing it.
///
/// The mapping is deliberately conservative:
///
/// * `Some(plan)` — a real plan was produced. **Adequate.**
/// * `None` — nothing was found. That does **not** mean the case is bad: the existing engine
///   falls back to AI compilation in exactly this situation, so `None` covers both "genuinely
///   too vague" and "needs the AI path". Calling it `Weak` would infer a defect from an absence,
///   so it is `InsufficientData` with the reason stated.
fn concrete_steps(case: &TestCaseRow) -> QualityDimension {
    let key = "concrete_steps";
    let dimension = "concrete, reproducible steps";

    if case.steps.is_empty() {
        return QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::Weak,
            summary: "the case has no steps, so nothing describes how to reproduce it".to_string(),
            evidence: "steps=[]".to_string(),
            deterministic: true,
        };
    }

    let empty_actions = case
        .steps
        .iter()
        .filter(|s| s.action.trim().is_empty())
        .count();

    match crate::execution_plan::compile_deterministic(case) {
        Some(plan) => QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::Adequate,
            summary: format!(
                "the existing deterministic compiler built a real {}-step plan from this case's \
                 own steps, so they are concrete enough to execute without guessing",
                plan.actions.len()
            ),
            evidence: format!(
                "{} step(s) declared, {empty_actions} with an empty action; compiled to {} \
                 plan step(s)",
                case.steps.len(),
                plan.actions.len()
            ),
            deterministic: true,
        },
        None => QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::InsufficientData,
            summary:
                "the deterministic compiler found no plan in these steps. That is NOT a finding \
                 of vagueness: the existing engine falls back to AI compilation in exactly this \
                 situation, so this case may well execute. Reported as unmeasured rather than weak."
                    .to_string(),
            evidence: format!(
                "{} step(s) declared, {empty_actions} with an empty action; \
                 compile_deterministic returned None",
                case.steps.len()
            ),
            deterministic: true,
        },
    }
}

// ---------------------------------------------------------------------------
// Dimension 4 — negative / boundary coverage  (NOT ASSESSABLE)
// ---------------------------------------------------------------------------

/// The roadmap asks for negative/boundary coverage *"where the requirement supports it"*.
///
/// Deciding whether a requirement "supports" a negative case means reading meaning out of
/// `requirements.text` — prose. This schema has no field recording it, no rule deriving it, and
/// Phase 8 introduces no AI judge (scoring rule 2 would require labelling such a result a
/// heuristic with a confidence; rule 7 forbids a second validation engine).
///
/// So it is reported **not assessable**, naming the missing source. A guess here would be exactly
/// the fabrication the global rules forbid.
fn negative_boundary_coverage() -> QualityDimension {
    QualityDimension {
        key: "negative_boundary_coverage",
        dimension: "negative / boundary coverage where the requirement supports it",
        verdict: QualityVerdict::NotAssessed,
        summary:
            "not assessable in this schema. Deciding whether a requirement supports a negative or \
             boundary case means reading meaning from `requirements.text`; no column records that \
             and no deterministic rule derives it. Reported rather than guessed."
                .to_string(),
        evidence:
            "missing source: requirements.text is prose only; there is no negative/boundary flag, \
             keyword contract, or rule in this codebase"
                .to_string(),
        deterministic: true,
    }
}

// ---------------------------------------------------------------------------
// Dimension 5 — duplicate similarity
// ---------------------------------------------------------------------------

/// Near-duplicate detection over the **existing** embeddings.
///
/// Reuses `test_case_embeddings` (one row per case that reached APPROVED) and the same cosine
/// `ai_generation::rank_similar_test_cases` uses.
///
/// Coverage limit, reported honestly: only cases that were APPROVED *and* embedded have a row, so
/// a project with no embeddings is `InsufficientData`, never "no duplicates".
async fn duplicate_similarity(
    store: &Store,
    case: &TestCaseRow,
    project_id: &str,
) -> QualityDimension {
    let key = "duplicate_similarity";
    let dimension = "duplicate similarity";

    let rows = match store.list_test_case_embeddings(project_id).await {
        Ok(r) => r,
        Err(e) => {
            return QualityDimension {
                key,
                dimension,
                verdict: QualityVerdict::InsufficientData,
                summary: "the embedding store could not be read, so duplication is unknown"
                    .to_string(),
                evidence: format!("store read failed: {e}"),
                deterministic: true,
            }
        }
    };

    let Some(mine) = rows.iter().find(|r| r.test_case_id == case.id) else {
        return QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::InsufficientData,
            summary:
                "this case has no embedding row. `test_case_embeddings` only holds cases that \
                 reached APPROVED, so duplication cannot be judged for it."
                    .to_string(),
            evidence: format!(
                "project holds {} embedding(s); none for this case",
                rows.len()
            ),
            deterministic: true,
        };
    };

    let mut nearest: Option<(Uuid, f32)> = None;
    for other in rows.iter().filter(|r| r.test_case_id != case.id) {
        let sim = crate::ai_generation::cosine_similarity(&mine.embedding, &other.embedding);
        if nearest.map(|(_, s)| sim > s).unwrap_or(true) {
            nearest = Some((other.test_case_id, sim));
        }
    }

    let Some((other_id, sim)) = nearest else {
        return QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::InsufficientData,
            summary:
                "this is the only embedded case in the project, so there is nothing to compare it \
                 against"
                    .to_string(),
            evidence: "1 embedding in project, so there is no second case to compare against"
                .to_string(),
            deterministic: true,
        };
    };

    let similarity = sim as f64;
    if similarity >= NEAR_DUPLICATE_SIMILARITY {
        QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::Weak,
            summary: format!(
                "another case in this project is a near-duplicate (cosine {similarity:.3} >= \
                 {NEAR_DUPLICATE_SIMILARITY})"
            ),
            evidence: format!("nearest={other_id} similarity={similarity:.6}"),
            deterministic: true,
        }
    } else {
        QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::Adequate,
            summary: format!(
                "no near-duplicate in this project (nearest cosine {similarity:.3} < \
                 {NEAR_DUPLICATE_SIMILARITY})"
            ),
            evidence: format!("nearest={other_id} similarity={similarity:.6}"),
            deterministic: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Dimension 6 — automation feasibility
// ---------------------------------------------------------------------------

/// Can any configured executor run this kind of case?
///
/// The authoritative source is `ExecutorRegistry::registered_kinds()` — the executors this
/// deployment actually has.
fn automation_feasibility(inputs: &QualityInputs<'_>) -> QualityDimension {
    let key = "automation_feasibility";
    let dimension = "automation feasibility against available executor capabilities";
    let kinds = inputs.executor_kinds;

    if kinds.is_empty() {
        return QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::InsufficientData,
            summary:
                "no executor is registered in this deployment, so feasibility cannot be judged \
                 for any case"
                    .to_string(),
            evidence: "ExecutorRegistry::registered_kinds() = []".to_string(),
            deterministic: true,
        };
    }

    QualityDimension {
        key,
        dimension,
        verdict: QualityVerdict::Adequate,
        summary: format!(
            "{} executor(s) are configured, so a case of this shape has somewhere to run",
            kinds.len()
        ),
        evidence: format!("registered_kinds={kinds:?}"),
        deterministic: true,
    }
}

// ---------------------------------------------------------------------------
// Dimension 7 — relevance to recorded change / risk signals
// ---------------------------------------------------------------------------

/// Has any declared change been traced to this case?
///
/// Reuses `Store::count_distinct_changes_for_node`, the same reader the existing risk assessment
/// uses. Zero recorded changes is `InsufficientData` with the count — never "irrelevant", because
/// nothing having been declared against a case is not evidence about its quality.
async fn change_relevance(store: &Store, case: &TestCaseRow, project_id: &str) -> QualityDimension {
    let key = "change_relevance";
    let dimension = "relevance to recorded risk / change signals";

    let node = match test_node_id(store, project_id, case.id).await {
        Ok(n) => n,
        Err(e) => {
            return QualityDimension {
                key,
                dimension,
                verdict: QualityVerdict::InsufficientData,
                summary: "the quality graph could not be read, so change relevance is unknown"
                    .to_string(),
                evidence: format!("store read failed: {e}"),
                deterministic: true,
            }
        }
    };

    match store
        .count_distinct_changes_for_node(project_id, node.as_ref())
        .await
    {
        Ok(0) => QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::InsufficientData,
            summary:
                "no declared change has been traced to this case. That is an absence of recorded \
                 signal, not evidence that the case is irrelevant."
                    .to_string(),
            evidence: format!("count_distinct_changes_for_node = 0; node={node:?}"),
            deterministic: true,
        },
        Ok(n) => QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::Adequate,
            summary: format!(
                "{n} distinct declared change(s) have been traced to this case, so its relevance \
                 is evidenced rather than asserted"
            ),
            evidence: format!("count_distinct_changes_for_node = {n}; node={node:?}"),
            deterministic: true,
        },
        Err(e) => QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::InsufficientData,
            summary: "the change-impact store could not be read".to_string(),
            evidence: format!("store read failed: {e}"),
            deterministic: true,
        },
    }
}

// ---------------------------------------------------------------------------
// Dimension 8 — execution reliability (product outcomes only)
// ---------------------------------------------------------------------------

/// Execution history, read **only** through the established Phase 7.4 helpers.
///
/// Deliberately about *product* outcomes. `INFRA_ERROR` is excluded from both halves by
/// `completed_failure_rate_fraction` and reported separately by `infra_errors`, so an
/// infrastructure problem is visible here without ever being read as a product-test defect.
///
/// A case that has never run is `InsufficientData`, never `Adequate`: not a defect, and not a
/// clean record.
async fn execution_reliability(
    store: &Store,
    case: &TestCaseRow,
    project_id: &str,
) -> QualityDimension {
    let key = "execution_reliability";
    let dimension = "execution reliability (product outcomes only)";

    let rows = match store.list_execution_telemetry(project_id, 1000).await {
        Ok(r) => r,
        Err(e) => {
            return QualityDimension {
                key,
                dimension,
                verdict: QualityVerdict::InsufficientData,
                summary: "telemetry could not be read, so execution reliability is unknown"
                    .to_string(),
                evidence: format!("store read failed: {e}"),
                deterministic: true,
            }
        }
    };

    let mine: Vec<&crate::store::ExecutionTelemetryRow> = rows
        .iter()
        .filter(|r| r.test_case_id == Some(case.id))
        .collect();

    let completed = crate::qa_memory::QaMemoryReader::completed_count(&mine);
    let infra = crate::qa_memory::QaMemoryReader::infra_errors(&mine);
    let rate = crate::qa_memory::QaMemoryReader::completed_failure_rate_fraction(&mine);

    let evidence = format!(
        "recorded rows={}, completed(PASSED|FAILED|TIMEOUT)={completed}, \
         infrastructure_errors={infra}, completed_failure_rate_fraction={rate:?}",
        mine.len()
    );

    match rate {
        None => QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::InsufficientData,
            summary: format!(
                "this case has {} recorded execution(s) but none completed as a pass or a failure, \
                 so its product reliability is unknown. That is neither a quality defect nor a \
                 clean record.",
                mine.len()
            ),
            evidence,
            deterministic: true,
        },
        Some(r) => QualityDimension {
            key,
            dimension,
            verdict: QualityVerdict::Adequate,
            summary: format!(
                "{} of {completed} completed executions failed ({:.1}%); {infra} infrastructure \
                 error(s) are reported separately and are not counted as product failures",
                (r * completed as f64).round() as usize,
                r * 100.0
            ),
            evidence,
            deterministic: true,
        },
    }
}
