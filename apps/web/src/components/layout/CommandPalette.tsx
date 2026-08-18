/**
 * Global Command Palette (Cmd+K / Ctrl+K)
 * Fast keyboard-first navigation across all AIQA workspaces and control plane actions.
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
  Activity,
  Bug,
  ScrollText,
  FolderKanban,
  ListChecks,
  Brain,
} from 'lucide-react';
import type { ViewId } from '../../App';

interface CommandPaletteProps {
  isOpen: boolean;
  onClose: () => void;
  onSelectView: (view: ViewId) => void;
  onOpenNewRunModal: () => void;
}

export const CommandPalette: React.FC<CommandPaletteProps> = ({
  isOpen,
  onClose,
  onSelectView,
  onOpenNewRunModal,
}) => {
  const [search, setSearch] = useState('');

  useEffect(() => {
    if (isOpen) {
      setSearch('');
    }
  }, [isOpen]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        if (isOpen) onClose();
      } else if (e.key === 'Escape' && isOpen) {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const actions = [
    {
      category: 'Live Control Plane',
      items: [
        { id: 'dashboard', label: 'Go to Dashboard', icon: LayoutDashboard, action: () => onSelectView('dashboard') },
        { id: 'runs', label: 'Go to Execution Hub / Runs', icon: PlayCircle, action: () => onSelectView('runs') },
        { id: 'intelligence', label: 'Go to Failure Intelligence', icon: Activity, action: () => onSelectView('intelligence') },
        { id: 'bug-drafts', label: 'Go to Bug Drafts / RCA', icon: Bug, action: () => onSelectView('bug-drafts') },
        { id: 'audit-log', label: 'Go to Audit Log', icon: ScrollText, action: () => onSelectView('audit-log') },
        { id: 'settings', label: 'Go to Settings & API Keys', icon: Settings, action: () => onSelectView('settings') },
      ],
    },
    {
      category: 'Quality Workspaces (Staged)',
      items: [
        { id: 'projects', label: 'Projects & Workspaces', icon: FolderKanban, action: () => onSelectView('projects') },
        { id: 'quality-graph', label: 'Requirements & Traceability Graph', icon: Layers, action: () => onSelectView('quality-graph') },
        { id: 'test-generator', label: 'AI Test Case Generator', icon: Sparkles, action: () => onSelectView('test-generator') },
        { id: 'test-validator', label: 'Test Case Validator', icon: CheckCheck, action: () => onSelectView('test-validator') },
        { id: 'test-suites', label: 'Test Suites & Execution Plans', icon: ListChecks, action: () => onSelectView('test-suites') },
        { id: 'orchestrator-plan', label: 'AIQA Orchestrator Planner', icon: Workflow, action: () => onSelectView('orchestrator-plan') },
        { id: 'rbac', label: 'RBAC Testing Matrix', icon: ShieldCheck, action: () => onSelectView('rbac') },
        { id: 'api-testing', label: 'API Testing Console', icon: Network, action: () => onSelectView('api-testing') },
        { id: 'mobile-testing', label: 'Mobile Testing Hub', icon: Smartphone, action: () => onSelectView('mobile-testing') },
        { id: 'custom-testing', label: 'Custom Testing Suites', icon: Sliders, action: () => onSelectView('custom-testing') },
        { id: 'reports', label: 'Quality & Defect Reports', icon: FileText, action: () => onSelectView('reports') },
        { id: 'model-gateway', label: 'Model Gateway & AI Telemetry', icon: Brain, action: () => onSelectView('model-gateway') },
      ],
    },
    {
      category: 'Quick Actions',
      items: [
        {
          id: 'new-run',
          label: 'Launch New Execution Run…',
          icon: Plus,
          action: () => {
            onClose();
            onOpenNewRunModal();
          },
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
    onClose();
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-20 bg-slate-950/80 backdrop-blur-sm p-4 animate-in fade-in duration-100"
      onClick={onClose}
    >
      <div
        className="w-full max-w-xl rounded-xl bg-[#0B1120] border border-slate-700/80 shadow-2xl overflow-hidden flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Search Input Bar */}
        <div className="flex items-center gap-3 px-4 py-3 border-b border-slate-800 bg-[#020617]">
          <Search className="w-4 h-4 text-cyan-400 shrink-0" />
          <input
            autoFocus
            type="text"
            placeholder="Type a command or search workspaces…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="flex-1 bg-transparent border-none outline-none text-xs text-slate-100 placeholder-slate-500 font-sans"
          />
          <button
            onClick={onClose}
            className="p-1 text-slate-500 hover:text-slate-300 rounded cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Results List */}
        <div className="max-h-80 overflow-y-auto p-2 space-y-2">
          {filtered.length === 0 ? (
            <div className="py-8 text-center text-xs text-slate-500 font-mono">
              No matching commands found for "{search}"
            </div>
          ) : (
            filtered.map((group) => (
              <div key={group.category} className="space-y-0.5">
                <div className="px-2 py-1 text-[9px] font-mono uppercase tracking-wider text-slate-500 font-semibold">
                  {group.category}
                </div>
                {group.items.map((item) => {
                  const Icon = item.icon;
                  return (
                    <button
                      key={item.id}
                      onClick={() => handleSelect(item.action)}
                      className="w-full flex items-center justify-between px-3 py-1.5 rounded-md text-xs text-slate-300 hover:text-white hover:bg-cyan-600/90 transition-colors group cursor-pointer"
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
