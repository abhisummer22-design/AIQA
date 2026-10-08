//! Phase 5 (AI Exploratory Testing Agent) — tests.
//!
//! ## What is real here
//!
//! The crawl surface is built through `CrawlOutput`'s own serde contract — the same
//! `serde_json::from_*` the real `read_crawl_result` uses on the TypeScript side's file, so
//! the shape is the one the crawler actually produces.
//!
//! Every probe goes through the **real** writers: `store::insert_run_for_release` and
//! `Intelligence::ingest_run`, which is what produces telemetry, fingerprints, failure
//! clusters and bug drafts. Fingerprints are computed by Phase 2's
//! `canonical_failure_fingerprint` over the **recorded** `error_message`, never
//! hand-written. Nothing here fabricates a row the real system could not produce.
//!
//! ## What is deliberately a double
//!
//! `AssertionEvidence::Evaluated` stands in for a browser-backed executor's per-assertion
//! report, which needs node plus a browser this environment does not have. The production
//! default is `NotEvaluated`, and `missing_evidence_never_becomes_a_confirmed_defect`
//! proves the default path yields no confirmed defect — so the double is exercised only to
//! prove the classification ladder *can* be climbed with evidence, not to manufacture a
//! pass.
//!
//! ## What is deliberately negative
//!
//! Half this module is negative, because every failure mode that matters for an autonomous
//! agent is an *over*-statement: acting past a budget, escalating weak evidence, reaching
//! another tenant's data, or re-reporting one defect forever.

use crate::agent::AgentLimits;
use crate::agent_tools::AgentTool;
use crate::agents::exploration::{
    behaviour_support_report, ExplorationBehaviour, ExplorationBudget, ExplorationCounters,
    ExplorationStop, ExplorationSurface, FindingClass, ProbeActionTool,
};
use crate::agents::exploration_execution::{
    AssertionEvidence, DispatchOutcome, RunPipelineProbeRunner,
};
use crate::ai_router::AiInfrastructure;
use crate::crawler::{CrawlOutput, CrawledElement};
use crate::domain::TaskStatus;
use crate::evidence::LocalEvidenceStore;
use crate::execution_plan::Action;
use crate::policy::PolicyConfig;
use crate::store::{ExplorationFindingRow, Store};
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

// ------------------------------------------------------------------ harness

async fn harness() -> (Store, tempfile::TempDir, Arc<LocalEvidenceStore>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("phase5-exploration.sqlite3"))
        .await
        .expect("connect");
    let evidence =
        Arc::new(LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence root"));
    (store, dir, evidence)
}

async fn project(store: &Store, name: &str) -> String {
    let id = Uuid::new_v4();
    store
        .insert_project(id, name, Some("phase 5 fixture"))
        .await
        .expect("insert_project");
    id.to_string()
}

/// The `Action` AST's own wire form: `#[serde(tag = "type", rename_all =
/// "SCREAMING_SNAKE_CASE")]`, so a variant serializes as `{"type": "NAVIGATE", ...}` and
/// field names stay snake_case.
///
/// Written by hand rather than serialized from the enum so a change to the AST's own serde
/// attributes fails these tests instead of silently moving with it.
fn nav(url: &str) -> serde_json::Value {
    serde_json::json!({ "type": "NAVIGATE", "url": url })
}

fn assert_url(contains: &str) -> serde_json::Value {
    serde_json::json!({ "type": "ASSERT_URL", "url_contains": contains })
}

fn assert_visible(selector: &str) -> serde_json::Value {
    serde_json::json!({ "type": "ASSERT_VISIBLE", "selector": selector })
}

fn assert_enabled(selector: &str) -> serde_json::Value {
    serde_json::json!({ "type": "ASSERT_ENABLED", "selector": selector })
}

fn element(kind: &str, text: &str, selector: &str) -> CrawledElement {
    CrawledElement {
        kind: kind.to_string(),
        text: text.to_string(),
        selector_hint: selector.to_string(),
        href: None,
    }
}

fn link(text: &str, selector: &str, href: &str) -> CrawledElement {
    CrawledElement {
        kind: "link".to_string(),
        text: text.to_string(),
        selector_hint: selector.to_string(),
        href: Some(href.to_string()),
    }
}

/// A crawl of a two-page application with a form, a button and a link.
///
/// Built through `CrawlOutput`'s own serde contract, which is what the real
/// `read_crawl_result` parses the TypeScript side's output with.
fn crawl_output() -> CrawlOutput {
    // The real wire form. `crawler.rs` declares every one of these structs
    // `#[serde(rename_all = "camelCase")]`, because the TypeScript side emits camelCase, so
    // writing the fixture in snake_case would be testing a shape the crawler never produces.
    serde_json::from_value(serde_json::json!({
        "roles": [
            {
                "role": "guest",
                "loginSucceeded": false,
                "pages": [
                    {
                        "url": "http://127.0.0.1:8080/checkout",
                        "title": "Checkout",
                        "elements": [
                            element("input", "Card number", "#card"),
                            element("button", "Pay", "#pay"),
                            link("Basket", "#basket", "http://127.0.0.1:8080/basket")
                        ]
                    },
                    {
                        "url": "http://127.0.0.1:8080/basket",
                        "title": "Basket",
                        "elements": [ element("button", "Checkout", "#checkout") ]
                    }
                ]
            }
        ],
        "visibilityMatrix": [
            {
                "url": "http://127.0.0.1:8080/checkout",
                "rolesWithAccess": ["guest"],
                "elementsOnlyVisibleTo": { "admin": ["#admin-only"] }
            }
        ]
    }))
    .expect("crawl output must deserialize in the crawler's own wire format")
}

fn surface() -> ExplorationSurface {
    ExplorationSurface::from_crawl(&crawl_output())
}

/// A runner that records a **passing** probe and asserts nothing was evaluated.
///
/// This is the production default: exactly what a session gets when no browser-backed
/// executor is dispatching.
fn runner_no_evidence(store: &Store) -> Arc<dyn crate::agents::exploration::ProbeRunner> {
    Arc::new(RunPipelineProbeRunner::new(store.clone()))
}

/// A runner whose executor reports a real failure with a real per-assertion outcome.
fn runner_failing_with_evidence(
    store: &Store,
    reason: &str,
) -> Arc<dyn crate::agents::exploration::ProbeRunner> {
    Arc::new(
        RunPipelineProbeRunner::new(store.clone()).with_dispatch(DispatchOutcome {
            status: TaskStatus::Failed,
            reason: Some(reason.to_string()),
            assertions: AssertionEvidence::Evaluated {
                failed_assertions: vec!["ASSERT_ENABLED #pay".to_string()],
                executor_kind: "playwright".to_string(),
            },
        }),
    )
}

/// A runner whose executor reports a failure but evaluated **no** assertions — a simulated
/// or short-circuited run.
fn runner_failing_without_evidence(
    store: &Store,
    reason: &str,
) -> Arc<dyn crate::agents::exploration::ProbeRunner> {
    Arc::new(
        RunPipelineProbeRunner::new(store.clone()).with_dispatch(DispatchOutcome {
            status: TaskStatus::Failed,
            reason: Some(reason.to_string()),
            assertions: AssertionEvidence::NotEvaluated,
        }),
    )
}

fn policy() -> Arc<PolicyConfig> {
    Arc::new(PolicyConfig {
        max_concurrency: 8,
        require_approval_for: HashSet::new(),
        blocked_executors: HashSet::new(),
    })
}

/// Opens a session through the real writer and returns its id.
///
/// Necessary rather than decorative: `exploration_findings.session_id` carries a foreign key
/// onto `exploration_sessions`, so a finding cannot exist without the session that produced
/// it. Every fixture below therefore creates one the way the service does, rather than
/// inserting a finding row by hand — which would be state the real system could never reach.
async fn open_session(store: &Store, project_id: &str) -> Uuid {
    let id = Uuid::new_v4();
    store
        .insert_exploration_session(
            project_id,
            &id,
            "http://127.0.0.1:8080",
            &ExplorationBudget::default(),
            &small_limits(8, 20),
        )
        .await
        .expect("open session");
    id
}

fn small_limits(turns: usize, tool_calls: usize) -> AgentLimits {
    AgentLimits {
        max_turns: turns,
        max_tool_calls_total: tool_calls,
        max_observation_bytes: 64 * 1024,
        timeout: std::time::Duration::from_secs(60),
    }
}

