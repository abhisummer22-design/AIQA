/**
 * FE-3 — reusable authentication state-view layer.
 *
 * Consumes FE-2's real `AuthContext` (the one source of truth for "is this
 * browser authenticated" — see `context/AuthContext.tsx`) and renders one
 * of eight distinct, truthful UI states. Nothing here is a static mock: no
 * card renders unless the real `status` it corresponds to is the real
 * current value, and no state claims more than the backend actually said
 * (no invented role/project data, no fabricated "connected" before a
 * request has actually succeeded).
 *
 * Two things are exported for the rest of the app to use:
 *
 *   - `renderAuthState(auth, actions?)` — the *single* place that maps an
 *     `AuthStatus` to a component (this sprint's brief §3: "do not
 *     duplicate authentication-state decisions across multiple
 *     components"). Every consumer, including `StateGate` below and
 *     `SettingsPage`'s panel, goes through this one function.
 *   - `<StateGate>` — the reusable gate future FE units (Dashboard,
 *     Projects, Audit, Evidence, Execution Hub, Findings, Reports) drop
 *     around real, authenticated-only content. It renders `children` only
 *     when `status === 'authenticated'`; every other state renders the
 *     matching state card instead. Not wired into any page in this sprint
 *     (brief §5: "do not wire those future pages to fake authentication in
 *     this sprint") — it exists so the *next* unit that wires a real page
 *     doesn't have to invent this pattern or re-derive the state mapping.
 *
 * Visual language matches the existing `common/EmptyState.tsx` card
 * pattern (icon-in-box, badge, title, description, CTA row) rather than
 * introducing a new one, per this sprint's "do not create a completely new
 * design system" constraint.
 */

import React from 'react';
import {
  AlertTriangle,
  CircleHelp,
  Key,
  Loader2,
  LogOut,
  LucideIcon,
  RefreshCw,
  RotateCcw,
  ShieldAlert,
  ShieldCheck,
  WifiOff,
  XCircle,
} from 'lucide-react';
import { AuthContextType, AuthStatus } from '../../context/AuthContext';
import type { ApiError } from '../../services/aiqaApi';

// ------------------------------------------------------------- primitive

/** FE-6: exported (was module-private through FE-3/4/5) so a later unit's
 *  own resource-state model — e.g. Audit's Loading/Empty/Error/Unauthorized/
 *  Forbidden/Offline set, which is real but distinct from `AuthStatus` — can
 *  reuse the exact same visual primitive instead of duplicating this card's
 *  markup a second time (this sprint's brief: "use the existing design-
 *  system state components... do not create a completely new design
 *  language"). Genuinely generic already — nothing about its own props is
 *  auth-specific except `maskedKey`, which any non-auth caller simply omits. */
export interface StateCardProps {
  icon: LucideIcon;
  iconClassName: string;
  /** Short, uppercase, machine-scannable — matches the existing status-pill
   *  convention (`StatusBadge`, `EmptyState`'s `badge` prop) rather than a
   *  free-form heading. */
  statusLabel: string;
  statusClassName: string;
  spin?: boolean;
  title: string;
  description: string;
  /** Masked key row — never the raw key. Rendered only when a key exists. */
  maskedKey?: string | null;
  /** Extra content below the description (e.g. a request id). */
  children?: React.ReactNode;
  actions?: React.ReactNode;
}

export const StateCard: React.FC<StateCardProps> = ({
  icon: Icon,
  iconClassName,
  statusLabel,
  statusClassName,
  spin,
  title,
  description,
  maskedKey,
  children,
  actions,
}) => (
  <div
    role="status"
    aria-live="polite"
    className="rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-3"
  >
    <div className="flex items-start justify-between gap-3">
      <div className="flex items-start gap-2.5">
        <div className={`w-8 h-8 rounded-md bg-[#020617] border border-slate-700/80 flex items-center justify-center shrink-0 ${iconClassName}`}>
          <Icon className={`w-4 h-4 ${spin ? 'animate-spin' : ''}`} />
        </div>
        <div>
          <h3 className="text-sm font-semibold text-slate-100">{title}</h3>
          <p className="text-xs text-slate-400 mt-0.5 leading-relaxed max-w-md">{description}</p>
        </div>
      </div>
      <span
        className={`shrink-0 inline-flex items-center gap-1 px-2 py-1 rounded border text-[10px] font-mono font-bold uppercase tracking-wide ${statusClassName}`}
      >
        {statusLabel}
      </span>
    </div>

    {maskedKey && (
      <div className="px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 font-mono text-xs text-slate-300 inline-block">
        {maskedKey}
      </div>
    )}

    {children}

    {actions && <div className="flex flex-wrap items-center gap-2 pt-1">{actions}</div>}
  </div>
);

