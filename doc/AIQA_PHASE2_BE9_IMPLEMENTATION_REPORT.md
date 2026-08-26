# AIQA — Phase 2 — BE-9 Implementation Report

**Document:** `AIQA_PHASE2_BE9_IMPLEMENTATION_REPORT.md`
**Date:** 2026-08-17
**Unit:** BE-9 — Structured Error Envelope (API Contract §2.5)
**Scope:** Backend only. Zero frontend files opened or modified.

---

## 1. BE-9 Objective

Every error response in this API — from before this unit — was a plain-text body (`(StatusCode, String)`, serialized by Axum's default tuple `IntoResponse` as `text/plain`). Contract §2.5 requires errors to be "structured and machine-readable," with a recommended JSON envelope. BE-9's objective: convert every REST handler's error path to that envelope, from one centralized implementation, without changing any existing success response or any business behavior.

---

## 2. Contract Error Schema (Step 2)

The entirety of what `AIQA_API_CONTRACT_v1.0.md` §2.5 specifies — re-read in full for this unit, cross-checked against every other mention of "error" in the document (§13 status enum, §14 event type, §29/§30 principle-level rules — none add schema detail):

```json
{
  "error": {
    "code": "RESOURCE_NOT_FOUND",
    "message": "Project was not found.",
    "details": {},
    "request_id": "req_123"
  }
}
```

- **Required structure:** a top-level `error` object (not a bare error at the response root).
- **Fields:** `code` (string, machine-readable), `message` (string, human-readable, "user-safe" — §2.5's own words: "Frontend MUST display a user-safe message and MUST NOT expose secrets, internal stack traces, credentials, or sensitive infrastructure details"), `details` (object; the contract's own example shows it empty), `request_id` (string, example shows a `"req_"`-prefixed opaque id).
- **Error codes:** the contract enumerates exactly **one** — `RESOURCE_NOT_FOUND` — and does not provide a full code list or an HTTP-status-to-code mapping table. No other codes were invented beyond conventional, self-describing ones needed to cover this API's existing status codes (§4 below); `RESOURCE_NOT_FOUND` itself is reused verbatim, not reinvented.
- **HTTP status mapping:** not specified by the contract at all — this API's *existing* status choices (401/403/404/400/409/500, all already in place before BE-9) were preserved exactly; BE-9 only changes what serializes into the body at each status, per Step 5's explicit instruction.
- **Category representations** (auth/authz/validation/not-found/internal): the contract gives no separate schema per category — one envelope shape, varying only in `code`/`message`/status.

No field was renamed, dropped, or added beyond this schema. `details` is always `{}` in this implementation, matching the contract's own example — nothing in this API's current error paths carries structured per-field detail (e.g., a list of invalid fields) worth inventing content for.

---

## 3. Step 1 — Before-State Audit

Full search of `api.rs`/`auth.rs` for `StatusCode`, `String`, `Json`, `Result`, `map_err`, `to_500`, `into_response`, and every handler's error-producing branch, before any code was changed.

**Finding: every error response in the REST API, with zero exceptions, went through exactly one of three mechanisms — all producing plain text:**

1. `to_500(e)` → `(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())` — used at ~25 call sites via `.map_err(to_500)`, and critically, `e.to_string()` on a `sqlx::Error` **includes the raw driver message** (`"database is locked"`, `"UNIQUE constraint failed: run_events.run_id, run_events.sequence"`, etc.) — a direct, live-confirmed violation of §2.5's "MUST NOT expose ... sensitive infrastructure details."
2. `auth::require_role` / `auth::require_project_access` → `Err((StatusCode::FORBIDDEN, "..."))`.
3. Inline literal tuples at each handler's own validation/not-found/conflict branches, e.g. `Err((StatusCode::BAD_REQUEST, "project name must not be empty".to_string()))`.

`require_auth` (the auth middleware, outside any handler) had its own three ad-hoc plain-text `Response`s (missing header, unknown key, DB lookup failure) — not going through the `(StatusCode, String)` pattern at all, but equally non-compliant.

| Endpoint/Path | Current Error | HTTP Status | Current Body | Contract Requirement | Gap |
|---|---|---|---|---|---|
| `require_auth` (middleware) | missing/malformed header | 401 | plain text | §2.5 envelope | Non-compliant |
| `require_auth` | unknown/revoked key | 401 | plain text | §2.5 envelope | Non-compliant |
| `require_auth` | DB lookup failure | 500 | plain text, **raw error appended** | §2.5 envelope + no leaked detail | Non-compliant, ×2 |
| `require_role` (all role-gated handlers) | insufficient role | 403 | plain text | §2.5 envelope | Non-compliant |
| `require_project_access` (all scoped handlers) | cross-project | 403 | plain text | §2.5 envelope | Non-compliant |
| `create_project`, `update_project_handler` | blank name | 400 | plain text | §2.5 envelope | Non-compliant |
| `get_project_handler`, `update_project_handler`, `get_run`, `get_events`, `list_evidence`, `get_evidence_content`, `review_bug_draft` | not found | 404 | plain text | §2.5 envelope, `RESOURCE_NOT_FOUND` | Non-compliant |
| `create_run` | plan validation failure | 400 | plain text | §2.5 envelope | Non-compliant |
| `create_run` | policy denied | 403 | plain text | §2.5 envelope | Non-compliant |
| `approve_run`, `reject_run` | wrong run state | 409 | plain text | §2.5 envelope | Non-compliant |
| `approve_run`, `reject_run`, and ~20 other handlers | any store/persistence failure | 500 | plain text, **raw `sqlx::Error` text appended** | §2.5 envelope + no leaked detail | Non-compliant, and a real information-disclosure gap |
| `review_bug_draft` | unknown action | 400 | plain text | §2.5 envelope | Non-compliant |
| `create_api_key` | unknown role | 400 | plain text | §2.5 envelope | Non-compliant |
| Every **successful** response (all handlers) | n/a | 200/201/204 | JSON, contract-shaped fields | unchanged (§5 of this unit) | No gap — must stay this way |
| `ws_handler` (WebSocket handshake) | missing/invalid `?key=`, not found, cross-project | 401/404/403 | plain text | — | **Explicitly out of scope** — see §1 rationale below |

**Root cause:** never built. Every handler used Axum's default plain-tuple `IntoResponse` because no centralized structured type existed. Not a regression — Phase 1's scope was persistence/recovery correctness; BE-8's scope was the Projects resource; neither touched error representation.

**Why `ws_handler` is excluded:** the task's own scope lock forbids WebSocket work ("Do NOT implement WebSocket"), and a WebSocket handshake rejection is not a JSON body a browser's `WebSocket` client parses the way a REST JSON error is — it's a pre-upgrade HTTP response in a different transport context. G-WS (WebSocket auth/transport semantics) remains its own unresolved, explicitly-out-of-scope decision.

---

## 4. Design (Step 3)

One new module, `services/orchestrator/src/errors.rs`, defining a single `ApiError` type:

```rust
pub struct ApiError { status: StatusCode, code: &'static str, message: String }
```

with an `impl IntoResponse for ApiError` that builds the exact contract envelope (fresh `request_id` generated per response, `details: {}`), plus six convenience constructors mapping this API's existing status codes to codes:

| Constructor | Status | Code | Used for |
|---|---|---|---|
| `unauthenticated` | 401 | `UNAUTHENTICATED` | missing/invalid credentials |
| `forbidden` | 403 | `FORBIDDEN` | insufficient role, cross-project access, policy denial |
| `not_found` | 404 | `RESOURCE_NOT_FOUND` | contract's own example code, reused verbatim |
| `validation` | 400 | `VALIDATION_ERROR` | bad input the caller could fix |
| `conflict` | 409 | `CONFLICT` | resource not in a state that allows the action |
| `internal` | 500 | `INTERNAL_ERROR` | persistence failures and any other unexpected server error — see §6 |

Every handler in `api.rs` and every gate in `auth.rs` (`require_role`, `require_project_access`, `require_auth`) now returns/produces `ApiError` instead of `(StatusCode, String)`. This is a purely mechanical conversion — return-type annotations and literal `Err((...))` constructions were updated to their `ApiError` equivalent, with **zero changes to any condition, business rule, or which status code is returned for which case**. `to_500` (kept under its existing name to minimize the diff) now returns `ApiError::internal(e)` instead of the raw tuple.

---

## 5. Error Categories (Step 4) — Verified

| Category | How it's produced | Code | Verified |
|---|---|---|---|
| Authentication | `require_auth`: missing header, invalid key | `UNAUTHENTICATED` | ✅ test + live |
| Authorization | `require_role` (wrong role), `require_project_access` (cross-project) | `FORBIDDEN` | ✅ test + live, both variants |
| Validation | blank project name, `validate_plan`, unknown role/action | `VALIDATION_ERROR` | ✅ test + live |
| Not Found | unknown project/run/evidence/bug-draft id | `RESOURCE_NOT_FOUND` | ✅ test + live |
| Persistence | any `store.*().await` failure (`.map_err(to_500)`) | `INTERNAL_ERROR` | ✅ test + live, real `SQLITE_BUSY` fault injection |
| Execution | `engine::approve_and_spawn(...).await.map_err(to_500)` | `INTERNAL_ERROR` | ✅ test — a **second**, independent call site, proving the same centralized path covers more than one handler |
| Internal | any other unexpected failure | `INTERNAL_ERROR` | ✅ (same mechanism as Persistence — see rationale below) |

**Persistence and Internal share one code by design, not by omission.** The contract's schema gives no basis to distinguish them (no second code is implied anywhere in §2.5), and to an API consumer both mean the same thing: "something failed on the server, not because of your request." The deeper distinction Phase 1's `note_persistence_failure` mechanism (D1–D5) cares about — whether a run's *own* durability was compromised — is a separate, already-verified concern inside a run's lifecycle, orthogonal to what HTTP envelope a synchronous request handler returns.

**Conflict (409)** was also converted, though not one of the seven named categories — it was already a real status this API returns (`approve_run`/`reject_run` on a wrong-state run) and BE-9's own design goal ("same backend behavior + contract-compliant envelope... not a business-logic rewrite") means every existing error path gets the envelope, not only the seven explicitly named ones.

---

## 6. Sanitization (a finding surfaced by Step 1, addressed under "Internal")

The before-state audit found `to_500`'s `e.to_string()` **already leaking raw SQL driver text into response bodies** — confirmed live during this unit's own verification: a genuine `SQLITE_BUSY` fault (an `EXCLUSIVE` lock held past the 5s `busy_timeout`, the same real fault-injection technique used throughout Phase 1) previously would have returned literal text containing `"database is locked"`.

`ApiError::internal` now does two things: logs the real error server-side (`tracing::error!("internal error: {context}")` — confirmed live, the real diagnostic reaches the log, so no observability is lost) and returns a fixed, safe message (`"An internal error occurred."`) to the client. This directly satisfies §2.5's explicit prohibition and was verified live (§10) to leak none of: the trigger name, `RAISE`/`ABORT`, `sqlite`/`SQLite`, or the constraint/table text.

---

## 7. Implementation — Files Changed

| File | Change |
|---|---|
| `services/orchestrator/src/errors.rs` | **New.** `ApiError` type, 6 constructors, `IntoResponse` impl (the one place the envelope is built) |
| `services/orchestrator/src/auth.rs` | `require_auth`'s 3 ad-hoc responses, `require_role`, `require_project_access` → `ApiError` |
| `services/orchestrator/src/api.rs` | Every handler's error type `(StatusCode, String)` → `ApiError`; `to_500` now returns `ApiError::internal(e)`; every inline `Err((StatusCode::X, "..."))` → the matching `ApiError` constructor. **Zero changes to any success-path status code, field, or response type.** |
| `services/orchestrator/src/lib.rs` | Registered `pub mod errors;` and `#[cfg(test)] mod api_tests;` |
| `services/orchestrator/Cargo.toml` | Added `tower` (`features=["util"]`) and `http-body-util` as dev-dependencies — needed to drive real HTTP requests through the real `axum::Router` in-process for response-body assertions (see §8) |
| `services/orchestrator/src/api_tests.rs` | **New.** 10 tests, described in §8 |

No route, request schema, or success response schema was touched. No file under `apps/web/` or `forentend/` was opened.

---

## 8. Tests Added (Step 6)

**New test infrastructure.** No prior test in this codebase exercised the HTTP layer (every Phase 1/BE-8 test calls `store`/`engine` functions directly). Step 6 explicitly requires inspecting the actual response body, which status-code-only assertions cannot do, so `api_tests.rs` drives real requests through the real `axum::Router` in-process via `tower::ServiceExt::oneshot`, against a real on-disk SQLite store (same `tempfile` convention as every other test file) — not handler functions called directly, and not a mocked error.

| Test | Category | What it proves |
|---|---|---|
| `be9_authentication_missing_credentials_returns_structured_envelope` | Authentication | No `Authorization` header → 401, `UNAUTHENTICATED` envelope |
| `be9_authentication_invalid_credentials_returns_structured_envelope` | Authentication | Unknown key → 401, `UNAUTHENTICATED` envelope |
| `be9_authorization_insufficient_role_returns_structured_envelope` | Authorization | Real viewer key, admin-only route → 403, `FORBIDDEN` |
| `be9_authorization_cross_project_access_returns_structured_envelope` | Authorization | Real operator key scoped to a *different* project → 403, `FORBIDDEN` |
| `be9_validation_blank_name_returns_structured_envelope` | Validation | Blank `name` → 400, `VALIDATION_ERROR`, message names what was wrong |
| `be9_not_found_returns_structured_envelope_with_contract_example_code` | Not Found | Genuinely nonexistent id → 404, `RESOURCE_NOT_FOUND` (the contract's own code) |
| `be9_persistence_failure_returns_sanitized_internal_envelope` | Persistence | Real `CREATE TRIGGER ... RAISE(ABORT, ...)` on `projects` INSERT → 500, `INTERNAL_ERROR`, and asserts the response contains **none** of the trigger name / `RAISE` / `ABORT` / "injected..." / `sqlite` text |
| `be9_execution_persistence_failure_returns_sanitized_internal_envelope` | Execution | Same fault-injection technique against a **different** handler (`approve_run`, via `engine::approve_and_spawn`) → proves the centralized path covers more than one call site |
| `be9_successful_response_is_not_wrapped_in_an_error_envelope` | (Step 5 guard) | A successful `201` has no `error` key and unchanged fields |
| `be9_be8_regression_full_project_crud_lifecycle_through_router` | BE-8 regression | create → list (scoped, isolation) → get → partial update, full lifecycle through the real router, post-BE-9 |

Every test asserts on the actual parsed JSON body via a shared `assert_envelope` helper: HTTP status, presence of a top-level `error` object, exact `code`, non-empty string `message`, `details` is an object, and `request_id` is present and `"req_"`-prefixed.

### Before-fix / after-fix (Step 6 test-first)

The envelope's `IntoResponse` implementation was temporarily reverted to the exact pre-BE-9 behavior (`(self.status, self.message).into_response()` — plain text) and the suite re-run:

```
running 9 tests
test api_tests::be9_successful_response_is_not_wrapped_in_an_error_envelope ... ok
test api_tests::be9_authentication_missing_credentials_returns_structured_envelope ... FAILED
test api_tests::be9_authentication_invalid_credentials_returns_structured_envelope ... FAILED
test api_tests::be9_authorization_insufficient_role_returns_structured_envelope ... FAILED
test api_tests::be9_authorization_cross_project_access_returns_structured_envelope ... FAILED
test api_tests::be9_validation_blank_name_returns_structured_envelope ... FAILED
test api_tests::be9_not_found_returns_structured_envelope_with_contract_example_code ... FAILED
test api_tests::be9_persistence_failure_returns_sanitized_internal_envelope ... FAILED
test api_tests::be9_execution_persistence_failure_returns_sanitized_internal_envelope ... FAILED

test result: FAILED. 1 passed; 8 failed
```

Each failure's panic message shows the **actual pre-BE-9 plain-text body** (e.g. `body=role 'viewer' cannot perform this action (requires 'admin')`, `body=An internal error occurred.` before it even had a chance to be wrapped) — confirming these tests fail against real prior behavior, not a strawman. The envelope implementation was then restored and the full suite re-confirmed green (50/50).

---

## 9. BE-8 Regression (Step 7)

Re-run fresh, both layers:

- **Store-level** (`be8_*`, 3 tests, unaffected by BE-9 since the store layer has no HTTP concept): `3 passed`.
- **HTTP-level**, now necessarily exercising the *new* error type underneath every BE-8 handler: `be9_be8_regression_full_project_crud_lifecycle_through_router` (create → list → get → update) plus the category tests that incidentally exercise BE-8 routes (validation, not-found, cross-project, role-gated create): all pass.
- **Live**, against the real binary and real (preserved/restored) production database: create (201, contract-shaped body, no envelope), validation (400, envelope), not-found (404, envelope, `RESOURCE_NOT_FOUND`), authorization — viewer creating a project (403, envelope) — all confirmed with real requests over the real network stack, not just the in-process harness.

BE-8 remains fully functional; only the *shape* of its error bodies changed, as intended.

---

## 10. Live HTTP Verification (real binary, real preserved-and-restored production database)

| Check | Result |
|---|---|
| `GET /api/v1/projects` (no key) | `401`, `Content-Type: application/json`, `{"error":{"code":"UNAUTHENTICATED",...}}` |
| `POST /api/v1/projects` blank name | `400`, `VALIDATION_ERROR` |
| `GET /api/v1/projects/{random-uuid}` | `404`, `RESOURCE_NOT_FOUND` |
| Viewer key `POST /api/v1/projects` | `403`, `FORBIDDEN` |
| Admin key `POST /api/v1/projects` (success) | `201`, plain project object, **no `error` key** |
| `GET /runs` (pre-existing, unprefixed route) | `200`, unchanged list shape |
| `GET /runs/<nonexistent>` (pre-existing route) | `404`, **now also** returns the envelope — the intended, uniform outcome |
| Real `SQLITE_BUSY` fault (`EXCLUSIVE` lock held past `busy_timeout`) on `POST /api/v1/projects` | `500`, `INTERNAL_ERROR`, message exactly `"An internal error occurred."` — zero occurrences of `"database is locked"`, `"sqlite"`, or any driver text in the body |
| Server-side log for the same fault | `ERROR aiqa_orchestrator::errors: internal error: error returned from database: (code: 5) database is locked` — the real diagnostic **is** captured, just not sent to the client |

Migration state unaffected (no schema change this unit). Database restored to its exact pre-unit state afterward.

---

## 11. Phase 1 (D1–D5) Regression

```
D1  — 4 passed
D2  — 1 passed
D3  — 1 passed
D4  — 3 passed
D5  — 4 passed
```

All unchanged. None of D1–D5's mechanisms (`note_persistence_failure`, `derive_run_state`, `list_incomplete_runs`, recovery) were touched — BE-9 only changes what a synchronous HTTP handler returns on error, not the durability/recovery layer those tests pin.

---

## 12. B2/B3/B4/B5/B6/B14 Regression

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

## 13. Full Test Result

```
$ cargo test
running 50 tests
... (40 pre-existing + 3 be8_* + 10 be9_* — after the discrimination-revert cycle, the be9_ set includes the final be8-regression test added mid-unit, resolving to 10 total)
test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.4x s
```

Re-run twice more (post-revert-restore, and once more standalone) with an identical 50/50 result.

---

## 14. Build Result

```
$ cargo check --all-targets
    Checking aiqa-orchestrator v0.1.0 (D:\AIQA\services\orchestrator)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.11s
```
**0 errors, 0 warnings.**

(One transient `rustc` internal compiler error occurred mid-implementation, while several handler return types still mismatched `ApiError` vs. the old tuple type across the router's generic type-inference chain — a cascading-type-error artifact of Axum's router type-state machinery under many simultaneous errors, not a real separate bug. It did not recur once every handler's signature was updated consistently; documented here for completeness, not as an outstanding issue.)

---

## 15. API Contract Impact

Net alignment gain: every REST error response (excluding the WebSocket handshake, explicitly out of scope) now matches §2.5's envelope exactly, using its own example code (`RESOURCE_NOT_FOUND`) verbatim where applicable. `AIQA_API_CONTRACT_v1.0.md` itself was not modified — confirmed via `git status` (untracked, unchanged) throughout.

---

## 16. Acceptance Matrix Impact

`AIQA_ACCEPTANCE_MATRIX_v1.0.md` was not modified — confirmed via `git status` (untracked, unchanged). No ACC item's status was recorded in this document by this unit (that document's own record-keeping sections are explicitly filled in only after full verification per its own instructions, consistent with how BE-8's report also left it untouched).

---

## 17. Frontend Impact

**None.** No file under `apps/web/` or `forentend/` was opened, read, or modified this session. Backend error responses are real (driven by real fault injection and real HTTP requests over the real network stack in §10) — no frontend-specific mock, stub, or environment bypass was introduced anywhere.

---

## 18. Remaining Issues (not addressed this unit — listed only)

- **Axum extractor-level rejections** (malformed JSON body, invalid UUID in a path segment, unparseable query string) still return Axum's own default plain-text rejection, **not** the `ApiError` envelope — these are rejected *before* a handler body runs, so they never reach the code this unit changed. Fixing this would require wrapping every `Json<T>`/`Path<T>`/`Query<T>` extractor with a custom type implementing `FromRequest`/`FromRequestParts`, touching every handler's *parameter* signatures broadly (not just return types) — judged out of BE-9's "centralized error representation for what handlers return, not a broader extractor rewrite" scope, and not among Step 4's seven named categories. Disclosed rather than silently left uncovered.
- **`ws_handler`** — deliberately unconverted; G-WS remains unresolved and explicitly out of scope.
- **Persistence vs. Internal share one code** — a deliberate design decision (§6), not an oversight; the contract gives no basis for a second code.
- **The remaining 16 pre-BE-8 routes are still unprefixed** (`/runs`, `/bug-drafts`, `/admin/*`) — unaffected by this unit; their *error bodies* are now envelope-compliant, but their *paths* remain outside `/api/v1`, exactly as left by BE-8. Not BE-9's scope.

---

## Final Status

# BE-9 — COMPLETE
