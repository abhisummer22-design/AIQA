/**
 * RBAC testing by measurement.
 *
 * ## Why there is no matrix on this screen
 *
 * The obvious design is a Role-Permission Matrix: a grid of routes against
 * roles, filled in with what each role may do. It is what a QA person asks for,
 * and this page used to be asked for it. It is also the single easiest thing in
 * this codebase to get wrong, and it has been got wrong three separate ways:
 *
 *  - FE-11 removed a hardcoded matrix of invented roles (`BillingManager`, ...) and
 *    an invented "Evaluate Authorization Matrix" button, because it was fabricated.
 *  - A hand-written replacement had 18 rows whose methods disagreed with the
 *    router -- `/executions` registers POST, the table claimed GET.
 *  - Two independent source parsers disagreed with each other about how many routes
 *    had no role gate at all: 66 versus 8. One of them reported `ai-usage` as
 *    admin-only, which is false -- it answers 200 to a viewer.
 *
 * So nothing here declares a permission. Each cell is filled by sending a real
 * request to the server with a real key of that role and showing what came back
 * (`POST /api/v1/rbac/probe`). A grid filled in this way is a record of what was
 * observed, on this deployment, at this moment -- which is the only kind of matrix
 * a QA report can actually stand behind.
 *
 * Two consequences are visible in the UI and worth understanding before reading
 * any result:
 *
 *  1. **A cell is blank until you probe it.** An unprobed row is not "allowed".
 *     It says "not probed", because nobody has asked yet.
 *  2. **"Allowed" does not mean "worked."** Authorization runs inside the handler,
 *     so a request can be allowed and then refused with 400 or 404. Those cells say
 *     so explicitly rather than reporting a green tick, because "the gate let you in
 *     and the input was wrong" and "you may do this" are different claims.
 */

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { aiqaApi, ApiError } from '../../services/aiqaApi';
import { renderResourceErrorState } from '../state/StateViews';
import type {
  RbacGate,
  RbacProbeResult,
  RbacProbeTarget,
  RbacProbeTargetsResponse,
  Role,
} from '../../types/contract';

/* ------------------------------------------------------------------ styles */

/** One pill per outcome. The colours follow this app's existing dark palette
 *  (`RBACTestingPage`'s role pills) rather than inventing a second one: rose for
 *  refused-by-role, amber for refused-by-scope, emerald for genuinely done, sky for
 *  "the gate let you in and something else refused", slate for a measurement that
 *  proves nothing. */
const GATE_TONE: Record<RbacGate, string> = {
  role_refused: 'text-rose-300 bg-rose-950/70 border border-rose-800',
  scope_refused: 'text-amber-300 bg-amber-950/70 border border-amber-800',
  refused_unknown: 'text-yellow-200 bg-yellow-950/70 border border-yellow-800',
  allowed_succeeded: 'text-emerald-300 bg-emerald-950/70 border border-emerald-800',
  allowed_then_refused: 'text-sky-300 bg-sky-950/70 border border-sky-800',
  verb_not_served: 'text-slate-400 bg-slate-800/60 border border-slate-700',
  key_rejected: 'text-slate-400 bg-slate-800/60 border border-slate-700',
  probe_failed: 'text-slate-400 bg-slate-800/60 border border-slate-700',
};

const TH = 'px-2 py-1.5 text-left text-[10px] font-mono uppercase tracking-wider text-content-muted';
const TD = 'px-2 py-1.5 align-top text-[11px] text-slate-300 border-t border-slate-800';

const GateCell: React.FC<{ r: RbacProbeResult | undefined }> = ({ r }) => {
  if (!r) {
    return (
      <td className={TD}>
        <span className="text-slate-500 italic">not probed</span>
      </td>
    );
  }
  return (
    <td className={TD}>
      <span
        className={`inline-block rounded px-1.5 py-0.5 font-mono text-[10px] font-semibold ${GATE_TONE[r.gate]}`}
      >
        {r.status}
      </span>
      <div className="mt-0.5 text-[10px] leading-snug text-content-muted">{r.gate_label}</div>
    </td>
  );
};

/* ------------------------------------------------------------------ helpers */

