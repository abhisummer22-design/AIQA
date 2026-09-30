/**
 * The real login-role builder added to `CrawlGroundingSection` — lets a
 * caller supply a real NAVIGATE/FILL/CLICK login sequence
 * (`crawler::CrawlRole.login_actions`'s own closed vocabulary) so a crawl
 * can authenticate as a real role instead of only ever seeing anonymous
 * pages. Rendered directly (not through the full `TestCaseGeneratorPage`),
 * same "render the component directly" convention as
 * `ApplicationSourceSection.test.tsx`/`DocumentUploadSection.test.tsx` in
 * this same folder. `fetch` is mocked only at the transport boundary.
 *
 * Every click that changes this component's own state is fired via
 * `fireEvent`/`act` (never a raw `.click()`) so the DOM genuinely reflects
 * the update before the next assertion queries it.
 *
 * The pre-existing anonymous-only crawl behavior (URL input, polling,
 * completed/failed states) is already covered end-to-end through
 * `TestCaseGeneratorPage.test.tsx`'s "crawl grounding" describe block and
 * is not re-tested here.
 *
 * The last describe covers the effect that restores an already-selected
 * crawl's real summary (`crawlRunId`) — what the button shows when that
 * restore is torn down before its request has answered.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CrawlGroundingSection } from './CrawlGroundingSection';
import { setApiKey } from '../../services/aiqaApi';
import type { AppSourceConnection, AppSourceType } from '../../types';
import type { CrawlOutput } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const RAW_KEY = 'aiqa_crawlgrounding0000000000';
const PROJECT_ID = 'proj-crawl-grounding-1';

function emptySources(): Record<AppSourceType, AppSourceConnection> {
  return {
    STAGING: { type: 'STAGING', label: 'Staging', url: '', status: 'IDLE' },
    DEMO: { type: 'DEMO', label: 'Demo', url: '', status: 'IDLE' },
    LIVE: { type: 'LIVE', label: 'Live', url: '', status: 'IDLE' },
    FIGMA: { type: 'FIGMA', label: 'Figma', url: '', status: 'IDLE' },
  };
}

function noop() {}

async function clickButton(el: Element) {
  await act(async () => {
    fireEvent.click(el);
  });
}

async function changeInput(el: Element, value: string) {
  await act(async () => {
    fireEvent.change(el, { target: { value } });
  });
}

beforeEach(() => {
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('CrawlGroundingSection — login role builder', () => {
  it('a real login role and its steps are sent as real PlanActions alongside the always-included anonymous role', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/crawl') && init?.method === 'POST') {
        const body = JSON.parse(String(init.body));
        expect(body.roles).toEqual([
          { name: 'anonymous', login_actions: [] },
          {
            name: 'admin',
            login_actions: [
              { type: 'NAVIGATE', url: 'https://target.example.com/login' },
              { type: 'FILL', selector: '#username', value: 'admin' },
              { type: 'FILL', selector: '#password', value: 'hunter2' },
              { type: 'CLICK', selector: '#submit-login' },
            ],
          },
        ]);
        return jsonResponse(201, { run_id: 'crawl-run-1' });
      }
      return jsonResponse(200, {});
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);

    render(
      <CrawlGroundingSection
        appSources={emptySources()}
        projectId={PROJECT_ID}
        crawlRunId={null}
        onCrawlRunIdChange={noop}
      />,
    );

    await changeInput(document.getElementById('crawl-target-url')!, 'https://target.example.com');
    await clickButton(document.getElementById('btn-add-login-role')!);

    const roleCard = document.querySelector('[id^="login-role-"]')!;
    await changeInput(roleCard.querySelector('input[type="text"]')!, 'admin');

    // A freshly-added role opens in Quick mode; this test wants the exact
    // PlanActions the raw builder sends, so switch to Advanced first.
    await clickButton(screen.getByText('Advanced (customize exactly what it clicks/fills) →'));

    // The default 4-step template: NAVIGATE, FILL, FILL, CLICK — each now
    // captioned (Login page/Username/Password/Login button), which is why
    // the selector inputs no longer share one generic "#selector" placeholder.
    const urlInputs = roleCard.querySelectorAll('input[placeholder="https://your-app.example.com/login"]');
    const usernameSelectorInput = roleCard.querySelector('input[placeholder="#username or input[name=email]"]')!;
    const passwordSelectorInput = roleCard.querySelector('input[placeholder="#password"]')!;
    const loginButtonSelectorInput = roleCard.querySelector('input[placeholder="button[type=submit]"]')!;
    const valueInputs = roleCard.querySelectorAll('input[placeholder="value"]');

    await changeInput(urlInputs[0], 'https://target.example.com/login');
    await changeInput(usernameSelectorInput, '#username');
    await changeInput(valueInputs[0], 'admin');
    await changeInput(passwordSelectorInput, '#password');
    await changeInput(valueInputs[1], 'hunter2');
    await changeInput(loginButtonSelectorInput, '#submit-login');

    await clickButton(document.getElementById('btn-start-crawl')!);

    await waitFor(() =>
      expect(fetchMock.mock.calls.some(([u, i]) => String(u).includes('/crawl') && (i as RequestInit)?.method === 'POST')).toBe(true),
    );
  });

  it('the second FILL step (the password, by this template) is masked by default and its real value is unaffected by masking', async () => {
    render(
      <CrawlGroundingSection
        appSources={emptySources()}
        projectId={PROJECT_ID}
        crawlRunId={null}
        onCrawlRunIdChange={noop}
      />,
    );

    await clickButton(document.getElementById('btn-add-login-role')!);

    const roleCard = document.querySelector('[id^="login-role-"]')!;
    // This test wants the raw builder's own inputs — a freshly-added role opens in Quick mode instead.
    await clickButton(screen.getByText('Advanced (customize exactly what it clicks/fills) →'));
    const valueInputs = roleCard.querySelectorAll('input[placeholder="value"]') as NodeListOf<HTMLInputElement>;

    expect(valueInputs[0].type).toBe('text');
    expect(valueInputs[1].type).toBe('password');

    await changeInput(valueInputs[1], 'hunter2');
    expect(valueInputs[1].value).toBe('hunter2');

    // The reveal toggle only changes the input's masking, never the value.
    const revealButtons = roleCard.querySelectorAll('button[title*="mask"], button[title*="reveal"]');
    await clickButton(revealButtons[revealButtons.length - 1]);
    expect(valueInputs[1].type).toBe('text');
    expect(valueInputs[1].value).toBe('hunter2');
  });

  it('a real client-side validation error blocks the crawl and never sends the doomed request, for an empty role name', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);

    render(
      <CrawlGroundingSection
        appSources={emptySources()}
        projectId={PROJECT_ID}
        crawlRunId={null}
        onCrawlRunIdChange={noop}
      />,
    );

    await changeInput(document.getElementById('crawl-target-url')!, 'https://target.example.com');
    await clickButton(document.getElementById('btn-add-login-role')!);
    // Role name left empty — a real, specific rejection, not a silent no-op.

    await clickButton(document.getElementById('btn-start-crawl')!);

    await waitFor(() => expect(screen.getByText(/Every login role needs a name/)).toBeTruthy());
    expect(fetchMock.mock.calls.some(([u]) => String(u).includes('/crawl'))).toBe(false);
  });

  it('removing every step from a role and leaving it empty is a real, specific validation error, not a silent pass', async () => {
    render(
      <CrawlGroundingSection
        appSources={emptySources()}
        projectId={PROJECT_ID}
        crawlRunId={null}
        onCrawlRunIdChange={noop}
      />,
    );

    await changeInput(document.getElementById('crawl-target-url')!, 'https://target.example.com');
    await clickButton(document.getElementById('btn-add-login-role')!);

    const roleCard = document.querySelector('[id^="login-role-"]')!;
    await changeInput(roleCard.querySelector('input[type="text"]')!, 'admin');
    // Per-step removal is an Advanced-only control — a freshly-added role opens in Quick mode.
    await clickButton(screen.getByText('Advanced (customize exactly what it clicks/fills) →'));

    // Remove all 4 default steps, one at a time.
    for (let i = 0; i < 4; i++) {
      const removeStepButton = roleCard.querySelector('button[title="Remove this step"]')!;
      await clickButton(removeStepButton);
    }
    expect(roleCard.querySelectorAll('button[title="Remove this step"]').length).toBe(0);

    await clickButton(document.getElementById('btn-start-crawl')!);

    await waitFor(() => expect(screen.getByText(/needs at least one login step/)).toBeTruthy());
  });

  it('removing a role removes it from what would be sent, without affecting the other real role', async () => {
    render(
      <CrawlGroundingSection
        appSources={emptySources()}
        projectId={PROJECT_ID}
        crawlRunId={null}
        onCrawlRunIdChange={noop}
      />,
    );

    await clickButton(document.getElementById('btn-add-login-role')!);
    let roleCards = document.querySelectorAll('[id^="login-role-"]');
    await changeInput(roleCards[0].querySelector('input[type="text"]')!, 'admin');

    await clickButton(document.getElementById('btn-add-login-role')!);
    roleCards = document.querySelectorAll('[id^="login-role-"]');
    expect(roleCards.length).toBe(2);
    await changeInput(roleCards[1].querySelector('input[type="text"]')!, 'editor');

    await clickButton(roleCards[0].querySelector('button[title="Remove this role"]')!);

    roleCards = document.querySelectorAll('[id^="login-role-"]');
    expect(roleCards.length).toBe(1);
    expect((roleCards[0].querySelector('input[type="text"]') as HTMLInputElement).value).toBe('editor');
  });
});

describe('CrawlGroundingSection — Quick login mode (the default for a freshly-added role)', () => {
  it('shows only Login page URL / Username / Password — no NAVIGATE/FILL/CLICK dropdown, no raw selector input', async () => {
    render(
      <CrawlGroundingSection appSources={emptySources()} projectId={PROJECT_ID} crawlRunId={null} onCrawlRunIdChange={noop} />,
    );
    await clickButton(document.getElementById('btn-add-login-role')!);

    const roleCard = document.querySelector('[id^="login-role-"]')!;
    expect(roleCard.querySelector('select')).toBeNull();
    expect(screen.getByPlaceholderText('https://your-app.example.com/login')).toBeTruthy();
    expect(screen.getByPlaceholderText('you@example.com')).toBeTruthy();
    expect(screen.getByPlaceholderText('your password, or env:AIQA_LOGIN_ADMIN_PASSWORD')).toBeTruthy();
    expect(screen.getByText('Advanced (customize exactly what it clicks/fills) →')).toBeTruthy();
  });

  it('sends real PlanActions built from just Login page URL / Username / Password, using the best-effort default selectors', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/crawl') && init?.method === 'POST') {
        const body = JSON.parse(String(init.body));
        expect(body.roles[1]).toEqual({
          name: 'admin',
          login_actions: [
            { type: 'NAVIGATE', url: 'https://target.example.com/login' },
            {
              type: 'FILL',
              selector:
                'input[type="email"], input[name="username"], input[name="email"], input[id="username"], input[id="email"], input[type="text"]',
              value: 'admin@example.com',
            },
            { type: 'FILL', selector: 'input[type="password"]', value: 'hunter2' },
            { type: 'CLICK', selector: 'button[type="submit"], input[type="submit"]' },
          ],
        });
        return jsonResponse(201, { run_id: 'crawl-run-1' });
      }
      return jsonResponse(200, {});
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);

    render(
      <CrawlGroundingSection appSources={emptySources()} projectId={PROJECT_ID} crawlRunId={null} onCrawlRunIdChange={noop} />,
    );

    await changeInput(document.getElementById('crawl-target-url')!, 'https://target.example.com');
    await clickButton(document.getElementById('btn-add-login-role')!);
    const roleCard = document.querySelector('[id^="login-role-"]')!;
    await changeInput(roleCard.querySelector('input[type="text"]')!, 'admin');
    await changeInput(screen.getByPlaceholderText('https://your-app.example.com/login'), 'https://target.example.com/login');
    await changeInput(screen.getByPlaceholderText('you@example.com'), 'admin@example.com');
    await changeInput(screen.getByPlaceholderText('your password, or env:AIQA_LOGIN_ADMIN_PASSWORD'), 'hunter2');

    await clickButton(document.getElementById('btn-start-crawl')!);

    await waitFor(() =>
      expect(fetchMock.mock.calls.some(([u, i]) => String(u).includes('/crawl') && (i as RequestInit)?.method === 'POST')).toBe(true),
    );
  });

  it('a missing username gives a friendly, jargon-free error — never "a FILL step needs a value"', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(
      <CrawlGroundingSection appSources={emptySources()} projectId={PROJECT_ID} crawlRunId={null} onCrawlRunIdChange={noop} />,
    );

    await changeInput(document.getElementById('crawl-target-url')!, 'https://target.example.com');
    await clickButton(document.getElementById('btn-add-login-role')!);
    const roleCard = document.querySelector('[id^="login-role-"]')!;
    await changeInput(roleCard.querySelector('input[type="text"]')!, 'admin');
    await changeInput(screen.getByPlaceholderText('your password, or env:AIQA_LOGIN_ADMIN_PASSWORD'), 'hunter2');

    await clickButton(document.getElementById('btn-start-crawl')!);

    await waitFor(() => expect(screen.getByText(/enter a username or email/)).toBeTruthy());
    expect(screen.queryByText(/FILL step needs a value/)).toBeNull();
    expect(fetchMock.mock.calls.some(([u]) => String(u).includes('/crawl'))).toBe(false);
  });

  it('switching to Advanced keeps exactly what Quick mode already had typed in — nothing is lost', async () => {
    render(
      <CrawlGroundingSection appSources={emptySources()} projectId={PROJECT_ID} crawlRunId={null} onCrawlRunIdChange={noop} />,
    );
    await changeInput(document.getElementById('crawl-target-url')!, 'https://target.example.com');
    await clickButton(document.getElementById('btn-add-login-role')!);
    const roleCard = document.querySelector('[id^="login-role-"]')!;
    await changeInput(roleCard.querySelector('input[type="text"]')!, 'admin');
    await changeInput(screen.getByPlaceholderText('you@example.com'), 'admin@example.com');
    await changeInput(screen.getByPlaceholderText('your password, or env:AIQA_LOGIN_ADMIN_PASSWORD'), 'hunter2');

    await clickButton(screen.getByText('Advanced (customize exactly what it clicks/fills) →'));

    const valueInputs = roleCard.querySelectorAll('input[placeholder="value"]') as NodeListOf<HTMLInputElement>;
    expect(valueInputs[0].value).toBe('admin@example.com');
    expect(valueInputs[1].value).toBe('hunter2');
    const usernameSelectorInput = roleCard.querySelector(
      'input[placeholder="#username or input[name=email]"]',
    ) as HTMLInputElement;
    expect(usernameSelectorInput.value).toBe(
      'input[type="email"], input[name="username"], input[name="email"], input[id="username"], input[id="email"], input[type="text"]',
    );
  });
});

describe('CrawlGroundingSection — Phase 5: env:NAME logins, and the crawl on offer', () => {
  /** A fresh login role in its default Quick mode — just a name, username and password (the login
   *  page URL is left as Quick mode's own default, the current crawl target). */
  async function addRole(values: { username: string; password: string }) {
    await changeInput(document.getElementById('crawl-target-url')!, 'https://target.example.com');
    await clickButton(document.getElementById('btn-add-login-role')!);
    const roleCard = document.querySelector('[id^="login-role-"]')!;
    await changeInput(roleCard.querySelector('input[type="text"]')!, 'admin');
    await changeInput(screen.getByPlaceholderText('you@example.com'), values.username);
    await changeInput(screen.getByPlaceholderText('your password, or env:AIQA_LOGIN_ADMIN_PASSWORD'), values.password);
  }

  it('a login written as env:NAME references is sent as written — the browser never holds a password', async () => {
    const sent: unknown[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (String(url).includes('/crawl') && init?.method === 'POST') {
          sent.push(JSON.parse(String(init.body)));
          return jsonResponse(201, { run_id: 'crawl-run-1' });
        }
        return jsonResponse(200, {});
      }),
    );
    setApiKey(RAW_KEY);
    render(<CrawlGroundingSection appSources={emptySources()} projectId={PROJECT_ID} crawlRunId={null} onCrawlRunIdChange={noop} />);

    await addRole({ username: 'env:AIQA_LOGIN_ADMIN_USERNAME', password: 'env:AIQA_LOGIN_ADMIN_PASSWORD' });
    await clickButton(document.getElementById('btn-start-crawl')!);

    await waitFor(() => expect(sent.length).toBe(1));
    const admin = (sent[0] as { roles: { name: string; login_actions: { type: string; value?: string }[] }[] }).roles[1];
    expect(admin.login_actions.filter((a) => a.type === 'FILL').map((a) => a.value)).toEqual([
      'env:AIQA_LOGIN_ADMIN_USERNAME',
      'env:AIQA_LOGIN_ADMIN_PASSWORD',
    ]);
  });

  it('a value starting env: that names no allowed variable is stopped here with a specific message, and nothing is sent', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, {}));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<CrawlGroundingSection appSources={emptySources()} projectId={PROJECT_ID} crawlRunId={null} onCrawlRunIdChange={noop} />);

    await addRole({ username: 'admin', password: 'env:GROQ_API_KEY' });
    await clickButton(document.getElementById('btn-start-crawl')!);

    const message = await screen.findByText(/must name a variable that starts AIQA_LOGIN_ or AIQA_SECRET_/);
    expect(message.textContent).toContain('env:AIQA_LOGIN_ADMIN_PASSWORD');
    expect(message.textContent).not.toContain('GROQ_API_KEY');
    expect(fetchMock.mock.calls.some(([u, i]) => String(u).includes('/crawl') && (i as RequestInit)?.method === 'POST')).toBe(false);
  });

  it('a literal value is still accepted, as it always was', async () => {
    const sent: unknown[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (String(url).includes('/crawl') && init?.method === 'POST') {
          sent.push(JSON.parse(String(init.body)));
          return jsonResponse(201, { run_id: 'crawl-run-1' });
        }
        return jsonResponse(200, {});
      }),
    );
    setApiKey(RAW_KEY);
    render(<CrawlGroundingSection appSources={emptySources()} projectId={PROJECT_ID} crawlRunId={null} onCrawlRunIdChange={noop} />);

    await addRole({ username: 'admin', password: 'hunter2' });
    await clickButton(document.getElementById('btn-start-crawl')!);
    await waitFor(() => expect(sent.length).toBe(1));
  });

  it('explains env:NAME beside the login builder, and says what a literal password costs', async () => {
    setApiKey(RAW_KEY);
    render(<CrawlGroundingSection appSources={emptySources()} projectId={PROJECT_ID} crawlRunId={null} onCrawlRunIdChange={noop} />);
    expect(document.getElementById('crawl-env-hint')).toBeNull();

    await clickButton(document.getElementById('btn-add-login-role')!);
    const hint = document.getElementById('crawl-env-hint')!.textContent!;
    expect(hint).toContain('env:AIQA_LOGIN_ADMIN_PASSWORD');
    expect(hint).toContain('nothing is embedded');
    expect(hint).toContain('cannot be used to compile tests');
  });

  it('tells the page which finished crawl is on offer and its roles — whatever key style the result uses — and nothing once a new crawl starts', async () => {
    const onCrawlReady = vi.fn();
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        const u = String(url);
        if (u.includes('/crawl') && u.endsWith('/result')) {
          // The real API sends camelCase (`loginSucceeded`); the contract type says snake_case. Both are read.
          return jsonResponse(200, {
            roles: [
              { role: 'anonymous', loginSucceeded: true, loginError: null, pages: [] },
              { role: 'admin', loginSucceeded: true, loginError: null, pages: [] },
              { role: 'auditor', loginSucceeded: false, loginError: 'selector never resolved', pages: [] },
              { role: 'legacy', login_succeeded: true, login_error: null, pages: [] },
            ],
            visibilityMatrix: [],
          });
        }
        if (u.includes('/crawl') && init?.method === 'POST') return jsonResponse(201, { run_id: 'crawl-run-1' });
        if (u.includes('/executions/crawl-run-1')) {
          return jsonResponse(200, { run_id: 'crawl-run-1', state: 'COMPLETED', tasks: [{ id: 't', name: 'Crawl', reason: null }] });
        }
        return jsonResponse(200, {});
      }),
    );
    setApiKey(RAW_KEY);
    render(
      <CrawlGroundingSection
        appSources={emptySources()}
        projectId={PROJECT_ID}
        crawlRunId={null}
        onCrawlRunIdChange={noop}
        onCrawlReady={onCrawlReady}
      />,
    );
    // Nothing is on offer before a crawl.
    expect(onCrawlReady).toHaveBeenLastCalledWith(null);

    await changeInput(document.getElementById('crawl-target-url')!, 'https://target.example.com');
    await clickButton(document.getElementById('btn-start-crawl')!);

    await waitFor(
      () =>
        expect(onCrawlReady).toHaveBeenLastCalledWith({
          runId: 'crawl-run-1',
          roles: [
            { name: 'anonymous', loggedIn: true },
            { name: 'admin', loggedIn: true },
            { name: 'auditor', loggedIn: false },
            { name: 'legacy', loggedIn: true },
          ],
        }),
      { timeout: 10000 },
    );

    // Starting another crawl takes the finished one off offer until the new one finishes.
    onCrawlReady.mockClear();
    await clickButton(document.getElementById('btn-start-crawl')!);
    expect(onCrawlReady).toHaveBeenCalledWith(null);
  }, 20000);
});

