/**
 * FE-12 — the one shared place that turns a raw backend status/state
 * constant into a human-readable display label.
 *
 * This is deliberately the *only* transformation applied: real backend
 * enums (`domain.rs::TaskStatus`/`RunState`, `store.rs`'s `review_state`)
 * are serialized `SCREAMING_SNAKE_CASE` (Contract §13, re-verified against
 * `domain.rs` directly) — that raw value is never renamed anywhere in this
 * app (not here, not in `types/contract.ts`, not in any API call or
 * comparison; see that file's own `TaskStatus` doc comment on the one
 * known, deliberately-preserved `INFRA_ERROR`-vs-contract's-
 * `INFRASTRUCTURE_ERROR` naming gap). Before this sprint, two different
 * real pages displayed the exact same raw value two different ways —
 * `ExecutionHubPage.tsx` printed it with the underscore literally visible
 * (`"PENDING_APPROVAL"`, `"NEEDS_REVIEW"`), while `ReportsPage.tsx` (and
 * the pre-existing, since-orphaned `common/StatusBadge.tsx`) replaced the
 * underscore with a space (`"NEEDS REVIEW"`). Space-separated was already
 * the majority convention, so it is the one this sprint keeps — this
 * function is the single place that decision now lives, rather than each
 * page re-deriving or re-copying it.
 */
export function formatStatusLabel(raw: string): string {
  return raw.replace(/_/g, ' ');
}
