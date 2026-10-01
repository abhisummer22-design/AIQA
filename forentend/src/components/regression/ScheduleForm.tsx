/**
 * Phase 8C — making a schedule: what it repeats (the cases of a suite, or of the whole project), how often, when it first runs, how many
 * cases at a time, and whether it starts by itself. Beside it, what it would repeat now and what it could not, from the service.
 */
import React from 'react';
import { Loader2 } from 'lucide-react';
import type { RegressionScope, TestSuiteRow } from '../../types/contract';
import { CUSTOM_EVERY, EVERY_CHOICES, MAX_CONCURRENCY, MAX_EVERY_MINUTES, MIN_EVERY_MINUTES, scopeText, type ScheduleFormState } from './regressionModel';

export type ScopeState = { status: 'loading' } | { status: 'ready'; scope: RegressionScope } | { status: 'failed'; message: string };

const INPUT =
  'w-full px-2 py-1.5 rounded-md bg-[#020617] border border-slate-700 text-xs text-slate-100 placeholder:text-content-muted focus:outline-none focus:border-violet-500 disabled:opacity-50';

export const ScheduleForm: React.FC<{
  form: ScheduleFormState;
  onChange: (next: ScheduleFormState) => void;
  problems: string[];
  suites: TestSuiteRow[];
  scope: ScopeState;
  onSubmit: () => void;
  submitting: boolean;
}> = ({ form, onChange, problems, suites, scope, onSubmit, submitting }) => {
  const set = (patch: Partial<ScheduleFormState>) => onChange({ ...form, ...patch });
  return (
    <form
      id="regression-form"
      onSubmit={(e) => {
        e.preventDefault();
        if (problems.length === 0 && !submitting) onSubmit();
      }}
      className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 space-y-4"
    >
      <h2 className="text-sm font-semibold text-slate-100">New schedule</h2>

      <label className="block space-y-1">
        <span className="text-xs font-semibold text-slate-100">Name</span>
        <input id="regression-name" value={form.name} onChange={(e) => set({ name: e.target.value })} placeholder="Nightly regression" className={INPUT} />
      </label>

      <div className="space-y-1">
        <label htmlFor="regression-suite" className="text-xs font-semibold text-slate-100">
          What it repeats
        </label>
        <select id="regression-suite" value={form.suiteId} onChange={(e) => set({ suiteId: e.target.value })} className={INPUT}>
          <option value="">Every case of the project that has been run once</option>
          {suites.map((s) => (
            <option key={s.id} value={s.id}>
              Suite: {s.name}
            </option>
          ))}
        </select>
        <div id="regression-scope" className="text-[11px] text-slate-400" aria-live="polite">
          {scope.status === 'loading' && (
            <span className="inline-flex items-center gap-1">
              <Loader2 className="w-3 h-3 animate-spin" /> Looking at the cases…
            </span>
          )}
          {scope.status === 'failed' && <span className="text-amber-300">The cases could not be looked at: {scope.message}</span>}
          {scope.status === 'ready' && (
            <>
              <span id="regression-scope-text">{scopeText(scope.scope)}</span>
              {scope.scope.skipped.length > 0 && (
                <details id="regression-scope-skipped" className="mt-1">
                  <summary className="cursor-pointer text-content-muted">Why some cannot</summary>
                  <ul className="mt-1 space-y-0.5">
                    {scope.scope.skipped.map((s) => (
                      <li key={s.case_id}>
                        <span className="text-slate-300">{s.title}</span>: {s.reason}
                      </li>
                    ))}
                  </ul>
                </details>
              )}
            </>
          )}
        </div>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1">
          <label htmlFor="regression-every" className="text-xs font-semibold text-slate-100">
            How often
          </label>
          <select id="regression-every" value={form.every} onChange={(e) => set({ every: e.target.value })} className={INPUT}>
            {EVERY_CHOICES.map((c) => (
              <option key={c.minutes} value={String(c.minutes)}>
                {c.label}
              </option>
            ))}
            <option value={CUSTOM_EVERY}>Another time (in minutes)</option>
          </select>
          {form.every === CUSTOM_EVERY && (
            <input
              id="regression-custom"
              type="number"
              min={MIN_EVERY_MINUTES}
              max={MAX_EVERY_MINUTES}
              aria-label="Minutes between runs"
              value={form.customMinutes}
              onChange={(e) => set({ customMinutes: e.target.valueAsNumber })}
              className={INPUT}
            />
          )}
        </div>
        <label className="block space-y-1">
          <span className="text-xs font-semibold text-slate-100">First run at (optional)</span>
          <input id="regression-start" type="datetime-local" value={form.startAt} onChange={(e) => set({ startAt: e.target.value })} className={INPUT} />
          <span className="block text-[10px] text-content-muted">Your clock. Empty: one interval from now. Later runs keep this hour.</span>
        </label>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <label className="block space-y-1">
          <span className="text-xs font-semibold text-slate-100">Cases at a time</span>
          <input
            id="regression-concurrency"
            type="number"
            min={1}
            max={MAX_CONCURRENCY}
            value={form.concurrency}
            onChange={(e) => set({ concurrency: e.target.valueAsNumber })}
            className={INPUT}
          />
        </label>
        <label className="flex items-start gap-2 text-[11px] text-slate-300 cursor-pointer sm:mt-5">
          <input id="regression-enabled" type="checkbox" checked={form.enabled} onChange={(e) => set({ enabled: e.target.checked })} className="mt-0.5 accent-violet-500" />
          <span>
            Start it by itself
            <span className="block text-content-muted">Untick to run it only when asked (a CI job runs it that way).</span>
          </span>
        </label>
      </div>

      {problems.length > 0 && (
        <ul id="regression-problems" className="space-y-0.5">
          {problems.map((p) => (
            <li key={p} className="text-[11px] text-amber-300">
              {p}
            </li>
          ))}
        </ul>
      )}

      <button
        type="submit"
        id="regression-create"
        disabled={problems.length > 0 || submitting}
        className="px-3 py-1.5 rounded-md bg-violet-600 text-xs font-semibold text-white hover:bg-violet-500 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {submitting ? 'Making the schedule…' : 'Make the schedule'}
      </button>
    </form>
  );
};
