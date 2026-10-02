/**
 * The one dialog implementation this app uses.
 *
 * Why this exists: eight dialogs were spread across `components/autopilot/`,
 * `components/openapi/`, `components/intelligence/` and
 * `components/test-generator/`, and they did not agree on anything that
 * matters to a keyboard or screen-reader user.
 *
 *   - `FlakyTestDetailModal`, `TelemetryDetailModal`,
 *     `FailureInvestigationWorkspaceModal` and `TelemetryRetentionModal` had no
 *     `role="dialog"`, no `aria-modal`, no Escape handler, no focus trap and no
 *     focus restore. Two of them are the only way to close a dialog, and both of
 *     those close buttons are icon-only with no accessible name.
 *   - The other four (`EditTestCaseModal`, `McpConnectModal`, `SkillDetailModal`,
 *     `SkillManagerModal`, `DeleteProjectDialog`) had the semantics but never
 *     moved focus INTO the dialog on open and never restored it on close, so
 *     focus stayed on the trigger behind the overlay: Tab walked the page
 *     underneath while the dialog was up.
 *   - `tabIndex` appeared NOWHERE in `src/` before this, which is what made a
 *     focus trap structurally impossible to write inline: there was no element
 *     you could make programmatically focusable except a natively focusable one.
 *
 * One primitive, so the focus logic is written once. It:
 *   - marks the dialog (`role="dialog"`, `aria-modal`, `aria-label`/`labelledby`)
 *   - moves focus to the dialog itself on open, so the first Tab enters its
 *     content rather than continuing from the trigger
 *   - keeps Tab and Shift+Tab inside the dialog
 *   - closes on Escape and on a backdrop click, and NOT on a click inside
 *   - restores focus to whatever was focused before it opened
 *   - locks background scroll, so the page behind cannot be scrolled by wheel
 *
 * `initialFocusRef` optionally names the control that should take focus instead
 * of the dialog container — useful when the first field is what the user came
 * for.
 */
import React, { useCallback, useEffect, useRef } from 'react';

const FOCUSABLE = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled]):not([type="hidden"])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

export interface ModalProps {
  /** Stable id, used as the `aria-labelledby` target's anchor in tests. */
  id: string;
  /** Accessible name. Pass `undefined` only when `labelledBy` is given. */
  ariaLabel?: string;
  /** Id of the element that names this dialog, when it has a visible heading. */
  labelledBy?: string;
  onClose: () => void;
  /** Blocks backdrop-click and Escape closing, e.g. while a save is in flight. */
  dismissable?: boolean;
  /** Focused on open instead of the dialog container. */
  initialFocusRef?: React.RefObject<HTMLElement>;
  children: React.ReactNode;
  /** Applied to the dialog panel. Width/spacing live here. */
  panelClassName?: string;
  /** Applied to the fixed backdrop wrapper. */
  overlayClassName?: string;
}

