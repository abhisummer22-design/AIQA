/**
 * Formatting helpers.
 *
 * Every function here derives its output from a value the orchestrator
 * actually returned. Where the backend gives no value, these return `null`
 * so the caller renders an em dash — none of them substitute a default.
 *
 * Durations in particular are computed from the real `started_at` /
 * `finished_at` timestamps on `TaskState` and `RunSummary`. The backend has
 * always returned these; the previous UI displayed invented `durationSec`
 * values instead.
 */

/** Milliseconds between two RFC 3339 timestamps, or null if either is absent. */
export function durationMs(startedAt: string | null, finishedAt: string | null): number | null {
  if (!startedAt || !finishedAt) return null;
  const a = new Date(startedAt).getTime();
  const b = new Date(finishedAt).getTime();
  if (Number.isNaN(a) || Number.isNaN(b)) return null;
  const d = b - a;
  return d >= 0 ? d : null;
}

/** Compact human duration: 940ms, 4.2s, 1m 12s, 2h 5m. */
export function formatDuration(ms: number | null): string {
  if (ms === null) return "—";
  if (ms < 1000) return `${Math.round(ms)}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(s < 10 ? 1 : 0)}s`;
  const m = Math.floor(s / 60);
  const rem = Math.round(s % 60);
  if (m < 60) return `${m}m ${rem}s`;
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m`;
}

/** Duration of a run or task in one call. */
export function formatSpan(startedAt: string | null, finishedAt: string | null): string {
  return formatDuration(durationMs(startedAt, finishedAt));
}

/** "3m ago", "2h ago", "5d ago". Absolute date beyond a week. */
export function relativeTime(iso: string | null): string {
  if (!iso) return "—";
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return "—";
  const diff = Date.now() - t;
  if (diff < 0) return "just now";
  const s = Math.floor(diff / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  if (d < 7) return `${d}d ago`;
  return new Date(iso).toLocaleDateString();
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * A 0–1 score as a percentage string, or "—" when unscored.
 * Never coerces null to 0 — an unscored test is not a zero-risk test.
 */
export function formatScore(v: number | null | undefined): string {
  if (v === null || v === undefined) return "—";
  const pct = v <= 1 ? v * 100 : v;
  return `${pct.toFixed(1)}`;
}

/** Short uuid prefix for dense tables. */
export function shortId(id: string): string {
  return id.slice(0, 8);
}
