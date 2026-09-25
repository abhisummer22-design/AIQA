/**
 * Phase 7 — the shapes the autopilot tests send back from a mocked backend, built once so every test file says only what it is
 * about. Test-only: nothing that ships imports this.
 */
import type {
  AutopilotEstimate,
  AutopilotEvent,
  AutopilotReport,
  AutopilotRun,
  AutopilotRunDetail,
  AutopilotRunState,
  AutopilotStep,
  AutopilotStepName,
  AutopilotStepState,
  TestCaseRow,
} from '../../types/contract';
import { STEP_ORDER } from './autopilotModel';

export const PROJECT_ID = 'proj-a';
export const RUN_ID = '11111111-1111-4111-8111-111111111111';
export const SUITE_ID = '22222222-2222-4222-8222-222222222222';

/** A free, local estimate (nothing charges) unless `paid` says a provider does. */
export function estimate(paid = false): AutopilotEstimate {
  return {
    requirements: 6,
    cases: { min: 6, max: 18 },
    steps: [
      { step: 'REQUIREMENTS', ai_calls: { min: 0, max: 0 }, seconds: { min: 1, max: 2 }, note: null },
      { step: 'CRAWL', ai_calls: { min: 0, max: 0 }, seconds: { min: 30, max: 90 }, note: 'as 2 roles' },
      { step: 'GENERATE', ai_calls: { min: 1, max: 3 }, seconds: { min: 60, max: 240 }, note: null },
      { step: 'EXECUTE', ai_calls: { min: 0, max: 6 }, seconds: { min: 120, max: 600 }, note: null },
    ],
    tasks: [
      {
        task: 'TEST_CASE_GENERATION',
        used_for: 'Writing the test cases',
        provider: {
          id: paid ? 'custom-paid' : 'ollama',
          name: paid ? 'Paid API' : 'Ollama',
          model: paid ? 'gpt-x' : 'qwen3:14b',
          kind: paid ? 'API' : 'LOCAL',
          pricing: paid ? { kind: 'METERED', input_usd_per_1k: 0.001, output_usd_per_1k: 0.002 } : { kind: 'UNMETERED' },
          observed: null,
        },
        calls: { min: 1, max: 3 },
        prompt_tokens: { min: 4000, max: 12000 },
        completion_tokens: { min: 2000, max: 9000 },
        seconds: { min: 60, max: 240 },
        cost_usd: paid ? { min: 0.02, max: 0.05 } : { min: 0, max: 0 },
        latency: { basis: 'ASSUMED', tokens_per_second: 20 },
      },
    ],
    totals: {
      ai_calls: { min: 1, max: 9 },
      seconds: { min: 210, max: 930 },
      cost_usd: paid ? { min: 0.02, max: 0.05 } : { min: 0, max: 0 },
      paid,
    },
    assumptions: ['A group of ten requirements is one call.'],
    warnings: [],
  };
}

export function step(name: AutopilotStepName, state: AutopilotStepState, extra: Partial<AutopilotStep> = {}): AutopilotStep {
  return {
    run_id: RUN_ID,
    name,
    position: STEP_ORDER.indexOf(name),
    state,
    attempts: state === 'PENDING' ? 0 : 1,
    detail: {},
    error: null,
    started_at: state === 'PENDING' ? null : '2026-09-26T10:00:00Z',
    finished_at: state === 'DONE' ? '2026-09-26T10:00:30Z' : null,
    ...extra,
  };
}

/** The eight steps of a run, each `PENDING` unless the map says otherwise. */
export function steps(states: Partial<Record<AutopilotStepName, AutopilotStepState | Partial<AutopilotStep> & { state: AutopilotStepState }>> = {}): AutopilotStep[] {
  return STEP_ORDER.map((name) => {
    const given = states[name];
    if (!given) return step(name, 'PENDING');
    return typeof given === 'string' ? step(name, given) : step(name, given.state, given);
  });
}

export function event(id: number, message: string, extra: Partial<AutopilotEvent> = {}): AutopilotEvent {
  return { id, at: `2026-09-26T10:00:${String(id).padStart(2, '0')}Z`, kind: 'note', step: null, message, data: null, ...extra };
}

export function run(state: AutopilotRunState, extra: Partial<AutopilotRun> = {}): AutopilotRun {
  return {
    id: RUN_ID,
    project_id: PROJECT_ID,
    name: 'Autopilot: https://shop.example.test',
    state,
    config: { base_url: 'https://shop.example.test', roles: [{ name: 'anonymous', login_actions: [] }] },
    estimate: estimate(),
    approved_at: state === 'AWAITING_APPROVAL' ? null : '2026-09-26T09:59:00Z',
    approved_paid: false,
    created_by_name: 'Abhishek',
    current_step: null,
    error: null,
    cancel_requested: false,
    created_at: '2026-09-26T09:58:00Z',
    started_at: state === 'AWAITING_APPROVAL' ? null : '2026-09-26T09:59:30Z',
    finished_at: state === 'COMPLETED' || state === 'FAILED' || state === 'CANCELLED' ? '2026-09-26T10:05:30Z' : null,
    updated_at: '2026-09-26T10:05:30Z',
    ...extra,
  };
}

