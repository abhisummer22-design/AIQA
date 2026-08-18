/**
 * Global Command Palette (Cmd+K)
 * Fast navigation across the 8 AIQA workspaces and quick control plane operations
 */

import React, { useState, useEffect } from 'react';
import {
  Search,
  LayoutDashboard,
  Sparkles,
  Workflow,
  CheckCheck,
  PlayCircle,
  ShieldCheck,
  Network,
  Smartphone,
  Sliders,
  Layers,
  FileText,
  Settings,
  Plus,
  X,
  ArrowRight,
  Terminal,
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { NavPage, Environment } from '../../types';

export const CommandPalette: React.FC = () => {
  const {
    isCommandPaletteOpen,
    setCommandPaletteOpen,
    setActivePage,
    setCurrentEnvironment,
    setNewRunModalOpen,
  } = useAIQA();

  const [search, setSearch] = useState('');

  useEffect(() => {
    if (isCommandPaletteOpen) {
      setSearch('');
    }
  }, [isCommandPaletteOpen]);

  if (!isCommandPaletteOpen) return null;

  const actions = [
    {
      category: 'Workspaces',
      items: [
        { id: 'dashboard', label: 'Go to Dashboard', icon: LayoutDashboard, action: () => setActivePage('dashboard') },
        { id: 'generator', label: 'Go to Test Case Generator', icon: Sparkles, action: () => setActivePage('generator') },
        { id: 'orchestrator', label: 'Go to AIQA Orchestrator', icon: Workflow, action: () => setActivePage('orchestrator') },
        { id: 'validator', label: 'Go to Test Case Validator', icon: CheckCheck, action: () => setActivePage('validator') },
        { id: 'execution', label: 'Go to Test Execution Hub', icon: PlayCircle, action: () => setActivePage('execution') },
        { id: 'rbac', label: 'Go to RBAC Testing Matrix', icon: ShieldCheck, action: () => setActivePage('rbac') },
        { id: 'api-testing', label: 'Go to API Testing Console', icon: Network, action: () => setActivePage('api-testing') },
        { id: 'mobile-testing', label: 'Go to Mobile Testing Hub', icon: Smartphone, action: () => setActivePage('mobile-testing') },
        { id: 'custom', label: 'Go to Custom Testing Suites', icon: Sliders, action: () => setActivePage('custom') },
        { id: 'feature-map', label: 'Go to Feature Map / Flow', icon: Layers, action: () => setActivePage('feature-map') },
        { id: 'reports', label: 'Go to Quality & Defect Reports', icon: FileText, action: () => setActivePage('reports') },
        { id: 'settings', label: 'Go to Settings & AI Model', icon: Settings, action: () => setActivePage('settings') },
      ],
    },
    {
      category: 'Quick Actions',
      items: [
        {
          id: 'new-run',
          label: 'Launch New Execution Run...',
          icon: Plus,
          action: () => {
            setNewRunModalOpen(true);
            setCommandPaletteOpen(false);
          },
        },
        {
          id: 'switch-env-staging',
          label: 'Switch Environment to Staging Cluster',
          icon: Layers,
          action: () => setCurrentEnvironment('staging'),
        },
        {
          id: 'switch-env-prod',
          label: 'Switch Environment to Production Tier',
          icon: Layers,
          action: () => setCurrentEnvironment('production'),
        },
      ],
    },
  ];

  const filtered = actions
    .map((group) => ({
      ...group,
      items: group.items.filter((item) =>
        item.label.toLowerCase().includes(search.toLowerCase())
      ),
    }))
    .filter((group) => group.items.length > 0);

  const handleSelect = (actionFn: () => void) => {
    actionFn();
    setCommandPaletteOpen(false);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-20 bg-slate-950/80 backdrop-blur-sm p-4 animate-in fade-in duration-100">
      <div
        className="w-full max-w-xl rounded-lg bg-[#0B1120] border border-slate-700/80 shadow-2xl overflow-hidden flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Search Input Bar */}
        <div className="flex items-center gap-3 px-4 py-2.5 border-b border-slate-800 bg-[#020617]">
          <Search className="w-4 h-4 text-indigo-400 shrink-0" />
          <input
            autoFocus
            type="text"
            placeholder="Type a command or search workspace..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="flex-1 bg-transparent border-none outline-none text-xs text-slate-100 placeholder-slate-500 font-sans"
          />
          <button
            onClick={() => setCommandPaletteOpen(false)}
            className="p-1 text-slate-500 hover:text-slate-300 rounded cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Results List */}
        <div className="max-h-80 overflow-y-auto p-2 space-y-2">
          {filtered.length === 0 ? (
            <div className="py-6 text-center text-xs text-slate-500 font-mono">
              No matching commands found for "{search}"
            </div>
          ) : (
            filtered.map((group) => (
              <div key={group.category} className="space-y-0.5">
                <div className="px-2 py-1 text-[10px] font-mono uppercase tracking-wider text-slate-500 font-semibold">
                  {group.category}
                </div>
                {group.items.map((item) => {
                  const Icon = item.icon;
                  return (
                    <button
                      key={item.id}
                      onClick={() => handleSelect(item.action)}
                      className="w-full flex items-center justify-between px-3 py-1.5 rounded-md text-xs text-slate-300 hover:text-white hover:bg-indigo-600 transition-colors group cursor-pointer"
                    >
                      <div className="flex items-center gap-2.5">
                        <Icon className="w-3.5 h-3.5 text-slate-400 group-hover:text-white" />
                        <span>{item.label}</span>
                      </div>
                      <ArrowRight className="w-3.5 h-3.5 text-slate-600 group-hover:text-white opacity-0 group-hover:opacity-100 transition-opacity" />
                    </button>
                  );
                })}
              </div>
            ))
          )}
        </div>

        {/* Footer shortcuts */}
        <div className="px-4 py-2 bg-[#020617] border-t border-slate-800 text-[10px] font-mono text-slate-500 flex justify-between">
          <span>Press ESC to close</span>
          <span>Use ⌘K anytime to open</span>
        </div>
      </div>
    </div>
  );
};
