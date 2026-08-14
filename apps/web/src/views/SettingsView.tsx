/**
 * Settings — authentication and API-key administration.
 *
 * Two distinct things live here:
 *
 *  1. **This browser's key.** The orchestrator requires
 *     `Authorization: Bearer <key>` on every route but `/health`. The key is
 *     pasted in by the operator and kept in localStorage. It is never
 *     hardcoded, never committed, never logged, and never rendered back in
 *     full — only a masked prefix is shown, so a shoulder-surfer or a
 *     screenshot cannot leak it.
 *
 *  2. **Key administration** (`/admin/api-keys`, admin role only). Creating a
 *     key returns the plaintext exactly once, at creation — the orchestrator
 *     only ever persists a SHA-256 hash, so it genuinely cannot show it
 *     again. That one-time value is displayed with an explicit warning and
 *     is not stored anywhere by the UI.
 *
 * Note on the security boundary: hiding the admin panel from a non-admin key
 * is a convenience, not a control. The orchestrator enforces `require_role`
 * server-side on every admin route, and a 403 is surfaced honestly here.
 */

import React, { useState } from "react";
import { AlertTriangle, Check, Copy, KeyRound, Plus, Trash2 } from "lucide-react";
import { useApiAction, useApiResource } from "../hooks/useApi";
import { createApiKey, listApiKeys, revokeApiKey } from "../services/orchestrator";
import { clearApiKey, getApiKey, maskApiKey, setApiKey } from "../services/apiClient";
import type { ApiKeyListRow, ApiRole } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { Timestamp } from "../components/common/Badges";

export function SettingsView() {
  return (
    <div className="max-w-4xl space-y-4">
      <header>
        <h1 className="text-lg font-bold tracking-tight text-slate-100">Settings</h1>
        <p className="mt-0.5 text-xs text-slate-500">
          Authentication against the AIQA orchestrator.
        </p>
      </header>

      <SessionKeyPanel />
      <ApiKeyAdminPanel />
    </div>
  );
}

// ------------------------------------------------------- this browser's key

function SessionKeyPanel() {
  const [draft, setDraft] = useState("");
  const [saved, setSaved] = useState(false);
  const current = getApiKey();

  const save = () => {
    if (!draft.trim()) return;
    setApiKey(draft.trim());
    setDraft("");
    setSaved(true);
    // A full reload is the simplest correct way to re-run every resource
    // hook against the new credential.
    setTimeout(() => window.location.reload(), 400);
  };

  const clear = () => {
    clearApiKey();
    window.location.reload();
  };

  return (
    <section className="rounded-xl border border-slate-800 bg-[#0a0f1a]">
      <h2 className="flex items-center gap-2 border-b border-slate-800 px-4 py-2.5 text-sm font-semibold text-slate-200">
        <KeyRound size={14} className="text-cyan-400" />
        Orchestrator API key
      </h2>

      <div className="space-y-3 p-4">
        {current ? (
          <div className="flex items-center justify-between rounded-lg border border-emerald-800/50 bg-emerald-950/20 px-3 py-2">
            <div>
              <div className="text-xs text-emerald-300">Key configured for this browser</div>
              <code className="font-mono text-[10px] text-slate-500">{maskApiKey(current)}</code>
            </div>
            <button
              onClick={clear}
              className="rounded border border-slate-700 px-2.5 py-1 text-[11px] text-slate-400 transition hover:bg-slate-800"
            >
              Sign out
            </button>
          </div>
        ) : (
          <div className="rounded-lg border border-amber-800/50 bg-amber-950/20 px-3 py-2 text-xs text-amber-300">
            No API key stored. All authenticated screens will show “Authentication required”.
          </div>
        )}

        <div>
          <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-500">
            {current ? "Replace key" : "Enter key"}
          </label>
          <div className="flex gap-2">
            <input
              type="password"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && save()}
              placeholder="aiqa_…"
              autoComplete="off"
              spellCheck={false}
              className="flex-1 rounded border border-slate-700 bg-slate-900/60 px-2.5 py-1.5 font-mono text-xs text-slate-200 outline-none focus:border-cyan-600"
            />
            <button
              onClick={save}
              disabled={!draft.trim()}
              className="rounded border border-cyan-700/60 bg-cyan-950/40 px-3 py-1.5 text-xs font-medium text-cyan-300 transition hover:bg-cyan-900/40 disabled:opacity-40"
            >
              {saved ? <Check size={13} /> : "Save"}
            </button>
          </div>
          <p className="mt-1.5 text-[10px] leading-relaxed text-slate-600">
            The orchestrator mints a bootstrap admin key on its very first start and prints it once to stderr. Look for
            “AIQA bootstrap admin API key” in the orchestrator's console output. Stored in this browser's localStorage
            only; never sent anywhere except the orchestrator.
          </p>
        </div>
      </div>
    </section>
  );
}

