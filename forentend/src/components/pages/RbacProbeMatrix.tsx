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
 *  - FE-11 removed a hardcoded matrix of invented roles (`BillingManager`, ...)
 *    and an invented "Evaluate Authorization Matrix" button, because it was
 *    fabricated.
 *  - A hand-written replacement had 18 rows whose methods disagreed with the
 *    router -- `/executions` registers POST, the table claimed GET.
 *  - Two independent source parsers disagreed with each other about how many
 *    routes had no role gate at all: 66 versus 8. One of them reported
 *    `ai-usage` as admin-only, which is false -- it answers 200 to a viewer.
 *
 * So nothing here declares a permission. Each cell is filled by sending a real
 * request to the server with a real key of that role and showing what came back
 * (`POST /api/v1/rbac/probe`). A grid that has been filled in this way is a
 * record of what was observed, on this deployment, at this moment -- which is
 * the only kind of matrix a QA report can actually stand behind.
 *
 * Two consequences are visible in the UI and worth understanding before reading
 * any result:
 *
 *  1. **A cell is blank until you probe it.** An unprobed row is not "allowed".
 *     Blank means nobody has asked.
 *  2. **"Allowed" does not mean "worked."** Authorization runs inside the
 *     handler, so a request can be allowed and then refused with 400 or 404.
 *     Those cells say so explicitly (`allowed_then_refused`) rather than
 *     reporting a green tick, because "the gate let you in and the input was
 *     wrong" and "you may do this" are different claims.
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
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

const cell: React.CSSProperties = {
  padding: '0.4rem 0.5rem',
  borderBottom: '1px solid var(--border-subtle, #e5e7eb)',
  verticalAlign: 'top',
};

const headCell: React.CSSProperties = { ...cell, textAlign: 'left', fontWeight: 600 };

const GATE_TONE: Record<RbacGate, { bg: string; fg: string }> = {
  // Refused. The result you are looking for.
  role_refused: { bg: '#fee2e2', fg: '#991b1b' },
  scope_refused: { bg: '#ffedd5', fg: '#9a3412' },
  refused_unknown: { bg: '#fef3c7', fg: '#92400e' },
  // The gate did not object.
  allowed_succeeded: { bg: '#dcfce7', fg: '#166534' },
  allowed_then_refused: { bg: '#e0e7ff', fg: '#3730a3' },
  // The measurement itself was unsound.
  verb_not_served: { bg: '#f3f4f6', fg: '#4b5563' },
  key_rejected: { bg: '#f3f4f6', fg: '#4b5563' },
  probe_failed: { bg: '#f3f4f6', fg: '#4b5563' },
};

const GateCell: React.FC<{ r: RbacProbeResult | undefined }> = ({ r }) => {
  if (!r) {
    return (
      <td style={cell}>
        <span style={{ color: '#9ca3af', fontStyle: 'italic' }}>not probed</span>
      </td>
    );
  }
  const tone = GATE_TONE[r.gate];
  return (
    <td style={cell}>
      <span
        style={{
          display: 'inline-block',
          background: tone.bg,
          color: tone.fg,
          borderRadius: 4,
          padding: '0.1rem 0.4rem',
          fontWeight: 600,
          whiteSpace: 'nowrap',
        }}
      >
        {r.status}
      </span>
      <div style={{ fontSize: '0.75rem', color: '#4b5563', marginTop: '0.15rem' }}>
        {r.gate_label}
      </div>
    </td>
  );
}

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
  'Mint them from the Key inventory above. A key you do not have simply leaves\n' +
 'that column blank -- nothing is inferred for a role you cannot test.';

/* ------------------------------------------------------------------- page */

