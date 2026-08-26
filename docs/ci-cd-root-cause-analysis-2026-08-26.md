# AIQA CI/CD Root-Cause Analysis — 2026-08-26

## 1. Scope

**Repository:** `Abhidark/AIQA`  
**Pull Request inspected:** #10 — `Demo`  
**PR head:** `0f20932d3f8fa3e1522a1d3765e14ce7fc8c8efd`  
**PR merge commit used by GitHub Actions:** `dce95fddad6a31aa40371e3671571376d4e65d3d`  
**Workflow run:** `32977972480`  
**Workflow file:** `.github/workflows/ci.yml`  
**Analysis date:** 2026-08-26

## 2. Executive Summary

The inspected GitHub Actions run is failing for **three separate reasons**. They are not one common failure:

1. **Secret-scanning job failure — CI configuration defect.** Gitleaks starts correctly, but the `pull_request` job's `GITHUB_TOKEN` only has `contents: read`, `metadata: read`, and `packages: read`. Gitleaks then calls the pull-request commits API and receives HTTP 403 `Resource not accessible by integration`. The accepted permission reported by GitHub is `pull_requests=read`, confirming the missing permission. This is a workflow-permission problem, not evidence that a secret was detected.
2. **Frontend test failure — evidence Blob/preview test-environment incompatibility.** The frontend typecheck/lint step passes, and the test suite reaches 412 tests. Exactly 3 tests fail, all around evidence content: one `Blob` instance assertion and two EvidencePage content/redaction assertions. The failures are consistent with the test-created `Response` returning a Blob from a different JS realm than the global `Blob` constructor used by the assertion/UI helper. The production page intentionally uses `FileReader`, so the fix should make the test transport deterministic and realm-safe rather than weakening the UI behavior.
3. **Backend test failure — restart/approval state contract defect.** Cargo fmt and clippy pass. Of 322 backend tests, 319 pass, 2 are ignored, and one fails: `api_tests::approval_survives_a_process_restart_and_can_still_be_approved`. The observed response state is `PARTIAL`, while the test requires `RUNNING`. The current engine explicitly marks a newly approved run `RUNNING` before spawning background execution, but the restarted/re-hydrated execution path can retain/derive partial task state. The problem is therefore in the post-restart state transition/re-hydration contract, not in compilation or formatting.

There is also a **CI/CD architecture gap**: the repository currently contains only `.github/workflows/ci.yml`; there is no dedicated deployment/CD workflow in the inspected revision. The pipeline is CI only.

## 3. Failure Matrix

| Area | Job | Result | Root Cause | Priority |
|---|---|---|---|---|
| Secret scanning | `Secrets` | Failed | Missing `pull-requests: read` permission for Gitleaks on `pull_request` runs | P0 |
| Frontend | `Frontend (typecheck, test, build)` | Failed | Evidence tests use/expect cross-realm Blob identity and then fail to render text/redacted content | P0 |
| Backend | `Backend (format, clippy, test)` | Failed | Restarted pending-approval execution does not preserve the expected post-approval `RUNNING` contract | P0 |
| Build/lint | Frontend lint/typecheck | Passed | No compile/type failure observed | — |
| Rust quality | fmt/clippy | Passed | Only warnings, no blocking formatter/lint failure | — |
| Deployment | CD | Not present | No dedicated deployment workflow exists in `.github/workflows` | P1 |

## 4. Root Cause #1 — Gitleaks / GitHub Token Permissions

### 4.1 Observed failure

The `Secrets` job starts on a `pull_request` event. The runner reports these token permissions:

- `Contents: read`
- `Metadata: read`
- `Packages: read`

Gitleaks v8.24.3 then requests:

`GET https://api.github.com/repos/Abhidark/AIQA/pulls/10/commits`

GitHub responds:

`403 Resource not accessible by integration`

and advertises the required accepted permission as:

`pull_requests=read`

### 4.2 Why this happened

The workflow never grants the Gitleaks job permission to read pull-request metadata/commits. On a `pull_request` workflow, the default token is therefore insufficient for the API call that Gitleaks performs.

