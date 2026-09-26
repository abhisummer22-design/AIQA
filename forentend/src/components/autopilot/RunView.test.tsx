/**
 * Phase 7 — one autopilot run: what the page offers follows from the run's state. `fetch` is mocked at the transport boundary
 * only, so what is asserted is what the view really sends and shows.
 */
import { act, cleanup, fireEvent, render, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setApiKey } from '../../services/aiqaApi';
import { RunView } from './RunView';
import { PROJECT_ID, RUN_ID, SUITE_ID, detail, estimate, event, report, testCase } from './autopilotTestData';
import { formatTimestamp } from './autopilotModel';
import { backend, failure, requests } from './autopilotTestBackend';
import type { AutopilotEvent, AutopilotRunDetail } from '../../types/contract';

beforeEach(() => {
  localStorage.clear();
  setApiKey('aiqa_runviewtest00000000');
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  localStorage.clear();
});

const el = (id: string) => document.getElementById(id);
const text = (id: string) => el(id)?.textContent ?? '';
const button = (id: string) => el(id) as HTMLButtonElement;
const box = (id: string) => el(id) as HTMLInputElement;
const base = `/api/v1/projects/${PROJECT_ID}/autopilot/runs/${RUN_ID}`;

function renderRun(d: AutopilotRunDetail, routes: Parameters<typeof backend>[0] = {}, events: AutopilotEvent[] = d.events) {
  const onDetail = vi.fn();
  const fetchMock = backend({ 'GET /report': () => report(), 'GET /test-cases': () => [], ...routes });
  const view = render(<RunView projectId={PROJECT_ID} detail={d} events={events} onDetail={onDetail} />);
  return { fetchMock, onDetail, view };
}

describe('RunView — the header', () => {
  it('names the run, its state, who made it, what it tests and when', () => {
    renderRun(detail('RUNNING', { REQUIREMENTS: 'DONE', CRAWL: 'RUNNING' }, { run: { current_step: 'CRAWL' } }));
    expect(text('run-header')).toContain('Autopilot: https://shop.example.test');
    expect(text('run-state')).toBe('Running');
    expect(text('run-header')).toContain('made by Abhishek');
    expect(text('run-header')).toContain(formatTimestamp('2026-09-26T09:58:00Z'));
    expect(text('run-header')).toContain(`started ${formatTimestamp('2026-09-26T09:59:30Z')}`);
  });

  it('shows that the service is working on the run only while it is', () => {
    const { view } = renderRun(detail('RUNNING', {}, { working: true }));
    expect(el('run-working')).toBeTruthy();
    view.unmount();
    renderRun(detail('RUNNING', {}, { working: false }));
    expect(el('run-working')).toBeNull();
  });

  it('says when the estimate was accepted, and that the cost was too when it was', () => {
    const { view } = renderRun(detail('RUNNING', {}, { run: { approved_paid: false } }));
    expect(text('run-accepted')).toBe(`The estimate was accepted ${formatTimestamp('2026-09-26T09:59:00Z')}.`);
    view.unmount();
    renderRun(detail('RUNNING', {}, { run: { approved_paid: true } }));
    expect(text('run-accepted')).toBe(`The estimate was accepted ${formatTimestamp('2026-09-26T09:59:00Z')}, including its cost.`);
  });

  it('says how long an ended run took', () => {
    renderRun(detail('COMPLETED'));
    expect(text('run-header')).toContain('took 6 min');
  });
});

