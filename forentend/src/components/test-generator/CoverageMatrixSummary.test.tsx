import React from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { CoverageMatrixSummary } from './CoverageMatrixSummary';
import type { CoverageMatrix } from '../../types/contract';

function matrix(overrides: Partial<CoverageMatrix> = {}): CoverageMatrix {
  return {
    depth: 'STANDARD',
    techniques: [
      { technique: 'positive', applicable: 100, met: 100, short: 0, provided_by: 'Functional Testing' },
      { technique: 'negative', applicable: 90, met: 90, short: 0, provided_by: 'Negative Testing' },
      { technique: 'edge', applicable: 90, met: 90, short: 0, provided_by: 'Edge Case Analysis' },
    ],
    gaps: [],
    gap_total: 0,
    complete: 100,
    not_applicable: 8,
    unlinked_cases: 0,
    ...overrides,
  };
}

const panel = () => document.getElementById('coverage-matrix') as HTMLElement;
const row = (name: string) => screen.getByText(name).closest('li') as HTMLElement;

describe('CoverageMatrixSummary', () => {
  it('says the depth, how many testable requirements have every technique that applies, and what the depth asks for', () => {
    render(<CoverageMatrixSummary matrix={matrix()} total={108} />);

    expect(panel().textContent).toContain('Coverage at Standard depth: 100 of 100 testable requirements have every technique that applies');
    expect(panel().textContent).toContain('This depth asks for every technique that applies, once.');
  });

  it('describes each depth by what it asks for', () => {
    const { rerender } = render(<CoverageMatrixSummary matrix={matrix({ depth: 'SMOKE' })} total={108} />);
    expect(panel().textContent).toContain('Coverage at Smoke depth');
    expect(panel().textContent).toContain('one case per requirement, of the technique that matters most for it');
    rerender(<CoverageMatrixSummary matrix={matrix({ depth: 'EXHAUSTIVE' })} total={108} />);
    expect(panel().textContent).toContain('every technique that applies, twice');
    rerender(<CoverageMatrixSummary matrix={matrix({ depth: 'CHAOS_EXPLORATORY' })} total={108} />);
    expect(panel().textContent).toContain('Coverage at Chaos / exploratory depth');
    expect(panel().textContent).toContain('three times');
  });

  it('lists each technique with how many requirements met it and which skill provides it', () => {
    render(<CoverageMatrixSummary matrix={matrix()} total={108} />);

    expect(row('Positive').textContent).toContain('100 of 100 met');
    expect(row('Positive').textContent).toContain('via Functional Testing');
    expect(row('Negative').textContent).toContain('90 of 90 met');
    expect(row('Edge').textContent).toContain('via Edge Case Analysis');
  });

  it('says a technique no selected skill provides was not attempted, and which skill to select', () => {
    render(
      <CoverageMatrixSummary
        matrix={matrix({
          techniques: [
            { technique: 'positive', applicable: 10, met: 10, short: 0, provided_by: 'Functional Testing' },
            { technique: 'negative', applicable: 10, met: 0, short: 10, provided_by: null },
            { technique: 'security', applicable: 4, met: 4, short: 0, provided_by: null },
          ],
        })}
        total={20}
      />,
    );

    expect(row('Negative').textContent).toContain('0 of 10 met');
    expect(row('Negative').textContent).toContain('not attempted — no selected skill provides it (select Negative Testing)');
    // Met anyway, so nothing to warn about.
    expect(row('Security').textContent).not.toContain('not attempted');
  });

  it('names a technique it does not know as the backend wrote it', () => {
    render(
      <CoverageMatrixSummary
        matrix={matrix({ techniques: [{ technique: 'chaos' as never, applicable: 3, met: 1, short: 2, provided_by: null }] })}
        total={5}
      />,
    );
    expect(row('chaos').textContent).toContain('1 of 3 met');
    expect(row('chaos').textContent).toContain('not attempted');
    expect(row('chaos').textContent).not.toContain('(select');
  });

  it('reports what a top-up did and why it stopped, in the singular and the plural', () => {
    const { rerender } = render(
      <CoverageMatrixSummary
        matrix={matrix({ top_up: { rounds: 2, calls: 14, cases_added: 63, stopped: 'every target is met' } })}
        total={108}
      />,
    );
    expect(panel().textContent).toContain('Top-up: 2 rounds, 14 extra calls, 63 cases added — stopped because every target is met.');

    rerender(
      <CoverageMatrixSummary
        matrix={matrix({ top_up: { rounds: 1, calls: 1, cases_added: 1, stopped: 'the call limit was reached' } })}
        total={108}
      />,
    );
    expect(panel().textContent).toContain('Top-up: 1 round, 1 extra call, 1 case added — stopped because the call limit was reached.');
  });

  it('shows no top-up line when none ran', () => {
    render(<CoverageMatrixSummary matrix={matrix()} total={108} />);
    expect(panel().textContent).not.toContain('Top-up');
  });

  it('names the requirements still short with what they have of what is wanted, and how many more there are', () => {
    const gaps = Array.from({ length: 10 }, (_, i) => ({
      key: `FR-${i + 1}`,
      kind: 'functional',
      missing: [
        { technique: 'negative' as const, have: 0, target: 1 },
        { technique: 'edge' as const, have: 1, target: 2 },
      ],
    }));
    render(<CoverageMatrixSummary matrix={matrix({ gaps, gap_total: 25, complete: 75 })} total={108} />);

    const text = panel().textContent!;
    expect(text).toContain('25 requirements are still short of a technique a selected skill provides');
    expect(text).toContain('FR-1 (Negative 0/1, Edge 1/2); FR-2 (Negative 0/1, Edge 1/2)');
    expect(text).toContain('FR-6 (Negative 0/1, Edge 1/2) and 19 more.');
    expect(text).not.toContain('FR-7 (');
    expect(text).toContain('Turn on "Fill coverage gaps" to have them asked for.');
  });

  it('does not suggest a top-up when one already ran, and uses the singular for one requirement', () => {
    render(
      <CoverageMatrixSummary
        matrix={matrix({
          gaps: [{ key: 'FR-9', kind: 'functional', missing: [{ technique: 'edge', have: 0, target: 1 }] }],
          gap_total: 1,
          complete: 99,
          top_up: { rounds: 2, calls: 5, cases_added: 9, stopped: 'the last round added no case' },
        })}
        total={108}
      />,
    );
    const text = panel().textContent!;
    expect(text).toContain('1 requirement is still short of a technique a selected skill provides: FR-9 (Edge 0/1).');
    expect(text).not.toContain('Turn on "Fill coverage gaps"');
  });

  it('congratulates only when every testable requirement is complete and nothing is short', () => {
    const { rerender } = render(<CoverageMatrixSummary matrix={matrix()} total={108} />);
    expect(screen.getByText('Every technique that applies has its cases, for every testable requirement.')).toBeTruthy();

    rerender(<CoverageMatrixSummary matrix={matrix({ complete: 99 })} total={108} />);
    expect(screen.queryByText('Every technique that applies has its cases, for every testable requirement.')).toBeNull();

    rerender(
      <CoverageMatrixSummary
        matrix={matrix({ gap_total: 1, gaps: [{ key: 'FR-1', kind: 'functional', missing: [{ technique: 'edge', have: 0, target: 1 }] }] })}
        total={108}
      />,
    );
    expect(screen.queryByText('Every technique that applies has its cases, for every testable requirement.')).toBeNull();
  });

  it('counts the cases that could be tied to no requirement and the requirements that are only background', () => {
    render(<CoverageMatrixSummary matrix={matrix({ unlinked_cases: 3, not_applicable: 8 })} total={108} />);
    expect(panel().textContent).toContain('3 cases could not be tied to a requirement, so they count for none.');
    expect(panel().textContent).toContain('8 requirements are background or reference text: nothing to test.');
  });

  it('uses the singular for one such case and one such requirement, and says nothing when there are none', () => {
    const { rerender } = render(<CoverageMatrixSummary matrix={matrix({ unlinked_cases: 1, not_applicable: 1 })} total={101} />);
    expect(panel().textContent).toContain('1 case could not be tied to a requirement, so it counts for none.');
    expect(panel().textContent).toContain('1 requirement is background or reference text: nothing to test.');

    rerender(<CoverageMatrixSummary matrix={matrix({ unlinked_cases: 0, not_applicable: 0 })} total={100} />);
    expect(panel().textContent).not.toContain('could not be tied');
    expect(panel().textContent).not.toContain('background or reference');
  });
});
