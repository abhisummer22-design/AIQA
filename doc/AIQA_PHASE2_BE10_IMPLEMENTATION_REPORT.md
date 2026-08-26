# AIQA — Phase 2 — BE-10 Implementation Report

**Document:** `AIQA_PHASE2_BE10_IMPLEMENTATION_REPORT.md`
**Date:** 2026-08-17
**Unit:** BE-10 — Remaining Core Backend Contract Work
**Scope:** Backend only. Zero frontend files opened or modified.

---

## 1. BE-10 Objective

Determine, from the source-of-truth documents rather than assumption, what "BE-10" actually names — then complete exactly that, and nothing beyond it.

---

## 2. Exact Scope From the Implementation Plan (Step 1)

Every literal occurrence of the string `BE-10` across all seven source documents was located (`grep -n "BE-10"` against all of them). There are exactly **two**, both in `AIQA_INTEGRATION_IMPLEMENTATION_PLAN_v1.0.md`, and both agree:

```
| BE-10 | Structured errors | §2.5 envelope + `request_id` on every error |
```
```
2. BE-8…BE-10 pass — projects, `/api/v1` scoping, structured errors (Phase 2 core), and
```

No other document — the Backend Implementation Plan, the Contract, the Acceptance Matrix, the Phase 1 verification report, or either prior BE report — names a milestone "BE-10" or assigns it different content.

