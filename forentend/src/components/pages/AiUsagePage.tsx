/**
 * Phase 8A — AI usage and cost: what the AI calls came to.
 *
 * Backend contract (`services/orchestrator/src/ai_usage.rs`; `GET /api/v1/projects/:project_id/ai-usage` and `GET /api/v1/ai/usage`):
 * every call the router makes to a provider, the ones that answer and the ones that do not, is counted (never its prompt or its
 * answer). This page shows the counts for the chosen project or, on a key that is not tied to one project, for every project at
 * once: calls, failures, time, tokens, and the cost of the calls that could be priced. The days are cut at the viewer's own midnight.
 *
 * It only shows: nothing here is added up or priced again. A call to a model with no price is counted and never given a cost, and
 * the page says how many were left out and why.
 */

import React, { useCallback, useState } from 'react';
import { Coins, FolderOpen, RefreshCw } from 'lucide-react';
import { useProject } from '../../context/ProjectContext';
import { useAiUsage, type UsageTarget } from '../../hooks/useAiUsage';
import { useProviderNames } from '../../hooks/useProviderNames';
import { StateCard, renderResourceErrorState } from '../state/StateViews';
import { EmptyState } from '../common/EmptyState';
import { UsageReportView } from '../ai-usage/UsageReportView';
import { DAY_CHOICES, DEFAULT_DAYS } from '../ai-usage/aiUsageModel';
import { formatTimestamp } from '../autopilot/autopilotModel';

type Scope = 'project' | 'all';

const SEGMENT = (on: boolean) =>
  `px-2.5 py-1 text-xs cursor-pointer ${on ? 'bg-violet-950/70 text-violet-200' : 'text-slate-400 hover:bg-slate-800'}`;

export const AiUsagePage: React.FC = () => {
  const { projects, selectedProjectId, selectedProject } = useProject();
  const [scope, setScope] = useState<Scope>('project');
  const [days, setDays] = useState(DEFAULT_DAYS);
  const providerName = useProviderNames(true);

  const target: UsageTarget | null = scope === 'all' ? { kind: 'all' } : selectedProjectId ? { kind: 'project', projectId: selectedProjectId } : null;
  const usage = useAiUsage(target, days);

  const projectName = useCallback(
    (id: string | null): string => {
      if (id === null) return 'Not made for a project';
      return projects.find((p) => p.id === id)?.name ?? `A removed project (${id.slice(0, 8)})`;
    },
    [projects],
  );

  const renderBody = () => {
    if (target === null) {
      return (
        <EmptyState
          icon={FolderOpen}
          title="No project selected"
          description="Choose a project from the selector in the header to see what its AI calls came to, or look at every project at once."
        />
      );
    }
    if (usage.loading) {
      return (
        <StateCard
          icon={Coins}
          iconClassName="text-violet-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
          title="Loading AI usage"
          description="Reading what the AI calls came to…"
        />
      );
    }
    if (usage.error && !usage.report) {
      return renderResourceErrorState(usage.error, usage.refresh, {
        forbiddenTitle: scope === 'all' ? 'Not permitted to view every project' : 'Not permitted to view this project',
        forbiddenDescription:
          scope === 'all'
            ? "Every project's usage needs an admin key that is not tied to one project. Choose “This project” to see the selected project's own."
            : "This key's project scope does not permit viewing this project's AI usage.",
        genericTitle: "Couldn't load the AI usage",
      });
    }
    if (!usage.report) return null;
    return (
      <div className="space-y-3">
        {usage.error && (
          <p id="ai-usage-error" role="alert" className="p-2 rounded bg-amber-950/30 border border-amber-800/60 text-[11px] text-amber-200">
            Reading the usage again failed ({usage.error.message}); what is shown is from the read before.
          </p>
        )}
        {usage.report.totals.calls === 0 ? (
          <EmptyState
            icon={Coins}
            title={`No AI calls counted in the last ${usage.report.range.days === 1 ? 'day' : `${usage.report.range.days} days`}`}
            description="Every call to an AI provider is counted from the moment this version started: generating test cases, checking requirements, running an autopilot. Calls made before that are not in it."
          />
        ) : (
          <UsageReportView report={usage.report} providerName={providerName} projectName={projectName} />
        )}
      </div>
    );
  };

  return (
    <div id="aiqa-ai-usage-page" className="space-y-4 animate-in fade-in duration-200 text-slate-200">
      <header
        id="ai-usage-header"
        className="flex flex-col md:flex-row md:items-center justify-between gap-3 p-4 rounded-lg bg-[#0B1120] border border-slate-800/90 shadow-sm"
      >
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <span className="p-1.5 rounded bg-violet-950/80 border border-violet-700/60 text-violet-400">
              <Coins className="w-4 h-4" />
            </span>
            <h1 className="text-base font-bold text-slate-100 font-sans tracking-tight">AI Usage &amp; Cost</h1>
            <span className="px-2 py-0.5 rounded bg-violet-950/60 border border-violet-800/60 text-[10px] font-mono text-violet-300 uppercase">
              CALLS · TOKENS · COST
            </span>
          </div>
          <p className="text-xs text-slate-400 font-sans">
            {scope === 'all'
              ? 'What the AI calls of every project came to'
              : selectedProject
                ? `What the AI calls made for ${selectedProject.name} came to`
                : 'What the AI calls came to'}
            : how many, how long they took, how many tokens they used, and what the priced ones cost
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <div role="group" aria-label="Whose usage" className="inline-flex rounded-md border border-slate-700 overflow-hidden">
            <button type="button" id="ai-usage-scope-project" aria-pressed={scope === 'project'} onClick={() => setScope('project')} className={SEGMENT(scope === 'project')}>
              This project
            </button>
            <button type="button" id="ai-usage-scope-all" aria-pressed={scope === 'all'} onClick={() => setScope('all')} className={SEGMENT(scope === 'all')}>
              All projects
            </button>
          </div>
          <div role="group" aria-label="How far back" className="inline-flex rounded-md border border-slate-700 overflow-hidden">
            {DAY_CHOICES.map((d) => (
              <button key={d} type="button" id={`ai-usage-days-${d}`} aria-pressed={days === d} onClick={() => setDays(d)} className={SEGMENT(days === d)}>
                {d} days
              </button>
            ))}
          </div>
          <button
            type="button"
            id="ai-usage-refresh"
            onClick={usage.refresh}
            disabled={target === null || usage.loading || usage.refreshing}
            className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md border border-slate-700 text-xs text-slate-200 hover:bg-slate-800 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${usage.refreshing ? 'animate-spin' : ''}`} /> Refresh
          </button>
          {usage.updatedAt && (
            <span id="ai-usage-updated" className="text-[10px] text-slate-500 font-mono">
              read {formatTimestamp(usage.updatedAt)}
            </span>
          )}
        </div>
      </header>

      <div aria-live="polite">{renderBody()}</div>
    </div>
  );
};