describe('RunView — a run that has only been made', () => {
  it('shows the estimate and nothing has started: the report and the steps wait', () => {
    renderRun(detail('AWAITING_APPROVAL'));
    expect(el('run-estimate')).toBeTruthy();
    expect(el('autopilot-estimate')).toBeTruthy();
    expect(el('autopilot-report')).toBeNull();
    expect(el('run-running')).toBeNull();
  });

  it('cannot be started until the estimate is accepted', () => {
    renderRun(detail('AWAITING_APPROVAL'));
    expect(button('run-start').disabled).toBe(true);
    expect(text('run-missing')).toContain('read what the run would cost');
    fireEvent.click(box('accept-estimate'));
    expect(button('run-start').disabled).toBe(false);
    expect(el('run-missing')).toBeNull();
  });

  it('asks for the cost to be accepted separately only when a provider that charges is used', () => {
    const { view } = renderRun(detail('AWAITING_APPROVAL'));
    expect(el('accept-paid')).toBeNull(); // a free run
    view.unmount();
    renderRun(detail('AWAITING_APPROVAL', {}, { run: { estimate: estimate(true) } }));
    expect(el('accept-paid')).toBeTruthy();
    fireEvent.click(box('accept-estimate'));
    expect(button('run-start').disabled).toBe(true);
    expect(text('run-missing')).toContain('accept that cost');
    fireEvent.click(box('accept-paid'));
    expect(button('run-start').disabled).toBe(false);
  });

  it('starts a free run by sending that the estimate was accepted, and only that', async () => {
    const started = detail('RUNNING', { REQUIREMENTS: 'RUNNING' });
    const { fetchMock, onDetail } = renderRun(detail('AWAITING_APPROVAL'), { 'POST /start': () => started });
    fireEvent.click(box('accept-estimate'));
    await act(async () => {
      button('run-start').click();
    });
    await waitFor(() => expect(onDetail).toHaveBeenCalledTimes(1));
    const [call] = requests(fetchMock, 'POST /start');
    expect(call.path).toBe(`${base}/start`);
    expect(call.body).toEqual({ accept_estimate: true });
    expect(onDetail.mock.calls[0][0].run.state).toBe('RUNNING');
  });

  it('starts a run that costs money by sending both acceptances', async () => {
    const { fetchMock, onDetail } = renderRun(detail('AWAITING_APPROVAL', {}, { run: { estimate: estimate(true) } }), { 'POST /start': () => detail('RUNNING') });
    fireEvent.click(box('accept-estimate'));
    fireEvent.click(box('accept-paid'));
    await act(async () => {
      button('run-start').click();
    });
    await waitFor(() => expect(onDetail).toHaveBeenCalled());
    expect(requests(fetchMock, 'POST /start')[0].body).toEqual({ accept_estimate: true, accept_paid: true });
  });

  it('never sends the cost as accepted for a free run, even if it was ticked on an estimate that then changed', async () => {
    // A run's estimate is fixed once it is made; this guards the rule in the button itself.
    const { fetchMock } = renderRun(detail('AWAITING_APPROVAL'), { 'POST /start': () => detail('RUNNING') });
    fireEvent.click(box('accept-estimate'));
    await act(async () => {
      button('run-start').click();
    });
    await waitFor(() => expect(requests(fetchMock, 'POST /start')).toHaveLength(1));
    expect(requests(fetchMock, 'POST /start')[0].body).not.toHaveProperty('accept_paid');
  });

  it('shows the backend’s own refusal when the start is refused, and hands nothing to the page', async () => {
    const { onDetail } = renderRun(detail('AWAITING_APPROVAL'), {
      'POST /start': () => failure(400, 'VALIDATION_ERROR', 'the login variable AIQA_LOGIN_ADMIN_PASSWORD is not set'),
    });
    fireEvent.click(box('accept-estimate'));
    await act(async () => {
      button('run-start').click();
    });
    await waitFor(() => expect(el('run-error')).toBeTruthy());
    expect(text('run-error')).toBe('the login variable AIQA_LOGIN_ADMIN_PASSWORD is not set');
    expect(onDetail).not.toHaveBeenCalled();
    expect(button('run-start').disabled).toBe(false); // it can be tried again
  });

  it('discards a run that was never started, through cancel', async () => {
    const { fetchMock, onDetail } = renderRun(detail('AWAITING_APPROVAL'), { 'POST /cancel': () => ({ ...detail('CANCELLED'), outcome: 'cancelled' }) });
    await act(async () => {
      button('run-cancel').click();
    });
    await waitFor(() => expect(onDetail).toHaveBeenCalled());
    expect(requests(fetchMock, 'POST /cancel')[0].path).toBe(`${base}/cancel`);
    expect(onDetail.mock.calls[0][0].run.state).toBe('CANCELLED');
    expect(el('run-notice')).toBeNull();
  });
});

