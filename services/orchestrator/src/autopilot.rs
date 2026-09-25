//! Autopilot: from a project's uploaded requirements to a report of executed tests, in one run that a person starts
//! knowing what it will cost and that carries on after the service was restarted.
//!
//! This module is the run's vocabulary and its rules, all of it pure: the states of a run and of its steps, what a
//! person asks a run to do (`AutopilotConfig`) and the limits that holds to, and what accepting the estimate means.
//! What happens inside a step is in `autopilot_steps.rs`, the loop that goes through the steps in
//! `autopilot_runner.rs`, the tables in `autopilot_store.rs` and the routes in `autopilot_api.rs`.

use crate::ai_generation::SkillInput;
use crate::crawler::CrawlRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Roles a run may crawl and test as (the anonymous visitor counts as one).
pub const MAX_ROLES: u64 = 20;
/// Runs of the compiled tests that go at once when the person does not say.
pub const DEFAULT_CONCURRENCY: u64 = 2;
pub const MAX_CONCURRENCY: u64 = 8;
/// The longest a run's label may be.
pub const MAX_NAME_CHARS: usize = 80;

// ------------------------------------------------------------------------------------------------- states

/// Where a run is. `AwaitingApproval` is the state a run is created in: nothing has happened and nothing will until the
/// person accepts the estimate. `Waiting` is a run that stopped at a gate and is not being worked on until a person
/// lets it continue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunState {
    AwaitingApproval,
    Running,
    Waiting,
    Completed,
    Failed,
    Cancelled,
}

impl RunState {
    pub fn as_str(self) -> &'static str {
        match self {
            RunState::AwaitingApproval => "AWAITING_APPROVAL",
            RunState::Running => "RUNNING",
            RunState::Waiting => "WAITING",
            RunState::Completed => "COMPLETED",
            RunState::Failed => "FAILED",
            RunState::Cancelled => "CANCELLED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            RunState::AwaitingApproval,
            RunState::Running,
            RunState::Waiting,
            RunState::Completed,
            RunState::Failed,
            RunState::Cancelled,
        ]
        .into_iter()
        .find(|state| state.as_str() == s)
    }

    /// A run that has ended one way or another.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            RunState::Completed | RunState::Failed | RunState::Cancelled
        )
    }
}

/// Where one step of a run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StepState {
    Pending,
    Running,
    Waiting,
    Done,
    /// There was nothing to do (a crawl that was already made was used), or the run ended before the step was reached.
    Skipped,
    Failed,
}

impl StepState {
    pub fn as_str(self) -> &'static str {
        match self {
            StepState::Pending => "PENDING",
            StepState::Running => "RUNNING",
            StepState::Waiting => "WAITING",
            StepState::Done => "DONE",
            StepState::Skipped => "SKIPPED",
            StepState::Failed => "FAILED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            StepState::Pending,
            StepState::Running,
            StepState::Waiting,
            StepState::Done,
            StepState::Skipped,
            StepState::Failed,
        ]
        .into_iter()
        .find(|state| state.as_str() == s)
    }

    /// A step that needs no more work: a run resumed after a restart passes over it.
    pub fn is_settled(self) -> bool {
        matches!(self, StepState::Done | StepState::Skipped)
    }
}

/// The steps of a run, in the order they are done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Step {
    /// The requirements read out of the uploaded documents: are there any.
    Requirements,
    /// The application crawled as each role (or a crawl already made, used).
    Crawl,
    /// The requirements against what the crawl found.
    GapCheck,
}

impl Step {
    /// In the order they run.
    pub const ALL: [Step; 3] = [Step::Requirements, Step::Crawl, Step::GapCheck];

    pub fn as_str(self) -> &'static str {
        match self {
            Step::Requirements => "REQUIREMENTS",
            Step::Crawl => "CRAWL",
            Step::GapCheck => "GAP_CHECK",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Step::ALL.into_iter().find(|step| step.as_str() == s)
    }

    /// What the step is called in a sentence.
    pub fn label(self) -> &'static str {
        match self {
            Step::Requirements => "reading the requirements",
            Step::Crawl => "crawling the application",
            Step::GapCheck => "checking the requirements against the crawl",
        }
    }
}

// ------------------------------------------------------------------------------------------------- what a person asks for

