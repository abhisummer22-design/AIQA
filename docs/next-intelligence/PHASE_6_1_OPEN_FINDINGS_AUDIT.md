# PHASE 6.1 — Open-Finding Audit and Risk Correctness

**Phase type:** AUDIT ONLY. No application logic, migrations, API contracts or tests were
modified.
**Verdict:** **BLOCKED** — the audit completed, but three findings require a product decision
before any remediation can be scoped honestly.

---

## 1. Repository baseline

| | |
|---|---|
| Remote | `https://github.com/abhisummer22-design/AIQA.git` (fetch + push) |
| Branch | `main` |
| HEAD | `7930507785d203f3ff9cb3e575413bd95f04aabb` |
| Subject | `fix: correct risk signal measurement handling` |
| Tracking | `## main...origin/main` — **synchronized, 0 ahead / 0 behind** |
| Working tree | Clean except one **pre-existing untracked** file: `AIQA_NEXT_ROADMAP_PHASED_PLAN.md` |

```
7930507 (HEAD -> main, origin/main, origin/HEAD) fix: correct risk signal measurement handling
50488f2 feat: add QA intelligence memory
2a4b668 A grounded refusal now says whose judgement it is
6bbb8ca The header rendered the project selector twice
4088832 Project creation takes one input, not two
```

The untracked plan file was **preserved untouched** throughout. Nothing was reset, cleaned or
discarded.

### Test baseline (actually executed)

| Suite | Result |
|---|---|
| **Full backend** | `cargo test` → **1953 passed / 0 failed / 5 ignored** (32.88 s) |
| `risk_signal_tests` | 18 passed, 0 failed |
| `integrity_tests` | **25 passed / 1 FAILED / 1 ignored** — see finding **F2** |
| `change_impact_tests` | 16 passed, 0 failed |
| `risk_prioritisation` | 18 passed, 1 ignored |
| `qa_memory_risk_tests` | 18 passed, 0 failed |
| `store_tests` | 35 passed, 0 failed |
| `adaptive_intelligence_tests` | 38 passed, 0 failed |

The full suite passed **1953/0/5** on this run, yet `integrity_tests` run in isolation failed.
That is not a contradiction — it is finding **F2**, and it is the reason this phase is not a
simple PASS.

---

## 2. Summary of all findings

| ID | Finding | Status | Decision needed |
|---|---|---|---|
| **R1** | `GET /risk` read failure becomes a zero-risk result | **OPEN_CONFIRMED** — and materially wider than reported | **Yes** — API/status-code choice |
| **R2** | `change_frequency` measured but unused by any formula | **NEEDS_PRODUCT_DECISION** | **Yes** — weight + double-count |
| **R3** | No source for `security_sensitivity` / `business_criticality` | **OPEN_CONFIRMED** — still no source | **Yes** — schema decision |
| **F1** | `set_task_status` `else if` may skip `finished_at` | **NOT_REPRODUCIBLE** in production (documented contract) — plus a new contract violation | **No** (low risk), see §5 |
| **L4** | Diverging risk formulas | **OPEN_CONFIRMED** — worse than reported: one formula is dormant | **Yes** — compatibility |
| **F2** | *(new, not in the original five)* Integrity duration tolerance has no absolute floor → intermittent false "established defect", and a ~25 % flaky test | **OPEN_CONFIRMED** | No — correctness fix |

---

## 3. R1 — `GET /risk` read-error handling

**Status: OPEN_CONFIRMED.** Worse than reported.

### The reported instance

`api.rs:9189-9195` (inside `get_project_risk_handler`, which starts at `api.rs:9169`):

```rust
let relations = state
    .store
    .list_quality_relations_for_project(&pid)
    .await
    .unwrap_or_default();          // <-- a read failure becomes an empty Vec
let dependency_count = relations.len();
```

That value is then declared **measured**, unconditionally — `api.rs:9200-9206`:

```rust
Factor::measured(
    "dependency_count",
    (dependency_count as f64 / 5.0).min(1.0),
    "quality-graph relations recorded for this project, normalised by the same divisor \
     calibrate_risk uses",
    dependency_count,                       // evidence = 0 when the read failed
),
```