export const RbacProbeMatrix: React.FC = () => {
  const [targets, setTargets] = useState<RbacProbeTarget[] | null>(null);
  const [roles, setRoles] = useState<Role[]>([]);
  const [loadError, setLoadError] = useState<ApiError | null>(null);
  const [note, setNote] = useState('');

  const [keysRaw, setKeysRaw] = useState('');
  const [projectId, setProjectId] = useState('');
  const [results, setResults] = useState<Record<string, RbacProbeResult>>({});
  const rows = useMemo(
    () => Object.entries(results) as Array<[string, RbacProbeResult]>,
    [results],
  );
  const [running, setRunning] = useState<string | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const [filter, setFilter] = useState('');

  const keys = useMemo(() => parseKeys(keysRaw), [keysRaw]);
  // Used so the effect re-runs when the project changes, not just the keys.
  const keySig = useMemo(
    () => `${Object.keys(keys).sort().map((r) => `${r}:${(keys as any)[r]?.length ?? 0}`).join(',')}|${projectId}`,
    [keys, projectId],
  );

  const load = useCallback((signal?: AbortSignal) => {
    setLoadError(null);
    aiqaApi.rbac
      .probeTargets(signal)
      .then((r: RbacProbeTargetsResponse) => {
        setTargets(r.targets);
        setRoles(r.roles);
        setNote(r.note);
      })
      .catch((e: ApiError) => setLoadError(e));
  }, []);

  useEffect(() => {
    const ac = new AbortController();
    load(ac.signal);
    return () => ac.abort();
  }, [load]);

  const probe = useCallback(
    async (target: RbacProbeTarget, role: Role) => {
      const key = keys[role];
      if (!key) return;
      setRunning(`${target.id}:${role}`);
      setRunError(null);
      try {
        const r = await aiqaApi.rbac.probe({
          target_id: target.id,
          api_key: key,
          ...(projectId.trim() ? { project_id: projectId.trim() } : {}),
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
    async (target: RbacProbeTarget) => {
      for (const role of roles) {
        if (keys[role]) await probe(target, role);
      }
    },
    [probe, keys, roles],
  );

  const probeAll = useCallback(async () => {
    if (!targets) return;
    for (const t of targets) await probeRow(t);
  }, [targets, probeRow]);

  if (loadError) {
    return (
      <>
        {renderResourceErrorState(loadError, () => load(), {
          forbiddenTitle: 'Not permitted to probe roles',
          forbiddenDescription:
            'Probing needs a key of each role you want to test, so it is Operator and ' +
            'above. The targets could not be listed at this role, which means there is ' +
            'nothing to test yet.',
        })}
      </>
    );
  }
  if (!targets) return <p style={{ color: '#6b7280' }}>Loading the routes worth probing…</p>;

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
  const probed = Object.keys(results).length;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
      <p style={{ margin: 0, color: '#374151' }}>{note}</p>

      <fieldset style={{ border: '1px solid #d1d5db', borderRadius: 6, padding: '0.75rem' }}>
        <legend style={{ fontWeight: 600, padding: '0 0.35rem' }}>
          Keys to probe with
        </legend>
        <p style={{ margin: '0 0 0.5rem', fontSize: '0.8rem', color: '#6b7280' }}>
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
          style={{ width: '100%', fontFamily: 'monospace', fontSize: '0.8rem' }}
        />
        <div style={{ display: 'flex', gap: '0.75rem', alignItems: 'center', marginTop: '0.5rem', flexWrap: 'wrap' }}>
          <label style={{ fontSize: '0.85rem' }}>
            Project id{' '}
            <input
              aria-label="Project id to substitute into the route"
              value={projectId}
              onChange={(e) => setProjectId(e.target.value)}
              placeholder="uuid (optional)"
              style={{ marginLeft: '0.35rem', width: '22rem' }}
            />
          </label>
          <button
            type="button"
            onClick={probeAll}
            disabled={!anyKey || running !== null}
            style={{ padding: '0.3rem 0.7rem' }}
          >
            {running ? 'Probing…' : `Probe every route (${probed} results so far)`}
          </button>
          <span style={{ fontSize: '0.8rem', color: '#6b7280' }}>
            {anyKey
              ? `keys for: ${Object.keys(keys).join(', ')}`
              : 'no keys yet -- nothing can be probed'}
          </span>
        </div>
        {runError && (
          <p role="alert" style={{ color: '#991b1b', fontSize: '0.85rem' }}>
            {runError}
          </p>
        )}
      </fieldset>

      <input
        aria-label="Filter routes"
        placeholder="Filter by path, capability or category"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        style={{ padding: '0.35rem 0.5rem', width: '100%' }}
      />

      {Object.entries(grouped).map(([category, rows]) => (
        <div key={category}>
          <h3 style={{ margin: '0 0 0.35rem', fontSize: '0.95rem' }}>{category}</h3>
          <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.85rem' }}>
            {/*
             * `scope` is what makes each cell's header announced by a screen
             * reader, and here it is not decoration: this grid is read one role at
             * a time, so "403" on its own means nothing without knowing it is the
             * viewer column.
             */}
            <caption style={{ captionSide: 'top', textAlign: 'left', fontSize: '0.78rem', color: '#6b7280' }}>
              Measured access for {category}. Cells are blank until probed.
            </caption>
            <thead>
              <tr>
                <th scope="col" style={headCell}>Route</th>
                <th scope="col" style={headCell}>What it does</th>
                {roles.map((r) => (
                  <th key={r} scope="col" style={headCell}>
                    {r}
                  </th>
                ))}
                <th scope="col" style={headCell}>
                  <span className="sr-only">Probe this route</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((t) => (
                <tr key={t.id}>
                  <td style={cell}>
                    <code style={{ fontSize: '0.8rem' }}>
                      {t.method} {t.path}
                    </code>
                    {t.mutating && (
                      <div style={{ fontSize: '0.72rem', color: '#9a3412', marginTop: '0.15rem' }}>
                        can change something -- every id is a non-existent uuid, so a
                        probe that gets through still touches nothing real
                      </div>
                    )}
                  </td>
                  <td style={cell}>{t.capability}</td>
                  {roles.map((role) => (
                    <GateCell key={role} r={results[`${t.id}:${role}`]} />
                  ))}
                  <td style={cell}>
                    <button
                      type="button"
                      onClick={() => probeRow(t)}
                      disabled={!anyKey || running !== null}
                      title={t.mutating ? 'Sends real requests; ids are non-existent' : undefined}
                      style={{ fontSize: '0.78rem', padding: '0.15rem 0.45rem' }}
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
        <details>
          <summary style={{ cursor: 'pointer', fontSize: '0.85rem' }}>
            What the server actually said ({probed} probes)
          </summary>
          <ul style={{ fontSize: '0.78rem', color: '#374151', marginTop: '0.5rem' }}>
            {rows.map(([k, r]) => (
              <li key={k} style={{ marginBottom: '0.3rem' }}>
                <code>
                  {r.method} {r.path}
                </code>{' '}
                → <strong>{r.status}</strong> {r.gate_label}
                {r.message && (
                  <div style={{ color: '#6b7280', fontFamily: 'monospace' }}>
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