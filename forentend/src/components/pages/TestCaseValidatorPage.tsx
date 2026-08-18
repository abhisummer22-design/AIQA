/**
 * AIQA Test Case Validator Page
 * Verify traceability, quality, duplicates, coverage and risk before automatic execution.
 */

import React, { useState } from 'react';
import {
  CheckCheck,
  ShieldCheck,
  FileCheck,
  AlertCircle,
  Copy,
  Layers,
  Sparkles,
  Search,
  Filter,
  CheckCircle2,
  XCircle,
  Clock,
  ArrowRight,
  ExternalLink,
  ChevronDown,
  Wand2,
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { SectionHeader } from '../common/SectionHeader';
import { StatusBadge } from '../common/StatusBadge';
import { ValidationRuleCheck } from '../../types';

export const TestCaseValidatorPage: React.FC = () => {
  const { addActivity } = useAIQA();

  const [validationItems, setValidationItems] = useState<ValidationRuleCheck[]>([
    {
      id: 'VAL-101',
      testCaseId: 'TC-AUTH-001',
      testCaseTitle: 'Validate OAuth2 Authorization Code Exchange with PKCE',
      traceabilitySource: 'PRD-PAY-001',
      traceabilityStatus: 'LINKED',
      qualityScore: 98,
      duplicateSimilarityPct: 4,
      riskCoverage: 'SECURITY_BOUNDARY',
      approvalState: 'APPROVED',
      remediationSuggestions: [],
    },
    {
      id: 'VAL-102',
      testCaseId: 'TC-PAY-004',
      testCaseTitle: 'Verify Idempotency Key deduplication on rapid double-submit',
      traceabilitySource: 'FRD-AUTH-09',
      traceabilityStatus: 'LINKED',
      qualityScore: 94,
      duplicateSimilarityPct: 12,
      riskCoverage: 'CRITICAL_PATH',
      approvalState: 'APPROVED',
      remediationSuggestions: [],
    },
    {
      id: 'VAL-103',
      testCaseId: 'TC-UI-082',
      testCaseTitle: 'Verify button styling and hover drop-shadow on checkout card',
      traceabilitySource: 'FIGMA-FRAME-402',
      traceabilityStatus: 'PARTIAL',
      qualityScore: 62,
      duplicateSimilarityPct: 78,
      duplicateWithId: 'TC-UI-019',
      riskCoverage: 'SUPERFLUOUS',
      approvalState: 'REQUIRES_REMEDIATION',
      remediationSuggestions: [
        'High semantic overlap (78%) with TC-UI-019. Consider merging visual assertion into base snapshot.',
        'Traceability link to Figma Frame 402 lacks explicit design token acceptance criteria.',
      ],
    },
    {
      id: 'VAL-104',
      testCaseId: 'TC-LEDGER-023',
      testCaseTitle: 'Audit double-entry ledger balance consistency during partition failover',
      traceabilitySource: 'SRD-API-SPEC',
      traceabilityStatus: 'LINKED',
      qualityScore: 96,
      duplicateSimilarityPct: 2,
      riskCoverage: 'CRITICAL_PATH',
      approvalState: 'APPROVED',
      remediationSuggestions: [],
    },
    {
      id: 'VAL-105',
      testCaseId: 'TC-ANON-991',
      testCaseTitle: 'Test temporary user session logout redirect with fallback delay',
      traceabilitySource: 'FRD-AUTH-09',
      traceabilityStatus: 'ORPHAN',
      qualityScore: 54,
      duplicateSimilarityPct: 45,
      riskCoverage: 'EDGE_CASE',
      approvalState: 'AUTO_BLOCKED',
      remediationSuggestions: [
        'Orphan Test: No linked requirement ID found in active PRD / FRD Jira backlog.',
        'Quality Index 54/100: Preconditions lack deterministic seed user state.',
      ],
    },
  ]);

  const [selectedFilter, setSelectedFilter] = useState<'ALL' | 'APPROVED' | 'REQUIRES_REMEDIATION' | 'AUTO_BLOCKED'>('ALL');
  const [searchTerm, setSearchTerm] = useState('');
  const [selectedItem, setSelectedItem] = useState<ValidationRuleCheck | null>(validationItems[2]);

  const filteredItems = validationItems.filter((item) => {
    const matchesFilter = selectedFilter === 'ALL' || item.approvalState === selectedFilter;
    const matchesSearch =
      item.testCaseTitle.toLowerCase().includes(searchTerm.toLowerCase()) ||
      item.testCaseId.toLowerCase().includes(searchTerm.toLowerCase()) ||
      item.traceabilitySource.toLowerCase().includes(searchTerm.toLowerCase());
    return matchesFilter && matchesSearch;
  });

  const handleApprove = (id: string) => {
    setValidationItems(
      validationItems.map((item) =>
        item.id === id ? { ...item, approvalState: 'APPROVED' } : item
      )
    );
    addActivity({
      type: 'REVIEW_REQUIRED',
      title: `Test Case Validated & Approved: ${id}`,
      description: `Test case gate cleared for execution scheduling.`,
      actor: 'Test Case Validator',
    });
  };

  const handleAutoRemediate = (id: string) => {
    setValidationItems(
      validationItems.map((item) =>
        item.id === id
          ? {
              ...item,
              qualityScore: 92,
              duplicateSimilarityPct: 8,
              traceabilityStatus: 'LINKED',
              approvalState: 'APPROVED',
              remediationSuggestions: ['Auto-remediated by AI: merged redundant assertions, linked to PRD acceptance criterion.'],
            }
          : item
      )
    );
    addActivity({
      type: 'EVIDENCE_CAPTURED',
      title: `AI Remediation Applied: ${id}`,
      description: `Refactored assertions and resolved duplicate test overlap.`,
      actor: 'AI Quality Engine',
    });
  };

  return (
    <div className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Test Case Validator"
        subtitle="Verify requirement traceability, semantic duplicate overlap, quality index scoring, and risk coverage prior to automatic execution dispatch"
        actions={
          <div className="flex items-center gap-2">
            <button
              onClick={() => {
                validationItems.forEach((v) => handleAutoRemediate(v.id));
              }}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shadow-sm shadow-indigo-950/60 cursor-pointer"
            >
              <Wand2 className="w-3.5 h-3.5" />
              <span>Batch Remediate All</span>
            </button>
          </div>
        }
      />

      {/* Validation Top Metrics */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-2.5">
        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>Traceability Coverage</span>
            <FileCheck className="w-3.5 h-3.5 text-indigo-400" />
          </div>
          <div className="text-lg font-bold font-mono text-emerald-400">
            {Math.round(
              (validationItems.filter((v) => v.traceabilityStatus === 'LINKED').length /
                validationItems.length) *
                100
            )}
            % Verified
          </div>
          <div className="text-[10px] text-slate-500">PRD / FRD / Figma linked</div>
        </div>

        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>Mean Quality Score</span>
            <ShieldCheck className="w-3.5 h-3.5 text-indigo-400" />
          </div>
          <div className="text-lg font-bold font-mono text-slate-100">
            {Math.round(
              validationItems.reduce((acc, v) => acc + v.qualityScore, 0) / validationItems.length
            )}
            /100 Index
          </div>
          <div className="text-[10px] text-slate-500">Step precision & clarity</div>
        </div>

        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>Duplicate Clustered</span>
            <Copy className="w-3.5 h-3.5 text-yellow-400" />
          </div>
          <div className="text-lg font-bold font-mono text-yellow-300">
            {validationItems.filter((v) => v.duplicateSimilarityPct > 50).length} Overlapping
          </div>
          <div className="text-[10px] text-slate-500">Cosine similarity &gt; 50%</div>
        </div>

        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>Execution Gate Pass</span>
            <CheckCheck className="w-3.5 h-3.5 text-emerald-400" />
          </div>
          <div className="text-lg font-bold font-mono text-emerald-400">
            {validationItems.filter((v) => v.approvalState === 'APPROVED').length} / {validationItems.length} Tests
          </div>
          <div className="text-[10px] text-slate-500">Cleared for automated runner</div>
        </div>
      </div>

      {/* Main Validation Split View */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-4">
        {/* Left Column: Validation Rules List (7 cols) */}
        <div className="lg:col-span-7 rounded-lg border border-slate-800 bg-[#0B1120] p-3 space-y-3">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-2 border-b border-slate-800">
            <div className="flex items-center gap-2">
              <span className="text-xs font-bold text-slate-200 uppercase font-mono">
                Validation Assessment Queue ({filteredItems.length})
              </span>
            </div>

            {/* Filter Tabs */}
            <div className="flex items-center gap-1 bg-[#020617] p-1 rounded border border-slate-800 text-[10px] font-mono">
              {(['ALL', 'APPROVED', 'REQUIRES_REMEDIATION', 'AUTO_BLOCKED'] as const).map((filter) => (
                <button
                  key={filter}
                  onClick={() => setSelectedFilter(filter)}
                  className={`px-2 py-0.5 rounded cursor-pointer ${
                    selectedFilter === filter
                      ? 'bg-indigo-600 text-white font-bold'
                      : 'text-slate-400 hover:text-slate-200'
                  }`}
                >
                  {filter.replace('_', ' ')}
                </button>
              ))}
            </div>
          </div>

          <div className="space-y-2">
            {filteredItems.map((item) => {
              const isSelected = selectedItem?.id === item.id;
              return (
                <div
                  key={item.id}
                  onClick={() => setSelectedItem(item)}
                  className={`p-3 rounded-lg border text-xs cursor-pointer transition-all ${
                    isSelected
                      ? 'border-indigo-500 bg-indigo-950/30'
                      : 'border-slate-800 bg-[#020617] hover:border-slate-700'
                  }`}
                >
                  <div className="flex items-start justify-between gap-2 mb-1.5">
                    <div className="space-y-0.5">
                      <div className="flex items-center gap-2">
                        <span className="font-mono font-bold text-indigo-400 text-xs">{item.testCaseId}</span>
                        <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-slate-800 text-slate-300 border border-slate-700">
                          {item.traceabilitySource}
                        </span>
                        <span
                          className={`text-[9px] font-mono px-1.5 py-0.2 rounded font-bold ${
                            item.traceabilityStatus === 'LINKED'
                              ? 'bg-emerald-950 text-emerald-300 border border-emerald-800'
                              : item.traceabilityStatus === 'PARTIAL'
                              ? 'bg-yellow-950 text-yellow-300 border border-yellow-800'
                              : 'bg-rose-950 text-rose-300 border border-rose-800'
                          }`}
                        >
                          {item.traceabilityStatus}
                        </span>
                      </div>
                      <h4 className="font-semibold text-slate-100 text-xs">{item.testCaseTitle}</h4>
                    </div>

                    <span
                      className={`text-[10px] font-mono px-2 py-0.5 rounded font-bold shrink-0 ${
                        item.approvalState === 'APPROVED'
                          ? 'bg-emerald-950 text-emerald-300 border border-emerald-800'
                          : item.approvalState === 'REQUIRES_REMEDIATION'
                          ? 'bg-yellow-950 text-yellow-300 border border-yellow-800'
                          : 'bg-rose-950 text-rose-300 border border-rose-800'
                      }`}
                    >
                      {item.approvalState.replace('_', ' ')}
                    </span>
                  </div>

                  <div className="flex items-center justify-between text-[10px] font-mono text-slate-400 pt-1 border-t border-slate-800/80">
                    <div className="flex items-center gap-3">
                      <span>Quality: <strong className="text-slate-200">{item.qualityScore}/100</strong></span>
                      <span>
                        Duplicate Risk:{' '}
                        <strong
                          className={item.duplicateSimilarityPct > 50 ? 'text-yellow-400' : 'text-slate-300'}
                        >
                          {item.duplicateSimilarityPct}%
                        </strong>
                      </span>
                    </div>
                    <span className="text-indigo-300 font-semibold">{item.riskCoverage.replace('_', ' ')}</span>
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Right Column: Deep Inspection & Gate Sign-off (5 cols) */}
        <div className="lg:col-span-5 rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-4 text-xs">
          {selectedItem ? (
            <div className="space-y-4">
              <div className="pb-3 border-b border-slate-800">
                <div className="flex items-center justify-between">
                  <span className="font-mono text-indigo-400 font-bold text-xs">{selectedItem.testCaseId}</span>
                  <span
                    className={`text-[9px] font-mono px-2 py-0.5 rounded font-bold ${
                      selectedItem.approvalState === 'APPROVED'
                        ? 'bg-emerald-950 text-emerald-300 border border-emerald-800'
                        : 'bg-yellow-950 text-yellow-300 border border-yellow-800'
                    }`}
                  >
                    {selectedItem.approvalState}
                  </span>
                </div>
                <h3 className="text-sm font-bold text-slate-100 mt-1">{selectedItem.testCaseTitle}</h3>
              </div>

              {/* Traceability Spec Details */}
              <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-2">
                <div className="text-[10px] font-mono uppercase text-slate-400 font-bold flex items-center justify-between">
                  <span>Requirement Traceability</span>
                  <span className="text-emerald-400">Spec Validated</span>
                </div>
                <div className="text-slate-300 font-mono text-[11px] space-y-1">
                  <div>Source Document: <strong className="text-indigo-400">{selectedItem.traceabilitySource}</strong></div>
                  <div>Trace Status: <strong className="text-slate-100">{selectedItem.traceabilityStatus}</strong></div>
                  <div>Risk Domain: <strong className="text-slate-100">{selectedItem.riskCoverage}</strong></div>
                </div>
              </div>

              {/* Duplicate Detection Insight */}
              <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-2">
                <div className="text-[10px] font-mono uppercase text-slate-400 font-bold flex items-center justify-between">
                  <span>Semantic Overlap & Deduplication</span>
                  <span className={selectedItem.duplicateSimilarityPct > 50 ? 'text-yellow-400' : 'text-emerald-400'}>
                    {selectedItem.duplicateSimilarityPct}% Overlap
                  </span>
                </div>
                <p className="text-[11px] text-slate-400">
                  {selectedItem.duplicateWithId
                    ? `Overlaps with ${selectedItem.duplicateWithId}. Assertions are nearly identical to existing test suite.`
                    : 'Unique test vector. No redundant step clustering detected.'}
                </p>
              </div>

              {/* Remediation Suggestions */}
              {selectedItem.remediationSuggestions && selectedItem.remediationSuggestions.length > 0 && (
                <div className="p-3 rounded-lg bg-yellow-950/30 border border-yellow-800/80 space-y-2">
                  <div className="text-[10px] font-mono uppercase text-yellow-400 font-bold flex items-center gap-1.5">
                    <AlertCircle className="w-3.5 h-3.5" />
                    <span>Validator Remediation Recommendations</span>
                  </div>
                  <ul className="list-disc list-inside text-yellow-200/90 text-[11px] space-y-1">
                    {selectedItem.remediationSuggestions.map((s, idx) => (
                      <li key={idx}>{s}</li>
                    ))}
                  </ul>
                </div>
              )}

              {/* Action Controls */}
              <div className="pt-3 border-t border-slate-800 flex items-center gap-2">
                <button
                  onClick={() => handleAutoRemediate(selectedItem.id)}
                  className="flex-1 py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs cursor-pointer text-center"
                >
                  Auto-Fix & Approve
                </button>
                <button
                  onClick={() => handleApprove(selectedItem.id)}
                  className="px-3 py-1.5 rounded bg-[#020617] hover:bg-slate-800 text-slate-200 border border-slate-800 font-semibold text-xs cursor-pointer"
                >
                  Manual Sign-off
                </button>
              </div>
            </div>
          ) : (
            <div className="py-12 text-center text-slate-500 font-mono text-xs">
              Select a validation check item to inspect quality and traceability.
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
