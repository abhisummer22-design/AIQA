//! The report of an autopilot run: what it found, in one place, worked out from what its steps recorded and from what the
//! project's requirements and the run's cases say. Nothing here is a second source of truth: the steps' details and the cases
//! are, and the report only reads them, so it can be built again at any time, and the same inputs give the same report.
//!
//! `build` and `markdown` are pure; `report` (the REPORT step) gathers the two lists it needs through the API and keeps the
//! finished report with the step.

use crate::autopilot::{Step, StepError, StepOutcome};
use crate::autopilot_pipeline::PipelineApi;
use crate::autopilot_runner::StepCtx;
use crate::autopilot_store::{AutopilotRunRow, AutopilotStepRow};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Outcomes that mean no run of the case gave an answer: it was refused before it could run.
const NOT_RUN: [&str; 4] = [
    "NOT_EXECUTABLE",
    "COMPILATION_FAILED",
    "COMPILATION_UNAVAILABLE",
    "NOT_RUN",
];

/// What a report is built from.
pub struct ReportInput<'a> {
    pub run: &'a AutopilotRunRow,
    pub steps: &'a [AutopilotStepRow],
    /// The requirements of the run's documents, as `GET .../requirements` lists them.
    pub requirements: &'a [Value],
    /// The cases of the run's suite, as `GET .../test-cases?suite_id=` lists them.
    pub cases: &'a [Value],
    /// Whether the run has ended and this is what it found, or a look at a run still going.
    pub finished: bool,
}

fn step<'a>(steps: &'a [AutopilotStepRow], which: Step) -> Option<&'a AutopilotStepRow> {
    steps.iter().find(|s| s.name == which.as_str())
}

fn detail<'a>(steps: &'a [AutopilotStepRow], which: Step) -> &'a Value {
    static NULL: Value = Value::Null;
    step(steps, which).map_or(&NULL, |s| &s.detail)
}

fn seconds(
    from: Option<chrono::DateTime<chrono::Utc>>,
    to: Option<chrono::DateTime<chrono::Utc>>,
) -> Option<i64> {
    Some((to? - from?).num_seconds().max(0))
}

/// How a requirement came out, from how its cases did: `FAILED` when any case failed, `PASSED` when any passed, `INCONCLUSIVE`
/// when cases ran and none passed or failed, `NOT_EXECUTED` when it has cases and none ran, `NO_CASES` when it has none.
pub fn verdict(cases: usize, passed: usize, failed: usize, ran: usize) -> &'static str {
    if cases == 0 {
        "NO_CASES"
    } else if failed > 0 {
        "FAILED"
    } else if passed > 0 {
        "PASSED"
    } else if ran > 0 {
        "INCONCLUSIVE"
    } else {
        "NOT_EXECUTED"
    }
}

