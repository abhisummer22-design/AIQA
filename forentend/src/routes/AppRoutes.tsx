/**
 * FE-4 — the actual `<Routes>` table, built from `routeConfig.tsx`'s
 * single route list, plus the root redirect and the 404 fallback.
 *
 * Route protection (brief §6): each protected route's element is wrapped
 * in `ProtectedRoute`, which reads the real `AuthContext` (via `useAuth`)
 * and renders through FE-3's own `StateGate` — no second authentication
 * state machine, no route guard that bypasses `AuthContext`.
 */
import React from 'react';
import { Navigate, Route, Routes, useNavigate } from 'react-router-dom';
import { useAuth } from '../context/AuthContext';
import { StateGate } from '../components/state/StateViews';
import { NotFoundPage } from '../components/pages/NotFoundPage';
import { DEFAULT_ROUTE_PATH, routeDefs } from './routeConfig';

/**
 * Verifies a locally-stored key once on mount, for EVERY route.
 *
 * This used to live only inside `ProtectedRoute`, so it ran only on protected
 * routes -- and `/settings` is deliberately unprotected (`routeConfig.tsx`'s
 * `protectedRoute: false`, so a user with no key is not deadlocked on the one
 * page where a key can be configured). The consequence was that the page whose
 * entire job is reporting credential state could never report a verified one:
 * `AuthContext.status` stayed at its initial `key_present` forever, Settings
 * showed "Key saved, not yet verified", and the header's health indicator said
 * "Not Confirmed Online" -- while the very same key, in the very same browser
 * session, produced "Orchestrator Online" on /dashboard. Live-verified as a red
 * dot next to a green one on two pages at once.
 *
 * Kept out of `ProtectedRoute`, which still calls it: that call is now
 * redundant rather than wrong, and removing it would couple auth verification to
 * navigation. This effect runs once per mount regardless of route.
 */
const AuthVerifier: React.FC = () => {
  const auth = useAuth();
  const { validate, status } = auth;
  React.useEffect(() => {
    if (status === 'key_present') {
      void validate();
    }
  }, [status, validate]);
  return null;
};

const ProtectedRoute: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const auth = useAuth();
  const navigate = useNavigate();
  const { validate } = auth;

  // Background verification on mount when key exists locally
  React.useEffect(() => {
    if (auth.status === 'key_present') {
      void validate();
    }
  }, [auth.status, validate]);

  // Gives the auth-state card shown on any protected route a real,
  // working path back to the one place a key can actually be configured
  // — without this, a `no_key`/`unauthenticated` card reached by direct
  // URL navigation would have no "Configure API Key" action at all
  // (`renderAuthState` only renders that CTA when `onConfigure` is given).
  return (
    <StateGate auth={auth} actions={{ onConfigure: () => navigate('/settings') }}>
      {children}
    </StateGate>
  );
};

export const AppRoutes: React.FC = () => (
  <Routes>
    <Route path="/" element={<Navigate to={DEFAULT_ROUTE_PATH} replace />} />
    {routeDefs.map((route) => {
      const Component = route.Component;
      const page = route.protectedRoute ? (
        <ProtectedRoute>
          <Component />
        </ProtectedRoute>
      ) : (
        <Component />
      );
      // `React.createElement` rather than JSX here: react-router's `Route`
      // is typed as a bare `(props: RouteProps) => ReactElement` function
      // (not `React.FC`), which this project's React 19 type resolution
      // does not treat as `key`-aware through the JSX transform — a real
      // upstream type-declaration mismatch, not a runtime issue (`<Route
      // key={...} .../>` and `createElement(Route, {key, ...})` compile to
      // the exact same call). This keeps `Route` as a direct child of
      // `<Routes>`, which is what its runtime route-matching expects.
      return React.createElement(Route, { key: route.id, path: route.path, element: page });
    })}
    <Route path="*" element={<NotFoundPage />} />
    {/* Mounted inside `<Routes>` but rendering nothing: it needs no route of its
        own, only a place that stays mounted across navigation. */}
    <Route path="/" element={<AuthVerifier />} />
  </Routes>
);
