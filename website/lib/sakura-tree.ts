// The hero's sakura tree, a Somei-Yoshino grown once from a fixed seed: a thick, leaning, gnarled
// trunk; wide, low limbs that fork six times, thin out and droop at the tips into an umbrella
// crown; blossoms in dense, shaded clusters. The server draws it as an SVG poster
// (lib/tree-poster.ts) and the live scene (components/landing/tree-scene.ts) builds the same
// branches and blossoms in three.js through the same camera, so the two line up when one fades
// into the other. No three.js here: this module ships with the poster route and the scene chunk.

export type V3 = [number, number, number];
/** One straight piece of a limb; `depth` is its branching level, 0 for the trunk. */
export type Segment = { a: V3; b: V3; r0: number; r1: number; depth: number };
/** A blossom cluster: a soft cloud of flowers around `c`, `r` wide. */
export type Cluster = { c: V3; r: number; shade: number };
/** One flower: where, which way it faces, how big, and its baked colour (0–1 RGB). */
export type Flower = { p: V3; n: V3; size: number; spin: number; rgb: V3 };
/** A petal on the ground. */
export type Fallen = { x: number; z: number; spin: number; rgb: V3 };

/** The poster's box; the hero reserves this aspect ratio (25/21). */
export const POSTER = { w: 1000, h: 840 };

