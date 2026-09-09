/**
 * Two focused, standalone additions to `GeneratedTestCasesView`
 * (which otherwise has no test coverage; testing the whole pre-existing
 * component is out of scope here). Rendered directly with plain
 * props/callbacks, same convention as `ApplicationSourceSection.test.tsx`
 * in this same folder.
 *
 * 1. The collapsible "Document Excerpt" toggle inside the Source
 *    Specification & Acceptance Criteria panel — before this, a long real
 *    document excerpt always rendered in full, pushing the rest of the
 *    panel far down the page. It now starts collapsed (just the label + an
 *    Expand control) and only renders the real text once expanded — the
 *    underlying `sourceRequirement` value itself is unchanged either way,
 *    this only toggles whether it's shown.
 * 2. The "Edit" button that opens `EditTestCaseModal` — this view only
 *    owns the modal's open/close state and wires its real `onSave` through
 *    to `onEditTestCase`; the modal's own field-editing/validation/error
 *    behavior is covered in `EditTestCaseModal.test.tsx`, not repeated
 *    here.
 */
import React from 'react';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
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

describe('GeneratedTestCasesView — Edit Test Case wiring', () => {
  it('shows no Edit control at all when the caller provides no onEditTestCase — never a dead button', () => {
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
    expect(document.getElementById('btn-edit-test-case')).toBeNull();
  });

  it('Edit opens the real modal pre-filled with this case, and Save calls the real onEditTestCase with this case\'s real id', async () => {
    const onEditTestCase = vi.fn().mockResolvedValue(undefined);
    render(
      <GeneratedTestCasesView
        testCases={[testCase()]}
        onApproveTestCase={noop}
        onRejectTestCase={noop}
        onApproveAll={noop}
        onOpenExecutionHub={noop}
        onNavigateToSources={noop}
        onEditTestCase={onEditTestCase}
      />,
    );

    await act(async () => {
      fireEvent.click(document.getElementById('btn-edit-test-case')!);
    });

    expect(document.getElementById('edit-test-case-modal')).toBeTruthy();
    expect((document.getElementById('edit-tc-title') as HTMLInputElement).value).toBe(testCase().title);

    await act(async () => {
      fireEvent.click(document.getElementById('btn-save-test-case-edit')!);
    });

    expect(onEditTestCase).toHaveBeenCalledTimes(1);
    expect(onEditTestCase.mock.calls[0][0]).toBe('AI-001');

    // The modal closes itself on a successful save.
    expect(document.getElementById('edit-test-case-modal')).toBeNull();
  });
});
