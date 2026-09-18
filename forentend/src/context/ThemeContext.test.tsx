/**
 * Phase 6 gap-closing — targeted tests for the real theme context: default
 * resolution (stored preference, then real system preference, then the
 * app's existing dark-only default), persistence, the real `data-theme`
 * attribute this context is the one place that ever writes, and that an
 * explicit in-app choice wins over a later system-level change.
 */
import React from 'react';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ThemeProvider, useTheme } from './ThemeContext';

const THEME_STORAGE = 'aiqa.theme';

/** jsdom does not implement `matchMedia` — this stubs just enough of it
 *  for `(prefers-color-scheme: light)` queries, with a real, working
 *  `addEventListener`/`removeEventListener` pair so the live-change
 *  listener test can actually fire a change event, not just assert the
 *  call was attempted. */
function mockMatchMedia(prefersLight: boolean) {
  const listeners = new Set<(e: MediaQueryListEvent) => void>();
  const mql = {
    matches: prefersLight,
    media: '(prefers-color-scheme: light)',
    addEventListener: (_: 'change', cb: (e: MediaQueryListEvent) => void) => listeners.add(cb),
    removeEventListener: (_: 'change', cb: (e: MediaQueryListEvent) => void) => listeners.delete(cb),
  };
  vi.stubGlobal(
    'matchMedia',
    vi.fn().mockImplementation(() => mql),
  );
  return {
    fireChange: (matches: boolean) => {
      mql.matches = matches;
      listeners.forEach((cb) => cb({ matches } as MediaQueryListEvent));
    },
  };
}

const wrapper = ({ children }: { children: React.ReactNode }) => <ThemeProvider>{children}</ThemeProvider>;

beforeEach(() => {
  localStorage.clear();
  document.documentElement.removeAttribute('data-theme');
});

afterEach(() => {
  localStorage.clear();
  document.documentElement.removeAttribute('data-theme');
  vi.unstubAllGlobals();
});

describe('ThemeContext — real theme resolution and persistence', () => {
  it('defaults to dark when nothing is stored and the system has no light preference', () => {
    mockMatchMedia(false);
    const { result } = renderHook(() => useTheme(), { wrapper });
    expect(result.current.theme).toBe('dark');
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
  });

  it('honors a real system preference for light on first visit, never assumed', () => {
    mockMatchMedia(true);
    const { result } = renderHook(() => useTheme(), { wrapper });
    expect(result.current.theme).toBe('light');
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
  });

  it('a stored preference overrides the system preference', () => {
    localStorage.setItem(THEME_STORAGE, 'light');
    mockMatchMedia(false); // system says dark — the stored choice must still win
    const { result } = renderHook(() => useTheme(), { wrapper });
    expect(result.current.theme).toBe('light');
  });

  it('toggleTheme flips the real theme and persists it', () => {
    mockMatchMedia(false);
    const { result } = renderHook(() => useTheme(), { wrapper });
    expect(result.current.theme).toBe('dark');

    act(() => result.current.toggleTheme());
    expect(result.current.theme).toBe('light');
    expect(localStorage.getItem(THEME_STORAGE)).toBe('light');
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');

    act(() => result.current.toggleTheme());
    expect(result.current.theme).toBe('dark');
    expect(localStorage.getItem(THEME_STORAGE)).toBe('dark');
  });

  it('setTheme sets an explicit theme and persists it', () => {
    mockMatchMedia(false);
    const { result } = renderHook(() => useTheme(), { wrapper });

    act(() => result.current.setTheme('light'));
    expect(result.current.theme).toBe('light');
    expect(localStorage.getItem(THEME_STORAGE)).toBe('light');
  });

  it('a live system-preference change moves the theme only before any explicit in-app choice', async () => {
    const media = mockMatchMedia(false);
    const { result } = renderHook(() => useTheme(), { wrapper });
    expect(result.current.theme).toBe('dark');

    // No explicit choice made yet — a real OS-level flip to light still
    // moves this tab, matching ordinary `prefers-color-scheme` behavior.
    act(() => media.fireChange(true));
    await waitFor(() => expect(result.current.theme).toBe('light'));

    // Now the user makes their own explicit choice…
    act(() => result.current.setTheme('dark'));
    expect(result.current.theme).toBe('dark');

    // …and a further OS-level change must no longer override it.
    act(() => media.fireChange(true));
    expect(result.current.theme).toBe('dark');
  });

  it('useTheme throws when used outside a ThemeProvider, same convention every other context here follows', () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => renderHook(() => useTheme())).toThrow('useTheme must be used within a ThemeProvider');
    consoleError.mockRestore();
  });
});
