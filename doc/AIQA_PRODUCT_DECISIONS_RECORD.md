# AIQA — Product Decisions Record

**Date:** 22 August 2026
**Decided by:** the product owner, directly, in this session (`master.ai01@orangemantra.in`) — asked
one question per decision, not guessed, per this session's own governing instruction.
**Purpose:** close the six open decisions `AIQA_CONSOLIDATED_DOCUMENTATION_REVIEW.md` §9 and this week's
own Security Remediation report listed as blocking downstream work, with a real answer, a real owner, and
a real date, so no later document has to re-ask "who decides this?" again.

Each entry below is binding until formally superseded by a later dated decision in a document of this
same kind — not silently reinterpreted by whichever report happens to be written next.

---

## 1. Status naming — INFRA_ERROR vs INFRASTRUCTURE_ERROR

**Origin:** GAP-03 / Decision 2, `AIQA_PHASE4_FE15_PRODUCT_CONTRACT_DECISION_RESOLUTION_AUDIT_REPORT.md`.
**Question:** the shipped, tested, persisted backend value is `INFRA_ERROR`; the API Contract specifies
`INFRASTRUCTURE_ERROR`.

**Decision: correct the contract.** `INFRA_ERROR` remains the real, canonical value everywhere it
already is — code, database rows, tests, frontend. The API Contract document is the thing that was
wrong, not the shipped system.

**Why:** this was already the standing technical recommendation (FE-15 §9.2), pending exactly this
approval. Renaming a value that is already shipped, tested, and persisted in real data across every
layer would be real, risky, multi-file surgery to fix a naming mismatch that has zero functional
consequence — the contract catching up to reality is strictly cheaper and strictly safer.

**Follow-up (documentation only, not performed by this record):** update the API Contract document's
status-enum definition from `INFRASTRUCTURE_ERROR` to `INFRA_ERROR`, and note the correction with this
decision's date wherever the contract records its own change history.

---

## 2. Realtime event vocabulary — 11 shipped vs 15 contracted

**Origin:** GAP-05 / Decision 3, same source. Contract §14 (or §13, depending on which document —
see `AIQA_ID_REGISTRY.md` for the numbering-drift note) defines 15 event types; the backend emits 11
(`RUN_SCHEDULED`, 9× `TASK_*`, `RUN_COMPLETED`).

**Decision: trim the contract to the real 11.** The 4 extra contracted event types are not committed,
scheduled future work — they were speculative surface in the original contract draft that nothing has
ever been scoped to build.

**Why:** same principle as #1 — a contract describing capability that doesn't exist and isn't currently
planned is a liability (it invites an integrator to build against events that will never arrive), not an
aspiration worth preserving in a governing document. If any of the 4 removed types becomes real future
work, it re-enters the contract at that point, as part of the unit that actually implements it — not
before.

**Follow-up (documentation only):** remove the 4 unshipped event types from the API Contract's realtime
event section; note which 4 were removed and this decision's date.

---

## 3. ACC-012 — does the Execution Hub's executor picker satisfy "API Testing"?

**Origin:** Acceptance Matrix ACC-012, flagged "AMBIGUOUS, PRODUCT DECISION REQUIRED" by
`AIQA_PHASE4_FE15_PRODUCT_CONTRACT_DECISION_RESOLUTION_AUDIT_REPORT.md` §9 item 4.

**Decision, in the product owner's own words:** *"The underlying capability is real — Newman/Pytest/
Schemathesis run via the Execution Hub, this is real API testing. But the 'API Testing' page currently
shows 'not implemented,' which is confusing because the capability is actually hiding somewhere else.
For now: update that page so it clearly says 'this capability is in the Execution Hub, go there' — remove
the 'not implemented' message. Whether a full separate, rich API-testing screen (collections,
environments, request history, etc.) is needed — that's a separate decision for later."*

**Recorded as two distinct sub-decisions:**
- **ACC-012 itself: SATISFIED.** Choosing `newman`/`schemathesis`/`pytest` per task in the Execution Hub
  is accepted as the real, shipped API-testing capability. Mark ACC-012 verified in the Acceptance
  Matrix (Part C) on that basis.
