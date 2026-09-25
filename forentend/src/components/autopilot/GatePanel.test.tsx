/**
 * Phase 7 — the approval gate: reading the run's cases, approving and rejecting them, and letting the run go on. `fetch` is
 * mocked at the transport boundary; what is asserted is what the panel really sends and shows.
 */
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setApiKey } from '../../services/aiqaApi';
import { GatePanel } from './GatePanel';
import { PROJECT_ID, RUN_ID, SUITE_ID, detail, testCase } from './autopilotTestData';
import { backend, failure, requests } from './autopilotTestBackend';
import type { TestCaseRow } from '../../types/contract';

const CASES: TestCaseRow[] = [
  testCase('c1', 'Login with a valid user', 'VALIDATED'),
  testCase('c2', 'Login with a wrong password', 'VALIDATED', { tags: ['ai-generated', 'technique:negative'], requirement_reference: 'FR-002' }),
  testCase('c3', 'Login with an empty form', 'GENERATED', { validation_notes: 'a step has no expected result' }),
  testCase('c4', 'Log out', 'APPROVED'),
  testCase('c5', 'Change the language', 'REJECTED'),
];

beforeEach(() => {
  localStorage.clear();
  setApiKey('aiqa_gatetest0000000000');
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  localStorage.clear();
});

const CASES_ROUTE = 'GET /test-cases';

function renderGate(cases: TestCaseRow[] | (() => TestCaseRow[]) = CASES, extra: Parameters<typeof backend>[0] = {}, suiteId: string | null = SUITE_ID) {
  const onDetail = vi.fn();
  const fetchMock = backend({ [CASES_ROUTE]: () => (typeof cases === 'function' ? cases() : cases), ...extra });
  render(<GatePanel projectId={PROJECT_ID} runId={RUN_ID} suiteId={suiteId} onDetail={onDetail} />);
  return { fetchMock, onDetail };
}

const tally = () => document.getElementById('gate-tally')!.textContent!;
const button = (id: string) => document.getElementById(id) as HTMLButtonElement;

describe('GatePanel — reading the cases', () => {
  it('asks for the cases of the run’s suite, and only that suite', async () => {
    const { fetchMock } = renderGate();
    await waitFor(() => expect(document.getElementById('gate-cases')).toBeTruthy());
    const [call] = requests(fetchMock, CASES_ROUTE);
    expect(call.path).toBe(`/api/v1/projects/${PROJECT_ID}/test-cases`);
    expect(call.query).toEqual({ suite_id: SUITE_ID });
  });

  it('counts the cases by where they are, and lists each with its status, requirement and technique', async () => {
    renderGate();
    await waitFor(() => expect(document.getElementById('gate-cases')).toBeTruthy());
    expect(tally()).toBe('5 cases: 1 approved, 2 validated and waiting, 1 needing a fix, 1 rejected');
    const row = document.getElementById('gate-case-c2')!;
    expect(row.textContent).toContain('Login with a wrong password');
    expect(row.textContent).toContain('Validated');
    expect(row.textContent).toContain('FR-002');
    expect(row.textContent).toContain('negative');
    expect(document.getElementById('gate-case-c3')!.textContent).toContain('Needs fixing');
    expect(document.getElementById('gate-case-c3')!.textContent).toContain('a step has no expected result');
  });

  it('opens a case to show its steps and what it expects', async () => {
    renderGate();
    await waitFor(() => expect(document.getElementById('gate-cases')).toBeTruthy());
    expect(screen.queryByText('Expected: The user is logged in')).toBeNull();
    fireEvent.click(screen.getByText('Login with a valid user'));
    expect(screen.getByText('Expected: The user is logged in')).toBeTruthy();
    expect(screen.getByText(/Open the login page/)).toBeTruthy();
  });

  it('offers Approve only on a validated case, and Reject only before a case is approved', async () => {
    renderGate();
    await waitFor(() => expect(document.getElementById('gate-cases')).toBeTruthy());
    expect(button('gate-approve-c1')).toBeTruthy();
    expect(button('gate-approve-c3')).toBeNull(); // needs fixing first
    expect(button('gate-approve-c4')).toBeNull(); // already approved
    expect(button('gate-approve-c5')).toBeNull(); // rejected
    expect(button('gate-reject-c1')).toBeTruthy();
    expect(button('gate-reject-c3')).toBeTruthy();
    expect(button('gate-reject-c4')).toBeNull(); // the backend refuses to reject an approved case
    expect(button('gate-reject-c5')).toBeNull();
  });

  it('says so when the run wrote no case, and reads nothing when it has no suite yet', async () => {
    const { fetchMock } = renderGate([], {}, null);
    await waitFor(() => expect(document.getElementById('gate-empty')).toBeTruthy());
    expect(requests(fetchMock, CASES_ROUTE)).toHaveLength(0);
    expect(tally()).toBe('0 cases: 0 approved, 0 validated and waiting, 0 needing a fix, 0 rejected');
  });

  it('shows why the cases could not be read, and reads them again on Refresh', async () => {
    let fail = true;
    renderGate(() => {
      if (fail) throw new Error('unused');
      return CASES;
    });
    // The handler throws, which the mock turns into a rejected fetch: the panel says it could not read the cases.
    await waitFor(() => expect(document.getElementById('gate-load-error')).toBeTruthy());
    fail = false;
    fireEvent.click(button('gate-refresh'));
    await waitFor(() => expect(document.getElementById('gate-cases')).toBeTruthy());
    expect(document.getElementById('gate-load-error')).toBeNull();
  });
});

