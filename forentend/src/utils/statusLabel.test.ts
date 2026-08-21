import { describe, expect, it } from 'vitest';
import { formatStatusLabel } from './statusLabel';

describe('formatStatusLabel (FE-12 canonical status display)', () => {
  it('replaces underscores with spaces', () => {
    expect(formatStatusLabel('NEEDS_REVIEW')).toBe('NEEDS REVIEW');
    expect(formatStatusLabel('PENDING_APPROVAL')).toBe('PENDING APPROVAL');
    expect(formatStatusLabel('INFRA_ERROR')).toBe('INFRA ERROR');
  });

  it('leaves single-word statuses unchanged', () => {
    expect(formatStatusLabel('RUNNING')).toBe('RUNNING');
    expect(formatStatusLabel('COMPLETED')).toBe('COMPLETED');
    expect(formatStatusLabel('CANCELLED')).toBe('CANCELLED');
  });

  it('never alters the character content, only the separator — the raw value round-trips', () => {
    // This function must never be mistaken for a renaming/aliasing step —
    // it only ever swaps `_` for a space. Confirmed here so a future edit
    // that tries to "helpfully" title-case or reword a status cannot land
    // without this test failing.
    const raw = 'TASK_INFRA_ERROR';
    expect(formatStatusLabel(raw).replace(/ /g, '_')).toBe(raw);
  });
});
