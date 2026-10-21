// The hero's sakura tree, a Somei-Yoshino seen close up and grown once from a fixed seed: big
// wood laid out by hand (lib/hero-tree.ts), grown branches and twigs along it, blossom in bunches
// of three to five flowers, and petals in the air and on the ground. The live scene
// (components/landing/tree-scene.ts) draws it in three.js, and the hero's posters
// (public/hero-*.webp) are that scene rendered once. No three.js here.

export type V3 = [number, number, number];
/** One straight piece of a limb; `depth` is its branching level, 0 for the laid-out wood. */
export type Segment = { a: V3; b: V3; r0: number; r1: number; depth: number };
/** One flower: where, which way it faces, how big, and its baked colour (0–1 RGB). */
export type Flower = { p: V3; n: V3; size: number; spin: number; rgb: V3 };
/** A petal on the ground. */
export type Fallen = { x: number; z: number; spin: number; rgb: V3 };
/** A blossom nearer the camera, a little out of focus. */
export type Near = { p: V3; size: number; spin: number; rgb: V3 };

/**
 * Per theme: the blossom's brightness, the buds, the young twigs' red-brown, and the tint over
 * the scanned bark of the big wood.
 */
export const TREE_COLORS = {
  light: { bloom: 1, bud: "#b8405f", twig: "#6e4c45", bark: "#e6dcd8" },
  dark: { bloom: 0.9, bud: "#d0587a", twig: "#5a423d", bark: "#a0928e" },
} as const;
export type TreeTheme = keyof typeof TREE_COLORS;

/** Where the light comes from: up, left and toward the viewer. */
export const LIGHT: V3 = norm([-0.45, 1, 0.4]);

function rng(seed: number) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function add(a: V3, b: V3, k = 1): V3 {
  return [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k];
}
export function norm(a: V3): V3 {
  const l = Math.hypot(...a) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
}
const scaleV = (a: V3, k: number): V3 => [a[0] * k, a[1] * k, a[2] * k];
const dot = (a: V3, b: V3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const mix = (a: V3, b: V3, k: number): V3 => [
  a[0] + (b[0] - a[0]) * k,
  a[1] + (b[1] - a[1]) * k,
  a[2] + (b[2] - a[2]) * k,
];
const hex = (h: string): V3 => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255) as V3;

// Somei-Yoshino: nearly white petals with a pink blush, deeper pink toward the centre.
const PETAL_TONES = ["#fff3f6", "#fde4ec", "#fbd2de", "#f7bccd", "#f2a2b9"].map(hex);
/** What a petal turns toward in shade. */
const SHADOW = hex("#d98aa3");

/** How to grow the tree. */
export type GrowOpts = {
  seed: number;
  /** The deepest branching level; its limbs end in twig tips. */
  maxDepth: number;
  /** Side twigs: from which depth and how likely on each limb. */
  twigs: { from: number; p: number };
  /**
   * Blossom as a Somei-Yoshino carries it: bunches of 3–5 flowers on short stalks all along the
   * twigs from depth `from`, one every `spacing`, each flower `size` across, within `spread` of
   * the twig; `pale` lifts the tones toward white.
   */
  sprays: { from: number; spacing: number; size: number; spread: number; pale: number };
  /**
   * The big wood, laid by hand: each limb a polyline with a radius at every point. Along it,
   * every `every`, a grown branch of depth `depth` and length about `len` leaves along the limb,
   * pulled toward `up` and `wild` crooked.
   */
  scaffold: {
    pts: V3[];
    r: number[];
    sprout?: { every: number; len: number; depth: number; up: V3; wild: number };
  }[];
  /** Petals in the air: `n` of them, `size` long, falling through this box. */
  air: { n: number; x: [number, number]; y: [number, number]; z: [number, number]; size: number };
  /** Petals on the ground: `n`, spread over x and z. */
  ground: { n: number; x: [number, number]; z: [number, number] };
};

