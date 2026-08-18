/**
 * Bug drafts — the anomaly -> confirmed-defect review gate (PRD §14).
 *
 * Sources: `GET /bug-drafts`, `POST /bug-drafts/:id/review`.
 */

import React from "react";
import { Bug, Check, X, ShieldAlert, RotateCcw } from "lucide-react";
import { useApiAction, useApiResource } from "../hooks/useApi";
import { listBugDrafts, reviewBugDraft } from "../services/orchestrator";
import type { BugDraftRow } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { ConfidenceLabel, ReviewStateBadge, Timestamp } from "../components/common/Badges";
import { SectionHeader } from "../components/common/SectionHeader";

export function BugDraftsView() {
  const drafts = useApiResource<BugDraftRow[]>((signal) => listBugDrafts(signal), []);
  const review = useApiAction((id: string, action: "confirm" | "dismiss") => reviewBugDraft(id, action));

  const act = async (id: string, action: "confirm" | "dismiss") => {
    await review.run(id, action);
    drafts.reload();
  };

  return (
    <div className="space-y-4">
      <SectionHeader
        title="Bug Drafts & Human Review Gate"
        subtitle="AI-synthesized defect candidates awaiting human confirmation before formal ticket promotion."
        badge="REALTIME BACKEND"
        badgeTone="live"
        action={
          <button
            onClick={() => drafts.reload()}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-slate-700 bg-slate-900/60 hover:bg-slate-800 text-xs text-slate-300 transition-colors cursor-pointer"
          >
            <RotateCcw size={12} />
            <span>Refresh Drafts</span>
          </button>
        }
      />

      {review.error && (
        <div className="rounded-lg border border-rose-800/60 bg-rose-950/30 px-3 py-2 text-xs text-rose-300">
          {review.error.detail || review.error.message}
        </div>
      )}

      <section className="rounded-xl border border-slate-800 bg-[#0a0f1a] shadow-lg">
        <h2 className="flex items-center gap-2 border-b border-slate-800 px-4 py-3 text-sm font-semibold text-slate-200">
          <Bug size={14} className="text-cyan-400" />
          <span>Defect Candidates</span>
        </h2>
        <div className="p-3">
          <ResourceView
            state={drafts}
            loadingLabel="Loading bug drafts…"
            emptyTitle="No bug drafts awaiting review"
            emptyMessage="The intelligence pipeline has not drafted any defects from recent run failure clusters."
          >
            {(rows) => (
              <ul className="space-y-2.5">
                {rows.map((d) => {
                  const isPending = d.review_state !== "CONFIRMED" && d.review_state !== "DISMISSED";
                  return (
                    <li key={d.id} className="rounded-lg border border-slate-800 bg-slate-900/50 p-3.5 space-y-2">
                      <div className="flex flex-wrap items-start justify-between gap-2">
                        <h3 className="min-w-0 flex-1 text-sm font-semibold text-slate-100">{d.title}</h3>
                        <div className="flex shrink-0 items-center gap-2">
                          {d.severity && (
                            <span className="rounded bg-slate-800 px-2 py-0.5 font-mono text-[9px] uppercase tracking-wider text-slate-300 border border-slate-700">
                              {d.severity}
                            </span>
                          )}
                          <ReviewStateBadge state={d.review_state} />
                        </div>
                      </div>

                      <p className="whitespace-pre-wrap text-xs leading-relaxed text-slate-300 bg-slate-950/70 p-3 rounded border border-slate-800/80">
                        {d.description}
                      </p>

                      <div className="flex flex-wrap items-center gap-4 text-[11px] font-mono text-slate-400 pt-1">
                        <ConfidenceLabel value={d.confidence} />
                        <span>cluster: {d.failure_cluster_id.slice(0, 8)}</span>
                        <span>
                          drafted: <Timestamp value={d.created_at} />
                        </span>
                        {d.promoted_by && (
                          <span className="text-cyan-300">
                            reviewed by: {d.promoted_by} (<Timestamp value={d.promoted_at} />)
                          </span>
                        )}
                      </div>

                      {isPending && (
                        <div className="pt-2 border-t border-slate-800/60 flex items-center justify-end gap-2">
                          <button
                            disabled={review.isRunning}
                            onClick={() => act(d.id, "dismiss")}
                            className="flex items-center gap-1 rounded border border-slate-700 bg-slate-800/60 px-3 py-1.5 text-xs text-slate-400 hover:text-slate-200 hover:bg-slate-800 transition-colors disabled:opacity-40 cursor-pointer"
                          >
                            <X size={12} /> Dismiss
                          </button>
                          <button
                            disabled={review.isRunning}
                            onClick={() => act(d.id, "confirm")}
                            className="flex items-center gap-1 rounded border border-rose-800/60 bg-rose-950/60 px-3 py-1.5 text-xs font-semibold text-rose-300 hover:bg-rose-900/60 transition-colors disabled:opacity-40 cursor-pointer"
                          >
                            <Check size={12} /> Confirm as Defect
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
