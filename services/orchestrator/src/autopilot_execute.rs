//! The EXECUTE step: the approved cases are compiled and run against the application, a few at a time.
//!
//! Each case goes through the route the Test Cases page uses to run one (`POST .../test-cases/:id/handoff` with
//! `compile_and_execute`, grounded in the run's crawl and as the run's role), so the compiler, the executors, the policy
//! and the audit are the ones every other run has: this step decides only which case goes next, and reads how each run
//! ended. A case whose run ended in an infrastructure failure (the executor could not run it, or it timed out, which is
//! also what a restart leaves behind) is run again once, by the route made for that (`retry-failed`); a case that
//! genuinely failed is never run again.
//!
//! What has been done is saved in the step's detail after every change: a run id the moment its run exists, an outcome the
//! moment it is known. A step started again after a restart goes on with the cases that are left and looks at the runs
//! that were going, instead of running any case twice.

use crate::autopilot::{Step, StepError, StepOutcome};
use crate::autopilot_generate::{cases_of_suite, suite_of_run};
use crate::autopilot_runner::StepCtx;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

/// Runs whose state means they will not change again.
const ENDED: [&str; 5] = ["COMPLETED", "FAILED", "PARTIAL", "CANCELLED", "REJECTED"];

/// A note for the timeline after every this many cases have finished.
const PROGRESS_EVERY: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Place {
    /// Approved, not handed off yet.
    Pending,
    /// A run exists and is being watched.
    Running,
    /// Its outcome is known.
    Done,
}

/// One run of a case that has ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Attempt {
    run_id: String,
    outcome: String,
    reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CaseRun {
    id: String,
    title: String,
    requirement: Option<String>,
    place: Place,
    /// The run being watched, or the one that gave the outcome.
    run_id: Option<String>,
    /// The runs that ended before the last one (an infrastructure failure that was run again).
    attempts: Vec<Attempt>,
    /// How the case came out: a task's status (`PASSED`, `FAILED`, `TIMEOUT`, `INFRA_ERROR`, `NEEDS_REVIEW`,
    /// `CANCELLED`, ...), `NOT_EXECUTABLE`, `COMPILATION_FAILED`, `COMPILATION_UNAVAILABLE`, `NOT_RUN` (the route refused
    /// it for another reason) or `UNFINISHED` (its run did not end in time).
    outcome: Option<String>,
    reason: Option<String>,
    retried: bool,
}

struct Progress {
    role: Option<String>,
    cases: Vec<CaseRun>,
}

impl Progress {
    fn from_detail(detail: &Value, role: Option<String>) -> Self {
        let cases = serde_json::from_value(detail["cases"].clone()).unwrap_or_default();
        Progress { role, cases }
    }

    fn to_detail(&self, concurrency: usize) -> Value {
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        for case in self.cases.iter().filter(|c| c.place == Place::Done) {
            *counts
                .entry(
                    case.outcome
                        .clone()
                        .unwrap_or_else(|| "UNKNOWN".to_string()),
                )
                .or_default() += 1;
        }
        let count = |place| self.cases.iter().filter(|c| c.place == place).count();
        json!({
            "role": self.role,
            "concurrency": concurrency,
            "cases": self.cases,
            "total": self.cases.len(),
            "pending": count(Place::Pending),
            "running": count(Place::Running),
            "finished": count(Place::Done),
            "counts": counts,
        })
    }

    /// Adds the cases the suite has that are approved (or were run since, by an attempt whose progress was not saved) and
    /// are not here yet.
    fn add_new(&mut self, cases: &[Value]) {
        for case in cases {
            let Some(id) = case["id"].as_str() else {
                continue;
            };
            if self.cases.iter().any(|c| c.id == id) {
                continue;
            }
            let (place, run_id) = match case["status"].as_str() {
                Some("APPROVED") => (Place::Pending, None),
                Some("EXECUTED") => match case["executed_run_id"].as_str() {
                    Some(run) => (Place::Running, Some(run.to_string())),
                    None => continue,
                },
                _ => continue,
            };
            self.cases.push(CaseRun {
                id: id.to_string(),
                title: case["title"].as_str().unwrap_or("").to_string(),
                requirement: case["requirement_reference"].as_str().map(str::to_string),
                place,
                run_id,
                attempts: Vec::new(),
                outcome: None,
                reason: None,
                retried: false,
            });
        }
    }
}

/// How a run that has ended came out, and why when it says: the status of its one task (a run made by a handoff has one),
/// or, for a run without one, its own state. `None`: the run has not ended.
fn outcome_of_run(run: &Value) -> Option<(String, Option<String>)> {
    let state = run["state"].as_str().unwrap_or("");
    if !ENDED.contains(&state) {
        return None;
    }
    let task = &run["tasks"][0];
    let reason = task["reason"].as_str().map(tidy_reason).filter(|r| !r.is_empty());
    match task["status"].as_str() {
        Some("QUEUED") | Some("RUNNING") | None => Some((state.to_string(), reason)),
        Some(status) => Some((status.to_string(), reason)),
    }
}

/// The most a case's reason says.
const MAX_REASON_CHARS: usize = 400;

/// A run's reason as a person reads it in the report and the timeline. The terminal's colour codes are taken out, and of
/// Playwright's code frame (`> 22 |   await expect(...)`, the line that failed, among the lines around it) only that line is
/// kept beside the first line; any other reason is its words on one line. At most `MAX_REASON_CHARS` characters.
fn tidy_reason(reason: &str) -> String {
    let plain = strip_terminal_codes(reason);
    let mut lines = plain.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next().unwrap_or("");
    let marked = plain.lines().map(str::trim).find_map(|line| {
        let rest = line.strip_prefix("> ")?;
        let (number, code) = rest.split_once('|')?;
        let number = number.trim();
        (!number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
            .then(|| format!("line {number}: {}", code.split_whitespace().collect::<Vec<_>>().join(" ")))
    });
    let text = match marked {
        Some(failed) => format!("{first} \u{2014} failed at {failed}"),
        None => plain.split_whitespace().collect::<Vec<_>>().join(" "),
    };
    if text.chars().count() > MAX_REASON_CHARS {
        let cut: String = text.chars().take(MAX_REASON_CHARS).collect();
        format!("{}\u{2026}", cut.trim_end())
    } else {
        text
    }
}

/// `text` without ANSI escape sequences (`ESC [ ... final byte`): what a terminal turns into colours and cursor moves.
fn strip_terminal_codes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            // Parameters and intermediates, then one final byte in `@`..=`~`.
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// What a refused handoff was: the compiler's own word when the message starts with one (`NOT_EXECUTABLE: ...`), else
/// `NOT_RUN`.
fn refusal_outcome(problem: &str) -> &'static str {
    for word in [
        "NOT_EXECUTABLE",
        "COMPILATION_FAILED",
        "COMPILATION_UNAVAILABLE",
    ] {
        if problem.starts_with(word) {
            return word;
        }
    }
    "NOT_RUN"
}

