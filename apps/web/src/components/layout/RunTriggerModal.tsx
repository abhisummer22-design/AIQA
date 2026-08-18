/**
 * RunTriggerModal Component
 * Launches real test execution runs via the Rust orchestrator `POST /runs`.
 * Provides both a structured UI builder and a raw JSON DAG plan editor.
 */

import React, { useState } from 'react';
import {
  X,
  Play,
  Layers,
  Cpu,
  Sliders,
  ShieldCheck,
  Code2,
  ListTree,
  ShieldAlert,
  Terminal,
} from 'lucide-react';
import { useApiAction } from '../../hooks/useApi';
import { createRun } from '../../services/orchestrator';
import type { RunPlan, TaskDef } from '../../types/orchestrator';

interface RunTriggerModalProps {
  isOpen: boolean;
  onClose: () => void;
  onRunCreated: (runId: string) => void;
}

export const RunTriggerModal: React.FC<RunTriggerModalProps> = ({
  isOpen,
  onClose,
  onRunCreated,
}) => {
  const [activeTab, setActiveTab] = useState<'guided' | 'json'>('guided');
  const [name, setName] = useState('regression-suite');
  const [concurrency, setConcurrency] = useState<number>(4);
  const [projectId, setProjectId] = useState<string>('');

  // Guided suite selections
  const [selectedSuites, setSelectedSuites] = useState({
    smoke: true,
    apiContract: true,
    securityFuzz: false,
    mobileFlow: false,
  });

  // Raw JSON state
  const [jsonText, setJsonText] = useState<string>(
    JSON.stringify(
      [
        { id: "t1", name: "smoke", executor: "playwright", params: {} },
        { id: "t2", name: "api-contract", prerequisites: ["t1"], executor: "schemathesis", params: {} },
      ],
      null,
      2
    )
  );
  const [parseError, setParseError] = useState<string | null>(null);

  const create = useApiAction((plan: RunPlan, projId?: string) =>
    createRun(plan, projId || undefined)
  );

  if (!isOpen) return null;

  const buildTasksFromGuided = (): TaskDef[] => {
    const tasks: TaskDef[] = [];

    if (selectedSuites.smoke) {
      tasks.push({
        id: 'task-smoke',
        name: 'Smoke: Core Health & Ingress',
        executor: 'playwright',
        params: { test_pattern: 'e2e/smoke/**' },
      });
    }

    if (selectedSuites.apiContract) {
      tasks.push({
        id: 'task-api-contract',
        name: 'API: OpenAPI Spec & Contract Assertions',
        prerequisites: selectedSuites.smoke ? ['task-smoke'] : [],
        executor: 'schemathesis',
        params: { spec: 'openapi.json' },
      });
    }

    if (selectedSuites.securityFuzz) {
      tasks.push({
        id: 'task-sec-fuzz',
        name: 'Security: Auth & Token Revocation Fuzzing',
        prerequisites: selectedSuites.apiContract ? ['task-api-contract'] : [],
        executor: 'newman',
        params: { collection: 'security-matrix.json' },
      });
    }

    if (selectedSuites.mobileFlow) {
      tasks.push({
        id: 'task-mobile',
        name: 'Mobile: Critical User Journey Flows',
        prerequisites: selectedSuites.smoke ? ['task-smoke'] : [],
        executor: 'maestro',
        params: { flow: 'flows/login-checkout.yaml' },
      });
    }

    // Default fallback task if nothing selected
    if (tasks.length === 0) {
      tasks.push({
        id: 'task-sim-smoke',
        name: 'Smoke: Automated Pipeline Validation',
        params: {},
      });
    }

    return tasks;
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setParseError(null);

    let tasks: TaskDef[];

    if (activeTab === 'guided') {
      tasks = buildTasksFromGuided();
    } else {
      try {
        tasks = JSON.parse(jsonText);
        if (!Array.isArray(tasks)) throw new Error('Tasks must be a valid JSON array of TaskDef');
      } catch (err) {
        setParseError(err instanceof Error ? err.message : 'Invalid JSON');
        return;
      }
    }

    const plan: RunPlan = {
      name: name.trim() || 'unnamed-run',
      tasks,
      concurrency,
    };

    const result = await create.run(plan, projectId.trim() || undefined);
    if (result && result.run_id) {
      onRunCreated(result.run_id);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4 animate-in fade-in duration-100"
      onClick={onClose}
    >
      <div
        className="w-full max-w-xl rounded-xl bg-[#0B1120] border border-slate-700/80 shadow-2xl overflow-hidden flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-slate-800 bg-[#020617]">
          <div className="flex items-center gap-2">
            <div className="p-1.5 rounded-md bg-cyan-950/80 border border-cyan-800/60 text-cyan-400">
              <Play className="w-3.5 h-3.5 fill-current" />
            </div>
            <div>
              <h2 className="text-xs font-bold text-slate-100 font-sans">Launch Execution Run</h2>
              <p className="text-[10px] text-slate-400 font-mono">Dispatches RunPlan to Rust Orchestrator (:8080)</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1 text-slate-400 hover:text-slate-200 rounded cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Tab Switcher */}
        <div className="flex border-b border-slate-800 bg-[#060a12] px-4">
          <button
            type="button"
            onClick={() => setActiveTab('guided')}
            className={`flex items-center gap-1.5 px-3 py-2 text-xs font-medium border-b-2 transition-colors cursor-pointer ${
              activeTab === 'guided'
                ? 'border-cyan-400 text-cyan-300'
                : 'border-transparent text-slate-500 hover:text-slate-300'
            }`}
          >
            <ListTree size={12} />
            <span>Guided Builder</span>
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('json')}
            className={`flex items-center gap-1.5 px-3 py-2 text-xs font-medium border-b-2 transition-colors cursor-pointer ${
              activeTab === 'json'
                ? 'border-cyan-400 text-cyan-300'
                : 'border-transparent text-slate-500 hover:text-slate-300'
            }`}
          >
            <Code2 size={12} />
            <span>Raw DAG JSON</span>
          </button>
        </div>

        {/* Form Content */}
        <form onSubmit={handleSubmit} className="p-4 space-y-3.5 text-xs">
          {/* Run Name and Project */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <div>
              <label className="block text-slate-300 font-medium mb-1 font-mono text-[10px] uppercase">
                Run Name
              </label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e2e-regression-build"
                className="w-full px-3 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-100 focus:border-cyan-500 focus:outline-none font-sans text-xs"
                required
              />
            </div>
            <div>
              <label className="block text-slate-300 font-medium mb-1 font-mono text-[10px] uppercase">
                Project Scope (Optional)
              </label>
              <input
                type="text"
                value={projectId}
                onChange={(e) => setProjectId(e.target.value)}
                placeholder="prj-aiqa-core"
                className="w-full px-3 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-100 focus:border-cyan-500 focus:outline-none font-mono text-xs"
              />
            </div>
          </div>

          {/* Concurrency Level */}
          <div>
            <div className="flex justify-between text-slate-300 font-medium mb-1">
              <span className="font-mono text-[10px] uppercase">Concurrency Limit</span>
              <span className="font-mono text-cyan-400 text-[11px]">{concurrency} Parallel Slots</span>
            </div>
            <input
              type="range"
              min="1"
              max="16"
              value={concurrency}
              onChange={(e) => setConcurrency(parseInt(e.target.value))}
              className="w-full accent-cyan-500"
            />
          </div>

          {activeTab === 'guided' ? (
            /* Guided Suite Selectors */
            <div>
              <label className="block text-slate-300 font-medium mb-1.5 font-mono text-[10px] uppercase">
                Select Execution Suites to Schedule
              </label>
              <div className="space-y-1.5">
                {[
                  {
                    key: 'smoke',
                    label: 'Playwright Web E2E: Smoke & Ingress Suite',
                    executor: 'playwright',
                  },
                  {
                    key: 'apiContract',
                    label: 'Schemathesis API: OpenAPI Spec Contract Fuzzing',
                    executor: 'schemathesis',
                  },
                  {
                    key: 'securityFuzz',
                    label: 'Newman Security: Role & Token Revocation Tests',
                    executor: 'newman',
                  },
                  {
                    key: 'mobileFlow',
                    label: 'Maestro Mobile: Core Device Verification Flows',
                    executor: 'maestro',
                  },
                ].map((s) => (
                  <label
                    key={s.key}
                    className="flex items-center justify-between px-3 py-2 rounded-lg bg-[#020617] border border-slate-800/90 text-slate-300 cursor-pointer hover:bg-slate-900/60"
                  >
                    <div className="flex items-center gap-2">
                      <input
                        type="checkbox"
                        checked={(selectedSuites as any)[s.key]}
                        onChange={(e) =>
                          setSelectedSuites({
                            ...selectedSuites,
                            [s.key]: e.target.checked,
                          })
                        }
                        className="accent-cyan-500 rounded"
                      />
                      <span className="text-xs">{s.label}</span>
                    </div>
                    <span className="font-mono text-[9px] text-slate-500 uppercase px-1.5 py-0.2 rounded bg-slate-900 border border-slate-800">
                      {s.executor}
                    </span>
                  </label>
                ))}
              </div>
            </div>
          ) : (
            /* Raw JSON Editor */
            <div>
              <label className="block text-slate-300 font-medium mb-1 font-mono text-[10px] uppercase">
                TaskDef[] Array JSON
              </label>
              <textarea
                value={jsonText}
                onChange={(e) => setJsonText(e.target.value)}
                rows={8}
                spellCheck={false}
                className="w-full rounded-md border border-slate-700 bg-[#020617] px-3 py-2 font-mono text-[11px] leading-relaxed text-slate-300 outline-none focus:border-cyan-600"
              />
            </div>
          )}

          {parseError && (
            <div className="rounded border border-rose-800/60 bg-rose-950/30 px-2.5 py-1.5 text-[11px] text-rose-300">
              {parseError}
            </div>
          )}

          {create.error && (
            <div className="flex items-start gap-2 rounded border border-rose-800/60 bg-rose-950/30 px-2.5 py-1.5 text-[11px] text-rose-300">
              <ShieldAlert size={13} className="mt-px shrink-0 text-rose-400" />
              <span>{create.error.detail || create.error.message}</span>
            </div>
          )}

          {/* Footer Actions */}
          <div className="pt-3 border-t border-slate-800 flex items-center justify-end gap-2">
            <button
              type="button"
              onClick={onClose}
              className="px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-300 font-medium transition-colors cursor-pointer"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={create.isRunning}
              className="px-4 py-1.5 rounded-md bg-cyan-600 hover:bg-cyan-500 text-white font-semibold transition-all shadow-sm shadow-cyan-950 cursor-pointer flex items-center gap-1.5 disabled:opacity-40"
            >
              <Play className="w-3.5 h-3.5 fill-current" />
              <span>{create.isRunning ? 'Submitting Plan…' : 'Submit & Execute Plan'}</span>
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