**This is worse than "reports zero dependency risk."** It reports
`measured_weight` including this factor's 0.10 while never having looked. The four sibling factors
are all gated on their denominators (`api.rs:9211-9271`); `dependency_count` is the **only** one
declared `measured` unconditionally, because unlike the others it has no guard to fall through to.

### Internal inconsistency within one handler

The same handler propagates its other three reads correctly:

| Read | Line | Handling |
|---|---|---|
| `list_execution_telemetry` | `api.rs:9180` | `.map_err(ApiError::internal)?` |
| `list_flakiness_metrics` | `api.rs:9187` | `.map_err(ApiError::internal)?` |
| `list_healing_strategy_metrics` | `api.rs:9194` | `.map_err(ApiError::internal)?` |
| `list_quality_relations_for_project` | `api.rs:9195` | **`.unwrap_or_default()`** |

### Reachability

`Store::list_quality_relations_for_project` returns `Result<Vec<QualityRelationRow>>` and its
query ends `.await?` (`store.rs:3921+`), so it genuinely returns `Err`. The pool is
`SqlitePoolOptions::new().max_connections(8)` with `busy_timeout(5s)` (`store.rs:491-492`); no
`acquire_timeout` is set, so sqlx's default applies, and `PoolClosed`/I/O errors are also
reachable. A failure is not hypothetical.

### Scope: the pattern is not confined to this endpoint

Auditing every `.unwrap_or_default()` that follows an `.await` in non-test files surfaced **three
sites on risk-scoring or risk-selection paths**. The two most serious are in Phase 3:

| File:line | Masked read | Consequence on failure |
|---|---|---|
| `change_impact.rs:512` (`safety_floor`) | `list_execution_telemetry` | **The SafetyFloor empties.** This tier is documented as cases with recorded failure evidence that are "never dropped for speed". A storage failure silently drops the safety net. |
| `change_impact.rs:592` (`risk_scores`) | `list_test_intelligence` | The `HighRisk` tier selects nothing, silently. |
| `qa_memory_risk.rs:1577, 1593` (`prioritise_tests`) | `list_quality_nodes`, `list_change_impacts` | The `Changed` band is silently omitted from the prioritisation. |
| `api.rs:8832` | `get_feedback_for_planning` | Planning proceeds with empty healing feedback. |
| `api.rs:9195` (`GET /risk`) | `list_quality_relations_for_project` | As reported. |

**Direction of failure matters:** every one of these degrades risk assessment toward *testing
less*, with no error surfaced. That is the opposite of the safe default.

I checked whether the ~85 other `unwrap_or_default()` occurrences are the same class. **They are
not.** The large majority are on `Option` fields, JSON decoding and `Uuid::parse_str` — defaulting
an absent optional value is a defensible pattern there. Only the five above mask a **`Result` from
a store/service read** on a risk path.

### Existing tests

- `api_tests.rs::test_project_risk_intelligence_endpoint` — the only test that exercises this
  endpoint. Covers the no-history branch and the has-telemetry branch. **No read-failure case.**
- No test anywhere simulates a store read failure on a risk path. `engine_tests.rs` uses
  `pool.close()` in four places (L477, L567, L858, L1029), so a genuine-failure mechanism already
  exists in this codebase and is the natural basis for a regression test.

### Impact

A storage fault during risk assessment yields a **plausible-looking, wrong, "measured" answer**
rather than an error. Quantified for this endpoint: forcing a genuine dependency factor of 1.0 to
0.0 lowers the score by `0.10 / 0.90 = 0.111`, i.e. up to 11.1 points — enough to move a project
across a band boundary (`CRITICAL ≥ 0.70`, `HIGH ≥ 0.50`).

### Recommended next action

Fix the three risk-path sites (`change_impact.rs:512`, `:592`, `qa_memory_risk.rs:1577/1593`) plus
`api.rs:9195`, by propagating the read error. **The `GET /risk` status-code change is a product
decision** (see §8). The Phase 3/Phase 4 sites are not — propagating a genuine read failure there
is unambiguously correct.

---

## 4. R2 — `change_frequency`

**Status: NEEDS_PRODUCT_DECISION.** The factual part is confirmed; what to do about it is not a
technical question.

