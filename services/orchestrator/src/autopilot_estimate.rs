//! Phase 7B — what an autopilot run would cost, before it is started.
//!
//! The autopilot takes a project from its documents to a report: read the requirements, crawl the application, generate
//! cases, validate them, wait for a person to approve them, compile and run them. Most of that is free and quick; the AI
//! calls of generation are what take time (hours, on a local model) and, on a paid provider, money. So a person sees an
//! estimate first — AI calls, tokens, time and dollars per provider — and starts the run knowing what it is.
//!
//! This file is that arithmetic and nothing else: no provider, no store, no clock. The caller gathers what the estimate
//! rests on (how many calls the generation plan makes, which provider each task would go to and what it charges, what
//! earlier calls to it measured) and gets back ranges — a minimum and a maximum, never a single figure that looks exact —
//! with every assumption written down beside them:
//!
//! * a **measured** speed (the average of at least [`MIN_OBSERVED_CALLS`] earlier calls to that provider and model) is used
//!   as it is, spread by [`OBSERVED_SPREAD`]; without one, a speed **assumed** for that kind of provider is used and the
//!   estimate says so;
//! * a provider with no price is never given one: `Unmetered` costs `0`, a metered one costs what its two rates say, and a
//!   task nothing can serve has no cost figure at all (`None`), not `0`;
//! * the wait for a person to approve the cases is not counted (nobody can know it), and the estimate says so.

use serde::Serialize;

/// The fewest earlier calls an average of them is trusted for.
pub const MIN_OBSERVED_CALLS: u64 = 3;
/// A measured average is spread to `avg / 1.6 ..= avg * 1.8`: what one call took says what the next will, roughly.
pub const OBSERVED_SPREAD: (f64, f64) = (1.0 / 1.6, 1.8);

/// Extra generation calls a run may need because an answer could not be read (each call retries up to twice), as a share
/// of the calls the plan makes. An allowance, not a measurement.
pub const RETRY_ALLOWANCE: f64 = 0.25;

/// A crawl: seconds per page, the most pages one role reaches (`helpers/crawler.ts`) and the ten more a focused crawl may
/// take, and the seconds a role takes to set up (its login and first page) at least and at most.
pub const CRAWL_SECONDS_PER_PAGE: f64 = 2.5;
pub const CRAWL_PAGES_PER_ROLE: u64 = 20;
pub const CRAWL_FOCUS_EXTRA_PAGES: u64 = 10;
pub const CRAWL_ROLE_SETUP_SECONDS: (u64, u64) = (5, 20);

/// What one AI compile call carries and answers (a case and the crawl's real selectors, and a plan), and what one call of
/// the gap check's AI step does (eight requirements against a list of pages).
pub const COMPILE_TOKENS: (u64, u64) = (2_500, 600);
pub const VERIFY_TOKENS: (u64, u64) = (2_500, 500);
pub const VERIFY_REQUIREMENTS_PER_CALL: u64 = 8;

/// A browser run of one compiled case, at least and at most, in seconds.
pub const RUN_SECONDS_PER_CASE: (u64, u64) = (10, 45);
/// Validating a case is a structure check, not a run.
pub const VALIDATE_SECONDS_PER_CASE: f64 = 0.05;

/// The tokens a second a kind of provider is assumed to write when nothing has been measured, at least, typically and at
/// most, and the seconds each call takes besides writing (reading the prompt, the network).
const LOCAL_SPEED: (f64, f64, f64, u64) = (10.0, 20.0, 40.0, 8);
const API_SPEED: (f64, f64, f64, u64) = (50.0, 100.0, 250.0, 2);
const OTHER_SPEED: (f64, f64, f64, u64) = (20.0, 40.0, 100.0, 4);

/// A minimum and a maximum.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Range<T> {
    pub min: T,
    pub max: T,
}

impl<T: Copy> Range<T> {
    pub fn new(min: T, max: T) -> Self {
        Self { min, max }
    }
}

impl Range<u64> {
    fn zero() -> Self {
        Self::new(0, 0)
    }

    fn plus(self, other: Self) -> Self {
        Self::new(self.min + other.min, self.max + other.max)
    }
}

/// Which kind of provider serves a task, as far as its speed is guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProviderKind {
    /// Runs on the person's own hardware (Ollama): free, and slow.
    Local,
    /// A hosted service (Groq, Gemini).
    Api,
    /// Anything else (a custom or OpenAI-compatible endpoint).
    Other,
}

