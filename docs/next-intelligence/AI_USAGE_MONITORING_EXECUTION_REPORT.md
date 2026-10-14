# AI Usage Monitoring Execution Report

This document records the verification, implementation, and publication status of AI Usage Monitoring phases in AIQA.

---

## Baseline Verification
- **Repository:** `H:\AIQA`
- **Initial Branch:** `main`
- **Initial HEAD:** `21d6f8ee375a84a49e30bcfb3c6ba167ad6a61c4`
- **Remote `origin/main` Baseline:** `a7c31e0ad120704f187aaa50c3ebeae9c766bb5f` (preserved untouched)
- **Remote `origin/demo` Baseline:** `21d6f8ee375a84a49e30bcfb3c6ba167ad6a61c4`
- **Untracked File Preserved:** `docs/GROQ_API_QA_MONITORING_AUDIT_REPORT.md` (untracked, unmodified)

---

## Phase 1 — AI Usage Metrics Validation and Hardening

### Review Findings
1. **Histogram Correctness:**
   - `ai_provider_execution_duration_seconds` has 12 bounded buckets (`0.1` to `180.0`).
   - Observations are strictly cumulative and non-decreasing.
   - `+Inf` bucket is explicitly rendered and matches `count`.
   - `_sum` correctly divides microsecond totals by `1,000,000.0`.
   - Concurrency is safe using `AtomicU64`.
2. **Counter Semantics:**
   - `ai_provider_calls_total` records provider-execution calls at the `AiInfrastructure::execute_for` boundary once per call (with `outcome` in `{"success", "rejected", "timeout", "invalid_response", "error"}`).
   - It represents logical provider executions including internal transient retries, not individual raw socket retransmissions.
3. **Cardinality & Security:**
   - Label values (`provider`, `task`, `outcome`) are strictly bounded and sanitized. No credentials, prompts, project IDs, or user IDs are exposed as metric labels.
4. **Classification Truthfulness:**
   - Real HTTP statuses preserved when returned (`200`, `429`, `502`, `503`, `401`).
   - Timeouts and transport errors yield `None` (no synthetic status codes).
   - `InvalidResponse` yields `None` (never synthetic `200`).
5. **Backwards Compatibility:**
   - 15-minute SQLite ledger in `ai_usage` table and `GET /api/v1/ai/usage` remain 100% compatible.

### Changed Paths
- `services/orchestrator/src/ai_router.rs`
- `services/orchestrator/src/ai_usage.rs`
- `services/orchestrator/src/metrics.rs`
- `services/orchestrator/src/retention_and_metrics_tests.rs`
- `docs/next-intelligence/AI_USAGE_MONITORING_EXECUTION_REPORT.md`

### Test Verification
- `cargo check --lib --manifest-path services/orchestrator/Cargo.toml`: Exit code 0 (passed)
- `cargo test --manifest-path services/orchestrator/Cargo.toml ai_usage`: 27 passed, 0 failed
- `cargo test --manifest-path services/orchestrator/Cargo.toml metrics`: 20 passed, 0 failed
- `cargo test --manifest-path services/orchestrator/Cargo.toml groq`: 29 passed, 0 failed
- `cargo test --manifest-path services/orchestrator/Cargo.toml retention_and_metrics_tests::ai_provider_execution_is_recorded_in_metrics_and_ledger_compatibly`: 1 passed, 0 failed
- Full backend suite: `cargo test --manifest-path services/orchestrator/Cargo.toml -p aiqa-orchestrator`: 2,218 passed, 0 failed, 5 ignored (Exit code 0)
- `git diff --check`: 0 warnings, clean

### Publication
- **Local Commit:** `bbd3368a9e3bbfe45aa14f01271890adc0302eb8`
- **Commit Message:** `feat(monitoring): add AI provider latency and outcome metrics`
- **Published to Remote:** `origin/demo` (fast-forward verified)
- **`origin/main` Status:** Preserved untouched at `a7c31e0ad120704f187aaa50c3ebeae9c766bb5f`

---

