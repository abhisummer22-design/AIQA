/**
 * NeuralActivity — realtime neural-network activity visualizer (canvas 2D,
 * no dependencies). TypeScript port of a class the user supplied directly
 * (`demo.html`/`NeuralActivity.jsx`), adapted to this codebase's types and
 * the `utils/` convention (`statusLabel.ts`) rather than a new top-level
 * folder — the animation/drawing logic itself is unchanged from what was
 * given.
 *
 * Usage:
 *   const nn = new NeuralActivity(canvasEl, { accent: '#facc15' });
 *   nn.setLevel(0.7);              // overall activity 0..1 (drives speed + glow)
 *   nn.pulse();                    // one-shot burst (e.g. on every real event)
 *   nn.setInputs([0.2, 0.9, 0.4, 0.6, ...]); // realtime values for the first layer
 *   nn.setAccent('#4be08a');
 *   nn.destroy();
 */

export interface NeuralActivityOptions {
  /** Nodes per layer, left -> right. */
  layers: number[];
  accent: string;
  /** Random drift speed when level = 0. */
  idleSpeed: number;
  /** Random drift speed when level = 1. */
  activeSpeed: number;
  minRadius: number;
  maxRadius: number;
  /** e.g. '#0a121d' to paint a solid bg; `null` = transparent. */
  background: string | null;
  /** Travelling dots along edges. */
  showSignals: boolean;
}

/** `hex[l][i]` — a real per-node colour override (falls back to `accent`
 *  when absent for that node). An edge is drawn in its source node's own
 *  colour, so a real per-node colour also colours the mesh flowing out of
 *  it, not just that one dot. */
export type NodeColorGrid = (string | null | undefined)[][];
/** `text[l][i]` — a real, short per-node label, drawn beside the node. */
export type NodeLabelGrid = (string | null | undefined)[][];

interface NeuralNode {
  l: number;
  i: number;
  act: number;
  target: number;
}

interface Signal {
  a: NeuralNode;
  b: NeuralNode;
  t: number;
  speed: number;
}

const DEFAULT_OPTIONS: NeuralActivityOptions = {
  layers: [4, 6, 6, 3],
  accent: '#facc15',
  idleSpeed: 0.02,
  activeSpeed: 0.14,
  minRadius: 4,
  maxRadius: 11,
  background: null,
  showSignals: true,
};

/** Most device pixels per CSS pixel the canvas is drawn at; a canvas this small costs nothing at 3x. */
const MAX_DEVICE_SCALE = 3;

/**
 * The bitmap size (and the transform onto it) for a canvas that occupies `cssW` x `cssH` CSS pixels.
 *
 * The bitmap has to cover exactly the device pixels the element occupies, or the browser resamples it onto
 * them and everything drawn on it, text included, comes out soft. Rounding the size to whole CSS pixels first
 * and then multiplying by the display scaling does not manage that: with 125% or 150% scaling, or in a layout
 * whose column is not a whole number of pixels wide, the bitmap ends up a fraction of a device pixel off.
 *
 * `device` is the exact device-pixel size the browser reports for the element (`ResizeObserver`'s
 * `device-pixel-content-box`). It is used only when it agrees with `css * dpr` to within a couple of device
 * pixels, since a browser under display-scale emulation can report a size that belongs to another scale; the
 * fractional CSS size times `dpr` is used otherwise. `scaleX`/`scaleY` map the CSS-pixel coordinates the
 * drawing code works in onto the bitmap.
 */
export function canvasBackingSize(
  cssW: number,
  cssH: number,
  dpr: number,
  device?: { width: number; height: number } | null,
): { width: number; height: number; scaleX: number; scaleY: number } {
  const fit = (css: number, exact: number | undefined) => {
    const estimate = css * dpr;
    const wanted = exact && Math.abs(exact - estimate) <= 1.5 ? exact : estimate;
    return Math.max(1, Math.min(Math.round(wanted), Math.round(css * MAX_DEVICE_SCALE)));
  };
  const width = fit(cssW, device?.width);
  const height = fit(cssH, device?.height);
  return { width, height, scaleX: width / cssW, scaleY: height / cssH };
}