describe('CrawlGroundingSection — visiting the pages the requirements mention first (Phase 6)', () => {
  const crawlBodies = (fetchMock: ReturnType<typeof vi.fn>): Record<string, unknown>[] =>
    fetchMock.mock.calls
      .filter(([u, i]: [string, RequestInit | undefined]) => String(u).includes('/crawl') && i?.method === 'POST')
      .map(([, i]: [string, RequestInit]) => JSON.parse(String(i.body)));

  const startWith = async (tick: boolean) => {
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/crawl') && init?.method === 'POST') return jsonResponse(201, { run_id: 'crawl-run-focus' });
      return jsonResponse(200, {});
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(
      <CrawlGroundingSection appSources={emptySources()} projectId={PROJECT_ID} crawlRunId={null} onCrawlRunIdChange={noop} />,
    );
    await changeInput(document.getElementById('crawl-target-url')!, 'https://target.example.com');
    if (tick) await clickButton(document.getElementById('crawl-focus-requirements')!);
    await clickButton(document.getElementById('btn-start-crawl')!);
    await waitFor(() => expect(crawlBodies(fetchMock)).toHaveLength(1));
    return crawlBodies(fetchMock)[0];
  };

  it('is off by default, and then the crawl request is exactly what it always was', async () => {
    const body = await startWith(false);
    expect(body).toEqual({ base_url: 'https://target.example.com', roles: [{ name: 'anonymous', login_actions: [] }] });
    expect('focus_requirements' in body).toBe(false);
    expect((document.getElementById('crawl-focus-requirements') as HTMLInputElement).checked).toBe(false);
  });

  it('asks for it when ticked', async () => {
    const body = await startWith(true);
    expect(body.focus_requirements).toBe(true);
    expect(body.base_url).toBe('https://target.example.com');
  });

  it('cannot be changed while a crawl is running', async () => {
    await startWith(true);
    await waitFor(() => expect((document.getElementById('crawl-focus-requirements') as HTMLInputElement).disabled).toBe(true));
  });
});

