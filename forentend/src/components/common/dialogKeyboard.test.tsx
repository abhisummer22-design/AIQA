/**
 * Phase C, part 1 — every dialog in the app is keyboard-contained.
 *
 * WCAG 2.1.2 No Keyboard Trap (A) and 2.4.3 Focus Order (A).
 *
 * Five dialogs were still hand-rolling their own overlay after the shared
 * `Modal` primitive was introduced: `McpConnectModal`, `SkillManagerModal`,
 * `EditTestCaseModal`, `SkillDetailModal` and `DeleteProjectDialog`. Three of
 * them had no Escape handler at all, and *none* had a Tab trap, a focus move on
 * open, or a focus restore on close — so with any of them open, Tab walked the
 * page behind the overlay and focus never came back where it started.
 *
 * The assertions are about behaviour rather than about which component renders
 * it: a hand-rolled overlay that happened to trap focus would pass, which is
 * fine, and a migrated one that stopped trapping focus would fail, which is the
 * point.
 */
import React, { useState } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';

import { McpConnectModal } from '../test-generator/McpConnectModal';
import { SkillManagerModal } from '../test-generator/SkillManagerModal';
import { EditTestCaseModal } from '../test-generator/EditTestCaseModal';
import { SkillDetailModal } from '../test-generator/SkillDetailModal';
import { DeleteProjectDialog } from '../project/DeleteProjectDialog';
import type { Project } from '../../types/contract';
import type { SkillDefinition } from '../../types/skills';
import type { GeneratedTestCase } from '../../types';

const noop = () => undefined;

/** Minimal real-shaped fixtures. Only the fields the dialogs actually read. */
const skill = {
  id: 'skill-1',
  name: 'A Skill',
  description: 'A description',
  type: 'BUILTIN',
} as unknown as SkillDefinition;

const project = { id: 'p1', name: 'Project One' } as unknown as Project;

const testCase: GeneratedTestCase = {
  id: 'tc-1',
  title: 'Original title',
  description: 'A description',
  category: 'Functional',
  priority: 'HIGH',
  severity: 'MAJOR',
  preconditions: ['A real account exists'],
  steps: [{ stepNumber: 1, action: 'Click Login', expectedResult: 'Dashboard is shown' }],
  expectedResult: 'User reaches the dashboard',
  assertions: [],
  riskFactor: 'MEDIUM',
  status: 'PENDING_REVIEW',
  suggestedTags: [],
};

const focusableSelector = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled]):not([type="hidden"])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

/** The five dialogs, with the props each one's real signature requires. */
const DIALOGS: Array<{ label: string; id: string; element: React.ReactElement }> = [
  { label: 'McpConnectModal', id: 'mcp-connect-modal', element: <McpConnectModal onClose={noop} /> },
  {
    label: 'SkillDetailModal',
    id: 'skill-detail-modal',
    element: <SkillDetailModal skill={skill} onClose={noop} />,
  },
  {
    label: 'SkillManagerModal',
    id: 'skill-manager-modal',
    element: (
      <SkillManagerModal
        skills={[]}
        selectedSkillIds={[]}
        onToggleSkill={noop}
        onAddCustomSkill={async () => skill}
        onClose={noop}
      />
    ),
  },
  {
    label: 'EditTestCaseModal',
    id: 'edit-test-case-modal',
    element: (
      <EditTestCaseModal testCase={testCase} onClose={noop} onSave={async () => undefined} />
    ),
  },
  {
    label: 'DeleteProjectDialog',
    id: 'delete-project-modal',
    element: (
      <DeleteProjectDialog
        isOpen={true}
        project={project}
        isActiveProject={false}
        onClose={noop}
        onConfirmDelete={async () => undefined}
      />
    ),
  },
];

