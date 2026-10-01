import React from 'react';
import { AlertTriangle, CheckCircle2 } from 'lucide-react';
import type { CoverageMatrix } from '../../types/contract';
import { DEPTH_LABEL, DEPTH_MEANING, techniqueLabel, techniqueSkill } from './coverageLabels';

interface CoverageMatrixSummaryProps {
  matrix: CoverageMatrix;
  /** Requirements the run was generated from. */
  total: number;
}

/** How many requirements with a gap are named. */
const NAMED_GAPS = 6;

const plural = (n: number, one: string, many: string): string => (n === 1 ? one : many);

/** What a requirement-driven run covered, by technique: how many of the
 *  requirements each technique applies to got the number of cases the depth
 *  asks for, which technique no selected skill provides (and so was never
 *  attempted), what a top-up added, and which requirements are still short.
 *  Every figure is the backend's own count for that run. */
export const CoverageMatrixSummary: React.FC<CoverageMatrixSummaryProps> = ({ matrix, total }) => {
  const testable = total - matrix.not_applicable;
  const gapsShown = matrix.gaps.slice(0, NAMED_GAPS);

  return (
    <div id="coverage-matrix" className="pt-2 border-t border-slate-800/80 space-y-1.5">
      <div className="text-slate-300">
        <span className="text-content-muted">Coverage at {DEPTH_LABEL[matrix.depth] ?? matrix.depth} depth: </span>
        {matrix.complete.toLocaleString()} of {testable.toLocaleString()} testable{' '}
        {plural(testable, 'requirement has', 'requirements have')} every technique that applies
      </div>
      <div className="text-content-muted">
        {DEPTH_MEANING[matrix.depth] ? `This depth asks for ${DEPTH_MEANING[matrix.depth]}.` : null}
      </div>

      <ul className="space-y-0.5">
        {matrix.techniques.map((t) => (
          <li key={t.technique} className="flex flex-wrap items-baseline gap-x-3 text-slate-300">
            <span className="text-slate-100 w-24">{techniqueLabel(t.technique)}</span>
            <span className={t.short === 0 ? 'text-emerald-300' : 'text-amber-300'}>
              {t.met.toLocaleString()} of {t.applicable.toLocaleString()} met
            </span>
            {t.provided_by ? (
              <span className="text-content-muted">via {t.provided_by}</span>
            ) : t.short > 0 ? (
              <span className="text-amber-300">
                not attempted — no selected skill provides it
                {techniqueSkill(t.technique) ? ` (select ${techniqueSkill(t.technique)})` : ''}
              </span>
            ) : null}
          </li>
        ))}
      </ul>

      {matrix.top_up && (
        <div className="text-slate-300">
          <span className="text-content-muted">Top-up: </span>
          {matrix.top_up.rounds} {plural(matrix.top_up.rounds, 'round', 'rounds')}, {matrix.top_up.calls} extra{' '}
          {plural(matrix.top_up.calls, 'call', 'calls')}, {matrix.top_up.cases_added.toLocaleString()}{' '}
          {plural(matrix.top_up.cases_added, 'case', 'cases')} added — stopped because {matrix.top_up.stopped}.
        </div>
      )}

      {matrix.gap_total === 0 && matrix.complete === testable && testable > 0 && (
        <div className="flex items-center gap-1.5 text-emerald-300">
          <CheckCircle2 className="w-3 h-3 shrink-0" />
          <span>Every technique that applies has its cases, for every testable requirement.</span>
        </div>
      )}

      {matrix.gap_total > 0 && (
        <div className="p-2 rounded bg-amber-950/40 border border-amber-800/80 text-amber-300 flex items-start gap-1.5">
          <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
          <span>
            {matrix.gap_total.toLocaleString()} {plural(matrix.gap_total, 'requirement is', 'requirements are')} still short of a
            technique a selected skill provides:{' '}
            {gapsShown
              .map((g) => `${g.key} (${g.missing.map((m) => `${techniqueLabel(m.technique)} ${m.have}/${m.target}`).join(', ')})`)
              .join('; ')}
            {matrix.gap_total > gapsShown.length ? ` and ${(matrix.gap_total - gapsShown.length).toLocaleString()} more` : ''}.
            {matrix.top_up ? '' : ' Turn on "Fill coverage gaps" to have them asked for.'}
          </span>
        </div>
      )}

      {matrix.unlinked_cases > 0 && (
        <div className="text-slate-400">
          {matrix.unlinked_cases.toLocaleString()} {plural(matrix.unlinked_cases, 'case', 'cases')} could not be tied to a
          requirement, so {plural(matrix.unlinked_cases, 'it counts', 'they count')} for none.
        </div>
      )}
      {matrix.not_applicable > 0 && (
        <div className="text-content-muted">
          {matrix.not_applicable.toLocaleString()} {plural(matrix.not_applicable, 'requirement is', 'requirements are')}{' '}
          background or reference text: nothing to test.
        </div>
      )}
    </div>
  );
};
