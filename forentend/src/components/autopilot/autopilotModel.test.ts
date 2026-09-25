/**
 * Phase 7 — the rules the Autopilot page decides by, tested without a screen.
 */
import { describe, expect, it } from 'vitest';
import type { AutopilotEstimate, AutopilotEvent, AutopilotStep } from '../../types/contract';
import {
  ANONYMOUS_ROLE,
  DEFAULT_CONCURRENCY,
  DEFAULT_VERIFICATIONS,
  DEPTHS,
  MAX_CONCURRENCY,
  MAX_NAME_CHARS,
  MAX_ROLES,
  MAX_VERIFICATIONS,
  MIN_CONCURRENCY,
  RUN_STATE_LABEL,
  SECRET_REFERENCE,
  STEP_LABEL,
  STEP_ORDER,
  VERDICT_LABEL,
  VERDICT_ORDER,
  acceptanceState,
  buildConfig,
  coverageLine,
  draftProblems,
  emptyDraft,
  errorText,
  estimateRequestFor,
  eventTone,
  focusStep,
  formatCost,
  formatDurationRange,
  formatRange,
  formatSeconds,
  formatTimestamp,
  formatTokens,
  formatUsd,
  isEnded,
  isHttpAddress,
  lastEventId,
  latencyLabel,
  mergeEvents,
  outcomeLabel,
  plural,
  pollDelayMs,
  providerLabel,
  reportTone,
  roleProblem,
  runPhase,
  secondsBetween,
  stepSummary,
  stepsFromActions,
  suiteIdOf,
  tallyCases,
  techniqueOf,
  toCrawlRole,
  withProviderNames,
  type RoleDraft,
  type WizardDraft,
} from './autopilotModel';

const step = (name: AutopilotStep['name'], state: AutopilotStep['state'], detail: Record<string, unknown> = {}): AutopilotStep => ({
  run_id: 'r',
  name,
  position: 0,
  state,
  attempts: 1,
  detail,
  error: null,
  started_at: null,
  finished_at: null,
});

const summary = (name: AutopilotStep['name'], state: AutopilotStep['state'], detail: Record<string, unknown>) => stepSummary(step(name, state, detail));

const event = (id: number, message = `event ${id}`): AutopilotEvent => ({ id, at: '2026-09-25T10:00:00Z', kind: 'note', step: null, message, data: null });

const loginRole = (name = 'admin', overrides: Partial<RoleDraft> = {}): RoleDraft => ({
  localId: 1,
  name,
  steps: [
    { localId: 1, type: 'NAVIGATE', url: 'https://shop.example.test/login', selector: '', value: '' },
    { localId: 2, type: 'FILL', selector: '#user', url: '', value: 'env:AIQA_LOGIN_ADMIN_USERNAME' },
    { localId: 3, type: 'FILL', selector: '#pass', url: '', value: 'env:AIQA_LOGIN_ADMIN_PASSWORD' },
    { localId: 4, type: 'CLICK', selector: '#go', url: '', value: '' },
  ],
  ...overrides,
});

const draft = (overrides: Partial<WizardDraft> = {}): WizardDraft => ({ ...emptyDraft(), baseUrl: 'https://shop.example.test', ...overrides });

describe('the limits are the backend’s own', () => {
  it('are pinned', () => {
    expect(MAX_ROLES).toBe(20);
    expect([MIN_CONCURRENCY, MAX_CONCURRENCY, DEFAULT_CONCURRENCY]).toEqual([1, 8, 2]);
    expect([MAX_VERIFICATIONS, DEFAULT_VERIFICATIONS, MAX_NAME_CHARS]).toEqual([60, 12, 80]);
    expect(DEPTHS.map((d) => d.value)).toEqual(['SMOKE', 'STANDARD', 'EXHAUSTIVE', 'CHAOS_EXPLORATORY']);
  });

  it('a secret reference may name only a variable that starts AIQA_LOGIN_ or AIQA_SECRET_', () => {
    for (const ok of ['env:AIQA_LOGIN_ADMIN_PASSWORD', 'env:AIQA_SECRET_TOKEN', 'env: AIQA_LOGIN_X ', 'env:AIQA_LOGIN_']) {
      expect(SECRET_REFERENCE.test(ok), ok).toBe(true);
    }
    for (const bad of ['env:HOME', 'env:aiqa_login_x', 'AIQA_LOGIN_X', 'env:AIQA_LOGIN_A B', 'env:AIQA_LOGIN_A-B']) {
      expect(SECRET_REFERENCE.test(bad), bad).toBe(false);
    }
  });
});