/// What an estimate is worked out from: what a run would be asked to do, whether or not a run is ever made of it.
#[derive(Debug, Clone)]
pub struct EstimateRequest {
    /// The documents the run is about; empty, every document of the project that has requirements.
    pub requirement_document_ids: Vec<Uuid>,
    pub skills: Vec<SkillInput>,
    /// `SMOKE`, `STANDARD`, `EXHAUSTIVE` or `CHAOS_EXPLORATORY`.
    pub depth: Option<String>,
    pub coverage_top_up: bool,
    /// Roles the crawl logs in as, `anonymous` included.
    pub roles: u64,
    pub focus_requirements: bool,
    /// The most requirements the gap check's AI step is asked about; `None`, the step is not used.
    pub verify_gaps: Option<u64>,
    pub execute: bool,
    pub run_concurrency: u64,
}

impl EstimateRequest {
    /// The limits every way of asking for an estimate, and every run, is held to. `Err` says which one is broken.
    pub fn check_limits(&self) -> Result<(), String> {
        if !(1..=MAX_ROLES).contains(&self.roles) {
            return Err(format!("roles must be between 1 and {MAX_ROLES}"));
        }
        if !(1..=MAX_CONCURRENCY).contains(&self.run_concurrency) {
            return Err(format!(
                "run_concurrency must be between 1 and {MAX_CONCURRENCY}"
            ));
        }
        if let Some(n) = self.verify_gaps {
            let most = crate::gap_verify::MAX_VERIFICATIONS_LIMIT as u64;
            if n == 0 || n > most {
                return Err(format!("verify_gaps must be between 1 and {most}"));
            }
        }
        Ok(())
    }
}

/// Whether cases the run has generated and validated are run without a person looking at them first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    /// The run stops for a person to approve the cases. The default: generated cases are not trusted unseen.
    #[default]
    Manual,
    /// Cases that passed validation are approved by the run itself.
    AutoValidated,
}

/// What a person asks a run to do. Stored with the run as it was given; the login steps in `roles` name their secrets
/// (`env:NAME`), never hold one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotConfig {
    /// A label for the run in lists; omitted, one is made from the time.
    #[serde(default)]
    pub name: Option<String>,
    /// The application under test.
    pub base_url: String,
    /// The roles to crawl and test as, each with the login steps a person has confirmed (`anonymous`, with none, is a
    /// role too).
    pub roles: Vec<CrawlRole>,
    /// A finished crawl of this project to use instead of crawling again.
    #[serde(default)]
    pub crawl_run_id: Option<Uuid>,
    /// The documents the run is about; omitted or empty, every document of the project that has requirements.
    #[serde(default)]
    pub requirement_document_ids: Vec<Uuid>,
    #[serde(default)]
    pub skills: Vec<SkillInput>,
    #[serde(default)]
    pub depth: Option<String>,
    #[serde(default)]
    pub coverage_top_up: bool,
    /// Crawl the pages the requirements are about first.
    #[serde(default)]
    pub focus_requirements: bool,
    /// Ask the AI about the requirements the words left unclear, at most this many.
    #[serde(default)]
    pub verify_gaps: Option<u64>,
    /// Whether approved cases are compiled and run (default true).
    #[serde(default = "yes")]
    pub execute: bool,
    /// How many runs of the compiled tests go at once (default 2).
    #[serde(default)]
    pub run_concurrency: Option<u64>,
    #[serde(default)]
    pub approval: ApprovalMode,
}

fn yes() -> bool {
    true
}

impl AutopilotConfig {
    pub fn concurrency(&self) -> u64 {
        self.run_concurrency.unwrap_or(DEFAULT_CONCURRENCY)
    }

    /// What an estimate of this run is worked out from.
    pub fn estimate_request(&self) -> EstimateRequest {
        EstimateRequest {
            requirement_document_ids: self.requirement_document_ids.clone(),
            skills: self.skills.clone(),
            depth: self.depth.clone(),
            coverage_top_up: self.coverage_top_up,
            roles: self.roles.len() as u64,
            focus_requirements: self.focus_requirements,
            verify_gaps: self.verify_gaps,
            execute: self.execute,
            run_concurrency: self.concurrency(),
        }
    }

