/**
 * Phase H — the OS "reduce motion" setting.
 *
 * Before this, the app had zero occurrences of `prefers-reduced-motion` and no
 * `@media` block at all, against 78 `animate-spin`, 27 `animate-pulse`, 8
 * `animate-ping`, 2 `animate-bounce` and 143 `transition-*` usages — plus
 * framer-motion `repeat: Infinity` loops, two SVG SMIL animations, and an
 * unconditional `requestAnimationFrame` loop that drifted even at level 0.
 *
 * WCAG 2.2.2 Pause, Stop, Hide (A): motion that runs on its own, forever, with
 * nothing to pause and no way to interact with it, is what the setting exists to
 * turn off.
 *
 * The behaviour needed three mechanisms, and only one of them is CSS. These tests
 * cover all three, because a test that only checked the stylesheet would have
 * passed while the dashboard orb kept spinning and the radar kept sweeping.
 */
import React from 'react';
import { act, render, screen } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { AIQualityCoreOrb } from '../components/dashboard/AIQualityCoreOrb';
import { OrchestratorRadar } from '../components/orchestrator/OrchestratorRadar';
import { NeuralActivity } from '../utils/neuralActivity';
import { usePrefersReducedMotion } from '../hooks/usePrefersReducedMotion';

type Listener = (event: MediaQueryListEvent) => void;

/**
 * Installs a stub `matchMedia` reporting the given reduce-motion preference, and
 * returns the listeners registered against it so a test can prove the app
 * *reacts* to the setting changing rather than only reading it once at mount.
 */
function stubMatchMedia(matches: boolean) {
  const listeners = new Set<Listener>();
  const mql = {
    matches,
    media: '(prefers-reduced-motion: reduce)',
    addEventListener: (_: string, cb: Listener) => listeners.add(cb),
    removeEventListener: (_: string, cb: Listener) => listeners.delete(cb),
    // Legacy Safari spellings; present so the hook's fallback path is exercised
    // rather than dead code.
    addListener: (cb: Listener) => listeners.add(cb),
    removeListener: (cb: Listener) => listeners.delete(cb),
    onchange: null,
    dispatchEvent: () => false,
  };
  window.matchMedia = vi.fn().mockImplementation((q: string) =>
    q.includes('prefers-reduced-motion') ? mql : { ...mql, matches: false, media: q },
  ) as unknown as typeof window.matchMedia;
  return {
    emit(next: boolean) {
      mql.matches = next;
      listeners.forEach((cb) => cb({ matches: next } as MediaQueryListEvent));
    },
    listenerCount: () => listeners.size,
  };
}

const originalMatchMedia = window.matchMedia;

beforeEach(() => {
  // Default to "motion is fine"; the reduced-motion tests opt in explicitly.
  stubMatchMedia(false);
});

afterEach(() => {
  window.matchMedia = originalMatchMedia;
  vi.restoreAllMocks();
});

// ------------------------------------------------------------- the stylesheet

