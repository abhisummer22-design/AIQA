/**
 * Phase 7 — the roles editor: writing a role's login steps by hand, asking for them to be proposed, and taking a proposal. `fetch`
 * is mocked at the transport boundary only.
 */
import React, { useRef, useState } from 'react';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setApiKey } from '../../services/aiqaApi';
import { LoginRolesEditor, describeAction } from './LoginRolesEditor';
import { PROJECT_ID } from './autopilotTestData';
import { backend, failure, requests } from './autopilotTestBackend';
import type { RoleDraft } from './autopilotModel';
import type { LoginSuggestion, LoginSuggestionResponse } from '../../types/contract';

beforeEach(() => {
  localStorage.clear();
  setApiKey('aiqa_rolestest0000000000');
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  localStorage.clear();
});

const BASE = 'https://shop.example.test';
const el = (id: string) => document.getElementById(id);
const text = (id: string) => el(id)?.textContent ?? '';
const button = (id: string) => el(id) as HTMLButtonElement;
const input = (label: string) => screen.getByLabelText(label) as HTMLInputElement;

function Harness({ initial = [], baseUrl = BASE, onRoles }: { initial?: RoleDraft[]; baseUrl?: string; onRoles?: (roles: RoleDraft[]) => void }) {
  const [roles, setRoles] = useState<RoleDraft[]>(initial);
  const ids = useRef(100);
  return (
    <LoginRolesEditor
      projectId={PROJECT_ID}
      baseUrl={baseUrl}
      roles={roles}
      onChange={(next) => {
        setRoles(next);
        onRoles?.(next);
      }}
      nextId={() => ids.current++}
    />
  );
}

const role = (name: string, steps: RoleDraft['steps'] = []): RoleDraft => ({ localId: 1, name, steps });

const SUGGESTION: LoginSuggestion = {
  role: 'admin',
  page_url: `${BASE}/login`,
  page_title: 'Sign in',
  actions: [
    { type: 'NAVIGATE', url: `${BASE}/login` },
    { type: 'FILL', selector: '#user-name', value: 'env:AIQA_LOGIN_ADMIN_USERNAME' },
    { type: 'FILL', selector: '#password', value: 'env:AIQA_LOGIN_ADMIN_PASSWORD' },
    { type: 'CLICK', selector: '#login-button' },
  ],
  secrets: ['AIQA_LOGIN_ADMIN_USERNAME', 'AIQA_LOGIN_ADMIN_PASSWORD'],
  confidence: 'high',
  reasons: ['The page has one password field.', 'The button next to it says Login.'],
  by: 'rules',
};
const RESPONSE: LoginSuggestionResponse = {
  crawl_run_id: 'crawl-1',
  suggestion: SUGGESTION,
  alternatives: [],
  ai: { requested: true, used: false, note: null },
  missing_secrets: [],
};

describe('describeAction', () => {
  it('says each kind of step in one short line', () => {
    expect(describeAction({ type: 'NAVIGATE', url: `${BASE}/login` })).toBe(`open ${BASE}/login`);
    expect(describeAction({ type: 'CLICK', selector: '#go' })).toBe('click #go');
    expect(describeAction({ type: 'FILL', selector: '#pw', value: 'env:AIQA_LOGIN_X' })).toBe('fill #pw with env:AIQA_LOGIN_X');
  });
});

