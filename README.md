# AIQA — Autonomous & Governed QA Intelligence Platform

> **Production-Grade Test Orchestration, Deterministic Multi-Executor Execution, Governed Self-Healing, and Adaptive QA Intelligence.**

---

## 1. Overview

**AIQA** is an enterprise-grade autonomous and governed Quality Assurance (QA) platform written natively in **Rust** (backend orchestrator) and **TypeScript / React 19** (web control plane). 

Unlike conventional test-generation tools that generate brittle test scripts in the background, AIQA enforces a **governed execution model**:
- **Authoritative Central Orchestration**: All scheduling, execution dispatch, policy enforcement, evidence collection, and state transitions are managed deterministically by the Rust orchestrator.
- **Closed Action AST Invariant**: AI agents and self-healing algorithms operate exclusively on structured, immutable Action Abstract Syntax Trees (`execution_plan::Action`). Arbitrary code execution, source file mutations, and unmonitored shell executions are architecturally impossible.
- **Human-in-the-Loop Governance**: Healing proposals, production runs, and gated executors enforce policy gates requiring human operator review and approval prior to execution.
- **Multi-Project Isolation**: Every query, execution run, telemetry record, and agent interaction is strictly scoped by project identifiers. `auth.rs` carries a single scope field, `project_id: Option<String>`; there is no separate tenant axis anywhere in the schema.
- **Empirical Closed-Loop Intelligence**: Execution telemetry is normalized in real-time, clustered using cryptographic SHA-256 fingerprinting with dynamic token masking, and integrated into a dynamic Quality Graph that recalibrates planning agents and healing confidence.

---

## 2. Key Capabilities

* **Grounded AI Test Generation**: Extracts actual text from product specifications (`.docx`, `.pdf`, `.txt`, `.md`, `.json`, `.yaml`) and OpenAPI schemas, generating grounded, requirement-traceable test cases across 14 built-in generation skills.
* **Deterministic DAG Scheduler**: Asynchronous dependency-graph execution engine implementing Kahn's topological cycle detection, concurrency budgeting, and timeout enforcement.
* **OS-Level Process Sandboxing**: Windows Job Object process-tree containment ensuring child process isolation and 100% clean termination of runaway test runners.
* **Multi-Executor Architecture**: Pluggable registry supporting browser automation (`Playwright`), API collection execution (`Newman`), mobile testing (`Maestro`), Python unit/contract testing (`Pytest`), and OpenAPI property-based fuzzing (`Schemathesis`), alongside high-performance in-process HTTP test execution (`CompiledApiExecutor`).
* **Safe Diagnostic Probing**: In-process diagnostic tools (`schema_inspector`, `dom_prober`, `requirement_lookup`, `security`) allowing multi-turn specialized agents to reason without mutating target applications or executing live suites.
* **Governed Self-Healing**: Deterministic failure diagnosis, AST diff calculation, risk classification, approval gating, and strictly bounded single-attempt verification loops.
* **Adaptive QA Intelligence**: Automatic failure clustering, statistical flakiness tracking with state-flip scoring, healing strategy reliability benchmarking, and directional quality trend analysis.
* **Full Observability & Auditability**: Prometheus exposition endpoint (`/metrics`), live WebSocket event streams (`/events`), structured append-only audit logs, and PDF / Excel compliance report generation.

---

## 3. System Architecture

```mermaid
flowchart TD
    subgraph INGESTION["1. Document & Schema Ingestion"]
        DOC["Product Docs\n(.docx, .txt, .md, .json, .yaml)"] --> EXT["doc_extract.rs\n(Text Extraction Engine)"]
        SCHEMA["OpenAPI / Swagger\nSchema"] --> S_INSP["schema_inspector.rs\n(Safe Schema Parser)"]
    end

    subgraph AI_LAYER["2. Central AI Infrastructure & Router"]
        EXT --> ROUTER["ai_router.rs\n(AiInfrastructure)"]
        S_INSP --> ROUTER
        ROUTER --> PROV{"Provider Selection\n& Fallback Cascade"}
        PROV --> OLLAMA["Ollama\n(Local LLM)"]
        PROV --> GEMINI["Google Gemini\n(Cloud LLM)"]
        PROV --> GROQ["Groq Cloud\n(High-Speed Inference)"]
        PROV --> MJ["MJ AI Gateway\n(Enterprise Proxy)"]
        PROV --> CUSTOM["Custom Providers\n(OpenAI Compatible)"]
    end

    subgraph GENERATION["3. Plan & Test Case Synthesis"]
        ROUTER --> DRAFT["Draft Test Cases\n(Non-Persisted)"]
        DRAFT --> REVIEW["Operator Review\n& Calibration"]
        REVIEW --> STORE[("SQLite Store\n(WAL Mode)")]
        STORE --> PLAN_HIERARCHY["Hierarchy:\nTestPlan -> TestSuite -> TestCase"]
    end

    subgraph GOVERNANCE["4. Governance & Policy Gating"]
        PLAN_HIERARCHY --> VAL["plan_validator.rs\n(Closed Action AST & SSRF Check)"]
        VAL --> AUTH["auth.rs\n(RBAC: Admin / Operator / Viewer)"]
        AUTH --> POL["policy.rs\n(Concurrency & Gated Executors)"]
        POL --> GATE{"Approval Gate\n(Prod / Risk / Maestro)"}
        GATE -- Requires Approval --> APPROVAL["Human Operator Gate\n(POST /approve)"]
        GATE -- Auto-Approved --> SCHED["scheduler.rs\n(DAG Engine)"]
        APPROVAL --> SCHED
    end

    subgraph EXECUTION["5. Execution & Sandboxing"]
        SCHED --> DISPATCH["agent_execution.rs\n(Job Object Containment)"]
        DISPATCH --> EX_REG["ExecutorRegistry"]
        EX_REG --> PLAYWRIGHT["Playwright\n(Web E2E)"]
        EX_REG --> NEWMAN["Newman\n(Postman API)"]
        EX_REG --> MAESTRO["Maestro\n(Mobile E2E)"]
        EX_REG --> PYTEST["Pytest\n(Python QA)"]
        EX_REG --> SCHEMATHESIS["Schemathesis\n(API Fuzzing)"]
        EX_REG --> COMPILED["CompiledApiExecutor\n(In-Process HTTP)"]
    end

    subgraph TELEMETRY["6. Telemetry & Intelligence"]
        PLAYWRIGHT & NEWMAN & MAESTRO & PYTEST & SCHEMATHESIS & COMPILED --> EVIDENCE["evidence.rs\n(Artifacts & Traces)"]
        EVIDENCE --> INGEST["intelligence.rs\n(ingest_run Hook)"]
        INGEST --> ADAPTIVE["adaptive_intelligence.rs\n(Normalization & Fingerprinting)"]
        ADAPTIVE --> CLUSTER[("Failure Clusters &\nFlakiness Metrics")]
        ADAPTIVE --> QGRAPH[("Quality Graph\n(Nodes & Relations)")]
        ADAPTIVE --> PROM["metrics.rs\n(/metrics Prometheus)"]
    end

    subgraph FEEDBACK["7. Governed Healing & Feedback Loop"]
        CLUSTER --> HEAL_DIAG["SelfHealingAgent\n(dom_prober Diagnosis)"]
        HEAL_DIAG --> HEAL_PROP["governed_healing.rs\n(AST Diff Proposal)"]
        HEAL_PROP --> HEAL_GATE{"Human Approval Gate\n(Max 1 Attempt)"}
        HEAL_GATE --> HEAL_VERIFY["Single-Task Verification Run"]
        HEAL_VERIFY --> QGRAPH
        QGRAPH --> PLAN_AGENT["TestPlanningAgent\n(Injected Insights)"]
        PLAN_AGENT -.-> ROUTER
    end
```

---

## 4. AI Architecture & Router

The AI subsystem (`services/orchestrator/src/ai_router.rs`) provides a resilient, provider-agnostic intelligence abstraction across all planning, generation, and hypothesis tasks.

### Multi-Provider Architecture
* **Local & Offline**: Native integration with local **Ollama** instances (`http://127.0.0.1:11434`), enabling air-gapped testing without external network egress.
* **Cloud & Enterprise**: First-class support for **Google Gemini** (`gemini-1.5-pro`, `gemini-1.5-flash`), **Groq Cloud** (`llama-3.3-70b-versatile`), the **MJ AI Gateway**, and arbitrary **OpenAI-compatible** endpoints.
* **Custom AI Providers**: Users can dynamically register custom AI providers with custom base URLs and headers through the Settings UI without restarting the orchestrator.

### Routing Modes & Resilience
* **Auto Mode**: Ranks active providers using capabilities, token limits, and health latency, executing an automatic fallback cascade if the primary provider times out or returns a rate limit (429).
* **Manual Mode**: Enforces explicit operator model and provider pin.
* **Per-Task Overrides**: Directs specific tasks (e.g. `TestPlanning`, `DomAnalysis`, `BugHypothesis`) to optimal model configurations.
* **TTL-Bounded Health Probing**: Periodically probes provider health with background caching (`AI_HEALTH_TTL_SECS`), guaranteeing the application never hangs on unreachable AI services.