// ------------------------------------------------------------- CTA button

const ctaClassName =
  'inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 text-white font-semibold text-xs shadow-sm shadow-violet-950/60 cursor-pointer transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-violet-400 disabled:opacity-50 disabled:cursor-not-allowed';
const secondaryClassName =
  'inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 font-medium text-xs border border-slate-700 cursor-pointer transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-violet-400';

/** Every button rendered by this module is a real `<button type="button">`
 *  — native keyboard accessibility (Tab focus, Enter/Space activation)
 *  comes for free from the element itself; nothing here needs a manual
 *  key handler (this sprint's brief §6). Exported for the same reason as
 *  `StateCard` above (FE-6). */
export const Cta: React.FC<{ label: string; icon: LucideIcon; onClick?: () => void; primary?: boolean; disabled?: boolean }> = ({
  label,
  icon: Icon,
  onClick,
  primary,
  disabled,
}) => (
  <button type="button" onClick={onClick} disabled={disabled} className={primary ? ctaClassName : secondaryClassName}>
    <Icon className="w-3.5 h-3.5" />
    {label}
  </button>
);

// ------------------------------------------------------------- per-state

export interface AuthStateActions {
  /** Reveals/focuses key entry. Optional — a page with no local key-entry
   *  UI of its own may omit this; the CTA is simply not rendered. */
  onConfigure?: () => void;
  /** Defaults to `auth.validate` when omitted. */
  onVerify?: () => void;
  /** Defaults to `auth.signOut` when omitted. */
  onSignOut?: () => void;
}

const NoKeyState: React.FC<{ onConfigure?: () => void }> = ({ onConfigure }) => (
  <StateCard
    icon={CircleHelp}
    iconClassName="text-slate-400"
    statusLabel="NO KEY CONFIGURED"
    statusClassName="text-slate-400 bg-slate-800/60 border-slate-700"
    title="No orchestrator API key configured"
    description="Configure an AIQA Orchestrator API key to connect this frontend to the real backend."
    actions={onConfigure && <Cta label="Configure API Key" icon={Key} onClick={onConfigure} primary />}
  />
);

const KeyPresentState: React.FC<{ maskedKey: string | null; onVerify: () => void; onSignOut: () => void }> = ({
  maskedKey,
  onVerify,
  onSignOut,
}) => (
  <StateCard
    icon={AlertTriangle}
    iconClassName="text-amber-400"
    statusLabel="KEY SAVED · UNVERIFIED"
    statusClassName="text-amber-400 bg-amber-950/60 border-amber-800"
    title="Key saved, not yet verified"
    description="A key exists locally but has not been verified against the orchestrator this session. It may or may not still be valid."
    maskedKey={maskedKey}
    actions={
      <>
        <Cta label="Verify Connection" icon={RefreshCw} onClick={onVerify} primary />
        <Cta label="Sign Out" icon={LogOut} onClick={onSignOut} />
      </>
    }
  />
);

const CheckingState: React.FC<{ maskedKey: string | null }> = ({ maskedKey }) => (
  <StateCard
    icon={Loader2}
    iconClassName="text-violet-400"
    spin
    statusLabel="VERIFYING…"
    statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
    title="Verifying connection"
    description="Checking the configured API key with the AIQA Orchestrator…"
    maskedKey={maskedKey}
    // No action is rendered here at all (not even a disabled one) — this
    // is what actually prevents a duplicate validation request while one
    // is already in flight, rather than relying on a disabled-button race.
  />
);