describe('the steps', () => {
  it('are the backend’s eight, in order, each with a title and what it does', () => {
    expect(STEP_ORDER).toEqual(['REQUIREMENTS', 'CRAWL', 'GAP_CHECK', 'GENERATE', 'VALIDATE', 'APPROVE', 'EXECUTE', 'REPORT']);
    for (const name of STEP_ORDER) {
      expect(STEP_LABEL[name].title.length).toBeGreaterThan(2);
      expect(STEP_LABEL[name].what.endsWith('.')).toBe(true);
    }
  });

  it('say what the requirements step found, in the singular and the plural', () => {
    expect(summary('REQUIREMENTS', 'DONE', { requirements: 6, documents: 2 })).toBe('6 requirements in 2 documents');
    expect(summary('REQUIREMENTS', 'DONE', { requirements: 1, documents: 1 })).toBe('1 requirement in 1 document');
    expect(summary('REQUIREMENTS', 'DONE', { requirements: 3 })).toBe('3 requirements in 0 documents');
    expect(summary('REQUIREMENTS', 'RUNNING', {})).toBeNull();
    expect(summary('REQUIREMENTS', 'DONE', { requirements: 'six' })).toBeNull();
    expect(summary('REQUIREMENTS', 'DONE', { requirements: Number.NaN })).toBeNull();
  });

  it('say what the crawl saw, that it is going, or that an earlier one was used', () => {
    expect(summary('CRAWL', 'DONE', { crawl_run_id: 'c', reused: true })).toBe('Used a crawl that was already made');
    expect(summary('CRAWL', 'DONE', { pages: 12, roles: [{}, {}] })).toBe('12 pages seen as 2 roles');
    expect(summary('CRAWL', 'DONE', { pages: 1, roles: [{}] })).toBe('1 page seen as 1 role');
    expect(summary('CRAWL', 'RUNNING', { crawl_run_id: 'c', reused: false })).toBe('Crawling…');
    expect(summary('CRAWL', 'PENDING', {})).toBeNull();
  });

  it('say how many requirements the crawl showed', () => {
    expect(summary('GAP_CHECK', 'DONE', { summary: { requirements: 10, found: 6, weak: 2, missing: 2 } })).toBe('6 found, 2 weak, 2 with no sign, of 10');
    expect(summary('GAP_CHECK', 'DONE', { summary: { requirements: 3 } })).toBe('0 found, 0 weak, 0 with no sign, of 3');
    expect(summary('GAP_CHECK', 'DONE', {})).toBeNull();
  });

  it('say how far the cases have got while generating and what they came to after', () => {
    const groups = [{ state: 'SAVED' }, { state: 'SAVED' }, { state: 'PENDING' }];
    expect(summary('GENERATE', 'RUNNING', { groups, cases_created: 24 })).toBe('24 cases so far · group 2 of 3 done');
    expect(summary('GENERATE', 'FAILED', { groups, cases_created: 1 })).toBe('1 case so far · group 2 of 3 done');
    expect(summary('GENERATE', 'DONE', { groups, cases: 36 })).toBe('36 cases in 3 groups');
    expect(summary('GENERATE', 'DONE', { groups: [{ state: 'SAVED' }], cases: 1 })).toBe('1 case in 1 group');
    expect(summary('GENERATE', 'DONE', { groups: [], cases: 0 })).toBe('0 cases in 0 groups');
    expect(summary('GENERATE', 'PENDING', {})).toBeNull();
    expect(summary('GENERATE', 'RUNNING', { groups: [], cases_created: 0 })).toBe('0 cases in 0 groups');
  });

  it('say how many cases were validated and how many need fixing', () => {
    expect(summary('VALIDATE', 'DONE', { validated: 5, invalid: [] })).toBe('5 validated');
    expect(summary('VALIDATE', 'DONE', { validated: 5, invalid: [{}, {}] })).toBe('5 validated, 2 need fixing');
    expect(summary('VALIDATE', 'PENDING', {})).toBeNull();
  });

  it('say how many cases are approved, rejected and waiting', () => {
    expect(summary('APPROVE', 'WAITING', { approved: 1, rejected: 0, pending: 4 })).toBe('1 approved, 4 waiting');
    expect(summary('APPROVE', 'DONE', { approved: 5, rejected: 2, pending: 0 })).toBe('5 approved, 2 rejected');
    expect(summary('APPROVE', 'WAITING', { pending: 3 })).toBe('0 approved, 3 waiting');
    expect(summary('APPROVE', 'PENDING', {})).toBeNull();
  });

  it('say how many cases have been run and how they came out', () => {
    expect(summary('EXECUTE', 'RUNNING', { total: 5, finished: 2, counts: { PASSED: 1, FAILED: 1 } })).toBe('2 of 5 run: 1 passed, 1 failed');
    expect(summary('EXECUTE', 'DONE', { total: 0, finished: 0, counts: {} })).toBe('0 of 0 run: 0 passed, 0 failed');
    expect(summary('EXECUTE', 'SKIPPED', { execute: false })).toBe('Not asked for');
    expect(summary('EXECUTE', 'PENDING', {})).toBeNull();
  });

  it('say how many findings the report has', () => {
    expect(summary('REPORT', 'DONE', { summary: { findings: 1 } })).toBe('1 finding');
    expect(summary('REPORT', 'DONE', { summary: { findings: 0 } })).toBe('0 findings');
    expect(summary('REPORT', 'DONE', { summary: { findings: 4 } })).toBe('4 findings');
    expect(summary('REPORT', 'PENDING', {})).toBeNull();
  });

  it('plural chooses by exactly one', () => {
    expect(plural(1, 'case', 'cases')).toBe('case');
    expect(plural(0, 'case', 'cases')).toBe('cases');
    expect(plural(2, 'case', 'cases')).toBe('cases');
  });
});