/** Bark, ground and the blossom brightness per theme, for the poster files and the live scene. */
export const TREE_COLORS = {
  light: { bark: "#4a3436", barkLit: "#6e5357", ground: "#efe6e9", bloom: 1, bud: "#b8405f" },
  dark: { bark: "#7d676c", barkLit: "#9c858a", ground: "#2a2527", bloom: 0.9, bud: "#d0587a" },
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
function norm(a: V3): V3 {
  const l = Math.hypot(...a) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
}
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
const MAX_DEPTH = 6;

function grow() {
  const r = rng(20261021);
  const rand3 = (k: number): V3 => [(r() - 0.5) * k, (r() - 0.5) * k, (r() - 0.5) * k];
  const segments: Segment[] = [];
  const tips: { p: V3; d: V3; depth: number }[] = [];

  const limb = (start: V3, dir: V3, len: number, radius: number, depth: number) => {
    const steps = depth === 0 ? 5 : depth < 3 ? 4 : 3;
    const gnarl = depth === 0 ? 0.32 : depth < 3 ? 0.42 : 0.55;
    // Gravity: deeper limbs sag more, and more toward their ends.
    const droop = [0, 0.02, 0.05, 0.09, 0.14, 0.2, 0.26][depth];
    const end = radius * (depth === MAX_DEPTH ? 0.45 : depth === 0 ? 0.62 : 0.72);
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
    if (depth >= 4) tips.push({ p: joints[Math.floor(steps / 2)].p, d, depth });
    if (depth === MAX_DEPTH) {
      tips.push({ p, d, depth: depth + 1 });
      return;
    }
    // Side twigs off the outer limbs, so the crown fills in between the forks.
    if (depth >= 3 && r() < 0.45) {
      const j = joints[Math.floor(r() * (steps - 1))];
      const side = norm(add(add(j.d, rand3(1.6)), [0, 0.3, 0]));
      limb(j.p, side, len * 0.45, end * 0.7, Math.min(MAX_DEPTH, depth + 2));
    }
    const n = depth === 0 ? 3 : r() < 0.4 ? 3 : 2;
    const turn = r() * Math.PI * 2;
    for (let i = 0; i < n; i++) {
      const az = turn + (i / n) * Math.PI * 2 + (r() - 0.5) * 0.8;
      // Spread around the parent, and pulled outward from the trunk: an umbrella, not a cone.
      const around: V3 = [Math.cos(az), 0, Math.sin(az) * 0.75];
      const radial = norm([p[0] + around[0] * 0.3, 0, p[2] + around[2] * 0.3]);
      const spread = depth === 0 ? 0.95 : 0.5 + r() * 0.35;
      let c = norm(add(d, around, spread));
      const outward = depth === 0 ? 0 : depth < 3 ? 0.55 : 0.3;
      c = norm(add(mix(c, radial, outward), [0, depth === 0 ? 0.45 : depth < 3 ? 0.3 : 0.12, 0]));
      const main = i === 0;
      limb(
        p,
        c,
        len * (depth === 0 ? 1.15 : 0.7 + r() * 0.14),
        end * (main ? 0.78 : 0.64 + r() * 0.08),
        depth + 1,
      );
    }
  };
  limb([0, 0, 0], norm([0.16, 1, 0.04]), 1.25, 0.22, 0);

  // Blossom clusters at the tips and partway along the outer twigs; some tips stay bare, a few
  // carry dark buds.
  const clusters: Cluster[] = [];
  const flowers: Flower[] = [];
  const buds: V3[] = [];
  for (const t of tips) {
    const atTip = t.depth > MAX_DEPTH;
    const roll = r();
    if (atTip && roll < 0.08) {
      for (let k = 0; k < 3; k++) buds.push(add(t.p, rand3(0.08)));
      continue;
    }
    if (!atTip && roll < 0.5) continue;
    const rad = (atTip ? 0.16 : 0.1) * (0.8 + r() * 0.45);
    const c = add(t.p, [0, rad * 0.35, 0]);
    // The cloud's own light: lit from above, darker on the underside and deep inside the crown.
    const height = Math.min(1, Math.max(0, c[1] / 2.6));
    clusters.push({ c, r: rad, shade: 0.78 + 0.22 * height });
    const count = atTip ? 16 : 6;
    for (let i = 0; i < count; i++) {
      const u = r() * Math.PI * 2;
      const v = Math.acos(1 - 1.6 * r()); // the upper part of the sphere, mostly
      const n: V3 = norm([Math.sin(v) * Math.cos(u), Math.cos(v), Math.sin(v) * Math.sin(u)]);
      const p = add(c, [n[0] * rad, n[1] * rad * 0.75, n[2] * rad]);
      // Shade toward a deeper pink, not toward grey: shadowed petals still read as blossom.
      const lit = Math.max(0, dot(n, LIGHT)) * 0.7 + height * 0.3;
      const base = PETAL_TONES[Math.floor(r() ** 1.6 * PETAL_TONES.length)];
      const rgb = mix(SHADOW, base, 0.35 + 0.65 * lit);
      flowers.push({
        p,
        n: norm(add(n, rand3(0.9))),
        size: 0.04 + r() * 0.022,
        spin: r() * Math.PI * 2,
        rgb,
      });
    }
  }
  // Far first, so the poster paints near ones over far ones.
  flowers.sort((a, b) => a.p[2] - b.p[2]);
  clusters.sort((a, b) => a.c[2] - b.c[2]);

  const fallen: Fallen[] = Array.from({ length: 70 }, () => {
    const a = r() * Math.PI * 2;
    const d = Math.sqrt(r()) * 2.2;
    const base = PETAL_TONES[2 + Math.floor(r() * 3)]; // the pinker ones show on paper
    return {
      x: Math.cos(a) * d + 0.3,
      z: Math.sin(a) * d * 0.5,
      spin: r() * Math.PI * 2,
      rgb: base,
    };
  });
  return { segments, clusters, flowers, buds, fallen };
}

export const TREE = grow();

/**
 * The camera both renderings use: perspective, looking straight down -z, framed so every flower
 * and the ground fit with a margin. Points nearer the camera spread wider, so the frame is
 * solved in screen space, a few times over.
 */
export const CAMERA = (() => {
  const fov = 30;
  const tan = Math.tan((fov * Math.PI) / 360);
  const aspect = POSTER.w / POSTER.h;
  const pts: V3[] = [...TREE.flowers.map((f) => f.p), [0, -0.12, 0]];
  const xs = pts.map((p) => p[0]);
  const ys = pts.map((p) => p[1]);
  const eye: V3 = [
    (Math.min(...xs) + Math.max(...xs)) / 2,
    (Math.min(...ys) + Math.max(...ys)) / 2,
    10,
  ];
  for (let k = 0; k < 6; k++) {
    let spread = 0;
    for (const p of pts) {
      const half = tan * (eye[2] - p[2]);
      spread = Math.max(
        spread,
        Math.abs(p[0] - eye[0]) / (half * aspect),
        Math.abs(p[1] - eye[1]) / half,
      );
    }
    eye[2] *= spread / 0.97;
  }
  return { fov, eye };
})();

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

/** Where a world point lands on the poster, and how many poster units one world unit is there. */
export function project(p: V3) {
  const [ex, ey, ez] = CAMERA.eye;
  const depth = ez - p[2];
  const half = Math.tan((CAMERA.fov * Math.PI) / 360) * depth;
  const aspect = POSTER.w / POSTER.h;
  return {
    x: ((p[0] - ex) / (half * aspect) + 1) * (POSTER.w / 2),
    y: (1 - (p[1] - ey) / half) * (POSTER.h / 2),
    scale: POSTER.h / 2 / half,
  };
}

/** 0–1 RGB to `#rrggbb`. */
export const toHex = (c: V3) =>
  `#${c
    .map((v) =>
      Math.round(Math.min(1, Math.max(0, v)) * 255)
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")}`;
