-- What AI calls actually took, per provider, model and task: the running totals of the calls that answered.
--
-- An estimate of a run (`autopilot_estimate.rs`) needs to say how long one call to a provider lasts, and nothing knows that
-- before a call has been made: a 14B model on a home PC and a hosted one differ by a factor of ten. Each generation call
-- adds its time here, and the next estimate uses the average once there are enough calls of it (and assumes a speed for the
-- kind of provider, and says so, until then).
--
-- One row per (provider, model, task), platform-wide like the providers themselves: nothing here belongs to a project and
-- nothing in it is a prompt or an answer, only counts. `token_calls` is how many of the calls reported token counts, so
-- an average of tokens is never diluted by a call that reported none.
CREATE TABLE ai_call_stats (
    provider           TEXT NOT NULL,
    model              TEXT NOT NULL,
    task               TEXT NOT NULL,
    calls              INTEGER NOT NULL DEFAULT 0,
    total_ms           INTEGER NOT NULL DEFAULT 0,
    token_calls        INTEGER NOT NULL DEFAULT 0,
    prompt_tokens      INTEGER NOT NULL DEFAULT 0,
    completion_tokens  INTEGER NOT NULL DEFAULT 0,
    first_at           TEXT NOT NULL,
    last_at            TEXT NOT NULL,
    PRIMARY KEY (provider, model, task)
);