describe('a run’s state', () => {
  it('is worded, and only completed, failed and cancelled are ended', () => {
    expect(RUN_STATE_LABEL.WAITING).toBe('Waiting for you');
    expect(RUN_STATE_LABEL.AWAITING_APPROVAL).toBe('Awaiting cost approval');
    for (const [state, ended] of [['AWAITING_APPROVAL', false], ['RUNNING', false], ['WAITING', false], ['COMPLETED', true], ['FAILED', true], ['CANCELLED', true]] as const) {
      expect(isEnded(state), state).toBe(ended);
    }
  });

  it('is looked at again often while it runs, seldom while it waits for a person, and not once it has ended', () => {
    expect(pollDelayMs('RUNNING')).toBe(2_000);
    expect(pollDelayMs('WAITING')).toBe(10_000);
    expect(pollDelayMs('AWAITING_APPROVAL')).toBe(10_000);
    expect(pollDelayMs('COMPLETED')).toBeNull();
    expect(pollDelayMs('FAILED')).toBeNull();
    expect(pollDelayMs('CANCELLED')).toBeNull();
  });

  it('decides the panel the page opens with', () => {
    const phase = (state: Parameters<typeof runPhase>[0]['run']['state']) => runPhase({ run: { state } as never, steps: [] });
    expect([phase('AWAITING_APPROVAL'), phase('RUNNING'), phase('WAITING'), phase('FAILED'), phase('CANCELLED'), phase('COMPLETED')]).toEqual([
      'estimate',
      'running',
      'gate',
      'failed',
      'cancelled',
      'completed',
    ]);
  });

  it('points at the step that failed or waits, else the one running, else the last that finished', () => {
    const steps = [step('REQUIREMENTS', 'DONE'), step('CRAWL', 'SKIPPED'), step('GAP_CHECK', 'RUNNING'), step('GENERATE', 'PENDING')];
    expect(focusStep(steps)?.name).toBe('GAP_CHECK');
    expect(focusStep([...steps.slice(0, 2), step('GAP_CHECK', 'DONE'), step('GENERATE', 'FAILED'), step('VALIDATE', 'PENDING')])?.name).toBe('GENERATE');
    expect(focusStep([step('REQUIREMENTS', 'DONE'), step('APPROVE', 'WAITING'), step('EXECUTE', 'PENDING')])?.name).toBe('APPROVE');
    expect(focusStep([step('REQUIREMENTS', 'DONE'), step('CRAWL', 'DONE'), step('GAP_CHECK', 'PENDING')])?.name).toBe('CRAWL');
    expect(focusStep([step('REQUIREMENTS', 'PENDING')])).toBeNull();
    expect(focusStep([])).toBeNull();
    // A step that failed outranks one that is only running.
    expect(focusStep([step('CRAWL', 'RUNNING'), step('GAP_CHECK', 'FAILED')])?.name).toBe('GAP_CHECK');
  });
});

describe('the timeline', () => {
  it('adds new events once each, oldest first, and a repeated id is the newer copy', () => {
    const merged = mergeEvents([event(3), event(1)], [event(2), event(3, 'again'), event(4)]);
    expect(merged.map((e) => e.id)).toEqual([1, 2, 3, 4]);
    expect(merged[2].message).toBe('again');
    expect(mergeEvents([], [])).toEqual([]);
    const shown = [event(1)];
    mergeEvents(shown, [event(2)]);
    expect(shown).toHaveLength(1);
  });

  it('knows the last event id it has, or none', () => {
    expect(lastEventId([])).toBeNull();
    expect(lastEventId([event(5), event(2), event(9), event(7)])).toBe(9);
    expect(lastEventId([event(0)])).toBe(0);
  });
});

describe('the estimate in words', () => {
  it('gives seconds, minutes and hours', () => {
    expect(formatSeconds(0)).toBe('0 s');
    expect(formatSeconds(59)).toBe('59 s');
    expect(formatSeconds(59.6)).toBe('60 s');
    expect(formatSeconds(60)).toBe('1 min');
    expect(formatSeconds(150)).toBe('3 min');
    expect(formatSeconds(3_540)).toBe('59 min');
    expect(formatSeconds(3_600)).toBe('1 h');
    expect(formatSeconds(3_900)).toBe('1 h 5 min');
    expect(formatSeconds(7_200)).toBe('2 h');
  });

  it('gives one figure when the range is one and both ends otherwise', () => {
    expect(formatRange({ min: 3, max: 3 })).toBe('3');
    expect(formatRange({ min: 3, max: 8 })).toBe('3–8');
    expect(formatRange({ min: 30, max: 90 }, formatSeconds)).toBe('30 s–2 min');
    expect(formatDurationRange({ min: 96, max: 1029 })).toBe('2 min–17 min');
    expect(formatDurationRange({ min: 5, max: 5 })).toBe('5 s');
  });

  it('gives dollars to two places, and to four when the cents are not enough', () => {
    expect(formatUsd(0)).toBe('$0.00');
    expect(formatUsd(1)).toBe('$1.00');
    expect(formatUsd(0.1)).toBe('$0.10');
    expect(formatUsd(0.28)).toBe('$0.28');
    expect(formatUsd(0.364)).toBe('$0.364');
    expect(formatUsd(0.0081)).toBe('$0.0081');
    expect(formatUsd(1.0063)).toBe('$1.0063');
    // Never finer than the hundredth of a cent, and rounded there.
    expect(formatUsd(0.00123)).toBe('$0.0012');
    expect(formatUsd(0.00006)).toBe('$0.0001');
    expect(formatUsd(0.00001)).toBe('$0.00');
  });

  it('says a free run is free, and that a cost nobody can state is not zero', () => {
    expect(formatCost(null)).toBe('not known (no provider to price it)');
    expect(formatCost({ min: 0, max: 0 })).toBe('$0 (free providers)');
    expect(formatCost({ min: 0.28, max: 0.364 })).toBe('$0.28–$0.364');
    expect(formatCost({ min: 0, max: 0.5 })).toBe('$0.00–$0.50');
    expect(formatCost({ min: 2, max: 2 })).toBe('$2.00');
  });

  it('gives tokens in thousands from a thousand', () => {
    expect(formatTokens(0)).toBe('0');
    expect(formatTokens(999)).toBe('999');
    expect(formatTokens(1_000)).toBe('1.0k');
    expect(formatTokens(1_500)).toBe('1.5k');
    expect(formatTokens(9_999)).toBe('10.0k');
    expect(formatTokens(10_000)).toBe('10k');
    expect(formatTokens(132_000)).toBe('132k');
    expect(formatTokens(1_234_000)).toBe('1234k');
    expect(formatTokens(20_499)).toBe('20k');
  });

  it('says what a task’s speed rests on', () => {
    expect(latencyLabel({ basis: 'MEASURED', calls: 3, avg_seconds: 50 })).toBe('50 s a call, measured over 3 earlier calls');
    expect(latencyLabel({ basis: 'ASSUMED', tokens_per_second: 20 })).toBe('assumed 20 tokens a second (nothing measured yet)');
    expect(latencyLabel({ basis: 'UNKNOWN' })).toBe('no provider can serve this');
  });
});