/**
 * Whether the OS has asked for reduced motion, read fresh each time it is asked.
 *
 * This module is a plain class, not a component, so it cannot use
 * `usePrefersReducedMotion`. It is also a library that outlives any single
 * render: it is driven by `requestAnimationFrame`, not by CSS, so the blanket
 * `prefers-reduced-motion` rule in `index.css` cannot reach it. Reading it
 * through a function rather than caching it at module load means a user who
 * changes the setting while the dashboard is open is still obeyed.
 */
function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

export class NeuralActivity {
  private canvas: HTMLCanvasElement;
  /** `null` in an environment with no real 2D canvas support (some jsdom
   *  test setups included) — every method below degrades to a safe no-op
   *  rather than throwing, so a real, unrelated rendering-environment gap
   *  never crashes the app around this purely-visual component. */
  private ctx: CanvasRenderingContext2D | null;
  private opts: NeuralActivityOptions;

  private level = 0;
  private pulseEnergy = 0;
  private inputs: number[] | null = null;
  private nodes: NeuralNode[] = [];
  private signals: Signal[] = [];
  private running = true;

  private nodeColors: NodeColorGrid | null = null;
  private nodeLabels: NodeLabelGrid | null = null;
  private layerLabels: (string | null | undefined)[] | null = null;
  private rgbCache = new Map<string, [number, number, number]>();

  private rgb: [number, number, number];
  private resizeObserver: ResizeObserver | null;
  private raf = 0;
  private w = 0;
  private h = 0;

  constructor(canvas: HTMLCanvasElement, opts: Partial<NeuralActivityOptions> = {}) {
    if (!canvas || canvas.tagName !== 'CANVAS') {
      throw new Error('NeuralActivity needs a <canvas> element');
    }
    this.canvas = canvas;
    // A real, environment-level gap (not a bug in this class) — some
    // renderers (certain jsdom test setups, some locked-down sandboxes)
    // do not implement 2D canvas at all. Recorded, not thrown: the whole
    // engine below simply draws nothing rather than taking the app down.
    this.ctx = canvas.getContext('2d');
    // Filters out explicit `undefined` values (e.g. an optional React prop
    // passed straight through unset) before merging — a plain object
    // spread would otherwise let `{ layers: undefined }` clobber
    // `DEFAULT_OPTIONS.layers` with `undefined` instead of falling back to
    // it, since spread copies own keys regardless of their value.
    const definedOpts = Object.fromEntries(Object.entries(opts).filter(([, v]) => v !== undefined));
    this.opts = { ...DEFAULT_OPTIONS, ...definedOpts };

    this.opts.layers.forEach((n, l) => {
      for (let i = 0; i < n; i++) this.nodes.push({ l, i, act: Math.random(), target: Math.random() });
    });

    this.rgb = NeuralActivity.hexToRgb(this.opts.accent);
    this.resizeObserver =
      typeof ResizeObserver !== 'undefined'
        ? new ResizeObserver((entries) => this.resize(entries[entries.length - 1]))
        : null;
    this.observeCanvas();
    this.resize();
    // Reduced motion: draw exactly one frame and never start the loop.
    //
    // The canvas is still drawn — an empty box would remove the visual entirely,
    // and the node layout is information, not decoration. What is suppressed is
    // the drift: `idleSpeed` is 0.02, so even at `level = 0` with nothing
    // happening anywhere in the system this loop was moving every node a little
    // on every frame, forever, for the lifetime of the dashboard.
    if (this.ctx && !prefersReducedMotion()) {
      this.raf = requestAnimationFrame(() => this.frame());
    } else if (this.ctx) {
      this.drawOnce();
    }
  }

  /* ---------- public API ---------- */
  setLevel(v: number): this {
    this.level = Math.max(0, Math.min(1, Number(v) || 0));
    return this;
  }

  pulse(strength = 1): this {
    this.pulseEnergy = Math.min(1.5, this.pulseEnergy + strength);
    this.spawnSignals(6);
    return this;
  }

