/**
 * Intelligence — test risk/value/flakiness signals and failure clusters.
 *
 * Sources: `GET /intelligence/tests` and `GET /intelligence/failure-clusters`.
 */

import React, { useMemo } from "react";
import { Activity, Bug, GitBranch, ShieldCheck, Sparkles, AlertTriangle, RotateCcw } from "lucide-react";
import { useApiResource } from "../hooks/useApi";
import { listBugDrafts, listFailureClusters, listTestIntelligence } from "../services/orchestrator";
import type { BugDraftRow, FailureClusterRow, TestIntelRow } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { ConfidenceLabel, ReviewStateBadge, Timestamp } from "../components/common/Badges";
import { SectionHeader } from "../components/common/SectionHeader";
import { relativeTime } from "../lib/format";

export function IntelligenceView() {
  const tests = useApiResource<TestIntelRow[]>((signal) => listTestIntelligence(signal), []);
  const clusters = useApiResource<FailureClusterRow[]>((signal) => listFailureClusters(signal), []);
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
      <SectionHeader
        title="Quality & Failure Intelligence"
        subtitle="Test risk, execution value, flakiness signals, and recurring failure signature clusters computed by the intelligence engine."
        badge="REALTIME BACKEND"
        badgeTone="live"
        action={
          <button
            onClick={() => {
              tests.reload();
              clusters.reload();
              drafts.reload();
            }}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-slate-700 bg-slate-900/60 hover:bg-slate-800 text-xs text-slate-300 transition-colors cursor-pointer"
          >
            <RotateCcw size={12} />
            <span>Refresh Intelligence</span>
          </button>
        }
      />

      {/* Test intelligence */}
      <section className="rounded-xl border border-slate-800 bg-[#0a0f1a] shadow-lg">
        <h2 className="flex items-center gap-2 border-b border-slate-800 px-4 py-3 text-sm font-semibold text-slate-200">
          <Activity size={14} className="text-cyan-400" />
          <span>Per-Test Quality & Risk Signals</span>
        </h2>
        <div className="overflow-x-auto p-3">
          <ResourceView
            state={tests}
            loadingLabel="Loading test intelligence signals…"
            emptyTitle="No test signals available"
            emptyMessage="The intelligence pipeline computes per-test signals from completed run history. Complete a test run to generate signals."
          >
            {(rows) => (
              <table className="w-full min-w-[720px] text-left text-xs">
                <thead>
                  <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-500 border-b border-slate-800/80">
                    <th className="pb-2.5 font-normal">Test Target</th>
                    <th className="pb-2.5 text-right font-normal">Risk Score</th>
                    <th className="pb-2.5 text-right font-normal">Value Score</th>
                    <th className="pb-2.5 text-right font-normal">Flakiness</th>
                    <th className="pb-2.5 text-right font-normal">Failure Rate</th>
                    <th className="pb-2.5 text-right font-normal">Prerequisites</th>
                    <th className="pb-2.5 text-right font-normal">Exec Cost</th>
                    <th className="pb-2.5 text-right font-normal">History Window</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row) => (
                    <tr key={row.id} className="border-t border-slate-800/60 hover:bg-slate-900/30 transition-colors">
                      <td className="py-2.5 pr-3">
                        <div className="font-medium text-slate-200">{row.name}</div>
                        <div className="font-mono text-[10px] text-slate-500">{row.id.slice(0, 8)}</div>
                      </td>
                      <Score value={row.risk_score} tone="risk" />
                      <Score value={row.value_score} />
                      <Score value={row.flakiness_score} tone="flaky" />
                      <Score value={row.failure_rate} tone="risk" />
                      <Num value={row.dependency_count} />
                      <Num value={row.execution_cost_ms} suffix="ms" />
                      <td className="py-2.5 text-right font-mono text-[10px] text-slate-400">
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
      <section className="rounded-xl border border-slate-800 bg-[#0a0f1a] shadow-lg">
        <h2 className="flex items-center gap-2 border-b border-slate-800 px-4 py-3 text-sm font-semibold text-slate-200">
          <GitBranch size={14} className="text-cyan-400" />
          <span>Failure Signature Clusters</span>
        </h2>
        <div className="p-3">
          <ResourceView
            state={clusters}
            loadingLabel="Loading failure clusters…"
            emptyTitle="No failure clusters detected"
            emptyMessage="No recurring failure signatures have been identified across historical test runs."
          >
            {(rows) => (
              <ul className="space-y-2.5">
                {rows.map((c) => (
                  <li key={c.id} className="rounded-lg border border-slate-800 bg-slate-900/50 p-3.5 space-y-2">
                    <div className="flex flex-wrap items-start justify-between gap-2">
                      <code className="min-w-0 flex-1 break-all font-mono text-xs text-cyan-300 bg-slate-950/80 px-2 py-1 rounded border border-slate-800">
                        {c.signature}
                      </code>
                      <div className="flex shrink-0 items-center gap-2">
                        <ReviewStateBadge state={c.review_state} />
                        <span className="font-mono text-[10px] text-slate-400 px-1.5 py-0.5 rounded bg-slate-800 border border-slate-700">
                          {c.occurrence_count} occurrences
                        </span>
                      </div>
                    </div>

                    {c.root_cause_hypothesis ? (
                      <div className="rounded border border-slate-800 bg-slate-950/70 p-3 space-y-1.5">
                        <div className="font-mono text-[9px] uppercase tracking-wider text-cyan-400 font-semibold flex items-center gap-1.5">
                          <Sparkles size={11} />
                          <span>AI Root-Cause Hypothesis (Unconfirmed)</span>
                        </div>
                        <p className="text-xs leading-relaxed text-slate-300">{c.root_cause_hypothesis}</p>
                        <div className="flex items-center gap-3 pt-1">
                          <ConfidenceLabel value={c.confidence} />
                          <span className="font-mono text-[10px] text-slate-400">
                            {c.reproducibility === null
                              ? "reproducibility unmeasured"
                              : `${(c.reproducibility * 100).toFixed(0)}% reproducible`}
                          </span>
                        </div>
                      </div>
                    ) : (
                      <p className="font-mono text-[10px] text-slate-500">No root-cause hypothesis generated.</p>
                    )}

                    {(() => {
                      const linked = draftsByCluster.get(c.id) ?? [];
                      if (linked.length === 0) return null;
                      return (
                        <div className="rounded border border-slate-800 bg-slate-950/50 p-2.5">
                          <div className="mb-1.5 flex items-center gap-1.5 font-mono text-[9px] uppercase tracking-wider text-slate-400 font-semibold">
                            <Bug size={10} className="text-rose-400" />
                            <span>{linked.length} linked bug draft{linked.length > 1 ? "s" : ""}</span>
                          </div>
                          <ul className="space-y-1">
                            {linked.map((d) => (
                              <li key={d.id} className="flex items-start justify-between gap-2 text-xs text-slate-300">
                                <span className="truncate">{d.title}</span>
                                <ReviewStateBadge state={d.review_state} />
                              </li>
                            ))}
                          </ul>
                        </div>
                      );
                    })()}

                    <div className="flex gap-4 font-mono text-[10px] text-slate-500 pt-1 border-t border-slate-800/60">
                      <span>
                        first detected: <Timestamp value={c.first_seen_at} /> ({relativeTime(c.first_seen_at)})
                      </span>
                      <span>
                        last seen: <Timestamp value={c.last_seen_at} />
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

function Score({ value, tone = "neutral" }: { value: number | null; tone?: "neutral" | "risk" | "flaky" }) {
  if (value === null || value === undefined) {
    return <td className="py-2.5 pr-3 text-right font-mono text-[11px] text-slate-600">—</td>;
  }
  const pct = value <= 1 ? value * 100 : value;
  const cls =
    tone === "risk"
      ? pct >= 70
        ? "text-rose-400 font-semibold"
        : pct >= 40
          ? "text-amber-400 font-medium"
          : "text-slate-300"
      : tone === "flaky"
        ? pct >= 20
          ? "text-amber-400 font-medium"
          : "text-slate-300"
        : "text-slate-300";

  return <td className={`py-2.5 pr-3 text-right font-mono text-[11px] ${cls}`}>{pct.toFixed(1)}%</td>;
}

function Num({ value, suffix = "" }: { value: number | null; suffix?: string }) {
  return (
    <td className="py-2.5 pr-3 text-right font-mono text-[11px] text-slate-400">
      {value === null || value === undefined ? "—" : `${Math.round(value)}${suffix}`}
    </td>
  );
}