describe('accepting the estimate', () => {
  const estimate = (paid: boolean): Pick<AutopilotEstimate, 'totals'> => ({
    totals: { ai_calls: { min: 1, max: 2 }, seconds: { min: 1, max: 2 }, cost_usd: null, paid },
  });

  it('needs the estimate accepted, whatever it costs', () => {
    expect(acceptanceState(estimate(false), false, false)).toMatchObject({ canStart: false, paid: false });
    expect(acceptanceState(estimate(false), false, true).canStart).toBe(false);
    expect(acceptanceState(estimate(false), false, false).missing).toBe('Tick that you have read what the run would cost.');
    expect(acceptanceState(estimate(false), true, false)).toEqual({ paid: false, canStart: true, missing: null });
    expect(acceptanceState(estimate(false), true, true).canStart).toBe(true);
  });

  it('needs the cost accepted as well when a provider charges', () => {
    expect(acceptanceState(estimate(true), true, false)).toEqual({
      paid: true,
      canStart: false,
      missing: 'This run would call a provider that charges: tick that you accept that cost.',
    });
    expect(acceptanceState(estimate(true), false, true).canStart).toBe(false);
    expect(acceptanceState(estimate(true), true, true)).toEqual({ paid: true, canStart: true, missing: null });
  });

  it('takes an estimate that does not say to cost money', () => {
    expect(acceptanceState(null, true, false).paid).toBe(true);
    expect(acceptanceState(undefined, true, false).canStart).toBe(false);
    expect(acceptanceState({} as never, true, true).canStart).toBe(true);
  });
});