    /// The run's label: the one given, trimmed, or one made from the time it was created.
    pub fn display_name(&self, created_at: chrono::DateTime<chrono::Utc>) -> String {
        match self
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            Some(name) => name.to_string(),
            None => format!("Autopilot {}", created_at.format("%Y-%m-%d %H:%M")),
        }
    }

    /// What can be checked without looking at the project or the network (the address and the roles' login steps are
    /// checked where the crawl is: `crawler::validate_crawl_plan`). `Err` lists every problem.
    pub fn check(&self) -> Result<(), Vec<String>> {
        let mut problems = Vec::new();
        if let Some(name) = self.name.as_deref() {
            if name.trim().chars().count() > MAX_NAME_CHARS {
                problems.push(format!("name may be at most {MAX_NAME_CHARS} characters"));
            }
        }
        if self.base_url.trim().is_empty() {
            problems.push("base_url is the address of the application to test".to_string());
        }
        if self.roles.is_empty() {
            problems.push(
                "a run needs at least one role (anonymous, with no login steps, is a role too)"
                    .to_string(),
            );
        }
        // An empty list of roles is reported above, not twice.
        let mut request = self.estimate_request();
        request.roles = request.roles.max(1);
        if let Err(problem) = request.check_limits() {
            problems.push(problem);
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems)
        }
    }

    /// The steps this run goes through, in order.
    pub fn steps(&self) -> Vec<Step> {
        Step::ALL.to_vec()
    }
}

// ------------------------------------------------------------------------------------------------- accepting the estimate

/// What accepting an estimate has to say. A run costs nothing until it is started, so starting it is where the person
/// accepts what it would cost: `accept_estimate` for any run, and `accept_paid` as well when a provider that charges is
/// among those it would call. `Ok(paid)` says whether it is such a run.
pub fn check_acceptance(
    estimate: &serde_json::Value,
    accept_estimate: bool,
    accept_paid: bool,
) -> Result<bool, String> {
    if !accept_estimate {
        return Err(
            "starting a run needs accept_estimate: true, to say you have read what it would cost"
                .to_string(),
        );
    }
    // An estimate that does not say is treated as one that would cost money.
    let paid = estimate
        .pointer("/totals/paid")
        .and_then(|p| p.as_bool())
        .unwrap_or(true);
    if paid && !accept_paid {
        return Err(
            "this run would call a provider that charges: accept_paid: true says you accept that cost".to_string(),
        );
    }
    Ok(paid)
}

// ------------------------------------------------------------------------------------------------- what happens in a step

/// What a step reports when it has done what it was asked.
#[derive(Debug, Clone, PartialEq)]
pub enum StepOutcome {
    /// Finished. `detail` is what the step found (and is kept with it); `skipped` says nothing needed doing.
    Done {
        detail: serde_json::Value,
        skipped: bool,
    },
    /// Stopped at a gate: the run waits, and is not worked on until a person lets it continue.
    Wait {
        detail: serde_json::Value,
        why: String,
    },
}

/// Why a step did not finish.
#[derive(Debug, Clone, PartialEq)]
pub enum StepError {
    /// The person cancelled the run while the step was working.
    Cancelled,
    /// The step cannot go on; the text says why, in words meant for the person.
    Failed(String),
}

impl StepError {
    pub fn failed(message: impl Into<String>) -> Self {
        StepError::Failed(message.into())
    }
}

/// The kinds of event a run's timeline holds.
pub mod event {
    pub const CREATED: &str = "run.created";
    pub const STARTED: &str = "run.started";
    pub const COMPLETED: &str = "run.completed";
    pub const FAILED: &str = "run.failed";
    pub const CANCELLED: &str = "run.cancelled";
    pub const RESUMED: &str = "run.resumed";
    pub const STEP_STARTED: &str = "step.started";
    pub const STEP_DONE: &str = "step.done";
    pub const STEP_SKIPPED: &str = "step.skipped";
    pub const STEP_WAITING: &str = "step.waiting";
    pub const STEP_FAILED: &str = "step.failed";
    /// Something a step did that is worth a line in the timeline, without changing anything's state.
    pub const NOTE: &str = "note";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AutopilotConfig {
        serde_json::from_value(serde_json::json!({
            "base_url": "https://shop.example.test",
            "roles": [{"name": "anonymous"}],
        }))
        .unwrap()
    }

    #[test]
    fn every_run_state_step_state_and_step_reads_back_from_its_own_name() {
        for state in [
            RunState::AwaitingApproval,
            RunState::Running,
            RunState::Waiting,
            RunState::Completed,
            RunState::Failed,
            RunState::Cancelled,
        ] {
            assert_eq!(RunState::parse(state.as_str()), Some(state));
            let json = serde_json::to_value(state).unwrap();
            assert_eq!(json, serde_json::json!(state.as_str()), "same on the wire");
        }
        for state in [
            StepState::Pending,
            StepState::Running,
            StepState::Waiting,
            StepState::Done,
            StepState::Skipped,
            StepState::Failed,
        ] {
            assert_eq!(StepState::parse(state.as_str()), Some(state));
            assert_eq!(
                serde_json::to_value(state).unwrap(),
                serde_json::json!(state.as_str())
            );
        }
        for step in Step::ALL {
            assert_eq!(Step::parse(step.as_str()), Some(step));
            assert_eq!(
                serde_json::to_value(step).unwrap(),
                serde_json::json!(step.as_str())
            );
            assert!(!step.label().is_empty());
        }
        assert_eq!(RunState::parse("running"), None, "names are exact");
        assert_eq!(StepState::parse("nope"), None);
        assert_eq!(Step::parse("crawl"), None);
    }

