/**
 * Phase 8B — what the service answers for an API's OpenAPI document, for the tests of the OpenAPI Tests page. The API is the fixture
 * of the backend's own tests (`api_tests.rs`): a small pet shop whose `GET /pets/{petId}` documents a 404 and answers a bad id with 400.
 * Test-only.
 */
import type {
  OpenApiCaseKind,
  OpenApiCase,
  OpenApiCounts,
  OpenApiOutcome,
  OpenApiPlan,
  OpenApiReport,
  OpenApiReportCase,
  OpenApiReportOperation,
  OpenApiRunListRow,
} from '../../types/contract';

export const PROJECT_ID = 'proj-a';
export const RUN_ID = '11111111-1111-4111-8111-111111111111';
export const OTHER_RUN_ID = '22222222-2222-4222-8222-222222222222';
const BASE = 'http://127.0.0.1:8080/fixture';
const UNSAFE = 'POST changes data on the server: it is run only when unsafe methods are allowed';

export const OPERATIONS = [
  { id: 'GET /pets', method: 'GET', path: '/pets', summary: 'List the pets', tags: ['pets'], deprecated: false, requires_auth: false },
  { id: 'POST /pets', method: 'POST', path: '/pets', summary: null, tags: ['pets'], deprecated: false, requires_auth: false },
  { id: 'GET /pets/{petId}', method: 'GET', path: '/pets/{petId}', summary: 'One pet', tags: ['pets'], deprecated: false, requires_auth: false },
  { id: 'GET /secure', method: 'GET', path: '/secure', summary: null, tags: [], deprecated: false, requires_auth: true },
];

interface CaseSpec {
  operation: string;
  kind: OpenApiCaseKind;
  detail?: string;
  what: string;
  url: string;
  statuses: number[];
  schema?: boolean;
  unsafe?: boolean;
}

const SPECS: CaseSpec[] = [
  { operation: 'GET /pets', kind: 'HAPPY', what: 'a valid request gets a documented success', url: `${BASE}/pets?limit=1`, statuses: [200], schema: true },
  { operation: 'GET /pets', kind: 'MISSING_REQUIRED', detail: 'limit', what: 'without the required query parameter limit', url: `${BASE}/pets`, statuses: [400] },
  { operation: 'GET /pets', kind: 'WRONG_TYPE', detail: 'limit', what: 'the parameter limit with a value of the wrong kind', url: `${BASE}/pets?limit=not-a-number`, statuses: [400] },
  { operation: 'POST /pets', kind: 'HAPPY', what: 'a valid request gets a documented success', url: `${BASE}/pets`, statuses: [201], schema: true, unsafe: true },
  { operation: 'POST /pets', kind: 'MISSING_REQUIRED', detail: 'body.name', what: 'without the required body property name', url: `${BASE}/pets`, statuses: [422], unsafe: true },
  { operation: 'POST /pets', kind: 'MISSING_REQUIRED', detail: 'body.age', what: 'without the required body property age', url: `${BASE}/pets`, statuses: [422], unsafe: true },
  { operation: 'POST /pets', kind: 'WRONG_TYPE', detail: 'body.age', what: 'the body property age with a value of the wrong kind', url: `${BASE}/pets`, statuses: [422], unsafe: true },
  { operation: 'GET /pets/{petId}', kind: 'HAPPY', what: 'a valid request gets a documented success', url: `${BASE}/pets/1`, statuses: [200], schema: true },
  { operation: 'GET /pets/{petId}', kind: 'WRONG_TYPE', detail: 'petId', what: 'the parameter petId with a value of the wrong kind', url: `${BASE}/pets/not-a-number`, statuses: [404] },
  { operation: 'GET /pets/{petId}', kind: 'NOT_FOUND', detail: 'petId', what: 'a petId that is not there is a 404', url: `${BASE}/pets/999999999`, statuses: [404] },
  { operation: 'GET /secure', kind: 'HAPPY', what: 'a valid request gets a documented success', url: `${BASE}/secure`, statuses: [200], schema: true },
  { operation: 'GET /secure', kind: 'NO_AUTH', what: 'without a credential it is refused', url: `${BASE}/secure`, statuses: [401, 403] },
];

const idOf = (s: CaseSpec) => `${s.operation}#${s.kind}${s.detail ? `:${s.detail}` : ''}`;
const titleOf = (s: CaseSpec) => `${s.operation}: ${s.what}`;
const methodOf = (s: CaseSpec) => s.operation.split(' ')[0];

export const planCases = (): OpenApiCase[] =>
  SPECS.map((s) => ({
    id: idOf(s),
    operation: s.operation,
    kind: s.kind,
    title: titleOf(s),
    request: { method: methodOf(s), url: s.url, headers: { Authorization: 'env:AIQA_SECRET_API_TOKEN' }, body: null },
    expect: { statuses: s.statuses, schema: s.schema ? { type: 'object' } : null, why: 'the document lists it' },
    skipped: s.unsafe ? UNSAFE : null,
  }));

