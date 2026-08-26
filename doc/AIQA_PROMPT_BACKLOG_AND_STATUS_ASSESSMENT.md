# AIQA — Status Assessment & Prompt-Only Continuation Backlog

**Prepared:** 21 August 2026
**Prepared by:** an AI acting as a senior prompt engineer / software architect, at the user's request
**Contains:** an honest reading of where the project actually stands, and a prioritized library of ready-to-paste prompts for continuing it. **No source code appears anywhere in this document** — every item below is an instruction to hand to a developer or an AI coding agent, not an implementation.

### Method

I read the root `README.md`, both frontend READMEs, both `package.json` files, `Cargo.toml`, `docker-compose.yml`, the workers stub README, `.claude/launch.json`, the full git history (21 commits, 13–21 Aug 2026) and current `git status`/`git diff` signature, and directory/line-count structure across the whole repository. I then read, in full, the project's own **`AIQA_CONSOLIDATED_DOCUMENTATION_REVIEW.md`** — a 76-document meta-audit the project itself produced today — plus the two newest implementation reports (central AI model routing, Skills & MCP), and targeted sections of the two most recent full-project audits, the architecture/deep audits, and the Acceptance Matrix. I then independently re-checked several of the highest-stakes claims directly against the live source (CORS configuration, subprocess execution, the SSRF-able executor parameters, a "dead" database schema, a fake-data comment in the Settings page, and the git status anomaly) rather than taking any single report's word for it. I did **not** re-run the Rust or frontend test suites myself this session — that is the first prompt below, deliberately, because a report about a test count is not the same thing as a fresh green run.

No file in the repository was changed while producing this.

---

## Progress Log (kept current as real units land — do not let this drift the way the Acceptance Matrix did)

- **22 Aug — AI provider/model routing (Wave 4.1): DONE.** Live-verified end-to-end through a fresh Rust
  toolchain install, two independent cold builds, real Ollama calls, real HTTP calls, a live browser
  check, and a real process-restart persistence proof. Verdict upgraded **PARTIAL → GO**. Backend 177/1
  ignored, frontend 339/17 files, both independently reproduced twice.
- **22 Aug — Security remediation (Wave 1, all four dropped findings): DONE, 3 of 4 fully closed.** CORS
  is now an explicit allow-list; the SSRF hole is closed with real destination validation (including
  DNS-resolution checking, not just literal IPs); approval/rejection state is now durable across a real
  restart, and a *more severe* bug was found and fixed alongside it (a clean restart was silently
  force-failing every pending approval, not merely making it unreachable). Executor sandboxing is
  **partially** closed: a real Windows Job Object now guarantees process-tree kill-on-close plus
  process-count/memory ceilings, but filesystem/network isolation from inside an executor subprocess is
  explicitly, honestly **not** closed — see the new open decision below. Backend tests 177 → 190 (+13, all
  new, none weakened).
- **Correction:** the "git status shows almost every file as modified" issue flagged in §4 below turned
  out to be an artifact of the access path used to originally survey this repository, not a real issue —
  independently reconfirmed clean (`git status` clean, `HEAD` at `fa930fd`) from directly on the machine
  during the AI-routing verification above. The original Wave 0 git-hygiene prompt is retired; no action
  needed.
- **New open decision surfaced this week:** whether to install Docker Desktop or enable WSL so the
  executor-sandboxing fix can be extended from process-tree containment to real filesystem/network
  isolation. This requires a heavier install or admin elevation no session should take unilaterally —
  folded into Wave 2's decision list as item 6 below.
- **22 Aug — Governance catch-up (Wave 0.3/0.4/0.5 + all of Wave 2): DONE.** Four documents landed
  together: `AIQA_DOCUMENT_INDEX.md` (all 74 root docs catalogued current/superseded/historical),
  `AIQA_ID_REGISTRY.md` (disambiguates B# vs BE-#, the two BE-9s, the two BE-10s, FE-13/FE-15's
  non-feature nature), `AIQA_PRODUCT_DECISIONS_RECORD.md` (all six open decisions closed, by the product
  owner directly, exactly as walked through live), and a from-real-evidence rebuild of
  `AIQA_ACCEPTANCE_MATRIX_v1.0.md` (§5/§5A/§34/§35 — was 132/132 `NOT_STARTED`, now a real, cited,
  per-cell reconciliation). Wave 0.3 is done for the *documentation* contradiction (`apps/web`'s three
  misleading documents now carry dated superseding notes) but the record is honest that no decision was
  made on `apps/web/`'s actual long-term fate (delete/archive/keep) — that specific sub-question is still
  open, just no longer actively misleading anyone. All six Wave 2 decisions are closed with named revisit
  triggers (three of six — WebSocket auth, identity model, sandboxing depth — now explicitly share one
  trigger: "before any non-local deployment").
- **One small inconsistency caught in the rebuilt matrix, worth a fresh count next time someone touches
  it:** §5's own tally line claims "8 VERIFIED, 8 IMPLEMENTED/TESTED, 6 NOT_STARTED," but counting the 22
  Final-column cells directly gives 7 VERIFIED / 3 TESTED / 6 IMPLEMENTED / 6 NOT_STARTED (7+9+6=22, not
  8+8+6=22 as claimed). Doesn't change which items are VERIFIED or NOT_STARTED — every individual cell
  checks out against its own cited evidence in §5A — just the summary tally line is off by one between
  the VERIFIED and partial buckets. Flagging rather than silently repeating the claimed number.