describe('LoginRolesEditor — writing the steps by hand', () => {
  it('says the visitor is always crawled, with no steps to write', () => {
    render(<Harness />);
    expect(text('autopilot-roles')).toContain('anonymous');
    expect(text('autopilot-roles')).toContain('the visitor who has not logged in');
    expect(el('role-0')).toBeNull();
  });

  it('adds a role, which needs a name and a step before it is complete, and says what is missing', () => {
    const seen: RoleDraft[][] = [];
    render(<Harness onRoles={(r) => seen.push(r)} />);
    fireEvent.click(button('roles-add'));
    expect(el('role-0')).toBeTruthy();
    expect(text('role-0-problem')).toBe('Every role needs a name.');
    fireEvent.change(input('Role 1 name'), { target: { value: 'admin' } });
    expect(text('role-0-problem')).toBe('Role "admin" needs at least one login step.');
    expect(seen.at(-1)![0].name).toBe('admin');
  });

  it('adds steps of each kind: opening a page starts at the application’s address', () => {
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-add-navigate'));
    expect(input('Role 1 step 1 address').value).toBe(BASE);
    fireEvent.click(button('role-0-add-fill'));
    fireEvent.click(button('role-0-add-click'));
    expect(input('Role 1 step 2 selector').value).toBe('');
    expect(input('Role 1 step 2 value').value).toBe('');
    expect(input('Role 1 step 3 selector').value).toBe('');
    expect(screen.queryByLabelText('Role 1 step 3 value')).toBeNull(); // a click has no value
    expect(screen.queryByLabelText('Role 1 step 2 address')).toBeNull(); // a fill has no address
  });

  it('edits a step, and the problem line follows what is written', () => {
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-add-fill'));
    expect(text('role-0-problem')).toContain('a FILL step needs a selector');
    fireEvent.change(input('Role 1 step 1 selector'), { target: { value: '#user-name' } });
    expect(text('role-0-problem')).toContain('a FILL step needs a value');
    fireEvent.change(input('Role 1 step 1 value'), { target: { value: 'env:PASSWORD' } });
    expect(text('role-0-problem')).toContain('must name a variable that starts AIQA_LOGIN_ or AIQA_SECRET_');
    fireEvent.change(input('Role 1 step 1 value'), { target: { value: 'env:AIQA_LOGIN_ADMIN_USERNAME' } });
    expect(el('role-0-problem')).toBeNull();
  });

  it('changes the kind of a step', () => {
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-add-click'));
    fireEvent.change(screen.getByLabelText('Role 1 step 1 type'), { target: { value: 'FILL' } });
    expect(screen.getByLabelText('Role 1 step 1 value')).toBeTruthy();
    fireEvent.change(screen.getByLabelText('Role 1 step 1 type'), { target: { value: 'NAVIGATE' } });
    expect(screen.getByLabelText('Role 1 step 1 address')).toBeTruthy();
  });

  it('removes a step, and removes a role', () => {
    const seen: RoleDraft[][] = [];
    render(<Harness initial={[role('admin'), { localId: 2, name: 'customer', steps: [] }]} onRoles={(r) => seen.push(r)} />);
    fireEvent.click(button('role-0-add-click'));
    fireEvent.click(button('role-0-add-click'));
    expect(seen.at(-1)![0].steps).toHaveLength(2);
    fireEvent.click(screen.getByLabelText('Remove step 1 of role 1'));
    expect(seen.at(-1)![0].steps).toHaveLength(1);
    fireEvent.click(screen.getByLabelText('Remove role 1'));
    expect(seen.at(-1)!.map((r) => r.name)).toEqual(['customer']);
    expect(el('role-1')).toBeNull();
  });

  it('allows no more roles than a run may have, the visitor counted', () => {
    const nineteen = Array.from({ length: 19 }, (_, i): RoleDraft => ({ localId: i + 1, name: `role${i}`, steps: [] }));
    render(<Harness initial={nineteen} />);
    expect(button('roles-add').disabled).toBe(true);
  });

  it('allows the last one before that', () => {
    const eighteen = Array.from({ length: 18 }, (_, i): RoleDraft => ({ localId: i + 1, name: `role${i}`, steps: [] }));
    render(<Harness initial={eighteen} />);
    expect(button('roles-add').disabled).toBe(false);
  });
});

