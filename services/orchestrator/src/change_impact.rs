//! Phase 3 (Change Impact Intelligence + Smart Regression).
//!
//! ## What this is
//!
//! Change impact **resolution** plus **regression selection**, built entirely from
//! relations the system already stores. No new engine, no new graph vocabulary, and no
//! string similarity anywhere.
//!
//! ## Why the table alone was not the gap
//!
//! `change_impacts` had zero writers for its entire life. The reason was not a missing
//! `INSERT` statement: there was no module, no API, no caller, and — decisively — **no
//! source of change data at all**. The backend has no git integration, no diff parser, no
//! commit or PR concept (`git2`, `commit_sha`, `rev-parse`: zero occurrences). So the
//! "unfinished feature" reading of an empty table was wrong in one direction and right in
//! another: the table was genuinely a stub, and no amount of writing to it would have
//! produced a *true* impact record, because there was nothing truthful to record.
//!
//! ## How a change enters the system
//!
//! A change is **declared by a caller** — a `change_ref` plus one or more targets — and
//! this module's job is to resolve it against the graph. The declaration is the evidence
//! that a change occurred (the same footing as an operator declaring an incident); the
//! *impact* is then derived, never guessed.
//!
//! ## The kinds of change this repository can actually resolve
//!
//! | Target kind | Resolvable? | Why |
//! |---|---|---|
//! | `requirement` | **yes** | `quality_nodes` holds a `requirement` vertex keyed on the requirement key, and `validates` edges run case → requirement |
//! | `test_case` | **yes** | the case vertex is keyed on `test_cases.id` |
//! | `failure_cluster` | **yes** | the cluster vertex is keyed on the telemetry fingerprint, and `fails_with` edges run case → cluster |
//! | `module` | vertex yes, **impact no** | no `covers` edge is ever written (Phase 2 limitation), so a module change has no provable impact |
//! | `api_contract`, `page`, `code_file`, `configuration`, `dependency` | **no** | no vertex of any such type exists anywhere in the schema |
//!
//! Everything in the last two rows returns `INSUFFICIENT_DATA` with a stated reason. That
//! is the correct answer, not a gap: manufacturing a link from a URL substring or a title
//! match would produce exactly the false traceability this work exists to prevent.
//!
//! ## Smart regression
//!
//! Selection is a **policy over the existing regression scope**, not a second engine.
//! [`crate::regression_runner::Scope::restricted_to`] narrows the scope the existing
//! `scope()` already built, moving everything it drops into that scope's own `skipped`
//! list with a reason, so a dropped case is always visible.
//!
//! Two rules the selection obeys:
//!
//! * **The safety floor is never subtracted.** A case with recorded failure evidence is
//!   retained even when nothing links it to the change. Reducing runtime by dropping
//!   tests that have already caught something is the failure mode this is built to avoid.
//! * **Priority is a total order over tiers**, not a score, so the same inputs always
//!   produce the same selection.

