import React, { useState } from 'react';
import {
  CheckSquare,
  Square,
  Sparkles,
  Sliders,
  Play,
  Cpu,
  ShieldCheck,
  Zap,
  Activity,
  Layers,
  AlertTriangle,
  Flame,
  Gauge,
  Workflow,
  KeyRound,
  FileCode2,
  Lock,
  Skull,
  Eye,
  CheckCircle2,
  HelpCircle,
  RefreshCw,
  ArrowRight,
} from 'lucide-react';
import {
  TestGenerationCategory,
  GenerationExecutionState,
  UploadedRequirementDoc,
  AppSourceConnection,
  AppSourceType,
} from '../../types';

interface GenerationConfigSectionProps {
  selectedCategories: TestGenerationCategory[];
  onToggleCategory: (cat: TestGenerationCategory) => void;
  onSelectAllCategories: () => void;
  onDeselectAllCategories: () => void;
  requirementNotes: string;
  onRequirementNotesChange: (val: string) => void;
  depth: 'SMOKE' | 'STANDARD' | 'EXHAUSTIVE' | 'CHAOS_EXPLORATORY';
  onDepthChange: (depth: 'SMOKE' | 'STANDARD' | 'EXHAUSTIVE' | 'CHAOS_EXPLORATORY') => void;
  generationState: GenerationExecutionState;
  onTriggerGenerate: () => void;
  documents: UploadedRequirementDoc[];
  appSources: Record<AppSourceType, AppSourceConnection>;
}

interface CategoryMeta {
  id: TestGenerationCategory;
  title: string;
  description: string;
  icon: React.ComponentType<{ className?: string }>;
  accentColor: string;
}

export const TEST_CATEGORIES: CategoryMeta[] = [
  {
    id: 'Functional',
    title: 'Functional',
    description: 'Functional behavior and requirements.',
    icon: CheckCircle2,
    accentColor: 'text-cyan-400',
  },
  {
    id: 'Non-Functional',
    title: 'Non-Functional',
    description: 'Quality attributes beyond core functionality.',
    icon: Layers,
    accentColor: 'text-indigo-400',
  },
  {
    id: 'Positive',
    title: 'Positive',
    description: 'Valid expected user/system behavior.',
    icon: CheckSquare,
    accentColor: 'text-emerald-400',
  },
  {
    id: 'Negative',
    title: 'Negative',
    description: 'Invalid input and failure behavior.',
    icon: AlertTriangle,
    accentColor: 'text-rose-400',
  },
  {
    id: 'UI/UX',
    title: 'UI/UX',
    description: 'Interface and user-experience validation.',
    icon: Eye,
    accentColor: 'text-purple-400',
  },
  {
    id: 'Spike',
    title: 'Spike',
    description: 'Exploratory/spike scenarios.',
    icon: Flame,
    accentColor: 'text-orange-400',
  },
  {
    id: 'Load',
    title: 'Load',
    description: 'Load-related testing.',
    icon: Gauge,
    accentColor: 'text-amber-400',
  },
  {
    id: 'Edge Case',
    title: 'Edge Case',
    description: 'Boundary and unusual conditions.',
    icon: HelpCircle,
    accentColor: 'text-fuchsia-400',
  },
  {
    id: 'Senior Base',
    title: 'Senior Base',
    description: 'Senior-level/deep test scenarios.',
    icon: ShieldCheck,
    accentColor: 'text-teal-400',
  },
  {
    id: 'Business Logic',
    title: 'Business Logic',
    description: 'Business-rule validation.',
    icon: Workflow,
    accentColor: 'text-sky-400',
  },
  {
    id: 'API',
    title: 'API',
    description: 'API behavior and contract testing.',
    icon: FileCode2,
    accentColor: 'text-blue-400',
  },
  {
    id: 'Performance',
    title: 'Performance',
    description: 'Performance validation.',
    icon: Zap,
    accentColor: 'text-yellow-400',
  },
  {
    id: 'Security',
    title: 'Security',
    description: 'Security-focused testing.',
    icon: Lock,
    accentColor: 'text-red-400',
  },
  {
    id: 'Penetration',
    title: 'Penetration',
    description: 'Penetration-testing scenarios.',
    icon: Skull,
    accentColor: 'text-rose-500',
  },
];

