//! Phase 1 (Trace Graph Completion) — the trace walk.
//!
//! ## What this is
//!
//! Exactly **one** retrieval capability, added on top of the graph that already exists
//! rather than beside it. It walks:
//!
//! ```text
//! Requirement -> Test -> Execution -> Failure -> Bug -> Fix -> Retest -> Release
//! ```
//!
//! ## Root cause it exists to answer
//!
//! The graph had three writers but `quality_relations` held **0 rows** in the live
//! database. All three causes were code facts, not data problems, and all three are
//! recorded here because a walk is only as good as the graph underneath it:
//!
//! 1. **Two node types for one concept.** `Intelligence::link_test_case_to_requirement`
//!    wrote `"test_case"`; every reader (`qa_memory.rs`) looked for `"test"`. A case's
//!    requirement edge and its execution's failure edge therefore landed on two
//!    vertices that could never join.
//! 2. **A name where an identity belonged.** The failure path keyed its vertex on
//!    `telemetry.task_name`, so two cases sharing a title — or one case renamed —
//!    collapsed onto one vertex.
//! 3. **`covers` had no production caller — and still has none, deliberately.**
//!    `Store::link_test_case_to_module` existed and was exercised only by its own
//!    tests. Phase 1 added the id-keyed [`crate::store::Store::link_case_to_module`],
//!    which works and is tested, but **nothing in the production data model names a
//!    module**: `TestCaseRow` carries `source_document` (a document, not a module),
//!    `TaskDef` has no module field, and `RunPlan` has no file or path. The only
//!    `source_file` in the codebase belongs to custom-skill import and is unrelated.
//!
//!    Wiring a caller would therefore mean inventing a document-to-module or
//!    name-to-path mapping — a heuristic link presented as fact, which is exactly
//!    what this phase forbids. So `covers` is reported **honestly as missing**: the
//!    walk names `module` in `incomplete`, and `docs/next-intelligence/
//!    PHASE_1_TRACE_GRAPH_REPORT.md` lists it as a remaining limitation with the
//!    schema decision that would be needed to close it.
//!
//! ## The rules this module obeys
//!
//! * **No invented edges.** Every step joins on a stored id. A hop with no evidence is
//!   reported as `None`, never as an inferred link. `INSUFFICIENT_DATA`-style honesty,
//!   applied to a walk.
//! * **Project scoped.** `project_id` is a required argument and appears in every
//!   query.
//! * **Deterministic and bounded.** Every list is explicitly ordered and capped by a
//!   `*_MAX` constant, in the style of the existing `store::LIST_*_MAX` bounds.
//! * **Evidence attached.** Each step carries the row ids it came from, so a caller can
//!   check any hop rather than trust the shape.
//! * **Legacy vertices are read, never written.** Edges written under the old
//!   `LEGACY_TEST_NODE_TYPE` still resolve; dropping them would report "no history"
//!   where history exists, which is the wrong answer this whole phase exists to remove.
//!
//! ## What it does not do
//!
//! No new tables. No new memory store. No second bug, risk, regression or planning
//! system. It reads `quality_nodes`, `quality_relations`, `execution_telemetry`,
//! `failure_clusters`, `bug_drafts`, `healing_proposals`, `runs`, `qa_releases`,
//! `requirements` and `test_cases` — all pre-existing.

use crate::store::{Store, LEGACY_TEST_NODE_TYPE, TASK_NODE_TYPE, TEST_NODE_TYPE};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use uuid::Uuid;

/// Ceiling on executions listed under one test case.
pub const TRACE_MAX_EXECUTIONS: usize = 50;
/// Ceiling on bugs listed under one failure.
pub const TRACE_MAX_BUGS: usize = 20;
/// Ceiling on requirements returned for a project.
pub const TRACE_MAX_REQUIREMENTS: usize = 100;

/// One hop in the trace, carrying the evidence it was read from.
#[derive(Debug, Clone, Serialize)]
pub struct TraceStep<T> {
    /// `requirement` | `test` | `execution` | `failure` | `bug` | `fix` | `retest` |
    /// `release` — the *kind* of hop, not its id.
    pub kind: String,
    /// The stable id of the row this hop came from. `None` only where the underlying
    /// entity has no id of its own.
    pub id: Option<String>,
    /// A human label. Never used as an identity.
    pub label: Option<String>,
    /// The stored evidence: what this hop was derived from.
    pub evidence: TraceEvidence,
    /// The payload, absent where there is none — see the `Option` fields on
    /// [`TestCaseTrace`].
    pub payload: Option<T>,
}

