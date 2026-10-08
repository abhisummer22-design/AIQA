# Phase 4 — Memory → Test Planning Intelligence: Report

**Implemented:** 2026-10-07 · **Report finalised:** 2026-10-08
**Phase:** 4 of 6
**Status:** PASS
**Baseline before this phase:** 1763 passed / 0 failed / 2 ignored
**After this phase:** 1777 passed / 0 failed / 2 ignored (+14)

---

## 1. Objective

Make the existing planning agent *aware* of historical intelligence in a form that can
be audited, and make old history stop dominating current signals — without building a
second planning engine.

---

## 2. What already existed

Phase 1 found planning was already wired to memory:

- `TestPlanningAgent` (`agents/planning.rs`) already accepts
  `qa_feedback: Option<QaIntelligenceFeedback>`
- `api.rs:8409` already passes healing feedback
- `api.rs:8415` already passes `generate_planning_feedback(...)`
- `QaIntelligenceFeedback` already carried `fragile_tests`, `fragile_selectors`,
  `high_risk_components`, `top_failure_clusters`, `recommended_priorities`

So the wiring existed. What did not exist was any way to **audit** it: every field was
one flat string with no metric, no evidence, no confidence and no window. Master plan §6
requires all five.

---

## 3. What was built

### 3.1 `PlanningSignal` — structured, six fields

```rust
pub struct PlanningSignal {
    pub kind: String,          // module_risk | recurring_defect | release_regression
    pub subject: String,       // module ref, release label, defect title
    pub metric: String,        // "3 of 5 executions of this module's tests failed (60.0%)"
    pub evidence_runs: Vec<Uuid>,
    pub confidence: f64,       // derived from sample size, never asserted
    pub window: String,        // "2026-09-27 .. 2026-10-03 (5 execution(s))"
    pub basis: String,         // what it was computed from
}
```

Confidence is a stated convention, not a model: 1–2 samples → 0.3, 3–9 → 0.6, 10+ → 0.9.
Capped at 0.9 because even a large sample is history, not proof.

### 3.2 Added to `QaIntelligenceFeedback`, not replacing anything

`planning_signals: Vec<PlanningSignal>` sits **alongside** the existing string fields.
Those are read by the existing `agents::planning` renderer and its tests; Phase 4 had to
extend a planning engine, not rewrite it.

Wired at `generate_planning_feedback`, which now delegates to
`QaMemoryReader::planning_signals` — so there is **one** definition of what the history
means and **one** window rule, not a second interpretation in the feedback builder.

### 3.3 The 90-day window

```rust
pub const PLANNING_SIGNAL_WINDOW_DAYS: i64 = 90;
```

Master plan §6 requires that "old irrelevant history does not dominate current signals".
A failure from a year ago is not the same planning input as one from last week, so the
window is explicit rather than implied by whatever rows happen to exist.

History older than the window is still fully retrievable through `module_risk_history`
and friends — it simply does not drive planning on its own.

### 3.4 Signal kinds

| Kind | Built from |
|---|---|
| `module_risk` | a module's linked test cases via `covers` edges, over recent telemetry |
| `recurring_defect` | a bug draft whose cluster has `occurrence_count >= 2` and was seen inside the window |
| `release_regression` | a release with ≥2 recent executions, its failure rate measured |

Signals are sorted best-evidenced first (confidence, then evidence count, then kind,
then subject) and capped at 20, so truncation drops the weakest rather than an arbitrary
subset.

---

## 4. "No evidence ⇒ no influence"

The property that matters most, and it is enforced rather than preferred:

`planning_signals` returns an **empty vector** when the project has no history, and also
when history exists but none of it is recent enough. A brand-new project produces no
signals at all — not neutral-sounding ones, not placeholder ones, none.

Pinned by:

- `a_project_with_no_history_produces_no_signals_at_all` — a module *and* a release both
  exist, so nothing is missing for want of a subject, and the result is still empty
- `the_planning_agent_receives_no_signals_for_a_project_with_no_history` — the same
  through the real `generate_planning_feedback` path

---

## 5. Tests

14 tests in `src/qa_memory_planning_tests.rs` (581 lines).

