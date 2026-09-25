/**
 * Phase 7 — everything the Autopilot page decides that is not drawing: what a step says about itself, when a run needs looking
 * at again, how a person's form becomes a run's settings and what is wrong with it, what an estimate says in words, and when a
 * run may be started. Pure, so it is tested without a screen (`autopilotModel.test.ts`).
 *
 * The rules that must agree with the backend (`autopilot.rs`: the limits, what accepting an estimate means; `execution_plan.rs`:
 * which values a login step may name) are written here as the backend states them and tested against its own messages.
 */
import type {
  AutopilotApprovalMode,
  AutopilotConfig,
  AutopilotEstimate,
  AutopilotEstimateRequest,
  AutopilotEvent,
  AutopilotRange,
  AutopilotRunDetail,
  AutopilotRunState,
  AutopilotStep,
  AutopilotStepName,
  AutopilotVerdict,
  CrawlRole,
  GenerateAiSkillInput,
  PlanAction,
} from '../../types/contract';

// ------------------------------------------------------------------------------------------------ limits

/** `autopilot::MAX_ROLES`: the anonymous visitor counts as one. */
export const MAX_ROLES = 20;
export const MIN_CONCURRENCY = 1;
export const MAX_CONCURRENCY = 8;
export const DEFAULT_CONCURRENCY = 2;
/** `gap_verify::MAX_VERIFICATIONS_LIMIT`. */
export const MAX_VERIFICATIONS = 60;
export const DEFAULT_VERIFICATIONS = 12;
export const MAX_NAME_CHARS = 80;
/** `execution_plan::secret_reference_problem`: a value written `env:NAME` may only name a variable that starts like this. */
export const SECRET_REFERENCE = /^env:\s*(AIQA_LOGIN_|AIQA_SECRET_)[A-Za-z0-9_]*\s*$/;

export const DEPTHS: ReadonlyArray<{ value: string; label: string; hint: string }> = [
  { value: 'SMOKE', label: 'Smoke', hint: 'One case for each requirement, by its main technique.' },
  { value: 'STANDARD', label: 'Standard', hint: 'Each requirement by each technique that applies to it.' },
  { value: 'EXHAUSTIVE', label: 'Exhaustive', hint: 'Twice what standard asks of each technique.' },
  { value: 'CHAOS_EXPLORATORY', label: 'Chaos / exploratory', hint: 'The most: for hunting what nobody wrote down.' },
];

// ------------------------------------------------------------------------------------------------ steps

export const STEP_ORDER: readonly AutopilotStepName[] = [
  'REQUIREMENTS',
  'CRAWL',
  'GAP_CHECK',
  'GENERATE',
  'VALIDATE',
  'APPROVE',
  'EXECUTE',
  'REPORT',
];

export const STEP_LABEL: Record<AutopilotStepName, { title: string; what: string }> = {
  REQUIREMENTS: { title: 'Requirements', what: 'The documents have requirements to test.' },
  CRAWL: { title: 'Crawl', what: 'The application is crawled as each role (or an earlier crawl is used).' },
  GAP_CHECK: { title: 'Gap check', what: 'The requirements are lined up against what the crawl found.' },
  GENERATE: { title: 'Generate', what: 'The AI writes test cases for the requirements, a group at a time.' },
  VALIDATE: { title: 'Validate', what: 'Each case is checked for structure.' },
  APPROVE: { title: 'Approve', what: 'The cases wait for a person to approve them.' },
  EXECUTE: { title: 'Execute', what: 'The approved cases are compiled and run against the application.' },
  REPORT: { title: 'Report', what: 'What the run found, in one place.' },
};

const asNumber = (value: unknown): number | null => (typeof value === 'number' && Number.isFinite(value) ? value : null);
const asArray = (value: unknown): unknown[] => (Array.isArray(value) ? value : []);
const asRecord = (value: unknown): Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : {};

export const plural = (n: number, one: string, many: string): string => (n === 1 ? one : many);

