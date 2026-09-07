/**
 * The collapsible "Document Excerpt" toggle inside the Source Specification
 * & Acceptance Criteria panel — a focused, standalone test of just this new
 * behavior (`GeneratedTestCasesView` has no other test coverage; testing
 * the whole pre-existing component is out of scope here). Rendered
 * directly with plain props/callbacks, same convention as
 * `ApplicationSourceSection.test.tsx` in this same folder.
 *
 * Before this, a long real document excerpt always rendered in full,
 * pushing the rest of the panel far down the page. It now starts
 * collapsed (just the label + an Expand control) and only renders the real
 * text once expanded — the underlying `sourceRequirement` value itself is
 * unchanged either way, this only toggles whether it's shown.
 */
import React from 'react';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { GeneratedTestCasesView } from './GeneratedTestCasesView';
import type { GeneratedTestCase } from '../../types';

const LONG_EXCERPT =
  'Focus particularly on these test categories where applicable: Functional, Non-Functional. '.repeat(20);

function testCase(overrides: Partial<GeneratedTestCase> = {}): GeneratedTestCase {
  return {
    id: 'AI-001',
    title: 'Verify that notifyOnFailure sends an email with the correct parameters',
    description: 'A real generated test case.',
    category: 'Functional',
    priority: 'HIGH',
    severity: 'MAJOR',
    sourceDocument: 'BRD + PRD + FRD + SRD',
    requirementReference: '§3.1 Spec',
    sourceRequirement: LONG_EXCERPT,
    preconditions: [],
    steps: [{ stepNumber: 1, action: 'Trigger a failure.', expectedResult: 'An email is sent.' }],
    assertions: [],
    riskFactor: 'MEDIUM',
    status: 'PENDING_REVIEW',
    suggestedTags: [],
    ...overrides,
  };
}

function noop() {}

describe('GeneratedTestCasesView — collapsible Document Excerpt', () => {
  it('starts collapsed — the real excerpt text is not shown until Expand is clicked', () => {
    render(
      <GeneratedTestCasesView
        testCases={[testCase()]}
        onApproveTestCase={noop}
        onRejectTestCase={noop}
        onApproveAll={noop}
        onOpenExecutionHub={noop}
        onNavigateToSources={noop}
      />,
    );

    expect(screen.getByText('Document Excerpt')).toBeTruthy();
    expect(screen.queryByText(new RegExp(LONG_EXCERPT.slice(0, 40)))).toBeNull();
    expect(document.getElementById('btn-toggle-source-excerpt')!.textContent).toContain('Expand');
  });

  it('clicking Expand reveals the real excerpt text and flips the control to Collapse; clicking again hides it', () => {
    render(
      <GeneratedTestCasesView
        testCases={[testCase()]}
        onApproveTestCase={noop}
        onRejectTestCase={noop}
        onApproveAll={noop}
        onOpenExecutionHub={noop}
        onNavigateToSources={noop}
      />,
    );

    const toggle = document.getElementById('btn-toggle-source-excerpt')!;
    act(() => {
      fireEvent.click(toggle);
    });

    expect(screen.getByText(new RegExp(LONG_EXCERPT.slice(0, 40)))).toBeTruthy();
    expect(toggle.textContent).toContain('Collapse');

    act(() => {
      fireEvent.click(toggle);
    });
    expect(screen.queryByText(new RegExp(LONG_EXCERPT.slice(0, 40)))).toBeNull();
    expect(toggle.textContent).toContain('Expand');
  });

  it('a test case with no real sourceRequirement shows no excerpt block at all — never a fabricated placeholder excerpt', () => {
    render(
      <GeneratedTestCasesView
        testCases={[testCase({ sourceRequirement: undefined })]}
        onApproveTestCase={noop}
        onRejectTestCase={noop}
        onApproveAll={noop}
        onOpenExecutionHub={noop}
        onNavigateToSources={noop}
      />,
    );

    expect(screen.queryByText('Document Excerpt')).toBeNull();
    expect(document.getElementById('btn-toggle-source-excerpt')).toBeNull();
  });
});
