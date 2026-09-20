/**
 * `canvasBackingSize` — the one piece of `neuralActivity.ts` that decides how sharp the canvas is. The canvas
 * itself is not testable here (jsdom has none), but the arithmetic that sizes its bitmap is pure.
 */
import { describe, expect, it } from 'vitest';
import { canvasBackingSize } from './neuralActivity';

describe('canvasBackingSize — one bitmap pixel for each device pixel the canvas occupies', () => {
  it('is the CSS size at 1x for a layout of whole pixels', () => {
    expect(canvasBackingSize(494, 200, 1)).toEqual({ width: 494, height: 200, scaleX: 1, scaleY: 1 });
  });

  it('follows the fractional layout width, not a rounded one', () => {
    // 494.6875 CSS px at 2x occupies 989.375 device px. Rounding the CSS width first (495) and doubling it gave
    // 990, a bitmap the browser then had to resample onto the element, which is what made the text soft.
    const size = canvasBackingSize(494.6875, 200, 2);
    expect(size.width).toBe(989);
    expect(size.height).toBe(400);
    // the transform maps CSS-pixel coordinates onto exactly that bitmap
    expect(size.scaleX * 494.6875).toBeCloseTo(989, 9);
    expect(size.scaleY * 200).toBeCloseTo(400, 9);
  });

  it('copes with fractional display scaling (125%, 150%), which rarely divides a CSS size evenly', () => {
    const at125 = canvasBackingSize(353.4, 200, 1.25);
    expect(at125.width).toBe(442); // 441.75 device px
    expect(at125.height).toBe(250);
    const at150 = canvasBackingSize(351.33, 200, 1.5);
    expect(at150.width).toBe(527); // 526.995 device px
    expect(at150.height).toBe(300);
  });

  it('uses the exact device-pixel size the browser reports when it agrees with the estimate', () => {
    // 494.69 CSS px at 1x: the estimate rounds to 495, but the browser says the element occupies 494 device px.
    expect(canvasBackingSize(494.69, 200, 1, { width: 494, height: 200 }).width).toBe(494);
  });

  it('ignores a reported device-pixel size that belongs to another display scale', () => {
    // 495 device px for an element that is 494.69 CSS px wide at 2x is a 1x number, not this canvas's.
    expect(canvasBackingSize(494.69, 200, 2, { width: 495, height: 200 }).width).toBe(989);
    // and an empty report (the element is not laid out) falls back to the estimate too
    expect(canvasBackingSize(300, 200, 2, { width: 0, height: 0 })).toMatchObject({ width: 600, height: 400 });
  });

  it('never draws above 3x, and never asks for an empty bitmap', () => {
    expect(canvasBackingSize(100, 50, 5)).toMatchObject({ width: 300, height: 150 });
    expect(canvasBackingSize(0.2, 0.2, 1)).toMatchObject({ width: 1, height: 1 });
  });
});