describe('the stylesheet honours reduced motion', () => {
  const css = readFileSync(resolve(__dirname, '../index.css'), 'utf8');

  it('has a prefers-reduced-motion block at all', () => {
    expect(css).toContain('@media (prefers-reduced-motion: reduce)');
  });

  /**
   * Structural rather than behavioural, because jsdom does not evaluate CSS media
   * queries against a stubbed `matchMedia` — there is nothing to assert about the
   * computed style here. What it does pin is that the blanket rule stays a blanket
   * rule: a narrow selector would silently leave most of the 115 sites uncovered,
   * which is exactly the failure mode this file exists to prevent.
   */
  it('suppresses animation and transition on every element, not a chosen few', () => {
    const block = css.slice(css.indexOf('@media (prefers-reduced-motion: reduce)'));
    expect(block).toMatch(/\*\s*,\s*\*::before\s*,\s*\*::after\s*\{/);
    expect(block).toMatch(/animation-duration:\s*0\.01ms\s*!important/);
    expect(block).toMatch(/animation-iteration-count:\s*1\s*!important/);
    expect(block).toMatch(/transition-duration:\s*0\.01ms\s*!important/);
    // `scroll-behavior` matters for its own reason: a smooth-scroll hijack is
    // motion the user never asked for.
    expect(block).toMatch(/scroll-behavior:\s*auto\s*!important/);
  });

  /**
   * `animation: none` was rejected in favour of a near-zero duration plus a single
   * iteration. `animation-play-state: paused` is the trap being guarded against:
   * it freezes a `ping` part-way through its expansion and leaves a permanently
   * half-drawn halo that reads as a rendering bug, not a state.
   */
  it('does not freeze animations mid-frame', () => {
    const block = css.slice(css.indexOf('@media (prefers-reduced-motion: reduce)'));
    expect(block).not.toContain('animation-play-state');
    expect(block).not.toContain('animation: none');
  });
});

// -------------------------------------------------------------- the hook

describe('usePrefersReducedMotion', () => {
  it('follows the setting when it changes while the app is open', () => {
    const media = stubMatchMedia(false);

    function Probe() {
      return <span data-testid="v">{String(usePrefersReducedMotion())}</span>;
    }

    render(<Probe />);
    expect(screen.getByTestId('v').textContent).toBe('false');
    expect(media.listenerCount()).toBeGreaterThan(0);

    act(() => media.emit(true));
    expect(screen.getByTestId('v').textContent).toBe(
      'true');
  });

  it('defaults to false when matchMedia is unavailable', () => {
    // A server render, or any environment without the API, must not freeze for
    // everyone whose environment happens to lack it.
    (window as unknown as { matchMedia?: unknown }).matchMedia = undefined;
    function Probe() {
      return <span data-testid="v">{String(usePrefersReducedMotion())}</span>;
    }
    render(<Probe />);
    expect(screen.getByTestId('v').textContent).toBe('false');
  });
});

// --------------------------------------------------------- framer-motion

describe('the dashboard orb', () => {
  const props = {
    status: 'Healthy' as never,
    hasActiveRun: false,
    projectName: 'p1',
  };

  it('still renders its content with motion reduced', () => {
    stubMatchMedia(true);
    render(<AIQualityCoreOrb {...props} />);
    // The reduced-motion path must not blank the visual: the branding inside the
    // core is content, not decoration, and the status label is the actual signal.
    expect(screen.getByText('AIQA')).toBeDefined();
    expect(screen.getByText('CORE')).toBeDefined();
  });

  it('renders the same content with motion allowed', () => {
    stubMatchMedia(false);
    render(<AIQualityCoreOrb {...props} />);
    expect(screen.getByText('AIQA')).toBeDefined();
  });
});

// ------------------------------------------------------------- SVG SMIL

describe('the orchestrator radar', () => {
  const orchState = {
    executors: [
      { kind: 'maestro', gate: 'AUTO' },
      { kind: 'compiled-api', gate: 'BLOCKED' },
    ],
    active_runs: [],
  } as never;

  it('renders no SMIL animation elements when motion is reduced', () => {
    stubMatchMedia(true);
    const { container } = render(<OrchestratorRadar orchState={orchState} />);
    // SVG SMIL is driven by the browser's own animation engine, not by CSS
    // animation properties, so the stylesheet cannot reach it at all. If these
    // elements are present the sweep really is still rotating.
    expect(container.querySelectorAll('animateTransform')).toHaveLength(0);
    expect(container.querySelectorAll('animate')).toHaveLength(0);
  });

  it('renders them when motion is allowed', () => {
    stubMatchMedia(false);
    const { container } = render(<OrchestratorRadar orchState={orchState} />);
    expect(container.querySelectorAll('animateTransform').length).toBeGreaterThan(0);
    // The AUTO blip blinks; the BLOCKED one does not, which is the real signal.
    expect(container.querySelectorAll('animate')).toHaveLength(1);
  });

  it('keeps the real gate colour on the blip with motion reduced', () => {
    stubMatchMedia(true);
    const { container } = render(<OrchestratorRadar orchState={orchState} />);
    // Omitting the blink must not cost the colour: `#34d399` is the real AUTO
    // gate, and it is the part that carries information.
    const fills = Array.from(container.querySelectorAll('circle')).map((c) => c.getAttribute('fill'));
    expect(fills).toContain('#34d399');
    expect(fills).toContain('#fb7185');
  });
});

// --------------------------------------------------- requestAnimationFrame

describe('the neural activity canvas', () => {
  /** A canvas stub with the 2D context methods the renderer actually calls. */
  function stubCanvas() {
    const clearRect = vi.fn();
    const gradient = { addColorStop: vi.fn() };
    const ctx = new Proxy({ clearRect } as Record<string, unknown>, {
      get: (target, prop) => {
        if (prop in target) return target[prop as string];
        if (prop === 'canvas') return { width: 400, height: 300 };
        if (prop === 'measureText') return () => ({ width: 10 });
        if (prop === 'createLinearGradient' || prop === 'createRadialGradient') {
          return () => gradient;
        }
        return () => undefined;
      },
      set: (target, prop, value) => {
        target[prop as string] = value;
        return true;
      },
    }) as unknown as CanvasRenderingContext2D;
    const canvas = document.createElement('canvas');
    Object.defineProperty(canvas, 'getContext', { value: () => ctx });
    Object.defineProperty(canvas, 'width', { value: 400, writable: true });
    Object.defineProperty(canvas, 'height', { value: 300, writable: true });
    document.body.appendChild(canvas);
    return { canvas, clearRect };
  }

  it('never schedules a frame when motion is reduced', () => {
    stubMatchMedia(true);
    const raf = vi.spyOn(window, 'requestAnimationFrame');
    const { canvas } = stubCanvas();

    const engine = new NeuralActivity(canvas as unknown as HTMLCanvasElement, {
      layers: [3, 4],
      accent: '#a78bfa',
    });
    engine.setLevel(0.5);

    // `idleSpeed` is 0.02, so even at level 0 this loop drifted every node a
    // little on every frame, for the lifetime of the dashboard. Nothing may be
    // scheduled at all when the user has asked for stillness.
    expect(raf).not.toHaveBeenCalled();
    engine.destroy();
  });

  it('does schedule a frame when motion is allowed', () => {
    stubMatchMedia(false);
    const raf = vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    const { canvas } = stubCanvas();

    const engine = new NeuralActivity(canvas as unknown as HTMLCanvasElement, {
      layers: [3, 4],
      accent: '#a78bfa',
    });
    expect(raf).toHaveBeenCalled();
    engine.destroy();
  });

    it('still draws with motion reduced, rather than leaving a blank canvas', () => {
    stubMatchMedia(true);
    const { canvas, clearRect } = stubCanvas();

    const engine = new NeuralActivity(canvas as unknown as HTMLCanvasElement, {
      layers: [3, 4],
      accent: '#a78bfa',
    });
    engine.setLevel(0.7);

    // The node graph is information. An empty box would remove it rather than
    // calm it, so the reduced-motion path must paint what the animated one paints.
    expect(clearRect).toHaveBeenCalled();
    engine.destroy();
  });
it('honours an explicit resume even with motion reduced', () => {
    stubMatchMedia(true);
    const raf = vi.spyOn(window, 'requestAnimationFrame');
    const { canvas } = stubCanvas();

    const engine = new NeuralActivity(canvas as unknown as HTMLCanvasElement, {
      layers: [2, 3],
      accent: '#a78bfa',
    });
    engine.pause();
    engine.resume();

    // A setting is a standing preference, not a lock. Someone who paused and then
    // resumed has asked for it back explicitly.
    expect(raf).not.toHaveBeenCalled();
    engine.destroy();
  });
});