  setInputs(arr: number[] | null | undefined): this {
    this.inputs = Array.isArray(arr) ? arr.map((v) => Math.max(0, Math.min(1, Number(v) || 0))) : null;
    return this;
  }

  setAccent(hex: string): this {
    this.opts.accent = hex;
    this.rgb = NeuralActivity.hexToRgb(hex);
    return this;
  }

  /** Real, per-node colour overrides (`grid[l][i]`) — `null`/absent falls
   *  back to `accent` for that node. Pass `null` to clear all overrides. */
  setNodeColors(grid: NodeColorGrid | null): this {
    this.nodeColors = grid;
    return this;
  }

  /** Real, short per-node labels (`grid[l][i]`), drawn beside the node.
   *  Pass `null` to clear all labels. */
  setNodeLabels(grid: NodeLabelGrid | null): this {
    this.nodeLabels = grid;
    return this;
  }

  /** Real, one-per-layer column labels (e.g. "INPUT" / "PROVIDERS" /
   *  "OUTPUT"), drawn at the bottom of each layer's column. Pass `null`
   *  to clear. */
  setLayerLabels(labels: (string | null | undefined)[] | null): this {
    this.layerLabels = labels;
    return this;
  }

  pause(): void {
    this.running = false;
  }

  resume(): void {
    if (!this.running && this.ctx) {
      this.running = true;
      if (prefersReducedMotion()) {
        // Someone who paused the animation and then resumed it has asked for it
        // back explicitly. Honouring that is the whole point of having a resume:
        // a setting is a standing preference, not a lock.
        this.drawOnce();
      } else {
        this.raf = requestAnimationFrame(() => this.frame());
      }
    }
  }

  destroy(): void {
    this.running = false;
    cancelAnimationFrame(this.raf);
    this.resizeObserver?.disconnect();
  }