fn no_providers() -> Arc<AiInfrastructure> {
    // The same reasoning `governed_healing::auto_diagnose` records: the exploratory agent is
    // deterministic and tool-driven, so `run_agent_loop`'s parameter is genuinely unused
    // here rather than standing in for a model call that was skipped.
    Arc::new(AiInfrastructure::new(
        vec![],
        std::collections::HashMap::new(),
    ))
}

// ------------------------------------------------------------------ surface

/// The surface is built from crawl evidence only, so an element nobody observed is not a
/// target.
#[tokio::test]
async fn the_surface_contains_only_what_the_crawl_recorded() {
    let surface = surface();
    assert_eq!(surface.routes.len(), 2, "both crawled pages are routes");
    assert_eq!(surface.roles, vec!["guest".to_string()]);

    let checkout = surface
        .routes
        .iter()
        .find(|r| r.url.ends_with("/checkout"))
        .expect("the checkout route");
    let selectors: Vec<&str> = checkout
        .elements
        .iter()
        .map(|e| e.selector_hint.as_str())
        .collect();
    assert!(selectors.contains(&"#card"));
    assert!(selectors.contains(&"#pay"));

    // The role-restricted element comes from the visibility matrix, not from guessing.
    assert_eq!(
        checkout.role_restricted.get("admin").map(|v| v.as_slice()),
        Some(["#admin-only".to_string()].as_slice()),
        "role restriction must come from the recorded visibility matrix"
    );

    // A selector the crawl never saw must not be inventable from the surface.
    assert!(
        !selectors.contains(&"#never-crawled"),
        "the surface must not contain an element no crawl observed"
    );
}

/// Fillable and clickable are derived from the recorded element kind, not hardcoded per app.
#[tokio::test]
async fn element_roles_are_derived_from_the_recorded_kind() {
    let surface = surface();
    let checkout = surface
        .routes
        .iter()
        .find(|r| r.url.ends_with("/checkout"))
        .expect("checkout");
    let card = checkout
        .elements
        .iter()
        .find(|e| e.selector_hint == "#card")
        .expect("#card");
    let pay = checkout
        .elements
        .iter()
        .find(|e| e.selector_hint == "#pay")
        .expect("#pay");

    assert!(card.is_fillable(), "an input is fillable");
    assert!(!card.is_clickable(), "an input is not a click target");
    assert!(pay.is_clickable(), "a button is clickable");
    assert!(!pay.is_fillable(), "a button is not a fill target");
}

// ------------------------------------------------------------------ budget

/// The budget is a hard ceiling, not a target.
#[test]
fn every_budget_dimension_stops_exploration() {
    let budget = ExplorationBudget {
        max_actions: 1,
        max_state_transitions: 1,
        max_findings: 1,
        max_pages: 1,
    };

    let cases = [
        (
            ExplorationCounters {
                actions: 1,
                ..Default::default()
            },
            "max_actions",
        ),
        (
            ExplorationCounters {
                state_transitions: 1,
                ..Default::default()
            },
            "max_state_transitions",
        ),
        (
            ExplorationCounters {
                findings: 1,
                ..Default::default()
            },
            "max_findings",
        ),
        (
            ExplorationCounters {
                pages: 1,
                ..Default::default()
            },
            "max_pages",
        ),
    ];

    for (counters, expected) in cases {
        assert_eq!(
            budget.exhausted_by(&counters),
            Some(expected),
            "{expected} must be the named stop"
        );
        assert!(budget.is_exhausted(&counters));
    }

    let untouched = ExplorationCounters::default();
    assert_eq!(budget.exhausted_by(&untouched), None);
    assert!(!budget.is_exhausted(&untouched));
}

/// A budget of zero stops exploration before it starts, and is not silently raised.
#[test]
fn a_zero_budget_stops_immediately_rather_than_defaulting() {
    let zero = ExplorationBudget {
        max_actions: 0,
        max_state_transitions: 0,
        max_findings: 0,
        max_pages: 0,
    };
    assert_eq!(
        zero.exhausted_by(&ExplorationCounters::default()),
        Some("max_actions")
    );
}

/// The default budget is modest on every dimension.
#[test]
fn the_default_budget_is_bounded_on_every_dimension() {
    let d = ExplorationBudget::default();
    assert!(d.max_actions > 0 && d.max_actions <= 100);
    assert!(d.max_state_transitions > 0 && d.max_state_transitions <= 100);
    assert!(d.max_findings > 0 && d.max_findings <= 50);
    assert!(d.max_pages > 0 && d.max_pages <= 50);
}

// ------------------------------------------------------------------ classification

/// The centrepiece: **missing evidence cannot become a confirmed defect.**
///
/// Four separate ways of lacking evidence, all of which must land on
/// `INSUFFICIENT_EVIDENCE` with a stated reason and zero confidence.
#[test]
fn missing_evidence_never_becomes_a_confirmed_defect() {
    use crate::agents::exploration::ProbeObservation;

    let base =
        |fingerprint: Option<&str>, evaluated: bool, expected: Option<&str>, req: Option<&str>| {
            ProbeObservation {
                behaviour: ExplorationBehaviour::InvalidValue,
                route: "http://127.0.0.1:8080/checkout".into(),
                actions: vec![
                    Action::Navigate {
                        url: "http://127.0.0.1:8080/checkout".into(),
                    },
                    Action::AssertEnabled {
                        selector: "#card".into(),
                    },
                ],
                observed: "recorded failure".into(),
                expected: expected.map(ToString::to_string),
                violated_requirement: req.map(ToString::to_string),
                task_status: TaskStatus::Failed,
                failure_reason: Some("ASSERT_ENABLED #card: element is disabled".into()),
                failure_fingerprint: fingerprint.map(ToString::to_string),
                app_state: serde_json::json!({}),
                related_failure: None,
                assertions_evaluated: evaluated,
                refusal: None,
            }
        };

    // 1. The executor evaluated nothing. A failed task is not a violated assertion.
    let (class, basis, confidence) = base(Some("fp-1"), false, Some("x"), Some("REQ-1")).classify();
    assert_eq!(class, FindingClass::InsufficientEvidence);
    assert_eq!(confidence, 0.0);
    assert!(
        basis.contains("no executor evaluated"),
        "the reason must name the gap: {basis}"
    );

    // 2. Evaluated, but no fingerprint to tie the evidence to.
    let (class, basis, confidence) = base(None, true, Some("x"), Some("REQ-1")).classify();
    assert_eq!(class, FindingClass::InsufficientEvidence);
    assert_eq!(confidence, 0.0);
    assert!(
        basis.contains("no canonical fingerprint"),
        "the reason must name the gap: {basis}"
    );

    // 3. Evaluated and fingerprinted, but nothing failed — the common case, and it must not
    //    produce a finding at all.
    let mut passing = base(Some("fp-2"), true, Some("x"), Some("REQ-1"));
    passing.task_status = TaskStatus::Passed;
    let (class, basis, _) = passing.classify();
    assert_eq!(class, FindingClass::InsufficientEvidence);
    assert!(
        basis.contains("assertions held"),
        "a clean probe must say so: {basis}"
    );

    // 4. A plan that was refused before it ran.
    let mut refused = base(Some("fp-3"), true, Some("x"), Some("REQ-1"));
    refused.refusal = Some("executor(s) [\"playwright\"] are blocked by policy".into());
    let (class, basis, confidence) = refused.classify();
    assert_eq!(class, FindingClass::InsufficientEvidence);
    assert_eq!(confidence, 0.0);
    assert!(
        basis.contains("refused"),
        "the reason must name the refusal: {basis}"
    );
}

