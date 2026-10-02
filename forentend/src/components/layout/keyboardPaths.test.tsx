/**
 * Phase C, part 2 — the keyboard paths that were missing entirely.
 *
 * Four independent defects, all the same shape: something worked with a mouse and
 * had no keyboard route at all.
 *
 *   - The three header popovers closed on `onMouseLeave` and nothing else, so
 *     opening one with Enter left it stuck open with no way to shut it.
 *   - The flakiness and telemetry tables made a whole `<tr>` the click target and
 *     that row was the only route to the drill-down modal, so a keyboard user
 *     could read the table and open nothing.
 *   - Nine form controls had `outline-none` with no replacement, so focusing them
 *     changed nothing a sighted keyboard user could see.
 *   - There was no skip link, so every page load began at the header and Tab had
 *     to walk all twelve sidebar destinations before reaching the content.
 */
import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { MemoryRouter } from 'react-router-dom';

import { Header } from './Header';
import { Sidebar } from './Sidebar';
import { FlakinessSection } from '../intelligence/FlakinessSection';
import { TelemetryTableSection } from '../intelligence/TelemetryTableSection';
import { AppShell } from '../../App';
import { AIQAProvider } from '../../context/AIQAContext';
import { ProjectProvider } from '../../context/ProjectContext';
import { RealtimeProvider } from '../../context/RealtimeContext';
import { ThemeProvider } from '../../context/ThemeContext';
import { AuthProvider } from '../../context/AuthContext';

/** Every header control that opens a popover. */
const POPOVER_TRIGGERS = ['env-selector-button', 'backend-health-button', 'notifications-button'];

/**
 * Both `Header` and `Sidebar` read the collapsed-sidebar flag from `AIQAContext`,
 * and both call `useNavigate`. The app shell supplies both layers in production,
 * so every render here does too -- otherwise the test would be exercising a
 * configuration that never ships.
 */
function renderShell(ui: React.ReactElement, route = '/dashboard') {
  // The provider chain is copied from `App.tsx`, in the same order and for the
  // same reason: each of these components reads one of these contexts, and a test
  // that supplied a different arrangement would be exercising a configuration
  // that never ships.
  return render(
    <ThemeProvider>
      <AuthProvider>
        <ProjectProvider>
          <RealtimeProvider>
            <AIQAProvider>
              <MemoryRouter initialEntries={[route]}>{ui}</MemoryRouter>
            </AIQAProvider>
          </RealtimeProvider>
        </ProjectProvider>
      </AuthProvider>
    </ThemeProvider>,
  );
}

// ------------------------------------------------------------ header popovers

describe('header popovers can be dismissed from the keyboard', () => {
  it.each(POPOVER_TRIGGERS)('%s closes on Escape', (id) => {
    renderShell(<Header />);
    const trigger = document.getElementById(id);
    expect(trigger).not.toBeNull();

    fireEvent.click(trigger!);
    const opened = document.body.innerHTML;
    expect(opened).not.toBe('');

    fireEvent.keyDown(document, { key: 'Escape' });

    // If this fails the popover stayed open: it closed on onMouseLeave only, which
    // never fires for a keyboard user, so there was no way out of it.
    expect(document.body.innerHTML).not.toBe(opened);
  });

  it.each(POPOVER_TRIGGERS)('%s declares its expanded state', (id) => {
    renderShell(<Header />);
    const trigger = document.getElementById(id);

    // Without `aria-expanded` a screen reader announces a plain button with no
    // destination, so there is nothing to tell the user it opens something.
    expect(trigger?.getAttribute('aria-expanded')).toBe('false');
    expect(trigger?.getAttribute('aria-haspopup')).toBeTruthy();

    fireEvent.click(trigger!);
    expect(document.getElementById(id)?.getAttribute('aria-expanded')).toBe('true');
  });

  it('a press outside an open popover closes it', () => {
    renderShell(<Header />);
    fireEvent.click(document.getElementById('env-selector-button')!);
    const opened = document.body.innerHTML;

    fireEvent.pointerDown(document.body);
    expect(document.body.innerHTML).not.toBe(opened);
  });

  it('a press inside the open popover does not close it', () => {
    renderShell(<Header />);
    fireEvent.click(document.getElementById('env-selector-button')!);

    // A press on a popover option is an ordinary interaction, not a dismissal --
    // closing on it would make the control unusable.
    const option = screen.getByRole('button', { name: /UAT/i });
    fireEvent.pointerDown(option);
    expect(screen.getByRole('button', { name: /UAT/i })).toBeDefined();
  });
});

// --------------------------------------------------------- clickable row keys

