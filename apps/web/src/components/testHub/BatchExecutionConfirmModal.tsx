import React from "react";
import { AlertTriangle, Play, X, Shield, Clock, Cpu } from "lucide-react";
import { TestType } from "../../types";
import { TEST_TYPE_LABELS } from "./TestCategoryNav";

interface BatchExecutionConfirmModalProps {
  isOpen: boolean;
  onClose: () => void;
  onConfirm: () => void;
  testCount: number;
  testType: TestType | "ALL" | "SELECTED";
  environment: string;
  isExecuting?: boolean;
}

export const BatchExecutionConfirmModal: React.FC<BatchExecutionConfirmModalProps> = ({
  isOpen,
  onClose,
  onConfirm,
  testCount,
  testType,
  environment,
  isExecuting = false
}) => {
  if (!isOpen) return null;

  const scopeLabel =
    testType === "ALL"
      ? "All Test Categories (Full Regression Suite)"
      : testType === "SELECTED"
      ? `${testCount} Selected Test Cases`
      : TEST_TYPE_LABELS[testType as TestType] || testType;

  return (
    <div className="fixed inset-0 z-50 overflow-y-auto bg-black/75 backdrop-blur-xs flex items-center justify-center p-4 font-mono animate-in fade-in duration-150">
      <div
        id="batch-execution-confirm-modal"
        className="w-full max-w-md bg-[#0a0f1d] border border-amber-800/80 rounded-2xl shadow-2xl overflow-hidden flex flex-col"
      >
        {/* Header */}
        <div className="p-4 bg-gradient-to-r from-amber-950/70 via-[#0d1424] to-slate-900 border-b border-slate-800 flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            <div className="p-2 rounded-xl bg-amber-600/20 border border-amber-500/50 text-amber-400">
              <AlertTriangle className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-sm font-bold text-slate-100">
                Confirm Batch Test Execution
              </h2>
              <p className="text-[10px] text-slate-400">
                Distributed cluster runner dispatch validation
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

        {/* Content Body */}
        <div className="p-5 space-y-4 text-xs">
          <p className="text-slate-300 leading-relaxed">
            You are about to trigger execution for{" "}
            <strong className="text-cyan-400">{testCount} test cases</strong> across the distributed
            runner cluster.
          </p>

          <div className="p-3.5 rounded-xl bg-[#0e1422] border border-slate-800 space-y-2.5">
            <div className="flex items-center justify-between text-slate-300">
              <span className="text-slate-500">Execution Scope:</span>
              <strong className="text-slate-100 text-right truncate max-w-[200px]">
                {scopeLabel}
              </strong>
            </div>

            <div className="flex items-center justify-between text-slate-300">
              <span className="text-slate-500">Target Environment:</span>
              <span className="px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-emerald-400 font-bold uppercase text-[11px]">
                {environment}
              </span>
            </div>

            <div className="flex items-center justify-between text-slate-300">
              <span className="text-slate-500">Estimated Duration:</span>
              <span className="text-slate-200 flex items-center gap-1 font-bold">
                <Clock className="w-3.5 h-3.5 text-cyan-400" />
                ~{Math.max(2, Math.round(testCount * 0.4))} seconds
              </span>
            </div>

            <div className="flex items-center justify-between text-slate-300">
              <span className="text-slate-500">Distributed Workers:</span>
              <span className="text-slate-200 flex items-center gap-1 font-bold">
                <Cpu className="w-3.5 h-3.5 text-cyan-400" />
                4 Parallel Workers
              </span>
            </div>
          </div>

          <div className="p-2.5 rounded-lg bg-amber-950/30 border border-amber-900/50 text-amber-300 text-[11px] flex items-center gap-2">
            <Shield className="w-4 h-4 text-amber-400 shrink-0" />
            <span>Zero-Trust audit event will be recorded with your user credentials.</span>
          </div>
        </div>

        {/* Footer */}
        <div className="p-4 bg-[#0d1424] border-t border-slate-800 flex items-center justify-end gap-3">
          <button
            onClick={onClose}
            disabled={isExecuting}
            className="px-4 py-2 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-300 text-xs font-bold"
          >
            Cancel
          </button>

          <button
            onClick={onConfirm}
            disabled={isExecuting}
            className="flex items-center gap-2 px-5 py-2 rounded-lg bg-cyan-600 hover:bg-cyan-500 disabled:opacity-50 text-white text-xs font-bold shadow-md transition-all"
          >
            <Play className="w-4 h-4 fill-current" />
            <span>{isExecuting ? "Dispatching..." : "Confirm & Execute"}</span>
          </button>
        </div>
      </div>
    </div>
  );
};
