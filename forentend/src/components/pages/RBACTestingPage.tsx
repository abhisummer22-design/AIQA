/**
 * RBAC Testing — the real authorization surface that actually ships.
 *
 * History: this page originally rendered a fully fabricated
 * role×permission×action matrix (hardcoded `BillingManager`-style roles, an
 * invented "Evaluate Authorization Matrix" button, made-up allow/deny
 * cells). FE-11 replaced that with a truthful "deferred" state, correctly,
 * because no role-permission-matrix entity or evaluation endpoint existed.
 *
 * That deferral is still true of the *scenario-matrix tool* (ACC-011) — and
 * this page still says so. What it no longer does is leave the page empty,
 * because the enforcement itself is real, shipped, and now observable:
 *
 *   - **Your identity** — `GET /api/v1/auth/whoami`: the real role and
 *     project scope of the key this browser is actually using, plus the real
 *     role ordering `auth.rs::Role::at_least` enforces. The hierarchy comes
 *     from the backend rather than being restated here, so this page cannot
 *     drift from the code that does the enforcing.
 *   - **Key inventory** — `GET /admin/api-keys`: every real key with its
 *     real role, real project scope, and real revocation state. Admin-only;
 *     a non-admin caller gets a real 403, which is shown honestly instead of
 *     the control silently disappearing.
 *   - **Key management** (Phase 5 gap-closing) — real create/revoke,
 *     wired to `api.rs::create_api_key`/`revoke_api_key_handler`. Both
 *     endpoints already existed, Admin-gated and tested, since before this
 *     page did — `main.rs`'s own router comment named the missing UI
 *     explicitly ("FE-2's scoped admin UI is not built; removing these
 *     would leave no way to create or revoke a key"). This is what lets a
 *     team of real people get their own distinctly-named, individually-
 *     revocable identity — and therefore genuine per-person attribution in
 *     the audit log below — without OAuth/SSO or a new users table.
 *   - **Live enforcement** — every real 401/403 this backend produces now
 *     emits on the orchestrator log stream (`auth` source, added in
 *     `auth::require_auth`) and is written to the audit log as `authz.deny`.
 *     Before that, a 403 left no trace anywhere, so there was genuinely
 *     nothing real to show; now a denial appears here as it happens.
 *
 * What this page deliberately does NOT do, because the backend cannot: it
 * does not simulate or impersonate another role. There is no "evaluate this
 * role against this action" endpoint, and faking one client-side would be
 * exactly the fabrication this page was cleaned up to remove. The real way
 * to exercise a boundary is to use a key that genuinely has that role — the
 * resulting real allow/deny then shows up in the live feed below.
 */

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  AlertTriangle,
  Check,
  Copy,
  FileClock,
  Info,
  KeyRound,
  Loader2,
  Plus,
  RefreshCw,
  ShieldCheck,
  Trash2,
  UserCheck,
  X,
} from 'lucide-react';
import { SectionHeader } from '../common/SectionHeader';
import { OrchestratorLogsPanel } from '../dashboard/OrchestratorLogsPanel';
import { aiqaApi, ApiError } from '../../services/aiqaApi';
import { useAuth } from '../../context/AuthContext';
import { useProject } from '../../context/ProjectContext';
import { renderAuthState, renderResourceErrorState } from '../state/StateViews';
import type {
  ApiKeyListRow,
  AuditRow,
  CreateApiKeyResponse,
  Role,
  WhoAmIResponse,
} from '../../types/contract';

const roleStyles: Record<string, string> = {
  admin: 'text-rose-300 bg-rose-950/70 border-rose-800',
  operator: 'text-amber-300 bg-amber-950/70 border-amber-800',
  viewer: 'text-slate-300 bg-slate-800/70 border-slate-700',
};

const Pill: React.FC<{ label: string; className?: string }> = ({ label, className }) => (
  <span
    className={`inline-flex items-center px-1.5 py-0.5 rounded border text-[9px] font-mono font-bold uppercase tracking-wide shrink-0 ${
      className ?? 'text-slate-400 bg-slate-800/60 border-slate-700'
    }`}
  >
    {label}
  </span>
);

