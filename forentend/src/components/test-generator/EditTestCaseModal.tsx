/**
 * Edit Test Case — closes a real, verified gap: the backend has always
 * supported editing a test case's descriptive fields
 * (`api.rs::update_test_case_scoped`, `PATCH .../test-cases/:id`) and the
 * frontend API client already had `aiqaApi.testCases.update` wired to it,
 * but no UI anywhere ever called it — the only way to fix a wrong AI-
 * generated case was to Reject it and regenerate from scratch, discarding
 * whatever WAS right about it.
 *
 * Reuses the exact overlay/card visual pattern `SkillManagerModal.tsx`
 * already established (the only modal precedent in this codebase) rather
 * than inventing a second design language.
 *
 * `onSave` is the real save — a real backend PATCH for an already-
 * persisted case, or a local draft mutation for one that is not yet saved
 * (`TestCaseGeneratorPage.tsx` decides which, exactly like every other
 * approve/reject/validate action already branches on `hasPersisted`). This
 * modal itself never assumes which one happened; it only reports a real
 * failure if `onSave` rejects, and never closes on a failure.
 */
import React, { useState } from 'react';
import { X, Plus, Trash2, Loader2, AlertTriangle, Pencil } from 'lucide-react';
import type { GeneratedTestCase } from '../../types';

export interface EditTestCaseUpdates {
  title: string;
  description: string;
  priority: GeneratedTestCase['priority'];
  preconditions: string[];
  steps: Array<{ stepNumber: number; action: string; expectedResult: string }>;
  expectedResult: string;
}

interface EditTestCaseModalProps {
  testCase: GeneratedTestCase;
  onClose: () => void;
  onSave: (updates: EditTestCaseUpdates) => Promise<void>;
}

const PRIORITIES: GeneratedTestCase['priority'][] = ['CRITICAL', 'HIGH', 'MEDIUM', 'LOW'];

interface DraftStep {
  action: string;
  expectedResult: string;
}

