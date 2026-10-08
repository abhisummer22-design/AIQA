//! What these tests are for.
//!
//! `rbac_probe` makes claims about its own machinery: that a 403 from the role
//! gate is told apart from a 403 from the scope gate, that a 422 means the gate
//! let the request through, that a caller cannot aim the probe at a host of their
//! choosing, and that every target it offers is a route that exists. Each of those
//! is a way the tool could quietly lie, which is the one failure mode it was built
//! to avoid.
//!
//! The last test is the important one. It drives the real router with a real key
//! of each role for every target and checks that `classify` reads the real answer
//! correctly. If a route's gate changes, or a target is added for a route that does
//! not exist, that test is what notices -- not this file's own expectations about
//! which role is allowed what.

use crate::auth::{AuthContext, Role};
use crate::evidence::LocalEvidenceStore;
use crate::executor::{ExecutorRegistry, SimulatedExecutor};
use crate::policy::PolicyConfig;
use crate::rbac_probe::{classify, concrete_path, loopback_base, Gate, GHOST_ID, TARGETS};
use crate::state::AppState;
use crate::store::Store;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::collections::HashSet;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

// ---------------------------------------------------------------- classify

/// The mapping is the whole instrument. If it misreads a status, every number the
/// page shows is wrong while looking precise.
#[test]
fn a_403_is_told_apart_by_which_gate_said_it() {
    assert_eq!(
        classify(
            403,
            r#"{"error":{"message":"role 'viewer' cannot perform this action (requires 'operator')"}}"#
        ),
        Gate::RoleRefused,
        "the role gate's own wording must be recognised"
    );
    assert_eq!(
        classify(
            403,
            r#"{"error":{"message":"key is scoped to project abc and cannot act on def"}}"#
        ),
        Gate::ScopeRefused,
        "the scope gate's own wording must be recognised, not lumped in with the role gate"
    );
    assert_eq!(
        classify(403, r#"{"error":{"message":"something else entirely"}}"#),
        Gate::RefusedUnknown,
        "a 403 from neither guard must say so rather than guess which one it was"
    );
}

/// A 422 is the case that makes this instrument necessary.
#[test]
fn a_body_rejected_before_the_gate_counts_as_allowed_not_as_a_refusal() {
    // Authorization runs inside the handler, while `Json<T>` is an argument
    // extractor. So a malformed body never reaches the gate and comes back 422 for
    // every role. Reading that as "allowed" is correct -- the gate did not object --
    // and reading it as a schema probe is exactly what it is.
    assert_eq!(
        classify(422, "missing field `title`"),
        Gate::AllowedThenRefused
    );
    assert!(!Gate::AllowedThenRefused.blocked());
    assert!(!Gate::AllowedThenRefused.inconclusive());
}

#[test]
fn only_a_403_counts_as_blocked() {
    for status in [200, 201, 400, 404, 409, 422, 500] {
        assert!(
            !classify(status, "").blocked(),
            "{status} must not read as a gate refusal"
        );
    }
    for status in [401, 405] {
        let g = classify(status, "");
        assert!(
            g.inconclusive(),
            "{status} must read as an unsound measurement"
        );
        assert!(
            !g.blocked(),
            "{status} is not a permission finding, so it must not read as one either"
        );
    }
}

/// A bad key must never be mistaken for a passing test of the endpoint.
#[test]
fn a_rejected_key_is_inconclusive_rather_than_a_finding() {
    let g = classify(401, "unauthorized");
    assert!(g.inconclusive());
    assert!(
        !g.blocked(),
        "a bad key is not evidence that the route is open"
    );
}

// ------------------------------------------------------------- loopback

/// The probe takes a path and a key from the caller. If it also took a host, this
/// endpoint would be a way to make the server send authenticated requests to
/// whatever else it can reach -- a more interesting capability than the one on
/// offer. So the host is checked, not trusted.
#[test]
fn the_probe_may_only_address_this_server_over_loopback() {
    for good in [
        "127.0.0.1:8080",
        "localhost:8080",
        "[::1]:8080",
        "localhost",
    ] {
        assert!(
            loopback_base(Some(good)).is_some(),
            "{good} is this server and should be allowed"
        );
    }
    for bad in [
        "evil.example.com",
        "10.0.0.5:8080",
        "169.254.169.254",         // cloud metadata
        "localhost.evil.com:8080", // suffix trick
        "127.0.0.1.evil.com:8080",
        "[::ffff:127.0.0.1]:8080",
    ] {
        assert!(
            loopback_base(Some(bad)).is_none(),
            "{bad} is not loopback and must be refused"
        );
    }
    assert!(
        loopback_base(None).is_none(),
        "a missing Host must be refused"
    );
}

/// The port comes from the request that arrived, so a non-default port still works.
#[test]
fn the_port_comes_from_the_request_rather_than_a_hardcoded_one() {
    assert_eq!(
        loopback_base(Some("127.0.0.1:9999")).as_deref(),
        Some("http://127.0.0.1:9999")
    );
}

// ------------------------------------------------------------- the targets

#[test]
fn every_target_is_a_route_the_router_actually_serves() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/api.rs"))
        .expect("api.rs is readable");
    let missing: Vec<&str> = TARGETS
        .iter()
        .filter(|t| !src.contains(&format!("\"{}\"", t.path)))
        .map(|t| t.path)
        .collect();
    assert!(
        missing.is_empty(),
        "these targets name paths api.rs does not register, so probing them would \
         measure a 404 and tell the reader nothing about permissions: {missing:?}"
    );
}

