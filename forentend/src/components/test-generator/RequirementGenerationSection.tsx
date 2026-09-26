/**
 * Generate from the numbered requirements of the uploaded documents (Phase 3).
 *
 * The way documents have always reached the model is to paste each one (cut at
 * 20,000 characters) into every AI call, so each selected skill re-reads the
 * same text and a long document is never seen whole. When a document has been
 * read into numbered requirements (`DocumentUploadSection`), this opt-in
 * section lets generation use those instead: each skill is shown only the
 * requirements of the kinds it tests, in batches that fit the AI model that
 * would serve the call, and every case it produces names the requirement it
 * came from.
 *
 * Off by default — turning it on changes what the next generation sends, so
 * it is the user's choice, and the old way stays exactly as it was. While it
 * is on, the plan below is the backend's own planning for these documents and
 * skills (`POST .../generate-ai/plan`, no model is called): how many AI calls,
 * what each skill is shown, where the calls would go and how much each can
 * carry, and which requirements no selected skill would be shown at all.
 */
import React, { useEffect, useMemo, useState } from 'react';
import { AlertTriangle, ChevronDown, ChevronRight, Info, ListChecks, Loader2 } from 'lucide-react';
import { UploadedRequirementDoc } from '../../types';
import { ApiError, aiqaApi } from '../../services/aiqaApi';
import type { CoverageDepth, GenerateAiPlanResponse, GenerateAiSkillInput, RequirementRecord } from '../../types/contract';
import { DEPTH_LABEL, DEPTH_MEANING, techniqueLabel, techniqueSkill } from './coverageLabels';

interface RequirementGenerationSectionProps {
  projectId: string | null;
  /** Controlled by the page, which sends the documents' ids with the request. */
  enabled: boolean;
  onEnabledChange: (enabled: boolean) => void;
  /** The depth the request will ask for (the page's Exploration Depth) — what
   *  the coverage plan below is worked out for. */
  depth: CoverageDepth;
  /** Whether the request will also ask for the coverage gaps to be filled. */
  topUp: boolean;
  onTopUpChange: (topUp: boolean) => void;
  documents: UploadedRequirementDoc[];
  /** Exactly the skills the next generation would send. */
  skills: GenerateAiSkillInput[];
  disabled?: boolean;
}

/** Waits this long after the last change to the documents or skills before
 *  asking for a plan, so ticking several skills in a row is one request. */
const PLAN_DEBOUNCE_MS = 400;

/** How many requirements the list shows at once; the rest are reached by
 *  searching, and the count of what is not shown is always stated. */
const LIST_LIMIT = 200;

/** How many ids of requirements no skill would be shown are named. */
const NAMED_KEYS = 8;

function plural(n: number, one: string, many: string): string {
  return n === 1 ? one : many;
}

