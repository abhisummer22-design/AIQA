import React from 'react';
import { AlertTriangle, CheckCircle2 } from 'lucide-react';
import type { RequirementCoverage } from '../../types/contract';

interface RequirementCoverageSummaryProps {
  coverage: RequirementCoverage;
}

/** How many ids of the requirements left without a case are named. */
const NAMED_KEYS = 8;

const plural = (n: number, one: string, many: string): string => (n === 1 ? one : many);

const namedKeys = (keys: string[], count: number): string =>
  `${keys.slice(0, NAMED_KEYS).join(', ')}${count > NAMED_KEYS ? ` and ${(count - NAMED_KEYS).toLocaleString()} more` : ''}`;

/** What a requirement-driven run reached of the requirement set: how many
 *  requirements have at least one case, which were never shown to any skill
 *  (a kind no selected skill tests) and which were shown but got no case (a
 *  failed call, or nothing to test in them). Every figure is the backend's own
 *  count for that run — a run is never reported as complete on the strength of
 *  its cases alone. */
export const RequirementCoverageSummary: React.FC<RequirementCoverageSummaryProps> = ({ coverage }) => {
  const notShown = coverage.total - coverage.offered;
  const noCase = coverage.offered - coverage.covered;

  return (
    <div id="requirement-coverage" className="pt-2 border-t border-slate-800/80 space-y-1.5">
      <div className="text-slate-300">
        <span className="text-slate-500">Requirements: </span>
        {coverage.covered.toLocaleString()} of {coverage.total.toLocaleString()} {plural(coverage.covered, 'has', 'have')} a case ·{' '}
        {coverage.offered.toLocaleString()} {plural(coverage.offered, 'was', 'were')} shown to a skill ·{' '}
        {coverage.calls.toLocaleString()} AI {plural(coverage.calls, 'call', 'calls')} (about{' '}
        {coverage.budget_chars.toLocaleString()} characters of requirements each)
      </div>

      {notShown <= 0 && noCase <= 0 && (
        <div className="flex items-center gap-1.5 text-emerald-300">
          <CheckCircle2 className="w-3 h-3 shrink-0" />
          <span>Every requirement has at least one case.</span>
        </div>
      )}

      {notShown > 0 && (
        <div className="p-2 rounded bg-amber-950/40 border border-amber-800/80 text-amber-300 flex items-start gap-1.5">
          <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
          <span>
            {notShown.toLocaleString()} {plural(notShown, 'requirement is', 'requirements are')} of a kind none of the
            selected skills tests, so {plural(notShown, 'it was', 'they were')} not shown to any:{' '}
            {namedKeys(coverage.not_offered_keys, notShown)}. Select a skill that covers{' '}
            {plural(notShown, 'it', 'them')} and generate again.
          </span>
        </div>
      )}

      {noCase > 0 && (
        <div className="p-2 rounded bg-amber-950/40 border border-amber-800/80 text-amber-300 flex items-start gap-1.5">
          <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
          <span>
            {noCase.toLocaleString()} {plural(noCase, 'requirement was', 'requirements were')} shown to a skill but got no
            case (a failed call, or nothing in {plural(noCase, 'it', 'them')} to test):{' '}
            {namedKeys(coverage.uncovered_keys, noCase)}.
          </span>
        </div>
      )}

      {coverage.notes.length > 0 && (
        <ul className="space-y-0.5 text-slate-400">
          {coverage.notes.map((note, i) => (
            <li key={i}>{note}</li>
          ))}
        </ul>
      )}
    </div>
  );
};