## Phase 2 — Trace ID Correlation

### Review Findings
1. **Trace Propagation:**
   - Incoming HTTP headers (`x-request-id`, `traceparent`) are extracted in `auth::require_auth` and placed in task-local storage `REQUEST_TRACE_ID`.
   - `AgentContext.trace_id` propagates to AI requests via `context.create_ai_request(...)` and is active in task-local storage during agent turns.
   - `AiRequest` carries optional `trace_id` with builder methods `new`, `with_trace_id`, and `with_preferred`.
   - When no trace ID is provided or in context, `AiInfrastructure::generate` generates a non-empty fallback trace ID following project convention `req_<uuid_simple>`.
2. **Outbound Provider Headers:**
   - `GroqExecutor` attaches outbound `x-request-id` header when a valid ASCII trace ID exists.
   - Header attachment preserves authorization, timeouts, JSON formatting, and retry logic.
3. **Usage Event Correlation & Structured Logging:**
   - `AiCallEvent` carries `trace_id`.
   - `UsageLedger::record` emits structured log events (`ai_call_recorded`) containing `trace_id`, provider, model, task, duration, and outcome.
   - No prompts or credentials are persisted or logged.
4. **Cardinality Safety:**
   - Trace IDs are strictly excluded from Prometheus metric labels to prevent cardinality explosion.

### Changed Paths
- `services/orchestrator/src/agent.rs`
- `services/orchestrator/src/ai_generation.rs`
- `services/orchestrator/src/ai_router.rs`
- `services/orchestrator/src/ai_usage.rs`
- `services/orchestrator/src/api_tests.rs`
- `services/orchestrator/src/auth.rs`
- `services/orchestrator/src/copilot_synthesizer.rs`
- `services/orchestrator/src/custom_testing.rs`
- `services/orchestrator/src/execution_plan.rs`
- `services/orchestrator/src/gap_verify.rs`
- `services/orchestrator/src/intelligence.rs`
- `services/orchestrator/src/login_suggest.rs`
- `services/orchestrator/src/requirement_generation.rs`
- `services/orchestrator/src/retention_and_metrics_tests.rs`
- `docs/next-intelligence/AI_USAGE_MONITORING_EXECUTION_REPORT.md`

### Test Verification
- `cargo check --lib --manifest-path services/orchestrator/Cargo.toml`: Exit code 0 (passed)
- `cargo test --manifest-path services/orchestrator/Cargo.toml trace_id`: 5 passed, 0 failed
- `cargo test --manifest-path services/orchestrator/Cargo.toml groq_executor_attaches_outbound_x_request_id_header`: 1 passed, 0 failed
- Full backend suite: `cargo test --manifest-path services/orchestrator/Cargo.toml -p aiqa-orchestrator`: 2,224 passed, 0 failed, 5 ignored (Exit code 0)
- `git diff --check`: 0 warnings, clean

### Publication
- **Local Commit:** `bf8382e0f012110c515be6e6682bd493a6b2bf66`
- **Commit Message:** `feat(monitoring): propagate AI request trace IDs`
- **Published to Remote:** `origin/demo` (fast-forward verified)
- **`origin/main` Status:** Preserved untouched at `a7c31e0ad120704f187aaa50c3ebeae9c766bb5f`

---

## Phase 3 — Additive Database Usage Rollups

### Review Findings
1. **Gap Analysis:**
   - The existing `ai_usage` table (from migration `0023_ai_usage.sql`) previously recorded only coarse `calls` and `failed_calls`.
   - All failures (rate limits, timeouts, server errors, client errors) were lumped together into `failed_calls`.
   - Ephemeral Prometheus metrics from Phase 1 (`ai_provider_calls_total{outcome=...}`) provide process-level metrics, but persistent 15-minute slot querying in the database lacked granular error breakdown.
