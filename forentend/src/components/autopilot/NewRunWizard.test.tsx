/**
 * Phase 7 — making a run: the form, the estimate beside it that follows every change, and what is sent when the run is made.
 * `fetch` is mocked at the transport boundary only, so what is asserted is what the wizard really sends and shows.
 */
import type { ComponentProps } from 'react';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setApiKey } from '../../services/aiqaApi';
import { DEFAULT_SELECTED_SKILL_IDS } from '../../services/skillRegistry';
import { NewRunWizard } from './NewRunWizard';
import { PROJECT_ID, detail, estimate, run } from './autopilotTestData';
import { backend, failure, requests } from './autopilotTestBackend';
import type { RequirementDocumentRecord, RunListItem } from '../../types/contract';

beforeEach(() => {
  localStorage.clear();
  setApiKey('aiqa_wizardtest00000000');
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  localStorage.clear();
});

const el = (id: string) => document.getElementById(id);
const text = (id: string) => el(id)?.textContent ?? '';
const button = (id: string) => el(id) as HTMLButtonElement;
const field = (id: string) => el(id) as HTMLInputElement & HTMLSelectElement;
const change = (id: string, value: string) => fireEvent.change(field(id), { target: { value } });
const check = (id: string) => fireEvent.click(field(id));

const DOC = (id: string, file_name: string, requirements_count: number, doc_type: string | null = 'PRD'): RequirementDocumentRecord => ({
  id,
  file_name,
  doc_type,
  char_count: 5000,
  requirements_count,
  sections_count: 3,
  skipped_sections: [],
  furniture_lines: 0,
  created_at: '2026-09-01T00:00:00Z',
  updated_at: '2026-09-01T00:00:00Z',
});
const DOCS = [DOC('doc-1', 'PRD.pdf', 6), DOC('doc-2', 'SRS.docx', 1, null)];

const CRAWLS: RunListItem[] = [
  { run_id: 'crawl-done', name: 'Crawl: https://shop.example.test', state: 'COMPLETED', total: 1, passed: 1, failed: 0, project_id: PROJECT_ID },
  { run_id: 'crawl-partial', name: 'Crawl: https://shop.example.test/admin', state: 'PARTIAL', total: 1, passed: 1, failed: 0, project_id: PROJECT_ID },
  { run_id: 'crawl-running', name: 'Crawl: https://shop.example.test/x', state: 'RUNNING', total: 1, passed: 0, failed: 0, project_id: PROJECT_ID },
  { run_id: 'regression', name: 'Regression A1', state: 'COMPLETED', total: 9, passed: 9, failed: 0, project_id: PROJECT_ID },
];

interface Setup {
  documents?: RequirementDocumentRecord[] | 'error';
  executions?: RunListItem[];
  routes?: Parameters<typeof backend>[0];
  blockedBy?: ComponentProps<typeof NewRunWizard>['blockedBy'];
}

function renderWizard({ documents = DOCS, executions = [], routes = {}, blockedBy = null }: Setup = {}) {
  const onCreated = vi.fn();
  const onUploadDocuments = vi.fn();
  const onOpenBlocking = vi.fn();
  const fetchMock = backend({
    'GET /requirements': () => (documents === 'error' ? failure(500, 'INTERNAL_ERROR', 'the requirements could not be read') : { documents, requirements: [] }),
    'GET /executions': () => executions,
    'POST /autopilot/estimate': () => estimate(),
    'POST /autopilot/runs': () => detail('AWAITING_APPROVAL'),
    ...routes,
  });
  const view = render(
    <NewRunWizard
      projectId={PROJECT_ID}
      projectName="Project Alpha"
      onCreated={onCreated}
      onUploadDocuments={onUploadDocuments}
      blockedBy={blockedBy}
      onOpenBlocking={onOpenBlocking}
    />,
  );
  return { fetchMock, onCreated, onUploadDocuments, onOpenBlocking, view };
}