use crate::store::{
    QualityNodeRow, QualityRelationRow, Store, LEGACY_TEST_NODE_TYPE, TASK_NODE_TYPE,
    TEST_NODE_TYPE,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// Ceiling on impacted test cases reported for one change.
pub const IMPACT_MAX_TESTS: usize = 500;
/// Ceiling on regression candidates considered per project.
pub const SELECTION_MAX_CANDIDATES: usize = 2000;
/// Default read bound for the impact history.
pub const CHANGE_IMPACT_LIST_MAX: i64 = 500;

/// Risk score at or above which a case is treated as high-risk on its own account.
///
/// A threshold rather than a ranking, so "high risk" is a stated number a reader can
/// check instead of a relative judgement.
pub const HIGH_RISK_THRESHOLD: f64 = 0.7;

// ------------------------------------------------------------------ change declaration

/// One thing that changed, as the caller states it.
///
/// `kind` is checked against what the schema can actually resolve — see the module table.
/// An unknown kind is refused rather than treated as "probably a module".
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChangeTarget {
    pub kind: String,
    pub ref_id: String,
}

/// **The** supported change-target set for this build. Q5 (Phase 7.3).
///
/// This used to be stated twice by hand — once here, once as a `matches!` arm inside
/// [`ChangeImpactReader::resolve`] — and the two disagreed: `module` appeared in the `matches!`
/// accept-list while this table refused it, so a target could be accepted by one component and
/// silently refused by another. There is now exactly one list, and every other check derives from
/// it.
///
/// **Phase 7 approved scope (Q3):** `requirement`, `test_case`, `failure_cluster`. Nothing else.
///
/// | kind | supported | why |
/// |---|---|---|
/// | `requirement` | yes | `quality_nodes` holds a `requirement` vertex written by `link_case_to_requirement` |
/// | `test_case` | yes | `quality_nodes` holds a `test` vertex written by `upsert_quality_node` |
/// | `failure_cluster` | yes | `quality_nodes` holds a `failure_cluster` vertex keyed on the fingerprint |
/// | `module` | **no (deferred, Q4)** | the vertex type exists but **no authoritative module-identity source does** |
/// | `api_contract` | no | no vertex type records an API contract anywhere in the schema |
/// | `page` | no | no vertex type records a page or route; a target URL is not a stored identity and matching on it would be a string guess |
/// | `code_file` | no | the backend has no git integration, no diff parser and no file-to-test mapping |
/// | `configuration` | no | nothing records which tests a configuration change affects |
/// | `dependency` | no | no manifest or lockfile is read anywhere in the backend |
/// | `requirement_change` | no | a synonym for `requirement`, not a separate identity; declaring the requirement itself is correct |
/// | anything else | no | not a kind this build knows |
pub const SUPPORTED_CHANGE_KINDS: &[&str] = &["requirement", "test_case", "failure_cluster"];

/// Whether `kind` is one this build resolves. The single answer every caller must use.
pub fn is_supported_change_kind(kind: &str) -> bool {
    SUPPORTED_CHANGE_KINDS.contains(&kind.trim())
}

/// The human-readable list, for error messages and docs.
pub fn supported_change_kinds_joined() -> String {
    SUPPORTED_CHANGE_KINDS
        .iter()
        .map(|k| format!("`{k}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The change kinds this build resolves, and why the others cannot.
///
/// Returns `None` **only** for a supported kind, so a supported kind can never be refused by the
/// note table and an unsupported kind can never fall through as if it were supported. Both
/// directions are covered by `phase_7_3_*` tests.
fn resolution_note(kind: &str) -> Option<&'static str> {
    match kind {
        // The supported set. `None` means "resolvable", checked against
        // SUPPORTED_CHANGE_KINDS so the two cannot drift.
        "requirement" | "test_case" | "failure_cluster" => None,
        // Q4 (Phase 7.3): deferred. The prerequisite is an authoritative module-identity source,
        // not this edge. `store::link_test_case_to_module` and `store::link_case_to_module` both
        // write a `covers` edge and both are **uncalled by production code** -- `trace.rs` records
        // that as deliberate -- and no column anywhere stores which module a case belongs to.
        // Wiring a writer before such a source exists would mean inferring ownership from a
        // document or task name, which is a heuristic presented as fact.
        "module" => Some(
            "module-change impact is deferred and not supported by this build: nothing in the \
             schema records an authoritative module identity for a test case, so a `covers` edge \
             has no defensible source. Declaring a `requirement`, `test_case` or \
             `failure_cluster` target derives real impact today",
        ),
        "api_contract" => Some("no vertex type records an API contract anywhere in the schema"),
        "page" => Some(
            "no vertex type records a page or route; a target URL is not a stored identity \
             and matching on it would be a string guess",
        ),
        "code_file" => Some(
            "the backend has no git integration, no diff parser and no file-to-test mapping, \
             so a code file cannot be resolved to anything",
        ),
        "configuration" => Some("nothing records which tests a configuration change affects"),
        "dependency" => Some(
            "no manifest or lockfile is read anywhere in the backend, so dependency changes \
             have no stored representation",
        ),
        "requirement_change" => {
            Some("declare the requirement itself as a `requirement` target instead")
        }
        _ => None,
    }
}

/// What a target resolved to.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome")]
pub enum Resolution {
    /// The target matched exactly one stored vertex in this project.
    Resolved {
        node_id: Uuid,
        node_type: String,
        external_ref: Option<String>,
        name: String,
    },
    /// The target could not be resolved. `reason` states why, and `marker` is always
    /// `INSUFFICIENT_DATA` so a caller can branch on it without string matching.
    Insufficient {
        marker: &'static str,
        reason: String,
    },
}

// ------------------------------------------------------------------ impact tiers

/// Why a case is in the selected set. Ordered by priority; see [`selection_rank`].
///
/// `SafetyFloor` and `HighRisk` are the two tiers that exist *independently of the
/// change*: they are why the selection cannot shrink below a floor just because a change
/// is small.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// The case itself changed.
    DirectlyImpacted,
    /// The case shares a recorded failure with a directly impacted case.
    RelatedFailure,
    /// The case has recorded failure evidence, so it is never dropped for speed.
    SafetyFloor,
    /// The case's recorded risk is at or above [`HIGH_RISK_THRESHOLD`].
    HighRisk,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DirectlyImpacted => "directly_impacted",
            Self::RelatedFailure => "related_failure",
            Self::SafetyFloor => "safety_floor",
            Self::HighRisk => "high_risk",
        }
    }

    /// What a case in this tier is kept *for*, in one clause. Attached to every selected
    /// case so the selection explains itself rather than requiring the reader to
    /// reconstruct the policy.
    pub fn rationale(self) -> &'static str {
        match self {
            Self::DirectlyImpacted => {
                "the change names this test case, or a requirement or \
                 failure this case is linked to by a stored relation"
            }
            Self::RelatedFailure => {
                "this case has failed with the same recorded failure signature as a directly \
                 impacted case"
            }
            Self::SafetyFloor => {
                "this case has recorded failure evidence, so it is retained regardless of the \
                 change; dropping it to save runtime is exactly what must not happen"
            }
            Self::HighRisk => {
                "this case's recorded risk is at or above the stated threshold, so it is \
                 retained on its own account"
            }
        }
    }
}