describe('RunView — a run that is going', () => {
  const running = () => detail('RUNNING', { REQUIREMENTS: 'DONE', CRAWL: 'RUNNING' }, { run: { current_step: 'CRAWL' } });

  it('says what it is working on, and that closing the page does not stop it', () => {
    renderRun(running());
    expect(text('run-running')).toContain('Working on: Crawl');
    expect(text('run-running')).toContain('the run goes on in the service');
  });

  it('cancels the run, and says a run in the middle of a call stops when the call is done', async () => {
    const { fetchMock, onDetail } = renderRun(running(), {
      'POST /cancel': () => ({ ...detail('RUNNING', {}, { run: { cancel_requested: true } }), outcome: 'requested' }),
    });
    await act(async () => {
      button('run-cancel').click();
    });
    await waitFor(() => expect(el('run-notice')).toBeTruthy());
    expect(text('run-notice')).toContain('stops as soon as the call it is making has finished');
    expect(requests(fetchMock, 'POST /cancel')).toHaveLength(1);
    expect(onDetail).toHaveBeenCalledTimes(1);
  });

  it('shows a cancel that is already asked for, and cannot be asked for twice', () => {
    renderRun(detail('RUNNING', { CRAWL: 'RUNNING' }, { run: { cancel_requested: true } }));
    expect(el('run-cancelling')).toBeTruthy();
    expect(button('run-cancel').disabled).toBe(true);
  });

  it('shows no report to load automatically while it is going, only an offer to preview it', () => {
    renderRun(running());
    expect(el('report-preview')).toBeTruthy();
    expect(el('report-headline')).toBeNull();
  });
});

describe('RunView — a run at the approval gate', () => {
  it('shows the cases of the suite the run wrote', async () => {
    const waiting = detail('WAITING', { GENERATE: { state: 'DONE', detail: { suite_id: SUITE_ID, cases_created: 2, groups: [] } }, APPROVE: 'WAITING' }, { run: { current_step: 'APPROVE' } });
    const { fetchMock } = renderRun(waiting, { 'GET /test-cases': () => [testCase('c1', 'Login with a valid user', 'VALIDATED')] });
    await waitFor(() => expect(el('gate-cases')).toBeTruthy());
    expect(requests(fetchMock, 'GET /test-cases')[0].query).toEqual({ suite_id: SUITE_ID });
    expect(el('run-cancel')).toBeNull();
  });
});

describe('RunView — a run that failed', () => {
  const failed = () => detail('FAILED', { REQUIREMENTS: 'DONE', CRAWL: { state: 'FAILED', error: 'the crawl could not reach the application' } }, { run: { current_step: 'CRAWL', error: 'the crawl could not reach the application' } });

  it('says why it stopped, and that what was done before is kept', () => {
    renderRun(failed());
    expect(text('run-failed')).toContain('The run stopped');
    expect(text('run-failed')).toContain('the crawl could not reach the application');
    expect(text('run-failed')).toContain('is kept');
    expect(text('run-failed')).toContain('takes it up at Crawl');
  });

  it('starts it again where it stopped, and hands the run it answers with to the page', async () => {
    const { fetchMock, onDetail } = renderRun(failed(), { 'POST /resume': () => detail('RUNNING', { CRAWL: 'RUNNING' }) });
    await act(async () => {
      button('run-resume').click();
    });
    await waitFor(() => expect(onDetail).toHaveBeenCalled());
    expect(requests(fetchMock, 'POST /resume')[0].path).toBe(`${base}/resume`);
    expect(onDetail.mock.calls[0][0].run.state).toBe('RUNNING');
  });

  it('shows a spinner on Start while the run is being started, and on Start Again while it is being taken up', async () => {
    let release: (r: unknown) => void = () => {};
    const { view } = renderRun(detail('AWAITING_APPROVAL'), { 'POST /start': () => new Promise((resolve) => (release = resolve)) });
    fireEvent.click(box('accept-estimate'));
    const spinning = (id: string) => el(id)!.querySelector('.animate-spin') !== null;
    expect(spinning('run-start')).toBe(false);
    await act(async () => {
      button('run-start').click();
    });
    expect(spinning('run-start')).toBe(true);
    expect(button('run-cancel').disabled).toBe(true); // nothing else while it goes
    await act(async () => release(detail('RUNNING')));
    await waitFor(() => expect(spinning('run-start')).toBe(false));
    view.unmount();

    const again = renderRun(detail('FAILED', { CRAWL: 'FAILED' }, { run: { current_step: 'CRAWL', error: 'no page' } }), { 'POST /resume': () => new Promise((resolve) => (release = resolve)) });
    expect(spinning('run-resume')).toBe(false);
    await act(async () => {
      button('run-resume').click();
    });
    expect(spinning('run-resume')).toBe(true);
    await act(async () => release(detail('RUNNING')));
    await waitFor(() => expect(spinning('run-resume')).toBe(false));
    again.view.unmount();
  });

  it('says a run stopped without inventing a reason when none was recorded', () => {
    renderRun(detail('FAILED', { CRAWL: 'FAILED' }, { run: { current_step: 'CRAWL', error: null } }));
    expect(el('run-failed')!.querySelectorAll('p')).toHaveLength(1); // what is kept, and nothing about why
  });

  it('shows the backend’s words when another run is active and starting again is refused', async () => {
    renderRun(failed(), { 'POST /resume': () => failure(409, 'CONFLICT', 'this project has another autopilot run that has not ended: "Nightly"') });
    await act(async () => {
      button('run-resume').click();
    });
    await waitFor(() => expect(el('run-error')).toBeTruthy());
    expect(text('run-error')).toContain('another autopilot run that has not ended');
  });

  it('shows the report of what it did, since a report can be made of any run that has ended', async () => {
    renderRun(failed());
    await waitFor(() => expect(el('report-headline')).toBeTruthy());
  });
});