const estimateRequests = (fetchMock: ReturnType<typeof vi.fn>) => requests(fetchMock, 'POST /autopilot/estimate$');
const created = (fetchMock: ReturnType<typeof vi.fn>) => requests(fetchMock, 'POST /autopilot/runs$');
const waitForDocuments = () => waitFor(() => expect(el('wizard-doc-doc-1')).toBeTruthy());
/** The estimate request that follows the latest change (the form asks again a moment after each). */
const lastEstimate = (fetchMock: ReturnType<typeof vi.fn>) => estimateRequests(fetchMock).at(-1)!.body;
const settled = (fetchMock: ReturnType<typeof vi.fn>, before: number) => waitFor(() => expect(estimateRequests(fetchMock).length).toBeGreaterThan(before), { timeout: 3000 });

async function fillAddress(value = 'https://shop.example.test') {
  change('wizard-base-url', value);
}

describe('NewRunWizard — the form', () => {
  it('asks for the application, and offers the project’s documents with what each holds', async () => {
    renderWizard();
    await waitForDocuments();
    expect(text('wizard-application')).toContain('Project Alpha');
    const first = text('wizard-documents');
    expect(first).toContain('PRD.pdf');
    expect(first).toContain('6 requirements');
    expect(first).toContain('SRS.docx');
    expect(first).toContain('1 requirement');
    expect(first).not.toContain('1 requirements');
  });

  it('says a project with no requirements has nothing to start from, points to where to upload, and cannot make a run', async () => {
    const { onUploadDocuments } = renderWizard({ documents: [] });
    await waitFor(() => expect(el('wizard-no-documents')).toBeTruthy());
    await fillAddress();
    expect(button('wizard-create').disabled).toBe(true);
    fireEvent.click(button('wizard-upload'));
    expect(onUploadDocuments).toHaveBeenCalledTimes(1);
  });

  it('says why the requirements could not be read', async () => {
    renderWizard({ documents: 'error' });
    await waitFor(() => expect(text('wizard-documents')).toContain('the requirements could not be read'));
    expect(el('wizard-no-documents')).toBeNull();
  });

  it('offers the standard skills, with the default ones ticked', async () => {
    renderWizard();
    await waitForDocuments();
    for (const id of DEFAULT_SELECTED_SKILL_IDS) expect(field(`wizard-skill-${id}`).checked).toBe(true);
    expect(field('wizard-skill-skill-security-testing').checked).toBe(false);
  });

  it('starts on the recommended choices: standard depth, a person approves, cases run two at a time', async () => {
    renderWizard();
    await waitForDocuments();
    expect(field('wizard-depth').value).toBe('STANDARD');
    expect(field('wizard-approval-manual').checked).toBe(true);
    expect(field('wizard-approval-auto').checked).toBe(false);
    expect(field('wizard-execute').checked).toBe(true);
    expect(field('wizard-concurrency').value).toBe('2');
    expect(field('wizard-top-up').checked).toBe(false);
    expect(field('wizard-verify').checked).toBe(false);
    expect(el('wizard-verify-count')).toBeNull();
  });

  it('offers to reuse only a crawl that finished, and only a crawl', async () => {
    renderWizard({ executions: CRAWLS });
    await waitFor(() => expect(el('wizard-reuse-crawl')).toBeTruthy());
    const options = Array.from(field('wizard-reuse-crawl').options).map((o) => [o.value, o.textContent]);
    expect(options).toEqual([
      ['', 'Crawl the application again'],
      ['crawl-done', 'Use Crawl: https://shop.example.test'],
    ]);
  });

  it('offers no choice of crawl when the project has none that finished', async () => {
    renderWizard({ executions: [CRAWLS[2], CRAWLS[3]] });
    await waitForDocuments();
    expect(el('wizard-reuse-crawl')).toBeNull();
  });

  it('runs the cases as the visitor, or as any role that was written', async () => {
    renderWizard();
    await waitForDocuments();
    fireEvent.click(button('roles-add'));
    fireEvent.change(screen.getByLabelText('Role 1 name'), { target: { value: 'admin' } });
    expect(Array.from(field('wizard-execution-role').options).map((o) => o.value)).toEqual(['', 'admin']);
  });

  it('cannot choose how to run the cases when they are not to be run', async () => {
    renderWizard();
    await waitForDocuments();
    check('wizard-execute');
    expect(field('wizard-concurrency').disabled).toBe(true);
    expect(field('wizard-execution-role').disabled).toBe(true);
  });
});

