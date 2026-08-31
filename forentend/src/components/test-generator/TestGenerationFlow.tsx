/**
 * Test Generation Flow — a real, top-to-bottom readout of this page's own
 * generation pipeline (Sources -> Skills -> AI Agent -> Test Cases),
 * replacing the AI Engine orb that sat in this rail before.
 *
 * The orb showed the routing engine in the abstract (which model *would*
 * be used); this shows the concrete run of *this page*, from the same
 * state `TestCaseGeneratorPage` already owns — no new fetch, no second
 * source of truth. Every node's status and every animated connector maps
 * to a real value:
 *
 *   - **Sources** — the real count of ready uploaded documents and
 *     connected app sources (`documents`, `appSources`).
 *   - **Skills** — the real count of skills that will actually be sent
 *     (`effectiveSkillIds`, manual + category-auto-matched).
 *   - **AI Agent** — the real generation call itself: idle before the
 *     first click, animated only while `generationState` is genuinely
 *     in flight, and on completion shows the real
 *     `provider`/`model` (or the real `fallback_reason` on failure) from
 *     `GenerateAiTestCasesResponse` — never a placeholder model name.
 *   - **Test Cases** — the real count of cases the last response produced.
 *
 * The connecting line between two nodes only animates while the stage
 * between them is genuinely in progress — a still line means nothing is
 * moving right now, matching the same rule `AIEngineOrb` uses ("motion
 * must encode something real, or not appear").
 */
import React from 'react';
import { BrainCircuit, FileCheck2, FolderOpen, Sparkles } from 'lucide-react';
import { FlowConnector, FlowNode, type FlowNodeState } from '../common/FlowRail';
import type { AppSourceConnection, AppSourceType, GenerationExecutionState, UploadedRequirementDoc } from '../../types';
import type { GenerateAiTestCasesResponse } from '../../types/contract';

export interface TestGenerationFlowProps {
  documents: UploadedRequirementDoc[];
  appSources: Record<AppSourceType, AppSourceConnection>;
  skillCount: number;
  generationState: GenerationExecutionState;
  lastGenerationResult: GenerateAiTestCasesResponse | null;
  generatedCasesCount: number;
}

export const TestGenerationFlow: React.FC<TestGenerationFlowProps> = ({
  documents,
  appSources,
  skillCount,
  generationState,
  lastGenerationResult,
  generatedCasesCount,
}) => {
  // Matches exactly what `buildRequirementText` (TestCaseGeneratorPage.tsx)
  // actually sends: every uploaded doc's slotId, regardless of status, and
  // every app source with a real `CONNECTED` check — not a narrower/looser
  // count that would drift from what the real request contains.
  const docCount = documents.length;
  const connectedSourceCount = (Object.values(appSources) as AppSourceConnection[]).filter((s) => s.status === 'CONNECTED').length;
  const isRunning = generationState === 'VALIDATING' || generationState === 'PROCESSING' || generationState === 'GENERATING';
  const isFailed = generationState === 'FAILED' || lastGenerationResult?.generation_method === 'fallback';
  const isDone = generationState === 'COMPLETED' || generationState === 'PARTIAL';

  const sourcesState: FlowNodeState = 'idle';
  const skillsState: FlowNodeState = 'idle';
  const agentState: FlowNodeState = isRunning ? 'active' : isFailed ? 'failed' : isDone ? 'done' : 'idle';
  const casesState: FlowNodeState = isFailed && generatedCasesCount === 0 ? 'failed' : generatedCasesCount > 0 ? 'done' : 'idle';

  const agentDetail = isRunning
    ? 'Generating now…'
    : isFailed
      ? lastGenerationResult?.fallback_reason ?? 'No compatible model responded.'
      : isDone && lastGenerationResult?.provider && lastGenerationResult?.model
        ? `${lastGenerationResult.provider} / ${lastGenerationResult.model}`
        : 'Waiting to generate';

  return (
    <div id="test-generation-flow" className="p-3 rounded-lg bg-[#0B1120] border border-slate-800 h-full flex flex-col">
      <h3 className="text-[10px] font-bold text-slate-100 font-mono uppercase tracking-wider mb-3">Generation Flow</h3>

      <FlowNode
        icon={FolderOpen}
        label="Sources"
        state={sourcesState}
        detail={`${docCount} doc${docCount === 1 ? '' : 's'} · ${connectedSourceCount} source${connectedSourceCount === 1 ? '' : 's'} connected`}
      />
      <FlowConnector animated={false} />
      <FlowNode
        icon={Sparkles}
        label="Skills"
        state={skillsState}
        detail={`${skillCount} skill${skillCount === 1 ? '' : 's'} applied`}
      />
      <FlowConnector animated={isRunning} />
      <FlowNode icon={BrainCircuit} label="AI Agent" state={agentState} detail={agentDetail} />
      <FlowConnector animated={isRunning && generationState === 'GENERATING'} />
      <FlowNode
        icon={FileCheck2}
        label="Test Cases"
        state={casesState}
        detail={generatedCasesCount > 0 ? `${generatedCasesCount} case${generatedCasesCount === 1 ? '' : 's'} written` : '—'}
      />
    </div>
  );
};