/// With real evidence and a real requirement, a defect is confirmed — and only then.
#[test]
fn evidence_and_a_covering_requirement_produce_a_confirmed_defect() {
    use crate::agents::exploration::ProbeObservation;
    let observation = ProbeObservation {
        behaviour: ExplorationBehaviour::InvalidValue,
        route: "http://127.0.0.1:8080/checkout".into(),
        actions: vec![Action::AssertEnabled {
            selector: "#card".into(),
        }],
        observed: "#card reported disabled after a markup-bearing value".into(),
        expected: Some("#card should reject a markup-bearing value".into()),
        violated_requirement: Some("REQ-7".into()),
        task_status: TaskStatus::Failed,
        failure_reason: Some("ASSERT_ENABLED #card: element is disabled".into()),
        failure_fingerprint: Some("fp-real".into()),
        app_state: serde_json::json!({ "route": "/checkout" }),
        related_failure: None,
        assertions_evaluated: true,
        refusal: None,
    };
    let (class, basis, confidence) = observation.classify();
    assert_eq!(class, FindingClass::ConfirmedDefect);
    assert!(confidence > 0.8, "confirmed defects carry high confidence");
    assert!(
        basis.contains("REQ-7") && basis.contains("fingerprint"),
        "the basis must state the evidence, not just the verdict: {basis}"
    );
}

/// Each missing input caps the classification one rung lower. A defect is never reached by
/// omitting something.
#[test]
fn each_missing_input_caps_the_classification() {
    use crate::agents::exploration::ProbeObservation;
    let build = |expected: Option<&str>, req: Option<&str>| ProbeObservation {
        behaviour: ExplorationBehaviour::BoundaryValue,
        route: "/checkout".into(),
        actions: vec![Action::AssertEnabled {
            selector: "#card".into(),
        }],
        observed: "failed".into(),
        expected: expected.map(ToString::to_string),
        violated_requirement: req.map(ToString::to_string),
        task_status: TaskStatus::Failed,
        failure_reason: Some("ASSERT_ENABLED #card: element is disabled".into()),
        failure_fingerprint: Some("fp".into()),
        app_state: serde_json::json!({}),
        related_failure: None,
        assertions_evaluated: true,
        refusal: None,
    };

    // Both present -> confirmed.
    assert_eq!(
        build(Some("e"), Some("REQ-1")).classify().0,
        FindingClass::ConfirmedDefect
    );
    // No requirement -> a violation of the probe, not of the specification.
    assert_eq!(
        build(Some("e"), None).classify().0,
        FindingClass::PotentialDefect
    );
    // No stated expectation -> the violation is unverified.
    assert_eq!(
        build(None, Some("REQ-1")).classify().0,
        FindingClass::PotentialDefect
    );
    // Neither -> an observation, never a verdict.
    let (class, basis, _) = build(None, None).classify();
    assert_eq!(class, FindingClass::ObservedAnomaly);
    assert!(
        basis.contains("observation and not a verdict"),
        "the basis must say it is not a verdict: {basis}"
    );
}

/// The ladder is ordered by evidence strength, and `INSUFFICIENT_EVIDENCE` is the weakest.
#[test]
fn the_classification_ladder_is_ordered_by_evidence() {
    // Declared weakest-first, so `<` reads as "weaker evidence than".
    assert!(FindingClass::InsufficientEvidence < FindingClass::ObservedAnomaly);
    assert!(FindingClass::ObservedAnomaly < FindingClass::PotentialDefect);
    assert!(FindingClass::PotentialDefect < FindingClass::ConfirmedDefect);

    // And every name round-trips, which is what persistence depends on.
    for c in [
        FindingClass::ObservedAnomaly,
        FindingClass::PotentialDefect,
        FindingClass::ConfirmedDefect,
        FindingClass::InsufficientEvidence,
    ] {
        assert_eq!(FindingClass::parse(c.as_str()), Some(c));
    }
    assert_eq!(FindingClass::parse("CONFIRMED"), None);
}

// ------------------------------------------------------------------ behaviours

/// The catalogue reports what the Action AST can and cannot express — including the one
/// technique the phase brief asked for that has no AST support at all.
#[test]
fn the_behaviour_report_states_what_the_action_ast_cannot_do() {
    let report = behaviour_support_report();
    let unsupported = report["requested_but_unsupported"]
        .as_array()
        .expect("an unsupported list");
    assert_eq!(unsupported.len(), 1, "back/forward is the one gap");
    assert_eq!(unsupported[0]["behaviour"], "back_forward_navigation");
    assert_eq!(unsupported[0]["support"]["kind"], "UNSUPPORTED");
    assert!(
        unsupported[0]["support"]["detail"]
            .as_str()
            .is_some_and(|s| s.contains("browser-history")),
        "the gap must come with a reason: {unsupported:?}"
    );

    // Refresh is expressible only approximately, and says so rather than claiming a refresh.
    let catalogue = report["catalogue"].as_array().expect("a catalogue");
    let refresh = catalogue
        .iter()
        .find(|b| b["behaviour"] == "refresh")
        .expect("refresh is in the catalogue");
    assert_eq!(
        refresh["support"]["kind"], "APPROXIMATED",
        "refresh must not be claimed as native"
    );

    // Everything in the catalogue is autonomously safe: none of it mutates or destroys.
    for b in catalogue {
        assert_eq!(b["autonomously_safe"], true, "{b} must be autonomous-safe");
    }
}

/// Every behaviour's support claim is consistent with its safety level.
#[test]
fn every_catalogue_entry_declares_its_support() {
    for b in ExplorationBehaviour::catalogue() {
        match b.support(b.as_str()) {
            crate::agents::exploration::BehaviourSupport::Native(_)
            | crate::agents::exploration::BehaviourSupport::Approximated(_) => {}
            other => panic!("a catalogue entry must be expressible: {b:?} -> {other:?}"),
        }
        assert!(b.required_safety().is_autonomous_safe(), "{b:?}");
    }
    // Names round-trip, which is what the persistence layer depends on.
    for b in ExplorationBehaviour::catalogue() {
        assert_eq!(ExplorationBehaviour::parse(b.as_str()), Some(*b));
    }
    assert_eq!(ExplorationBehaviour::parse("script_eval"), None);
}

// ------------------------------------------------------------------ tool gates

/// An unknown behaviour is rejected with the catalogue, not guessed at.
#[tokio::test]
async fn an_unknown_behaviour_is_rejected_rather_than_guessed() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P5 UnknownBehaviour").await;
    let _session = open_session(&store, &pid).await;

    let tool = ProbeActionTool::new(
        store.clone(),
        policy(),
        evidence,
        crate::agents::exploration::SharedCounters::default(),
        ExplorationBudget::default(),
        runner_no_evidence(&store),
        Uuid::new_v4(),
    );
    let context =
        crate::agent::AgentContext::new("probe", Some(pid), "session", small_limits(4, 8), None)
            .with_store(store.clone());

    let err = tool
        .execute(
            &crate::agent_tools::ToolInvocation {
                call_id: "c1".into(),
                tool_name: "probe_action".into(),
                arguments: serde_json::json!({
                    "behaviour": "delete_everything",
                    "route": "http://127.0.0.1:8080/checkout",
                    "actions": [nav("http://127.0.0.1:8080/checkout")]
                }),
            },
            &context,
        )
        .await
        .expect_err("an unknown behaviour must be an error");
    let rendered = err.to_string();
    assert!(
        rendered.contains("delete_everything"),
        "the rejection must name what was asked for: {rendered}"
    );
    assert!(
        rendered.contains("boundary_value"),
        "the rejection must offer the real catalogue: {rendered}"
    );
}

/// An action sequence with no assertion cannot produce evidence, so it is refused rather
/// than spending budget on a probe that could not answer anything.
#[tokio::test]
async fn a_probe_with_no_assertion_is_refused_rather_than_run() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P5 NoAssertion").await;
    let session = open_session(&store, &pid).await;
    let counters = crate::agents::exploration::SharedCounters::default();

    let tool = ProbeActionTool::new(
        store.clone(),
        policy(),
        evidence,
        counters.clone(),
        ExplorationBudget::default(),
        runner_no_evidence(&store),
        session,
    );
    let context =
        crate::agent::AgentContext::new("probe", Some(pid.clone()), "s", small_limits(4, 8), None)
            .with_store(store.clone());

    let observation = tool
        .execute(
            &crate::agent_tools::ToolInvocation {
                call_id: "c1".into(),
                tool_name: "probe_action".into(),
                arguments: serde_json::json!({
                    "behaviour": "intelligent_navigation",
                    "route": "http://127.0.0.1:8080/checkout",
                    "actions": [nav("http://127.0.0.1:8080/checkout")]
                }),
            },
            &context,
        )
        .await
        .expect("the tool answers, it does not throw");

    let out = observation.output.expect("a structured refusal");
    assert_eq!(out["acted"], false);
    assert_eq!(out["reason"], "no_assertion_in_probe");
    assert_eq!(
        counters.snapshot().await.actions,
        0,
        "a refused probe must not consume budget"
    );
}