    #[test]
    fn only_completed_failed_and_cancelled_runs_have_ended_and_only_done_or_skipped_steps_are_settled(
    ) {
        assert!(!RunState::AwaitingApproval.is_terminal());
        assert!(!RunState::Running.is_terminal());
        assert!(!RunState::Waiting.is_terminal());
        assert!(RunState::Completed.is_terminal());
        assert!(RunState::Failed.is_terminal());
        assert!(RunState::Cancelled.is_terminal());
        assert!(StepState::Done.is_settled() && StepState::Skipped.is_settled());
        for state in [
            StepState::Pending,
            StepState::Running,
            StepState::Waiting,
            StepState::Failed,
        ] {
            assert!(!state.is_settled(), "{state:?}");
        }
    }

    #[test]
    fn the_steps_run_in_the_order_the_run_needs_them() {
        assert_eq!(
            Step::ALL,
            [Step::Requirements, Step::Crawl, Step::GapCheck],
            "the crawl needs the requirements read, the gap check needs the crawl"
        );
        assert_eq!(config().steps(), Step::ALL.to_vec());
    }

    #[test]
    fn a_config_needs_only_an_address_and_roles_and_the_rest_has_defaults() {
        let c = config();
        assert!(c.name.is_none() && c.crawl_run_id.is_none() && c.depth.is_none());
        assert!(c.requirement_document_ids.is_empty() && c.skills.is_empty());
        assert!(!c.coverage_top_up && !c.focus_requirements);
        assert!(c.verify_gaps.is_none());
        assert!(c.execute, "cases are run unless the person says not to");
        assert_eq!(c.concurrency(), DEFAULT_CONCURRENCY);
        assert_eq!(c.approval, ApprovalMode::Manual);
        assert!(c.check().is_ok());
    }

    #[test]
    fn a_config_asks_for_what_it_says_and_the_estimate_is_worked_out_from_the_same() {
        let c: AutopilotConfig = serde_json::from_value(serde_json::json!({
            "base_url": "https://shop.example.test",
            "roles": [{"name": "anonymous"}, {"name": "admin", "login_actions": [{"type": "NAVIGATE", "url": "https://shop.example.test/login"}]}],
            "requirement_document_ids": ["11111111-1111-1111-1111-111111111111"],
            "skills": [{"id": "s1", "name": "Functional Testing"}],
            "depth": "STANDARD",
            "coverage_top_up": true,
            "focus_requirements": true,
            "verify_gaps": 12,
            "execute": false,
            "run_concurrency": 4,
            "approval": "auto_validated",
        }))
        .unwrap();
        let e = c.estimate_request();
        assert_eq!(e.roles, 2, "one per role of the run");
        assert_eq!(e.requirement_document_ids.len(), 1);
        assert_eq!(e.skills.len(), 1);
        assert_eq!(e.skills[0].description, "", "a skill needs no description");
        assert_eq!(e.depth.as_deref(), Some("STANDARD"));
        assert!(e.coverage_top_up && e.focus_requirements && !e.execute);
        assert_eq!((e.verify_gaps, e.run_concurrency), (Some(12), 4));
        assert_eq!(c.approval, ApprovalMode::AutoValidated);
        assert_eq!(
            serde_json::to_value(ApprovalMode::Manual).unwrap(),
            "manual"
        );
        // What was stored reads back the same.
        let back: AutopilotConfig =
            serde_json::from_value(serde_json::to_value(&c).unwrap()).unwrap();
        assert_eq!(back.estimate_request().roles, 2);
        assert_eq!(back.approval, ApprovalMode::AutoValidated);
        assert_eq!(back.roles[1].login_actions.len(), 1);
    }

