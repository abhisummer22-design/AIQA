/**
 * RunTriggerModal Component
 * Launches real test execution runs with environment, executor, and concurrency parameters
 */

import React, { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  X,
  Play,
  Layers,
  Cpu,
  Sliders,
  ShieldCheck,
  CheckCircle2
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { Environment, ExecutionRun } from '../../types';

export const RunTriggerModal: React.FC = () => {
  const {
    isNewRunModalOpen,
    setNewRunModalOpen,
    currentEnvironment,
    triggerNewRun,
  } = useAIQA();
  const navigate = useNavigate();

  const [title, setTitle] = useState('Manual QA Verification Suite');
  const [environment, setEnvironment] = useState<Environment>(currentEnvironment);
  const [executor, setExecutor] = useState<ExecutionRun['executor']>('k8s-runner-pool-us');
  const [concurrency, setConcurrency] = useState<number>(8);
  const [suites, setSuites] = useState({
    auth: true,
    payments: true,
    rbac: true,
    apiGateway: true,
    visualRegression: false,
  });

  if (!isNewRunModalOpen) return null;

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const run = triggerNewRun({
      title,
      environment,
      executor,
      concurrency,
    });
    setNewRunModalOpen(false);
    navigate('/execution');
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4 animate-in fade-in duration-100">
      <div
        className="w-full max-w-lg rounded-lg bg-[#0B1120] border border-slate-700/80 shadow-2xl overflow-hidden flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-slate-800 bg-[#020617]">
          <div className="flex items-center gap-2">
            <div className="p-1.5 rounded-md bg-indigo-950/80 border border-indigo-800/60 text-indigo-400">
              <Play className="w-3.5 h-3.5" />
            </div>
            <div>
              <h2 className="text-xs font-bold text-slate-100 font-sans">Launch Execution Run</h2>
              <p className="text-[11px] text-slate-400">Dispatch test workloads to AIQA runner cluster</p>
            </div>
          </div>
          <button
            onClick={() => setNewRunModalOpen(false)}
            className="p-1 text-slate-400 hover:text-slate-200 rounded cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Form Content */}
        <form onSubmit={handleSubmit} className="p-4 space-y-3.5 text-xs">
          {/* Run Title */}
          <div>
            <label className="block text-slate-300 font-medium mb-1">Run Title & Context</label>
            <input
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              className="w-full px-3 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-100 focus:border-indigo-500 focus:outline-none font-sans text-xs"
              required
            />
          </div>

          {/* Target Environment */}
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-slate-300 font-medium mb-1">Target Environment</label>
              <select
                value={environment}
                onChange={(e) => setEnvironment(e.target.value as Environment)}
                className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none text-xs font-mono"
              >
                <option value="staging">Staging Cluster</option>
                <option value="production">Production Tier</option>
                <option value="uat">UAT Acceptance</option>
                <option value="ephemeral-pr">Ephemeral PR-1482</option>
              </select>
            </div>

            <div>
              <label className="block text-slate-300 font-medium mb-1">Runner Executor</label>
              <select
                value={executor}
                onChange={(e) => setExecutor(e.target.value as any)}
                className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-[11px]"
              >
                <option value="k8s-runner-pool-us">k8s-runner-pool-us</option>
                <option value="docker-local-agent">docker-local-agent</option>
                <option value="cloud-playwright-grid">cloud-playwright-grid</option>
                <option value="ephemeral-runner">ephemeral-runner</option>
              </select>
            </div>
          </div>

          {/* Concurrency Level */}
          <div>
            <div className="flex justify-between text-slate-300 font-medium mb-1">
              <span>Concurrency Slots</span>
              <span className="font-mono text-indigo-400">{concurrency} Parallel Workers</span>
            </div>
            <input
              type="range"
              min="1"
              max="24"
              value={concurrency}
              onChange={(e) => setConcurrency(parseInt(e.target.value))}
              className="w-full accent-indigo-500"
            />
          </div>

          {/* Test Suites Included */}
          <div>
            <label className="block text-slate-300 font-medium mb-1.5">Include Target Test Suites</label>
            <div className="space-y-1">
              {[
                { key: 'auth', label: 'Identity & Access OAuth PKCE Suite (6 tests)' },
                { key: 'payments', label: 'Payment Gateway & Stripe 3D Secure (8 tests)' },
                { key: 'rbac', label: 'RBAC Multi-Tenant Privilege Matrix (5 tests)' },
                { key: 'apiGateway', label: 'GraphQL / REST Gateway Schema Assertions (4 tests)' },
              ].map((s) => (
                <label
                  key={s.key}
                  className="flex items-center gap-2 px-2.5 py-1.5 rounded bg-[#020617] border border-slate-800 text-slate-300 cursor-pointer hover:bg-slate-900"
                >
                  <input
                    type="checkbox"
                    checked={(suites as any)[s.key]}
                    onChange={(e) =>
                      setSuites({ ...suites, [s.key]: e.target.checked })
                    }
                    className="accent-indigo-500 rounded"
                  />
                  <span>{s.label}</span>
                </label>
              ))}
            </div>
          </div>

          {/* Footer Actions */}
          <div className="pt-3 border-t border-slate-800 flex items-center justify-end gap-2">
            <button
              type="button"
              onClick={() => setNewRunModalOpen(false)}
              className="px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-300 font-medium transition-colors cursor-pointer"
            >
              Cancel
            </button>
            <button
              type="submit"
              className="px-4 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold transition-all shadow-sm shadow-indigo-950/60 cursor-pointer flex items-center gap-1.5"
            >
              <Play className="w-3.5 h-3.5 fill-current" />
              <span>Start Execution</span>
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
