//! What each step of an autopilot run does. Every one of them calls the routes a person would use, through the run's
//! `PipelineApi`, and reports what it found in words and numbers a person can read in the run's page.
//!
//! A step can be started again after the service was restarted, so what it has done that must not be done twice (a crawl
//! it started) is saved as it goes (`StepCtx::checkpoint`) and looked for first (`StepCtx::detail`).

use crate::autopilot::{Step, StepError, StepOutcome};
use crate::autopilot_runner::{StepCtx, StepRunner};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::time::Instant;
use uuid::Uuid;

/// The steps as they are: each one calls the API.
pub struct ApiSteps;

#[async_trait]
impl StepRunner for ApiSteps {
    async fn run(&self, step: Step, ctx: &StepCtx) -> Result<StepOutcome, StepError> {
        match step {
            Step::Requirements => requirements(ctx).await,
            Step::Crawl => crawl(ctx).await,
            Step::GapCheck => gap_check(ctx).await,
            Step::Generate => crate::autopilot_generate::generate(ctx).await,
            Step::Validate => crate::autopilot_generate::validate(ctx).await,
            Step::Approve => crate::autopilot_generate::approve(ctx).await,
            Step::Execute => crate::autopilot_execute::execute(ctx).await,
            Step::Report => crate::autopilot_report::report(ctx).await,
        }
    }
}

fn done(detail: Value) -> Result<StepOutcome, StepError> {
    Ok(StepOutcome::Done {
        detail,
        skipped: false,
    })
}

// ------------------------------------------------------------------------------------------------- requirements