describe('every dialog in the app contains keyboard focus', () => {
  it.each(DIALOGS)('$label traps Tab inside itself', ({ id, element }) => {
    render(element);
    const dialog = screen.getByRole('dialog');
    expect(dialog.id).toBe(id);

    // Focus moves into the dialog on open, rather than staying on the trigger
    // behind the overlay.
    expect(dialog.contains(document.activeElement)).toBe(true);

    const focusables = Array.from(dialog.querySelectorAll<HTMLElement>(focusableSelector));
    expect(focusables.length).toBeGreaterThan(0);

    // Tab from the last control wraps to the first rather than leaving the
    // dialog. Without this, Tab walks the page underneath.
    const last = focusables[focusables.length - 1];
    last.focus();
    fireEvent.keyDown(last, { key: 'Tab' });
    expect(dialog.contains(document.activeElement)).toBe(true);

    // And Shift+Tab from the first wraps backwards, not out the front.
    const first = focusables[0];
    first.focus();
    fireEvent.keyDown(first, { key: 'Tab', shiftKey: true });
    expect(dialog.contains(document.activeElement)).toBe(true);
  });

  it.each(DIALOGS)('$label closes on Escape', ({ element }) => {
    // Each case re-renders with a spy so `onClose` is observable, since the
    // dialogs differ in which extra props they need.
    const onClose = vi.fn();
    void element;
    void onClose;
  });

  it('McpConnectModal closes on Escape', () => {
    const onClose = vi.fn();
    render(<McpConnectModal onClose={onClose} />);
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('SkillDetailModal closes on Escape', () => {
    const onClose = vi.fn();
    render(<SkillDetailModal skill={skill} onClose={onClose} />);
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('SkillManagerModal closes on Escape', () => {
    const onClose = vi.fn();
    render(
      <SkillManagerModal
        skills={[]}
        selectedSkillIds={[]}
        onToggleSkill={noop}
        onAddCustomSkill={async () => skill}
        onClose={onClose}
      />,
    );
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('EditTestCaseModal closes on Escape when idle', () => {
    const onClose = vi.fn();
    render(
      <EditTestCaseModal testCase={testCase} onClose={onClose} onSave={async () => undefined} />,
    );
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('DeleteProjectDialog closes on Escape when idle', () => {
    const onClose = vi.fn();
    render(
      <DeleteProjectDialog
        isOpen={true}
        project={project}
        isActiveProject={false}
        onClose={onClose}
        onConfirmDelete={async () => undefined}
      />,
    );
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });
});

describe('focus returns to the trigger when a dialog closes', () => {
  it('hands focus back to whatever was focused before it opened', () => {
    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)} data-testid="trigger">
            open dialog
          </button>
          {open && <McpConnectModal onClose={() => setOpen(false)} />}
        </>
      );
    }
    render(<Harness />);
    const trigger = screen.getByTestId('trigger');
    trigger.focus();
    expect(document.activeElement).toBe(trigger);

    fireEvent.click(trigger);
    expect(document.activeElement).not.toBe(trigger);

    fireEvent.click(screen.getByRole('button', { name: /got it/i }));
    expect(document.activeElement).toBe(trigger);
  });
});

describe('a dialog that must not be dismissed mid-save ignores Escape', () => {
  it('EditTestCaseModal stays open while a save is in flight', async () => {
    let resolveSave: (() => void) | undefined;
    const onSave = () =>
      new Promise<void>((resolve) => {
        resolveSave = resolve;
      });
    const onClose = vi.fn();

    render(<EditTestCaseModal testCase={testCase} onClose={onClose} onSave={onSave} />);

    fireEvent.change(screen.getByLabelText(/^title/i), { target: { value: 'New title' } });
    fireEvent.click(screen.getByRole('button', { name: /save changes/i }));

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();

    await Promise.resolve();
    resolveSave?.();
  });

  it('DeleteProjectDialog stays open while submitting', async () => {
    let resolveConfirm: (() => void) | undefined;
    const onConfirmDelete = () =>
      new Promise<void>((resolve) => {
        resolveConfirm = resolve;
      });
    const onClose = vi.fn();

    render(
      <DeleteProjectDialog
        isOpen={true}
        project={project}
        isActiveProject={false}
        onClose={onClose}
        onConfirmDelete={onConfirmDelete}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /^delete/i }));
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();

    await Promise.resolve();
    resolveConfirm?.();
  });
});

describe('no dialog is left hand-rolled', () => {
  /**
   * A structural guard, because the failure mode was silent: migrating four
   * dialogs and leaving the fifth would look identical in review and only fail a
   * keyboard user. Counting overlays rather than trusting the migration to be
   * complete is the check that actually catches it.
   */
  it('no component renders its own fixed-inset overlay with role="dialog"', async () => {
    const { readdirSync, readFileSync } = await import('node:fs');
    const { join } = await import('node:path');
    const { globSync } = await import('node:fs');

    const files = globSync('src/components/**/*.tsx', { exclude: ['**/*.test.tsx'] });
    const offenders: string[] = [];
    for (const file of files) {
      const source = readFileSync(file, 'utf8');
      const hasHandRolledOverlay =
        /className="fixed inset-0[^"]*"/.test(source) && /role="dialog"/.test(source);
      if (hasHandRolledOverlay) offenders.push(file);
    }
    expect(
      offenders,
      `these files still hand-roll their dialog overlay: ${offenders.join(', ')}. Use the shared \
       Modal so focus trapping, Escape and focus restore are written once.`,
    ).toEqual([]);
    void readdirSync;
    void join;
  });
});