describe('RunView — a run that was cancelled or completed', () => {
  it('says a cancelled run was cancelled, that its cases are still in the project, and offers nothing to do', () => {
    renderRun(detail('CANCELLED'));
    expect(text('run-cancelled')).toContain(`was cancelled at ${formatTimestamp('2026-09-26T10:05:30Z')}`);
    expect(text('run-cancelled')).toContain('still in the project');
    expect(el('run-start')).toBeNull();
    expect(el('run-cancel')).toBeNull();
    expect(el('run-resume')).toBeNull();
  });

  it('shows the report of a completed run by itself', async () => {
    renderRun(detail('COMPLETED', { REPORT: 'DONE' }));
    await waitFor(() => expect(el('report-headline')).toBeTruthy());
    expect(el('run-start')).toBeNull();
    expect(el('run-failed')).toBeNull();
  });
});

describe('RunView — the steps', () => {
  it('lists the eight steps in order, each with its state', () => {
    renderRun(detail('RUNNING', { REQUIREMENTS: 'DONE', CRAWL: 'DONE', GAP_CHECK: 'SKIPPED', GENERATE: 'RUNNING' }));
    const items = Array.from(document.querySelectorAll('#autopilot-steps > li'));
    expect(items.map((li) => li.id)).toEqual(['REQUIREMENTS', 'CRAWL', 'GAP_CHECK', 'GENERATE', 'VALIDATE', 'APPROVE', 'EXECUTE', 'REPORT'].map((n) => `step-${n}`));
    expect(items.map((li) => li.getAttribute('data-state'))).toEqual(['DONE', 'DONE', 'SKIPPED', 'RUNNING', 'PENDING', 'PENDING', 'PENDING', 'PENDING']);
  });

  it('puts the steps in their own order even when the backend sends them out of it', () => {
    const d = detail('RUNNING');
    renderRun({ ...d, steps: [...d.steps].reverse() });
    expect(Array.from(document.querySelectorAll('#autopilot-steps > li')).map((li) => li.id)[0]).toBe('step-REQUIREMENTS');
  });

  it('says in a line what a step found, how long it took and how often it was tried', () => {
    renderRun(
      detail('RUNNING', {
        REQUIREMENTS: { state: 'DONE', detail: { requirements: 6, documents: 1 } },
        CRAWL: { state: 'DONE', attempts: 2, detail: { pages: 4, roles: ['anonymous'] } },
      }),
    );
    expect(text('step-REQUIREMENTS')).toContain('6 requirements in 1 document');
    expect(text('step-REQUIREMENTS')).toContain('30 s'); // 10:00:00 to 10:00:30
    expect(text('step-CRAWL')).toContain('4 pages seen as 1 role');
    expect(text('step-CRAWL')).toContain('attempt 2');
    expect(text('step-REQUIREMENTS')).not.toContain('attempt');
  });

  it('shows a line about what a step found, or its error, only when it has one', () => {
    renderRun(detail('RUNNING', { REQUIREMENTS: 'DONE', CRAWL: 'RUNNING' }));
    // A step that is done or running with nothing recorded says nothing more than its name and state.
    for (const name of ['REQUIREMENTS', 'CRAWL']) {
      expect(el(`step-${name}`)!.querySelectorAll('.text-\\[11px\\]')).toHaveLength(0);
    }
    // One that has not started says what it will do; one with something found says it, once.
    expect(el('step-GENERATE')!.querySelectorAll('.text-\\[11px\\]')).toHaveLength(1);
  });

  it('says what a step that has not started will do, and the error of one that failed', () => {
    renderRun(detail('FAILED', { REQUIREMENTS: 'DONE', CRAWL: { state: 'FAILED', error: 'no page could be reached' } }, { run: { error: 'no page could be reached' } }));
    expect(text('step-GENERATE')).toContain('The AI writes test cases');
    expect(text('step-CRAWL')).toContain('no page could be reached');
  });

  it('marks the step to look at: the one that failed, else the one running', () => {
    const { view } = renderRun(detail('RUNNING', { REQUIREMENTS: 'DONE', CRAWL: 'RUNNING' }));
    expect(el('step-CRAWL')!.className).toContain('bg-[#020617]');
    expect(el('step-REQUIREMENTS')!.className).not.toContain('bg-[#020617]');
    view.unmount();
    renderRun(detail('FAILED', { REQUIREMENTS: 'DONE', CRAWL: 'DONE', GENERATE: 'FAILED' }));
    expect(el('step-GENERATE')!.className).toContain('bg-[#020617]');
  });
});