### Trace, source to consumer

| Stage | Location |
|---|---|
| Source table | `change_impacts` (migration `0031_change_impacts_scope.sql`) |
| Writers | `store.rs:3542 insert_change_impact`, called from `change_impact.rs:725` and `:752` |
| Reader | `store.rs:3921 count_distinct_changes_for_node` — called from exactly one place, `intelligence.rs:579` |
| Persisted | `risk_assessments.change_frequency` (`store.rs:3439/3447/3458`) and `test_value_assessments.change_relevance` (`store.rs:3478/3487/3496`) — the same value into both |
| Consumers | `integrity.rs:1568` (Phase 6 detector, recorded weight `0.00`) and `store.rs:3888` (the reader that exists **only** to serve that detector) |
| Formulas | **none** |

`intelligence.rs:592-595` weights `historical_failure_rate`, `flakiness_score`, `dep_norm` and
`cost_norm`. `calibrate_risk` (`adaptive_intelligence.rs:813-818`) weights failure rate, flakiness,
severity, dependency, healing regression and security sensitivity. **Neither references
`change_frequency`.**

### Which of the three states is it?

- *Used by risk* — **no.**
- *Intentionally informational* — **no.** Nothing documents it as informational. It sits among the
  risk-factor columns of a table named `risk_assessments`, every sibling of which feeds `score`,
  and the remediation report describes it as a "signal". That placement implies intent to be an
  input.
- *Accidentally unused* — **this is the honest classification.** It has never been consumed; it
  was a placeholder `0.0` before the remediation and is a measured, still-unused value after it.

The one place that states a weight for it says `0.00` (`integrity.rs:1568`) — which is *correct
today*, and confirms no formula uses it.

### The double-counting hazard the plan asked about

`change_impacts` already reaches test selection through two other routes:

1. **`change_impact.rs`** — `DirectlyImpacted` and `RelatedFailure` tiers, derived from the declared
   change's impact analysis.
2. **`qa_memory_risk.rs:1570-1595`** — `prioritise_tests(change_ref)` builds a `changed` set and
   applies a distinct `PriorityBand::Changed` (asserted by
   `risk_prioritisation_tests.rs:525 a_named_change_promotes_its_cases_without_demoting_others`).

If `change_frequency` were given weight in `risk_assessments.score`, it would reach selection a
**third** time, via the `HighRisk` tier (`change_impact.rs`, threshold `0.7`). A test impacted by
the current change would then be promoted by the tier *and* by `DirectlyImpacted`/`Changed`.

Mitigating detail: selection de-duplicates (`change_impact.rs` checks
`selected.contains_key(case_id)` before inserting), so a case is never selected twice. The
overlap is in the *signal*, not the row.

Distinction that must be preserved: `change_frequency` measures **historical churn**
(`COUNT(DISTINCT change_ref)` for that node, all time), while routes 1 and 2 concern **this
specific change**. Those are different facts. A churn signal is defensible on its own terms — but
choosing its weight is a model decision, not a bug fix.

### Recommended next action

**Decision required before any code change.** If approved, the weight must be set deliberately
against the existing 0.40/0.20/0.15/0.15 budget, and the churn-vs-current-change distinction must
be documented so the correlation is not mistaken for a second observation of the same fact.

---

## 5. R3 — Missing signal sources

**Status: OPEN_CONFIRMED.** No authoritative source has appeared.

Exhaustive search of all 32 migrations for `security*`, `criticalit*`, `business_*`:

| Match | Location | What it is |
|---|---|---|
| `business_criticality` | `0002_quality_graph_and_intelligence.sql:49` | the nullable `risk_assessments` column |
| `security_sensitivity` | `0002_quality_graph_and_intelligence.sql:53` | the nullable `risk_assessments` column |
| `business_criticality` | `0002_quality_graph_and_intelligence.sql:66` | the nullable `test_value_assessments` column |
| `security` | `0020_requirements.sql:51` | the `requirements.kind` **CHECK constraint** — a *category*, which Phase 4 already rejected as an importance |

Production struct fields are the same nullable columns and the `RiskFactors.security_sensitivity:
f64` input. **Nothing new.**

