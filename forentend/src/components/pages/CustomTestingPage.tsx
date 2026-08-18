/**
 * AIQA Custom Testing Workspace
 * Modular harness for Visual Regression, Load & Stress, Chaos Resilience, and Synthetic E2E Journeys.
 */

import React, { useState } from 'react';
import {
  Sliders,
  Plus,
  Play,
  CheckCircle2,
  AlertTriangle,
  Zap,
  Activity,
  Layers,
  Flame,
  Camera,
  RotateCcw,
  Sparkles
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { SectionHeader } from '../common/SectionHeader';
import { StatusBadge } from '../common/StatusBadge';
import { EmptyState } from '../common/EmptyState';
import { CustomTestSuite } from '../../types';

export const CustomTestingPage: React.FC = () => {
  const {
    customSuites,
    addCustomSuite,
    runCustomSuite,
    currentEnvironment,
  } = useAIQA();

  const [isAddSuiteOpen, setIsAddSuiteOpen] = useState(false);
  const [name, setName] = useState('Gateway 504 Gateway Timeout Synthetic Retry');
  const [category, setCategory] = useState<CustomTestSuite['category']>('CHAOS_RESILIENCE');
  const [description, setDescription] = useState('Verify upstream circuit breaker falls back to cached state when latency exceeds 2000ms.');
  const [targetUrl, setTargetUrl] = useState('https://staging.enterprise.com/api/v1/gateway');
  const [concurrencyLevel, setConcurrencyLevel] = useState<number>(6);
  const [timeoutSeconds, setTimeoutSeconds] = useState<number>(120);

  const getCategoryConfig = (cat: CustomTestSuite['category']) => {
    switch (cat) {
      case 'VISUAL_REGRESSION':
        return { label: 'Visual Pixel Diff', icon: Camera, color: 'text-purple-400 border-purple-800 bg-purple-950/60' };
      case 'LOAD_AND_STRESS':
        return { label: 'Load & Stress', icon: Flame, color: 'text-amber-400 border-amber-800 bg-amber-950/60' };
      case 'CHAOS_RESILIENCE':
        return { label: 'Chaos & Fault Injection', icon: Zap, color: 'text-rose-400 border-rose-800 bg-rose-950/60' };
      case 'SYNTHETIC_E2E':
        return { label: 'Synthetic Journey', icon: Activity, color: 'text-cyan-400 border-cyan-800 bg-cyan-950/60' };
      default:
        return { label: cat, icon: Sliders, color: 'text-slate-400 border-slate-800 bg-slate-900' };
    }
  };

  const handleCreateSuite = (e: React.FormEvent) => {
    e.preventDefault();
    const suite: CustomTestSuite = {
      id: `cst-${Date.now().toString().slice(-4)}`,
      name,
      category,
      description,
      configuration: { targetUrl, concurrencyLevel },
      targetUrl,
      concurrencyLevel,
      timeoutSeconds,
      status: 'QUEUED',
    };
    addCustomSuite(suite);
    setIsAddSuiteOpen(false);
  };

  return (
    <div className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Custom Testing Workflows"
        subtitle="Modular execution harness for visual diffing, chaos resilience, stress testing, and synthetic user journeys"
        badge={`${customSuites.length} MODULES`}
        icon={Sliders}
        actions={
          <button
            onClick={() => setIsAddSuiteOpen(true)}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shadow-sm shadow-indigo-950/60 cursor-pointer"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>Create Custom Suite</span>
          </button>
        }
      />

      {/* Modular Archetype Grid */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-2.5">
        {[
          { title: 'Visual Regression', desc: 'Pixel-diff rendering across viewport resolutions and theme tokens.' },
          { title: 'Load & Stress', desc: 'Ramp concurrent virtual users to benchmark P99 response degradation.' },
          { title: 'Chaos & Resilience', desc: 'Inject pod kills, network packet latency, and DB failovers.' },
          { title: 'Synthetic E2E', desc: 'Continuous production heartbeats simulating golden checkout paths.' },
        ].map((item) => (
          <div key={item.title} className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1 text-xs">
            <div className="font-bold text-slate-200 text-xs">{item.title}</div>
            <p className="text-slate-400 text-[11px] leading-normal">{item.desc}</p>
          </div>
        ))}
      </div>

      {/* Active Suites List */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <h3 className="text-[11px] font-bold uppercase tracking-wider text-slate-400 font-mono">
            Configured Test Suites ({customSuites.length})
          </h3>
          <span className="text-[10px] font-mono text-slate-500">
            Target: <strong className="text-slate-300">{currentEnvironment}</strong>
          </span>
        </div>

        {customSuites.length === 0 ? (
          <EmptyState
            icon={Sliders}
            title="No Custom Suites Defined"
            description="Create visual regression, chaos resilience, or synthetic load testing flows."
            actionLabel="Create Custom Suite"
            onAction={() => setIsAddSuiteOpen(true)}
          />
        ) : (
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
            {customSuites.map((suite) => {
              const config = getCategoryConfig(suite.category);
              const Icon = config.icon;

              return (
                <div
                  key={suite.id}
                  className="p-3.5 rounded-lg border border-slate-800 bg-[#0B1120] space-y-3 hover:border-slate-700 transition-colors"
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="space-y-0.5">
                      <div className="flex items-center gap-1.5 flex-wrap">
                        <span className="font-mono text-xs font-bold text-indigo-400">{suite.id}</span>
                        <span className={`px-1.5 py-0.2 rounded border text-[9px] font-mono font-semibold flex items-center gap-1 ${config.color}`}>
                          <Icon className="w-2.5 h-2.5" />
                          <span>{config.label}</span>
                        </span>
                      </div>
                      <h4 className="text-xs font-bold text-slate-100 mt-1 font-sans">{suite.name}</h4>
                      <p className="text-[11px] text-slate-400">{suite.description}</p>
                    </div>
                    <StatusBadge status={suite.status} size="sm" />
                  </div>

                  <div className="p-2.5 rounded bg-[#020617] border border-slate-800/80 grid grid-cols-3 gap-2 text-[10px] font-mono text-slate-400">
                    <div>
                      <span className="text-slate-600 block text-[9px]">Concurrency</span>
                      <strong className="text-slate-200">{suite.concurrencyLevel} threads</strong>
                    </div>
                    <div>
                      <span className="text-slate-600 block text-[9px]">Timeout</span>
                      <strong className="text-slate-200">{suite.timeoutSeconds}s</strong>
                    </div>
                    <div>
                      <span className="text-slate-600 block text-[9px]">Last Executed</span>
                      <strong className="text-slate-200">
                        {suite.lastRunTime ? new Date(suite.lastRunTime).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : 'Never'}
                      </strong>
                    </div>
                  </div>

                  <div className="flex items-center justify-between pt-0.5">
                    <div className="font-mono text-[10px] text-slate-500 truncate max-w-xs">
                      Target: {suite.targetUrl}
                    </div>
                    <button
                      onClick={() => runCustomSuite(suite.id)}
                      className="px-2.5 py-1 rounded bg-[#020617] hover:bg-indigo-600 hover:text-white text-indigo-300 font-semibold text-xs border border-slate-800 transition-all flex items-center gap-1.5 cursor-pointer shadow-sm"
                    >
                      <Play className="w-3 h-3 fill-current" />
                      <span>Run Suite</span>
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Create Modal */}
      {isAddSuiteOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4 animate-in fade-in duration-100">
          <div
            className="w-full max-w-md rounded-lg bg-[#0B1120] border border-slate-800 shadow-2xl p-4 space-y-3.5 text-xs"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between pb-2.5 border-b border-slate-800">
              <h3 className="text-xs font-bold text-slate-100 uppercase font-mono">Create Custom Test Suite</h3>
              <button
                onClick={() => setIsAddSuiteOpen(false)}
                className="text-slate-400 hover:text-slate-200 cursor-pointer"
              >
                ✕
              </button>
            </div>

            <form onSubmit={handleCreateSuite} className="space-y-3">
              <div>
                <label className="block text-slate-300 font-medium mb-1">Suite Name</label>
                <input
                  type="text"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none text-xs"
                  required
                />
              </div>

              <div>
                <label className="block text-slate-300 font-medium mb-1">Testing Category</label>
                <select
                  value={category}
                  onChange={(e) => setCategory(e.target.value as any)}
                  className="w-full px-2 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                >
                  <option value="VISUAL_REGRESSION">Visual Pixel Regression</option>
                  <option value="LOAD_AND_STRESS">Load & Stress Benchmark</option>
                  <option value="CHAOS_RESILIENCE">Chaos & Fault Injection</option>
                  <option value="SYNTHETIC_E2E">Synthetic Golden Path</option>
                </select>
              </div>

              <div>
                <label className="block text-slate-300 font-medium mb-1">Description & Assertion Objectives</label>
                <textarea
                  rows={3}
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-sans text-xs"
                  required
                />
              </div>

              <div>
                <label className="block text-slate-300 font-medium mb-1">Target URL</label>
                <input
                  type="url"
                  value={targetUrl}
                  onChange={(e) => setTargetUrl(e.target.value)}
                  className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                  required
                />
              </div>

              <div className="grid grid-cols-2 gap-2.5">
                <div>
                  <label className="block text-slate-300 font-medium mb-1">Concurrency</label>
                  <input
                    type="number"
                    min="1"
                    max="32"
                    value={concurrencyLevel}
                    onChange={(e) => setConcurrencyLevel(parseInt(e.target.value))}
                    className="w-full px-2 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                  />
                </div>
                <div>
                  <label className="block text-slate-300 font-medium mb-1">Timeout (seconds)</label>
                  <input
                    type="number"
                    min="10"
                    max="600"
                    value={timeoutSeconds}
                    onChange={(e) => setTimeoutSeconds(parseInt(e.target.value))}
                    className="w-full px-2 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                  />
                </div>
              </div>

              <div className="pt-2.5 border-t border-slate-800 flex justify-end gap-2">
                <button
                  type="button"
                  onClick={() => setIsAddSuiteOpen(false)}
                  className="px-3 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-300 hover:bg-slate-800 cursor-pointer"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="px-3.5 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold cursor-pointer shadow-sm shadow-indigo-950/60"
                >
                  Create Suite
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
};
