/**
 * Phase 4 — API Testing: real, ad-hoc HTTP request execution.
 *
 * Replaces the previous honest redirect to the Execution Hub (ACC-012 —
 * that page correctly stopped claiming "not implemented" once real API
 * testing shipped via Newman/Pytest/Schemathesis, but had no request-
 * builder UI of its own). Re-verified against the current backend before
 * writing this: `POST .../api-testing/execute` (`api_testing.rs`) now
 * exists for real, reusing the exact Phase 2.1 execution pipeline
 * (`execution_plan::validate_plan` — the same real SSRF guard —
 * `materialize_task_def`, `create_run_core`) and the real, native
 * `executor::CompiledApiExecutor` (direct `reqwest`, real assertion
 * evaluation against the real response) — no new executor, no new
 * execution engine, no new persistence path.
 *
 * "Execute" (ad-hoc) and "Save as Test Case" are deliberately distinct
 * actions (brief §11/§26): executing never implies persistence, and
 * saving reuses the exact existing `POST .../test-cases` a Test Case
 * Generator draft already uses — a saved case's step text is phrased so
 * the existing deterministic compiler (`execution_plan::compile_
 * deterministic_api`) can independently re-derive the same method/URL/
 * status-assertion plan later, with zero new persistence code.
 */
import React, { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Network, AlertTriangle, CheckCircle2, XCircle, Play, Plus, Trash2, Save, Clock, ShieldAlert, FileJson, ListChecks } from 'lucide-react';
import { SectionHeader } from '../common/SectionHeader';
import { EmptyState } from '../common/EmptyState';
import { ApiTestingFlowRail } from '../api-testing/ApiTestingFlowRail';
import { useProject } from '../../context/ProjectContext';
import { ApiError, aiqaApi } from '../../services/aiqaApi';
import type { ApiTestExecutionResponse, PlanAction } from '../../types/contract';

const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'] as const;
type Method = (typeof METHODS)[number];

export type ExecStatus =
  | 'IDLE'
  | 'VALIDATION_FAILED'
  | 'NO_ACTIVE_PROJECT'
  | 'EXECUTING'
  | 'PASSED'
  | 'FAILED'
  | 'INFRA_ERROR'
  | 'STILL_RUNNING'
  | 'PROJECT_ACCESS_DENIED'
  | 'REQUEST_FAILED';

type KVRow = { key: string; value: string };

type AssertionRow =
  | { kind: 'STATUS'; status: string }
  | { kind: 'HEADER'; name: string; value: string }
  | { kind: 'BODY_FIELD'; path: string; equals: string };

function kvRowsToRecord(rows: KVRow[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const r of rows) {
    if (r.key.trim()) out[r.key.trim()] = r.value;
  }
  return out;
}

function assertionRowsToActions(rows: AssertionRow[]): PlanAction[] {
  const out: PlanAction[] = [];
  for (const r of rows) {
    if (r.kind === 'STATUS' && r.status.trim()) {
      const n = Number(r.status.trim());
      if (Number.isInteger(n)) out.push({ type: 'ASSERT_STATUS', status: n });
    } else if (r.kind === 'HEADER' && r.name.trim()) {
      out.push({ type: 'ASSERT_HEADER', name: r.name.trim(), value: r.value });
    } else if (r.kind === 'BODY_FIELD' && r.path.trim()) {
      let equals: unknown = r.equals;
      try {
        equals = JSON.parse(r.equals);
      } catch {
        // not JSON — compared as a literal string, same as typed
      }
      out.push({ type: 'ASSERT_BODY_FIELD', path: r.path.trim(), equals });
    }
  }
  return out;
}