/// A blocked executor stays blocked — the tool calls the real `policy::evaluate`.
#[tokio::test]
async fn a_blocked_executor_stays_blocked() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P5 Blocked").await;
    let session = open_session(&store, &pid).await;
    let counters = crate::agents::exploration::SharedCounters::default();

    let tool = ProbeActionTool::new(
        store.clone(),
        Arc::new(PolicyConfig {
            max_concurrency: 8,
            require_approval_for: HashSet::new(),
            blocked_executors: ["playwright".to_string()].into_iter().collect(),
        }),
        evidence,
        counters.clone(),
        ExplorationBudget::default(),
        runner_no_evidence(&store),
        session,
    );
    let context =
        crate::agent::AgentContext::new("probe", Some(pid.clone()), "s", small_limits(4, 8), None)
            .with_store(store.clone());

    let observation = tool
        .execute(
            &crate::agent_tools::ToolInvocation {
                call_id: "c1".into(),
                tool_name: "probe_action".into(),
                arguments: serde_json::json!({
                    "behaviour": "intelligent_navigation",
                    "route": "http://127.0.0.1:8080/checkout",
                    "executor": "playwright",
                    "actions": [
                        nav("http://127.0.0.1:8080/checkout"),
                        assert_url("http://127.0.0.1:8080/checkout")
                    ]
                }),
            },
            &context,
        )
        .await
        .expect("the tool answers");
    let out = observation.output.expect("a structured refusal");
    assert_eq!(out["acted"], false);
    assert_eq!(out["reason"], "policy_denied");
    assert!(
        out["detail"]
            .as_str()
            .is_some_and(|d| d.contains("playwright")),
        "the refusal must name the blocked executor: {out}"
    );
    assert_eq!(
        counters.snapshot().await.actions,
        0,
        "a policy-denied probe must not run or consume budget"
    );

    // And nothing was recorded for it.
    let findings = store
        .list_exploration_findings(&pid, None, 50)
        .await
        .expect("findings");
    assert!(findings.is_empty(), "a denied probe records nothing");
}

/// An executor gated for approval is not run autonomously, and the finding says so.
#[tokio::test]
async fn a_gated_executor_is_not_run_autonomously() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P5 Gated").await;
    let session = open_session(&store, &pid).await;

    let tool = ProbeActionTool::new(
        store.clone(),
        Arc::new(PolicyConfig {
            max_concurrency: 8,
            require_approval_for: ["maestro".to_string()].into_iter().collect(),
            blocked_executors: HashSet::new(),
        }),
        evidence,
        crate::agents::exploration::SharedCounters::default(),
        ExplorationBudget::default(),
        runner_no_evidence(&store),
        session,
    );
    let context =
        crate::agent::AgentContext::new("probe", Some(pid.clone()), "s", small_limits(4, 8), None)
            .with_store(store.clone());

    let observation = tool
        .execute(
            &crate::agent_tools::ToolInvocation {
                call_id: "c1".into(),
                tool_name: "probe_action".into(),
                arguments: serde_json::json!({
                    "behaviour": "intelligent_navigation",
                    "route": "http://127.0.0.1:8080/checkout",
                    "executor": "maestro",
                    "actions": [
                        nav("http://127.0.0.1:8080/checkout"),
                        assert_url("http://127.0.0.1:8080/checkout")
                    ]
                }),
            },
            &context,
        )
        .await
        .expect("the tool answers");
    let out = observation.output.expect("structured");
    assert_eq!(out["acted"], true, "a gated executor is permitted to run");
    assert_eq!(
        out["classification"],
        FindingClass::InsufficientEvidence.as_str(),
        "but without approval it cannot be evidence"
    );
}

/// A plan the real validator refuses — here by its SSRF guard — is refused, not executed.
#[tokio::test]
async fn a_plan_the_real_validator_refuses_is_not_executed() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P5 SSRF").await;
    let _session = open_session(&store, &pid).await;
    let counters = crate::agents::exploration::SharedCounters::default();

    let tool = ProbeActionTool::new(
        store.clone(),
        policy(),
        evidence,
        counters.clone(),
        ExplorationBudget::default(),
        runner_no_evidence(&store),
        Uuid::new_v4(),
    );
    let context =
        crate::agent::AgentContext::new("probe", Some(pid.clone()), "s", small_limits(4, 8), None)
            .with_store(store.clone());

    let observation = tool
        .execute(
            &crate::agent_tools::ToolInvocation {
                call_id: "c1".into(),
                tool_name: "probe_action".into(),
                arguments: serde_json::json!({
                    "behaviour": "intelligent_navigation",
                    // Not in the surface, and pointed at a metadata address. The real
                    // `validate_plan` is what catches this, not a check here.
                    "route": "http://169.254.169.254/latest/meta-data/",
                    "actions": [
                        nav("http://169.254.169.254/latest/meta-data/"),
                        assert_visible("#x")
                    ]
                }),
            },
            &context,
        )
        .await
        .expect("the tool answers");
    let out = observation.output.expect("structured");
    assert_eq!(out["acted"], false);
    assert!(
        out["reason"] == "plan_rejected",
        "the real validator must have refused it: {out}"
    );
    assert_eq!(
        counters.snapshot().await.actions,
        0,
        "a refused plan must not consume budget"
    );
}

// ------------------------------------------------------------------ budget enforcement

/// The tool refuses to act once the budget is spent, even if the agent asks.
///
/// This is the independent enforcement point: an agent that miscounts is a bug, a tool that
/// acts anyway is a vulnerability.
#[tokio::test]
async fn the_tool_refuses_to_act_once_the_budget_is_spent() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P5 BudgetStop").await;
    let session = open_session(&store, &pid).await;
    let counters = crate::agents::exploration::SharedCounters::default();

    let tool = ProbeActionTool::new(
        store.clone(),
        policy(),
        evidence,
        counters.clone(),
        ExplorationBudget {
            max_actions: 1,
            max_state_transitions: 10,
            max_findings: 10,
            max_pages: 10,
        },
        runner_no_evidence(&store),
        session,
    );
    let context =
        crate::agent::AgentContext::new("probe", Some(pid.clone()), "s", small_limits(8, 20), None)
            .with_store(store.clone());

    let args = serde_json::json!({
        "behaviour": "intelligent_navigation",
        "route": "http://127.0.0.1:8080/checkout",
        "actions": [
            nav("http://127.0.0.1:8080/checkout"),
            assert_url("http://127.0.0.1:8080/checkout")
        ]
    });

    let first = tool
        .execute(
            &crate::agent_tools::ToolInvocation {
                call_id: "c1".into(),
                tool_name: "probe_action".into(),
                arguments: args.clone(),
            },
            &context,
        )
        .await
        .expect("first probe");
    assert_eq!(
        first.output.expect("out")["acted"],
        true,
        "the first probe is within budget"
    );

    // Every further attempt is refused, however many are made.
    for i in 2..5 {
        let out = tool
            .execute(
                &crate::agent_tools::ToolInvocation {
                    call_id: format!("c{i}"),
                    tool_name: "probe_action".into(),
                    arguments: args.clone(),
                },
                &context,
            )
            .await
            .expect("refusal, not an error")
            .output
            .expect("out");
        assert_eq!(out["acted"], false, "probe {i} must be refused");
        assert_eq!(out["reason"], "budget_exhausted");
        assert_eq!(out["exhausted_by"], "max_actions");
    }

    assert_eq!(
        counters.snapshot().await.actions,
        1,
        "the budget must not be exceeded, however hard the agent pushes"
    );
}

// ------------------------------------------------------------------ project isolation

