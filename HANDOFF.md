# AIQA — Session Handoff (2026-08-31)

Written because the user is switching Claude accounts mid-session. Read this
fully before touching anything — there are two live checkouts, a live
background generation job that was interrupted, and several uncommitted
features.

## 1. The two-checkout situation (important, read first)

This repo exists in **two separate places on disk** that are NOT symlinked —
editing one does not affect the other:

- **Worktree (where Claude Code actually runs)**: `D:\AIQA\.claude\worktrees\run-this-9eeb2e`
  branch `claude/run-this-9eeb2e`.
- **Main checkout (what the user's browser/dev server actually points at)**:
  `D:\AIQA` branch `Demo`.

`preview_start`/the Browser pane always serves from **`D:\AIQA\forentend`**
(the main checkout), regardless of which worktree Claude's cwd is in. The
convention established this session: **after editing any file in the
worktree, `cp` it into the matching path under `D:\AIQA` too**, or the user's
live preview will keep showing stale code. Every file below has already been
copied to both locations as of this handoff — but if you edit further, keep
copying both ways.

## 2. What shipped this session (all real, tested, verified live)

1. **Truthfulness Cleanup Step 1 — Application Source connectivity**
   (`app_source.rs`, `ApplicationSourceSection.tsx`) — real SSRF-guarded
   backend connectivity check, replacing a fake setTimeout simulation.
   Fully committed and pushed before this handoff's uncommitted work began.

2. **AI Engine Orb visual rework** (`AIEngineOrb.tsx`) — denser real
   Fibonacci-sphere particle sphere with a wobble+spin dual layer and
   layered glow; removed the redundant on-orb score display (real score is
   already shown as text elsewhere in the panel). Every visual property
   still derives from real routing-score/capability data — no new
   decorative elements.

3. **Test Case ID display fix** (`GeneratedTestCasesView.tsx`) — the UI was
   showing the raw backend UUID as "Test Case ID". Now computes a stable
   `TC-01`, `TC-02`... label from real creation order (or an existing
   `traceability.testCaseId` if the AI-generation path already set one like
   `AI-001`). The real UUID is untouched underneath for all API calls.

4. **Real Product Documentation content extraction — the big one**
   (`doc_extract.rs`, new backend module + `POST
   /api/v1/projects/:id/documents/extract-text`, `DocumentUploadSection.tsx`
   rewritten). Before this, uploading a BRD/FRD did *nothing* — AIQA's own
   `buildRequirementText()` only ever sent the AI model the document's
   *filename*, never its content (`"content is not analyzed by this
   request"`). Now:
   - `.docx` is parsed for real (it's a zip; reads `word/document.xml`,
     extracts real `<w:t>` text runs, paragraph-aware).
   - `.txt`/`.md`/`.json`/`.yaml` read verbatim.
   - Legacy `.doc` and `.pdf` are honestly refused with a specific reason
     (not guessed at, not silently dropped).
   - Extracted text (capped at 20,000 chars/doc, disclosed if truncated) is
     folded into the real AI generation prompt.
   - **Verified live**: uploading the user's real BRD+FRD produced a real
     40,639-character prompt containing actual document text, and the AI
     (see below) genuinely used it — citing real FR-003/FR-005 rate-limit
     numbers that match the FRD exactly.
   - Caught and fixed a real bug during verification: the extraction regex
     `<w:t[^>]*>` also matched `<w:tblPr>`/`<w:tblW>` etc., leaking raw
     table XML into extracted text for any doc containing a table. Fixed,
     regression-tested (`table_property_elements_are_never_mistaken_for_text_runs`).
   - Also fixed a jsdom incompatibility: `File.arrayBuffer()` doesn't exist
     in the test environment; switched to `FileReader.readAsDataURL`.

   Tests: 12 new backend unit tests (`doc_extract`), 6 new backend
   integration tests (`extract_document_text_*`), 6 new frontend tests
   (`DocumentUploadSection.test.tsx`). Full suites green: backend 375/375,
   frontend 490/490, clean `tsc`, clean production build.

## 3. Manually-authored test cases (before the doc-extraction feature existed)

Before item 4 above was built, the AI generation pipeline had no way to read
real documents at all, so **35 test cases were hand-authored** (by reading
the user's real BRD/FRD/RTM myself and writing genuinely-grounded cases via
direct `POST .../test-cases` calls) to cover every one of the platform's 17
real modules. These are real, saved, and still in the project — not
something to redo.

## 4. Live AI generation — IN PROGRESS, was interrupted

The user asked for ~500 test cases total. Plan agreed with the user: run a
first batch to **100**, let them review quality, then decide whether to push
to 500.

**Current real state**: the background bulk-generation job was killed when
the previous Claude Code process/session ended (orphaned background task,
confirmed via `tasklist`/`netstat` — nothing was running when this handoff
session picked back up). Real progress made before the interruption:

- Test case count went from **35 → 82** (47 new cases genuinely generated by
  the user's own Ollama model and saved via the real endpoint) before the
  process died mid-batch, short of the 100 target.
- Every one of those 82 currently in the DB is real — verified by pulling
  them back from `GET /projects/.../test-cases` and inspecting content (real
  FR-id citations, rate-limit numbers matching the FRD verbatim).

**IMPORTANT — model correction the user made mid-session**: I initially
started a *local* Ollama with `gemma3:12b` by mistake. The user corrected
this — they have `qwen3:14b` running on a separate machine reachable via
**Tailscale** at `100.64.34.28:11434` (see their `tailscale status` output:
`server  100.64.34.28  abhisummer26@  windows`), and that is the model that
must be used, not a local one. The orchestrator must be started with:

```bash
OLLAMA_BASE_URL="http://100.64.34.28:11434"
```

(bash) or as an environment variable before `cargo run --bin
aiqa-orchestrator`, from `D:\AIQA\services\orchestrator` (the **main**
checkout, since that's what the live preview/generation was actually using).
Do **not** fall back to a local Ollama model — the user was explicit that
their own `qwen3:14b` via Tailscale must be the one AIQA uses.

**Status at the moment this handoff was written**: the orchestrator is
*currently already running* (started fresh during this handoff, from
`D:\AIQA\services\orchestrator`, with `OLLAMA_BASE_URL=http://100.64.34.28:11434`
correctly set) — check `netstat -ano | grep ':8080'` before starting a
second one. If it's still up, skip step 2 below and go straight to step 3.

**To resume the batch to 100 (then ask the user about 500)**:

1. Confirm the Tailscale server/`qwen3:14b` is still reachable:
   `curl http://100.64.34.28:11434/api/tags` (must show `qwen3:14b`).
2. Start the orchestrator from `D:\AIQA\services\orchestrator` with
   `OLLAMA_BASE_URL=http://100.64.34.28:11434 cargo run --bin
   aiqa-orchestrator` (background it; wait for "listening on" in its log).
3. Call `POST /api/v1/ai/refresh` once with an admin key so the router
   re-discovers the now-reachable provider (it caches discovery at startup;
   if Ollama wasn't up yet when the orchestrator started, this forces a
   re-check).
4. Mint a fresh scratch admin API key directly in
   `D:\AIQA\infra\data\orchestrator.sqlite3` (the pattern used all session:
   insert into `api_keys` with `role='admin'`, `project_id=NULL`, delete it
   when done — see any of this session's earlier key-minting Bash calls for
   the exact SQL). The frontend key `localStorage` key name is
   `aiqa.orchestrator.apiKey`; API key format is `aiqa_<64 hex chars>`, and
   the stored hash is SHA-256 of the raw key.
5. Re-run the bulk generator script:
   `C:\Users\Orange\AppData\Local\Temp\claude\D--AIQA--claude-worktrees-run-this-9eeb2e\92048e7d-7155-4391-86be-873d5bef8023\scratchpad\bulk_gen.py`
   (target count as `sys.argv[1]`, e.g. `python bulk_gen.py 100`). It:
   - Reads the real extracted FRD from
     `...\scratchpad\frd.md` (already extracted this session from the
     user's real `FRD_AmbianceHolidays 1.docx`; regenerate via
     `extract_docx.py` in the same folder if that file is gone).
   - Splits it into its real `## 4.x Module` sections.
   - Rotates through real test categories (Functional/Negative/Edge
     Case/Security/UI-UX/Business Logic/API/Non-Functional/Positive) ×
     every module section, calling the real
     `POST .../test-cases/generate-ai` endpoint each time with real FRD
     content in the prompt.
   - De-duplicates against titles already in the project (fetches the real
     current list first) so re-running tops up rather than duplicating.
   - Saves each new, non-duplicate case via the real
     `POST .../test-cases` endpoint.
   - Prints per-call progress (`module / category -> +N (total X, Ys)`) so
     you can watch it live — **the API key path is hardcoded to
     `C:\Users\Orange\AppData\Local\Temp\aiqa_key.txt`**, so write the fresh
     scratch key there before running.
   - Takes roughly 30-60s per call, ~6-8 cases/call → expect ~10-15 more
     calls (~10-15 min) to close the gap from 82 to 100, and roughly 45-75
     min total if the user says yes to pushing on to 500 afterward.
6. When done (or when the user wants to stop and review), **delete the
   scratch API key** from the sqlite file — do not leave it lying around.
7. Ask the user whether to continue toward 500 once they've reviewed the
   100.

**The project/target throughout**: project id
`67a87ad1-0db7-4c08-b3b7-158107bacd4e` (project name "ambiance",
representing the real "Ambiance Holidays B2B Travel Platform" whose
BRD/FRD/SOW/RTM the user supplied from `C:\Users\Orange\Downloads\`).

## 5. Current uncommitted changes (nothing has been committed/pushed this session past the Step 1 work)

Worktree (`D:\AIQA\.claude\worktrees\run-this-9eeb2e`) — all real, all
tested, ready to commit whenever the user asks:

```
M forentend/src/components/dashboard/AIEngineOrb.tsx
M forentend/src/components/pages/DashboardPage.test.tsx
M forentend/src/components/pages/TestCaseGeneratorPage.tsx
M forentend/src/components/test-generator/DocumentUploadSection.tsx
M forentend/src/components/test-generator/GeneratedTestCasesView.tsx
M forentend/src/services/aiqaApi.ts
M forentend/src/types.ts
M forentend/src/types/contract.ts
M services/orchestrator/Cargo.lock
M services/orchestrator/Cargo.toml
M services/orchestrator/src/api.rs
M services/orchestrator/src/api_tests.rs
M services/orchestrator/src/lib.rs
?? forentend/src/components/test-generator/DocumentUploadSection.test.tsx
?? services/orchestrator/src/doc_extract.rs
```

Also `?? executors/playwright/tests/generated/` — gitignored runtime
artifact, not meant to be committed (leftover from earlier live-execution
testing this session).

**Do not commit/push without the user explicitly asking** — that rule held
all session and should keep holding.

**Note on the main checkout** (`D:\AIQA`, branch `Demo`): running `git
status` there also shows `M forentend/package-lock.json` and `M
forentend/package.json` (staged) and an untracked
`docs/SPRINT_PLAN_2026-08-24.md` that **I did not create or touch this
session** — these are presumably from the user's own separate work or
another process in that checkout. Don't assume they're safe to discard;
investigate before touching (per the standing git-safety rules) if they
become relevant.

## 6. Known, disclosed limitations (not bugs — stated honestly in the UI/code)

- PDF content extraction: not implemented. `doc_extract.rs` returns a real,
  specific "not yet supported" error; the UI shows it honestly per-file.
- Legacy `.doc` (pre-2007 binary Word format): same treatment — real,
  specific "unsupported, save as .docx" error.
- Extracted text is capped at 20,000 characters per document; truncation is
  disclosed both in the API response (`truncated: true`) and the UI badge.

## 7. Environment quick-reference

- Backend: `services/orchestrator` (Rust/Axum/SQLite). Run from
  `D:\AIQA\services\orchestrator` for anything the live preview should
  reflect (main checkout, not the worktree copy).
- Frontend dev server: `preview_start({name: "forentend"})` — always serves
  `D:\AIQA\forentend`, port 3000.
- Dev DB: `D:\AIQA\infra\data\orchestrator.sqlite3` — gitignored, has real
  accumulated projects/keys/test cases from this whole session's live
  verification passes. The "ambiance" project and its 82 (at last check)
  test cases live here.
- Scratch API key convention used all session: mint directly via
  `sqlite3`/Python (`INSERT INTO api_keys ... role='admin' ...`), use for
  live verification, **always delete it again when done** — never leave a
  standing credential in the DB.
- Ollama for AI generation: **the user's own machine via Tailscale**,
  `http://100.64.34.28:11434`, model `qwen3:14b`. Do not substitute a local
  model without the user's explicit say-so (this was already a live
  correction once this session — see §4).

## 8. Immediate next action

Resume the bulk-generation batch per §4, starting from 82/100, then check
in with the user about quality and whether to continue to 500.
