/**
 * FE-4 — the single, centralized route definition every navigation surface
 * (Sidebar, CommandPalette, the router itself) reads from. No component
 * outside this file hardcodes a path string for one of these 14 pages —
 * this is what "do not scatter hardcoded navigation paths throughout
 * components" (FE-4's brief §5) means in practice.
 *
 * FRD/SRD/PRD/flow-diagram note (FE-4 brief §3, §20; re-checked every FE
 * unit since): no such documents exist anywhere in this repository
 * (confirmed by an exhaustive repo-wide search, re-run at the start of
 * every FE unit — see each unit's own report). In their absence, route
 * naming/scope was reconciled against the next-best actually-authoritative
 * source: this codebase's own existing, already-product-reviewed page
 * vocabulary (`types.ts`'s `NavPage`, `Sidebar.tsx`'s existing labels)
 * plus, from FE-6 onward, the real backend contract — not FE-4's brief's
 * own illustrative route-name list, several of which (`/analytics`,
 * `/findings`, `/ui-testing`, `/security-testing`, `/performance-testing`)
 * still have no corresponding page/backend support in `forentend` today
 * and remain NOT implemented as routes here. `/audit` and `/evidence` were
 * both on that original list too, but FE-6 and FE-7 respectively
 * implemented each for real once its own real backend endpoint was
 * confirmed to exist — see `AuditPage.tsx`'s and `EvidencePage.tsx`'s own
 * doc comments.
 *
 * Product-design status (brief §1, §18) — exactly two pages are
 * product-owner-designed references and were not touched beyond a single
 * `setActivePage` → `useNavigate` call each (§ "Dashboard"/"Test
 * Generator" below); nine remain Google-Studio-generated and are explicitly
 * NOT treated as approved final design; `audit` and `evidence` are a
 * third, distinct bucket — built fresh in FE-6/FE-7 following the approved
 * pages' visual language, but not themselves product-reviewed (see
 * `RouteDef.designStatus`'s own doc comment for why neither is folded into
 * either existing bucket).
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
  ScrollText,
  FileSearch,
  Settings,
} from 'lucide-react';
import { DashboardPage } from '../components/pages/DashboardPage';
import { TestCaseGeneratorPage } from '../components/pages/TestCaseGeneratorPage';
import { AIQAOrchestratorPage } from '../components/pages/AIQAOrchestratorPage';
import { TestCaseValidatorPage } from '../components/pages/TestCaseValidatorPage';
import { ExecutionHubPage } from '../components/pages/ExecutionHubPage';
import { RBACTestingPage } from '../components/pages/RBACTestingPage';
import { APITestingPage } from '../components/pages/APITestingPage';
import { MobileTestingPage } from '../components/pages/MobileTestingPage';
import { CustomTestingPage } from '../components/pages/CustomTestingPage';
import { FeatureMapFlowPage } from '../components/pages/FeatureMapFlowPage';
import { ReportsPage } from '../components/pages/ReportsPage';
import { AuditPage } from '../components/pages/AuditPage';
import { EvidencePage } from '../components/pages/EvidencePage';
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
   *  these pages, which is out of scope).
   *
   *  `new-follows-approved-language` (introduced FE-6, reused FE-7): a page
   *  built fresh in a later FE unit (not a pre-existing Google-Studio page,
   *  and not one of the two approved-page files themselves) that follows
   *  the Dashboard/Test Generator visual language by construction — an
   *  honest third bucket, since calling it `google-studio-not-final` would
   *  misstate its provenance, and `product-owner-approved` would falsely
   *  claim a product review that has not happened. Deliberately named
   *  sprint-agnostically (not `fe6-...`) so any later unit's new page can
   *  reuse it without another rename. Currently `AuditPage` (FE-6),
   *  `EvidencePage` (FE-7), `ExecutionHubPage` (FE-8), and `ReportsPage`
   *  (FE-11 — rebuilt onto the real Findings/FailureCluster backend).
   *
   *  `unsupported-not-implemented` (introduced FE-11, brief §26): a page
   *  whose previous Google-Studio-generated content was fully fabricated
   *  (no corresponding backend entity/route exists at all — re-verified
   *  against the real route table, not inferred from the page's name) and
   *  was replaced with a truthful "not implemented" state rather than
   *  migrated to real data, because there is no real capability to migrate
   *  it onto. Distinct from `google-studio-not-final` (still-fake content
   *  that *could* eventually become real) — this bucket has no real
   *  backend counterpart to build toward. Currently `orchestrator`,
   *  `validator`, `rbac`, `api-testing`, `mobile-testing`, `custom`, and
   *  `feature-map` — see each page's own module doc comment for the
   *  specific backend gap. */
  designStatus:
    | 'product-owner-approved'
    | 'google-studio-not-final'
    | 'new-follows-approved-language'
    | 'unsupported-not-implemented';
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
    // FE-11: the 'PLAN' badge was removed — it described the previous
    // fabricated orchestration-planning capability, which no longer exists
    // on this page (see AIQAOrchestratorPage.tsx's own doc comment).
    Component: AIQAOrchestratorPage,
    designStatus: 'unsupported-not-implemented',
    protectedRoute: true,
  },
  {
    id: 'validator',
    path: '/validator',
    label: 'Test Case Validator',
    icon: CheckCheck,
    // FE-11: the 'GATE' badge was removed — see AIQAOrchestratorPage's
    // note above; same reasoning, TestCaseValidatorPage.tsx.
    Component: TestCaseValidatorPage,
    designStatus: 'unsupported-not-implemented',
    protectedRoute: true,
  },
  {
    id: 'execution',
    path: '/execution',
    label: 'Test Execution Hub',
    icon: PlayCircle,
    badge: 'LIVE',
    badgeType: 'live',
    Component: ExecutionHubPage,
    // FE-8: the previous `TestExecutionPage` (Google-Studio-generated, 100%
    // AIQAContext fake data — fabricated logs, fabricated evidence files,
    // fabricated stack traces) was replaced in place by a real
    // implementation (`ExecutionHubPage.tsx`) that follows Dashboard/Test
    // Generator's visual language — same third bucket as `audit`/`evidence`.
    designStatus: 'new-follows-approved-language',
    protectedRoute: true,
  },
  {
    id: 'rbac',
    path: '/rbac',
    label: 'RBAC Testing',
    icon: ShieldCheck,
    Component: RBACTestingPage,
    designStatus: 'unsupported-not-implemented',
    protectedRoute: true,
  },
  {
    id: 'api-testing',
    path: '/api-testing',
    label: 'API Testing',
    icon: Network,
    Component: APITestingPage,
    designStatus: 'unsupported-not-implemented',
    protectedRoute: true,
  },
  {
    id: 'mobile-testing',
    path: '/mobile-testing',
    label: 'Mobile Testing',
    icon: Smartphone,
    Component: MobileTestingPage,
    designStatus: 'unsupported-not-implemented',
    protectedRoute: true,
  },
  {
    id: 'custom',
    path: '/custom-testing',
    label: 'Custom Testing',
    icon: Sliders,
    Component: CustomTestingPage,
    designStatus: 'unsupported-not-implemented',
    protectedRoute: true,
  },
  {
    id: 'feature-map',
    path: '/feature-map',
    label: 'Feature Map / Flow',
    icon: Layers,
    Component: FeatureMapFlowPage,
    designStatus: 'unsupported-not-implemented',
    protectedRoute: true,
  },
  {
    id: 'reports',
    path: '/reports',
    label: 'Reports',
    icon: FileText,
    Component: ReportsPage,
    // FE-11: rebuilt onto the real Findings/FailureCluster backend — same
    // "new, follows the approved visual language" bucket as Audit/Evidence/
    // Execution Hub (see this file's designStatus doc comment above).
    designStatus: 'new-follows-approved-language',
    protectedRoute: true,
  },
  {
    id: 'audit',
    path: '/audit',
    label: 'Audit Trail',
    icon: ScrollText,
    Component: AuditPage,
    designStatus: 'new-follows-approved-language',
    protectedRoute: true,
  },
  {
    id: 'evidence',
    path: '/evidence',
    label: 'Evidence Viewer',
    icon: FileSearch,
    Component: EvidencePage,
    designStatus: 'new-follows-approved-language',
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
