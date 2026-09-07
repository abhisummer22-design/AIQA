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
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CrawlGroundingSection } from './CrawlGroundingSection';
import { setApiKey } from '../../services/aiqaApi';
import type { AppSourceConnection, AppSourceType } from '../../types';

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

    // The default 4-step template: NAVIGATE, FILL, FILL, CLICK.
    const urlInputs = roleCard.querySelectorAll('input[placeholder="https://your-app.example.com/login"]');
    const selectorInputs = roleCard.querySelectorAll('input[placeholder="#selector"]');
    const valueInputs = roleCard.querySelectorAll('input[placeholder="value"]');

    await changeInput(urlInputs[0], 'https://target.example.com/login');
    await changeInput(selectorInputs[0], '#username');
    await changeInput(valueInputs[0], 'admin');
    await changeInput(selectorInputs[1], '#password');
    await changeInput(valueInputs[1], 'hunter2');
    await changeInput(selectorInputs[2], '#submit-login');

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
