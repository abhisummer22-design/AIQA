//! What the AI calls came to, counted (`0023_ai_usage.sql`): how many calls each provider and model was asked, how many of
//! them failed, how long they took, how many tokens they used and what the priced ones cost, per project and per day.
//!
//! The router reports every call it makes to a provider, the ones that answer and the ones that do not, to a `UsageSink`;
//! `UsageLedger` adds each to the row of its quarter of an hour. Only counts are kept, never a prompt or an answer.
//! `build_report` reads them back for the usage page.
//!
//! A cost is only ever counted for a call that answered, went to a provider that has a price, and reported both its token
//! counts. Every other call is counted as one, and the report says how many there were of each kind rather than showing a
//! total that looks more certain than it is.

use crate::ai_router::{
    estimate_cost, AiTask, ExecutionError, ExecutionOutput, ProviderPricing, TokenUsage,
};
use crate::store::Store;
use anyhow::Result;
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use std::collections::BTreeMap;

// ------------------------------------------------------------------- one call

/// Bounded classification of error outcomes from an AI provider execution attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiErrorCategory {
    ProviderRejected,
    NonSuccessStatus,
    Timeout,
    ProviderUnavailable,
    ModelUnavailable,
    InvalidResponse,
}

impl AiErrorCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ProviderRejected => "provider_rejected",
            Self::NonSuccessStatus => "non_success_status",
            Self::Timeout => "timeout",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::ModelUnavailable => "model_unavailable",
            Self::InvalidResponse => "invalid_response",
        }
    }
}

/// One call the router made to a provider, as the router saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct AiCallEvent {
    pub provider: String,
    pub model: String,
    pub task: AiTask,
    /// How long the call took, whether it answered or not.
    pub took_ms: u64,
    pub answered: bool,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    /// What the provider charged when the call was made: `Unmetered` for a local model, and for a hosted one whose price
    /// nobody entered.
    pub pricing: ProviderPricing,
    /// HTTP status code from the provider, if genuinely available from the execution result.
    pub http_status: Option<u16>,
    /// Bounded category of error, if the call was not successful.
    pub error_category: Option<AiErrorCategory>,
    /// Correlation trace identifier propagating through the AI request lifecycle.
    pub trace_id: Option<String>,
}

impl AiCallEvent {
    pub fn of_call(
        provider: &str,
        model: &str,
        task: AiTask,
        took: std::time::Duration,
        result: &Result<ExecutionOutput, ExecutionError>,
        pricing: ProviderPricing,
    ) -> Self {
        let usage = result
            .as_ref()
            .map(|output| output.usage)
            .unwrap_or_default();
        let (http_status, error_category) = match result {
            Ok(_) => (Some(200), None),
            Err(ExecutionError::ProviderRejected { status, .. }) => {
                (Some(*status), Some(AiErrorCategory::ProviderRejected))
            }
            Err(ExecutionError::NonSuccessStatus(status)) => {
                (Some(*status), Some(AiErrorCategory::NonSuccessStatus))
            }
            Err(ExecutionError::Timeout) => (None, Some(AiErrorCategory::Timeout)),
            Err(ExecutionError::ProviderUnavailable(_)) => {
                (None, Some(AiErrorCategory::ProviderUnavailable))
            }
            Err(ExecutionError::ModelUnavailable(_)) => {
                (None, Some(AiErrorCategory::ModelUnavailable))
            }
            Err(ExecutionError::InvalidResponse(_)) => {
                (None, Some(AiErrorCategory::InvalidResponse))
            }
        };
        Self {
            provider: provider.to_string(),
            model: model.to_string(),
            task,
            took_ms: u64::try_from(took.as_millis()).unwrap_or(u64::MAX),
            answered: result.is_ok(),
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            pricing,
            http_status,
            error_category,
            trace_id: None,
        }
    }

    /// Attaches an optional correlation trace ID to this call event.
    pub fn with_trace_id(mut self, trace_id: Option<String>) -> Self {
        self.trace_id = trace_id;
        self
    }

    /// A call that answered, to a provider that has a price. What a provider charges for a call that failed, if anything,
    /// is not something it reports, so such a call is never given a cost.
    fn metered(&self) -> bool {
        self.answered && matches!(self.pricing, ProviderPricing::Metered { .. })
    }

    /// What the call cost, in millionths of a US dollar. Only a call to a provider with a price that answered and reported
    /// both its token counts has a cost; a price that is not a sum of money (a negative one, a number that is not one) gives
    /// none either.
    pub fn cost_micros(&self) -> Option<i64> {
        if !self.metered() {
            return None;
        }
        let usage = TokenUsage {
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
        };
        let usd = estimate_cost(&usage, self.pricing)?;
        (usd.is_finite() && usd >= 0.0).then(|| (usd * 1_000_000.0).round() as i64)
    }
}

/// Told of every call the router makes to a provider (`AiInfrastructure::set_usage_sink`).
#[async_trait::async_trait]
pub trait UsageSink: Send + Sync {
    /// Never fails: counting a call is not worth losing the call over.
    async fn record(&self, event: AiCallEvent);
}

/// The sink of the running app: each call is added to the row of its quarter of an hour in `ai_usage`, under the project
/// the request being served is about (none when it is about no project, or when the call is made outside a request).
pub struct UsageLedger {
    store: Store,
}

impl UsageLedger {
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

#[async_trait::async_trait]
impl UsageSink for UsageLedger {
    async fn record(&self, event: AiCallEvent) {
        let project = crate::auth::request_project().unwrap_or_default();
        tracing::info!(
            target: "ai_usage",
            trace_id = event.trace_id.as_deref().unwrap_or(""),
            provider = %event.provider,
            model = %event.model,
            task = %event.task.name(),
            answered = event.answered,
            took_ms = event.took_ms,
            "ai_call_recorded"
        );
        if let Err(e) = self
            .store
            .add_ai_usage(&slot_of(Utc::now()), &project, &event)
            .await
        {
            tracing::warn!("could not count an AI call: {e}");
        }
    }
}

// ---------------------------------------------------------------------- store

const SLOT_FORMAT: &str = "%Y-%m-%dT%H:%M";

/// The quarter of an hour (UTC) `at` falls in, like "2026-09-26T13:15".
pub(crate) fn slot_of(at: DateTime<Utc>) -> String {
    at.date_naive()
        .and_hms_opt(at.hour(), at.minute() / 15 * 15, 0)
        .expect("an hour, and a quarter of it")
        .format(SLOT_FORMAT)
        .to_string()
}

macro_rules! usage_columns {
    () => {
        "slot, project_id, provider, model, task, calls, failed_calls, answered_ms, failed_ms, token_calls, prompt_tokens, completion_tokens, metered_calls, priced_calls, cost_micros, http_429_calls, http_5xx_calls, timeout_calls"
    };
}

/// One row of `ai_usage`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AiUsageRow {
    pub slot: String,
    pub project_id: String,
    pub provider: String,
    pub model: String,
    pub task: String,
    pub calls: i64,
    pub failed_calls: i64,
    pub answered_ms: i64,
    pub failed_ms: i64,
    pub token_calls: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub metered_calls: i64,
    pub priced_calls: i64,
    pub cost_micros: i64,
    pub http_429_calls: i64,
    pub http_5xx_calls: i64,
    pub timeout_calls: i64,
}