/** The plan for the fixture document: 12 cases, 8 that run and 4 (the POST ones) that change data. */
export function planFor(over: Partial<OpenApiPlan> = {}): OpenApiPlan {
  return {
    spec: { title: 'Fixture Pets', version: '1.0', servers: [BASE] },
    base_url: BASE,
    operations: OPERATIONS,
    cases: planCases(),
    skipped: [],
    notes: [],
    counts: { cases: 12, runnable: 8, not_run: 4 },
    missing_secrets: [],
    ...over,
  };
}

const counts = (over: Partial<OpenApiCounts> = {}): OpenApiCounts => ({ passed: 0, failed: 0, error: 0, pending: 0, not_run: 0, ...over });

const outcomeOf = (s: CaseSpec): OpenApiOutcome => (s.unsafe ? 'NOT_RUN' : idOf(s) === 'GET /pets/{petId}#WRONG_TYPE:petId' ? 'FAILED' : 'PASSED');

export function reportCases(operation: string): OpenApiReportCase[] {
  return SPECS.filter((s) => s.operation === operation).map((s) => {
    const outcome = outcomeOf(s);
    return {
      id: idOf(s),
      kind: s.kind,
      title: titleOf(s),
      outcome,
      reason: s.unsafe ? UNSAFE : outcome === 'FAILED' ? 'Status is one of 404: got 400 Bad Request' : null,
      request: { method: methodOf(s), url: s.url },
      expect: { statuses: s.statuses, schema: Boolean(s.schema), why: 'the document lists it' },
      actual: s.unsafe ? null : outcome === 'FAILED' ? { status: 400, duration_ms: 12, assertions: [{ description: 'Status is one of 404', actual: '400 Bad Request', passed: false }] } : null,
      skipped: s.unsafe ? UNSAFE : null,
      task_id: s.unsafe ? null : `api-${String(SPECS.indexOf(s) + 1).padStart(4, '0')}`,
    };
  });
}

export function reportOperations(): OpenApiReportOperation[] {
  const byVerdict: Record<string, [OpenApiReportOperation['verdict'], OpenApiCounts]> = {
    'GET /pets': ['PASSED', counts({ passed: 3 })],
    'POST /pets': ['NOT_RUN', counts({ not_run: 4 })],
    'GET /pets/{petId}': ['FAILED', counts({ passed: 2, failed: 1 })],
    'GET /secure': ['PASSED', counts({ passed: 2 })],
  };
  return OPERATIONS.map((op) => ({
    id: op.id,
    summary: op.summary,
    tags: op.tags,
    verdict: byVerdict[op.id][0],
    counts: byVerdict[op.id][1],
    why_no_cases: null,
    cases: reportCases(op.id),
  }));
}

/** A finished run's report: 12 cases, 8 run, 7 passed, 1 failed (the drift), 4 not run; 3 of 4 operations tested. */
export function reportFor(over: Partial<OpenApiReport> = {}): OpenApiReport {
  return {
    id: RUN_ID,
    run_id: '33333333-3333-4333-8333-333333333333',
    project_id: PROJECT_ID,
    spec: { title: 'Fixture Pets', version: '1.0', base_url: BASE },
    created_at: '2026-09-26T10:15:03.000Z',
    created_by: 'test-key',
    run_state: 'COMPLETED',
    final: true,
    summary: { cases: 12, ran: 8, passed: 7, failed: 1, error: 0, pending: 0, not_run: 4, pass_rate: 0.875 },
    by_kind: {
      HAPPY: counts({ passed: 3, not_run: 1 }),
      NO_AUTH: counts({ passed: 1 }),
      MISSING_REQUIRED: counts({ passed: 1, not_run: 2 }),
      WRONG_TYPE: counts({ passed: 1, failed: 1, not_run: 1 }),
      NOT_FOUND: counts({ passed: 1 }),
    },
    operations: reportOperations(),
    coverage: { operations: 4, tested: 3, not_tested: [{ operation: 'POST /pets', reason: 'none of its cases was run' }] },
    notes: ['4 case(s) were not run because their request changes data (POST, PUT, PATCH, DELETE): run again with unsafe methods allowed to run them.'],
    ...over,
  };
}

export function runRow(over: Partial<OpenApiRunListRow> = {}): OpenApiRunListRow {
  return {
    id: RUN_ID,
    run_id: '33333333-3333-4333-8333-333333333333',
    title: 'Fixture Pets',
    version: '1.0',
    base_url: BASE,
    cases: 12,
    cases_run: 8,
    created_by: 'test-key',
    created_at: '2026-09-26T10:15:03.000Z',
    run_state: 'COMPLETED',
    ...over,
  };
}
