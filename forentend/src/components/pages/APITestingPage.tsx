/**
 * AIQA API Testing Workspace
 * Multi-adapter API test harness supporting REST HTTP, GraphQL, gRPC, and WebSocket protocols.
 */

import React, { useState } from 'react';
import {
  Network,
  Send,
  Plus,
  Play,
  CheckCircle2,
  XCircle,
  FileCode,
  ShieldCheck,
  Zap,
  Code2,
  Clock,
  ChevronRight,
  Globe,
  Sliders,
  Terminal,
  Activity
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { SectionHeader } from '../common/SectionHeader';
import { StatusBadge } from '../common/StatusBadge';
import { EmptyState } from '../common/EmptyState';
import { APIEndpointTest, APIAdapterType, APIAssertion } from '../../types';

export const APITestingPage: React.FC = () => {
  const {
    apiTests,
    activeApiTestId,
    setActiveApiTestId,
    addApiTest,
    executeApiTest,
    currentEnvironment,
  } = useAIQA();

  const [activeTab, setActiveTab] = useState<'params' | 'headers' | 'auth' | 'body' | 'assertions'>('headers');
  const [isNewEndpointOpen, setIsNewEndpointOpen] = useState(false);

  // New Test Form State
  const [newName, setNewName] = useState('Create Ephemeral Tenant Session');
  const [newMethod, setNewMethod] = useState<APIEndpointTest['method']>('POST');
  const [newAdapter, setNewAdapter] = useState<APIAdapterType>('REST_HTTP');
  const [newUrl, setNewUrl] = useState('https://staging.enterprise.com/api/v2/tenants/session');

  const activeTest = apiTests.find((t) => t.id === activeApiTestId) || apiTests[0];

  const handleCreateTest = (e: React.FormEvent) => {
    e.preventDefault();
    const newTest: APIEndpointTest = {
      id: `api-t-${Date.now().toString().slice(-4)}`,
      name: newName,
      adapter: newAdapter,
      method: newMethod,
      endpointUrl: newUrl,
      headers: { 'Content-Type': 'application/json' },
      queryParams: {},
      authType: 'BEARER_TOKEN',
      bodyType: newMethod === 'GET' ? 'NONE' : 'JSON',
      bodyPayload: newMethod === 'GET' ? undefined : JSON.stringify({ tenantKey: 'enterprise_preview' }, null, 2),
      assertions: [
        { id: 'as-new-1', property: 'STATUS_CODE', operator: 'EQUALS', targetValue: '200' },
        { id: 'as-new-2', property: 'RESPONSE_TIME', operator: 'LESS_THAN', targetValue: '500' },
      ],
    };
    addApiTest(newTest);
    setIsNewEndpointOpen(false);
  };

  const getMethodColor = (m: string) => {
    switch (m) {
      case 'GET':
        return 'text-emerald-400 bg-emerald-950/80 border-emerald-800';
      case 'POST':
        return 'text-cyan-400 bg-cyan-950/80 border-cyan-800';
      case 'PUT':
      case 'PATCH':
        return 'text-amber-400 bg-amber-950/80 border-amber-800';
      case 'DELETE':
        return 'text-rose-400 bg-rose-950/80 border-rose-800';
      default:
        return 'text-slate-300 bg-slate-900 border-slate-800';
    }
  };

  return (
    <div className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="API Testing & Protocol Adapters"
        subtitle="Multi-protocol test execution harness across REST, GraphQL, gRPC, and WebSocket interfaces"
        badge={activeTest ? activeTest.adapter : 'API ADAPTER'}
        icon={Network}
        actions={
          <div className="flex items-center gap-2">
            <button
              onClick={() => setIsNewEndpointOpen(true)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 text-xs font-medium border border-slate-800 cursor-pointer"
            >
              <Plus className="w-3.5 h-3.5" />
              <span>New Endpoint Test</span>
            </button>
            {activeTest && (
              <button
                onClick={() => executeApiTest(activeTest.id)}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shadow-sm shadow-indigo-950/60 cursor-pointer"
              >
                <Play className="w-3.5 h-3.5 fill-current" />
                <span>Execute Endpoint</span>
              </button>
            )}
          </div>
        }
      />

      {/* Protocol Adapter Badges */}
      <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-2.5 flex items-center justify-between gap-4 overflow-x-auto text-xs">
        <div className="flex items-center gap-1.5 font-mono">
          <span className="text-slate-500 uppercase text-[10px] font-semibold">Supported Adapters:</span>
          {(['REST_HTTP', 'GRAPHQL', 'GRPC', 'WEBSOCKET'] as APIAdapterType[]).map((ad) => (
            <span
              key={ad}
              className={`px-2 py-0.5 rounded border text-[10px] font-bold ${
                activeTest?.adapter === ad
                  ? 'bg-indigo-950 border-indigo-700 text-indigo-300'
                  : 'bg-[#020617] border-slate-800 text-slate-400'
              }`}
            >
              {ad}
            </span>
          ))}
        </div>
        <span className="text-[10px] font-mono text-slate-500 shrink-0">
          Target: <strong className="text-slate-300">{currentEnvironment}</strong>
        </span>
      </div>

      {/* Main API Console Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-4">
        {/* Left: Endpoint List (4 Cols) */}
        <div className="lg:col-span-4 rounded-lg border border-slate-800 bg-[#0B1120] p-3 space-y-2.5">
          <div className="flex items-center justify-between pb-2 border-b border-slate-800">
            <h3 className="text-[11px] font-bold uppercase tracking-wider text-slate-400 font-mono">
              Endpoint Test Catalog ({apiTests.length})
            </h3>
            <button
              onClick={() => setIsNewEndpointOpen(true)}
              className="text-indigo-400 hover:text-indigo-300 text-xs font-mono flex items-center gap-1 cursor-pointer"
            >
              <Plus className="w-3 h-3" />
              <span>Add</span>
            </button>
          </div>

          <div className="space-y-1.5">
            {apiTests.map((t) => {
              const isSelected = t.id === activeTest?.id;
              return (
                <div
                  key={t.id}
                  onClick={() => setActiveApiTestId(t.id)}
                  className={`p-2.5 rounded-md border transition-all cursor-pointer space-y-1 ${
                    isSelected
                      ? 'bg-[#020617] border-indigo-700 ring-1 ring-indigo-900/40'
                      : 'bg-[#020617] border-slate-800/80 hover:border-slate-700'
                  }`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className={`px-1.5 py-0.2 rounded font-mono text-[9px] font-bold border ${getMethodColor(t.method)}`}>
                      {t.method}
                    </span>
                    {t.lastExecution && (
                      <StatusBadge status={t.lastExecution.status} size="sm" />
                    )}
                  </div>
                  <div className="font-semibold text-xs text-slate-200 truncate font-sans">
                    {t.name}
                  </div>
                  <div className="font-mono text-[10px] text-slate-500 truncate">
                    {t.endpointUrl}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Right: Request & Response Inspector (8 Cols) */}
        <div className="lg:col-span-8 space-y-3">
          {activeTest ? (
            <>
              {/* Endpoint Request Bar */}
              <div className="p-2 rounded-lg border border-slate-800 bg-[#0B1120] flex items-center gap-2">
                <span className={`px-2 py-0.5 rounded font-mono text-[10px] font-bold border ${getMethodColor(activeTest.method)}`}>
                  {activeTest.method}
                </span>
                <input
                  type="text"
                  readOnly
                  value={activeTest.endpointUrl}
                  className="flex-1 px-2.5 py-1 rounded bg-[#020617] border border-slate-800 text-xs text-slate-200 font-mono focus:outline-none"
                />
                <button
                  onClick={() => executeApiTest(activeTest.id)}
                  className="px-3 py-1 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs flex items-center gap-1 cursor-pointer shadow-sm shadow-indigo-950/60"
                >
                  <Send className="w-3 h-3 fill-current" />
                  <span>Send</span>
                </button>
              </div>

              {/* Request Configuration Tabs */}
              <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-3.5 space-y-3">
                <div className="flex border-b border-slate-800 gap-4 text-xs font-mono">
                  {(['headers', 'params', 'auth', 'body', 'assertions'] as const).map((tab) => (
                    <button
                      key={tab}
                      onClick={() => setActiveTab(tab)}
                      className={`pb-1.5 uppercase font-bold cursor-pointer text-[10px] ${
                        activeTab === tab
                          ? 'text-indigo-400 border-b-2 border-indigo-400'
                          : 'text-slate-500 hover:text-slate-300'
                      }`}
                    >
                      {tab}
                    </button>
                  ))}
                </div>

                {/* Tab content */}
                {activeTab === 'headers' && (
                  <div className="space-y-1.5 text-xs font-mono">
                    {Object.entries(activeTest.headers).map(([k, v]) => (
                      <div key={k} className="p-2 rounded bg-[#020617] border border-slate-800 flex justify-between text-[11px]">
                        <span className="text-slate-400">{k}:</span>
                        <span className="text-slate-200">{v}</span>
                      </div>
                    ))}
                  </div>
                )}

                {activeTab === 'params' && (
                  <div className="space-y-1.5 text-xs font-mono">
                    {Object.keys(activeTest.queryParams).length === 0 ? (
                      <div className="text-slate-500 py-2 text-[11px]">No query parameters defined.</div>
                    ) : (
                      Object.entries(activeTest.queryParams).map(([k, v]) => (
                        <div key={k} className="p-2 rounded bg-[#020617] border border-slate-800 flex justify-between text-[11px]">
                          <span className="text-slate-400">{k}:</span>
                          <span className="text-slate-200">{v}</span>
                        </div>
                      ))
                    )}
                  </div>
                )}

                {activeTab === 'auth' && (
                  <div className="p-2.5 rounded bg-[#020617] border border-slate-800 text-xs font-mono space-y-1">
                    <div className="text-slate-400 text-[11px]">Authentication Type: <strong className="text-indigo-400">{activeTest.authType}</strong></div>
                    <div className="text-slate-500 text-[10px]">Injects authorization bearer tokens automatically from vault secrets.</div>
                  </div>
                )}

                {activeTab === 'body' && (
                  <div className="space-y-1">
                    <pre className="p-2.5 rounded bg-[#020617] border border-slate-800 font-mono text-[10px] text-slate-300 overflow-x-auto">
                      {activeTest.bodyPayload || '// No request body'}
                    </pre>
                  </div>
                )}

                {activeTab === 'assertions' && (
                  <div className="space-y-1.5">
                    {activeTest.assertions.map((as) => (
                      <div
                        key={as.id}
                        className="p-2 rounded bg-[#020617] border border-slate-800 flex items-center justify-between text-xs font-mono"
                      >
                        <div className="flex items-center gap-2">
                          <span className="text-indigo-400 font-bold text-[10px]">{as.property}</span>
                          <span className="text-slate-500 text-[10px]">{as.operator}</span>
                          <span className="text-slate-200 font-semibold text-[10px]">{as.targetValue}</span>
                        </div>
                        {as.passed !== undefined && (
                          <span
                            className={`px-1.5 py-0.2 rounded text-[9px] font-bold ${
                              as.passed
                                ? 'bg-emerald-950 text-emerald-300 border border-emerald-800'
                                : 'bg-rose-950 text-rose-300 border border-rose-800'
                            }`}
                          >
                            {as.passed ? 'PASSED' : 'FAILED'}
                          </span>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {/* Response Viewer */}
              {activeTest.lastExecution && (
                <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-3.5 space-y-2.5">
                  <div className="flex items-center justify-between pb-2 border-b border-slate-800">
                    <div className="flex items-center gap-2 font-mono text-xs">
                      <span className="font-bold text-slate-300 uppercase text-[10px]">Response Execution</span>
                      <span className="px-1.5 py-0.2 rounded bg-emerald-950 text-emerald-400 border border-emerald-800 font-bold text-[10px]">
                        {activeTest.lastExecution.statusCode || 200} OK
                      </span>
                      <span className="text-slate-400 text-[10px]">
                        {activeTest.lastExecution.responseTimeMs || 45}ms
                      </span>
                    </div>
                    <StatusBadge status={activeTest.lastExecution.status} size="sm" />
                  </div>

                  <pre className="p-2.5 rounded bg-[#020617] border border-slate-800 font-mono text-[10px] text-slate-300 overflow-x-auto max-h-56">
                    {activeTest.lastExecution.responseBody}
                  </pre>
                </div>
              )}
            </>
          ) : (
            <EmptyState
              icon={Network}
              title="No Endpoint Selected"
              description="Choose an API test item or configure a new endpoint request."
            />
          )}
        </div>
      </div>

      {/* New Endpoint Modal */}
      {isNewEndpointOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4 animate-in fade-in duration-100">
          <div
            className="w-full max-w-md rounded-lg bg-[#0B1120] border border-slate-800 shadow-2xl p-4 space-y-3.5 text-xs"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between pb-2.5 border-b border-slate-800">
              <h3 className="text-xs font-bold text-slate-100 uppercase font-mono">Configure API Endpoint Test</h3>
              <button
                onClick={() => setIsNewEndpointOpen(false)}
                className="text-slate-400 hover:text-slate-200 cursor-pointer"
              >
                ✕
              </button>
            </div>

            <form onSubmit={handleCreateTest} className="space-y-3">
              <div>
                <label className="block text-slate-300 font-medium mb-1">Test Name</label>
                <input
                  type="text"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none text-xs"
                  required
                />
              </div>

              <div className="grid grid-cols-2 gap-2.5">
                <div>
                  <label className="block text-slate-300 font-medium mb-1">Adapter</label>
                  <select
                    value={newAdapter}
                    onChange={(e) => setNewAdapter(e.target.value as any)}
                    className="w-full px-2 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                  >
                    <option value="REST_HTTP">REST HTTP</option>
                    <option value="GRAPHQL">GraphQL</option>
                    <option value="GRPC">gRPC</option>
                    <option value="WEBSOCKET">WebSocket</option>
                  </select>
                </div>

                <div>
                  <label className="block text-slate-300 font-medium mb-1">HTTP Method</label>
                  <select
                    value={newMethod}
                    onChange={(e) => setNewMethod(e.target.value as any)}
                    className="w-full px-2 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                  >
                    <option value="GET">GET</option>
                    <option value="POST">POST</option>
                    <option value="PUT">PUT</option>
                    <option value="PATCH">PATCH</option>
                    <option value="DELETE">DELETE</option>
                  </select>
                </div>
              </div>

              <div>
                <label className="block text-slate-300 font-medium mb-1">Endpoint URL</label>
                <input
                  type="url"
                  value={newUrl}
                  onChange={(e) => setNewUrl(e.target.value)}
                  className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                  required
                />
              </div>

              <div className="pt-2.5 border-t border-slate-800 flex justify-end gap-2">
                <button
                  type="button"
                  onClick={() => setIsNewEndpointOpen(false)}
                  className="px-3 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-300 hover:bg-slate-800 cursor-pointer"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="px-3.5 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold cursor-pointer shadow-sm shadow-indigo-950/60"
                >
                  Add Test
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
};
