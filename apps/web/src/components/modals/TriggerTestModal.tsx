import React, { useState } from "react";
import { X, Play, Layers, Cpu, Globe2, ShieldAlert } from "lucide-react";
import { EnvironmentType } from "../../types";

interface TriggerTestModalProps {
  isOpen: boolean;
  onClose: () => void;
  onTrigger: (data: {
    testName: string;
    project: string;
    environment: EnvironmentType;
    executor: string;
  }) => void;
}

export const TriggerTestModal: React.FC<TriggerTestModalProps> = ({
  isOpen,
  onClose,
  onTrigger
}) => {
  const [testName, setTestName] = useState("Full Security & Contract Fuzzing Suite");
  const [project, setProject] = useState("Apex AI Agent");
  const [environment, setEnvironment] = useState<EnvironmentType>("staging");
  const [executor, setExecutor] = useState("Schemathesis / OpenAPI");

  if (!isOpen) return null;

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onTrigger({
      testName,
      project,
      environment,
      executor
    });
    onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm">
      <div className="bg-[#0D0D0E] border border-slate-800 rounded w-full max-w-lg overflow-hidden shadow-2xl flex flex-col">
        {/* Header */}
        <div className="px-4 py-3 border-b border-slate-800 flex items-center justify-between bg-slate-900/40">
          <div className="flex items-center gap-2">
            <Play className="w-4 h-4 text-indigo-400 fill-current" />
            <div>
              <h3 className="text-xs font-bold font-mono uppercase text-white tracking-wider">Trigger Test Execution</h3>
              <p className="text-[10px] text-slate-400 font-mono">Launch automated test suites across distributed workers</p>
            </div>
          </div>
          <button onClick={onClose} aria-label="Close" className="p-1 rounded text-slate-400 hover:text-white">
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Form Body */}
        <form onSubmit={handleSubmit} className="p-4 space-y-3 text-xs font-mono">
          <div>
            <label className="text-slate-400 uppercase text-[9px] font-bold block mb-1">
              Test Suite / Scenario Name
            </label>
            <input
              type="text"
              value={testName}
              onChange={(e) => setTestName(e.target.value)}
              className="w-full p-2 rounded bg-[#070708] border border-slate-800 text-slate-100 text-xs font-mono focus:outline-none focus:border-indigo-500"
              required
            />
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="text-slate-400 uppercase text-[9px] font-bold block mb-1">
                Target Project
              </label>
              <select
                value={project}
                onChange={(e) => setProject(e.target.value)}
                className="w-full p-2 rounded bg-[#070708] border border-slate-800 text-slate-100 text-xs font-mono focus:outline-none focus:border-indigo-500"
              >
                <option value="Apex AI Agent">Apex AI Agent</option>
                <option value="Payment Core API">Payment Core API</option>
                <option value="Search RAG Pipeline">Search RAG Pipeline</option>
                <option value="Omni LLM Service">Omni LLM Service</option>
              </select>
            </div>

            <div>
              <label className="text-slate-400 uppercase text-[9px] font-bold block mb-1">
                Environment Target
              </label>
              <select
                value={environment}
                onChange={(e) => setEnvironment(e.target.value as EnvironmentType)}
                className="w-full p-2 rounded bg-[#070708] border border-slate-800 text-slate-100 text-xs font-mono focus:outline-none focus:border-indigo-500"
              >
                <option value="local">Local Dev Runner</option>
                <option value="development">Development Pods</option>
                <option value="staging">Staging Integration</option>
                <option value="production">Production Non-Destructive</option>
              </select>
            </div>
          </div>

          <div>
            <label className="text-slate-400 uppercase text-[9px] font-bold block mb-1">
              Test Executor Engine
            </label>
            <select
              value={executor}
              onChange={(e) => setExecutor(e.target.value)}
              className="w-full p-2 rounded bg-[#070708] border border-slate-800 text-slate-100 text-xs font-mono focus:outline-none focus:border-indigo-500"
            >
              <option value="Schemathesis / OpenAPI">Schemathesis (Contract &amp; Fuzzing)</option>
              <option value="Playwright API + PyTest">Playwright API &amp; PyTest</option>
              <option value="DeepEval / LLM-as-Judge">DeepEval (Hallucination / RAG Evals)</option>
              <option value="Promptfoo + K6 Load">Promptfoo (Prompt Injection &amp; Load)</option>
            </select>
          </div>

          {/* Buttons */}
          <div className="pt-3 border-t border-slate-800 flex items-center justify-end gap-2">
            <button
              type="button"
              onClick={onClose}
              className="px-3 py-1.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 font-mono text-[11px]"
            >
              Cancel
            </button>
            <button
              type="submit"
              className="px-3.5 py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-mono text-[11px] font-bold flex items-center gap-1.5 shadow-md shadow-indigo-950"
            >
              <Play className="w-3.5 h-3.5 fill-current" />
              <span>Launch Execution</span>
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};