impl AiUsageRow {
    fn from_row(r: &SqliteRow) -> Self {
        Self {
            slot: r.get("slot"),
            project_id: r.get("project_id"),
            provider: r.get("provider"),
            model: r.get("model"),
            task: r.get("task"),
            calls: r.get("calls"),
            failed_calls: r.get("failed_calls"),
            answered_ms: r.get("answered_ms"),
            failed_ms: r.get("failed_ms"),
            token_calls: r.get("token_calls"),
            prompt_tokens: r.get("prompt_tokens"),
            completion_tokens: r.get("completion_tokens"),
            metered_calls: r.get("metered_calls"),
            priced_calls: r.get("priced_calls"),
            cost_micros: r.get("cost_micros"),
            http_429_calls: r.try_get("http_429_calls").unwrap_or(0),
            http_5xx_calls: r.try_get("http_5xx_calls").unwrap_or(0),
            timeout_calls: r.try_get("timeout_calls").unwrap_or(0),
        }
    }
}

impl Store {
    /// Adds one call to the row of its `slot`, `project_id` (empty when the call was made for no project), provider, model
    /// and task.
    pub async fn add_ai_usage(
        &self,
        slot: &str,
        project_id: &str,
        event: &AiCallEvent,
    ) -> Result<()> {
        let took = i64::try_from(event.took_ms).unwrap_or(i64::MAX);
        let (answered_ms, failed_ms) = if event.answered { (took, 0) } else { (0, took) };
        let both_counts = event.prompt_tokens.is_some() && event.completion_tokens.is_some();
        let cost = event.cost_micros();
        let is_429 = i64::from(!event.answered && event.http_status == Some(429));
        let is_5xx = i64::from(
            !event.answered
                && event
                    .http_status
                    .map_or(false, |s| (500..=599).contains(&s)),
        );
        let is_timeout = i64::from(
            !event.answered && event.error_category == Some(AiErrorCategory::Timeout),
        );
        sqlx::query(
            "INSERT INTO ai_usage
                 (slot, project_id, provider, model, task, calls, failed_calls, answered_ms, failed_ms, token_calls,
                  prompt_tokens, completion_tokens, metered_calls, priced_calls, cost_micros,
                  http_429_calls, http_5xx_calls, timeout_calls)
             VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (slot, project_id, provider, model, task) DO UPDATE SET
                 calls = calls + 1,
                 failed_calls = failed_calls + excluded.failed_calls,
                 answered_ms = answered_ms + excluded.answered_ms,
                 failed_ms = failed_ms + excluded.failed_ms,
                 token_calls = token_calls + excluded.token_calls,
                 prompt_tokens = prompt_tokens + excluded.prompt_tokens,
                 completion_tokens = completion_tokens + excluded.completion_tokens,
                 metered_calls = metered_calls + excluded.metered_calls,
                 priced_calls = priced_calls + excluded.priced_calls,
                 cost_micros = cost_micros + excluded.cost_micros,
                 http_429_calls = http_429_calls + excluded.http_429_calls,
                 http_5xx_calls = http_5xx_calls + excluded.http_5xx_calls,
                 timeout_calls = timeout_calls + excluded.timeout_calls",
        )
        .bind(slot)
        .bind(project_id)
        .bind(&event.provider)
        .bind(&event.model)
        .bind(event.task.name())
        .bind(i64::from(!event.answered))
        .bind(answered_ms)
        .bind(failed_ms)
        .bind(i64::from(both_counts))
        .bind(i64::from(event.prompt_tokens.unwrap_or(0)))
        .bind(i64::from(event.completion_tokens.unwrap_or(0)))
        .bind(i64::from(event.metered()))
        .bind(i64::from(cost.is_some()))
        .bind(cost.unwrap_or(0))
        .bind(is_429)
        .bind(is_5xx)
        .bind(is_timeout)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The rows from `since_slot` on, oldest first: of one project when `project_id` is given, of every project (and of the
    /// calls made for none) when it is not.
    pub async fn ai_usage_since(
        &self,
        since_slot: &str,
        project_id: Option<&str>,
    ) -> Result<Vec<AiUsageRow>> {
        let rows = match project_id {
            Some(project) => sqlx::query(concat!(
                "SELECT ",
                usage_columns!(),
                " FROM ai_usage WHERE project_id = ? AND slot >= ? ORDER BY slot, provider, model, task"
            ))
            .bind(project)
            .bind(since_slot)
            .fetch_all(&self.pool)
            .await?,
            None => sqlx::query(concat!(
                "SELECT ",
                usage_columns!(),
                " FROM ai_usage WHERE slot >= ? ORDER BY slot, project_id, provider, model, task"
            ))
            .bind(since_slot)
            .fetch_all(&self.pool)
            .await?,
        };
        Ok(rows.iter().map(AiUsageRow::from_row).collect())
    }
}

// --------------------------------------------------------------------- report

pub const MAX_DAYS: u32 = 366;
/// Every time zone lies within fourteen hours of UTC.
pub const MAX_OFFSET_MINUTES: i32 = 14 * 60;

/// The days a report covers, on the clock of whoever asks for it: `days` calendar days, the last of them the one it is now.
/// The offset is the one the clock has now, for every day of the window (across a change of summer time a day or two are an
/// hour out).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub days: u32,
    pub tz_offset_minutes: i32,
    pub first_day: NaiveDate,
    pub today: NaiveDate,
}

impl Window {
    pub fn new(now: DateTime<Utc>, days: u32, tz_offset_minutes: i32) -> Self {
        let days = days.clamp(1, MAX_DAYS);
        let tz_offset_minutes = tz_offset_minutes.clamp(-MAX_OFFSET_MINUTES, MAX_OFFSET_MINUTES);
        let today = (now + Duration::minutes(i64::from(tz_offset_minutes))).date_naive();
        Self {
            days,
            tz_offset_minutes,
            first_day: today - Duration::days(i64::from(days) - 1),
            today,
        }
    }

    /// The first moment (UTC, as a slot) that falls on the first day: its midnight, on the asker's clock.
    pub fn since_slot(&self) -> String {
        let midnight = self
            .first_day
            .and_hms_opt(0, 0, 0)
            .expect("midnight exists");
        (midnight - Duration::minutes(i64::from(self.tz_offset_minutes)))
            .format(SLOT_FORMAT)
            .to_string()
    }

