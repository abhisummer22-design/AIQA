//! BE-14 — project-scoped realtime event stream (API Contract §14).
//!
//! Unlike `api_tests.rs` (in-process `tower::oneshot`), a WebSocket upgrade
//! can't be driven that way — these tests bind the real `axum::Router` to a
//! real, ephemeral TCP port and connect with an actual WebSocket client
//! (`tokio-tungstenite`), so "an authorized subscriber can subscribe" means
//! the real upgrade/auth/multiplex path ran, not that an internal
//! broadcaster was called directly the way production code never does.

use crate::ai_router::test_support::{healthy_model, healthy_provider, FakeDiscovery, FakeExecutor};
use crate::ai_router::{AiExecutor, AiInfrastructure, CapabilityLevel};
use crate::domain::RunEvent;
use crate::evidence::LocalEvidenceStore;
use crate::executor::{ExecutorRegistry, SimulatedExecutor};
use crate::policy::PolicyConfig;
use crate::state::AppState;
use crate::store::Store;
use futures_util::StreamExt;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::{Message as WsMessage, Error as WsError};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use uuid::Uuid;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn test_state() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("ws.sqlite3")).await.expect("connect");
    let evidence = LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence store");
    let registry = ExecutorRegistry::new(Arc::new(SimulatedExecutor));
    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: HashSet::new(),
        blocked_executors: HashSet::new(),
    };
    let mut executors: std::collections::HashMap<&'static str, Arc<dyn AiExecutor>> = std::collections::HashMap::new();
    executors.insert("mock", Arc::new(FakeExecutor::ok("{}")));
    let ai_infra = Arc::new(AiInfrastructure::new(
        vec![Arc::new(FakeDiscovery(healthy_provider("mock", vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)])))],
        executors,
    ));
    (AppState::new(Arc::new(registry), store, policy, evidence, ai_infra), dir)
}

async fn mint_key(state: &AppState, role: &str, project_id: Option<&str>) -> String {
    let raw = crate::auth::generate_key();
    state
        .store
        .insert_api_key(Uuid::new_v4(), "ws-test-key", &crate::auth::hash_key(&raw), role, project_id)
        .await
        .expect("insert_api_key");
    raw
}

async fn make_project(state: &AppState, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    state.store.insert_project(id, name, None).await.expect("insert_project");
    id
}

/// Binds the real router to a real, ephemeral TCP port and serves it in
/// the background. Returns the HTTP origin ("http://127.0.0.1:PORT") —
/// callers derive both REST and `ws://` URLs from it.
async fn spawn_server(state: AppState) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let app = crate::api::router(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

fn simple_plan(name: &str) -> Value {
    serde_json::json!({
        "name": name,
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "simulate": {"outcome": "pass", "delay_ms": 60}}],
    })
}

async fn create_run(client: &reqwest::Client, base: &str, project: Uuid, key: &str, plan: Value) -> Uuid {
    let resp = client
        .post(format!("{base}/api/v1/projects/{project}/executions"))
        .bearer_auth(key)
        .json(&plan)
        .send()
        .await
        .expect("create run request");
    assert_eq!(resp.status(), reqwest::StatusCode::CREATED, "create run must succeed");
    let body: Value = resp.json().await.expect("json body");
    Uuid::parse_str(body["run_id"].as_str().expect("run_id")).expect("valid uuid")
}

/// Attempts the WS handshake against the project events stream. `Ok` means
/// the upgrade actually succeeded (101 Switching Protocols); `Err` carries
/// the real pre-upgrade HTTP status tungstenite captured, for asserting the
/// rejection cases.
async fn try_connect_project_stream(base: &str, project: Uuid, key: Option<&str>) -> Result<WsStream, WsError> {
    let ws_base = base.replacen("http://", "ws://", 1);
    let url = match key {
        Some(k) => format!("{ws_base}/api/v1/projects/{project}/events?key={k}"),
        None => format!("{ws_base}/api/v1/projects/{project}/events"),
    };
    tokio_tungstenite::connect_async(url).await.map(|(ws, _resp)| ws)
}

fn rejection_status(err: &WsError) -> Option<u16> {
    match err {
        WsError::Http(resp) => Some(resp.status().as_u16()),
        _ => None,
    }
}

/// Reads the next real text frame as JSON, bounded so a bug (no event ever
/// arriving) fails fast instead of hanging the suite.
async fn recv_event(ws: &mut WsStream, timeout: Duration) -> Value {
    loop {
        let msg = tokio::time::timeout(timeout, ws.next())
            .await
            .expect("timed out waiting for a realtime event")
            .expect("stream ended before an event arrived")
            .expect("websocket error while waiting for an event");
        if let WsMessage::Text(text) = msg {
            return serde_json::from_str(&text).expect("event payload must be valid JSON");
        }
        // Ignore ping/pong/other control frames and keep waiting.
    }
}

