/**
 * Phase 8B — what to test: the OpenAPI document (pasted, or by address), where the API is, which kinds of case, the headers that carry a
 * credential (by the name of the orchestrator's variable that holds it: never the credential), and whether requests that change data
 * may be run. It only holds and shows; the service reads the document and makes the cases.
 */
import React from 'react';
import { AlertTriangle, Plus, Trash2 } from 'lucide-react';
import {
  KIND_LABEL,
  KIND_ORDER,
  MAX_CASES_LIMIT,
  type CredentialRow,
  type OpenApiFormState,
} from './openApiModel';
import type { OpenApiCaseKind } from '../../types/contract';

const INPUT =
  'w-full px-2 py-1.5 rounded-md bg-[#020617] border border-slate-700 text-xs text-slate-100 placeholder:text-content-muted focus:outline-none focus:border-violet-500 disabled:opacity-50';
const SEGMENT = (on: boolean) => `px-2.5 py-1 text-xs cursor-pointer ${on ? 'bg-violet-950/70 text-violet-200' : 'text-slate-400 hover:bg-slate-800'}`;

let nextCredentialId = 1;
const newCredential = (): CredentialRow => ({ id: nextCredentialId++, header: 'Authorization', variable: '' });

export const OpenApiForm: React.FC<{
  form: OpenApiFormState;
  onChange: (next: OpenApiFormState) => void;
  problems: string[];
  onShowCases: () => void;
  showing: boolean;
  disabled?: boolean;
}> = ({ form, onChange, problems, onShowCases, showing, disabled = false }) => {
  const set = (patch: Partial<OpenApiFormState>) => onChange({ ...form, ...patch });
  const toggleKind = (kind: OpenApiCaseKind) =>
    set({ kinds: form.kinds.includes(kind) ? form.kinds.filter((k) => k !== kind) : KIND_ORDER.filter((k) => k === kind || form.kinds.includes(k)) });
  const setCredential = (id: number, patch: Partial<CredentialRow>) => set({ credentials: form.credentials.map((c) => (c.id === id ? { ...c, ...patch } : c)) });

  return (
    <form
      id="openapi-form"
      onSubmit={(e) => {
        e.preventDefault();
        if (problems.length === 0 && !showing && !disabled) onShowCases();
      }}
      className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 space-y-4"
    >
      <fieldset disabled={disabled} aria-labelledby="openapi-document-title" className="space-y-3">
        <div className="flex flex-wrap items-center gap-2">
          <h3 id="openapi-document-title" className="text-xs font-semibold text-slate-100 mr-2">
            The OpenAPI document
          </h3>
          <div role="group" aria-label="Where the document is" className="inline-flex rounded-md border border-slate-700 overflow-hidden">
            <button type="button" id="openapi-source-paste" aria-pressed={form.source === 'paste'} onClick={() => set({ source: 'paste' })} className={SEGMENT(form.source === 'paste')}>
              Paste it
            </button>
            <button type="button" id="openapi-source-address" aria-pressed={form.source === 'address'} onClick={() => set({ source: 'address' })} className={SEGMENT(form.source === 'address')}>
              Give its address
            </button>
          </div>
        </div>
        {form.source === 'paste' ? (
          <textarea
            id="openapi-spec"
            aria-label="OpenAPI document"
            value={form.spec}
            onChange={(e) => set({ spec: e.target.value })}
            rows={10}
            spellCheck={false}
            placeholder={'openapi: 3.0.3\ninfo:\n  title: My API\n  version: "1.0"\nservers:\n  - url: https://api.example.com/v1\npaths:\n  /pets: …'}
            className={`${INPUT} font-mono`}
          />
        ) : (
          <input
            id="openapi-spec-url"
            aria-label="Address of the OpenAPI document"
            value={form.specUrl}
            onChange={(e) => set({ specUrl: e.target.value })}
            placeholder="https://api.example.com/openapi.json"
            className={INPUT}
          />
        )}
        <label className="block space-y-1">
          <span className="text-[11px] text-slate-300">Where the API is</span>
          <input
            id="openapi-base-url"
            value={form.baseUrl}
            onChange={(e) => set({ baseUrl: e.target.value })}
            placeholder="https://api.example.com/v1 (empty: the document’s own server)"
            className={INPUT}
          />
        </label>
      </fieldset>

      <fieldset disabled={disabled} className="space-y-1.5">
        <legend className="text-xs font-semibold text-slate-100">Kinds of case</legend>
        {KIND_ORDER.map((kind) => (
          <label key={kind} className="flex items-start gap-2 text-[11px] text-slate-300 cursor-pointer">
            <input id={`openapi-kind-${kind}`} type="checkbox" checked={form.kinds.includes(kind)} onChange={() => toggleKind(kind)} className="mt-0.5 accent-violet-500" />
            <span>
              <span className="text-slate-100">{KIND_LABEL[kind].title}</span>
              <span className="text-content-muted"> — {KIND_LABEL[kind].hint}</span>
            </span>
          </label>
        ))}
        <label className="flex items-center gap-2 pt-1 text-[11px] text-slate-300">
          Most cases for one operation
          <input
            id="openapi-max-cases"
            type="number"
            min={1}
            max={MAX_CASES_LIMIT}
            value={form.maxCases}
            onChange={(e) => set({ maxCases: Number(e.target.value) })}
            className="w-16 px-2 py-1 rounded-md bg-[#020617] border border-slate-700 text-xs text-slate-100"
          />
        </label>
      </fieldset>

      <fieldset disabled={disabled} className="space-y-2">
        <legend className="text-xs font-semibold text-slate-100">Credentials</legend>
        <p className="text-[11px] text-content-muted">
          A header that carries a credential is given as the name of a variable in the orchestrator’s environment (it must start with AIQA_SECRET_ or
          AIQA_LOGIN_). The value is read when a request is made and is never stored, shown or sent anywhere else.
        </p>
        {form.credentials.map((row) => (
          <div key={row.id} className="flex flex-wrap items-center gap-2">
            <input
              id={`openapi-credential-header-${row.id}`}
              aria-label="Header"
              value={row.header}
              onChange={(e) => setCredential(row.id, { header: e.target.value })}
              placeholder="Authorization"
              className={`${INPUT} sm:w-44`}
            />
            <input
              id={`openapi-credential-variable-${row.id}`}
              aria-label="Variable"
              value={row.variable}
              onChange={(e) => setCredential(row.id, { variable: e.target.value })}
              placeholder="AIQA_SECRET_API_TOKEN"
              className={`${INPUT} sm:w-64 font-mono`}
            />
            <button
              type="button"
              id={`openapi-credential-remove-${row.id}`}
              aria-label="Remove this header"
              onClick={() => set({ credentials: form.credentials.filter((c) => c.id !== row.id) })}
              className="p-1.5 rounded text-slate-400 hover:bg-slate-800 cursor-pointer"
            >
              <Trash2 className="w-3.5 h-3.5" />
            </button>
          </div>
        ))}
        <button
          type="button"
          id="openapi-add-credential"
          onClick={() => set({ credentials: [...form.credentials, newCredential()] })}
          className="inline-flex items-center gap-1 px-2 py-1 rounded border border-slate-700 text-[11px] text-slate-200 hover:bg-slate-800 cursor-pointer"
        >
          <Plus className="w-3 h-3" /> Add a header
        </button>
      </fieldset>

      <fieldset disabled={disabled} className="space-y-1">
        <label className="flex items-start gap-2 text-[11px] text-slate-300 cursor-pointer">
          <input id="openapi-unsafe" type="checkbox" checked={form.includeUnsafe} onChange={(e) => set({ includeUnsafe: e.target.checked })} className="mt-0.5 accent-amber-500" />
          <span>Also run the requests that change data (POST, PUT, PATCH, DELETE)</span>
        </label>
        {form.includeUnsafe && (
          <p id="openapi-unsafe-warning" className="flex items-start gap-1.5 text-[11px] text-amber-300">
            <AlertTriangle className="w-3.5 h-3.5 mt-0.5 shrink-0" />
            These requests are really sent: they can create, change or delete data in the API they are aimed at. Use a test environment.
          </p>
        )}
      </fieldset>

      {problems.length > 0 && (
        <ul id="openapi-problems" className="space-y-0.5">
          {problems.map((p) => (
            <li key={p} className="text-[11px] text-amber-300">
              {p}
            </li>
          ))}
        </ul>
      )}
      <button
        type="submit"
        id="openapi-show-cases"
        disabled={problems.length > 0 || showing || disabled}
        className="px-3 py-1.5 rounded-md bg-violet-600 text-xs font-semibold text-white hover:bg-violet-500 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {showing ? 'Reading the document…' : 'Show the cases'}
      </button>
    </form>
  );
};