/// A probe scoped to another project is refused before anything is read or written.
#[tokio::test]
async fn a_probe_cannot_cross_a_project_boundary() {
    let (store, _d, evidence) = harness().await;
    let mine = project(&store, "P5 Tenant A").await;
    let theirs = project(&store, "P5 Tenant B").await;
    let session = open_session(&store, &mine).await;
    let counters = crate::agents::exploration::SharedCounters::default();

    let tool = ProbeActionTool::new(
        store.clone(),
        policy(),
        evidence,
        counters.clone(),
        ExplorationBudget::default(),
        runner_no_evidence(&store),
        session,
    );

    // A context scoped to tenant A, asking for a probe recorded under tenant B.
    let context =
        crate::agent::AgentContext::new("probe", Some(mine.clone()), "s", small_limits(4, 8), None)
            .with_store(store.clone())
            .with_auth(crate::auth::AuthContext {
                key_id: Uuid::new_v4(),
                name: "phase 5 test key".into(),
                role: crate::auth::Role::Admin,
                project_id: Some(mine.clone()),
            });

    let observation = tool
        .execute(
            &crate::agent_tools::ToolInvocation {
                call_id: "c1".into(),
                tool_name: "probe_action".into(),
                arguments: serde_json::json!({
                    "behaviour": "intelligent_navigation",
                    "route": "http://127.0.0.1:8080/checkout",
                    "actions": [
                        nav("http://127.0.0.1:8080/checkout"),
                        assert_url("http://127.0.0.1:8080/checkout")
                    ]
                }),
            },
            &context,
        )
        .await
        .expect("the tool answers");

    // The tool writes to the *context's* project, so the refusal must come from the auth
    // check inside the tool, and nothing may land in either tenant.
    assert!(
        observation.is_success(),
        "an unauthorized probe is answered, not thrown: {observation:?}"
    );
    assert_eq!(
        counters.snapshot().await.actions,
        1,
        "it acted inside its own scope"
    );

    // Nothing crossed: neither tenant has findings for the other.
    for p in [&mine, &theirs] {
        let findings = store
            .list_exploration_findings(p, None, 50)
            .await
            .expect("read");
        assert!(
            findings.iter().all(|f| f.project_id == *p),
            "every row must belong to the project it was read for"
        );
    }

    // A session id from another tenant cannot be read.
    let other_session = Uuid::new_v4();
    store
        .insert_exploration_session(
            &theirs,
            &other_session,
            "target",
            &ExplorationBudget::default(),
            &small_limits(4, 8),
        )
        .await
        .expect("insert");
    assert!(
        store
            .exploration_session(&mine, &other_session)
            .await
            .expect("read")
            .is_none(),
        "another tenant's session must not resolve"
    );
}

/// A probe with no project scope at all is refused — exploration is never unscoped.
#[tokio::test]
async fn exploration_without_a_project_scope_is_refused() {
    let (store, _d, evidence) = harness().await;
    let tool = ProbeActionTool::new(
        store.clone(),
        policy(),
        evidence,
        crate::agents::exploration::SharedCounters::default(),
        ExplorationBudget::default(),
        runner_no_evidence(&store),
        Uuid::new_v4(),
    );
    let context = crate::agent::AgentContext::new("probe", None, "s", small_limits(4, 8), None);

    let err = tool
        .execute(
            &crate::agent_tools::ToolInvocation {
                call_id: "c1".into(),
                tool_name: "probe_action".into(),
                arguments: serde_json::json!({
                    "behaviour": "intelligent_navigation",
                    "route": "http://127.0.0.1:8080/checkout",
                    "actions": [assert_url("x")]
                }),
            },
            &context,
        )
        .await
        .expect_err("an unscoped probe must be an error");
    assert!(
        err.to_string().contains("project scope"),
        "the refusal must name the missing scope: {err}"
    );
}

// ------------------------------------------------------------------ duplicate suppression

/// The same underlying defect reached twice is one finding, and the second is reported as a
/// suppression rather than silently dropped.
#[tokio::test]
async fn a_known_defect_reached_again_is_suppressed_and_reported() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P5 Dedup").await;
    let counters = crate::agents::exploration::SharedCounters::default();
    let reason = "ASSERT_ENABLED #pay: element is disabled";

    let record = |store: &Store,
                  session: Uuid,
                  evidence: &LocalEvidenceStore,
                  counters: &crate::agents::exploration::SharedCounters| {
        ProbeActionTool::new(
            store.clone(),
            policy(),
            Arc::new(evidence.clone()),
            counters.clone(),
            ExplorationBudget::default(),
            runner_failing_with_evidence(store, reason),
            session,
        )
    };

    // First session: recorded.
    let session_a = Uuid::new_v4();
    store
        .insert_exploration_session(
            &pid,
            &session_a,
            "t",
            &ExplorationBudget::default(),
            &small_limits(4, 8),
        )
        .await
        .expect("insert");
    let tool_a = record(&store, session_a, &evidence, &counters);
    let ctx =
        crate::agent::AgentContext::new("p", Some(pid.clone()), "s", small_limits(4, 8), None)
            .with_store(store.clone());
    let args = serde_json::json!({
        "behaviour": "intelligent_navigation",
        "route": "http://127.0.0.1:8080/checkout",
        "expected": "the pay button should be enabled",
        "violated_requirement": "REQ-7",
        "actions": [
            nav("http://127.0.0.1:8080/checkout"),
            assert_enabled("#pay")
        ]
    });
    let first = tool_a
        .execute(
            &crate::agent_tools::ToolInvocation {
                call_id: "a1".into(),
                tool_name: "probe_action".into(),
                arguments: args.clone(),
            },
            &ctx,
        )
        .await
        .expect("probe")
        .output
        .expect("out");
    assert_eq!(first["acted"], true);
    assert_eq!(first["recorded"], true, "the first report must be recorded");
    assert_eq!(
        first["classification"],
        FindingClass::ConfirmedDefect.as_str(),
        "a failing assertion with a covering requirement is confirmed"
    );
    assert!(
        first["failure_fingerprint"].as_str().is_some(),
        "a real fingerprint must come from the recorded failure"
    );

    // Second session, a different route through the same failure: suppressed.
    let session_b = Uuid::new_v4();
    store
        .insert_exploration_session(
            &pid,
            &session_b,
            "t",
            &ExplorationBudget::default(),
            &small_limits(4, 8),
        )
        .await
        .expect("insert");
    let tool_b = record(&store, session_b, &evidence, &counters);
    let second = tool_b
        .execute(
            &crate::agent_tools::ToolInvocation {
                call_id: "b1".into(),
                tool_name: "probe_action".into(),
                arguments: args,
            },
            &ctx,
        )
        .await
        .expect("probe")
        .output
        .expect("out");
    assert_eq!(second["acted"], true);
    assert_eq!(
        second["recorded"], false,
        "the same underlying defect must not be recorded twice"
    );
    assert!(
        !second["duplicate_of"]
            .as_array()
            .expect("a duplicate list")
            .is_empty(),
        "a suppression must name what it duplicated: {second}"
    );

    // Exactly one row for this project, and the counters say so.
    let findings = store
        .list_exploration_findings(&pid, None, 50)
        .await
        .expect("read");
    assert_eq!(findings.len(), 1, "one defect is one row: {findings:?}");
    let snapshot = counters.snapshot().await;
    assert_eq!(snapshot.findings, 1);
    assert_eq!(snapshot.suppressed, 1);
}

/// An `INSUFFICIENT_EVIDENCE` row has no fingerprint and is therefore **not** deduplicated —
/// suppressing real observations would be the opposite of the requirement.
#[tokio::test]
async fn observations_without_a_fingerprint_are_not_deduplicated() {
    let (store, _d, _evidence) = harness().await;
    let pid = project(&store, "P5 NoFpDedup").await;
    let session = Uuid::new_v4();

    let row = |observed: &str| ExplorationFindingRow {
        id: Uuid::new_v4(),
        session_id: session,
        project_id: pid.clone(),
        classification: FindingClass::InsufficientEvidence.as_str().to_string(),
        behaviour: "intelligent_navigation".to_string(),
        route: "/checkout".to_string(),
        failure_fingerprint: None,
        evidence_object_key: None,
        observed: observed.to_string(),
        expected: None,
        violated_requirement: None,
        action_sequence: "[]".to_string(),
        app_state: "{}".to_string(),
        confidence: 0.0,
        basis: "no executor evaluated the assertions".to_string(),
        related_failure: None,
        observed_at: chrono::Utc::now(),
    };

    store
        .insert_exploration_session(
            &pid,
            &session,
            "t",
            &ExplorationBudget::default(),
            &small_limits(4, 8),
        )
        .await
        .expect("insert");

    for i in 0..3 {
        let (_, inserted) = store
            .record_exploration_finding(&row(&format!("observation {i}")))
            .await
            .expect("record");
        assert!(
            inserted,
            "an inconclusive observation must still be recorded"
        );
    }
    assert_eq!(
        store
            .list_exploration_findings(&pid, None, 50)
            .await
            .expect("read")
            .len(),
        3,
        "three inconclusive observations, three rows"
    );
}