export const GenerationConfigSection: React.FC<GenerationConfigSectionProps> = ({
  selectedCategories,
  onToggleCategory,
  onSelectAllCategories,
  onDeselectAllCategories,
  requirementNotes,
  onRequirementNotesChange,
  depth,
  onDepthChange,
  generationState,
  onTriggerGenerate,
  documents,
  appSources,
}) => {
  const [validationWarning, setValidationWarning] = useState<string | null>(null);

  const allSelected = selectedCategories.length === TEST_CATEGORIES.length;
  const isGenerating = ['VALIDATING', 'PROCESSING', 'GENERATING'].includes(generationState);

  // Phase 1 (AI Test Case Generator integration) — validation now reflects
  // what the real backend contract (`POST .../test-cases/generate-ai`)
  // actually requires: a non-empty `requirement_text`. Uploaded documents
  // and connected application sources do not, on their own, provide any
  // analyzable content to that endpoint today (their content is never
  // extracted/fetched — see `DocumentUploadSection`/`ApplicationSourceSection`),
  // so they can no longer substitute for requirement text the way the old
  // local-template generator allowed.
  const handleGenerateClick = () => {
    if (selectedCategories.length === 0) {
      setValidationWarning('Please select at least one test category to focus on.');
      return;
    }

    if (requirementNotes.trim().length === 0) {
      setValidationWarning(
        'Enter requirement text describing what to test. AI generation needs real written requirements — uploading a document or connecting a source alone does not provide analyzable content yet.'
      );
      return;
    }

    setValidationWarning(null);
    onTriggerGenerate();
  };

  return (
    <div id="test-generation-configuration-section" className="space-y-4">
      {/* Header with Select All Controls */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-2 border-b border-slate-800/80">
        <div>
          <div className="flex items-center gap-2">
            <Sliders className="w-4 h-4 text-cyan-400" />
            <h2 className="text-sm font-bold text-slate-100 font-sans tracking-tight">
              Test Generation Configuration
            </h2>
            <span className="text-[10px] font-mono font-semibold px-2 py-0.5 rounded bg-cyan-950/60 border border-cyan-800/60 text-cyan-300">
              {selectedCategories.length} / {TEST_CATEGORIES.length} SELECTED
            </span>
          </div>
          <p className="text-xs text-slate-400 font-sans mt-0.5">
            Selected categories and depth are sent to the AI model as focus instructions alongside your requirement text.
          </p>
        </div>

        {/* Multi-select Helpers */}
        <div className="flex items-center gap-2">
          {allSelected ? (
            <button
              type="button"
              onClick={onDeselectAllCategories}
              disabled={isGenerating}
              className="text-xs font-mono text-slate-400 hover:text-slate-200 px-2 py-1 rounded bg-[#020617] border border-slate-800 cursor-pointer disabled:opacity-50"
            >
              Deselect All
            </button>
          ) : (
            <button
              type="button"
              onClick={onSelectAllCategories}
              disabled={isGenerating}
              className="text-xs font-mono text-indigo-400 hover:text-indigo-300 px-2.5 py-1 rounded bg-indigo-950/60 border border-indigo-800/80 cursor-pointer disabled:opacity-50"
            >
              Select All (14)
            </button>
          )}
        </div>
      </div>

      {/* 14 Categories Multi-Select Chips / Cards Grid */}
      <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-7 gap-2">
        {TEST_CATEGORIES.map((cat) => {
          const isSelected = selectedCategories.includes(cat.id);
          const Icon = cat.icon;

          return (
            <div
              key={cat.id}
              id={`cat-card-${cat.id.toLowerCase().replace(/\s+/g, '-')}`}
              onClick={() => !isGenerating && onToggleCategory(cat.id)}
              className={`p-2.5 rounded-lg border text-left cursor-pointer transition-all duration-150 flex flex-col justify-between select-none ${
                isSelected
                  ? 'bg-gradient-to-b from-indigo-950/60 to-[#0B1120] border-indigo-500/80 shadow-[0_0_12px_rgba(99,102,241,0.2)]'
                  : 'bg-[#0B1120] border-slate-800/90 hover:border-slate-700 opacity-70 hover:opacity-100'
              } ${isGenerating ? 'cursor-not-allowed opacity-50' : ''}`}
            >
              <div className="flex items-center justify-between">
                <Icon className={`w-3.5 h-3.5 ${cat.accentColor}`} />
                {isSelected ? (
                  <CheckSquare className="w-3.5 h-3.5 text-indigo-400" />
                ) : (
                  <Square className="w-3.5 h-3.5 text-slate-600" />
                )}
              </div>

              <div className="mt-2">
                <div className="text-xs font-bold font-sans text-slate-100 leading-tight">
                  {cat.title}
                </div>
                <div className="text-[10px] text-slate-400 font-sans mt-0.5 line-clamp-2 leading-tight">
                  {cat.description}
                </div>
              </div>
            </div>
          );
        })}
      </div>

      {/* Optional Guidance / Requirements Prompt & AI Engine Controls */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-3 pt-2">
        {/* Specific Requirement Focus / Context Input (8 Cols) */}
        <div className="lg:col-span-8 p-3 rounded-lg bg-[#0B1120] border border-slate-800 space-y-1.5">
          <label className="block text-xs font-bold font-mono text-slate-300">
            Requirement Text (Required)
          </label>
          <textarea
            rows={3}
            placeholder="E.g. Focus specifically on edge cases in token renewal and OAuth PKCE challenge-response flow across multi-tenant org boundaries..."
            value={requirementNotes}
            onChange={(e) => onRequirementNotesChange(e.target.value)}
            disabled={isGenerating}
            className="w-full p-2.5 rounded bg-[#020617] border border-slate-700 hover:border-slate-600 focus:border-indigo-500 text-xs font-mono text-slate-100 placeholder-slate-500 focus:outline-none resize-none"
          />
          <div className="flex items-center justify-between text-[10px] font-mono text-slate-500">
            <span>The actual text sent to the AI model — uploaded documents and connected sources are referenced by name only, not analyzed</span>
            <span>{requirementNotes.length} chars</span>
          </div>
        </div>

        {/* Generation Settings (4 Cols) — FE-16: the former "AI Reasoning
            Engine" provider selector (gemini-2.0-flash / gemini-1.5-pro /
            claude-3-5-sonnet) was removed here. It never had any real
            generation effect — selecting a value only changed a cosmetic
            string in the generated case's metadata — so per FE-15 GAP-02
            Option A it was removed rather than kept as a misleading
            interaction. See the FE-16 report. */}
        <div className="lg:col-span-4 p-3 rounded-lg bg-[#0B1120] border border-slate-800 space-y-2.5">
          <div>
            <label className="block text-xs font-bold font-mono text-slate-300 mb-1">
              Exploration Depth
            </label>
            <select
              value={depth}
              onChange={(e) => onDepthChange(e.target.value as any)}
              disabled={isGenerating}
              className="w-full px-2.5 py-1.5 rounded bg-[#020617] border border-slate-700 text-xs font-mono text-slate-200 focus:outline-none focus:border-indigo-500 cursor-pointer"
            >
              <option value="SMOKE">Smoke (Critical P0 Scenarios)</option>
              <option value="STANDARD">Standard (Core Acceptance & Edge)</option>
              <option value="EXHAUSTIVE">Exhaustive (Full Combinatorial Matrix)</option>
              <option value="CHAOS_EXPLORATORY">Chaos & Exploratory (Adversarial)</option>
            </select>
          </div>
        </div>
      </div>

      {/* Validation Warning */}
      {validationWarning && (
        <div className="p-2.5 rounded bg-amber-950/40 border border-amber-800/80 text-xs font-mono text-amber-300 flex items-center gap-2">
          <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0" />
          <span>{validationWarning}</span>
        </div>
      )}

      {/* Primary CTA Area with Visual Pipeline Communication */}
      <div className="p-4 rounded-lg bg-gradient-to-r from-[#020617] via-[#0B1120] to-[#020617] border border-indigo-900/60 shadow-lg flex flex-col md:flex-row md:items-center justify-between gap-4">
        {/* Pipeline Flow Visualization: Skills+Requirement -> AI Model Router -> Validated Test Cases.
            Phase 1: reflects the real backend pipeline (`ai_generation.rs` ->
            `ai_router.rs`) this button now actually calls — not a decorative
            label for local template assembly. */}
        <div className="flex items-center gap-2 text-xs font-mono text-slate-300">
          <div className="flex items-center gap-1.5 px-2.5 py-1 rounded bg-[#020617] border border-slate-800">
            <span className="w-1.5 h-1.5 rounded-full bg-cyan-400" />
            <span className="font-semibold text-cyan-300">Requirement + Skills</span>
          </div>
          <ArrowRight className="w-3.5 h-3.5 text-indigo-400 shrink-0" />
          <div className="flex items-center gap-1.5 px-2.5 py-1 rounded bg-[#020617] border border-slate-800">
            <span className="w-1.5 h-1.5 rounded-full bg-purple-400" />
            <span className="font-semibold text-purple-300">AI Model Router</span>
          </div>
          <ArrowRight className="w-3.5 h-3.5 text-indigo-400 shrink-0" />
          <div className="flex items-center gap-1.5 px-2.5 py-1 rounded bg-[#020617] border border-slate-800">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
            <span className="font-semibold text-emerald-300">Validated Test Cases</span>
          </div>
        </div>

        {/* Action Button */}
        <button
          type="button"
          id="btn-generate-test-cases-primary"
          onClick={handleGenerateClick}
          disabled={isGenerating}
          className="flex items-center justify-center gap-2 px-6 py-2.5 rounded-md bg-indigo-600 hover:bg-indigo-500 disabled:bg-indigo-900/60 text-white font-bold text-xs font-mono shadow-md shadow-indigo-950/80 cursor-pointer disabled:cursor-not-allowed transition-all shrink-0"
        >
          {isGenerating ? (
            <>
              <RefreshCw className="w-4 h-4 animate-spin text-cyan-300" />
              <span>Generating ({generationState})...</span>
            </>
          ) : (
            <>
              <Sparkles className="w-4 h-4 fill-current text-cyan-300" />
              <span>Generate Test Cases</span>
            </>
          )}
        </button>
      </div>
    </div>
  );
};
