import { useEffect, useState } from 'react';

/**
 * Whether the user has asked the OS to reduce motion.
 *
 * `window.matchMedia` is the only source of truth for this, and it is read
 * through a listener rather than once at mount: a user can change the setting
 * while the app is open, and a hook that captured it on mount would keep
 * animating for them indefinitely.
 *
 * Returns `false` during SSR and in any environment without `matchMedia` (jsdom
 * before a test stubs it). That default is deliberate: an app that cannot ask the
 * question should animate as it always has, rather than freeze for everyone whose
 * environment happens to be missing an API.
 */
export function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState(false);

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
      return;
    }
    const query = window.matchMedia('(prefers-reduced-motion: reduce)');
    setReduced(query.matches);

    // `addEventListener` is the modern spelling; Safari < 14 only has the
    // deprecated `addListener`, and this app is not desktop-only.
    const onChange = (event: MediaQueryListEvent) => setReduced(event.matches);
    if (typeof query.addEventListener === 'function') {
      query.addEventListener('change', onChange);
      return () => query.removeEventListener('change', onChange);
    }
    query.addListener(onChange);
    return () => query.removeListener(onChange);
  }, []);

  return reduced;
}