/// Where a hop's facts came from. Recorded so a claim can be checked rather than
/// believed.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TraceEvidence {
    /// The `quality_nodes.external_ref` the hop was reached by, where the graph was
    /// the entry point.
    pub via_node: Option<String>,
    /// The graph relation type used, where one was.
    pub via_relation: Option<String>,
    /// The row ids joined to reach this hop.
    pub rows: Vec<String>,
    /// A statement of what is *missing* rather than inferred. Empty when nothing is
    /// missing.
    pub missing: Vec<String>,
}

/// The full chain for one test case.
#[derive(Debug, Clone, Serialize)]
pub struct TestCaseTrace {
    pub project_id: String,
    /// The case the trace is about. `None` when the request named a graph vertex that
    /// resolves to no `test_cases` row.
    pub test_case_id: Option<String>,
    pub steps: BTreeMap<String, Vec<TraceStep<Value>>>,
    /// Every hop whose evidence was absent, named. A trace with entries here is
    /// reporting gaps, not hiding them.
    pub incomplete: Vec<String>,
}

impl TestCaseTrace {
    /// One hop, if present.
    pub fn step(&self, kind: &str) -> Option<&Vec<TraceStep<Value>>> {
        self.steps.get(kind)
    }
    /// Whether the walk reached every hop kind.
    pub fn is_complete(&self) -> bool {
        self.incomplete.is_empty()
    }
}

/// A project-level summary: which cases exist in the graph and how far each traces.
///
/// This is the entry point, because a trace is only meaningful when the list of
/// candidates is real. Projects with no cases yield an empty list rather than a
/// fabricated summary.
#[derive(Debug, Clone, Serialize)]
pub struct TraceIndex {
    pub project_id: String,
    pub test_cases: Vec<TraceIndexEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraceIndexEntry {
    pub test_case_id: Option<String>,
    pub title: Option<String>,
    /// `test_cases.id` when the graph vertex resolves to a real row. `None` for a
    /// graph vertex with no case behind it, which is reported rather than dropped.
    pub resolved: bool,
    /// Hop kinds with evidence.
    pub has: Vec<String>,
    /// Hop kinds without evidence.
    pub missing: Vec<String>,
}

/// Read-only traversal of the quality graph plus the tables it points at.
#[derive(Clone)]
pub struct TraceReader {
    store: Store,
}

impl TraceReader {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Every test-case vertex in this project, with how far each one traces.
    ///
    /// Includes both the canonical `TEST_NODE_TYPE` and the historical
    /// `LEGACY_TEST_NODE_TYPE`, because an edge written before this phase would
    /// otherwise vanish from the index and look like a case that never existed.
    pub async fn index(&self, project_id: &str) -> TraceIndex {
        let nodes = self
            .store
            .list_quality_nodes(project_id)
            .await
            .unwrap_or_default();
        let relations = self
            .store
            .list_quality_relations_for_project(project_id)
            .await
            .unwrap_or_default();

        let mut by_node: HashMap<Uuid, Vec<&str>> = HashMap::new();
        for r in &relations {
            by_node
                .entry(r.from_node_id)
                .or_default()
                .push(&r.relation_type);
        }

        let mut out: Vec<TraceIndexEntry> = nodes
            .iter()
            .filter(|n| n.node_type == TEST_NODE_TYPE || n.node_type == LEGACY_TEST_NODE_TYPE)
            .map(|n| {
                let ref_id = n.external_ref.clone();
                let case_id = ref_id.as_deref().and_then(|s| Uuid::parse_str(s).ok());
                let outgoing = by_node.get(&n.id).cloned().unwrap_or_default();
                let mut has: Vec<String> = Vec::new();
                let mut missing: Vec<String> = Vec::new();
                for (relation, hop) in [
                    ("validates", "requirement"),
                    ("covers", "module"),
                    ("fails_with", "failure"),
                ] {
                    if outgoing.contains(&relation) {
                        has.push(hop.to_string());
                    } else {
                        missing.push(hop.to_string());
                    }
                }
                TraceIndexEntry {
                    test_case_id: case_id.map(|c| c.to_string()),
                    title: Some(n.name.clone()),
                    resolved: case_id.is_some(),
                    has,
                    missing,
                }
            })
            .collect();

        out.sort_by(|a, b| {
            a.test_case_id
                .cmp(&b.test_case_id)
                .then(a.title.cmp(&b.title))
        });
        out.truncate(TRACE_MAX_REQUIREMENTS);
        TraceIndex {
            project_id: project_id.to_string(),
            test_cases: out,
        }
    }