/// The requirements the run is about exist: the documents were read for them when they were uploaded, so this is a look,
/// not a job, and a run with nothing to test stops here, saying what to do.
pub async fn requirements(ctx: &StepCtx) -> Result<StepOutcome, StepError> {
    let chosen = &ctx.config.requirement_document_ids;
    let mut path = ctx.path("requirements");
    if !chosen.is_empty() {
        let ids: Vec<String> = chosen.iter().map(Uuid::to_string).collect();
        path = format!("{path}?document_ids={}", ids.join(","));
    }
    let reply = ctx.api.get(&path).await;
    if !reply.is_ok() {
        return Err(StepError::failed(format!(
            "the requirements could not be read: {}",
            reply.problem()
        )));
    }
    let documents = reply.body["documents"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let requirements = reply.body["requirements"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let known: HashSet<&str> = documents.iter().filter_map(|d| d["id"].as_str()).collect();
    let missing: Vec<String> = chosen
        .iter()
        .map(Uuid::to_string)
        .filter(|id| !known.contains(id.as_str()))
        .collect();
    if !missing.is_empty() {
        return Err(StepError::failed(format!(
            "these requirement documents are not in the project any more: {}",
            missing.join(", ")
        )));
    }
    if requirements.is_empty() {
        return Err(StepError::failed(
            "there are no requirements to test: upload a requirements document first (Test Case Generator, Source & Generation)",
        ));
    }
    let mut by_kind: BTreeMap<String, u64> = BTreeMap::new();
    for requirement in &requirements {
        let kind = requirement["kind"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        *by_kind.entry(kind).or_default() += 1;
    }
    done(json!({
        "documents": documents.len(),
        "requirements": requirements.len(),
        "by_kind": by_kind,
    }))
}

// ------------------------------------------------------------------------------------------------- crawl

/// The application is crawled as each role, or a crawl already made is used. The crawl is an ordinary run of the
/// project (its page and evidence are in Executions), started and watched here.
pub async fn crawl(ctx: &StepCtx) -> Result<StepOutcome, StepError> {
    if let Some(run) = ctx.config.crawl_run_id {
        return reuse_crawl(ctx, run).await;
    }
    // A crawl an earlier attempt started is the one to wait for, not a reason to start another.
    let crawl_run = match ctx.detail.get("crawl_run_id").and_then(Value::as_str) {
        Some(id) => id.to_string(),
        None => start_crawl(ctx).await?,
    };
    wait_for_crawl(ctx, &crawl_run).await
}

async fn reuse_crawl(ctx: &StepCtx, run: Uuid) -> Result<StepOutcome, StepError> {
    let reply = ctx.api.get(&ctx.path(&format!("executions/{run}"))).await;
    if !reply.is_ok() {
        return Err(StepError::failed(format!(
            "the crawl you chose ({run}) could not be read from this project: {}",
            reply.problem()
        )));
    }
    if !reply.body["name"]
        .as_str()
        .unwrap_or("")
        .starts_with("Crawl:")
    {
        return Err(StepError::failed(format!("run {run} is not a crawl")));
    }
    let state = reply.body["state"].as_str().unwrap_or("unknown");
    if state != "COMPLETED" {
        return Err(StepError::failed(format!(
            "the crawl you chose ({run}) has not finished successfully: it is {state}"
        )));
    }
    Ok(StepOutcome::Done {
        detail: json!({"crawl_run_id": run.to_string(), "reused": true}),
        skipped: true,
    })
}

async fn start_crawl(ctx: &StepCtx) -> Result<String, StepError> {
    let roles = serde_json::to_value(&ctx.config.roles)
        .map_err(|e| StepError::failed(format!("the roles could not be written: {e}")))?;
    let reply = ctx
        .api
        .post(
            &ctx.path("crawl"),
            json!({
                "base_url": ctx.config.base_url,
                "roles": roles,
                "focus_requirements": ctx.config.focus_requirements,
            }),
        )
        .await;
    if !reply.is_ok() {
        return Err(StepError::failed(format!(
            "the crawl could not be started: {}",
            reply.problem()
        )));
    }
    let Some(run) = reply.body["run_id"].as_str() else {
        return Err(StepError::failed(
            "the crawl was started but the service did not say which run it is",
        ));
    };
    ctx.checkpoint(&json!({"crawl_run_id": run, "reused": false}))
        .await?;
    ctx.note(
        &format!("Crawl started as run {run}"),
        Some(json!({"crawl_run_id": run})),
    )
    .await;
    Ok(run.to_string())
}

/// Looks at the crawl's run until it ends (or the person cancels the autopilot, or it takes too long).
async fn wait_for_crawl(ctx: &StepCtx, crawl_run: &str) -> Result<StepOutcome, StepError> {
    let path = ctx.path(&format!("executions/{crawl_run}"));
    let started = Instant::now();
    let mut said_held = false;
    loop {
        if let Err(cancelled) = ctx.check_cancel().await {
            // Best effort: the crawl is stopped too, so it does not go on for nothing.
            let _ = ctx.api.post(&format!("{path}/cancel"), json!({})).await;
            return Err(cancelled);
        }
        let reply = ctx.api.get(&path).await;
        if !reply.is_ok() {
            return Err(StepError::failed(format!(
                "the crawl (run {crawl_run}) could not be read: {}",
                reply.problem()
            )));
        }
        match reply.body["state"].as_str().unwrap_or("") {
            "COMPLETED" => break,
            state @ ("FAILED" | "PARTIAL" | "CANCELLED" | "REJECTED") => {
                let reason = reply.body["tasks"][0]["reason"]
                    .as_str()
                    .map(|r| format!(": {r}"))
                    .unwrap_or_default();
                return Err(StepError::failed(format!(
                    "the crawl (run {crawl_run}) ended {state}{reason}"
                )));
            }
            "PENDING_APPROVAL" if !said_held => {
                said_held = true;
                ctx.note(
                    "The crawl is held for approval under this project's policy (Executions): the autopilot waits for it",
                    None,
                )
                .await;
            }
            _ => {}
        }
        if started.elapsed() > ctx.options.crawl_timeout {
            return Err(StepError::failed(format!(
                "the crawl (run {crawl_run}) did not finish in {} minutes; it is still that run in Executions, where it can be cancelled",
                ctx.options.crawl_timeout.as_secs() / 60
            )));
        }
        ctx.pause().await?;
    }

    let reply = ctx
        .api
        .get(&ctx.path(&format!("crawl/{crawl_run}/result")))
        .await;
    if !reply.is_ok() {
        return Err(StepError::failed(format!(
            "the crawl finished but left no result to read: {}",
            reply.problem()
        )));
    }
    let mut roles = Vec::new();
    let mut pages_total = 0u64;
    let mut reached_none = Vec::new();
    for role in reply.body["roles"].as_array().cloned().unwrap_or_default() {
        let name = role["role"].as_str().unwrap_or("?").to_string();
        let pages = role["pages"].as_array().map_or(0, Vec::len) as u64;
        let logged_in = role["loginSucceeded"].as_bool().unwrap_or(false);
        let error = role["loginError"].as_str().map(str::to_string);
        pages_total += pages;
        if !logged_in {
            let why = error
                .clone()
                .unwrap_or_else(|| "no reason was recorded".to_string());
            ctx.note(
                &format!("The crawl could not log in as {name}: {why}"),
                None,
            )
            .await;
        }
        if pages == 0 {
            reached_none.push(name.clone());
        }
        roles.push(json!({"role": name, "login_succeeded": logged_in, "pages": pages, "login_error": error}));
    }
    if pages_total == 0 {
        return Err(StepError::failed(format!(
            "the crawl reached no page as {}: check the address and the login steps, then start again",
            if reached_none.is_empty() { "any role".to_string() } else { reached_none.join(", ") }
        )));
    }
    done(json!({
        "crawl_run_id": crawl_run,
        "reused": false,
        "pages": pages_total,
        "roles": roles,
    }))
}

// ------------------------------------------------------------------------------------------------- gap check

/// The requirements against what the crawl found, by the gap check the application already has (and, when the run asks
/// for it, the AI's look at the ones the words left unclear).
pub async fn gap_check(ctx: &StepCtx) -> Result<StepOutcome, StepError> {
    let crawl = ctx.detail_of(Step::Crawl).await;
    let Some(crawl_run) = crawl["crawl_run_id"].as_str() else {
        return Err(StepError::failed(
            "there is no crawl to check against: the crawl step did not record one",
        ));
    };
    let mut body = json!({
        "crawl_run_id": crawl_run,
        "document_ids": ctx.config.requirement_document_ids,
    });
    if let Some(most) = ctx.config.verify_gaps {
        body["verify"] = json!(true);
        body["max_verifications"] = json!(most);
    }
    let reply = ctx.api.post(&ctx.path("gap-check"), body).await;
    if !reply.is_ok() {
        return Err(StepError::failed(format!(
            "the requirements could not be checked against the crawl: {}",
            reply.problem()
        )));
    }
    let summary = reply.body["summary"].clone();
    let missing = summary["missing"].as_u64().unwrap_or(0);
    let weak = summary["weak"].as_u64().unwrap_or(0);
    if missing > 0 || weak > 0 {
        ctx.note(
            &format!(
                "{missing} requirement(s) show no sign in the crawl and {weak} only a weak one: cases for them may not be able to run"
            ),
            None,
        )
        .await;
    }
    done(json!({
        "crawl_run_id": crawl_run,
        "summary": summary,
        "notes": reply.body["notes"],
        "method": reply.body["method"],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopilot_pipeline::scripted::ScriptedApi;
    use crate::autopilot_pipeline::Reply;
    use crate::autopilot_test_support::*;
    use std::time::Duration;

    // ------------------------------------------------------------------------------ requirements

    fn requirements_reply(kinds: &[&str], documents: &[&str]) -> Value {
        json!({
            "documents": documents.iter().map(|id| json!({"id": id, "file_name": "FRD.md"})).collect::<Vec<_>>(),
            "requirements": kinds.iter().enumerate().map(|(i, k)| json!({"key": format!("FR-{i}"), "kind": k, "text": "x"})).collect::<Vec<_>>(),
        })
    }

    #[tokio::test]
    async fn the_requirements_step_counts_what_the_documents_hold_by_kind() {
        let f = fixture(Step::Requirements, config(json!({})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p("requirements"),
                vec![ok(requirements_reply(
                    &["functional", "functional", "security", "performance"],
                    &["d1", "d2"],
                ))],
            )
        })
        .await;
        let (detail, skipped) = detail_of_done(requirements(&f.ctx).await);
        assert!(!skipped);
        assert_eq!(
            detail,
            json!({"documents": 2, "requirements": 4, "by_kind": {"functional": 2, "performance": 1, "security": 1}})
        );
        assert_eq!(
            f.api.seen(),
            [format!("GET {}", f.p("requirements"))],
            "no filter when no document is chosen"
        );
    }

    #[tokio::test]
    async fn the_requirements_step_asks_only_for_the_documents_the_run_is_about() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let f = fixture(
            Step::Requirements,
            config(json!({"requirement_document_ids": [a, b]})),
            |p| {
                ScriptedApi::new().on(
                    "GET",
                    &format!("{}?document_ids={a},{b}", p("requirements")),
                    vec![ok(requirements_reply(
                        &["functional"],
                        &[&a.to_string(), &b.to_string()],
                    ))],
                )
            },
        )
        .await;
        let (detail, _) = detail_of_done(requirements(&f.ctx).await);
        assert_eq!(detail["documents"], 2);
        assert_eq!(f.api.seen().len(), 1);
    }

    #[tokio::test]
    async fn the_requirements_step_fails_with_what_to_do_when_there_is_nothing_to_test() {
        let f = fixture(Step::Requirements, config(json!({})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p("requirements"),
                vec![ok(requirements_reply(&[], &[]))],
            )
        })
        .await;
        let why = failed(requirements(&f.ctx).await);
        assert!(why.contains("there are no requirements to test"), "{why}");
        assert!(why.contains("upload a requirements document"), "{why}");
    }

    #[tokio::test]
    async fn the_requirements_step_names_a_chosen_document_that_is_gone_and_a_service_error() {
        let (kept, gone) = (Uuid::new_v4(), Uuid::new_v4());
        let f = fixture(
            Step::Requirements,
            config(json!({"requirement_document_ids": [kept, gone]})),
            |p| {
                ScriptedApi::new().on(
                    "GET",
                    &format!("{}?document_ids={kept},{gone}", p("requirements")),
                    vec![ok(requirements_reply(
                        &["functional"],
                        &[&kept.to_string()],
                    ))],
                )
            },
        )
        .await;
        let why = failed(requirements(&f.ctx).await);
        assert!(
            why.contains(&gone.to_string()) && !why.contains(&kept.to_string()),
            "{why}"
        );

        let f = fixture(Step::Requirements, config(json!({})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p("requirements"),
                vec![Reply::error(500, "internal", "the database is locked")],
            )
        })
        .await;
        let why = failed(requirements(&f.ctx).await);
        assert_eq!(
            why,
            "the requirements could not be read: the database is locked"
        );
    }

    // ------------------------------------------------------------------------------ crawl

    fn run_reply(state: &str, name: &str) -> Reply {
        ok(
            json!({"run_id": "r", "name": name, "state": state, "tasks": [{"id": "t", "status": "FAILED", "reason": "the page never loaded"}]}),
        )
    }

    fn result_body(roles: Value) -> Reply {
        ok(json!({"roles": roles, "visibilityMatrix": []}))
    }

    #[tokio::test]
    async fn a_crawl_that_was_already_made_is_used_and_the_step_is_skipped() {
        let run = Uuid::new_v4();
        let f = fixture(Step::Crawl, config(json!({"crawl_run_id": run})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p(&format!("executions/{run}")),
                vec![run_reply("COMPLETED", "Crawl: https://shop.example.test")],
            )
        })
        .await;
        let (detail, skipped) = detail_of_done(crawl(&f.ctx).await);
        assert!(skipped, "nothing was done");
        assert_eq!(
            detail,
            json!({"crawl_run_id": run.to_string(), "reused": true})
        );
        assert_eq!(f.api.seen().len(), 1, "no crawl was started");
    }

    #[tokio::test]
    async fn a_crawl_chosen_that_is_not_a_finished_crawl_of_the_project_is_refused_saying_why() {
        let run = Uuid::new_v4();
        let f = fixture(Step::Crawl, config(json!({"crawl_run_id": run})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p(&format!("executions/{run}")),
                vec![Reply::error(404, "not_found", "run not found")],
            )
        })
        .await;
        let why = failed(crawl(&f.ctx).await);
        assert!(
            why.contains("could not be read from this project: run not found"),
            "{why}"
        );

        let f = fixture(Step::Crawl, config(json!({"crawl_run_id": run})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p(&format!("executions/{run}")),
                vec![run_reply("COMPLETED", "Login smoke test")],
            )
        })
        .await;
        assert_eq!(
            failed(crawl(&f.ctx).await),
            format!("run {run} is not a crawl")
        );

        for state in ["RUNNING", "FAILED"] {
            let f = fixture(Step::Crawl, config(json!({"crawl_run_id": run})), |p| {
                ScriptedApi::new().on(
                    "GET",
                    &p(&format!("executions/{run}")),
                    vec![run_reply(state, "Crawl: https://x")],
                )
            })
            .await;
            let why = failed(crawl(&f.ctx).await);
            assert!(why.contains(&format!("it is {state}")), "{why}");
        }
    }

    #[tokio::test]
    async fn a_crawl_is_started_with_the_roles_and_watched_until_it_ends_then_summarised() {
        let f = fixture(
            Step::Crawl,
            config(json!({
                "focus_requirements": true,
                "roles": [{"name": "anonymous"}, {"name": "admin", "login_actions": [{"type": "NAVIGATE", "url": "https://shop.example.test/login"}]}],
            })),
            |p| {
                ScriptedApi::new()
                    .on("POST", &p("crawl"), vec![Reply::new(202, json!({"run_id": "crawl-1"}))])
                    .on(
                        "GET",
                        &p("executions/crawl-1"),
                        vec![run_reply("RUNNING", "Crawl: x"), run_reply("RUNNING", "Crawl: x"), run_reply("COMPLETED", "Crawl: x")],
                    )
                    .on(
                        "GET",
                        &p("crawl/crawl-1/result"),
                        vec![result_body(json!([
                            {"role": "anonymous", "loginSucceeded": true, "pages": [{}, {}, {}]},
                            {"role": "admin", "loginSucceeded": true, "pages": [{}, {}]},
                        ]))],
                    )
            },
        )
        .await;
        let (detail, skipped) = detail_of_done(crawl(&f.ctx).await);
        assert!(!skipped);
        assert_eq!(detail["crawl_run_id"], "crawl-1");
        assert_eq!(detail["pages"], 5);
        assert_eq!(detail["roles"][1]["role"], "admin");
        assert_eq!(detail["roles"][1]["pages"], 2);
        assert_eq!(detail["roles"][1]["login_succeeded"], true);

        // What was asked for: the address, the roles with their login steps, the focus.
        let sent = f.api.body_of("POST", &f.p("crawl")).unwrap();
        assert_eq!(sent["base_url"], "https://shop.example.test");
        assert_eq!(sent["focus_requirements"], true);
        assert_eq!(sent["roles"][1]["name"], "admin");
        assert_eq!(sent["roles"][1]["login_actions"][0]["type"], "NAVIGATE");
        // Watched until it ended: three looks at the run, then the result.
        let seen = f.api.seen();
        assert_eq!(
            seen.iter()
                .filter(|s| s.ends_with("executions/crawl-1"))
                .count(),
            3,
            "{seen:?}"
        );
        assert_eq!(
            seen.last().unwrap(),
            &format!("GET {}", f.p("crawl/crawl-1/result"))
        );
        // The run's id is saved the moment it is known, and the timeline says so.
        let step = f
            .ctx
            .store
            .list_autopilot_steps(f.ctx.run_id)
            .await
            .unwrap()
            .into_iter()
            .find(|s| s.name == "CRAWL")
            .unwrap();
        assert_eq!(step.detail["crawl_run_id"], "crawl-1");
        assert!(timeline(&f)
            .await
            .iter()
            .any(|m| m == "Crawl started as run crawl-1"));
    }

    #[tokio::test]
    async fn a_crawl_an_earlier_attempt_started_is_waited_for_not_started_again() {
        let mut f = fixture(Step::Crawl, config(json!({})), |p| {
            ScriptedApi::new()
                .on(
                    "GET",
                    &p("executions/crawl-1"),
                    vec![run_reply("COMPLETED", "Crawl: x")],
                )
                .on(
                    "GET",
                    &p("crawl/crawl-1/result"),
                    vec![result_body(
                        json!([{"role": "anonymous", "loginSucceeded": true, "pages": [{}]}]),
                    )],
                )
        })
        .await;
        f.ctx.detail = json!({"crawl_run_id": "crawl-1", "reused": false});
        let (detail, _) = detail_of_done(crawl(&f.ctx).await);
        assert_eq!(detail["crawl_run_id"], "crawl-1");
        assert!(
            !f.api.seen().iter().any(|s| s.starts_with("POST")),
            "{:?}",
            f.api.seen()
        );
    }

    #[tokio::test]
    async fn a_crawl_that_cannot_be_started_fails_the_step_with_the_services_reason() {
        let f = fixture(Step::Crawl, config(json!({})), |p| {
            ScriptedApi::new().on(
                "POST",
                &p("crawl"),
                vec![Reply::error(
                    422,
                    "validation",
                    "environment variable AIQA_LOGIN_ADMIN_PASSWORD is not set",
                )],
            )
        })
        .await;
        let why = failed(crawl(&f.ctx).await);
        assert_eq!(why, "the crawl could not be started: environment variable AIQA_LOGIN_ADMIN_PASSWORD is not set");
        let f = fixture(Step::Crawl, config(json!({})), |p| {
            ScriptedApi::new().on("POST", &p("crawl"), vec![Reply::new(202, json!({}))])
        })
        .await;
        assert!(failed(crawl(&f.ctx).await).contains("did not say which run"));
    }

    #[tokio::test]
    async fn a_crawl_that_ends_badly_fails_the_step_with_the_tasks_reason() {
        for state in ["FAILED", "PARTIAL", "CANCELLED", "REJECTED"] {
            let f = fixture(Step::Crawl, config(json!({})), |p| {
                ScriptedApi::new()
                    .on(
                        "POST",
                        &p("crawl"),
                        vec![Reply::new(202, json!({"run_id": "c"}))],
                    )
                    .on(
                        "GET",
                        &p("executions/c"),
                        vec![run_reply(state, "Crawl: x")],
                    )
            })
            .await;
            let why = failed(crawl(&f.ctx).await);
            assert_eq!(
                why,
                format!("the crawl (run c) ended {state}: the page never loaded")
            );
        }
        // No reason recorded: none is invented.
        let f = fixture(Step::Crawl, config(json!({})), |p| {
            ScriptedApi::new()
                .on(
                    "POST",
                    &p("crawl"),
                    vec![Reply::new(202, json!({"run_id": "c"}))],
                )
                .on(
                    "GET",
                    &p("executions/c"),
                    vec![ok(
                        json!({"name": "Crawl: x", "state": "FAILED", "tasks": []}),
                    )],
                )
        })
        .await;
        assert_eq!(
            failed(crawl(&f.ctx).await),
            "the crawl (run c) ended FAILED"
        );
    }

    #[tokio::test]
    async fn a_crawl_run_that_cannot_be_read_or_leaves_no_result_fails_the_step() {
        let f = fixture(Step::Crawl, config(json!({})), |p| {
            ScriptedApi::new()
                .on(
                    "POST",
                    &p("crawl"),
                    vec![Reply::new(202, json!({"run_id": "c"}))],
                )
                .on(
                    "GET",
                    &p("executions/c"),
                    vec![Reply::error(404, "not_found", "run not found")],
                )
        })
        .await;
        assert!(failed(crawl(&f.ctx).await)
            .contains("the crawl (run c) could not be read: run not found"));

        let f = fixture(Step::Crawl, config(json!({})), |p| {
            ScriptedApi::new()
                .on(
                    "POST",
                    &p("crawl"),
                    vec![Reply::new(202, json!({"run_id": "c"}))],
                )
                .on(
                    "GET",
                    &p("executions/c"),
                    vec![run_reply("COMPLETED", "Crawl: x")],
                )
                .on(
                    "GET",
                    &p("crawl/c/result"),
                    vec![Reply::error(404, "not_found", "crawl result not found")],
                )
        })
        .await;
        let why = failed(crawl(&f.ctx).await);
        assert!(
            why.contains("left no result to read: crawl result not found"),
            "{why}"
        );
    }

    #[tokio::test]
    async fn a_crawl_that_reached_no_page_fails_the_step_and_one_role_that_could_not_log_in_is_only_noted(
    ) {
        let f = fixture(Step::Crawl, config(json!({})), |p| {
            ScriptedApi::new()
                .on("POST", &p("crawl"), vec![Reply::new(202, json!({"run_id": "c"}))])
                .on("GET", &p("executions/c"), vec![run_reply("COMPLETED", "Crawl: x")])
                .on("GET", &p("crawl/c/result"), vec![result_body(json!([{"role": "admin", "loginSucceeded": false, "loginError": "wrong password", "pages": []}]))])
        })
        .await;
        let why = failed(crawl(&f.ctx).await);
        assert!(
            why.starts_with("the crawl reached no page as admin"),
            "{why}"
        );
        assert!(timeline(&f)
            .await
            .iter()
            .any(|m| m == "The crawl could not log in as admin: wrong password"));

        let f = fixture(Step::Crawl, config(json!({})), |p| {
            ScriptedApi::new()
                .on(
                    "POST",
                    &p("crawl"),
                    vec![Reply::new(202, json!({"run_id": "c"}))],
                )
                .on(
                    "GET",
                    &p("executions/c"),
                    vec![run_reply("COMPLETED", "Crawl: x")],
                )
                .on(
                    "GET",
                    &p("crawl/c/result"),
                    vec![result_body(json!([
                        {"role": "anonymous", "loginSucceeded": true, "pages": [{}]},
                        {"role": "admin", "loginSucceeded": false, "pages": []},
                    ]))],
                )
        })
        .await;
        let (detail, _) = detail_of_done(crawl(&f.ctx).await);
        assert_eq!(detail["pages"], 1);
        assert_eq!(detail["roles"][1]["login_succeeded"], false);
        assert!(timeline(&f)
            .await
            .iter()
            .any(|m| m == "The crawl could not log in as admin: no reason was recorded"));
    }

    #[tokio::test]
    async fn a_cancelled_run_stops_the_crawl_it_started_and_the_step() {
        let f = fixture(Step::Crawl, config(json!({})), |p| {
            ScriptedApi::new()
                .on(
                    "POST",
                    &p("crawl"),
                    vec![Reply::new(202, json!({"run_id": "c"}))],
                )
                .on(
                    "GET",
                    &p("executions/c"),
                    vec![run_reply("RUNNING", "Crawl: x")],
                )
                .on("POST", &p("executions/c/cancel"), vec![ok(json!({}))])
        })
        .await;
        f.ctx
            .store
            .cancel_autopilot_run(f.ctx.run_id)
            .await
            .unwrap();
        assert_eq!(crawl(&f.ctx).await, Err(StepError::Cancelled));
        assert!(
            f.api
                .seen()
                .contains(&format!("POST {}", f.p("executions/c/cancel"))),
            "{:?}",
            f.api.seen()
        );
    }

    #[tokio::test]
    async fn a_crawl_that_takes_too_long_is_given_up_on_with_where_to_find_it() {
        let mut f = fixture(Step::Crawl, config(json!({})), |p| {
            ScriptedApi::new()
                .on(
                    "POST",
                    &p("crawl"),
                    vec![Reply::new(202, json!({"run_id": "c"}))],
                )
                .on(
                    "GET",
                    &p("executions/c"),
                    vec![run_reply("RUNNING", "Crawl: x")],
                )
        })
        .await;
        f.ctx.options.crawl_timeout = Duration::from_millis(30);
        let why = failed(crawl(&f.ctx).await);
        assert!(
            why.starts_with("the crawl (run c) did not finish in 0 minutes"),
            "{why}"
        );
        assert!(why.contains("Executions"), "{why}");
    }

    #[tokio::test]
    async fn a_crawl_held_for_approval_is_waited_for_and_the_timeline_says_so_once() {
        let f = fixture(Step::Crawl, config(json!({})), |p| {
            ScriptedApi::new()
                .on(
                    "POST",
                    &p("crawl"),
                    vec![Reply::new(202, json!({"run_id": "c"}))],
                )
                .on(
                    "GET",
                    &p("executions/c"),
                    vec![
                        run_reply("PENDING_APPROVAL", "Crawl: x"),
                        run_reply("PENDING_APPROVAL", "Crawl: x"),
                        run_reply("COMPLETED", "Crawl: x"),
                    ],
                )
                .on(
                    "GET",
                    &p("crawl/c/result"),
                    vec![result_body(
                        json!([{"role": "anonymous", "loginSucceeded": true, "pages": [{}]}]),
                    )],
                )
        })
        .await;
        detail_of_done(crawl(&f.ctx).await);
        let held = timeline(&f)
            .await
            .into_iter()
            .filter(|m| m.contains("held for approval"))
            .count();
        assert_eq!(held, 1);
    }

    // ------------------------------------------------------------------------------ gap check

    async fn with_crawl_recorded(f: &Fixture) {
        f.ctx
            .store
            .save_autopilot_step_detail(f.ctx.run_id, "CRAWL", &json!({"crawl_run_id": "crawl-9"}))
            .await
            .unwrap();
    }

    fn gap_reply(missing: u64, weak: u64) -> Reply {
        ok(json!({
            "crawl": {"run_id": "crawl-9"},
            "summary": {"requirements": 10, "found": 10 - missing - weak, "weak": weak, "missing": missing},
            "notes": ["a note"],
            "method": {"verification": {"requested": false}},
        }))
    }

    #[tokio::test]
    async fn the_gap_check_uses_the_crawl_the_crawl_step_recorded_and_keeps_the_summary() {
        let doc = Uuid::new_v4();
        let f = fixture(
            Step::GapCheck,
            config(json!({"requirement_document_ids": [doc]})),
            |p| ScriptedApi::new().on("POST", &p("gap-check"), vec![gap_reply(0, 0)]),
        )
        .await;
        with_crawl_recorded(&f).await;
        let (detail, _) = detail_of_done(gap_check(&f.ctx).await);
        assert_eq!(detail["crawl_run_id"], "crawl-9");
        assert_eq!(detail["summary"]["requirements"], 10);
        assert_eq!(detail["notes"], json!(["a note"]));
        assert_eq!(detail["method"]["verification"]["requested"], false);
        let sent = f.api.body_of("POST", &f.p("gap-check")).unwrap();
        assert_eq!(sent["crawl_run_id"], "crawl-9");
        assert_eq!(sent["document_ids"], json!([doc]));
        assert!(
            sent.get("verify").is_none(),
            "the AI is not asked unless the run says so: {sent}"
        );
        assert!(
            timeline(&f)
                .await
                .iter()
                .all(|m| !m.contains("show no sign")),
            "nothing to warn about"
        );
    }

    #[tokio::test]
    async fn the_gap_check_asks_the_ai_about_at_most_the_number_the_run_allows() {
        let f = fixture(Step::GapCheck, config(json!({"verify_gaps": 12})), |p| {
            ScriptedApi::new().on("POST", &p("gap-check"), vec![gap_reply(0, 0)])
        })
        .await;
        with_crawl_recorded(&f).await;
        detail_of_done(gap_check(&f.ctx).await);
        let sent = f.api.body_of("POST", &f.p("gap-check")).unwrap();
        assert_eq!(sent["verify"], true);
        assert_eq!(sent["max_verifications"], 12);
    }

    #[tokio::test]
    async fn the_gap_check_notes_requirements_the_crawl_did_not_find_and_fails_when_it_cannot_run()
    {
        let f = fixture(Step::GapCheck, config(json!({})), |p| {
            ScriptedApi::new().on("POST", &p("gap-check"), vec![gap_reply(3, 2)])
        })
        .await;
        with_crawl_recorded(&f).await;
        detail_of_done(gap_check(&f.ctx).await);
        assert!(timeline(&f)
            .await
            .iter()
            .any(|m| m == "3 requirement(s) show no sign in the crawl and 2 only a weak one: cases for them may not be able to run"));

        let f = fixture(Step::GapCheck, config(json!({})), |p| {
            ScriptedApi::new().on(
                "POST",
                &p("gap-check"),
                vec![Reply::error(422, "validation", "the crawl reached no page")],
            )
        })
        .await;
        with_crawl_recorded(&f).await;
        assert_eq!(
            failed(gap_check(&f.ctx).await),
            "the requirements could not be checked against the crawl: the crawl reached no page"
        );

        // No crawl recorded (the crawl step did not finish): nothing is guessed.
        let f = fixture(Step::GapCheck, config(json!({})), |_| ScriptedApi::new()).await;
        assert!(failed(gap_check(&f.ctx).await).contains("the crawl step did not record one"));
        assert!(f.api.seen().is_empty());
    }

    #[tokio::test]
    async fn every_step_of_a_run_has_something_that_runs_it() {
        // The real dispatcher covers every step (a step added to the enum and not to `ApiSteps` would not compile), and
        // it hands each to its own function: the requirements step, given an empty project, says what only it says.
        let f = fixture(Step::Requirements, config(json!({})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p("requirements"),
                vec![ok(requirements_reply(&[], &[]))],
            )
        })
        .await;
        let why = failed(ApiSteps.run(Step::Requirements, &f.ctx).await);
        assert!(why.contains("there are no requirements"), "{why}");
        let f = fixture(Step::GapCheck, config(json!({})), |_| ScriptedApi::new()).await;
        let why = failed(ApiSteps.run(Step::GapCheck, &f.ctx).await);
        assert!(why.contains("no crawl to check against"), "{why}");
        let f = fixture(Step::Crawl, config(json!({})), |p| {
            ScriptedApi::new().on(
                "POST",
                &p("crawl"),
                vec![Reply::error(500, "internal", "no crawler")],
            )
        })
        .await;
        let why = failed(ApiSteps.run(Step::Crawl, &f.ctx).await);
        assert!(why.starts_with("the crawl could not be started"), "{why}");
    }
}
