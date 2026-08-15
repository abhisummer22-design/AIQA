/**
 * Intelligence — test risk/value/flakiness signals and failure clusters.
 *
 * Sources: `GET /intelligence/tests` and `GET /intelligence/failure-clusters`.
 *
 * Scores are rendered exactly as stored. A metric the orchestrator has not
 * computed shows "—", never 0 and never an invented percentage: PRD §15
 * requires that uncertain AI output not be presented as fact, and a missing
 * score is not the same as a low one. Root-cause hypotheses are shown with
 * their confidence, reproducibility and review state attached, so a
 * hypothesis can never read as a confirmed diagnosis.
 */

import React, { useMemo } from "react";
import { Activity, Bug, GitBranch } from "lucide-react";
import { useApiResource } from "../hooks/useApi";
import { listBugDrafts, listFailureClusters, listTestIntelligence } from "../services/orchestrator";
import type { BugDraftRow, FailureClusterRow, TestIntelRow } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { ConfidenceLabel, ReviewStateBadge, Timestamp } from "../components/common/Badges";
import { relativeTime } from "../lib/format";

export function IntelligenceView() {
  const tests = useApiResource<TestIntelRow[]>((signal) => listTestIntelligence(signal), []);
  const clusters = useApiResource<FailureClusterRow[]>((signal) => listFailureClusters(signal), []);
  // Drafts carry `failure_cluster_id`, so each cluster can show what it
  // produced. The two lists were previously rendered on separate screens
  // with the relationship between them invisible.
  const drafts = useApiResource<BugDraftRow[]>((signal) => listBugDrafts(signal), []);

  const draftsByCluster = useMemo(() => {
    const map = new Map<string, BugDraftRow[]>();
    for (const d of drafts.data ?? []) {
      if (!map.has(d.failure_cluster_id)) map.set(d.failure_cluster_id, []);
      map.get(d.failure_cluster_id)!.push(d);
    }
    return map;
  }, [drafts.data]);

  return (
    <div className="space-y-4">
      <header>
        <h1 className="text-lg font-bold tracking-tight text-slate-100">Intelligence</h1>
        <p className="mt-0.5 text-xs text-slate-500">
          Risk, value and flakiness signals computed by the orchestrator's intelligence pipeline. Unscored fields show
          as <span className="font-mono">—</span> rather than a placeholder number.
        </p>
      </header>

      {/* Test intelligence */}
      <section className="rounded-xl border border-slate-800 bg-[#0a0f1a]">
        <h2 className="flex items-center gap-2 border-b border-slate-800 px-4 py-2.5 text-sm font-semibold text-slate-200">
          <Activity size={14} className="text-cyan-400" />
          Test signals
        </h2>
        <div className="overflow-x-auto p-3">
          <ResourceView
            state={tests}
            loadingLabel="Loading test intelligence…"
            emptyTitle="No test signals"
            emptyMessage="The intelligence pipeline has not produced per-test signals yet. These are derived from completed run history."
          >
            {(rows) => (
              <table className="w-full min-w-[720px] text-left text-xs">
                <thead>
                  <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-600">
                    <th className="pb-2 font-normal">Test</th>
                    <th className="pb-2 text-right font-normal">Risk</th>
                    <th className="pb-2 text-right font-normal">Value</th>
                    <th className="pb-2 text-right font-normal">Flakiness</th>
                    <th className="pb-2 text-right font-normal">Failure rate</th>
                    <th className="pb-2 text-right font-normal">Deps</th>
                    <th className="pb-2 text-right font-normal">Cost</th>
                    <th className="pb-2 text-right font-normal">Window</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row) => (
                    <tr key={row.id} className="border-t border-slate-800/70">
                      <td className="py-2 pr-3">
                        <div className="font-medium text-slate-200">{row.name}</div>
                        <div className="font-mono text-[10px] text-slate-600">{row.id.slice(0, 8)}</div>
                      </td>
                      <Score value={row.risk_score} tone="risk" />
                      <Score value={row.value_score} />
                      <Score value={row.flakiness_score} tone="flaky" />
                      <Score value={row.failure_rate} tone="risk" />
                      <Num value={row.dependency_count} />
                      <Num value={row.execution_cost_ms} suffix="ms" />
                      <td className="py-2 text-right font-mono text-[10px] text-slate-500">
                        {row.window_runs === null
                          ? "—"
                          : `${row.window_failures ?? 0}/${row.window_runs}`}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </ResourceView>
        </div>
      </section>

      {/* Failure clusters */}
      <section className="rounded-xl border border-slate-800 bg-[#0a0f1a]">
        <h2 className="flex items-center gap-2 border-b border-slate-800 px-4 py-2.5 text-sm font-semibold text-slate-200">
          <GitBranch size={14} className="text-cyan-400" />
          Failure clusters
        </h2>
        <div className="p-3">
          <ResourceView
            state={clusters}
            loadingLabel="Loading failure clusters…"
            emptyTitle="No failure clusters"
            emptyMessage="No recurring failure signatures have been identified."
          >
            {(rows) => (
              <ul className="space-y-2">
                {rows.map((c) => (
                  <li key={c.id} className="rounded-lg border border-slate-800 bg-slate-900/40 p-3">
                    <div className="flex flex-wrap items-start justify-between gap-2">
                      <code className="min-w-0 flex-1 break-all font-mono text-[11px] text-slate-300">
                        {c.signature}
                      </code>
                      <div className="flex shrink-0 items-center gap-2">
                        <ReviewStateBadge state={c.review_state} />
                        <span className="font-mono text-[10px] text-slate-500">×{c.occurrence_count}</span>
                      </div>
                    </div>

                    {c.root_cause_hypothesis ? (
                      <div className="mt-2 rounded border border-slate-800 bg-slate-950/60 px-2.5 py-1.5">
                        <div className="font-mono text-[9px] uppercase tracking-wider text-slate-600">
                          Root-cause hypothesis
                        </div>
                        <p className="mt-0.5 text-[11px] leading-relaxed text-slate-400">{c.root_cause_hypothesis}</p>
                        <div className="mt-1 flex items-center gap-3">
                          <ConfidenceLabel value={c.confidence} />
                          <span className="font-mono text-[10px] text-slate-600">
                            {c.reproducibility === null
                              ? "reproducibility not measured"
                              : `${(c.reproducibility * 100).toFixed(0)}% reproducible`}
                          </span>
                        </div>
                      </div>
                    ) : (
                      <p className="mt-2 font-mono text-[10px] text-slate-600">No root-cause hypothesis generated.</p>
                    )}

                    {(() => {
                      const linked = draftsByCluster.get(c.id) ?? [];
                      if (linked.length === 0) return null;
                      return (
                        <div className="mt-2 rounded border border-slate-800 bg-slate-950/40 px-2.5 py-1.5">
                          <div className="mb-1 flex items-center gap-1.5 font-mono text-[9px] uppercase tracking-wider text-slate-600">
                            <Bug size={9} />
                            {linked.length} bug draft{linked.length > 1 ? "s" : ""} from this cluster
                          </div>
                          <ul className="space-y-1">
                            {linked.map((d) => (
                              <li key={d.id} className="flex items-start justify-between gap-2">
                                <span className="min-w-0 flex-1 truncate text-[11px] text-slate-300">{d.title}</span>
                                <ReviewStateBadge state={d.review_state} />
                              </li>
                            ))}
                          </ul>
                        </div>
                      );
                    })()}

                    <div className="mt-2 flex gap-3 font-mono text-[10px] text-slate-600">
                      <span>
                        first <Timestamp value={c.first_seen_at} /> ({relativeTime(c.first_seen_at)})
                      </span>
                      <span>
                        last <Timestamp value={c.last_seen_at} />
                      </span>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </ResourceView>
        </div>
      </section>
    </div>
  );
}

/** 0–1 score rendered as a percentage; null renders as an em dash. */
function Score({ value, tone = "neutral" }: { value: number | null; tone?: "neutral" | "risk" | "flaky" }) {
  if (value === null || value === undefined) {
    return <td className="py-2 pr-3 text-right font-mono text-[11px] text-slate-600">—</td>;
  }
  const pct = value <= 1 ? value * 100 : value;
  const cls =
    tone === "risk"
      ? pct >= 70
        ? "text-rose-400"
        : pct >= 40
          ? "text-amber-400"
          : "text-slate-300"
      : tone === "flaky"
        ? pct >= 20
          ? "text-amber-400"
          : "text-slate-300"
        : "text-slate-300";

  return <td className={`py-2 pr-3 text-right font-mono text-[11px] ${cls}`}>{pct.toFixed(1)}</td>;
}

function Num({ value, suffix = "" }: { value: number | null; suffix?: string }) {
  return (
    <td className="py-2 pr-3 text-right font-mono text-[11px] text-slate-400">
      {value === null || value === undefined ? "—" : `${Math.round(value)}${suffix}`}
    </td>
  );
}
