import React from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { RequirementCoverageSummary } from './RequirementCoverageSummary';
import type { RequirementCoverage } from '../../types/contract';

function coverage(overrides: Partial<RequirementCoverage> = {}): RequirementCoverage {
  return {
    total: 140,
    offered: 140,
    covered: 140,
    uncovered_keys: [],
    not_offered_keys: [],
    notes: [],
    calls: 12,
    budget_chars: 12000,
    ...overrides,
  };
}

const summary = () => document.getElementById('requirement-coverage') as HTMLElement;

describe('RequirementCoverageSummary', () => {
  it('states how many requirements have a case, were shown to a skill, and how many calls it took', () => {
    render(<RequirementCoverageSummary coverage={coverage({ covered: 120, uncovered_keys: [] })} />);

    expect(summary().textContent).toContain(
      'Requirements: 120 of 140 have a case · 140 were shown to a skill · 12 AI calls (about 12,000 characters of requirements each)',
    );
  });

  it('says every requirement has a case only when that is so', () => {
    const { unmount } = render(<RequirementCoverageSummary coverage={coverage()} />);
    expect(screen.getByText('Every requirement has at least one case.')).toBeTruthy();
    unmount();

    render(<RequirementCoverageSummary coverage={coverage({ covered: 139, uncovered_keys: ['FR-009'] })} />);
    expect(screen.queryByText('Every requirement has at least one case.')).toBeNull();
  });

  it('names the requirements that were shown to a skill but got no case', () => {
    render(<RequirementCoverageSummary coverage={coverage({ covered: 137, uncovered_keys: ['FR-009', 'FR-010', 'FR-011'] })} />);

    const text = summary().textContent!;
    expect(text).toContain('3 requirements were shown to a skill but got no case');
    expect(text).toContain('FR-009, FR-010, FR-011.');
  });

  it('names the requirements of a kind no selected skill tests, and says to select one', () => {
    render(
      <RequirementCoverageSummary
        coverage={coverage({ total: 143, offered: 140, covered: 140, not_offered_keys: ['NFR-1', 'NFR-2', 'NFR-3'] })}
      />,
    );

    const text = summary().textContent!;
    expect(text).toContain('3 requirements are of a kind none of the selected skills tests, so they were not shown to any: NFR-1, NFR-2, NFR-3.');
    expect(text).toContain('Select a skill that covers them and generate again.');
    expect(screen.queryByText('Every requirement has at least one case.')).toBeNull();
  });

  it('uses the singular for one requirement', () => {
    render(<RequirementCoverageSummary coverage={coverage({ total: 141, offered: 140, not_offered_keys: ['NFR-1'], covered: 139, uncovered_keys: ['FR-9'] })} />);

    const text = summary().textContent!;
    expect(text).toContain('1 requirement is of a kind none of the selected skills tests, so it was not shown to any: NFR-1.');
    expect(text).toContain('1 requirement was shown to a skill but got no case (a failed call, or nothing in it to test): FR-9.');
  });

  it('names at most eight and counts the rest, from the run\'s own totals rather than the length of a capped list', () => {
    const listed = Array.from({ length: 100 }, (_, i) => `FR-${i + 1}`);
    render(<RequirementCoverageSummary coverage={coverage({ total: 300, offered: 300, covered: 150, uncovered_keys: listed })} />);

    const text = summary().textContent!;
    expect(text).toContain('150 requirements were shown to a skill but got no case');
    expect(text).toContain('FR-1, FR-2, FR-3, FR-4, FR-5, FR-6, FR-7, FR-8 and 142 more.');
  });

  it('shows the coverage by technique when the run has a matrix, and nothing of the kind when it has none', () => {
    const { unmount } = render(
      <RequirementCoverageSummary
        coverage={coverage({
          // Four requirements never shown to a skill: `total` and `offered` differ, and the
          // matrix must be told the first (the requirements the run had), not the second.
          offered: 136,
          covered: 136,
          not_offered_keys: ['NFR-1', 'NFR-2', 'NFR-3', 'NFR-4'],
          matrix: {
            depth: 'STANDARD',
            techniques: [{ technique: 'negative', applicable: 90, met: 60, short: 30, provided_by: 'Negative Testing' }],
            gaps: [],
            gap_total: 0,
            complete: 100,
            not_applicable: 8,
            unlinked_cases: 0,
          },
        })}
      />,
    );
    expect(document.getElementById('coverage-matrix')).toBeTruthy();
    expect(summary().contains(document.getElementById('coverage-matrix'))).toBe(true);
    // The matrix is told how many requirements the run had (140, not the 136 shown to a skill),
    // to say how many are testable: 140 less the 8 that are only background.
    expect(document.getElementById('coverage-matrix')!.textContent).toContain('100 of 132 testable requirements');
    unmount();

    render(<RequirementCoverageSummary coverage={coverage()} />);
    expect(document.getElementById('coverage-matrix')).toBeNull();
  });

  it('shows the run\'s notes, and none when there are none', () => {
    const { unmount } = render(
      <RequirementCoverageSummary coverage={coverage({ notes: ['Security Testing was skipped: nothing to test in these requirements.'] })} />,
    );
    expect(screen.getByText('Security Testing was skipped: nothing to test in these requirements.')).toBeTruthy();
    unmount();

    render(<RequirementCoverageSummary coverage={coverage()} />);
    expect(summary().querySelector('ul')).toBeNull();
  });
});