describe('a clickable table row is reachable without a mouse', () => {
  const flakyRow = { id: 'node-1', node_id: 'cap/login', flakiness_score: 0.42 } as never;
  const telemetryRow = {
    id: 'tel-1',
    task_name: 'click_login',
    task_id: 't-1',
    status: 'FAILED',
    environment: 'qa',
    duration_ms: 12,
    created_at: '2026-01-01T00:00:00Z',
  } as never;

  it('the flakiness row is focusable and says what it opens', () => {
    renderShell(<FlakinessSection flakiness={[flakyRow]} onRefresh={() => undefined} />);
    const el = screen.getByTestId('flakiness-row-node-1');

    expect(el.tagName).toBe('TR');
    // Reachable at all: without tabindex there is no way to get here.
    expect(el.getAttribute('tabindex')).toBe('0');
    expect(el.getAttribute('aria-label')).toMatch(/login/i);
  });

  it('Enter on the flakiness row opens the drill-down', () => {
    renderShell(<FlakinessSection flakiness={[flakyRow]} onRefresh={() => undefined} />);
    const el = screen.getByTestId('flakiness-row-node-1');

    fireEvent.keyDown(el, { key: 'Enter' });
    // The modal it opens is the observable proof the key was handled.
    expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  });

  it('Space on the flakiness row opens it too, like any button', () => {
    renderShell(<FlakinessSection flakiness={[flakyRow]} onRefresh={() => undefined} />);
    fireEvent.keyDown(screen.getByTestId('flakiness-row-node-1'), { key: ' ' });
    expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  });

  it('Enter on the telemetry row opens the detail modal', () => {
    let opened: unknown = null;
    renderShell(
      <TelemetryTableSection
        telemetry={[telemetryRow]}
        onSelectTelemetry={(row) => {
          opened = row;
        }}
        onRefresh={() => undefined}
      />,
    );

    const el = screen.getByTestId('telemetry-row-tel-1');
    expect(el.getAttribute('tabindex')).toBe('0');
    expect(el.getAttribute('aria-label')).toMatch(/click_login/);

    fireEvent.keyDown(el, { key: 'Enter' });
    expect(opened).toBe(telemetryRow);
  });
});

// -------------------------------------------------------------- focus visible

describe('focus is always visible', () => {
  /**
   * Structural, because a real focus indicator cannot be measured in jsdom: the
   * engine has no layout and Tailwind has not run. What this pins is that the
   * pattern which removes the native outline never appears without a replacement
   * on the same element -- which is the exact shape of the defect. Nine controls
   * had `focus:outline-none` sitting alone in the class list, so focusing them
   * changed nothing.
   */
  it('no control removes its outline without providing a replacement', async () => {
    const { readFileSync, globSync } = await import('node:fs');

    const offenders: string[] = [];
    for (const file of globSync('src/**/*.tsx', { exclude: ['**/*.test.tsx'] })) {
      const source = readFileSync(file, 'utf8');
      for (const match of source.matchAll(/className="([^"]*outline-none[^"]*)"/g)) {
        if (!/focus(-visible)?:(ring|border|bg|shadow)/.test(match[1])) {
          offenders.push(`${file}:${source.slice(0, match.index).split('\n').length}`);
        }
      }
    }
    expect(
      offenders,
      `these elements drop the native focus ring with nothing in its place, so a keyboard \
       user cannot see where they are: ${offenders.join(', ')}`,
    ).toEqual([]);
  });
});

// --------------------------------------------------------------- the sidebar

describe('the sidebar says which page you are on', () => {
  it('marks exactly the current destination with aria-current', () => {
    renderShell(<Sidebar />);
    const current = document.querySelectorAll('[aria-current="page"]');
    expect(current.length).toBeGreaterThan(0);
    // Exactly one. Two would mean the control cannot say which page you are on,
    // which is the same problem as none.
    expect(current.length).toBe(1);
  });

  it('moves the marker when the route changes', () => {
    // Scoped to each render's own container: rendering twice into one document
    // would leave the first sidebar in the tree, and a document-wide query would
    // then match both and always compare the first one against itself.
    const onDashboard = renderShell(<Sidebar />, '/dashboard');
    const first = onDashboard.container
      .querySelector('[aria-current="page"]')
      ?.textContent;

    const onSettings = renderShell(<Sidebar />, '/settings');
    const second = onSettings.container.querySelector('[aria-current="page"]')?.textContent;

    expect(first).toBeDefined();
    expect(second).toBeDefined();
    expect(second).not.toBe(first);
  });

  it('names its navigation landmark', () => {
    renderShell(<Sidebar />);
    // A bare "navigation" is ambiguous on a page with more than one nav.
    expect(screen.getByRole('navigation', { name: /primary/i })).toBeDefined();
  });
});

// ------------------------------------------------------------- the skip link

describe('there is a way past the navigation', () => {
  it('renders a skip link targeting the content', () => {
    renderShell(<AppShell />);

    const skip = screen.getByRole('link', { name: /skip to main content/i });
    expect(skip.getAttribute('href')).toBe('#main-content-area');
    // The target has to exist, or the link goes nowhere.
    expect(document.getElementById('main-content-area')).not.toBeNull();
  });

  it('is hidden until focused, then visible', () => {
    renderShell(<AppShell />);
    const skip = screen.getByRole('link', { name: /skip to main content/i });

    // `sr-only` alone would hide it from everyone, including the keyboard user it
    // exists for, so the focus-visible counterpart is part of the fix.
    expect(skip.className).toMatch(/sr-only/);
    expect(skip.className).toMatch(/focus:not-sr-only/);
  });

  it('is the first thing in the tab order, ahead of the header controls', () => {
    renderShell(<AppShell />);
    const first = document.querySelector<HTMLElement>(
      'a[href], button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])',
    );
    // Otherwise every page load begins at the header and Tab has to walk all
    // twelve sidebar destinations before reaching the content.
    expect(first?.textContent).toMatch(/skip to main content/i);
  });
});