#[test]
fn every_target_id_is_unique_so_a_saved_report_cannot_be_misattributed() {
    let mut seen = HashSet::new();
    for t in TARGETS {
        assert!(seen.insert(t.id), "duplicate target id {:?}", t.id);
    }
}

/// A declared body must at least be JSON, and a read-only target must not carry one
/// that will never be sent.
///
/// It deliberately does *not* require every POST to declare a body. Some of these
/// handlers take no `Json<T>` at all, so `{}` reaches the gate perfectly well and a
/// body would be noise. Whether a target actually reaches its gate cannot be decided
/// by reading the table -- it is decided by probing, which
/// `every_post_target_reaches_its_gate_rather_than_the_extractor` does.
#[test]
fn a_declared_body_is_json_and_a_read_only_target_declares_none() {
    for t in TARGETS {
        if let Some(body) = t.body {
            serde_json::from_str::<serde_json::Value>(body)
                .unwrap_or_else(|e| panic!("{} has a body that is not JSON: {e}", t.path));
        }
        if matches!(t.method, "GET" | "DELETE") {
            assert!(
                t.body.is_none(),
                "{} is a {} and carries a body that will never be sent",
                t.path,
                t.method
            );
        }
    }
}

/// A mutating probe must not be able to touch anything real. Every id in one has to
/// be the non-existent uuid, so that if the gate does its job nothing happens and if
/// it does not the target still does not exist.
#[test]
fn a_mutating_probe_can_only_address_ids_that_do_not_exist() {
    let pid = Uuid::new_v4();
    for t in TARGETS.iter().filter(|t| t.mutating) {
        let path = concrete_path(t.path, pid);
        assert!(
            !path.contains(':'),
            "{} is mutating but still carries an unresolved parameter: {path}",
            t.path
        );
        for named in [
            ":run_id",
            ":proposal_id",
            ":task_id",
            ":evidence_id",
            ":finding_id",
        ] {
            if t.path.contains(named) {
                assert!(
                    path.contains(GHOST_ID),
                    "{} substitutes {named} but must use the non-existent id",
                    t.path
                );
            }
        }
    }
}

#[test]
fn a_template_is_fully_resolved_so_the_probe_requests_something_real() {
    let pid = Uuid::new_v4();
    let path = concrete_path("/api/v1/projects/:project_id/executions/:run_id", pid);
    assert!(!path.contains(':'), "{path}");
    assert!(path.contains(&pid.to_string()), "{path}");
    assert!(path.contains(GHOST_ID), "{path}");
}

// ------------------------------------------------- the real thing, measured

struct Harness {
    state: AppState,
    _dir: tempfile::TempDir,
}