/** One line on what a step found (or has done so far), from the detail the backend keeps with it. `null`: nothing to say yet. */
export function stepSummary(step: Pick<AutopilotStep, 'name' | 'state' | 'detail' | 'error'>): string | null {
  const d = asRecord(step.detail);
  switch (step.name) {
    case 'REQUIREMENTS': {
      const requirements = asNumber(d.requirements);
      const documents = asNumber(d.documents);
      if (requirements === null) return null;
      return `${requirements} ${plural(requirements, 'requirement', 'requirements')} in ${documents ?? 0} ${plural(documents ?? 0, 'document', 'documents')}`;
    }
    case 'CRAWL': {
      if (d.reused === true) return 'Used a crawl that was already made';
      const pages = asNumber(d.pages);
      const roles = asArray(d.roles).length;
      if (pages !== null) return `${pages} ${plural(pages, 'page', 'pages')} seen as ${roles} ${plural(roles, 'role', 'roles')}`;
      return typeof d.crawl_run_id === 'string' ? 'Crawling…' : null;
    }
    case 'GAP_CHECK': {
      const summary = asRecord(d.summary);
      const total = asNumber(summary.requirements);
      if (total === null) return null;
      return `${asNumber(summary.found) ?? 0} found, ${asNumber(summary.weak) ?? 0} weak, ${asNumber(summary.missing) ?? 0} with no sign, of ${total}`;
    }
    case 'GENERATE': {
      const groups = asArray(d.groups);
      const cases = asNumber(d.cases_created) ?? asNumber(d.cases);
      if (groups.length === 0 && cases === null) return null;
      const saved = groups.filter((g) => asRecord(g).state === 'SAVED').length;
      const inProgress = step.state === 'RUNNING' || step.state === 'FAILED';
      if (inProgress && groups.length > 0) {
        return `${cases ?? 0} ${plural(cases ?? 0, 'case', 'cases')} so far · group ${saved} of ${groups.length} done`;
      }
      return `${cases ?? 0} ${plural(cases ?? 0, 'case', 'cases')} in ${groups.length} ${plural(groups.length, 'group', 'groups')}`;
    }
    case 'VALIDATE': {
      const validated = asNumber(d.validated);
      if (validated === null) return null;
      const invalid = asArray(d.invalid).length;
      return invalid > 0 ? `${validated} validated, ${invalid} need fixing` : `${validated} validated`;
    }
    case 'APPROVE': {
      const pending = asNumber(d.pending);
      const approved = asNumber(d.approved);
      if (pending === null && approved === null) return null;
      const parts = [`${approved ?? 0} approved`];
      if ((asNumber(d.rejected) ?? 0) > 0) parts.push(`${asNumber(d.rejected)} rejected`);
      if ((pending ?? 0) > 0) parts.push(`${pending} waiting`);
      return parts.join(', ');
    }
    case 'EXECUTE': {
      if (d.execute === false) return 'Not asked for';
      const total = asNumber(d.total);
      if (total === null) return null;
      const counts = asRecord(d.counts);
      const finished = asNumber(d.finished) ?? 0;
      return `${finished} of ${total} run: ${asNumber(counts.PASSED) ?? 0} passed, ${asNumber(counts.FAILED) ?? 0} failed`;
    }
    case 'REPORT': {
      const summary = asRecord(d.summary);
      const findings = asNumber(summary.findings);
      return findings === null ? null : `${findings} ${plural(findings, 'finding', 'findings')}`;
    }
  }
}

// ------------------------------------------------------------------------------------------------ run state

export const RUN_STATE_LABEL: Record<AutopilotRunState, string> = {
  AWAITING_APPROVAL: 'Awaiting approval',
  RUNNING: 'Running',
  WAITING: 'Waiting for you',
  COMPLETED: 'Completed',
  FAILED: 'Failed',
  CANCELLED: 'Cancelled',
};

export const isEnded = (state: AutopilotRunState): boolean => state === 'COMPLETED' || state === 'FAILED' || state === 'CANCELLED';

/** How long to wait before looking at a run again, or `null` when it will not change on its own. A running run is looked at
 *  often; one waiting for a person only in case someone else acted; an ended one never. */
export function pollDelayMs(state: AutopilotRunState): number | null {
  if (state === 'RUNNING') return 2_000;
  if (state === 'WAITING' || state === 'AWAITING_APPROVAL') return 10_000;
  return null;
}

/** Which panel a run's page shows first. */
export type RunPhase = 'estimate' | 'running' | 'gate' | 'failed' | 'cancelled' | 'completed';

export function runPhase(detail: Pick<AutopilotRunDetail, 'run' | 'steps'>): RunPhase {
  switch (detail.run.state) {
    case 'AWAITING_APPROVAL':
      return 'estimate';
    case 'RUNNING':
      return 'running';
    case 'WAITING':
      return 'gate';
    case 'FAILED':
      return 'failed';
    case 'CANCELLED':
      return 'cancelled';
    case 'COMPLETED':
      return 'completed';
  }
}