describe('GatePanel — approving and rejecting', () => {
  it('approves one case by name, and reads the cases again', async () => {
    let status: TestCaseRow['status'] = 'VALIDATED';
    const { fetchMock } = renderGate(() => [testCase('c1', 'Login with a valid user', status)], {
      'POST /approve-cases': () => {
        status = 'APPROVED';
        return { suite_id: SUITE_ID, approved: ['c1'], refused: [] };
      },
    });
    await waitFor(() => expect(button('gate-approve-c1')).toBeTruthy());
    await act(async () => {
      button('gate-approve-c1').click();
    });
    await waitFor(() => expect(document.getElementById('gate-approve-c1')).toBeNull());
    const [call] = requests(fetchMock, 'POST /approve-cases');
    expect(call.path).toBe(`/api/v1/projects/${PROJECT_ID}/autopilot/runs/${RUN_ID}/approve-cases`);
    expect(call.body).toEqual({ case_ids: ['c1'] });
    expect(tally()).toBe('1 case: 1 approved, 0 validated and waiting, 0 needing a fix, 0 rejected');
    expect(document.getElementById('gate-result')!.textContent).toContain('1 case approved.');
  });

  it('approves every validated case at once, with no names, and says which the backend refused and why', async () => {
    const { fetchMock } = renderGate(CASES, {
      'POST /approve-cases': () => ({
        suite_id: SUITE_ID,
        approved: ['c1'],
        refused: [{ id: 'c2', title: 'Login with a wrong password', reason: 'it was already decided' }],
      }),
    });
    await waitFor(() => expect(document.getElementById('gate-cases')).toBeTruthy());
    expect(button('gate-approve-all').textContent).toContain('Approve all 2 validated');
    await act(async () => {
      button('gate-approve-all').click();
    });
    await waitFor(() => expect(document.getElementById('gate-result')).toBeTruthy());
    expect(requests(fetchMock, 'POST /approve-cases')[0].body).toEqual({});
    const result = document.getElementById('gate-result')!.textContent!;
    expect(result).toContain('1 case approved, 1 refused:');
    expect(result).toContain('Login with a wrong password: it was already decided');
  });

  it('cannot approve all when no case is validated', async () => {
    renderGate([testCase('c3', 'Needs a fix', 'GENERATED'), testCase('c4', 'Log out', 'APPROVED')]);
    await waitFor(() => expect(document.getElementById('gate-cases')).toBeTruthy());
    expect(button('gate-approve-all').disabled).toBe(true);
  });

  it('rejects a case through the ordinary reject route, then reads the cases again', async () => {
    let status: TestCaseRow['status'] = 'VALIDATED';
    const { fetchMock } = renderGate(() => [testCase('c1', 'Login with a valid user', status)], {
      'POST /test-cases/c1/reject': () => {
        status = 'REJECTED';
        return testCase('c1', 'Login with a valid user', 'REJECTED');
      },
    });
    await waitFor(() => expect(button('gate-reject-c1')).toBeTruthy());
    await act(async () => {
      button('gate-reject-c1').click();
    });
    await waitFor(() => expect(document.getElementById('gate-reject-c1')).toBeNull());
    expect(requests(fetchMock, 'POST /test-cases/c1/reject')).toHaveLength(1);
    expect(tally()).toContain('1 rejected');
  });

  it('shows the backend’s own words when an action is refused, and leaves the cases as they were', async () => {
    renderGate(CASES, { 'POST /approve-cases': () => failure(403, 'FORBIDDEN', 'this key may only view') });
    await waitFor(() => expect(button('gate-approve-c1')).toBeTruthy());
    await act(async () => {
      button('gate-approve-c1').click();
    });
    await waitFor(() => expect(document.getElementById('gate-error')).toBeTruthy());
    expect(document.getElementById('gate-error')!.textContent).toBe('this key may only view');
    expect(tally()).toContain('2 validated and waiting');
  });
});

