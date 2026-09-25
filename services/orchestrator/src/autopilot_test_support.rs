//! What the tests of the autopilot's steps share: a running run in a real (temporary) database, a step context whose API is
//! scripted, and the small readers of a step's outcome.

use crate::autopilot::{AutopilotConfig, Step, StepError, StepOutcome};
use crate::autopilot_pipeline::scripted::ScriptedApi;
use crate::autopilot_pipeline::{PipelineApi, Reply};
use crate::autopilot_runner::{RunnerOptions, StepCtx};
use crate::autopilot_store::NewAutopilotRun;
use crate::store::Store;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

pub(crate) fn config(extra: Value) -> AutopilotConfig {
    let mut base = json!({
        "base_url": "https://shop.example.test",
        "roles": [{"name": "anonymous"}],
    });
    for (key, value) in extra.as_object().cloned().unwrap_or_default() {
        base[key] = value;
    }
    serde_json::from_value(base).unwrap()
}

pub(crate) struct Fixture {
    pub ctx: StepCtx,
    pub api: Arc<ScriptedApi>,
    pub _dir: tempfile::TempDir,
}

impl Fixture {
    pub fn p(&self, tail: &str) -> String {
        self.ctx.path(tail)
    }
}

/// A running run in a real (temporary) database, with a step context whose API is scripted by `script`.
pub(crate) async fn fixture(
    step: Step,
    config: AutopilotConfig,
    script: impl FnOnce(&dyn Fn(&str) -> String) -> ScriptedApi,
) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::connect(&dir.path().join("t.sqlite3")).await.unwrap();
    let project = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    store
        .insert_autopilot_run(
            &NewAutopilotRun {
                id: run_id,
                project_id: &project.to_string(),
                name: "t",
                config: &serde_json::to_value(&config).unwrap(),
                estimate: &json!({}),
                created_by_key: Uuid::new_v4(),
                created_by_name: "k",
            },
            &Step::ALL.map(Step::as_str),
        )
        .await
        .unwrap();
    store.start_autopilot_run(run_id, false).await.unwrap();
    store
        .begin_autopilot_step(run_id, step.as_str())
        .await
        .unwrap();
    let api = Arc::new(script(&|tail| format!("/api/v1/projects/{project}/{tail}")));
    let ctx = StepCtx {
        run_id,
        project_id: project,
        step,
        config,
        detail: json!({}),
        store,
        api: api.clone() as Arc<dyn PipelineApi>,
        options: RunnerOptions {
            poll_every: Duration::from_millis(1),
            crawl_timeout: Duration::from_secs(5),
        },
    };
    Fixture {
        ctx,
        api,
        _dir: dir,
    }
}

pub(crate) fn ok(body: Value) -> Reply {
    Reply::new(200, body)
}

pub(crate) fn failed(outcome: Result<StepOutcome, StepError>) -> String {
    match outcome {
        Err(StepError::Failed(why)) => why,
        other => panic!("expected a failed step, got {other:?}"),
    }
}

pub(crate) fn detail_of_done(outcome: Result<StepOutcome, StepError>) -> (Value, bool) {
    match outcome {
        Ok(StepOutcome::Done { detail, skipped }) => (detail, skipped),
        other => panic!("expected a finished step, got {other:?}"),
    }
}

pub(crate) async fn timeline(f: &Fixture) -> Vec<String> {
    f.ctx
        .store
        .list_autopilot_events(f.ctx.run_id, None, 100)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.message)
        .collect()
}
