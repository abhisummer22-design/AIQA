/**
 * Phase 7 — making an autopilot run: the application, the documents, the roles, what to generate and how far to go, with the
 * estimate of exactly that beside the form (`POST .../autopilot/estimate`, worked out again a moment after each change).
 *
 * Making the run costs nothing and starts nothing: the backend stores it with its estimate, and it starts only when a person
 * accepts that estimate on the run's page. What could only fail later (an address or login steps a crawl would refuse, a variable
 * that is not set, no requirements) is refused here, in the backend's own words.
 */
import React, { useEffect, useMemo, useRef, useState } from 'react';
import { AlertTriangle, FilePlus2, Loader2, Rocket } from 'lucide-react';
import { ApiError, aiqaApi } from '../../services/aiqaApi';
import { useSkillSelection } from '../../hooks/useSkillSelection';
import { DEFAULT_SELECTED_SKILL_IDS } from '../../services/skillRegistry';
import type { AutopilotEstimate, AutopilotRun, AutopilotRunDetail, GenerateAiSkillInput, ListRequirementsResponse, RunListItem } from '../../types/contract';
import { EstimatePanel } from './EstimatePanel';
import { LoginRolesEditor } from './LoginRolesEditor';
import {
  ANONYMOUS_ROLE,
  DEFAULT_VERIFICATIONS,
  DEPTHS,
  MAX_CONCURRENCY,
  MAX_VERIFICATIONS,
  MIN_CONCURRENCY,
  RUN_STATE_LABEL,
  buildConfig,
  draftProblems,
  emptyDraft,
  estimateRequestFor,
  plural,
  type WizardDraft,
} from './autopilotModel';

const ESTIMATE_DELAY_MS = 400;
const CRAWL_RUN_PREFIX = 'Crawl:';

const Card: React.FC<{ id: string; title: string; hint?: string; children: React.ReactNode }> = ({ id, title, hint, children }) => (
  <section id={id} className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-2.5">
    <div>
      <h2 className="text-xs font-semibold text-slate-100">{title}</h2>
      {hint && <p className="text-[11px] text-slate-500">{hint}</p>}
    </div>
    {children}
  </section>
);

const INPUT_CLS = 'w-full rounded-md border border-slate-800 bg-[#020617] px-2 py-1.5 text-xs text-slate-200 outline-none focus:ring-1 focus:ring-violet-500 disabled:opacity-50';
const LABEL_CLS = 'text-[11px] text-slate-400';

interface Props {
  projectId: string;
  projectName?: string;
  onCreated: (detail: AutopilotRunDetail) => void;
  /** Sends the person to where requirement documents are uploaded. */
  onUploadDocuments?: () => void;
  /** A run of this project that has not ended: a project runs one at a time, so no other can be made until it has. */
  blockedBy?: Pick<AutopilotRun, 'id' | 'name' | 'state'> | null;
  onOpenBlocking?: () => void;
}