| Requirement | Source Section | Current Status | Gap | BE-10 Required? |
|---|---|---|---|---|
| §2.5 envelope + `request_id` on every REST error | Integration Plan §15 (`BE-10` row), reaffirmed by the Go-condition text | Handler-level errors: **done** (this session's own prior unit, labeled "BE-9" due to a one-step numbering drift — see below) | Extractor-level rejections (malformed JSON, bad path/query) were explicitly disclosed as **not yet converted** in that unit's own "Remaining Issues" | **Yes — this is the actual open BE-10 work** |
| `/api/v1` scoping of the 16 remaining pre-existing routes | Integration Plan §13 item 10, §15 (labeled `BE-9` in the *document's own* table) | Not started (BE-8's own report explicitly deferred it) | Real, but see §3 — this is the *document's* BE-9, not BE-10 | **No — different milestone; see dependency note below** |

**On the numbering drift.** This session's own labels have run one step ahead of the Integration Plan's original table since the "BE-8"/"BE-9" units: the plan's own `BE-9` = "`/api/v1` + scoped routing" and `BE-10` = "Structured errors" — but this session completed "Structured errors" under the label **"BE-9,"** one turn ago (`AIQA_PHASE2_BE9_IMPLEMENTATION_REPORT.md`), and that report's own "Remaining Issues" (§18) already named the one thing it left undone:

> "Axum extractor-level rejections (malformed JSON body, invalid UUID in a path segment, unparseable query string) still return Axum's own default plain-text rejection, **not** the `ApiError` envelope... judged out of BE-9's scope... Disclosed rather than silently left uncovered."

Per the instruction not to assume BE-10 means "implement whatever is missing," and per the documents' own singular, unambiguous definition of BE-10 (§2.5 envelope + request_id **on every error**), the correct, narrow reading is: **BE-10 is the completion of that exact requirement** — closing the one gap the prior unit explicitly and honestly left open, not a new, undocumented scope. Moving the remaining 16 routes under `/api/v1` is real, necessary work, but it is the *document's* "BE-9," a different, larger, separately-gated milestone (Integration Plan's own risk register, R2: "the single largest item in this plan... touches every route, handler, test"), not "BE-10" by any source document's own naming. It is noted as a dependency in §23/§24 below, not implemented here.

---

## 3. Before-State Audit (Step 2)

Fresh, live confirmation (not assumed from the prior report) against the rebuilt binary, before any code was touched:

| Extractor | Trigger | Status (unchanged by this unit) | Body before |
|---|---|---|---|
| `Json<T>` | Malformed JSON syntax | 400 | `Failed to parse the request body as JSON: key must be a string at line 1 column 2` (plain text) |
| `Path<Uuid>` | Non-UUID path segment | 400 | `Invalid URL: UUID parsing failed: invalid character: found 'n' at 0` (plain text) |
| `Json<T>` | Missing/wrong `Content-Type` | 415 | `Expected request with 'Content-Type: application/json'` (plain text) |
| `Query<T>` | Value that doesn't parse as the declared type | 400 | `Failed to deserialize query string: invalid digit found in string` (plain text) |

All four confirmed `Content-Type: text/plain; charset=utf-8` — genuinely non-compliant with §2.5, exactly as the prior report disclosed.

---

## 4. Gap Matrix (Step 3 — Contract-First)

| Extractor path | HTTP method/route affected | Current status | Contract requirement | Comparison | Gap |
|---|---|---|---|---|---|
| `Json<T>` body parsing | Every `POST`/`PATCH` route (`/api/v1/projects`, `/runs`, `/bug-drafts/:id/review`, `/admin/api-keys`) | 400/415, plain text | §2.5 envelope | Contract gives no fine-grained status for this case — none was changed | Body shape only |
| `Path<Uuid>` parsing | Every route with a `:id` segment | 400, plain text | §2.5 envelope | Same | Body shape only |
| `Query<T>` parsing | `GET /runs`, `/runs/:id/events`, `/admin/audit-log` | 400, plain text | §2.5 envelope | Same | Body shape only |

No route, status code, or success schema is affected — confirmed empirically in §11.

**Ambiguity check (Step 3's explicit instruction):** the contract does not specify a distinct HTTP status or code for "malformed JSON" vs. "wrong Content-Type" vs. "bad path parameter" vs. "bad query value." Rather than inventing four new codes the contract never asked for, all four reuse the existing `VALIDATION_ERROR` code already established in the prior unit for "a request the caller could fix by sending different input" — the same category these genuinely are, just caught one layer earlier (by Axum's extractor, before a handler body runs, instead of inside handler logic). This is not an invented resolution to a genuine ambiguity; it is applying an existing, already-justified category to a case that fits it exactly.

---

## 5. Root Cause

BE-9's handler-level conversion could only change what a **handler function's own return value** produces. A malformed request is rejected by Axum's `Json`/`Path`/`Query` extractors *before* a handler body ever executes — those rejections were never inside BE-9's reach, and its own report said so explicitly rather than silently leaving the gap undocumented.

---

## 6. Implementation

**`services/orchestrator/src/errors.rs`** — three new generic wrapper types, each a thin drop-in replacement for the corresponding Axum extractor:

```rust
pub struct ApiJson<T>(pub T);   // wraps axum::extract::Json<T>
pub struct ApiPath<T>(pub T);   // wraps axum::extract::Path<T>
pub struct ApiQuery<T>(pub T);  // wraps axum::extract::Query<T>
```

Each implements `FromRequest`/`FromRequestParts` by delegating to the real Axum extractor; on success it passes the value through unchanged (verified in §8/§11 to be a true no-op for valid input); on failure it converts the rejection via a new `ApiError::from_rejection(status, message)` constructor, which **preserves Axum's own status code exactly** (per Step 5) and assigns `VALIDATION_ERROR` uniformly (per §4's ambiguity resolution).

`#[async_trait::async_trait]` is used for these impls because `axum-core` 0.4's `FromRequest`/`FromRequestParts` traits are themselves declared with `#[async_trait]` (confirmed by reading `axum-core`'s own source in the local cargo registry cache, not assumed) — native `async fn` trait syntax does not satisfy the same desugared bounds and produced a real compile error (`E0195`) until this was corrected.