/** The step a person should look at: the one that failed or waits, else the one running, else the last one that finished. */
export function focusStep(steps: readonly AutopilotStep[]): AutopilotStep | null {
  const by = (states: string[]) => steps.find((s) => states.includes(s.state));
  const settled = [...steps].reverse().find((s) => s.state === 'DONE' || s.state === 'SKIPPED');
  return by(['FAILED', 'WAITING']) ?? by(['RUNNING']) ?? settled ?? null;
}

/** New events added to what is shown: by id, once each, oldest first. */
export function mergeEvents(shown: readonly AutopilotEvent[], incoming: readonly AutopilotEvent[]): AutopilotEvent[] {
  const byId = new Map<number, AutopilotEvent>();
  for (const event of shown) byId.set(event.id, event);
  for (const event of incoming) byId.set(event.id, event);
  return [...byId.values()].sort((a, b) => a.id - b.id);
}

export const lastEventId = (events: readonly AutopilotEvent[]): number | null =>
  events.length === 0 ? null : events.reduce((most, e) => Math.max(most, e.id), events[0].id);

// ------------------------------------------------------------------------------------------------ the estimate in words

export function formatSeconds(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)} s`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${hours} h` : `${hours} h ${rest} min`;
}

/** `3` when the two are equal, else `3–8`. */
export function formatRange(range: AutopilotRange, format: (n: number) => string = (n) => String(n)): string {
  return range.min === range.max ? format(range.min) : `${format(range.min)}–${format(range.max)}`;
}

export const formatDurationRange = (range: AutopilotRange): string => formatRange(range, formatSeconds);

/** Dollars to the hundredth of a cent, without trailing zeros past the cent. */
export function formatUsd(usd: number): string {
  const trimmed = usd.toFixed(4).replace(/0+$/, '');
  const decimals = trimmed.length - trimmed.indexOf('.') - 1;
  return `$${decimals < 2 ? usd.toFixed(2) : trimmed}`;
}

export function formatCost(range: AutopilotRange | null): string {
  if (range === null) return 'not known (no provider to price it)';
  if (range.min === 0 && range.max === 0) return '$0 (free providers)';
  return formatRange(range, formatUsd);
}

export const formatTokens = (n: number): string => (n >= 1000 ? `${(n / 1000).toFixed(n >= 10_000 ? 0 : 1)}k` : String(n));

/** What a task's latency is based on, in words. */
export function latencyLabel(latency: AutopilotEstimate['tasks'][number]['latency']): string {
  switch (latency.basis) {
    case 'MEASURED':
      return `${latency.avg_seconds} s a call, measured over ${latency.calls} earlier calls`;
    case 'ASSUMED':
      return `assumed ${latency.tokens_per_second} tokens a second (nothing measured yet)`;
    case 'UNKNOWN':
      return 'no provider can serve this';
  }
}

// ------------------------------------------------------------------------------------------------ accepting the estimate

/** Whether the Start button may be pressed, and what is still missing, by the backend's own rule (`autopilot::check_acceptance`):
 *  the estimate always, the cost as well when a provider that charges is among those the run would call. An estimate that does not
 *  say is taken to cost money. */
export function acceptanceState(
  estimate: Pick<AutopilotEstimate, 'totals'> | null | undefined,
  acceptedEstimate: boolean,
  acceptedPaid: boolean,
): { paid: boolean; canStart: boolean; missing: string | null } {
  const paid = estimate?.totals?.paid ?? true;
  if (!acceptedEstimate) return { paid, canStart: false, missing: 'Tick that you have read what the run would cost.' };
  if (paid && !acceptedPaid) return { paid, canStart: false, missing: 'This run would call a provider that charges: tick that you accept that cost.' };
  return { paid, canStart: true, missing: null };
}

// ------------------------------------------------------------------------------------------------ the form

export type LoginStepType = 'NAVIGATE' | 'FILL' | 'CLICK';

export interface LoginStepDraft {
  localId: number;
  type: LoginStepType;
  url: string;
  selector: string;
  value: string;
}

export interface RoleDraft {
  localId: number;
  name: string;
  steps: LoginStepDraft[];
}

export interface WizardDraft {
  name: string;
  baseUrl: string;
  /** Empty: every document of the project that has requirements. */
  documentIds: string[];
  /** The roles that need a login; the anonymous visitor is always the first role and is not listed here. */
  roles: RoleDraft[];
  /** A finished crawl to use instead of crawling again. */
  reuseCrawlId: string | null;
  skills: GenerateAiSkillInput[];
  /** `''`: no depth chosen (the number of cases is then only guessed). */
  depth: string;
  topUp: boolean;
  focusRequirements: boolean;
  /** `null`: the AI does not look at unclear requirements. */
  verifyGaps: number | null;
  execute: boolean;
  concurrency: number;
  approval: AutopilotApprovalMode;
  /** `''`: the visitor when there is one (the backend's default). */
  executionRole: string;
}

