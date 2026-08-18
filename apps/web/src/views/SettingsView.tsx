/**
 * Settings — authentication and API-key administration.
 *
 * Sources: `GET /admin/api-keys`, `POST /admin/api-keys`, `DELETE /admin/api-keys/:id`.
 */

import React, { useState } from "react";
import { AlertTriangle, Check, Copy, KeyRound, Plus, Trash2, ShieldCheck, ShieldAlert, RotateCcw } from "lucide-react";
import { useApiAction, useApiResource } from "../hooks/useApi";
import { createApiKey, listApiKeys, revokeApiKey } from "../services/orchestrator";
import { clearApiKey, getApiKey, maskApiKey, setApiKey } from "../services/apiClient";
import type { ApiKeyListRow, ApiRole } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { Timestamp } from "../components/common/Badges";
import { SectionHeader } from "../components/common/SectionHeader";

export function SettingsView() {
  return (
    <div className="max-w-4xl space-y-4">
      <SectionHeader
        title="Settings & Control Plane Security"
        subtitle="Orchestrator authentication tokens and administrative API key management."
        badge="REALTIME BACKEND"
        badgeTone="live"
      />

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
    setTimeout(() => window.location.reload(), 400);
  };

  const clear = () => {
    clearApiKey();
    window.location.reload();
  };

  return (
    <section className="rounded-xl border border-slate-800 bg-[#0a0f1a] shadow-lg">
      <h2 className="flex items-center gap-2 border-b border-slate-800 px-4 py-3 text-sm font-semibold text-slate-200">
        <KeyRound size={14} className="text-cyan-400" />
        <span>Active Browser Authentication Key</span>
      </h2>

      <div className="space-y-3.5 p-4">
        {current ? (
          <div className="flex items-center justify-between rounded-lg border border-emerald-800/60 bg-emerald-950/20 px-3.5 py-2.5">
            <div>
              <div className="text-xs font-semibold text-emerald-300">Active Key Configured</div>
              <code className="font-mono text-xs text-slate-400 mt-0.5 block">{maskApiKey(current)}</code>
            </div>
            <button
              onClick={clear}
              className="rounded border border-slate-700 bg-slate-800 px-3 py-1 text-xs text-slate-300 transition hover:bg-slate-700 cursor-pointer"
            >
              Sign Out / Clear Key
            </button>
          </div>
        ) : (
          <div className="rounded-lg border border-amber-800/60 bg-amber-950/20 px-3.5 py-2.5 text-xs text-amber-300 flex items-center gap-2">
            <AlertTriangle size={14} className="shrink-0 text-amber-400" />
            <span>No API key configured for this browser. Protected endpoints will return 401 Unauthorized.</span>
          </div>
        )}

        <div>
          <label className="mb-1.5 block font-mono text-[10px] uppercase tracking-wider text-slate-400 font-semibold">
            {current ? "Replace Configured API Key" : "Enter Orchestrator API Key"}
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
              className="flex-1 rounded-md border border-slate-700 bg-slate-900/80 px-3 py-1.5 font-mono text-xs text-slate-200 outline-none focus:border-cyan-500"
            />
            <button
              onClick={save}
              disabled={!draft.trim()}
              className="rounded-md border border-cyan-700/60 bg-cyan-950/60 px-4 py-1.5 text-xs font-semibold text-cyan-300 transition hover:bg-cyan-900/60 disabled:opacity-40 cursor-pointer flex items-center gap-1.5"
            >
              {saved ? <Check size={13} /> : "Save & Authenticate"}
            </button>
          </div>
          <p className="mt-1.5 text-[11px] leading-relaxed text-slate-500 font-sans">
            The orchestrator mints a bootstrap admin key on its first start. Saved locally in this browser's localStorage; never transmitted anywhere other than the orchestrator.
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
    <section className="rounded-xl border border-slate-800 bg-[#0a0f1a] shadow-lg">
      <div className="flex items-center justify-between border-b border-slate-800 px-4 py-3">
        <h2 className="text-sm font-semibold text-slate-200 flex items-center gap-2">
          <ShieldCheck size={14} className="text-cyan-400" />
          <span>API Key Management (Admin Endpoint)</span>
        </h2>
        <div className="flex items-center gap-2">
          <button
            onClick={() => keys.reload()}
            className="p-1 rounded border border-slate-800 text-slate-400 hover:text-slate-200 transition-colors cursor-pointer"
            title="Refresh Key List"
          >
            <RotateCcw size={12} />
          </button>
          <button
            onClick={() => setShowCreate((s) => !s)}
            className="flex items-center gap-1 rounded-md border border-cyan-700/60 bg-cyan-950/40 px-2.5 py-1 text-xs font-semibold text-cyan-300 transition hover:bg-cyan-900/60 cursor-pointer"
          >
            <Plus size={12} /> New Key
          </button>
        </div>
      </div>

      <div className="space-y-3.5 p-4">
        {mintedKey && (
          <div className="rounded-lg border border-amber-700/80 bg-amber-950/40 p-3.5 space-y-2">
            <div className="flex items-start gap-2">
              <AlertTriangle size={15} className="mt-0.5 shrink-0 text-amber-400" />
              <div className="min-w-0 flex-1">
                <div className="text-xs font-bold text-amber-300">Copy this API Key now — shown only once</div>
                <p className="mt-0.5 text-[11px] text-amber-400/90">
                  The orchestrator stores only a SHA-256 hash and cannot reveal it again.
                </p>
                <div className="mt-2 flex items-center gap-2">
                  <code className="min-w-0 flex-1 break-all rounded bg-slate-950/90 px-3 py-2 font-mono text-xs text-slate-200 border border-slate-800">
                    {mintedKey}
                  </code>
                  <button
                    onClick={() => {
                      navigator.clipboard?.writeText(mintedKey);
                      setCopied(true);
                      setTimeout(() => setCopied(false), 1500);
                    }}
                    className="shrink-0 rounded-md border border-slate-700 bg-slate-800 p-2 text-slate-300 transition hover:bg-slate-700 cursor-pointer"
                    title="Copy Key"
                  >
                    {copied ? <Check size={14} className="text-emerald-400" /> : <Copy size={14} />}
                  </button>
                </div>
                <button
                  onClick={() => setMintedKey(null)}
                  className="mt-2 text-xs text-amber-400/80 underline hover:text-amber-300 cursor-pointer"
                >
                  I have securely saved this key — dismiss
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
          <div className="rounded border border-rose-800/60 bg-rose-950/30 px-2.5 py-1.5 text-xs text-rose-300">
            {revoke.error.detail || revoke.error.message}
          </div>
        )}

        <ResourceView
          state={keys}
          compact
          loadingLabel="Loading registered API keys…"
          emptyTitle="No API keys configured"
          emptyMessage="No API keys are registered on this orchestrator."
        >
          {(rows) => (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs min-w-[620px]">
                <thead>
                  <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-500 border-b border-slate-800/80">
                    <th className="pb-2.5 font-normal">Key Name</th>
                    <th className="pb-2.5 font-normal">Assigned Role</th>
                    <th className="pb-2.5 font-normal">Project Scope</th>
                    <th className="pb-2.5 font-normal">Created At</th>
                    <th className="pb-2.5 font-normal">Status</th>
                    <th className="pb-2.5 text-right font-normal">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((k) => (
                    <tr key={k.id} className="border-t border-slate-800/60 hover:bg-slate-900/30 transition-colors">
                      <td className="py-2.5 pr-3 text-slate-200 font-semibold">{k.name}</td>
                      <td className="py-2.5 pr-3">
                        <span className="rounded bg-slate-900 border border-slate-800 px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-cyan-300 font-semibold">
                          {k.role}
                        </span>
                      </td>
                      <td className="py-2.5 pr-3 font-mono text-[10px] text-slate-400">{k.project_id ?? "global"}</td>
                      <td className="py-2.5 pr-3">
                        <Timestamp value={k.created_at} />
                      </td>
                      <td className="py-2.5 pr-3">
                        {k.revoked_at ? (
                          <span className="font-mono text-[10px] text-rose-400 bg-rose-950/40 px-1.5 py-0.5 rounded border border-rose-900/40">
                            revoked
                          </span>
                        ) : (
                          <span className="font-mono text-[10px] text-emerald-400 bg-emerald-950/40 px-1.5 py-0.5 rounded border border-emerald-900/40">
                            active
                          </span>
                        )}
                      </td>
                      <td className="py-2.5 text-right">
                        {!k.revoked_at && (
                          <button
                            onClick={() => doRevoke(k.id)}
                            disabled={revoke.isRunning}
                            title="Revoke key"
                            className="rounded border border-rose-800/60 bg-rose-950/40 p-1.5 text-rose-400 transition hover:bg-rose-900/60 disabled:opacity-40 cursor-pointer"
                          >
                            <Trash2 size={12} />
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
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
    <div className="rounded-lg border border-slate-700 bg-slate-900/70 p-3.5 space-y-3">
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-2.5">
        <div>
          <label className="block text-[10px] font-mono uppercase text-slate-400 mb-1">Key Name</label>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="ci-github-actions"
            className="w-full rounded border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs text-slate-200 outline-none focus:border-cyan-500 font-sans"
          />
        </div>
        <div>
          <label className="block text-[10px] font-mono uppercase text-slate-400 mb-1">Assigned Role</label>
          <select
            value={role}
            onChange={(e) => setRole(e.target.value as ApiRole)}
            className="w-full rounded border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs text-slate-300 outline-none focus:border-cyan-500 font-mono"
          >
            <option value="viewer">viewer (read-only)</option>
            <option value="operator">operator (launch/approve)</option>
            <option value="admin">admin (full control plane)</option>
          </select>
        </div>
        <div>
          <label className="block text-[10px] font-mono uppercase text-slate-400 mb-1">Project Scope (Optional)</label>
          <input
            value={projectId}
            onChange={(e) => setProjectId(e.target.value)}
            placeholder="global or project_id"
            className="w-full rounded border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs text-slate-200 outline-none focus:border-cyan-500 font-mono"
          />
        </div>
      </div>

      {create.error && (
        <div className="rounded border border-rose-800/60 bg-rose-950/30 px-2.5 py-1.5 text-xs text-rose-300">
          {create.error.detail || create.error.message}
        </div>
      )}

      <div className="flex justify-end gap-2 pt-1">
        <button
          onClick={onCancel}
          className="rounded border border-slate-700 bg-slate-800 px-3 py-1.5 text-xs text-slate-400 transition hover:bg-slate-700 cursor-pointer"
        >
          Cancel
        </button>
        <button
          onClick={submit}
          disabled={create.isRunning || !name.trim()}
          className="rounded border border-cyan-700/60 bg-cyan-600 px-4 py-1.5 text-xs font-semibold text-white transition hover:bg-cyan-500 disabled:opacity-40 cursor-pointer"
        >
          {create.isRunning ? "Creating…" : "Mint API Key"}
        </button>
      </div>
    </div>
  );
}