export const RequirementGenerationSection: React.FC<RequirementGenerationSectionProps> = ({
  projectId,
  enabled,
  onEnabledChange,
  depth,
  topUp,
  onTopUpChange,
  documents,
  skills,
  disabled = false,
}) => {
  const usable = useMemo(() => documents.filter((d) => d.requirementsDocumentId), [documents]);
  const leftOut = documents.filter((d) => !d.requirementsDocumentId);
  const docIds = usable.map((d) => d.requirementsDocumentId as string);
  const docIdsKey = docIds.join(',');
  const skillsKey = JSON.stringify(skills.map((s) => [s.id, s.name, s.description ?? '']));
  const totalRequirements = usable.reduce((sum, d) => sum + (d.requirementsCount ?? 0), 0);

  const [plan, setPlan] = useState<GenerateAiPlanResponse | null>(null);
  const [planLoading, setPlanLoading] = useState(false);
  const [planError, setPlanError] = useState<string | null>(null);

  useEffect(() => {
    if (!enabled || !projectId || docIds.length === 0) {
      setPlan(null);
      setPlanError(null);
      setPlanLoading(false);
      return;
    }
    const controller = new AbortController();
    setPlanLoading(true);
    const timer = setTimeout(() => {
      aiqaApi.testCases
        .planGenerateAi(
          projectId,
          { skills, requirement_document_ids: docIds, depth, ...(topUp ? { coverage_top_up: true } : {}) },
          controller.signal,
        )
        .then((p) => {
          if (controller.signal.aborted) return;
          setPlan(p);
          setPlanError(null);
        })
        .catch((err: unknown) => {
          if (controller.signal.aborted) return;
          setPlan(null);
          setPlanError(err instanceof ApiError ? err.message : 'Could not work out the plan.');
        })
        .finally(() => {
          if (!controller.signal.aborted) setPlanLoading(false);
        });
    }, PLAN_DEBOUNCE_MS);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
    // `skills`/`docIds` are rebuilt on every render; their keys carry what
    // matters.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, projectId, docIdsKey, skillsKey, depth, topUp]);

  // The requirements themselves, read only when the user asks to see them.
  const [showList, setShowList] = useState(false);
  const [listed, setListed] = useState<RequirementRecord[] | null>(null);
  const [listLoading, setListLoading] = useState(false);
  const [listError, setListError] = useState<string | null>(null);
  const [search, setSearch] = useState('');

  useEffect(() => {
    if (!showList || !projectId || docIds.length === 0) return;
    const controller = new AbortController();
    setListLoading(true);
    setListError(null);
    aiqaApi.requirements
      .list(projectId, { documentIds: docIds }, controller.signal)
      .then((r) => {
        if (controller.signal.aborted) return;
        setListed(r.requirements);
      })
      .catch((err: unknown) => {
        if (controller.signal.aborted) return;
        setListed(null);
        setListError(err instanceof ApiError ? err.message : 'Could not read the requirements.');
      })
      .finally(() => {
        if (!controller.signal.aborted) setListLoading(false);
      });
    return () => controller.abort();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showList, projectId, docIdsKey]);

  const slotOfDocument = (documentId: string): string =>
    usable.find((d) => d.requirementsDocumentId === documentId)?.slotId ?? '';

  const needle = search.trim().toLowerCase();
  const matching = (listed ?? []).filter(
    (r) =>
      needle === '' ||
      r.key.toLowerCase().includes(needle) ||
      r.kind.toLowerCase().includes(needle) ||
      (r.section ?? '').toLowerCase().includes(needle) ||
      r.text.toLowerCase().includes(needle),
  );

  const notOffered = plan ? plan.requirements_total - plan.offered : 0;
  const nothingToUse = usable.length === 0;

  return (
    <div id="requirement-generation-section" className="space-y-3">
      <div className="pb-2 border-b border-slate-800/80">
        <div className="flex items-center gap-2">
          <ListChecks className="w-4 h-4 text-violet-400" />
          <h2 className="text-sm font-bold text-slate-100 font-sans tracking-tight">Generate from Numbered Requirements</h2>
          <span className="text-[10px] font-mono font-semibold px-2 py-0.5 rounded bg-violet-950/60 border border-violet-800/60 text-violet-300">
            OPTIONAL
          </span>
        </div>
        <p className="text-xs text-slate-400 font-sans mt-0.5">
          Instead of pasting each document into every AI call, generate from the numbered requirements read out of them:
          each skill is shown only the requirements it tests, in batches that fit the AI model's window, and every case
          names the requirement it came from.
        </p>
      </div>

      <label className={`flex items-center gap-1.5 text-[11px] font-mono ${nothingToUse ? 'text-slate-500' : 'text-slate-300 cursor-pointer'}`}>
        <input
          type="checkbox"
          id="requirement-mode-toggle"
          // Shown off while there is nothing to generate from — that is what a
          // generation would then do — though the choice itself is kept, so a
          // document uploaded again finds it as it was left.
          checked={enabled && !nothingToUse}
          onChange={(e) => onEnabledChange(e.target.checked)}
          disabled={disabled || nothingToUse}
          className="accent-violet-600"
        />
        <span>Generate from the requirements of my uploaded documents</span>
      </label>

      {documents.length === 0 && (
        <p className="text-[11px] text-slate-500 font-sans flex items-start gap-1">
          <Info className="w-3 h-3 shrink-0 mt-0.5" />
          <span>Upload a document first — its requirements are read when it is uploaded.</span>
        </p>
      )}
      {documents.length > 0 && nothingToUse && (
        <p className="text-[11px] text-amber-300 font-sans flex items-start gap-1">
          <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
          <span>
            No uploaded document has requirements to generate from (an older backend, or a document that could not be
            read), so documents will be pasted as usual.
          </span>
        </p>
      )}

      {enabled && !nothingToUse && (
        <div className="space-y-2">
          <div className="text-[11px] font-mono text-slate-300">
            {usable.length} {plural(usable.length, 'document', 'documents')} ·{' '}
            {totalRequirements.toLocaleString()} {plural(totalRequirements, 'requirement', 'requirements')}
          </div>

          {leftOut.length > 0 && (
            <div className="text-[11px] text-amber-300 font-sans flex items-start gap-1">
              <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
              <span>
                Left out while this is on (no requirements were read from {plural(leftOut.length, 'it', 'them')}):{' '}
                {leftOut.map((d) => `${d.slotId} (${d.fileName})`).join(', ')}.
              </span>
            </div>
          )}

          {/* Phase 4 — the coverage top-up: opt-in, within this mode. */}
          <label className="flex items-center gap-1.5 text-[11px] font-mono text-slate-300 cursor-pointer">
            <input
              type="checkbox"
              id="coverage-top-up-toggle"
              checked={topUp}
              onChange={(e) => onTopUpChange(e.target.checked)}
              disabled={disabled}
              className="accent-violet-600"
            />
            <span>Fill coverage gaps with extra calls</span>
          </label>
          <p className="text-[11px] text-slate-500 font-sans">
            After the first pass, a requirement that is short of the depth's targets (say, it has no negative or edge
            case) is asked about again, one technique at a time, until every target is met, a round adds nothing, or the
            call limit is reached.
            {topUp && plan?.coverage
              ? ` Up to ${plan.coverage.top_up_rounds} ${plural(plan.coverage.top_up_rounds, 'round', 'rounds')} and ${plan.coverage.top_up_call_limit.toLocaleString()} more ${plural(plan.coverage.top_up_call_limit, 'call', 'calls')} in all.`
              : ''}
          </p>

          {planLoading && (
            <div className="flex items-center gap-1.5 text-[11px] font-mono text-violet-300">
              <Loader2 className="w-3 h-3 animate-spin" />
              <span>Working out the plan…</span>
            </div>
          )}

          {planError && !planLoading && (
            <div className="p-2 rounded bg-rose-950/40 border border-rose-800/80 text-[11px] font-mono text-rose-300">
              Could not work out the plan: {planError}
            </div>
          )}

          {plan && !planLoading && (
            <div id="requirement-plan" className="p-2.5 rounded bg-[#020617] border border-slate-800 space-y-1.5 text-[11px] font-mono text-slate-300">
              <div className="text-slate-100">
                {plan.requirements_total.toLocaleString()} {plural(plan.requirements_total, 'requirement', 'requirements')} →{' '}
                {plan.calls.toLocaleString()} AI {plural(plan.calls, 'call', 'calls')} (one request may make at most{' '}
                {plan.max_calls.toLocaleString()})
              </div>

              {plan.skills.length > 0 && (
                <ul className="space-y-0.5">
                  {plan.skills.map((s) => (
                    <li key={`${s.wave}-${s.skill_name}`} className="flex flex-wrap items-baseline gap-x-2">
                      <span className="text-slate-100">{s.skill_name}</span>
                      <span className="text-slate-500">{s.wave}</span>
                      <span>
                        {s.requirements.toLocaleString()} {plural(s.requirements, 'requirement', 'requirements')} ·{' '}
                        {s.batches.toLocaleString()} {plural(s.batches, 'batch', 'batches')}
                      </span>
                      {s.note && <span className="text-amber-300">{s.note}</span>}
                    </li>
                  ))}
                </ul>
              )}

              <div className="text-slate-400">
                {plan.provider_name
                  ? `Calls go to ${plan.provider_name}${plan.model ? ` / ${plan.model}` : ''} · about ${plan.budget_chars.toLocaleString()} characters of requirements per call${
                      plan.prompt_window_tokens ? ` (window ${plan.prompt_window_tokens.toLocaleString()} tokens)` : ''
                    }`
                  : 'No AI provider can serve generation right now.'}
              </div>

              {plan.prompt_window_tokens !== null && plan.prompt_window_tokens <= 4096 && (
                <div className="text-slate-500">
                  This model's window is small, so requirements go out in many small calls. A larger window (for a local
                  model, its context size — OLLAMA_NUM_CTX) needs fewer.
                </div>
              )}

              <div className="text-slate-500">
                Rough size: about {plan.estimated_prompt_tokens.toLocaleString()} tokens in and{' '}
                {plan.estimated_completion_tokens.toLocaleString()} out across all calls
                {topUp ? ' of the first pass (a top-up adds to it)' : ''}.
              </div>

              {plan.coverage && (
                <div id="coverage-plan" className="pt-1.5 border-t border-slate-800/80 space-y-1">
                  <div className="text-slate-100">
                    Coverage at {DEPTH_LABEL[plan.coverage.depth] ?? plan.coverage.depth} depth
                    <span className="text-slate-500">
                      {DEPTH_MEANING[plan.coverage.depth] ? ` — ${DEPTH_MEANING[plan.coverage.depth]}` : ''}. Change it under
                      Exploration Depth.
                    </span>
                  </div>
                  <ul className="space-y-0.5">
                    {plan.coverage.techniques.map((t) => (
                      <li key={t.technique} className="flex flex-wrap items-baseline gap-x-2">
                        <span className="text-slate-100 w-24">{techniqueLabel(t.technique)}</span>
                        <span>
                          {t.requirements.toLocaleString()} {plural(t.requirements, 'requirement', 'requirements')} ×{' '}
                          {t.per_requirement}
                        </span>
                        {t.provided_by ? (
                          <span className="text-slate-500">via {t.provided_by}</span>
                        ) : (
                          <span className="text-amber-300">
                            no selected skill provides it
                            {techniqueSkill(t.technique) ? ` — select ${techniqueSkill(t.technique)}` : ''}; it will not be
                            attempted
                          </span>
                        )}
                      </li>
                    ))}
                  </ul>
                </div>
              )}

              {notOffered > 0 && (
                <div className="text-amber-300 flex items-start gap-1">
                  <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
                  <span>
                    {notOffered.toLocaleString()} {plural(notOffered, 'requirement is', 'requirements are')} of a kind none
                    of the selected skills tests, so {plural(notOffered, 'it', 'they')} would not be sent to any:{' '}
                    {plan.not_offered_keys.slice(0, NAMED_KEYS).join(', ')}
                    {notOffered > NAMED_KEYS ? ` and ${(notOffered - NAMED_KEYS).toLocaleString()} more` : ''}. Select a
                    skill that covers {plural(notOffered, 'it', 'them')}.
                  </span>
                </div>
              )}

              {!plan.within_limit && (
                <div className="text-rose-300 flex items-start gap-1">
                  <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
                  <span>
                    This would take {plan.calls.toLocaleString()} AI calls, more than the {plan.max_calls.toLocaleString()} one
                    request may make. Select fewer skills or documents.
                  </span>
                </div>
              )}
            </div>
          )}

          <button
            type="button"
            id="btn-show-requirements"
            onClick={() => setShowList((v) => !v)}
            className="flex items-center gap-1 text-[11px] font-mono text-violet-400 hover:text-violet-300 cursor-pointer"
          >
            {showList ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
            <span>{showList ? 'Hide the requirements' : `Show the requirements (${totalRequirements.toLocaleString()})`}</span>
          </button>

          {showList && (
            <div id="requirement-list" className="space-y-1.5">
              {listLoading && (
                <div className="flex items-center gap-1.5 text-[11px] font-mono text-violet-300">
                  <Loader2 className="w-3 h-3 animate-spin" />
                  <span>Reading the requirements…</span>
                </div>
              )}
              {listError && !listLoading && (
                <div className="p-2 rounded bg-rose-950/40 border border-rose-800/80 text-[11px] font-mono text-rose-300">
                  {listError}
                </div>
              )}
              {listed && !listLoading && (
                <>
                  <input
                    type="text"
                    aria-label="Search the requirements"
                    placeholder="Search by id, kind, section or text"
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    className="w-full px-2 py-1 rounded bg-[#020617] border border-slate-700 focus:border-violet-500 text-[11px] font-mono text-slate-100 placeholder-slate-500 focus:outline-none"
                  />
                  <ul className="max-h-72 overflow-y-auto space-y-1 pr-1">
                    {matching.slice(0, LIST_LIMIT).map((r) => (
                      <li
                        key={`${r.document_id}-${r.ordinal}`}
                        className="p-1.5 rounded bg-[#020617] border border-slate-800/80 text-[11px] font-sans text-slate-300"
                      >
                        <div className="flex flex-wrap items-baseline gap-x-2 font-mono">
                          <span className="font-bold text-slate-100">{r.key}</span>
                          <span className="text-violet-300">{r.kind}</span>
                          {usable.length > 1 && <span className="text-slate-500">{slotOfDocument(r.document_id)}</span>}
                          {r.section && <span className="text-slate-500 truncate">{r.section}</span>}
                        </div>
                        <div className="break-words">{r.text}</div>
                      </li>
                    ))}
                  </ul>
                  <div className="text-[10px] font-mono text-slate-500">
                    {matching.length > LIST_LIMIT
                      ? `Showing the first ${LIST_LIMIT} of ${matching.length.toLocaleString()} matching requirements — search to narrow them.`
                      : `${matching.length.toLocaleString()} ${plural(matching.length, 'requirement', 'requirements')}${
                          needle ? ' match' : ''
                        }.`}
                  </div>
                </>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
