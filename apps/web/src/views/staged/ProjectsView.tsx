/**
 * Projects & Workspaces View (Contract §5)
 * Manages project context, repository links, environments, and quality targets.
 */

import React, { useState } from 'react';
import {
  FolderKanban,
  Plus,
  Layers,
  GitBranch,
  Search,
  ExternalLink,
  ShieldCheck,
  Activity,
  Sliders,
} from 'lucide-react';
import { SectionHeader } from '../../components/common/SectionHeader';
import { BackendNotice } from '../../components/common/BackendNotice';
import { Card } from '../../components/common/Card';
import { MetricCard } from '../../components/common/MetricCard';

export function ProjectsView() {
  const [search, setSearch] = useState('');

  return (
    <div className="space-y-4">
      <SectionHeader
        title="Projects & Workspaces"
        subtitle="Project-scoped isolation, context metadata, repositories, and test environments."
        badge="STAGED SPECIFICATION"
        badgeTone="warn"
        action={
          <button
            disabled
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-slate-800 border border-slate-700 text-slate-500 text-xs font-semibold cursor-not-allowed opacity-60"
            title="Requires POST /api/v1/projects endpoint"
          >
            <Plus size={13} />
            <span>Create Project</span>
          </button>
        }
      />

      <BackendNotice
        feature="Project Management & Isolation"
        contractSection="§5 (Project APIs)"
        endpoint="GET /api/v1/projects | POST /api/v1/projects"
        requires="The Rust orchestrator currently operates on flat routes (/runs). A projects catalogue and server-side project scoping must be added to enable multi-project isolation (ACC-001)."
        payloadExample={`// POST /api/v1/projects
{
  "name": "E-Commerce Core Gateway",
  "key": "PRJ-ECOMM",
  "description": "Payment gateway & checkout verification suite",
  "repository_url": "https://github.com/org/ecommerce-core",
  "default_environment": "staging"
}`}
      />

      {/* Metric Tiles */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <MetricCard label="Registered Projects" value="—" subtext="BACKEND API REQUIRED" icon={<FolderKanban size={14} />} />
        <MetricCard label="Quality Coverage Target" value="—" subtext="BACKEND API REQUIRED" icon={<ShieldCheck size={14} />} />
        <MetricCard label="Linked Repositories" value="—" subtext="BACKEND API REQUIRED" icon={<GitBranch size={14} />} />
        <MetricCard label="Active Test Environments" value="—" subtext="BACKEND API REQUIRED" icon={<Layers size={14} />} />
      </div>

      {/* Project Workspace Container */}
      <Card
        icon={<FolderKanban size={14} />}
        title="Project Directory"
        action={
          <div className="flex items-center gap-2">
            <div className="relative">
              <Search size={12} className="absolute left-2.5 top-2 text-slate-500" />
              <input
                disabled
                placeholder="Filter projects…"
                className="pl-7 pr-3 py-1 rounded bg-slate-900 border border-slate-800 text-xs text-slate-500 font-mono outline-none"
              />
            </div>
          </div>
        }
      >
        <div className="py-12 px-4 text-center space-y-3">
          <div className="w-10 h-10 rounded-full bg-slate-900 border border-slate-800 flex items-center justify-center mx-auto text-slate-500">
            <FolderKanban size={20} />
          </div>
          <div className="max-w-md mx-auto space-y-1">
            <h3 className="text-sm font-semibold text-slate-200">No Project Data Loaded</h3>
            <p className="text-xs text-slate-400 leading-relaxed">
              The frontend is structured to isolate all requirements, test cases, and runs by <code className="font-mono text-cyan-300">project_id</code>. Once the <code className="font-mono text-cyan-300">GET /api/v1/projects</code> endpoint is implemented in the orchestrator, your workspaces will appear here.
            </p>
          </div>
        </div>
      </Card>
    </div>
  );
}