### What the AI Calls Came To (`ai_usage.rs`)
Every call the router makes to a provider is counted when it ends: the ones that answer and the ones that do not (in a fallback chain, each attempt). The count is made in the one place every provider call goes through (`AiInfrastructure::execute_for`), so no page, run or fallback escapes it and nothing is counted twice; embeddings do not go through the router and are not counted. Only counts are kept, never a prompt or an answer: `ai_usage` (migration 0023) has a row for each quarter of an hour (UTC), project, provider, model and task, with the calls, the failed calls, the time of those that answered and of those that failed, the tokens the provider reported and the cost of the calls that could be priced (whole millionths of a dollar). Counting is fail-open: a call is never lost because writing its count failed. The count starts when this version starts; earlier calls are not in it, and a price that changes later does not rewrite the cost of calls already made.
- **What is priced**: a call is given a cost only when it answered, went to a provider that has a price (Groq and Gemini at one list rate for the provider, the cheapest model's; a custom provider at the two prices entered in Settings) and reported both its token counts. A model with no price (Ollama, the MJ gateway, a hosted provider whose price was not entered) is counted and never given a cost, not even `0`; a call to a priced provider that reported no token counts is counted and not priced; a call that failed is not priced. The report says how many answered calls fall in each group (`priced_calls`, `no_price_calls`, `no_tokens_calls`), so a total never looks more certain than it is.
- **Reading it**: `GET /api/v1/projects/:project_id/ai-usage?days=&tz_offset_minutes=` (Viewer role; a key sees only its own project) and `GET /api/v1/ai/usage` (Admin role, on a key that is not tied to one project; `?project_id=` for one project, otherwise every project at once, split by project too). `days` is calendar days back from today (30 unless said, at most 366); `tz_offset_minutes` is how far the asker's clock is ahead of UTC (India: 330), so a day is cut at the asker's own midnight. The answer has `totals`, `by_provider` (each provider and model), `by_task`, `by_day` (every day of the range, the quiet ones too) and `notes`; each is the calls, those that answered and failed, the tokens in and out, the time of the answers and of the failures, the average answer, the three counts above and the cost.
- **On screen**: AI Usage & Cost (`AiUsagePage.tsx`, in the navigation): tiles for the calls, tokens, estimated cost (with what it rests on) and AI time; a bar for each day (calls, tokens or cost); tables by provider and model, by task and, for every project at once, by project. *This project* / *All projects*, a week, a month or a quarter, and *Refresh* choose what is read; a row whose cost is `—` had no priced call, and `*` marks a row with some calls left out of its cost.

---

## 5. Test Generation Workflow

```
Specification Doc (.docx / .pdf / .txt / .md / .json / .yaml)
                       ↓
       POST /documents/extract-text (docx paragraph & run parser; pdf-extract for text-based PDFs)
                       ↓
       Diagnostic Schema Probing (schema_inspector)
                       ↓
       AiInfrastructure Grounded Prompt Construction
                       ↓
       POST /test-cases/generate-ai (Draft Generation)
                       ↓
       Operator Calibration & Validation Review
                       ↓
       POST /test-cases (Persistence to SQLite Store)
                       ↓
       Structured Hierarchy: Test Plan -> Test Suite -> Test Case
```

1. **Document Text Extraction (`doc_extract.rs`)**:
   - Parses `.docx` files natively by reading `word/document.xml` within the Zip archive, extracting structured paragraph text while ignoring XML table formatting attributes.
   - Reads text-based `.pdf` files for real, page by page, with `pdf-extract` (`Cargo.toml`, `pdf-extract = "0.7"`). A malformed PDF can make that parser panic, so the read is wrapped in `catch_unwind` and a panic is reported the same way an unreadable file is, rather than taking the request down. A scanned or image-only PDF has no text layer and is refused with its own specific message — *"this PDF has no extractable text content — it may be a scan (an image of a page); scanned PDFs are not read, export it with selectable text or save it as .docx."*
   - Reads `.txt`, `.md`, `.json`, and `.yaml` specifications directly.
   - Legacy `.doc` (pre-2007 binary Word) remains the one genuinely unsupported format, refused with a specific "save it as .docx and re-upload" message rather than guessed at.
   - Enforces safe text extraction ceilings (capped at 20,000 characters per document) with explicit truncation notices.
2. **Deterministic Pre-Validation**: Validates target endpoints against SSRF policies and verifies schema parameters before sending prompts to the LLM.
3. **Draft Separation Invariant**: AI generation produces non-persisted drafts. An operator must review and explicitly commit cases through `POST /api/v1/projects/:project_id/test-cases`.
4. **Hierarchical Organization**: Test cases belong to Test Suites, which roll up into comprehensive Test Plans (`0011_test_plan_suite_hierarchy.sql`).

---

## 6. Execution Architecture & Sandboxing

### Authoritative DAG Scheduler (`scheduler.rs` & `engine.rs`)
The orchestrator executes test runs as Directed Acyclic Graphs (DAGs):
- **Cycle Detection**: Applies Kahn's topological sorting algorithm to validate prerequisite task relationships.
- **Concurrency Budgeting**: Enforces strict concurrency limits (`AIQA_MAX_CONCURRENCY`) to prevent resource exhaustion.
- **Graceful Cancellation & Recovery**: Interrupted runs caused by unclean host reboots are automatically detected and transitioned to terminal states (`INTERRUPTED`) upon orchestrator startup.

### Process Containment & Sandboxing (`executor.rs`)
Every external executor subprocess (`Playwright`, `Newman`, `Maestro`, `Pytest`, `Schemathesis`) runs under isolated process boundaries:
- **Windows Win32 Job Objects**: Each subprocess is assigned to an isolated Windows Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`). If a task times out or is cancelled, Windows guarantees the immediate termination of the entire child process tree, preventing orphaned browsers or zombie processes.
- **SSRF Network Protection**: Target URLs and action endpoints are verified against private network blacklists (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `127.0.0.1`, `169.254.169.254`).
- **Resource Timeouts**: Tasks enforce hard execution ceilings (`AIQA_TASK_TIMEOUT_SECS`), transitioning exceeding tasks to `TIMEOUT`.

### Compiling a Case Against a Real Application (`execution_plan.rs`, `grounding.rs`)
A browser case is compiled from prose, and prose names no selector. Naming a finished crawl (`crawl_run_id`, and the `role` the case runs as) in `POST .../test-cases/:id/compile`, or in a `compile_and_execute` handoff, grounds the compile in it:
- **Real selectors only**: the crawl records every link, button and form field with a locator it checked matches exactly one element; the AI is shown those most to do with the case and told to copy from them. A plan that uses a selector the crawl never found is `NOT_EXECUTABLE`, whatever the AI said. An element that only appears after an interaction is not in a crawl of a page's first state, so a case that needs one stays `NOT_EXECUTABLE` until it is crawled in that state.
- **Login first, secrets at run time**: the role's login steps run before the case. Their credentials are `env:NAME` references (`NAME` starting `AIQA_LOGIN_` or `AIQA_SECRET_`) read from the orchestrator's environment when the test runs: the plan, the generated spec, the stored run and the evidence hold only the names, and a run whose variable is not set is refused before it starts. A role crawled with a literal password cannot be used for this — crawl it again with `env:` references.
- **Task routing**: the compile is the `TestCaseCompilation` task, so Settings → task routing decides who answers it (a local model, Groq, ...).
- **Evidence**: the run's log lists each step (`AIQA_STEP {...}`, never a value) and the page it ended on (`AIQA_FINAL`).

### Checking the Documents Against the Application (`gap_check.rs`)
The requirements read out of the uploaded documents (`requirements.rs`) and a crawl of the running application (`crawler.rs`) can be lined up: `POST /api/v1/projects/:project_id/gap-check` (optional body `{crawl_run_id, role, document_ids}`; with no crawl named, the project's most recent one) answers, without calling any AI provider:
- **Per requirement**: `FOUND` (a page carries the words the requirement is about: in its address, title, headings, a link, a button or a field), `WEAK` (something related is on a page, not enough to say the feature is there), `MISSING` (no page carries them) or `NOT_CHECKABLE` (a crawl cannot show a response time, an API, a lock-out rule or background text). Each comes with the pages and the words behind it, so a person can see why. It is evidence that a feature has somewhere to live, not a proof that it works.
- **Per page**: the pages no requirement points at (`undocumented_pages`, "not in the documents").
- **Per requirement, its cases**: how many saved (not rejected) cases test it, by status and by technique (`technique:<t>` tags), and which techniques that kind of requirement still lacks. The summary counts requirements the application shows that no case tests, and requirements with cases the crawl found no sign of.
- **Two optional steps** narrow down what the words left `WEAK` or `MISSING` (both need the Operator role, both are reported in the response's `method` and neither can turn a requirement into `FOUND` on its own say-so):
  - `use_embeddings: true` compares meaning with the embedder (Gemini, `GEMINI_API_KEY`): a page clearly closer in meaning than the crawl's others is named as a candidate, a `MISSING` requirement with one becomes `WEAK` (resting on `embedding`, unconfirmed). Needs at least 6 pages; at most 150 embedder calls; stops after 3 failures in a row. The similarity thresholds are starting points, not tuned against a real corpus.
  - `verify: true` (with `max_verifications`, default 12, at most 60) asks the router's AI (`Traceability` in Settings → task routing, so a local model, Groq or a paid one) about the unclear requirements, several to a call, each call sized to the provider's window: `CONFIRMED` (the pages it names — only ones it was shown — make the requirement `FOUND`, resting on `ai`), `NOT_FOUND` or `UNCLEAR`, with its reason kept beside the verdict. The page list and the requirement texts are data in the prompt, not instructions; a confirmation that names no page is `UNCLEAR`. Recorded in the audit log as `gap_check.verify`.
- **A crawl that looks for the requirements' pages first**: `POST .../crawl` with `focus_requirements: true` gives the crawl the words the project's requirements are about (stems of the words of the functional, usability, data and security requirements, without the words nearly every requirement shares; at most 200). Links whose text or address mention them are followed before the others and may take up to 10 pages beyond the usual limit of 20 per role, so on a large application the pages the documents talk about are not left to breadth-first order. It orders and extends a crawl; a crawl without the option is exactly what it was. The crawl form (Test Case Generator → Crawl This Application) has it as a checkbox, "Visit the pages the requirements mention first", off by default. The Playwright crawler also records each page's `h1`-`h3` headings now, which the gap check reads.
- **On screen**: Requirement Coverage (`RequirementCoveragePage.tsx`, in the navigation). It lists the project's crawls, runs the words check against the newest one that finished (again when the crawl or the role changes) and shows the counts, each requirement with its status, its best page and how many cases test it (a row opens to the words and the elements behind a match, and to the techniques still without a case), and the pages no requirement mentions. The embedder and the AI check are ticked there and run only from a button, and the page says what each step did. Settings → task routing lists the gap check as `Requirement gap check`.

### Suggesting a Role's Login Steps (`login_suggest.rs`)
A crawl as `anonymous` records the login page's fields, each with a locator it checked matches one element, in the order they are in the page. `POST /api/v1/projects/:project_id/login-suggestion` with `{role, crawl_run_id?, use_ai?}` (with no crawl named, the project's newest one that has a page with a password field) reads them and proposes the steps that log the role in, which the person reads and confirms before a crawl or a run uses them:
- **By rules, no provider**: open the page that has the password field (the `anonymous` role's first, one whose address looks like a login before others), fill the text field nearest before the password field, fill the password field, click the button after it whose words say log in (`Log in`, `Sign in`, `Continue`…; never `Forgot password`, `Sign up` or a social login). Every locator is one the crawl recorded, and the two values are `env:AIQA_LOGIN_<ROLE>_USERNAME` / `_PASSWORD` references named after the role — never a credential; the response names the variables not yet set in the orchestrator's environment.
- **How sure**: `high` when each part's own words say what it is, `medium` when a part was found only by where it sits (a field whose label does not say user name, a button no words explain), `low` when a person must look (a login form beside a sign-up form, no user name field, no button). Each choice comes with its reason.
- **With the AI** (`use_ai: true`, Operator role; only asked when the rules are not `high`): the router's AI (task `GeneralAi`, so a small local model can do it) is shown the page's title, headings and elements as `E1…En` and answers which ids are the user name, the password and the submit. Anything it names must be a listed element of the right kind, so it cannot invent a selector; its answer is at most `medium`; when it differs, both come back (`suggestion` and `alternatives`): the rules' one first when it was `medium`, the AI's first when the rules were `low`. Recorded in the audit log as `login_suggestion.ai`.

### What an Autopilot Run Would Cost (`autopilot_estimate.rs`)
`POST /api/v1/projects/:project_id/autopilot/estimate` (Operator role; reads only, calls no AI) says what a run over the project's requirements would take before anyone starts it. Body, every field optional: `{requirement_document_ids, skills, depth, coverage_top_up, roles, focus_requirements, verify_gaps, execute, run_concurrency}`. The answer is ranges (a minimum and a maximum, never one figure that looks exact):
- **Steps** in the order a run takes them (requirements, crawl, gap check, generate, validate, approve, execute, report), each with its AI calls and seconds; **tasks** (generation, compiling, the gap check's AI step) with the provider and model each would go to right now, its calls, tokens, seconds and dollars.
- **Where the numbers come from**: the calls are the generation plan's (`requirement_generation::plan_calls`) plus a quarter for answers that cannot be read and the top-up rounds; the tokens are the plan's own estimate; the dollars are the two rates of a metered provider (`Unmetered` costs `0`; a task nothing can serve has no cost figure at all, not `0`). The time of a call is **measured** once at least 3 earlier calls to that provider and model are recorded (`ai_call_stats`, migration 0021: every generation call adds its time and tokens, whichever page or run made it), and **assumed** for the kind of provider before that (a local model 20 tokens a second, a hosted one 100), and the estimate says which.
- **Assumptions and warnings** are listed: no provider for a task, a run that may cost money (`totals.paid`), no depth chosen, requirements no selected skill tests. The wait for a person to approve the cases is not counted.

### Autopilot Runs (`autopilot.rs`, `autopilot_runner.rs`, `autopilot_steps.rs`)
One run takes a project from its uploaded requirements towards a report of executed tests. It is made with the estimate above, starts only when a person accepts that, and every step calls a route the application already has, as the key that made the run — so nothing happens in a run that this key could not do by hand.
- **Making a run**: `POST /api/v1/projects/:project_id/autopilot/runs` (Operator role) with `{base_url, roles: [{name, login_actions}], crawl_run_id?, requirement_document_ids?, skills?, depth?, coverage_top_up?, focus_requirements?, verify_gaps?, execute?, run_concurrency?, approval?, name?}`. The roles carry the login steps a person confirmed (`login-suggestion` proposes them; their values are `env:NAME` references, never credentials). It stores the run with its estimate and starts nothing, and refuses what could only fail later: an address or login steps a crawl would refuse, a variable that is not set (it is named), no requirements to test, a `crawl_run_id` that is not a finished crawl of the project. A project has one run that has not ended at a time (the `409` names the one in the way).
- **Accepting the cost**: `POST .../autopilot/runs/:run_id/start` with `{"accept_estimate": true}`, and `"accept_paid": true` as well when the run's estimate says a provider that charges is among those it would call. Without either, nothing starts. Making, starting and cancelling a run are audited (`autopilot.create`, `autopilot.start`, `autopilot.cancel`).
- **Steps** (in order; each is a row with its state, its attempts and what it found): `REQUIREMENTS` (the documents have requirements: counts by kind), `CRAWL` (starts a crawl through `POST .../crawl` as the roles given, or uses `crawl_run_id`; the crawl's run id is saved the moment it is known, and the step watches that run until it ends, is cancelled or takes longer than 30 minutes), `GAP_CHECK` (`POST .../gap-check` against that crawl, with the AI's look at unclear requirements when `verify_gaps` is set), `GENERATE`, `VALIDATE`, `APPROVE`, `EXECUTE` and `REPORT` (below). A step that fails ends the run `FAILED` with the step's name and the service's own reason.
- **Generating**: the run makes a suite named after itself and asks the AI for cases ten requirements at a time (`POST .../test-cases/generate-ai` with the crawl as grounding, the run's skills, depth and top-up, and `requirement_keys` naming the group: the same generation the Test Case Generator page makes, so the router picks the provider, falls back when the first choice fails and records what each call took). Each case is saved with `POST .../test-cases` as the page would save it (tagged `ai-generated`, `technique:<t>`, `autopilot:<run id>` and a marker naming its place, with its requirement's text and document) and the step keeps its progress after every group: after a restart, or after a person starts a failed run again, it goes on with the group it was in, without asking the AI again for drafts it already has and without saving a case twice. A group the AI could not write fails the step with the router's reason; a wave of it that failed, and a group the router served with another provider than its first choice, are lines in the timeline.
- **Validating**: each case is checked by the structural check the Validator page uses (`POST .../test-cases/:id/validate`). A case that fails stays as it is, with its reasons, for a person to fix.
- **The gate**: the cases wait for a person, and so does the run (`WAITING`), until they approve or reject them (in the run's page, or in Test Cases) and say to go on (`POST .../autopilot/runs/:run_id/continue`); cases left undecided are not run. `POST .../autopilot/runs/:run_id/approve-cases` (optionally `{case_ids}`, otherwise every validated case) approves the run's cases as the person asking: each is an ordinary approval, audited under their name. A run made with `"approval": "auto_validated"` approves the cases that passed validation itself and does not wait. A run waiting at the gate is not touched by a restart.
- **Running the cases**: each approved case is handed off with the route the Test Cases page uses (`POST .../test-cases/:id/handoff` with `compile_and_execute`, grounded in the run's crawl and as `execution_role`: the run's own choice, else the crawl's role that never logged in, else the first role), so the compiler, the executors, the policy and the audit are the ones every run has. At most `run_concurrency` (default 2) at once. The outcome of each case is a task status (`PASSED`, `FAILED`, `TIMEOUT`, `INFRA_ERROR`, `NEEDS_REVIEW`, ...), or `NOT_EXECUTABLE` / `COMPILATION_FAILED` / `COMPILATION_UNAVAILABLE` / `NOT_RUN` when the route refused the case (with its reason), or `UNFINISHED` when its run did not end within 20 minutes (a run held for approval by the project's policy is waited for). A case that ended in `INFRA_ERROR` or `TIMEOUT` is run again once, through `POST .../executions/:run_id/retry-failed`; a case that genuinely `FAILED` never is. The step keeps each run id the moment the run exists and each outcome the moment it is known, so after a restart it watches the runs that were going, runs only the cases not yet run and never runs one twice. `"execute": false` skips the step. Cancelling the run cancels the runs being watched.
- **The report**: the last step writes the run's report and keeps it with the step; `GET .../autopilot/runs/:run_id/report` (Viewer; `?format=markdown` gives text to paste) returns it, or, for a run that is going or ended before it got there, works it out from what the run has recorded and says `"final": false`. It has the requirements one by one with a verdict from how their cases came out (`PASSED`, `FAILED`, `INCONCLUSIVE`, `NOT_EXECUTED`, `NO_CASES`), the failures (the case, its requirement, the reason and the run to open), the cases that gave no answer and why, what was crawled, generated (cases, providers, whether the router fell back, tokens and time), validated, approved and run, the estimate beside what was used, the time each step took, and notes on what to know before trusting the numbers. A case a person rejected at the gate is not counted as a case of its requirement (a requirement whose only case was rejected is `NO_CASES`, and a note says how many were rejected); `execution.ran` is the number of cases really run (a case the compiler refused reached an outcome, `finished`, but nothing was run for it). A failed case's reason is shown without the terminal's colour codes and, of Playwright's code frame, only the line that failed (the run itself keeps the raw text). Nothing in it is stored twice: it is read from the steps' details, the requirements and the cases.
- **Starting again**: `POST .../autopilot/runs/:run_id/resume` takes a `FAILED` run over and starts it at the step that failed, keeping what was done before it (a crawl already made, groups of cases already saved). The run acts as the person who resumed it from then on. It is refused while the project has another run that has not ended.
- **Watching**: `GET .../autopilot/runs` (the project's runs, newest first), `GET .../autopilot/runs/:run_id` (the run, its steps, the latest of its timeline, and `working`: whether this process is working on it now) and `GET .../autopilot/runs/:run_id/events?after=<id>&limit=` (the timeline, oldest first, from where a page was).
- **Cancelling**: `POST .../autopilot/runs/:run_id/cancel`. A run nobody is working on ends at once; one that is being worked on is asked to stop and does at the next point it can (between steps, and every two seconds while it watches a crawl, which it then cancels too).
- **Restarts and keys**: the database is the run's memory. Each step is marked running before it starts and saves its checkpoints, so when the service starts again the runs that were `RUNNING` are picked up at the step they were in (a crawl already started is waited for, not started again); runs waiting for their estimate or at a gate stay waiting for a person. The key that made a run is looked up again before every call the run makes, so revoking it stops its runs at their next call. A step that panics ends its run `FAILED`.
- **On screen**: Autopilot (`AutopilotPage.tsx`, in the navigation). *New run* is a form: the application's address, which requirement documents, the roles with their login steps (written by hand, or proposed by *Suggest login steps* with how sure it is, why, and which variables are not set yet; a proposal is applied only when it is taken, and with no crawl yet the page offers to look at the login page first), what to generate (skills, depth, filling gaps, crawling the requirements' pages first, the AI check of unclear requirements), whether to run the approved cases (how many at a time, as which role) and who approves them; beside it, the estimate, worked out again a moment after each change. Making the run stores it and starts nothing. The run's page (`?run=<id>`) shows the estimate with the two things to accept (that it was read, and its cost when a provider that charges is used) before *Start the run*, then the steps with what each found, the timeline, *Cancel*, the gate (read each case, approve or reject it, *Approve all validated*, *Continue*), *Start it again where it stopped* for a failed run, and the report (requirements by verdict, failures, cases that gave no answer, cost against estimate, *Copy as Markdown*). It looks at a running run every two seconds and at one waiting for a person every ten, and coming back to the page lands on the run that has not ended. Settings → task routing lists the login suggestion as `Login suggestion`.

### API Tests from an OpenAPI Document (`openapi_spec.rs`, `openapi_cases.rs`, `openapi_runs.rs`, `openapi_api.rs`, `json_schema.rs`)
Contract tests for an API made from its own OpenAPI 3.x or Swagger 2.0 document, by rule, with no AI and no cost: what the document promises against what the API does. Nothing is guessed: a case is made only when the document says what the answer must be, and the plan says why an operation got none.
- **Reading the document**: the text (`spec`, JSON or YAML, at most 5 MB) or an address (`spec_url`, fetched through the same guarded client the other tools use, 20 seconds). YAML is read by a real YAML reader (`serde_yaml`, used only here): sequences at the indentation of their key, block text, flow style, anchors and merge keys, and unquoted numbers as response codes (`200:`) or as a version are all read, while a document that expands into a flood of aliases or nests without end is refused. `$ref`s are resolved (nesting at most 12 deep, at most 300 operations); an operation is named `METHOD /path` and keeps its parameters, its JSON request body, the responses it documents and whether it needs a credential. Where the API is comes from `base_url`, else the document's first absolute server, else (for a document given by its address) its first relative server, or `/` when it names none, worked out against that address as OpenAPI defines it (`/api/v3` in a document served from `https://host/api/v3/openapi.json` is `https://host/api/v3`); a server that still has a `{variable}` with no default is not an address, and a pasted document has none to work a relative one out from, so both are asked for `base_url`.
- **The cases** (`kinds`, all five unless narrowed; `operations` narrows to some, `max_cases_per_operation` is 8 unless said, at most 20; at most 500 in all): `HAPPY` (the smallest valid request, from an `example` (beside the schema, on a parameter or a request body's media type, or in it), `default` or `enum`, else from the schema's type, that must get one of the success statuses the document lists and, when it gives a schema, a body that fits it), `NO_AUTH` (the same without the credential, for an operation that needs one: 401 or 403), `MISSING_REQUIRED` (one required query or header parameter, or one required property of the body, left out), `WRONG_TYPE` (text where a number is due) and `NOT_FOUND` (an id that is not there, for an operation that documents a 404). The last three expect the client errors the operation itself documents; one that documents none gets no such case, and the notes say so once for all such operations. Where a path parameter has no example, default or enum, the id in a request is made up, and one note says that a 404 there may only mean the API has no such thing.
- **Requests that change data** (POST, PUT, PATCH, DELETE) are derived and shown, and run only with `include_unsafe: true`; otherwise the report lists them as *not run* with the reason, so pointing this at a real API creates and deletes nothing by default.
- **Credentials** are only references: `auth_headers: {"Authorization": "env:AIQA_SECRET_API_TOKEN"}`, the name of a variable of the orchestrator's environment that starts `AIQA_SECRET_` or `AIQA_LOGIN_`. The value is read when the request is made and is never stored, shown or returned, and is scrubbed from the evidence. A value typed in place of a reference is refused (the message does not repeat it), and so is a header name HTTP does not allow (letters, digits and ``!#$%&'*+-.^_`|~``), before anything is made; a variable that is not set is named (`missing_secrets` in the plan, a `400` when running).
- **Showing the cases**: `POST /api/v1/projects/:project_id/openapi/plan` (Operator role; runs nothing) takes `{spec | spec_url, base_url?, kinds?, operations?, include_unsafe?, auth_headers?, max_cases_per_operation?}` and answers `{spec, base_url, operations, cases, skipped, notes, counts, missing_secrets}`: each case has its request and what it expects (the statuses, the schema and why, in the document's terms).
- **Running them**: `POST .../openapi/runs` (Operator role; the same body, plus `concurrency` 1 to 8, default 4, and `name`) makes one ordinary execution with a task (`api-0001`, ...) for each case that may be run, through the route every run uses, so a target the orchestrator may not reach is refused (`TARGET_NOT_ALLOWED`) and the run has the policy, the evidence and the audit of any other (`openapi.run`). Two actions were added to the closed plan AST for it: `ASSERT_STATUS_IN {statuses}` and `ASSERT_BODY_SCHEMA {schema}`; the schema check (`json_schema.rs`) understands `type`, `nullable`, `enum`, `const`, `required`, `properties`, `additionalProperties`, `items`, minimum and maximum, `pattern`, `allOf`, `anyOf` and `oneOf`. It answers `201` at once with `{id, run_id, cases, cases_run, not_run, state, notes}`; what each case was for is kept (`openapi_runs`, migration 0024) so the report can be made while the tasks are still finishing.
- **The report**: `GET .../openapi/runs/:id` (Viewer role; `?format=markdown` gives text to paste, `?format=junit` JUnit XML) has, operation by operation in the document's order, the cases with what each expected and what came back (status, time and the reason it failed, in the assertions' own words: a body that does not fit the success schema is left out of the reason when the status was already wrong, and a body that is not JSON says what content type it claimed; the evidence of the first 200 failures is read, a case that passed says only that), how many operations of the document were tested and which were not and why, the pass rate, notes on what to know before trusting it, and `final: false` while the run is going. `GET .../openapi/runs` lists the project's 50 latest runs, newest first, with each run's state.
- **On screen**: OpenAPI Tests (`OpenApiTestsPage.tsx`, in the navigation). A form takes the document (pasted or by address), where the API is, the kinds of case, the credentials as variable names, and the box that lets requests that change data run (with a warning that they are really sent). *Show the cases* lists them before anything is sent, operation by operation, with the request and the answer each expects, and the operations to leave out; *Run N cases* starts the run and the page goes to its report (`?run=<id>`), which it looks at every two seconds until the run has ended. The runs of the project are listed beside the form; the report can be copied as Markdown.

### Scheduled Regression (`regression.rs`, `regression_runner.rs`, `regression_api.rs`)
A schedule runs again, every so often, the cases a project has approved and run once, and says what changed since the last time. Nothing is compiled again and no AI is asked: a case that was handed off remembers the run it was last executed in (`executed_run_id`), that run's plan holds the task the case ran as with its compiled steps, and a regression run is a new execution made of those same tasks (through `create_run_core`, so the plan checks, the policy, the evidence, the audit and the Execution Hub are those of any run). Repeating a case leaves the case as it was.
- **Scope**: a suite of the project, or every case. `EXECUTED` cases are repeated; an `APPROVED` case that was never run, one whose last run was simulated, and one whose run or task is gone are listed as *not repeated*, each with the reason (`GET /api/v1/projects/:project_id/regression/scope?suite_id=` shows it before anything is made). Cases in other states are not in scope.
- **Schedules** (`POST` and `GET .../regression/schedules`, `PATCH` and `DELETE .../regression/schedules/:id`; Operator to change, Viewer to read): `name`, `every_minutes` (5 to 44640), an optional `start_at` (the first time it is due and the grid the later ones fall on: a nightly schedule keeps its hour however long a run takes or the service was down, and after downtime it makes one run, not a queue), `concurrency` (1 to 8, 2 unless said), `suite_id` (`null` for every case) and `enabled`; at most 20 to a project. A schedule made with `enabled: false` never runs by itself and is run when asked, which is how a CI job uses it.
- **The loop**: every 30 seconds the enabled schedules whose time has come each get a run, as the key that made the schedule (looked up again every time, so revoking the key pauses the schedule and `last_note` says why). A schedule whose previous run has not ended is skipped this time and moved on, so runs do not pile up; one with nothing to repeat says so in `last_note`.
- **Run now**: `POST .../regression/schedules/:id/run` (Operator) makes a run as the person asking (`409` while the previous one has not ended) and leaves the schedule's slots as they are.
- **Reports**: `GET .../regression/schedules/:id/runs` (the latest 50, newest first, each with its verdict) and `GET .../regression/runs/:id` (Viewer; `?format=markdown` gives text to paste, `?format=junit` JUnit XML). For each case, what it came to now beside what it came to the last time: in the previous run of the same schedule when that repeated the case and got an answer, else in the run its task was copied from. A case is `REGRESSED` (passed, now fails or errors), `FIXED`, `STILL_FAILING`, `STILL_PASSING` or `NEW`, regressions come first, and the run's verdict is `REGRESSED`, `FAILING`, `PASSING`, `RUNNING` or `NOTHING_RAN`. The results are the execution's own tasks (tables `regression_schedules` and `regression_runs`, migration 0025, only remember which cases a run repeated and which it could not).
- **On screen**: Scheduled Regression (`RegressionPage.tsx`, in the navigation). *New schedule* asks for a name, what it repeats (every case run once, or one suite; a line says how many cases would be repeated and why some cannot), how often (every 15 minutes, hour, 6 hours, day, week, or minutes typed in), an optional first run (the viewer's clock) and how many cases at a time, and whether it starts by itself. A schedule shows how often it runs, when next, its runs (each with its verdict) and the note it left; *Run now*, *Pause* / *Resume* and *Remove* (asks first) are beside it. A run's report (`?run=`) shows regressions first, each case with what it came to now beside what it came to the last time, what could not be repeated, and *Copy as Markdown*; it is looked at every two seconds until the run has ended.

### CI Integration (`ci.rs`, `junit.rs`, `scripts/ci/`)
A CI job can start a run, wait for it, put every case on the build's test page and fail the build on the verdict. Nothing new runs on the server: the two reports above (a scheduled regression's and an OpenAPI test run's) can be read as JUnit, waited for, and summed up in headers.
- **`?format=junit`** on `GET .../regression/runs/:id` and `GET .../openapi/runs/:id` (Viewer role) gives JUnit XML (`application/xml`): one `<testsuite>` with a `<testcase>` for each case (`regression.<schedule>` or the operation, `GET /pets`, as its class); `<failure>` for a case that failed, `<error>` for one that could not be run to the end, `<skipped>` (with the reason) for one that was not run or, in a regression, not repeated. A regression case's output says what became of it since the last time. It is made from the report as it is asked for (`junit.rs`); nothing is stored.
- **`?wait=`** (seconds, at most 300; `400` beyond that) holds the request until the run has ended or the time is up, looking again every two seconds (`ci.rs`), so a job asks once instead of polling. The answer's `X-AIQA-Final` (`true` once the run has ended) and `X-AIQA-Verdict` headers say how it came out without reading the body; they are on every format.
- **`verdict`** was added to the OpenAPI report, as a regression has it: `RUNNING`, `FAILING` (a case failed or could not be run to the end), `PASSING` or `NOTHING_RAN`.
- **The scripts**: `scripts/ci/aiqa-ci.sh` (bash, curl, sed, grep, awk) and `scripts/ci/aiqa-ci.ps1` (PowerShell) start a regression (`regression <schedule-id>`) or an OpenAPI run (`openapi <spec-url-or-file> [base-url]`), wait, save the JUnit XML, print the report in words (and add it to GitHub's job summary), and exit `0` (acceptable), `1` (the build should fail on the result: `AIQA_FAIL_ON` is `failing`, the default, `regressed` or `never`) or `2` (no result). The key is in `AIQA_KEY`, never an argument. Setup, every variable and examples for GitHub Actions, GitLab, Jenkins and Azure DevOps are in [docs/ci/README.md](docs/ci/README.md).
- **On screen**: the Scheduled Regression page has *Download JUnit* beside *Copy as Markdown* on a report, and *Use it from CI* on a schedule: its id, and the lines to copy into a job.
- **Local checks**: `scripts/ci/local-checks.ps1` runs on the machine what `.github/workflows/ci.yml` runs (the secret scan with Gitleaks, the frontend's lint, tests and build, the backend's clippy and tests), without GitHub, and exits `0` (passed), `1` (a step failed) or `2` (a step asked for alone could not run). See [docs/ci/README.md](docs/ci/README.md).

---

## 7. Specialized Diagnostic Tools & Agents

AIQA separates reasoning tools from test executors:

| Diagnostic Tool (`tools/`) | Purpose | Safety Level | Target App Impact |
| :--- | :--- | :--- | :--- |
| `schema_inspector` | Parses OpenAPI 3.x and Swagger definitions | `ReadOnly` | Zero network impact; schema metadata inspection |
| `dom_prober` | Queries and verifies selector candidates against recorded DOM snapshots | `Diagnostic` | Offline analysis; no browser launching |
| `requirement_lookup` | Traces test cases to requirements and coverage gaps in the Quality Graph | `ReadOnly` | Local database query |
| `security` | Evaluates selector safety, input patterns, and SSRF address boundaries | `ReadOnly` | Deterministic sanitization |

### Native Specialized Agents (`agents/`)
- **`TestPlanningAgent`**: Synthesizes prioritized test suites (P0 Authentication, P1 Contract, P2 Validation, P3 Resiliency) and identifies orphan requirements lacking test coverage.
- **`SelfHealingAgent`**: Multi-turn failure diagnosis that probes DOM snapshots, categorizes root causes (e.g. `SelectorRenamedOrRelocated`, `RoleGatedVisibilityDifferential`, `AmbiguousSelector`), and generates candidate selector fixes.

---

## 8. Governed Self-Healing Architecture

The AIQA self-healing subsystem (`services/orchestrator/src/governed_healing.rs`) executes under a strict governance framework:

```
[Test Failure Recorded]
           ↓
[SelfHealingAgent Probes Offline DOM]
           ↓
[Generate Proposal: POST /healing/proposals]
           ↓
[Compute AST Diff & Risk Level (Low/Medium/High/Critical)]
           ↓
[Enforce Invariants: Single Attempt Limit (Max 1), Target & Env Immutability]
           ↓
[Policy Gate: Human Operator Approval Required for High Risk / Production]
           ↓
[POST /approve -> Dispatched to Authoritative Engine]
           ↓
[Single-Task Verification Run]
           ↓
[Outcome Recorded: VERIFIED | NOT_VERIFIED | REGRESSION_INTRODUCED]
           ↓
[Quality Graph Strategy Win-Rate & Test Planning Agent Feedback Calibrated]
```

### Core Invariants of Self-Healing
1. **Closed Action AST**: Modifications are restricted strictly to browser and API actions (`Navigate`, `Click`, `Fill`, `AssertVisible`, `Request`). Arbitrary code, shell scripting, or source file modifications are structurally rejected.
2. **Immutable Security Boundaries**: Proposals cannot alter target URLs, test environments, executor types, credentials, or project scope boundaries.
3. **Hard Bounded Single Attempt**: Exactly **one** healing attempt is permitted per failed task. Recursive healing cascades and runaway loops are rejected by database unique constraints.
4. **Human Operator Sign-Off**: Proposals involving high risk, confidence below `0.80`, gated executors (`maestro`), or production environments remain in `PENDING_APPROVAL` until approved via `POST .../approve`.
5. **Quality Feedback Integration**: Verification outcomes update strategy confidence via Bayesian smoothing and inject fragile selector warnings directly into the `TestPlanningAgent`.

---

## 9. Adaptive QA Intelligence & Quality Graph

Phase 6 and Phase 7 introduce continuous empirical quality intelligence (`adaptive_intelligence.rs`):

### Execution Telemetry Normalization
Every run ingested via `intelligence::ingest_run` normalizes execution metrics:
- Task durations, retry counts, error classifications, assertion results, and HTTP status codes.
- Deterministic failure classification into standard taxonomy (`AuthenticationFailure`, `AuthorizationFailure`, `NetworkFailure`, `Timeout`, `SelectorNotFound`, `SelectorAmbiguous`, `AssertionFailure`, `ValidationFailure`, `ServerError`, `ClientError`, `RateLimit`, `DependencyFailure`, `EnvironmentFailure`, `DataFailure`, `Unknown`).

### Cryptographic Failure Clustering
Dynamic tokens (UUIDs, timestamps, IP addresses, memory addresses, numeric IDs) are stripped via compiled regular expressions before computing a 64-character SHA-256 fingerprint:
$$\text{Fingerprint} = \text{SHA256}(\text{Category} \parallel ":" \parallel \text{MaskedSignature} \parallel [":selector:" \parallel \text{Selector}] \parallel [":endpoint:" \parallel \text{Endpoint}])$$
Recurring failures cluster together regardless of changing timestamps or session identifiers.

### Statistical Flakiness Intelligence
Calculates outcome instability across execution windows (minimum 2 runs):
$$\text{FlakinessScore} = \frac{\text{Flips}}{\text{TotalRuns} - 1}$$
- **Stable**: $\text{Score} < 0.15$
- **Potentially Flaky**: $0.15 \le \text{Score} < 0.40$
- **Highly Flaky**: $\text{Score} \ge 0.40$

### Quality Graph Topology
Maintains relational nodes (`test`, `task`, `action`, `component`, `failure_cluster`, `strategy`) and directed edges (`fails_with`, `healed_by`, `belongs_to`), visualizable through the interactive Quality Graph explorer.

---

## 10. Observability & Metrics

### Prometheus Exposition (`GET /metrics`)
Exposes live telemetry, flakiness, and healing reliability metrics in Prometheus text format:
```prometheus
# HELP aiqa_execution_telemetry_total Total number of execution telemetry records ingested
# TYPE aiqa_execution_telemetry_total counter
aiqa_execution_telemetry_total{status="passed"} 1240
aiqa_execution_telemetry_total{status="failed"} 32

# HELP aiqa_flaky_tests_total Total number of tests currently classified as flaky
# TYPE aiqa_flaky_tests_total gauge
aiqa_flaky_tests_total 3

# HELP aiqa_healing_strategy_attempts_total Total healing attempts recorded per strategy
# TYPE aiqa_healing_strategy_attempts_total counter
aiqa_healing_strategy_attempts_total{strategy="SelectorSynonymReplacement"} 14

# HELP aiqa_healing_strategy_successes_total Total verified healing successes per strategy
# TYPE aiqa_healing_strategy_successes_total counter
aiqa_healing_strategy_successes_total{strategy="SelectorSynonymReplacement"} 12
```

### Intelligence REST Endpoints
* `GET /api/v1/projects/:project_id/intelligence/telemetry`: Real-time execution telemetry records.
* `GET /api/v1/projects/:project_id/intelligence/failures`: Clustered failure signatures, hypotheses, and review states.
* `GET /api/v1/projects/:project_id/intelligence/flakiness`: Flakiness scores, run sequences, and environment correlations.
* `GET /api/v1/projects/:project_id/intelligence/strategies`: Empirical healing strategy reliability scores.
* `GET /api/v1/projects/:project_id/intelligence/trends`: Trajectory metrics (`IMPROVING`, `STABLE`, `DEGRADING`).
* `GET /api/v1/projects/:project_id/intelligence/quality-graph`: Graph vertices and directed relational edges.

---

## 11. Security Architecture

1. **Role-Based Access Control (RBAC)**:
   - **`Admin`**: Full platform authority, API key provisioning, policy configuration, audit log inspection.
   - **`Operator`**: Project creation, test authoring, test execution, healing proposal approval/rejection.
   - **`Viewer`**: Strictly read-only access to dashboards, test cases, execution telemetry, and reports.
2. **Project Scoping**: All data queries, task definitions, and state transitions enforce mandatory `project_id` scoping. `auth.rs` carries exactly one scope field, `project_id: Option<String>` (`NULL` = an unscoped `Admin` key that can reach every project), and no `tenant` column exists in any of the 26 SQLite migrations. Cross-project data leaks are structurally prevented at the database layer.
3. **SSRF Boundary Enforcement**: Target URLs and outgoing webhook destinations reject RFC1918 private subnets (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`), localhost (`127.0.0.1`), link-local, and cloud metadata endpoints (`169.254.169.254`).
4. **Credential & Secret Redaction**: `adaptive_intelligence.rs::sanitize_telemetry_text` masks three things in telemetry text with the literal `[REDACTED]`: a `Bearer <token>` value; a key/value pair whose key is `authorization`, `cookie`, `set-cookie`, `api[_-]?key`, `password`, `secret`, `token`, `access_token` or `client_secret`; and a cookie pair named `session`, `sessionid`, `auth_token`, `token` or `jwt`. This is telemetry-log sanitisation, not a universal pre-persistence or pre-rendering gate — it is not applied to every table, and there is no `aiqa_*` prefix pattern in it (a stored key's hash is never logged, but the mask function does not itself look for the `aiqa_` key format).
5. **Append-Only Audit Logging**: Administrative and operator actions (approvals, rejections, API key creation, run dispatches) are recorded in the `audit_log` table (migration `0003`). It is append-only by convention — the only write path is a plain `INSERT` — and carries **no tamper-evidence**: there is no hash column, no `prev_hash`/`entry_hash` chain and no signature, so a row edited or deleted in place is indistinguishable from an original one. Do not treat this store as tamper-evident.

---

## 12. Privacy & AI Safety Model

* **Zero Chain-of-Thought Persistence**: Raw internal model reasoning and chain-of-thought tokens are never written to disk, stored in the database, or returned via REST endpoints.
* **Operational Rationale Only**: AI-generated responses retain only concise, factual, and structured operational rationales.
* **No Unmonitored Code Mutation**: AI agents are advisory reasoning units. Agents cannot rewrite local source code, alter test scripts on disk, or run unmonitored commands.
* **Bounded Authorizations**: All agent decisions translate into structured proposals requiring policy verification and human approval.

---

## 13. Project Structure

```
AIQA/
├── .agents/                        # Agent instructions and specialized skills
├── .env                            # Local development configuration (gitignored)
├── start.bat                       # One-click Windows startup script
├── stop.bat                        # One-click Windows shutdown script
├── docs/                           # Architecture reports, developer guides & sprint plans
│   ├── AIQA_AGENT_FOUNDATION_DEVELOPER_GUIDE.md
│   ├── AIQA_DIAGNOSTIC_TOOLS_DEVELOPER_GUIDE.md
│   ├── AIQA_PHASE_3_SPECIALIZED_AGENTS_REPORT.md
│   ├── AIQA_PHASE_4_GOVERNED_EXECUTION_INTEGRATION_REPORT.md
│   ├── AIQA_PHASE_5_GOVERNED_SELF_HEALING_REPORT.md
│   ├── AIQA_PHASE_6_ADAPTIVE_QA_INTELLIGENCE_REPORT.md
│   └── AIQA_PHASE_7_INTELLIGENCE_DASHBOARD_REPORT.md
├── executors/                      # Concrete test runner project fixtures
│   ├── maestro/                    # Mobile E2E runner (Android / iOS)
│   ├── newman/                     # Postman API collection runner
│   ├── playwright/                 # Browser E2E runner (Chromium / Firefox / WebKit)
│   ├── pytest/                     # Python unit & integration runner
│   └── schemathesis/               # Property-based OpenAPI contract fuzzer
├── forentend/                      # React 19 / TypeScript Web Control Plane
│   ├── package.json
│   ├── vite.config.ts
│   └── src/
│       ├── components/             # Reusable UI components
│       │   ├── intelligence/       # Adaptive QA Intelligence & Observability views
│       │   ├── layout/             # Navigation, TopBar, and Sidebar
│       │   └── pages/              # 22 full-featured application page components
│       ├── context/                # ProjectContext, AuthContext
│       ├── routes/                 # App routing configuration
│       ├── services/               # Typed API client layer (aiqaApi.ts)
│       └── types/                  # Authoritative TypeScript contract types
├── infra/                          # Infrastructure & persistence data
│   ├── data/                       # SQLite database & local evidence storage
│   ├── docker-compose.yml          # Optional containerized persistence services
│   └── migrations/                 # Postgres target schema — a forward design, NOT applied by the running service (see store.rs)
└── services/
    └── orchestrator/               # Core Rust Orchestrator Service
        ├── Cargo.toml
        ├── migrations/             # 26 SQLite database migrations (the only set sqlx applies)
        └── src/
            ├── main.rs             # Service bootstrap, configuration & runner registration
            ├── lib.rs              # Library exports & test module bindings
            ├── adaptive_intelligence.rs # Normalization, clustering, flakiness & trends
            ├── agent_execution.rs  # Governed agent execution dispatcher & approval lifecycle
            ├── agents/             # TestPlanningAgent & SelfHealingAgent implementations
            ├── ai_router.rs        # Central AI Infrastructure, provider discovery & routing
            ├── ai_usage.rs         # Counting what the AI calls came to (calls, tokens, cost) & the usage report
            ├── api.rs              # Axum REST & WebSocket route definitions
            ├── app_source.rs       # SSRF-guarded application connectivity verification
            ├── auth.rs             # API key authentication & RBAC middleware
            ├── ci.rs               # For a CI job: wait for a run to end (`?wait=`) & say how it came out in headers
            ├── doc_extract.rs      # Native document text extractor (.docx, .pdf, .txt, .md)
            ├── domain.rs           # Core domain entities, RunPlan & TaskDef
            ├── engine.rs           # Run lifecycle manager & unclean run recovery
            ├── evidence.rs         # Artifact persistence & evidence store
            ├── execution_plan.rs   # Closed Action AST & plan compiler
            ├── executor.rs         # Win32 Job Object sandboxed subprocess runners
            ├── governed_healing.rs # Closed AST diffing, risk analysis & healing verification
            ├── intelligence.rs     # Run outcome ingestion hook
            ├── json_schema.rs      # Small JSON Schema validator for response bodies
            ├── junit.rs            # JUnit XML of a regression or an OpenAPI test report
            ├── metrics.rs          # Prometheus exposition metrics collector
            ├── openapi_api.rs      # Routes: show, run and report API tests made from an OpenAPI document
            ├── openapi_cases.rs    # Contract-test cases worked out by rule from the document (no AI)
            ├── openapi_runs.rs     # Stored OpenAPI runs & their per-operation report
            ├── openapi_spec.rs     # OpenAPI 3.x / Swagger 2.0 reader ($ref resolution, operations)
            ├── performance_testing.rs # Load, Stress, Spike & Volume test runner
            ├── plan_validator.rs   # AST validation, SSRF checks & Kahn's DAG cycle verification
            ├── policy.rs           # Concurrency limits & gated executor policies
            ├── regression.rs       # Scheduled regression: when a schedule is due, what can be repeated, the comparison & report
            ├── regression_api.rs   # Routes: schedules, run now, history, the report of a run
            ├── regression_runner.rs # The loop that makes the runs that are due
            ├── scheduler.rs        # Asynchronous DAG scheduler
            ├── store.rs            # SQLite persistence layer with connection pooling
            └── tools/              # Safe diagnostic tools (schema, dom, requirement, security)
```

> **The two `migrations/` directories are not duplicates of each other.** `services/orchestrator/migrations/` (0001–0026, SQLite dialect) is the only set the service applies, via `sqlx::migrate!("./migrations")` in `store.rs`. `infra/migrations/` (001–020) is the Postgres target schema — it uses Postgres-only constructs such as `CREATE TYPE ... AS ENUM`, `SERIAL`, `JSONB` and `TIMESTAMPTZ`, it is never applied by the running service, and `infra/docker-compose.yml`'s own header describes it as "not yet wired into the orchestrator". The two sets have diverged; the Postgres one is a forward design, not a parallel copy.

---

## 14. Technology Stack

### Backend Orchestrator
* **Language & Edition**: Rust 2021
* **HTTP Framework**: Axum 0.7 (with WebSockets)
* **Async Runtime**: Tokio 1.x (multi-threaded work-stealing runtime)
* **Database & Persistence**: SQLite 3 with Write-Ahead Logging (WAL) via SQLx 0.9
* **HTTP Client**: Reqwest 0.12 (with rustls)
* **Document Parsing**: Zip 2.x & native XML extraction
* **Process Sandboxing**: `windows-sys` (Win32 Job Objects)
* **Metrics**: Prometheus client exposition
* **Serialization**: Serde & Serde JSON

### Frontend Control Plane
* **Framework**: React 19 (`react 19.0.1`, `react-dom 19.0.1`)
* **Build Tool**: Vite 6 (`vite 6.4.3`)
* **Routing**: React Router v7 (`react-router-dom 7.18.2`)
* **Styling**: Tailwind CSS v4 (`@tailwindcss/vite 4.1.14`)
* **Icons**: Lucide React (`lucide-react 0.546.0`)
* **Data Visualization**: Recharts (`recharts 3.10.1`)
* **Export Utilities**: jsPDF (`jspdf 4.2.1`), jsPDF-AutoTable, XLSX
* **Testing**: Vitest (`vitest 2.1.9`), Testing Library, JSDOM

---

## 15. Local Development Guide

### Prerequisites
* **Rust**: `rustc` and `cargo` (version 1.78+)
* **Node.js**: Node.js 18+ and `npm`
* **Operating System**: Windows 10 / 11 (utilizes Win32 Job Objects for process tree sandboxing)
* **Optional Executors**:
  - `playwright`: Node.js with `@playwright/test` installed in `executors/playwright`
  - `newman`: Node.js with `newman` installed in `executors/newman`
  - `pytest`: Python 3.10+ with `pytest` installed
  - `maestro`: Maestro CLI installed for mobile testing

### Quick Start (Windows)
AIQA includes turnkey batch scripts for local operations:

1. **Start Backend & Frontend**:
   ```cmd
   start.bat
   ```
   * Clears stale processes on ports `8080`, `3000`, and `5173`.
   * Loads configurations from `.env`.
   * Installs frontend `node_modules` if missing.
   * Starts the Rust Orchestrator on `http://127.0.0.1:8080`.
   * Polls `GET /health` until the backend is healthy.
   * Starts the Vite frontend on `http://localhost:3000` and launches your browser.

2. **Stop All Services**:
   ```cmd
   stop.bat
   ```
   * Terminate all listening processes on ports `8080`, `3000`, and `5173`.

### Manual Startup

1. **Run Backend Orchestrator**:
   ```cmd
   cd services\orchestrator
   cargo run -p aiqa-orchestrator
   ```
   *Upon initial boot, a bootstrap admin API key is generated and printed to stderr. Copy this key for API access.*

2. **Run Frontend Web UI**:
   ```cmd
   cd forentend
   npm install
   npm run dev
   ```

---

## 16. Environment Variables

Create a `.env` file in the root directory. Never commit secrets to source control.

```dotenv
# ==============================================================================
# AIQA Configuration Template
# ==============================================================================

# Central AI Provider Credentials
MJ_AI_ENDPOINT=http://127.0.0.1:8000
MJ_AI_API_KEY=<configure-this-value>
MJ_AI_TIMEOUT_SECS=60
MJ_AI_MAX_TOKENS=4096

# Ollama Local Configuration
OLLAMA_BASE_URL=http://127.0.0.1:11434
OLLAMA_TIMEOUT_SECS=120
OLLAMA_DISCOVERY_TIMEOUT_SECS=5
# Optional — context window (tokens) sent as options.num_ctx with every request.
# Unset leaves Ollama's own default, which can silently cut a long prompt (e.g. 16384).
# Test-case generation sends the built-in skills (Functional, Edge Case, Security, ...) in
# their detailed form, which gives more accurate cases, only when a call can carry 7000
# tokens or more (Groq's 8000, Ollama with OLLAMA_NUM_CTX=8192, Gemini). On a smaller window
# (Ollama's own 4096) they send the short one-sentence form, so the prompt still fits.
OLLAMA_NUM_CTX=

# --- Target network -----------------------------------------------------
# Where crawls and test executions are allowed to go.
#
# By default every private/internal address is refused, so a scoped API key
# cannot be used to probe the network the orchestrator runs in. That also means
# you cannot test an app running on your own machine or LAN -- which, for a QA
# tool, is usually the app you actually want to test.
#
# Set this to 1 to allow loopback and RFC1918/fc00::/7: a dev server on
# localhost:4200, a device on 192.168.x.x, a Docker service on 172.17.x.x.
#
# It does NOT unlock link-local (169.254.0.0/16 and fe80::/10), which is where
# the cloud metadata endpoint lives, nor multicast, broadcast, unspecified, or
# the documentation ranges. Those stay refused either way.
#
# Any value other than 1/true/TRUE/yes/YES reads as OFF, so a typo fails
# closed. Unset = OFF. Every private target reached under this flag is logged.
# AIQA_ALLOW_PRIVATE_TARGETS=1

# Groq Cloud Configuration
GROQ_BASE_URL=https://api.groq.com/openai/v1
GROQ_API_KEY=<configure-this-value>
GROQ_TIMEOUT_SECS=60
# Optional — the most tokens Groq accepts in one request (its per-minute limit for the
# model in use). A generation call is sized to fit it. Unset = 8000.
GROQ_REQUEST_TOKEN_LIMIT=

# Google Gemini Configuration
GEMINI_BASE_URL=https://generativelanguage.googleapis.com
GEMINI_API_KEY=<configure-this-value>
GEMINI_TIMEOUT_SECS=60

# Orchestrator Policy & Concurrency
AIQA_MAX_CONCURRENCY=4
AIQA_TASK_TIMEOUT_SECS=300
AIQA_REQUIRE_APPROVAL_FOR=maestro
AIQA_BLOCKED_EXECUTORS=
# Optional — Phase 10 raises human-gated healing proposals for tests that keep
# failing. On by default; set to off (or 0 / false / no / disabled) to disable.
AIQA_AUTONOMOUS_HEALING=
AIQA_CORS_ALLOWED_ORIGINS=http://localhost:3000,http://127.0.0.1:3000

# Generating test cases from numbered requirements (Test Case Generator, opt-in)
# Each uploaded document is read whole and split into numbered requirements; a
# generation can then be given those instead of the pasted documents.
# Optional — characters of requirements one AI call may carry. Unset = worked out
# from the window of the model that would serve the call (for Ollama, OLLAMA_NUM_CTX,
# else 4096 tokens), which is what to raise for fewer, larger calls.
REQUIREMENT_BATCH_CHARS=
# Optional — the most AI calls one such request may make (one per skill per batch).
# A larger request is refused before any call is made. Unset = 150. A coverage
# top-up (below) shares this limit with the first pass: it stops when it is reached.
REQUIREMENT_MAX_CALLS=
# Optional — the most rounds of gap-filling one coverage top-up makes (1 to 5).
# Unset = 2. A top-up asks, technique by technique, for the cases a requirement is
# short of at the chosen depth (a negative case, an edge case, ...), and stops when
# every target is met, a round adds nothing, or the call limit is reached.
COVERAGE_TOP_UP_ROUNDS=

# Planning Schema Files (POST /api/v1/projects/:id/planning/test-plan)
# The one directory a request's `schema_file_path` may point into (symlinks,
# junctions and `..` are resolved first, so they cannot escape it). Unset or
# blank = server-side schema files are disabled and callers send the schema
# inline as `schema_content` instead.
AIQA_SCHEMA_ROOT=

# Login secrets for the crawler and for compiled browser tests (optional)
# A login step's FILL value written env:NAME (for example env:AIQA_LOGIN_ADMIN_PASSWORD)
# is read from this environment when the crawl or the test runs. The plan, the
# generated spec, the stored run and the evidence hold only the name, and a run
# whose variable is not set is refused before it starts. NAME must start with
# AIQA_LOGIN_ or AIQA_SECRET_ — no other variable can be referenced.
AIQA_LOGIN_ADMIN_USERNAME=
AIQA_LOGIN_ADMIN_PASSWORD=

# Performance Testing Safeguards
PERF_TEST_MAX_CONCURRENCY=50
PERF_TEST_MAX_DURATION_SECS=300
PERF_TEST_MAX_TOTAL_REQUESTS=10000

# Webhook Notifications
AIQA_WEBHOOK_URL=
# Optional — when set, every webhook delivery carries a real HMAC-SHA256
# signature (`X-AIQA-Signature-256: sha256=<hex>`) over the raw JSON body,
# so the receiver can verify it genuinely came from this server. Unset
# means an unsigned delivery, exactly like before this existed.
AIQA_WEBHOOK_SECRET=

# Frontend Dev Injection (Local Development Only)
VITE_AIQA_ORCH_KEY=<configure-this-value>
```

---

## 17. Testing & Verification

The codebase enforces strict test verification across both backend and frontend layers:

### Backend Test Suite (`services/orchestrator`)
```cmd
cd services\orchestrator

# Run full test suite (1,631 tests passing, 2 ignored)
cargo test -p aiqa-orchestrator

# Verify compilation and lint integrity
cargo check -p aiqa-orchestrator
cargo check --tests -p aiqa-orchestrator

# Verify code formatting
cargo fmt --check
```

### Frontend Test Suite (`forentend`)
```cmd
cd forentend

# Run full test suite (1,501 tests across 71 test files)
npm test

# Type-check TypeScript sources
npm run lint

# Build production bundle
npm run build
```

---

## 18. API Reference

All protected endpoints require an `Authorization: Bearer <api_key>` header unless noted.

### Core & Auth
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `GET` | `/health` | Healthcheck returning `"ok"` | No |
| `GET` | `/metrics` | Prometheus metrics exposition format | Yes |
| `GET` | `/api/v1/auth/whoami` | Retrieve caller identity, role, and permissions | Yes |
| `POST` | `/admin/api-keys` | Provision a new API key (`Admin` role) | Yes |
| `GET` | `/admin/api-keys` | List active API keys (`Admin` role) | Yes |
| `DELETE` | `/admin/api-keys/:id` | Revoke an API key (`Admin` role) | Yes |
| `GET` | `/admin/audit-log` | System-wide audit log entries | Yes |

### Projects & Dashboard
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/projects` | Create a new project | Yes |
| `GET` | `/api/v1/projects` | List all accessible projects | Yes |
| `GET` | `/api/v1/projects/:id` | Get project details | Yes |
| `PATCH` | `/api/v1/projects/:id` | Update project metadata | Yes |
| `DELETE` | `/api/v1/projects/:id` | Delete project and associated records | Yes |
| `GET` | `/api/v1/projects/:project_id/dashboard` | Aggregated dashboard health metrics | Yes |
| `GET` | `/api/v1/projects/:project_id/kpis` | Real-time quality KPI figures | Yes |

### Executions & Tasks
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/projects/:project_id/executions` | Dispatch a new execution run plan | Yes |
| `GET` | `/api/v1/projects/:project_id/executions` | List runs for a project | Yes |
| `GET` | `/api/v1/projects/:project_id/executions/:run_id` | Get run details and task statuses | Yes |
| `POST` | `/api/v1/projects/:project_id/executions/:run_id/approve` | Approve a run held in approval gate | Yes |
| `POST` | `/api/v1/projects/:project_id/executions/:run_id/reject` | Reject a held run | Yes |
| `POST` | `/api/v1/projects/:project_id/executions/:run_id/cancel` | Cancel an active execution run | Yes |
| `POST` | `/api/v1/projects/:project_id/executions/:run_id/retry-failed`| Re-execute failed tasks in a run | Yes |
| `GET` | `/api/v1/projects/:project_id/executions/:run_id/tasks` | List individual tasks for a run | Yes |
| `GET` | `/api/v1/projects/:project_id/executions/:run_id/tasks/:task_id`| Get task execution details and logs | Yes |

### Test Cases, Plans & Suites
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/projects/:project_id/test-cases` | Create a verified test case | Yes |
| `GET` | `/api/v1/projects/:project_id/test-cases` | List test cases with filtering | Yes |
| `GET` | `/api/v1/projects/:project_id/test-cases/:id` | Get test case definition | Yes |
| `POST` | `/api/v1/projects/:project_id/test-cases/:id/validate` | Validate test case AST and targets | Yes |
| `POST` | `/api/v1/projects/:project_id/test-cases/:id/approve` | Approve test case for execution | Yes |
| `POST` | `/api/v1/projects/:project_id/test-cases/:id/handoff` | Materialize case into executable run | Yes |
| `POST` | `/api/v1/projects/:project_id/test-cases/generate-ai` | Generate non-persisted test drafts (from requirements: `requirement_document_ids`, and optionally `requirement_keys` to take only some of them) | Yes |
| `POST` | `/api/v1/projects/:project_id/gap-check` | Line the requirements up with a crawl and with their cases | Yes |
| `POST` | `/api/v1/projects/:project_id/login-suggestion` | Suggest a role's login steps from what a crawl saw on the login page | Yes |
| `POST` | `/api/v1/projects/:project_id/autopilot/estimate` | What an autopilot run would cost: AI calls, tokens, time and dollars, as ranges | Yes |
| `POST` | `/api/v1/projects/:project_id/autopilot/runs` | Make an autopilot run with its estimate (starts nothing) | Yes |
| `GET` | `/api/v1/projects/:project_id/autopilot/runs` | List the project's autopilot runs | Yes |
| `GET` | `/api/v1/projects/:project_id/autopilot/runs/:run_id` | A run with its steps and the latest of its timeline | Yes |
| `POST` | `/api/v1/projects/:project_id/autopilot/runs/:run_id/start` | Accept the estimate and start the run | Yes |
| `POST` | `/api/v1/projects/:project_id/autopilot/runs/:run_id/cancel` | Cancel a run | Yes |
| `POST` | `/api/v1/projects/:project_id/autopilot/runs/:run_id/continue` | Let a run that waits at the approval gate go on | Yes |
| `POST` | `/api/v1/projects/:project_id/autopilot/runs/:run_id/resume` | Take a failed run over and start it again at the step that failed | Yes |
| `POST` | `/api/v1/projects/:project_id/autopilot/runs/:run_id/approve-cases` | Approve the run's validated cases (or the ones named) as the person asking | Yes |
| `GET` | `/api/v1/projects/:project_id/autopilot/runs/:run_id/report` | The run's report (`?format=markdown` for text) | Yes |
| `GET` | `/api/v1/projects/:project_id/autopilot/runs/:run_id/events` | A run's timeline, from an event id | Yes |
| `POST` | `/api/v1/projects/:project_id/openapi/plan` | The API test cases an OpenAPI document gives, with what each expects (runs nothing) | Yes |
| `POST` | `/api/v1/projects/:project_id/openapi/runs` | Run those cases as one execution | Yes |
| `GET` | `/api/v1/projects/:project_id/openapi/runs` | List the project's OpenAPI test runs | Yes |
| `GET` | `/api/v1/projects/:project_id/openapi/runs/:id` | A run's report, operation by operation (`?format=markdown` for text, `?format=junit` for JUnit XML, `?wait=` to wait for the end) | Yes |
| `GET` | `/api/v1/projects/:project_id/regression/scope` | What a regression schedule would repeat now, and what it could not and why | Yes |
| `POST` | `/api/v1/projects/:project_id/regression/schedules` | Make a regression schedule | Yes |
| `GET` | `/api/v1/projects/:project_id/regression/schedules` | List the project's schedules, each with its latest run | Yes |
| `PATCH` | `/api/v1/projects/:project_id/regression/schedules/:id` | Change a schedule (name, interval, start, suite, concurrency, on or off) | Yes |
| `DELETE` | `/api/v1/projects/:project_id/regression/schedules/:id` | Remove a schedule and its history | Yes |
| `POST` | `/api/v1/projects/:project_id/regression/schedules/:id/run` | Run a schedule now | Yes |
| `GET` | `/api/v1/projects/:project_id/regression/schedules/:id/runs` | A schedule's latest runs, newest first | Yes |
| `GET` | `/api/v1/projects/:project_id/regression/runs/:id` | A regression run's report, compared with the last time (`?format=markdown` for text, `?format=junit` for JUnit XML, `?wait=` to wait for the end) | Yes |
| `POST` | `/api/v1/projects/:project_id/test-plans` | Create a test plan | Yes |
| `GET` | `/api/v1/projects/:project_id/test-plans` | List test plans | Yes |
| `POST` | `/api/v1/projects/:project_id/test-suites` | Create a test suite | Yes |
| `GET` | `/api/v1/projects/:project_id/test-suites` | List test suites | Yes |

### Governed Self-Healing
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/projects/:project_id/healing/proposals` | Submit a diagnostic healing proposal | Yes |
| `GET` | `/api/v1/projects/:project_id/healing/proposals` | List healing proposals | Yes |
| `GET` | `/api/v1/projects/:project_id/healing/proposals/:proposal_id` | Get proposal details & risk rating | Yes |
| `GET` | `/api/v1/projects/:project_id/healing/proposals/:proposal_id/diff` | Get Action AST diff | Yes |
| `POST` | `/api/v1/projects/:project_id/healing/proposals/:proposal_id/approve` | Human operator approval | Yes |
| `POST` | `/api/v1/projects/:project_id/healing/proposals/:proposal_id/reject` | Human operator rejection | Yes |
| `POST` | `/api/v1/projects/:project_id/healing/proposals/:proposal_id/execute` | Dispatch single verification run | Yes |
| `GET` | `/api/v1/projects/:project_id/healing/feedback` | Retrieve planning agent feedback | Yes |

### Adaptive QA Intelligence
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/projects/:project_id/intelligence/telemetry` | Raw execution telemetry records | Yes |
| `GET` | `/api/v1/projects/:project_id/intelligence/failures` | Clustered failure fingerprints | Yes |
| `GET` | `/api/v1/projects/:project_id/intelligence/flakiness` | Flakiness scores & sequence history | Yes |
| `GET` | `/api/v1/projects/:project_id/intelligence/strategies`| Healing strategy reliability scores | Yes |
| `GET` | `/api/v1/projects/:project_id/intelligence/trends` | Trajectory quality trend analyses | Yes |
| `GET` | `/api/v1/projects/:project_id/intelligence/quality-graph` | Quality graph topology (nodes & edges) | Yes |

### Real-Time WebSockets
| Protocol | Endpoint | Description | Auth Mechanism |
| :--- | :--- | :--- | :--- |
| `WS` | `/api/v1/projects/:project_id/events?key=<key>` | Live execution events stream | Query Param `?key=` |
| `WS` | `/api/v1/logs/ws?key=<key>` | System-wide orchestrator log stream | Query Param `?key=` |
| `WS` | `/api/v1/projects/:project_id/logs/ws?key=<key>`| Project-scoped log stream | Query Param `?key=` |

### Additional registered endpoints

The tables above are **not** a complete API reference: they documented only **76 of the 144** method-endpoints the service registers. The remaining endpoints are listed below, grouped by what they act on. A handful of entries that duplicate rows already given above (`POST .../healing/proposals`, `GET .../test-cases`, `GET .../executions`, `GET .../openapi/runs`, `GET .../regression/schedules`, `GET .../regression/schedules/:id/runs`) are not repeated here.

Every one of these is registered inside `protected_routes()` and requires `Authorization: Bearer <api_key>`, so the **Auth Required** column is `Yes` throughout unless a row says otherwise.

#### AI Providers, Settings & Skills (state-mutating)
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/ai/providers/custom` | Register a custom OpenAI-compatible AI provider | Yes |
| `DELETE` | `/api/v1/ai/providers/custom/:id` | Remove a custom AI provider | Yes |
| `POST` | `/api/v1/ai/refresh` | Force provider rediscovery / health re-check | Yes |
| `PUT` | `/api/v1/ai/settings` | Update AI routing & per-task overrides | Yes |
| `PUT` | `/api/v1/execution-settings` | Update execution display settings | Yes |
| `POST` | `/api/v1/skills` | Create a persisted custom generation skill | Yes |
| `DELETE` | `/api/v1/skills/:id` | Remove a custom generation skill | Yes |
| `POST` | `/api/v1/skills/resolve` | Resolve the real instruction each named skill expands to | Yes |

#### AI Providers, Settings & Skills (read-only)
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/ai/status` | Live discovery snapshot, active providers and settings | Yes |
| `GET` | `/api/v1/ai/settings` | Current AI routing settings and per-task overrides | Yes |
| `GET` | `/api/v1/execution-settings` | Current execution display settings | Yes |
| `GET` | `/api/v1/skills/files` | `SKILL.md` files found on disk, re-read on every call | Yes |

#### Mobile Fleet (state-mutating)
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/mobile/emulators/boot` | Boot an Android emulator (AVD) on the real fleet | Yes |
| `POST` | `/api/v1/mobile/simulators/boot` | Boot an iOS simulator on the real fleet | Yes |

#### Mobile Fleet (read-only)
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/mobile/fleet` | Real adb devices, AVDs and Maestro CLI availability | Yes |

#### Project Analysis, Crawl & Generation Tools (state-mutating)
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/projects/:project_id/api-testing/execute` | Execute an API collection / request test | Yes |
| `POST` | `/api/v1/projects/:project_id/crawl` | Crawl the running application as one or more roles | Yes |
| `POST` | `/api/v1/projects/:project_id/custom-testing/generate` | Generate cases from a custom testing configuration | Yes |
| `POST` | `/api/v1/projects/:project_id/documents/extract-text` | Extract real text from an uploaded `.docx` / `.pdf` / text document | Yes |
| `POST` | `/api/v1/projects/:project_id/verify-application-source` | SSRF-guarded connectivity check against the application | Yes |
| `POST` | `/api/v1/projects/:project_id/planning/test-plan` | Build a test plan against schema files or inline schema | Yes |
| `POST` | `/api/v1/projects/:project_id/test-cases/generate-ai/plan` | Show the AI generation plan (calls, tokens, time, cost) without generating | Yes |

#### Test Cases, Plans & Suites (state-mutating)
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/projects/:project_id/test-cases/:id/compile` | Compile a browser case against a named crawl (grounding) | Yes |
| `POST` | `/api/v1/projects/:project_id/test-cases/:id/reject` | Reject a case at the review gate | Yes |
| `PUT` | `/api/v1/projects/:project_id/test-cases/:id/suite` | Move a case to another suite | Yes |
| `PATCH` | `/api/v1/projects/:project_id/test-cases/:id` | Update a case's definition or metadata | Yes |
| `DELETE` | `/api/v1/projects/:project_id/test-cases/:id` | Delete a test case | Yes |
| `PATCH` | `/api/v1/projects/:project_id/test-plans/:id` | Update a test plan | Yes |
| `DELETE` | `/api/v1/projects/:project_id/test-plans/:id` | Delete a test plan | Yes |
| `PATCH` | `/api/v1/projects/:project_id/test-suites/:id` | Update a test suite | Yes |
| `DELETE` | `/api/v1/projects/:project_id/test-suites/:id` | Delete a test suite | Yes |
| `PUT` | `/api/v1/projects/:project_id/test-suites/:id/plan` | Attach or detach a suite's parent plan | Yes |

#### Findings (state-mutating)
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/projects/:project_id/findings/:finding_id/analysis` | On-demand failure analysis for a finding | Yes |
| `POST` | `/api/v1/projects/:project_id/findings/:finding_id/ai-hypothesis` | On-demand AI root-cause hypothesis for a finding | Yes |
| `POST` | `/api/v1/projects/:project_id/findings/:finding_id/review` | Record a human review decision on a finding (see §19 on promotion) | Yes |

#### Performance Testing (state-mutating & read-only)
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/projects/:project_id/performance-testing/start` | Start a Load / Stress / Spike / Volume run | Yes |
| `POST` | `/api/v1/projects/:project_id/performance-testing/:run_id/cancel` | Cancel an active performance run | Yes |
| `GET` | `/api/v1/projects/:project_id/performance-testing` | List the project's performance runs | Yes |
| `GET` | `/api/v1/projects/:project_id/performance-testing/:run_id/status` | A performance run's live status | Yes |

#### Intelligence Retention — read `dry-run` and DESTRUCTIVE `prune`
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/projects/:project_id/intelligence/retention/dry-run` | Report what a retention prune *would* delete. Safe to call; changes nothing | Yes |
| `POST` | `/api/v1/projects/:project_id/intelligence/retention/prune` | **DESTRUCTIVE — permanently deletes telemetry and intelligence records past the retention window. There is no undo, no soft-delete and no recycle bin. `Admin` role only; do not call it from a UI action that is easy to click by accident** | Yes (`Admin` only) |

#### Execution, Evidence & Audit Read Models
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/projects/:project_id/executions/:run_id/events` | A run's recorded events, from an event id | Yes |
| `GET` | `/api/v1/projects/:project_id/executions/:run_id/evidence` | The evidence list for one run | Yes |
| `GET` | `/api/v1/projects/:project_id/evidence/:evidence_id` | One evidence record's metadata | Yes |
| `GET` | `/api/v1/projects/:project_id/evidence/:evidence_id/content` | One evidence record's file content | Yes |
| `GET` | `/api/v1/projects/:project_id/audit` | The project's audit log (`audit_log`) | Yes |
| `GET` | `/api/v1/projects/:project_id/requirements` | Requirements read out of the uploaded documents | Yes |
| `GET` | `/api/v1/projects/:project_id/requirement-impact` | Which requirements a change would affect | Yes |
| `GET` | `/api/v1/projects/:project_id/crawl/:run_id/result` | A finished crawl's recorded result | Yes |

#### Pipeline, Orchestrator, Logs & Risk
| Method | Endpoint | Description | Auth Required |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/projects/:project_id/pipeline` | Stage-by-stage pipeline state, with honestly-reported unavailable stages | Yes |
| `GET` | `/api/v1/orchestrator/state` | Executor capability routing, policy gates and the live DAG | Yes |
| `GET` | `/api/v1/logs` | System-wide orchestrator log records | Yes |
| `GET` | `/api/v1/projects/:project_id/logs` | Project-scoped log records | Yes |
| `GET` | `/api/v1/projects/:project_id/intelligence/risk` | Risk classification for the project's tests | Yes |
| `GET` | `/api/v1/projects/:project_id/intelligence/tests` | The tests the intelligence layer knows about | Yes |
| `GET` | `/api/v1/projects/:project_id/intelligence/investigations` | Recorded failure investigations | Yes |
| `GET` | `/api/v1/projects/:project_id/intelligence/failures/:cluster_id/investigation` | One cluster's investigation | Yes |
| `GET` | `/api/v1/projects/:project_id/intelligence/failures/:cluster_id/environments` | One cluster's environment breakdown | Yes |
| `GET` | `/api/v1/projects/:project_id/intelligence/failures/:cluster_id/timeline` | One cluster's failure timeline | Yes |

---

## 19. Reports & Evidence

* **Evidence Persistence (`evidence.rs`)**: Subprocess outputs, execution logs, screenshots, and error dumps are saved to disk under `infra/data/evidence/`.
* **Traceable Findings (`findings`)**: When a test fails or introduces an unexpected outcome, the engine generates an auditable `finding` record.
* **Finding Promotion**: Operators can review findings and invoke on-demand AI hypothesis analysis (`/findings/:id/ai-hypothesis`). Promotion happens **inside AIQA's own store**: `bug_drafts` records `promoted_at` / `promoted_by` and a `review_state`, changed through `POST /api/v1/projects/:project_id/findings/:finding_id/review`. There is **no external tracking-system integration** — no Jira, Linear or GitHub Issues connector, no external ticket id column and no ticket route. A promoted bug is marked in AIQA; it is not filed anywhere else.
* **Export Formats**: The Reports view (`ReportsPage.tsx`) provides one-click export to **PDF** (via jsPDF and AutoTable) and **Excel / CSV** (via XLSX) for audit compliance.

---

## 20. Implementation Status

| Milestone | Component / Capability | Status | Verification Baseline |
| :--- | :--- | :--- | :--- |
| **Phase 0** | Core Architecture, SQLite Store, Engine & Scheduler | **VERIFIED** | 521 tests passed |
| **Phase 1** | Native Rust Agent Foundation & Tool Abstraction | **VERIFIED** | 20 unit & boundary tests |
| **Phase 2** | Safe Diagnostic Tools (`schema_inspector`, `dom_prober`) | **VERIFIED** | 32 diagnostic tests |
| **Phase 3** | Specialized QA Agents (`TestPlanningAgent`, `SelfHealingAgent`) | **VERIFIED** | 10 multi-turn agent tests |
| **Phase 4** | Governed Execution Integration & Plan Validator | **VERIFIED** | 33 policy & sandbox tests |
| **Phase 5** | Governed Self-Healing Service & Closed Action AST Diffing | **VERIFIED** | 42 governance tests |
| **Phase 6** | Adaptive QA Intelligence & Production Observability Backend | **VERIFIED** | 40 intelligence tests |
| **Phase 7** | QA Intelligence Dashboard & Observability UI (`forentend`) | **VERIFIED** | 19 UI tests (1,501 total frontend) |
| **Total Backend** | Complete Rust Orchestrator Suite (`services/orchestrator`) | **VERIFIED** | **1,631 passed, 0 failed, 2 ignored** |
| **Total Frontend**| Complete Web Control Plane Suite (`forentend`) | **VERIFIED** | **1,501 tests across 71 test files** |

---

## 21. Roadmap

* **Phase 8 — Autonomous Crawl Auto-Grounding**: Deep integration between the offline web crawler and the `TestPlanningAgent` to automatically infer state transitions and form interaction schemas without user prompting.
* **Distributed Subprocess Workers**: Remote execution node clustering for high-throughput browser farm execution across Linux and Windows worker nodes.
* **Native S3 / MinIO Evidence Backend**: Pluggable cloud object storage for multi-terabyte screenshot and video evidence archives.

---

## 22. Contributing

We welcome contributions to AIQA. Please adhere to the following guidelines:
1. **Source Code Invariants**: Never introduce unmonitored code evaluation, Python runtime wrappers, or bypasses to the Closed Action AST.
2. **Deterministic Governance**: All changes to agent outputs or self-healing must pass through the `PlanValidator` and respect policy gates.
3. **Verification Standards**: Ensure `cargo test -p aiqa-orchestrator`, `cargo check`, `cargo fmt --check`, and `npm test` (in `forentend`) pass with zero errors before submitting pull requests.

---

## 23. Security & Vulnerability Reporting

If you identify a security vulnerability or potential SSRF bypass within AIQA, please report it responsibly:
* Submit a detailed report with reproduction steps to the project administrators.
* Please avoid opening public issues containing sensitive exploit demonstrations prior to remediation.

---

## 24. License

License: Not currently specified.
