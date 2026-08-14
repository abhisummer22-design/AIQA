/**
 * Bug drafts — the anomaly -> confirmed-defect review gate (PRD §14).
 *
 * Sources: `GET /bug-drafts`, `POST /bug-drafts/:id/review`.
 *
 * A draft is explicitly *not* a defect. Promotion happens only when a human
 * presses Confirm, which calls the real review endpoint; the orchestrator
 * records who did it and audits the decision. Nothing here promotes
 * automatically, and confidence is always shown next to the claim so a
 * low-confidence draft never reads as an established fact.
 *
 * The previous UI shipped two hardcoded "bug candidates" with invented 94%
 * and 89% confidence values and an "evidenceCount" that counted nothing.
 */

import React from "react";
import { Bug, Check, X } from "lucide-react";
import { useApiAction, useApiResource } from "../hooks/useApi";
import { listBugDrafts, reviewBugDraft } from "../services/orchestrator";
import type { BugDraftRow } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { ConfidenceLabel, ReviewStateBadge, Timestamp } from "../components/common/Badges";

export function BugDraftsView() {
  const drafts = useApiResource<BugDraftRow[]>((signal) => listBugDrafts(signal), []);
  const review = useApiAction((id: string, action: "confirm" | "dismiss") => reviewBugDraft(id, action));

  const act = async (id: string, action: "confirm" | "dismiss") => {
    await review.run(id, action);
    drafts.reload();
  };

  return (
    <div className="space-y-4">
      <header>
        <h1 className="text-lg font-bold tracking-tight text-slate-100">Bug Drafts</h1>
        <p className="mt-0.5 text-xs text-slate-500">
          AI-generated drafts awaiting human review. A draft becomes a confirmed defect only when someone confirms it —
          the decision is recorded and audited server-side.
        </p>
      </header>

      {review.error && (
        <div className="rounded-lg border border-rose-800/60 bg-rose-950/30 px-3 py-2 text-xs text-rose-300">
          {review.error.detail || review.error.message}
        </div>
      )}

      <section className="rounded-xl border border-slate-800 bg-[#0a0f1a]">
        <h2 className="flex items-center gap-2 border-b border-slate-800 px-4 py-2.5 text-sm font-semibold text-slate-200">
          <Bug size={14} className="text-cyan-400" />
          Drafts
        </h2>
        <div className="p-3">
          <ResourceView
            state={drafts}
            loadingLabel="Loading bug drafts…"
            emptyTitle="No bug drafts"
            emptyMessage="The intelligence pipeline has not drafted any bugs. Drafts are produced from failure clusters."
          >
            {(rows) => (
              <ul className="space-y-2">
                {rows.map((d) => {
                  const isPending = d.review_state !== "CONFIRMED" && d.review_state !== "DISMISSED";
                  return (
                    <li key={d.id} className="rounded-lg border border-slate-800 bg-slate-900/40 p-3">
                      <div className="flex flex-wrap items-start justify-between gap-2">
                        <h3 className="min-w-0 flex-1 text-sm font-medium text-slate-200">{d.title}</h3>
                        <div className="flex shrink-0 items-center gap-2">
                          {d.severity && (
                            <span className="rounded bg-slate-800 px-1.5 py-0.5 font-mono text-[9px] uppercase tracking-wider text-slate-400">
                              {d.severity}
                            </span>
                          )}
                          <ReviewStateBadge state={d.review_state} />
                        </div>
                      </div>

                      <p className="mt-1.5 whitespace-pre-wrap text-[11px] leading-relaxed text-slate-400">
                        {d.description}
                      </p>

                      <div className="mt-2 flex flex-wrap items-center gap-3">
                        <ConfidenceLabel value={d.confidence} />
                        <span className="font-mono text-[10px] text-slate-600">
                          cluster {d.failure_cluster_id.slice(0, 8)}
                        </span>
                        <span className="font-mono text-[10px] text-slate-600">
                          drafted <Timestamp value={d.created_at} />
                        </span>
                        {d.promoted_by && (
                          <span className="font-mono text-[10px] text-slate-500">
                            reviewed by {d.promoted_by} <Timestamp value={d.promoted_at} />
                          </span>
                        )}
                      </div>

                      {isPending && (
                        <div className="mt-2.5 flex gap-2">
                          <button
                            disabled={review.isRunning}
                            onClick={() => act(d.id, "confirm")}
                            className="flex items-center gap-1 rounded border border-rose-700/60 px-2.5 py-1 text-[11px] text-rose-300 transition hover:bg-rose-950/40 disabled:opacity-40"
                          >
                            <Check size={11} /> Confirm as defect
                          </button>
                          <button
                            disabled={review.isRunning}
                            onClick={() => act(d.id, "dismiss")}
                            className="flex items-center gap-1 rounded border border-slate-700 px-2.5 py-1 text-[11px] text-slate-400 transition hover:bg-slate-800 disabled:opacity-40"
                          >
                            <X size={11} /> Dismiss
                          </button>
                        </div>
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
          </ResourceView>
        </div>
      </section>
    </div>
  );
}