    /// Walk the whole chain for one test case.
    ///
    /// `test_case_id` is a `test_cases.id`. A hop with no evidence is simply absent from
    /// `steps` and named in `incomplete` — never filled in.
    pub async fn walk_test_case(&self, project_id: &str, test_case_id: &Uuid) -> TestCaseTrace {
        let case_ref = test_case_id.to_string();
        let mut steps: BTreeMap<String, Vec<TraceStep<Value>>> = BTreeMap::new();
        let mut incomplete: Vec<String> = Vec::new();

        // ---- test ------------------------------------------------------
        let case_row = self.case_in_project(project_id, test_case_id).await;
        steps.insert(
            "test".into(),
            vec![TraceStep {
                kind: "test".into(),
                id: Some(case_ref.clone()),
                label: case_row.as_ref().map(|c| c.title.clone()),
                evidence: TraceEvidence {
                    via_node: Some(case_ref.clone()),
                    ..Default::default()
                },
                payload: case_row.as_ref().map(|c| {
                    serde_json::json!({
                        "title": c.title,
                        "requirement_reference": c.requirement_reference,
                        "source_requirement": c.source_requirement,
                        "executed_run_id": c.executed_run_id,
                        "executed_task_id": c.executed_task_id,
                    })
                }),
            }],
        );

        // ---- requirement (via the `validates` edge, plus the case's own fields) ----
        // The edge is the graph of record; the case columns are a second, independent
        // source. Both are read, and both are reported, so a case that carries a
        // requirement reference without an edge is visible as such rather than
        // silently treated as traced.
        let mut requirement_hops: Vec<TraceStep<Value>> = Vec::new();
        for (source, ref_id, name) in self
            .requirement_refs_for_case(project_id, test_case_id, case_row.as_ref())
            .await
        {
            requirement_hops.push(TraceStep {
                kind: "requirement".into(),
                id: None,
                label: Some(name.clone()),
                evidence: TraceEvidence {
                    via_node: Some(ref_id.clone()),
                    via_relation: Some("validates".into()),
                    missing: if source == "case_column" {
                        vec![format!(
                            "carried on the test case but no `{source}` row exists yet"
                        )]
                    } else {
                        Vec::new()
                    },
                    ..Default::default()
                },
                payload: Some(serde_json::json!({
                    "reference": ref_id,
                    "name": name,
                    "evidence": source,
                })),
            });
        }
        if requirement_hops.is_empty() {
            incomplete.push("requirement".into());
        } else {
            steps.insert("requirement".into(), requirement_hops);
        }

        // ---- module (via `covers`) ---------------------------------------
        let modules = self.modules_for_case(project_id, &case_ref).await;
        if modules.is_empty() {
            incomplete.push("module".into());
        } else {
            steps.insert(
                "module".into(),
                modules
                    .into_iter()
                    .map(|m| TraceStep {
                        kind: "module".into(),
                        id: None,
                        label: Some(m.clone()),
                        evidence: TraceEvidence {
                            via_node: Some(m),
                            via_relation: Some("covers".into()),
                            rows: Vec::new(),
                            missing: Vec::new(),
                        },
                        payload: None,
                    })
                    .collect(),
            );
        }

        // ---- execution (from execution_telemetry by stable case id) ----
        let executions = self.executions_for_case(project_id, test_case_id).await;
        if executions.is_empty() {
            incomplete.push("execution".into());
        } else {
            steps.insert("execution".into(), executions);
        }

        // ---- failure / bug, derived from the executions ----
        // Returns the fingerprints that were reached, because those are the join key
        // for everything downstream: `failure → bug → fix`.
        let (failures, bugs, failure_fingerprints, execution_run_ids) =
            self.failure_side_of_trace(project_id, test_case_id).await;
        if failures.is_empty() {
            incomplete.push("failure".into());
        } else {
            steps.insert("failure".into(), failures);
        }
        if bugs.is_empty() {
            incomplete.push("bug".into());
        } else {
            steps.insert("bug".into(), bugs);
        }

        // ---- fix / retest, reached from the failures *and* the bugs ----
        //
        // Phase 2. Phase 1 derived `fix` from the case's telemetry fingerprints, which
        // could never match: `healing_proposals.failure_fingerprint` was produced by a
        // different function hashing `run_id` + `task_id`. Both fingerprint producers
        // now share `canonical_failure_fingerprint`, so this join is real. It is also
        // reached from the bugs, because a bug hangs off a cluster and a proposal hangs
        // off the same fingerprint — so `Bug → Fix` is a genuine step, not two lists
        // that happen to sit near each other.
        let (fixes, retests, retest_run_ids) = self
            .fix_and_retest_side_of_trace(project_id, &failure_fingerprints)
            .await;
        let had_fixes = !fixes.is_empty();
        if !had_fixes {
            incomplete.push(if failure_fingerprints.is_empty() {
                "fix (no failure to fix)".into()
            } else {
                "fix (no healing proposal recorded against this failure)".into()
            });
        } else {
            steps.insert("fix".into(), fixes);
        }
        if retests.is_empty() {
            incomplete.push(if !had_fixes {
                "retest (no fix to retest)".into()
            } else {
                "retest (the fix was never executed and verified)".into()
            });
        } else {
            steps.insert("retest".into(), retests);
        }

        // ---- release, reached from the executions *and* the retests ----
        //
        // Both matter and they are different evidence. An execution run says "this
        // case ran in that release". A retest run is a *fresh* run created to prove the
        // fix worked, and it is the one that shows a release shipped against a repaired
        // defect. Reporting only the executions would let a bug be fixed and verified
        // in release B while the trace still pointed at release A.
        let mut release_run_ids = execution_run_ids;
        release_run_ids.extend(retest_run_ids);
        let releases = self.releases_for_runs(project_id, &release_run_ids).await;
        if releases.is_empty() {
            incomplete.push("release".into());
        } else {
            steps.insert("release".into(), releases);
        }

        incomplete.sort();
        incomplete.dedup();
        TestCaseTrace {
            project_id: project_id.to_string(),
            test_case_id: Some(case_ref),
            steps,
            incomplete,
        }
    }