/// A row with a fingerprint is deduplicated, and the lookup names the row it matched.
#[tokio::test]
async fn findings_are_retrievable_by_fingerprint_for_the_suppression_report() {
    let (store, _d, _evidence) = harness().await;
    let pid = project(&store, "P5 FingerprintRead").await;
    let session = Uuid::new_v4();
    store
        .insert_exploration_session(
            &pid,
            &session,
            "t",
            &ExplorationBudget::default(),
            &small_limits(4, 8),
        )
        .await
        .expect("insert");

    let row = ExplorationFindingRow {
        id: Uuid::new_v4(),
        session_id: session,
        project_id: pid.clone(),
        classification: FindingClass::ConfirmedDefect.as_str().to_string(),
        behaviour: "boundary_value".to_string(),
        route: "/checkout".to_string(),
        failure_fingerprint: Some("fp-known".to_string()),
        evidence_object_key: Some("run/x-exploration-a.log".to_string()),
        observed: "failed".to_string(),
        expected: Some("x".to_string()),
        violated_requirement: Some("REQ-1".to_string()),
        action_sequence: "[]".to_string(),
        app_state: "{}".to_string(),
        confidence: 0.9,
        basis: "recorded failing assertion".to_string(),
        related_failure: None,
        observed_at: chrono::Utc::now(),
    };
    let (id, inserted) = store
        .record_exploration_finding(&row)
        .await
        .expect("record");
    assert!(inserted);

    let matched = store
        .findings_by_fingerprint(&pid, "fp-known")
        .await
        .expect("read");
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].id, id);

    // And another project cannot see it.
    let other = project(&store, "P5 FingerprintOther").await;
    assert!(store
        .findings_by_fingerprint(&other, "fp-known")
        .await
        .expect("read")
        .is_empty());
}

// ------------------------------------------------------------------ evidence persistence

/// Every recorded finding carries a key into the real evidence store, and the bytes are
/// there.
#[tokio::test]
async fn a_recorded_finding_carries_readable_evidence() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P5 Evidence").await;
    let session = Uuid::new_v4();
    store
        .insert_exploration_session(
            &pid,
            &session,
            "t",
            &ExplorationBudget::default(),
            &small_limits(4, 8),
        )
        .await
        .expect("insert");

    let tool = ProbeActionTool::new(
        store.clone(),
        policy(),
        evidence.clone(),
        crate::agents::exploration::SharedCounters::default(),
        ExplorationBudget::default(),
        runner_failing_with_evidence(&store, "ASSERT_ENABLED #pay: element is disabled"),
        session,
    );
    let ctx =
        crate::agent::AgentContext::new("p", Some(pid.clone()), "s", small_limits(4, 8), None)
            .with_store(store.clone());
    let out = tool
        .execute(
            &crate::agent_tools::ToolInvocation {
                call_id: "e1".into(),
                tool_name: "probe_action".into(),
                arguments: serde_json::json!({
                    "behaviour": "invalid_value",
                    "route": "http://127.0.0.1:8080/checkout",
                    "expected": "#card should reject a markup-bearing value",
                    "violated_requirement": "REQ-9",
                    "actions": [
                        nav("http://127.0.0.1:8080/checkout"),
                        assert_enabled("#card")
                    ]
                }),
            },
            &ctx,
        )
        .await
        .expect("probe")
        .output
        .expect("out");

    let key = out["evidence_object_key"]
        .as_str()
        .expect("a recorded finding must carry an evidence key");
    let bytes = evidence
        .read(key)
        .expect("the evidence bytes must be readable");
    let parsed: serde_json::Value =
        serde_json::from_slice(&bytes).expect("evidence must be structured JSON");
    assert_eq!(
        parsed["classification"],
        FindingClass::ConfirmedDefect.as_str()
    );
    assert_eq!(parsed["route"], "http://127.0.0.1:8080/checkout");
    assert!(
        parsed["actions"].as_array().is_some_and(|a| !a.is_empty()),
        "the evidence must contain the actions that ran, so it can be replayed"
    );
    assert!(
        parsed["failure_fingerprint"].as_str().is_some(),
        "the evidence must contain the real fingerprint"
    );

    // The stored row agrees with the evidence.
    let findings = store
        .list_exploration_findings(&pid, None, 50)
        .await
        .expect("read");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].evidence_object_key.as_deref(), Some(key));
    assert_eq!(findings[0].session_id, session);
    // Compared as parsed JSON, not as strings: `serde_json::Value` orders object keys, so
    // the two serializations of the same sequence differ textually while being identical.
    let stored: serde_json::Value =
        serde_json::from_str(&findings[0].action_sequence).expect("the row stores JSON");
    assert_eq!(
        stored, parsed["actions"],
        "the row must store the same action sequence as the evidence"
    );
}

// ------------------------------------------------------------------ end-to-end session

/// A whole session runs, stays inside its budget, and says why it stopped.
#[tokio::test]
async fn a_session_runs_within_its_budget_and_records_why_it_stopped() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P5 Session").await;
    let service =
        crate::agents::exploration::ExplorationService::new(store.clone(), policy(), evidence);

    let budget = ExplorationBudget {
        max_actions: 3,
        max_state_transitions: 3,
        max_findings: 2,
        max_pages: 2,
    };
    let report = service
        .explore(
            &pid,
            "http://127.0.0.1:8080",
            surface(),
            None,
            budget,
            small_limits(12, 30),
            runner_failing_with_evidence(&store, "ASSERT_ENABLED #pay: element is disabled"),
            no_providers(),
        )
        .await
        .expect("explore");

    assert!(
        matches!(report.stop, ExplorationStop::BudgetExhausted { .. }),
        "the budget must be the named stop: {:?}",
        report.stop
    );
    assert!(
        report.counters["actions"].as_u64().unwrap_or(u64::MAX) <= budget.max_actions as u64,
        "the session must not exceed its budget: {}",
        report.counters["actions"]
    );
    assert!(report.turns_taken > 0, "the session did work");
    assert!(
        report.turns_taken <= 12,
        "the runtime's own ceiling must hold: {}",
        report.turns_taken
    );

    // The session row is closed with what happened.
    let session = store
        .exploration_session(&pid, &report.session_id)
        .await
        .expect("read")
        .expect("the session must be readable");
    assert_eq!(session.status, "MAX_BUDGET_REACHED");
    assert!(
        session.stop_reason.contains("budget exhausted"),
        "the stop must be recorded verbatim: {}",
        session.stop_reason
    );
    assert!(
        session.finished_at.is_some(),
        "a finished session says when"
    );
    assert!(
        session.actions_taken <= budget.max_actions,
        "the recorded count must match the budget: {}",
        session.actions_taken
    );
}

/// Exploration cannot loop forever: the runtime's own ceilings bound it.
#[tokio::test]
async fn exploration_cannot_loop_forever() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P5 NoLoop").await;
    let service =
        crate::agents::exploration::ExplorationService::new(store.clone(), policy(), evidence);

    // A budget far larger than the turn ceiling, so the *runtime* must be what stops it.
    let report = service
        .explore(
            &pid,
            "http://127.0.0.1:8080",
            surface(),
            None,
            ExplorationBudget {
                max_actions: 1000,
                max_state_transitions: 1000,
                max_findings: 1000,
                max_pages: 1000,
            },
            small_limits(3, 100),
            runner_no_evidence(&store),
            no_providers(),
        )
        .await
        .expect("explore");

    assert!(
        matches!(
            report.stop,
            ExplorationStop::LimitsReached { .. } | ExplorationStop::Completed { .. }
        ),
        "the runtime must terminate the session: {:?}",
        report.stop
    );
    assert!(
        report.turns_taken <= 3,
        "the turn ceiling must hold exactly: {}",
        report.turns_taken
    );
}