export const emptyDraft = (): WizardDraft => ({
  name: '',
  baseUrl: '',
  documentIds: [],
  roles: [],
  reuseCrawlId: null,
  skills: [],
  depth: 'STANDARD',
  topUp: false,
  focusRequirements: false,
  verifyGaps: null,
  execute: true,
  concurrency: DEFAULT_CONCURRENCY,
  approval: 'manual',
  executionRole: '',
});

export const ANONYMOUS_ROLE = 'anonymous';

function toPlanAction(step: LoginStepDraft): PlanAction {
  switch (step.type) {
    case 'NAVIGATE':
      return { type: 'NAVIGATE', url: step.url.trim() };
    case 'CLICK':
      return { type: 'CLICK', selector: step.selector.trim() };
    case 'FILL':
      return { type: 'FILL', selector: step.selector.trim(), value: step.value };
  }
}

export const toCrawlRole = (role: RoleDraft): CrawlRole => ({ name: role.name.trim(), login_actions: role.steps.map(toPlanAction) });

/** A role's login steps as the drafts a person edits (what a suggestion becomes when they take it). */
export function stepsFromActions(actions: PlanAction[], nextId: () => number): LoginStepDraft[] {
  const steps: LoginStepDraft[] = [];
  for (const action of actions) {
    const base = { localId: nextId(), url: '', selector: '', value: '' };
    if (action.type === 'NAVIGATE') steps.push({ ...base, type: 'NAVIGATE', url: action.url });
    else if (action.type === 'CLICK') steps.push({ ...base, type: 'CLICK', selector: action.selector });
    else if (action.type === 'FILL') steps.push({ ...base, type: 'FILL', selector: action.selector, value: action.value });
  }
  return steps;
}

/** What is wrong with one role's login steps, before a run that could only fail at its crawl is made. The backend is the authority
 *  (address, duplicate names, unset variables); this catches what it would only meet once the crawl runs. */
export function roleProblem(role: RoleDraft): string | null {
  const name = role.name.trim();
  if (!name) return 'Every role needs a name.';
  if (name.toLowerCase() === ANONYMOUS_ROLE) return `"${ANONYMOUS_ROLE}" is the visitor who has not logged in: it is always included.`;
  if (role.steps.length === 0) return `Role "${name}" needs at least one login step.`;
  for (const step of role.steps) {
    if (step.type === 'NAVIGATE' && !step.url.trim()) return `Role "${name}": a NAVIGATE step needs a URL.`;
    if ((step.type === 'FILL' || step.type === 'CLICK') && !step.selector.trim()) return `Role "${name}": a ${step.type} step needs a selector.`;
    if (step.type === 'FILL' && !step.value) return `Role "${name}": a FILL step needs a value.`;
    if (step.type === 'FILL' && step.value.startsWith('env:') && !SECRET_REFERENCE.test(step.value)) {
      return `Role "${name}": a value starting env: must name a variable that starts AIQA_LOGIN_ or AIQA_SECRET_ (for example env:AIQA_LOGIN_ADMIN_PASSWORD).`;
    }
  }
  return null;
}

export const isHttpAddress = (text: string): boolean => {
  try {
    const url = new URL(text.trim());
    return url.protocol === 'http:' || url.protocol === 'https:';
  } catch {
    return false;
  }
};