/// Asserts no real event arrives within `dur` — used for the cross-project
/// leak check. Bounded, not an arbitrary long sleep.
async fn assert_silence(ws: &mut WsStream, dur: Duration, context: &str) {
    match tokio::time::timeout(dur, ws.next()).await {
        Ok(Some(Ok(WsMessage::Text(text)))) => panic!("{context}: unexpected event received: {text}"),
        Ok(Some(Ok(_))) | Ok(None) => {}
        Ok(Some(Err(e))) => panic!("{context}: websocket error while expecting silence: {e}"),
        Err(_) => {} // timed out with nothing received — the desired outcome
    }
}

// ---------------------------------------------------------------- BE-14

/// Category: authorized subscriber can establish a subscription.
#[tokio::test]
async fn be14_authorized_subscriber_can_connect_to_project_stream() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "WS Auth OK").await;
    let key = mint_key(&state, "operator", Some(&project.to_string())).await;
    let base = spawn_server(state).await;

    let ws = try_connect_project_stream(&base, project, Some(&key)).await;
    assert!(ws.is_ok(), "authorized subscriber must be able to connect: {ws:?}");
}

/// Category: missing authentication is rejected.
#[tokio::test]
async fn be14_missing_key_is_rejected() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "WS No Key").await;
    let base = spawn_server(state).await;

    let err = try_connect_project_stream(&base, project, None).await.expect_err("must be rejected");
    assert_eq!(rejection_status(&err), Some(401), "err={err:?}");
}

/// Category: nonexistent project is rejected with structured behavior
/// where applicable (pre-upgrade WS responses are deliberately plain, not
/// the JSON envelope — see `errors.rs`'s module doc comment on `ws_handler`
/// / G-WS; this asserts the real status code that IS guaranteed).
#[tokio::test]
async fn be14_nonexistent_project_is_rejected() {
    let (state, _dir) = test_state().await;
    let admin_key = mint_key(&state, "admin", None).await;
    let base = spawn_server(state).await;
    let phantom = Uuid::new_v4();

    let err = try_connect_project_stream(&base, phantom, Some(&admin_key)).await.expect_err("must be rejected");
    assert_eq!(rejection_status(&err), Some(404), "err={err:?}");
}

/// Category: Project-A-scoped caller cannot subscribe to Project B.
#[tokio::test]
async fn be14_cross_project_subscription_is_rejected() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "WS Tenant A").await;
    let project_b = make_project(&state, "WS Tenant B").await;
    let key_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let base = spawn_server(state).await;

    let err = try_connect_project_stream(&base, project_b, Some(&key_a)).await.expect_err("must be rejected");
    assert_eq!(rejection_status(&err), Some(403), "a project-A-scoped key must not reach project B's stream; err={err:?}");
}

/// Category: two real projects produce distinct execution events; each
/// subscriber receives only its own project's events. Also exercises
/// requirement "verify emitted events are real": events are triggered via
/// a genuine `POST .../executions` through the real router, not by calling
/// `RunRecord::emit`/the broadcaster directly.
#[tokio::test]
async fn be14_subscribers_receive_only_their_own_projects_real_events() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "WS Isolation A").await;
    let project_b = make_project(&state, "WS Isolation B").await;
    let key_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let key_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;
    let base = spawn_server(state).await;
    let client = reqwest::Client::new();

    let mut ws_a = try_connect_project_stream(&base, project_a, Some(&key_a)).await.expect("A connects");
    let mut ws_b = try_connect_project_stream(&base, project_b, Some(&key_b)).await.expect("B connects");

    let run_a = create_run(&client, &base, project_a, &key_a, simple_plan("run-a")).await;

    // Category: cross-project leak — B must see nothing from A's run.
    let ev_a = recv_event(&mut ws_a, Duration::from_secs(5)).await;
    assert_eq!(ev_a["run_id"], run_a.to_string(), "event={ev_a}");
    assert_eq!(ev_a["project_id"], project_a.to_string(), "event={ev_a}");
    assert_silence(&mut ws_b, Duration::from_millis(400), "project B subscriber during project A's run").await;

    // Now trigger a real event in B and confirm A never sees it either —
    // the leak check in both directions, not just one.
    let run_b = create_run(&client, &base, project_b, &key_b, simple_plan("run-b")).await;
    let ev_b = recv_event(&mut ws_b, Duration::from_secs(5)).await;
    assert_eq!(ev_b["run_id"], run_b.to_string(), "event={ev_b}");
    assert_eq!(ev_b["project_id"], project_b.to_string(), "event={ev_b}");

    // Drain whatever A already received (its own run may still be emitting
    // TASK_STARTED/RUN_COMPLETED) and confirm none of it is B's run.
    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(deadline - tokio::time::Instant::now(), ws_a.next()).await {
            Ok(Some(Ok(WsMessage::Text(text)))) => {
                let ev: Value = serde_json::from_str(&text).expect("json");
                assert_ne!(ev["run_id"], run_b.to_string(), "project A subscriber leaked project B's run: {ev}");
                assert_ne!(ev["project_id"], project_b.to_string(), "project A subscriber leaked project B's project_id: {ev}");
            }
            _ => break,
        }
    }
}

