# AIQA — Development Requirements (Pending Work Only)

**Purpose of this document:** a single, prioritized catalogue of everything in AIQA that is still pending
— not yet built, not yet decided, or not yet independently confirmed — as of 23 August 2026. This is a
**requirements and reasoning document, not code.** No file in this pair (`AIQA_DEVELOPMENT.md` and its
companion `AIQA_IMPLEMENTATION.md`) contains or should ever contain source code — every item below is
meant to be handed to Claude Code as a natural-language instruction, the same way every prior unit in this
project was built.

This document answers **"what is still pending, and why."** Its companion, `AIQA_IMPLEMENTATION.md`,
answers **"in what order, and with what exact prompt do I make Claude Code build or resolve it."** Use
this one to understand and prioritize; use that one to actually execute, one prompt at a time.

Everything here is drawn from the current, real state of `AIQA_ACCEPTANCE_MATRIX_v1.0.md`,
`AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md`, and `AIQA_PRODUCT_DECISIONS_RECORD.md` — nothing here is
invented; every item below traces back to a real, disclosed gap or a real, still-open decision in this
project's own governing documents.

---

## Priority Tiers

- **P0 — Trust.** Nothing else should be fully relied on until these are resolved. These items are about
  confirming that what's already claimed as done is *actually* done, not about building anything new.
- **P1 — Decisions.** Only a human product owner can make these calls. No engineering should start on the
  affected items until each decision is made and recorded.
- **P2 — Build.** Real engineering work, unlocked once its P1 decision (if any) is made.
- **P3 — Housekeeping.** Small, low-risk, no dependency on anything else.

---

## P0 — Trust: Confirm What's Already Claimed

### P0.1 — Independent re-verification of ACC-022's new VERIFIED status
`ACC-022` (Recovery/Fault Tolerance) was just self-upgraded from `IMPLEMENTED` to `VERIFIED` by the same
session that ran the underlying test — a real orchestrator-process-kill-and-restart, plus reused
WebSocket-reconnect evidence from `E2E-005`. This project's own history shows self-verified claims have a
100% real-defect hit rate under independent re-check (both Phase 1 verification reports). This specific
upgrade also carries its own internal tension worth resolving: the same edit admits that one of `ACC-022`'s
five named durability dimensions ("unrelated tasks continue while one worker fails") is *architecturally
undemonstrable* in this single-process system, yet the status was still raised to the highest tier. Needs
a fresh session, no memory of the edit, using the discrimination-by-reversion technique already proven in
this codebase.

### P0.2 — Independent re-verification of E2E-003's PARTIAL downgrade
Same edit round self-downgraded `E2E-003` from `PASSED` to `PARTIAL` for a disclosed p95/p99 gap. Lower
priority than P0.1 (a downgrade is inherently a more conservative, lower-risk self-correction than an
upgrade), but should still get a second look in the same pass, since it was made by the same
non-independent session.

### P0.3 — Broader independent verification sweep (originally Wave 3 / Prompt 3.2)
Every backend unit from BE-12 onward, and all 26 frontend FE units, were verified only by the session that
built them. At minimum, sample: the realtime event stream (BE-14), the evidence/findings surface (BE-15),
Test Intelligence, the Execution Hub, and the AI router/Skills panel. Same discrimination-by-reversion
technique as P0.1.

### P0.4 — One real, human-observed click-through of all 15 pages
No one has ever manually clicked through this product start to finish, in a real browser, against a real
backend. This is the one kind of verification no automated suite substitutes for, and it is also the exact
thing the product owner is personally planning to do — this item can be done *by* the product owner
directly rather than delegated back to Claude Code, per the standing plan.

---

## P1 — Decisions Only the Product Owner Can Make

### P1.1 — The Ollama / AI-runtime dispute
`AIQA_AI_MODEL_AUTO_DISCOVERY_IMPLEMENTATION_REPORT.md` discloses a real, unresolved contradiction: one
session found and used a real local Ollama installation; the product owner separately stated no Ollama is
installed. The one test proving genuinely different AI output per skill selection was written but
deliberately never executed against an agreed-real target because of this. **Decision needed:** what AI
model runtime is actually meant to be available in the real deployment target this product will run on.
Nothing about the AI Model Gateway's live-verified status should be treated as final until this is settled
and the disputed test is re-run against whatever is agreed.

### P1.2 — Build-or-descope, one decision per item, for the 7 honest stub pages
AIQA Orchestrator, Custom Testing, Mobile Testing, Feature Map/Flow, and RBAC Testing's scenario-matrix
view are still real, honestly-disclosed gaps (Test Case Validator and API Testing are already resolved via
redirect, not pending). Each needs its own one-page recommendation — what real build would take, what
formal descope would take, which this project's actual trajectory suggests — decided one at a time, the
same pattern already used successfully for the six prior product decisions recorded in
`AIQA_PRODUCT_DECISIONS_RECORD.md`. Whatever is descoped needs the PRD/FRD/SRD reissued at a new version
number that actually matches the agreed real scope.

