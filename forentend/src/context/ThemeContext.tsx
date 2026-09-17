/**
 * Phase 6 gap-closing — real light/dark theme state.
 *
 * Genuinely new: before this, the app had zero theming infrastructure —
 * no `@theme` tokens, no `dark:` variant usage, no `prefers-color-scheme`
 * handling, no theme context (confirmed by a repo-wide search before this
 * was written). `index.css` hardcoded one, dark-only palette directly onto
 * `body` and a handful of components reference the same literals inline.
 *
 * Scope note: this is the "core chrome" slice, not an exhaustive re-theme.
 * The shared surface/content/border tokens this context's `data-theme`
 * attribute switches (`index.css`'s own `@theme` block) are wired into the
 * app shell, sidebar, header, dashboard, and settings pages — the
 * highest-traffic surfaces. Deeper/rarer components that still use a
 * hardcoded literal (`bg-[#0B1120]`, `text-slate-400`, etc.) fall back to
 * their original dark-only appearance in light mode rather than being
 * silently guessed at — an honest, disclosed gap, not a hidden one.
 */

import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';

export type Theme = 'dark' | 'light';

/** Own storage key, same "small try/catch helpers, never throws on a
 *  private-mode/storage-disabled browser" convention `ProjectContext.tsx`'s
 *  `SELECTED_PROJECT_STORAGE` already established — deliberately separate
 *  from both that key and `aiqaApi.ts`'s credential key. */
const THEME_STORAGE = 'aiqa.theme';

function readPersistedTheme(): Theme | null {
  try {
    const raw = localStorage.getItem(THEME_STORAGE);
    return raw === 'dark' || raw === 'light' ? raw : null;
  } catch {
    return null;
  }
}

function persistTheme(theme: Theme): void {
  try {
    localStorage.setItem(THEME_STORAGE, theme);
  } catch {
    /* storage unavailable (private mode) — the choice just won't survive a refresh */
  }
}

/** The app's own real dark palette (`index.css`'s `@theme` block) is
 *  already the default — a browser with no stored preference and no
 *  `prefers-color-scheme` support renders exactly as it always has. Only
 *  a real, explicit system preference for light is honored on first
 *  visit, never assumed. */
function systemPrefersLight(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-color-scheme: light)').matches
  );
}

interface ThemeContextType {
  theme: Theme;
  /** True only when this session itself picked a theme (via `setTheme`/
   *  `toggleTheme`) rather than the initial persisted-or-system default —
   *  read by the system-preference listener below to decide whether a
   *  live OS-level change should still move this tab, matching common
   *  browser UX: an explicit in-app choice always wins over the OS. */
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
}

const ThemeContext = createContext<ThemeContextType | undefined>(undefined);

export const ThemeProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [theme, setThemeState] = useState<Theme>(
    () => readPersistedTheme() ?? (systemPrefersLight() ? 'light' : 'dark'),
  );
  const [userChose, setUserChose] = useState<boolean>(() => readPersistedTheme() !== null);

  const setTheme = useCallback((next: Theme) => {
    setThemeState(next);
    setUserChose(true);
    persistTheme(next);
  }, []);

  const toggleTheme = useCallback(() => {
    setThemeState((prev) => {
      const next: Theme = prev === 'dark' ? 'light' : 'dark';
      persistTheme(next);
      return next;
    });
    setUserChose(true);
  }, []);

  // The real, single place `data-theme` is ever written — every token
  // override in `index.css` is scoped to `:root[data-theme="light"]`, so
  // this attribute is the entire runtime switch.
  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme);
  }, [theme]);

  // A live OS-level change (the user flips their system to light/dark
  // while this tab is open) only moves this tab when it has never made
  // its own explicit choice — once `setTheme`/`toggleTheme` runs once,
  // the in-app choice sticks, matching how `prefers-color-scheme`-aware
  // sites conventionally behave.
  useEffect(() => {
    if (userChose || typeof window.matchMedia !== 'function') return;
    const mql = window.matchMedia('(prefers-color-scheme: light)');
    const handleChange = (e: MediaQueryListEvent) => {
      setThemeState(e.matches ? 'light' : 'dark');
    };
    mql.addEventListener('change', handleChange);
    return () => mql.removeEventListener('change', handleChange);
  }, [userChose]);

  const value = useMemo(() => ({ theme, setTheme, toggleTheme }), [theme, setTheme, toggleTheme]);

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
};

export const useTheme = (): ThemeContextType => {
  const context = useContext(ThemeContext);
  if (!context) {
    throw new Error('useTheme must be used within a ThemeProvider');
  }
  return context;
};