fn selection_rank(t: Tier) -> u8 {
    match t {
        Tier::DirectlyImpacted => 0,
        Tier::RelatedFailure => 1,
        Tier::SafetyFloor => 2,
        Tier::HighRisk => 3,
    }
}

// ------------------------------------------------------------------ results

/// One impacted test case, with the tier it entered by and the relation used.
#[derive(Debug, Clone, Serialize)]
pub struct ImpactedTest {
    pub test_case_id: Uuid,
    pub title: String,
    pub tier: Tier,
    pub priority: u8,
    /// The stored edge or key this was derived from, so the claim can be checked.
    pub via: String,
}

/// A case that was considered and **not** selected, with the reason.
///
/// Recorded rather than dropped silently: a regression run that executes 12 of 60 cases
/// must be able to say what happened to the other 48.
#[derive(Debug, Clone, Serialize)]
pub struct NotSelected {
    pub test_case_id: Uuid,
    pub title: String,
    pub reason: String,
}

/// One declared target and what it resolved to.
#[derive(Debug, Clone, Serialize)]
pub struct TargetOutcome {
    pub target: ChangeTarget,
    pub resolution: Resolution,
    /// Test cases reached from this target, if any.
    pub impacted: Vec<ImpactedTest>,
}

/// The full answer for one declared change.
#[derive(Debug, Clone, Serialize)]
pub struct ChangeImpact {
    pub project_id: String,
    pub change_ref: String,
    pub change_type: String,
    pub targets: Vec<TargetOutcome>,
    /// The union across every target, deduplicated and ordered by priority.
    pub impacted_tests: Vec<ImpactedTest>,
    /// Cases the change did not touch and that carry no risk or failure evidence.
    pub not_selected: Vec<NotSelected>,
    /// Cases retained only because they carry recorded evidence. Reported separately so a
    /// reader can audit the floor rather than take it on trust.
    pub safety_floor: Vec<Uuid>,
    /// Populated only when nothing could be resolved at all.
    pub insufficient_reason: Option<String>,
    /// Rows actually written by this call; `false` entries were already recorded.
    pub recorded: usize,
    pub already_recorded: usize,
}