- **The matrix's own headline finding, unchanged by any of this week's progress:** §34 **E2E Tests:
  NOT_VERIFIED** (the 7 formal E2E-001…007 scenarios have still never been executed) and §35 **Final
  Acceptance: PENDING PRODUCT-OWNER SIGN-OFF** (every technical field is real and cited; what's missing
  is the product owner's own explicit sign-off statement, not another audit). This is now the single
  biggest remaining gap in the whole project, by the project's own definition of done.
- **Sequencing note, recorded here the same way the project records its own decisions:** this document's
  §7 originally recommended Wave 3 (E2E proof) before Waves 4-5 (build/descope the rest). The product
  owner has since explicitly chosen the opposite order — build the remaining not-real modules first,
  step by step, then test — in this session's own conversation. Recorded here, not silently overridden:
  the tradeoff is that further building now adds to what Wave 3 will eventually have to prove works,
  rather than confirming the existing ~40 units are solid before adding to them. Proceeding on the
  product owner's explicit instruction.
- **Still fully open:** Wave 3 (E2E proof — now the standout gap, see above), Wave 4.2 (the seven honest
  stubs + missing backend domains — up next), Wave 5.1/5.3/5.4. Wave 5.2 (Skills → generation influence)
  was prompted; no implementation report received yet as of this update.

---

## Today's Build Phase Plan — UPDATED after the real decision round (22 Aug, later same afternoon)

The 12-item decision round actually ran (`AIQA_SCOPE_RECOMMENDATIONS_REPORT.md` +
`AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md`), taken live from the product owner, one question at a time.
**This supersedes the six-phase plan directly below** — several phases in it turned out to be
DEFER/DESCOPE, not BUILD. Recorded here rather than silently deleted, per this project's own practice.

**Part A — done:** ACC-012 fix re-verified fresh this session (a separate, since-deleted background
session had already made the same fix; it was not trusted, it was redone/reverified from scratch — 341
passed/18 files, up from 339/17, clean `tsc`, clean build). Matrix ACC-012 moved TESTED → VERIFIED.
**Worth double-checking, not yet independently confirmed:** the report's tally line now claims "9
VERIFIED" — my own direct row-count before this fix gave 7 VERIFIED, so +1 for ACC-012 should read 8, not
9. Same kind of tally-line arithmetic slip flagged earlier in this log. Doesn't change which items are
real; worth a fresh count next time the matrix file itself is opened directly.

**Decisions from the 12-item round:** 3 **descoped** (API Testing's separate rich authoring UI, Custom
Testing's generic authoring/DSL, the generic API-test dispatch entity), 6 **deferred** with named revisit
conditions (AIQA Orchestrator, Mobile Testing, Feature Map/Flow visual, RBAC scenario-matrix, Context
Ingestion, Quality-graph queries), 4 **to build** (Test Case Validator page, API telemetry UI, Reports
export, Model Gateway task-type expansion). Full reasoning per item lives in
`AIQA_SCOPE_RECOMMENDATIONS_REPORT.md`; the three descopes and six deferrals are now the addendum-of-
record in `AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md` (v6.0 PDF stays the unedited historical original).

**Two things flagged back to the product owner, not silently corrected:**
1. **Custom Testing / ACC-015's descope reads broader than intended.** Earlier in this session, when this
   decision was first walked through live, the guidance given was: descope only the *generic execution-
   DSL/plugin* reading of Custom Testing — NOT the Acceptance Matrix's own narrower ACC-015 acceptance
   criteria (§20: "user selects project/module/feature/focused objective; generated tests remain scoped to
   it" — a reuse of the existing generator/execution path, not a new framework). `AIQA_PRD_FRD_SRD_v6.1_
   SCOPE_CORRECTIONS.md` §1.2 as written descopes "Custom Testing" without carving that narrower reading
   back out. **This needs a small correction to §1.2** before the smaller, legitimate, still-worth-building
   version of ACC-015 gets lost inside a broader descope it was never meant to cover.
2. **RBAC scenario-matrix (item 6) was decided DEFER against the report's own BUILD recommendation** — the
   report is explicit this went the other way from what it assessed as one of the cheapest, lowest-risk
   items on the whole list, and records the disagreement honestly rather than re-litigating it. This
   specific question wasn't one of the screenshots shared in this conversation, so no second opinion was
   given on it here. Flagging in case it's worth a second look, not overriding the decision as recorded.

**Confirmed final build order for today — ALL FOUR DONE (22 Aug, evening):**
1. **Test Case Validator page — DONE.** Investigated first; the real lifecycle was already wired in
   `TestCaseGeneratorPage.tsx` with zero test coverage. No new frontend logic — a redirect/copy fix on the
   stale dedicated page (same shape as ACC-012) plus 14 new tests (7 direct, 7 redirect page). ACC-005:
   NOT_STARTED/BLOCKED → TESTED (Final stays TESTED, not VERIFIED — the real validator is structural-only,
   ACC-005's richer semantic criteria remain unimplemented, honestly disclosed). Also fixed §1.2 of
   `AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md` per the flagged catch — Custom Testing's descope now
   explicitly excludes ACC-015's narrower project/module/feature-scoped reading.
2. **API telemetry UI — DONE.** New `/telemetry` page (`TelemetryPage.tsx`) consuming the real, pre-existing
   `GET /metrics` for the first time anywhere in the frontend. Honest, load-bearing finding: the real
   endpoint only emits six status/count gauges — no latency/duration/histogram data exists anywhere in the
   backend, so p95/p99 genuinely cannot be shown; the shipped page discloses this explicitly rather than
   fabricating a number. ACC-013: NOT_STARTED → TESTED.
3. **Reports export (PDF/Excel) — DONE.** Investigated first what "Bug/Test/Final Audit Report" resolve to
   (real Findings, real Dashboard/KPIs, and a computed reconciliation of the two — no 4th data source
   invented, no fake "module" filter added). Real `jspdf`/`xlsx` client-side generation, dynamic-imported
   after measuring a real bundle-size regression, a real font-encoding bug found and fixed empirically, a
   dead-code fallback branch found and removed. 11 new tests assert actual generated content, not blob
   existence. ACC-016: IMPLEMENTED → TESTED. **Caught and fixed, with disclosure, the recurring tally-line
   miscount** flagged twice earlier in this log — confirmed the real split is 8 VERIFIED / 9 TESTED+
   IMPLEMENTED / 5 NOT_STARTED, matching this log's own independent count. §34/§35's stale frontend test
   counts also refreshed.
4. **Model Gateway — `BugAnalysis` task type — DONE.** `AiTask::BugAnalysis` already existed unused in the
   router enum; the real work was building its one missing caller. Wired as an on-demand (HTTP-triggered,
   not synchronous-in-run-completion) AI-assisted root-cause hypothesis layer over the existing rule-based
   failure-clustering, additive-only (4 new nullable columns, never touching the existing deterministic
   `root_cause_hypothesis`/`confidence`), never auto-promoted (a human still reviews via the existing real
   endpoint). **Independently reached its own live GO** — a real local Ollama call against two genuinely
   different real failure signatures produced two genuinely different hypothesis texts (not template
   substitution), the same standard the original AI router GO was held to. ACC-018 Final stays VERIFIED
   (extension of an already-VERIFIED capability that itself also went live). 16 new backend tests, mapped
   to ACC-018's own 5-item verification checklist.

**Page-copy honesty pass — DONE (22 Aug, evening, after the 4 build items above).** Investigated whether
Test Case Generator's "focus" field already satisfies ACC-015's narrower reading before touching any page:
**genuinely no** — no module/feature selector exists, the field doesn't provably constrain the local
generator's output, and it isn't even wired to the real AI-generation backend path at all. ACC-015 stays
`NOT_STARTED`, correctly, not forced into a fix it doesn't qualify for. The other 4 deferred stub pages
(AIQA Orchestrator, Mobile Testing, Feature Map/Flow, RBAC scenario-matrix) had their copy changed from a
generic "NOT IMPLEMENTED" to a specific statement of their real status: deferred (not rejected), citing
`AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md` and each one's own actual named revisit condition. Tests
confirm the old generic string is gone from all 4 and each page's revisit condition is genuinely distinct
text, not one sentence reused four times. Custom Testing's page was correctly left untouched (its generic
copy remains accurate for a genuinely `NOT_STARTED`, undecided item). **All 7 of the original honest-stub
pages are now accounted for: 2 fixed to real capability (API Testing, Test Case Validator), 4 specifically
explain their real deferred status, 1 (Custom Testing) correctly still says "not implemented" because
that's still true and nothing was found to justify changing it.**

**22 Aug — End-of-day checkpoint (fresh, from-scratch verification): DONE.** Full `cargo clean` + rebuild +
backend test run reproduced 206/0/2 exactly, matching every report above — not just re-reading old numbers.
Frontend cache-clear + rebuild + test run reproduced 371/0/20 exactly. Live dual-server check: real 401 from
the orchestrator's own auth middleware, zero console errors on the real frontend. This checkpoint is what
caught the one real defect of the day, described next — not assumed, not guessed, found by actually running
the product exactly as tomorrow's tester would.

**23 Aug — `start.bat` fix: DONE.** The end-of-day checkpoint found `start.bat` was launching the wrong,
abandoned frontend — `apps\web` on port 5173 — instead of the real, fully-tested `forentend` on port 3000,
which every single unit built today was actually written and tested against. Fixed: directory and every
echoed port reference corrected to `forentend` / 3000; `stop.bat` needed no change (it already swept port
3000 as a pre-existing leftover from an earlier cleanup). Re-verified live, 3 times over 33 seconds for
stability, not just a momentary bind: orchestrator on 8080 gave a real, structurally-correct 401
UNAUTHENTICATED; forentend on 3000 gave a real 200 with genuine Vite dev-server HTML. Process identities
confirmed directly (`aiqa-orchestrator.exe`, real `node.exe`), not assumed from an open port. One honest,
disclosed environmental wrinkle — `start`'s `cmd /k` wrapper throws "Input redirection is not supported"
when launched from a console-less automated context — was isolated, explained, and confirmed to not apply
to a normal human double-click; worked around only for the purpose of completing verification. Stopped
cleanly afterward via the real `stop.bat`; nothing left listening on 8080/3000/5173 after. `apps/web` itself
was left untouched — no decision made about deleting it, only about what `start.bat` launches.

**Today's build phase is now genuinely, fully complete.** Combined with the 3-descoped/6-deferred scope
decisions, every page in the product is now either real or honestly, explicitly labeled — nothing is
silently broken or fake — and the one thing that could have quietly wasted tomorrow's manual testing time
(a start script pointing at the wrong, untested frontend) has been found and fixed, not just built around.
Double-clicking `start.bat` tomorrow now launches exactly the frontend every test, every report, and every
acceptance-matrix row today was actually verified against.

**What's still open, deliberately not started without a fresh check-in (per this session's own stated
rule):** Wave 3 — the formal E2E-001…007 walkthrough and a genuine live full-OS-process-kill recovery test
(still the Acceptance Matrix's own biggest named gap, and the planned next step, starting from a correctly
wired `start.bat`); the 6 deferred items (AIQA Orchestrator, Mobile Testing, Context Ingestion, Quality-graph
queries, Feature Map/Flow visual, RBAC scenario-matrix) — each has a named revisit condition in
`AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md`, none is scheduled; Wave 5's remaining items (Skills →
generation wiring — status still unconfirmed since it was first prompted, real MCP connector backend); and
`apps/web`'s own final fate, still undecided though no longer misleading anyone.

---

## Original Today's Build Phase Plan (22 Aug, afternoon, superseded by the block above)

The product owner asked directly for a priority order and phase-by-phase build prompts today, aiming to
start manual testing tomorrow. Recorded here as a decision, the same way this project records its other
sequencing calls: the separate "scoping recommendation only, don't build yet" half of Wave 4.2 (Part B,
as originally handed over) is superseded by this direct prioritization — moving straight to building,
phase by phase, rather than a standalone scoping report first.

**Honest framing, stated plainly rather than silently overpromised:** finishing every remaining stub page
and missing backend domain today, to the real/tested/verified standard this project has held everywhere
else, is not realistic for all of them. The phases below are ordered so the fastest, lowest-risk,
already-has-real-backend-behind-it items land first and for real today; a few items are explicitly
flagged as unlikely to finish today (or blocked outright) rather than rushed to a fake finish.

**Phase 1** — ACC-012 copy fix + a real Test Case Validator page (both reuse already-real, already-tested
backend capability — lowest risk, highest immediate payoff).
**Phase 2** — RBAC Testing scenario-matrix page (small new backend endpoint over already-real enforcement
+ a frontend view).
**Phase 3** — Custom Testing (reuse the existing generator/execution path, add project/module/feature/
objective scoping).
**Phase 4** — API Latency/Telemetry (surface timing data from real executions; p95/p99 aggregation).
**Phase 5** — Quality Graph real routes + the Feature Map/Flow page (the schema already exists,
unused — this is genuinely new traceability-query logic on top of it).
**Phase 6** — Reports completion (Final Audit Report + PDF/Excel export).

**Flagged as likely spilling past today — not to be rushed:**
- **AIQA Orchestrator visualizer** (ACC-006's real task-routing/prioritization engine across testing
  categories) — architecturally the largest NOT_STARTED item; deserves its own multi-day unit, not a
  same-day squeeze.
- **Context Ingestion** (ACC-002) — a genuine multi-part upload/parse/state-machine pipeline across many
  document types; same reasoning.
- **Mobile Testing** (ACC-014) — blocked on more than time: there is no real device/emulator in this
  environment to verify against, the same limitation Maestro's own code comment already discloses. More
  code here today would repeat the exact "looks built, never proven" pattern this whole engagement has
  been correcting elsewhere. Needs a product-owner decision on investing in a device/emulator before more
  code is written, not a rushed build.
- **Settings tabs** (Wave 5.1) — needs a per-tab build-or-honest-stub decision first (SSO alone is a
  substantial standalone integration).
- **Skills → generation wiring** (Wave 5.2, already separately prompted) and **real MCP backend**
  (Wave 5.3) — can slot in as bonus phases if today's core phases finish with time remaining.

---

## 1. What AIQA Actually Is

AIQA is specified, across a PRD/FRD/SRD v6.0 and a Business Logic & Architecture v2.0, as an ambitious project-centric AI quality-engineering platform: ingest project context, maintain a living quality model, generate and validate tests, route work through a central orchestrator to specialized testing capabilities, execute through a common platform, stream results live, investigate failures, preserve evidence, and produce reports — explicitly "not limited to test-case generation."

What has actually been built, in roughly eight days of very dense, single-developer, AI-agent-assisted work (13–21 August 2026), is **narrower than that vision but genuinely real**: a Rust/Axum orchestrator with a real dependency-DAG scheduler, real subprocess test executors, durable SQLite persistence, server-enforced authentication/RBAC/project isolation, real WebSocket event streaming, real evidence capture, a rule-based (not LLM-based) failure-intelligence layer, and — as of today — a real central AI provider/model-routing layer feeding the test-case generator. A React/TypeScript frontend is wired to essentially all of it.

The single most important thing to understand about this project is one its own internal audits already say clearly: **the engineering is unusually disciplined, and the paperwork does not match the product.** Individual reports are careful and refuse to fabricate data. But the document that is supposed to answer "what percentage of this is done" (the Acceptance Matrix) still says almost nothing has started, two competing frontends both got signed off as "the" frontend, and a handful of real security findings were raised once, deferred, and then quietly never mentioned again. None of that is a reason to distrust the code — it's a reason to fix the layer above the code before adding more to it.

---

## 2. How Far It's Actually Built

### 2.1 Backend (`services/orchestrator/` — Rust/Axum/SQLx, ~14,000 lines of source, roughly matched line-for-line by test code)

| Capability | Status | Note |
|---|---|---|
| Auth (bearer API keys, 3 roles), project scoping/multi-tenancy | **Real** | 14 server-side `require_role` gates confirmed directly in `api.rs`; every route but `/health` requires a key |
| Execution engine — dependency DAG, cycle detection, concurrent dispatch, cancellation, approval gates, crash recovery | **Real** | `engine.rs`, `scheduler.rs`, `policy.rs` |
| Executors — Playwright, Newman, Pytest, Schemathesis | **Real**, verified end-to-end as real subprocess invocations | Maestro adapter is real code but unverifiable in this dev environment (no mobile device) |
| Evidence capture (stdout/stderr, local disk) | **Real** | not MinIO — see §2.3 |
| Audit log, Findings/failure clustering, Test Intelligence (risk/value/flakiness scoring) | **Real**, explicitly rule-based/deterministic, never LLM-backed | `intelligence.rs` |
| Realtime event stream (WebSocket, per-project) | **Real** | dedup, backoff, lag handling all present and tested |
| Test-case persistence & lifecycle (`GENERATED → VALIDATED → APPROVED → EXECUTED`) | **Real, newest** | added today; 9 routes, conditional-SQL state enforcement |
| Central AI provider/model routing (`ai_router.rs`, 1,107 lines) | **Real architecture**, one open question | See §2.4 |
| Dashboard KPIs | **Real but partial** | only 3 of 11 documented KPI concepts have backing data/queries |
| Distributed Python worker fleet (`services/workers/`) | **Pure stub** | every adapter `raise`s `NotImplementedError`; nothing here talks to the orchestrator |
| Postgres/pgvector, Redis, MinIO (`infra/`) | **Not wired** | `docker-compose.yml` describes the target; the running system uses SQLite + local disk instead, and the README says so honestly |
| Executor sandboxing/container isolation | **Not implemented** | see §4 — this is the most important open item |

### 2.2 Frontend — two directories exist; only one is what's actually shipping

- **`forentend/`** (the misspelling of "frontend" is real, was flagged early, and was never corrected — it is now permanent across the whole codebase) is the frontend that **every single one of the 26 "FE" development units actually landed in**: 15 pages, react-router, a 339-test Vitest suite across 17 files, and the real integrations described below. `.claude/launch.json` — the actual dev-launch configuration — points only at this directory.
- **`apps/web/`** was, on 17 August, formally audited and declared *"RECOMMENDED FRONTEND DIRECTORY — CONFIDENCE: HIGH,"* with a plan to delete `forentend/`, and its own consolidation work was separately signed off "PASS" twice. None of the subsequent 26 FE units went there. It has real API wiring for an earlier, smaller feature set (Dashboard, Runs, RunDetail, Settings, Intelligence, Audit, BugDrafts) plus 12 honestly-empty "staged" placeholder views, but it has had no new work since, has no test suite, and is not what `npm run dev`/the launch config actually starts. **It is an orphaned track that three separate documents still describe as the shipping product.** This is the single largest unresolved contradiction in the whole documentation set, and it needs a decision, not another audit.

Of `forentend/`'s 15 pages:

| Page | Status |
|---|---|
| Dashboard | Real (SQL-backed), but only 3 of 11 intended KPI concepts are populated; the activity-timeline chart is honestly empty |
| Execution Hub | Real — DAG editing, approvals, cancellation, live resync |
| Evidence, Audit Trail, Reports (Findings + Test Intelligence) | Real, backend-persisted, realtime-refreshed |
| Test Case Generator | Real persistence/lifecycle (new) + real AI-routed generation when a model is reachable (new, see §2.4); Skills & MCP panel is real selectable UI state, but selected skills **do not yet change** what gets generated, and nothing is persisted across a reload |
| Settings | Only the Authentication tab and the brand-new AI Intelligence panel are real; General/SSO/Execution/Environments/Policies/Notifications tabs are **deliberately preserved fake content**, under an explicit "do not remove existing fake data" instruction written directly into the file's own header comment (confirmed by direct inspection) |
| AIQA Orchestrator (plan/DAG visualizer), API Testing, Custom Testing, Mobile Testing, Feature Map/Flow, RBAC Testing (scenario-matrix visual only — real RBAC *enforcement* lives in the backend and is separately real), Test Case Validator | **Honest stubs.** Each renders an explicit "not implemented" state instead of fake data. This is 7 of 15 pages. |

### 2.3 Infrastructure reality vs. aspiration

The SRD's target topology is Postgres+pgvector, Redis-coordinated distributed workers, and S3-compatible object storage. What is actually running is single-process, single-machine: SQLite (via `sqlx`) and local-disk evidence blobs. The README discloses this honestly. This is a legitimate, working MVP substitute — but it means none of the distributed-worker, horizontal-scaling, or multi-instance-approval story in the original spec exists yet, only its contract stub.

### 2.4 The two newest capabilities (added today, not yet covered by any full-project audit)

- **Skills & MCP Phase 1** — a real, extensible skill-selection UI (8 built-in skills, a Skill Manager, a "Connect MCP" dialog) that is honest about what it isn't: no MCP backend exists at all, selecting a skill does not change generated output, and a custom skill created in the Skill Manager is gone on reload. This is disclosed in the feature's own UI copy, not hidden.
- **Central AI provider/model routing** — real Ollama discovery (a genuine, bounded HTTP call), real presence-only checks for Gemini/OpenAI/Anthropic (an environment variable existing, never a live authenticated call), a documented capability-scoring algorithm, AUTO/MANUAL routing, and truthful fallback, all now sitting in front of the test-case generator. Its own report rates itself **PARTIAL, not GO**, for an unusually candid reason: this session discovered a live, working local Ollama installation, generated a real test case with it — and then the user stated their system has no Ollama installed at all. That contradiction was never resolved. The one test that would prove live, differential AI behavior is written but was deliberately left un-run. Everything else about the feature (discovery, scoring, fallback, persistence, security) is independently proven by 177 passing backend tests; only the "is there really a model behind this" question is open.

---

## 3. The Five Things That Matter Most Right Now

**1. Two frontends both got a "PASS."** `apps/web/` was formally chosen as the official frontend with a plan to delete the other one; every subsequent line of real product work went into the other one anyway. Nobody has retired either decision. Anyone who opens the repo cold and reads the consolidation reports will build on the wrong directory.

**2. Four real security findings were raised once and then vanished.** Missing executor sandboxing (rated **Critical**), SSRF through executor URL parameters (a confirmed path to a cloud metadata address), CORS wide open to any origin, and approval state that a process restart quietly erases — all four were deferred to "Phase 2" on 17 August and never mentioned again in the ~55 documents written since, including two "fresh full audits" that report a clean security bill of health. I checked the source directly: **the CORS policy and the un-sandboxed subprocess execution are still exactly as described, today.** A clean recent audit is not a retraction of an older Critical finding that audit never looked for.

**3. The project's own "definition of done" says almost nothing is done.** The Acceptance Matrix — 132 cells across 22 requirement areas — is essentially still 100% `NOT_STARTED`, after roughly forty completed, tested units of work. Three other documents independently re-graded the same 22 areas and got a real, mixed, partly-positive picture. None of those re-gradings was ever written back into the matrix that's supposed to be authoritative.

**4. Nobody has ever actually clicked through the finished product, or actually broken it on purpose.** All seven end-to-end scenarios the SRD names remain unexecuted. Recovery-from-crash is verified by reading the code, not by killing the process and watching it recover. Verification quality is genuinely excellent through the first third of the project — two independent passes, run by a different reviewer than the implementer, actually found and overturned real defects — and then it stops. Every unit since (roughly 15 backend endpoints, all 26 frontend units, the new AI router, the new Skills UI) verified itself.

**5. The newest, most ambitious feature is honestly built but unproven live.** The AI routing layer is real engineering with excellent test coverage, but it currently cannot say with confidence that a real model is behind it, because of an unresolved factual disagreement about whether Ollama exists on this machine at all. That has to be settled before anyone can claim the AI features actually work end-to-end, not just that the plumbing around them is correct.

---

## 4. What I Personally Verified in the Live Source (not just relayed from a report)

- `services/orchestrator/src/main.rs:129` — `CorsLayer::new().allow_origin(Any)` — confirmed, current.
- `services/orchestrator/src/executor.rs` — `shell_command()` invokes `cmd /C` / a raw `std::process::Command` directly; `sanitize_token`/`safe_param` do real allowlist-based string sanitization, which is not the same thing as process isolation — confirmed, current.
- `services/orchestrator/src/executor.rs` — the `pytest`/`schemathesis` adapters take `base_url`/`url` straight from task parameters with no destination-host restriction beyond character sanitization — confirmed, current.
- A repository-wide search for `change_impacts`/`quality_relations` (two tables defined in `migrations/0002`) across every backend source file returns **zero matches** — confirmed dead schema.
- `forentend/src/components/pages/SettingsPage.tsx` — the "do not remove existing fake data" instruction is a real, verbatim comment in the file, appearing twice.
- The current `git status` shows essentially every tracked file as "modified," which looks alarming — but `git diff --shortstat` shows exactly equal insertion and deletion counts on every file (e.g., 260 files changed, 80,158 insertions, 80,158 deletions), the textbook signature of a line-ending conversion, not real content drift. `core.fileMode` is already `false`. This is cosmetic, not a lost-work risk — but it is worth cleaning up, because right now it makes `git status` useless as a signal for anyone working in this tree.
- Actual commit history (21 commits, 13–21 Aug) shows the "all work since FE-11 is uncommitted" risk flagged by the project's own consolidated review **has since been committed** — the newest commits on `master` visibly include that span of work. That specific critical risk looks resolved as of right now; treat that as provisional until Wave 0 below re-confirms it with a fresh, live check, the same way everything else in this section was checked rather than assumed.

---

## 5. How to Use the Prompts Below

Each numbered prompt is written to be pasted, on its own, to an AI coding agent with full read/write access to this repository (this project has clearly been built with one, and its own report style is the model to keep following). A few are marked **(human decision)** — those are for you and whoever owns the product, not something to hand to an agent to resolve unilaterally. Work through the waves in order; later waves assume earlier ones landed. Every implementation prompt tells the agent to keep the project's own existing discipline — verify live, disclose what wasn't checked, never fabricate a result — because that discipline is this project's biggest asset and the easiest thing to lose by rushing.

---

## 6. The Prompt Backlog

### Wave 0 — Trust the Ground You're Standing On

**Prompt 0.1 — Fresh, live re-verification of current build/test state**
```
Before making any change, independently re-verify the project's current state from scratch. Run the
full backend test suite (cargo test, from services/orchestrator) and the full frontend test suite
(vitest run, from forentend), both from a clean build, and record the exact pass/fail counts and file
counts. Compare them against the counts claimed in AIQA_AI_MODEL_AUTO_DISCOVERY_IMPLEMENTATION_REPORT.md
(177 passed / 1 ignored backend, 339 passed across 17 files frontend). If the numbers differ, say so
plainly and investigate why before doing anything else. Also run a production build on both the
backend and forentend and confirm both are clean. Do not proceed to any other prompt in this backlog
until this one produces a real, fresh, passing baseline you have personally observed.
```

**Prompt 0.2 — Git hygiene: stop line endings from hiding real changes**
```
git status currently shows nearly every tracked file in this repository as modified, but git diff
--shortstat shows exactly equal insertion and deletion counts across the board — a line-ending
normalization artifact, not real content drift. Diagnose the exact cause (likely a core.autocrlf /
.gitattributes mismatch between how this repository was authored and how it was checked out on this
machine), fix the configuration so it cannot recur, and get the working tree back to a state where
git status only shows files with genuine content changes. Do not use this as an opportunity to also
commit unrelated changes — this should be a hygiene-only pass, and it should be a single, clearly
labeled commit.
```

**Prompt 0.3 — Resolve the two-frontend contradiction**
```
Read AIQA_FRONTEND_PRE_IMPLEMENTATION_AUDIT.md, AIQA_FRONTEND_CONSOLIDATION_REPORT.md,
AIQA_FRONTEND_CONTRACT_READINESS_MATRIX.md, AIQA_FRONTEND_PHASE2_INTEGRATION_READINESS_REPORT.md, and
AIQA_INTEGRATION_IMPLEMENTATION_PLAN_v1.0.md §1.1/§2. These documents disagree about which of
apps/web/ and forentend/ is the real product frontend. In fact, every one of the 26 FE-1 through FE-26
development units landed in forentend/, which is also the only directory .claude/launch.json actually
starts. Present this contradiction and its evidence to the product owner and get an explicit decision:
either formally retire and archive or delete apps/web/, or formally justify keeping both. Whatever is
decided, add a clearly dated note to the top of all three apps/web-track documents stating the actual
current status, so a future reader is never again misled into building on the wrong directory. Do not
delete anything before that decision is made and recorded.
```

**Prompt 0.4 — Rebuild the Acceptance Matrix from real evidence**
```
AIQA_ACCEPTANCE_MATRIX_v1.0.md is the project's stated definition of "done," and it currently shows
essentially all 132 cells as NOT_STARTED despite roughly forty completed, tested units of work. Three
existing documents already independently re-assessed the same 22 ACC areas with real evidence:
AIQA_PHASE4_FE13_DOCUMENTATION_CONTRACT_DRIFT_DEAD_CODE_CLEANUP_IMPLEMENTATION_REPORT.md §8,
AIQA_PHASE4_FE15_PRODUCT_CONTRACT_DECISION_RESOLUTION_AUDIT_REPORT.md §9.2, and the most recent fresh
full-project audit's module-by-module table. Reconcile these three assessments against each other and
against a fresh, live check of the actual running system (not just a re-read of the reports), area by
area, and update the matrix's actual cell values and its Final Verification Record to reflect only what
you can currently prove, citing the exact file/route/test that backs each VERIFIED cell. Where the
three source documents disagree, say so in the matrix rather than picking one silently. Anything you
cannot currently verify stays NOT_STARTED or BLOCKED — do not mark anything VERIFIED on the strength of
a report alone.
```

**Prompt 0.5 — Publish a document index and a canonical ID registry**
```
This repository has roughly 70 audit/implementation/report markdown files in its root with no index,
no changelog, and no marking of which documents are superseded. Work-item IDs also collide across
phases: B1-B15 (Backend Plan defect IDs) versus BE-1-BE-15 (Integration Plan milestones) are
unrelated numbering systems that look identical; there are two different reports both titled around
"BE-10"; FE-13 and FE-15 are redefined differently by different later documents. Produce a single new
document, AIQA_DOCUMENT_INDEX.md, that lists every existing report with a one-line description, its
status (current / superseded / historical), and, where applicable, which later document superseded it.
Alongside it, produce a canonical work-item ID registry that disambiguates every collision described
above. Do not rewrite or delete any existing report to do this — only add the index and registry.
```

### Wave 1 — Close the Four Dropped Security Findings

**Prompt 1.1 — Executor sandboxing (the Critical, unretracted finding)**
```
services/orchestrator/src/executor.rs invokes Playwright, Newman, Pytest, Maestro, and Schemathesis as
direct host subprocesses (via cmd /C on Windows), with only string-level parameter sanitization
(sanitize_token/safe_param) standing between untrusted task input and the command line — there is no
process, container, or VM-level isolation. This was rated Critical in AIQA_ARCHITECTURE_AUDIT_REPORT.md
on 17 August and has not been mentioned, fixed, or formally retracted in any of the roughly 55
documents written since. Investigate what level of isolation is actually achievable in this project's
real deployment environment (container-based sandboxing, a restricted service account with a minimal
filesystem/network surface, or another concrete mechanism), propose the smallest change that closes
the gap without breaking the existing five executor adapters and their current test coverage, and
implement it. If full sandboxing genuinely cannot be done in this environment right now, do not silently
leave the finding open again — write a new, dated report that either formally accepts the risk with a
named compensating control and an owner, or explicitly schedules the fix, so it cannot quietly vanish
from the record a second time.
```

**Prompt 1.2 — Close the SSRF hole in executor URL parameters**
```
The pytest and schemathesis executor adapters in services/orchestrator/src/executor.rs accept
base_url/url directly from the task's params with no restriction beyond character-level sanitization,
which a Backend Audit already confirmed can reach a cloud metadata address. Add a real destination
check before these adapters are allowed to run: block link-local, loopback-outside-the-intended-target,
cloud-metadata, and other private/internal network ranges by default, and make the actually-intended
target (this orchestrator's own local health endpoint) an explicit exception rather than the accidental
byproduct of a default value. Add a test that proves a metadata-range or otherwise disallowed URL is
rejected before a subprocess is ever spawned, alongside the existing sanitize_token/safe_param test
coverage. Document the resulting trust model (what hosts are and are not reachable from these
executors) in executors' own README files.
```

**Prompt 1.3 — Lock down CORS**
```
services/orchestrator/src/main.rs currently configures CorsLayer::new().allow_origin(Any), rated Medium
in the Backend Plan and Deep Audit and never revisited since 17 August. Replace it with an explicit,
environment-driven allow-list covering only the origins the dashboard is actually served from in each
real deployment environment (local dev, and whatever the intended staging/production origins are —
ask the product owner if these are not already documented anywhere). Add a test that proves a request
from a disallowed origin is rejected. Do not simply narrow the default without confirming with the
product owner what the real allowed origins should be — a CORS policy that quietly breaks the actual
dashboard is a worse outcome than the current permissive one.
```

**Prompt 1.4 — Make approval/rejection state durable**
```
Run approval and rejection state is currently process-local: a restart of the orchestrator silently
voids any pending approval and later returns 404 for it, per the Backend Audit's L4 finding (deferred
to "Phase 2" on 17 August and never revisited). Investigate exactly where this state currently lives
in engine.rs/store.rs, and change it so a pending approval survives a process restart the same way
runs/tasks/events already do (this project already has a durable SQLite store — the fix is very likely
to persist this state the same way, not to introduce a new storage mechanism). Add a test that starts
a run requiring approval, restarts the process, and confirms the approval is still pending and can
still be approved or rejected afterward.
```

### Wave 2 — Decisions Only a Human Product Owner Can Make (human decision)

**Prompt 2.1 — Run a structured decision session and record binding answers**
```
Several product/contract questions have been raised, in some cases across eight or more separate
documents over four days, and none has ever actually been decided:

1. Status naming — the shipped backend value is INFRA_ERROR; the contract (§13) specifies
   INFRASTRUCTURE_ERROR and forbids alternate spellings. Should the contract be corrected to match the
   shipped, tested value, or should the backend be renamed (with the legacy-client risk that implies)?
2. Realtime event vocabulary — the contract (§14) defines 15 event types; the backend emits 11. Are the
   missing types real future scope that should stay in the contract, or should the contract be trimmed
   to match what actually ships?
3. ACC-012 scope — does the Execution Hub's existing executor-selection UI satisfy the "API Testing"
   requirement area, or does that area need its own dedicated interface?
4. WebSocket authentication — today's implementation authenticates the realtime connection via a query
   parameter, which is a known, disclosed limitation (browsers put query parameters in logs/history).
   Is that acceptable for the real deployment target, or does it need to move to a short-lived ticket
   or another mechanism?
5. Identity model — should authentication remain bearer-API-key-only indefinitely, or does the product
   need a real user/session/login model at some point, and if so on what timeline?
6. Executor sandboxing depth (new, raised by the security remediation unit) — the four dropped security
   findings are now three-of-four fully closed and the fourth (executor sandboxing) has real process-tree
   containment (a Windows Job Object with kill-on-close, process-count and memory ceilings), but
   filesystem/network isolation from inside an executor subprocess is still open, and closing it fully
   needs either Docker Desktop installed or WSL enabled — both a heavier install or an admin-elevation
   step no session should take on its own. Decide whether to do this now, defer it with a stated
   timeline, or formally accept the current process-containment-only level as sufficient for this
   project's real deployment target.

Do not resolve any of these unilaterally. Convene whoever owns the product and the contract, walk
through each question with the evidence cited above, and produce a single new, dated document
(e.g. AIQA_PRODUCT_DECISIONS_RECORD.md) recording the actual decision, who made it, and the date, for
each of the six. Once recorded, file follow-up prompts to bring the contract and/or the code into line
with whatever was decided — this prompt's job is only to get the decisions made and written down.
```

### Wave 3 — Prove It End-to-End

**Prompt 3.1 — Actually run the end-to-end scenarios**
```
AIQA_ACCEPTANCE_MATRIX_v1.0.md defines seven end-to-end scenarios (E2E-001 through E2E-007); none has
ever been executed — all remain BLOCKED. Execute every one of them for real against a running instance
of the orchestrator and frontend, starting with the specific scenario a prior audit already identified
as implementation-ready: kill the real orchestrator process in the middle of an in-flight run, restart
it, and confirm it actually recovers the way the code claims (this is ACC-022, currently verified only
by reading the code, never by actually doing it). For each scenario, submit a real task to a real
executor (Playwright or Pytest, since those are verified working) and watch it through to a real,
observed completion — do not accept a report's description of what should happen as a substitute for
watching it happen. Write up the results as a new, dated report in this project's existing format,
being explicit about which scenarios passed, which failed, and which could not be attempted and why.
```

**Prompt 3.2 — Commission real independent verification of the self-verified work**
```
Every backend unit from BE-12 onward and all 26 frontend FE-1 through FE-26 units were verified only by
the same session that implemented them. Earlier in the project, two genuinely independent verification
passes (AIQA_PHASE1_FINAL_VERIFICATION_REPORT.md and
AIQA_PHASE1_FINAL_INDEPENDENT_VERIFICATION_REPORT.md) each found real defects the original
implementation report had missed — a 100% hit rate across two attempts. Using a fresh session with no
memory of writing the original units, and using the same discrimination-by-reversion technique the
second of those two reports used (temporarily reverting a claimed fix in the live source and confirming
the original symptom actually returns, then restoring it), independently re-verify a representative
sample of the self-verified work — at minimum the realtime event stream (BE-14), the evidence/findings
surface (BE-15), and a handful of the frontend units covering Test Intelligence, the Execution Hub, and
the new AI router and Skills panel. Report exactly what was checked, what technique was used for each
check, and any defect found, in the same evidence-based style as the two Phase 1 verification reports.
```

**Prompt 3.3 — One real, human-observed click-through (human decision to schedule, agent or human to execute)**
```
No one has ever manually clicked through this product end-to-end as a real user would, start to finish,
in a real browser, with a real backend behind it. Mint a fresh API key, start the real orchestrator and
the real forentend dev server, and walk through all 15 pages in order: create a project, launch a real
execution against at least two different executors, watch it complete live over the WebSocket
connection, review its evidence, confirm a finding gets drafted from any failure, review it in Test
Intelligence, and check every honestly-stubbed page actually shows its disclosed "not implemented"
state rather than an error. Take a screenshot of each page. File the screenshots and a short report of
anything that looked wrong, confusing, or inconsistent with what the code/tests claim — this is the one
kind of verification no automated test suite can substitute for.
```

### Wave 4 — Decide, Then Close, the Real Scope Gap

**Prompt 4.1.5 — Fix the ACC-012 page-copy defect (small, already decided, zero remaining ambiguity)**
```
AIQA_PRODUCT_DECISIONS_RECORD.md decision #3 already settled this: the dedicated "API Testing" page
currently renders a NotImplementedState telling the user the capability doesn't exist, but it actually
does — Newman/Pytest/Schemathesis execution via the Execution Hub's executor picker is real, shipped API
testing, just surfaced under a different page. Update the "API Testing" page's copy so it no longer says
"not implemented": state plainly that this capability lives in the Execution Hub, and link or route the
user there. Do not build a new, separate, dedicated API-testing UI (collections, environments, request
history) — that remains a distinct, undecided future question per the same decision record. This is a
small, self-contained frontend copy/routing change; keep it to a single, clearly labeled unit, and update
AIQA_ACCEPTANCE_MATRIX_v1.0.md's ACC-012 row from TESTED to VERIFIED once the copy fix is confirmed live,
citing the exact file changed.
```

**Prompt 4.1 — Resolve the Ollama dispute and re-verify the AI router live**
```
AIQA_AI_MODEL_AUTO_DISCOVERY_IMPLEMENTATION_REPORT.md discloses an unresolved contradiction: this
session found, started, and successfully used a real local Ollama installation and model — and then the
user stated their system has no Ollama installed at all. Because of that, the one test proving live,
differential AI behavior (different skill selections producing genuinely different generated test
content against a real model) was written but deliberately never executed, and the feature's own
verdict is PARTIAL, not GO. Before doing anything else with the AI routing feature: establish, together
with the user, the actual, current, agreed-upon fact of what AI model runtime (if any) is meant to be
available in the real target deployment. Once that is settled, run the existing #[ignore]-marked test
for real against whatever was agreed, and update the feature's verdict based on what actually happens —
do not mark it GO on the strength of the earlier, now-disputed observation.
```

**Prompt 4.2 — Decide the fate of the seven honest stubs and the missing backend domains, then act on the decision**
```
Seven frontend pages (AIQA Orchestrator visualizer, API Testing, Custom Testing, Mobile Testing,
Feature Map/Flow, RBAC Testing's scenario-matrix view, Test Case Validator) and roughly a dozen
contract-specified backend domains (context ingestion, quality-graph queries, requirement traceability,
RBAC scenario-matrix evaluation, generic API-test dispatch/telemetry, mobile/device-farm dispatch,
custom test suites, report generation as a real entity with export, and the full Model Gateway beyond
today's single-task AI router) currently have zero backend implementation and are honestly disclosed as
such. For each one, produce a short, one-page recommendation covering: what it would take to build for
real, what it would take to formally descope it instead, and which of those this project's actual
trajectory suggests. Bring these recommendations to the product owner for a real build-or-descope
decision on each — one question per item, the same pattern already used successfully for the six prior
product decisions, not a single bundled yes/no. Once decided: for anything descoped, reissue the
PRD/FRD/SRD at a new version number whose stated scope actually matches what the product owner just
agreed the product will be — do not leave the governing specification describing a product a third
larger than what's being built with no version marking the difference. For anything to be built, the
product owner has directed building step by step, not all at once: order the "build" items by lowest
effort/risk first and highest-value-with-real-dependents next, propose that order back before starting,
and implement only the first item in its own single, clearly labeled, independently reportable unit —
leave the rest queued for their own subsequent prompts rather than attempting them all in one pass.
```

### Wave 5 — Finish What's Already Half-Open

**Prompt 5.1 — Decide the fate of the remaining fake Settings tabs**
```
SettingsPage.tsx carries a standing instruction, written into its own header comment, to preserve
pre-existing fake content in the General, SSO, Execution, Environments, Policies, and Notifications
tabs — only the Authentication tab and today's new AI Intelligence panel are real. That instruction was
reasonable as a short-term way to avoid regressing unrelated work mid-sprint; it is a worse choice as a
permanent state, because a user configuring a "Policy" or "Environment" that silently does nothing is a
trust problem, not a cosmetic one. Get an explicit decision from the product owner on each remaining
fake tab: build it for real on a stated timeline, or replace its fake controls with the same honest
"not yet available" pattern already used successfully on the seven stubbed pages elsewhere in this
product. Implement whichever is decided, tab by tab, preserving the Authentication and AI Intelligence
panels exactly as they are.
```

**Prompt 5.2 — Make selected Skills actually affect what gets generated**
```
The Skills & MCP Phase 1 panel lets a user genuinely select and deselect skills, and that selection is
captured into the generation session — but, by the feature's own explicit disclosure, selecting a
different skill does not currently change the generated test-case content at all; every category's
output remains fully deterministic regardless of skill selection. Design and implement a real
connection from selectedSkillIds through to the AI-routed generation call in ai_generation.rs, such
that a genuinely different skill selection produces a genuinely different, observably different prompt
or output — and prove this the same way the routing layer already proves its own claims, with a test
that selects two different real skill combinations and asserts the actual generated content differs,
not merely that a parameter was passed through.
```

**Prompt 5.3 — Build a real MCP connector backend**
```
The "Connect MCP" dialog currently and correctly tells the truth: no MCP backend, connector, or
credential store exists anywhere in this project. Design and implement Phase 2 of the Skills & MCP
work: a real MCP client integration that can connect to at least one real MCP server, discover its
tools, and surface them as real, selectable SkillDefinition entries of type MCP (the type system for
this already exists in types/skills.ts and needs no change). Handle credentials, if any are required by
a real MCP server, the same way this project already handles AI-provider credentials elsewhere — never
logged, never serialized into a client-visible response. Do not mark "Connect MCP" as functional until
a real connection to a real MCP server has actually been demonstrated and tested, not merely coded.
```

**Prompt 5.4 — Give the hosted AI providers a real reachability check**
```
Gemini, OpenAI, and Anthropic currently only report configured: true when their respective API key
environment variable is present — reachable is hardcoded false for all three, because no real live
authenticated call is made. Once real credentials are available for at least one of these providers,
add a genuine, bounded, low-cost reachability check (analogous to Ollama's real /api/tags call) for
each, and make sure a failed or expired credential is reported honestly (Unavailable/Degraded) rather
than silently staying in whatever state it last had. Extend the existing credential-non-exposure tests
to cover these new calls, the same way they already cover the Ollama and presence-only paths.
```

---

## 7. Suggested Order of Operations

Do Wave 0 first, in full, before anything else — it is cheap, it is almost entirely non-code, and everything after it depends on actually knowing where things stand. Wave 1 (security) comes next because an unretracted Critical finding is a liability independent of whether it turns out to still be exploitable in your real deployment target — confirm it either way and close the loop. Wave 2 can run in parallel with Wave 1 since it needs a person's time, not engineering time, and several later prompts are blocked on its answers. Do Wave 3 before trusting any claim of release-readiness — right now, no one has proven this product works by actually using it, only by reading code and reports about it. Waves 4 and 5 are where real product scope gets decided and then either built or formally cut; do them last, once the foundation under them is solid and honestly understood.

---

*This document is an assessment and a set of natural-language instructions only. It contains no source code, and no file in the AIQA repository was modified in producing it.*