describe('the form becomes the settings of a run', () => {
  it('starts as a standard run that goes through manual approval, two cases at a time', () => {
    const d = emptyDraft();
    expect(d).toMatchObject({ baseUrl: '', depth: 'STANDARD', execute: true, concurrency: 2, approval: 'manual', executionRole: '', verifyGaps: null, topUp: false, focusRequirements: false });
    expect(d.documentIds).toEqual([]);
    expect(d.roles).toEqual([]);
    expect(emptyDraft().roles).not.toBe(d.roles);
  });

  it('always crawls the visitor first, then the roles with their steps as the backend takes them', () => {
    const config = buildConfig(draft({ roles: [loginRole()] }));
    expect(config.roles).toEqual([
      { name: ANONYMOUS_ROLE, login_actions: [] },
      {
        name: 'admin',
        login_actions: [
          { type: 'NAVIGATE', url: 'https://shop.example.test/login' },
          { type: 'FILL', selector: '#user', value: 'env:AIQA_LOGIN_ADMIN_USERNAME' },
          { type: 'FILL', selector: '#pass', value: 'env:AIQA_LOGIN_ADMIN_PASSWORD' },
          { type: 'CLICK', selector: '#go' },
        ],
      },
    ]);
    expect(buildConfig(draft()).roles).toEqual([{ name: 'anonymous', login_actions: [] }]);
  });

  it('trims what a person typed and leaves out what they did not choose', () => {
    const config = buildConfig(draft({ name: '  Release 5  ', baseUrl: '  https://shop.example.test  ', depth: '', executionRole: '  ' }));
    expect(config.name).toBe('Release 5');
    expect(config.base_url).toBe('https://shop.example.test');
    expect(config).not.toHaveProperty('depth');
    expect(config).not.toHaveProperty('crawl_run_id');
    expect(config).not.toHaveProperty('verify_gaps');
    expect(config).not.toHaveProperty('execution_role');
    expect(buildConfig(draft({ name: '   ' }))).not.toHaveProperty('name');
  });

  it('sends what was chosen', () => {
    const config = buildConfig(
      draft({
        depth: 'EXHAUSTIVE',
        topUp: true,
        focusRequirements: true,
        verifyGaps: 12,
        execute: false,
        concurrency: 4,
        approval: 'auto_validated',
        reuseCrawlId: 'crawl-1',
        documentIds: ['d1', 'd2'],
        skills: [{ id: 'skill-functional', name: 'Functional Testing' }],
        roles: [loginRole()],
        executionRole: ' admin ',
      }),
    );
    expect(config).toMatchObject({
      depth: 'EXHAUSTIVE',
      coverage_top_up: true,
      focus_requirements: true,
      verify_gaps: 12,
      execute: false,
      run_concurrency: 4,
      approval: 'auto_validated',
      crawl_run_id: 'crawl-1',
      requirement_document_ids: ['d1', 'd2'],
      skills: [{ id: 'skill-functional', name: 'Functional Testing' }],
      execution_role: 'admin',
    });
    // The visitor is what the cases run as by default, so naming it changes nothing.
    expect(buildConfig(draft({ executionRole: 'anonymous' }))).not.toHaveProperty('execution_role');
  });

  it('asks for an estimate of the same run: the roles counted with the visitor', () => {
    const request = estimateRequestFor(draft({ roles: [loginRole(), loginRole('editor')], depth: 'STANDARD', verifyGaps: 5, topUp: true, concurrency: 3, execute: false, focusRequirements: true, documentIds: ['d1'] }));
    expect(request).toEqual({
      requirement_document_ids: ['d1'],
      skills: [],
      coverage_top_up: true,
      roles: 3,
      focus_requirements: true,
      execute: false,
      run_concurrency: 3,
      depth: 'STANDARD',
      verify_gaps: 5,
    });
    const bare = estimateRequestFor(draft({ depth: '' }));
    expect(bare.roles).toBe(1);
    expect(bare).not.toHaveProperty('depth');
    expect(bare).not.toHaveProperty('verify_gaps');
  });

  it('turns a suggestion’s steps into steps a person can edit, and a role’s steps back into the backend’s', () => {
    let id = 100;
    const steps = stepsFromActions(
      [
        { type: 'NAVIGATE', url: 'https://shop.example.test/' },
        { type: 'FILL', selector: '#u', value: 'env:AIQA_LOGIN_ADMIN_USERNAME' },
        { type: 'CLICK', selector: '#go' },
        { type: 'ASSERT_VISIBLE', selector: '#never-taken' },
      ],
      () => id++,
    );
    expect(steps).toEqual([
      { localId: 100, type: 'NAVIGATE', url: 'https://shop.example.test/', selector: '', value: '' },
      { localId: 101, type: 'FILL', url: '', selector: '#u', value: 'env:AIQA_LOGIN_ADMIN_USERNAME' },
      { localId: 102, type: 'CLICK', url: '', selector: '#go', value: '' },
    ]);
    expect(toCrawlRole({ localId: 1, name: '  admin ', steps })).toEqual({
      name: 'admin',
      login_actions: [
        { type: 'NAVIGATE', url: 'https://shop.example.test/' },
        { type: 'FILL', selector: '#u', value: 'env:AIQA_LOGIN_ADMIN_USERNAME' },
        { type: 'CLICK', selector: '#go' },
      ],
    });
  });
});

describe('what is wrong with a role', () => {
  it('nothing, for a role with a name and steps that are complete', () => {
    expect(roleProblem(loginRole())).toBeNull();
  });

  it('a role needs a name that is not the visitor’s, and steps', () => {
    expect(roleProblem(loginRole('  '))).toBe('Every role needs a name.');
    expect(roleProblem(loginRole('Anonymous'))).toBe('"anonymous" is the visitor who has not logged in: it is always included.');
    expect(roleProblem(loginRole('admin', { steps: [] }))).toBe('Role "admin" needs at least one login step.');
  });

  it('every step needs what it uses', () => {
    const one = (patch: Partial<RoleDraft['steps'][number]>): RoleDraft => loginRole('admin', { steps: [{ localId: 1, type: 'FILL', url: '', selector: '#u', value: 'x', ...patch }] });
    expect(roleProblem(one({ type: 'NAVIGATE', url: ' ' }))).toBe('Role "admin": a NAVIGATE step needs a URL.');
    expect(roleProblem(one({ type: 'CLICK', selector: '' }))).toBe('Role "admin": a CLICK step needs a selector.');
    expect(roleProblem(one({ type: 'FILL', selector: ' ' }))).toBe('Role "admin": a FILL step needs a selector.');
    expect(roleProblem(one({ type: 'FILL', value: '' }))).toBe('Role "admin": a FILL step needs a value.');
    expect(roleProblem(one({ type: 'FILL', value: 'env:HOME' }))).toContain('must name a variable that starts AIQA_LOGIN_ or AIQA_SECRET_');
    expect(roleProblem(one({ type: 'FILL', value: 'env:AIQA_LOGIN_ADMIN_PASSWORD' }))).toBeNull();
    expect(roleProblem(one({ type: 'FILL', value: 'a literal' }))).toBeNull();
    expect(roleProblem(one({ type: 'CLICK', selector: '#go', value: '' }))).toBeNull();
  });
});