describe('NewRunWizard — the estimate beside it', () => {
  it('is asked for once the form has settled, with what the form says and nothing it does not', async () => {
    const { fetchMock } = renderWizard();
    await waitForDocuments();
    await settled(fetchMock, 0);
    const body = lastEstimate(fetchMock);
    expect(body).toMatchObject({
      requirement_document_ids: [],
      coverage_top_up: false,
      roles: 1,
      focus_requirements: false,
      execute: true,
      run_concurrency: 2,
      depth: 'STANDARD',
    });
    expect(body.skills.map((s: { id: string }) => s.id).sort()).toEqual([...DEFAULT_SELECTED_SKILL_IDS].sort());
    expect(body).not.toHaveProperty('verify_gaps');
    expect(requests(fetchMock, 'POST /autopilot/estimate$')[0].path).toBe(`/api/v1/projects/${PROJECT_ID}/autopilot/estimate`);
  });

  it('shows the estimate it is given: cases, calls, time and cost', async () => {
    renderWizard();
    await waitFor(() => expect(el('autopilot-estimate')).toBeTruthy(), { timeout: 3000 });
    expect(text('estimate-cases')).toContain('6–18');
    expect(text('estimate-cost')).toContain('$0 (free providers)');
  });

  it('asks once for a burst of changes, not for each keystroke', async () => {
    const { fetchMock } = renderWizard();
    await waitForDocuments();
    await settled(fetchMock, 0);
    const before = estimateRequests(fetchMock).length;
    change('wizard-depth', 'SMOKE');
    change('wizard-depth', 'EXHAUSTIVE');
    change('wizard-concurrency', '4');
    await settled(fetchMock, before);
    await new Promise((r) => setTimeout(r, 600));
    expect(estimateRequests(fetchMock).length).toBe(before + 1);
    expect(lastEstimate(fetchMock)).toMatchObject({ depth: 'EXHAUSTIVE', run_concurrency: 4 });
  });

  it('follows the documents ticked, the roles written and the depth', async () => {
    const { fetchMock } = renderWizard();
    await waitForDocuments();
    await settled(fetchMock, 0);
    let before = estimateRequests(fetchMock).length;

    check('wizard-doc-doc-2');
    await settled(fetchMock, before);
    expect(lastEstimate(fetchMock).requirement_document_ids).toEqual(['doc-2']);

    before = estimateRequests(fetchMock).length;
    fireEvent.click(button('roles-add'));
    await settled(fetchMock, before);
    expect(lastEstimate(fetchMock).roles).toBe(2); // the visitor and the one just added

    before = estimateRequests(fetchMock).length;
    change('wizard-depth', '');
    await settled(fetchMock, before);
    expect(lastEstimate(fetchMock)).not.toHaveProperty('depth');
  });

  it('follows the options that change how many calls are made', async () => {
    const { fetchMock } = renderWizard();
    await waitForDocuments();
    await settled(fetchMock, 0);
    let before = estimateRequests(fetchMock).length;
    check('wizard-top-up');
    check('wizard-focus');
    check('wizard-verify');
    await settled(fetchMock, before);
    expect(lastEstimate(fetchMock)).toMatchObject({ coverage_top_up: true, focus_requirements: true, verify_gaps: 12 });

    before = estimateRequests(fetchMock).length;
    change('wizard-verify-count', '30');
    check('wizard-execute');
    await settled(fetchMock, before);
    expect(lastEstimate(fetchMock)).toMatchObject({ verify_gaps: 30, execute: false });
  });

  it('says why the estimate could not be made, and shows none rather than the last one', async () => {
    let fail = false;
    const { fetchMock } = renderWizard({ routes: { 'POST /autopilot/estimate': () => (fail ? failure(400, 'VALIDATION_ERROR', 'a run may use at most 20 roles') : estimate()) } });
    await waitFor(() => expect(el('autopilot-estimate')).toBeTruthy(), { timeout: 3000 });
    fail = true;
    const before = estimateRequests(fetchMock).length;
    change('wizard-concurrency', '3');
    await waitFor(() => expect(el('wizard-estimate-error')).toBeTruthy(), { timeout: 3000 });
    expect(text('wizard-estimate-error')).toBe('a run may use at most 20 roles');
    expect(el('autopilot-estimate')).toBeNull();
    expect(estimateRequests(fetchMock).length).toBeGreaterThan(before);
  });
});