/// The report, as JSON.
pub fn build(input: &ReportInput) -> Value {
    let run = input.run;
    let config = &run.config;
    let requirements_detail = detail(input.steps, Step::Requirements);
    let crawl = detail(input.steps, Step::Crawl);
    let gaps = detail(input.steps, Step::GapCheck);
    let generation = detail(input.steps, Step::Generate);
    let validation = detail(input.steps, Step::Validate);
    let approval = detail(input.steps, Step::Approve);
    let execution = detail(input.steps, Step::Execute);
    let executed = step(input.steps, Step::Execute).is_some_and(|s| s.state == "DONE");
    let execution_on = config["execute"].as_bool().unwrap_or(true);

    // How each case came out, by case id, from the execute step.
    let mut outcomes: BTreeMap<&str, &Value> = BTreeMap::new();
    for case in execution["cases"].as_array().into_iter().flatten() {
        if let Some(id) = case["id"].as_str() {
            outcomes.insert(id, case);
        }
    }
    let outcome_of = |case: &Value| -> Option<&str> {
        outcomes
            .get(case["id"].as_str()?)
            .and_then(|c| c["outcome"].as_str())
    };

    // Requirement by requirement.
    let mut coverage = Vec::new();
    let mut by_verdict: BTreeMap<&str, u64> = BTreeMap::new();
    let mut seen_keys = std::collections::HashSet::new();
    for requirement in input.requirements {
        let Some(key) = requirement["key"].as_str() else {
            continue;
        };
        if !seen_keys.insert(key) {
            continue;
        }
        // A case a person rejected at the gate tests nothing: it is not a case of the requirement here.
        let mine: Vec<&Value> = input
            .cases
            .iter()
            .filter(|c| c["requirement_reference"].as_str() == Some(key) && c["status"] != "REJECTED")
            .collect();
        let count = |wanted: &str| {
            mine.iter()
                .filter(|c| outcome_of(c) == Some(wanted))
                .count()
        };
        let passed = count("PASSED");
        let failed = count("FAILED");
        let ran = mine
            .iter()
            .filter(|c| outcome_of(c).is_some_and(|o| !NOT_RUN.contains(&o)))
            .count();
        let verdict = verdict(mine.len(), passed, failed, ran);
        *by_verdict.entry(verdict).or_default() += 1;
        coverage.push(json!({
            "key": key,
            "kind": requirement["kind"],
            "text": requirement["text"],
            "cases": mine.len(),
            "ran": ran,
            "passed": passed,
            "failed": failed,
            "not_run": mine.len() - ran,
            "verdict": verdict,
        }));
    }

    // Cases that failed are what the run found; cases that could not be run are what stands in the way of finding more.
    let mut findings = Vec::new();
    let mut not_run = Vec::new();
    for case in execution["cases"].as_array().into_iter().flatten() {
        let outcome = case["outcome"].as_str().unwrap_or("");
        let row = json!({
            "case_id": case["id"], "title": case["title"], "requirement": case["requirement"],
            "outcome": outcome, "reason": case["reason"], "run_id": case["run_id"], "attempts": case["attempts"],
        });
        if outcome == "FAILED" {
            findings.push(row);
        } else if outcome != "PASSED" && !outcome.is_empty() {
            not_run.push(row);
        }
    }

    let counts = &execution["counts"];
    let count_of = |name: &str| counts[name].as_u64().unwrap_or(0);
    let passed = count_of("PASSED");
    let failed = count_of("FAILED");
    let finished_cases = execution["finished"].as_u64().unwrap_or(0);
    // A case the compiler refused reached an outcome, but nothing was run for it.
    let refused_cases: u64 = NOT_RUN.iter().map(|outcome| count_of(outcome)).sum();
    let ran_cases = finished_cases.saturating_sub(refused_cases);

    let started = run.started_at;
    // The end of a run that has not ended yet is now (a report is written before its run completes).
    let now = chrono::Utc::now();
    let ended = Some(run.finished_at.unwrap_or(now));
    // The report is the last step of its run: the run completes right after it.
    let state = if input.finished && run.state == "RUNNING" {
        "COMPLETED"
    } else {
        run.state.as_str()
    };

    // What a person should read before trusting the numbers.
    let mut notes: Vec<String> = Vec::new();
    if let Some(error) = &run.error {
        notes.push(format!("The run failed: {error}"));
    }
    if generation["fallback_used"] == true {
        let names: Vec<&str> = generation["providers"]
            .as_object()
            .map(|p| p.keys().map(String::as_str).collect())
            .unwrap_or_default();
        notes.push(format!(
            "The AI router used a provider other than its first choice for some of the generation ({}).",
            names.join(", ")
        ));
    }
    let failed_waves: usize = generation["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|g| g["failed_waves"].as_array().map_or(0, Vec::len))
        .sum();
    if failed_waves > 0 {
        notes.push(format!(
            "{failed_waves} wave(s) of the AI failed during generation, so some cases that a skill would have written are missing."
        ));
    }
    if let Some(invalid) = validation["invalid"].as_array().filter(|i| !i.is_empty()) {
        notes.push(format!(
            "{} case(s) failed the structural check and were left for a person to fix.",
            invalid.len()
        ));
    }
    let undecided = approval["pending"].as_u64().unwrap_or(0);
    if undecided > 0 {
        notes.push(format!("{undecided} generated case(s) were left undecided at the approval gate and were not run."));
    }
    let rejected = approval["rejected"].as_u64().unwrap_or(0);
    if rejected > 0 {
        notes.push(format!("{rejected} case(s) were rejected at the approval gate and are not counted."));
    }
    if !execution_on {
        notes.push(
            "The run was made with execution off: the cases were written and approved but not run."
                .to_string(),
        );
    }
    let no_case = by_verdict.get("NO_CASES").copied().unwrap_or(0);
    if no_case > 0 {
        notes.push(format!("{no_case} requirement(s) have no case."));
    }
    let missing = gaps["summary"]["missing"].as_u64().unwrap_or(0);
    if missing > 0 {
        notes.push(format!("{missing} requirement(s) showed no sign in the crawl, so their cases may not be able to run."));
    }
    if !not_run.is_empty() {
        notes.push(format!(
            "{} case(s) could not give an answer (see not_run).",
            not_run.len()
        ));
    }
    if !input.finished {
        notes.push("The run has not ended: this is what it has found so far.".to_string());
    }

    let step_rows: Vec<Value> = input
        .steps
        .iter()
        .map(|s| {
            json!({
                "name": s.name, "state": s.state, "attempts": s.attempts,
                "seconds": seconds(s.started_at, s.finished_at),
            })
        })
        .collect();

    json!({
        "final": input.finished,
        "run": {
            "id": run.id, "name": run.name, "state": state, "error": run.error,
            "created_by": run.created_by_name, "created_at": run.created_at,
            "started_at": run.started_at, "finished_at": run.finished_at.or(input.finished.then_some(now)),
            "seconds": seconds(started, ended),
        },
        "application": {
            "base_url": config["base_url"],
            "roles": config["roles"].as_array().map(|roles| roles.iter().filter_map(|r| r["name"].as_str()).collect::<Vec<_>>()),
            "execution_role": execution["role"],
        },
        "requirements": {
            "total": requirements_detail["requirements"].as_u64().unwrap_or(input.requirements.len() as u64),
            "by_kind": requirements_detail["by_kind"],
        },
        "crawl": {"crawl_run_id": crawl["crawl_run_id"], "reused": crawl["reused"], "pages": crawl["pages"], "roles": crawl["roles"]},
        "gaps": gaps["summary"],
        "generation": {
            "cases": generation["cases"],
            "groups": generation["groups"].as_array().map_or(0, Vec::len),
            "providers": generation["providers"],
            "fallback_used": generation["fallback_used"],
            "prompt_tokens": generation["prompt_tokens"],
            "completion_tokens": generation["completion_tokens"],
            "latency_ms": generation["latency_ms"],
        },
        "validation": {"validated": validation["validated"], "invalid": validation["invalid"]},
        "approval": {
            "approved": approval["approved"], "rejected": approval["rejected"], "undecided": approval["pending"],
            "automatic": approval["automatic"], "released": approval["released"],
        },
        "execution": {
            "enabled": execution_on,
            "done": executed,
            "cases": execution["total"],
            "finished": finished_cases,
            "ran": ran_cases,
            "counts": counts,
            "passed": passed,
            "failed": failed,
            "role": execution["role"],
        },
        "findings": findings,
        "not_run": not_run,
        "coverage": coverage,
        "coverage_summary": by_verdict,
        "cost": {
            "estimated": run.estimate["totals"],
            "actual": {
                "ai_prompt_tokens": generation["prompt_tokens"],
                "ai_completion_tokens": generation["completion_tokens"],
                "ai_seconds": generation["latency_ms"].as_u64().map(|ms| ms / 1000),
                "run_seconds": seconds(started, ended),
            },
        },
        "steps": step_rows,
        "notes": notes,
    })
}

fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// A `|` in a table cell would end it.
fn cell(value: &Value) -> String {
    text(value).replace('|', "\\|").replace('\n', " ")
}

/// The report as Markdown, for pasting where people read.
pub fn markdown(report: &Value) -> String {
    let mut out = String::new();
    let run = &report["run"];
    out.push_str(&format!("# Autopilot report: {}\n\n", text(&run["name"])));
    out.push_str(&format!(
        "- **Application:** {} (roles: {})\n- **Run:** {} — {}{}\n",
        text(&report["application"]["base_url"]),
        report["application"]["roles"]
            .as_array()
            .map(|r| r.iter().map(text).collect::<Vec<_>>().join(", "))
            .unwrap_or_default(),
        text(&run["state"]),
        text(&run["created_by"]),
        run["seconds"]
            .as_i64()
            .map(|s| format!(", {s} s"))
            .unwrap_or_default(),
    ));
    if report["final"] == false {
        out.push_str("- **Not final:** the run has not ended.\n");
    }
    out.push('\n');

    let execution = &report["execution"];
    out.push_str("## Results\n\n");
    if execution["enabled"] == false {
        out.push_str("Execution was off: the cases were written but not run.\n\n");
    } else {
        out.push_str(&format!(
            "{} case(s) run: {} passed, {} failed.\n\n",
            text(&execution["ran"]),
            text(&execution["passed"]),
            text(&execution["failed"]),
        ));
    }
    let coverage = &report["coverage_summary"];
    if let Some(summary) = coverage.as_object().filter(|s| !s.is_empty()) {
        let parts: Vec<String> = summary.iter().map(|(k, v)| format!("{v} {k}")).collect();
        out.push_str(&format!("Requirements: {}.\n\n", parts.join(", ")));
    }

    if let Some(findings) = report["findings"].as_array().filter(|f| !f.is_empty()) {
        out.push_str("## Failures\n\n| Case | Requirement | Why | Run |\n|---|---|---|---|\n");
        for f in findings {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                cell(&f["title"]),
                cell(&f["requirement"]),
                cell(&f["reason"]),
                cell(&f["run_id"])
            ));
        }
        out.push('\n');
    }
    if let Some(rows) = report["not_run"].as_array().filter(|f| !f.is_empty()) {
        out.push_str("## Cases that gave no answer\n\n| Case | Requirement | Outcome | Why |\n|---|---|---|---|\n");
        for f in rows {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                cell(&f["title"]),
                cell(&f["requirement"]),
                cell(&f["outcome"]),
                cell(&f["reason"])
            ));
        }
        out.push('\n');
    }
    if let Some(rows) = report["coverage"].as_array().filter(|c| !c.is_empty()) {
        out.push_str("## Requirements\n\n| Requirement | Verdict | Cases | Passed | Failed |\n|---|---|---|---|---|\n");
        for r in rows {
            out.push_str(&format!(
                "| {} — {} | {} | {} | {} | {} |\n",
                cell(&r["key"]),
                cell(&r["text"]).chars().take(80).collect::<String>(),
                cell(&r["verdict"]),
                cell(&r["cases"]),
                cell(&r["passed"]),
                cell(&r["failed"]),
            ));
        }
        out.push('\n');
    }
    let generation = &report["generation"];
    out.push_str("## How it was made\n\n");
    out.push_str(&format!(
        "- Generation: {} case(s) in {} group(s), {} prompt / {} completion tokens\n",
        text(&generation["cases"]),
        text(&generation["groups"]),
        text(&generation["prompt_tokens"]),
        text(&generation["completion_tokens"]),
    ));
    if let Some(providers) = generation["providers"]
        .as_object()
        .filter(|p| !p.is_empty())
    {
        let names: Vec<&str> = providers.keys().map(String::as_str).collect();
        out.push_str(&format!("- Providers: {}\n", names.join(", ")));
    }
    if let Some(notes) = report["notes"].as_array().filter(|n| !n.is_empty()) {
        out.push_str("\n## Notes\n\n");
        for note in notes {
            out.push_str(&format!("- {}\n", text(note)));
        }
    }
    out
}