const PanelCard: React.FC<{
  icon: React.ComponentType<{ className?: string }>;
  iconClassName: string;
  title: string;
  subtitle: string;
  headerRight?: React.ReactNode;
  children: React.ReactNode;
}> = ({ icon: Icon, iconClassName, title, subtitle, headerRight, children }) => (
  <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-3">
    <div className="flex items-center justify-between gap-2 pb-2 border-b border-slate-800/80">
      <div className="flex items-center gap-2 min-w-0">
        <Icon className={`w-4 h-4 shrink-0 ${iconClassName}`} />
        <div className="min-w-0">
          <h2 className="text-sm font-bold text-slate-100 tracking-tight">{title}</h2>
          <p className="text-[11px] text-slate-500">{subtitle}</p>
        </div>
      </div>
      {headerRight}
    </div>
    {children}
  </div>
);

function formatTime(iso: string): string {
  try {
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? iso : d.toTimeString().slice(0, 8);
  } catch {
    return iso;
  }
}

export const RBACTestingPage: React.FC = () => {
  const navigate = useNavigate();
  const auth = useAuth();
  const { selectedProjectId, projects } = useProject();

  const [me, setMe] = useState<WhoAmIResponse | null>(null);
  const [meError, setMeError] = useState<ApiError | null>(null);

  const [keys, setKeys] = useState<ApiKeyListRow[] | null>(null);
  /** A real 403 here is expected for a non-admin caller and is shown as
   *  such — the panel is not hidden, because hiding it would misrepresent
   *  the boundary as absent rather than enforced. */
  const [keysError, setKeysError] = useState<ApiError | null>(null);

  // ------------------------------------------- Phase 5 gap-closing: key mgmt
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [createName, setCreateName] = useState('');
  const [createRole, setCreateRole] = useState<Role>('operator');
  /** Empty string = global scope (`project_id: undefined` on the real
   *  request) — matches `ApiKeyListRow.project_id === null`'s own meaning. */
  const [createProjectId, setCreateProjectId] = useState('');
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  /** The real, one-time plaintext key just minted — never refetchable once
   *  this state clears, same "shown exactly once" contract the backend
   *  itself holds (`CreateApiKeyResponse`'s own doc comment). */
  const [justCreated, setJustCreated] = useState<CreateApiKeyResponse | null>(null);
  const [copied, setCopied] = useState(false);

  /** Two-click confirm, not a modal — revoking is a soft, already
   *  Admin-gated, already-reversible-by-minting-a-new-key action, so a
   *  full dialog would be more ceremony than the real stakes call for.
   *  Holds the id currently awaiting a second click. */
  const [confirmRevokeId, setConfirmRevokeId] = useState<string | null>(null);
  const [revokingId, setRevokingId] = useState<string | null>(null);
  const [revokeError, setRevokeError] = useState<string | null>(null);

  const [denials, setDenials] = useState<AuditRow[] | null>(null);
  const [denialsError, setDenialsError] = useState<ApiError | null>(null);

  const [loading, setLoading] = useState(true);
  const isUnmounted = useRef(false);

  const load = useCallback(
    async (signal?: AbortSignal) => {
      setLoading(true);
      // Each lane is loaded independently: a real 403 on the admin-only key
      // inventory must not blank out the identity panel a viewer can
      // legitimately see.
      const whoami = aiqaApi.rbac
        .whoami(signal)
        .then((r) => {
          if (!isUnmounted.current) {
            setMe(r);
            setMeError(null);
          }
        })
        .catch((e) => {
          if (!isUnmounted.current) {
            setMe(null);
            if (e instanceof ApiError) setMeError(e);
          }
        });

      const inventory = aiqaApi.rbac
        .apiKeys(signal)
        .then((r) => {
          if (!isUnmounted.current) {
            setKeys(r);
            setKeysError(null);
          }
        })
        .catch((e) => {
          if (!isUnmounted.current) {
            setKeys(null);
            if (e instanceof ApiError) setKeysError(e);
          }
        });

      const audit = selectedProjectId
        ? aiqaApi.audit
            .list(selectedProjectId, { limit: 100 }, signal)
            .then((result) => {
              if (!isUnmounted.current) {
                // Only real denials — this panel is about enforcement, and
                // every row here is a real recorded refusal, never a
                // synthesized "scenario result".
                setDenials(result.rows.filter((r) => r.outcome === 'denied'));
                setDenialsError(null);
              }
            })
            .catch((e) => {
              if (!isUnmounted.current) {
                setDenials(null);
                if (e instanceof ApiError) setDenialsError(e);
              }
            })
        : Promise.resolve(setDenials(null));

      await Promise.allSettled([whoami, inventory, audit]);
      if (!isUnmounted.current) setLoading(false);
    },
    [selectedProjectId],
  );

  useEffect(() => {
    isUnmounted.current = false;
    if (auth.status !== 'authenticated') {
      setLoading(false);
      return;
    }
    const controller = new AbortController();
    void load(controller.signal);
    return () => {
      isUnmounted.current = true;
      controller.abort();
    };
  }, [auth.status, load]);

  const handleCreateKey = useCallback(async () => {
    if (!createName.trim()) {
      setCreateError('Name is required.');
      return;
    }
    setCreating(true);
    setCreateError(null);
    try {
      const result = await aiqaApi.rbac.createApiKey({
        name: createName.trim(),
        role: createRole,
        project_id: createProjectId || undefined,
      });
      setJustCreated(result);
      setCreateName('');
      setCreateRole('operator');
      setCreateProjectId('');
      setShowCreateForm(false);
      void load();
    } catch (e) {
      setCreateError(e instanceof ApiError ? e.message : 'Could not create the key.');
    } finally {
      setCreating(false);
    }
  }, [createName, createRole, createProjectId, load]);

  const handleRevokeKey = useCallback(
    async (id: string) => {
      setRevokingId(id);
      setRevokeError(null);
      try {
        await aiqaApi.rbac.revokeApiKey(id);
        setConfirmRevokeId(null);
        void load();
      } catch (e) {
        setRevokeError(e instanceof ApiError ? e.message : 'Could not revoke this key.');
      } finally {
        setRevokingId(null);
      }
    },
    [load],
  );

  if (auth.status !== 'authenticated') {
    return (
      <div id="rbac-testing-page" className="space-y-4 animate-in fade-in duration-150">
        <SectionHeader
          title="RBAC Security Testing"
          subtitle="Real role enforcement, key scopes, and live authorization denials"
          badge="AUTHORIZATION"
          icon={ShieldCheck}
        />
        {renderAuthState(auth)}
      </div>
    );
  }

  return (
    <div id="rbac-testing-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="RBAC Security Testing"
        subtitle="Real role enforcement, key scopes, and live authorization denials"
        badge="AUTHORIZATION"
        icon={ShieldCheck}
        actions={
          <button
            type="button"
            id="btn-rbac-refresh"
            onClick={() => void load()}
            disabled={loading}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] border border-slate-700 hover:border-violet-600 text-slate-300 hover:text-violet-300 font-mono text-xs font-semibold cursor-pointer transition-colors disabled:opacity-50"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
            Refresh
          </button>
        }
      />

      <div className="grid grid-cols-1 xl:grid-cols-[minmax(0,1fr)_400px] gap-4 items-start">
        <div className="space-y-4 min-w-0">
          {/* ------------------------------------------- your real identity */}
          <PanelCard
            icon={UserCheck}
            iconClassName="text-emerald-400"
            title="Your Effective Authorization"
            subtitle="The real role and scope of the key this browser is using"
          >
            {meError ? (
              renderResourceErrorState(meError, () => void load(), {
                forbiddenTitle: 'Not permitted to read your own identity',
                forbiddenDescription: 'The orchestrator refused this request.',
                offlineDescriptionSuffix: 'to read your identity.',
                genericTitle: "Couldn't load your identity",
              })
            ) : !me ? (
              <div className="flex items-center gap-2 text-xs font-mono text-slate-400 py-2">
                <Loader2 className="w-3.5 h-3.5 animate-spin text-violet-400" /> Loading real identity…
              </div>
            ) : (
              <div className="space-y-3">
                <div className="rounded-md bg-[#020617] border border-slate-800 p-3 space-y-2">
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className="text-xs font-mono font-bold text-slate-100">{me.name}</span>
                    <Pill label={me.role} className={roleStyles[me.role]} />
                  </div>
                  <p className="text-[11px] text-slate-400">
                    {me.project_id ? (
                      <>
                        Scoped to a single project (<span className="font-mono">{me.project_id}</span>) — this key
                        is refused on every other project by <span className="font-mono">require_project_access</span>.
                      </>
                    ) : (
                      <>
                        Global scope — this key is not restricted to one project. Project-level refusals do not
                        apply to it.
                      </>
                    )}
                  </p>
                </div>

                {/* The real ordering the backend enforces, served by the
                    backend — not a second copy of the taxonomy. */}
                <div>
                  <p className="text-[10px] font-mono uppercase tracking-wider text-slate-500 mb-1.5">
                    Real role hierarchy (least → most privileged)
                  </p>
                  <div className="flex items-center gap-1.5 flex-wrap">
                    {me.role_hierarchy.map((role: Role, i: number) => (
                      <React.Fragment key={role}>
                        {i > 0 && <span className="text-slate-600 text-[10px] font-mono">&lt;</span>}
                        <Pill
                          label={role}
                          className={`${roleStyles[role]} ${role === me.role ? 'ring-1 ring-emerald-500/70' : 'opacity-70'}`}
                        />
                      </React.Fragment>
                    ))}
                  </div>
                  <p className="text-[10px] font-mono text-slate-500 mt-1.5">
                    A route requiring a given role accepts that role and every role above it
                    (<span className="font-mono">Role::at_least</span>).
                  </p>
                </div>
              </div>
            )}
          </PanelCard>

          {/* --------------------------------------------- key inventory */}
          <PanelCard
            icon={KeyRound}
            iconClassName="text-violet-400"
            title="API Key Inventory"
            subtitle="Every real key, with its real role and project scope"
            headerRight={
              <div className="flex items-center gap-2">
                {keys ? <Pill label={`${keys.length} keys`} /> : undefined}
                {!keysError && (
                  <button
                    type="button"
                    id="btn-create-api-key-toggle"
                    onClick={() => {
                      setShowCreateForm((v) => !v);
                      setCreateError(null);
                    }}
                    className="flex items-center gap-1 px-2 py-1 rounded-md bg-violet-950/60 border border-violet-800/80 hover:border-violet-600 text-violet-300 hover:text-violet-200 font-mono text-[10px] font-semibold cursor-pointer transition-colors"
                  >
                    <Plus className="w-3 h-3" />
                    New Key
                  </button>
                )}
              </div>
            }
          >
            {justCreated && (
              <div className="rounded-md bg-emerald-950/40 border border-emerald-800/70 p-3 space-y-2">
                <div className="flex items-start gap-2">
                  <ShieldCheck className="w-3.5 h-3.5 text-emerald-400 shrink-0 mt-0.5" />
                  <p className="text-[11px] text-emerald-200">
                    Key created. This is the only time the real plaintext key is ever shown — copy it now and hand
                    it to whoever this key is for. Only its SHA-256 hash is stored from here on.
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <code className="flex-1 min-w-0 truncate px-2 py-1.5 rounded bg-[#020617] border border-slate-800 text-[11px] font-mono text-slate-100">
                    {justCreated.key}
                  </code>
                  <button
                    type="button"
                    id="btn-copy-new-api-key"
                    onClick={() => {
                      void navigator.clipboard.writeText(justCreated.key).then(() => {
                        setCopied(true);
                        setTimeout(() => setCopied(false), 2000);
                      });
                    }}
                    className="flex items-center gap-1 px-2 py-1.5 rounded-md bg-[#020617] border border-slate-700 hover:border-emerald-600 text-slate-300 hover:text-emerald-300 font-mono text-[10px] font-semibold cursor-pointer transition-colors shrink-0"
                  >
                    {copied ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
                    {copied ? 'Copied' : 'Copy'}
                  </button>
                </div>
                <button
                  type="button"
                  onClick={() => setJustCreated(null)}
                  className="text-[10px] font-mono text-slate-500 hover:text-slate-300 cursor-pointer"
                >
                  Dismiss
                </button>
              </div>
            )}

            {showCreateForm && (
              <div className="rounded-md bg-[#020617] border border-slate-800 p-3 space-y-2.5">
                <div>
                  <label className="block text-[10px] font-mono uppercase tracking-wider text-slate-500 mb-1">
                    Name (whose key this is — shown in the audit log)
                  </label>
                  <input
                    type="text"
                    value={createName}
                    onChange={(e) => setCreateName(e.target.value)}
                    placeholder="e.g. priya-operator"
                    className="w-full px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-mono text-slate-200 focus:outline-none focus:border-violet-500"
                  />
                </div>
                <div className="flex gap-2">
                  <div className="flex-1">
                    <label className="block text-[10px] font-mono uppercase tracking-wider text-slate-500 mb-1">
                      Role
                    </label>
                    <select
                      value={createRole}
                      onChange={(e) => setCreateRole(e.target.value as Role)}
                      className="w-full px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-mono text-slate-200 focus:outline-none focus:border-violet-500 cursor-pointer"
                    >
                      <option value="viewer">Viewer</option>
                      <option value="operator">Operator</option>
                      <option value="admin">Admin</option>
                    </select>
                  </div>
                  <div className="flex-1">
                    <label className="block text-[10px] font-mono uppercase tracking-wider text-slate-500 mb-1">
                      Project scope
                    </label>
                    <select
                      value={createProjectId}
                      onChange={(e) => setCreateProjectId(e.target.value)}
                      className="w-full px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-mono text-slate-200 focus:outline-none focus:border-violet-500 cursor-pointer"
                    >
                      <option value="">Global (every project)</option>
                      {projects.map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.name}
                        </option>
                      ))}
                    </select>
                  </div>
                </div>
                {createError && <p className="text-[11px] text-rose-400">{createError}</p>}
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    id="btn-submit-create-api-key"
                    onClick={() => void handleCreateKey()}
                    disabled={creating}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 disabled:bg-violet-900/60 text-white font-mono text-xs font-semibold cursor-pointer disabled:cursor-not-allowed transition-colors"
                  >
                    {creating && <Loader2 className="w-3 h-3 animate-spin" />}
                    Create Key
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setShowCreateForm(false);
                      setCreateError(null);
                    }}
                    className="px-3 py-1.5 rounded-md bg-[#0B1120] border border-slate-700 text-slate-400 hover:text-slate-200 font-mono text-xs cursor-pointer transition-colors"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            )}

            {revokeError && <p className="text-[11px] text-rose-400">{revokeError}</p>}

            {keysError ? (
              keysError.kind === 'forbidden' ? (
                // A real, expected boundary — shown as enforcement working,
                // not as a broken panel.
                <div className="rounded-md bg-[#020617] border border-amber-800/70 p-3 flex items-start gap-2">
                  <ShieldCheck className="w-3.5 h-3.5 text-amber-400 shrink-0 mt-0.5" />
                  <p className="text-[11px] text-slate-400">
                    Your key is <span className="font-mono text-amber-300">{me?.role ?? 'not admin'}</span>, so the
                    orchestrator refused this admin-only list with a real 403. That refusal is the RBAC boundary
                    working — this panel is not hidden, because hiding it would suggest the capability is absent
                    rather than denied.
                  </p>
                </div>
              ) : (
                renderResourceErrorState(keysError, () => void load(), {
                  forbiddenTitle: 'Not permitted to list API keys',
                  forbiddenDescription: 'Listing API keys requires the Admin role.',
                  offlineDescriptionSuffix: 'to load the key inventory.',
                  genericTitle: "Couldn't load API keys",
                })
              )
            ) : !keys ? (
              <div className="flex items-center gap-2 text-xs font-mono text-slate-400 py-2">
                <Loader2 className="w-3.5 h-3.5 animate-spin text-violet-400" /> Loading real keys…
              </div>
            ) : keys.length === 0 ? (
              <p className="text-[11px] text-slate-400">No API keys exist.</p>
            ) : (
              <ul className="space-y-1">
                {keys.map((k) => (
                  <li
                    key={k.id}
                    className="flex items-center justify-between gap-2 px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800"
                  >
                    <div className="min-w-0">
                      <div className="flex items-center gap-2 flex-wrap">
                        <span className="text-xs font-mono text-slate-200 truncate">{k.name}</span>
                        {k.revoked_at && <Pill label="revoked" className="text-rose-300 bg-rose-950/70 border-rose-800" />}
                      </div>
                      <div className="text-[10px] font-mono text-slate-500">
                        {k.project_id ? `scoped: ${k.project_id.slice(0, 8)}…` : 'global scope'}
                      </div>
                    </div>
                    <div className="flex items-center gap-1.5 shrink-0">
                      <Pill label={k.role} className={roleStyles[k.role]} />
                      {!k.revoked_at &&
                        (confirmRevokeId === k.id ? (
                          <div className="flex items-center gap-1">
                            <button
                              type="button"
                              onClick={() => void handleRevokeKey(k.id)}
                              disabled={revokingId === k.id}
                              title="Confirm revoke"
                              className="p-1 rounded bg-rose-950/70 border border-rose-800 text-rose-300 hover:text-rose-200 cursor-pointer disabled:opacity-50"
                            >
                              {revokingId === k.id ? (
                                <Loader2 className="w-3 h-3 animate-spin" />
                              ) : (
                                <Check className="w-3 h-3" />
                              )}
                            </button>
                            <button
                              type="button"
                              onClick={() => setConfirmRevokeId(null)}
                              title="Cancel"
                              className="p-1 rounded bg-[#0B1120] border border-slate-700 text-slate-400 hover:text-slate-200 cursor-pointer"
                            >
                              <X className="w-3 h-3" />
                            </button>
                          </div>
                        ) : (
                          <button
                            type="button"
                            onClick={() => setConfirmRevokeId(k.id)}
                            title={`Revoke ${k.name}`}
                            className="p-1 rounded bg-[#0B1120] border border-slate-700 hover:border-rose-700 text-slate-500 hover:text-rose-400 cursor-pointer transition-colors"
                          >
                            <Trash2 className="w-3 h-3" />
                          </button>
                        ))}
                    </div>
                  </li>
                ))}
              </ul>
            )}
            <p className="text-[10px] font-mono text-slate-500 flex items-start gap-1.5">
              <Info className="w-3 h-3 shrink-0 mt-0.5" />
              <span>
                Only key metadata is ever returned — the key itself is shown once at creation and only its
                SHA-256 hash is stored.
              </span>
            </p>
          </PanelCard>

          {/* ------------------------------------- recorded real denials */}
          <PanelCard
            icon={FileClock}
            iconClassName="text-amber-400"
            title="Recorded Authorization Denials"
            subtitle="Real refusals written to this project's audit log"
            headerRight={denials ? <Pill label={`${denials.length} denied`} /> : undefined}
          >
            {!selectedProjectId ? (
              <p className="text-[11px] text-slate-400">
                Select a project to load its real audit-recorded denials. Live denials still stream in the panel
                on the right.
              </p>
            ) : denialsError ? (
              renderResourceErrorState(denialsError, () => void load(), {
                forbiddenTitle: 'Not permitted to read this audit trail',
                forbiddenDescription: 'Reading the audit trail requires Operator role or above.',
                offlineDescriptionSuffix: 'to load recorded denials.',
                genericTitle: "Couldn't load the audit trail",
              })
            ) : !denials ? (
              <div className="flex items-center gap-2 text-xs font-mono text-slate-400 py-2">
                <Loader2 className="w-3.5 h-3.5 animate-spin text-violet-400" /> Loading real audit records…
              </div>
            ) : denials.length === 0 ? (
              <p className="text-[11px] text-slate-400">
                No authorization denials recorded for this project. Nothing is invented to fill this space.
              </p>
            ) : (
              <ul className="space-y-1">
                {denials.slice(0, 25).map((d, i) => (
                  <li
                    key={`${d.occurred_at}-${i}`}
                    className="px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800"
                  >
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="text-[10px] font-mono text-slate-500">{formatTime(d.occurred_at)}</span>
                      <Pill label={d.action} className="text-amber-300 bg-amber-950/60 border-amber-800" />
                      {d.role && <Pill label={d.role} className={roleStyles[d.role]} />}
                      {d.actor && <span className="text-[10px] font-mono text-slate-400">{d.actor}</span>}
                    </div>
                    {d.detail && <p className="text-[11px] text-slate-300 mt-0.5">{d.detail}</p>}
                  </li>
                ))}
              </ul>
            )}
            <button
              type="button"
              onClick={() => navigate('/audit')}
              className="text-[10px] font-mono text-violet-400 hover:text-violet-300 cursor-pointer"
            >
              Open full Audit Trail →
            </button>
          </PanelCard>

          {/* -------------------------- honest note on what is NOT here */}
          <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-3 flex items-start gap-2">
            <AlertTriangle className="w-3.5 h-3.5 text-amber-400 shrink-0 mt-0.5" />
            <p className="text-[11px] text-slate-400 leading-relaxed">
              <span className="font-semibold text-slate-300">Not implemented:</span> a systematic
              role×permission×action evaluation matrix (ACC-011). No such endpoint exists, and this page will not
              simulate one — it cannot impersonate a role you do not hold. To exercise a boundary for real, use a
              key that genuinely has that role; the resulting real allow or deny is recorded above and streams
              live on the right.
            </p>
          </div>
        </div>

        <div className="min-w-0 xl:sticky xl:top-4">
          <OrchestratorLogsPanel projectId={selectedProjectId} />
        </div>
      </div>
    </div>
  );
};