2. **Schema & Migration:**
   - Selected next valid sequential migration: `services/orchestrator/migrations/0033_ai_usage_error_rollups.sql`.
   - Used additive SQLite `ALTER TABLE ai_usage ADD COLUMN ... INTEGER NOT NULL DEFAULT 0` for:
     * `http_429_calls`
     * `http_5xx_calls`
     * `timeout_calls`
   - Guarantees complete backwards compatibility: existing records receive default 0 without data loss or table rewrites.
3. **Correct Counting Semantics:**
   - Mutually exclusive classification based on `AiCallEvent` fields:
     * `http_429_calls`: `!event.answered && event.http_status == Some(429)`
     * `http_5xx_calls`: `!event.answered && event.http_status.map_or(false, |s| (500..=599).contains(&s))`
     * `timeout_calls`: `!event.answered && event.error_category == Some(AiErrorCategory::Timeout)`
   - Successful calls (`event.answered == true`) never increment any error counter.
   - Timeouts never count as 429 or 5xx.
   - 429 rate limits never count as 5xx.
   - Preserves existing 15-minute slot boundaries, token counts, pricing calculations, and row semantics.
   - Upserts in `Store::add_ai_usage` correctly accumulate `excluded.http_429_calls`, `excluded.http_5xx_calls`, `excluded.timeout_calls`.
4. **API and Reporting Compatibility:**
   - `AiUsageRow::from_row` safely reads columns using `try_get(...).unwrap_or(0)`.
   - `Tally` and `Tally::json()` additively expose `http_429_calls`, `http_5xx_calls`, `timeout_calls` in totals, by_provider, by_task, by_day, and by_project.
   - Existing endpoints (`GET /api/v1/ai/usage` and `GET /api/v1/projects/:id/ai-usage`) remain 100% backward compatible.

### Changed Paths
- `services/orchestrator/migrations/0033_ai_usage_error_rollups.sql`
- `services/orchestrator/src/ai_usage.rs`
- `services/orchestrator/src/change_impact_tests.rs`
- `services/orchestrator/src/integration_e2e_tests.rs`
- `services/orchestrator/src/trace_chain_tests.rs`
- `services/orchestrator/src/trace_tests.rs`
- `docs/next-intelligence/AI_USAGE_MONITORING_EXECUTION_REPORT.md`

### Test Verification
- `cargo check --lib --manifest-path services/orchestrator/Cargo.toml`: Exit code 0 (passed)
- `cargo test --manifest-path services/orchestrator/Cargo.toml ai_usage`: 29 passed, 0 failed
- `cargo test --manifest-path services/orchestrator/Cargo.toml change_impact_tests::phase_three_added_one_additive_migration`: 1 passed, 0 failed
- `cargo test --manifest-path services/orchestrator/Cargo.toml trace_chain_tests::a_fresh_migration_supplies_every_table_the_chain_traverses`: 1 passed, 0 failed
- `cargo test --manifest-path services/orchestrator/Cargo.toml trace_tests::phase_one_introduces_no_new_migration`: 1 passed, 0 failed
- `cargo test --manifest-path services/orchestrator/Cargo.toml qa_memory_migration_tests`: 1 passed, 0 failed
- `cargo test --manifest-path services/orchestrator/Cargo.toml phase8a_a_call_that_failed_is_counted_as_failed_and_costs_nothing`: 1 passed, 0 failed
- `cargo test --manifest-path services/orchestrator/Cargo.toml retention_and_metrics_tests::ai_provider_execution_is_recorded_in_metrics_and_ledger_compatibly`: 1 passed, 0 failed
- Full backend suite: `cargo test --manifest-path services/orchestrator/Cargo.toml -p aiqa-orchestrator`: 2,226 passed, 0 failed, 5 ignored (Exit code 0)
- `git diff --check`: 0 warnings, clean

### Publication
- **Local Commit:** `abadc7ec23f038be29e8c37e6da48ba95d43fb8a`
- **Commit Message:** `feat(monitoring): add AI usage error rollups`
- **Published to Remote:** `origin/demo` (fast-forward verified)
- **`origin/main` Status:** Preserved untouched at `a7c31e0ad120704f187aaa50c3ebeae9c766bb5f`