/** A connected source, so the URL guard on the crawl button
 *  (`!targetUrl.trim()`) is already satisfied — the URL input itself is
 *  disabled while a restore is in flight, so it can't be typed into then. */
function connectedSources(): Record<AppSourceType, AppSourceConnection> {
  return {
    ...emptySources(),
    STAGING: { type: 'STAGING', label: 'Staging', url: 'https://target.example.com', status: 'CONNECTED' },
  };
}

function crawlOutput(): CrawlOutput {
  return {
    roles: [
      {
        role: 'anonymous',
        login_succeeded: true,
        login_error: null,
        pages: [{ url: 'https://target.example.com/login', title: 'Login', elements: [] }],
      },
    ],
    visibility_matrix: [],
  };
}

/** Stands in for the page: owns `crawlRunId` and hands the section the same
 *  kind of setter, so the section's own `handleCrawl` really clears it. */
function ControlledSection({ initialRunId }: { initialRunId: string | null }) {
  const [crawlRunId, setCrawlRunId] = React.useState<string | null>(initialRunId);
  return (
    <CrawlGroundingSection
      appSources={connectedSources()}
      projectId={PROJECT_ID}
      crawlRunId={crawlRunId}
      onCrawlRunIdChange={setCrawlRunId}
    />
  );
}