describe('RunView — the timeline', () => {
  const many = (n: number): AutopilotEvent[] => Array.from({ length: n }, (_, i) => event(i + 1, `event number ${i + 1}`));

  it('says nothing has happened when there is nothing to show', () => {
    renderRun(detail('AWAITING_APPROVAL'), {}, []);
    expect(text('autopilot-timeline')).toContain('Nothing has happened yet.');
  });

  it('shows each event with its time (UTC) and, when a step said it, the step', () => {
    renderRun(detail('RUNNING'), {}, [event(1, 'The run was made'), event(5, 'Read 6 requirements', { step: 'REQUIREMENTS', kind: 'step.done' })]);
    // The time of the day on the viewer's clock, then the words.
    expect(text('event-1')).toBe(`${formatTimestamp('2026-09-26T10:00:01Z').slice(11)}The run was made`);
    expect(text('event-5')).toBe(`${formatTimestamp('2026-09-26T10:00:05Z').slice(11)}Requirements: Read 6 requirements`);
  });

  it('shows a custom provider by the name it was given (Settings) where a line of the timeline names its id', async () => {
    const id = 'custom-ai-0f1b4add-810d-44da-a369-4a33eda509d4';
    renderRun(
      detail('RUNNING'),
      { 'GET /ai/status': () => ({ providers: [{ id, name: 'Mock Autopilot' }], settings: {}, routing_preview: null }) },
      [event(1, `Generated group 1/2: 10 case(s) via ${id}/mock-autopilot-1`)],
    );
    await waitFor(() => expect(text('event-1')).toContain('via Mock Autopilot/mock-autopilot-1'));
    expect(text('event-1')).not.toContain('custom-ai-');
  });

  it('shows the id as it is until the names are known, and when they cannot be read', () => {
    const id = 'custom-ai-0f1b4add-810d-44da-a369-4a33eda509d4';
    renderRun(detail('RUNNING'), { 'GET /ai/status': () => failure(500, 'INTERNAL_ERROR', 'no names today') }, [event(1, `via ${id}/m`)]);
    expect(text('event-1')).toContain(`via ${id}/m`);
  });

  it('colours a failure, a wait and a success differently from a plain note', () => {
    renderRun(detail('RUNNING'), {}, [
      event(1, 'plain', { kind: 'note' }),
      event(2, 'bad', { kind: 'step.failed' }),
      event(3, 'wait', { kind: 'step.waiting' }),
      event(4, 'good', { kind: 'step.done' }),
    ]);
    const colour = (id: string) => el(id)!.querySelector('span:last-child')!.className;
    expect(colour('event-1')).toContain('text-slate-300');
    expect(colour('event-2')).toContain('text-rose-300');
    expect(colour('event-3')).toContain('text-amber-300');
    expect(colour('event-4')).toContain('text-emerald-300');
  });

  it('shows the latest forty, and all of them on request', () => {
    renderRun(detail('RUNNING'), {}, many(55));
    expect(document.querySelectorAll('#autopilot-timeline li')).toHaveLength(40);
    expect(el('event-15')).toBeNull(); // the first fifteen are folded away
    expect(el('event-16')).toBeTruthy();
    expect(el('event-55')).toBeTruthy();
    fireEvent.click(button('timeline-toggle'));
    expect(document.querySelectorAll('#autopilot-timeline li')).toHaveLength(55);
    expect(button('timeline-toggle').textContent).toBe('Only the latest 40');
    fireEvent.click(button('timeline-toggle'));
    expect(document.querySelectorAll('#autopilot-timeline li')).toHaveLength(40);
  });

  it('offers no toggle when all the events are already shown', () => {
    renderRun(detail('RUNNING'), {}, many(40));
    expect(el('timeline-toggle')).toBeNull();
    expect(within(el('autopilot-timeline')!).getAllByRole('listitem')).toHaveLength(40);
  });
});
