/**
 * Test Case Generator View (Contract §8)
 * AI-assisted test generation from documents, APIs, schemas, and PRD specifications.
 */

import React, { useState } from 'react';
import {
  Sparkles,
  UploadCloud,
  FileText,
  Sliders,
  Play,
  ShieldAlert,
  Layers,
  Code2,
  CheckCircle2,
  Cpu,
} from 'lucide-react';
import { SectionHeader } from '../../components/common/SectionHeader';
import { BackendNotice } from '../../components/common/BackendNotice';
import { Card } from '../../components/common/Card';
import { MetricCard } from '../../components/common/MetricCard';

const CATEGORIES = [
  { id: 'functional', label: 'Functional & User Workflows' },
  { id: 'edge-case', label: 'Edge Cases & Boundary Limits' },
  { id: 'security', label: 'Security & Penetration Testing' },
  { id: 'rbac', label: 'RBAC & Multi-Tenant Isolation' },
  { id: 'api', label: 'REST / GraphQL Contract & Fuzzing' },
  { id: 'performance', label: 'Performance & Latency Thresholds' },
  { id: 'mobile', label: 'Mobile Device Compatibility' },
  { id: 'business-logic', label: 'Complex Business Logic & Invariants' },
];

export function TestGeneratorView() {
  const [selectedCategories, setSelectedCategories] = useState<Record<string, boolean>>({
    functional: true,
    'edge-case': true,
    security: true,
    rbac: true,
    api: true,
  });
  const [includePositive, setIncludePositive] = useState(true);
  const [includeNegative, setIncludeNegative] = useState(true);
  const [modelTemperature, setModelTemperature] = useState(0.2);

  const toggleCategory = (id: string) => {
    setSelectedCategories((prev) => ({ ...prev, [id]: !prev[id] }));
  };

  return (
    <div className="space-y-4">
      <SectionHeader
        title="AI Test Case Generator"
        subtitle="Multi-category test generation synthesized from project specifications, schemas, and PRD documents."
        badge="STAGED SPECIFICATION"
        badgeTone="warn"
      />

      <BackendNotice
        feature="AI Test Generation Pipeline"
        contractSection="§8 (Test Case Generator APIs)"
        endpoint="POST /api/v1/projects/{project_id}/test-generation"
        requires="An LLM-backed inference service and async generation job worker. The current Rust backend has no LLM or model gateway integrations."
        payloadExample={`// POST /api/v1/projects/{project_id}/test-generation
{
  "scope": {
    "module_ids": ["mod_auth_01"],
    "requirement_ids": ["req_pkce_09"]
  },
  "categories": ["FUNCTIONAL", "SECURITY", "RBAC", "API", "EDGE_CASE"],
  "include_positive": true,
  "include_negative": true,
  "constraints": {
    "max_steps": 8,
    "require_traceability": true
  }
}`}
      />

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        {/* Left Column: Configuration Wizard */}
        <div className="lg:col-span-1 space-y-4">
          <Card icon={<Sliders size={14} />} title="Generation Parameters">
            <div className="space-y-3.5 text-xs">
              <div>
                <label className="block text-slate-300 font-medium mb-1 font-mono text-[10px] uppercase">
                  Coverage Categories
                </label>
                <div className="space-y-1.5">
                  {CATEGORIES.map((cat) => (
                    <label
                      key={cat.id}
                      className="flex items-center gap-2 p-2 rounded bg-slate-900/60 border border-slate-800 text-slate-300 cursor-pointer hover:bg-slate-900"
                    >
                      <input
                        type="checkbox"
                        checked={!!selectedCategories[cat.id]}
                        onChange={() => toggleCategory(cat.id)}
                        className="accent-cyan-500 rounded"
                      />
                      <span className="text-xs">{cat.label}</span>
                    </label>
                  ))}
                </div>
              </div>

              <div className="pt-2 border-t border-slate-800 space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-slate-300 font-medium font-mono text-[10px] uppercase">
                    Positive Path Assertions
                  </span>
                  <input
                    type="checkbox"
                    checked={includePositive}
                    onChange={(e) => setIncludePositive(e.target.checked)}
                    className="accent-cyan-500 rounded"
                  />
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-slate-300 font-medium font-mono text-[10px] uppercase">
                    Negative / Invariant Rejections
                  </span>
                  <input
                    type="checkbox"
                    checked={includeNegative}
                    onChange={(e) => setIncludeNegative(e.target.checked)}
                    className="accent-cyan-500 rounded"
                  />
                </div>
              </div>

              <button
                disabled
                className="w-full py-2 rounded-md bg-slate-800 border border-slate-700 text-slate-500 font-semibold text-xs cursor-not-allowed opacity-60 flex items-center justify-center gap-1.5"
              >
                <Sparkles size={13} />
                <span>Trigger AI Generation</span>
              </button>
            </div>
          </Card>
        </div>

        {/* Right Column: Ingestion & Generated Catalog */}
        <div className="lg:col-span-2 space-y-4">
          <Card icon={<UploadCloud size={14} />} title="Context Ingestion Source (Contract §6)">
            <div className="p-6 border-2 border-dashed border-slate-800 rounded-lg text-center space-y-2 bg-slate-950/40">
              <UploadCloud size={24} className="mx-auto text-slate-600" />
              <div className="text-xs font-semibold text-slate-300">
                Upload PRD, OpenAPI 3.0 Specs, or Database DDL
              </div>
              <p className="text-[11px] text-slate-500 max-w-sm mx-auto">
                Documents are chunked, embedded, and mapped into project quality nodes for targeted context synthesis.
              </p>
              <div className="pt-2">
                <span className="inline-flex items-center px-2 py-0.5 rounded bg-slate-900 border border-slate-800 text-[10px] font-mono text-slate-400">
                  Target: POST /api/v1/projects/{'{id}'}/context
                </span>
              </div>
            </div>
          </Card>

          <Card icon={<FileText size={14} />} title="Generated Test Case Catalogue">
            <div className="py-10 px-4 text-center space-y-2">
              <div className="text-xs text-slate-500 font-mono">
                No active generation jobs. Staging state ready.
              </div>
              <p className="text-[11px] text-slate-600 max-w-md mx-auto">
                Generated tests will be displayed here with requirement links, execution steps, expected outcomes, and gate approval buttons.
              </p>
            </div>
          </Card>
        </div>
      </div>
    </div>
  );
}