**`services/orchestrator/src/api.rs`** — every handler's `Json<T>`, `Path<Uuid>`, `Query<T>` parameter was replaced with `ApiJson<T>`, `ApiPath<Uuid>`, `ApiQuery<T>`. `ws_handler` (the WebSocket handshake) was **explicitly excluded** and still uses plain `Path`/`Query` — G-WS remains untouched (§24). `reject_run`'s `Option<Json<RejectBody>>` → `Option<ApiJson<RejectBody>>`, with its one call site updated (`b.reason` → `b.0.reason`, since `ApiJson` does not implement `Deref` the way Axum's own `Json` does — a deliberate choice to keep the wrapper minimal rather than add ergonomics nothing else needs).

No handler's business logic, validation rule, or status-code choice changed.

---

## 7. Files Changed

| File | Change |
|---|---|
| `services/orchestrator/src/errors.rs` | 3 new wrapper types (`ApiJson`, `ApiPath`, `ApiQuery`) + `ApiError::from_rejection` constructor + module doc update |
| `services/orchestrator/src/api.rs` | Every handler's extractor parameter type swapped (mechanical); `ws_handler` deliberately left on plain Axum extractors; one call-site fix in `reject_run` |
| `services/orchestrator/src/api_tests.rs` | 5 new tests (§8) |

No route, no request/response schema, no migration, no frontend file.

---

## 8. Tests Added (Step 4 — Test-First)

All in `api_tests.rs`, driving real requests through the real router (same convention as BE-9):

- `be10_malformed_json_body_returns_structured_envelope`
- `be10_invalid_path_parameter_returns_structured_envelope`
- `be10_missing_content_type_returns_structured_envelope`
- `be10_invalid_query_parameter_returns_structured_envelope`
- `be10_valid_requests_through_converted_extractors_are_unaffected` — a regression guard proving the wrappers are transparent pass-throughs for *valid* input, not just error-path shims (creates a project via `ApiJson`, fetches it via `ApiPath`, lists via `ApiQuery`, all succeed exactly as before)

---

## 9. Before-Fix Results

Three representative handler signatures (`create_project`'s `ApiJson`, `get_project_handler`'s `ApiPath`, `list_runs`'s `ApiQuery`) were temporarily reverted to plain Axum extractors and the new suite re-run:

```
running 5 tests
test api_tests::be10_missing_content_type_returns_structured_envelope ... FAILED
test api_tests::be10_invalid_path_parameter_returns_structured_envelope ... FAILED
test api_tests::be10_invalid_query_parameter_returns_structured_envelope ... FAILED
test api_tests::be10_malformed_json_body_returns_structured_envelope ... FAILED
test api_tests::be10_valid_requests_through_converted_extractors_are_unaffected ... ok

test result: FAILED. 1 passed; 4 failed
```

Each panic shows the real, pre-fix plain-text body (e.g. `body=Invalid URL: UUID parsing failed: invalid character: found `n` at 0`), confirming the tests are genuinely discriminating, not tautological. The three signatures were then restored.

---

## 10. After-Fix Results

```
running 5 tests
test api_tests::be10_missing_content_type_returns_structured_envelope ... ok
test api_tests::be10_invalid_path_parameter_returns_structured_envelope ... ok
test api_tests::be10_malformed_json_body_returns_structured_envelope ... ok
test api_tests::be10_valid_requests_through_converted_extractors_are_unaffected ... ok
test api_tests::be10_invalid_query_parameter_returns_structured_envelope ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 50 filtered out
```

---

## 11. Real HTTP Verification

Rebuilt binary, real (preserved-and-restored) production database, real network requests:

| Check | Before | After |
|---|---|---|
| Malformed JSON body | `400`, `text/plain`, plain message | `400`, `application/json`, `{"error":{"code":"VALIDATION_ERROR","message":"Failed to parse the request body as JSON: key must be a string at line 1 column 2",...}}` |
| Invalid UUID path segment | `400`, `text/plain` | `400`, `application/json`, `VALIDATION_ERROR` envelope, same message text |
| Missing Content-Type | `415`, `text/plain` | `415`, `application/json`, `VALIDATION_ERROR` envelope |
| Bad query parameter | `400`, `text/plain` | `400`, `application/json`, `VALIDATION_ERROR` envelope |
| Valid create (regression) | — | `201`, plain project object, **no `error` key** — success path unaffected |

All four statuses are byte-identical to before this unit — only the body's `Content-Type` and shape changed, exactly as scoped. The extractor messages themselves (e.g. "JSON syntax error at line 1 column 2") describe the *caller's own* malformed input, not server internals — consistent with, and not a new exception to, Contract §2.5's prohibition on leaking secrets/stack traces/infrastructure details (that prohibition targets server-side failures, already handled by `ApiError::internal`'s sanitization in BE-9; a validation message describing what the client sent wrong is the intended, expected content of a `VALIDATION_ERROR`).

---

## 12. Database Verification

No schema change this unit. No migration. The live verification in §11 used the real, preserved-and-restored production database; state was confirmed unaffected before and after.

---

## 13. BE-8 Regression

```
be8_* (store-level): 4 passed
```
(Includes `be9_be8_regression_full_project_crud_lifecycle_through_router`, matched by the same substring filter — the full create→list→get→update lifecycle through the real router, now additionally exercising `ApiJson`/`ApiPath` on every leg, still passing.) Project create, list, get, update, validation, not-found, and project isolation all confirmed unaffected, both via the automated suite and the live checks in §11 (valid create still returns `201` with the unchanged schema).

---

## 14. BE-9 Regression

```
be9_*: 10 passed
```
Structured errors, `ApiError`, sanitized internal errors, `request_id`, `details`, and correct error codes all confirmed unaffected — the persistence/execution fault-injection tests (`be9_persistence_failure_returns_sanitized_internal_envelope`, `be9_execution_persistence_failure_returns_sanitized_internal_envelope`) still pass unmodified, proving BE-9's handler-level path and BE-10's new extractor-level path coexist without interference.

---

## 15. Phase 1 (D1–D5) Regression

```
D1 — 4 passed
D2 — 1 passed
D3 — 1 passed
D4 — 3 passed
D5 — 4 passed
```
All unchanged.

---

## 16. B2/B3/B4/B5/B6/B14 Regression

```
B2  — 3 passed
B3  — 1 passed
B4  — 1 passed
B5  — 1 passed
B6  — 2 passed
B14 — 2 passed
```
All unchanged.

---

## 17. Full Test Suite

```
$ cargo test
running 55 tests
... (40 pre-BE-9 + 10 be9_* + 5 be10_*)
test result: ok. 55 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.5x s
```
Re-run twice more (post-revert-restore, and once more standalone): identical 55/55.

---

## 18. Build Result

```
$ cargo check --all-targets
    Checking aiqa-orchestrator v0.1.0 (D:\AIQA\services\orchestrator)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.5x s
```
**0 errors, 0 warnings.**

(One real compile error occurred mid-implementation — `E0195`, native `async fn` trait-impl syntax not matching `axum-core`'s `#[async_trait]`-based trait declarations — resolved by inspecting `axum-core`'s actual source in the local registry cache rather than guessing; documented in §6, not a remaining issue.)

---

## 19. Contract Impact

§2.5's envelope now covers genuinely every REST error path except the WebSocket handshake (excluded, G-WS). `AIQA_API_CONTRACT_v1.0.md` was not modified — confirmed via `git status` (untracked, unchanged) throughout this unit.

---

## 20. Acceptance Matrix Impact

`AIQA_ACCEPTANCE_MATRIX_v1.0.md` was not modified — confirmed via `git status` (untracked, unchanged). No ACC item's recorded status was touched.

---

## 21. Frontend Impact

**None.** No file under `apps/web/` or `forentend/` was opened, read, or modified this session.

---

## 22. Repository Diff Audit (Step 12)

| File | Classification |
|---|---|
| `services/orchestrator/src/errors.rs` | BE-10 production change |
| `services/orchestrator/src/api.rs` | BE-10 production change (mechanical extractor-type swap only) |
| `services/orchestrator/src/api_tests.rs` | BE-10 test |
| `AIQA_PHASE2_BE10_IMPLEMENTATION_REPORT.md` | Documentation/report |

`auth.rs`, `lib.rs`, `Cargo.toml`, `Cargo.lock` show as modified in `git status` but carry **zero new changes from this session** — their diffs are entirely BE-9's, untouched here. No unrelated production change exists.

---

## 23. Remaining Issues

- **Moving the remaining 16 pre-existing routes under `/api/v1/projects/{project_id}/...`** — this is the Integration Plan's own `BE-9` (not `BE-10`), explicitly deferred in BE-8's report and reconfirmed out of scope here per §2's analysis. Real, necessary, but a separately-gated, much larger unit (Integration Plan's own risk register: "the single largest item in this plan").
- **Pagination/filtering (§3.3–3.4), status rename (§13, `BE-11`), Dashboard/KPI JSON (§23, `BE-12`), CORS restriction (`B12`)** — all real, named items later in the same Phase-2 plan, none of them "BE-10" by any source document, not attempted here per the instruction not to assume BE-10 means "whatever is missing."

---

## 24. Authentication Dependencies (Step 9)

None introduced or touched. `require_auth`, `require_role`, `require_project_access` are unmodified since BE-9. `G-AUTHN` (keys-vs-users) remains unresolved and was not approached — nothing in BE-10's actual scope (extractor-level error bodies) required it.

---

## 25. WebSocket Dependencies (Step 10)

`ws_handler` is unmodified — still authenticates via `?key=` and still returns plain-text pre-upgrade rejections, deliberately excluded from the `ApiJson`/`ApiPath`/`ApiQuery` conversion (its `Path`/`Query` extractors were left on the original Axum types). `G-WS` remains unresolved and was not approached.

---

## Final Status

# BE-10 — COMPLETE