describe('what is wrong with the form', () => {
  it('nothing, for a run with an address', () => {
    expect(draftProblems(draft())).toEqual([]);
    expect(draftProblems(draft({ roles: [loginRole()], executionRole: 'admin', topUp: true, verifyGaps: 60, concurrency: 8 }))).toEqual([]);
  });

  it('the address is asked for and has to be http or https', () => {
    expect(draftProblems(emptyDraft())).toEqual(['Give the address of the application to test.']);
    expect(draftProblems(draft({ baseUrl: '   ' }))).toEqual(['Give the address of the application to test.']);
    expect(draftProblems(draft({ baseUrl: 'shop.example.test' }))).toEqual(['The address of the application must start with http:// or https://.']);
    expect(draftProblems(draft({ baseUrl: 'ftp://shop.example.test' }))).toHaveLength(1);
    expect(isHttpAddress('http://127.0.0.1:8080/app')).toBe(true);
    expect(isHttpAddress(' https://x.test ')).toBe(true);
    expect(isHttpAddress('javascript:alert(1)')).toBe(false);
  });

  it('the name, the roles and their number are held to the backend’s limits', () => {
    expect(draftProblems(draft({ name: 'x'.repeat(MAX_NAME_CHARS) }))).toEqual([]);
    expect(draftProblems(draft({ name: 'x'.repeat(MAX_NAME_CHARS + 1) }))).toEqual(['The name may be at most 80 characters.']);
    expect(draftProblems(draft({ name: `  ${'x'.repeat(MAX_NAME_CHARS)}  ` }))).toEqual([]);
    const many = (n: number) => Array.from({ length: n }, (_, i) => loginRole(`role${i}`, { localId: i }));
    expect(draftProblems(draft({ roles: many(MAX_ROLES - 1) }))).toEqual([]);
    expect(draftProblems(draft({ roles: many(MAX_ROLES) }))).toEqual(['A run may use at most 20 roles, the visitor included.']);
    expect(draftProblems(draft({ roles: [loginRole('admin'), loginRole(' Admin ')] }))).toEqual(['Two roles are called "Admin".']);
    expect(draftProblems(draft({ roles: [loginRole('  ')] }))).toEqual(['Every role needs a name.']);
  });

  it('the role the cases run as has to be one of the roles', () => {
    expect(draftProblems(draft({ executionRole: 'root' }))).toEqual(['The cases are to run as "root", which is not one of the roles.']);
    expect(draftProblems(draft({ executionRole: 'anonymous' }))).toEqual([]);
    expect(draftProblems(draft({ executionRole: '' }))).toEqual([]);
    expect(draftProblems(draft({ roles: [loginRole('admin')], executionRole: ' admin ' }))).toEqual([]);
  });

  it('filling gaps needs a depth, the AI’s look at unclear requirements is limited, and so are the cases at once', () => {
    expect(draftProblems(draft({ topUp: true, depth: '' }))).toEqual(['Filling gaps needs a depth to fill up to: choose one.']);
    expect(draftProblems(draft({ topUp: false, depth: '' }))).toEqual([]);
    expect(draftProblems(draft({ verifyGaps: 0 }))).toEqual(['The AI may look at between 1 and 60 unclear requirements.']);
    expect(draftProblems(draft({ verifyGaps: 61 }))).toHaveLength(1);
    expect(draftProblems(draft({ verifyGaps: 1 }))).toEqual([]);
    expect(draftProblems(draft({ verifyGaps: 60 }))).toEqual([]);
    expect(draftProblems(draft({ concurrency: 0 }))).toEqual(['Run between 1 and 8 cases at a time.']);
    expect(draftProblems(draft({ concurrency: 9 }))).toHaveLength(1);
    expect(draftProblems(draft({ concurrency: 1 }))).toEqual([]);
    expect(draftProblems(draft({ concurrency: 8 }))).toEqual([]);
  });

  it('every problem is listed, not only the first', () => {
    const problems = draftProblems(draft({ baseUrl: '', name: 'x'.repeat(81), concurrency: 0, verifyGaps: 0, topUp: true, depth: '' }));
    expect(problems).toHaveLength(5);
  });
});

describe('the gate and the report', () => {
  it('counts the cases by where they are, validated ones being undecided too', () => {
    const cases = ['GENERATED', 'VALIDATED', 'VALIDATED', 'APPROVED', 'APPROVED', 'APPROVED', 'REJECTED', 'EXECUTED', 'SOMETHING_NEW'].map((status) => ({ status }));
    expect(tallyCases(cases)).toEqual({ undecided: 3, validated: 2, approved: 3, rejected: 1, executed: 1 });
    expect(tallyCases([])).toEqual({ undecided: 0, validated: 0, approved: 0, rejected: 0, executed: 0 });
  });

  it('orders the verdicts with what needs a look first, and words each', () => {
    expect(VERDICT_ORDER).toEqual(['FAILED', 'INCONCLUSIVE', 'NOT_EXECUTED', 'NO_CASES', 'PASSED']);
    for (const verdict of VERDICT_ORDER) expect(VERDICT_LABEL[verdict].length).toBeGreaterThan(2);
    expect(VERDICT_LABEL.NOT_EXECUTED).toBe('Not run');
  });

  it('words every outcome a case can come to, and gives an unknown one as it is', () => {
    expect(outcomeLabel('PASSED')).toBe('Passed');
    expect(outcomeLabel('FAILED')).toBe('Failed');
    expect(outcomeLabel('INFRA_ERROR')).toBe('Infrastructure error');
    expect(outcomeLabel('TIMEOUT')).toBe('Timed out');
    expect(outcomeLabel('NEEDS_REVIEW')).toBe('Needs review');
    expect(outcomeLabel('NOT_EXECUTABLE')).toBe('Cannot be run: not enough to compile');
    expect(outcomeLabel('COMPILATION_FAILED')).toBe('Could not be compiled');
    expect(outcomeLabel('COMPILATION_UNAVAILABLE')).toBe('No AI available to compile it');
    expect(outcomeLabel('NOT_RUN')).toBe('Not run');
    expect(outcomeLabel('UNFINISHED')).toBe('Did not finish in time');
    expect(outcomeLabel('CANCELLED')).toBe('Cancelled');
    expect(outcomeLabel('BLOCKED')).toBe('BLOCKED');
  });
});

