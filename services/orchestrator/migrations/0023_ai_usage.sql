-- What the AI calls came to, counted: one row per quarter of an hour, project, provider, model and task.
--
-- Every call the router makes to a provider (the ones that answer and the ones that do not) adds itself to the row of its
-- moment (`ai_usage.rs`). This is what the usage and cost page reads: how much each provider and model was used, how long
-- its calls take, how many tokens they use and what the priced ones came to. Nothing here is a prompt or an answer, only
-- counts.
--
-- The moment is a quarter of an hour in UTC (`slot`, "2026-09-26T13:15"), so that a day can be cut where the person looking
-- at it has midnight (the offset of every time zone is a multiple of fifteen minutes), and a run that starts at 00:00 local
-- time is counted on the day it started, not on the one before. `project_id` is empty for a call that was not made for a
-- project. Money is kept in millionths of a US dollar, as whole numbers, so that sums never drift.
--
-- `calls` is every call, `failed_calls` those of them that did not answer. The rest are counts of the calls that answered:
-- `token_calls` reported both token counts (an average of tokens is never diluted by a call that reported none),
-- `metered_calls` went to a provider that has a price, `priced_calls` are those of them with token counts to price them
-- by, and `cost_micros` is what those came to. A call to a provider with no price (a local model, or a hosted one whose
-- price nobody entered) is never given a cost.
CREATE TABLE ai_usage (
    slot               TEXT NOT NULL,
    project_id         TEXT NOT NULL DEFAULT '',
    provider           TEXT NOT NULL,
    model              TEXT NOT NULL,
    task               TEXT NOT NULL,
    calls              INTEGER NOT NULL DEFAULT 0,
    failed_calls       INTEGER NOT NULL DEFAULT 0,
    answered_ms        INTEGER NOT NULL DEFAULT 0,
    failed_ms          INTEGER NOT NULL DEFAULT 0,
    token_calls        INTEGER NOT NULL DEFAULT 0,
    prompt_tokens      INTEGER NOT NULL DEFAULT 0,
    completion_tokens  INTEGER NOT NULL DEFAULT 0,
    metered_calls      INTEGER NOT NULL DEFAULT 0,
    priced_calls       INTEGER NOT NULL DEFAULT 0,
    cost_micros        INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (slot, project_id, provider, model, task)
);

CREATE INDEX idx_ai_usage_project ON ai_usage (project_id, slot);