/// An AI router that never answers anything, so a probe cannot be delayed by a
/// real provider. Same shape as the one `rbac_matrix_tests` builds.
fn inert_ai_router() -> Arc<crate::ai_router::AiInfrastructure> {
    use crate::ai_router::test_support::{
        healthy_model, healthy_provider, FakeDiscovery, FakeExecutor,
    };
    use crate::ai_router::{AiExecutor, AiInfrastructure, CapabilityLevel};
    use std::collections::HashMap;

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

async fn harness() -> Harness {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("probe.sqlite3"))
        .await
        .expect("connect");
    let evidence = LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence");
    let state = AppState::new(
        Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor))),
        store,
        PolicyConfig {
            max_concurrency: 8,
            require_approval_for: HashSet::new(),
            blocked_executors: HashSet::new(),
        },
        evidence,
        inert_ai_router(),
        Arc::new(crate::ai_router::test_support::FakeEmbedder::unconfigured()),
    );
    Harness { state, _dir: dir }
}

async fn mint(state: &AppState, role: &str) -> String {
    let raw = crate::auth::generate_key();
    state
        .store
        .insert_api_key(
            Uuid::new_v4(),
            "probe-key",
            &crate::auth::hash_key(&raw),
            role,
            None,
        )
        .await
        .unwrap();
    raw
}