describe('how the report reads', () => {
  it('says how the requirements came out in one sentence, the worst first', () => {
    expect(coverageLine({ PASSED: 12, FAILED: 1, INCONCLUSIVE: 1 })).toBe('Of 14 requirements: 1 failed, 1 inconclusive, 12 passed.');
    expect(coverageLine({ NOT_EXECUTED: 2, NO_CASES: 1, PASSED: 3 })).toBe('Of 6 requirements: 2 not run, 1 without a case, 3 passed.');
    expect(coverageLine({ PASSED: 1 })).toBe('Of 1 requirement: 1 passed.');
  });

  it('leaves out a verdict that has none, and says when there is nothing to say', () => {
    expect(coverageLine({ FAILED: 0, PASSED: 4 })).toBe('Of 4 requirements: 4 passed.');
    expect(coverageLine({})).toBe('No requirement was looked at.');
    expect(coverageLine({ PASSED: 0 })).toBe('No requirement was looked at.');
  });

  it('reads bad when a requirement failed, however many passed', () => {
    expect(reportTone({ FAILED: 1, PASSED: 40 })).toBe('bad');
    expect(reportTone({ FAILED: 1 })).toBe('bad');
  });

  it('reads as a warning when nothing failed but some requirement could not be answered', () => {
    expect(reportTone({ PASSED: 5, INCONCLUSIVE: 1 })).toBe('warn');
    expect(reportTone({ PASSED: 5, NOT_EXECUTED: 1 })).toBe('warn');
    expect(reportTone({ PASSED: 5, NO_CASES: 1 })).toBe('warn');
  });

  it('reads good only when every requirement passed, and plain when there is nothing', () => {
    expect(reportTone({ PASSED: 3 })).toBe('good');
    expect(reportTone({})).toBe('plain');
    expect(reportTone({ PASSED: 0, FAILED: 0 })).toBe('plain');
  });
});

describe('the timeline', () => {
  it('colours a failure bad, a wait, a retry or a cancel a warning, and a finish good', () => {
    expect(eventTone('run.failed')).toBe('bad');
    expect(eventTone('step.failed')).toBe('bad');
    expect(eventTone('step.waiting')).toBe('warn');
    expect(eventTone('run.retried')).toBe('warn');
    expect(eventTone('run.cancelled')).toBe('warn');
    expect(eventTone('run.completed')).toBe('good');
    expect(eventTone('step.done')).toBe('good');
  });

  it('leaves everything else plain, including a kind it has never seen', () => {
    for (const kind of ['run.created', 'run.started', 'run.resumed', 'run.continued', 'step.started', 'step.skipped', 'note', 'something.new']) {
      expect(eventTone(kind), kind).toBe('plain');
    }
  });
});

describe('provider names', () => {
  const ID = 'custom-ai-0f1b4add-810d-44da-a369-4a33eda509d4';
  const names: Record<string, string> = { [ID]: 'Mock Autopilot', ollama: 'Ollama' };
  const nameOf = (id: string) => names[id] ?? id;

  it('puts the name a provider was given where the router wrote its id, wherever it is in the words', () => {
    expect(withProviderNames(`Generated group 1/2: 10 case(s) via ${ID}/mock-autopilot-1`, nameOf)).toBe('Generated group 1/2: 10 case(s) via Mock Autopilot/mock-autopilot-1');
    expect(withProviderNames(`fell back from ${ID} to ${ID}`, nameOf)).toBe('fell back from Mock Autopilot to Mock Autopilot');
  });

  it('leaves an id it does not know as it is, and words with no id alone', () => {
    const other = 'custom-ai-11111111-2222-4333-8444-555555555555';
    expect(withProviderNames(`via ${other}/m`, nameOf)).toBe(`via ${other}/m`);
    expect(withProviderNames('no provider here: ollama/qwen', nameOf)).toBe('no provider here: ollama/qwen');
    expect(withProviderNames('', nameOf)).toBe('');
  });

  it('does not take a lookalike for an id', () => {
    expect(withProviderNames('custom-ai-1234 is not an id', nameOf)).toBe('custom-ai-1234 is not an id');
    expect(withProviderNames(`${ID.slice(0, -1)}`, nameOf)).toBe(ID.slice(0, -1));
  });

  it('shows a provider and model as Name/model, for a custom provider and a built-in one, and a bare id as its name', () => {
    expect(providerLabel(`${ID}/mock-autopilot-1`, nameOf)).toBe('Mock Autopilot/mock-autopilot-1');
    expect(providerLabel('ollama/qwen3:14b', nameOf)).toBe('Ollama/qwen3:14b');
    expect(providerLabel('ollama', nameOf)).toBe('Ollama');
    expect(providerLabel('groq/llama-3/70b', nameOf)).toBe('groq/llama-3/70b'); // the model may hold a slash; only the id is named
    expect(providerLabel('ollama/a/b', nameOf)).toBe('Ollama/a/b');
  });
});