// -------------------------------------------------------------- key admin

function ApiKeyAdminPanel() {
  const keys = useApiResource<ApiKeyListRow[]>((signal) => listApiKeys(signal), []);
  const [showCreate, setShowCreate] = useState(false);
  const [mintedKey, setMintedKey] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const revoke = useApiAction((id: string) => revokeApiKey(id));

  const doRevoke = async (id: string) => {
    await revoke.run(id);
    keys.reload();
  };

  return (
    <section className="rounded-xl border border-slate-800 bg-[#0a0f1a]">
      <div className="flex items-center justify-between border-b border-slate-800 px-4 py-2.5">
        <h2 className="text-sm font-semibold text-slate-200">API keys (admin)</h2>
        <button
          onClick={() => setShowCreate((s) => !s)}
          className="flex items-center gap-1 rounded border border-slate-700 px-2.5 py-1 text-[11px] text-slate-300 transition hover:bg-slate-800"
        >
          <Plus size={11} /> New key
        </button>
      </div>

      <div className="space-y-3 p-4">
        {mintedKey && (
          <div className="rounded-lg border border-amber-700/60 bg-amber-950/30 p-3">
            <div className="flex items-start gap-2">
              <AlertTriangle size={14} className="mt-0.5 shrink-0 text-amber-400" />
              <div className="min-w-0 flex-1">
                <div className="text-xs font-semibold text-amber-300">Copy this key now — it is shown once</div>
                <p className="mt-0.5 text-[10px] text-amber-400/80">
                  The orchestrator stores only a SHA-256 hash and cannot display it again.
                </p>
                <div className="mt-2 flex items-center gap-2">
                  <code className="min-w-0 flex-1 break-all rounded bg-slate-950/80 px-2 py-1.5 font-mono text-[10px] text-slate-300">
                    {mintedKey}
                  </code>
                  <button
                    onClick={() => {
                      navigator.clipboard?.writeText(mintedKey);
                      setCopied(true);
                      setTimeout(() => setCopied(false), 1500);
                    }}
                    className="shrink-0 rounded border border-slate-700 p-1.5 text-slate-400 transition hover:bg-slate-800"
                  >
                    {copied ? <Check size={12} /> : <Copy size={12} />}
                  </button>
                </div>
                <button
                  onClick={() => setMintedKey(null)}
                  className="mt-2 text-[10px] text-amber-400/70 underline hover:text-amber-300"
                >
                  I've saved it — dismiss
                </button>
              </div>
            </div>
          </div>
        )}

        {showCreate && (
          <CreateKeyForm
            onCreated={(key) => {
              setMintedKey(key);
              setShowCreate(false);
              keys.reload();
            }}
            onCancel={() => setShowCreate(false)}
          />
        )}

        {revoke.error && (
          <div className="rounded border border-rose-800/60 bg-rose-950/30 px-2.5 py-1.5 text-[11px] text-rose-300">
            {revoke.error.detail || revoke.error.message}
          </div>
        )}

        <ResourceView
          state={keys}
          compact
          loadingLabel="Loading API keys…"
          emptyTitle="No API keys"
          emptyMessage="No keys are registered on this orchestrator."
        >
          {(rows) => (
            <table className="w-full text-left text-xs">
              <thead>
                <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-600">
                  <th className="pb-2 font-normal">Name</th>
                  <th className="pb-2 font-normal">Role</th>
                  <th className="pb-2 font-normal">Project</th>
                  <th className="pb-2 font-normal">Created</th>
                  <th className="pb-2 font-normal">Status</th>
                  <th className="pb-2" />
                </tr>
              </thead>
              <tbody>
                {rows.map((k) => (
                  <tr key={k.id} className="border-t border-slate-800/70">
                    <td className="py-2 pr-3 text-slate-200">{k.name}</td>
                    <td className="py-2 pr-3">
                      <span className="rounded bg-slate-800 px-1.5 py-0.5 font-mono text-[9px] uppercase tracking-wider text-slate-400">
                        {k.role}
                      </span>
                    </td>
                    <td className="py-2 pr-3 font-mono text-[10px] text-slate-500">{k.project_id ?? "global"}</td>
                    <td className="py-2 pr-3">
                      <Timestamp value={k.created_at} />
                    </td>
                    <td className="py-2 pr-3">
                      {k.revoked_at ? (
                        <span className="font-mono text-[10px] text-slate-600">revoked</span>
                      ) : (
                        <span className="font-mono text-[10px] text-emerald-400">active</span>
                      )}
                    </td>
                    <td className="py-2 text-right">
                      {!k.revoked_at && (
                        <button
                          onClick={() => doRevoke(k.id)}
                          disabled={revoke.isRunning}
                          title="Revoke key"
                          className="rounded border border-rose-800/60 p-1 text-rose-400 transition hover:bg-rose-950/40 disabled:opacity-40"
                        >
                          <Trash2 size={11} />
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </ResourceView>
      </div>
    </section>
  );
}

function CreateKeyForm({ onCreated, onCancel }: { onCreated: (key: string) => void; onCancel: () => void }) {
  const [name, setName] = useState("");
  const [role, setRole] = useState<ApiRole>("viewer");
  const [projectId, setProjectId] = useState("");

  const create = useApiAction((body: { name: string; role: ApiRole; project_id?: string }) => createApiKey(body));

  const submit = async () => {
    if (!name.trim()) return;
    const result = await create.run({
      name: name.trim(),
      role,
      project_id: projectId.trim() || undefined,
    });
    if (result) onCreated(result.key);
  };

  return (
    <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-3">
      <div className="grid grid-cols-3 gap-2">
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Key name"
          className="rounded border border-slate-700 bg-slate-900/60 px-2.5 py-1.5 text-xs text-slate-200 outline-none focus:border-cyan-600"
        />
        <select
          value={role}
          onChange={(e) => setRole(e.target.value as ApiRole)}
          className="rounded border border-slate-700 bg-slate-900/60 px-2 py-1.5 text-xs text-slate-300 outline-none focus:border-cyan-600"
        >
          <option value="viewer">viewer</option>
          <option value="operator">operator</option>
          <option value="admin">admin</option>
        </select>
        <input
          value={projectId}
          onChange={(e) => setProjectId(e.target.value)}
          placeholder="project_id (optional)"
          className="rounded border border-slate-700 bg-slate-900/60 px-2.5 py-1.5 text-xs text-slate-200 outline-none focus:border-cyan-600"
        />
      </div>

      {create.error && (
        <div className="mt-2 rounded border border-rose-800/60 bg-rose-950/30 px-2.5 py-1.5 text-[11px] text-rose-300">
          {create.error.detail || create.error.message}
        </div>
      )}

      <div className="mt-2 flex justify-end gap-2">
        <button
          onClick={onCancel}
          className="rounded border border-slate-700 px-2.5 py-1 text-[11px] text-slate-400 transition hover:bg-slate-800"
        >
          Cancel
        </button>
        <button
          onClick={submit}
          disabled={create.isRunning || !name.trim()}
          className="rounded border border-cyan-700/60 bg-cyan-950/40 px-2.5 py-1 text-[11px] font-medium text-cyan-300 transition hover:bg-cyan-900/40 disabled:opacity-40"
        >
          {create.isRunning ? "Creating…" : "Create key"}
        </button>
      </div>
    </div>
  );
}
