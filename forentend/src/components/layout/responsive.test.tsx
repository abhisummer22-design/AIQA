/**
 * Phase F — the layout that had no narrow-width story at all.
 *
 * The sidebar was a permanent column at every width: `w-16` or `w-64`, with no
 * breakpoint, no positioning, no overlay and no mobile open state. The only way
 * to change it was to collapse it between two desktop widths, so on a phone a
 * permanent 256px column left 119px of content, and even collapsed it consumed
 * 17% of the viewport. The header's hamburger and the sidebar's own collapse
 * button were the same control with the same effect at every size.
 *
 * Below 768px the sidebar is now an overlay drawer: off-canvas by default, opened
 * by the header's hamburger, dismissed by Escape, by the backdrop, or by choosing
 * a destination.
 *
 * What this file deliberately does NOT do is assert a pixel width. No browser was
 * attached when this was written, so there is no measurement behind any figure and
 * none is claimed. What is asserted is behaviour, which does not need a browser.
 */
import React from 'react';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { MemoryRouter } from 'react-router-dom';

import { Sidebar, matchesMedia } from './Sidebar';
import { Header } from './Header';
import { AIQAProvider } from '../../context/AIQAContext';
import { ProjectProvider } from '../../context/ProjectContext';
import { RealtimeProvider } from '../../context/RealtimeContext';
import { AuthProvider } from '../../context/AuthContext';
import { ThemeProvider } from '../../context/ThemeContext';

const MOBILE = '(max-width: 767px)';

/** Installs a `matchMedia` that reports one answer, and tracks its listeners. */
function stubViewport(mobile: boolean) {
  const listeners = new Set<() => void>();
  const list = {
    matches: mobile,
    media: MOBILE,
    addEventListener: (_: string, cb: () => void) => listeners.add(cb),
    removeEventListener: (_: string, cb: () => void) => listeners.delete(cb),
    addListener: (cb: () => void) => listeners.add(cb),
    removeListener: (cb: () => void) => listeners.delete(cb),
    onchange: null,
    dispatchEvent: () => false,
  };
  const original = window.matchMedia;
  window.matchMedia = vi.fn().mockImplementation((q: string) =>
    q.includes('max-width') ? list : { ...list, matches: false, media: q },
  ) as unknown as typeof window.matchMedia;
  return {
    resize(nextMobile: boolean) {
      list.matches = nextMobile;
      // Dispatched with an event, because every registered listener shares this
      // one stub and some of them -- ThemeContext's own scheme listener -- read
      // `event.matches`. Calling them bare takes down an unrelated provider.
      act(() =>
        listeners.forEach((cb) => cb({ matches: nextMobile } as MediaQueryListEvent)),
      );
    },
    original,
  };
}

function renderShell(ui: React.ReactElement, route = '/dashboard') {
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

afterEach(() => {
  vi.restoreAllMocks();
});

describe('at a desktop width the sidebar is a column', () => {
  beforeEach(() => {
    stubViewport(false);
  });

  it('renders the column, not a drawer', () => {
    renderShell(<Sidebar />);
    expect(screen.getByTestId('sidebar-column')).toBeDefined();
    expect(screen.queryByTestId('sidebar-drawer')).toBeNull();
  });

  it('keeps its collapse button, which is hidden below md', () => {
    renderShell(<Sidebar />);
    const collapse = screen.getByTitle(/Collapse Sidebar|Expand Sidebar/i);
    expect(collapse.className).toMatch(/hidden/);
    expect(collapse.className).toMatch(/md:flex/);
  });
});

describe('at a phone width the sidebar is a drawer', () => {
  beforeEach(() => {
    stubViewport(true);
  });

  it('renders the drawer, off-canvas and closed', () => {
    renderShell(<Sidebar />);
    const drawer = screen.getByTestId('sidebar-drawer');

    expect(screen.queryByTestId('sidebar-column')).toBeNull();
    expect(drawer.getAttribute('data-open')).toBe('false');
    // Off-canvas rather than "collapsed to 64px": a 256px column at 375px leaves
    // 119px of content, which is the problem this whole change is about.
    expect(drawer.className).toMatch(/-translate-x-full/);
    expect(screen.queryByTestId('sidebar-backdrop')).toBeNull();
  });

  it('opens when the header asks it to, and shows a backdrop', () => {
    renderShell(<Sidebar />);
    act(() => {
      window.dispatchEvent(new Event('aiqa:open-sidebar'));
    });

    expect(screen.getByTestId('sidebar-drawer').getAttribute('data-open')).toBe('true');
    expect(screen.getByTestId('sidebar-drawer').className).toMatch(/translate-x-0/);
    // The backdrop is what makes the drawer feel modal rather than merely present.
    expect(screen.getByTestId('sidebar-backdrop')).toBeDefined();
  });

  it('closes on Escape', () => {
    renderShell(<Sidebar />);
    act(() => {
      window.dispatchEvent(new Event('aiqa:open-sidebar'));
    });
    expect(screen.getByTestId('sidebar-drawer').getAttribute('data-open')).toBe('true');

    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.getByTestId('sidebar-drawer').getAttribute('data-open')).toBe('false');
  });

  it('closes when the backdrop is pressed', () => {
    renderShell(<Sidebar />);
    act(() => {
      window.dispatchEvent(new Event('aiqa:open-sidebar'));
    });
    fireEvent.click(screen.getByTestId('sidebar-backdrop'));
    expect(screen.getByTestId('sidebar-drawer').getAttribute('data-open')).toBe('false');
  });

  it('closes after a destination is chosen', () => {
    renderShell(<Sidebar />);
    act(() => {
      window.dispatchEvent(new Event('aiqa:open-sidebar'));
    });
    fireEvent.click(screen.getByRole('button', { name: /Settings/i }));
    // Someone who navigated somewhere should be looking at it, not at the menu.
    expect(screen.getByTestId('sidebar-drawer').getAttribute('data-open')).toBe('false');
  });

  it('does not strand the drawer open when the viewport grows back', () => {
    const viewport = stubViewport(true);
    renderShell(<Sidebar />);
    act(() => {
      window.dispatchEvent(new Event('aiqa:open-sidebar'));
    });
    expect(screen.getByTestId('sidebar-drawer').getAttribute('data-open')).toBe('true');

    act(() => viewport.resize(false));
    // Otherwise the drawer stays open over content its backdrop no longer covers.
    expect(screen.queryByTestId('sidebar-drawer')).toBeNull();
    expect(screen.getByTestId('sidebar-column')).toBeDefined();
  });
});