export const Modal: React.FC<ModalProps> = ({
  id,
  ariaLabel,
  labelledBy,
  onClose,
  dismissable = true,
  initialFocusRef,
  children,
  panelClassName = 'w-full max-w-2xl',
  overlayClassName = 'items-start justify-center pt-10',
}) => {
  const panelRef = useRef<HTMLDivElement>(null);
  // Where focus came from, so it can be handed back on close.
  const returnFocusTo = useRef<HTMLElement | null>(null);

  // On open: remember the trigger, then move focus into the dialog.
  useEffect(() => {
    returnFocusTo.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const target = initialFocusRef?.current ?? panelRef.current;
    target?.focus();
    // Background scroll lock, restored on unmount.
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = previousOverflow;
    };
    // Deliberately mount-only: re-running on every render would steal focus
    // back from whatever the user has since tabbed to inside the dialog.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // On unmount: give focus back to the trigger.
  useEffect(
    () => () => {
      returnFocusTo.current?.focus?.();
    },
    [],
  );

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (event.key === 'Escape' && dismissable) {
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== 'Tab') return;

      const panel = panelRef.current;
      if (!panel) return;
      // Built with an explicit loop rather than `Array.from(querySelectorAll(...))`
      // so the element type is `HTMLElement` and not inferred `unknown`.
      //
      // No visibility filtering here, and deliberately so: the usual
      // `offsetParent !== null` test for "is this rendered" reports false for
      // EVERY element in a DOM without layout (jsdom included), which silently
      // emptied the list and turned the trap into a no-op that still looked
      // correct. The selector already excludes disabled controls; `[hidden]`
      // is the one remaining case worth skipping.
      const focusable: HTMLElement[] = [];
      panel.querySelectorAll<HTMLElement>(FOCUSABLE).forEach((el) => {
        if (el.hasAttribute('hidden')) return;
        focusable.push(el);
      });
      if (focusable.length === 0) {
        // Nothing focusable inside: keep focus on the dialog itself rather than
        // letting it escape to the page behind the overlay.
        event.preventDefault();
        panel.focus();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const active = document.activeElement;
      // Only intercept at the ends; everywhere else the browser's own order is
      // correct and wrapping mid-list would feel wrong.
      if (event.shiftKey && (active === first || active === panel)) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && active === last) {
        event.preventDefault();
        first.focus();
      }
    },
    [dismissable, onClose],
  );

  return (
    <div
      className={`fixed inset-0 z-50 flex ${overlayClassName} bg-slate-950/80 backdrop-blur-sm p-4 animate-in fade-in duration-100`}
      onClick={dismissable ? onClose : undefined}
    >
      <div
        ref={panelRef}
        id={id}
        role="dialog"
        aria-modal="true"
        aria-label={ariaLabel}
        aria-labelledby={labelledBy}
        // Makes the container itself focusable, which is what focus lands on when
        // the dialog has no better initial target.
        tabIndex={-1}
        onKeyDown={onKeyDown}
        onClick={(e) => e.stopPropagation()}
        className={`${panelClassName} rounded-lg bg-surface-card border border-border-default shadow-2xl overflow-hidden flex flex-col max-h-[85vh] outline-none`}
      >
        {children}
      </div>
    </div>
  );
};

/** The header row every dialog in this app uses: icon, title, optional subtitle,
 *  and the close button. The close button always gets an accessible name here,
 *  because two of the existing icon-only close buttons had none. */
export const ModalHeader: React.FC<{
  icon?: React.ReactNode;
  title: string;
  subtitle?: string;
  onClose: () => void;
  closeLabel?: string;
  extra?: React.ReactNode;
  actions?: React.ReactNode;
}> = ({ icon, title, subtitle, onClose, closeLabel, extra, actions }) => (
  <div className="flex items-center justify-between px-4 py-3 border-b border-border-default bg-surface-page">
    <div className="flex items-center gap-2 min-w-0">
      {icon}
      <div className="min-w-0">
        <h2 className="text-sm font-bold text-content-primary font-sans truncate">{title}</h2>
        {subtitle && <p className="text-[11px] text-content-muted font-sans">{subtitle}</p>}
      </div>
    </div>
    <div className="flex items-center gap-2 shrink-0">
      {extra}
      {actions}
      <button
        type="button"
        onClick={onClose}
        aria-label={closeLabel ?? `Close ${title}`}
        className="p-1.5 text-content-muted hover:text-content-secondary rounded cursor-pointer"
      >
        <XIcon />
      </button>
    </div>
  </div>
);

/** Kept local so this primitive does not depend on an icon import, and so the
 *  close glyph is `aria-hidden` (the button's own `aria-label` is its name). */
const XIcon: React.FC = () => (
  <svg
    aria-hidden="true"
    viewBox="0 0 24 24"
    width="16"
    height="16"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
  >
    <path d="M18 6 6 18M6 6l12 12" />
  </svg>
);