/** Key per role. Held in memory only -- never persisted, because an API key in
 *  localStorage is an API key in a browser's disk cache. */
type Keys = Partial<Record<Role, string>>;

function parseKeys(raw: string): Keys {
  const out: Keys = {};
  for (const line of raw.split(/\r?\n/)) {
    const at = line.indexOf('=');
    if (at <= 0) continue;
    const role = line.slice(0, at).trim().toLowerCase();
    const key = line.slice(at + 1).trim();
    if ((role === 'viewer' || role === 'operator' || role === 'admin') && key) {
      out[role] = key;
    }
  }
  return out;
}

const KEY_PLACEHOLDER =
  'Paste one key per line, in the form role=value:\n' +
  'viewer=aiqa_...\noperator=aiqa_...\nadmin=aiqa_...\n\n' +
  'Mint them from the Key inventory below. A role you have no key for simply\n' +
  'leaves that column unprobed -- nothing is inferred for a role you cannot test.';

/* -------------------------------------------------------------------- page */

export const RbacProbeMatrix: React.FC = () => {
  const [targets, setTargets] = useState<RbacProbeTarget[] | null>(null);
  const [roles, setRoles] = useState<Role[]>([]);
  const [loadError, setLoadError] = useState<ApiError | null>(null);
  const [note, setNote] = useState('');

  const [keysRaw, setKeysRaw] = useState('');
  const [projectId, setProjectId] = useState('');
  const [results, setResults] = useState<Record<string, RbacProbeResult>>({});
  const [running, setRunning] = useState<string | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const [filter, setFilter] = useState('');
  /**
   * Off by default, and deliberately not remembered: probing a target that creates
   * a record is a thing you do on purpose, once, having read what it will create.
   *
   * This flag exists because the sweep got it wrong first time. The original claim
   * was that every mutating target addresses a non-existent id, so a probe that got
   * past the gate could not touch anything real. That was true of every route with
   * an id in its path and false of `POST /admin/api-keys` and
   * `POST /api/v1/projects`, which have none -- so running "probe every route" with
   * an admin key minted four API keys and created four projects. The backend now
   * refuses those two unless a request opts in; this is the opt-in.
   */
  const [allowCreation, setAllowCreation] = useState(false);

  const keys = useMemo(() => parseKeys(keysRaw), [keysRaw]);
  const rows = useMemo(
    () => Object.entries(results) as Array<[string, RbacProbeResult]>,
    [results],
  );

  const load = useCallback((signal?: AbortSignal) => {
    setLoadError(null);
    aiqaApi.rbac
      .probeTargets(signal)
      .then((r: RbacProbeTargetsResponse) => {
        // Ignore a response whose request was already superseded. StrictMode mounts,
        // unmounts and mounts again, so the first request is aborted and its
        // rejection can land after the second has already succeeded.
        if (signal?.aborted) return;
        setTargets(r.targets);
        setRoles(r.roles);
        setNote(r.note);
      })
      .catch((e: ApiError) => {
        // An abort is this component tidying up, not a failure to report. Without
        // this the panel reads "Couldn't load this data -- signal is aborted without
        // reason" over a grid that loaded perfectly well. There is no `ApiErrorKind`
        // for a cancellation, so the signal itself is the only honest test.
        if (signal?.aborted) return;
        setLoadError(e);
      });
  }, []);

  useEffect(() => {
    const ac = new AbortController();
    load(ac.signal);
    return () => ac.abort();
  }, [load]);

  const probe = useCallback(
    async (target: RbacProbeTarget, role: Role, optInToCreation: boolean) => {
      const key = keys[role];
      if (!key) return;
      setRunning(`${target.id}:${role}`);
      setRunError(null);
      try {
        const r = await aiqaApi.rbac.probe({
          target_id: target.id,
          api_key: key,
          ...(projectId.trim() ? { project_id: projectId.trim() } : {}),
          ...(optInToCreation ? { allow_creation: true } : {}),
        });
        setResults((prev) => ({ ...prev, [`${target.id}:${role}`]: r }));
      } catch (e) {
        const err = e as ApiError;
        setRunError(err?.message ?? String(e));
      } finally {
        setRunning(null);
      }
    },
    [keys, projectId],
  );

  const probeRow = useCallback(
    async (target: RbacProbeTarget, optInToCreation: boolean) => {
      // Sequentially, not in parallel: a probe is a real request at a real handler,
      // and a row that fires three at once against `/crawl` would be testing
      // concurrency rather than permissions.
      for (const role of roles) {
        if (keys[role]) await probe(target, role, optInToCreation);
      }
    },
    [probe, keys, roles],
  );

  const probeAll = useCallback(async () => {
    if (!targets) return;
    // Never opts in: a sweep must not mint keys or create projects as a side effect.
    // Rows that create are refused by the backend and their cells stay empty, which
    // is the honest outcome -- a refusal is a result, and "not run" is not one.
    for (const t of targets) await probeRow(t, false);
  }, [targets, probeRow]);

  if (loadError) {
    return (
      <>
        {renderResourceErrorState(loadError, () => load(), {
          forbiddenTitle: 'Not permitted to probe roles',
          forbiddenDescription:
            "Probing needs a key of each role you want to test, so it is Operator and above. The targets could not be listed at this role, which means there is nothing to test yet.",
        })}
      </>
    );
  }
  if (!targets) {
    return (
      <p className="py-2 font-mono text-xs text-slate-400">
        Loading the routes worth probing…
      </p>
    );
  }

  const filtered = targets.filter((t) => {
    if (!filter.trim()) return true;
    const f = filter.toLowerCase();
    return (
      t.path.toLowerCase().includes(f) ||
      t.capability.toLowerCase().includes(f) ||
      t.category.toLowerCase().includes(f)
    );
  });

  const grouped: Record<string, RbacProbeTarget[]> = {};
  for (const t of filtered) {
    (grouped[t.category] ??= []).push(t);
  }

  const anyKey = Object.keys(keys).length > 0;
  const probed = rows.length;

  return (
    <div className="flex flex-col gap-3">
      <p className="text-[11px] leading-relaxed text-slate-400">{note}</p>

      <fieldset className="rounded-md border border-slate-800 bg-[#020617] p-3">
        <legend className="px-1 text-[10px] font-mono uppercase tracking-wider text-content-muted">
          Keys to probe with
        </legend>
        <p className="mb-2 text-[11px] leading-relaxed text-slate-400">
          Held in memory for this page only, never written to storage. Probing needs a
          key for each role you want to test; a role with no key leaves its column
          unprobed rather than being assumed either way.
        </p>
        <textarea
          aria-label="API keys to probe with, one per line as role=value"
          value={keysRaw}
          onChange={(e) => setKeysRaw(e.target.value)}
          placeholder={KEY_PLACEHOLDER}
          rows={5}
          spellCheck={false}
          className="w-full rounded border border-slate-800 bg-slate-900/60 p-2 font-mono text-[11px] text-slate-200 placeholder:text-slate-600"
        />
        <div className="mt-2 flex flex-wrap items-center gap-3">
          <label className="text-[11px] text-slate-400">
            Project id
            <input
              aria-label="Project id to substitute into the route"
              value={projectId}
              onChange={(e) => setProjectId(e.target.value)}
              placeholder="uuid (optional)"
              className="ml-2 w-80 rounded border border-slate-800 bg-slate-900/60 px-2 py-1 font-mono text-[11px] text-slate-200 placeholder:text-slate-600"
            />
          </label>
          <button
            type="button"
            onClick={probeAll}
            disabled={!anyKey || running !== null}
            className="rounded-md border border-violet-600 bg-[#020617] px-3 py-1.5 font-mono text-xs font-semibold text-slate-300 transition-colors hover:border-violet-600 hover:text-violet-300 disabled:opacity-50"
          >
            {running ? 'Probing…' : `Probe every route (${probed} results so far)`}
          </button>
          <span className="font-mono text-[10px] text-content-muted">
            {anyKey
              ? `keys for: ${Object.keys(keys).sort().join(', ')}`
              : 'no keys yet -- nothing can be probed'}
          </span>
        </div>
        {runError && (
          <p role="alert" className="mt-2 text-[11px] text-rose-300">
            {runError}
          </p>
        )}
        {/*
         * Off by default and not persisted. Only the per-row button consults it; the
         * sweep above never does, so this cannot become a switch that quietly makes
         * "probe everything" create records.
         */}
        <label className="mt-2 flex items-start gap-2 text-[11px] text-slate-400">
          <input
            type="checkbox"
            checked={allowCreation}
            onChange={(e) => setAllowCreation(e.target.checked)}
            className="mt-0.5"
          />
          <span>
            Allow a single-row probe to create a real record
            <span className="block text-[10px] text-slate-500">
              Applies only to <code className="font-mono">POST /admin/api-keys</code> and{' '}
              <code className="font-mono">POST /api/v1/projects</code>, which have no id in
              their path for a probe to substitute. A successful probe will mint a real API
              key or create a real project. The sweep above ignores this.
            </span>
          </span>
        </label>
      </fieldset>

      <input
        aria-label="Filter routes"
        placeholder="Filter by path, capability or category"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        className="rounded border border-slate-800 bg-[#020617] px-2 py-1.5 text-[11px] text-slate-200 placeholder:text-slate-500"
      />

      {Object.entries(grouped).map(([category, list]) => (
        <div key={category}>
          <h3 className="mb-1 font-mono text-[10px] font-semibold uppercase tracking-wider text-content-muted">
            {category}
          </h3>
          <table className="w-full border-collapse">
            {/*
             * `scope` is not decoration here: the grid is read one role at a time, so
             * a bare "403" means nothing without knowing it is the viewer column.
             */}
            <caption className="caption-top py-1 text-left text-[10px] text-slate-500">
              Measured access for {category}. Cells stay blank until probed.
            </caption>
            <thead>
              <tr>
                <th scope="col" className={TH}>Route</th>
                <th scope="col" className={TH}>What it does</th>
                {roles.map((r) => (
                  <th key={r} scope="col" className={TH}>
                    {r}
                  </th>
                ))}
                <th scope="col" className={TH}>
                  <span className="sr-only">Probe this route</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {list.map((t) => (
                <tr key={t.id}>
                  <td className={TD}>
                    <code className="font-mono text-[11px] text-violet-300">
                      {t.method} {t.path}
                    </code>
                    {t.mutating && (
                      <div className="mt-0.5 text-[10px] leading-snug text-amber-300/90">
                        {t.creates
                          ? 'creates a real record if it gets past the gate -- needs the box above'
                          : 'can change something -- every id is a non-existent uuid, so a probe that gets through still touches nothing real'}
                      </div>
                    )}
                  </td>
                  <td className={TD}>{t.capability}</td>
                  {roles.map((role) => (
                    <GateCell key={role} r={results[`${t.id}:${role}`]} />
                  ))}
                  <td className={TD}>
                    <button
                      type="button"
                      onClick={() => probeRow(t)}
                      disabled={!anyKey || running !== null}
                      title={
                        t.mutating
                          ? t.creates
                            ? 'This probe mints a real key or creates a real project'
                            : 'Sends real requests; every id is a non-existent uuid'
                          : undefined
                      }
                      className="rounded border border-slate-700 px-2 py-0.5 font-mono text-[10px] text-slate-300 transition-colors hover:border-violet-600 hover:text-violet-300 disabled:opacity-50"
                    >
                      {running?.startsWith(`${t.id}:`) ? '…' : 'Probe'}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ))}

      {probed > 0 && (
        <details className="rounded border border-slate-800 bg-[#020617] p-2">
          <summary className="cursor-pointer font-mono text-[11px] text-slate-300">
            What the server actually said ({probed} probes)
          </summary>
          <ul className="mt-2 space-y-1.5 text-[10px] text-slate-400">
            {rows.map(([k, r]) => (
              <li key={k}>
                <code className="font-mono text-violet-300">
                  {r.method} {r.path}
                </code>{' '}
                → <strong className="font-mono text-slate-200">{r.status}</strong>{' '}
                {r.gate_label}
                {r.message && (
                  <div className="font-mono text-slate-500 break-all">
                    {r.message.slice(0, 200)}
                  </div>
                )}
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
};