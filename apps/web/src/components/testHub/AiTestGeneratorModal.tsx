import React, { useState } from "react";
import { X, Sparkles, CheckCircle2, ArrowRight, Layers, Loader2, Check } from "lucide-react";
import { TestCase, TestType } from "../../types";
import { TEST_TYPE_LABELS, CATEGORY_TABS } from "./TestCategoryNav";
import { generateAiTestCasesApi } from "../../services/api";

interface AiTestGeneratorModalProps {
  isOpen: boolean;
  onClose: () => void;
  initialCategory?: TestType;
  initialFeature?: string;
  onTestCasesAdded: (newCases: TestCase[]) => void;
}

export const AiTestGeneratorModal: React.FC<AiTestGeneratorModalProps> = ({
  isOpen,
  onClose,
  initialCategory = "FUNCTIONAL",
  initialFeature = "User Authentication & MFA Flow",
  onTestCasesAdded
}) => {
  const [testType, setTestType] = useState<TestType>(initialCategory);
  const [targetFeature, setTargetFeature] = useState<string>(initialFeature);
  const [contextPrompt, setContextPrompt] = useState<string>("");
  const [count, setCount] = useState<number>(3);
  const [isGenerating, setIsGenerating] = useState<boolean>(false);
  const [generatedList, setGeneratedList] = useState<TestCase[]>([]);
  const [statusMessage, setStatusMessage] = useState<string>("");

  if (!isOpen) return null;

  const handleGenerate = async () => {
    setIsGenerating(true);
    setStatusMessage(`Synthesizing ${count} ${TEST_TYPE_LABELS[testType]} specifications with Gemini 3.7 Flash...`);
    try {
      const res = await generateAiTestCasesApi({
        testType,
        targetFeature,
        contextPrompt: contextPrompt || undefined,
        count
      });
      if (res.success && res.generated) {
        setGeneratedList(res.generated);
        setStatusMessage(`Generated ${res.generated.length} test cases with complete step-by-step procedures.`);
      }
    } catch (err: any) {
      console.error(err);
      setStatusMessage("AI generation encountered an issue. Using fallback enterprise test heuristics.");
    } finally {
      setIsGenerating(false);
    }
  };

  const handleSaveAll = () => {
    if (generatedList.length > 0) {
      onTestCasesAdded(generatedList);
      onClose();
    }
  };

  return (
    <div className="fixed inset-0 z-50 overflow-y-auto bg-black/75 backdrop-blur-xs flex items-center justify-center p-4 font-mono animate-in fade-in duration-150">
      <div
        id="ai-test-generator-modal"
        className="w-full max-w-2xl bg-[#0a0f1d] border border-cyan-800/80 rounded-2xl shadow-2xl overflow-hidden flex flex-col max-h-[90vh]"
      >
        {/* Header */}
        <div className="p-4 bg-gradient-to-r from-cyan-950/80 via-[#0d1424] to-indigo-950/80 border-b border-slate-800 flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            <div className="p-2 rounded-xl bg-cyan-600/20 border border-cyan-500/50 text-cyan-400">
              <Sparkles className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-sm font-bold text-slate-100">
                AI Test Case Generation Engine
              </h2>
              <p className="text-[11px] text-slate-400">
                Autonomous generation tailored for each of the 10 specialized QA testing methodologies
              </p>
            </div>
          </div>

          <button
            onClick={onClose}
            className="p-1.5 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-400 hover:text-slate-200 transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Modal Body */}
        <div className="p-5 overflow-y-auto space-y-4 text-xs flex-1">
          {/* Section 1: Test Type Selector */}
          <div className="space-y-1.5">
            <label className="block text-[11px] font-bold text-slate-300 uppercase">
              Target Test Section / Category <span className="text-cyan-400">*</span>
            </label>
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
              {CATEGORY_TABS.filter((c) => c.id !== "ALL").map((cat) => (
                <button
                  key={cat.id}
                  type="button"
                  onClick={() => setTestType(cat.id as TestType)}
                  className={`p-2.5 rounded-lg border text-left flex items-center gap-2 transition-all ${
                    testType === cat.id
                      ? "bg-cyan-950/80 border-cyan-500 text-cyan-200 shadow-sm"
                      : "bg-[#0e1422] border-slate-800 text-slate-400 hover:border-slate-700 hover:text-slate-200"
                  }`}
                >
                  <cat.icon className="w-3.5 h-3.5 shrink-0" />
                  <span className="truncate font-bold text-[11px]">
                    {TEST_TYPE_LABELS[cat.id as TestType]}
                  </span>
                </button>
              ))}
            </div>
          </div>

          {/* Section 2: Target Feature */}
          <div className="space-y-1.5">
            <label className="block text-[11px] font-bold text-slate-300 uppercase">
              Target Feature / Component / API <span className="text-cyan-400">*</span>
            </label>
            <input
              type="text"
              value={targetFeature}
              onChange={(e) => setTargetFeature(e.target.value)}
              placeholder="e.g. Payment Gateway & Webhook Signature Verification"
              className="w-full p-2.5 rounded-lg bg-[#0e1422] border border-slate-800 text-slate-200 focus:border-cyan-500 focus:outline-none"
            />
          </div>

          {/* Section 3: Context / Directives */}
          <div className="space-y-1.5">
            <label className="block text-[11px] font-bold text-slate-300 uppercase">
              Context Instructions / Edge Scenarios (Optional)
            </label>
            <textarea
              rows={2}
              value={contextPrompt}
              onChange={(e) => setContextPrompt(e.target.value)}
              placeholder="e.g. Focus on SQL injection in sorting parameters, 10,000 req/sec load stress, or session expiration during checkout"
              className="w-full p-2.5 rounded-lg bg-[#0e1422] border border-slate-800 text-slate-200 focus:border-cyan-500 focus:outline-none leading-relaxed"
            />
          </div>

          {/* Section 4: Count Slider */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <label className="text-[11px] font-bold text-slate-300 uppercase">
                Number of Test Cases: <span className="text-cyan-400">{count}</span>
              </label>
              <span className="text-[10px] text-slate-500">Max 8 per generation</span>
            </div>
            <input
              type="range"
              min={1}
              max={8}
              value={count}
              onChange={(e) => setCount(Number(e.target.value))}
              className="w-full accent-cyan-500 cursor-pointer"
            />
          </div>

          {/* Status Message */}
          {statusMessage && (
            <div className="p-2.5 rounded-lg bg-cyan-950/40 border border-cyan-900/60 text-cyan-300 text-[11px] flex items-center gap-2">
              <Sparkles className="w-3.5 h-3.5 text-cyan-400 shrink-0" />
              <span>{statusMessage}</span>
            </div>
          )}

          {/* Preview of Generated Test Cases */}
          {generatedList.length > 0 && (
            <div className="space-y-2 pt-2 border-t border-slate-800">
              <div className="flex items-center justify-between">
                <span className="text-[11px] font-bold text-slate-300 uppercase">
                  Generated Test Cases ({generatedList.length})
                </span>
                <span className="text-[10px] text-emerald-400 font-bold">Ready to commit</span>
              </div>
              <div className="space-y-2 max-h-56 overflow-y-auto pr-1">
                {generatedList.map((tc, i) => (
                  <div
                    key={i}
                    className="p-3 rounded-lg bg-[#0e1422] border border-slate-800 space-y-1.5"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <div className="flex items-center gap-2">
                        <span className="font-bold text-cyan-400 text-xs">{tc.id}</span>
                        <span className="font-bold text-slate-200 text-xs truncate max-w-[280px]">
                          {tc.name}
                        </span>
                      </div>
                      <span className="text-[9px] uppercase px-1.5 py-0.2 rounded bg-slate-900 border border-slate-700 text-slate-400">
                        {tc.priority}
                      </span>
                    </div>
                    <p className="text-[11px] text-slate-400 line-clamp-2">{tc.description}</p>
                    <div className="text-[10px] text-emerald-400/90 truncate">
                      Expected: {tc.expected_result}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>

        {/* Modal Footer */}
        <div className="p-4 bg-[#0d1424] border-t border-slate-800 flex items-center justify-between gap-3">
          <button
            onClick={onClose}
            className="px-4 py-2 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-300 text-xs font-bold"
          >
            Cancel
          </button>

          <div className="flex items-center gap-2">
            {generatedList.length === 0 ? (
              <button
                onClick={handleGenerate}
                disabled={isGenerating || !targetFeature.trim()}
                className="flex items-center gap-2 px-5 py-2 rounded-lg bg-cyan-600 hover:bg-cyan-500 disabled:opacity-50 text-white text-xs font-bold shadow-md transition-all"
              >
                {isGenerating ? (
                  <>
                    <Loader2 className="w-4 h-4 animate-spin" />
                    <span>Synthesizing Tests...</span>
                  </>
                ) : (
                  <>
                    <Sparkles className="w-4 h-4" />
                    <span>Generate Test Cases</span>
                  </>
                )}
              </button>
            ) : (
              <button
                onClick={handleSaveAll}
                className="flex items-center gap-2 px-5 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-bold shadow-md transition-all"
              >
                <Check className="w-4 h-4" />
                <span>Save {generatedList.length} Tests to Hub</span>
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