describe('GatePanel — letting the run go on', () => {
  it('says how many cases will be run, and how many are left undecided', async () => {
    renderGate();
    await waitFor(() => expect(document.getElementById('gate-cases')).toBeTruthy());
    expect(button('gate-continue').textContent).toContain('run the 1 approved case');
    expect(document.getElementById('gate-undecided')!.textContent).toBe('3 cases are undecided and will not be run.');
    expect(document.getElementById('gate-none-approved')).toBeNull();
  });

  it('warns that continuing runs nothing when no case is approved', async () => {
    renderGate([testCase('c1', 'Login', 'VALIDATED')]);
    await waitFor(() => expect(document.getElementById('gate-cases')).toBeTruthy());
    expect(button('gate-continue').textContent).toContain('run the 0 approved cases');
    expect(document.getElementById('gate-none-approved')!.textContent).toContain('continuing runs nothing');
    expect(document.getElementById('gate-undecided')!.textContent).toBe('1 case is undecided and will not be run.');
  });

  it('continues the run and hands the run it answers with to the page', async () => {
    const running = detail('RUNNING', { EXECUTE: 'RUNNING' });
    const { fetchMock, onDetail } = renderGate(CASES, { 'POST /continue': () => running });
    await waitFor(() => expect(document.getElementById('gate-cases')).toBeTruthy());
    await act(async () => {
      button('gate-continue').click();
    });
    await waitFor(() => expect(onDetail).toHaveBeenCalledTimes(1));
    expect(onDetail.mock.calls[0][0].run.state).toBe('RUNNING');
    expect(requests(fetchMock, 'POST /continue')[0].path).toBe(`/api/v1/projects/${PROJECT_ID}/autopilot/runs/${RUN_ID}/continue`);
  });

  it('shows why continuing was refused (another person got there first) and does not hand anything to the page', async () => {
    const { onDetail } = renderGate(CASES, { 'POST /continue': () => failure(409, 'CONFLICT', 'this run is no longer waiting at a gate') });
    await waitFor(() => expect(document.getElementById('gate-cases')).toBeTruthy());
    await act(async () => {
      button('gate-continue').click();
    });
    await waitFor(() => expect(document.getElementById('gate-error')).toBeTruthy());
    expect(document.getElementById('gate-error')!.textContent).toBe('this run is no longer waiting at a gate');
    expect(onDetail).not.toHaveBeenCalled();
  });

  it('cannot do two things at once: every button waits while one action is going', async () => {
    let release: (r: unknown) => void = () => {};
    renderGate(CASES, { 'POST /approve-cases': () => new Promise((resolve) => (release = resolve)) });
    await waitFor(() => expect(document.getElementById('gate-cases')).toBeTruthy());
    await act(async () => {
      button('gate-approve-all').click();
    });
    expect(button('gate-continue').disabled).toBe(true);
    expect(button('gate-approve-c1').disabled).toBe(true);
    expect(button('gate-reject-c1').disabled).toBe(true);
    await act(async () => release({ suite_id: SUITE_ID, approved: [], refused: [] }));
    await waitFor(() => expect(button('gate-continue').disabled).toBe(false));
  });
});