describe('NewRunWizard — what is wrong with the form', () => {
  it('cannot make a run with no address, and says so', async () => {
    renderWizard();
    await waitForDocuments();
    expect(button('wizard-create').disabled).toBe(true);
    expect(text('wizard-problems')).toContain('Give the address of the application to test.');
  });

  it('cannot make a run for an address that is not http or https', async () => {
    renderWizard();
    await waitForDocuments();
    await fillAddress('shop.example.test');
    expect(button('wizard-create').disabled).toBe(true);
    expect(text('wizard-problems')).toContain('must start with http:// or https://');
    await fillAddress('https://shop.example.test');
    expect(button('wizard-create').disabled).toBe(false);
    expect(el('wizard-problems')).toBeNull();
  });

  it('lists what is missing from a role, and cannot make the run until it is complete', async () => {
    renderWizard();
    await waitForDocuments();
    await fillAddress();
    fireEvent.click(button('roles-add'));
    expect(button('wizard-create').disabled).toBe(true);
    expect(text('wizard-problems')).toContain('Every role needs a name.');
    fireEvent.change(screen.getByLabelText('Role 1 name'), { target: { value: 'admin' } });
    fireEvent.click(button('role-0-add-navigate'));
    expect(button('wizard-create').disabled).toBe(false);
  });

  it('says two roles cannot have the same name', async () => {
    renderWizard();
    await waitForDocuments();
    await fillAddress();
    for (const i of [0, 1]) {
      fireEvent.click(button('roles-add'));
      fireEvent.change(screen.getByLabelText(`Role ${i + 1} name`), { target: { value: 'admin' } });
      fireEvent.click(button(`role-${i}-add-navigate`));
    }
    expect(text('wizard-problems')).toContain('Two roles are called "admin".');
    // The choice of who runs the cases lists the name once, not once for each role that has it.
    expect(Array.from(field('wizard-execution-role').options).map((o) => o.value)).toEqual(['', 'admin']);
  });

  it('cannot fill gaps with no depth to fill up to', async () => {
    renderWizard();
    await waitForDocuments();
    await fillAddress();
    change('wizard-depth', '');
    check('wizard-top-up');
    expect(text('wizard-problems')).toContain('Filling gaps needs a depth');
    expect(button('wizard-create').disabled).toBe(true);
  });
});