    /// The day, on the asker's clock, that a slot starts on; `None` for text that is not a slot.
    fn day_of(&self, slot: &str) -> Option<NaiveDate> {
        let start = NaiveDateTime::parse_from_str(slot, SLOT_FORMAT).ok()?;
        Some((start + Duration::minutes(i64::from(self.tz_offset_minutes))).date())
    }
}

/// The counts of some rows, added up.
#[derive(Debug, Default, Clone)]
struct Tally {
    calls: i64,
    failed: i64,
    answered_ms: i64,
    failed_ms: i64,
    token_calls: i64,
    prompt_tokens: i64,
    completion_tokens: i64,
    metered: i64,
    priced: i64,
    cost_micros: i64,
    http_429_calls: i64,
    http_5xx_calls: i64,
    timeout_calls: i64,
}

impl Tally {
    fn add(&mut self, row: &AiUsageRow) {
        self.calls += row.calls;
        self.failed += row.failed_calls;
        self.answered_ms += row.answered_ms;
        self.failed_ms += row.failed_ms;
        self.token_calls += row.token_calls;
        self.prompt_tokens += row.prompt_tokens;
        self.completion_tokens += row.completion_tokens;
        self.metered += row.metered_calls;
        self.priced += row.priced_calls;
        self.cost_micros += row.cost_micros;
        self.http_429_calls += row.http_429_calls;
        self.http_5xx_calls += row.http_5xx_calls;
        self.timeout_calls += row.timeout_calls;
    }

    fn answered(&self) -> i64 {
        (self.calls - self.failed).max(0)
    }

    /// Answered calls to a model that has no price: a local one, or a hosted one whose price nobody entered.
    fn without_price(&self) -> i64 {
        (self.answered() - self.metered).max(0)
    }

    /// Answered calls to a provider with a price that reported no token counts to price them by.
    fn without_tokens(&self) -> i64 {
        (self.metered - self.priced).max(0)
    }

    fn json(&self) -> Value {
        let answered = self.answered();
        let avg_answer_ms = if answered > 0 {
            json!(self.answered_ms / answered)
        } else {
            Value::Null
        };
        json!({
            "calls": self.calls,
            "answered": answered,
            "failed": self.failed,
            "prompt_tokens": self.prompt_tokens,
            "completion_tokens": self.completion_tokens,
            "token_calls": self.token_calls,
            "answered_ms": self.answered_ms,
            "failed_ms": self.failed_ms,
            "avg_answer_ms": avg_answer_ms,
            "priced_calls": self.priced,
            "no_price_calls": self.without_price(),
            "no_tokens_calls": self.without_tokens(),
            "cost_usd": self.cost_micros as f64 / 1_000_000.0,
            "http_429_calls": self.http_429_calls,
            "http_5xx_calls": self.http_5xx_calls,
            "timeout_calls": self.timeout_calls,
        })
    }

    /// `json()`, with one more field naming what the tally is of.
    fn keyed(&self, key: &str, value: Value) -> Value {
        let mut object = self.json();
        object[key] = value;
        object
    }
}

fn calls_text(n: i64) -> String {
    if n == 1 {
        "1 call".to_string()
    } else {
        format!("{n} calls")
    }
}

/// What to know before trusting the numbers, from the totals.
fn notes(total: &Tally) -> Vec<String> {
    let mut notes = Vec::new();
    let (without_price, without_tokens) = (total.without_price(), total.without_tokens());
    if without_price > 0 {
        notes.push(format!(
            "{} went to models with no price (a local model, or a hosted one whose price is not entered in Settings), \
             so no cost is counted for them.",
            calls_text(without_price)
        ));
    }
    if without_tokens > 0 {
        notes.push(format!(
            "{} to a provider with a price reported no token counts, so no cost is counted for {}.",
            calls_text(without_tokens),
            if without_tokens == 1 { "it" } else { "them" }
        ));
    }
    if total.failed > 0 {
        notes.push(format!(
            "{} did not answer; nothing is counted as {} cost.",
            calls_text(total.failed),
            if total.failed == 1 { "its" } else { "their" }
        ));
    }
    if total.priced > 0 {
        notes.push(
            "A price is a rate per provider (Groq and Gemini: the cheapest model's list rate), so a pricier model, a free \
             tier or a plan bills differently."
                .to_string(),
        );
    }
    notes
}

/// Tallies `rows` by `key_of`, the most-called first (ties in the order of the keys).
fn grouped<K: Ord>(rows: &[&AiUsageRow], key_of: impl Fn(&AiUsageRow) -> K) -> Vec<(K, Tally)> {
    let mut groups: BTreeMap<K, Tally> = BTreeMap::new();
    for row in rows {
        groups.entry(key_of(row)).or_default().add(row);
    }
    let mut groups: Vec<(K, Tally)> = groups.into_iter().collect();
    groups.sort_by_key(|g| std::cmp::Reverse(g.1.calls));
    groups
}

/// The usage page's numbers over `rows` (the rows from `window.since_slot()` on): totals, and the same by provider and
/// model, by task, by day (every day of the window, the quiet ones too) and, when asked for, by project. Rows that fall
/// outside the window are left out of every one of them, so that they all add up to the same totals.
pub fn build_report(rows: &[AiUsageRow], window: &Window, by_project: bool) -> Value {
    let rows: Vec<&AiUsageRow> = rows
        .iter()
        .filter(|row| {
            window
                .day_of(&row.slot)
                .is_some_and(|day| day >= window.first_day)
        })
        .collect();

    let mut total = Tally::default();
    for row in &rows {
        total.add(row);
    }

    let mut days: BTreeMap<NaiveDate, Tally> = (0..i64::from(window.days))
        .map(|n| (window.first_day + Duration::days(n), Tally::default()))
        .collect();
    for row in &rows {
        if let Some(day) = window.day_of(&row.slot) {
            days.entry(day.min(window.today)).or_default().add(row);
        }
    }

    let mut report = json!({
        "range": {
            "days": window.days,
            "from": window.first_day.to_string(),
            "to": window.today.to_string(),
            "tz_offset_minutes": window.tz_offset_minutes,
        },
        "totals": total.json(),
        "by_provider": grouped(&rows, |r| (r.provider.clone(), r.model.clone()))
            .iter()
            .map(|((provider, model), tally)| {
                let mut entry = tally.keyed("provider", json!(provider));
                entry["model"] = json!(model);
                entry
            })
            .collect::<Vec<_>>(),
        "by_task": grouped(&rows, |r| r.task.clone())
            .iter()
            .map(|(task, tally)| tally.keyed("task", json!(task)))
            .collect::<Vec<_>>(),
        "by_day": days
            .iter()
            .map(|(day, tally)| tally.keyed("day", json!(day.to_string())))
            .collect::<Vec<_>>(),
        "notes": notes(&total),
    });
    if by_project {
        report["by_project"] = grouped(&rows, |r| r.project_id.clone())
            .iter()
            .map(|(project, tally)| {
                let id = if project.is_empty() {
                    Value::Null
                } else {
                    json!(project)
                };
                tally.keyed("project_id", id)
            })
            .collect();
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, s).unwrap()
    }

    fn priced() -> ProviderPricing {
        ProviderPricing::Metered {
            input_usd_per_1k: 0.000_075,
            output_usd_per_1k: 0.0003,
        }
    }

