# AIQA — ACC-005 Test Case Validator: Investigation and Fix

**Date:** 22 August 2026
**Scope:** Build order item 1 of the confirmed sequence (`AIQA_SCOPE_RECOMMENDATIONS_REPORT.md`).
Investigate first, per the standing rule — do not assume a new page is needed.

---

## 1. What was found

**Redirect fix, not a new page — confirmed before writing any code.** The real validate → approve/
reject → handoff lifecycle was already fully wired in `TestCaseGeneratorPage.tsx`
(`handleValidateReal`/`handleApproveReal`/`handleRejectReal`/`handleHandoffReal`, all calling the real
`aiqaApi.testCases.*` client against the real backend), with real, status-gated buttons already rendered
in `GeneratedTestCasesView.tsx`: "Validate Test Case" appears only when a real, persisted case's status
is `PENDING_REVIEW` (backend: `GENERATED`); "Approve Test Case" appears only once it's genuinely
`VALIDATED`; "Reject" is available at either state. This UI already correctly enforces the real backend's
state machine — a case cannot reach Approve without first passing Validate, matching the backend's own
`409` guard (confirmed by reading `api.rs::approve_test_case_scoped`, not merely inferred).

**What had zero test coverage:** despite being fully wired, none of `validate`/`approve`/`reject` had a
single direct test anywhere in the frontend suite before this unit. `TestCaseGeneratorPage.test.tsx`'s
existing AUD-005 block tested persistence, save, and handoff — never validate/approve/reject.

**The dedicated `TestCaseValidatorPage.tsx` was a stale stub**, not a placeholder waiting to be built:
its own doc comment (written at FE-11, before AUD-005 existed) stated *"there is no requirement-
traceability entity, no test-case quality-scoring model… anywhere in the backend"* — true when written,
false since AUD-005 landed, never updated. No new frontend logic was needed anywhere — only a copy/
routing fix on this one page, plus tests for capability that already existed.

**A real, honest limit, disclosed rather than glossed over:** the real backend validator
(`api.rs::validate_test_case_scoped`) is **structural only** — title, description, steps, and
expected_result non-empty, every step has an action and an expected result. It does **not** implement
ACC-005's own richer Acceptance Criteria (duplicate/overlap detection, quality-benchmark scoring,
business-logic/RBAC/security coverage, risk assessment). The redirect page states this explicitly; this
report and the Acceptance Matrix update do not claim more than what was actually proven.

---

## 2. What was changed

| File | Change |
|---|---|
| `forentend/src/components/pages/TestCaseValidatorPage.tsx` | Rewritten: stale "not implemented"/"no backend capability" claim → honest redirect naming the Test Generator page, a real `navigate('/test-generator')` action, and an explicit disclosure of what real validation does *not* check |
| `forentend/src/components/pages/TestCaseValidatorPage.test.tsx` | **New**, 7 tests: unauthenticated gating, honest redirect copy, the "does not yet check" disclosure renders, no fabricated demo content survives, no invented backend request, real navigation, Dashboard non-regression |
| `forentend/src/components/pages/CaseCPages.test.tsx` | `TestCaseValidatorPage` removed from the shared "not implemented" bucket (now 5 of the original 7, alongside `APITestingPage`'s earlier removal) — doc comment and counts updated |
| `forentend/src/components/pages/TestCaseGeneratorPage.test.tsx` | **7 new tests** added to the existing AUD-005 block, no existing test touched: Validate success (real POST, real `GENERATED`→`VALIDATED` transition reflected by the real next button appearing), two distinct real rejection reasons (`no steps defined`, `expected_result is empty` — proof the UI surfaces whatever the backend actually said, not one fixed string), Approve success, Approve real rejection (`not in VALIDATED state`), Reject, and the real `#generated-test-cases-loading` state during an in-flight fetch |

**Zero new frontend logic was written.** Every assertion added exercises code that already existed
before this session; the only production-code change is the one page's copy and navigation target.

---

## 3. Verification

```
cd forentend && npx vitest run    → 351 passed, 0 failed, 19 files (was 341/18 before this unit)
cd forentend && npx tsc --noEmit  → clean
cd forentend && npx vite build    → clean, 2716 modules
```

Net new tests: +7 (`TestCaseValidatorPage.test.tsx`) +7 (new AUD-005 cases) −4 (removed from
`CaseCPages.test.tsx`) = +10, matching 341→351 exactly.

---

## 4. Acceptance Matrix updated

`AIQA_ACCEPTANCE_MATRIX_v1.0.md` ACC-005: Frontend/Integration moved from `NOT_STARTED`/`BLOCKED` to
`TESTED`; Test moved to `VERIFIED`; **Final moved from `IMPLEMENTED` to `TESTED` — deliberately not
`VERIFIED`.** Per this unit's own instruction not to mark verified beyond what the new tests actually
prove: the real, tested lifecycle (structural validation + approve/reject/handoff) is genuinely proven
end-to-end; ACC-005's full semantic criteria (duplicate/quality/business-logic/RBAC/security/risk
coverage) remain unimplemented and unproven, so `Final` reflects a real, tested, but partial capability,
not a fully closed acceptance criterion. §5A cites every file and test named above by name.

---

## 5. §1.2 correction to `AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md` — confirmed

**Corrected, same day, before this unit began building anything.** The original §1.2 descoped "Custom
Testing" against the broad, informal reading used by narrative audits (a generic, user-authored test-
suite/step-authoring framework) — but never checked that against `AIQA_ACCEPTANCE_MATRIX_v1.0.md`'s own
**ACC-015** criteria first, which describe something narrower and different: *"User can select: project,
module, feature, focused testing objective. Generated tests must remain scoped to the selected
objective."* That narrower reading was never put to the product owner and was never decided against.

§1.2 now explicitly scopes the descope to the broad generic-DSL/plugin reading only, states the narrower
ACC-015 reading is unaffected and remains open/undecided, and flags — without answering — that the
existing Test Case Generator already takes a `focus` input, which may substantially satisfy the narrower
criteria already. That question is left for a future session, not answered by this correction or this
report.

---

## 6. What this report deliberately did not do

- Did not build a new Test Case Validator page — investigation confirmed the real capability already
  exists elsewhere, so none was needed.
- Did not implement duplicate detection, quality-benchmark scoring, business-logic/RBAC/security
  coverage, or risk assessment — none of these exist in the real backend; building them was never in
  this unit's scope (build order item 1 was investigate-and-close the *page*, not extend the validator).
- Did not investigate or resolve ACC-015's narrower reading — flagged, not answered, per §5 above.
- Did not touch any backend file — `services/orchestrator/` is unmodified by this unit; every change is
  in `forentend/`.