export function grow(o: GrowOpts) {
  const MAX_DEPTH = o.maxDepth;
  const r = rng(o.seed);
  const rand3 = (k: number): V3 => [(r() - 0.5) * k, (r() - 0.5) * k, (r() - 0.5) * k];
  const segments: Segment[] = [];
  const tips: { p: V3; depth: number }[] = [];

  // A grown branch: a few crooked steps that sag under their weight, a side twig now and then,
  // and two or three forks, down to twig tips at MAX_DEPTH.
  const limb = (start: V3, dir: V3, len: number, radius: number, depth: number) => {
    const steps = depth < 3 ? 4 : 3;
    const gnarl = depth < 3 ? 0.42 : 0.55;
    // Gravity: deeper limbs sag more, and more toward their ends.
    const droop = [0, 0.02, 0.05, 0.09, 0.14, 0.2, 0.26, 0.32][depth];
    const end = radius * (depth === MAX_DEPTH ? 0.45 : 0.72);
    let p = start;
    let d = dir;
    const joints: { p: V3; d: V3 }[] = [];
    for (let s = 1; s <= steps; s++) {
      d = norm(add(add(d, rand3(gnarl)), [0, -droop * (s / steps), 0]));
      const q = add(p, d, len / steps);
      const t0 = (s - 1) / steps;
      const t1 = s / steps;
      segments.push({
        a: p,
        b: q,
        r0: radius + (end - radius) * t0,
        r1: radius + (end - radius) * t1,
        depth,
      });
      p = q;
      joints.push({ p, d });
    }
    if (depth >= 4) tips.push({ p: joints[Math.floor(steps / 2)].p, depth });
    if (depth === MAX_DEPTH) {
      tips.push({ p, depth: depth + 1 });
      return;
    }
    // Side twigs off the outer limbs, so the crown fills in between the forks.
    if (depth >= o.twigs.from && r() < o.twigs.p) {
      const j = joints[Math.floor(r() * (steps - 1))];
      const side = norm(add(add(j.d, rand3(1.6)), [0, 0.3, 0]));
      limb(j.p, side, len * 0.45, end * 0.7, Math.min(MAX_DEPTH, depth + 2));
    }
    const n = r() < 0.4 ? 3 : 2;
    const turn = r() * Math.PI * 2;
    for (let i = 0; i < n; i++) {
      const az = turn + (i / n) * Math.PI * 2 + (r() - 0.5) * 0.8;
      // Spread around the parent, and pulled outward from the trunk: an umbrella, not a cone.
      const around: V3 = [Math.cos(az), 0, Math.sin(az) * 0.75];
      const radial = norm([p[0] + around[0] * 0.3, 0, p[2] + around[2] * 0.3]);
      const spread = 0.5 + r() * 0.35;
      let c = norm(add(d, around, spread));
      const outward = depth < 3 ? 0.55 : 0.3;
      c = norm(add(mix(c, radial, outward), [0, depth < 3 ? 0.3 : 0.12, 0]));
      const main = i === 0;
      limb(p, c, len * (0.7 + r() * 0.14), end * (main ? 0.78 : 0.64 + r() * 0.08), depth + 1);
    }
  };
  for (const s of o.scaffold) {
    let carry = s.sprout ? r() * s.sprout.every : 0;
    for (let k = 1; k < s.pts.length; k++) {
      const [a, b] = [s.pts[k - 1], s.pts[k]];
      segments.push({ a, b, r0: s.r[k - 1], r1: s.r[k], depth: 0 });
      const sp = s.sprout;
      if (!sp) continue;
      const along = add(b, a, -1);
      const len = Math.hypot(...along);
      let t = carry;
      for (; t < len; t += sp.every * (0.6 + 0.8 * r())) {
        const f = t / len;
        const rad = (s.r[k - 1] + (s.r[k] - s.r[k - 1]) * f) * 0.3;
        const dir = norm(add(add(norm(along), sp.up), rand3(sp.wild)));
        limb(mix(a, b, f), dir, sp.len * (0.7 + 0.6 * r()), rad, sp.depth);
      }
      carry = t - len;
    }
  }

  // Bunches all along the twigs and at their tips; a few tips carry dark buds instead.
  const flowers: Flower[] = [];
  const buds: V3[] = [];
  const sp = o.sprays;
  const top = Math.max(...segments.map((g) => g.b[1])) || 1;
  const bunch = (at: V3) => {
    const m = 3 + Math.floor(r() * 3);
    const height = Math.min(1, Math.max(0, at[1] / top));
    // Each bunch its own pink, from nearly white to the blush of an opening flower.
    const tone = PETAL_TONES[Math.floor(r() ** 1.4 * PETAL_TONES.length)];
    for (let j = 0; j < m; j++) {
      // Flowers face out and up from their stalk, lit from above and paler where lit.
      const n = norm(add([0, 0.45, 0.6], rand3(1.6)));
      const lit = Math.max(0, dot(n, LIGHT)) * 0.5 + height * 0.5;
      const base = mix(tone, [1, 1, 1], sp.pale * (0.6 + 0.4 * lit));
      flowers.push({
        p: add(at, rand3(sp.spread * 0.9)),
        n,
        size: sp.size * (0.8 + r() * 0.4),
        spin: r() * Math.PI * 2,
        rgb: mix(mix(SHADOW, [1, 1, 1], sp.pale * 0.3), base, 0.4 + 0.6 * lit),
      });
    }
  };
  for (const g of segments) {
    if (g.depth < sp.from) continue;
    const len = Math.hypot(g.b[0] - g.a[0], g.b[1] - g.a[1], g.b[2] - g.a[2]);
    const n = Math.floor(len / sp.spacing + r());
    for (let k = 0; k < n; k++) {
      const t = r();
      const at = add(mix(g.a, g.b, t), [0, sp.spread * 0.5, 0]);
      bunch(add(at, rand3(sp.spread)));
    }
  }
  for (const t of tips) {
    if (t.depth <= MAX_DEPTH) continue;
    if (r() < 0.01) {
      for (let k = 0; k < 3; k++) buds.push(add(t.p, rand3(0.05)));
      continue;
    }
    for (let k = 0; k < 2; k++) bunch(add(t.p, rand3(sp.spread * 1.5)));
  }
  // Far first.
  flowers.sort((a, b) => a.p[2] - b.p[2]);

  const span = (v: [number, number]) => v[0] + (v[1] - v[0]) * r();
  // The pinker tones on the ground, where they show against the page.
  const fallen: Fallen[] = Array.from({ length: o.ground.n }, () => ({
    x: span(o.ground.x),
    z: span(o.ground.z),
    spin: r() * Math.PI * 2,
    rgb: PETAL_TONES[2 + Math.floor(r() * 3)],
  }));
  const aloft: Flower[] = Array.from({ length: o.air.n }, () => ({
    p: [span(o.air.x), span(o.air.y), span(o.air.z)],
    n: norm(rand3(2)),
    size: o.air.size,
    spin: r() * Math.PI * 2,
    rgb: PETAL_TONES[1 + Math.floor(r() * 4)],
  }));
  // Where the wind sways around (components/landing/tree-scene.ts): the middle of the blossom.
  const centre = scaleV(
    flowers.reduce<V3>((m, f) => add(m, f.p), [0, 0, 0]),
    1 / flowers.length,
  );
  // `near` is placed by the framing (lib/hero-tree.ts); the big wood the scene builds whole.
  const near: Near[] = [];
  const limbs = o.scaffold.map((s) => ({ pts: s.pts, r: s.r }));
  return { segments, flowers, buds, fallen, centre, aloft, air: o.air, near, limbs };
}