fn finished_count(progress: &Progress) -> usize {
    progress
        .cases
        .iter()
        .filter(|c| c.place == Place::Done)
        .count()
}

/// Stops the runs being watched, best effort, when the autopilot run is cancelled.
async fn stop_running(ctx: &StepCtx, progress: &Progress) {
    for case in progress.cases.iter().filter(|c| c.place == Place::Running) {
        if let Some(run) = &case.run_id {
            let _ = ctx
                .api
                .post(&ctx.path(&format!("executions/{run}/cancel")), json!({}))
                .await;
        }
    }
}

/// Runs the approved cases.
pub async fn execute(ctx: &StepCtx) -> Result<StepOutcome, StepError> {
    if !ctx.config.execute {
        return Ok(StepOutcome::Done {
            detail: json!({"execute": false}),
            skipped: true,
        });
    }
    let suite = suite_of_run(ctx).await?;
    let crawl = ctx.detail_of(Step::Crawl).await["crawl_run_id"]
        .as_str()
        .map(str::to_string);
    let concurrency = ctx.config.concurrency().max(1) as usize;
    let mut progress = Progress::from_detail(&ctx.detail, ctx.config.execution_role());
    progress.add_new(&cases_of_suite(ctx, suite).await?);
    ctx.checkpoint(&progress.to_detail(concurrency)).await?;
    if progress.cases.is_empty() {
        ctx.note("No case was approved, so nothing was run", None)
            .await;
    }

    let mut watching: HashMap<String, Instant> = HashMap::new();
    let mut announced = finished_count(&progress);
    loop {
        if let Err(cancelled) = ctx.check_cancel().await {
            stop_running(ctx, &progress).await;
            return Err(cancelled);
        }
        let mut changed = false;

        // Start cases while there is room.
        while progress
            .cases
            .iter()
            .filter(|c| c.place == Place::Running)
            .count()
            < concurrency
        {
            let Some(at) = progress
                .cases
                .iter()
                .position(|c| c.place == Place::Pending)
            else {
                break;
            };
            let mut body = json!({"compile_and_execute": true});
            if let Some(crawl) = &crawl {
                body["crawl_run_id"] = json!(crawl);
            }
            if let Some(role) = &progress.role {
                body["role"] = json!(role);
            }
            let id = progress.cases[at].id.clone();
            let reply = ctx
                .api
                .post(&ctx.path(&format!("test-cases/{id}/handoff")), body)
                .await;
            let case = &mut progress.cases[at];
            match reply.body["run_id"].as_str() {
                Some(run) if reply.is_ok() => {
                    case.place = Place::Running;
                    case.run_id = Some(run.to_string());
                    watching.insert(id, Instant::now());
                }
                _ => {
                    let problem = reply.problem();
                    let outcome = refusal_outcome(&problem);
                    case.place = Place::Done;
                    case.outcome = Some(outcome.to_string());
                    case.reason = Some(problem.clone());
                    let title = case.title.clone();
                    ctx.note(
                        &format!("{title}: {outcome} — {problem}"),
                        Some(json!({"case_id": id})),
                    )
                    .await;
                }
            }
            changed = true;
            ctx.checkpoint(&progress.to_detail(concurrency)).await?;
        }

        // Look at the runs that are going.
        let running: Vec<usize> = progress
            .cases
            .iter()
            .enumerate()
            .filter(|(_, c)| c.place == Place::Running)
            .map(|(at, _)| at)
            .collect();
        for at in running {
            let (id, run_id) = {
                let case = &progress.cases[at];
                (case.id.clone(), case.run_id.clone().unwrap_or_default())
            };
            let started = *watching.entry(id).or_insert_with(Instant::now);
            let reply = ctx
                .api
                .get(&ctx.path(&format!("executions/{run_id}")))
                .await;
            let ended = if reply.is_ok() {
                outcome_of_run(&reply.body)
            } else {
                Some((
                    "UNKNOWN".to_string(),
                    Some(format!("the run could not be read: {}", reply.problem())),
                ))
            };
            let Some((outcome, reason)) = ended else {
                if started.elapsed() > ctx.options.run_timeout {
                    let minutes = ctx.options.run_timeout.as_secs() / 60;
                    let case = &mut progress.cases[at];
                    case.place = Place::Done;
                    case.outcome = Some("UNFINISHED".to_string());
                    case.reason = Some(format!(
                        "the run did not end in {minutes} minutes (it may be waiting for approval under the project's policy; it is run {run_id} in Executions)"
                    ));
                    changed = true;
                    ctx.checkpoint(&progress.to_detail(concurrency)).await?;
                }
                continue;
            };
            changed = true;
            let retryable = matches!(outcome.as_str(), "INFRA_ERROR" | "TIMEOUT");
            let mut retry_note = None;
            if retryable && !progress.cases[at].retried {
                let retry = ctx
                    .api
                    .post(
                        &ctx.path(&format!("executions/{run_id}/retry-failed")),
                        json!({}),
                    )
                    .await;
                match retry.body["run_id"].as_str() {
                    Some(next) if retry.is_ok() => {
                        let case = &mut progress.cases[at];
                        case.attempts.push(Attempt {
                            run_id: run_id.clone(),
                            outcome: outcome.clone(),
                            reason: reason.clone(),
                        });
                        case.retried = true;
                        case.run_id = Some(next.to_string());
                        watching.insert(case.id.clone(), Instant::now());
                        let title = case.title.clone();
                        ctx.note(
                            &format!("{title}: {outcome} on the first run, run again once"),
                            Some(json!({"run_id": next, "was": run_id})),
                        )
                        .await;
                        ctx.checkpoint(&progress.to_detail(concurrency)).await?;
                        continue;
                    }
                    _ => retry_note = Some(retry.problem()),
                }
            }
            let case = &mut progress.cases[at];
            case.place = Place::Done;
            case.outcome = Some(outcome.clone());
            case.reason = match (reason, retry_note) {
                (Some(reason), Some(why)) => {
                    Some(format!("{reason} (it could not be run again: {why})"))
                }
                (None, Some(why)) => Some(format!("it could not be run again: {why}")),
                (reason, None) => reason,
            };
            if outcome != "PASSED" {
                let (title, why) = (case.title.clone(), case.reason.clone());
                ctx.note(
                    &format!(
                        "{title}: {outcome}{}",
                        why.map(|w| format!(" — {w}")).unwrap_or_default()
                    ),
                    Some(json!({"case_id": case.id, "run_id": case.run_id})),
                )
                .await;
            }
            ctx.checkpoint(&progress.to_detail(concurrency)).await?;
        }

        let finished = finished_count(&progress);
        if finished >= announced + PROGRESS_EVERY {
            announced = finished;
            ctx.note(
                &format!("Ran {finished} of {} case(s)", progress.cases.len()),
                None,
            )
            .await;
        }
        if progress.cases.iter().all(|c| c.place == Place::Done) {
            break;
        }
        if !changed {
            ctx.pause().await?;
        }
    }

    let detail = progress.to_detail(concurrency);
    Ok(StepOutcome::Done {
        detail,
        skipped: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopilot_pipeline::scripted::ScriptedApi;
    use crate::autopilot_pipeline::Reply;
    use crate::autopilot_test_support::*;
    use std::time::Duration;

    const SUITE: &str = "5b1f0b9e-7c5e-4a55-9a3e-0c7f6f0b1a11";

    fn case(id: &str, status: &str) -> Value {
        json!({"id": id, "title": format!("Case {id}"), "status": status, "requirement_reference": format!("FR-{id}"), "tags": []})
    }

    fn executed(id: &str, run: &str) -> Value {
        let mut c = case(id, "EXECUTED");
        c["executed_run_id"] = json!(run);
        c
    }

    fn handoff_ok(run: &str) -> Reply {
        Reply::new(201, json!({"run_id": run, "status": "SCHEDULED"}))
    }

    /// What Playwright printed for a failed assertion (colour codes and all), as the executor keeps it in a task's reason.
    const PLAYWRIGHT_REASON: &str = "playwright exited with Some(1)\n\n    \u{1b}[0m \u{1b}[90m 20 |\u{1b}[39m   \u{1b}[36mawait\u{1b}[39m page\u{1b}[33m.\u{1b}[39mgoto(\u{1b}[32m\"https://shop.example.test/\"\u{1b}[39m)\u{1b}[33m;\u{1b}[39m\n     \u{1b}[90m 21 |\u{1b}[39m   console\u{1b}[33m.\u{1b}[39mlog(\u{1b}[32m\"AIQA_STEP\"\u{1b}[39m)\u{1b}[33m;\u{1b}[39m\n    \u{1b}[31m\u{1b}[1m>\u{1b}[22m\u{1b}[39m\u{1b}[90m 22 |\u{1b}[39m   \u{1b}[36mawait\u{1b}[39m expect(page.locator(\"h1\"))\u{1b}[33m.\u{1b}[39mtoHaveText(\u{1b}[32m\"Welcome back\"\u{1b}[39m)\u{1b}[33m;\u{1b}[39m\n     \u{1b}[90m    |\u{1b}[39m                          \u{1b}[31m\u{1b}[1m^\u{1b}[22m\u{1b}[39m\n     \u{1b}[90m 23 |\u{1b}[39m });\u{1b}[0m\n    \u{1b}[2m    at D:\\repo\\generated.spec.ts:22:94\u{1b}[22m\n\n\u{1b}[1A\u{1b}[2K\u{1b}[31m  1 failed\u{1b}[39m\n";

    #[test]
    fn a_reason_is_read_without_the_terminals_colour_codes() {
        assert_eq!(tidy_reason("\u{1b}[31mexpected 1, got 2\u{1b}[0m"), "expected 1, got 2");
        assert_eq!(strip_terminal_codes("a\u{1b}[1A\u{1b}[2Kb\u{1b}[38;5;9mc"), "abc");
        assert_eq!(strip_terminal_codes("a\u{1b}[3~b\u{1b}[@c"), "abc", "the final byte may be any of `@` to `~`");
        assert_eq!(strip_terminal_codes("a\u{1b}[Hb"), "ab", "a sequence with no parameters");
        // An escape that is not a control sequence, and a lone one at the end, are left alone (nothing is swallowed).
        assert_eq!(strip_terminal_codes("x\u{1b}Ty"), "x\u{1b}Ty");
        assert_eq!(strip_terminal_codes("x\u{1b}"), "x\u{1b}");
        assert_eq!(strip_terminal_codes("x\u{1b}[31"), "x", "an unfinished sequence ends the text");
    }

    #[test]
    fn of_playwrights_code_frame_only_the_line_that_failed_is_kept() {
        assert_eq!(
            tidy_reason(PLAYWRIGHT_REASON),
            "playwright exited with Some(1) \u{2014} failed at line 22: await expect(page.locator(\"h1\")).toHaveText(\"Welcome back\");"
        );
    }

    #[test]
    fn any_other_reason_is_its_words_on_one_line() {
        assert_eq!(tidy_reason("expected the cart badge to show 1"), "expected the cart badge to show 1");
        assert_eq!(tidy_reason("no browser\ncould be   started\n"), "no browser could be started");
        assert_eq!(tidy_reason(""), "");
        assert_eq!(tidy_reason("\u{1b}[31m\u{1b}[0m"), "");
        // A `>` that is not a marked code line (no line number before the bar) is just text.
        assert_eq!(tidy_reason("> a | b"), "> a | b");
        assert_eq!(tidy_reason("> 2a | b"), "> 2a | b", "a number is all digits");
        assert_eq!(tidy_reason("> | x"), "> | x", "and there is one");
    }

    #[test]
    fn a_long_reason_is_cut_with_an_ellipsis_and_a_short_one_is_not() {
        let long = "word ".repeat(200);
        let tidy = tidy_reason(&long);
        assert!(tidy.chars().count() <= MAX_REASON_CHARS, "{tidy}");
        assert!(tidy.ends_with('\u{2026}'), "{tidy}");
        let exact = "x".repeat(MAX_REASON_CHARS);
        assert_eq!(tidy_reason(&exact), exact, "exactly the most is not cut");
        let over = "x".repeat(MAX_REASON_CHARS + 1);
        assert_eq!(tidy_reason(&over).chars().count(), MAX_REASON_CHARS + 1, "the cut text and its ellipsis");
    }

    #[test]
    fn a_run_that_ended_gives_its_tidied_reason_and_no_reason_when_nothing_is_left_of_it() {
        let ended = |reason: Value| json!({"state": "COMPLETED", "tasks": [{"status": "FAILED", "reason": reason}]});
        assert_eq!(outcome_of_run(&ended(json!("\u{1b}[31mboom\u{1b}[0m"))), Some(("FAILED".to_string(), Some("boom".to_string()))));
        assert_eq!(outcome_of_run(&ended(json!(PLAYWRIGHT_REASON))).unwrap().1.unwrap().chars().take(30).collect::<String>(), "playwright exited with Some(1)");
        assert_eq!(outcome_of_run(&ended(json!("\u{1b}[0m"))), Some(("FAILED".to_string(), None)));
        assert_eq!(outcome_of_run(&ended(Value::Null)), Some(("FAILED".to_string(), None)));
    }

    fn run_reply(state: &str, status: &str, reason: Option<&str>) -> Reply {
        ok(json!({"state": state, "tasks": [{"id": "t", "status": status, "reason": reason}]}))
    }

    async fn prepare(f: &Fixture) {
        f.ctx
            .store
            .save_autopilot_step_detail(f.ctx.run_id, "GENERATE", &json!({"suite_id": SUITE}))
            .await
            .unwrap();
        f.ctx
            .store
            .save_autopilot_step_detail(f.ctx.run_id, "CRAWL", &json!({"crawl_run_id": "crawl-9"}))
            .await
            .unwrap();
    }

    fn listing(f: &Fixture) -> String {
        f.p(&format!("test-cases?suite_id={SUITE}"))
    }

    fn outcomes(detail: &Value) -> Vec<(String, String)> {
        detail["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                (
                    c["id"].as_str().unwrap().to_string(),
                    c["outcome"].as_str().unwrap_or("-").to_string(),
                )
            })
            .collect()
    }

    async fn saved(f: &Fixture) -> Value {
        f.ctx.detail_of(Step::Execute).await
    }

    fn pair(a: &str, b: &str) -> (String, String) {
        (a.to_string(), b.to_string())
    }

    // ------------------------------------------------------------------------------ the pure parts

    #[test]
    fn a_run_that_has_ended_is_read_by_the_status_of_its_task_and_one_that_has_not_is_not_read() {
        let of = |state: &str, task: Value| outcome_of_run(&json!({"state": state, "tasks": task}));
        assert_eq!(
            of("COMPLETED", json!([{"status": "PASSED", "reason": null}])),
            Some(("PASSED".into(), None))
        );
        assert_eq!(
            of(
                "FAILED",
                json!([{"status": "FAILED", "reason": "expected 1, got 2"}])
            ),
            Some(("FAILED".into(), Some("expected 1, got 2".into())))
        );
        assert_eq!(
            of(
                "PARTIAL",
                json!([{"status": "INFRA_ERROR", "reason": "no browser"}])
            )
            .unwrap()
            .0,
            "INFRA_ERROR"
        );
        assert_eq!(
            of("FAILED", json!([{"status": "TIMEOUT"}])).unwrap().0,
            "TIMEOUT"
        );
        // The run's own state stands in when the task says nothing that ended.
        assert_eq!(
            of("CANCELLED", json!([{"status": "QUEUED"}])).unwrap().0,
            "CANCELLED"
        );
        assert_eq!(of("REJECTED", json!([])).unwrap().0, "REJECTED");
        assert_eq!(
            of("FAILED", json!([{"status": "RUNNING"}])).unwrap().0,
            "FAILED"
        );
        // Not ended: no answer yet.
        for state in ["CREATED", "PENDING_APPROVAL", "SCHEDULED", "RUNNING", ""] {
            assert_eq!(of(state, json!([{"status": "PASSED"}])), None, "{state}");
        }
    }

    #[test]
    fn a_refused_handoff_is_told_by_the_compilers_own_word_or_is_not_run() {
        assert_eq!(
            refusal_outcome("NOT_EXECUTABLE: no selector"),
            "NOT_EXECUTABLE"
        );
        assert_eq!(
            refusal_outcome("COMPILATION_FAILED: bad plan"),
            "COMPILATION_FAILED"
        );
        assert_eq!(
            refusal_outcome("COMPILATION_UNAVAILABLE: no provider"),
            "COMPILATION_UNAVAILABLE"
        );
        assert_eq!(
            refusal_outcome("environment variable AIQA_LOGIN_ADMIN_USERNAME is not set"),
            "NOT_RUN"
        );
        assert_eq!(refusal_outcome("the service answered 502"), "NOT_RUN");
        assert_eq!(
            refusal_outcome("a NOT_EXECUTABLE case"),
            "NOT_RUN",
            "the word has to start the message"
        );
    }

    #[test]
    fn approved_cases_are_added_once_and_a_case_run_already_is_watched_not_run_again() {
        let mut progress = Progress::from_detail(&json!({}), Some("admin".to_string()));
        progress.add_new(&[
            case("a", "APPROVED"),
            case("b", "VALIDATED"),
            case("c", "REJECTED"),
            executed("d", "run-d"),
            case("e", "EXECUTED"),
            json!({"title": "no id", "status": "APPROVED"}),
        ]);
        let found: Vec<(&str, Place, Option<&str>)> = progress
            .cases
            .iter()
            .map(|c| (c.id.as_str(), c.place, c.run_id.as_deref()))
            .collect();
        assert_eq!(
            found,
            [
                ("a", Place::Pending, None),
                ("d", Place::Running, Some("run-d"))
            ],
            "only approved ones, and executed ones that have a run"
        );
        assert_eq!(progress.cases[0].title, "Case a");
        assert_eq!(progress.cases[0].requirement.as_deref(), Some("FR-a"));
        // Again: nothing is added twice, and a case that is known keeps what it has.
        progress.cases[0].place = Place::Done;
        progress.add_new(&[case("a", "APPROVED"), case("f", "APPROVED")]);
        assert_eq!(progress.cases.len(), 3);
        assert_eq!(progress.cases[0].place, Place::Done);
        assert_eq!(progress.cases[2].id, "f");
    }

    #[test]
    fn the_progress_is_written_with_its_counts_and_read_back_the_same() {
        let mut progress = Progress::from_detail(&json!({}), None);
        progress.add_new(&[
            case("a", "APPROVED"),
            case("b", "APPROVED"),
            case("c", "APPROVED"),
            case("d", "APPROVED"),
        ]);
        progress.cases[0].place = Place::Done;
        progress.cases[0].outcome = Some("PASSED".into());
        progress.cases[1].place = Place::Done;
        progress.cases[1].outcome = Some("FAILED".into());
        progress.cases[2].place = Place::Running;
        progress.cases[2].run_id = Some("r".into());
        let detail = progress.to_detail(2);
        assert_eq!(detail["total"], 4);
        assert_eq!(
            (
                detail["pending"].as_u64(),
                detail["running"].as_u64(),
                detail["finished"].as_u64()
            ),
            (Some(1), Some(1), Some(2))
        );
        assert_eq!(detail["counts"], json!({"FAILED": 1, "PASSED": 1}));
        assert_eq!(detail["concurrency"], 2);
        assert!(detail["role"].is_null());
        let back = Progress::from_detail(&detail, Some("x".into()));
        assert_eq!(back.cases, progress.cases);
        assert_eq!(
            back.role.as_deref(),
            Some("x"),
            "the role is the run's, read again"
        );
        // A detail that holds no cases (or not what it should) reads as none.
        assert!(Progress::from_detail(&json!({"cases": "no"}), None)
            .cases
            .is_empty());
        assert_eq!(finished_count(&back), 2);
    }

    // ------------------------------------------------------------------------------ the step

    #[tokio::test]
    async fn a_run_made_with_execution_off_skips_the_step_and_touches_nothing() {
        let f = fixture(Step::Execute, config(json!({"execute": false})), |_| {
            ScriptedApi::new()
        })
        .await;
        let (detail, skipped) = detail_of_done(execute(&f.ctx).await);
        assert!(skipped);
        assert_eq!(detail, json!({"execute": false}));
        assert!(f.api.seen().is_empty());
    }

    #[tokio::test]
    async fn approved_cases_are_handed_off_compiled_grounded_in_the_crawl_and_as_the_role_and_each_outcome_is_kept(
    ) {
        let mut f = fixture(Step::Execute, config(json!({"execution_role": "admin", "roles": [{"name": "anonymous"}, {"name": "admin", "login_actions": [{"type": "NAVIGATE", "url": "https://shop.example.test/login"}]}]})), |_| {
            ScriptedApi::new()
        })
        .await;
        prepare(&f).await;
        f.api = std::sync::Arc::new(
            ScriptedApi::new()
                .on(
                    "GET",
                    &listing(&f),
                    vec![ok(json!([
                        case("a", "APPROVED"),
                        case("b", "APPROVED"),
                        case("c", "REJECTED"),
                        case("d", "VALIDATED")
                    ]))],
                )
                .on(
                    "POST",
                    &f.p("test-cases/a/handoff"),
                    vec![handoff_ok("run-a")],
                )
                .on(
                    "POST",
                    &f.p("test-cases/b/handoff"),
                    vec![handoff_ok("run-b")],
                )
                .on(
                    "GET",
                    &f.p("executions/run-a"),
                    vec![
                        run_reply("RUNNING", "RUNNING", None),
                        run_reply("COMPLETED", "PASSED", None),
                    ],
                )
                .on(
                    "GET",
                    &f.p("executions/run-b"),
                    vec![run_reply(
                        "FAILED",
                        "FAILED",
                        Some("expected the cart badge to show 1"),
                    )],
                ),
        );
        f.ctx.api = f.api.clone();

        let (detail, skipped) = detail_of_done(execute(&f.ctx).await);
        assert!(!skipped);
        assert_eq!(
            outcomes(&detail),
            [pair("a", "PASSED"), pair("b", "FAILED")],
            "the rejected and the unapproved case are not run"
        );
        assert_eq!(detail["counts"], json!({"FAILED": 1, "PASSED": 1}));
        assert_eq!(detail["role"], "admin");
        assert_eq!(
            detail["cases"][1]["reason"],
            "expected the cart badge to show 1"
        );
        assert_eq!(detail["cases"][0]["run_id"], "run-a");
        assert_eq!(detail["cases"][0]["requirement"], "FR-a");
        assert_eq!(
            (
                detail["total"].as_u64(),
                detail["finished"].as_u64(),
                detail["pending"].as_u64(),
                detail["running"].as_u64()
            ),
            (Some(2), Some(2), Some(0), Some(0))
        );
        // The handoff is the compile-and-run route, grounded in the crawl the run made and as the run's role.
        let sent = f.api.body_of("POST", &f.p("test-cases/a/handoff")).unwrap();
        assert_eq!(
            sent,
            json!({"compile_and_execute": true, "crawl_run_id": "crawl-9", "role": "admin"})
        );
        // What is kept is what is returned, and a failure is a line in the timeline (a pass is not).
        assert_eq!(saved(&f).await, detail);
        let lines = timeline(&f).await;
        assert!(
            lines.contains(&"Case b: FAILED — expected the cart badge to show 1".to_string()),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.starts_with("Case a")), "{lines:?}");
    }

    #[tokio::test]
    async fn the_visitor_is_the_default_role_and_a_run_of_logged_in_roles_uses_the_first() {
        let mut f = fixture(Step::Execute, config(json!({})), |_| ScriptedApi::new()).await;
        prepare(&f).await;
        f.api = std::sync::Arc::new(
            ScriptedApi::new()
                .on(
                    "GET",
                    &listing(&f),
                    vec![ok(json!([case("a", "APPROVED")]))],
                )
                .on(
                    "POST",
                    &f.p("test-cases/a/handoff"),
                    vec![handoff_ok("run-a")],
                )
                .on(
                    "GET",
                    &f.p("executions/run-a"),
                    vec![run_reply("COMPLETED", "PASSED", None)],
                ),
        );
        f.ctx.api = f.api.clone();
        detail_of_done(execute(&f.ctx).await);
        let sent = f.api.body_of("POST", &f.p("test-cases/a/handoff")).unwrap();
        assert_eq!(
            sent,
            json!({"compile_and_execute": true, "crawl_run_id": "crawl-9"}),
            "no role: the crawl's role that never logged in"
        );

        let login = json!([{"type": "NAVIGATE", "url": "https://shop.example.test/login"}]);
        let mut g = fixture(Step::Execute, config(json!({"roles": [{"name": "standard", "login_actions": login}, {"name": "admin", "login_actions": login}]})), |_| ScriptedApi::new()).await;
        prepare(&g).await;
        g.api = std::sync::Arc::new(
            ScriptedApi::new()
                .on(
                    "GET",
                    &listing(&g),
                    vec![ok(json!([case("a", "APPROVED")]))],
                )
                .on(
                    "POST",
                    &g.p("test-cases/a/handoff"),
                    vec![handoff_ok("run-a")],
                )
                .on(
                    "GET",
                    &g.p("executions/run-a"),
                    vec![run_reply("COMPLETED", "PASSED", None)],
                ),
        );
        g.ctx.api = g.api.clone();
        detail_of_done(execute(&g.ctx).await);
        assert_eq!(
            g.api.body_of("POST", &g.p("test-cases/a/handoff")).unwrap()["role"],
            "standard"
        );
    }

    #[tokio::test]
    async fn only_as_many_cases_run_at_once_as_the_run_allows() {
        let cases = json!([
            case("a", "APPROVED"),
            case("b", "APPROVED"),
            case("c", "APPROVED")
        ]);
        let script = |f: &Fixture| {
            ScriptedApi::new()
                .on("GET", &listing(f), vec![ok(cases.clone())])
                .on(
                    "POST",
                    &f.p("test-cases/a/handoff"),
                    vec![handoff_ok("run-a")],
                )
                .on(
                    "POST",
                    &f.p("test-cases/b/handoff"),
                    vec![handoff_ok("run-b")],
                )
                .on(
                    "POST",
                    &f.p("test-cases/c/handoff"),
                    vec![handoff_ok("run-c")],
                )
                .on(
                    "GET",
                    &f.p("executions/run-a"),
                    vec![
                        run_reply("RUNNING", "RUNNING", None),
                        run_reply("COMPLETED", "PASSED", None),
                    ],
                )
                .on(
                    "GET",
                    &f.p("executions/run-b"),
                    vec![
                        run_reply("RUNNING", "RUNNING", None),
                        run_reply("COMPLETED", "PASSED", None),
                    ],
                )
                .on(
                    "GET",
                    &f.p("executions/run-c"),
                    vec![run_reply("COMPLETED", "PASSED", None)],
                )
        };
        let position = |seen: &[String], needle: &str| {
            seen.iter()
                .position(|s| s.ends_with(needle))
                .unwrap_or_else(|| panic!("{needle} not in {seen:?}"))
        };

        // One at a time: the second is handed off only after the first has been looked at and has ended.
        let mut one = fixture(Step::Execute, config(json!({"run_concurrency": 1})), |_| {
            ScriptedApi::new()
        })
        .await;
        prepare(&one).await;
        one.api = std::sync::Arc::new(script(&one));
        one.ctx.api = one.api.clone();
        let (detail, _) = detail_of_done(execute(&one.ctx).await);
        assert_eq!(detail["counts"], json!({"PASSED": 3}));
        assert_eq!(detail["concurrency"], 1);
        let seen = one.api.seen();
        assert!(
            position(&seen, "test-cases/b/handoff") > position(&seen, "executions/run-a"),
            "{seen:?}"
        );
        assert!(
            position(&seen, "test-cases/c/handoff") > position(&seen, "executions/run-b"),
            "{seen:?}"
        );
        assert!(position(&seen, "executions/run-a") < position(&seen, "test-cases/b/handoff"));

        // Two at a time: the first two go before either is looked at, the third waits for one of them.
        let mut two = fixture(Step::Execute, config(json!({"run_concurrency": 2})), |_| {
            ScriptedApi::new()
        })
        .await;
        prepare(&two).await;
        two.api = std::sync::Arc::new(script(&two));
        two.ctx.api = two.api.clone();
        detail_of_done(execute(&two.ctx).await);
        let seen = two.api.seen();
        assert!(
            position(&seen, "test-cases/b/handoff") < position(&seen, "executions/run-a"),
            "{seen:?}"
        );
        assert!(
            position(&seen, "test-cases/c/handoff") > position(&seen, "executions/run-a"),
            "{seen:?}"
        );
    }

    #[tokio::test]
    async fn a_case_the_route_refuses_is_an_outcome_with_the_reason_and_the_others_still_run() {
        let mut f = fixture(Step::Execute, config(json!({})), |_| ScriptedApi::new()).await;
        prepare(&f).await;
        f.api = std::sync::Arc::new(
            ScriptedApi::new()
                .on(
                    "GET",
                    &listing(&f),
                    vec![ok(json!([
                        case("a", "APPROVED"),
                        case("b", "APPROVED"),
                        case("c", "APPROVED"),
                        case("d", "APPROVED"),
                        case("e", "APPROVED")
                    ]))],
                )
                .on(
                    "POST",
                    &f.p("test-cases/a/handoff"),
                    vec![Reply::error(
                        400,
                        "validation",
                        "NOT_EXECUTABLE: no selector for the cart badge",
                    )],
                )
                .on(
                    "POST",
                    &f.p("test-cases/b/handoff"),
                    vec![Reply::error(
                        400,
                        "validation",
                        "COMPILATION_UNAVAILABLE: no provider can compile",
                    )],
                )
                .on(
                    "POST",
                    &f.p("test-cases/c/handoff"),
                    vec![Reply::error(
                        400,
                        "validation",
                        "environment variable AIQA_LOGIN_ADMIN_USERNAME is not set",
                    )],
                )
                .on(
                    "POST",
                    &f.p("test-cases/d/handoff"),
                    vec![Reply::error(
                        409,
                        "conflict",
                        "test case must be APPROVED before handoff (current: EXECUTED)",
                    )],
                )
                .on(
                    "POST",
                    &f.p("test-cases/e/handoff"),
                    vec![handoff_ok("run-e")],
                )
                .on(
                    "GET",
                    &f.p("executions/run-e"),
                    vec![run_reply("COMPLETED", "PASSED", None)],
                ),
        );
        f.ctx.api = f.api.clone();
        let (detail, _) = detail_of_done(execute(&f.ctx).await);
        assert_eq!(
            outcomes(&detail),
            [
                pair("a", "NOT_EXECUTABLE"),
                pair("b", "COMPILATION_UNAVAILABLE"),
                pair("c", "NOT_RUN"),
                pair("d", "NOT_RUN"),
                pair("e", "PASSED")
            ]
        );
        assert_eq!(
            detail["cases"][0]["reason"],
            "NOT_EXECUTABLE: no selector for the cart badge"
        );
        assert_eq!(detail["cases"][0]["run_id"], Value::Null);
        assert_eq!(
            detail["counts"],
            json!({"COMPILATION_UNAVAILABLE": 1, "NOT_EXECUTABLE": 1, "NOT_RUN": 2, "PASSED": 1})
        );
        let lines = timeline(&f).await;
        assert!(
            lines.contains(
                &"Case a: NOT_EXECUTABLE — NOT_EXECUTABLE: no selector for the cart badge"
                    .to_string()
            ),
            "{lines:?}"
        );
    }

    #[tokio::test]
    async fn an_infrastructure_failure_is_run_again_once_and_a_real_failure_never() {
        let mut f = fixture(Step::Execute, config(json!({"run_concurrency": 4})), |_| {
            ScriptedApi::new()
        })
        .await;
        prepare(&f).await;
        f.api = std::sync::Arc::new(
            ScriptedApi::new()
                .on(
                    "GET",
                    &listing(&f),
                    vec![ok(json!([
                        case("a", "APPROVED"),
                        case("b", "APPROVED"),
                        case("c", "APPROVED"),
                        case("d", "APPROVED")
                    ]))],
                )
                .on(
                    "POST",
                    &f.p("test-cases/a/handoff"),
                    vec![handoff_ok("run-a")],
                )
                .on(
                    "POST",
                    &f.p("test-cases/b/handoff"),
                    vec![handoff_ok("run-b")],
                )
                .on(
                    "POST",
                    &f.p("test-cases/c/handoff"),
                    vec![handoff_ok("run-c")],
                )
                .on(
                    "POST",
                    &f.p("test-cases/d/handoff"),
                    vec![handoff_ok("run-d")],
                )
                // a: no browser the first time, fine the second.
                .on(
                    "GET",
                    &f.p("executions/run-a"),
                    vec![run_reply("PARTIAL", "INFRA_ERROR", Some("no browser"))],
                )
                .on(
                    "POST",
                    &f.p("executions/run-a/retry-failed"),
                    vec![Reply::new(201, json!({"run_id": "run-a2"}))],
                )
                .on(
                    "GET",
                    &f.p("executions/run-a2"),
                    vec![
                        run_reply("RUNNING", "RUNNING", None),
                        run_reply("COMPLETED", "PASSED", None),
                    ],
                )
                // b: a real failure, not run again.
                .on(
                    "GET",
                    &f.p("executions/run-b"),
                    vec![run_reply("FAILED", "FAILED", Some("wrong total"))],
                )
                // c: times out both times.
                .on(
                    "GET",
                    &f.p("executions/run-c"),
                    vec![run_reply("FAILED", "TIMEOUT", Some("30 s"))],
                )
                .on(
                    "POST",
                    &f.p("executions/run-c/retry-failed"),
                    vec![Reply::new(201, json!({"run_id": "run-c2"}))],
                )
                .on(
                    "GET",
                    &f.p("executions/run-c2"),
                    vec![run_reply("FAILED", "TIMEOUT", Some("30 s again"))],
                )
                // d: the retry is refused.
                .on(
                    "GET",
                    &f.p("executions/run-d"),
                    vec![run_reply("PARTIAL", "INFRA_ERROR", None)],
                )
                .on(
                    "POST",
                    &f.p("executions/run-d/retry-failed"),
                    vec![Reply::error(
                        409,
                        "conflict",
                        "run has not reached a terminal state yet",
                    )],
                ),
        );
        f.ctx.api = f.api.clone();
        let (detail, _) = detail_of_done(execute(&f.ctx).await);

        assert_eq!(
            outcomes(&detail),
            [
                pair("a", "PASSED"),
                pair("b", "FAILED"),
                pair("c", "TIMEOUT"),
                pair("d", "INFRA_ERROR")
            ]
        );
        let a = &detail["cases"][0];
        assert_eq!(a["retried"], true);
        assert_eq!(a["run_id"], "run-a2", "the run that gave the outcome");
        assert_eq!(
            a["attempts"],
            json!([{"run_id": "run-a", "outcome": "INFRA_ERROR", "reason": "no browser"}])
        );
        assert_eq!(detail["cases"][1]["retried"], false);
        assert_eq!(detail["cases"][1]["attempts"], json!([]));
        let c = &detail["cases"][2];
        assert_eq!(c["attempts"][0]["outcome"], "TIMEOUT");
        assert_eq!(c["reason"], "30 s again");
        assert_eq!(
            detail["cases"][3]["reason"],
            "it could not be run again: run has not reached a terminal state yet"
        );
        let seen = f.api.seen();
        assert!(
            !seen.iter().any(|s| s.ends_with("run-b/retry-failed")),
            "a real failure is never run again: {seen:?}"
        );
        assert_eq!(
            seen.iter()
                .filter(|s| s.ends_with("run-c/retry-failed"))
                .count(),
            1
        );
        assert!(
            !seen.iter().any(|s| s.ends_with("run-c2/retry-failed")),
            "once: {seen:?}"
        );
        assert!(timeline(&f)
            .await
            .contains(&"Case a: INFRA_ERROR on the first run, run again once".to_string()));
    }

    #[tokio::test]
    async fn a_run_that_does_not_end_in_time_or_cannot_be_read_is_an_outcome_not_a_failed_step() {
        let mut f = fixture(Step::Execute, config(json!({"run_concurrency": 4})), |_| {
            ScriptedApi::new()
        })
        .await;
        prepare(&f).await;
        f.ctx.options.run_timeout = Duration::from_millis(30);
        f.api = std::sync::Arc::new(
            ScriptedApi::new()
                .on(
                    "GET",
                    &listing(&f),
                    vec![ok(json!([case("a", "APPROVED"), case("b", "APPROVED")]))],
                )
                .on(
                    "POST",
                    &f.p("test-cases/a/handoff"),
                    vec![handoff_ok("run-a")],
                )
                .on(
                    "POST",
                    &f.p("test-cases/b/handoff"),
                    vec![handoff_ok("run-b")],
                )
                .on(
                    "GET",
                    &f.p("executions/run-a"),
                    vec![run_reply("PENDING_APPROVAL", "QUEUED", None)],
                )
                .on(
                    "GET",
                    &f.p("executions/run-b"),
                    vec![Reply::error(404, "not_found", "run not found")],
                ),
        );
        f.ctx.api = f.api.clone();
        let (detail, _) = detail_of_done(execute(&f.ctx).await);
        assert_eq!(
            outcomes(&detail),
            [pair("a", "UNFINISHED"), pair("b", "UNKNOWN")]
        );
        let reason = detail["cases"][0]["reason"].as_str().unwrap();
        assert!(
            reason.starts_with("the run did not end in 0 minutes") && reason.contains("run run-a"),
            "{reason}"
        );
        assert_eq!(
            detail["cases"][1]["reason"],
            "the run could not be read: run not found"
        );
    }

    #[tokio::test]
    async fn after_a_restart_the_cases_done_are_kept_the_runs_that_were_going_are_watched_and_only_the_rest_are_run(
    ) {
        let mut f = fixture(Step::Execute, config(json!({})), |_| ScriptedApi::new()).await;
        prepare(&f).await;
        f.api = std::sync::Arc::new(
            ScriptedApi::new()
                // a was done, b's run was going, c was never started, d was handed off after the last save.
                .on(
                    "GET",
                    &listing(&f),
                    vec![ok(json!([
                        executed("a", "run-a"),
                        executed("b", "run-b"),
                        case("c", "APPROVED"),
                        executed("d", "run-d")
                    ]))],
                )
                .on(
                    "POST",
                    &f.p("test-cases/c/handoff"),
                    vec![handoff_ok("run-c")],
                )
                .on(
                    "GET",
                    &f.p("executions/run-b"),
                    vec![run_reply("COMPLETED", "PASSED", None)],
                )
                .on(
                    "GET",
                    &f.p("executions/run-c"),
                    vec![run_reply("COMPLETED", "PASSED", None)],
                )
                .on(
                    "GET",
                    &f.p("executions/run-d"),
                    vec![run_reply("FAILED", "FAILED", Some("wrong price"))],
                ),
        );
        f.ctx.api = f.api.clone();
        f.ctx.detail = json!({"cases": [
            {"id": "a", "title": "Case a", "requirement": "FR-a", "place": "DONE", "run_id": "run-a", "attempts": [], "outcome": "PASSED", "reason": null, "retried": false},
            {"id": "b", "title": "Case b", "requirement": "FR-b", "place": "RUNNING", "run_id": "run-b", "attempts": [], "outcome": null, "reason": null, "retried": false},
        ]});
        let (detail, _) = detail_of_done(execute(&f.ctx).await);
        assert_eq!(
            outcomes(&detail),
            [
                pair("a", "PASSED"),
                pair("b", "PASSED"),
                pair("c", "PASSED"),
                pair("d", "FAILED")
            ],
            "in the order they are listed"
        );
        let seen = f.api.seen();
        assert!(
            !seen.iter().any(|s| s.contains("a/handoff")
                || s.contains("b/handoff")
                || s.contains("d/handoff")
                || s.ends_with("executions/run-a")),
            "{seen:?}"
        );
        assert_eq!(
            seen.iter().filter(|s| s.contains("/handoff")).count(),
            1,
            "one case was left to run"
        );
    }

    #[tokio::test]
    async fn a_run_with_no_approved_case_finishes_the_step_and_says_so() {
        let mut f = fixture(Step::Execute, config(json!({})), |_| ScriptedApi::new()).await;
        prepare(&f).await;
        f.api = std::sync::Arc::new(ScriptedApi::new().on(
            "GET",
            &listing(&f),
            vec![ok(json!([case("a", "REJECTED"), case("b", "VALIDATED")]))],
        ));
        f.ctx.api = f.api.clone();
        let (detail, skipped) = detail_of_done(execute(&f.ctx).await);
        assert!(!skipped);
        assert_eq!(
            (detail["total"].as_u64(), detail["finished"].as_u64()),
            (Some(0), Some(0))
        );
        assert_eq!(detail["counts"], json!({}));
        assert!(timeline(&f)
            .await
            .contains(&"No case was approved, so nothing was run".to_string()));
        assert_eq!(f.api.seen().len(), 1, "only the listing was asked for");
    }

    #[tokio::test]
    async fn a_cancelled_run_stops_the_runs_being_watched_and_the_step() {
        let mut f = fixture(Step::Execute, config(json!({})), |_| ScriptedApi::new()).await;
        prepare(&f).await;
        f.api = std::sync::Arc::new(
            ScriptedApi::new()
                .on(
                    "GET",
                    &listing(&f),
                    vec![ok(json!([executed("a", "run-a"), case("b", "APPROVED")]))],
                )
                .on("POST", &f.p("executions/run-a/cancel"), vec![ok(json!({}))]),
        );
        f.ctx.api = f.api.clone();
        f.ctx
            .store
            .cancel_autopilot_run(f.ctx.run_id)
            .await
            .unwrap();
        assert_eq!(execute(&f.ctx).await, Err(StepError::Cancelled));
        let seen = f.api.seen();
        assert!(
            seen.iter().any(|s| s.ends_with("executions/run-a/cancel")),
            "{seen:?}"
        );
        assert!(
            !seen.iter().any(|s| s.contains("/handoff")),
            "nothing new is started: {seen:?}"
        );
    }

    #[tokio::test]
    async fn the_step_needs_the_suite_and_says_how_far_it_has_got_every_ten_cases() {
        let f = fixture(Step::Execute, config(json!({})), |_| ScriptedApi::new()).await;
        assert!(failed(execute(&f.ctx).await).contains("the generate step did not record one"));

        let mut f = fixture(Step::Execute, config(json!({"run_concurrency": 8})), |_| {
            ScriptedApi::new()
        })
        .await;
        prepare(&f).await;
        let ids: Vec<String> = (1..=12).map(|n| format!("c{n:02}")).collect();
        let mut script = ScriptedApi::new().on(
            "GET",
            &listing(&f),
            vec![ok(json!(ids
                .iter()
                .map(|id| case(id, "APPROVED"))
                .collect::<Vec<_>>()))],
        );
        for id in &ids {
            script = script
                .on(
                    "POST",
                    &f.p(&format!("test-cases/{id}/handoff")),
                    vec![handoff_ok(&format!("run-{id}"))],
                )
                .on(
                    "GET",
                    &f.p(&format!("executions/run-{id}")),
                    vec![run_reply("COMPLETED", "PASSED", None)],
                );
        }
        f.api = std::sync::Arc::new(script);
        f.ctx.api = f.api.clone();
        let (detail, _) = detail_of_done(execute(&f.ctx).await);
        assert_eq!(detail["counts"], json!({"PASSED": 12}));
        let lines = timeline(&f).await;
        let progress: Vec<&String> = lines.iter().filter(|l| l.starts_with("Ran ")).collect();
        assert_eq!(progress.len(), 1, "{lines:?}");
        assert_eq!(
            progress[0], "Ran 12 of 12 case(s)",
            "the step reports when ten more have finished since it last said"
        );
    }

    #[tokio::test]
    async fn the_step_says_how_far_it_has_got_when_ten_have_finished_and_not_when_nine_have() {
        for (count, said) in [(9usize, false), (10, true)] {
            let mut f = fixture(Step::Execute, config(json!({"run_concurrency": 8})), |_| {
                ScriptedApi::new()
            })
            .await;
            prepare(&f).await;
            let ids: Vec<String> = (1..=count).map(|n| format!("c{n:02}")).collect();
            let mut script = ScriptedApi::new().on(
                "GET",
                &listing(&f),
                vec![ok(json!(ids.iter().map(|id| case(id, "APPROVED")).collect::<Vec<_>>()))],
            );
            for id in &ids {
                script = script
                    .on("POST", &f.p(&format!("test-cases/{id}/handoff")), vec![handoff_ok(&format!("run-{id}"))])
                    .on("GET", &f.p(&format!("executions/run-{id}")), vec![run_reply("COMPLETED", "PASSED", None)]);
            }
            f.api = std::sync::Arc::new(script);
            f.ctx.api = f.api.clone();
            detail_of_done(execute(&f.ctx).await);
            let lines = timeline(&f).await;
            let progress: Vec<&String> = lines.iter().filter(|l| l.starts_with("Ran ")).collect();
            if said {
                assert_eq!(progress, [&format!("Ran {count} of {count} case(s)")], "{lines:?}");
            } else {
                assert!(progress.is_empty(), "{count} cases: {lines:?}");
            }
        }
    }
}