/// The requirements of the run's documents and the cases of its suite, as the API lists them.
pub async fn collect(
    api: &dyn PipelineApi,
    project_id: Uuid,
    document_ids: &[Uuid],
    suite: Option<&str>,
) -> Result<(Vec<Value>, Vec<Value>), String> {
    let mut path = format!("/api/v1/projects/{project_id}/requirements");
    if !document_ids.is_empty() {
        let ids: Vec<String> = document_ids.iter().map(Uuid::to_string).collect();
        path = format!("{path}?document_ids={}", ids.join(","));
    }
    let reply = api.get(&path).await;
    if !reply.is_ok() {
        return Err(format!(
            "the requirements could not be read: {}",
            reply.problem()
        ));
    }
    let requirements = reply.body["requirements"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let cases = match suite {
        Some(suite) => {
            let reply = api
                .get(&format!(
                    "/api/v1/projects/{project_id}/test-cases?suite_id={suite}"
                ))
                .await;
            if !reply.is_ok() {
                return Err(format!(
                    "the run's test cases could not be listed: {}",
                    reply.problem()
                ));
            }
            reply.body.as_array().cloned().unwrap_or_default()
        }
        None => Vec::new(),
    };
    Ok((requirements, cases))
}

/// The REPORT step: builds the report and keeps it with the step (`report` in its detail).
pub async fn report(ctx: &StepCtx) -> Result<StepOutcome, StepError> {
    let run = ctx
        .store
        .get_autopilot_run(ctx.run_id)
        .await
        .map_err(|e| StepError::failed(format!("could not read the run: {e}")))?
        .ok_or_else(|| StepError::failed("the run no longer exists"))?;
    let steps = ctx
        .store
        .list_autopilot_steps(ctx.run_id)
        .await
        .map_err(|e| StepError::failed(format!("could not read the run's steps: {e}")))?;
    let suite = detail(&steps, Step::Generate)["suite_id"]
        .as_str()
        .map(str::to_string);
    let (requirements, cases) = collect(
        &*ctx.api,
        ctx.project_id,
        &ctx.config.requirement_document_ids,
        suite.as_deref(),
    )
    .await
    .map_err(StepError::failed)?;
    let built = build(&ReportInput {
        run: &run,
        steps: &steps,
        requirements: &requirements,
        cases: &cases,
        finished: true,
    });
    let summary = json!({
        "coverage_summary": built["coverage_summary"],
        "findings": built["findings"].as_array().map_or(0, Vec::len),
        "passed": built["execution"]["passed"],
        "failed": built["execution"]["failed"],
    });
    ctx.note(
        &format!(
            "Report written: {} finding(s), {} passed, {} failed",
            text(&summary["findings"]),
            text(&summary["passed"]),
            text(&summary["failed"])
        ),
        None,
    )
    .await;
    Ok(StepOutcome::Done {
        detail: json!({"report": built, "summary": summary}),
        skipped: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopilot::AutopilotConfig;
    use crate::autopilot_pipeline::scripted::ScriptedApi;
    use crate::autopilot_pipeline::Reply;
    use crate::autopilot_test_support::*;
    use chrono::TimeZone;

    fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc
            .timestamp_opt(1_700_000_000 + seconds, 0)
            .unwrap()
    }

    fn run_row(state: &str, config: Value) -> AutopilotRunRow {
        AutopilotRunRow {
            id: Uuid::parse_str("6f95ee30-3fc8-4bc6-9d6d-f8cb4c8ef988").unwrap(),
            project_id: "p".into(),
            name: "Release 5".into(),
            state: state.into(),
            config,
            estimate: json!({"totals": {"ai_calls": {"min": 1, "max": 3}, "paid": false}}),
            approved_at: Some(at(0)),
            approved_paid: false,
            created_by_key: Uuid::nil(),
            created_by_name: "maker".into(),
            current_step: None,
            error: None,
            cancel_requested: false,
            created_at: at(0),
            started_at: Some(at(10)),
            finished_at: Some(at(610)),
            updated_at: at(610),
        }
    }

    fn step_row(which: Step, state: &str, detail: Value) -> AutopilotStepRow {
        AutopilotStepRow {
            run_id: Uuid::nil(),
            name: which.as_str().into(),
            position: 0,
            state: state.into(),
            attempts: 1,
            detail,
            error: None,
            started_at: Some(at(20)),
            finished_at: Some(at(80)),
        }
    }

    fn config_value() -> Value {
        json!({"base_url": "https://shop.example.test", "roles": [{"name": "anonymous"}, {"name": "admin"}], "execute": true})
    }

    fn requirement(key: &str, kind: &str) -> Value {
        json!({"key": key, "kind": kind, "text": format!("Requirement {key}.")})
    }

    fn case(id: &str, requirement: &str, status: &str) -> Value {
        json!({"id": id, "title": format!("Case {id}"), "status": status, "requirement_reference": requirement})
    }

    fn exec_case(id: &str, requirement: &str, outcome: &str, reason: Option<&str>) -> Value {
        json!({"id": id, "title": format!("Case {id}"), "requirement": requirement, "place": "DONE", "run_id": format!("run-{id}"),
               "attempts": [], "outcome": outcome, "reason": reason, "retried": false})
    }

    /// A run that went all the way: five requirements, six cases, one of each kind of ending.
    fn steps() -> Vec<AutopilotStepRow> {
        vec![
            step_row(
                Step::Requirements,
                "DONE",
                json!({"requirements": 5, "documents": 1, "by_kind": {"functional": 5}}),
            ),
            step_row(
                Step::Crawl,
                "DONE",
                json!({"crawl_run_id": "crawl-1", "reused": false, "pages": 12, "roles": [{"role": "anonymous", "pages": 12}]}),
            ),
            step_row(
                Step::GapCheck,
                "DONE",
                json!({"summary": {"requirements": 5, "found": 3, "weak": 1, "missing": 1}}),
            ),
            step_row(
                Step::Generate,
                "DONE",
                json!({
                    "suite_id": "s1", "cases": 6, "providers": {"groq/llama": 1, "ollama/qwen": 1}, "fallback_used": true,
                    "prompt_tokens": 3000, "completion_tokens": 1500, "latency_ms": 90_000,
                    "groups": [{"failed_waves": [{"reason": "timed out"}]}, {"failed_waves": []}],
                }),
            ),
            step_row(
                Step::Validate,
                "DONE",
                json!({"validated": 5, "invalid": [{"id": "x"}]}),
            ),
            step_row(
                Step::Approve,
                "DONE",
                json!({"approved": 6, "rejected": 0, "pending": 2, "automatic": false, "released": true}),
            ),
            step_row(
                Step::Execute,
                "DONE",
                json!({
                    "role": "admin", "total": 6, "finished": 6,
                    "counts": {"PASSED": 3, "FAILED": 1, "NOT_EXECUTABLE": 1, "TIMEOUT": 1},
                    "cases": [
                        exec_case("c1", "FR-1", "PASSED", None),
                        exec_case("c2", "FR-1", "FAILED", Some("expected 1, got 2")),
                        exec_case("c3", "FR-2", "PASSED", None),
                        exec_case("c4", "FR-3", "NOT_EXECUTABLE", Some("NOT_EXECUTABLE: no selector")),
                        exec_case("c5", "FR-4", "PASSED", None),
                        exec_case("c6", "FR-5", "TIMEOUT", Some("30 s")),
                    ],
                }),
            ),
        ]
    }

    fn requirements() -> Vec<Value> {
        vec![
            requirement("FR-1", "functional"),
            requirement("FR-2", "functional"),
            requirement("FR-3", "functional"),
            requirement("FR-4", "functional"),
            requirement("FR-5", "functional"),
            requirement("FR-6", "functional"),
            requirement("FR-1", "functional"),
        ]
    }

    fn cases() -> Vec<Value> {
        vec![
            case("c1", "FR-1", "EXECUTED"),
            case("c2", "FR-1", "EXECUTED"),
            case("c3", "FR-2", "EXECUTED"),
            case("c4", "FR-3", "EXECUTED"),
            case("c5", "FR-4", "EXECUTED"),
            case("c6", "FR-5", "EXECUTED"),
            case("c7", "FR-4", "VALIDATED"),
        ]
    }

    fn build_it(state: &str, finished: bool) -> Value {
        let run = run_row(state, config_value());
        let steps = steps();
        build(&ReportInput {
            run: &run,
            steps: &steps,
            requirements: &requirements(),
            cases: &cases(),
            finished,
        })
    }

    // ------------------------------------------------------------------------------ the verdict

    #[test]
    fn a_requirement_is_failed_if_a_case_failed_passed_if_one_passed_and_otherwise_as_far_as_it_got(
    ) {
        //                    cases passed failed ran
        assert_eq!(verdict(0, 0, 0, 0), "NO_CASES");
        assert_eq!(verdict(3, 2, 1, 3), "FAILED", "a failure outweighs a pass");
        assert_eq!(verdict(2, 1, 0, 2), "PASSED");
        assert_eq!(
            verdict(3, 1, 0, 1),
            "PASSED",
            "cases that were not run do not undo a pass"
        );
        assert_eq!(
            verdict(2, 0, 0, 1),
            "INCONCLUSIVE",
            "ran, but neither passed nor failed"
        );
        assert_eq!(verdict(1, 0, 0, 0), "NOT_EXECUTED");
        assert_eq!(verdict(1, 0, 1, 1), "FAILED");
    }

    // ------------------------------------------------------------------------------ the report

    #[test]
    fn the_report_says_what_the_run_did_step_by_step_from_what_the_steps_recorded() {
        let report = build_it("COMPLETED", true);
        assert_eq!(report["final"], true);
        assert_eq!(report["run"]["name"], "Release 5");
        assert_eq!(report["run"]["state"], "COMPLETED");
        assert_eq!(report["run"]["created_by"], "maker");
        assert_eq!(report["run"]["seconds"], 600);
        assert_eq!(
            report["application"]["base_url"],
            "https://shop.example.test"
        );
        assert_eq!(
            report["application"]["roles"],
            json!(["anonymous", "admin"])
        );
        assert_eq!(report["application"]["execution_role"], "admin");
        assert_eq!(
            report["requirements"],
            json!({"total": 5, "by_kind": {"functional": 5}})
        );
        assert_eq!(report["crawl"]["pages"], 12);
        assert_eq!(report["crawl"]["reused"], false);
        assert_eq!(report["gaps"]["missing"], 1);
        assert_eq!(report["generation"]["cases"], 6);
        assert_eq!(report["generation"]["groups"], 2);
        assert_eq!(report["generation"]["prompt_tokens"], 3000);
        assert_eq!(report["generation"]["fallback_used"], true);
        assert_eq!(report["validation"]["validated"], 5);
        assert_eq!(report["validation"]["invalid"].as_array().unwrap().len(), 1);
        assert_eq!(
            report["approval"],
            json!({"approved": 6, "rejected": 0, "undecided": 2, "automatic": false, "released": true})
        );
        assert_eq!(report["execution"]["enabled"], true);
        assert_eq!(report["execution"]["done"], true);
        assert_eq!(report["execution"]["passed"], 3);
        assert_eq!(report["execution"]["failed"], 1);
        assert_eq!(report["execution"]["finished"], 6);
        assert_eq!(report["execution"]["counts"]["TIMEOUT"], 1);
        assert_eq!(
            report["cost"]["estimated"]["ai_calls"],
            json!({"min": 1, "max": 3})
        );
        assert_eq!(report["cost"]["actual"]["ai_prompt_tokens"], 3000);
        assert_eq!(report["cost"]["actual"]["ai_seconds"], 90);
        assert_eq!(report["cost"]["actual"]["run_seconds"], 600);
        assert_eq!(report["steps"].as_array().unwrap().len(), 7);
        assert_eq!(
            report["steps"][0],
            json!({"name": "REQUIREMENTS", "state": "DONE", "attempts": 1, "seconds": 60})
        );
    }

    #[test]
    fn a_failed_case_is_a_finding_and_a_case_that_gave_no_answer_is_listed_with_why() {
        let report = build_it("COMPLETED", true);
        let findings = report["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["case_id"], "c2");
        assert_eq!(findings[0]["title"], "Case c2");
        assert_eq!(findings[0]["requirement"], "FR-1");
        assert_eq!(findings[0]["reason"], "expected 1, got 2");
        assert_eq!(findings[0]["run_id"], "run-c2");
        let others: Vec<(&str, &str)> = report["not_run"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["case_id"].as_str().unwrap(),
                    r["outcome"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(others, [("c4", "NOT_EXECUTABLE"), ("c6", "TIMEOUT")]);
    }

    #[test]
    fn every_requirement_has_a_verdict_from_its_cases_and_an_id_two_documents_share_is_listed_once()
    {
        let report = build_it("COMPLETED", true);
        let rows = report["coverage"].as_array().unwrap();
        let verdicts: Vec<(&str, &str)> = rows
            .iter()
            .map(|r| (r["key"].as_str().unwrap(), r["verdict"].as_str().unwrap()))
            .collect();
        assert_eq!(
            verdicts,
            [
                ("FR-1", "FAILED"),
                ("FR-2", "PASSED"),
                ("FR-3", "NOT_EXECUTED"),
                ("FR-4", "PASSED"),
                ("FR-5", "INCONCLUSIVE"),
                ("FR-6", "NO_CASES")
            ]
        );
        let fr1 = &rows[0];
        assert_eq!(
            (
                fr1["cases"].as_u64(),
                fr1["ran"].as_u64(),
                fr1["passed"].as_u64(),
                fr1["failed"].as_u64(),
                fr1["not_run"].as_u64()
            ),
            (Some(2), Some(2), Some(1), Some(1), Some(0))
        );
        let fr4 = &rows[3];
        assert_eq!(
            (
                fr4["cases"].as_u64(),
                fr4["ran"].as_u64(),
                fr4["not_run"].as_u64()
            ),
            (Some(2), Some(1), Some(1)),
            "the case that was never run counts as not run"
        );
        let fr3 = &rows[2];
        assert_eq!(
            (fr3["ran"].as_u64(), fr3["not_run"].as_u64()),
            (Some(0), Some(1)),
            "refused before it could run"
        );
        assert_eq!(rows[0]["text"], "Requirement FR-1.");
        assert_eq!(rows[0]["kind"], "functional");
        assert_eq!(
            report["coverage_summary"],
            json!({"FAILED": 1, "INCONCLUSIVE": 1, "NOT_EXECUTED": 1, "NO_CASES": 1, "PASSED": 2})
        );
    }

    #[test]
    fn the_cases_run_are_those_that_reached_an_outcome_except_the_ones_refused_before_they_could_run() {
        // Six reached an outcome; one of them (FR-3's) was refused by the compiler, so nothing was run for it. A timeout was run.
        let report = build_it("COMPLETED", true);
        assert_eq!(report["execution"]["finished"], 6);
        assert_eq!(report["execution"]["ran"], 5);
        // Every kind of refusal counts as not run, and no count ever goes below nothing.
        let run = run_row("COMPLETED", config_value());
        let mut steps = steps();
        steps[6].detail["finished"] = json!(4);
        steps[6].detail["counts"] = json!({"NOT_EXECUTABLE": 1, "COMPILATION_FAILED": 1, "COMPILATION_UNAVAILABLE": 1, "NOT_RUN": 1, "PASSED": 5});
        let report = build(&ReportInput {
            run: &run,
            steps: &steps,
            requirements: &requirements(),
            cases: &cases(),
            finished: true,
        });
        assert_eq!(report["execution"]["ran"], 0, "4 finished, 4 refused");
    }

    #[test]
    fn a_case_the_person_rejected_at_the_gate_is_not_a_case_of_its_requirement() {
        let run = run_row("COMPLETED", config_value());
        let mut steps = steps();
        steps[5].detail["rejected"] = json!(2);
        let mut cases = cases();
        cases.push(case("c8", "FR-6", "REJECTED")); // FR-6's only case
        cases.push(case("c9", "FR-2", "REJECTED")); // FR-2 also has c3, which ran
        let report = build(&ReportInput {
            run: &run,
            steps: &steps,
            requirements: &requirements(),
            cases: &cases,
            finished: true,
        });
        let row = |key: &str| {
            report["coverage"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["key"] == key)
                .unwrap()
                .clone()
        };
        assert_eq!(row("FR-6")["verdict"], "NO_CASES", "nothing approved tests it");
        assert_eq!(row("FR-6")["cases"], 0);
        assert_eq!(row("FR-6")["not_run"], 0);
        assert_eq!(row("FR-2")["cases"], 1, "the rejected case is not counted beside the one that ran");
        assert_eq!(row("FR-2")["verdict"], "PASSED");
        let notes: Vec<&str> = report["notes"].as_array().unwrap().iter().map(|n| n.as_str().unwrap()).collect();
        assert!(notes.contains(&"2 case(s) were rejected at the approval gate and are not counted."), "{notes:?}");
        // No rejection, no note.
        let plain: Vec<String> = build_it("COMPLETED", true)["notes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().to_string())
            .collect();
        assert!(plain.iter().all(|n| !n.contains("rejected")), "{plain:?}");
    }

    #[test]
    fn the_notes_say_what_to_know_before_trusting_the_numbers() {
        let notes: Vec<String> = build_it("COMPLETED", true)["notes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().to_string())
            .collect();
        let has = |part: &str| notes.iter().any(|n| n.contains(part));
        assert!(
            has("other than its first choice") && has("groq/llama, ollama/qwen"),
            "{notes:?}"
        );
        assert!(has("1 wave(s) of the AI failed"), "{notes:?}");
        assert!(has("1 case(s) failed the structural check"), "{notes:?}");
        assert!(has("2 generated case(s) were left undecided"), "{notes:?}");
        assert!(has("1 requirement(s) have no case"), "{notes:?}");
        assert!(
            has("1 requirement(s) showed no sign in the crawl"),
            "{notes:?}"
        );
        assert!(has("2 case(s) could not give an answer"), "{notes:?}");
        assert!(!has("has not ended") && !has("The run failed"), "{notes:?}");
    }

    #[test]
    fn a_report_of_a_run_that_is_still_going_says_so_and_a_final_one_reads_as_completed() {
        let live = build_it("RUNNING", false);
        assert_eq!(live["final"], false);
        assert_eq!(live["run"]["state"], "RUNNING");
        assert!(live["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n == "The run has not ended: this is what it has found so far."));
        // Written by the last step, while the run is still marked running: it reads as the completed run it is about to be.
        let mut run = run_row("RUNNING", config_value());
        run.finished_at = None;
        let steps = steps();
        let frozen = build(&ReportInput {
            run: &run,
            steps: &steps,
            requirements: &requirements(),
            cases: &cases(),
            finished: true,
        });
        assert_eq!(frozen["run"]["state"], "COMPLETED");
        assert!(
            frozen["run"]["finished_at"].is_string(),
            "{}",
            frozen["run"]
        );
        assert!(frozen["run"]["seconds"].as_i64().unwrap() > 0);
        // A run that failed says why, and stays failed.
        let mut failed = run_row("FAILED", config_value());
        failed.error =
            Some("Generating the test cases: the AI could not generate group 1/1".into());
        let report = build(&ReportInput {
            run: &failed,
            steps: &steps[..3],
            requirements: &requirements(),
            cases: &[],
            finished: true,
        });
        assert_eq!(report["run"]["state"], "FAILED");
        assert!(
            report["notes"][0]
                .as_str()
                .unwrap()
                .starts_with("The run failed: Generating the test cases"),
            "{}",
            report["notes"]
        );
        assert_eq!(report["execution"]["done"], false);
        assert_eq!(report["coverage_summary"], json!({"NO_CASES": 6}));
    }

    #[test]
    fn a_run_made_with_execution_off_says_the_cases_were_not_run() {
        let mut config = config_value();
        config["execute"] = json!(false);
        let run = run_row("COMPLETED", config);
        let report = build(&ReportInput {
            run: &run,
            steps: &steps()[..6],
            requirements: &requirements(),
            cases: &cases(),
            finished: true,
        });
        assert_eq!(report["execution"]["enabled"], false);
        assert!(report["notes"].as_array().unwrap().iter().any(|n| n == "The run was made with execution off: the cases were written and approved but not run."));
    }

    // ------------------------------------------------------------------------------ markdown

    #[test]
    fn the_markdown_reads_as_a_summary_with_the_failures_and_the_requirements_in_tables() {
        let md = markdown(&build_it("COMPLETED", true));
        assert!(md.starts_with("# Autopilot report: Release 5\n"), "{md}");
        assert!(
            md.contains("- **Application:** https://shop.example.test (roles: anonymous, admin)"),
            "{md}"
        );
        assert!(md.contains("- **Run:** COMPLETED — maker, 600 s"), "{md}");
        assert!(md.contains("5 case(s) run: 3 passed, 1 failed."), "{md}");
        assert!(
            md.contains(
                "Requirements: 1 FAILED, 1 INCONCLUSIVE, 1 NOT_EXECUTED, 1 NO_CASES, 2 PASSED."
            ),
            "{md}"
        );
        assert!(md.contains("## Failures\n\n| Case | Requirement | Why | Run |\n|---|---|---|---|\n| Case c2 | FR-1 | expected 1, got 2 | run-c2 |"), "{md}");
        assert!(md.contains("## Cases that gave no answer"), "{md}");
        assert!(
            md.contains("| Case c4 | FR-3 | NOT_EXECUTABLE | NOT_EXECUTABLE: no selector |"),
            "{md}"
        );
        assert!(
            md.contains("## Requirements\n\n| Requirement | Verdict | Cases | Passed | Failed |"),
            "{md}"
        );
        assert!(
            md.contains("| FR-1 — Requirement FR-1. | FAILED | 2 | 1 | 1 |"),
            "{md}"
        );
        assert!(
            md.contains(
                "- Generation: 6 case(s) in 2 group(s), 3000 prompt / 1500 completion tokens"
            ),
            "{md}"
        );
        assert!(md.contains("- Providers: groq/llama, ollama/qwen"), "{md}");
        assert!(md.contains("## Notes\n\n- The AI router used"), "{md}");
        assert!(!md.contains("Not final"), "{md}");
    }

    #[test]
    fn the_markdown_of_a_live_report_says_it_is_not_final_and_one_with_no_execution_says_so() {
        let live = markdown(&build_it("RUNNING", false));
        assert!(
            live.contains("- **Not final:** the run has not ended."),
            "{live}"
        );
        let mut config = config_value();
        config["execute"] = json!(false);
        let run = run_row("COMPLETED", config);
        let report = build(&ReportInput {
            run: &run,
            steps: &steps()[..6],
            requirements: &[],
            cases: &[],
            finished: true,
        });
        let md = markdown(&report);
        assert!(
            md.contains("Execution was off: the cases were written but not run."),
            "{md}"
        );
        assert!(
            !md.contains("## Failures") && !md.contains("## Requirements"),
            "empty sections are left out: {md}"
        );
    }

    #[test]
    fn a_pipe_or_a_new_line_in_a_cell_does_not_break_the_table() {
        assert_eq!(cell(&json!("a | b\nc")), "a \\| b c");
        assert_eq!(cell(&Value::Null), "");
        assert_eq!(cell(&json!(3)), "3");
    }

    // ------------------------------------------------------------------------------ the step

    #[tokio::test]
    async fn the_requirements_and_the_cases_are_listed_the_way_a_page_would() {
        let project = Uuid::new_v4();
        let doc = Uuid::new_v4();
        let api = ScriptedApi::new()
            .on(
                "GET",
                &format!("/api/v1/projects/{project}/requirements?document_ids={doc}"),
                vec![ok(
                    json!({"requirements": [requirement("FR-1", "functional")]}),
                )],
            )
            .on(
                "GET",
                &format!("/api/v1/projects/{project}/requirements"),
                vec![ok(
                    json!({"requirements": [requirement("FR-9", "security")]}),
                )],
            )
            .on(
                "GET",
                &format!("/api/v1/projects/{project}/test-cases?suite_id=s1"),
                vec![ok(json!([case("c1", "FR-1", "EXECUTED")]))],
            );
        let api: &dyn PipelineApi = &api;
        let (r, c) = collect(api, project, &[doc], Some("s1")).await.unwrap();
        assert_eq!((r.len(), c.len()), (1, 1));
        assert_eq!(r[0]["key"], "FR-1");
        let (r, c) = collect(api, project, &[], None).await.unwrap();
        assert_eq!(r[0]["key"], "FR-9", "no document chosen: every document");
        assert!(
            c.is_empty(),
            "no suite yet: no cases, and no request for them"
        );

        let broken = ScriptedApi::new().on(
            "GET",
            &format!("/api/v1/projects/{project}/requirements"),
            vec![Reply::error(500, "internal", "boom")],
        );
        let broken: &dyn PipelineApi = &broken;
        assert_eq!(
            collect(broken, project, &[], None).await.unwrap_err(),
            "the requirements could not be read: boom"
        );
        let no_cases = ScriptedApi::new()
            .on(
                "GET",
                &format!("/api/v1/projects/{project}/requirements"),
                vec![ok(json!({"requirements": []}))],
            )
            .on(
                "GET",
                &format!("/api/v1/projects/{project}/test-cases?suite_id=s1"),
                vec![Reply::error(500, "internal", "locked")],
            );
        let no_cases: &dyn PipelineApi = &no_cases;
        assert_eq!(
            collect(no_cases, project, &[], Some("s1"))
                .await
                .unwrap_err(),
            "the run's test cases could not be listed: locked"
        );
    }

    #[tokio::test]
    async fn the_report_step_builds_the_report_from_the_run_and_keeps_it_with_the_step() {
        let mut f = fixture(Step::Report, config_json(), |_| ScriptedApi::new()).await;
        // What the earlier steps recorded.
        for row in steps() {
            f.ctx
                .store
                .save_autopilot_step_detail(f.ctx.run_id, &row.name, &row.detail)
                .await
                .unwrap();
        }
        f.api = std::sync::Arc::new(
            ScriptedApi::new()
                .on(
                    "GET",
                    &f.p("requirements"),
                    vec![ok(json!({"requirements": requirements()}))],
                )
                .on(
                    "GET",
                    &f.p("test-cases?suite_id=s1"),
                    vec![ok(json!(cases()))],
                ),
        );
        f.ctx.api = f.api.clone();

        let (detail, skipped) = detail_of_done(report(&f.ctx).await);
        assert!(!skipped);
        let built = &detail["report"];
        assert_eq!(built["final"], true);
        assert_eq!(
            built["run"]["state"], "COMPLETED",
            "written by the last step, before the run completes"
        );
        assert_eq!(built["coverage_summary"]["FAILED"], 1);
        assert_eq!(built["findings"].as_array().unwrap().len(), 1);
        assert_eq!(
            detail["summary"],
            json!({"coverage_summary": built["coverage_summary"], "findings": 1, "passed": 3, "failed": 1})
        );
        assert!(timeline(&f)
            .await
            .contains(&"Report written: 1 finding(s), 3 passed, 1 failed".to_string()));
    }

    fn config_json() -> AutopilotConfig {
        config(json!({}))
    }

    #[tokio::test]
    async fn the_report_step_fails_when_what_it_reads_cannot_be_read() {
        let f = fixture(Step::Report, config(json!({})), |p| {
            ScriptedApi::new().on(
                "GET",
                &p("requirements"),
                vec![Reply::error(500, "internal", "boom")],
            )
        })
        .await;
        assert_eq!(
            failed(report(&f.ctx).await),
            "the requirements could not be read: boom"
        );
    }
}