This is **not a secret leak result**. Gitleaks did not get far enough to evaluate the pull request contents through that API path.

### 4.3 Fix

Prefer a least-privilege, job-level permission block:

```yaml
jobs:
  secrets:
    permissions:
      contents: read
      pull-requests: read
```

Do not grant write permissions to this job.

### 4.4 Validation

After the change:

1. Push the workflow change.
2. Re-run the `Secrets` job.
3. Confirm the job can enumerate PR commits.
4. Confirm Gitleaks then proceeds to actual scanning.
5. Treat any subsequent leak finding as a separate security issue.

## 5. Root Cause #2 — Frontend Evidence Tests

### 5.1 Observed failure

The frontend job successfully completes dependency installation and `npm run lint`/TypeScript checking. The test suite reports:

- **409 passed**
- **3 failed**
- **412 total**

Failed tests:

1. `src/services/aiqaApi.test.ts`
   - `item 2c — aiqaApi.evidence.content fetches real raw bytes as a Blob at the exact real /content path`
   - Failure: `expected Blob { size: 14, type: 'text/plain' } to be an instance of Blob`
   - Assertion is at `aiqaApi.test.ts:303`.
2. `src/components/pages/EvidencePage.test.tsx`
   - `item 25/26 — a real text evidence preview loads and renders the real content`
   - Failure: expected text `real captured log output` is not rendered.
3. `src/components/pages/EvidencePage.test.tsx`
   - `item 28 — the current session's own key is redacted even if evidence content happened to contain it`
   - Failure: expected `/leaked header/` is not rendered.

### 5.2 Relevant implementation

`aiqaApi.evidence.content()` intentionally returns raw evidence bytes as a `Blob` using the browser response body. `EvidencePage` then converts the Blob to text with `FileReader` and applies the current-session API-key redaction before rendering.

The page implementation is explicit that `FileReader` was chosen because it works in this project's jsdom/test environment where newer Blob read methods are unavailable.

### 5.3 Root cause

The direct failure is the mismatch between the **Blob object produced by the test's `Response` implementation** and the **global `Blob` constructor used by the assertion**. In Vitest/jsdom-style environments, `Response.blob()` can return a Blob from a different realm/implementation. That object is structurally a Blob and has the right size/type, but `instanceof Blob` can be false when the constructors come from different realms.

The two EvidencePage failures are a downstream symptom: the content reader/preview path is not receiving a test Blob in the same representation the assertions/helper expect, so the asynchronous preview never reaches the expected text state.

### 5.4 Fix recommendation

Use one consistent Blob implementation in the frontend test environment.

Recommended approach:

- Mock `fetch` with a deterministic response shape whose `blob()` returns `new globalThis.Blob([...], { type })`.
- Avoid relying on a cross-realm `Response` object's `blob()` implementation for unit tests of the API client.
- Keep the production API implementation returning real response blobs.
- Keep the `FileReader`-based production preview path unchanged unless a browser compatibility requirement changes.
- Add one explicit test helper, e.g. `blobResponse(text, contentType)`, so all evidence tests use the same Blob realm.

Example test helper:

```ts
function blobResponse(text: string, contentType = 'text/plain'): Response {
  const blob = new globalThis.Blob([text], { type: contentType });
  return {
    ok: true,
    status: 200,
    headers: new Headers({ 'Content-Type': contentType }),
    blob: async () => blob,
  } as unknown as Response;
}
```

If the project wants a stronger contract test, the test can assert `blob.size`, `blob.type`, and decoded text instead of `instanceof Blob`; however, the preferred fix is to make the test transport use the same global Blob realm rather than hiding a real transport mismatch.

### 5.5 Why this is not a build failure

The frontend typecheck passes and there is no TypeScript/compiler error. The pipeline stops because `vitest run` exits non-zero after these three test failures.

### 5.6 Secondary frontend CI debt

The workflow uses `npm install` because the current PR was merged without refreshing `forentend/package-lock.json`. This is temporarily tolerated by the workflow, but it makes CI less deterministic.