    fn answered(
        prompt: Option<u32>,
        completion: Option<u32>,
    ) -> Result<ExecutionOutput, ExecutionError> {
        Ok(ExecutionOutput {
            text: "an answer nobody should keep".to_string(),
            usage: TokenUsage {
                prompt_tokens: prompt,
                completion_tokens: completion,
            },
        })
    }

    fn event(
        result: &Result<ExecutionOutput, ExecutionError>,
        pricing: ProviderPricing,
    ) -> AiCallEvent {
        AiCallEvent::of_call(
            "groq",
            "llama",
            AiTask::TestCaseGeneration,
            std::time::Duration::from_millis(1500),
            result,
            pricing,
        )
    }

    // ------------------------------------------------------------ one call

    #[test]
    fn an_answered_call_carries_what_the_provider_reported() {
        let e = event(&answered(Some(2000), Some(500)), priced());
        assert_eq!(
            e,
            AiCallEvent {
                provider: "groq".to_string(),
                model: "llama".to_string(),
                task: AiTask::TestCaseGeneration,
                took_ms: 1500,
                answered: true,
                prompt_tokens: Some(2000),
                completion_tokens: Some(500),
                pricing: priced(),
                http_status: Some(200),
                error_category: None,
                trace_id: None,
            }
        );
    }

    #[test]
    fn call_event_classifies_successful_call() {
        let e = event(&answered(Some(100), Some(50)), priced());
        assert!(e.answered);
        assert_eq!(e.http_status, Some(200));
        assert_eq!(e.error_category, None);
    }

    #[test]
    fn call_event_classifies_http_429_and_502_rejections_truthfully() {
        let e_429 = event(
            &Err(ExecutionError::ProviderRejected {
                status: 429,
                message: "rate limit".into(),
            }),
            priced(),
        );
        assert!(!e_429.answered);
        assert_eq!(e_429.http_status, Some(429));
        assert_eq!(
            e_429.error_category,
            Some(AiErrorCategory::ProviderRejected)
        );

        let e_502 = event(
            &Err(ExecutionError::ProviderRejected {
                status: 502,
                message: "bad gateway".into(),
            }),
            priced(),
        );
        assert!(!e_502.answered);
        assert_eq!(e_502.http_status, Some(502));
        assert_eq!(
            e_502.error_category,
            Some(AiErrorCategory::ProviderRejected)
        );
    }

    #[test]
    fn call_event_classifies_timeout_with_no_http_status() {
        let e = event(&Err(ExecutionError::Timeout), priced());
        assert!(!e.answered);
        assert_eq!(e.http_status, None);
        assert_eq!(e.error_category, Some(AiErrorCategory::Timeout));
    }

    #[test]
    fn call_event_classifies_provider_unavailable_with_no_http_status() {
        let e = event(
            &Err(ExecutionError::ProviderUnavailable("connect failed".into())),
            priced(),
        );
        assert!(!e.answered);
        assert_eq!(e.http_status, None);
        assert_eq!(
            e.error_category,
            Some(AiErrorCategory::ProviderUnavailable)
        );
    }

    #[test]
    fn call_event_classifies_invalid_response_without_inventing_200() {
        let e = event(
            &Err(ExecutionError::InvalidResponse("empty response".into())),
            priced(),
        );
        assert!(!e.answered);
        assert_eq!(
            e.http_status, None,
            "must not invent 200 when execution result only reports logical failure"
        );
        assert_eq!(e.error_category, Some(AiErrorCategory::InvalidResponse));
    }

    #[test]
    fn a_call_that_did_not_answer_has_no_tokens_and_no_cost_but_still_took_its_time() {
        let e = event(&Err(ExecutionError::Timeout), priced());
        assert!(!e.answered);
        assert_eq!((e.prompt_tokens, e.completion_tokens), (None, None));
        assert_eq!(e.took_ms, 1500);
        assert_eq!(
            e.cost_micros(),
            None,
            "a call that did not answer is not billed here"
        );
    }

    #[test]
    fn how_long_a_call_took_is_in_milliseconds_and_never_wraps() {
        let e = AiCallEvent::of_call(
            "p",
            "m",
            AiTask::GeneralAi,
            std::time::Duration::from_secs(u64::MAX),
            &answered(None, None),
            ProviderPricing::Unmetered,
        );
        assert_eq!(e.took_ms, u64::MAX);
    }

    #[test]
    fn only_an_answered_call_to_a_provider_with_a_price_and_token_counts_has_a_cost() {
        // 2000 tokens in at $0.000075 per 1,000 and 500 out at $0.0003 per 1,000 come to $0.0003: 300 millionths.
        assert_eq!(
            event(&answered(Some(2000), Some(500)), priced()).cost_micros(),
            Some(300)
        );
        // A cost that is not a whole number of millionths goes to the nearest one: 8.25 down, 9.75 up.
        assert_eq!(
            event(&answered(Some(110), Some(0)), priced()).cost_micros(),
            Some(8)
        );
        assert_eq!(
            event(&answered(Some(130), Some(0)), priced()).cost_micros(),
            Some(10)
        );
        assert_eq!(
            event(&answered(Some(2000), None), priced()).cost_micros(),
            None,
            "no completion count"
        );
        assert_eq!(
            event(&answered(None, Some(500)), priced()).cost_micros(),
            None,
            "no prompt count"
        );
        assert_eq!(
            event(&answered(Some(2000), Some(500)), ProviderPricing::Unmetered).cost_micros(),
            None,
            "a provider with no price is never given a cost, not even a zero"
        );
    }

    #[test]
    fn a_price_that_is_not_a_sum_of_money_gives_no_cost() {
        for (input, output) in [
            (-1.0, 0.0003),
            (0.0003, -1.0),
            (f64::NAN, 0.0),
            (0.0, f64::INFINITY),
        ] {
            let pricing = ProviderPricing::Metered {
                input_usd_per_1k: input,
                output_usd_per_1k: output,
            };
            assert_eq!(
                event(&answered(Some(1000), Some(1000)), pricing).cost_micros(),
                None,
                "{input} / {output}"
            );
        }
        let free = ProviderPricing::Metered {
            input_usd_per_1k: 0.0,
            output_usd_per_1k: 0.0,
        };
        assert_eq!(
            event(&answered(Some(1000), Some(1000)), free).cost_micros(),
            Some(0),
            "a price of nothing is a price: the call is priced, at nothing"
        );
    }

    // ------------------------------------------------------------- the slot

    #[test]
    fn a_slot_is_the_quarter_of_an_hour_a_moment_falls_in() {
        for (moment, slot) in [
            (at(2026, 9, 26, 13, 0, 0), "2026-09-26T13:00"),
            (at(2026, 9, 26, 13, 14, 59), "2026-09-26T13:00"),
            (at(2026, 9, 26, 13, 15, 0), "2026-09-26T13:15"),
            (at(2026, 9, 26, 13, 44, 59), "2026-09-26T13:30"),
            (at(2026, 9, 26, 13, 45, 0), "2026-09-26T13:45"),
            (at(2026, 9, 26, 23, 59, 59), "2026-09-26T23:45"),
            (at(2026, 1, 2, 0, 0, 0), "2026-01-02T00:00"),
        ] {
            assert_eq!(slot_of(moment), slot, "{moment}");
        }
    }