describe('LoginRolesEditor — proposing the steps', () => {
  it('cannot ask for a suggestion for a role that has no name yet', () => {
    render(<Harness initial={[role('')]} />);
    expect(button('role-0-suggest').disabled).toBe(true);
    fireEvent.change(input('Role 1 name'), { target: { value: 'admin' } });
    expect(button('role-0-suggest').disabled).toBe(false);
  });

  it('asks for the role’s steps, with the AI allowed by default', async () => {
    const fetchMock = backend({ 'POST /login-suggestion': () => RESPONSE });
    render(<Harness initial={[role(' admin ')]} />);
    fireEvent.click(button('role-0-suggest'));
    await waitFor(() => expect(el('role-0-suggestion')).toBeTruthy());
    const [call] = requests(fetchMock, 'POST /login-suggestion');
    expect(call.path).toBe(`/api/v1/projects/${PROJECT_ID}/login-suggestion`);
    expect(call.body).toEqual({ role: 'admin', use_ai: true });
  });

  it('does not ask the AI when that is switched off', async () => {
    const fetchMock = backend({ 'POST /login-suggestion': () => RESPONSE });
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(el('roles-use-ai')!);
    fireEvent.click(button('role-0-suggest'));
    await waitFor(() => expect(el('role-0-suggestion')).toBeTruthy());
    expect(requests(fetchMock, 'POST /login-suggestion')[0].body).toEqual({ role: 'admin', use_ai: false });
  });

  it('shows the proposal with how sure it is, who made it, from which page, the steps and why', async () => {
    backend({ 'POST /login-suggestion': () => RESPONSE });
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-suggest'));
    await waitFor(() => expect(el('role-0-suggestion-0')).toBeTruthy());
    const card = text('role-0-suggestion-0');
    expect(card).toContain('high');
    expect(card).toContain('proposed by rules');
    expect(card).toContain(`${BASE}/login`);
    expect(card).toContain(`open ${BASE}/login`);
    expect(card).toContain('fill #user-name with env:AIQA_LOGIN_ADMIN_USERNAME');
    expect(card).toContain('click #login-button');
    expect(card).toContain('The page has one password field.');
  });

  it('applies nothing until the person takes it; then the role has exactly those steps and the proposal goes away', async () => {
    const seen: RoleDraft[][] = [];
    backend({ 'POST /login-suggestion': () => RESPONSE });
    render(<Harness initial={[role('admin')]} onRoles={(r) => seen.push(r)} />);
    fireEvent.click(button('role-0-suggest'));
    await waitFor(() => expect(el('role-0-suggestion')).toBeTruthy());
    expect(seen).toHaveLength(0);
    expect(document.querySelectorAll('#role-0 ol > li[id^="role-0-step"]')).toHaveLength(0);

    fireEvent.click(screen.getByText('Use these steps'));
    expect(el('role-0-suggestion')).toBeNull();
    const steps = seen.at(-1)![0].steps;
    expect(steps.map((s) => s.type)).toEqual(['NAVIGATE', 'FILL', 'FILL', 'CLICK']);
    expect(steps[0].url).toBe(`${BASE}/login`);
    expect(steps[2]).toMatchObject({ selector: '#password', value: 'env:AIQA_LOGIN_ADMIN_PASSWORD' });
    expect(el('role-0-problem')).toBeNull(); // complete, and nothing in it is a credential
    expect(input('Role 1 step 3 value').value).toBe('env:AIQA_LOGIN_ADMIN_PASSWORD');
  });

  it('shows the other answer beside the first when the rules and the AI differ, each with its own button', async () => {
    const alt: LoginSuggestion = { ...SUGGESTION, by: 'ollama/qwen3:14b', confidence: 'medium', actions: [{ type: 'CLICK', selector: '#other' }] };
    backend({ 'POST /login-suggestion': () => ({ ...RESPONSE, alternatives: [alt] }) });
    const seen: RoleDraft[][] = [];
    render(<Harness initial={[role('admin')]} onRoles={(r) => seen.push(r)} />);
    fireEvent.click(button('role-0-suggest'));
    await waitFor(() => expect(el('role-0-suggestion-1')).toBeTruthy());
    expect(text('role-0-suggestion-1')).toContain('another answer by ollama/qwen3:14b');
    expect(text('role-0-suggestion-1')).toContain('medium');
    fireEvent.click(el('role-0-suggestion-1')!.querySelector('button')!);
    expect(seen.at(-1)![0].steps.map((s) => s.selector)).toEqual(['#other']);
  });

  it('says why the AI was not used when it was asked for and could not answer', async () => {
    backend({ 'POST /login-suggestion': () => ({ ...RESPONSE, ai: { requested: true, used: false, note: 'no provider is available for this task' } }) });
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-suggest'));
    await waitFor(() => expect(el('role-0-suggestion')).toBeTruthy());
    expect(text('role-0-suggestion')).toContain('AI: no provider is available for this task');
  });

  it('names the variables that are not set yet, and never asks for the values', async () => {
    backend({ 'POST /login-suggestion': () => ({ ...RESPONSE, missing_secrets: ['AIQA_LOGIN_ADMIN_USERNAME', 'AIQA_LOGIN_ADMIN_PASSWORD'] }) });
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-suggest'));
    await waitFor(() => expect(el('role-0-missing-secrets')).toBeTruthy());
    expect(text('role-0-missing-secrets')).toContain('AIQA_LOGIN_ADMIN_USERNAME, AIQA_LOGIN_ADMIN_PASSWORD');
    expect(text('role-0-missing-secrets')).toContain('Never type the real values here');
    expect(document.querySelector('input[type="password"]')).toBeNull();
  });

  it('shows nothing about variables when all are set', async () => {
    backend({ 'POST /login-suggestion': () => RESPONSE });
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-suggest'));
    await waitFor(() => expect(el('role-0-suggestion')).toBeTruthy());
    expect(el('role-0-missing-secrets')).toBeNull();
  });

  it('cannot ask again while the first answer is awaited', async () => {
    let release: (r: unknown) => void = () => {};
    backend({ 'POST /login-suggestion': () => new Promise((resolve) => (release = resolve)) });
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-suggest'));
    await waitFor(() => expect(el('role-0-busy')).toBeTruthy());
    expect(text('role-0-busy')).toContain('Reading the login page');
    expect(button('role-0-suggest').disabled).toBe(true);
    await act(async () => release(RESPONSE));
    await waitFor(() => expect(el('role-0-suggestion')).toBeTruthy());
    expect(button('role-0-suggest').disabled).toBe(false);
  });

  it('shows the backend’s words when the suggestion is refused for another reason', async () => {
    backend({ 'POST /login-suggestion': () => failure(400, 'VALIDATION_ERROR', 'role must be a name of letters, digits and underscores') });
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-suggest'));
    await waitFor(() => expect(el('role-0-error')).toBeTruthy());
    expect(text('role-0-error')).toBe('role must be a name of letters, digits and underscores');
  });
});

