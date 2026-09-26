/**
 * Phase 8C — what the service says about scheduled regression, as the tests need it. Test-only.
 */
import type { RegressionCase, RegressionReport, RegressionRunAccount, RegressionSchedule, RegressionScope, RegressionSummary } from '../../types/contract';

export const PROJECT_ID = 'proj-a';
export const SCHEDULE_ID = '11111111-1111-4111-8111-111111111111';
export const OTHER_SCHEDULE_ID = '22222222-2222-4222-8222-222222222222';
export const RUN_ID = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
export const OTHER_RUN_ID = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';

/** Four cases: one regressed, one still failing, one fixed, one still passing. */
export const summary = (over: Partial<RegressionSummary> = {}): RegressionSummary => ({
  cases: 4,
  ran: 4,
  passed: 2,
  failed: 2,
  error: 0,
  pending: 0,
  not_run: 0,
  regressed: 1,
  fixed: 1,
  still_failing: 1,
  still_passing: 1,
  new: 0,
  uncompared: 0,
  ...over,
});

export const accountFor = (over: Partial<RegressionRunAccount> = {}): RegressionRunAccount => ({
  id: RUN_ID,
  run_id: '33333333-3333-4333-8333-333333333333',
  created_at: '2026-09-26T02:00:00.000Z',
  trigger: 'SCHEDULE',
  run_state: 'FAILED',
  final: true,
  verdict: 'REGRESSED',
  summary: summary(),
  skipped: 1,
  ...over,
});

export const scheduleFor = (over: Partial<RegressionSchedule> = {}): RegressionSchedule => ({
  id: SCHEDULE_ID,
  project_id: PROJECT_ID,
  name: 'Nightly',
  suite_id: null,
  every_minutes: 1440,
  anchor_at: '2026-09-27T02:00:00.000Z',
  next_run_at: '2026-09-27T02:00:00.000Z',
  concurrency: 2,
  enabled: true,
  last_started_at: '2026-09-26T02:00:00.000Z',
  last_note: null,
  created_by: 'test-key',
  created_at: '2026-09-20T00:00:00.000Z',
  last_run: accountFor(),
  ...over,
});

export const CASES: RegressionCase[] = [
  { case_id: 'case-login', task_id: 'case-login', title: 'Login rejects a wrong password', outcome: 'FAILED', before: 'PASSED', before_from: 'PREVIOUS_RUN', change: 'REGRESSED', reason: 'expected an HTTP status among 200, got real status 500', source_run_id: 's1' },
  { case_id: 'case-checkout', task_id: 'case-checkout', title: 'Checkout totals', outcome: 'FAILED', before: 'FAILED', before_from: 'PREVIOUS_RUN', change: 'STILL_FAILING', reason: 'the total was 0', source_run_id: 's2' },
  { case_id: 'case-search', task_id: 'case-search', title: 'Search finds a product', outcome: 'PASSED', before: 'FAILED', before_from: 'SOURCE_RUN', change: 'FIXED', reason: null, source_run_id: 's3' },
  { case_id: 'case-profile', task_id: 'case-profile', title: 'Profile page loads', outcome: 'PASSED', before: 'PASSED', before_from: 'PREVIOUS_RUN', change: 'STILL_PASSING', reason: null, source_run_id: 's4' },
];

export const reportFor = (over: Partial<RegressionReport> = {}): RegressionReport => ({
  id: RUN_ID,
  schedule_id: SCHEDULE_ID,
  schedule_name: 'Nightly',
  project_id: PROJECT_ID,
  run_id: '33333333-3333-4333-8333-333333333333',
  trigger: 'SCHEDULE',
  created_at: '2026-09-26T02:00:00.000Z',
  created_by: 'test-key',
  run_state: 'FAILED',
  final: true,
  verdict: 'REGRESSED',
  summary: summary(),
  cases: CASES,
  skipped: [{ case_id: 'case-new', title: 'Never run', reason: 'approved, but not run yet: hand it off once, so that there is something to repeat' }],
  notes: ['1 case(s) are compared with the run they were copied from: this schedule has no earlier run of them.'],
  ...over,
});

export const scopeFor = (over: Partial<RegressionScope> = {}): RegressionScope => ({
  repeatable: [
    { case_id: 'case-login', title: 'Login rejects a wrong password', source_run_id: 's1' },
    { case_id: 'case-checkout', title: 'Checkout totals', source_run_id: 's2' },
    { case_id: 'case-search', title: 'Search finds a product', source_run_id: 's3' },
  ],
  skipped: [{ case_id: 'case-new', title: 'Never run', reason: 'approved, but not run yet: hand it off once, so that there is something to repeat' }],
  ...over,
});