export type Tree = ReturnType<typeof grow>;

/** A perspective camera looking straight down -z, and the poster it frames. */
export type Camera = { fov: number; eye: V3; poster: { w: number; h: number } };

/** Five petals with the Sakura Seal's notch at each tip, around (0, 0), radius ≤ 1. */
export function flowerOutline(steps = 60): [number, number][] {
  return Array.from({ length: steps }, (_, i) => {
    const a = (i / steps) * Math.PI * 2;
    const lobe = Math.abs(Math.cos(2.5 * a));
    const notch = Math.max(0, Math.cos(10 * (a - Math.PI / 10))) ** 24 * 0.14;
    const r = 0.38 + 0.62 * lobe ** 0.7 - notch;
    return [Math.sin(a) * r, -Math.cos(a) * r];
  });
}

/** One loose petal: the Seal's petal, a notched teardrop, about 1 tall. */
export function petalOutline(steps = 28): [number, number][] {
  return Array.from({ length: steps }, (_, i) => {
    const a = (i / steps) * Math.PI * 2;
    const notch = Math.max(0, Math.cos(a)) ** 30 * 0.18;
    const w = 0.42 * Math.sin(a) * (0.75 + 0.25 * Math.cos(a));
    return [w, Math.cos(a) * (0.5 - notch) + 0.1 * Math.cos(a) ** 2];
  });
}

/** Where a world point lands on the poster, in poster units. */
export function project(p: V3, cam: Camera) {
  const [ex, ey, ez] = cam.eye;
  const { w, h } = cam.poster;
  const half = Math.tan((cam.fov * Math.PI) / 360) * (ez - p[2]);
  return {
    x: ((p[0] - ex) / (half * (w / h)) + 1) * (w / 2),
    y: (1 - (p[1] - ey) / half) * (h / 2),
  };
}