describe('the header hamburger drives whichever surface exists', () => {
  it('opens the drawer on a phone', () => {
    stubViewport(true);
    renderShell(
      <>
        <Header />
        <Sidebar />
      </>,
    );

    fireEvent.click(document.getElementById('sidebar-toggle-button')!);
    expect(screen.getByTestId('sidebar-drawer').getAttribute('data-open')).toBe('true');
  });

  it('is a mobile-only control, so it does not duplicate the sidebar collapse button', () => {
    stubViewport(true);
    renderShell(<Header />);
    const button = document.getElementById('sidebar-toggle-button');
    // `md:hidden`: on desktop the sidebar carries its own collapse control, and
    // two controls doing one thing is how they drift apart.
    expect(button?.className).toMatch(/md:hidden/);
    // And it says what it does and what it controls.
    expect(button?.getAttribute('aria-label')).toMatch(/navigation/i);
    expect(button?.getAttribute('aria-controls')).toBe('main-sidebar');
  });
});

describe('matchesMedia is safe where the API does not exist', () => {
  /**
   * jsdom does not implement `window.matchMedia`, and an unguarded call took down
   * 595 tests the first time this was written. This pins the guard that prevents
   * a recurrence, because "it works in a real browser" is not a defence.
   */
  it('returns false instead of throwing when matchMedia is unavailable', () => {
    const original = window.matchMedia;
    (window as unknown as { matchMedia?: unknown }).matchMedia = undefined;
    try {
      expect(matchesMedia(MOBILE)).toBe(false);
    } finally {
      window.matchMedia = original;
    }
  });

  it('renders the sidebar at all with matchMedia missing', () => {
    const original = window.matchMedia;
    (window as unknown as { matchMedia?: unknown }).matchMedia = undefined;
    try {
      renderShell(<Sidebar />);
      // Desktop layout, which is the one that works without the API.
      expect(screen.getByTestId('sidebar-column')).toBeDefined();
    } finally {
      window.matchMedia = original;
    }
  });
});

describe('header overflow is never silently clipped', () => {
  /**
   * The harmful part of a too-wide header was not cosmetic. `body` sets
   * `overflow-x: hidden` and the app shell sets `overflow-hidden`, so anything
   * past the right edge was clipped with no way to scroll to it -- the controls
   * were genuinely unreachable. This pins the safety net.
   *
   * It cannot pin a pixel width, because jsdom has no layout and Tailwind has not
   * run. So it pins the mechanism that makes overflow reachable instead.
   */
  it('the header can be scrolled horizontally rather than clipping its overflow', () => {
    stubViewport(true);
    renderShell(<Header />);
    const header = document.querySelector('header');
    expect(header?.className).toMatch(/overflow-x-auto/);
  });
});