// ------------------------------------------------------------------ reader

/// Resolves declared changes against the stored graph and selects a regression set.
#[derive(Clone)]
pub struct ChangeImpactReader {
    store: Store,
}

impl ChangeImpactReader {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Resolve one declared target to a stored vertex, or explain why it cannot be.
    ///
    /// Matching is **exact** on `external_ref` within the project. No prefix matching, no
    /// case folding, no substring: a near miss is reported as unresolved, because a
    /// requirement key that resembles another is precisely where a heuristic would invent
    /// an impact.
    pub async fn resolve(&self, project_id: &str, target: &ChangeTarget) -> Result<Resolution> {
        let kind = target.kind.trim();
        let ref_id = target.ref_id.trim();

        if ref_id.is_empty() {
            return Ok(Resolution::Insufficient {
                marker: "INSUFFICIENT_DATA",
                reason: "the target carries no reference to resolve".to_string(),
            });
        }
        if let Some(note) = resolution_note(kind) {
            return Ok(Resolution::Insufficient {
                marker: "INSUFFICIENT_DATA",
                reason: note.to_string(),
            });
        }
        // Q5 (Phase 7.3): the accept-list is the shared constant, not a second hand-written
        // `matches!`. `module` is no longer in it (Q3/Q4), so it now takes the same path as every
        // other unsupported kind instead of being accepted here and refused above.
        if !is_supported_change_kind(kind) {
            return Ok(Resolution::Insufficient {
                marker: "INSUFFICIENT_DATA",
                reason: format!(
                    "`{kind}` is not a change kind this build can resolve; the supported kinds \
                     are {}",
                    supported_change_kinds_joined()
                ),
            });
        }

        // A test case is addressed by its UUID. Parsed rather than string-compared so a
        // malformed id is a clear refusal instead of a silent no-match.
        if kind == "test_case" && Uuid::parse_str(ref_id).is_err() {
            return Ok(Resolution::Insufficient {
                marker: "INSUFFICIENT_DATA",
                reason: "a test_case target must be a test_cases.id (a UUID)".to_string(),
            });
        }

        // Every arm here is a supported kind (Q5), so the fallback is unreachable rather than a
        // silent `module` default that contradicted the refusal table.
        let wanted: &[&str] = match kind {
            "test_case" => &[TEST_NODE_TYPE, LEGACY_TEST_NODE_TYPE],
            "requirement" => &["requirement"],
            "failure_cluster" => &["failure_cluster"],
            _ => &[],
        };

        let all_nodes: Vec<QualityNodeRow> = self.store.list_quality_nodes(project_id).await?;
        let matches: Vec<&QualityNodeRow> = all_nodes
            .iter()
            .filter(|n| wanted.contains(&n.node_type.as_str()))
            .filter(|n| n.external_ref.as_deref() == Some(ref_id))
            .collect();

        Ok(match matches.len() {
            1 => {
                let n = matches[0];
                Resolution::Resolved {
                    node_id: n.id,
                    node_type: n.node_type.clone(),
                    external_ref: n.external_ref.clone(),
                    name: n.name.clone(),
                }
            }
            0 => Resolution::Insufficient {
                marker: "INSUFFICIENT_DATA",
                reason: format!(
                    "no `{kind}` in this project is keyed on `{ref_id}`; the graph holds no \
                     vertex for it, so its impact cannot be derived"
                ),
            },
            n => Resolution::Insufficient {
                marker: "INSUFFICIENT_DATA",
                reason: format!(
                    "`{ref_id}` matches {n} `{kind}` vertices in this project; refusing to \
                     guess which one changed"
                ),
            },
        })
    }