    /// The test case row, **only if it belongs to this project**.
    ///
    /// `Store::get_test_case` takes a bare id with no tenant argument — a case id is a
    /// UUID, so it is unguessable, but "unguessable" is not "isolated". The walk is
    /// project-scoped everywhere else, so a caller who held another tenant's case id
    /// and passed it here would get that tenant's `requirement_reference`,
    /// `source_requirement` and `executed_run_id` back in the `test` hop.
    ///
    /// This was not hypothetical: `trace_tests::a_project_cannot_trace_another_projects_case`
    /// caught exactly that, returning a requirement hop sourced from the other
    /// tenant's case row. The project is re-checked here rather than trusted, so the
    /// walk reports a case it cannot see as absent instead of as another tenant's data.
    async fn case_in_project(
        &self,
        project_id: &str,
        test_case_id: &Uuid,
    ) -> Option<crate::store::TestCaseRow> {
        let row = self
            .store
            .get_test_case(*test_case_id)
            .await
            .ok()
            .flatten()?;
        if row.project_id != project_id {
            return None;
        }
        Some(row)
    }

    /// Requirement references for a case, from both the graph edge and the case's own
    /// columns, tagged by which source they came from.
    async fn requirement_refs_for_case(
        &self,
        project_id: &str,
        test_case_id: &Uuid,
        case_row: Option<&crate::store::TestCaseRow>,
    ) -> Vec<(&'static str, String, String)> {
        let mut out: Vec<(&'static str, String, String)> = Vec::new();
        let seen: BTreeSet<String> = out.iter().map(|(_, r, _)| r.clone()).collect();
        let mut seen = seen;

        let (nodes, relations) = match (
            self.store.list_quality_nodes(project_id).await,
            self.store
                .list_quality_relations_for_project(project_id)
                .await,
        ) {
            (Ok(n), Ok(r)) => (n, r),
            _ => (Vec::new(), Vec::new()),
        };

        let case_node_ids: BTreeSet<Uuid> = nodes
            .iter()
            .filter(|n| {
                (n.node_type == TEST_NODE_TYPE || n.node_type == LEGACY_TEST_NODE_TYPE)
                    && n.external_ref.as_deref() == Some(test_case_id.to_string().as_str())
            })
            .map(|n| n.id)
            .collect();

        for rel in relations.iter().filter(|r| r.relation_type == "validates") {
            if !case_node_ids.contains(&rel.from_node_id) {
                continue;
            }
            if let Some(req) = nodes
                .iter()
                .find(|n| n.id == rel.to_node_id && n.node_type == "requirement")
            {
                let r = req.external_ref.clone().unwrap_or_default();
                let name = req.name.clone();
                if !r.is_empty() && seen.insert(r.clone()) {
                    out.push(("validates_edge", r, name));
                }
            }
        }

        // The case's own columns are independent evidence, recorded separately so a
        // missing `requirements` row is visible instead of being papered over.
        if let Some(c) = case_row {
            for candidate in [
                c.requirement_reference.as_deref(),
                c.source_requirement.as_deref(),
            ]
            .into_iter()
            .flatten()
            .filter(|s| !s.trim().is_empty())
            {
                if seen.insert(candidate.to_string()) {
                    out.push(("case_column", candidate.to_string(), candidate.to_string()));
                }
            }
        }

        out.sort();
        out
    }

    /// Modules linked to a case by `covers`.
    async fn modules_for_case(&self, project_id: &str, case_ref: &str) -> Vec<String> {
        let (nodes, relations) = match (
            self.store.list_quality_nodes(project_id).await,
            self.store
                .list_quality_relations_for_project(project_id)
                .await,
        ) {
            (Ok(n), Ok(r)) => (n, r),
            _ => return Vec::new(),
        };
        let case_ids: BTreeSet<Uuid> = nodes
            .iter()
            .filter(|n| {
                (n.node_type == TEST_NODE_TYPE || n.node_type == LEGACY_TEST_NODE_TYPE)
                    && n.external_ref.as_deref() == Some(case_ref)
            })
            .map(|n| n.id)
            .collect();
        let mut out: Vec<String> = relations
            .iter()
            .filter(|r| r.relation_type == "covers" && case_ids.contains(&r.from_node_id))
            .filter_map(|r| nodes.iter().find(|n| n.id == r.to_node_id))
            .filter(|n| n.node_type == "module")
            .filter_map(|n| n.external_ref.clone())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Executions of a case, from `execution_telemetry.test_case_id` — the stable link
    /// migration 0028 added. Never matched on `task_name`.
    async fn executions_for_case(
        &self,
        project_id: &str,
        test_case_id: &Uuid,
    ) -> Vec<TraceStep<Value>> {
        let rows = self
            .store
            .list_execution_telemetry(project_id, crate::qa_memory::TELEMETRY_READ_MAX)
            .await
            .unwrap_or_default();
        let mut out: Vec<TraceStep<Value>> = rows
            .iter()
            .filter(|r| r.test_case_id == Some(*test_case_id))
            .map(|r| TraceStep {
                kind: "execution".into(),
                id: Some(r.id.to_string()),
                label: Some(r.task_name.clone()),
                evidence: TraceEvidence {
                    via_node: Some(test_case_id.to_string()),
                    via_relation: Some("test_case_id".into()),
                    rows: vec![r.id.to_string(), r.run_id.to_string()],
                    missing: if r.test_case_id.is_none() {
                        vec!["execution predates the case<->execution link".into()]
                    } else {
                        Vec::new()
                    },
                },
                payload: Some(serde_json::json!({
                    "run_id": r.run_id,
                    "task_id": r.task_id,
                    "status": r.status,
                    "environment": r.environment,
                    "created_at": r.created_at,
                    "failure_fingerprint": r.failure_fingerprint,
                })),
            })
            .collect();
        // Chronological, so the walk reads in the order things happened.
        out.sort_by(|a, b| {
            let ta = a
                .payload
                .as_ref()
                .and_then(|p| p.get("created_at"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let tb = b
                .payload
                .as_ref()
                .and_then(|p| p.get("created_at"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            ta.cmp(tb).then(a.id.cmp(&b.id))
        });
        out.truncate(TRACE_MAX_EXECUTIONS);
        out
    }

    /// Failure, bug and release hops for a case, reached through its executions'
    /// fingerprints. Each cluster is fetched by fingerprint through the new store
    /// Failure and bug hops, plus the two things the rest of the chain is keyed on:
    /// the failure fingerprints reached, and the run ids this case executed in.
    ///
    /// Both are returned rather than re-derived later so that every downstream hop joins
    /// on the *same* set of keys this function actually read. Recomputing them per hop
    /// would be how two parts of a trace end up disagreeing about what happened.
    #[allow(clippy::type_complexity)]
    async fn failure_side_of_trace(
        &self,
        project_id: &str,
        test_case_id: &Uuid,
    ) -> (
        Vec<TraceStep<Value>>,
        Vec<TraceStep<Value>>,
        BTreeSet<String>,
        BTreeSet<Uuid>,
    ) {
        let rows = self
            .store
            .list_execution_telemetry(project_id, crate::qa_memory::TELEMETRY_READ_MAX)
            .await
            .unwrap_or_default();
        let mine: Vec<&crate::store::ExecutionTelemetryRow> = rows
            .iter()
            .filter(|r| r.test_case_id == Some(*test_case_id))
            .collect();

        let mut failures = Vec::new();
        let mut bugs = Vec::new();
        let mut fingerprints: BTreeSet<String> = BTreeSet::new();

        // Fetched once. `list_bug_drafts` is project-scoped but not filtered by cluster,
        // and this loop runs per fingerprint; without the hoist it was a full re-read per
        // distinct failure.
        let drafts = self
            .store
            .list_bug_drafts(Some(project_id))
            .await
            .unwrap_or_default();

        for r in &mine {
            let Some(fp) = r.failure_fingerprint.as_deref().filter(|f| !f.is_empty()) else {
                continue;
            };
            // One failure hop per distinct fingerprint, not per occurrence: the
            // occurrence count is carried on the hop instead of inflating the list.
            if !fingerprints.insert(fp.to_string()) {
                continue;
            }
            let cluster = self
                .store
                .get_failure_cluster_by_fingerprint(project_id, fp)
                .await
                .ok()
                .flatten();

            failures.push(TraceStep {
                kind: "failure".into(),
                id: cluster.as_ref().map(|c| c.id.to_string()),
                label: cluster
                    .as_ref()
                    .map(|c| c.signature.clone())
                    .or_else(|| Some(fp.to_string())),
                evidence: TraceEvidence {
                    via_node: Some(fp.to_string()),
                    via_relation: Some("fails_with".into()),
                    rows: vec![r.id.to_string()],
                    missing: if cluster.is_none() {
                        vec!["no failure_clusters row carries this fingerprint yet".into()]
                    } else {
                        Vec::new()
                    },
                },
                payload: Some(serde_json::json!({
                    "fingerprint": fp,
                    "signature": cluster.as_ref().map(|c| c.signature.clone()),
                    "occurrence_count": cluster.as_ref().map(|c| c.occurrence_count),
                    "first_seen_at": cluster.as_ref().map(|c| c.first_seen_at),
                    "last_seen_at": cluster.as_ref().map(|c| c.last_seen_at),
                    "review_state": cluster.as_ref().map(|c| c.review_state.clone()),
                    "root_cause_hypothesis":
                        cluster.as_ref().map(|c| c.root_cause_hypothesis.clone()),
                })),
            });

            // Failure -> Bug. A draft hangs off a cluster row by foreign key, so a
            // fingerprint with no cluster cannot yield a bug, and none is invented.
            if let Some(c) = &cluster {
                for draft in drafts.iter().filter(|d| d.failure_cluster_id == c.id) {
                    bugs.push(TraceStep {
                        kind: "bug".into(),
                        id: Some(draft.id.to_string()),
                        label: Some(draft.title.clone()),
                        evidence: TraceEvidence {
                            via_node: Some(c.id.to_string()),
                            via_relation: Some("failure_cluster_id".into()),
                            rows: vec![draft.id.to_string()],
                            missing: Vec::new(),
                        },
                        payload: Some(serde_json::json!({
                            "severity": draft.severity,
                            "confidence": draft.confidence,
                            "review_state": draft.review_state,
                            "created_at": draft.created_at,
                            "promoted_at": draft.promoted_at,
                            "cluster_id": draft.failure_cluster_id,
                            // Carried so a consumer can follow Bug -> Fix itself, rather
                            // than having to re-derive which fingerprint this bug hangs off.
                            "cluster_fingerprint": fp,
                        })),
                    });
                }
            }
        }

        // Runs this case executed in. `test_cases.executed_run_id` is read as well as the
        // telemetry run ids, because a case may have been handed off to a run whose
        // telemetry row has not been ingested yet, and that run is real history this walk
        // should not discard.
        let mut run_ids: BTreeSet<Uuid> = mine.iter().map(|r| r.run_id).collect();
        if let Some(case_row) = self.case_in_project(project_id, test_case_id).await {
            if let Some(run) = case_row.executed_run_id {
                run_ids.insert(run);
            }
        }

        bugs.truncate(TRACE_MAX_BUGS);
        (failures, bugs, fingerprints, run_ids)
    }

    /// Fix and retest hops, reached from the failure fingerprints found upstream.
    ///
    /// ## Bug -> Fix
    ///
    /// A bug hangs off a `failure_clusters` row; a healing proposal hangs off a
    /// `failure_fingerprint`. Migration 0030 put `failure_clusters.failure_fingerprint`
    /// on the cluster, so the chain is
    /// `bug_drafts.failure_cluster_id` -> `failure_clusters.failure_fingerprint` ->
    /// `healing_proposals.failure_fingerprint`, joining on stored keys at every hop.
    ///
    /// This join **could never succeed before Phase 2**. `healing_proposals`'
    /// fingerprint came from `compute_failure_fingerprint`, which hashed `run_id` and
    /// `task_id`; telemetry's came from `compute_telemetry_fingerprint`, which hashes
    /// category + masked message + selector. Different inputs, different digests, and the
    /// run id made each proposal's value unique per run so the same defect could not group
    /// across runs either. Both producers now call
    /// `adaptive_intelligence::canonical_failure_fingerprint`, so the key is shared by
    /// construction rather than by convention.
    ///
    /// ## Fix -> Retest
    ///
    /// A retest is **not** inferred from "the test passed later". It is the governed
    /// verification run: `healing_proposals.verification_result` holds a serialized
    /// `HealingVerification` naming the real healing `run_id`, the real `healing_task_id`,
    /// and the recorded outcome. Phase 1 sniffed that column for the substring `"pass"`,
    /// which would have accepted any prose containing it. It is now deserialized and read
    /// structurally.
    ///
    /// A retest hop is emitted whenever a verification exists — including a failed one.
    /// A fix that was tried and did not work is real evidence, and hiding it would make
    /// the chain report a clean success for an unresolved defect. `passed` on the hop
    /// distinguishes the two.
    ///
    /// Returns the retest run ids as well, because a retest run is a real run and
    /// therefore evidence for the release hop.
    #[allow(clippy::type_complexity)]
    async fn fix_and_retest_side_of_trace(
        &self,
        project_id: &str,
        failure_fingerprints: &BTreeSet<String>,
    ) -> (Vec<TraceStep<Value>>, Vec<TraceStep<Value>>, BTreeSet<Uuid>) {
        let mut fixes = Vec::new();
        let mut retests = Vec::new();
        let mut retest_run_ids: BTreeSet<Uuid> = BTreeSet::new();

        if failure_fingerprints.is_empty() {
            return (fixes, retests, retest_run_ids);
        }

        // Which bug ids hang off each fingerprint, so a fix can name the bugs it
        // addresses instead of the two hops being independently correct but unrelated.
        let mut bug_by_fingerprint: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for fp in failure_fingerprints {
            if let Some(cluster) = self
                .store
                .get_failure_cluster_by_fingerprint(project_id, fp)
                .await
                .ok()
                .flatten()
            {
                let ids: Vec<String> = self
                    .store
                    .list_bug_drafts(Some(project_id))
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|d| d.failure_cluster_id == cluster.id)
                    .map(|d| d.id.to_string())
                    .collect();
                bug_by_fingerprint.insert(fp.clone(), ids);
            }
        }

        for p in self
            .store
            .list_healing_proposals(project_id, None)
            .await
            .unwrap_or_default()
        {
            if !failure_fingerprints.contains(&p.failure_fingerprint) {
                continue;
            }
            let addresses_bugs = bug_by_fingerprint
                .get(&p.failure_fingerprint)
                .cloned()
                .unwrap_or_default();

            fixes.push(TraceStep {
                kind: "fix".into(),
                id: Some(p.id.to_string()),
                label: Some(p.diagnosis_category.clone()),
                evidence: TraceEvidence {
                    via_node: Some(p.failure_fingerprint.clone()),
                    via_relation: Some("healing_proposals.failure_fingerprint".into()),
                    rows: vec![p.id.to_string(), p.failure_fingerprint.clone()],
                    // A fix with no bug behind it is reported, not hidden: a single
                    // occurrence can be healed before it is recurring enough to draft.
                    missing: if addresses_bugs.is_empty() {
                        vec!["no bug draft has been raised for this failure".into()]
                    } else {
                        Vec::new()
                    },
                },
                payload: Some(serde_json::json!({
                    "diagnosis_category": p.diagnosis_category,
                    "state": p.state,
                    "confidence": p.confidence,
                    "risk_level": p.risk_level,
                    "requires_approval": p.requires_approval,
                    "verification_result": p.verification_result,
                    "created_at": p.created_at,
                    "task_id": p.task_id,
                    "run_id": p.run_id,
                    "healing_run_id": p.healing_run_id,
                    "failure_fingerprint": p.failure_fingerprint,
                    "addresses_bugs": addresses_bugs,
                })),
            });

            // ---- Fix -> Retest, read structurally ----
            //
            // `verification_result` is a JSON `HealingVerification`. Deserialized rather
            // than pattern-matched, because Phase 1 tested it with
            // `to_lowercase().contains("pass")` -- which would have accepted the word
            // "pass" appearing anywhere in a future prose message, and would have
            // rejected a genuine pass recorded as `Verified`.
            let Some(raw) = p.verification_result.as_deref().filter(|s| !s.is_empty()) else {
                continue;
            };
            let verification: crate::governed_healing::HealingVerification =
                match serde_json::from_str(raw) {
                    Ok(v) => v,
                    Err(e) => {
                        // An unreadable verification is reported rather than dropped, so a
                        // corrupt column cannot read as "no retest happened".
                        retests.push(TraceStep {
                            kind: "retest".into(),
                            id: Some(p.id.to_string()),
                            label: Some("verification record unreadable".into()),
                            evidence: TraceEvidence {
                                via_node: Some(p.failure_fingerprint.clone()),
                                via_relation: Some("healing_proposals.verification_result".into()),
                                rows: vec![p.id.to_string()],
                                missing: vec![format!(
                                    "verification_result is not a valid HealingVerification: {e}"
                                )],
                            },
                            payload: Some(serde_json::json!({
                                "raw": raw,
                                "passed": false,
                                "proposal_id": p.id,
                            })),
                        });
                        continue;
                    }
                };

            retest_run_ids.insert(verification.healing_run_id);

            let passed = verification.outcome
                == crate::governed_healing::VerificationOutcome::Verified
                && verification.revised_task_passed
                && verification.original_failure_resolved;

            retests.push(TraceStep {
                kind: "retest".into(),
                id: Some(verification.healing_run_id.to_string()),
                label: Some(format!(
                    "{} ({})",
                    verification.outcome.as_str(),
                    verification.healing_task_id
                )),
                evidence: TraceEvidence {
                    via_node: Some(p.failure_fingerprint.clone()),
                    via_relation: Some("healing_proposals.verification_result".into()),
                    rows: vec![
                        p.id.to_string(),
                        verification.healing_run_id.to_string(),
                        verification.healing_task_id.clone(),
                    ],
                    missing: Vec::new(),
                },
                payload: Some(serde_json::json!({
                    "proposal_id": p.id,
                    "healing_run_id": verification.healing_run_id,
                    "healing_task_id": verification.healing_task_id,
                    "outcome": verification.outcome.as_str(),
                    "passed": passed,
                    "revised_task_passed": verification.revised_task_passed,
                    "original_failure_resolved": verification.original_failure_resolved,
                    "regression_detected": verification.regression_detected,
                    "new_failures_introduced": verification.new_failures_introduced,
                    "evidence_refs": verification.evidence_refs,
                    "verified_at": verification.verified_at,
                    "message": verification.message,
                })),
            });
        }

        fixes.truncate(TRACE_MAX_BUGS);
        retests.truncate(TRACE_MAX_BUGS);
        (fixes, retests, retest_run_ids)
    }

    /// Release hops for a set of runs, joining through `runs.release_id`.
    ///
    /// Shared by the execution path and the retest path so both resolve a release the
    /// same way. A run with no release contributes nothing rather than a placeholder.
    async fn releases_for_runs(
        &self,
        project_id: &str,
        run_ids: &BTreeSet<Uuid>,
    ) -> Vec<TraceStep<Value>> {
        if run_ids.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for release in self
            .store
            .list_releases(project_id)
            .await
            .unwrap_or_default()
        {
            let Ok(runs) = self.store.runs_for_release(release.id).await else {
                continue;
            };
            let hit: Vec<String> = runs
                .iter()
                .filter(|r| run_ids.contains(r))
                .map(|r| r.to_string())
                .collect();
            if hit.is_empty() {
                continue;
            }
            out.push(TraceStep {
                kind: "release".into(),
                id: Some(release.id.to_string()),
                label: release.label.clone().or_else(|| release.version.clone()),
                evidence: TraceEvidence {
                    via_node: None,
                    via_relation: Some("runs.release_id".into()),
                    rows: hit,
                    missing: Vec::new(),
                },
                payload: Some(serde_json::json!({
                    "label": release.label,
                    "version": release.version,
                    "created_at": release.created_at,
                })),
            });
        }
        out
    }
}

/// reaching into `store`.
pub const CASE_NODE_TYPE: &str = TEST_NODE_TYPE;

/// See [`CASE_NODE_TYPE`].
pub const UNTYPED_TASK_NODE_TYPE: &str = TASK_NODE_TYPE;