export function detail(state: AutopilotRunState, stepStates: Parameters<typeof steps>[0] = {}, extra: Omit<Partial<AutopilotRunDetail>, 'run'> & { run?: Partial<AutopilotRun> } = {}): AutopilotRunDetail {
  const { run: runExtra, ...rest } = extra;
  return {
    run: run(state, runExtra),
    steps: steps(stepStates),
    events: [event(1, 'The run was made')],
    working: state === 'RUNNING',
    ...rest,
  };
}

export function testCase(id: string, title: string, status: TestCaseRow['status'], extra: Partial<TestCaseRow> = {}): TestCaseRow {
  return {
    id,
    project_id: PROJECT_ID,
    title,
    description: `${title} — description`,
    category: 'Functional',
    priority: 'HIGH',
    severity: 'MAJOR',
    preconditions: [],
    steps: [{ step_number: 1, action: 'Open the login page', expected_result: 'The form is shown' }],
    expected_result: 'The user is logged in',
    assertions: [],
    test_data: null,
    source_document: null,
    requirement_reference: 'FR-001',
    source_requirement: null,
    traceability: null,
    risk_factor: null,
    tags: ['ai-generated', 'technique:positive'],
    source: 'AI',
    status,
    validation_notes: null,
    executed_run_id: null,
    executed_task_id: null,
    created_by: 'Abhishek',
    created_at: '2026-09-26T10:01:00Z',
    updated_at: '2026-09-26T10:01:00Z',
    ...extra,
  };
}

/** A finished run's report: one requirement failed, one passed, one had no case. */
export function report(extra: Partial<AutopilotReport> = {}): AutopilotReport {
  return {
    final: true,
    run: {
      id: RUN_ID,
      name: 'Autopilot: https://shop.example.test',
      state: 'COMPLETED',
      error: null,
      created_by: 'Abhishek',
      created_at: '2026-09-26T09:58:00Z',
      started_at: '2026-09-26T09:59:30Z',
      finished_at: '2026-09-26T10:05:30Z',
      seconds: 360,
    },
    application: { base_url: 'https://shop.example.test', roles: ['anonymous'], execution_role: null },
    requirements: { total: 3, by_kind: { functional: 3 } },
    crawl: { crawl_run_id: 'crawl-1', reused: false, pages: 4, roles: [] },
    gaps: null,
    generation: {
      cases: 5,
      groups: 1,
      providers: { 'ollama/qwen3:14b': 1 },
      fallback_used: false,
      prompt_tokens: 4200,
      completion_tokens: 1800,
      latency_ms: 90000,
    },
    validation: { validated: 5, invalid: [] },
    approval: { approved: 4, rejected: 1, undecided: 0, automatic: false, released: true },
    execution: { enabled: true, done: true, cases: 4, finished: 4, ran: 4, counts: { PASSED: 3, FAILED: 1 }, passed: 3, failed: 1, role: null },
    findings: [
      {
        case_id: 'case-2',
        title: 'Login rejects a wrong password',
        requirement: 'FR-001',
        outcome: 'FAILED',
        reason: 'The error message did not appear',
        run_id: 'exec-1',
        attempts: [{ run_id: 'exec-1', outcome: 'FAILED', reason: 'The error message did not appear' }],
      },
    ],
    not_run: [
      { case_id: 'case-9', title: 'Export a report as CSV', requirement: 'FR-003', outcome: 'NOT_EXECUTABLE', reason: 'the page for it was not crawled', run_id: null, attempts: [] },
    ],
    coverage: [
      { key: 'FR-002', kind: 'functional', text: 'Users can sort products.', cases: 2, ran: 2, passed: 2, failed: 0, not_run: 0, verdict: 'PASSED' },
      { key: 'FR-001', kind: 'functional', text: 'Users can log in.', cases: 2, ran: 2, passed: 1, failed: 1, not_run: 0, verdict: 'FAILED' },
      { key: 'FR-003', kind: 'functional', text: 'Any report can be exported as CSV.', cases: 0, ran: 0, passed: 0, failed: 0, not_run: 0, verdict: 'NO_CASES' },
    ],
    coverage_summary: { PASSED: 1, FAILED: 1, NO_CASES: 1 },
    cost: {
      estimated: estimate().totals,
      actual: { ai_prompt_tokens: 4200, ai_completion_tokens: 1800, ai_seconds: 90, run_seconds: 200 },
    },
    steps: [
      { name: 'GENERATE', state: 'DONE', attempts: 1, seconds: 90 },
      { name: 'EXECUTE', state: 'DONE', attempts: 2, seconds: 200 },
    ],
    notes: ['One requirement has no case: nothing tests it.'],
    ...extra,
  };
}