    /// Test cases reachable from one vertex, by stored relation only.
    ///
    /// Three relations point *into* the vertex from a case: `validates` (case →
    /// requirement), `fails_with` (case → cluster) and `covers` (case → module). All
    /// three are read as reverse edges, so the direction the schema already uses is not
    /// reversed, and a case that is itself a vertex contributes directly.
    ///
    /// `related` then adds one hop of genuine dependency: cases that failed with the same
    /// fingerprint as a directly reached case. That is a recorded relationship - both
    /// clusters exist, both edges exist - not a guess.
    pub async fn impacted_from_node(
        &self,
        project_id: &str,
        node_id: Uuid,
    ) -> Result<Vec<ImpactedTest>> {
        let nodes: Vec<QualityNodeRow> = self.store.list_quality_nodes(project_id).await?;
        let relations: Vec<QualityRelationRow> = self
            .store
            .list_quality_relations_for_project(project_id)
            .await?;
        let telemetry = self
            .store
            .list_execution_telemetry(project_id, crate::qa_memory::TELEMETRY_READ_MAX)
            .await?;

        // Which nodes are case vertices, and which real case each is.
        let case_of: BTreeMap<Uuid, Uuid> = nodes
            .iter()
            .filter(|n| n.node_type == TEST_NODE_TYPE || n.node_type == LEGACY_TEST_NODE_TYPE)
            .filter_map(|n| {
                let id = Uuid::parse_str(n.external_ref.as_deref()?).ok()?;
                Some((n.id, id))
            })
            .collect();

        let mut found: BTreeMap<Uuid, (Tier, String)> = BTreeMap::new();

        // The vertex may itself be the changed case.
        if let Some(case_id) = case_of.get(&node_id).copied() {
            found.insert(
                case_id,
                (
                    Tier::DirectlyImpacted,
                    "the changed vertex is this test case".to_string(),
                ),
            );
        }

        // Reverse edges into the vertex.
        for rel in relations.iter().filter(|r| r.to_node_id == node_id) {
            let from_node_id = rel.from_node_id;
            let relation_type = rel.relation_type.as_str();
            if !matches!(relation_type, "validates" | "fails_with" | "covers") {
                continue;
            }
            let Some(case_id) = case_of.get(&from_node_id).copied() else {
                continue;
            };
            found.entry(case_id).or_insert((
                Tier::DirectlyImpacted,
                format!("this case is `{relation_type}`-linked to the changed vertex"),
            ));
        }

        // One hop of real dependency: same recorded failure signature.
        let direct: BTreeSet<Uuid> = found.keys().copied().collect();
        if !direct.is_empty() {
            let fingerprints: BTreeSet<&str> = telemetry
                .iter()
                .filter(|r| r.test_case_id.map(|c| direct.contains(&c)) == Some(true))
                .filter_map(|r| r.failure_fingerprint.as_deref())
                .filter(|f| !f.is_empty())
                .collect();
            if !fingerprints.is_empty() {
                // The cluster vertices keyed on those fingerprints.
                let cluster_nodes: BTreeSet<Uuid> = nodes
                    .iter()
                    .filter(|n| n.node_type == "failure_cluster")
                    .filter(|n| {
                        n.external_ref.as_deref().map(|r| fingerprints.contains(r)) == Some(true)
                    })
                    .map(|n| n.id)
                    .collect();
                for rel in relations
                    .iter()
                    .filter(|r| r.relation_type == "fails_with")
                    .filter(|r| cluster_nodes.contains(&r.to_node_id))
                {
                    let Some(case_id) = case_of.get(&rel.from_node_id).copied() else {
                        continue;
                    };
                    if direct.contains(&case_id) {
                        continue;
                    }
                    found.entry(case_id).or_insert((
                        Tier::RelatedFailure,
                        "this case is `fails_with`-linked to a cluster a directly impacted \
                         case also failed with"
                            .to_string(),
                    ));
                }
            }
        }

        let titles: BTreeMap<Uuid, String> = self
            .store
            .list_test_cases(project_id, None, None)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|c| (c.id, c.title))
            .collect();