The four states the plan asks me to distinguish, verified:

| State | Present? | Evidence |
|---|---|---|
| **Measured zero** | Yes — but only for `change_frequency` | `intelligence.rs:577` `Some(count_distinct_changes_for_node(...))` |
| **Explicitly declared zero** | **No** | Nothing asserts these two signals anywhere |
| **Unmeasured value** | Yes — correctly represented | `intelligence.rs:575-576` `Option<f64> = None` → SQL `NULL`; `api.rs:9275` `Factor::unmeasured`; `qa_memory_risk.rs:978` reports it unmeasured |
| **Unavailable source** | **Yes — this is the true state** | No column, field, tag or derived value exists in the schema |

### Recommended next action

Keep both explicitly unmeasured. Do **not** derive them from `requirements.kind`, `priority`,
`category`, `severity` or tags — that would be inventing the signal, and
`risk_signal_tests.rs::neither_signal_is_invented_from_a_proxy` already fails if anyone tries.
Creating a real source is a **schema decision** (see §8).

---

## 6. F1 — `set_task_status` timestamps

**Status: NOT_REPRODUCIBLE in production.** The original "latent trap" framing was incomplete: the
behaviour is **documented as intended**, and a new, separate defect exists.

### The implementation

`store.rs:31037-31076` — `if set_started { … } else if set_finished { … } else { … }`.

### The contract is documented in the code

`store.rs:5885-5888`:

> *"the two writes have to be separate, because `set_task_status` treats `set_started`/`set_finished`
> as mutually exclusive and a combined call would never set `finished_at`, which history reads
> filter on."*

`store_tests.rs:253-256` says the same. So the `else if` is **stated intent**, not an undisclosed
bug.

### Every production caller uses exactly one flag

| Call site | Flags |
|---|---|
| `engine.rs:838` (task starts) | `(true, false)` |
| `engine.rs:508` (task reaches terminal) | `(false, true)` |
| `engine.rs:654-660` (orphan → `InfraError`) | `(false, true)` |

**The both-true state is unreachable in production.** Confirmed by exhaustive search.

### Why the skipped `finished_at` matters

`store.rs:3356-3359` and `:3372-3374` — `task_history_by_name` filters
`WHERE t.name = ?1 AND t.finished_at IS NOT NULL`. `store.rs:1465-1467`
(`dashboard_avg_task_duration_ms`) likewise requires both timestamps. So a task left without
`finished_at` is invisible to history and excluded from duration averages.

### New sub-finding: an existing caller violates the documented contract

`engine_tests.rs:2725`, inside the `seed_run` test helper:

```rust
.set_task_status(run_id, &task_id, task_status, None, true, true)
```

This sets `started_at` and leaves `finished_at` **NULL**. Current impact is nil: `seed_run`'s four
callers (`seed_completed_crawl` and three more) assert nothing about risk, history or
intelligence, so nothing observes the missing timestamp today.

It is nonetheless a live inconsistency and a trap: the next test that seeds a run and expects
`intelligence.rs` or `task_history_by_name` to see it will get nothing, with no obvious cause.

### Recommended next action

Not urgent. Two small options, neither requiring a product decision:

- **(a)** Change `engine_tests.rs:2725` to two calls, matching the documented contract.
- **(b)** Make the signature enforce the invariant — e.g. replace two `bool`s with an enum
  (`Start` / `Finish` / `StartAndFinish`), which removes the trap permanently but changes a public
  method's signature.

Option (a) is the bounded fix. Option (b) is better design and a slightly larger change.

---

## 7. L4 — Diverging risk formulas

**Status: OPEN_CONFIRMED.** The divergence is real, and the audit found something the earlier
reports did not: **one of the two formulas cannot execute in production at all.**

### Every risk calculation, compared