Recommended cleanup after the lockfile is updated:

1. Regenerate and commit `forentend/package-lock.json`.
2. Change CI from `npm install` to `npm ci`.
3. Fail CI on lockfile drift.

The run also reports 6 dependency vulnerabilities (3 moderate, 2 high, 1 critical). They did not fail the current job, but they should be triaged separately rather than mixed into this root-cause fix.

## 6. Root Cause #3 — Backend Restart / Approval State

### 6.1 Observed failure

Backend quality gates pass:

- `cargo fmt -- --check`: passed
- `cargo clippy --all-targets`: passed
- `cargo test`: 319 passed, 1 failed, 2 ignored out of 322 tests

Failed test:

`api_tests::approval_survives_a_process_restart_and_can_still_be_approved`

The failed assertion is:

- actual: `PARTIAL`
- expected: `RUNNING`

The panic is at `src\\api_tests.rs:6639:9` in the GitHub Actions log.

### 6.2 What the engine intends to do

The current `approve_and_spawn()` implementation:

1. Durably approves the run.
2. Updates `approved_by`/`approved_at` in memory.
3. Calls `mark_run_running()` synchronously.
4. Persists `RUNNING`.
5. Emits `RunScheduled`.
6. Starts `spawn_run()` in the background.

The code comments explicitly state that the approval HTTP response should not race the scheduler and should be able to return the post-approval `RUNNING` state.

### 6.3 Why the restart case can return PARTIAL

The restart path is asymmetric with normal execution:

- `recover_incomplete_runs()` intentionally skips `PENDING_APPROVAL` runs so a human approval is not destroyed by restart.
- The in-memory execution engine is rebuilt from the stored run plan, not from a full persisted scheduler snapshot.
- `derive_run_state()` maps any persisted `BLOCKED`, `INFRA_ERROR`, or `TIMEOUT` task to `PARTIAL`.

Therefore a restarted approval flow can observe a run whose persisted task state and/or reconstructed scheduler state already imply a partial run, while the approval operation is intended to transition the run to `RUNNING` before background execution starts.

The key contract failure is that the run-level state after approval is not stable across the restart/re-hydration boundary: the test requires `RUNNING`, but the persisted/reconstructed state can be re-derived as `PARTIAL` before the asynchronous execution lifecycle establishes the expected running state.

### 6.4 Recommended fix

Treat `PENDING_APPROVAL -> RUNNING` as an authoritative run-level transition, not as a state that can immediately be overwritten by derived-task status during approval.

Concrete remediation:

1. In the re-hydration path, preserve the persisted run state exactly when the run is still `PENDING_APPROVAL`.
2. On approval, transactionally persist the approval and `RUNNING` transition before spawning the scheduler.
3. Re-hydrate the run after approval without replacing `RUNNING` with `derive_run_state()` unless the run is actually terminal.
4. Only compute `PARTIAL`, `FAILED`, `COMPLETED`, or `CANCELLED` after all relevant tasks are terminal or the run is explicitly being recovered/finalized.
5. Add a regression test that performs the exact sequence:
   - create gated run;
   - persist as `PENDING_APPROVAL`;
   - construct a fresh `AppState`/router from the same SQLite database;
   - GET the run and confirm it is still `PENDING_APPROVAL`;
   - approve it;
   - immediately GET the run and require `RUNNING`;
   - then poll until the simulated task finishes and assert the eventual terminal state.

### 6.5 Do not “fix” this by weakening the test

Changing the assertion from `RUNNING` to `PARTIAL` would hide the lifecycle defect unless the documented product contract explicitly says approval may immediately return a terminal/partial result. The current engine comments and related API tests state the opposite: approval is expected to make the run `RUNNING` before background execution.

## 7. CI/CD Workflow Findings

### 7.1 Only CI is present

The repository currently has one workflow under `.github/workflows`:

`ci.yml`