    // ---------------------------------------------------------------- store

    async fn store() -> (Store, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::connect(&dir.path().join("usage.sqlite3"))
            .await
            .unwrap();
        (store, dir)
    }

    fn call(
        provider: &str,
        model: &str,
        task: AiTask,
        result: Result<ExecutionOutput, ExecutionError>,
        ms: u64,
        pricing: ProviderPricing,
    ) -> AiCallEvent {
        AiCallEvent::of_call(
            provider,
            model,
            task,
            std::time::Duration::from_millis(ms),
            &result,
            pricing,
        )
    }

    #[tokio::test]
    async fn calls_of_the_same_moment_and_kind_add_up_in_one_row_and_the_rest_stay_apart() {
        let (store, _dir) = store().await;
        let slot = "2026-09-26T13:15";
        let task = AiTask::TestCaseGeneration;
        // Two answered calls and one that failed, all to the same provider and model, for the same project and moment.
        for event in [
            call(
                "groq",
                "llama",
                task,
                answered(Some(2000), Some(500)),
                1500,
                priced(),
            ),
            call(
                "groq",
                "llama",
                task,
                answered(Some(1000), Some(250)),
                2500,
                priced(),
            ),
            call(
                "groq",
                "llama",
                task,
                Err(ExecutionError::Timeout),
                30_000,
                priced(),
            ),
        ] {
            store.add_ai_usage(slot, "p1", &event).await.unwrap();
        }
        // Another project, another model, another task and another moment each get a row of their own.
        store
            .add_ai_usage(
                slot,
                "p2",
                &call(
                    "groq",
                    "llama",
                    task,
                    answered(Some(10), Some(10)),
                    100,
                    priced(),
                ),
            )
            .await
            .unwrap();
        store
            .add_ai_usage(
                slot,
                "p1",
                &call(
                    "groq",
                    "other",
                    task,
                    answered(Some(10), Some(10)),
                    100,
                    priced(),
                ),
            )
            .await
            .unwrap();
        store
            .add_ai_usage(
                slot,
                "p1",
                &call(
                    "groq",
                    "llama",
                    AiTask::GeneralAi,
                    answered(Some(10), Some(10)),
                    100,
                    priced(),
                ),
            )
            .await
            .unwrap();
        store
            .add_ai_usage(
                "2026-09-26T13:30",
                "p1",
                &call(
                    "groq",
                    "llama",
                    task,
                    answered(Some(10), Some(10)),
                    100,
                    priced(),
                ),
            )
            .await
            .unwrap();

        let rows = store
            .ai_usage_since("2026-09-26T13:15", Some("p1"))
            .await
            .unwrap();
        assert_eq!(rows.len(), 4, "{rows:#?}");
        let main = rows
            .iter()
            .find(|r| r.slot == slot && r.model == "llama" && r.task == "TEST_CASE_GENERATION")
            .expect("the row the three calls went into");
        assert_eq!(
            main,
            &AiUsageRow {
                slot: slot.to_string(),
                project_id: "p1".to_string(),
                provider: "groq".to_string(),
                model: "llama".to_string(),
                task: "TEST_CASE_GENERATION".to_string(),
                calls: 3,
                failed_calls: 1,
                answered_ms: 4000,
                failed_ms: 30_000,
                token_calls: 2,
                prompt_tokens: 3000,
                completion_tokens: 750,
                metered_calls: 2,
                priced_calls: 2,
                // 300 for the first call, 75 + 75 = 150 for the second.
                cost_micros: 450,
                http_429_calls: 0,
                http_5xx_calls: 0,
                timeout_calls: 1,
            }
        );
    }