const AuthenticatedState: React.FC<{ maskedKey: string | null; onVerify: () => void; onSignOut: () => void }> = ({
  maskedKey,
  onVerify,
  onSignOut,
}) => (
  <StateCard
    icon={ShieldCheck}
    iconClassName="text-emerald-400"
    statusLabel="CONNECTED"
    statusClassName="text-emerald-400 bg-emerald-950/60 border-emerald-800"
    title="AIQA Orchestrator authentication verified"
    // Role/project information is deliberately not shown: `AuthContext`'s
    // validation call (`GET /api/v1/projects`) does not capture or store
    // any role/project claim about the key, so there is nothing real to
    // display here — inventing one would violate this sprint's own "do
    // not invent role/project information" rule.
    description="This key was accepted by a real, protected backend request."
    maskedKey={maskedKey}
    actions={
      <>
        <Cta label="Verify Connection" icon={RefreshCw} onClick={onVerify} />
        <Cta label="Sign Out" icon={LogOut} onClick={onSignOut} />
      </>
    }
  />
);

const UnauthorizedState: React.FC<{
  maskedKey: string | null;
  onVerify: () => void;
  onSignOut: () => void;
  onConfigure?: () => void;
}> = ({ maskedKey, onVerify, onSignOut, onConfigure }) => (
  <StateCard
    icon={XCircle}
    iconClassName="text-rose-400"
    statusLabel="AUTHENTICATION REQUIRED · 401"
    statusClassName="text-rose-400 bg-rose-950/60 border-rose-800"
    title="Authentication required"
    description="The orchestrator rejected this API key. It may be invalid, revoked, or mistyped."
    maskedKey={maskedKey}
    actions={
      <>
        {onConfigure && <Cta label="Replace Key" icon={Key} onClick={onConfigure} primary />}
        <Cta label="Retry" icon={RefreshCw} onClick={onVerify} />
        <Cta label="Sign Out" icon={LogOut} onClick={onSignOut} />
      </>
    }
  />
);

const ForbiddenState: React.FC<{ maskedKey: string | null; onVerify: () => void; onSignOut: () => void }> = ({
  maskedKey,
  onVerify,
  onSignOut,
}) => (
  <StateCard
    icon={ShieldAlert}
    iconClassName="text-orange-400"
    statusLabel="ACCESS FORBIDDEN · 403"
    statusClassName="text-orange-400 bg-orange-950/60 border-orange-800"
    title="Access forbidden"
    description="This key is valid, but it is not permitted to access the requested resource. This is a permission/scope problem, not a bad credential — the key stays configured."
    maskedKey={maskedKey}
    // Deliberately no "replace key" CTA here — brief §Forbidden: "do not
    // automatically force the user to replace the key" / "do not present
    // it as authentication missing". Retry is still offered, since the
    // *same* key may simply need a different resource/role context; sign
    // out remains available like every other state where a key exists.
    actions={
      <>
        <Cta label="Retry" icon={RefreshCw} onClick={onVerify} />
        <Cta label="Sign Out" icon={LogOut} onClick={onSignOut} />
      </>
    }
  />
);

const OfflineState: React.FC<{ maskedKey: string | null; onVerify: () => void; onSignOut: () => void }> = ({
  maskedKey,
  onVerify,
  onSignOut,
}) => (
  <StateCard
    icon={WifiOff}
    iconClassName="text-slate-400"
    statusLabel="ORCHESTRATOR UNREACHABLE"
    statusClassName="text-slate-400 bg-slate-800/60 border-slate-700"
    title="Orchestrator unreachable"
    description="Unable to reach the AIQA Orchestrator. Check the network connection or backend availability."
    maskedKey={maskedKey}
    actions={
      <>
        <Cta label="Retry" icon={RefreshCw} onClick={onVerify} />
        <Cta label="Sign Out" icon={LogOut} onClick={onSignOut} />
      </>
    }
  />
);

const GenericErrorState: React.FC<{
  maskedKey: string | null;
  message: string;
  requestId?: string | null;
  onVerify: () => void;
  onSignOut: () => void;
}> = ({ maskedKey, message, requestId, onVerify, onSignOut }) => (
  <StateCard
    icon={AlertTriangle}
    iconClassName="text-rose-400"
    statusLabel="ERROR"
    statusClassName="text-rose-400 bg-rose-950/60 border-rose-800"
    title="Something went wrong"
    // `message` is `ApiError.message` — already a safe, hand-written
    // string from the backend's own sanitized error envelope (Contract
    // §2.5), never a stack trace or raw response body.
    description={message}
    maskedKey={maskedKey}
    actions={
      <>
        <Cta label="Retry" icon={RefreshCw} onClick={onVerify} />
        <Cta label="Sign Out" icon={LogOut} onClick={onSignOut} />
      </>
    }
  >
    {requestId && <p className="text-[10px] font-mono text-content-muted">request_id: {requestId}</p>}
  </StateCard>
);