### P1.3 — Build-or-descope for the ~12 missing backend domains
Context ingestion, quality-graph queries, requirement traceability, RBAC scenario-matrix evaluation,
generic API-test dispatch/telemetry, mobile/device-farm dispatch, custom test suites, report generation as
a real durable entity, and the full Model Gateway beyond today's single-task router — same
recommend-then-decide pattern as P1.2, and the two lists overlap (several stub pages are the frontend face
of these same missing backend domains, so a single decision often resolves both at once).

### P1.4 — Fate of the six remaining fake Settings tabs
General, SSO, Execution, Environments, Policies, and Notifications are deliberately-preserved fake content
under a standing instruction in `SettingsPage.tsx`'s own header comment. That instruction was reasonable
short-term; it is a trust problem as a permanent state (a user configuring a "Policy" that silently does
nothing). Needs a build-or-honest-stub decision per tab, same as P1.2/P1.3.

### P1.5 — `apps/web`'s final fate
The abandoned, superseded frontend directory (port 5173) is no longer misleading anyone (`start.bat` was
fixed to launch the real `forentend` instead), but its own fate — delete it, archive it, or leave it — has
never been decided.

---

## P2 — Build (Unlocked Once Its P1 Decision Lands)

### P2.1 — Whichever P1.2/P1.3 items are decided "build," in decided priority order
The product owner has directed building step by step, not all at once. Once P1.2/P1.3 decisions land, order
the resulting build items by lowest effort/risk first and highest-value-with-real-dependents next, and
implement exactly one at a time, each as its own independently reportable unit — the same discipline
already used successfully for today's four-item build sequence (Test Case Validator, API Telemetry,
Reports Export, AI Bug Analysis).

### P2.2 — Whichever P1.4 tabs are decided "build"
Same one-at-a-time discipline, preserving the Authentication and AI Intelligence panels exactly as they
are.

### P2.3 — Make selected Skills actually affect generated output
The Skills & MCP Phase 1 panel lets a user genuinely select/deselect skills, and that selection is
captured — but by the feature's own explicit disclosure, it currently changes nothing about the generated
test-case content. This is real, scoped work independent of any P1 decision: connect `selectedSkillIds`
through to the AI-routed generation call so a genuinely different selection produces genuinely different,
provable output.

### P2.4 — A real MCP connector backend
The "Connect MCP" dialog currently and correctly discloses that no MCP backend exists. Building Phase 2 of
Skills & MCP (a real client that connects to at least one real MCP server, discovers its tools, and
surfaces them as real `SkillDefinition` entries) is a large, independent unit, not gated on any P1
decision above.

### P2.5 — Real reachability checks for the hosted AI providers
Gemini, OpenAI, and Anthropic currently only report `configured: true` from an environment-variable
presence check; `reachable` is hardcoded `false` for all three. Once real credentials exist for at least
one, add a genuine, bounded, low-cost live check analogous to Ollama's real `/api/tags` call, and make sure
a failed/expired credential reports honestly rather than staying stale.

---

## P3 — Housekeeping

### P3.1 — Fix the stale frontend test count in §34
`AIQA_ACCEPTANCE_MATRIX_v1.0.md` §34 still shows the frontend suite as 370 passed; the real, current count
is 371 (from the page-copy honesty pass). Small, no dependency on anything else — deliberately left
untouched during the most recent reconciliation pass specifically because it wasn't in that pass's stated
scope, not because it doesn't need fixing.

---

## Summary Table

| Tier | Item | Blocks | Blocked by |
|---|---|---|---|
| P0.1 | Independent re-verify ACC-022 upgrade | Trusting the new VERIFIED status | — |
| P0.2 | Independent re-verify E2E-003 downgrade | Trusting §28/§34's E2E rollup | — |
| P0.3 | Broader self-verification sweep | General confidence in self-verified units | — |
| P0.4 | Human click-through, all 15 pages | Final sign-off | Product owner's own time |
| P1.1 | Ollama/runtime dispute decision | Trusting the AI Model Gateway's GO verdict | Product owner |
| P1.2 | 7 stub pages: build-or-descope | P2.1 | Product owner |
| P1.3 | ~12 backend domains: build-or-descope | P2.1 | Product owner |
| P1.4 | 6 fake Settings tabs: build-or-descope | P2.2 | Product owner |
| P1.5 | `apps/web` fate | Repo cleanliness only | Product owner |
| P2.1 | Build decided-yes items from P1.2/P1.3 | — | P1.2, P1.3 |
| P2.2 | Build decided-yes Settings tabs | — | P1.4 |
| P2.3 | Skills → generation wiring | — | — |
| P2.4 | Real MCP connector backend | — | — |
| P2.5 | Hosted AI provider reachability checks | — | Real credentials for ≥1 provider |
| P3.1 | Fix stale §34 frontend count | — | — |

---

*This document contains no source code and describes no implementation detail beyond what's needed to
scope and prioritize the work. See `AIQA_IMPLEMENTATION.md` for the exact, ready-to-run prompts.*
