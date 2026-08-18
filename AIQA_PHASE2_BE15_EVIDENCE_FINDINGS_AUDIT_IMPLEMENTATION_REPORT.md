# AIQA — Phase 2 — BE-15 Implementation Report

**Unit:** BE-15 — Evidence/Findings/Audit on contract paths (Integration Plan milestone table: `BE-15 | Evidence/findings/audit on contract paths | §15, §16, §25`)
**Mode:** Implementation sprint (no large independent verification pass, per instruction)
**Status: COMPLETE**

## 1. BE-15 entry located (Step 1)

- Integration Plan milestone table: `BE-15 | Evidence/findings/audit on contract paths | §15, §16, §25`.
- Gap matrix rows (pre-BE-15 state): `15 Evidence — PARTIAL — Works; wrong path/scope`; `16 Findings/RCA — PARTIAL — Concept present, different names; no POST .../analysis`; `25 Audit — PARTIAL — Works; admin-only, unscoped, wrong path`.
- Contract sections read in full: §15 (Evidence APIs), §16 (Findings / Failure Intelligence APIs), §25 (Audit APIs).
- Acceptance Matrix sections read in full: §14/ACC-009 (Evidence), §15/ACC-010 (Failure/RCA Intelligence), §25/ACC-020 (Audit/Governance).
- Document terminology preserved: "Finding" (§16) and "Audit" (§25) are used in route/handler naming; the underlying real entity (`bug_drafts`) keeps its existing Rust/DB name — see §3.

## 2. Scope actually needed (Step 2 finding)

§15.1 (`GET .../executions/{run_id}/evidence`) was **already done** by "Actual BE-9" — exact path match, unchanged. §15.2's *path* was also already right (`GET .../evidence/{evidence_id}`), but its *response shape* was not: it served raw bytes, not the "Detail" (metadata) shape the contract's own text implies (see §4). §16 and §25 had no project-scoped contract-path routes at all — real, working analogues existed only at legacy/global paths (`/bug-drafts`, `/admin/audit-log`). BE-15's actual work was therefore: fix §15.2's response shape, add §16.1–16.3, add §25.1 — not touch §15.1 at all.

## 3. "Finding" — document terminology, real backend entity

Contract §16 describes a "Finding" as something with a review lifecycle where "AI hypotheses MUST remain clearly distinguishable from confirmed facts" and uncertain items "use `NEEDS_REVIEW` rather than being automatically classified as confirmed defects." This backend's `bug_drafts` table (joined with `failure_clusters` for its root-cause hypothesis/occurrence data) is exactly this: every draft starts `NEEDS_REVIEW`, is created only by the deterministic, non-LLM `intelligence::classify_failure` (confidence capped ≤0.6), and only becomes `CONFIRMED`/`DISMISSED` via an explicit human call (`review_bug_draft`). This is real, pre-existing, already-tested (B14) functionality — reused verbatim under the contract's route names, not reinvented.

## 4. §15.2 Evidence Detail — response-shape fix (not just a path check)

**Before:** `GET /api/v1/projects/{project_id}/evidence/{evidence_id}` served raw binary bytes + `Content-Type` header (identical to the legacy `/evidence/:id/content`) — a real path match that a prior session's own comment called "exact path match," without checking response shape.

**Contract text says otherwise:** §16.2 [sic, §15.2] is named "Evidence Detail" (paralleling other "X Detail" endpoints in the same contract, e.g. §8.4, which return metadata) and separately states "Large binary artifacts SHOULD be accessed through secure artifact references rather than embedded in normal API responses."

