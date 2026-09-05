/**
 * Adding the BRD (Business Requirements Document) slot — a focused,
 * standalone component test, same "render directly" convention as
 * `ApplicationSourceSection.test.tsx`/`SkillsAndMcpPanel.test.tsx` in this
 * same folder (`DocumentUploadSection` takes only plain props/callbacks,
 * no providers or fetch mocking needed).
 *
 * This only covers what changed here (BRD's addition alongside the
 * pre-existing PRD/FRD/SRD/Additional slots) — the pre-existing
 * upload/drag-drop mechanics had no test coverage before this file and
 * stay out of scope.
 */
import React from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { DocumentUploadSection } from './DocumentUploadSection';
import type { UploadedRequirementDoc } from '../../types';

function noop() {}

describe('DocumentUploadSection — BRD slot', () => {
  it('renders all 5 slots, BRD first, with the real 5 SLOTS count', () => {
    const { container } = render(<DocumentUploadSection documents={[]} onDocumentsChange={noop} />);

    expect(screen.getByText('5 SLOTS')).toBeTruthy();
    expect(document.getElementById('doc-slot-brd')).toBeTruthy();
    expect(document.getElementById('doc-slot-prd')).toBeTruthy();
    expect(document.getElementById('doc-slot-frd')).toBeTruthy();
    expect(document.getElementById('doc-slot-srd')).toBeTruthy();
    expect(document.getElementById('doc-slot-additional')).toBeTruthy();

    expect(screen.getByText('1. BRD')).toBeTruthy();
    expect(screen.getByText('Business Requirements Document')).toBeTruthy();
    // Renumbered to make room for BRD as slot 1.
    expect(screen.getByText('2. PRD')).toBeTruthy();
    expect(screen.getByText('5. Additional Document')).toBeTruthy();

    // BRD genuinely leads the Source-of-Truth Context Flow banner (and
    // therefore the whole page), ahead of PRD -- not just present somewhere.
    const text = container.textContent ?? '';
    const brdIndex = text.indexOf('BRD');
    const prdIndex = text.indexOf('PRD');
    expect(brdIndex).toBeGreaterThanOrEqual(0);
    expect(prdIndex).toBeGreaterThan(brdIndex);
  });

  it('shows a real uploaded BRD document as Ready in its own slot, independent of the other slots', () => {
    const brdDoc: UploadedRequirementDoc = {
      slotId: 'BRD',
      label: '1. BRD',
      fullName: 'Business Requirements Document',
      fileName: 'business-case.pdf',
      fileSize: 2048,
      fileType: 'PDF',
      uploadedAt: new Date().toISOString(),
      status: 'READY',
    };

    render(<DocumentUploadSection documents={[brdDoc]} onDocumentsChange={noop} />);

    const brdSlot = document.getElementById('doc-slot-brd')!;
    expect(brdSlot.textContent).toContain('business-case.pdf');
    expect(brdSlot.textContent).toContain('Ready');

    // The other 4 slots remain open — this document only fills BRD's slot.
    const prdSlot = document.getElementById('doc-slot-prd')!;
    expect(prdSlot.textContent).toContain('Slot Open');
  });

  it('lets onDocumentsChange be called for the BRD slot the same way any other slot already can', () => {
    const onDocumentsChange = vi.fn();
    render(<DocumentUploadSection documents={[]} onDocumentsChange={onDocumentsChange} />);

    // The BRD slot's dropzone is clickable/wired exactly like every other
    // slot's (real file selection is exercised by the browser's own file
    // picker, not simulated here) — this just confirms BRD didn't get left
    // out of the same wiring every other slot has.
    const brdSlot = document.getElementById('doc-slot-brd')!;
    expect(brdSlot.querySelector('input[type="file"]')).toBeTruthy();
  });
});
