//! The steps of an autopilot run that make test cases and bring them to the point where a person decides about them:
//! GENERATE (the AI writes cases for the requirements, a group at a time, and each is saved as a test case), VALIDATE (each
//! case is checked for structure) and APPROVE (the gate: the cases wait for a person, or the run approves the validated ones
//! when it was told to).
//!
//! GENERATE keeps what it has done in the step's saved detail after every group, so a step started again after a restart
//! goes on with the group it was in: a group whose cases were written but not yet saved keeps its drafts (no second AI call),
//! and a case saved before the restart is known by its marker tag and not saved twice.

use crate::autopilot::{Step, StepError, StepOutcome};
use crate::autopilot_cases::{
    case_request, chunks, marker, settings_text, CaseInfo, CHUNK_REQUIREMENTS,
};
use crate::autopilot_runner::StepCtx;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

/// A group's places in its saved detail.
const PENDING: &str = "PENDING";
const GENERATED: &str = "GENERATED";
const SAVED: &str = "SAVED";

fn done(detail: Value) -> Result<StepOutcome, StepError> {
    Ok(StepOutcome::Done {
        detail,
        skipped: false,
    })
}

fn suite_of(detail: &Value) -> Option<Uuid> {
    detail["suite_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
}

/// The cases of the run's suite, as the API lists them.
pub(crate) async fn cases_of_suite(ctx: &StepCtx, suite: Uuid) -> Result<Vec<Value>, StepError> {
    let reply = ctx
        .api
        .get(&ctx.path(&format!("test-cases?suite_id={suite}")))
        .await;
    if !reply.is_ok() {
        return Err(StepError::failed(format!(
            "the run's test cases could not be listed: {}",
            reply.problem()
        )));
    }
    Ok(reply.body.as_array().cloned().unwrap_or_default())
}

/// The suite the generate step made, from the saved detail of that step.
pub(crate) async fn suite_of_run(ctx: &StepCtx) -> Result<Uuid, StepError> {
    suite_of(&ctx.detail_of(Step::Generate).await).ok_or_else(|| {
        StepError::failed("there is no suite of test cases: the generate step did not record one")
    })
}

// ------------------------------------------------------------------------------------------------- generate

/// What the requirements are, by id: their text and the document each is from. An id two documents share is left out
/// (it cannot be told which is meant).
struct Requirements {
    document_ids: Vec<String>,
    keys: Vec<String>,
    by_key: HashMap<String, Option<(String, Option<String>)>>,
}

async fn read_requirements(ctx: &StepCtx) -> Result<Requirements, StepError> {
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
    let names: HashMap<&str, &str> = documents
        .iter()
        .filter_map(|d| Some((d["id"].as_str()?, d["file_name"].as_str()?)))
        .collect();
    let mut keys = Vec::new();
    let mut by_key: HashMap<String, Option<(String, Option<String>)>> = HashMap::new();
    for requirement in reply.body["requirements"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        let Some(key) = requirement["key"].as_str() else {
            continue;
        };
        let text = requirement["text"].as_str().unwrap_or("").to_string();
        let document = requirement["document_id"]
            .as_str()
            .and_then(|id| names.get(id))
            .map(|name| name.to_string());
        keys.push(key.to_string());
        match by_key.get(key) {
            None => {
                by_key.insert(key.to_string(), Some((text, document)));
            }
            Some(_) => {
                by_key.insert(key.to_string(), None);
            }
        }
    }
    Ok(Requirements {
        document_ids: documents
            .iter()
            .filter_map(|d| d["id"].as_str().map(str::to_string))
            .collect(),
        keys,
        by_key,
    })
}

fn group_state(group: &Value) -> &str {
    group["state"].as_str().unwrap_or(PENDING)
}

/// The AI writes test cases for the requirements a group at a time and each is saved as a test case of the run's suite.
pub async fn generate(ctx: &StepCtx) -> Result<StepOutcome, StepError> {
    let requirements = read_requirements(ctx).await?;
    let mut detail = ctx.detail.clone();

    // The groups: fixed the first time, so an attempt after a restart goes on with the same ones.
    if !detail["groups"].is_array() {
        let groups: Vec<Value> = chunks(&requirements.keys, CHUNK_REQUIREMENTS)
            .into_iter()
            .map(|keys| json!({"keys": keys, "state": PENDING}))
            .collect();
        if groups.is_empty() {
            return Err(StepError::failed(
                "there are no requirements to generate test cases for",
            ));
        }
        detail["groups"] = Value::Array(groups);
        detail["cases_created"] = json!(0);
        ctx.checkpoint(&detail).await?;
    }
    let suite = match suite_of(&detail) {
        Some(suite) => suite,
        None => {
            let suite = make_suite(ctx).await?;
            detail["suite_id"] = json!(suite.to_string());
            ctx.checkpoint(&detail).await?;
            suite
        }
    };
    let crawl = ctx.detail_of(Step::Crawl).await["crawl_run_id"]
        .as_str()
        .map(str::to_string);
    let document_ids = if ctx.config.requirement_document_ids.is_empty() {
        requirements.document_ids.clone()
    } else {
        ctx.config
            .requirement_document_ids
            .iter()
            .map(Uuid::to_string)
            .collect()
    };
    let total = detail["groups"].as_array().map_or(0, Vec::len);

    for at in 0..total {
        ctx.check_cancel().await?;
        let group = detail["groups"][at].clone();
        if group_state(&group) == SAVED {
            continue;
        }
        let mut group = group;
        if group_state(&group) == PENDING {
            generate_group(ctx, &document_ids, crawl.as_deref(), at, total, &mut group).await?;
            detail["groups"][at] = group.clone();
            ctx.checkpoint(&detail).await?;
        }
        save_group(ctx, &requirements, suite, at, total, &mut group).await?;
        detail["groups"][at] = group;
        let so_far: usize = detail["groups"]
            .as_array()
            .map(|groups| {
                groups
                    .iter()
                    .map(|g| g["cases"].as_array().map_or(0, Vec::len))
                    .sum()
            })
            .unwrap_or(0);
        detail["cases_created"] = json!(so_far);
        ctx.checkpoint(&detail).await?;
    }

    let cases = detail["cases_created"].as_u64().unwrap_or(0);
    if cases == 0 {
        return Err(StepError::failed(
            "no test case was generated: none of the requirements was given a case (check that a skill that tests their kind is selected)",
        ));
    }
    done(summary_of(&detail))
}

/// A suite for the run's cases, named after the run.
async fn make_suite(ctx: &StepCtx) -> Result<Uuid, StepError> {
    let name = ctx
        .store
        .get_autopilot_run(ctx.run_id)
        .await
        .ok()
        .flatten()
        .map(|run| run.name)
        .unwrap_or_else(|| "autopilot run".to_string());
    let reply = ctx
        .api
        .post(
            &ctx.path("test-suites"),
            json!({
                "name": format!("Autopilot: {name}"),
                "description": format!("Test cases written by autopilot run {}", ctx.run_id),
            }),
        )
        .await;
    if !reply.is_ok() {
        return Err(StepError::failed(format!(
            "the suite for the run's test cases could not be made: {}",
            reply.problem()
        )));
    }
    reply.body["id"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| {
            StepError::failed("the suite was made but the service did not say which it is")
        })
}

/// One group's request to the AI. On success the group keeps the drafts (until they are saved), who wrote them and what it
/// took; a group the AI could not do fails the step, with the reason.
async fn generate_group(
    ctx: &StepCtx,
    document_ids: &[String],
    crawl: Option<&str>,
    at: usize,
    total: usize,
    group: &mut Value,
) -> Result<(), StepError> {
    let keys: Vec<String> = group["keys"]
        .as_array()
        .map(|keys| {
            keys.iter()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let mut body = json!({
        "requirement_text": settings_text(&ctx.config),
        "skills": ctx.config.skills,
        "requirement_document_ids": document_ids,
        "requirement_keys": keys,
        "coverage_top_up": ctx.config.coverage_top_up,
    });
    if let Some(depth) = ctx.config.depth.as_deref().filter(|d| !d.trim().is_empty()) {
        body["depth"] = json!(depth);
    }
    if let Some(crawl) = crawl {
        body["crawl_run_id"] = json!(crawl);
    }
    let label = format!("group {}/{total}", at + 1);
    ctx.note(
        &format!("Generating {label}: {} requirement(s)", keys.len()),
        None,
    )
    .await;
    let reply = ctx
        .api
        .post(&ctx.path("test-cases/generate-ai"), body)
        .await;
    if !reply.is_ok() {
        return Err(StepError::failed(format!(
            "generating {label} failed: {}",
            reply.problem()
        )));
    }
    if reply.body["generation_method"] != "ai" {
        return Err(StepError::failed(format!(
            "the AI could not generate {label}: {}",
            reply.body["fallback_reason"]
                .as_str()
                .unwrap_or("no reason was given")
        )));
    }
    let drafts = reply.body["test_cases"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let waves = reply.body["waves"].as_array().cloned().unwrap_or_default();
    let mut who: Vec<String> = Vec::new();
    let mut fallback = false;
    let mut failed_waves = Vec::new();
    for wave in waves.iter().filter(|w| w["skipped"] != true) {
        if let (Some(provider), Some(model)) = (wave["provider"].as_str(), wave["model"].as_str()) {
            let name = format!("{provider}/{model}");
            if !who.contains(&name) {
                who.push(name);
            }
        }
        fallback |= wave["fallback_used"] == true;
        if let Some(reason) = wave["failure_reason"].as_str() {
            failed_waves.push(json!({
                "wave": wave["wave"],
                "skills": wave["skill_names"],
                "reason": reason,
            }));
        }
    }
    let by = if who.is_empty() {
        "an AI provider".to_string()
    } else {
        who.join(", ")
    };
    ctx.note(
        &format!(
            "Generated {label}: {} case(s) via {by}{}",
            drafts.len(),
            if fallback { " (the first choice failed, so the router used another)" } else { "" }
        ),
        Some(json!({"provider": reply.body["provider"], "model": reply.body["model"], "fallback_used": fallback})),
    )
    .await;
    for failed in &failed_waves {
        ctx.note(
            &format!(
                "In {label} a wave of the AI failed and its cases are missing: {}",
                failed["reason"].as_str().unwrap_or("no reason was given")
            ),
            None,
        )
        .await;
    }
    *group = json!({
        "keys": keys,
        "state": GENERATED,
        "drafts": drafts,
        "provider": reply.body["provider"],
        "model": reply.body["model"],
        "providers": who,
        "fallback_used": fallback,
        "failed_waves": failed_waves,
        "prompt_tokens": reply.body["total_prompt_tokens"],
        "completion_tokens": reply.body["total_completion_tokens"],
        "latency_ms": reply.body["total_latency_ms"],
        "uncovered": reply.body["requirements"]["uncovered_keys"],
    });
    Ok(())
}

/// Saves a group's drafts as test cases of the run's suite. A case saved by an attempt that a restart cut short is known by
/// its marker tag and not saved again.
async fn save_group(
    ctx: &StepCtx,
    requirements: &Requirements,
    suite: Uuid,
    at: usize,
    total: usize,
    group: &mut Value,
) -> Result<(), StepError> {
    let existing: HashMap<String, String> = cases_of_suite(ctx, suite)
        .await?
        .into_iter()
        .filter_map(|case| {
            let id = case["id"].as_str()?.to_string();
            let marker = case["tags"]
                .as_array()?
                .iter()
                .filter_map(|t| t.as_str())
                .find(|t| t.starts_with("ap:"))?
                .to_string();
            Some((marker, id))
        })
        .collect();
    let drafts = group["drafts"].as_array().cloned().unwrap_or_default();
    let (provider, model) = (
        group["provider"].as_str().map(str::to_string),
        group["model"].as_str().map(str::to_string),
    );
    let mut saved: Vec<String> = Vec::new();
    let mut skipped: Vec<Value> = Vec::new();
    for (index, draft_json) in drafts.iter().enumerate() {
        let tag = marker(ctx.run_id, at, index);
        if let Some(id) = existing.get(&tag) {
            saved.push(id.clone());
            continue;
        }
        let draft: crate::ai_generation::GeneratedTestCaseDraft = match serde_json::from_value(
            draft_json.clone(),
        ) {
            Ok(draft) => draft,
            Err(e) => {
                skipped.push(json!({"title": draft_json["title"], "reason": format!("the draft could not be read: {e}")}));
                continue;
            }
        };
        let requirement = draft
            .requirement_id
            .as_deref()
            .and_then(|key| requirements.by_key.get(key))
            .and_then(Option::as_ref);
        let skill = ctx
            .config
            .skills
            .iter()
            .find(|s| s.id == draft.skill_id)
            .map(|s| s.name.as_str());
        let request = case_request(
            &draft,
            &CaseInfo {
                run: ctx.run_id,
                marker: tag,
                skill_name: skill,
                requirement_text: requirement.map(|(text, _)| text.as_str()),
                document: requirement.and_then(|(_, document)| document.as_deref()),
                suite: Some(suite),
                provider: provider.as_deref(),
                model: model.as_deref(),
            },
        );
        let reply = ctx.api.post(&ctx.path("test-cases"), request).await;
        match reply.body["id"].as_str() {
            Some(id) if reply.is_ok() => saved.push(id.to_string()),
            _ => skipped.push(json!({"title": draft.title, "reason": reply.problem()})),
        }
    }
    if !skipped.is_empty() {
        ctx.note(
            &format!(
                "{} case(s) of group {}/{total} could not be saved, the first because: {}",
                skipped.len(),
                at + 1,
                skipped[0]["reason"]
                    .as_str()
                    .unwrap_or("no reason was given")
            ),
            None,
        )
        .await;
    }
    let mut kept = group.clone();
    kept["state"] = json!(SAVED);
    kept["cases"] = json!(saved);
    kept["skipped"] = json!(skipped);
    if let Some(object) = kept.as_object_mut() {
        object.remove("drafts");
    }
    *group = kept;
    Ok(())
}

/// What the generate step reports: totals, and for each group who wrote it and what it took (without the drafts).
fn summary_of(detail: &Value) -> Value {
    let groups: Vec<Value> = detail["groups"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|g| {
            json!({
                "keys": g["keys"],
                "cases": g["cases"].as_array().map_or(0, Vec::len),
                "skipped": g["skipped"],
                "providers": g["providers"],
                "fallback_used": g["fallback_used"],
                "failed_waves": g["failed_waves"],
                "prompt_tokens": g["prompt_tokens"],
                "completion_tokens": g["completion_tokens"],
                "latency_ms": g["latency_ms"],
                "uncovered": g["uncovered"],
            })
        })
        .collect();
    let sum = |field: &str| -> u64 { groups.iter().filter_map(|g| g[field].as_u64()).sum() };
    let mut providers: BTreeMap<String, u64> = BTreeMap::new();
    for group in &groups {
        for name in group["providers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            *providers.entry(name.to_string()).or_default() += 1;
        }
    }
    json!({
        "suite_id": detail["suite_id"],
        "cases": detail["cases_created"],
        "groups": groups,
        "prompt_tokens": sum("prompt_tokens"),
        "completion_tokens": sum("completion_tokens"),
        "latency_ms": sum("latency_ms"),
        "providers": providers,
        "fallback_used": groups.iter().any(|g| g["fallback_used"] == true),
    })
}

// ------------------------------------------------------------------------------------------------- validate

/// Each case the run saved is checked for structure by the check the Validator page uses (a title, a description, steps,
/// an expected result). A case that fails is left as it is, with its reasons, for a person to fix.
pub async fn validate(ctx: &StepCtx) -> Result<StepOutcome, StepError> {
    let suite = suite_of_run(ctx).await?;
    let cases = cases_of_suite(ctx, suite).await?;
    if cases.is_empty() {
        return Err(StepError::failed("there are no test cases to validate"));
    }
    let mut validated = 0u64;
    let mut already = 0u64;
    let mut invalid = Vec::new();
    for case in &cases {
        ctx.check_cancel().await?;
        let (Some(id), status) = (case["id"].as_str(), case["status"].as_str().unwrap_or(""))
        else {
            continue;
        };
        if status != "GENERATED" {
            already += 1;
            continue;
        }
        let reply = ctx
            .api
            .post(&ctx.path(&format!("test-cases/{id}/validate")), json!({}))
            .await;
        if reply.is_ok() {
            validated += 1;
        } else {
            invalid.push(json!({"id": id, "title": case["title"], "reason": reply.problem()}));
        }
    }
    if !invalid.is_empty() {
        ctx.note(
            &format!(
                "{} case(s) failed the structural check and stay as they are for a person to fix, the first because: {}",
                invalid.len(),
                invalid[0]["reason"].as_str().unwrap_or("no reason was given")
            ),
            None,
        )
        .await;
    }
    done(json!({
        "suite_id": suite.to_string(),
        "cases": cases.len(),
        "validated": validated,
        "already_validated": already,
        "invalid": invalid,
    }))
}

// ------------------------------------------------------------------------------------------------- approve

/// Cases by where they are: how many of the run's cases are waiting for a decision, approved, rejected or already run.
fn tally(cases: &[Value]) -> (u64, u64, u64, u64) {
    let mut counts = (0, 0, 0, 0);
    for case in cases {
        match case["status"].as_str().unwrap_or("") {
            "GENERATED" | "VALIDATED" => counts.0 += 1,
            "APPROVED" => counts.1 += 1,
            "REJECTED" => counts.2 += 1,
            "EXECUTED" => counts.3 += 1,
            _ => {}
        }
    }
    counts
}

/// The gate. The cases wait for a person to approve or reject them (in Autopilot or in Test Cases) and the run waits with
/// them, until the person lets it continue; cases they left undecided are not run. A run made with `auto_validated`
/// approves the cases that passed validation itself, and does not wait.
pub async fn approve(ctx: &StepCtx) -> Result<StepOutcome, StepError> {
    let suite = suite_of_run(ctx).await?;
    let mut cases = cases_of_suite(ctx, suite).await?;
    let automatic = ctx.config.approval == crate::autopilot::ApprovalMode::AutoValidated;
    let mut approved_by_run = 0u64;
    if automatic {
        for case in cases.iter().filter(|c| c["status"] == "VALIDATED") {
            ctx.check_cancel().await?;
            let Some(id) = case["id"].as_str() else {
                continue;
            };
            let reply = ctx
                .api
                .post(&ctx.path(&format!("test-cases/{id}/approve")), json!({}))
                .await;
            if reply.is_ok() {
                approved_by_run += 1;
            }
        }
        if approved_by_run > 0 {
            ctx.note(
                &format!("Approved {approved_by_run} validated case(s), because the run was made with automatic approval"),
                None,
            )
            .await;
        }
        cases = cases_of_suite(ctx, suite).await?;
    }
    let (pending, approved, rejected, executed) = tally(&cases);
    let released = ctx.detail["released"] == true;
    let detail = json!({
        "suite_id": suite.to_string(),
        "cases": cases.len(),
        "approved": approved,
        "rejected": rejected,
        "executed": executed,
        "pending": pending,
        "automatic": automatic,
        "approved_by_run": approved_by_run,
        "released": released,
    });
    if pending == 0 || released || automatic {
        return done(detail);
    }
    let not_validated = cases.iter().filter(|c| c["status"] == "GENERATED").count();
    Ok(StepOutcome::Wait {
        why: format!(
            "{pending} generated case(s) wait for approval ({approved} approved so far{}): approve or reject them in Autopilot or in Test Cases, then let the run continue",
            if not_validated > 0 {
                format!(", {not_validated} not validated yet")
            } else {
                String::new()
            }
        ),
        detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopilot::ApprovalMode;
    use crate::autopilot_pipeline::scripted::ScriptedApi;
    use crate::autopilot_pipeline::Reply;
    use crate::autopilot_test_support::*;
    use std::sync::Arc;

    const SUITE: &str = "5b1f0b9e-7c5e-4a55-9a3e-0c7f6f0b1a11";

    fn created(id: &str) -> Reply {
        Reply::new(201, json!({"id": id}))
    }

    /// `n` functional requirements FR-001, FR-002, ... of one document, as the requirements route lists them.
    fn requirements_reply(n: usize) -> Reply {
        ok(json!({
            "documents": [{"id": "d1", "file_name": "FRD.md"}],
            "requirements": (1..=n).map(|i| json!({
                "document_id": "d1", "key": format!("FR-{i:03}"), "kind": "functional", "text": format!("The user can do thing {i}."),
            })).collect::<Vec<_>>(),
        }))
    }

    fn draft(key: &str, n: usize) -> Value {
        json!({
            "title": format!("Case {n} for {key}"),
            "category": "Functional Testing",
            "skill_id": "skill-functional",
            "preconditions": [],
            "steps": [{"action": "Do the thing", "expected_result": "The thing happens"}],
            "expected_result": "The thing happens",
            "priority": "HIGH",
            "severity": "MAJOR",
            "evidence": "evidenced",
            "requirement_id": key,
            "technique": "positive",
        })
    }

    fn wave(provider: &str, model: &str, fallback: bool, failure: Option<&str>) -> Value {
        json!({
            "wave": "base", "skill_names": ["Functional Testing"], "case_count": 1, "provider": provider, "model": model,
            "fallback_used": fallback, "failure_reason": failure, "skipped": false,
        })
    }

    fn generated(drafts: Vec<Value>, waves: Vec<Value>) -> Reply {
        ok(json!({
            "test_cases": drafts, "generation_method": "ai", "provider": "ollama", "model": "qwen", "fallback_reason": null,
            "waves": waves, "total_prompt_tokens": 1000, "total_completion_tokens": 500, "total_latency_ms": 40000,
            "requirements": {"uncovered_keys": ["FR-009"]},
        }))
    }

    fn skills() -> Value {
        json!([{"id": "skill-functional", "name": "Functional Testing", "description": ""}])
    }

    async fn save_crawl(f: &Fixture) {
        f.ctx
            .store
            .save_autopilot_step_detail(f.ctx.run_id, "CRAWL", &json!({"crawl_run_id": "crawl-9"}))
            .await
            .unwrap();
    }

    async fn saved_detail(f: &Fixture) -> Value {
        f.ctx.detail_of(Step::Generate).await
    }

    fn run8(f: &Fixture) -> String {
        f.ctx.run_id.to_string()[..8].to_string()
    }

    // ------------------------------------------------------------------------------ generate

    #[tokio::test]
    async fn requirements_are_generated_for_in_groups_and_every_case_is_saved_in_the_runs_suite_with_its_tags(
    ) {
        let f = fixture(
            Step::Generate,
            config(json!({"skills": skills(), "depth": "STANDARD", "coverage_top_up": true})),
            |p| {
                ScriptedApi::new()
                    .on("GET", &p("requirements"), vec![requirements_reply(12)])
                    .on("POST", &p("test-suites"), vec![created(SUITE)])
                    .on(
                        "POST",
                        &p("test-cases/generate-ai"),
                        vec![
                            generated(
                                vec![draft("FR-001", 1), draft("FR-002", 2)],
                                vec![wave("ollama", "qwen", false, None)],
                            ),
                            generated(
                                vec![draft("FR-011", 3)],
                                vec![wave("ollama", "qwen", false, None)],
                            ),
                        ],
                    )
                    .on(
                        "GET",
                        &p(&format!("test-cases?suite_id={SUITE}")),
                        vec![ok(json!([]))],
                    )
                    .on(
                        "POST",
                        &p("test-cases"),
                        vec![created("c1"), created("c2"), created("c3")],
                    )
            },
        )
        .await;
        save_crawl(&f).await;

        let (detail, skipped) = detail_of_done(generate(&f.ctx).await);
        assert!(!skipped);
        assert_eq!(detail["suite_id"], SUITE);
        assert_eq!(detail["cases"], 3);
        assert_eq!(
            detail["groups"].as_array().unwrap().len(),
            2,
            "twelve requirements are two groups: {detail}"
        );
        assert_eq!(detail["groups"][0]["cases"], 2);
        assert_eq!(detail["groups"][1]["cases"], 1);
        assert_eq!(detail["groups"][0]["keys"].as_array().unwrap().len(), 10);
        assert_eq!(detail["groups"][1]["keys"], json!(["FR-011", "FR-012"]));
        assert_eq!(
            detail["prompt_tokens"], 2000,
            "the tokens of every group add up"
        );
        assert_eq!(detail["completion_tokens"], 1000);
        assert_eq!(detail["latency_ms"], 80000);
        assert_eq!(detail["providers"], json!({"ollama/qwen": 2}));
        assert_eq!(detail["fallback_used"], false);
        assert_eq!(detail["groups"][0]["uncovered"], json!(["FR-009"]));
        assert!(
            detail["groups"][0].get("drafts").is_none(),
            "the drafts are not kept once saved"
        );

        // The suite is named after the run.
        let suite = f.api.body_of("POST", &f.p("test-suites")).unwrap();
        assert_eq!(suite["name"], "Autopilot: t");
        assert!(suite["description"]
            .as_str()
            .unwrap()
            .contains(&f.ctx.run_id.to_string()));
        // Each group asks for its own requirements, of the project's documents, as the run says.
        let asked = f.api.bodies_of("POST", &f.p("test-cases/generate-ai"));
        assert_eq!(asked.len(), 2);
        assert_eq!(asked[0]["requirement_keys"].as_array().unwrap().len(), 10);
        assert_eq!(asked[1]["requirement_keys"], json!(["FR-011", "FR-012"]));
        assert_eq!(
            asked[0]["requirement_document_ids"],
            json!(["d1"]),
            "no document chosen means every document read"
        );
        assert_eq!(asked[0]["skills"], skills());
        assert_eq!(asked[0]["depth"], "STANDARD");
        assert_eq!(asked[0]["coverage_top_up"], true);
        assert_eq!(
            asked[0]["crawl_run_id"], "crawl-9",
            "the crawl grounds the cases"
        );
        assert_eq!(
            asked[0]["requirement_text"],
            "Application under test: https://shop.example.test. Roles: anonymous. Requested generation depth: STANDARD."
        );
        // Each case is saved into the suite with the run's tag, its own marker and its requirement's text and document.
        let saved = f.api.bodies_of("POST", &f.p("test-cases"));
        assert_eq!(saved.len(), 3);
        assert_eq!(saved[0]["suite_id"], SUITE);
        assert_eq!(saved[0]["requirement_reference"], "FR-001");
        assert_eq!(saved[0]["source_requirement"], "The user can do thing 1.");
        assert_eq!(saved[0]["source_document"], "FRD.md");
        let tags: Vec<&str> = saved[0]["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        assert!(
            tags.contains(&format!("autopilot:{}", f.ctx.run_id).as_str()),
            "{tags:?}"
        );
        assert!(
            tags.contains(&format!("ap:{}:0:0", run8(&f)).as_str()),
            "{tags:?}"
        );
        assert!(tags.contains(&"technique:positive"), "{tags:?}");
        let second: Vec<&str> = saved[1]["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        assert!(
            second.contains(&format!("ap:{}:0:1", run8(&f)).as_str()),
            "the second case of the first group: {second:?}"
        );
        let third: Vec<&str> = saved[2]["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        assert!(
            third.contains(&format!("ap:{}:1:0", run8(&f)).as_str()),
            "the first case of the second group: {third:?}"
        );
        assert!(saved[0]["description"]
            .as_str()
            .unwrap()
            .contains("skill: Functional Testing"));
        // The timeline says what happened to each group and who wrote it.
        let lines = timeline(&f).await;
        assert!(
            lines
                .iter()
                .any(|l| l == "Generating group 1/2: 10 requirement(s)"),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l == "Generated group 1/2: 2 case(s) via ollama/qwen"),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l == "Generated group 2/2: 1 case(s) via ollama/qwen"),
            "{lines:?}"
        );
    }

    #[tokio::test]
    async fn the_documents_a_run_chose_are_the_ones_generated_from_and_a_short_list_is_one_group() {
        let doc = Uuid::new_v4();
        let f = fixture(
            Step::Generate,
            config(json!({"requirement_document_ids": [doc], "skills": skills()})),
            |p| {
                ScriptedApi::new()
                    .on(
                        "GET",
                        &format!("{}?document_ids={doc}", p("requirements")),
                        vec![requirements_reply(3)],
                    )
                    .on("POST", &p("test-suites"), vec![created(SUITE)])
                    .on(
                        "POST",
                        &p("test-cases/generate-ai"),
                        vec![generated(
                            vec![draft("FR-001", 1)],
                            vec![wave("ollama", "qwen", false, None)],
                        )],
                    )
                    .on(
                        "GET",
                        &p(&format!("test-cases?suite_id={SUITE}")),
                        vec![ok(json!([]))],
                    )
                    .on("POST", &p("test-cases"), vec![created("c1")])
            },
        )
        .await;
        let (detail, _) = detail_of_done(generate(&f.ctx).await);
        assert_eq!(detail["groups"].as_array().unwrap().len(), 1);
        let asked = f.api.bodies_of("POST", &f.p("test-cases/generate-ai"));
        assert_eq!(asked[0]["requirement_document_ids"], json!([doc]));
        assert!(
            asked[0].get("depth").is_none(),
            "no depth unless the run has one: {}",
            asked[0]
        );
        assert!(
            asked[0].get("crawl_run_id").is_none(),
            "no crawl recorded, none sent"
        );
        assert_eq!(asked[0]["coverage_top_up"], false);
    }

    #[tokio::test]
    async fn a_group_the_ai_cannot_write_fails_the_step_and_the_next_attempt_goes_on_from_that_group(
    ) {
        let f = fixture(Step::Generate, config(json!({"skills": skills()})), |p| {
            ScriptedApi::new()
                .on("GET", &p("requirements"), vec![requirements_reply(12)])
                .on("POST", &p("test-suites"), vec![created(SUITE)])
                .on(
                    "POST",
                    &p("test-cases/generate-ai"),
                    vec![
                        generated(vec![draft("FR-001", 1)], vec![wave("ollama", "qwen", false, None)]),
                        ok(json!({"generation_method": "fallback", "fallback_reason": "every provider timed out", "test_cases": []})),
                        generated(vec![draft("FR-011", 2)], vec![wave("ollama", "qwen", false, None)]),
                    ],
                )
                .on("GET", &p(&format!("test-cases?suite_id={SUITE}")), vec![ok(json!([]))])
                .on("POST", &p("test-cases"), vec![created("c1"), created("c2")])
        })
        .await;

        let why = failed(generate(&f.ctx).await);
        assert_eq!(
            why,
            "the AI could not generate group 2/2: every provider timed out"
        );
        let kept = saved_detail(&f).await;
        assert_eq!(
            kept["groups"][0]["state"], "SAVED",
            "the group that was done is kept: {kept}"
        );
        assert_eq!(kept["groups"][1]["state"], "PENDING");
        assert_eq!(kept["cases_created"], 1);
        assert_eq!(kept["suite_id"], SUITE);

        // The step is started again with what it saved: the suite and the first group are not made again.
        let mut again = f;
        again.ctx.detail = kept;
        let (detail, _) = detail_of_done(generate(&again.ctx).await);
        assert_eq!(detail["cases"], 2);
        assert_eq!(
            again.api.bodies_of("POST", &again.p("test-suites")).len(),
            1,
            "one suite"
        );
        let asked = again
            .api
            .bodies_of("POST", &again.p("test-cases/generate-ai"));
        assert_eq!(asked.len(), 3, "the first group once, the second twice");
        assert_eq!(
            asked[2]["requirement_keys"],
            json!(["FR-011", "FR-012"]),
            "the retry is the group that failed"
        );
        assert_eq!(
            again.api.bodies_of("POST", &again.p("test-cases")).len(),
            2,
            "each case saved once"
        );
    }

    #[tokio::test]
    async fn a_failed_request_is_reported_with_the_group_and_the_services_reason() {
        let f = fixture(Step::Generate, config(json!({})), |p| {
            ScriptedApi::new()
                .on("GET", &p("requirements"), vec![requirements_reply(2)])
                .on("POST", &p("test-suites"), vec![created(SUITE)])
                .on(
                    "POST",
                    &p("test-cases/generate-ai"),
                    vec![Reply::error(
                        400,
                        "validation",
                        "this would take 200 AI calls",
                    )],
                )
        })
        .await;
        assert_eq!(
            failed(generate(&f.ctx).await),
            "generating group 1/1 failed: this would take 200 AI calls"
        );
    }

    #[tokio::test]
    async fn a_restart_between_writing_the_cases_and_saving_them_keeps_the_drafts_and_saves_each_case_once(
    ) {
        let mut f = fixture(Step::Generate, config(json!({"skills": skills()})), |_| {
            ScriptedApi::new()
        })
        .await;
        // The first case of the group was saved before the service stopped: the listing has it, under its marker.
        let marker = format!("ap:{}:0:0", run8(&f));
        f.api = Arc::new(
            ScriptedApi::new()
                .on("GET", &f.p("requirements"), vec![requirements_reply(2)])
                .on(
                    "GET",
                    &f.p(&format!("test-cases?suite_id={SUITE}")),
                    vec![ok(json!([{"id": "already", "tags": [marker]}]))],
                )
                .on("POST", &f.p("test-cases"), vec![created("c2")]),
        );
        f.ctx.api = f.api.clone();
        f.ctx.detail = json!({
            "suite_id": SUITE,
            "cases_created": 0,
            "groups": [{"keys": ["FR-001", "FR-002"], "state": "GENERATED", "provider": "ollama", "model": "qwen",
                        "drafts": [draft("FR-001", 1), draft("FR-002", 2)]}],
        });

        let (detail, _) = detail_of_done(generate(&f.ctx).await);
        assert_eq!(detail["cases"], 2, "both cases are the run's: {detail}");
        let seen = f.api.seen();
        assert!(
            !seen.iter().any(|s| s.contains("generate-ai")),
            "the AI is not asked again: {seen:?}"
        );
        let saved = f.api.bodies_of("POST", &f.p("test-cases"));
        assert_eq!(saved.len(), 1, "only the case that was not saved");
        assert_eq!(saved[0]["requirement_reference"], "FR-002");
        let kept = saved_detail(&f).await;
        assert_eq!(kept["groups"][0]["cases"], json!(["already", "c2"]));
    }

    #[tokio::test]
    async fn a_draft_the_service_refuses_is_skipped_with_its_reason_and_the_rest_are_saved() {
        let f = fixture(Step::Generate, config(json!({"skills": skills()})), |p| {
            ScriptedApi::new()
                .on("GET", &p("requirements"), vec![requirements_reply(2)])
                .on("POST", &p("test-suites"), vec![created(SUITE)])
                .on(
                    "POST",
                    &p("test-cases/generate-ai"),
                    vec![generated(
                        vec![
                            draft("FR-001", 1),
                            draft("FR-002", 2),
                            json!({"title": "not a draft"}),
                        ],
                        vec![wave("ollama", "qwen", false, None)],
                    )],
                )
                .on(
                    "GET",
                    &p(&format!("test-cases?suite_id={SUITE}")),
                    vec![ok(json!([]))],
                )
                .on(
                    "POST",
                    &p("test-cases"),
                    vec![
                        Reply::error(400, "validation", "title must not be empty"),
                        created("c2"),
                    ],
                )
        })
        .await;
        let (detail, _) = detail_of_done(generate(&f.ctx).await);
        assert_eq!(detail["cases"], 1, "{detail}");
        let skipped = detail["groups"][0]["skipped"].as_array().unwrap();
        assert_eq!(skipped.len(), 2, "{skipped:?}");
        assert_eq!(skipped[0]["reason"], "title must not be empty");
        assert!(
            skipped[1]["reason"]
                .as_str()
                .unwrap()
                .starts_with("the draft could not be read"),
            "{skipped:?}"
        );
        let lines = timeline(&f).await;
        assert!(
            lines.iter().any(|l| l == "2 case(s) of group 1/1 could not be saved, the first because: title must not be empty"),
            "{lines:?}"
        );
    }

    #[tokio::test]
    async fn a_run_that_wrote_no_case_at_all_fails_and_says_what_to_check() {
        let f = fixture(Step::Generate, config(json!({"skills": skills()})), |p| {
            ScriptedApi::new()
                .on("GET", &p("requirements"), vec![requirements_reply(2)])
                .on("POST", &p("test-suites"), vec![created(SUITE)])
                .on(
                    "POST",
                    &p("test-cases/generate-ai"),
                    vec![generated(vec![], vec![wave("ollama", "qwen", false, None)])],
                )
                .on(
                    "GET",
                    &p(&format!("test-cases?suite_id={SUITE}")),
                    vec![ok(json!([]))],
                )
        })
        .await;
        let why = failed(generate(&f.ctx).await);
        assert!(why.starts_with("no test case was generated"), "{why}");
        assert!(why.contains("skill that tests their kind"), "{why}");
    }

    #[tokio::test]
    async fn a_router_that_used_another_provider_or_lost_a_wave_is_said_so_in_the_timeline_and_the_summary(
    ) {
        let f = fixture(Step::Generate, config(json!({"skills": skills()})), |p| {
            ScriptedApi::new()
                .on("GET", &p("requirements"), vec![requirements_reply(2)])
                .on("POST", &p("test-suites"), vec![created(SUITE)])
                .on(
                    "POST",
                    &p("test-cases/generate-ai"),
                    vec![generated(
                        vec![draft("FR-001", 1)],
                        vec![
                            wave("groq", "llama", true, None),
                            wave("ollama", "qwen", false, Some("timed out after 300 s")),
                            json!({"wave": "dependent", "skipped": true, "provider": null, "model": null}),
                        ],
                    )],
                )
                .on("GET", &p(&format!("test-cases?suite_id={SUITE}")), vec![ok(json!([]))])
                .on("POST", &p("test-cases"), vec![created("c1")])
        })
        .await;
        let (detail, _) = detail_of_done(generate(&f.ctx).await);
        assert_eq!(detail["fallback_used"], true);
        assert_eq!(
            detail["providers"],
            json!({"groq/llama": 1, "ollama/qwen": 1}),
            "a wave that was skipped names no one"
        );
        assert_eq!(detail["groups"][0]["fallback_used"], true);
        assert_eq!(
            detail["groups"][0]["failed_waves"][0]["reason"],
            "timed out after 300 s"
        );
        let lines = timeline(&f).await;
        assert!(
            lines.iter().any(|l| l == "Generated group 1/1: 1 case(s) via groq/llama, ollama/qwen (the first choice failed, so the router used another)"),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|l| l == "In group 1/1 a wave of the AI failed and its cases are missing: timed out after 300 s"),
            "{lines:?}"
        );
    }

    #[tokio::test]
    async fn generation_stops_between_groups_when_the_run_is_cancelled_and_needs_requirements_and_a_suite(
    ) {
        let f = fixture(Step::Generate, config(json!({"skills": skills()})), |p| {
            ScriptedApi::new()
                .on("GET", &p("requirements"), vec![requirements_reply(2)])
                .on("POST", &p("test-suites"), vec![created(SUITE)])
        })
        .await;
        f.ctx
            .store
            .cancel_autopilot_run(f.ctx.run_id)
            .await
            .unwrap();
        assert_eq!(generate(&f.ctx).await, Err(StepError::Cancelled));
        assert!(
            !f.api.seen().iter().any(|s| s.contains("generate-ai")),
            "{:?}",
            f.api.seen()
        );

        let f = fixture(Step::Generate, config(json!({})), |p| {
            ScriptedApi::new().on("GET", &p("requirements"), vec![requirements_reply(0)])
        })
        .await;
        assert_eq!(
            failed(generate(&f.ctx).await),
            "there are no requirements to generate test cases for"
        );

        let f = fixture(Step::Generate, config(json!({})), |p| {
            ScriptedApi::new()
                .on("GET", &p("requirements"), vec![requirements_reply(1)])
                .on(
                    "POST",
                    &p("test-suites"),
                    vec![Reply::error(403, "forbidden", "not allowed")],
                )
        })
        .await;
        assert_eq!(
            failed(generate(&f.ctx).await),
            "the suite for the run's test cases could not be made: not allowed"
        );
        let f = fixture(Step::Generate, config(json!({})), |p| {
            ScriptedApi::new()
                .on("GET", &p("requirements"), vec![requirements_reply(1)])
                .on("POST", &p("test-suites"), vec![Reply::new(201, json!({}))])
        })
        .await;
        assert!(failed(generate(&f.ctx).await).contains("did not say which it is"));
        let f = fixture(Step::Generate, config(json!({})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p("requirements"),
                vec![Reply::error(500, "internal", "database locked")],
            )
        })
        .await;
        assert_eq!(
            failed(generate(&f.ctx).await),
            "the requirements could not be read: database locked"
        );
    }

    // ------------------------------------------------------------------------------ validate

    fn case(id: &str, status: &str) -> Value {
        json!({"id": id, "title": format!("Case {id}"), "status": status, "tags": []})
    }

    async fn with_suite(f: &Fixture) {
        f.ctx
            .store
            .save_autopilot_step_detail(
                f.ctx.run_id,
                "GENERATE",
                &json!({"suite_id": SUITE, "cases": 3}),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn each_generated_case_is_checked_and_one_that_fails_is_left_for_a_person() {
        let f = fixture(Step::Validate, config(json!({})), |p| {
            ScriptedApi::new()
                .on(
                    "GET",
                    &p(&format!("test-cases?suite_id={SUITE}")),
                    vec![ok(json!([
                        case("a", "GENERATED"),
                        case("b", "GENERATED"),
                        case("c", "VALIDATED"),
                        case("d", "REJECTED")
                    ]))],
                )
                .on("POST", &p("test-cases/a/validate"), vec![ok(json!({}))])
                .on(
                    "POST",
                    &p("test-cases/b/validate"),
                    vec![Reply::error(
                        409,
                        "conflict",
                        "structural validation failed: no steps defined",
                    )],
                )
        })
        .await;
        with_suite(&f).await;

        let (detail, _) = detail_of_done(validate(&f.ctx).await);
        assert_eq!(detail["cases"], 4);
        assert_eq!(detail["validated"], 1);
        assert_eq!(
            detail["already_validated"], 2,
            "one validated and one rejected are not checked again"
        );
        assert_eq!(detail["invalid"].as_array().unwrap().len(), 1);
        assert_eq!(detail["invalid"][0]["id"], "b");
        assert_eq!(detail["invalid"][0]["title"], "Case b");
        assert_eq!(
            detail["invalid"][0]["reason"],
            "structural validation failed: no steps defined"
        );
        let seen = f.api.seen();
        assert!(
            !seen
                .iter()
                .any(|s| s.ends_with("/c/validate") || s.ends_with("/d/validate")),
            "{seen:?}"
        );
        let lines = timeline(&f).await;
        assert!(
            lines.iter().any(|l| l == "1 case(s) failed the structural check and stay as they are for a person to fix, the first because: structural validation failed: no steps defined"),
            "{lines:?}"
        );
    }

    #[tokio::test]
    async fn validation_needs_a_suite_with_cases_in_it_and_stops_when_cancelled() {
        let f = fixture(Step::Validate, config(json!({})), |_| ScriptedApi::new()).await;
        assert!(failed(validate(&f.ctx).await).contains("the generate step did not record one"));

        let f = fixture(Step::Validate, config(json!({})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p(&format!("test-cases?suite_id={SUITE}")),
                vec![ok(json!([]))],
            )
        })
        .await;
        with_suite(&f).await;
        assert_eq!(
            failed(validate(&f.ctx).await),
            "there are no test cases to validate"
        );

        let f = fixture(Step::Validate, config(json!({})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p(&format!("test-cases?suite_id={SUITE}")),
                vec![Reply::error(500, "internal", "boom")],
            )
        })
        .await;
        with_suite(&f).await;
        assert_eq!(
            failed(validate(&f.ctx).await),
            "the run's test cases could not be listed: boom"
        );

        let f = fixture(Step::Validate, config(json!({})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p(&format!("test-cases?suite_id={SUITE}")),
                vec![ok(json!([case("a", "GENERATED")]))],
            )
        })
        .await;
        with_suite(&f).await;
        f.ctx
            .store
            .cancel_autopilot_run(f.ctx.run_id)
            .await
            .unwrap();
        assert_eq!(validate(&f.ctx).await, Err(StepError::Cancelled));
        assert!(
            !f.api.seen().iter().any(|s| s.contains("/validate")),
            "{:?}",
            f.api.seen()
        );
    }

    // ------------------------------------------------------------------------------ approve

    #[test]
    fn the_cases_are_counted_by_where_they_are_in_their_lifecycle() {
        let cases = vec![
            case("1", "GENERATED"),
            case("2", "VALIDATED"),
            case("3", "VALIDATED"),
            case("4", "APPROVED"),
            case("5", "REJECTED"),
            case("6", "EXECUTED"),
            case("7", "SOMETHING_NEW"),
        ];
        assert_eq!(
            tally(&cases),
            (3, 1, 1, 1),
            "waiting, approved, rejected, executed"
        );
        assert_eq!(tally(&[]), (0, 0, 0, 0));
    }

    #[tokio::test]
    async fn the_gate_waits_for_a_person_while_cases_are_undecided_and_says_how_many() {
        let f = fixture(Step::Approve, config(json!({})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p(&format!("test-cases?suite_id={SUITE}")),
                vec![ok(json!([
                    case("a", "VALIDATED"),
                    case("b", "VALIDATED"),
                    case("c", "GENERATED"),
                    case("d", "APPROVED"),
                    case("e", "REJECTED")
                ]))],
            )
        })
        .await;
        with_suite(&f).await;
        match approve(&f.ctx).await {
            Ok(StepOutcome::Wait { detail, why }) => {
                assert_eq!(
                    why,
                    "3 generated case(s) wait for approval (1 approved so far, 1 not validated yet): approve or reject them in Autopilot or in Test Cases, then let the run continue"
                );
                assert_eq!(detail["pending"], 3);
                assert_eq!(detail["approved"], 1);
                assert_eq!(detail["rejected"], 1);
                assert_eq!(detail["cases"], 5);
                assert_eq!(detail["released"], false);
                assert_eq!(detail["automatic"], false);
            }
            other => panic!("expected the gate to wait, got {other:?}"),
        }
        assert!(
            f.api.seen().iter().all(|s| s.starts_with("GET")),
            "nothing is approved on a person's behalf: {:?}",
            f.api.seen()
        );
    }

    #[tokio::test]
    async fn the_gate_without_anything_to_say_about_validation_does_not_mention_it() {
        let f = fixture(Step::Approve, config(json!({})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p(&format!("test-cases?suite_id={SUITE}")),
                vec![ok(json!([case("a", "VALIDATED")]))],
            )
        })
        .await;
        with_suite(&f).await;
        match approve(&f.ctx).await {
            Ok(StepOutcome::Wait { why, .. }) => {
                assert_eq!(
                    why,
                    "1 generated case(s) wait for approval (0 approved so far): approve or reject them in Autopilot or in Test Cases, then let the run continue"
                );
            }
            other => panic!("expected the gate to wait, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn the_gate_lets_the_run_through_when_every_case_is_decided_or_the_person_said_go() {
        let decided = fixture(Step::Approve, config(json!({})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p(&format!("test-cases?suite_id={SUITE}")),
                vec![ok(json!([case("a", "APPROVED"), case("b", "REJECTED")]))],
            )
        })
        .await;
        with_suite(&decided).await;
        let (detail, _) = detail_of_done(approve(&decided.ctx).await);
        assert_eq!(
            (
                detail["approved"].as_u64(),
                detail["rejected"].as_u64(),
                detail["pending"].as_u64()
            ),
            (Some(1), Some(1), Some(0))
        );

        // Cases still undecided when the person lets the run continue are not run.
        let mut released = fixture(Step::Approve, config(json!({})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p(&format!("test-cases?suite_id={SUITE}")),
                vec![ok(json!([case("a", "APPROVED"), case("b", "VALIDATED")]))],
            )
        })
        .await;
        with_suite(&released).await;
        released.ctx.detail = json!({"released": true});
        let (detail, _) = detail_of_done(approve(&released.ctx).await);
        assert_eq!(detail["released"], true);
        assert_eq!(detail["approved"], 1);
        assert_eq!(detail["pending"], 1, "left undecided: {detail}");
    }

    #[tokio::test]
    async fn a_run_told_to_approve_the_validated_cases_does_and_never_waits() {
        let mut f = fixture(
            Step::Approve,
            config(json!({"approval": "auto_validated"})),
            |p| {
                ScriptedApi::new()
                    .on(
                        "GET",
                        &p(&format!("test-cases?suite_id={SUITE}")),
                        vec![
                            ok(json!([
                                case("a", "VALIDATED"),
                                case("b", "VALIDATED"),
                                case("c", "GENERATED"),
                                case("d", "REJECTED")
                            ])),
                            ok(json!([
                                case("a", "APPROVED"),
                                case("b", "APPROVED"),
                                case("c", "GENERATED"),
                                case("d", "REJECTED")
                            ])),
                        ],
                    )
                    .on("POST", &p("test-cases/a/approve"), vec![ok(json!({}))])
                    .on(
                        "POST",
                        &p("test-cases/b/approve"),
                        vec![Reply::error(
                            409,
                            "conflict",
                            "test case must be VALIDATED before approval",
                        )],
                    )
            },
        )
        .await;
        with_suite(&f).await;
        assert_eq!(f.ctx.config.approval, ApprovalMode::AutoValidated);
        f.ctx.detail = json!({});
        let (detail, _) = detail_of_done(approve(&f.ctx).await);
        assert_eq!(detail["automatic"], true);
        assert_eq!(detail["approved_by_run"], 1, "b was refused: {detail}");
        assert_eq!(
            detail["approved"], 2,
            "read again after the approvals: {detail}"
        );
        assert_eq!(
            detail["pending"], 1,
            "the case that never validated is not run"
        );
        let seen = f.api.seen();
        assert!(
            !seen
                .iter()
                .any(|s| s.ends_with("/c/approve") || s.ends_with("/d/approve")),
            "only validated cases: {seen:?}"
        );
        let lines = timeline(&f).await;
        assert!(
            lines.iter().any(|l| l == "Approved 1 validated case(s), because the run was made with automatic approval"),
            "{lines:?}"
        );
    }

    #[tokio::test]
    async fn the_gate_needs_the_suite_and_can_be_cancelled_while_it_approves() {
        let f = fixture(Step::Approve, config(json!({})), |_| ScriptedApi::new()).await;
        assert!(failed(approve(&f.ctx).await).contains("the generate step did not record one"));

        let f = fixture(
            Step::Approve,
            config(json!({"approval": "auto_validated"})),
            |p| {
                ScriptedApi::new().on(
                    "GET",
                    &p(&format!("test-cases?suite_id={SUITE}")),
                    vec![ok(json!([case("a", "VALIDATED")]))],
                )
            },
        )
        .await;
        with_suite(&f).await;
        f.ctx
            .store
            .cancel_autopilot_run(f.ctx.run_id)
            .await
            .unwrap();
        assert_eq!(approve(&f.ctx).await, Err(StepError::Cancelled));
        assert!(
            !f.api.seen().iter().any(|s| s.contains("/approve")),
            "{:?}",
            f.api.seen()
        );
    }

    #[tokio::test]
    async fn the_steps_are_the_ones_the_dispatcher_runs() {
        use crate::autopilot_runner::StepRunner;
        let f = fixture(Step::Validate, config(json!({})), |_| ScriptedApi::new()).await;
        assert!(failed(
            crate::autopilot_steps::ApiSteps
                .run(Step::Validate, &f.ctx)
                .await
        )
        .contains("the generate step did not record one"));
        assert!(failed(
            crate::autopilot_steps::ApiSteps
                .run(Step::Approve, &f.ctx)
                .await
        )
        .contains("the generate step did not record one"));
        let f = fixture(Step::Generate, config(json!({})), |p| {
            ScriptedApi::new().on("GET", &p("requirements"), vec![requirements_reply(0)])
        })
        .await;
        assert!(failed(
            crate::autopilot_steps::ApiSteps
                .run(Step::Generate, &f.ctx)
                .await
        )
        .contains("no requirements to generate"));
    }
}