**Fix:** the bare path now returns `EvidenceRow` JSON metadata (the same shape §15.1's list already returns per item — zero new fields). The raw bytes were **not removed** — moved to a new sibling `GET .../evidence/{evidence_id}/content`, same handler body, same behavior, same isolation checks. One pre-existing test (`actual_be9_cross_project_isolation_on_evidence_by_id`) asserted raw bytes at the bare path; updated to check metadata there and bytes at `/content` — explicitly because this is the intentional, documented BE-15 shape change, not a hidden regression.

## 5. Endpoints added

| Endpoint | Method | Contract § | Notes |
|---|---|---|---|
| `.../evidence/{evidence_id}` | GET | 15.2 | **Changed**: now metadata (`EvidenceRow`), not bytes |
| `.../evidence/{evidence_id}/content` | GET | — | New; raw bytes, moved from the path above |
| `.../findings` | GET | 16.1 | New; real `bug_drafts`, project-filtered |
| `.../findings/{finding_id}` | GET | 16.2 | New |
| `.../findings/{finding_id}/analysis` | POST | 16.3 | New; see §6 |
| `.../findings/{finding_id}/review` | POST | — | New; not a contract endpoint, real pre-existing action (§7) |
| `.../audit` | GET | 25.1 | New; project-filtered, reuses `AuditRow` |

All under `/api/v1/projects/{project_id}/...`, all behind `require_scoped_project` (project exists + caller access), matching every existing scoped route.

## 6. §16.3 Failure Analysis — what "may correlate" resolved to

Contract: "Failure analysis may correlate: execution evidence, environment data, historical failures, logs." This backend's only real, directly-linked correlation for a finding is its **failure cluster** (`root_cause_hypothesis`, `signature`, `occurrence_count`, `reproducibility`, `first_seen_at`/`last_seen_at` — all real, already-computed by `intelligence::classify_failure`/`upsert_failure_cluster`, never fabricated at request time). The response is `{finding: BugDraftRow, failure_cluster: FailureClusterRow}` — two already-real, already-existing DTOs composed, zero new field names invented.

**Not included, and why:** "environment data" and "logs" (contract's other listed correlation dimensions) have no linkage anywhere in this schema from a `bug_draft`/`failure_cluster` to a specific evidence row, environment record, or log — `failure_clusters` is keyed by `(project_id, signature)`, not by a specific run/task/evidence id, so there is no honest way to resolve "the evidence for this finding" without inventing a linkage the schema doesn't have. Per the no-fake-data rule, this is omitted rather than guessed. `POST` (not `GET`) is used because the contract specifies it explicitly, even though this implementation has no side effect on the data itself; the endpoint is audited (`finding.analysis`) as a real, minimal side effect, consistent with this codebase's existing "audit anything notable" convention.

## 7. `.../findings/{finding_id}/review` — not a contract endpoint, kept

§16 defines no review/promotion action. The real, necessary, pre-existing capability (`review_bug_draft`, legacy `POST /bug-drafts/:id/review`) has nowhere else to go under the new scheme, so it was mounted as a sub-resource of the contract-defined `findings/{finding_id}`, exactly the precedent Actual BE-9 already set for `/approve`/`/reject`/`/events` under `executions/{run_id}` (Contract §2.1: "non-breaking additions may remain within the current version"). It reuses `review_bug_draft`'s exact confirm/dismiss logic — not a second copy — via a shared `require_finding_in_project` ownership check.

## 8. §25.1 Audit — scoped route + a resolved role-bar decision

`list_audit` gained a `project_filter: Option<&str>` parameter (`WHERE project_id = ?`, no "default" sentinel — unlike `quality_nodes`, a `NULL` audit row genuinely means "no project context," e.g. an admin key action, and correctly never matches a real project's scoped view). The legacy, unscoped `list_audit(None, limit)` call is unchanged.

**Role bar, resolved:** the legacy `/admin/audit-log` requires `Role::Admin` (global, cross-tenant visibility). The new `.../projects/{id}/audit` requires only `Role::Operator` + project access — the same bar every other scoped resource in *your own* project uses (create/cancel a run, review a finding). The contract specifies no per-endpoint role for §25.1, so this is a documented, non-blocking scope decision, not an invented rule: seeing your own project's audit trail is not a cross-tenant action, so gating it behind global Admin would be stricter than every other project-scoped resource for no stated reason.

## 9. Authorization

Every new route reuses existing helpers, none new: `require_scoped_project` (project existence + caller access, Actual BE-9's own helper) for every route; a new tiny `require_finding_in_project` (mirrors `require_resource_in_project`, resolving ownership through `bug_draft_project` since findings carry no `project_id` column of their own — same pattern evidence/tasks already use, resolving through their owning run). No second authorization system.

## 10. Data / migrations

**None.** Every new endpoint reads/writes tables that already exist (`evidence`, `bug_drafts`, `failure_clusters`, `audit_log`). Four small store methods were added (`get_evidence_by_id`, `get_bug_draft_by_id`, `get_failure_cluster_by_id`, and `list_audit` gained a filter parameter) — all straightforward `SELECT ... WHERE id = ?` reads or an added `WHERE` clause, no schema change.

## 11. Realtime

Not required by §15/§16/§25, and none was added. No new event bus, no synthetic events. (BE-14's real event stream is untouched.)

## 12. Files changed

- `services/orchestrator/src/store.rs` — `list_audit` gained `project_filter`; added `get_evidence_by_id`, `get_bug_draft_by_id`, `get_failure_cluster_by_id`.
- `services/orchestrator/src/api.rs` — evidence route split (`get_evidence_detail_scoped` new at the bare path, `get_evidence_content_scoped` moved to `/content`); new `list_findings_scoped`, `get_finding_detail_scoped`, `get_finding_analysis_scoped` (+`FindingAnalysisResponse`), `review_finding_scoped`, `get_audit_log_scoped`, `require_finding_in_project`; 7 new routes mounted; both pre-existing `list_audit(...)` call sites updated for the new parameter.
- `services/orchestrator/src/api_tests.rs` — 8 new `be15_*` tests; 1 pre-existing test (`actual_be9_cross_project_isolation_on_evidence_by_id`) updated to match the intentional §15.2 shape change (metadata at the bare path, bytes at `/content`), and 1 audit test call site updated for the new `list_audit` parameter.

## 13. Migrations added

None.

## 14. Tests added (8)

1. `be15_findings_list_and_detail_reflect_real_recurring_failures` — real data (two real failing runs → real recurring-failure clustering → real draft), `NEEDS_REVIEW` default.
2. `be15_finding_analysis_correlates_real_failure_cluster_and_is_audited` — real cluster data in the response, real audit entry.
3. `be15_finding_review_scoped_confirms_and_dismisses` — both transitions, persisted state, audited, invalid action rejected as `VALIDATION_ERROR`.
4. `be15_findings_cross_project_isolation_and_role_check` — list/detail/analysis/review all blocked cross-project (403); viewer role blocked on review (403); finding provably untouched by every rejected attempt.
5. `be15_findings_require_authentication_and_reject_nonexistent_resources` — 401 / nonexistent project 404 / nonexistent finding 404.
6. `be15_evidence_detail_is_metadata_not_raw_bytes` — asserts the §15.2 shape directly (real `size_bytes`/`content_type`/`object_key`, not a byte string).
7. `be15_audit_log_scoped_to_project_and_isolated` — real audited actions in two projects, each project's view contains only its own rows, cross-project blocked, nonexistent project 404, missing auth 401.
8. `be15_legacy_admin_audit_log_and_bug_drafts_still_work` — regression: `/admin/audit-log` still Admin-only and functional, `/bug-drafts` still functional.

Plus the updated `actual_be9_cross_project_isolation_on_evidence_by_id` (now covers both the metadata and content paths).

## 15. Targeted test result

```
running 8 tests
test api_tests::be15_legacy_admin_audit_log_and_bug_drafts_still_work ... ok
test api_tests::be15_findings_require_authentication_and_reject_nonexistent_resources ... ok
test api_tests::be15_audit_log_scoped_to_project_and_isolated ... ok
test api_tests::be15_evidence_detail_is_metadata_not_raw_bytes ... ok
test api_tests::be15_finding_analysis_correlates_real_failure_cluster_and_is_audited ... ok
test api_tests::be15_findings_cross_project_isolation_and_role_check ... ok
test api_tests::be15_findings_list_and_detail_reflect_real_recurring_failures ... ok
test api_tests::be15_finding_review_scoped_confirms_and_dismisses ... ok

test result: ok. 8 passed; 0 failed
```

## 16. `cargo check --all-targets`

Clean — no warnings, no errors.

## 17. Full `cargo test` result

```
test result: ok. 96 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ~3.9s
```
88 pre-existing (D1–D5, B-series, BE-8, BE-9 errors, BE-10, Actual BE-9, BE-12, BE-13, BE-14) + 8 new BE-15 tests. Re-ran the full suite 4 times consecutively — 96/96 every run, no flakiness. Exactly one pre-existing test's *assertions* changed (the evidence-shape test, §4), and two call sites were updated for `list_audit`'s new parameter — no test was deleted, and no assertion was weakened.

## 18. Preserved guarantees (Step 3 — explicitly re-checked, not just assumed)

Project isolation, BE-12 dashboard/KPI, BE-13 cancellation (including real subprocess interruption), BE-14 realtime (project isolation, `project_sequence`, Lagged/`4001` close), structured REST errors, legacy routes, `/metrics`, and existing authentication semantics — none were touched by this unit. Full regression above confirms this rather than asserting it.

## 19. Explicit out-of-scope work discovered but left untouched

- **§13 canonical status naming** (`INFRA_ERROR` vs the contract's `INFRASTRUCTURE_ERROR`) — a known, separately-tracked gap (not part of BE-15's own `§15, §16, §25` scope). Does not block BE-15; left alone.
- **§17 RBAC** (roles/matrix CRUD) — MISSING per the gap matrix, unrelated to evidence/findings/audit; not touched.
- **§21 Reports** ("report generation" is listed as an auditable action category in §25) — Reports themselves are entirely unbuilt (MISSING); BE-15 audits the actions this backend *can* actually take (`finding.review`, `finding.analysis`, plus everything already audited before this unit), not a category with no real event to audit yet.
- G-WS, G-AUTHN — untouched, as instructed.

## 20. Remaining blocker or limitation

None blocking. Disclosed, non-blocking limitations:
- §16.3's "may correlate ... environment data, logs" is only partially honored (cluster data only) — no real linkage from a finding to specific evidence/environment/log records exists in this schema; fabricating one was rejected (§6).
- The evidence-detail/content split (§4) is a judgment call grounded in the contract's own text, not an example schema the contract gives verbatim — documented in full above rather than silently assumed.
