import React, { useState } from 'react';
import type { GenerationWaveSummary } from '../../types/contract';

interface GenerationWaveResultsProps {
  waves: GenerationWaveSummary[];
  /** Turns a provider id into the name to show; the id itself when not given. */
  providerName?: (id: string) => string;
}

/** How many calls are listed before the rest are folded away. A run from
 *  requirements makes one call per skill per batch, which can be a hundred. */
const ROWS_SHOWN = 12;

const skillLabel = (w: GenerationWaveSummary): string =>
  w.skill_names.length > 0 ? w.skill_names.join(', ') : 'No skill selected';

/** A call's skill, and — for a call from requirements — the batch it carried. */
const callLabel = (w: GenerationWaveSummary): string => (w.batch ? `${skillLabel(w)} (batch ${w.batch})` : skillLabel(w));

const batchLabel = (w: GenerationWaveSummary): string =>
  `batch ${w.batch}${w.requirements !== undefined ? ` · ${w.requirements} ${w.requirements === 1 ? 'requirement' : 'requirements'}` : ''}`;

const tokensLabel = (w: GenerationWaveSummary): string =>
  w.prompt_tokens !== null && w.completion_tokens !== null
    ? `${w.prompt_tokens} in / ${w.completion_tokens} out`
    : 'Tokens not reported';

const latencyLabel = (w: GenerationWaveSummary): string =>
  w.latency_ms !== null ? `${(w.latency_ms / 1000).toFixed(1)}s` : 'Time not reported';

/** One row per skill call of the last generation: which provider/model served
 *  it, or why it failed. A failed call means that skill contributed no cases
 *  to the run, which the overall "Completed" state alone does not say. */
export const GenerationWaveResults: React.FC<GenerationWaveResultsProps> = ({ waves, providerName = (id) => id }) => {
  const [showAll, setShowAll] = useState(false);
  if (waves.length === 0) return null;
  const failed = waves.filter((w) => w.failure_reason !== null);

  // Past ROWS_SHOWN calls the list is folded, failures first: the calls that
  // went wrong are the ones worth reading.
  const folded = waves.length > ROWS_SHOWN && !showAll;
  const rows = waves.map((w, i) => ({ w, i }));
  const shown = folded
    ? [...rows.filter((r) => r.w.failure_reason !== null), ...rows.filter((r) => r.w.failure_reason === null)].slice(0, ROWS_SHOWN)
    : rows;

  return (
    <div id="generation-wave-results" className="pt-2 border-t border-slate-800/80 space-y-1.5">
      <div className="text-slate-500">
        Per-skill results: {waves.length - failed.length} of {waves.length} succeeded
      </div>
      {failed.length > 0 && (
        <div
          id="generation-wave-warning"
          className="p-2 rounded bg-amber-950/40 border border-amber-800/80 text-amber-300"
        >
          {failed.length === 1 ? '1 skill call failed' : `${failed.length} skill calls failed`} — this run has no
          cases for: {failed.map(callLabel).join('; ')}. Generate again to retry.
        </div>
      )}
      <ul className="space-y-1">
        {shown.map(({ w, i }) => (
          <li
            key={`${w.wave}-${i}`}
            className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 text-slate-300"
          >
            <span className={`font-bold ${w.failure_reason !== null ? 'text-rose-300' : 'text-emerald-300'}`}>
              {w.failure_reason !== null ? 'FAILED' : 'OK'}
            </span>
            <span className="text-slate-100">{skillLabel(w)}</span>
            {w.batch && <span className="text-slate-400">{batchLabel(w)}</span>}
            {w.failure_reason !== null ? (
              <span className="text-rose-300 break-words min-w-0">{w.failure_reason}</span>
            ) : (
              <>
                <span>
                  {w.case_count} {w.case_count === 1 ? 'case' : 'cases'}
                </span>
                <span>{[w.provider ? providerName(w.provider) : null, w.model].filter(Boolean).join(' / ') || 'Provider not reported'}</span>
                {w.fallback_used === true && <span className="text-amber-300">fallback</span>}
                <span>{tokensLabel(w)}</span>
                <span>{latencyLabel(w)}</span>
              </>
            )}
          </li>
        ))}
      </ul>
      {waves.length > ROWS_SHOWN && (
        <button
          type="button"
          id="btn-toggle-wave-rows"
          onClick={() => setShowAll((v) => !v)}
          className="text-violet-400 hover:text-violet-300 cursor-pointer"
        >
          {showAll ? 'Show fewer calls' : `Show all ${waves.length} calls`}
        </button>
      )}
    </div>
  );
};