        let mut out: Vec<ImpactedTest> = found
            .into_iter()
            .map(|(case_id, (tier, via))| ImpactedTest {
                test_case_id: case_id,
                title: titles
                    .get(&case_id)
                    .cloned()
                    .unwrap_or_else(|| "(case not readable in this project)".to_string()),
                tier,
                priority: selection_rank(tier),
                via,
            })
            .collect();
        out.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then(a.test_case_id.cmp(&b.test_case_id))
        });
        out.truncate(IMPACT_MAX_TESTS);
        Ok(out)
    }

    /// Cases with recorded failure evidence — the safety floor.
    ///
    /// Derived, not declared: there is no mandatory-test flag in the schema (Phase 3
    /// verified this — no such concept exists anywhere in the codebase). What *can* be
    /// read honestly is "this case has actually failed", which is the strongest available
    /// proxy for "must not be dropped to save runtime". Documented as a limitation: a real
    /// mandatory flag is a schema decision.
    pub async fn safety_floor(&self, project_id: &str) -> Result<BTreeMap<Uuid, String>> {
        let mut floor: BTreeMap<Uuid, String> = BTreeMap::new();

        let telemetry = self
            .store
            .list_execution_telemetry(project_id, crate::qa_memory::TELEMETRY_READ_MAX)
            .await?;
        for r in &telemetry {
            let (Some(case_id), Some(fp)) = (r.test_case_id, r.failure_fingerprint.as_deref())
            else {
                continue;
            };
            if fp.is_empty() {
                continue;
            }
            floor
                .entry(case_id)
                .and_modify(|why| why.push_str(", recorded failure"))
                .or_insert_with(|| format!("failed with fingerprint {fp}"));
        }

        // A case whose failure produced a **bug draft** is the strongest form of "has caught
        // something": a human-reviewable defect record exists against it. Resolved
        // through the draft's cluster and that cluster's fingerprint, both stored keys -
        // draft -> cluster -> fingerprint -> the cases that failed with it.
        let drafts = self.store.list_bug_drafts(Some(project_id)).await?;
        for d in drafts {
            let cluster = self
                .store
                .get_failure_cluster_by_id(d.failure_cluster_id)
                .await?;
            let Some(fp) = cluster.and_then(|c| c.failure_fingerprint) else {
                continue;
            };
            if fp.is_empty() {
                continue;
            }
            for r in telemetry
                .iter()
                .filter(|r| r.failure_fingerprint.as_deref() == Some(fp.as_str()))
            {
                let Some(case_id) = r.test_case_id else {
                    continue;
                };
                floor
                    .entry(case_id)
                    .and_modify(|why| why.push_str(", has a bug draft against it"))
                    .or_insert_with(|| format!("has bug draft {} against its failure", d.id));
            }
        }

        Ok(floor)
    }

    /// Recorded risk per case vertex, from the existing intelligence read.
    ///
    /// Reuses [`Store::list_test_intelligence`] rather than adding a query, so risk is
    /// read exactly as `GET /intelligence/tests` reads it and cannot disagree with that
    /// view.
    pub async fn risk_scores(&self, project_id: &str) -> Result<BTreeMap<Uuid, f64>> {
        let nodes = self.store.list_quality_nodes(project_id).await?;
        let case_node: BTreeMap<Uuid, Uuid> = nodes
            .iter()
            .filter(|n| n.node_type == TEST_NODE_TYPE || n.node_type == LEGACY_TEST_NODE_TYPE)
            .filter_map(|n| {
                let id = Uuid::parse_str(n.external_ref.as_deref()?).ok()?;
                Some((n.id, id))
            })
            .collect();
        Ok(self
            .store
            .list_test_intelligence(Some(project_id))
            .await?
            .into_iter()
            .filter_map(|row| {
                let case_id = case_node.get(&row.id).copied()?;
                Some((case_id, row.risk_score?))
            })
            .collect())
    }

    /// Full analysis of one declared change, and the writes it produces.
    ///
    /// Records one row per (target, selected case) and one row per resolved target that
    /// selected nothing. Re-running the same `change_ref` is idempotent: the unique index
    /// from migration 0031 turns a repeat into `already_recorded` rather than a duplicate.
    pub async fn analyse(
        &self,
        project_id: &str,
        change_ref: &str,
        change_type: &str,
        targets: &[ChangeTarget],
    ) -> Result<ChangeImpact> {
        let mut outcomes: Vec<TargetOutcome> = Vec::new();
        let mut union: BTreeMap<Uuid, ImpactedTest> = BTreeMap::new();
        let mut any_resolved = false;

        for target in targets {
            let resolution = self.resolve(project_id, target).await?;
            let impacted = match &resolution {
                Resolution::Resolved { node_id, .. } => {
                    any_resolved = true;
                    let found = self.impacted_from_node(project_id, *node_id).await?;
                    for i in &found {
                        union
                            .entry(i.test_case_id)
                            .and_modify(|e| {
                                // Keep the strongest tier if two targets reach one case.
                                if i.priority < e.priority {
                                    e.tier = i.tier;
                                    e.priority = i.priority;
                                    e.via = i.via.clone();
                                }
                            })
                            .or_insert_with(|| i.clone());
                    }
                    found
                }
                Resolution::Insufficient { .. } => Vec::new(),
            };
            outcomes.push(TargetOutcome {
                target: target.clone(),
                resolution,
                impacted,
            });
        }

        let floor = self.safety_floor(project_id).await?;
        let risk = self.risk_scores(project_id).await?;

        // Apply the two change-independent tiers to the union, then select.
        let mut selected: BTreeMap<Uuid, ImpactedTest> = union.clone();
        for (case_id, why) in &floor {
            if selected.contains_key(case_id) {
                continue;
            }
            selected.insert(
                *case_id,
                ImpactedTest {
                    test_case_id: *case_id,
                    title: String::new(),
                    tier: Tier::SafetyFloor,
                    priority: selection_rank(Tier::SafetyFloor),
                    via: why.clone(),
                },
            );
        }
        for (case_id, score) in &risk {
            if *score < HIGH_RISK_THRESHOLD || selected.contains_key(case_id) {
                continue;
            }
            selected.insert(
                *case_id,
                ImpactedTest {
                    test_case_id: *case_id,
                    title: String::new(),
                    tier: Tier::HighRisk,
                    priority: selection_rank(Tier::HighRisk),
                    via: format!("recorded risk score {score} >= {HIGH_RISK_THRESHOLD}"),
                },
            );
        }

        let mut selected_list: Vec<ImpactedTest> = selected.into_values().collect();
        selected_list.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then(a.test_case_id.cmp(&b.test_case_id))
        });
        selected_list.truncate(SELECTION_MAX_CANDIDATES);

        // Everything the project can run that was not selected, with a reason.
        let chosen: BTreeSet<Uuid> = selected_list.iter().map(|i| i.test_case_id).collect();
        let not_selected: Vec<NotSelected> = self
            .store
            .list_test_cases(project_id, None, None)
            .await?
            .into_iter()
            .filter(|c| !chosen.contains(&c.id))
            .map(|c| NotSelected {
                test_case_id: c.id,
                title: c.title,
                reason: "no stored relation links this case to the change, and it has no \
                         recorded failure evidence or risk score at or above the threshold"
                    .to_string(),
            })
            .collect();

        // ---- persist ----
        // Q7 (Phase 7.3): a refused target leaves **no** `change_impacts` row, because that
        // table's rows all mean "this node's impact was derived". Writing one for an unresolved
        // target would invent that claim, so it is not written. `change_impacts` has no status
        // column and no migration is permitted here, so the refusal is recorded where the schema
        // *does* support it unambiguously: `audit_log`, whose `outcome` column is exactly
        // 'ok' | 'denied' | 'error' and whose `target` can carry the change_ref.
        //
        // The audit trail is not an impact record. `list_change_impacts` still returns nothing for
        // a refused change, so nothing downstream can mistake this for a resolved impact.
        for outcome in &outcomes {
            if let Resolution::Insufficient { marker, reason } = &outcome.resolution {
                self.store
                    .insert_audit(
                        None,
                        None,
                        Some(project_id),
                        "change_impact.unsupported_target",
                        Some(change_ref),
                        Some(&format!(
                            "{} target `{}` was not resolved ({}): {}",
                            outcome.target.kind, outcome.target.ref_id, marker, reason
                        )),
                        "denied",
                    )
                    .await?;
            }
        }

        let mut recorded = 0usize;
        let mut already = 0usize;
        for outcome in &outcomes {
            let Resolution::Resolved { node_id, .. } = &outcome.resolution else {
                continue;
            };
            if outcome.impacted.is_empty() {
                // A resolved node with no impact is a real negative result; record it once
                // with no selected test rather than recording nothing at all.
                if let Some(f) = SUPPORTED_CHANGE_KINDS
                    .iter()
                    .find(|k| outcome.target.kind.trim() == **k)
                {
                    match self
                        .store
                        .insert_change_impact(
                            project_id,
                            change_ref,
                            change_type,
                            node_id,
                            None,
                            Some(&format!(
                                "`{f}` resolves to a graph vertex but no test case is linked \
                                 to it by any stored relation"
                            )),
                        )
                        .await
                    {
                        Ok((_, true)) => recorded += 1,
                        Ok((_, false)) => already += 1,
                        Err(_) => {}
                    }
                }
                continue;
            }
            for i in &outcome.impacted {
                let node_id = match self.test_node_id(project_id, i.test_case_id).await? {
                    Some(n) => n,
                    None => continue,
                };
                match self
                    .store
                    .insert_change_impact(
                        project_id,
                        change_ref,
                        change_type,
                        &node_id,
                        Some(&node_id),
                        Some(&format!(
                            "{} [{}] via {}",
                            i.tier.as_str(),
                            i.tier.rationale(),
                            i.via
                        )),
                    )
                    .await
                {
                    Ok((_, true)) => recorded += 1,
                    Ok((_, false)) => already += 1,
                    Err(_) => {}
                }
            }
        }

        Ok(ChangeImpact {
            project_id: project_id.to_string(),
            change_ref: change_ref.to_string(),
            change_type: change_type.to_string(),
            targets: outcomes,
            impacted_tests: selected_list,
            not_selected,
            safety_floor: floor.keys().copied().collect(),
            insufficient_reason: (!any_resolved).then(|| {
                "no declared target resolved to a stored graph vertex in this project, so no \
                 impact could be derived. This is reported rather than guessed."
                    .to_string()
            }),
            recorded,
            already_recorded: already,
        })
    }

    /// The case vertex id for a test case, if this project has one.
    async fn test_node_id(&self, project_id: &str, case_id: Uuid) -> Result<Option<Uuid>> {
        let want = case_id.to_string();
        Ok(self
            .store
            .list_quality_nodes(project_id)
            .await?
            .into_iter()
            .find(|n| {
                (n.node_type == TEST_NODE_TYPE || n.node_type == LEGACY_TEST_NODE_TYPE)
                    && n.external_ref.as_deref() == Some(want.as_str())
            })
            .map(|n| n.id))
    }
}

// ------------------------------------------------------------------ node-type reference

/// Re-exported so a caller can tell a case vertex from a task vertex without reaching
/// into `store`, and so the unused-import warning does not hide a genuine dependency.
pub const UNLINKED_TASK_NODE_TYPE: &str = TASK_NODE_TYPE;