// ------------------------------------------------------------- the map

/**
 * The single mapping from `AuthStatus` to a rendered state card (brief §3).
 * Every action callback defaults to the real `AuthContext` action of the
 * same purpose when not overridden, so a bare `renderAuthState(auth)` is
 * always a fully-functional, real (non-mock) render.
 */
export function renderAuthState(auth: AuthContextType, actions: AuthStateActions = {}): React.ReactNode {
  const onVerify = actions.onVerify ?? (() => void auth.validate());
  const onSignOut = actions.onSignOut ?? (() => auth.signOut());
  const status: AuthStatus = auth.status;

  switch (status) {
    case 'no_key':
      return <NoKeyState onConfigure={actions.onConfigure} />;
    case 'key_present':
      return <KeyPresentState maskedKey={auth.maskedKey} onVerify={onVerify} onSignOut={onSignOut} />;
    case 'checking':
      return <CheckingState maskedKey={auth.maskedKey} />;
    case 'authenticated':
      return <AuthenticatedState maskedKey={auth.maskedKey} onVerify={onVerify} onSignOut={onSignOut} />;
    case 'unauthenticated':
      return (
        <UnauthorizedState maskedKey={auth.maskedKey} onVerify={onVerify} onSignOut={onSignOut} onConfigure={actions.onConfigure} />
      );
    case 'forbidden':
      return <ForbiddenState maskedKey={auth.maskedKey} onVerify={onVerify} onSignOut={onSignOut} />;
    case 'offline':
      return <OfflineState maskedKey={auth.maskedKey} onVerify={onVerify} onSignOut={onSignOut} />;
    case 'error':
      return (
        <GenericErrorState
          maskedKey={auth.maskedKey}
          message={auth.lastError?.message ?? 'An unexpected error occurred.'}
          requestId={auth.lastError?.requestId}
          onVerify={onVerify}
          onSignOut={onSignOut}
        />
      );
    default: {
      // Exhaustiveness guard: if `AuthStatus` ever grows a new member,
      // this fails to compile rather than silently rendering nothing.
      const _exhaustive: never = status;
      return _exhaustive;
    }
  }
}

// ------------------------------------------------------------- StateGate

export interface StateGateProps {
  /** The real `AuthContext` value to gate on — pass `useAuth()`'s result. */
  auth: AuthContextType;
  actions?: AuthStateActions;
  children: React.ReactNode;
}

/**
 * Reusable gate for future FE units (§5 of this sprint's brief): renders
 * `children` only when `auth.status === 'authenticated'`; every other
 * state renders the matching card from `renderAuthState` instead. Not
 * wired into any real page in this sprint — provided so the next unit
 * that wires a real page (Dashboard, Projects, Audit, Evidence, Execution
 * Hub, Findings, Reports) has a ready, non-duplicated pattern:
 *
 * ```tsx
 * <StateGate auth={useAuth()}>
 *   <RealAuthenticatedContent />
 * </StateGate>
 * ```
 */
export const StateGate: React.FC<StateGateProps> = ({ auth, actions, children }) => {
  // If no key is configured in localStorage, prompt user to configure one
  if (auth.status === 'no_key') {
    return <>{renderAuthState(auth, actions)}</>;
  }
  // If orchestrator explicitly rejected the key with 401, show actionable auth error
  if (auth.status === 'unauthenticated') {
    return <>{renderAuthState(auth, actions)}</>;
  }
  // If key exists (key_present, checking, authenticated, offline, error), render
  // page content directly without blocking. Background verification runs silently.
  return <>{children}</>;
};

// ------------------------------------------------------- resource error state

