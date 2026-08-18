/**
 * FE-5 — the one reusable, real project-scope control. Consumes
 * `ProjectContext` (never a second fetch/selection mechanism) and, for the
 * "why is nothing selectable" states, `AuthContext` directly — the same
 * split `ProjectContext.tsx` itself documents: auth-level recovery
 * (offline/401/...) is `AuthContext.validate()`'s job, not this
 * component's.
 *
 * Visual language deliberately copies the existing `Header.tsx` "Target
 * Environments" dropdown (icon+label+chevron trigger, absolutely
 * positioned panel, `onMouseLeave`-to-close) rather than inventing a new
 * pattern — this sprint's brief §9: "avoid introducing a new design
 * language". Every real project name comes from `ProjectContext.projects`;
 * nothing here is a placeholder/demo entry.
 */

import React, { useEffect, useRef, useState } from 'react';
import { AlertTriangle, Check, ChevronDown, FolderOpen, FolderX, Loader2, RefreshCw, WifiOff } from 'lucide-react';
import { useProject } from '../../context/ProjectContext';
import { useAuth } from '../../context/AuthContext';

export const ProjectSelector: React.FC = () => {
  const auth = useAuth();
  const { projects, selectedProject, selectedProjectId, loading, error, selectProject, refreshProjects } = useProject();
  const [isOpen, setIsOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setIsOpen(false);
    };
    const onClickOutside = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setIsOpen(false);
    };
    document.addEventListener('keydown', onKeyDown);
    document.addEventListener('mousedown', onClickOutside);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('mousedown', onClickOutside);
    };
  }, [isOpen]);

  // Not authenticated this session: no real project request has been (or
  // will be) made (brief §5) — the control is present (so the header's
  // layout doesn't jump) but explicitly says so rather than showing an
  // empty/misleading dropdown.
  if (auth.status !== 'authenticated' && projects.length === 0) {
    return (
      <div
        className="hidden lg:flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-[#020617] border border-slate-800/80 text-xs text-slate-500 select-none"
        title="Configure and verify an orchestrator API key to load projects"
      >
        <FolderOpen className="w-3.5 h-3.5 text-slate-600" />
        <span>No project scope</span>
      </div>
    );
  }

  const triggerLabel = loading
    ? 'Loading projects…'
    : error
      ? 'Project error'
      : selectedProject
        ? selectedProject.name
        : projects.length === 0
          ? 'No accessible projects'
          : 'Select a project';

  return (
    <div className="relative" ref={rootRef}>
      <button
        type="button"
        id="project-selector-button"
        aria-haspopup="listbox"
        aria-expanded={isOpen}
        onClick={() => setIsOpen((v) => !v)}
        className="flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-[#020617] hover:bg-slate-900 border border-slate-800 text-xs text-slate-200 transition-colors cursor-pointer max-w-[220px]"
      >
        {loading ? (
          <Loader2 className="w-3.5 h-3.5 text-indigo-400 animate-spin shrink-0" />
        ) : error ? (
          <AlertTriangle className="w-3.5 h-3.5 text-rose-400 shrink-0" />
        ) : (
          <FolderOpen className="w-3.5 h-3.5 text-slate-400 shrink-0" />
        )}
        <span className="font-medium truncate">{triggerLabel}</span>
        <ChevronDown className="w-3 h-3 text-slate-500 ml-0.5 shrink-0" />
      </button>

      {isOpen && (
        <div
          role="listbox"
          aria-label="Projects"
          className="absolute left-0 mt-1.5 w-72 rounded-lg bg-[#0B1120] border border-slate-700/80 shadow-2xl p-1.5 z-50 text-xs"
        >
          <div className="px-2.5 py-1 flex items-center justify-between">
            <span className="text-[10px] font-mono text-slate-400 uppercase tracking-wider font-semibold">Projects</span>
            {auth.status === 'authenticated' && (
              <button
                type="button"
                onClick={() => void refreshProjects()}
                className="flex items-center gap-1 text-[10px] text-slate-400 hover:text-slate-200 cursor-pointer"
                title="Refresh project list"
              >
                <RefreshCw className={`w-3 h-3 ${loading ? 'animate-spin' : ''}`} />
                Refresh
              </button>
            )}
          </div>

          {auth.status !== 'authenticated' && (
            <div className="px-2.5 py-3 flex items-start gap-2 text-slate-400">
              <WifiOff className="w-3.5 h-3.5 shrink-0 mt-0.5" />
              <span>
                Showing the last known project scope — the orchestrator connection is not currently verified, so it
                can&apos;t be refreshed right now.
              </span>
            </div>
          )}

          {error && (
            <div className="mx-1 my-1 px-2.5 py-2 rounded-md bg-rose-950/40 border border-rose-900/60 text-rose-300 space-y-1.5">
              <div className="flex items-center gap-1.5 font-semibold">
                <AlertTriangle className="w-3.5 h-3.5" />
                Couldn&apos;t load projects
              </div>
              <p className="text-[11px] text-rose-300/90">{error.message}</p>
              {auth.status === 'authenticated' && (
                <button
                  type="button"
                  onClick={() => void refreshProjects()}
                  className="text-[11px] font-semibold text-rose-200 hover:text-white underline cursor-pointer"
                >
                  Retry
                </button>
              )}
            </div>
          )}

          {!error && !loading && projects.length === 0 && (
            <div className="px-2.5 py-4 flex flex-col items-center text-center gap-1.5 text-slate-400">
              <FolderX className="w-5 h-5 text-slate-600" />
              <span className="font-medium text-slate-300">No accessible projects</span>
              <span className="text-[11px] max-w-[200px]">
                This API key has no projects assigned to it on the orchestrator.
              </span>
            </div>
          )}

          {!error &&
            projects.map((p) => (
              <button
                key={p.id}
                type="button"
                role="option"
                aria-selected={p.id === selectedProjectId}
                onClick={() => {
                  selectProject(p.id);
                  setIsOpen(false);
                }}
                className={`w-full flex items-center justify-between gap-2 px-2.5 py-1.5 rounded-md text-left transition-colors cursor-pointer ${
                  p.id === selectedProjectId
                    ? 'bg-slate-800/90 text-slate-100 font-semibold'
                    : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/50'
                }`}
              >
                <span className="truncate">{p.name}</span>
                {p.id === selectedProjectId && <Check className="w-3.5 h-3.5 text-indigo-400 shrink-0" />}
              </button>
            ))}
        </div>
      )}
    </div>
  );
};