| | **A. `calibrate_risk`** | **B. `intelligence.rs` risk_score** | **C. `value_score`** | **D. `PriorityBand`** |
|---|---|---|---|---|
| Location | `adaptive_intelligence.rs:813-818` | `intelligence.rs:592-604` | `intelligence.rs:664-667` | `qa_memory_risk.rs:1627-1674` |
| failure rate | 0.35 | 0.40 | 0.40 | via band |
| flakiness | 0.20 | 0.20 | 0.20 **inverted** | via band |
| failure severity | 0.15 | **absent** | — | — |
| dependency | 0.10 | 0.15 | — | — |
| healing regression | 0.10 | **absent** | — | — |
| security sensitivity | 0.10 | **absent** | — | — |
| execution cost | **absent** | 0.15 | 0.10 **inverted** | — |
| own prior score | — | — | 0.30 | — |
| **Total** | **1.00** | **0.90** | **1.00** | n/a (bands) |
| Normalisation | none | ÷ `measured_weight` (0.90) | none | n/a |
| Thresholds | 0.70 / 0.50 / 0.25 | **none — emits no band** | none | 7 bands |
| Output | score + band | score only | score only | ordered bands |

Three genuinely different quantities, not two:

- **A** is *failure-and-healing risk* — weighted toward severity and governed-healing regressions.
- **B** is *execution cost-and-failure risk* — includes how expensive the test is, which is not a
  risk property.
- **C** is *test value*, deliberately **inverted** (stable and cheap is more valuable), so it is
  **not** comparable to A or B by construction and must never be merged with them.
- **D** is an *ordered band system*, not a score; it answers "what should I run first", a
  different question.

### The new finding: A is dormant

`calibrate_risk` has exactly one non-comment production caller: `qa_memory_risk.rs:970`, inside
`calibrate_or_none`. That function **refuses unless all six factors measure** — and
`security_sensitivity` has no source while `dependency_count` is structurally unavailable for a
module, release or test case. Phase 4's own tests assert the refusal holds:

- `qa_memory_risk_tests.rs:206` — `view.calibration.is_none()`
- `risk_prioritisation_tests.rs:207` — `risk.calibration.is_none()`
- `risk_signal_tests.rs:915` — `entry.risk.calibration.is_none()`

The module comment states it plainly: *"Why there is no risk score, and that is the finding."*

**Therefore `calibrate_risk` produces no production output.** `GET /risk` reaches a band only via
`calibrate_partial`. This means:

- `RISK_LEVEL_THRESHOLDS` — the canonical band table — is exercised in production by exactly one
  endpoint.
- A and B are not two formulas competing for the same consumer. **B is the only formula whose
  stored score is consumed for decisions** (`change_impact::risk_scores()` → `HighRisk` tier, and
  `GET /intelligence/tests`).

### Is the divergence intentional or accidental?

Partly each, and the honest answer differs per pair:

- **A vs B** — **unintentional duplication of intent, different inputs.** Both are called "risk
  score", both are 0–1, and a reader cannot tell from the value which one produced it. They
  disagree systematically: B charges a test more for being slow, A never does.
- **C** — **intentionally a different metric** (value, inverted). Correct as-is; must stay
  separate.
- **D** — **intentionally a different question** (ordering, not scoring). Correct as-is.

### Recommended next action

Do **not** unify. First decide which of A or B is meant to be *the* risk score, because:

- B is live and decision-bearing; changing it moves `HighRisk` selection.
- A is dormant; unifying toward A would **revive** a formula that has never run in production,
  immediately changing every `GET /risk` score.

That is a model decision with compatibility impact, not a refactor.

---

## 8. New finding F2 — integrity duration tolerance has no absolute floor

**Status: OPEN_CONFIRMED.** Not among the five findings; discovered by this audit and the reason
the phase cannot be a clean PASS.

### Evidence

`integrity_tests::a_genuine_status_mismatch_is_detected_and_can_establish_a_defect` fails
intermittently. **12 consecutive runs: 3 failures (runs 1, 4, 10), ~25 %.** Identical failure every
time:

```
panicked at src\integrity_tests.rs:443:5:
assertion `left == right` failed
  left: 2
  right: 1
```

The assertion is `assert_eq!(report.defects_established, 1)`. The verifier intermittently reports
**two** established product defects where only the status mismatch should qualify.

### Root cause — confirmed

`integrity.rs:729` and `:787`, inside `task_duration_consistency`:

```rust
const TOLERANCE: f64 = 0.10;                       // L729
…
let spread = (a - b).abs() / a.max(b).max(1.0);    // L786
if spread <= TOLERANCE {                           // L787
    c.outcome = IntegrityOutcome::Consistent;
    c.defect_established = false;
    …
}
```