/** Everything that is wrong with the form, each in words. Empty: it may be sent (the backend still has the last word). */
export function draftProblems(draft: WizardDraft): string[] {
  const problems: string[] = [];
  if (!draft.baseUrl.trim()) problems.push('Give the address of the application to test.');
  else if (!isHttpAddress(draft.baseUrl)) problems.push('The address of the application must start with http:// or https://.');
  if (draft.name.trim().length > MAX_NAME_CHARS) problems.push(`The name may be at most ${MAX_NAME_CHARS} characters.`);
  if (draft.roles.length + 1 > MAX_ROLES) problems.push(`A run may use at most ${MAX_ROLES} roles, the visitor included.`);
  const seen = new Set<string>();
  for (const role of draft.roles) {
    const problem = roleProblem(role);
    if (problem) problems.push(problem);
    const key = role.name.trim().toLowerCase();
    if (key && seen.has(key)) problems.push(`Two roles are called "${role.name.trim()}".`);
    seen.add(key);
  }
  const chosenRole = draft.executionRole.trim();
  if (chosenRole && chosenRole !== ANONYMOUS_ROLE && !draft.roles.some((r) => r.name.trim() === chosenRole)) {
    problems.push(`The cases are to run as "${chosenRole}", which is not one of the roles.`);
  }
  if (draft.topUp && !draft.depth) problems.push('Filling gaps needs a depth to fill up to: choose one.');
  if (draft.verifyGaps !== null && (draft.verifyGaps < 1 || draft.verifyGaps > MAX_VERIFICATIONS)) {
    problems.push(`The AI may look at between 1 and ${MAX_VERIFICATIONS} unclear requirements.`);
  }
  if (draft.concurrency < MIN_CONCURRENCY || draft.concurrency > MAX_CONCURRENCY) {
    problems.push(`Run between ${MIN_CONCURRENCY} and ${MAX_CONCURRENCY} cases at a time.`);
  }
  return problems;
}

/** The form as the settings of a run (`autopilot.rs::AutopilotConfig`). */
export function buildConfig(draft: WizardDraft): AutopilotConfig {
  const name = draft.name.trim();
  const executionRole = draft.executionRole.trim();
  const config: AutopilotConfig = {
    base_url: draft.baseUrl.trim(),
    roles: [{ name: ANONYMOUS_ROLE, login_actions: [] }, ...draft.roles.map(toCrawlRole)],
    requirement_document_ids: draft.documentIds,
    skills: draft.skills,
    coverage_top_up: draft.topUp,
    focus_requirements: draft.focusRequirements,
    execute: draft.execute,
    run_concurrency: draft.concurrency,
    approval: draft.approval,
  };
  if (name) config.name = name;
  if (draft.reuseCrawlId) config.crawl_run_id = draft.reuseCrawlId;
  if (draft.depth) config.depth = draft.depth;
  if (draft.verifyGaps !== null) config.verify_gaps = draft.verifyGaps;
  if (executionRole && executionRole !== ANONYMOUS_ROLE) config.execution_role = executionRole;
  return config;
}

/** What an estimate of the form is asked for (`POST .../autopilot/estimate`). */
export function estimateRequestFor(draft: WizardDraft): AutopilotEstimateRequest {
  const request: AutopilotEstimateRequest = {
    requirement_document_ids: draft.documentIds,
    skills: draft.skills,
    coverage_top_up: draft.topUp,
    roles: draft.roles.length + 1,
    focus_requirements: draft.focusRequirements,
    execute: draft.execute,
    run_concurrency: draft.concurrency,
  };
  if (draft.depth) request.depth = draft.depth;
  if (draft.verifyGaps !== null) request.verify_gaps = draft.verifyGaps;
  return request;
}

// ------------------------------------------------------------------------------------------------ the gate and the report

export interface CaseRow {
  id: string;
  title: string;
  status: string;
  requirement_reference?: string | null;
}

/** How many of the run's cases are where in their lifecycle. */
export function tallyCases(cases: readonly Pick<CaseRow, 'status'>[]): Record<'undecided' | 'validated' | 'approved' | 'rejected' | 'executed', number> {
  const counts = { undecided: 0, validated: 0, approved: 0, rejected: 0, executed: 0 };
  for (const c of cases) {
    if (c.status === 'GENERATED') counts.undecided += 1;
    else if (c.status === 'VALIDATED') {
      counts.validated += 1;
      counts.undecided += 1;
    } else if (c.status === 'APPROVED') counts.approved += 1;
    else if (c.status === 'REJECTED') counts.rejected += 1;
    else if (c.status === 'EXECUTED') counts.executed += 1;
  }
  return counts;
}

export const VERDICT_ORDER: readonly AutopilotVerdict[] = ['FAILED', 'INCONCLUSIVE', 'NOT_EXECUTED', 'NO_CASES', 'PASSED'];

export const VERDICT_LABEL: Record<AutopilotVerdict, string> = {
  PASSED: 'Passed',
  FAILED: 'Failed',
  INCONCLUSIVE: 'Inconclusive',
  NOT_EXECUTED: 'Not run',
  NO_CASES: 'No case',
};

