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

const ProtectedRoute: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const auth = useAuth();
  const navigate = useNavigate();
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
  </Routes>
);