export const APITestingPage: React.FC = () => {
  const navigate = useNavigate();
  const { selectedProjectId: realProjectId, selectedProject } = useProject();

  const [method, setMethod] = useState<Method>('GET');
  const [url, setUrl] = useState('');
  const [queryParams, setQueryParams] = useState<KVRow[]>([]);
  const [headers, setHeaders] = useState<KVRow[]>([]);
  const [bodyText, setBodyText] = useState('');
  const [assertions, setAssertions] = useState<AssertionRow[]>([{ kind: 'STATUS', status: '200' }]);

  const [status, setStatus] = useState<ExecStatus>('IDLE');
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [result, setResult] = useState<ApiTestExecutionResponse | null>(null);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const isExecuting = status === 'EXECUTING';

  const handleExecute = async () => {
    if (!realProjectId) {
      setStatus('NO_ACTIVE_PROJECT');
      setErrorMessage('Select a project before executing an API test.');
      return;
    }
    if (!url.trim()) {
      setStatus('VALIDATION_FAILED');
      setErrorMessage('INVALID_URL: enter a request URL before executing.');
      return;
    }
    let parsedBody: unknown;
    if (bodyText.trim()) {
      try {
        parsedBody = JSON.parse(bodyText);
      } catch (e) {
        setStatus('VALIDATION_FAILED');
        setErrorMessage(`INVALID_JSON_BODY: ${e instanceof Error ? e.message : 'request body is not valid JSON'}`);
        return;
      }
    }

    setErrorMessage(null);
    setSaveMessage(null);
    setResult(null);
    setStatus('EXECUTING');

    try {
      const response = await aiqaApi.apiTesting.execute(realProjectId, {
        method,
        url: url.trim(),
        query_params: queryParams.filter((r) => r.key.trim()).map((r) => [r.key.trim(), r.value] as [string, string]),
        headers: kvRowsToRecord(headers),
        body: parsedBody,
        assertions: assertionRowsToActions(assertions),
      });
      setResult(response);
      if (response.task_status === 'PASSED') {
        setStatus('PASSED');
      } else if (response.task_status === 'FAILED') {
        setStatus('FAILED');
      } else if (response.task_status === 'INFRA_ERROR') {
        setStatus('INFRA_ERROR');
        setErrorMessage(response.reason || 'The real request could not be completed.');
      } else if (response.task_status === 'RUNNING') {
        setStatus('STILL_RUNNING');
      } else {
        setStatus('INFRA_ERROR');
        setErrorMessage(response.reason || `Unexpected real task status: ${response.task_status}`);
      }
    } catch (err) {
      setResult(null);
      if (err instanceof ApiError && err.kind === 'forbidden') {
        setStatus('PROJECT_ACCESS_DENIED');
        setErrorMessage(err.message);
      } else if (err instanceof ApiError && err.kind === 'validation_error') {
        setStatus('VALIDATION_FAILED');
        setErrorMessage(err.message);
      } else {
        setStatus('REQUEST_FAILED');
        setErrorMessage(err instanceof ApiError ? err.message : 'API test execution failed.');
      }
    }
  };

  const handleSaveAsTestCase = async () => {
    if (!realProjectId) return;
    setSaving(true);
    setSaveMessage(null);
    const statusAssertion = assertions.find((a) => a.kind === 'STATUS' && a.status.trim());
    const expectedResult = statusAssertion && statusAssertion.kind === 'STATUS' ? `HTTP ${statusAssertion.status.trim()}` : 'Request completes successfully';
    try {
      const created = await aiqaApi.testCases.create(realProjectId, {
        title: `${method} ${url.trim()}`.slice(0, 200),
        description: 'Ad-hoc API test built and saved from the API Testing page.',
        category: 'API',
        priority: 'MEDIUM',
        severity: 'MINOR',
        preconditions: [],
        steps: [{ step_number: 1, action: `${method} ${url.trim()}`, expected_result: expectedResult }],
        expected_result: expectedResult,
        assertions: assertions.map((a) => (a.kind === 'STATUS' ? `Status equals ${a.status}` : a.kind === 'HEADER' ? `Header ${a.name} equals ${a.value}` : `Body field ${a.path} equals ${a.equals}`)),
        requirement_reference: 'API Testing page (ad-hoc request builder)',
        traceability: {
          source: 'api_testing',
          method,
          url: url.trim(),
          query_params: queryParams.filter((r) => r.key.trim()),
          headers: Object.keys(kvRowsToRecord(headers)),
          assertion_count: assertions.length,
        },
      });
      setSaveMessage(`Saved as test case ${created.id} — GENERATED status, review it on the Test Case Generator page.`);
    } catch (err) {
      setSaveMessage(err instanceof ApiError ? `Save failed: ${err.message}` : 'Save failed.');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div id="api-testing-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="API Testing"
        subtitle="Build and execute a real, authorized API request against a real target — real status, real latency, real assertions. No load/stress/performance testing here."
        badge={status === 'PASSED' ? 'PASSED' : status === 'FAILED' ? 'FAILED' : undefined}
        icon={Network}
      />

      {!realProjectId ? (
        <EmptyState
          icon={AlertTriangle}
          badge="NO ACTIVE PROJECT"
          title="Select a project before executing an API test"
          description="API Testing requests are project-scoped, same as the Test Case Generator — choose a project from the selector before building a request."
          actionLabel="Go to Dashboard"
          onAction={() => navigate('/dashboard')}
        />
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-start">
          <div className="lg:col-span-5 space-y-4">
            <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-xs font-mono text-slate-400">Project Context</span>
                <span className="text-xs font-mono font-semibold text-cyan-300">{selectedProject?.name ?? realProjectId}</span>
              </div>

              <div className="flex gap-2">
                <select
                  id="api-testing-method"
                  value={method}
                  onChange={(e) => setMethod(e.target.value as Method)}
                  disabled={isExecuting}
                  className="px-2 py-2 rounded-md bg-[#020617] border border-slate-700 text-sm font-mono font-bold text-cyan-300 cursor-pointer disabled:opacity-60"
                >
                  {METHODS.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
                <input
                  id="api-testing-url"
                  type="text"
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  placeholder="https://api.example.com/v1/resource"
                  disabled={isExecuting}
                  className="flex-1 min-w-0 px-3 py-2 rounded-md bg-[#020617] border border-slate-700 text-sm font-mono text-slate-100 placeholder-slate-600 focus:outline-none focus:border-indigo-500 disabled:opacity-60"
                />
              </div>

              <KVListEditor label="Query Parameters" rows={queryParams} setRows={setQueryParams} disabled={isExecuting} keyPlaceholder="param" valuePlaceholder="value" idPrefix="query" />
              <KVListEditor label="Headers" rows={headers} setRows={setHeaders} disabled={isExecuting} keyPlaceholder="Header-Name" valuePlaceholder="value" idPrefix="header" />
              <p className="text-[10px] font-mono text-slate-500 -mt-2">
                Authentication: add an <span className="text-slate-300">Authorization</span> header directly above (e.g. <span className="text-slate-300">Bearer &lt;token&gt;</span>) — there is no
                separate credential-storage feature; the header value is sent exactly once, redacted from any response data this page displays.
              </p>

              <div>
                <label htmlFor="api-testing-body" className="block text-xs font-mono text-slate-400 mb-1 flex items-center gap-1.5">
                  <FileJson className="w-3.5 h-3.5" /> Request Body (JSON, optional)
                </label>
                <textarea
                  id="api-testing-body"
                  value={bodyText}
                  onChange={(e) => setBodyText(e.target.value)}
                  placeholder='{"key": "value"}'
                  disabled={isExecuting}
                  rows={4}
                  className="w-full px-3 py-2 rounded-md bg-[#020617] border border-slate-700 text-xs font-mono text-slate-100 placeholder-slate-600 focus:outline-none focus:border-indigo-500 disabled:opacity-60 resize-y"
                />
              </div>
            </div>

            <AssertionsEditor assertions={assertions} setAssertions={setAssertions} disabled={isExecuting} />

            <div className="flex items-center justify-between">
              {getStatusBadge(status)}
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  id="btn-save-as-test-case"
                  onClick={handleSaveAsTestCase}
                  disabled={saving || !url.trim()}
                  className="flex items-center gap-1.5 px-4 py-2 rounded-md bg-slate-800 hover:bg-slate-700 border border-slate-600 text-slate-200 font-mono text-xs font-bold cursor-pointer transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  <Save className="w-3.5 h-3.5" />
                  <span>{saving ? 'Saving…' : 'Save as Test Case'}</span>
                </button>
                <button
                  type="button"
                  id="btn-execute-api-test"
                  onClick={handleExecute}
                  disabled={isExecuting}
                  className="flex items-center gap-1.5 px-5 py-2 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-mono text-xs font-bold cursor-pointer transition-colors shadow-md disabled:opacity-60 disabled:cursor-not-allowed"
                >
                  <Play className="w-4 h-4" />
                  <span>{isExecuting ? 'Executing…' : 'Execute'}</span>
                </button>
              </div>
            </div>
            {saveMessage && <p id="api-testing-save-message" className="text-[11px] font-mono text-slate-400">{saveMessage}</p>}
            <p className="text-[10px] font-mono text-slate-500">
              "Execute" runs this exact request for real, right now, and is not saved anywhere by itself. "Save as Test Case" persists it via the existing test case pipeline — review, approve, and
              hand it off to real execution from the Test Case Generator page, exactly like any other generated case.
            </p>
          </div>

          <div className="lg:col-span-5 space-y-4">
            {errorMessage && (status === 'VALIDATION_FAILED' || status === 'NO_ACTIVE_PROJECT' || status === 'INFRA_ERROR' || status === 'PROJECT_ACCESS_DENIED' || status === 'REQUEST_FAILED') && (
              <div id="api-testing-error" className="p-3 rounded-md bg-rose-950/60 border border-rose-700 text-xs font-mono text-rose-300">
                {errorMessage}
              </div>
            )}

            {status === 'STILL_RUNNING' && result && (
              <div className="p-3 rounded-md bg-amber-950/60 border border-amber-700 text-xs font-mono text-amber-300">
                Execution is still running past the backend's own wait ceiling — the real run continues.{' '}
                <button type="button" className="underline cursor-pointer" onClick={() => navigate(`/execution?run=${result.run_id}`)}>
                  View in Execution Hub
                </button>
                .
              </div>
            )}

            {result && result.result && (status === 'PASSED' || status === 'FAILED') && (
              <div id="api-testing-response" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-4">
                <div className="flex items-center justify-between pb-2 border-b border-slate-800/80">
                  <div className="flex items-center gap-2">
                    {status === 'PASSED' ? <CheckCircle2 className="w-4 h-4 text-emerald-400" /> : <XCircle className="w-4 h-4 text-rose-400" />}
                    <span className="text-sm font-mono font-bold text-slate-200">Actual Response</span>
                  </div>
                  <span className={`px-2.5 py-1 rounded text-[11px] font-mono font-bold border ${status === 'PASSED' ? 'bg-emerald-950/80 border-emerald-700 text-emerald-300' : 'bg-rose-950/80 border-rose-700 text-rose-300'}`}>
                    {result.result.status}
                  </span>
                </div>

                <div className="grid grid-cols-2 gap-3 text-xs font-mono">
                  <div>
                    <div className="flex items-center gap-1 text-slate-500 mb-0.5">
                      <Clock className="w-3 h-3" /> Network Request Latency
                    </div>
                    <div className="text-slate-200">{result.result.request_duration_ms} ms (real, measured request/response time)</div>
                  </div>
                  <div>
                    <div className="flex items-center gap-1 text-slate-500 mb-0.5">
                      <Clock className="w-3 h-3" /> End-to-End Task Duration
                    </div>
                    <div className="text-slate-200">
                      {result.started_at && result.finished_at ? `${Math.max(0, new Date(result.finished_at).getTime() - new Date(result.started_at).getTime())} ms` : 'Not reported'} (includes real
                      scheduling overhead)
                    </div>
                  </div>
                </div>

                <div>
                  <div className="text-[11px] font-mono text-slate-400 mb-1">Response Headers (sensitive values redacted)</div>
                  <div className="bg-[#020617] border border-slate-800 rounded p-2 max-h-32 overflow-y-auto text-[11px] font-mono text-slate-300 space-y-0.5">
                    {result.result.response_headers.map(([k, v]) => (
                      <div key={k}>
                        <span className="text-slate-500">{k}:</span> {v}
                      </div>
                    ))}
                  </div>
                </div>

                <div>
                  <div className="text-[11px] font-mono text-slate-400 mb-1">Response Body</div>
                  <pre className="bg-[#020617] border border-slate-800 rounded p-2 max-h-48 overflow-auto text-[11px] font-mono text-slate-300 whitespace-pre-wrap break-all">
                    {result.response_body_preview ?? '(no body captured)'}
                  </pre>
                </div>

                <div>
                  <div className="flex items-center gap-1.5 text-[11px] font-mono text-slate-400 mb-1.5">
                    <ListChecks className="w-3.5 h-3.5" /> Assertions (real, evaluated results)
                  </div>
                  <div className="space-y-1">
                    {result.result.assertions.length === 0 && <p className="text-[11px] font-mono text-slate-500">No assertions were configured for this request.</p>}
                    {result.result.assertions.map((a, i) => (
                      <div key={i} className={`flex items-center justify-between px-2 py-1.5 rounded text-[11px] font-mono border ${a.passed ? 'bg-emerald-950/40 border-emerald-800 text-emerald-300' : 'bg-rose-950/40 border-rose-800 text-rose-300'}`}>
                        <span>{a.description}</span>
                        <span className="flex items-center gap-1">
                          {a.passed ? <CheckCircle2 className="w-3 h-3" /> : <XCircle className="w-3 h-3" />}
                          {a.passed ? 'PASS' : `FAIL (actual: ${a.actual})`}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            )}

            {!result && status === 'IDLE' && (
              <div className="p-6 rounded-lg bg-[#0B1120] border border-slate-800 border-dashed text-center text-xs font-mono text-slate-500">
                Build a request and click Execute to see the real response here.
              </div>
            )}
          </div>

          <div className="hidden lg:block lg:col-span-2 min-w-0">
            <ApiTestingFlowRail method={method} status={status} result={result} />
          </div>
        </div>
      )}
    </div>
  );
};

function getStatusBadge(status: ExecStatus) {
  switch (status) {
    case 'EXECUTING':
      return (
        <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-bold bg-purple-950/80 border border-purple-700/80 text-purple-300">
          <Play className="w-3.5 h-3.5 text-purple-400 animate-pulse" />
          Executing Request
        </span>
      );
    case 'PASSED':
      return (
        <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-bold bg-emerald-950/80 border border-emerald-700/80 text-emerald-300">
          <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
          Passed
        </span>
      );
    case 'FAILED':
      return (
        <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-bold bg-rose-950/80 border border-rose-700/80 text-rose-300">
          <XCircle className="w-3.5 h-3.5 text-rose-400" />
          Failed
        </span>
      );
    case 'INFRA_ERROR':
    case 'REQUEST_FAILED':
    case 'PROJECT_ACCESS_DENIED':
    case 'VALIDATION_FAILED':
      return (
        <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-bold bg-rose-950/80 border border-rose-700/80 text-rose-300">
          <ShieldAlert className="w-3.5 h-3.5 text-rose-400" />
          {status === 'INFRA_ERROR' ? 'Execution Error' : status === 'PROJECT_ACCESS_DENIED' ? 'Access Denied' : status === 'VALIDATION_FAILED' ? 'Validation Failed' : 'Request Failed'}
        </span>
      );
    case 'STILL_RUNNING':
      return (
        <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-bold bg-amber-950/80 border border-amber-700/80 text-amber-300">
          <Clock className="w-3.5 h-3.5 text-amber-400" />
          Still Running
        </span>
      );
    case 'NO_ACTIVE_PROJECT':
    case 'IDLE':
    default:
      return (
        <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-semibold bg-slate-900 border border-slate-700 text-cyan-400">
          <span className="w-2 h-2 rounded-full bg-cyan-400 animate-pulse" />
          Ready
        </span>
      );
  }
}

const KVListEditor: React.FC<{ label: string; rows: KVRow[]; setRows: (r: KVRow[]) => void; disabled: boolean; keyPlaceholder: string; valuePlaceholder: string; idPrefix: string }> = ({
  label,
  rows,
  setRows,
  disabled,
  keyPlaceholder,
  valuePlaceholder,
  idPrefix,
}) => (
  <div>
    <div className="flex items-center justify-between mb-1">
      <span className="text-xs font-mono text-slate-400">{label}</span>
      <button
        type="button"
        id={`btn-add-${idPrefix}-row`}
        onClick={() => setRows([...rows, { key: '', value: '' }])}
        disabled={disabled}
        className="flex items-center gap-1 text-[10px] font-mono text-indigo-400 hover:text-indigo-300 cursor-pointer disabled:opacity-50"
      >
        <Plus className="w-3 h-3" /> Add
      </button>
    </div>
    <div className="space-y-1">
      {rows.map((row, i) => (
        <div key={i} className="flex gap-1.5">
          <input
            type="text"
            value={row.key}
            onChange={(e) => {
              const next = [...rows];
              next[i] = { ...next[i], key: e.target.value };
              setRows(next);
            }}
            placeholder={keyPlaceholder}
            disabled={disabled}
            className="flex-1 min-w-0 px-2 py-1 rounded bg-[#020617] border border-slate-700 text-[11px] font-mono text-slate-100 placeholder-slate-600 disabled:opacity-60"
          />
          <input
            type="text"
            value={row.value}
            onChange={(e) => {
              const next = [...rows];
              next[i] = { ...next[i], value: e.target.value };
              setRows(next);
            }}
            placeholder={valuePlaceholder}
            disabled={disabled}
            className="flex-1 min-w-0 px-2 py-1 rounded bg-[#020617] border border-slate-700 text-[11px] font-mono text-slate-100 placeholder-slate-600 disabled:opacity-60"
          />
          <button type="button" onClick={() => setRows(rows.filter((_, j) => j !== i))} disabled={disabled} className="text-slate-500 hover:text-rose-400 cursor-pointer disabled:opacity-50">
            <Trash2 className="w-3.5 h-3.5" />
          </button>
        </div>
      ))}
    </div>
  </div>
);

const AssertionsEditor: React.FC<{ assertions: AssertionRow[]; setAssertions: (a: AssertionRow[]) => void; disabled: boolean }> = ({ assertions, setAssertions, disabled }) => (
  <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-2">
    <div className="flex items-center justify-between">
      <span className="text-xs font-mono text-slate-400 flex items-center gap-1.5">
        <ListChecks className="w-3.5 h-3.5" /> Assertions
      </span>
      <div className="flex items-center gap-2">
        <button type="button" onClick={() => setAssertions([...assertions, { kind: 'STATUS', status: '200' }])} disabled={disabled} className="text-[10px] font-mono text-indigo-400 hover:text-indigo-300 cursor-pointer disabled:opacity-50">
          + Status
        </button>
        <button type="button" onClick={() => setAssertions([...assertions, { kind: 'HEADER', name: '', value: '' }])} disabled={disabled} className="text-[10px] font-mono text-indigo-400 hover:text-indigo-300 cursor-pointer disabled:opacity-50">
          + Header
        </button>
        <button type="button" onClick={() => setAssertions([...assertions, { kind: 'BODY_FIELD', path: '', equals: '' }])} disabled={disabled} className="text-[10px] font-mono text-indigo-400 hover:text-indigo-300 cursor-pointer disabled:opacity-50">
          + Body Field
        </button>
      </div>
    </div>
    {assertions.length === 0 && <p className="text-[11px] font-mono text-slate-500">No assertions configured — execution will still run for real, with no pass/fail evaluation.</p>}
    <div className="space-y-1.5">
      {assertions.map((a, i) => (
        <div key={i} className="flex items-center gap-1.5">
          {a.kind === 'STATUS' && (
            <>
              <span className="text-[11px] font-mono text-slate-500 w-24 shrink-0">Status equals</span>
              <input
                type="text"
                value={a.status}
                onChange={(e) => {
                  const next = [...assertions];
                  next[i] = { ...a, status: e.target.value };
                  setAssertions(next);
                }}
                placeholder="200"
                disabled={disabled}
                className="w-20 px-2 py-1 rounded bg-[#020617] border border-slate-700 text-[11px] font-mono text-slate-100 disabled:opacity-60"
              />
            </>
          )}
          {a.kind === 'HEADER' && (
            <>
              <span className="text-[11px] font-mono text-slate-500 w-24 shrink-0">Header</span>
              <input
                type="text"
                value={a.name}
                onChange={(e) => {
                  const next = [...assertions];
                  next[i] = { ...a, name: e.target.value };
                  setAssertions(next);
                }}
                placeholder="Content-Type"
                disabled={disabled}
                className="flex-1 min-w-0 px-2 py-1 rounded bg-[#020617] border border-slate-700 text-[11px] font-mono text-slate-100 disabled:opacity-60"
              />
              <span className="text-[11px] font-mono text-slate-500">=</span>
              <input
                type="text"
                value={a.value}
                onChange={(e) => {
                  const next = [...assertions];
                  next[i] = { ...a, value: e.target.value };
                  setAssertions(next);
                }}
                placeholder="application/json"
                disabled={disabled}
                className="flex-1 min-w-0 px-2 py-1 rounded bg-[#020617] border border-slate-700 text-[11px] font-mono text-slate-100 disabled:opacity-60"
              />
            </>
          )}
          {a.kind === 'BODY_FIELD' && (
            <>
              <span className="text-[11px] font-mono text-slate-500 w-24 shrink-0">Body field</span>
              <input
                type="text"
                value={a.path}
                onChange={(e) => {
                  const next = [...assertions];
                  next[i] = { ...a, path: e.target.value };
                  setAssertions(next);
                }}
                placeholder="data.id"
                disabled={disabled}
                className="flex-1 min-w-0 px-2 py-1 rounded bg-[#020617] border border-slate-700 text-[11px] font-mono text-slate-100 disabled:opacity-60"
              />
              <span className="text-[11px] font-mono text-slate-500">=</span>
              <input
                type="text"
                value={a.equals}
                onChange={(e) => {
                  const next = [...assertions];
                  next[i] = { ...a, equals: e.target.value };
                  setAssertions(next);
                }}
                placeholder='"value" or 42'
                disabled={disabled}
                className="flex-1 min-w-0 px-2 py-1 rounded bg-[#020617] border border-slate-700 text-[11px] font-mono text-slate-100 disabled:opacity-60"
              />
            </>
          )}
          <button type="button" onClick={() => setAssertions(assertions.filter((_, j) => j !== i))} disabled={disabled} className="text-slate-500 hover:text-rose-400 cursor-pointer disabled:opacity-50">
            <Trash2 className="w-3.5 h-3.5" />
          </button>
        </div>
      ))}
    </div>
  </div>
);