describe('NewRunWizard — making the run', () => {
  it('sends what the form says as the run’s settings: the visitor first, the roles as written, the documents, the skills and the switches', async () => {
    const { fetchMock, onCreated } = renderWizard({ executions: CRAWLS });
    await waitForDocuments();
    await fillAddress('  https://shop.example.test  ');
    change('wizard-name', '  Release 5 smoke ');
    check('wizard-doc-doc-1');
    fireEvent.click(button('roles-add'));
    fireEvent.change(screen.getByLabelText('Role 1 name'), { target: { value: ' admin ' } });
    fireEvent.click(button('role-0-add-navigate'));
    fireEvent.click(button('role-0-add-fill'));
    fireEvent.change(screen.getByLabelText('Role 1 step 2 selector'), { target: { value: '#password' } });
    fireEvent.change(screen.getByLabelText('Role 1 step 2 value'), { target: { value: 'env:AIQA_LOGIN_ADMIN_PASSWORD' } });
    change('wizard-depth', 'EXHAUSTIVE');
    check('wizard-top-up');
    change('wizard-concurrency', '4');
    change('wizard-execution-role', 'admin');
    fireEvent.click(field('wizard-approval-auto'));
    change('wizard-reuse-crawl', 'crawl-done');

    await act(async () => {
      button('wizard-create').click();
    });
    await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));

    const [call] = created(fetchMock);
    expect(call.path).toBe(`/api/v1/projects/${PROJECT_ID}/autopilot/runs`);
    const { skills, ...config } = call.body;
    expect(config).toEqual({
      name: 'Release 5 smoke',
      base_url: 'https://shop.example.test',
      roles: [
        { name: 'anonymous', login_actions: [] },
        {
          name: 'admin',
          login_actions: [
            { type: 'NAVIGATE', url: 'https://shop.example.test' },
            { type: 'FILL', selector: '#password', value: 'env:AIQA_LOGIN_ADMIN_PASSWORD' },
          ],
        },
      ],
      requirement_document_ids: ['doc-1'],
      crawl_run_id: 'crawl-done',
      depth: 'EXHAUSTIVE',
      coverage_top_up: true,
      focus_requirements: false,
      execute: true,
      run_concurrency: 4,
      approval: 'auto_validated',
      execution_role: 'admin',
    });
    expect(skills.map((s: { id: string }) => s.id).sort()).toEqual([...DEFAULT_SELECTED_SKILL_IDS].sort());
    expect(onCreated.mock.calls[0][0].run.state).toBe('AWAITING_APPROVAL');
  });

  it('sends the plainest run when nothing but the address was given', async () => {
    const { fetchMock } = renderWizard();
    await waitForDocuments();
    await fillAddress();
    await act(async () => {
      button('wizard-create').click();
    });
    await waitFor(() => expect(created(fetchMock)).toHaveLength(1));
    const { skills: _skills, ...config } = created(fetchMock)[0].body;
    expect(config).toEqual({
      base_url: 'https://shop.example.test',
      roles: [{ name: 'anonymous', login_actions: [] }],
      requirement_document_ids: [],
      depth: 'STANDARD',
      coverage_top_up: false,
      focus_requirements: false,
      execute: true,
      run_concurrency: 2,
      approval: 'manual',
    });
  });

  it('says the run starts nothing, so the cost is accepted on the next page', async () => {
    renderWizard();
    await waitForDocuments();
    expect(text('wizard-side')).toContain('starts nothing');
  });

  it('shows the backend’s own refusal, and leaves the form as it was so it can be fixed and sent again', async () => {
    const { onCreated } = renderWizard({ routes: { 'POST /autopilot/runs': () => failure(400, 'VALIDATION_ERROR', 'the login variable AIQA_LOGIN_ADMIN_PASSWORD is not set') } });
    await waitForDocuments();
    await fillAddress();
    await act(async () => {
      button('wizard-create').click();
    });
    await waitFor(() => expect(el('wizard-create-error')).toBeTruthy());
    expect(text('wizard-create-error')).toBe('the login variable AIQA_LOGIN_ADMIN_PASSWORD is not set');
    expect(onCreated).not.toHaveBeenCalled();
    expect(field('wizard-base-url').value).toBe('https://shop.example.test');
    expect(button('wizard-create').disabled).toBe(false);
  });

  it('cannot make a run twice by pressing twice', async () => {
    let release: (r: unknown) => void = () => {};
    const { fetchMock } = renderWizard({ routes: { 'POST /autopilot/runs': () => new Promise((resolve) => (release = resolve)) } });
    await waitForDocuments();
    await fillAddress();
    await act(async () => {
      button('wizard-create').click();
    });
    expect(button('wizard-create').disabled).toBe(true);
    await act(async () => release(detail('AWAITING_APPROVAL')));
    expect(created(fetchMock)).toHaveLength(1);
  });
});

describe('NewRunWizard — a project that already has a run going', () => {
  const blocking = run('RUNNING', { name: 'Nightly smoke' });

  it('says which run it is, cannot make another, and offers to open that one', async () => {
    const { onOpenBlocking } = renderWizard({ blockedBy: blocking });
    await waitForDocuments();
    await fillAddress();
    expect(text('wizard-blocked')).toContain('“Nightly smoke” (running)');
    expect(text('wizard-blocked')).toContain('one at a time');
    expect(button('wizard-create').disabled).toBe(true);
    fireEvent.click(button('wizard-open-blocking'));
    expect(onOpenBlocking).toHaveBeenCalledTimes(1);
  });

  it('shows nothing of the kind when no run is going', async () => {
    renderWizard();
    await waitForDocuments();
    expect(el('wizard-blocked')).toBeNull();
  });
});
