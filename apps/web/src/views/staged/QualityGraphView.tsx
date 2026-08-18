/**
 * Quality Graph & Traceability Flow View (Contract §7)
 * Visualizes requirement -> feature -> module -> test -> execution -> evidence relationships.
 */

import React from 'react';
import {
  Layers,
  GitMerge,
  Search,
  Network,
  Share2,
  FileCheck2,
  CheckCircle2,
  ShieldCheck,
  Workflow,
} from 'lucide-react';
import { SectionHeader } from '../../components/common/SectionHeader';
import { BackendNotice } from '../../components/common/BackendNotice';
import { Card } from '../../components/common/Card';
import { MetricCard } from '../../components/common/MetricCard';

export function QualityGraphView() {
  return (
    <div className="space-y-4">
      <SectionHeader
        title="Quality Graph & Requirements Traceability"
        subtitle="End-to-end topological mapping across requirements, features, endpoints, test cases, and defect findings."
        badge="STAGED SPECIFICATION"
        badgeTone="warn"
      />

      <BackendNotice
        feature="Quality Graph Topology Engine"
        contractSection="§7 (Quality Graph & Requirement APIs)"
        endpoint="GET /api/v1/projects/{project_id}/quality-graph"
        requires="Migration 0002 created the underlying relational tables for requirements and modules, but no HTTP route currently exposes the query API or graph adjacency matrix."
        payloadExample={`// GET /api/v1/projects/{project_id}/quality-graph
{
  "nodes": [
    { "id": "req-101", "type": "REQUIREMENT", "name": "OAuth2 PKCE Token Flow" },
    { "id": "feat-204", "type": "FEATURE", "name": "Authentication Gateway" },
    { "id": "tc-301", "type": "TEST_CASE", "name": "Verify Token Refresh Expiry" }
  ],
  "edges": [
    { "source": "req-101", "target": "feat-204", "relation": "IMPLEMENTS" },
    { "source": "feat-204", "target": "tc-301", "relation": "COVERS" }
  ]
}`}
      />

      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <MetricCard label="Mapped Requirements" value="—" subtext="BACKEND API REQUIRED" icon={<FileCheck2 size={14} />} />
        <MetricCard label="Feature Nodes" value="—" subtext="BACKEND API REQUIRED" icon={<Layers size={14} />} />
        <MetricCard label="Traceability Ratio" value="—" subtext="BACKEND API REQUIRED" icon={<GitMerge size={14} />} />
        <MetricCard label="Uncovered Surfaces" value="—" subtext="BACKEND API REQUIRED" icon={<ShieldCheck size={14} />} />
      </div>

      <Card icon={<Network size={14} />} title="Topological Quality Traceability Map">
        <div className="p-8 text-center space-y-3">
          <div className="w-12 h-12 rounded-full bg-slate-900 border border-slate-800 flex items-center justify-center mx-auto text-cyan-400">
            <Share2 size={22} />
          </div>
          <div className="max-w-md mx-auto space-y-1">
            <h3 className="text-sm font-semibold text-slate-200">Interactive Graph Stream Pending</h3>
            <p className="text-xs text-slate-400 leading-relaxed">
              Traceability graph rendering is ready. When the quality graph endpoint responds with project nodes and edge links, the interactive SVG canvas will visualize live coverage status from requirement through to defect evidence.
            </p>
          </div>
        </div>
      </Card>
    </div>
  );
}