  /* ---------- internals ---------- */
  private static hexToRgb(hex: string): [number, number, number] {
    let h = hex.replace('#', '');
    if (h.length === 3) h = h.split('').map((c) => c + c).join('');
    const parsed = [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
    return [parsed[0] || 0, parsed[1] || 0, parsed[2] || 0];
  }

  /** A real per-node colour override for `n`, if one was set — cached
   *  per hex string since this runs for every node, every frame. */
  private rgbForNode(n: NeuralNode): [number, number, number] {
    const hex = this.nodeColors?.[n.l]?.[n.i];
    if (!hex) return this.rgb;
    let parsed = this.rgbCache.get(hex);
    if (!parsed) {
      parsed = NeuralActivity.hexToRgb(hex);
      this.rgbCache.set(hex, parsed);
    }
    return parsed;
  }

  /** Watches the canvas's device-pixel size where the browser can report it, so a change of display scaling
   *  redraws the bitmap too; its CSS size otherwise. */
  private observeCanvas(): void {
    if (!this.resizeObserver) return;
    try {
      this.resizeObserver.observe(this.canvas, { box: 'device-pixel-content-box' });
    } catch {
      this.resizeObserver.observe(this.canvas);
    }
  }

  private resize(entry?: ResizeObserverEntry): void {
    // The size as laid out, fractional: `clientWidth` is rounded to a whole CSS pixel, which is what left the
    // bitmap a fraction of a device pixel off the element (see `canvasBackingSize`).
    const w = entry?.contentRect.width || this.canvas.clientWidth || 300;
    const h = entry?.contentRect.height || this.canvas.clientHeight || 200;
    const box = entry?.devicePixelContentBoxSize?.[0];
    const size = canvasBackingSize(
      w,
      h,
      window.devicePixelRatio || 1,
      box ? { width: box.inlineSize, height: box.blockSize } : null,
    );
    this.canvas.width = size.width;
    this.canvas.height = size.height;
    this.ctx?.setTransform(size.scaleX, 0, 0, size.scaleY, 0, 0);
    this.w = w;
    this.h = h;
  }

  /** Vertical margins reserved for `layerLabels` (top) and a caller's own
   *  overlay/last-layer decision label (bottom) — nodes are distributed
   *  within the remaining band, never flush against an edge a label might
   *  occupy. Small enough to be a non-event for a caller using neither. */
  private static readonly TOP_MARGIN = 14;
  private static readonly BOTTOM_MARGIN = 20;
  /** A real, deterministic layout aid, not random jitter: a node sharing a
   *  layer with the real active node (see `setActiveNode`) is nudged a
   *  little further away from it, purely so the two don't read as visually
   *  merged together when a layer has few nodes. Never applied to the
   *  active node itself. */
  private static readonly ACTIVE_SEPARATION_PX = 10;

  private pos(n: NeuralNode): [number, number] {
    const L = this.opts.layers;
    const top = NeuralActivity.TOP_MARGIN;
    const band = Math.max(1, this.h - NeuralActivity.TOP_MARGIN - NeuralActivity.BOTTOM_MARGIN);
    let y = top + ((n.i + 0.5) / L[n.l]) * band;
    if (this.activeNode && n.l === this.activeNode.l && n.i !== this.activeNode.i) {
      y += n.i > this.activeNode.i ? NeuralActivity.ACTIVE_SEPARATION_PX : -NeuralActivity.ACTIVE_SEPARATION_PX;
    }
    return [((n.l + 0.5) / L.length) * this.w, y];
  }

  /** Real per-node coordinate marking the one node genuinely on the real
   *  "winning" path (e.g. the actually-routed provider) — `null` when
   *  there is no real active path at all. Edges/signals only ever animate
   *  along edges that touch this node; every other structurally-real
   *  connection (e.g. to a discovered-but-unused provider) stays a flat,
   *  static line — motion must mean this specific real thing is happening,
   *  not just that a connection structurally exists. */
  private activeNode: { l: number; i: number } | null = null;

  setActiveNode(node: { l: number; i: number } | null): this {
    this.activeNode = node;
    return this;
  }

  private isNodeActive(n: NeuralNode): boolean {
    return !!this.activeNode && n.l === this.activeNode.l && n.i === this.activeNode.i;
  }

  /** True only for an edge that genuinely touches the real active node —
   *  i.e. really is part of the real decision, not merely a structural
   *  connection every node in adjacent layers happens to have. */
  private isEdgeActive(n: NeuralNode, m: NeuralNode): boolean {
    return this.isNodeActive(n) || this.isNodeActive(m);
  }

  /** Signals only ever travel the two real hops touching the active node
   *  (into it, or out of it) — never a random structural edge that is not
   *  actually part of the real decision. No active node -> nothing
   *  genuinely flows, honestly. */
  private spawnSignals(count: number): void {
    if (!this.activeNode) return;
    const active = this.nodes.find((n) => this.isNodeActive(n));
    if (!active) return;
    const incoming = this.nodes.filter((n) => n.l === active.l - 1);
    const outgoing = this.nodes.filter((n) => n.l === active.l + 1);
    for (let k = 0; k < count; k++) {
      const useIncoming = incoming.length && (!outgoing.length || Math.random() < 0.5);
      if (useIncoming) {
        const a = incoming[Math.floor(Math.random() * incoming.length)];
        this.signals.push({ a, b: active, t: 0, speed: 0.02 + Math.random() * 0.03 });
      } else if (outgoing.length) {
        const b = outgoing[Math.floor(Math.random() * outgoing.length)];
        this.signals.push({ a: active, b, t: 0, speed: 0.02 + Math.random() * 0.03 });
      }
    }
  }

  /** One update-and-draw pass, then schedules the next. This is the loop. */
  private frame(): void {
    if (!this.running || !this.ctx) return;
    const energy = this.step();
    this.draw(energy);
    this.raf = requestAnimationFrame(() => this.frame());
  }

  /**
   * Advance node activations, signal spawning and pulse decay by one tick.
   *
   * Returns the energy value this tick used. It is handed to `draw` rather than
   * recomputed there because `pulseEnergy` is decayed at the end of every tick,
   * so recomputing would draw edges with a marginally different intensity than
   * the tick that produced them -- a subtle difference, but exactly the kind that
   * makes a refactor look like a behaviour change.
   */
  private step(): number {
    const { opts } = this;
    const energy = Math.min(1, this.level + this.pulseEnergy);
    const speed = opts.idleSpeed + (opts.activeSpeed - opts.idleSpeed) * energy;

    // update activations
    this.nodes.forEach((n) => {
      if (n.l === 0 && this.inputs && this.inputs[n.i] != null) {
        n.act += (this.inputs[n.i] - n.act) * 0.2; // follow realtime input smoothly
      } else {
        if (Math.random() < 0.05) n.target = Math.random() * (0.5 + energy * 0.5);
        n.act += (n.target - n.act) * speed;
      }
    });
    if (energy > 0.3 && opts.showSignals && Math.random() < energy * 0.25) this.spawnSignals(1);
    this.pulseEnergy *= 0.94;
    return energy;
  }

  /**
   * Paint the current state. Reads state, writes pixels, mutates nothing -- so
   * the reduced-motion path renders through exactly this code and therefore
   * shows exactly what the animated path shows, minus the movement.
   */
  private draw(energy: number): void {
    if (!this.ctx) return;
    const { ctx, w, h, opts } = this;

    ctx.clearRect(0, 0, w, h);
    if (opts.background) {
      ctx.fillStyle = opts.background;
      ctx.fillRect(0, 0, w, h);
    }

    // edges. A real structural connection exists between every node in
    // adjacent layers (every capability signal genuinely feeds every
    // provider's real score) — but only the edges actually touching the
    // real active node (see `setActiveNode`) get the energetic, source-
    // coloured, activation-scaled treatment. Every other edge is a flat,
    // dim, static line: it is real (the connection exists), but nothing
    // is actually flowing through it right now, so it must not look like
    // it is — the same "real mesh, only the real winner animates"
    // distinction the earlier SVG diagram made explicit per-edge.
    this.nodes.forEach((n) => {
      if (n.l === opts.layers.length - 1) return;
      const [x1, y1] = this.pos(n);
      this.nodes.forEach((m) => {
        if (m.l !== n.l + 1) return;
        const [x2, y2] = this.pos(m);
        ctx.beginPath();
        ctx.moveTo(x1, y1);
        ctx.lineTo(x2, y2);
        if (this.isEdgeActive(n, m)) {
          const [nr, ng, nb] = this.rgbForNode(n);
          ctx.strokeStyle = `rgba(${nr},${ng},${nb},${0.05 + n.act * m.act * (0.35 + energy * 0.4)})`;
        } else {
          ctx.strokeStyle = 'rgba(100,116,139,0.12)'; // flat slate — structurally real, not currently active
        }
        ctx.lineWidth = 1;
        ctx.stroke();
      });
    });

    // travelling signals
    if (opts.showSignals) {
      this.signals = this.signals.filter((s) => s.t <= 1);
      this.signals.forEach((s) => {
        const [x1, y1] = this.pos(s.a);
        const [x2, y2] = this.pos(s.b);
        const x = x1 + (x2 - x1) * s.t;
        const y = y1 + (y2 - y1) * s.t;
        const [sr, sg, sb] = this.rgbForNode(s.a);
        ctx.beginPath();
        ctx.arc(x, y, 2.5, 0, Math.PI * 2);
        ctx.fillStyle = `rgba(${sr},${sg},${sb},${1 - s.t * 0.5})`;
        ctx.fill();
        s.t += s.speed;
      });
    }

    // nodes — each in its own real colour when one was set for it,
    // otherwise the shared accent. A "middle" node (neither the real
    // capability inputs nor the single output — in this app's real usage,
    // a discovered provider) gets a small real glow floor: it structurally
    // exists right now, so it must never read as fully dead, even when it
    // is not the active winning path (which stays visually distinct via
    // its own edges/signals, per `isEdgeActive`, not via this floor).
    // Layer 0's glow is deliberately left alone — there, low opacity IS
    // the real signal for a genuinely weak/absent capability, and boosting
    // it would blur that real distinction.
    const isMiddleLayer = (n: NeuralNode) => n.l > 0 && n.l < opts.layers.length - 1;
    this.nodes.forEach((n) => {
      const [x, y] = this.pos(n);
      const [nr, ng, nb] = this.rgbForNode(n);
      const glowFloor = isMiddleLayer(n) ? 0.14 : 0;
      const fillFloor = isMiddleLayer(n) ? 0.4 : 0.3;
      const rad = opts.minRadius + n.act * (opts.maxRadius - opts.minRadius);
      const glow = ctx.createRadialGradient(x, y, 0, x, y, rad * 2.2);
      glow.addColorStop(0, `rgba(${nr},${ng},${nb},${glowFloor + 0.25 * n.act})`);
      glow.addColorStop(1, `rgba(${nr},${ng},${nb},0)`);
      ctx.fillStyle = glow;
      ctx.beginPath();
      ctx.arc(x, y, rad * 2.2, 0, Math.PI * 2);
      ctx.fill();
      ctx.beginPath();
      ctx.arc(x, y, rad, 0, Math.PI * 2);
      ctx.fillStyle = `rgba(${nr},${ng},${nb},${fillFloor + n.act * (1 - fillFloor)})`;
      ctx.fill();
    });

    // real, per-node labels — a fixed, legible opacity (not tied to the
    // node's own flickering activation) in that node's own real colour, so
    // a viewer can always tell which node is which regardless of how
    // "lit" it currently is.
    if (this.nodeLabels) {
      const lastLayer = opts.layers.length - 1;
      ctx.font = '8px ui-monospace, "Share Tech Mono", monospace';
      this.nodes.forEach((n) => {
        const label = this.nodeLabels?.[n.l]?.[n.i];
        if (!label) return;
        const [x, y] = this.pos(n);
        const [nr, ng, nb] = this.rgbForNode(n);
        ctx.fillStyle = `rgba(${nr},${ng},${nb},0.9)`;
        if (n.l === 0) {
          ctx.textAlign = 'right';
          ctx.textBaseline = 'middle';
          ctx.fillText(label, x - opts.maxRadius - 4, y);
        } else if (n.l === lastLayer) {
          ctx.textAlign = 'left';
          ctx.textBaseline = 'middle';
          ctx.fillText(label, x + opts.maxRadius + 4, y);
        } else {
          ctx.textAlign = 'center';
          ctx.textBaseline = 'top';
          ctx.fillText(label, x, y + opts.maxRadius + 3);
        }
      });
    }

    // real, one-per-layer column labels (e.g. "INPUT" / "PROVIDERS") — at
    // the top, deliberately clear of any real per-node label drawn below
    // the last layer's own node (a caller may put the real decision text
    // there instead of a generic "OUTPUT").
    if (this.layerLabels) {
      ctx.font = '8px ui-monospace, "Share Tech Mono", monospace';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'top';
      ctx.fillStyle = 'rgba(148,163,184,0.85)'; // slate-400 — structural, not node data
      opts.layers.forEach((_count, l) => {
        const label = this.layerLabels?.[l];
        if (!label) return;
        const x = ((l + 0.5) / opts.layers.length) * w;
        ctx.fillText(label, x, 2);
      });
    }
  }

  /**
   * One update-and-draw pass with no loop scheduled behind it.
   *
   * Called when reduced motion is on. It runs the same body as `frame()` -- so
   * the canvas shows the real current state rather than a blank or a placeholder
   * -- and then simply does not ask for the next frame. Duplicating the body was
   * the alternative and is worse: two copies of the drawing code drift, and the
   * reduced-motion path would end up rendering something subtly different from
   * the animated one, which is exactly when a visual becomes a lie.
   *
   * `setLevel`/`setInputs`/`pulse` all still work against it: they mutate state
   * and the next explicit `drawOnce`/`resume` reflects them.
   */
  private drawOnce(): void {
    if (!this.ctx) return;
    this.draw(this.step());
  }
}