The denominator is floored at `1.0`, so for any duration of 0–1 ms the tolerance is **0.1 ms** —
finer than the clock's own resolution.

The two sides are **independently taken measurements**:

- Backend: `tasks.finished_at − tasks.started_at`, written by `store.rs:31047-31065` using
  `now_rfc3339()` at each transition — two separate wall-clock instants.
- API: `execution_telemetry.duration_ms`, derived by `ExecutionTelemetry::from_task_state`
  (`adaptive_intelligence.rs:16708-16711`) from the `TaskState` timestamps.

In the fixture the `TaskState` carries no timestamps, so the API side is **0**, while the backend
side is real elapsed time. The outcome therefore depends entirely on whether the two
`set_task_status` calls land in the same millisecond:

- same millisecond → `backend = 0` → `spread = 0.0` → **Consistent** → 1 defect
- straddling a millisecond → `backend = 1` → `spread = 1.0` → **Inconsistent** → 2 defects

That is the observed 3-in-12 distribution.

### Why this matters beyond the test

`decide_pair` sets `defect_established = true` when both layers recorded a measurement and they
disagree (`integrity.rs:552`). In production, `engine.rs` builds `TaskState` timestamps and
`set_task_status` stamps its own `now_rfc3339()` — **different instants, so the two clocks are
genuinely independent there too.**

Any task whose real duration is single-digit milliseconds can therefore be reported as an
**established cross-layer defect that did not happen**. For a 5 ms task the tolerance is 0.5 ms —
within rounding distance of the measurement itself.

This is the exact failure mode Phase 6 was built to prevent: an over-claim of a confirmed product
defect. It is the same class as the fabricated zeros the risk-signal remediation removed — a
measurement artifact presented as a finding.

### Recommended next action

Add an absolute floor alongside the relative tolerance — e.g. treat two durations as agreeing
when `|a − b| ≤ max(0.10 × max(a,b), 50 ms)`. Two independent points:

1. The 50 ms figure is a **product decision** (what counts as clock noise for this system).
2. The test must then be made deterministic, not merely less flaky — a test that passes 75 % of the
   time cannot serve as a gate. Its fixture should supply a fixed API duration rather than
   leaving it at zero.

---

## 9. Relevant tests and verification results

### What was run, and what it showed

| Check | Command | Result |
|---|---|---|
| Full backend suite | `cargo test` | 1953 passed / 0 failed / 5 ignored |
| `risk_signal_tests` | `cargo test risk_signal_tests` | 18 passed |
| `integrity_tests` | `cargo test integrity_tests` | **25 passed / 1 FAILED / 1 ignored** |
| `change_impact_tests` | `cargo test change_impact_tests` | 16 passed |
| `risk_prioritisation` | `cargo test risk_prioritisation` | 18 passed, 1 ignored |
| `qa_memory_risk_tests` | `cargo test qa_memory_risk_tests` | 18 passed |
| `store_tests` | `cargo test store_tests` | 35 passed |
| `adaptive_intelligence_tests` | `cargo test adaptive_intelligence_tests` | 38 passed |
| Flake quantification | 12 × `cargo test integrity_tests` | **3 failures (25 %)** |

`build`, `clippy` and `rustfmt` were **not** re-run: no code changed, so they would only re-measure
a tree identical to the one already verified at `7930507`.

### Why "the suite is green" is not sufficient evidence here

This phase's own instruction is not to treat a passing test as proof. Two concrete cases where the
tests are green but the behaviour is not:

- **R1** — `test_project_risk_intelligence_endpoint` passes while a storage failure yields a
  false "measured" dependency factor. The test has no read-failure case.
- **L4** — `calibrate_risk` is fully tested (`adaptive_intelligence_tests` 38 passed) yet has **no
  production caller that can execute it**. Passing tests describe a dormant code path.

### Verification not performed, and why

- **No runtime reproduction of R1.** Confirming the masked read at runtime needs a test that fails
  a store read; adding one would modify the test suite, which this phase forbids. Reachability is
  instead established from the signature (`.await?`), the pool configuration, and the existing
  `pool.close()` precedent in `engine_tests.rs`.
