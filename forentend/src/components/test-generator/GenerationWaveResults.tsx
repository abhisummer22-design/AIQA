import React from 'react';
import type { GenerationWaveSummary } from '../../types/contract';

interface GenerationWaveResultsProps {
  waves: GenerationWaveSummary[];
}

const skillLabel = (w: GenerationWaveSummary): string =>
  w.skill_names.length > 0 ? w.skill_names.join(', ') : 'No skill selected';

const tokensLabel = (w: GenerationWaveSummary): string =>
  w.prompt_tokens !== null && w.completion_tokens !== null
    ? `${w.prompt_tokens} in / ${w.completion_tokens} out`
    : 'Tokens not reported';

const latencyLabel = (w: GenerationWaveSummary): string =>
  w.latency_ms !== null ? `${(w.latency_ms / 1000).toFixed(1)}s` : 'Time not reported';

/** One row per skill call of the last generation: which provider/model served
 *  it, or why it failed. A failed call means that skill contributed no cases
 *  to the run, which the overall "Completed" state alone does not say. */
export const GenerationWaveResults: React.FC<GenerationWaveResultsProps> = ({ waves }) => {
  if (waves.length === 0) return null;
  const failed = waves.filter((w) => w.failure_reason !== null);

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
          cases for: {failed.map(skillLabel).join('; ')}. Generate again to retry.
        </div>
      )}
      <ul className="space-y-1">
        {waves.map((w, i) => (
          <li
            key={`${w.wave}-${i}`}
            className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 text-slate-300"
          >
            <span className={`font-bold ${w.failure_reason !== null ? 'text-rose-300' : 'text-emerald-300'}`}>
              {w.failure_reason !== null ? 'FAILED' : 'OK'}
            </span>
            <span className="text-slate-100">{skillLabel(w)}</span>
            {w.failure_reason !== null ? (
              <span className="text-rose-300 break-words min-w-0">{w.failure_reason}</span>
            ) : (
              <>
                <span>
                  {w.case_count} {w.case_count === 1 ? 'case' : 'cases'}
                </span>
                <span>{[w.provider, w.model].filter(Boolean).join(' / ') || 'Provider not reported'}</span>
                {w.fallback_used === true && <span className="text-amber-300">fallback</span>}
                <span>{tokensLabel(w)}</span>
                <span>{latencyLabel(w)}</span>
              </>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
};
