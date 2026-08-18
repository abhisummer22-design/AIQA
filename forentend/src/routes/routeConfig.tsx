/**
 * FE-4 — the single, centralized route definition every navigation surface
 * (Sidebar, CommandPalette, the router itself) reads from. No component
 * outside this file hardcodes a path string for one of these 12 pages —
 * this is what "do not scatter hardcoded navigation paths throughout
 * components" (this sprint's brief §5) means in practice.
 *
 * FRD/SRD/PRD/flow-diagram note (brief §3, §20): no such documents exist
 * anywhere in this repository (confirmed by an exhaustive repo-wide
 * search — see the FE-4 report §14 for the exact search performed and its
 * result). In their absence, route naming/scope was reconciled against the
 * next-best actually-authoritative source: this codebase's own existing,
 * already-product-reviewed page vocabulary (`types.ts`'s `NavPage`,
 * `Sidebar.tsx`'s existing labels) — not the brief's own illustrative
 * route-name list, several of which (`/analytics`, `/findings`, `/audit`,
 * `/evidence`, `/ui-testing`, `/security-testing`, `/performance-testing`)
 * have no corresponding page in `forentend` today and are therefore NOT
 * implemented as routes here (implementing them would mean fabricating new
 * pages, which is explicitly out of scope: "FE-4 is NOT the phase for
 * implementing real backend pages").
 *
 * Product-design status (brief §1, §18) — exactly two pages are
 * product-owner-designed references and were not touched beyond a single
 * `setActivePage` → `useNavigate` call each (§ "Dashboard"/"Test
 * Generator" below); the other nine remain Google-Studio-generated and are
 * explicitly NOT treated as approved final design — routed here, visually
 * untouched, so navigation exists without locking in incorrect UI.
 */

import type { ComponentType } from 'react';
import type { LucideIcon } from 'lucide-react';
import {
  LayoutDashboard,
  Sparkles,
  Workflow,
  CheckCheck,
  PlayCircle,
  ShieldCheck,
  Network,
  Smartphone,
  Sliders,
  Layers,
  FileText,
  Settings,
} from 'lucide-react';
import { DashboardPage } from '../components/pages/DashboardPage';
import { TestCaseGeneratorPage } from '../components/pages/TestCaseGeneratorPage';
import { AIQAOrchestratorPage } from '../components/pages/AIQAOrchestratorPage';
import { TestCaseValidatorPage } from '../components/pages/TestCaseValidatorPage';
import { TestExecutionPage } from '../components/pages/TestExecutionPage';
import { RBACTestingPage } from '../components/pages/RBACTestingPage';
import { APITestingPage } from '../components/pages/APITestingPage';
import { MobileTestingPage } from '../components/pages/MobileTestingPage';
import { CustomTestingPage } from '../components/pages/CustomTestingPage';
import { FeatureMapFlowPage } from '../components/pages/FeatureMapFlowPage';
import { ReportsPage } from '../components/pages/ReportsPage';
import { SettingsPage } from '../components/pages/SettingsPage';
import type { NavPage } from '../types';

export interface RouteDef {
  id: NavPage;
  /** Absolute path, e.g. `/dashboard`. */
  path: string;
  label: string;
  icon: LucideIcon;
  badge?: string;
  badgeType?: 'info' | 'warning' | 'live';
  Component: ComponentType;
  /** Product-design status (brief §1/§18) — for the report and for
   *  anyone reading this file; not rendered as UI (adding a visible
   *  "not final design" banner would itself be a small visual redesign of
   *  these pages, which is out of scope). */
  designStatus: 'product-owner-approved' | 'google-studio-not-final';
  /**
   * Whether this route requires a real, backend-verified session
   * (`AuthContext.status === 'authenticated'`) before its content renders
   * — enforced via `StateGate` in `AppRoutes.tsx`, not a second auth
   * mechanism (brief §6, §14).
   *
   * Settings is the one deliberate exception: it must stay reachable
   * regardless of auth state, because it is where the credential is
   * configured in the first place — gating it would deadlock a user who
   * has no key yet (brief's own note-worthy omission: §6 does not name
   * Settings, but §14's "route guards do not create a second
   * authentication mechanism" and plain deadlock-avoidance both point the
   * same way). This is a resolved, documented judgment call, not a
   * silently-invented one — see the FE-4 report §14.
   */
  protectedRoute: boolean;
}