- **No source/schema diff against a remote.** HEAD is byte-identical to `origin/main`
  (0 ahead / 0 behind), so "current repository" and "latest branch" are the same tree.

---

## 10. Potential regressions and API compatibility concerns

| Concern | Detail | Severity |
|---|---|---|
| **R1 on `GET /risk`** | Propagating the read error converts a `200` into a `5xx`. Any client polling this endpoint sees a new failure mode. | Medium — **API contract change** |
| **R1 on Phase 3/4 paths** | Propagating errors turns a silent partial answer into a failed analysis run. Selection currently succeeds on partial data; it would begin to fail. | Medium — behaviour change, arguably a fix |
| **F2 tolerance floor** | A 50 ms absolute floor makes the verifier *less* sensitive. Genuine disagreements under 50 ms would no longer be reported as defects. That is the intended direction, but it reduces detection. | Medium |
| **R2 weight** | Any new weight changes every stored `risk_assessments.score`, moving `HighRisk` membership and therefore regression scope. | High |
| **L4 unification toward A** | Would revive dormant `calibrate_risk`, changing every `GET /risk` score and band. | High |
| **L4 unification toward B** | Would change `HighRisk` selection and `GET /intelligence/tests` output. | High |
| **F1 signature change** | Option (b) changes a public `Store` method signature; `engine.rs` and ~15 test call sites. | Low |
| **R3 schema** | Adding a source means a migration and new API fields. | Medium |

---

## 11. Recommended remediation order

Ordered by risk-reduction per unit of change, and deliberately **not** by the finding numbering.

1. **F2** — correctness defect producing false confirmed defects. Bounded (one tolerance
   expression), no API surface, and until it is fixed the integrity suite cannot be trusted as a
   gate for anything else.
2. **R1 on Phase 3 / Phase 4 paths** (`change_impact.rs:512`, `:592`; `qa_memory_risk.rs:1577`,
   `:1593`) — a storage fault currently makes AIQA test *less*. No API contract involved.
3. **F1 option (a)** — one-line test-helper correction; removes a documented-contract violation.
4. **R1 on `GET /risk`** — blocked on a product decision (§12).
5. **R2** — blocked on a product decision (§12).
6. **L4** — blocked on a product decision (§12). Explicitly *after* the above, because both
   candidate unifications change scores that R2 and F2 also affect.
7. **R3** — blocked on a schema decision (§12). No code until then.

R3 is listed last deliberately: it is the only item where the correct action is *nothing*, and it
must not be "fixed" by inventing a proxy.

---

## 12. Product decisions required

### D1 — R1: how should `GET /risk` report a failed dependency read?

| Option | Behaviour | Consequence |
|---|---|---|
| **A. Propagate** | Return `5xx` via `ApiError::internal`, matching the handler's other three reads | Consistent; new failure mode for clients polling the endpoint |
| **B. Explicit partial** | Return `200` with the dependency factor as `unmeasured` and `measured_weight` reduced | No status change; caller must read coverage. Mirrors what the remediation already does for the other factors |
| **C. Status quo** | Keep reporting a measured `0.0` | Rejected: a storage fault reads as a clean measurement |

**Recommendation: B.** It preserves the API contract, and it is the same mechanism already in use
four lines above — a factor that cannot be measured is declared unmeasured with its weight excluded.
Option A is defensible if a caller must never see a partial answer.

### D2 — R2: should `change_frequency` carry weight?

| Option | Consequence |
|---|---|
| **A. Leave at weight 0** | Stays a stored, integrity-checked measurement that no formula uses. Honest, but the column invites the assumption it is an input |
| **B. Give it weight** | Changes every stored score and `HighRisk` membership. Requires choosing a weight against the existing 0.40/0.20/0.15/0.15 budget, plus documenting historical-churn vs current-change so the overlap with the `DirectlyImpacted` and `Changed` paths is not mistaken for a second observation of one fact |
| **C. Remove the column** | Destroys a real measurement; not recommended |

**Recommendation: A for now**, with B deferred until D3 settles which formula is canonical —
otherwise the weight is chosen against a model that may be replaced.