describe('LoginRolesEditor — with no crawl to propose from', () => {
  const NO_CRAWL = () => failure(404, 'RESOURCE_NOT_FOUND', 'this project has no crawl to read a login page from');

  it('says so, and offers to look at the login page', async () => {
    backend({ 'POST /login-suggestion': NO_CRAWL });
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-suggest'));
    await waitFor(() => expect(el('role-0-no-crawl')).toBeTruthy());
    expect(text('role-0-no-crawl')).toContain('this project has no crawl to read a login page from');
    expect(button('role-0-crawl').disabled).toBe(false);
  });

  it('treats a crawl with no password field the same way: nothing to propose from', async () => {
    backend({ 'POST /login-suggestion': () => failure(400, 'VALIDATION_ERROR', 'no page of the crawl has a password field: the crawl did not reach a login page') });
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-suggest'));
    await waitFor(() => expect(el('role-0-no-crawl')).toBeTruthy());
  });

  it('cannot look at the login page without the application’s address', async () => {
    backend({ 'POST /login-suggestion': NO_CRAWL });
    render(<Harness initial={[role('admin')]} baseUrl="" />);
    fireEvent.click(button('role-0-suggest'));
    await waitFor(() => expect(el('role-0-no-crawl')).toBeTruthy());
    expect(button('role-0-crawl').disabled).toBe(true);
    expect(text('role-0-no-crawl')).toContain('Give the application’s address first.');
  });

  it('crawls the login page as the visitor, waits for it, and then proposes from it', async () => {
    vi.useFakeTimers();
    let suggestions = 0;
    let polls = 0;
    const fetchMock = backend({
      'POST /login-suggestion': () => (suggestions++ === 0 ? NO_CRAWL() : { ...RESPONSE, crawl_run_id: 'crawl-9' }),
      'POST /crawl': () => ({ run_id: 'crawl-9', task_id: 't1', state: 'PENDING_APPROVAL' }),
      'GET /executions/crawl-9': () => ({ run_id: 'crawl-9', name: `Crawl: ${BASE}`, state: polls++ < 1 ? 'RUNNING' : 'COMPLETED', tasks: [] }),
    });
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-suggest'));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(el('role-0-no-crawl')).toBeTruthy();

    fireEvent.click(button('role-0-crawl'));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(text('role-0-busy')).toContain('Looking at the login page');
    const [crawl] = requests(fetchMock, 'POST /crawl');
    expect(crawl.path).toBe(`/api/v1/projects/${PROJECT_ID}/crawl`);
    expect(crawl.body).toEqual({ base_url: BASE, roles: [{ name: 'anonymous', login_actions: [] }] });

    await act(async () => void (await vi.advanceTimersByTimeAsync(2_000))); // still running
    expect(el('role-0-suggestion')).toBeNull();
    await act(async () => void (await vi.advanceTimersByTimeAsync(2_000))); // finished: proposes
    expect(el('role-0-suggestion')).toBeTruthy();
    expect(requests(fetchMock, 'POST /login-suggestion')).toHaveLength(2);
    expect(requests(fetchMock, 'GET /executions/crawl-9')).toHaveLength(2);
  });

  it('says the crawl ended badly, and why, instead of proposing', async () => {
    vi.useFakeTimers();
    backend({
      'POST /login-suggestion': NO_CRAWL,
      'POST /crawl': () => ({ run_id: 'crawl-9', task_id: 't1', state: 'PENDING_APPROVAL' }),
      'GET /executions/crawl-9': () => ({ run_id: 'crawl-9', name: 'Crawl', state: 'FAILED', tasks: [{ reason: 'the site refused the connection' }] }),
    });
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-suggest'));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    fireEvent.click(button('role-0-crawl'));
    await act(async () => void (await vi.advanceTimersByTimeAsync(2_000)));
    expect(text('role-0-error')).toBe('The crawl of the login page ended FAILED: the site refused the connection.');
    expect(el('role-0-suggestion')).toBeNull();
  });

  it('gives up waiting for a crawl that does not finish', async () => {
    vi.useFakeTimers();
    backend({
      'POST /login-suggestion': NO_CRAWL,
      'POST /crawl': () => ({ run_id: 'crawl-9', task_id: 't1', state: 'PENDING_APPROVAL' }),
      'GET /executions/crawl-9': () => ({ run_id: 'crawl-9', name: 'Crawl', state: 'RUNNING', tasks: [] }),
    });
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-suggest'));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    fireEvent.click(button('role-0-crawl'));
    await act(async () => void (await vi.advanceTimersByTimeAsync(2_000 * 46)));
    expect(text('role-0-error')).toContain('did not finish in time');
  });

  it('says why the crawl could not be started', async () => {
    backend({
      'POST /login-suggestion': NO_CRAWL,
      'POST /crawl': () => failure(400, 'VALIDATION_ERROR', 'base_url is not a public address'),
    });
    render(<Harness initial={[role('admin')]} />);
    fireEvent.click(button('role-0-suggest'));
    await waitFor(() => expect(el('role-0-no-crawl')).toBeTruthy());
    fireEvent.click(button('role-0-crawl'));
    await waitFor(() => expect(el('role-0-error')).toBeTruthy());
    expect(text('role-0-error')).toBe('base_url is not a public address');
  });
});