/// What a provider charges, per 1,000 tokens.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Pricing {
    Unmetered,
    Metered {
        input_usd_per_1k: f64,
        output_usd_per_1k: f64,
    },
}

/// What earlier calls of a task to this provider and model measured.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Observed {
    pub calls: u64,
    /// Average wall-clock time of one call, in milliseconds.
    pub avg_ms: f64,
    /// Average tokens one call carried and wrote, when the provider reported them.
    pub avg_prompt_tokens: Option<f64>,
    pub avg_completion_tokens: Option<f64>,
}

/// The provider and model a task would go to right now, what it charges and what has been measured of it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub model: String,
    pub kind: ProviderKind,
    pub pricing: Pricing,
    pub observed: Option<Observed>,
}

/// Where a task would go. `None`: nothing can serve it right now.
pub type Server = Option<ProviderProfile>;

/// Everything the estimate rests on.
#[derive(Debug, Clone)]
pub struct EstimateInput {
    /// The requirements the run is about.
    pub requirements: u64,
    /// The generation plan (`requirement_generation::plan_calls`): the calls of a first pass over every requirement and
    /// what they carry and write, roughly.
    pub first_pass_calls: u64,
    pub first_pass_prompt_tokens: u64,
    pub first_pass_completion_tokens: u64,
    /// Rounds of gap-filling calls after the first pass; `None` when the run does not ask for them.
    pub top_up_rounds: Option<u64>,
    /// The cases the depth asks for in all (each requirement's target per technique, added up); `0` when no depth is set.
    pub cases_target: u64,
    /// Roles the crawl logs in as, `anonymous` included, and whether it looks for the requirements' pages first.
    pub roles: u64,
    pub focus: bool,
    /// The most requirements the gap check's AI step is asked about; `None` when the run does not use it.
    pub verify: Option<u64>,
    /// Whether approved cases are compiled and run, and how many runs go at once.
    pub execute: bool,
    pub run_concurrency: u64,
    pub generation: Server,
    pub compile: Server,
    pub traceability: Server,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepEstimate {
    pub step: &'static str,
    pub ai_calls: Range<u64>,
    pub seconds: Range<u64>,
    /// What the step waits for or leaves out, when that matters.
    pub note: Option<String>,
}

/// How long one call of a task is taken to last.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "basis", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Latency {
    /// The average of `calls` earlier calls to this provider and model.
    Measured { calls: u64, avg_seconds: f64 },
    /// Nothing measured yet: what a provider of this kind is assumed to write in a second.
    Assumed { tokens_per_second: f64 },
    /// No provider serves the task.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskEstimate {
    /// The router task (`Settings -> task routing`) the calls go through.
    pub task: &'static str,
    pub used_for: &'static str,
    pub provider: Option<ProviderProfile>,
    pub calls: Range<u64>,
    pub prompt_tokens: Range<u64>,
    pub completion_tokens: Range<u64>,
    pub seconds: Range<u64>,
    /// `None` when no provider serves the task (a cost nobody can state), never a made-up `0`.
    pub cost_usd: Option<Range<f64>>,
    pub latency: Latency,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Totals {
    pub ai_calls: Range<u64>,
    /// Without the wait for a person to approve the cases.
    pub seconds: Range<u64>,
    /// `None` when a task with calls has no provider to price them.
    pub cost_usd: Option<Range<f64>>,
    /// Some provider that would serve a task charges (its highest cost is above zero).
    pub paid: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Estimate {
    pub requirements: u64,
    /// The cases a run would make: at least one for each requirement, at most what the depth asks for.
    pub cases: Range<u64>,
    pub steps: Vec<StepEstimate>,
    pub tasks: Vec<TaskEstimate>,
    pub totals: Totals,
    /// What the figures are built on, one line each, so none of them has to be taken on faith.
    pub assumptions: Vec<String>,
    /// What a person should read before starting.
    pub warnings: Vec<String>,
}

fn ceil_div(a: u64, b: u64) -> u64 {
    a.div_ceil(b.max(1))
}

fn secs(x: f64) -> u64 {
    x.max(0.0).round() as u64
}

/// The seconds one call may take at least and at most, its basis, and (when measured) the tokens it typically writes.
fn call_seconds(server: &Server, completion_tokens_per_call: f64) -> ((f64, f64), Latency) {
    let Some(profile) = server else {
        return ((0.0, 0.0), Latency::Unknown);
    };
    if let Some(o) = profile.observed.filter(|o| o.calls >= MIN_OBSERVED_CALLS) {
        let avg = o.avg_ms / 1000.0;
        return (
            (avg * OBSERVED_SPREAD.0, avg * OBSERVED_SPREAD.1),
            Latency::Measured {
                calls: o.calls,
                avg_seconds: (avg * 100.0).round() / 100.0,
            },
        );
    }
    let (slow, typical, fast, overhead) = match profile.kind {
        ProviderKind::Local => LOCAL_SPEED,
        ProviderKind::Api => API_SPEED,
        ProviderKind::Other => OTHER_SPEED,
    };
    let per = |speed: f64| overhead as f64 + completion_tokens_per_call / speed;
    (
        (per(fast), per(slow)),
        Latency::Assumed {
            tokens_per_second: typical,
        },
    )
}

/// The dollars for `prompt` tokens in and `completion` tokens out, or `None` when nothing serves the task.
fn cost(server: &Server, prompt: Range<u64>, completion: Range<u64>) -> Option<Range<f64>> {
    let profile = server.as_ref()?;
    Some(match profile.pricing {
        Pricing::Unmetered => Range::new(0.0, 0.0),
        Pricing::Metered {
            input_usd_per_1k,
            output_usd_per_1k,
        } => {
            let at = |p: u64, c: u64| {
                p as f64 / 1000.0 * input_usd_per_1k + c as f64 / 1000.0 * output_usd_per_1k
            };
            Range::new(
                round_cents(at(prompt.min, completion.min)),
                round_cents(at(prompt.max, completion.max)),
            )
        }
    })
}

fn round_cents(usd: f64) -> f64 {
    (usd * 10_000.0).round() / 10_000.0
}

/// One task's estimate: `calls` of it, each carrying about `prompt_per_call` tokens and writing about
/// `completion_per_call`.
fn task_estimate(
    task: &'static str,
    used_for: &'static str,
    server: &Server,
    calls: Range<u64>,
    prompt_per_call: f64,
    completion_per_call: f64,
) -> TaskEstimate {
    // A measured average of what a call carried and wrote is worth more than the plan's rough figure.
    let (prompt_per_call, completion_per_call) = match server.as_ref().and_then(|p| p.observed) {
        Some(o) if o.calls >= MIN_OBSERVED_CALLS => (
            o.avg_prompt_tokens.unwrap_or(prompt_per_call),
            o.avg_completion_tokens.unwrap_or(completion_per_call),
        ),
        _ => (prompt_per_call, completion_per_call),
    };
    let prompt = Range::new(
        (calls.min as f64 * prompt_per_call).round() as u64,
        (calls.max as f64 * prompt_per_call).round() as u64,
    );
    let completion = Range::new(
        (calls.min as f64 * completion_per_call).round() as u64,
        (calls.max as f64 * completion_per_call).round() as u64,
    );
    let ((fast, slow), latency) = call_seconds(server, completion_per_call);
    TaskEstimate {
        task,
        used_for,
        provider: server.clone(),
        calls,
        prompt_tokens: prompt,
        completion_tokens: completion,
        seconds: Range::new(secs(calls.min as f64 * fast), secs(calls.max as f64 * slow)),
        cost_usd: cost(server, prompt, completion),
        latency,
    }
}

/// The estimate of a run. Pure: the same input always gives the same figures.
pub fn estimate(input: &EstimateInput) -> Estimate {
    let mut assumptions = Vec::new();
    let mut warnings = Vec::new();

    // ---- cases
    let cases = Range::new(
        input.requirements,
        input.cases_target.max(input.requirements),
    );

    // ---- generation: the first pass, what may be retried, and the gap-filling rounds
    let retry_max = (input.first_pass_calls as f64 * RETRY_ALLOWANCE).ceil() as u64;
    let top_up_max = input
        .top_up_rounds
        .map_or(0, |rounds| input.first_pass_calls * rounds);
    let generation_calls = Range::new(
        input.first_pass_calls,
        input.first_pass_calls + retry_max + top_up_max,
    );
    let per_call = |total: u64| total as f64 / input.first_pass_calls.max(1) as f64;
    let generation = task_estimate(
        "TEST_CASE_GENERATION",
        "generating the cases",
        &input.generation,
        generation_calls,
        per_call(input.first_pass_prompt_tokens),
        per_call(input.first_pass_completion_tokens),
    );

    // ---- compile: the compiler tries rules first, so an AI call for a case is at most one
    let compile_calls = if input.execute {
        Range::new(0, cases.max)
    } else {
        Range::zero()
    };
    let compile = task_estimate(
        "TEST_CASE_COMPILATION",
        "compiling the cases that rules cannot",
        &input.compile,
        compile_calls,
        COMPILE_TOKENS.0 as f64,
        COMPILE_TOKENS.1 as f64,
    );

    // ---- the gap check's AI step
    let verify_calls = match input.verify {
        Some(max) => Range::new(0, ceil_div(max, VERIFY_REQUIREMENTS_PER_CALL)),
        None => Range::zero(),
    };
    let verify = task_estimate(
        "TRACEABILITY",
        "checking the requirements the words left unclear",
        &input.traceability,
        verify_calls,
        VERIFY_TOKENS.0 as f64,
        VERIFY_TOKENS.1 as f64,
    );

    // ---- steps, in the order they run
    let pages_max = CRAWL_PAGES_PER_ROLE
        + if input.focus {
            CRAWL_FOCUS_EXTRA_PAGES
        } else {
            0
        };
    let roles = input.roles.max(1);
    let crawl = Range::new(
        roles * CRAWL_ROLE_SETUP_SECONDS.0 + secs(roles as f64 * CRAWL_SECONDS_PER_PAGE),
        roles * CRAWL_ROLE_SETUP_SECONDS.1
            + secs((roles * pages_max) as f64 * CRAWL_SECONDS_PER_PAGE),
    );
    let concurrency = input.run_concurrency.max(1);
    let execution = if input.execute {
        Range::new(
            cases.min * RUN_SECONDS_PER_CASE.0 / concurrency,
            cases.max * RUN_SECONDS_PER_CASE.1 / concurrency,
        )
    } else {
        Range::zero()
    };
    let validation = Range::new(
        secs(cases.min as f64 * VALIDATE_SECONDS_PER_CASE),
        secs(cases.max as f64 * VALIDATE_SECONDS_PER_CASE),
    );
    let steps = vec![
        StepEstimate {
            step: "REQUIREMENTS",
            ai_calls: Range::zero(),
            seconds: Range::zero(),
            note: None,
        },
        StepEstimate {
            step: "CRAWL",
            ai_calls: Range::zero(),
            seconds: crawl,
            note: Some(format!("{roles} role(s), up to {pages_max} pages each")),
        },
        StepEstimate {
            step: "GAP_CHECK",
            ai_calls: verify.calls,
            seconds: verify.seconds,
            note: input
                .verify
                .is_none()
                .then(|| "words only: no AI call".to_string()),
        },
        StepEstimate {
            step: "GENERATE",
            ai_calls: generation.calls,
            seconds: generation.seconds,
            note: None,
        },
        StepEstimate {
            step: "VALIDATE",
            ai_calls: Range::zero(),
            seconds: validation,
            note: None,
        },
        StepEstimate {
            step: "APPROVE",
            ai_calls: Range::zero(),
            seconds: Range::zero(),
            note: Some("waits for a person; the time it takes is not counted".to_string()),
        },
        StepEstimate {
            step: "EXECUTE",
            ai_calls: compile.calls,
            seconds: execution.plus(compile.seconds),
            note: (!input.execute)
                .then(|| "not asked for: the cases are only generated".to_string()),
        },
        StepEstimate {
            step: "REPORT",
            ai_calls: Range::zero(),
            seconds: Range::zero(),
            note: None,
        },
    ];
    let tasks: Vec<TaskEstimate> = [generation, compile, verify]
        .into_iter()
        .filter(|t| t.calls.max > 0)
        .collect();

    // ---- totals
    let mut totals = Totals {
        ai_calls: Range::zero(),
        seconds: crawl.plus(validation).plus(execution),
        cost_usd: Some(Range::new(0.0, 0.0)),
        paid: false,
    };
    for t in &tasks {
        totals.ai_calls = totals.ai_calls.plus(t.calls);
        totals.seconds = totals.seconds.plus(t.seconds);
        totals.cost_usd = match (totals.cost_usd, t.cost_usd) {
            (Some(a), Some(b)) => Some(Range::new(
                round_cents(a.min + b.min),
                round_cents(a.max + b.max),
            )),
            _ => None,
        };
        if t.cost_usd.is_some_and(|c| c.max > 0.0) {
            totals.paid = true;
        }
    }

    // ---- what the figures are built on
    assumptions.push(format!(
        "Crawl: {CRAWL_SECONDS_PER_PAGE} s a page, up to {pages_max} pages a role, {}-{} s to set a role up.",
        CRAWL_ROLE_SETUP_SECONDS.0, CRAWL_ROLE_SETUP_SECONDS.1
    ));
    for t in &tasks {
        let who = t.provider.as_ref().map_or("no provider".to_string(), |p| {
            format!("{}/{}", p.name, p.model)
        });
        assumptions.push(match &t.latency {
            Latency::Measured { calls, avg_seconds } => format!(
                "{}: {avg_seconds} s a call on {who}, measured over {calls} earlier calls.",
                t.used_for
            ),
            Latency::Assumed { tokens_per_second } => format!(
                "{}: {who} is assumed to write {tokens_per_second} tokens a second (nothing has been measured yet, so this may be far off): the run records what it measures and the next estimate uses it.",
                t.used_for
            ),
            Latency::Unknown => format!("{}: no provider can serve this task right now.", t.used_for),
        });
    }
    assumptions.push(format!(
        "Generation: up to {}% more calls than the plan for answers that cannot be read{}.",
        (RETRY_ALLOWANCE * 100.0) as u64,
        match input.top_up_rounds {
            Some(rounds) => format!(", and up to {rounds} round(s) of gap-filling calls, each at most as many as the first pass"),
            None => String::new(),
        }
    ));
    if input.execute {
        assumptions.push(format!(
            "Running: {}-{} s a case, {concurrency} at a time; compiling makes at most one AI call a case (rules are tried first).",
            RUN_SECONDS_PER_CASE.0, RUN_SECONDS_PER_CASE.1
        ));
    }
    assumptions
        .push("Approval waits for a person: the time it takes is not in the total.".to_string());

    // ---- what to read before starting
    for t in &tasks {
        if t.provider.is_none() {
            warnings.push(format!(
                "No provider can serve {} right now: the run would stop at that step.",
                t.used_for
            ));
        }
    }
    if totals.paid {
        let names: Vec<String> = tasks
            .iter()
            .filter(|t| t.cost_usd.is_some_and(|c| c.max > 0.0))
            .filter_map(|t| t.provider.as_ref().map(|p| p.name.clone()))
            .collect();
        warnings.push(format!(
            "This run may cost up to ${:.2} on {}.",
            totals.cost_usd.map_or(0.0, |c| c.max),
            names.join(", ")
        ));
    }
    if input.cases_target == 0 {
        warnings.push(
            "No depth was chosen, so the number of cases is only guessed as one for each requirement.".to_string(),
        );
    }
    if input.requirements == 0 {
        warnings.push("The project has no requirements to generate from.".to_string());
    }

    Estimate {
        requirements: input.requirements,
        cases,
        steps,
        tasks,
        totals,
        assumptions,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local() -> ProviderProfile {
        ProviderProfile {
            id: "ollama".to_string(),
            name: "Ollama".to_string(),
            model: "qwen3:14b".to_string(),
            kind: ProviderKind::Local,
            pricing: Pricing::Unmetered,
            observed: None,
        }
    }

    fn paid() -> ProviderProfile {
        ProviderProfile {
            id: "custom-1".to_string(),
            name: "Paid AI".to_string(),
            model: "big-1".to_string(),
            kind: ProviderKind::Api,
            pricing: Pricing::Metered {
                input_usd_per_1k: 0.002,
                output_usd_per_1k: 0.01,
            },
            observed: None,
        }
    }

    fn input() -> EstimateInput {
        EstimateInput {
            requirements: 30,
            first_pass_calls: 10,
            first_pass_prompt_tokens: 40_000,
            first_pass_completion_tokens: 20_000,
            top_up_rounds: None,
            cases_target: 90,
            roles: 2,
            focus: false,
            verify: None,
            execute: true,
            run_concurrency: 2,
            generation: Some(local()),
            compile: Some(local()),
            traceability: Some(local()),
        }
    }

    fn task<'a>(e: &'a Estimate, name: &str) -> &'a TaskEstimate {
        e.tasks.iter().find(|t| t.task == name).unwrap()
    }

    fn step<'a>(e: &'a Estimate, name: &str) -> &'a StepEstimate {
        e.steps.iter().find(|s| s.step == name).unwrap()
    }

    #[test]
    fn the_generation_calls_are_the_plans_with_room_for_retries_and_the_top_up_rounds() {
        let e = estimate(&input());
        let g = task(&e, "TEST_CASE_GENERATION");
        // Ten calls; a quarter more (rounded up) for answers that cannot be read.
        assert_eq!(g.calls, Range::new(10, 13));
        let mut with = input();
        with.top_up_rounds = Some(2);
        let g = estimate(&with)
            .tasks
            .into_iter()
            .find(|t| t.task == "TEST_CASE_GENERATION")
            .unwrap();
        // Two rounds, each at most as many calls as the first pass.
        assert_eq!(g.calls, Range::new(10, 33));
        // Tokens follow the calls: 4,000 in and 2,000 out for one.
        assert_eq!(g.prompt_tokens, Range::new(40_000, 132_000));
        assert_eq!(g.completion_tokens, Range::new(20_000, 66_000));
    }

    #[test]
    fn a_plan_of_no_calls_costs_nothing_and_divides_by_nothing() {
        let mut none = input();
        none.first_pass_calls = 0;
        none.first_pass_prompt_tokens = 0;
        none.first_pass_completion_tokens = 0;
        let e = estimate(&none);
        assert!(
            e.tasks.iter().all(|t| t.task != "TEST_CASE_GENERATION"),
            "{:?}",
            e.tasks
        );
        assert_eq!(step(&e, "GENERATE").ai_calls, Range::zero());
    }

    #[test]
    fn a_provider_nothing_has_measured_is_given_an_assumed_speed_for_its_kind_and_the_estimate_says_so(
    ) {
        let e = estimate(&input());
        let g = task(&e, "TEST_CASE_GENERATION");
        assert_eq!(
            g.latency,
            Latency::Assumed {
                tokens_per_second: 20.0
            }
        );
        // 2,000 tokens written a call: 8 s and 2,000/40 s at least, 8 s and 2,000/10 s at most.
        assert_eq!(g.seconds, Range::new(10 * 58, 13 * 208));
        assert!(
            e.assumptions.iter().any(|a| a
                .contains("Ollama/qwen3:14b is assumed to write 20 tokens a second")
                && a.contains("nothing has been measured yet")),
            "{:?}",
            e.assumptions
        );
        // A hosted service is assumed faster, another endpoint in between.
        let mut api = input();
        api.generation = Some(paid());
        let g = task(&estimate(&api), "TEST_CASE_GENERATION").clone();
        assert_eq!(
            g.latency,
            Latency::Assumed {
                tokens_per_second: 100.0
            }
        );
        assert_eq!(g.seconds, Range::new(10 * 10, 13 * 42));
        let mut other = input();
        other.generation = Some(ProviderProfile {
            kind: ProviderKind::Other,
            ..local()
        });
        let g = task(&estimate(&other), "TEST_CASE_GENERATION").clone();
        assert_eq!(
            g.latency,
            Latency::Assumed {
                tokens_per_second: 40.0
            }
        );
        assert_eq!(g.seconds, Range::new(10 * 24, 13 * 104));
    }

    #[test]
    fn what_earlier_calls_measured_replaces_the_assumption_once_there_are_enough_of_them() {
        let observed = |calls| Observed {
            calls,
            avg_ms: 50_000.0,
            avg_prompt_tokens: Some(3_000.0),
            avg_completion_tokens: Some(1_000.0),
        };
        let mut i = input();
        i.generation = Some(ProviderProfile {
            observed: Some(observed(MIN_OBSERVED_CALLS)),
            ..local()
        });
        let e = estimate(&i);
        let g = task(&e, "TEST_CASE_GENERATION");
        assert_eq!(
            g.latency,
            Latency::Measured {
                calls: 3,
                avg_seconds: 50.0
            }
        );
        // 50 s spread to 31.25 s and 90 s a call.
        assert_eq!(g.seconds, Range::new(313, 1170));
        // What a call carried and wrote is the measured average, not the plan's.
        assert_eq!(g.prompt_tokens, Range::new(30_000, 39_000));
        assert_eq!(g.completion_tokens, Range::new(10_000, 13_000));
        assert!(
            e.assumptions
                .iter()
                .any(|a| a
                    .contains("50 s a call on Ollama/qwen3:14b, measured over 3 earlier calls")),
            "{:?}",
            e.assumptions
        );
        // Two calls are not an average.
        i.generation = Some(ProviderProfile {
            observed: Some(observed(MIN_OBSERVED_CALLS - 1)),
            ..local()
        });
        let g = task(&estimate(&i), "TEST_CASE_GENERATION").clone();
        assert_eq!(
            g.latency,
            Latency::Assumed {
                tokens_per_second: 20.0
            }
        );
        assert_eq!(g.completion_tokens, Range::new(20_000, 26_000));
        // A measured average that carries no token figures keeps the plan's tokens.
        i.generation = Some(ProviderProfile {
            observed: Some(Observed {
                avg_prompt_tokens: None,
                avg_completion_tokens: None,
                ..observed(5)
            }),
            ..local()
        });
        let g = task(&estimate(&i), "TEST_CASE_GENERATION").clone();
        assert_eq!(g.prompt_tokens, Range::new(40_000, 52_000));
        assert!(matches!(g.latency, Latency::Measured { calls: 5, .. }));
    }

    #[test]
    fn what_a_provider_charges_is_what_its_two_rates_say_and_an_unmetered_one_costs_nothing() {
        let e = estimate(&input());
        assert_eq!(
            task(&e, "TEST_CASE_GENERATION").cost_usd,
            Some(Range::new(0.0, 0.0))
        );
        assert!(!e.totals.paid);
        let mut i = input();
        i.generation = Some(paid());
        let e = estimate(&i);
        let g = task(&e, "TEST_CASE_GENERATION");
        // 40,000 in at $0.002 and 20,000 out at $0.01 a thousand: 0.08 + 0.20; at most 13 calls' worth.
        assert_eq!(g.cost_usd, Some(Range::new(0.28, 0.364)));
        assert!(e.totals.paid);
        assert_eq!(e.totals.cost_usd, Some(Range::new(0.28, 0.364)));
        assert!(
            e.warnings
                .iter()
                .any(|w| w.contains("may cost up to $0.36 on Paid AI")),
            "{:?}",
            e.warnings
        );
    }

    #[test]
    fn a_task_nothing_can_serve_has_no_cost_figure_and_the_run_is_warned() {
        let mut i = input();
        i.generation = None;
        let e = estimate(&i);
        let g = task(&e, "TEST_CASE_GENERATION");
        assert_eq!(g.cost_usd, None);
        assert_eq!(g.latency, Latency::Unknown);
        assert_eq!(g.provider, None);
        assert_eq!(g.seconds, Range::zero());
        assert_eq!(
            e.totals.cost_usd, None,
            "a total that includes a cost nobody can state does not pretend to be 0"
        );
        assert!(
            e.warnings
                .iter()
                .any(|w| w.contains("No provider can serve generating the cases")),
            "{:?}",
            e.warnings
        );
        assert!(e
            .assumptions
            .iter()
            .any(|a| a.contains("no provider can serve this task right now")));
    }

    #[test]
    fn the_crawl_takes_its_pages_per_role_and_more_when_it_looks_for_the_requirements_pages() {
        let e = estimate(&input());
        // Two roles: 5 s and 20 s to set each up; a page or 20 pages of 2.5 s (rounded per total).
        assert_eq!(
            step(&e, "CRAWL").seconds,
            Range::new(2 * 5 + 5, 2 * 20 + 100)
        );
        assert_eq!(
            step(&e, "CRAWL").note.as_deref(),
            Some("2 role(s), up to 20 pages each")
        );
        let mut focused = input();
        focused.focus = true;
        let e = estimate(&focused);
        assert_eq!(step(&e, "CRAWL").seconds, Range::new(15, 40 + 150));
        assert_eq!(
            step(&e, "CRAWL").note.as_deref(),
            Some("2 role(s), up to 30 pages each")
        );
        // No roles at all is still one (the anonymous one).
        let mut none = input();
        none.roles = 0;
        assert_eq!(
            step(&estimate(&none), "CRAWL").note.as_deref(),
            Some("1 role(s), up to 20 pages each")
        );
    }

    #[test]
    fn running_the_cases_takes_seconds_a_case_shared_between_the_runs_that_go_at_once() {
        let e = estimate(&input());
        // 30 cases at least, 90 at most, 10-45 s each, two at a time; the compile calls' own seconds come on top.
        let compile_seconds = task(&e, "TEST_CASE_COMPILATION").seconds;
        assert_eq!(
            step(&e, "EXECUTE").seconds,
            Range::new(30 * 10 / 2, 90 * 45 / 2).plus(compile_seconds)
        );
        let mut one = input();
        one.run_concurrency = 0;
        let e = estimate(&one);
        let compile_seconds = task(&e, "TEST_CASE_COMPILATION").seconds;
        assert_eq!(
            step(&e, "EXECUTE").seconds,
            Range::new(300, 90 * 45).plus(compile_seconds)
        );
        // Not asked for: no runs, no compile calls, and the step says why.
        let mut off = input();
        off.execute = false;
        let e = estimate(&off);
        assert_eq!(step(&e, "EXECUTE").seconds, Range::zero());
        assert_eq!(
            step(&e, "EXECUTE").note.as_deref(),
            Some("not asked for: the cases are only generated")
        );
        assert!(e.tasks.iter().all(|t| t.task != "TEST_CASE_COMPILATION"));
    }

    #[test]
    fn compiling_makes_at_most_one_ai_call_a_case_and_may_make_none() {
        let e = estimate(&input());
        let c = task(&e, "TEST_CASE_COMPILATION");
        assert_eq!(c.calls, Range::new(0, 90));
        assert_eq!(c.prompt_tokens, Range::new(0, 90 * 2_500));
        assert_eq!(c.completion_tokens, Range::new(0, 90 * 600));
    }

    #[test]
    fn the_gap_checks_ai_step_is_counted_only_when_asked_for_and_eight_requirements_go_in_a_call() {
        let e = estimate(&input());
        assert!(e.tasks.iter().all(|t| t.task != "TRACEABILITY"));
        assert_eq!(
            step(&e, "GAP_CHECK").note.as_deref(),
            Some("words only: no AI call")
        );
        assert_eq!(step(&e, "GAP_CHECK").seconds, Range::zero());
        let mut i = input();
        i.verify = Some(12);
        let e = estimate(&i);
        assert_eq!(task(&e, "TRACEABILITY").calls, Range::new(0, 2));
        assert_eq!(step(&e, "GAP_CHECK").ai_calls, Range::new(0, 2));
        assert_eq!(step(&e, "GAP_CHECK").note, None);
        i.verify = Some(8);
        assert_eq!(task(&estimate(&i), "TRACEABILITY").calls, Range::new(0, 1));
        i.verify = Some(9);
        assert_eq!(task(&estimate(&i), "TRACEABILITY").calls, Range::new(0, 2));
    }

    #[test]
    fn the_cases_are_at_least_one_a_requirement_and_at_most_what_the_depth_asks_for() {
        let e = estimate(&input());
        assert_eq!(e.cases, Range::new(30, 90));
        let mut no_depth = input();
        no_depth.cases_target = 0;
        let e = estimate(&no_depth);
        assert_eq!(e.cases, Range::new(30, 30));
        assert!(
            e.warnings.iter().any(|w| w.contains("No depth was chosen")),
            "{:?}",
            e.warnings
        );
        // A target below one a requirement cannot lower the least.
        let mut low = input();
        low.cases_target = 10;
        assert_eq!(estimate(&low).cases, Range::new(30, 30));
    }

    #[test]
    fn the_totals_add_the_steps_and_leave_out_the_wait_for_an_approval() {
        let e = estimate(&input());
        let sum: u64 = e.tasks.iter().map(|t| t.calls.max).sum();
        assert_eq!(e.totals.ai_calls.max, sum);
        assert_eq!(e.totals.ai_calls.min, 10);
        let by_steps: u64 = e.steps.iter().map(|s| s.seconds.max).sum();
        assert_eq!(
            e.totals.seconds.max, by_steps,
            "the steps' seconds add up to the total"
        );
        let by_steps: u64 = e.steps.iter().map(|s| s.seconds.min).sum();
        assert_eq!(e.totals.seconds.min, by_steps);
        assert_eq!(step(&e, "APPROVE").seconds, Range::zero());
        assert!(step(&e, "APPROVE")
            .note
            .as_deref()
            .unwrap()
            .contains("waits for a person"));
        assert!(e
            .assumptions
            .iter()
            .any(|a| a.contains("Approval waits for a person")));
        // Steps come in the order the run takes them.
        let order: Vec<&str> = e.steps.iter().map(|s| s.step).collect();
        assert_eq!(
            order,
            [
                "REQUIREMENTS",
                "CRAWL",
                "GAP_CHECK",
                "GENERATE",
                "VALIDATE",
                "APPROVE",
                "EXECUTE",
                "REPORT"
            ]
        );
    }

    #[test]
    fn a_project_with_no_requirements_is_warned_and_estimates_nothing_to_generate() {
        let mut none = input();
        none.requirements = 0;
        none.first_pass_calls = 0;
        none.first_pass_prompt_tokens = 0;
        none.first_pass_completion_tokens = 0;
        none.cases_target = 0;
        let e = estimate(&none);
        assert!(
            e.warnings.iter().any(|w| w.contains("no requirements")),
            "{:?}",
            e.warnings
        );
        assert_eq!(e.cases, Range::zero());
    }

    #[test]
    fn the_same_input_always_gives_the_same_estimate() {
        assert_eq!(estimate(&input()), estimate(&input()));
    }
}