    #[test]
    fn the_limits_are_the_ones_the_estimate_endpoint_always_had() {
        let mut request = config().estimate_request();
        assert!(request.check_limits().is_ok());
        request.roles = 0;
        assert_eq!(
            request.check_limits().unwrap_err(),
            "roles must be between 1 and 20"
        );
        request.roles = 21;
        assert!(request.check_limits().is_err());
        request.roles = 20;
        assert!(request.check_limits().is_ok(), "twenty is allowed");
        request.run_concurrency = 0;
        assert_eq!(
            request.check_limits().unwrap_err(),
            "run_concurrency must be between 1 and 8"
        );
        request.run_concurrency = 9;
        assert!(request.check_limits().is_err());
        request.run_concurrency = 8;
        assert!(request.check_limits().is_ok(), "eight is allowed");
        request.verify_gaps = Some(0);
        assert!(request
            .check_limits()
            .unwrap_err()
            .starts_with("verify_gaps must be between 1 and "));
        request.verify_gaps = Some(crate::gap_verify::MAX_VERIFICATIONS_LIMIT as u64 + 1);
        assert!(request.check_limits().is_err());
        request.verify_gaps = Some(crate::gap_verify::MAX_VERIFICATIONS_LIMIT as u64);
        assert!(request.check_limits().is_ok(), "the most is allowed");
    }

    #[test]
    fn a_config_that_names_no_address_or_a_long_label_or_too_much_is_refused_with_every_reason() {
        let mut c = config();
        c.base_url = "  ".to_string();
        c.name = Some("x".repeat(MAX_NAME_CHARS + 1));
        c.run_concurrency = Some(0);
        let problems = c.check().unwrap_err();
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(
            problems[0].contains("at most 80 characters"),
            "{problems:?}"
        );
        assert!(problems[1].starts_with("base_url"), "{problems:?}");
        assert!(problems[2].starts_with("run_concurrency"), "{problems:?}");
        // The longest label is fine, and spaces around it do not count.
        c = config();
        c.name = Some(format!(" {} ", "x".repeat(MAX_NAME_CHARS)));
        assert!(c.check().is_ok());
    }

    #[test]
    fn a_config_with_no_role_is_refused_once_and_a_config_with_too_many_is_refused_by_the_limit() {
        let mut c = config();
        c.roles.clear();
        let problems = c.check().unwrap_err();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].contains("at least one role") && problems[0].contains("anonymous"),
            "{problems:?}"
        );
        let role = |n: usize| crate::crawler::CrawlRole {
            name: format!("role{n}"),
            login_actions: Vec::new(),
        };
        c.roles = (0..MAX_ROLES as usize).map(role).collect();
        assert!(c.check().is_ok(), "twenty roles are allowed");
        c.roles = (0..=MAX_ROLES as usize).map(role).collect();
        assert_eq!(
            c.check().unwrap_err(),
            ["roles must be between 1 and 20".to_string()]
        );
    }

    #[test]
    fn a_run_is_named_by_its_label_or_by_when_it_was_made() {
        let at = chrono::DateTime::parse_from_rfc3339("2026-09-25T14:30:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let mut c = config();
        assert_eq!(c.display_name(at), "Autopilot 2026-09-25 14:30");
        c.name = Some("   ".to_string());
        assert_eq!(c.display_name(at), "Autopilot 2026-09-25 14:30");
        c.name = Some("  Release 4 smoke  ".to_string());
        assert_eq!(c.display_name(at), "Release 4 smoke");
    }

    #[test]
    fn a_run_starts_only_when_the_estimate_is_accepted_and_a_paid_one_only_when_that_is_accepted_too(
    ) {
        let free = serde_json::json!({"totals": {"paid": false}});
        let paid = serde_json::json!({"totals": {"paid": true}});
        assert_eq!(check_acceptance(&free, true, false), Ok(false));
        assert_eq!(check_acceptance(&free, true, true), Ok(false));
        assert!(check_acceptance(&free, false, false)
            .unwrap_err()
            .contains("accept_estimate: true"));
        assert!(
            check_acceptance(&free, false, true).is_err(),
            "accepting the cost is not accepting the estimate"
        );
        assert_eq!(check_acceptance(&paid, true, true), Ok(true));
        let refused = check_acceptance(&paid, true, false).unwrap_err();
        assert!(refused.contains("accept_paid: true"), "{refused}");
        assert!(check_acceptance(&paid, false, true).is_err());
        // An estimate that does not say is taken to cost money.
        let unclear = serde_json::json!({"totals": {}});
        assert!(check_acceptance(&unclear, true, false).is_err());
        assert_eq!(check_acceptance(&unclear, true, true), Ok(true));
        assert!(check_acceptance(&serde_json::json!(null), true, false).is_err());
    }
}