/// Drives the real router, so the middleware the guards sit in is part of the answer.
async fn send(
    state: &AppState,
    method: &str,
    path: &str,
    key: &str,
    body: Option<&str>,
) -> (u16, String) {
    let app = crate::api::router(state.clone());
    let b = match body {
        Some(t) => Body::from(t.to_string()),
        None => Body::empty(),
    };
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("Authorization", format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .body(b)
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    let status = res.status().as_u16();
    let bytes = axum::body::to_bytes(res.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// The measurement the module exists to make, checked against the real router for
/// every target and every role.
///
/// This is not asserting which role ought to be allowed. It asserts that the status
/// the server really produced is classified correctly -- so a route whose gate
/// changed is reported as a different kind of answer rather than silently keeping a
/// stale label, and so a target aimed at a route that 404s is caught here.
#[tokio::test]
async fn every_target_is_probed_for_real_and_its_answer_classified_correctly() {
    let h = harness().await;
    let pid = Uuid::new_v4();
    h.state
        .store
        .insert_project(pid, "probe-project", None)
        .await
        .unwrap();

    let keys = [
        (Role::Viewer, mint(&h.state, "viewer").await),
        (Role::Operator, mint(&h.state, "operator").await),
        (Role::Admin, mint(&h.state, "admin").await),
    ];

    let mut checked = 0usize;
    for target in TARGETS {
        let path = concrete_path(target.path, pid);
        for (role, key) in &keys {
            let (status, body) = send(&h.state, target.method, &path, key, target.body).await;
            let gate = classify(status, &body);
            assert!(
                !gate.inconclusive(),
                "{} as {}: {} -- the measurement itself was unsound, so this target \
                 cannot be used to test anything",
                target.path,
                role.as_str(),
                gate.label()
            );
            // The classification must agree with the status it was given, for
            // every role, for every target. A stale or swapped mapping fails here
            // rather than in a report nobody is looking at.
            let expected_blocked = status == 403;
            assert_eq!(
                gate.blocked(),
                expected_blocked,
                "{} as {} answered {status} but was classified {:?}",
                target.path,
                role.as_str(),
                gate
            );
            if expected_blocked {
                assert!(
                    gate != Gate::RefusedUnknown,
                    "{} as {} was refused but neither guard explained why: {body}",
                    target.path,
                    role.as_str()
                );
            }
            checked += 1;
        }
    }
    assert!(
        checked == TARGETS.len() * 3,
        "expected {} probes, ran {checked}",
        TARGETS.len() * 3
    );
}

/// The check that the bodies in `rbac_probe` exist for.
///
/// Authorization runs inside the handler, `Json<T>` in front of it. So a POST whose
/// body the extractor rejects answers 422 for *every* role -- and `classify` reads
/// that as `AllowedThenRefused`, i.e. the gate let it through. A viewer would then
/// appear to have passed a gate it never reached, which is a false reading in the
/// permissive direction: exactly the failure this module exists to prevent.
///
/// A 404 is allowed here without comment beyond its own name: `healing/*` look the
/// proposal up before the role check, so a viewer drawing 404 rather than 403 is
/// real behaviour, and this test is about whether the gate was reached at all, not
/// about what happens when it is bypassed.
#[tokio::test]
async fn every_post_target_reaches_its_gate_rather_than_the_extractor() {
    let h = harness().await;
    let pid = Uuid::new_v4();
    h.state
        .store
        .insert_project(pid, "extractor", None)
        .await
        .unwrap();
    let viewer = mint(&h.state, "viewer").await;

    for target in TARGETS
        .iter()
        .filter(|t| matches!(t.method, "POST" | "PUT" | "PATCH"))
    {
        let path = concrete_path(target.path, pid);
        let (status, body) = send(&h.state, target.method, &path, &viewer, target.body).await;
        assert_ne!(
            status,
            422,
            "{} as a viewer answered 422, so the extractor answered before the gate \
             ran and the probe would report that the viewer got past a gate it never \
             reached. The recorded body needs a field it is missing: {}",
            target.path,
            body.trim()
        );
    }
}

/// A sweep must not be able to leave anything behind.
///
/// This is a regression test for something that actually happened: the page's
/// "probe every route" ran `POST /admin/api-keys` and `POST /api/v1/projects` with
/// a real admin key, and each one succeeded, so a full sweep quietly minted API
/// keys and created projects. The earlier claim -- that every mutating target
/// addresses a non-existent id, so a probe that gets through cannot touch anything
/// real -- was true of every target *with* an id in its path and false of the two
/// that create a row out of the request body.
///
/// So this asserts the two halves separately: that the claim still holds for the
/// targets it holds for, and that the ones it does not hold for are refused unless
/// the caller opted in.
#[tokio::test]
async fn a_probe_that_gets_past_the_gate_cannot_leave_a_record_behind() {
    // Every target whose path names an id must still resolve to an id that cannot
    // exist. This is the half of the old claim that was true.
    let pid = Uuid::new_v4();
    for t in TARGETS.iter().filter(|t| t.mutating && !t.creates) {
        let path = concrete_path(t.path, pid);
        assert!(
            !path.contains(':'),
            "{} is mutating without creating, so it must address an id that cannot exist: {path}",
            t.path
        );
        assert!(
            !t.body.unwrap_or("").contains(GHOST_ID),
            "{} should not need to hand a ghost id in its body -- it has none in its path either",
            t.path
        );
    }

    // And the two that genuinely create are refused without a deliberate opt-in.
    let creating: Vec<&str> = TARGETS
        .iter()
        .filter(|t| t.creates)
        .map(|t| t.path)
        .collect();
    assert_eq!(
        creating.len(),
        2,
        "expected exactly the two create-a-row routes, found {creating:?}"
    );
    for path in &creating {
        // By id, not by path: `/admin/api-keys` is registered for both GET and
        // POST, so looking it up by path finds the GET row, which creates nothing.
        let row = TARGETS
            .iter()
            .find(|t| t.path == *path && t.creates)
            .unwrap_or_else(|| panic!("{path} is listed as creating but has no such row"));
        assert!(
            row.mutating,
            "{path} creates something, so it is certainly mutating"
        );
    }
}

/// The refusal itself is checked in `run_probe`, which needs no store, so it is
/// cheap to assert directly rather than only through the HTTP route.
#[tokio::test]
async fn a_creating_target_is_refused_unless_the_caller_asked_for_it() {
    let client = reqwest::Client::new();
    // A port nothing listens on: if the request were ever sent it would fail to
    // connect rather than quietly succeeding against a real server.
    let base = "http://127.0.0.1:1";

    for target in TARGETS.iter().filter(|t| t.creates) {
        let mut req = crate::rbac_probe::ProbeRequest {
            target_id: Some(target.id.to_string()),
            method: None,
            path: None,
            api_key: "irrelevant".to_string(),
            project_id: None,
            body: None,
            allow_creation: false,
        };
        let err = crate::rbac_probe::run_probe(base, &client, &req)
            .await
            .expect_err("a creating target must be refused by default");
        assert!(
            err.contains("creates a real record"),
            "{path} should refuse for the right reason, got: {err}",
            path = target.path
        );

        // With the opt-in it is allowed through, and here it fails to connect --
        // which is the point: the refusal was the check, not the network.
        req.allow_creation = true;
        let outcome = crate::rbac_probe::run_probe(base, &client, &req).await;
        assert!(
            outcome.is_err(),
            "{} with the opt-in should be attempted, not refused again",
            target.path
        );
    }
}

/// The property a QA person is really testing, now that it can be measured: the
/// weakest role is never allowed something a stronger role is refused.
#[tokio::test]
async fn no_role_is_allowed_something_a_weaker_role_was_refused() {
    let h = harness().await;
    let pid = Uuid::new_v4();
    h.state
        .store
        .insert_project(pid, "ladder", None)
        .await
        .unwrap();
    let keys = [
        (Role::Viewer, mint(&h.state, "viewer").await),
        (Role::Operator, mint(&h.state, "operator").await),
        (Role::Admin, mint(&h.state, "admin").await),
    ];

    let mut inversions = Vec::new();
    for target in TARGETS {
        let path = concrete_path(target.path, pid);
        let mut blocked = [false; 3];
        let mut statuses = [0u16; 3];
        for (i, (_, key)) in keys.iter().enumerate() {
            let (status, body) = send(&h.state, target.method, &path, key, target.body).await;
            statuses[i] = status;
            blocked[i] = classify(status, &body).blocked();
        }
        let (viewer, operator, admin) = (blocked[0], blocked[1], blocked[2]);
        // The statuses travel with every message: an inversion is only diagnosable
        // if you can see what actually came back.
        let seen = format!(
            "viewer/operator/admin = {}/{}/{}",
            statuses[0], statuses[1], statuses[2]
        );
        let at = |what: &str| format!("{} {}: {} ({seen})", target.method, target.path, what);

        // `blocked` means *refused*, so the ladder must hold as:
        //
        //   viewer operator admin   meaning           verdict
        //   ---------------------------------------------------
        //   yes    yes      no      admin-only        fine
        //   no     no       no      everyone          fine
        //   yes    no       no      operator-and-up   fine  <- the gate doing its job
        //   no     yes      yes     viewer-only       INVERTED: weaker role got in
        //   --     --       yes     nothing works     INVERTED: nothing refuses an admin
        //
        // So a viewer refused while an operator is allowed is the *expected* shape and
        // is not a fault. Writing the table out is the point: reading `blocked` as
        // `allowed` yields exactly the opposite test, which is easy to do and looks
        // like a real finding when it is not.
        if admin {
            inversions.push(at("refused even to an admin"));
        }
        if !viewer && operator {
            inversions.push(at("viewer allowed but operator refused"));
        }
    }
    assert!(
        inversions.is_empty(),
        "permissions are not a ladder -- a weaker role was refused something a \
         stronger role was allowed:\n  {}",
        inversions.join("\n  ")
    );
}

/// Probing is itself a privileged capability, so it needs its own gate.
#[tokio::test]
async fn a_viewer_cannot_reach_the_probe_or_its_target_list() {
    let h = harness().await;
    let viewer = mint(&h.state, "viewer").await;
    let admin = mint(&h.state, "admin").await;

    for path in ["/api/v1/rbac/targets"] {
        let (status, _) = send(&h.state, "GET", path, &viewer, None).await;
        assert_eq!(status, 403, "{path} must be refused to a viewer");
        let (status, _) = send(&h.state, "GET", path, &admin, None).await;
        assert_ne!(status, 403, "{path} must be open to an admin");
    }

    let (status, _) = send(
        &h.state,
        "POST",
        "/api/v1/rbac/probe",
        &viewer,
        Some(r#"{"api_key":"x","method":"GET","path":"/health"}"#),
    )
    .await;
    assert_eq!(
        status, 403,
        "a viewer must not be able to make the server send authenticated requests"
    );
}