/** FE-12 — the single place that maps a real, per-request `ApiError` (401/
 *  403/offline/other) to a state card, for the five real, backend-scoped
 *  resource pages (Dashboard, Audit, Evidence, Execution Hub, Reports).
 *
 *  Before this sprint, each of those five pages hand-copied an identical
 *  four-branch switch — same icons, same `statusLabel`/`statusClassName`
 *  for every branch, same `unauthorized`/`offline` title and description
 *  verbatim in all five, only the `forbidden` title/description (which
 *  correctly names the specific resource and, where relevant, the specific
 *  role requirement) and the `offline`/generic descriptions' trailing
 *  clause varying per page. That is exactly the kind of "vocabulary
 *  duplicated identically across pages" this sprint's brief asks to
 *  centralize (§10) — the four-branch *shape*, the two universally-
 *  identical branches, and the icon/color/statusLabel vocabulary for all
 *  four branches move here; only the genuinely page-specific `forbidden`
 *  copy (and the offline description's short suffix) stays a parameter,
 *  since collapsing those into one generic sentence would either lose real
 *  accuracy (e.g. Audit's real "requires Operator role or above") or force
 *  a misleadingly generic message onto a page with a different real
 *  requirement — this sprint's own rule against merging genuinely distinct
 *  meanings under one label. */
export interface ResourceErrorCopy {
  /** e.g. "Not permitted to view this audit trail". */
  forbiddenTitle: string;
  /** The full 403 description sentence(s) — kept page-specific since the
   *  real required role differs per endpoint. */
  forbiddenDescription: string;
  /** Trailing clause only, e.g. "to load real dashboard data." / "to load
   *  findings." Combined as `Unable to reach the AIQA Orchestrator ${suffix}`.
   *  Omit for the plain "Unable to reach the AIQA Orchestrator." */
  offlineDescriptionSuffix?: string;
  /** The generic (non-401/403/offline) error title — also genuinely varies
   *  per page today (`"Couldn't load the audit trail"` vs `"...the
   *  dashboard"` vs `"...findings"` vs the two pages that already used the
   *  plain generic form). Defaults to that existing generic form. */
  genericTitle?: string;
}

export function renderResourceErrorState(error: ApiError, onRetry: () => void, copy: ResourceErrorCopy): React.ReactNode {
  const requestIdNode = error.requestId ? <p className="text-[10px] font-mono text-content-muted">request_id: {error.requestId}</p> : null;

  switch (error.kind) {
    case 'unauthorized':
      return (
        <StateCard
          icon={XCircle}
          iconClassName="text-rose-400"
          statusLabel="AUTHENTICATION REQUIRED · 401"
          statusClassName="text-rose-400 bg-rose-950/60 border-rose-800"
          title="Authentication required"
          description="The orchestrator rejected this request. The API key may have been revoked since it was last verified."
          actions={<Cta label="Retry" icon={RotateCcw} onClick={onRetry} primary />}
        >
          {requestIdNode}
        </StateCard>
      );
    case 'forbidden':
      return (
        <StateCard
          icon={ShieldAlert}
          iconClassName="text-orange-400"
          statusLabel="ACCESS FORBIDDEN · 403"
          statusClassName="text-orange-400 bg-orange-950/60 border-orange-800"
          title={copy.forbiddenTitle}
          description={copy.forbiddenDescription}
          actions={<Cta label="Retry" icon={RotateCcw} onClick={onRetry} primary />}
        >
          {requestIdNode}
        </StateCard>
      );
    case 'offline':
      return (
        <StateCard
          icon={WifiOff}
          iconClassName="text-slate-400"
          statusLabel="ORCHESTRATOR UNREACHABLE"
          statusClassName="text-slate-400 bg-slate-800/60 border-slate-700"
          title="Orchestrator unreachable"
          description={copy.offlineDescriptionSuffix ? `Unable to reach the AIQA Orchestrator ${copy.offlineDescriptionSuffix}` : 'Unable to reach the AIQA Orchestrator.'}
          actions={<Cta label="Retry" icon={RotateCcw} onClick={onRetry} primary />}
        >
          {requestIdNode}
        </StateCard>
      );
    default:
      return (
        <StateCard
          icon={AlertTriangle}
          iconClassName="text-rose-400"
          statusLabel="ERROR"
          statusClassName="text-rose-400 bg-rose-950/60 border-rose-800"
          title={copy.genericTitle ?? "Couldn't load this data"}
          description={error.message}
          actions={<Cta label="Retry" icon={RotateCcw} onClick={onRetry} primary />}
        >
          {requestIdNode}
        </StateCard>
      );
  }
}