| Master plan §7 case | Test |
|---|---|
| Memory influences planning when evidence exists | `a_module_with_failures_produces_a_planning_signal`, `a_recurring_defect_produces_a_planning_signal`, `a_release_with_repeated_failures_produces_a_planning_signal` |
| Memory does **not** influence when no evidence | `a_project_with_no_history_produces_no_signals_at_all`, `the_planning_agent_receives_no_signals_for_a_project_with_no_history` |
| Old history does not dominate | `history_older_than_the_window_produces_no_signal`, `a_recent_failure_outranks_old_history_in_the_same_module` |
| Project isolation | `signals_never_cross_a_project_boundary` |
| Requirement-specific | `a_signal_names_the_subject_it_is_about` |
| Risk prioritisation | `the_best_evidenced_signal_is_offered_first` |
| Regression behaviour | `the_planning_agent_receives_the_signals_when_evidence_exists` plus the existing suite |
| §6 Explainability | `every_signal_carries_the_five_fields_the_plan_requires` |
| §1.5 no chain-of-thought | `no_signal_carries_a_field_that_looks_like_hidden_reasoning` |
| Determinism | `signals_are_deterministic_for_the_same_history` |

```
cargo test --lib qa_memory_planning
  → 14 passed / 0 failed
```

The tests drive the **real** production path: `Intelligence::ingest_run` is used to
create clusters and bug drafts, rather than a test-only accessor being added to
production code.

---

## 6. A real defect this phase found in Phase 3

### `bug_history` returned an empty list for every project

`QaMemoryReader::bug_history` looked the failure cluster up with:

```rust
self.store.get_failure_cluster_by_signature(project_id, &draft.failure_cluster_id.to_string())
//                                                                              ^^^^ an ID
```

`get_failure_cluster_by_signature` runs `WHERE project_id = ? AND signature = ?`. It was
being handed a **cluster id** where a **signature** was expected, so it never matched,
so `bug_history` returned an empty list for *every* project.

**Why this is dangerous rather than merely wrong:** the resulting answer is *"there are
no recurring defects anywhere"*. That is the most misleading output the system can
produce — it reports absence while data exists, from the very function whose purpose is
to report recurrence.

**Fix:** `get_failure_cluster_by_id_scoped(project_id, draft.failure_cluster_id)` — the
method that already existed and was meant to be called.

**Why Phase 3's tests missed it:** this phase's own `bug_history` coverage asserted only
the **empty** case (`assert!(reader.bug_history(&pid).await.is_empty())`). The
non-empty case was never tested, so a function that always returned empty passed.

That is a real gap in Phase 3's suite, not in Phase 4's. Phase 6's end-to-end chain
test is the regression guard that closes it.

---

## 7. Bugs in this phase's own work

| # | Defect | Whose | Resolution |
|---|---|---|---|
| P1 | `signals_never_cross_a_project_boundary` asserted that the *other* project was **non-empty** | **the test** | removed the contradictory assertion; it would have meant the very leak the test exists to catch. The owning project being non-empty is now asserted instead |
| P2 | `bug_history` correlation (§6) | **production, from Phase 3** | fixed with the correct by-id lookup |

P1 is the third time in this plan that a failing test's real fault was its premise. It
is worth stating the pattern plainly rather than burying it: **7 of the 9 test failures
across all six phases were wrong premises, and several would have passed while a real
defect shipped.**

---

## 8. Regression

```text
before Phase 4:  1763 passed / 0 failed / 2 ignored
after  Phase 4:  1777 passed / 0 failed / 2 ignored
```

No existing test weakened. `agents/tests.rs:641` received one added field
(`planning_signals: vec![]`) on a fixture that predates structured signals and asserts
only the string fields; its assertions are unchanged.

---

## 9. Known limitations carried forward

1. **A defect signal carries no evidence runs.** It has `occurrence_count` and a time
   window, but no run ids, because the link from a cluster to its runs was still broken
   at this point (§6). Phase 6's migration 0030 fixes the link; the signal's
   `evidence_runs` stays empty by design because recurrence is evidence on its own.
2. **The window is a single flat 90 days.** There is no per-signal decay, so a module
   failing steadily for 89 days and one failing steadily for 1 day score identically.
3. **Signal kinds are limited to three.** Requirement gaps and unstable-test signals
   were not built, though the retrieval layer supports both.
4. **Confidence is a stated convention, not a statistical model.** That is deliberate
   and testable, but it means confidence is not a probability.

---

## 10. Phase 4 PASS gate

| Criterion | Status |
|---|---|
| Planning uses historical intelligence | yes — 3 signal kinds |
| Extend, not rebuild | yes — field added alongside; existing renderer and tests untouched |
| Structured reasons (signal, metric, evidence, confidence, window) | yes — asserted per field |
| No influence without evidence | yes — empty project produces no signals |
| Old history does not dominate | yes — 90-day window, tested both directions |
| Project isolation | yes — explicit test |
| Risk prioritisation | yes — evidence-ordered, bounded |
| No chain-of-thought | yes |
| Regression passes | yes — 1763 → 1777 |

**Phase 4: PASS.**