    #[tokio::test]
    async fn a_call_that_reported_only_one_token_count_adds_it_but_is_not_a_call_with_tokens() {
        let (store, _dir) = store().await;
        store
            .add_ai_usage(
                "2026-09-26T13:15",
                "",
                &call(
                    "ollama",
                    "qwen",
                    AiTask::GeneralAi,
                    answered(Some(700), None),
                    100,
                    ProviderPricing::Unmetered,
                ),
            )
            .await
            .unwrap();
        let rows = store
            .ai_usage_since("2026-09-26T00:00", None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(
            (row.token_calls, row.prompt_tokens, row.completion_tokens),
            (0, 700, 0)
        );
        assert_eq!(
            (row.metered_calls, row.priced_calls, row.cost_micros),
            (0, 0, 0),
            "no price, no cost"
        );
        assert_eq!(row.project_id, "", "a call made for no project");
    }

    #[tokio::test]
    async fn a_call_to_a_provider_with_a_price_but_no_token_counts_is_metered_and_not_priced() {
        let (store, _dir) = store().await;
        store
            .add_ai_usage(
                "2026-09-26T13:15",
                "p1",
                &call(
                    "groq",
                    "llama",
                    AiTask::GeneralAi,
                    answered(None, None),
                    100,
                    priced(),
                ),
            )
            .await
            .unwrap();
        let rows = store
            .ai_usage_since("2026-09-26T00:00", None)
            .await
            .unwrap();
        assert_eq!(
            (
                rows[0].metered_calls,
                rows[0].priced_calls,
                rows[0].cost_micros
            ),
            (1, 0, 0)
        );
    }

    #[tokio::test]
    async fn reading_gives_the_rows_from_a_moment_on_oldest_first_of_one_project_or_of_all() {
        let (store, _dir) = store().await;
        let e = call(
            "ollama",
            "qwen",
            AiTask::GeneralAi,
            answered(Some(1), Some(1)),
            10,
            ProviderPricing::Unmetered,
        );
        for (slot, project) in [
            ("2026-09-26T13:30", "p2"),
            ("2026-09-26T13:00", "p1"),
            ("2026-09-26T12:45", "p1"),
            ("2026-09-26T13:15", "p1"),
            ("2026-09-26T13:15", ""),
        ] {
            store.add_ai_usage(slot, project, &e).await.unwrap();
        }
        // The latest moment has the provider that sorts first: oldest first is by the moment, not by the name.
        let first_by_name = call(
            "aaa",
            "m",
            AiTask::GeneralAi,
            answered(Some(1), Some(1)),
            10,
            ProviderPricing::Unmetered,
        );
        store
            .add_ai_usage("2026-09-26T13:30", "p1", &first_by_name)
            .await
            .unwrap();
        let of = |rows: &[AiUsageRow]| {
            rows.iter()
                .map(|r| format!("{} {}", r.slot, r.project_id))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            of(&store.ai_usage_since("2026-09-26T13:00", None).await.unwrap()),
            [
                "2026-09-26T13:00 p1",
                "2026-09-26T13:15 ",
                "2026-09-26T13:15 p1",
                "2026-09-26T13:30 p1",
                "2026-09-26T13:30 p2"
            ],
            "the moment itself is included, an earlier one is not, and a project's rows and the rows of no project are all here"
        );
        assert_eq!(
            of(&store
                .ai_usage_since("2026-09-26T13:00", Some("p1"))
                .await
                .unwrap()),
            [
                "2026-09-26T13:00 p1",
                "2026-09-26T13:15 p1",
                "2026-09-26T13:30 p1"
            ]
        );
        assert!(store
            .ai_usage_since("2026-09-26T13:45", None)
            .await
            .unwrap()
            .is_empty());
    }

    // -------------------------------------------------------------- window

    #[test]
    fn a_window_ends_with_the_day_it_is_now_on_the_askers_clock() {
        // 19:00 UTC on the 26th is half past midnight on the 27th in India (UTC+5:30).
        let w = Window::new(at(2026, 9, 26, 19, 0, 0), 7, 330);
        assert_eq!(w.today.to_string(), "2026-09-27");
        assert_eq!(w.first_day.to_string(), "2026-09-21");
        assert_eq!(
            w.since_slot(),
            "2026-09-20T18:30",
            "the midnight it starts at, in UTC"
        );
        // 03:00 UTC on the 26th is 22:00 on the 25th five hours behind UTC.
        let w = Window::new(at(2026, 9, 26, 3, 0, 0), 1, -300);
        assert_eq!(
            (w.first_day.to_string(), w.today.to_string()),
            ("2026-09-25".to_string(), "2026-09-25".to_string())
        );
        assert_eq!(w.since_slot(), "2026-09-25T05:00");
        // At UTC itself nothing moves.
        let w = Window::new(at(2026, 9, 26, 0, 0, 0), 2, 0);
        assert_eq!(
            (w.first_day.to_string(), w.since_slot()),
            ("2026-09-25".to_string(), "2026-09-25T00:00".to_string())
        );
    }

    #[test]
    fn a_window_keeps_to_a_year_and_to_the_zones_there_are() {
        let now = at(2026, 9, 26, 12, 0, 0);
        assert_eq!(Window::new(now, 0, 0).days, 1);
        assert_eq!(Window::new(now, 1, 0).days, 1);
        assert_eq!(Window::new(now, 366, 0).days, 366);
        assert_eq!(Window::new(now, 367, 0).days, 366);
        assert_eq!(Window::new(now, 30, 99_999).tz_offset_minutes, 840);
        assert_eq!(Window::new(now, 30, 840).tz_offset_minutes, 840);
        assert_eq!(Window::new(now, 30, -99_999).tz_offset_minutes, -840);
        assert_eq!(Window::new(now, 30, -840).tz_offset_minutes, -840);
        assert_eq!(Window::new(now, 30, 330).tz_offset_minutes, 330);
    }

    #[test]
    fn a_slot_falls_on_the_day_it_starts_on_the_askers_clock() {
        let w = Window::new(at(2026, 9, 26, 19, 0, 0), 7, 330);
        // Half past six in the evening UTC is midnight in India: the slot that starts then is the new day's.
        assert_eq!(
            w.day_of("2026-09-26T18:15").map(|d| d.to_string()),
            Some("2026-09-26".to_string())
        );
        assert_eq!(
            w.day_of("2026-09-26T18:30").map(|d| d.to_string()),
            Some("2026-09-27".to_string())
        );
        assert_eq!(w.day_of("not a slot"), None);
        assert_eq!(w.day_of("2026-09-26"), None);
    }

    // -------------------------------------------------------------- report

    fn row(slot: &str, project: &str, provider: &str, model: &str, task: &str) -> AiUsageRow {
        AiUsageRow {
            slot: slot.to_string(),
            project_id: project.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            task: task.to_string(),
            ..Default::default()
        }
    }

    /// Three days ending on the 26th in India, and rows on each of them, on the midnight between two, and outside.
    fn week() -> (Window, Vec<AiUsageRow>) {
        let window = Window::new(at(2026, 9, 26, 12, 0, 0), 3, 330);
        assert_eq!(window.since_slot(), "2026-09-23T18:30");
        let rows = vec![
            // Before the window: half an hour before its midnight.
            AiUsageRow {
                calls: 100,
                ..row("2026-09-23T18:15", "p1", "groq", "llama", "GENERAL_AI")
            },
            // The window's first moment: midnight on the 24th.
            AiUsageRow {
                calls: 1,
                answered_ms: 1000,
                ..row(
                    "2026-09-23T18:30",
                    "",
                    "ollama",
                    "qwen",
                    "TEST_CASE_COMPILATION",
                )
            },
            AiUsageRow {
                calls: 4,
                failed_calls: 1,
                answered_ms: 6000,
                failed_ms: 30_000,
                token_calls: 3,
                prompt_tokens: 6000,
                completion_tokens: 1500,
                metered_calls: 3,
                priced_calls: 3,
                cost_micros: 900,
                ..row(
                    "2026-09-24T03:00",
                    "p1",
                    "groq",
                    "llama",
                    "TEST_CASE_GENERATION",
                )
            },
            // A quarter of an hour before midnight on the 26th, and the moment of it.
            AiUsageRow {
                calls: 2,
                answered_ms: 2000,
                token_calls: 1,
                prompt_tokens: 500,
                completion_tokens: 150,
                metered_calls: 2,
                priced_calls: 1,
                cost_micros: 100,
                ..row(
                    "2026-09-25T18:15",
                    "p1",
                    "groq",
                    "llama",
                    "TEST_CASE_GENERATION",
                )
            },
            AiUsageRow {
                calls: 5,
                answered_ms: 50_000,
                token_calls: 5,
                prompt_tokens: 5000,
                completion_tokens: 2000,
                ..row("2026-09-25T18:30", "p2", "ollama", "qwen", "GENERAL_AI")
            },
        ];
        (window, rows)
    }

    #[test]
    fn the_report_adds_up_what_is_in_the_window_and_leaves_out_what_is_not() {
        let (window, rows) = week();
        let report = build_report(&rows, &window, true);
        assert_eq!(
            report["range"],
            json!({"days": 3, "from": "2026-09-24", "to": "2026-09-26", "tz_offset_minutes": 330})
        );
        let totals = &report["totals"];
        assert_eq!(
            totals["calls"], 12,
            "the 100 calls before the window are not counted"
        );
        assert_eq!(totals["failed"], 1);
        assert_eq!(totals["answered"], 11);
        assert_eq!(totals["prompt_tokens"], 11_500);
        assert_eq!(totals["completion_tokens"], 3650);
        assert_eq!(totals["token_calls"], 9);
        assert_eq!(totals["answered_ms"], 59_000);
        assert_eq!(totals["failed_ms"], 30_000);
        assert_eq!(
            totals["avg_answer_ms"], 5363,
            "59,000 ms over 11 answers, rounded down"
        );
        assert_eq!(totals["priced_calls"], 4);
        assert_eq!(
            totals["no_price_calls"], 6,
            "11 answered, 5 to a provider with a price"
        );
        assert_eq!(
            totals["no_tokens_calls"], 1,
            "5 to a provider with a price, 4 of them priced"
        );
        assert_eq!(totals["cost_usd"], 0.001, "1,000 millionths of a dollar");
    }

    #[test]
    fn the_report_groups_by_provider_task_and_day_the_most_called_first() {
        let (window, rows) = week();
        let report = build_report(&rows, &window, false);
        let names = |key: &str, of: &[&str]| -> Vec<Vec<String>> {
            report[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| {
                    of.iter()
                        .map(|f| e[*f].as_str().unwrap().to_string())
                        .collect()
                })
                .collect()
        };
        // Six calls each: the tie is broken by name.
        assert_eq!(
            names("by_provider", &["provider", "model"]),
            [["groq", "llama"], ["ollama", "qwen"]]
        );
        let calls = |key: &str| -> Vec<i64> {
            report[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["calls"].as_i64().unwrap())
                .collect()
        };
        assert_eq!(calls("by_provider"), [6, 6]);
        assert_eq!(
            names("by_task", &["task"]),
            [
                ["TEST_CASE_GENERATION"],
                ["GENERAL_AI"],
                ["TEST_CASE_COMPILATION"]
            ]
        );
        assert_eq!(calls("by_task"), [6, 5, 1]);
        // Every day of the window, oldest first: the midnight between the 25th and the 26th belongs to the 26th.
        assert_eq!(
            names("by_day", &["day"]),
            [["2026-09-24"], ["2026-09-25"], ["2026-09-26"]]
        );
        assert_eq!(calls("by_day"), [5, 2, 5]);
        assert!(report.get("by_project").is_none(), "not asked for");
    }

    #[test]
    fn the_groups_of_a_report_each_carry_the_counts_of_their_own_rows() {
        let (window, rows) = week();
        let report = build_report(&rows, &window, true);
        let groq = &report["by_provider"][0];
        assert_eq!(
            (
                groq["calls"].clone(),
                groq["failed"].clone(),
                groq["cost_usd"].clone(),
                groq["priced_calls"].clone()
            ),
            (json!(6), json!(1), json!(0.001), json!(4))
        );
        assert_eq!(groq["no_tokens_calls"], 1);
        assert_eq!(groq["no_price_calls"], 0);
        let ollama = &report["by_provider"][1];
        assert_eq!(
            (
                ollama["calls"].clone(),
                ollama["cost_usd"].clone(),
                ollama["no_price_calls"].clone()
            ),
            (json!(6), json!(0.0), json!(6))
        );
        assert_eq!(ollama["avg_answer_ms"], 8500, "51,000 ms over 6 answers");
        // By project: the most called first, and the calls made for no project last, under a `null` id.
        let projects: Vec<(Value, i64)> = report["by_project"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["project_id"].clone(), e["calls"].as_i64().unwrap()))
            .collect();
        assert_eq!(
            projects,
            [(json!("p1"), 6), (json!("p2"), 5), (Value::Null, 1)]
        );
        // By day: the first day has the two rows of the 24th, the last the row of the midnight.
        assert_eq!(report["by_day"][0]["cost_usd"], 0.0009);
        assert_eq!(report["by_day"][1]["cost_usd"], 0.0001);
        assert_eq!(report["by_day"][2]["cost_usd"], 0.0);
        assert_eq!(report["by_day"][0]["prompt_tokens"], 6000);
    }

