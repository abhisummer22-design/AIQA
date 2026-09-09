/**
 * Edit Test Case modal — closes a real, verified gap: the backend and API
 * client both already supported editing a test case's descriptive fields,
 * but no UI ever called them (only Reject-and-regenerate existed). Rendered
 * directly, same "render the component directly" convention as
 * `SkillManagerModal`'s own caller conventions in this folder — this modal
 * takes only plain props/callbacks, no providers needed.
 */
import React from 'react';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { EditTestCaseModal } from './EditTestCaseModal';
import type { GeneratedTestCase } from '../../types';

function testCase(overrides: Partial<GeneratedTestCase> = {}): GeneratedTestCase {
  return {
    id: 'AI-001',
    title: 'Valid login succeeds',
    description: 'Covers the real happy-path login flow.',
    category: 'Functional',
    priority: 'HIGH',
    severity: 'MAJOR',
    preconditions: ['User has a real, active account'],
    steps: [
      { stepNumber: 1, action: 'Enter valid credentials', expectedResult: 'Fields accept the input' },
      { stepNumber: 2, action: 'Click Login', expectedResult: 'Dashboard is shown' },
    ],
    expectedResult: 'User reaches the dashboard',
    assertions: [],
    riskFactor: 'MEDIUM',
    status: 'PENDING_REVIEW',
    suggestedTags: [],
    ...overrides,
  };
}

async function clickButton(el: Element) {
  await act(async () => {
    fireEvent.click(el);
  });
}

async function changeInput(el: Element, value: string) {
  await act(async () => {
    fireEvent.change(el, { target: { value } });
  });
}

describe('EditTestCaseModal', () => {
  it('pre-fills every real field from the real test case, not placeholders', () => {
    render(<EditTestCaseModal testCase={testCase()} onClose={vi.fn()} onSave={vi.fn()} />);

    expect((document.getElementById('edit-tc-title') as HTMLInputElement).value).toBe('Valid login succeeds');
    expect((document.getElementById('edit-tc-description') as HTMLTextAreaElement).value).toBe(
      'Covers the real happy-path login flow.',
    );
    expect((document.getElementById('edit-tc-priority') as HTMLSelectElement).value).toBe('HIGH');
    expect(screen.getByDisplayValue('User has a real, active account')).toBeTruthy();
    expect(screen.getByDisplayValue('Enter valid credentials')).toBeTruthy();
    expect(screen.getByDisplayValue('Dashboard is shown')).toBeTruthy();
    expect((document.getElementById('edit-tc-expected-result') as HTMLTextAreaElement).value).toBe(
      'User reaches the dashboard',
    );
  });

  it('saving sends the real edited values, correctly renumbering steps after a removal', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(<EditTestCaseModal testCase={testCase()} onClose={onClose} onSave={onSave} />);

    await changeInput(document.getElementById('edit-tc-title')!, 'Valid login succeeds (edited)');

    // Remove step 1 (Enter valid credentials) — step 2 must become the new
    // step 1 on save.
    await clickButton(document.querySelector('#edit-step-0 button[title="Remove step"]')!);

    await clickButton(document.getElementById('btn-save-test-case-edit')!);

    expect(onSave).toHaveBeenCalledTimes(1);
    const payload = onSave.mock.calls[0][0];
    expect(payload.title).toBe('Valid login succeeds (edited)');
    expect(payload.steps).toEqual([{ stepNumber: 1, action: 'Click Login', expectedResult: 'Dashboard is shown' }]);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('cannot save with an empty title, or with zero steps', async () => {
    const onSave = vi.fn();
    render(<EditTestCaseModal testCase={testCase()} onClose={vi.fn()} onSave={onSave} />);

    await changeInput(document.getElementById('edit-tc-title')!, '   ');
    expect((document.getElementById('btn-save-test-case-edit') as HTMLButtonElement).disabled).toBe(true);

    await changeInput(document.getElementById('edit-tc-title')!, 'A real title');
    expect((document.getElementById('btn-save-test-case-edit') as HTMLButtonElement).disabled).toBe(false);

    // Remove every step — must become unsaveable again, with a real, visible reason.
    let removeStepButton = document.querySelector('button[title="Remove step"]');
    while (removeStepButton) {
      await clickButton(removeStepButton);
      removeStepButton = document.querySelector('button[title="Remove step"]');
    }
    expect(screen.getByText(/At least one step is required/)).toBeTruthy();
    expect((document.getElementById('btn-save-test-case-edit') as HTMLButtonElement).disabled).toBe(true);
    expect(onSave).not.toHaveBeenCalled();
  });

  it('a real save failure is shown honestly and the modal stays open, never a silent close', async () => {
    const onSave = vi.fn().mockRejectedValue(new Error('title must not be empty'));
    const onClose = vi.fn();
    render(<EditTestCaseModal testCase={testCase()} onClose={onClose} onSave={onSave} />);

    await clickButton(document.getElementById('btn-save-test-case-edit')!);

    expect(await screen.findByText('title must not be empty')).toBeTruthy();
    expect(onClose).not.toHaveBeenCalled();
  });

  it('Cancel discards changes without ever calling onSave', async () => {
    const onSave = vi.fn();
    const onClose = vi.fn();
    render(<EditTestCaseModal testCase={testCase()} onClose={onClose} onSave={onSave} />);

    await changeInput(document.getElementById('edit-tc-title')!, 'Changed but not saved');
    await clickButton(screen.getByText('Cancel'));

    expect(onSave).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