### D3 — L4: which formula is *the* risk score?

| Option | Consequence |
|---|---|
| **A. `intelligence.rs` (B above)** | It is the only decision-bearing formula. But it lacks severity and healing terms, and charges cost as risk |
| **B. `calibrate_risk` (A above)** | Richer inputs, but reviving it changes every `GET /risk` score immediately |
| **C. Keep both, document each** | No score changes. Requires naming them distinctly so a reader cannot confuse them, and stating which consumers each serves |
| **D. Merge** | Explicitly not recommended before A–C are decided |

**Recommendation: C as an interim step** — it is the only option that changes no score, and the
audit shows the divergence is not causing a live inconsistency (A is dormant, B is the only
decision-bearing formula, C and D are intentionally different metrics). A and B can then be
revisited with impact analysis.

### D4 — R3: is there an authoritative source for these two signals?

Options: **(a)** keep unmeasured indefinitely; **(b)** add a schema field (a security-sensitivity
classification, or a requirement criticality) plus a migration and API surface; **(c)** accept a
declared human-entered value as a *declaration* distinct from a measurement.

**Recommendation: (a) for now.** Any of (b)/(c) is a schema decision that must be recorded before
code, and none should be reached by inference from `kind`, `priority`, `category`, `severity` or
tags.

### D5 — F2: what is this system's clock-noise floor?

Affects only the absolute tolerance. Needs an absolute value; a 50 ms proposal is offered in §8 but
is a judgement about AIQA's own clocks, not a derivable fact.

---

## 13. Exact scope recommended for the next implementation step

If the decisions above are made, the next step should be a **single remediation phase** scoped to
items 1–3 only — the three that need no product decision:

**In scope**
- `integrity.rs`: add an absolute floor to the duration tolerance alongside the 10 % relative one.
- `integrity_tests.rs`: make `a_genuine_status_mismatch_is_detected_and_can_establish_a_defect`
  deterministic — the fixture must supply a fixed API duration instead of leaving it at zero — and
  add a test that a sub-tolerance duration difference is reported `Consistent` with
  `defect_established == false`.
- `change_impact.rs` (2 sites) and `qa_memory_risk.rs` (2 sites): propagate store read errors
  instead of `unwrap_or_default()`; add a regression test per path using the existing
  `pool.close()` technique, asserting the analysis reports the failure rather than a partial answer.
- `engine_tests.rs:2725`: replace the combined call with two calls, per the contract already
  documented at `store.rs:5885-5888`.

**Explicitly out of scope**
- Any change to `GET /risk`'s status codes or response shape (D1).
- Any weight change for `change_frequency` (D2).
- Any unification of the risk formulas (D3).
- Any migration, schema change or invented signal (D3, D4, R3).
- Any change to `set_task_status`'s signature (F1 option (b)) — the bounded fix only.

**Preconditions**
- D5 answered, so the absolute floor is a stated number.
- Each of the four propagated-read changes needs its own test, because the current green suites do
  not distinguish "no failures recorded" from "could not read the failures".

**Gate**
Each item gets root cause → fix → retest → regression. The integrity suite must go from ~75 % to
**100 % over at least 20 consecutive runs** before the phase can be called PASS; a suite that fails
one run in four cannot certify anything built on top of it.

---

## 14. Confirmation of execution restrictions

| Restriction | Status |
|---|---|
| Application logic modified | **No** |
| Migrations added or modified | **No** |
| API contracts changed | **No** |
| Existing tests modified to make findings appear resolved | **No** — no test file was touched |
| Phase 6.2 started | **No** |
| Phase 7 started | **No** |
| Committed or pushed | **No** |
| Historical reports rewritten | **No** — all prior reports left byte-unchanged; F1's contract and F2 are recorded here by addition |
| Checks claimed that did not run | **No** — only `cargo test` invocations are reported, with real counts; `build`/`clippy`/`rustfmt` are explicitly declared as not re-run and why |

`git status --short` at the end of this phase returns exactly one line — the pre-existing untracked
`AIQA_NEXT_ROADMAP_PHASED_PLAN.md`. No `.rs`, `.sql`, `.md` under `docs/next-intelligence/`, or
config file was added, modified or deleted.