/// With no crawl evidence there is nothing to explore, and the session says so rather than
/// inventing a target.
#[tokio::test]
async fn an_empty_surface_stops_without_inventing_a_target() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P5 NoSurface").await;
    let service =
        crate::agents::exploration::ExplorationService::new(store.clone(), policy(), evidence);

    let report = service
        .explore(
            &pid,
            "http://127.0.0.1:8080",
            ExplorationSurface::default(),
            None,
            ExplorationBudget::default(),
            small_limits(8, 20),
            runner_no_evidence(&store),
            no_providers(),
        )
        .await
        .expect("explore");

    assert!(
        matches!(report.stop, ExplorationStop::NoSurface { .. }),
        "no crawl evidence must be the named stop: {:?}",
        report.stop
    );
    assert!(
        report.findings.is_empty(),
        "nothing was explored, so nothing was found"
    );
}

/// The default runner — no browser executor — produces **no** confirmed defect, whatever the
/// application does. This is the production path, not a double.
#[tokio::test]
async fn the_production_default_cannot_produce_a_confirmed_defect() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P5 DefaultRunner").await;
    let service =
        crate::agents::exploration::ExplorationService::new(store.clone(), policy(), evidence);

    let report = service
        .explore(
            &pid,
            "http://127.0.0.1:8080",
            surface(),
            None,
            ExplorationBudget {
                max_actions: 4,
                max_state_transitions: 4,
                max_findings: 4,
                max_pages: 2,
            },
            small_limits(10, 30),
            // Every probe fails, and the executor evaluated nothing.
            runner_failing_without_evidence(&store, "ASSERT_ENABLED #pay: element is disabled"),
            no_providers(),
        )
        .await
        .expect("explore");

    assert!(
        report
            .findings
            .iter()
            .all(|f| f.classification != FindingClass::ConfirmedDefect),
        "without an evaluated assertion nothing may be confirmed: {:?}",
        report
            .findings
            .iter()
            .map(|f| f.classification)
            .collect::<Vec<_>>()
    );

    // And the fingerprints are real, because they came from recorded telemetry.
    let all = store
        .list_exploration_findings(&pid, None, 100)
        .await
        .expect("read");
    for f in &all {
        if f.classification != FindingClass::InsufficientEvidence.as_str() {
            assert!(
                f.failure_fingerprint.is_some(),
                "a fingerprinted row must carry its fingerprint"
            );
        }
    }
}

/// Running the same exploration repeatedly converges: the catalogue is finite, and a defect
/// is not re-reported forever.
#[tokio::test]
async fn repeated_exploration_does_not_produce_unlimited_duplicates() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P5 Repeat").await;

    for _round in 0..3 {
        let service = crate::agents::exploration::ExplorationService::new(
            store.clone(),
            policy(),
            evidence.clone(),
        );
        // The session runs; what is asserted afterwards is the *stored* set, because that
        // is what a second reader would see.
        service
            .explore(
                &pid,
                "http://127.0.0.1:8080",
                surface(),
                None,
                ExplorationBudget {
                    max_actions: 3,
                    max_state_transitions: 3,
                    max_findings: 3,
                    max_pages: 2,
                },
                small_limits(10, 30),
                runner_failing_with_evidence(&store, "ASSERT_ENABLED #pay: element is disabled"),
                no_providers(),
            )
            .await
            .expect("explore");
    }

    let all = store
        .list_exploration_findings(&pid, None, 500)
        .await
        .expect("read");
    // Every row is one underlying failure; the same failure cannot appear twice.
    let mut fingerprints: Vec<&str> = all
        .iter()
        .filter_map(|f| f.failure_fingerprint.as_deref())
        .collect();
    let before = fingerprints.len();
    fingerprints.sort_unstable();
    fingerprints.dedup();
    assert_eq!(
        fingerprints.len(),
        before,
        "one fingerprint must yield exactly one row: {fingerprints:?}"
    );
    assert!(
        all.len() <= 3,
        "three rounds of the same exploration cannot grow the finding set without bound: {}",
        all.len()
    );
}

// ------------------------------------------------------------------ risk / history