/** The step's own words for what a case's outcome means. */
export function outcomeLabel(outcome: string): string {
  switch (outcome) {
    case 'PASSED':
      return 'Passed';
    case 'FAILED':
      return 'Failed';
    case 'INFRA_ERROR':
      return 'Infrastructure error';
    case 'TIMEOUT':
      return 'Timed out';
    case 'NEEDS_REVIEW':
      return 'Needs review';
    case 'NOT_EXECUTABLE':
      return 'Cannot be run: not enough to compile';
    case 'COMPILATION_FAILED':
      return 'Could not be compiled';
    case 'COMPILATION_UNAVAILABLE':
      return 'No AI available to compile it';
    case 'NOT_RUN':
      return 'Not run';
    case 'UNFINISHED':
      return 'Did not finish in time';
    case 'CANCELLED':
      return 'Cancelled';
    default:
      return outcome;
  }
}

/** The words for a case that is a requirement's evidence: what a person reads in a row of the report. */
const VERDICT_WORD: Record<AutopilotVerdict, string> = {
  FAILED: 'failed',
  INCONCLUSIVE: 'inconclusive',
  NOT_EXECUTED: 'not run',
  NO_CASES: 'without a case',
  PASSED: 'passed',
};

/** How the requirements came out, in one sentence: the worst first. */
export function coverageLine(summary: Partial<Record<AutopilotVerdict, number>>): string {
  const total = VERDICT_ORDER.reduce((n, verdict) => n + (summary[verdict] ?? 0), 0);
  if (total === 0) return 'No requirement was looked at.';
  const parts = VERDICT_ORDER.filter((verdict) => (summary[verdict] ?? 0) > 0).map((verdict) => `${summary[verdict]} ${VERDICT_WORD[verdict]}`);
  return `Of ${total} ${plural(total, 'requirement', 'requirements')}: ${parts.join(', ')}.`;
}

export type Tone = 'good' | 'warn' | 'bad' | 'plain';

/** How the report reads overall: `bad` when a requirement failed, `warn` when none did but some could not be answered (not run,
 *  no case, inconclusive), `good` when every requirement passed, `plain` when there is nothing to say. */
export function reportTone(summary: Partial<Record<AutopilotVerdict, number>>): Tone {
  const count = (verdict: AutopilotVerdict) => summary[verdict] ?? 0;
  if (count('FAILED') > 0) return 'bad';
  if (count('INCONCLUSIVE') + count('NOT_EXECUTED') + count('NO_CASES') > 0) return 'warn';
  return count('PASSED') > 0 ? 'good' : 'plain';
}

/** The colour of a line of the timeline, from the kind of event it is. */
export function eventTone(kind: string): Tone {
  if (kind === 'run.failed' || kind === 'step.failed') return 'bad';
  if (kind === 'step.waiting' || kind === 'run.retried' || kind === 'run.cancelled') return 'warn';
  if (kind === 'run.completed' || kind === 'step.done') return 'good';
  return 'plain';
}

// ------------------------------------------------------------------------------------------------ small things

/** The words to show for a call that failed: the backend's own message when it gave one. */
export const errorText = (e: unknown, fallback: string): string => (e instanceof Error && e.message ? e.message : fallback);

/** `2026-09-26T10:15:03.123Z` as `2026-09-26 10:15:03` (UTC: the same everywhere, so a screen and its test agree). */
export function formatTimestamp(iso: string | null | undefined): string {
  if (!iso) return '';
  const match = /^(\d{4}-\d{2}-\d{2})[T ](\d{2}:\d{2}:\d{2})/.exec(iso);
  return match ? `${match[1]} ${match[2]}` : iso;
}

/** Seconds a run took, from its own times when the report does not say. `null`: not started, or not ended. */
export function secondsBetween(from: string | null | undefined, to: string | null | undefined): number | null {
  if (!from || !to) return null;
  const a = Date.parse(from);
  const b = Date.parse(to);
  return Number.isFinite(a) && Number.isFinite(b) && b >= a ? Math.round((b - a) / 1000) : null;
}

/** The technique a generated case was written for, from its tags (`technique:<name>`). */
export function techniqueOf(tags: readonly string[]): string | null {
  const tag = tags.find((t) => t.startsWith('technique:'));
  return tag ? tag.slice('technique:'.length) : null;
}

/** The suite the run's cases went into, once the GENERATE step has made it. */
export function suiteIdOf(steps: readonly Pick<AutopilotStep, 'name' | 'detail'>[]): string | null {
  const generate = steps.find((s) => s.name === 'GENERATE');
  const id = asRecord(generate?.detail).suite_id;
  return typeof id === 'string' && id ? id : null;
}