export const EditTestCaseModal: React.FC<EditTestCaseModalProps> = ({ testCase, onClose, onSave }) => {
  const [title, setTitle] = useState(testCase.title);
  const [description, setDescription] = useState(testCase.description);
  const [priority, setPriority] = useState<GeneratedTestCase['priority']>(testCase.priority);
  const [preconditions, setPreconditions] = useState<string[]>([...testCase.preconditions]);
  const [steps, setSteps] = useState<DraftStep[]>(
    testCase.steps.map((s) => ({ action: s.action, expectedResult: s.expectedResult })),
  );
  const [expectedResult, setExpectedResult] = useState(testCase.expectedResult ?? '');
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const updatePrecondition = (idx: number, value: string) =>
    setPreconditions((prev) => prev.map((p, i) => (i === idx ? value : p)));
  const removePrecondition = (idx: number) => setPreconditions((prev) => prev.filter((_, i) => i !== idx));
  const addPrecondition = () => setPreconditions((prev) => [...prev, '']);

  const updateStep = (idx: number, patch: Partial<DraftStep>) =>
    setSteps((prev) => prev.map((s, i) => (i === idx ? { ...s, ...patch } : s)));
  const removeStep = (idx: number) => setSteps((prev) => prev.filter((_, i) => i !== idx));
  const addStep = () => setSteps((prev) => [...prev, { action: '', expectedResult: '' }]);

  const canSave =
    title.trim().length > 0 &&
    steps.length > 0 &&
    steps.every((s) => s.action.trim().length > 0 && s.expectedResult.trim().length > 0) &&
    !saving;

  const handleSave = async () => {
    if (!canSave) return;
    setSaving(true);
    setSaveError(null);
    try {
      await onSave({
        title: title.trim(),
        description: description.trim(),
        priority,
        preconditions: preconditions.map((p) => p.trim()).filter((p) => p.length > 0),
        steps: steps.map((s, idx) => ({
          stepNumber: idx + 1,
          action: s.action.trim(),
          expectedResult: s.expectedResult.trim(),
        })),
        expectedResult: expectedResult.trim(),
      });
      onClose();
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : 'Failed to save this test case.');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-10 bg-slate-950/80 backdrop-blur-sm p-4 animate-in fade-in duration-100"
      onClick={onClose}
    >
      <div
        id="edit-test-case-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Edit Test Case"
        className="w-full max-w-2xl rounded-lg bg-[#0B1120] border border-slate-700/80 shadow-2xl overflow-hidden flex flex-col max-h-[85vh]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-slate-800 bg-[#020617]">
          <div className="flex items-center gap-2">
            <Pencil className="w-4 h-4 text-indigo-400" />
            <div>
              <h2 className="text-sm font-bold text-slate-100 font-sans">Edit Test Case</h2>
              <p className="text-[11px] text-slate-500 font-sans">
                Fix a specific field without discarding and regenerating the whole case.
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close Edit Test Case"
            disabled={saving}
            className="p-1.5 text-slate-500 hover:text-slate-300 rounded cursor-pointer disabled:opacity-50"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Form body */}
        <div className="flex-1 overflow-y-auto p-4 space-y-4">
          <div className="space-y-1.5">
            <label className="text-[10px] font-mono text-slate-400 uppercase tracking-wider block" htmlFor="edit-tc-title">
              Title
            </label>
            <input
              id="edit-tc-title"
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              disabled={saving}
              className="w-full px-2.5 py-1.5 rounded bg-[#020617] border border-slate-700 text-xs font-sans text-slate-100 focus:outline-none focus:border-indigo-500 disabled:opacity-50"
            />
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
            <div className="sm:col-span-2 space-y-1.5">
              <label className="text-[10px] font-mono text-slate-400 uppercase tracking-wider block" htmlFor="edit-tc-description">
                Description
              </label>
              <textarea
                id="edit-tc-description"
                rows={2}
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                disabled={saving}
                className="w-full px-2.5 py-1.5 rounded bg-[#020617] border border-slate-700 text-xs font-sans text-slate-100 focus:outline-none focus:border-indigo-500 resize-none disabled:opacity-50"
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-[10px] font-mono text-slate-400 uppercase tracking-wider block" htmlFor="edit-tc-priority">
                Priority
              </label>
              <select
                id="edit-tc-priority"
                value={priority}
                onChange={(e) => setPriority(e.target.value as GeneratedTestCase['priority'])}
                disabled={saving}
                className="w-full px-2.5 py-1.5 rounded bg-[#020617] border border-slate-700 text-xs font-mono text-slate-100 focus:outline-none focus:border-indigo-500 cursor-pointer disabled:opacity-50"
              >
                {PRIORITIES.map((p) => (
                  <option key={p} value={p}>
                    {p}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <span className="text-[10px] font-mono text-slate-400 uppercase tracking-wider">Preconditions</span>
              <button
                type="button"
                onClick={addPrecondition}
                disabled={saving}
                className="flex items-center gap-1 text-[10px] font-mono text-indigo-400 hover:text-indigo-300 disabled:opacity-50 cursor-pointer"
              >
                <Plus className="w-3 h-3" />
                Add
              </button>
            </div>
            {preconditions.length === 0 ? (
              <p className="text-[11px] text-slate-500 font-sans italic">None.</p>
            ) : (
              <div className="space-y-1.5">
                {preconditions.map((p, idx) => (
                  <div key={idx} id={`edit-precondition-${idx}`} className="flex items-center gap-1.5">
                    <input
                      type="text"
                      value={p}
                      onChange={(e) => updatePrecondition(idx, e.target.value)}
                      disabled={saving}
                      className="flex-1 px-2.5 py-1.5 rounded bg-[#020617] border border-slate-700 text-xs font-sans text-slate-100 focus:outline-none focus:border-indigo-500 disabled:opacity-50"
                    />
                    <button
                      type="button"
                      title="Remove precondition"
                      onClick={() => removePrecondition(idx)}
                      disabled={saving}
                      className="p-1 rounded text-slate-500 hover:text-rose-400 disabled:opacity-50 cursor-pointer shrink-0"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <span className="text-[10px] font-mono text-slate-400 uppercase tracking-wider">
                Execution Steps (at least 1 required)
              </span>
              <button
                type="button"
                onClick={addStep}
                disabled={saving}
                className="flex items-center gap-1 text-[10px] font-mono text-indigo-400 hover:text-indigo-300 disabled:opacity-50 cursor-pointer"
              >
                <Plus className="w-3 h-3" />
                Add step
              </button>
            </div>
            <div className="space-y-2">
              {steps.map((step, idx) => (
                <div key={idx} id={`edit-step-${idx}`} className="p-2.5 rounded bg-[#020617] border border-slate-800 space-y-1.5">
                  <div className="flex items-center justify-between">
                    <span className="text-[10px] font-mono text-slate-500">Step #{idx + 1}</span>
                    <button
                      type="button"
                      title="Remove step"
                      onClick={() => removeStep(idx)}
                      disabled={saving}
                      className="p-1 rounded text-slate-500 hover:text-rose-400 disabled:opacity-50 cursor-pointer"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                  <input
                    type="text"
                    placeholder="Action"
                    value={step.action}
                    onChange={(e) => updateStep(idx, { action: e.target.value })}
                    disabled={saving}
                    className="w-full px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-sans text-slate-100 placeholder-slate-500 focus:outline-none focus:border-indigo-500 disabled:opacity-50"
                  />
                  <input
                    type="text"
                    placeholder="Expected result"
                    value={step.expectedResult}
                    onChange={(e) => updateStep(idx, { expectedResult: e.target.value })}
                    disabled={saving}
                    className="w-full px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-sans text-emerald-300 placeholder-slate-500 focus:outline-none focus:border-emerald-600 disabled:opacity-50"
                  />
                </div>
              ))}
              {steps.length === 0 && (
                <p className="text-[11px] text-rose-300 font-sans italic">
                  At least one step is required — add one before saving.
                </p>
              )}
            </div>
          </div>

          <div className="space-y-1.5">
            <label className="text-[10px] font-mono text-slate-400 uppercase tracking-wider block" htmlFor="edit-tc-expected-result">
              Overall Expected Result
            </label>
            <textarea
              id="edit-tc-expected-result"
              rows={2}
              value={expectedResult}
              onChange={(e) => setExpectedResult(e.target.value)}
              disabled={saving}
              className="w-full px-2.5 py-1.5 rounded bg-[#020617] border border-slate-700 text-xs font-sans text-slate-100 focus:outline-none focus:border-indigo-500 resize-none disabled:opacity-50"
            />
          </div>

          {saveError && (
            <p className="text-[11px] font-mono text-rose-300 flex items-start gap-1.5">
              <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
              <span>{saveError}</span>
            </p>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center gap-2 px-4 py-3 border-t border-slate-800 bg-[#020617]">
          <button
            type="button"
            id="btn-save-test-case-edit"
            onClick={() => void handleSave()}
            disabled={!canSave}
            className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed text-white font-mono text-xs font-bold cursor-pointer"
          >
            {saving && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
            {saving ? 'Saving…' : 'Save Changes'}
          </button>
          <button
            type="button"
            onClick={onClose}
            disabled={saving}
            className="px-3.5 py-1.5 rounded-md bg-[#0B1120] border border-slate-700 text-slate-300 font-mono text-xs cursor-pointer disabled:opacity-50"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
};