- **A real, separate, confirmed defect — the dedicated "API Testing" page's copy is misleading.** It
  currently presents as `NotImplementedState`, actively telling a user the capability doesn't exist when
  it does, elsewhere. This is not a documentation-only fix; it is a source change (frontend page copy),
  explicitly out of scope for this documentation-only session. **Flagged as a follow-up task** (see the
  spawned task alongside this report) rather than silently left for someone to rediscover.
- **Not decided:** whether a full, separate, rich API-testing authoring UI (collections, environments,
  request history) is ever built. Explicitly deferred to a future, separate decision — not implied by
  either sub-decision above.

---

## 4. WebSocket authentication — query-param vs short-lived ticket/cookie

**Origin:** `G-WS`, `AIQA_INTEGRATION_IMPLEMENTATION_PLAN_v1.0.md` — *"a genuine contract ambiguity, not
an implementation defect"*; BE-14 shipped on the query-param technique explicitly without resolving it.

**Decision: keep the `?key=` query-parameter technique.** No change planned.

**Why:** already shipped and working across every WS-consuming surface (per-run and per-project
streams); every route it protects is still real-auth-gated regardless. The known downside — a real
Bearer-shaped key can land in server access logs or browser history via the URL — is accepted as
reasonable for the current deployment shape: a local, single-operator tool with no untrusted network
path to those logs.

**Revisit trigger:** if this orchestrator is ever deployed somewhere its access logs are not solely
under the operator's own control (matches the same trigger recorded for CORS and for sandboxing depth,
#6 below) — not a fixed date.

---

## 5. Identity model — API keys only vs a real user/session model

**Origin:** `G-AUTHN`, same source — *"API keys vs. a real user/session model"*, unresolved.

**Decision: stay API-key-only, indefinitely.** No user/session model is planned. Bearer API keys with
three fixed roles (`admin`/`operator`/`viewer`), scoped per-project, remain the entire identity model.

**Why:** correct and sufficient for the actual current shape of this product — one operator, or a small
number of operators sharing scoped keys, not a multi-tenant product with individually accountable human
users.

**Revisit trigger:** only if this product ever needs to support multiple distinct human users each with
their own account, audit identity, and login — not scheduled, not planned, no date attached.

---

## 6. Executor sandboxing depth — Docker/WSL vs defer vs accept as sufficient

**Origin:** this week's own `AIQA_SECURITY_REMEDIATION_FOUR_DEFERRED_FINDINGS_REPORT.md` §12 — real
Windows Job Object process-tree containment shipped and tested; filesystem/network isolation named as
the remaining, explicitly accepted residual risk pending exactly this decision.

**Decision: defer.** Keep the current Job Object containment (process-tree kill-on-close,
process-count and memory ceilings) as the interim, accepted level. Do not install Docker Desktop or
enable WSL at this time.

**Revisit trigger: before any non-local deployment.** The same trigger already recorded for CORS (#4
above) and effectively for #4/#5 in this record — this project has now recorded one consistent
deployment-shape line: everything currently accepted as "fine for a local, single-operator tool" is
explicitly revisited together, at the point this ever stops being one.

**What this decision does not change:** the Job Object mechanism itself is not provisional or a
placeholder — it is real, tested containment, staying in place regardless of whether Docker/WSL is ever
added later (the two are complementary, not alternatives — see the Security Remediation report §12).

---

## Summary table

| # | Decision | Answer | Revisit trigger |
|---|---|---|---|
| 1 | Status naming | Correct the contract to `INFRA_ERROR` | — (closed) |
| 2 | Event vocabulary | Trim contract to the real 11 | — (closed), re-add per-event as built |
| 3 | ACC-012 scope | Capability satisfied; page copy is a separate, flagged defect | Dedicated rich UI: open, undecided |
| 4 | WebSocket auth | Keep query-param | Any non-local deployment |
| 5 | Identity model | Stay API-key-only | Only if multi-user accounts become a real need |
| 6 | Sandboxing depth | Defer Docker/WSL | Any non-local deployment |

**One consequence worth stating plainly:** three of six decisions (#4, #5, #6) now share the identical
revisit trigger — "before any non-local deployment." That is not a coincidence this record is
introducing; it is the actual, honest shape of this project's current risk posture, made explicit instead
of scattered across three separate documents that each independently said "acceptable for a local tool"
without ever naming the same line together.
