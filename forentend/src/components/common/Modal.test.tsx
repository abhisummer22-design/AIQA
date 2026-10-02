import { describe, expect, it, vi } from 'vitest';
import React, { useRef, useState } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { Modal, ModalHeader } from './Modal';

/**
 * These assert behaviour, not markup: every one of the eight dialogs this
 * replaces was missing at least one of these, and the failure mode is a
 * keyboard user unable to open, use or leave a dialog. jsdom has no real focus
 * order, so the trap is verified through the events it actually handles.
 */

const Harness: React.FC<{
  initialFocus?: boolean;
  onClose?: () => void;
}> = ({ initialFocus = false, onClose }) => {
  const [open, setOpen] = useState(false);
  const fieldRef = useRef<HTMLInputElement>(null);
  return (
    <div>
      <button type="button" onClick={() => setOpen(true)}>
        Open
      </button>
      <button type="button">Outside</button>
      {open && (
        <Modal
          id="test-modal"
          ariaLabel="Test dialog"
          onClose={() => {
            setOpen(false);
            onClose?.();
          }}
          initialFocusRef={initialFocus ? fieldRef : undefined}
        >
          <ModalHeader title="Test" onClose={() => setOpen(false)} />
          <input ref={fieldRef} aria-label="First field" />
          <a href="#somewhere">Middle link</a>
          <button type="button">Last action</button>
        </Modal>
      )}
    </div>
  );
};

const openIt = () => fireEvent.click(screen.getByRole('button', { name: 'Open' }));

describe('Modal', () => {
  it('is announced as a dialog with an accessible name', () => {
    render(<Harness />);
    openIt();
    const dialog = screen.getByRole('dialog', { name: 'Test dialog' });
    expect(dialog.getAttribute('aria-modal')).toBe('true');
  });

  it('moves focus into the dialog when it opens, rather than leaving it on the trigger', () => {
    render(<Harness />);
    const trigger = screen.getByRole('button', { name: 'Open' });
    trigger.focus();
    expect(document.activeElement).toBe(trigger);

    openIt();

    const dialog = screen.getByRole('dialog');
    // Focus lands inside the dialog subtree, which is what stops the next Tab
    // from walking the page underneath the overlay.
    expect(dialog.contains(document.activeElement)).toBe(true);
    expect(document.activeElement).not.toBe(trigger);
  });

  it('focuses the requested control when one is named', () => {
    render(<Harness initialFocus />);
    openIt();
    expect(document.activeElement).toBe(screen.getByLabelText('First field'));
  });

  it('restores focus to the trigger when it closes', () => {
    render(<Harness />);
    const trigger = screen.getByRole('button', { name: 'Open' });
    trigger.focus();
    openIt();
    expect(screen.getByRole('dialog')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Close Test' }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('closes on Escape', () => {
    const onClose = vi.fn();
    render(<Harness onClose={onClose} />);
    openIt();
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('does NOT close on a click inside the dialog, but does on the backdrop', () => {
    const onClose = vi.fn();
    render(<Harness onClose={onClose} />);
    openIt();
    const dialog = screen.getByRole('dialog');

    fireEvent.click(dialog);
    expect(onClose).not.toHaveBeenCalled();

    // The backdrop is the dialog's parent element.
    fireEvent.click(dialog.parentElement!);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('traps Tab at the last control by wrapping to the first', () => {
    render(<Harness />);
    openIt();
    const dialog = screen.getByRole('dialog');
    // The first focusable element is ModalHeader's close button, which precedes
    // the children in DOM order -- so it is that, not the first field.
    const first = screen.getByRole('button', { name: 'Close Test' });
    const last = screen.getByRole('button', { name: 'Last action' });

    last.focus();
    fireEvent.keyDown(dialog, { key: 'Tab' });
    expect(document.activeElement).toBe(first);
  });

  it('traps Shift+Tab at the first control by wrapping to the last', () => {
    render(<Harness />);
    openIt();
    const dialog = screen.getByRole('dialog');
    const first = screen.getByRole('button', { name: 'Close Test' });
    const last = screen.getByRole('button', { name: 'Last action' });

    first.focus();
    fireEvent.keyDown(dialog, { key: 'Tab', shiftKey: true });
    expect(document.activeElement).toBe(last);
  });

  it('wraps back to the dialog itself when Shift+Tab leaves the first control', () => {
    render(<Harness />);
    openIt();
    const dialog = screen.getByRole('dialog');
    // Focus still sitting on the container (nothing inside focused yet) must not
    // be able to Tab backwards out of the overlay.
    dialog.focus();
    fireEvent.keyDown(dialog, { key: 'Tab', shiftKey: true });
    expect(screen.getByRole('dialog').contains(document.activeElement)).toBe(true);
  });

  it('leaves Tab alone in the middle of the list, where browser order is correct', () => {
    render(<Harness />);
    openIt();
    const dialog = screen.getByRole('dialog');
    const link = screen.getByRole('link', { name: 'Middle link' });

    link.focus();
    fireEvent.keyDown(dialog, { key: 'Tab' });
    // Untouched: the browser's own sequential focus navigation handles this, and
    // wrapping mid-list would be wrong.
    expect(document.activeElement).toBe(link);
  });

  it('locks background scroll while open and restores it after', () => {
    render(<Harness />);
    expect(document.body.style.overflow).toBe('');
    openIt();
    expect(document.body.style.overflow).toBe('hidden');
    fireEvent.click(screen.getByRole('button', { name: 'Close Test' }));
    expect(document.body.style.overflow).toBe('');
  });

  it('gives its header close button an accessible name even when the title is generic', () => {
    render(<Harness />);
    openIt();
    // Without an explicit closeLabel the button is named from the title, so it is
    // never just "button" -- two of the dialogs it replaces had an unnamed
    // icon-only close control that was the only way out.
    expect(screen.getByRole('button', { name: 'Close Test' })).toBeTruthy();
  });
});