export const NewRunWizard: React.FC<Props> = ({ projectId, projectName, onCreated, onUploadDocuments, blockedBy = null, onOpenBlocking }) => {
  const ids = useRef(1);
  const nextId = () => ids.current++;
  const [draft, setDraft] = useState<WizardDraft>(() => emptyDraft());
  const patch = (changes: Partial<WizardDraft>) => setDraft((d) => ({ ...d, ...changes }));

  const skillSelection = useSkillSelection(DEFAULT_SELECTED_SKILL_IDS);
  const chosenSkills: GenerateAiSkillInput[] = useMemo(
    () =>
      skillSelection.skills
        .filter((s) => skillSelection.selectedSkillIds.includes(s.id))
        .map((s) => ({ id: s.id, name: s.name, description: s.description })),
    [skillSelection.skills, skillSelection.selectedSkillIds],
  );
  useEffect(() => {
    setDraft((d) => (JSON.stringify(d.skills) === JSON.stringify(chosenSkills) ? d : { ...d, skills: chosenSkills }));
  }, [chosenSkills]);

  // What the project has: its requirement documents and its finished crawls.
  const [requirements, setRequirements] = useState<ListRequirementsResponse | null>(null);
  const [requirementsError, setRequirementsError] = useState<string | null>(null);
  const [crawls, setCrawls] = useState<RunListItem[]>([]);
  useEffect(() => {
    const controller = new AbortController();
    setRequirements(null);
    setRequirementsError(null);
    aiqaApi.requirements
      .list(projectId, {}, controller.signal)
      .then(setRequirements)
      .catch((e) => {
        if (!controller.signal.aborted) setRequirementsError(e instanceof ApiError ? e.detail ?? e.message : 'The requirements could not be read.');
      });
    aiqaApi.executions
      .list(projectId, { limit: 50 }, controller.signal)
      .then((runs) => setCrawls(runs.filter((r) => r.name.startsWith(CRAWL_RUN_PREFIX) && r.state === 'COMPLETED')))
      .catch(() => setCrawls([]));
    return () => controller.abort();
  }, [projectId]);

  // The estimate of the form as it stands, asked for again a moment after each change.
  const [estimate, setEstimate] = useState<AutopilotEstimate | null>(null);
  const [estimating, setEstimating] = useState(false);
  const [estimateError, setEstimateError] = useState<string | null>(null);
  const request = useMemo(() => estimateRequestFor(draft), [draft]);
  const requestKey = JSON.stringify(request);
  useEffect(() => {
    const controller = new AbortController();
    setEstimating(true);
    const timer = setTimeout(() => {
      aiqaApi.autopilot
        .estimate(projectId, JSON.parse(requestKey), controller.signal)
        .then((e) => {
          setEstimate(e);
          setEstimateError(null);
        })
        .catch((e) => {
          if (controller.signal.aborted) return;
          setEstimate(null);
          setEstimateError(e instanceof ApiError ? e.detail ?? e.message : 'The estimate could not be made.');
        })
        .finally(() => {
          if (!controller.signal.aborted) setEstimating(false);
        });
    }, ESTIMATE_DELAY_MS);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [projectId, requestKey]);

  const problems = draftProblems(draft);
  const noDocuments = requirements !== null && requirements.documents.length === 0;
  const blocked = blockedBy !== null;
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);

  const create = async () => {
    setCreating(true);
    setCreateError(null);
    try {
      onCreated(await aiqaApi.autopilot.runs.create(projectId, buildConfig(draft)));
    } catch (e) {
      setCreateError(e instanceof ApiError ? e.detail ?? e.message : 'The run could not be made.');
    } finally {
      setCreating(false);
    }
  };

  const toggleDocument = (id: string) =>
    patch({ documentIds: draft.documentIds.includes(id) ? draft.documentIds.filter((d) => d !== id) : [...draft.documentIds, id] });

  // Two roles with one name is a mistake the form names below; the choice of who runs the cases lists that name once.
  const namedRoles = [...new Set(draft.roles.map((r) => r.name.trim()).filter(Boolean))];

  return (
    <div id="autopilot-wizard" className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,26rem)] items-start">
      <div className="space-y-4">
        <Card id="wizard-application" title="The application" hint={`What ${projectName ?? 'this project'} is tested against.`}>
          <label className={LABEL_CLS} htmlFor="wizard-base-url">
            Address of the application
          </label>
          <input id="wizard-base-url" value={draft.baseUrl} onChange={(e) => patch({ baseUrl: e.target.value })} placeholder="https://shop.example.test" className={INPUT_CLS} />
          <label className={LABEL_CLS} htmlFor="wizard-name">
            Name of the run (optional)
          </label>
          <input id="wizard-name" value={draft.name} onChange={(e) => patch({ name: e.target.value })} placeholder="Release 5 smoke" className={INPUT_CLS} />
        </Card>

        <Card id="wizard-documents" title="Requirements" hint="The run generates cases for the numbered requirements of these documents. None ticked: every document.">
          {requirementsError && (
            <p role="alert" className="text-[11px] text-rose-300">
              {requirementsError}
            </p>
          )}
          {requirements === null && !requirementsError && (
            <p className="flex items-center gap-1.5 text-[11px] text-slate-400">
              <Loader2 className="w-3.5 h-3.5 animate-spin" /> Reading the project’s documents…
            </p>
          )}
          {noDocuments && (
            <div id="wizard-no-documents" className="p-2 rounded bg-amber-950/20 border border-amber-800/50 text-[11px] text-amber-200 space-y-1.5">
              <p className="flex items-center gap-1.5">
                <AlertTriangle className="w-3.5 h-3.5" /> This project has no requirements yet: a run needs documents to read them from.
              </p>
              {onUploadDocuments && (
                <button type="button" id="wizard-upload" onClick={onUploadDocuments} className="inline-flex items-center gap-1 px-2 py-1 rounded border border-amber-700 hover:bg-amber-950/50 cursor-pointer">
                  <FilePlus2 className="w-3.5 h-3.5" /> Upload a document in the Test Case Generator
                </button>
              )}
            </div>
          )}
          {requirements && requirements.documents.length > 0 && (
            <ul className="space-y-1">
              {requirements.documents.map((doc) => (
                <li key={doc.id}>
                  <label className="inline-flex items-center gap-2 text-xs text-slate-200 cursor-pointer">
                    <input type="checkbox" id={`wizard-doc-${doc.id}`} checked={draft.documentIds.includes(doc.id)} onChange={() => toggleDocument(doc.id)} className="accent-violet-500" />
                    <span>{doc.file_name}</span>
                    {doc.doc_type && <span className="font-mono text-[10px] text-slate-500">{doc.doc_type}</span>}
                    <span className="text-[10px] text-slate-500">
                      {doc.requirements_count} {plural(doc.requirements_count, 'requirement', 'requirements')}
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card id="wizard-roles" title="Roles" hint="Who the application is crawled and tested as. Confirm every login step: the run does exactly these.">
          <LoginRolesEditor projectId={projectId} baseUrl={draft.baseUrl} roles={draft.roles} onChange={(roles) => patch({ roles })} nextId={nextId} />
          {crawls.length > 0 && (
            <div className="pt-1">
              <label className={LABEL_CLS} htmlFor="wizard-reuse-crawl">
                Crawl
              </label>
              <select id="wizard-reuse-crawl" value={draft.reuseCrawlId ?? ''} onChange={(e) => patch({ reuseCrawlId: e.target.value || null })} className={INPUT_CLS}>
                <option value="">Crawl the application again</option>
                {crawls.map((c) => (
                  <option key={c.run_id} value={c.run_id}>
                    Use {c.name}
                  </option>
                ))}
              </select>
            </div>
          )}
        </Card>

        <Card id="wizard-generation" title="What to generate">
          <div className="flex flex-wrap gap-x-4 gap-y-1">
            {skillSelection.skills
              .filter((s) => s.availability !== 'NOT_CONFIGURED')
              .map((s) => (
                <label key={s.id} className="inline-flex items-center gap-1.5 text-xs text-slate-200 cursor-pointer" title={s.description}>
                  <input type="checkbox" id={`wizard-skill-${s.id}`} checked={skillSelection.isSelected(s.id)} onChange={() => skillSelection.toggleSkill(s.id)} className="accent-violet-500" />
                  {s.name}
                </label>
              ))}
          </div>
          <div className="grid gap-2 sm:grid-cols-2">
            <div>
              <label className={LABEL_CLS} htmlFor="wizard-depth">
                Depth
              </label>
              <select id="wizard-depth" value={draft.depth} onChange={(e) => patch({ depth: e.target.value })} className={INPUT_CLS}>
                <option value="">Not chosen (the number of cases is only guessed)</option>
                {DEPTHS.map((d) => (
                  <option key={d.value} value={d.value}>
                    {d.label}: {d.hint}
                  </option>
                ))}
              </select>
            </div>
            <div className="space-y-1 pt-4">
              <label className="flex items-center gap-1.5 text-xs text-slate-200 cursor-pointer">
                <input id="wizard-top-up" type="checkbox" checked={draft.topUp} onChange={(e) => patch({ topUp: e.target.checked })} className="accent-violet-500" />
                fill the gaps the first pass leaves (more AI calls)
              </label>
              <label className="flex items-center gap-1.5 text-xs text-slate-200 cursor-pointer">
                <input id="wizard-focus" type="checkbox" checked={draft.focusRequirements} onChange={(e) => patch({ focusRequirements: e.target.checked })} className="accent-violet-500" />
                crawl the pages the requirements are about first
              </label>
              <label className="flex items-center gap-1.5 text-xs text-slate-200 cursor-pointer">
                <input
                  id="wizard-verify"
                  type="checkbox"
                  checked={draft.verifyGaps !== null}
                  onChange={(e) => patch({ verifyGaps: e.target.checked ? DEFAULT_VERIFICATIONS : null })}
                  className="accent-violet-500"
                />
                ask the AI about requirements the words left unclear
                {draft.verifyGaps !== null && (
                  <input
                    id="wizard-verify-count"
                    aria-label="How many requirements the AI may look at"
                    type="number"
                    min={1}
                    max={MAX_VERIFICATIONS}
                    value={draft.verifyGaps}
                    onChange={(e) => patch({ verifyGaps: Number(e.target.value) })}
                    className="w-16 rounded border border-slate-800 bg-[#020617] px-1 py-0.5 text-xs"
                  />
                )}
              </label>
            </div>
          </div>
        </Card>

        <Card id="wizard-running" title="Running the cases and the gate">
          <label className="flex items-center gap-1.5 text-xs text-slate-200 cursor-pointer">
            <input id="wizard-execute" type="checkbox" checked={draft.execute} onChange={(e) => patch({ execute: e.target.checked })} className="accent-violet-500" />
            run the approved cases against the application (off: they are only written and approved)
          </label>
          <div className="grid gap-2 sm:grid-cols-2">
            <div>
              <label className={LABEL_CLS} htmlFor="wizard-concurrency">
                Cases at a time
              </label>
              <input
                id="wizard-concurrency"
                type="number"
                min={MIN_CONCURRENCY}
                max={MAX_CONCURRENCY}
                value={draft.concurrency}
                onChange={(e) => patch({ concurrency: Number(e.target.value) })}
                disabled={!draft.execute}
                className={INPUT_CLS}
              />
            </div>
            <div>
              <label className={LABEL_CLS} htmlFor="wizard-execution-role">
                Run the cases as
              </label>
              <select id="wizard-execution-role" value={draft.executionRole} onChange={(e) => patch({ executionRole: e.target.value })} disabled={!draft.execute} className={INPUT_CLS}>
                <option value="">The visitor ({ANONYMOUS_ROLE})</option>
                {namedRoles.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            </div>
          </div>
          <fieldset className="space-y-1">
            <legend className={LABEL_CLS}>Approving the cases</legend>
            <label className="flex items-start gap-1.5 text-xs text-slate-200 cursor-pointer">
              <input type="radio" name="wizard-approval" id="wizard-approval-manual" checked={draft.approval === 'manual'} onChange={() => patch({ approval: 'manual' })} className="accent-violet-500 mt-0.5" />
              <span>A person approves them. The run waits for that; generated cases are not trusted unseen (recommended).</span>
            </label>
            <label className="flex items-start gap-1.5 text-xs text-slate-200 cursor-pointer">
              <input type="radio" name="wizard-approval" id="wizard-approval-auto" checked={draft.approval === 'auto_validated'} onChange={() => patch({ approval: 'auto_validated' })} className="accent-violet-500 mt-0.5" />
              <span>The run approves the cases that pass validation and goes straight on to run them.</span>
            </label>
          </fieldset>
        </Card>
      </div>

      <aside id="wizard-side" className="space-y-3 lg:sticky lg:top-4">
        <Card id="wizard-estimate" title="What this run would cost" hint="Worked out again after each change. Nothing is charged until you start the run.">
          {estimating && !estimate && (
            <p className="flex items-center gap-1.5 text-[11px] text-slate-400">
              <Loader2 className="w-3.5 h-3.5 animate-spin" /> Working it out…
            </p>
          )}
          {estimateError && (
            <p id="wizard-estimate-error" role="alert" className="text-[11px] text-rose-300">
              {estimateError}
            </p>
          )}
          {estimate && (
            <div className={estimating ? 'opacity-60' : ''} aria-busy={estimating}>
              <EstimatePanel estimate={estimate} />
            </div>
          )}
        </Card>

        <div className="space-y-2">
          {problems.length > 0 && (
            <ul id="wizard-problems" className="space-y-0.5">
              {problems.map((p) => (
                <li key={p} className="text-[11px] text-rose-300">
                  {p}
                </li>
              ))}
            </ul>
          )}
          {blockedBy && (
            <div id="wizard-blocked" className="p-2 rounded bg-amber-950/30 border border-amber-800/60 text-[11px] text-amber-200 space-y-1">
              <p>
                {projectName ?? 'This project'} has a run that has not ended: “{blockedBy.name}” ({RUN_STATE_LABEL[blockedBy.state].toLowerCase()}). It runs one at a
                time: let that one finish, or cancel it, before making another.
              </p>
              {onOpenBlocking && (
                <button type="button" id="wizard-open-blocking" onClick={onOpenBlocking} className="px-2 py-0.5 rounded border border-amber-700 hover:bg-amber-950/50 cursor-pointer">
                  Open that run
                </button>
              )}
            </div>
          )}
          {createError && (
            <p id="wizard-create-error" role="alert" className="p-2 rounded bg-rose-950/30 border border-rose-800/60 text-[11px] text-rose-200">
              {createError}
            </p>
          )}
          <button
            type="button"
            id="wizard-create"
            onClick={() => void create()}
            disabled={creating || problems.length > 0 || noDocuments || blocked}
            className="w-full inline-flex items-center justify-center gap-2 px-3 py-2 rounded-md bg-violet-600 hover:bg-violet-500 text-white text-xs font-semibold cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {creating ? <Loader2 className="w-4 h-4 animate-spin" /> : <Rocket className="w-4 h-4" />}
            Make the run
          </button>
          <p className="text-[10px] text-slate-500">Making a run stores it with this estimate and starts nothing: you accept the cost on the next page.</p>
        </div>
      </aside>
    </div>
  );
};