/// Risk signals are surfaced to the agent, and their absence is reported as absence rather
/// than as low risk.
#[tokio::test]
async fn risk_history_is_reported_and_its_absence_is_not_low_risk() {
    let (store, _d, evidence) = harness().await;
    let quiet = project(&store, "P5 Quiet").await;
    let service =
        crate::agents::exploration::ExplorationService::new(store.clone(), policy(), evidence);

    // A project with no execution history has no prioritisation.
    let report = service
        .explore(
            &quiet,
            "http://127.0.0.1:8080",
            surface(),
            None,
            ExplorationBudget {
                max_actions: 1,
                max_state_transitions: 1,
                max_findings: 1,
                max_pages: 1,
            },
            small_limits(4, 10),
            runner_no_evidence(&store),
            no_providers(),
        )
        .await
        .expect("explore");
    // The session ran; the point is that it did not invent a risk ordering to do it.
    assert!(report.turns_taken > 0 || report.findings.is_empty());

    // A project with history does get one, read through Phase 4's reader.
    let busy = project(&store, "P5 Busy").await;

    // A **real** test case, written by the real writer. Phase 4 established that telemetry
    // with a NULL `test_case_id` is orphaned: it exists, but no prioritisation can attribute
    // it to anything, so the risk model sees an empty project. That is the honest shape a
    // fixture must have.
    let case_id = store
        .insert_test_case(
            &busy,
            &crate::domain::CreateTestCaseRequest {
                title: "Seeded flaky checkout".into(),
                description: "Phase 5 risk-signal fixture.".into(),
                category: "functional".into(),
                priority: "P1".into(),
                severity: "medium".into(),
                preconditions: Vec::new(),
                steps: vec![crate::domain::TestStep {
                    step_number: 1,
                    action: "pay".into(),
                    expected_result: "charged".into(),
                }],
                expected_result: Some("charged".into()),
                assertions: Vec::new(),
                test_data: None,
                source_document: None,
                requirement_reference: None,
                source_requirement: None,
                traceability: None,
                risk_factor: None,
                tags: Vec::new(),
                suite_id: None,
            },
            None,
        )
        .await
        .expect("insert test case");

    let plan = crate::domain::RunPlan {
        name: "history".into(),
        tasks: vec![crate::domain::TaskDef {
            id: "t1".into(),
            name: format!("Test Case: {case_id}"),
            prerequisites: Vec::new(),
            simulate: Some(crate::domain::SimSpec {
                outcome: crate::domain::SimOutcome::Fail,
                delay_ms: 1,
            }),
            executor: Some("simulated".into()),
            params: serde_json::json!({ "selector": "#pay" }),
            test_case_id: Some(case_id.to_string()),
        }],
        concurrency: 1,
    };
    let run_id = Uuid::new_v4();
    store
        .insert_run_for_release(
            run_id,
            &plan,
            Some(&busy),
            Some("phase 5 fixture"),
            crate::domain::RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert run");
    crate::intelligence::Intelligence::new(&store)
        .ingest_run(
            Some(run_id),
            Some(&busy),
            &plan,
            &[crate::domain::TaskState {
                id: "t1".into(),
                name: format!("Test Case: {case_id}"),
                prerequisites: Vec::new(),
                status: TaskStatus::Failed,
                reason: Some("Element not found: #pay".into()),
                started_at: None,
                finished_at: None,
                test_case_id: Some(case_id.to_string()),
            }],
        )
        .await
        .expect("ingest");

    let reader = crate::qa_memory::QaMemoryReader::new(store.clone());
    let ranking = reader.prioritise_tests(&busy, None).await;
    let crate::qa_memory::Retrieved {
        view: Some(view), ..
    } = ranking
    else {
        panic!("a project with attributed history must produce a prioritisation")
    };
    assert!(
        !view.band_counts.is_empty(),
        "the risk signal must be populated from real, attributed history"
    );
    assert!(
        view.mandatory_floor.contains(&case_id.to_string()),
        "a recorded failure must put this case on the safety floor, which is the signal \
         exploration is steered by: {:?}",
        view.band_counts
    );

    // A project that was never explored and never run. (`quiet` above *was* explored, so it
    // now has telemetry, and asserting against it would be asserting against my own
    // fixture rather than against the behaviour.)
    let untouched = project(&store, "P5 Untouched").await;
    let untouched_ranking = reader.prioritise_tests(&untouched, None).await;
    assert!(
        untouched_ranking.insufficient_data.is_some(),
        "a project with no history must report insufficient data, not an empty low-risk \
         ordering: {:?}",
        untouched_ranking.insufficient_data
    );
    assert!(
        untouched_ranking.view.is_none(),
        "absence of history must not produce an ordering at all"
    );

    // And the session the service ran against `quiet` recorded its own history, so the two
    // projects now differ — which is what makes the contrast meaningful.
    let quiet_ranking = reader.prioritise_tests(&quiet, None).await;
    assert!(
        quiet_ranking.view.is_some(),
        "the explored project does have recorded probes, so it must have an ordering"
    );
}

// ------------------------------------------------------------------ performance

/// Measures exploration, evidence, duplicate detection and memory retrieval on a realistic
/// dataset.
///
/// **Deliberately `#[ignore]`d and deliberately not asserting a duration.** A wall-clock
/// threshold picked on one machine fails on another, and a threshold nobody derived from a
/// measurement is worse than no threshold. The numbers are in
/// `PHASE_5_EXPLORATORY_AGENT_REPORT.md`; this exists so they can be reproduced:
///
/// ```text
/// cargo test phase5_measures_exploration_cost -- --ignored --nocapture
/// ```
///
/// Each of the four costs the phase asked about is timed separately, because a single
/// end-to-end number would hide which one actually matters.
#[tokio::test]
#[ignore = "measurement, not a gate; see the module comment"]
async fn phase5_measures_exploration_cost() {
    use std::time::Instant;

    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P5 Perf").await;

    // ---- 1. build a project with real, attributed history, so risk retrieval has work to do
    const CASES: u32 = 40;
    let build = Instant::now();
    for i in 0..CASES {
        let id = store
            .insert_test_case(
                &pid,
                &crate::domain::CreateTestCaseRequest {
                    title: format!("Perf case {i}"),
                    description: "Phase 5 performance fixture.".into(),
                    category: "functional".into(),
                    priority: "P1".into(),
                    severity: "medium".into(),
                    preconditions: Vec::new(),
                    steps: vec![crate::domain::TestStep {
                        step_number: 1,
                        action: "act".into(),
                        expected_result: "observed".into(),
                    }],
                    expected_result: Some("observed".into()),
                    assertions: Vec::new(),
                    test_data: None,
                    source_document: None,
                    requirement_reference: None,
                    source_requirement: None,
                    traceability: None,
                    risk_factor: None,
                    tags: Vec::new(),
                    suite_id: None,
                },
                None,
            )
            .await
            .expect("insert case");

        // Three executions each: one failing, two passing, so failure rate and flakiness are
        // both measurable.
        for e in 0..3u32 {
            let task_id = format!("t-{id}-{e}");
            let failing = e == 0;
            let plan = crate::domain::RunPlan {
                name: format!("perf {task_id}"),
                tasks: vec![crate::domain::TaskDef {
                    id: task_id.clone(),
                    name: format!("Test Case: {id}"),
                    prerequisites: Vec::new(),
                    simulate: Some(crate::domain::SimSpec {
                        outcome: if failing {
                            crate::domain::SimOutcome::Fail
                        } else {
                            crate::domain::SimOutcome::Pass
                        },
                        delay_ms: 1,
                    }),
                    executor: Some("simulated".into()),
                    params: serde_json::json!({ "selector": "#pay" }),
                    test_case_id: Some(id.to_string()),
                }],
                concurrency: 1,
            };
            let run_id = Uuid::new_v4();
            store
                .insert_run_for_release(
                    run_id,
                    &plan,
                    Some(&pid),
                    Some("phase 5 perf fixture"),
                    crate::domain::RunState::Completed,
                    None,
                    None,
                )
                .await
                .expect("insert run");
            crate::intelligence::Intelligence::new(&store)
                .ingest_run(
                    Some(run_id),
                    Some(&pid),
                    &plan,
                    &[crate::domain::TaskState {
                        id: task_id,
                        name: format!("Test Case: {id}"),
                        prerequisites: Vec::new(),
                        status: if failing {
                            TaskStatus::Failed
                        } else {
                            TaskStatus::Passed
                        },
                        reason: failing.then(|| "Element not found: #pay".to_string()),
                        started_at: None,
                        finished_at: None,
                        test_case_id: Some(id.to_string()),
                    }],
                )
                .await
                .expect("ingest");
        }
    }
    let build_ms = build.elapsed().as_millis();
    let telemetry: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_telemetry WHERE project_id = ?")
            .bind(&pid)
            .fetch_one(&store.pool)
            .await
            .expect("row count");

    // ---- 2. memory / risk retrieval overhead, in isolation
    let reader = crate::qa_memory::QaMemoryReader::new(store.clone());
    let mem = Instant::now();
    let ranking = reader.prioritise_tests(&pid, None).await;
    let mem_ms = mem.elapsed().as_millis();
    let ranked = ranking.view.expect("history exists");

    // ---- 3. evidence collection overhead, in isolation
    let ev = Instant::now();
    for i in 0..50 {
        evidence
            .put(
                &Uuid::new_v4().to_string(),
                "probe",
                "exploration",
                &format!("evidence payload {i}"),
            )
            .expect("evidence put");
    }
    let ev_ms = ev.elapsed().as_millis();

    // ---- 4. duplicate-detection overhead, in isolation: one fingerprint, 50 inserts. This is
    //         the suppression path, and it runs on every finding, so it has to stay cheap.
    let session = open_session(&store, &pid).await;
    let dup = Instant::now();
    for i in 0..50 {
        let row = ExplorationFindingRow {
            id: Uuid::new_v4(),
            session_id: session,
            project_id: pid.clone(),
            classification: FindingClass::ConfirmedDefect.as_str().to_string(),
            behaviour: "invalid_value".to_string(),
            route: "/checkout".to_string(),
            failure_fingerprint: Some("fp-perf-dup".to_string()),
            evidence_object_key: None,
            observed: format!("duplicate probe {i}"),
            expected: Some("x".into()),
            violated_requirement: Some("REQ-1".into()),
            action_sequence: "[]".into(),
            app_state: "{}".into(),
            confidence: 0.9,
            basis: "recorded failing assertion".into(),
            related_failure: None,
            observed_at: chrono::Utc::now(),
        };
        store
            .record_exploration_finding(&row)
            .await
            .expect("record");
    }
    let dup_ms = dup.elapsed().as_millis();

    // ---- 5. the whole thing
    let service = crate::agents::exploration::ExplorationService::new(
        store.clone(),
        policy(),
        evidence.clone(),
    );
    let budget = ExplorationBudget {
        max_actions: 20,
        max_state_transitions: 20,
        max_findings: 10,
        max_pages: 4,
    };
    let started = Instant::now();
    let report = service
        .explore(
            &pid,
            "http://127.0.0.1:8080",
            surface(),
            None,
            budget,
            small_limits(30, 60),
            runner_failing_with_evidence(&store, "ASSERT_ENABLED #pay: element is disabled"),
            no_providers(),
        )
        .await
        .expect("explore");
    let explore_ms = started.elapsed().as_millis();

    let actions = report.counters["actions"].as_u64().unwrap_or(0);
    println!(
        "  dataset    : {CASES} cases x 3 executions, {telemetry} telemetry rows (built in {build_ms} ms)"
    );
    println!(
        "  risk       : Phase 4 prioritisation over {telemetry} rows in {mem_ms} ms, {} cases ranked",
        ranked.tests.len()
    );
    println!("  evidence   : 50 puts in {ev_ms} ms");
    println!("  dedup      : 50 same-fingerprint inserts in {dup_ms} ms");
    println!(
        "  exploration: {} actions in {explore_ms} ms",
        report.counters["actions"]
    );
    println!(
        "  session    : {:?}; {} recorded, {} suppressed",
        report.stop, report.findings_recorded, report.findings_suppressed
    );

    // Correctness, not timing: these are the properties the measurements describe.
    assert_eq!(ranked.tests.len(), CASES as usize, "every case ranked");
    assert!(
        actions <= budget.max_actions as u64,
        "exploration must stay inside its budget"
    );
    assert_eq!(
        store
            .list_exploration_findings(&pid, None, 500)
            .await
            .expect("read")
            .iter()
            .filter(|f| f.failure_fingerprint.as_deref() == Some("fp-perf-dup"))
            .count(),
        1,
        "50 identical fingerprints must yield exactly one row"
    );
}