    #[test]
    fn every_grouping_adds_up_to_the_totals() {
        let (window, rows) = week();
        let report = build_report(&rows, &window, true);
        for key in ["by_provider", "by_task", "by_day", "by_project"] {
            for field in [
                "calls",
                "failed",
                "prompt_tokens",
                "completion_tokens",
                "priced_calls",
                "answered_ms",
            ] {
                let sum: i64 = report[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| e[field].as_i64().unwrap())
                    .sum();
                assert_eq!(json!(sum), report["totals"][field], "{key}.{field}");
            }
            let cost: f64 = report[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["cost_usd"].as_f64().unwrap())
                .sum();
            assert!(
                (cost - report["totals"]["cost_usd"].as_f64().unwrap()).abs() < 1e-9,
                "{key} cost"
            );
        }
    }

    #[test]
    fn a_row_dated_after_now_counts_on_the_last_day_so_the_days_still_add_up() {
        let window = Window::new(at(2026, 9, 26, 12, 0, 0), 2, 0);
        let rows = vec![
            AiUsageRow {
                calls: 1,
                ..row("2026-09-26T09:00", "", "p", "m", "T")
            },
            // A clock that was once set back leaves rows in the future.
            AiUsageRow {
                calls: 2,
                ..row("2026-09-28T09:00", "", "p", "m", "T")
            },
        ];
        let report = build_report(&rows, &window, false);
        assert_eq!(report["totals"]["calls"], 3);
        assert_eq!(report["by_day"][0]["calls"], 0);
        assert_eq!(report["by_day"][1]["calls"], 3);
        assert_eq!(report["by_day"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_row_that_is_not_dated_like_a_slot_is_left_out() {
        let window = Window::new(at(2026, 9, 26, 12, 0, 0), 2, 0);
        let rows = vec![AiUsageRow {
            calls: 7,
            ..row("yesterday", "", "p", "m", "T")
        }];
        assert_eq!(build_report(&rows, &window, false)["totals"]["calls"], 0);
    }

    #[test]
    fn rows_that_do_not_add_up_never_give_a_negative_count() {
        // More failures than calls, more calls to a provider with a price than calls that answered, more calls priced than
        // calls to a provider with a price: no row of the table says so, but a count below nothing is never shown.
        let window = Window::new(at(2026, 9, 26, 12, 0, 0), 2, 0);
        let rows = vec![AiUsageRow {
            calls: 1,
            failed_calls: 3,
            metered_calls: 5,
            priced_calls: 7,
            ..row("2026-09-26T09:00", "", "p", "m", "T")
        }];
        let totals = build_report(&rows, &window, false)["totals"].clone();
        assert_eq!(totals["calls"], 1);
        assert_eq!(totals["failed"], 3);
        assert_eq!(totals["answered"], 0);
        assert_eq!(totals["no_price_calls"], 0);
        assert_eq!(totals["no_tokens_calls"], 0);
        assert_eq!(totals["priced_calls"], 7);
    }

    #[test]
    fn a_report_of_nothing_has_every_day_at_zero_and_no_notes() {
        let window = Window::new(at(2026, 9, 26, 12, 0, 0), 3, 0);
        let report = build_report(&[], &window, true);
        assert_eq!(report["totals"]["calls"], 0);
        assert_eq!(
            report["totals"]["avg_answer_ms"],
            Value::Null,
            "an average of nothing is not zero"
        );
        assert_eq!(report["totals"]["cost_usd"], 0.0);
        assert_eq!(report["by_provider"], json!([]));
        assert_eq!(report["by_task"], json!([]));
        assert_eq!(report["by_project"], json!([]));
        assert_eq!(report["notes"], json!([]));
        let days: Vec<&str> = report["by_day"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["day"].as_str().unwrap())
            .collect();
        assert_eq!(days, ["2026-09-24", "2026-09-25", "2026-09-26"]);
    }

    #[test]
    fn the_notes_say_what_is_not_counted_and_only_when_there_is_something_not_counted() {
        let (window, rows) = week();
        let notes: Vec<String> = build_report(&rows, &window, false)["notes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            notes,
            [
                "6 calls went to models with no price (a local model, or a hosted one whose price is not entered in Settings), so no cost is counted for them.",
                "1 call to a provider with a price reported no token counts, so no cost is counted for it.",
                "1 call did not answer; nothing is counted as its cost.",
                "A price is a rate per provider (Groq and Gemini: the cheapest model's list rate), so a pricier model, a free tier or a plan bills differently.",
            ]
        );
        // Two of each, and nothing priced: the wording follows the number, and the last note has nothing to say.
        let rows = vec![AiUsageRow {
            calls: 4,
            failed_calls: 2,
            metered_calls: 2,
            ..row("2026-09-25T09:00", "", "groq", "llama", "GENERAL_AI")
        }];
        let notes: Vec<String> =
            build_report(&rows, &Window::new(at(2026, 9, 26, 12, 0, 0), 3, 0), false)["notes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_str().unwrap().to_string())
                .collect();
        assert_eq!(
            notes,
            [
                "2 calls to a provider with a price reported no token counts, so no cost is counted for them.",
                "2 calls did not answer; nothing is counted as their cost.",
            ]
        );
    }

    #[tokio::test]
    async fn error_rollups_429_5xx_timeout_success_and_upsert_isolation() {
        let (store, _dir) = store().await;
        let slot = "2026-09-26T14:00";
        let task = AiTask::TestCaseGeneration;

        // 1. Success call
        let success_event = call(
            "groq",
            "llama",
            task,
            answered(Some(100), Some(50)),
            250,
            priced(),
        );
        store.add_ai_usage(slot, "p1", &success_event).await.unwrap();

        let rows = store.ai_usage_since(slot, Some("p1")).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].calls, 1);
        assert_eq!(rows[0].failed_calls, 0);
        assert_eq!(rows[0].http_429_calls, 0);
        assert_eq!(rows[0].http_5xx_calls, 0);
        assert_eq!(rows[0].timeout_calls, 0);

        // 2. HTTP 429 Rate Limit (ProviderRejected)
        let rate_limit_event = call(
            "groq",
            "llama",
            task,
            Err(ExecutionError::ProviderRejected {
                status: 429,
                message: "rate limit".to_string(),
            }),
            100,
            priced(),
        );
        store.add_ai_usage(slot, "p1", &rate_limit_event).await.unwrap();

        let rows = store.ai_usage_since(slot, Some("p1")).await.unwrap();
        assert_eq!(rows[0].calls, 2);
        assert_eq!(rows[0].failed_calls, 1);
        assert_eq!(rows[0].http_429_calls, 1, "429 must increment exactly once");
        assert_eq!(rows[0].http_5xx_calls, 0, "429 must not increment 5xx");
        assert_eq!(rows[0].timeout_calls, 0, "429 must not increment timeout");

        // 3. HTTP 503 Server Error
        let server_error_event = call(
            "groq",
            "llama",
            task,
            Err(ExecutionError::NonSuccessStatus(503)),
            200,
            priced(),
        );
        store.add_ai_usage(slot, "p1", &server_error_event).await.unwrap();

        // 4. HTTP 500 Server Error
        let internal_error_event = call(
            "groq",
            "llama",
            task,
            Err(ExecutionError::NonSuccessStatus(500)),
            200,
            priced(),
        );
        store.add_ai_usage(slot, "p1", &internal_error_event).await.unwrap();

        let rows = store.ai_usage_since(slot, Some("p1")).await.unwrap();
        assert_eq!(rows[0].calls, 4);
        assert_eq!(rows[0].failed_calls, 3);
        assert_eq!(rows[0].http_429_calls, 1);
        assert_eq!(rows[0].http_5xx_calls, 2, "503 and 500 must both increment 5xx");
        assert_eq!(rows[0].timeout_calls, 0);

        // 5. Timeout
        let timeout_event = call(
            "groq",
            "llama",
            task,
            Err(ExecutionError::Timeout),
            30_000,
            priced(),
        );
        store.add_ai_usage(slot, "p1", &timeout_event).await.unwrap();

        let rows = store.ai_usage_since(slot, Some("p1")).await.unwrap();
        assert_eq!(rows[0].calls, 5);
        assert_eq!(rows[0].failed_calls, 4);
        assert_eq!(rows[0].http_429_calls, 1);
        assert_eq!(rows[0].http_5xx_calls, 2);
        assert_eq!(rows[0].timeout_calls, 1, "timeout must increment exactly once");

        // 6. Non-HTTP error (e.g. InvalidResponse)
        let invalid_resp_event = call(
            "groq",
            "llama",
            task,
            Err(ExecutionError::InvalidResponse("corrupt".to_string())),
            50,
            priced(),
        );
        store.add_ai_usage(slot, "p1", &invalid_resp_event).await.unwrap();

        let rows = store.ai_usage_since(slot, Some("p1")).await.unwrap();
        assert_eq!(rows[0].calls, 6);
        assert_eq!(rows[0].failed_calls, 5);
        assert_eq!(rows[0].http_429_calls, 1);
        assert_eq!(rows[0].http_5xx_calls, 2);
        assert_eq!(rows[0].timeout_calls, 1);

        // 7. Separate slot isolation
        let other_slot = "2026-09-26T14:15";
        store.add_ai_usage(other_slot, "p1", &timeout_event).await.unwrap();

        let rows = store.ai_usage_since(slot, Some("p1")).await.unwrap();
        assert_eq!(rows.len(), 2, "separate slot must produce separate row");
        let other_row = rows.iter().find(|r| r.slot == other_slot).unwrap();
        assert_eq!(other_row.calls, 1);
        assert_eq!(other_row.timeout_calls, 1);
        assert_eq!(other_row.http_429_calls, 0);
        assert_eq!(other_row.http_5xx_calls, 0);
    }

    #[tokio::test]
    async fn error_rollups_build_report_aggregation_and_backward_compatibility() {
        let (window, mut rows) = week();
        // Add error counters to some rows
        rows[1].http_429_calls = 3;
        rows[1].http_5xx_calls = 2;
        rows[1].timeout_calls = 1;

        let report = build_report(&rows, &window, true);

        // Totals must include additive error rollups
        assert_eq!(report["totals"]["http_429_calls"], 3);
        assert_eq!(report["totals"]["http_5xx_calls"], 2);
        assert_eq!(report["totals"]["timeout_calls"], 1);

        // Standard fields must remain identical
        assert_eq!(report["totals"]["calls"], 12);
        assert_eq!(report["totals"]["answered"], 11);
        assert_eq!(report["totals"]["failed"], 1);

        // Providers, tasks, and projects must also carry the rollup fields
        assert!(report["by_provider"][0].get("http_429_calls").is_some());
        assert!(report["by_task"][0].get("http_5xx_calls").is_some());
        assert!(report["by_day"][0].get("timeout_calls").is_some());
        assert!(report["by_project"][0].get("http_429_calls").is_some());
    }
}
