import React, { useState } from "react";
import { Settings, Cpu, ShieldCheck, Database, Key, Server, Check } from "lucide-react";

export const SettingsView: React.FC = () => {
  const [ollamaUrl, setOllamaUrl] = useState("http://localhost:11434");
  const [modelName, setModelName] = useState("gemma3:12b-instruct-q4_K_M");
  const [saved, setSaved] = useState(false);

  const handleSave = (e: React.FormEvent) => {
    e.preventDefault();
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  return (
    <div className="space-y-6 max-w-3xl">
      <div>
        <h1 className="text-xl font-bold font-mono text-white flex items-center gap-2.5">
          <Settings className="w-5 h-5 text-cyan-400" />
          AIQA Platform Configuration
        </h1>
        <p className="text-xs text-slate-400 font-mono mt-1">
          Configure local inference daemons, model routing fallbacks, test executors, and Jira issue synchronization
        </p>
      </div>

      <form onSubmit={handleSave} className="space-y-6">
        {/* Ollama Section */}
        <div className="p-5 rounded-xl bg-[#0e1422] border border-slate-800/90 space-y-4">
          <div className="flex items-center gap-2 text-cyan-400 pb-2 border-b border-slate-800">
            <Cpu className="w-4 h-4" />
            <h2 className="text-xs font-bold font-mono uppercase text-slate-200">Local AI Inference (Ollama)</h2>
          </div>

          <div className="space-y-3 text-xs font-mono">
            <div>
              <label className="text-slate-400 uppercase text-[10px] font-bold block mb-1">
                Ollama Daemon Base URL
              </label>
              <input
                type="text"
                value={ollamaUrl}
                onChange={(e) => setOllamaUrl(e.target.value)}
                className="w-full p-2.5 rounded bg-[#080d16] border border-slate-800 text-slate-100 text-xs focus:outline-none focus:border-cyan-500"
              />
            </div>

            <div>
              <label className="text-slate-400 uppercase text-[10px] font-bold block mb-1">
                Active Primary Model ID
              </label>
              <input
                type="text"
                value={modelName}
                onChange={(e) => setModelName(e.target.value)}
                className="w-full p-2.5 rounded bg-[#080d16] border border-slate-800 text-slate-100 text-xs focus:outline-none focus:border-cyan-500"
              />
            </div>
          </div>
        </div>

        {/* Fallback Section */}
        <div className="p-5 rounded-xl bg-[#0e1422] border border-slate-800/90 space-y-4">
          <div className="flex items-center gap-2 text-cyan-400 pb-2 border-b border-slate-800">
            <Server className="w-4 h-4" />
            <h2 className="text-xs font-bold font-mono uppercase text-slate-200">Cloud Fallback Dispatcher (Gemini 3.7)</h2>
          </div>

          <div className="space-y-2 text-xs font-mono text-slate-400">
            <p className="font-sans text-slate-300">
              When Ollama is offline or experiences memory pressure, the AIQA engine routes requests server-side via Gemini API without service interruption.
            </p>
            <div className="pt-2 flex items-center gap-2">
              <span className="w-2 h-2 rounded-full bg-emerald-400" />
              <span className="text-slate-200 font-semibold">Gemini Server-Side Bridge: Active &amp; Ready</span>
            </div>
          </div>
        </div>

        {/* Save button */}
        <div className="flex items-center justify-end gap-3">
          <button
            type="submit"
            className="px-5 py-2 rounded-lg bg-cyan-600 hover:bg-cyan-500 text-white text-xs font-mono font-bold flex items-center gap-2 shadow-sm"
          >
            {saved ? <Check className="w-4 h-4" /> : null}
            <span>{saved ? "Settings Saved" : "Save Configuration"}</span>
          </button>
        </div>
      </form>
    </div>
  );
};