describe('small things', () => {
  it('shows the backend’s own words for an error, and a fallback for anything that is not one', () => {
    expect(errorText(new Error('the crawl could not reach the application'), 'That did not work.')).toBe('the crawl could not reach the application');
    expect(errorText(new Error(''), 'That did not work.')).toBe('That did not work.');
    expect(errorText('a string', 'That did not work.')).toBe('That did not work.');
    expect(errorText(undefined, 'That did not work.')).toBe('That did not work.');
    expect(errorText(null, 'fallback')).toBe('fallback');
  });

  it('shows a time as UTC date and clock, without the fraction or the zone', () => {
    expect(formatTimestamp('2026-09-26T10:15:03Z')).toBe('2026-09-26 10:15:03');
    expect(formatTimestamp('2026-09-26T10:15:03.123456+00:00')).toBe('2026-09-26 10:15:03');
    expect(formatTimestamp('2026-09-26 10:15:03')).toBe('2026-09-26 10:15:03');
  });

  it('shows nothing for no time, and a time it cannot read as it is', () => {
    expect(formatTimestamp(null)).toBe('');
    expect(formatTimestamp(undefined)).toBe('');
    expect(formatTimestamp('')).toBe('');
    expect(formatTimestamp('yesterday')).toBe('yesterday');
  });

  it('counts the seconds between two times, and says nothing when one is missing or they are the wrong way round', () => {
    expect(secondsBetween('2026-09-26T10:00:00Z', '2026-09-26T10:00:30Z')).toBe(30);
    expect(secondsBetween('2026-09-26T10:00:00Z', '2026-09-26T10:06:00.400Z')).toBe(360);
    expect(secondsBetween('2026-09-26T10:00:00Z', '2026-09-26T10:00:00Z')).toBe(0);
    expect(secondsBetween(null, '2026-09-26T10:00:30Z')).toBeNull();
    expect(secondsBetween('2026-09-26T10:00:00Z', null)).toBeNull();
    expect(secondsBetween('2026-09-26T10:00:30Z', '2026-09-26T10:00:00Z')).toBeNull();
    expect(secondsBetween('not a time', '2026-09-26T10:00:00Z')).toBeNull();
  });

  it('reads the technique a case was written for from its tags', () => {
    expect(techniqueOf(['ai-generated', 'technique:negative', 'autopilot:abc'])).toBe('negative');
    expect(techniqueOf(['ai-generated'])).toBeNull();
    expect(techniqueOf([])).toBeNull();
    expect(techniqueOf(['technique:'])).toBe('');
  });

  it('finds the suite the run wrote from its generate step, and only from that', () => {
    expect(suiteIdOf([step('REQUIREMENTS', 'DONE', { suite_id: 'wrong' }), step('GENERATE', 'DONE', { suite_id: 's-1' })])).toBe('s-1');
    expect(suiteIdOf([step('GENERATE', 'RUNNING')])).toBeNull();
    expect(suiteIdOf([step('GENERATE', 'DONE', { suite_id: '' })])).toBeNull();
    expect(suiteIdOf([step('GENERATE', 'DONE', { suite_id: 7 })])).toBeNull();
    expect(suiteIdOf([step('REQUIREMENTS', 'DONE', { suite_id: 's-1' })])).toBeNull();
    expect(suiteIdOf([])).toBeNull();
  });
});

describe('what a person typed with spaces around it', () => {
  const role = (name: string, steps: RoleDraft['steps']): RoleDraft => ({ localId: 1, name, steps });

  it('trims the address and the selectors of a login step, and leaves the value as it was typed', () => {
    const steps: RoleDraft['steps'] = [
      { localId: 1, type: 'NAVIGATE', url: '  https://shop.example.test/login  ', selector: '', value: '' },
      { localId: 2, type: 'FILL', url: '', selector: '  #password ', value: ' env:AIQA_LOGIN_ADMIN_PASSWORD ' },
      { localId: 3, type: 'CLICK', url: '', selector: ' #go  ', value: '' },
    ];
    expect(toCrawlRole(role('admin', steps)).login_actions).toEqual([
      { type: 'NAVIGATE', url: 'https://shop.example.test/login' },
      { type: 'FILL', selector: '#password', value: ' env:AIQA_LOGIN_ADMIN_PASSWORD ' },
      { type: 'CLICK', selector: '#go' },
    ]);
  });

  it('runs the cases as a role whose name was typed with spaces around it', () => {
    const steps: RoleDraft['steps'] = [{ localId: 1, type: 'NAVIGATE', url: 'https://shop.example.test/login', selector: '', value: '' }];
    const base = { ...emptyDraft(), baseUrl: 'https://shop.example.test', roles: [role(' admin ', steps)] };
    expect(draftProblems({ ...base, executionRole: 'admin' })).toEqual([]);
    expect(draftProblems({ ...base, executionRole: ' admin ' })).toEqual([]);
  });
});

describe('counting cases and time', () => {
  it('counts cases that have been run, and only those, as run', () => {
    expect(tallyCases([{ status: 'EXECUTED' }, { status: 'EXECUTED' }])).toEqual({ undecided: 0, validated: 0, approved: 0, rejected: 0, executed: 2 });
    expect(tallyCases([{ status: 'APPROVED' }, { status: 'SOMETHING_NEW' }])).toEqual({ undecided: 0, validated: 0, approved: 1, rejected: 0, executed: 0 });
  });

  it('counts the seconds of a long interval exactly', () => {
    expect(secondsBetween('2026-09-26T10:00:00Z', '2026-09-26T11:00:00Z')).toBe(3600);
    expect(secondsBetween('2026-09-26T10:00:00Z', '2026-09-27T10:00:00Z')).toBe(86_400);
  });
});