export const routeDefs: RouteDef[] = [
  {
    id: 'dashboard',
    path: '/dashboard',
    label: 'Dashboard',
    icon: LayoutDashboard,
    Component: DashboardPage,
    designStatus: 'product-owner-approved',
    protectedRoute: true,
  },
  {
    id: 'generator',
    path: '/test-generator',
    label: 'Test Case Generator',
    icon: Sparkles,
    badge: 'AI',
    Component: TestCaseGeneratorPage,
    designStatus: 'product-owner-approved',
    protectedRoute: true,
  },
  {
    id: 'orchestrator',
    path: '/orchestrator',
    label: 'AIQA Orchestrator',
    icon: Workflow,
    badge: 'PLAN',
    Component: AIQAOrchestratorPage,
    designStatus: 'google-studio-not-final',
    protectedRoute: true,
  },
  {
    id: 'validator',
    path: '/validator',
    label: 'Test Case Validator',
    icon: CheckCheck,
    badge: 'GATE',
    Component: TestCaseValidatorPage,
    designStatus: 'google-studio-not-final',
    protectedRoute: true,
  },
  {
    id: 'execution',
    path: '/execution',
    label: 'Test Execution Hub',
    icon: PlayCircle,
    badge: 'LIVE',
    badgeType: 'live',
    Component: TestExecutionPage,
    designStatus: 'google-studio-not-final',
    protectedRoute: true,
  },
  {
    id: 'rbac',
    path: '/rbac',
    label: 'RBAC Testing',
    icon: ShieldCheck,
    Component: RBACTestingPage,
    designStatus: 'google-studio-not-final',
    protectedRoute: true,
  },
  {
    id: 'api-testing',
    path: '/api-testing',
    label: 'API Testing',
    icon: Network,
    Component: APITestingPage,
    designStatus: 'google-studio-not-final',
    protectedRoute: true,
  },
  {
    id: 'mobile-testing',
    path: '/mobile-testing',
    label: 'Mobile Testing',
    icon: Smartphone,
    Component: MobileTestingPage,
    designStatus: 'google-studio-not-final',
    protectedRoute: true,
  },
  {
    id: 'custom',
    path: '/custom-testing',
    label: 'Custom Testing',
    icon: Sliders,
    Component: CustomTestingPage,
    designStatus: 'google-studio-not-final',
    protectedRoute: true,
  },
  {
    id: 'feature-map',
    path: '/feature-map',
    label: 'Feature Map / Flow',
    icon: Layers,
    Component: FeatureMapFlowPage,
    designStatus: 'google-studio-not-final',
    protectedRoute: true,
  },
  {
    id: 'reports',
    path: '/reports',
    label: 'Reports',
    icon: FileText,
    Component: ReportsPage,
    designStatus: 'google-studio-not-final',
    protectedRoute: true,
  },
  {
    id: 'settings',
    path: '/settings',
    label: 'Settings / AI Model',
    icon: Settings,
    Component: SettingsPage,
    designStatus: 'google-studio-not-final',
    // Deliberately NOT protected — see `protectedRoute`'s doc comment.
    protectedRoute: false,
  },
];

/** The route a bare `/` (and any `NavPage` value that no longer resolves)
 *  redirects to. FRD/SRD/PRD do not exist to consult (see this file's
 *  module doc comment); this preserves the app's own pre-existing default
 *  — `AIQAContext` already defaulted `activePage` to `'dashboard'`, and
 *  the old `MainContentArea` switch's own `default` case already rendered
 *  `<DashboardPage />` — rather than inventing a new entry-flow decision. */
export const DEFAULT_ROUTE_PATH = '/dashboard';

export function routeDefFor(id: NavPage): RouteDef | undefined {
  return routeDefs.find((r) => r.id === id);
}