There is no dedicated deployment workflow for staging/production in the inspected revision. Therefore, strictly speaking, the current GitHub Actions problem is a **CI pipeline failure**, not a failed CD deployment.

### 7.2 Current CI structure

The workflow contains three functional jobs:

- `secrets` — Gitleaks
- `frontend` — Node install, typecheck/lint, Vitest, build
- `backend` — Rust fmt, clippy, tests

### 7.3 GitHub Actions Node 20 deprecation warning

The runner logs warn that Node 20 is deprecated and that Actions using Node 20 are being forced onto Node 24 by GitHub's runner environment.

This warning is not the cause of the current failure, but it is future CI reliability debt.

Recommended:

- Move action versions to releases that natively use the supported Node runtime.
- Prefer the latest stable major versions for `actions/checkout` and `actions/setup-node` after verifying compatibility.

### 7.4 Rust toolchain stability

The backend workflow asks for `stable` rather than pinning an explicit Rust toolchain. This is convenient, but can introduce future, unrelated CI breakage when a new compiler becomes stable.

Recommended:

- Commit a `rust-toolchain.toml` with the project-approved toolchain, or
- Pin the CI toolchain to a known version and update it intentionally.

## 8. Recommended Fix Order

### P0 — unblock CI

1. Add `pull-requests: read` to the Gitleaks job.
2. Fix the evidence test Blob realm/transport helper and re-run the 3 failed tests.
3. Fix the restart→approval state transition so approved restarted runs are reliably `RUNNING` before scheduler completion/finalization.

### P1 — make CI deterministic

4. Refresh and commit `forentend/package-lock.json`.
5. Change frontend CI from `npm install` to `npm ci`.
6. Pin the Rust toolchain.
7. Update GitHub Actions to remove Node 20 deprecation warnings.

### P1 — add actual CD

8. Add a separate deployment workflow with explicit environments, protected secrets, deployment approvals, health checks, and rollback strategy.
9. Make deployment depend on a clean CI gate rather than deploying directly from a failed PR run.

## 9. Validation Checklist

- [ ] `Secrets` job can read PR commit metadata and Gitleaks completes normally.
- [ ] Frontend `npm run lint` passes.
- [ ] Frontend evidence API tests pass, including the Blob check.
- [ ] Evidence text preview renders the actual response body.
- [ ] Evidence preview still redacts the current session API key.
- [ ] Backend `cargo fmt -- --check` passes.
- [ ] Backend `cargo clippy --all-targets` passes.
- [ ] Backend restart/approval regression test passes with `RUNNING` immediately after approval.
- [ ] Full backend test suite passes.
- [ ] `npm ci` succeeds from a committed lockfile.
- [ ] Frontend production build passes.
- [ ] A dedicated CD workflow exists before treating the repo as CI/CD complete.

## 10. References

- PR #10: https://github.com/Abhidark/AIQA/pull/10
- Failed workflow run: https://github.com/Abhidark/AIQA/actions/runs/32977972480
- CI workflow: https://github.com/Abhidark/AIQA/blob/0f20932d3f8fa3e1522a1d3765e14ce7fc8c8efd/.github/workflows/ci.yml
- Engine: https://github.com/Abhidark/AIQA/blob/0f20932d3f8fa3e1522a1d3765e14ce7fc8c8efd/services/orchestrator/src/engine.rs
- Frontend API client: https://github.com/Abhidark/AIQA/blob/0f20932d3f8fa3e1522a1d3765e14ce7fc8c8efd/forentend/src/services/aiqaApi.ts
- Frontend evidence page: https://github.com/Abhidark/AIQA/blob/0f20932d3f8fa3e1522a1d3765e14ce7fc8c8efd/forentend/src/components/pages/EvidencePage.tsx

## 11. Evidence Notes

The conclusions above are based on the actual GitHub Actions runner logs for run `32977972480`, the checked-in workflow configuration, and the source at PR head `0f20932d3f8fa3e1522a1d3765e14ce7fc8c8efd`. The analysis intentionally distinguishes confirmed CI failures from secondary maintenance warnings so the remediation order is clear.