/// Category: disconnect/resource cleanup. After the client disconnects,
/// the project's broadcast channel must no longer count it as a
/// subscriber — verified directly against the real `broadcast::Sender`
/// (`receiver_count()`), not inferred from absence of a crash.
#[tokio::test]
async fn be14_disconnect_releases_the_subscriber() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "WS Disconnect").await;
    let key = mint_key(&state, "operator", Some(&project.to_string())).await;
    let state_handle = state.clone();
    let base = spawn_server(state).await;

    let ws = try_connect_project_stream(&base, project, Some(&key)).await.expect("connect");

    // Give the server side a moment to finish the upgrade and subscribe.
    let mut subscribed = false;
    for _ in 0..50 {
        if state_handle.project_stream(&project.to_string()).await.tx.receiver_count() >= 1 {
            subscribed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(subscribed, "server must have subscribed to the project stream after a successful upgrade");

    drop(ws); // client disconnects

    let mut released = false;
    for _ in 0..50 {
        if state_handle.project_stream(&project.to_string()).await.tx.receiver_count() == 0 {
            released = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(released, "disconnecting the client must release its subscription (no leaked receiver)");
}

/// Category: existing per-run catch-up REST endpoint must continue to
/// work unchanged.
#[tokio::test]
async fn be14_per_run_events_rest_endpoint_still_works() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "REST Regression").await;
    let key = mint_key(&state, "operator", Some(&project.to_string())).await;
    let base = spawn_server(state).await;
    let client = reqwest::Client::new();

    let run_id = create_run(&client, &base, project, &key, simple_plan("rest-check")).await;
    tokio::time::sleep(Duration::from_millis(200)).await; // let the fast SimulatedExecutor task finish

    let resp = client
        .get(format!("{base}/api/v1/projects/{project}/executions/{run_id}/events"))
        .bearer_auth(&key)
        .send()
        .await
        .expect("events request");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let events: Vec<RunEvent> = resp.json().await.expect("json");
    assert!(!events.is_empty(), "a completed run must have real persisted/in-memory events");
    assert!(events.iter().all(|e| e.run_id == run_id));
}

/// The Lagged-path fix (Integration Plan §11.2 item 5 / audit M5): a
/// subscriber that falls too far behind must be closed with a distinct
/// code, not silently skipped forever. Backpressure is induced
/// deterministically by publishing filler messages directly onto the same
/// real broadcast channel production code uses (`AppState::project_stream`)
/// in a tight, non-yielding loop — this test is about the transport's own
/// lag-handling behavior, not about event authenticity (that's what the
/// isolation tests above cover, and none of them do this).
#[tokio::test]
async fn be14_lagging_subscriber_is_closed_with_distinct_code_not_silently_skipped() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "WS Lag").await;
    let key = mint_key(&state, "operator", Some(&project.to_string())).await;
    let stream = state.project_stream(&project.to_string()).await;
    let state_for_server = state.clone();
    let base = spawn_server(state_for_server).await;

    let mut ws = try_connect_project_stream(&base, project, Some(&key)).await.expect("connect");

    let mut subscribed = false;
    for _ in 0..50 {
        if stream.tx.receiver_count() >= 1 {
            subscribed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(subscribed, "server must have subscribed before the flood begins");

    // Non-yielding loop: no `.await` between sends, so the server's
    // consumer task cannot be scheduled to drain any of these until after
    // the flood is already well past the channel's capacity (1024).
    for i in 0..2_000u64 {
        let _ = stream.tx.send(RunEvent {
            event_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            task_id: None,
            sequence: i,
            timestamp: chrono::Utc::now(),
            schema_version: 1,
            event_type: crate::domain::EventType::RunScheduled,
            status: None,
            summary: Some(format!("filler {i}")),
            project_id: Some(project.to_string()),
            project_sequence: Some(i),
        });
    }

    let close = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("timed out waiting for the lag close")
        .expect("stream ended without a close frame");
    match close {
        Ok(WsMessage::Close(Some(frame))) => {
            assert_eq!(frame.code, tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::from(4001u16), "close frame={frame:?}")
        }
        other => panic!("expected a Close(4001) frame after lagging, got: {other:?}"),
    }
}

/// Category: existing per-run WebSocket must not regress.
#[tokio::test]
async fn be14_legacy_per_run_websocket_still_works() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Legacy WS Regression").await;
    let key = mint_key(&state, "operator", Some(&project.to_string())).await;
    let base = spawn_server(state).await;
    let client = reqwest::Client::new();

    let run_id = create_run(&client, &base, project, &key, simple_plan("legacy-ws")).await;

    let ws_base = base.replacen("http://", "ws://", 1);
    let (mut ws, _resp) = tokio_tungstenite::connect_async(format!("{ws_base}/runs/{run_id}/ws?key={key}"))
        .await
        .expect("legacy per-run ws connect");

    let ev = recv_event(&mut ws, Duration::from_secs(5)).await;
    assert_eq!(ev["run_id"], run_id.to_string(), "event={ev}");
}