describe('CrawlGroundingSection — restoring an already-selected crawl', () => {
  it('a restore abandoned while its request is still in flight does not leave the button stuck on "Crawling…"', () => {
    // Never answers, so the restore is genuinely still in flight when the
    // parent clears its selection below — what a project switch does.
    vi.stubGlobal('fetch', vi.fn().mockImplementation(() => new Promise<Response>(() => {})));
    setApiKey(RAW_KEY);

    const section = (crawlRunId: string | null) => (
      <CrawlGroundingSection
        appSources={connectedSources()}
        projectId={PROJECT_ID}
        crawlRunId={crawlRunId}
        onCrawlRunIdChange={noop}
      />
    );
    const { rerender } = render(section('crawl-run-1'));

    // Nothing but the restore has run, so it is what makes the section busy.
    let button = document.getElementById('btn-start-crawl') as HTMLButtonElement;
    expect(button.textContent).toMatch(/Crawling/);
    expect(button.disabled).toBe(true);

    rerender(section(null));

    button = document.getElementById('btn-start-crawl') as HTMLButtonElement;
    expect(button.textContent).not.toMatch(/Crawling/);
    expect(button.textContent).toMatch(/Crawl This Application/);
    expect(button.disabled).toBe(false);
    expect((document.getElementById('crawl-target-url') as HTMLInputElement).disabled).toBe(false);
  });

  it('a restore that already answered does not reset the status of the crawl the user starts next', async () => {
    const fetchMock = vi.fn().mockImplementation((url: string) =>
      String(url).endsWith('/result')
        ? Promise.resolve(jsonResponse(200, crawlOutput()))
        : // The crawl start request never answers, so the status
          // `handleCrawl` set stays observable.
          new Promise<Response>(() => {}),
    );
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);

    render(<ControlledSection initialRunId="crawl-run-1" />);
    await waitFor(() => expect(screen.getByText(/Crawled 1 real page/)).toBeTruthy());

    // `handleCrawl` clears `crawlRunId` through the parent, which is what
    // tears the finished restore's effect down.
    await clickButton(document.getElementById('btn-start-crawl')!);

    const button = document.getElementById('btn-start-crawl') as HTMLButtonElement;
    expect(button.textContent).toMatch(/Starting/);
    expect(button.disabled).toBe(true);
  });
});
