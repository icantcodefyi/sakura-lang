// The landing hero's tree (components/landing/hero.tsx): a close-up of a Somei-Yoshino. The
// camera stands low by the trunk on the right; one great limb sweeps up and left across the top
// of the hero over the copy, a smaller one goes up and off the right edge, and petals fall
// through the whole hero. The big wood is laid out in screen terms (fractions of the poster)
// and the branches and blossom on it are grown (lib/sakura-tree.ts). No three.js here.
import {
  type Camera,
  grow,
  type GrowOpts,
  type Near,
  project,
  type Tree,
  type V3,
} from "@/lib/sakura-tree";

// The poster matches the hero's shape at 1440 wide (about 1440 × 695); from lg up the hero
// shows it under `cover` from the bottom right.
const POSTER = { w: 1800, h: 870 };
// The copy's box on the poster at 1440 wide: right of x 0.49 and above y 0.08 stays clear of it.
const COPY = { x: 0.5, y: 0.09 };

// The camera: 30° tall, 3.4 m of view at the trunk, the trunk's foot at (0.95, 0.98).
const FOV = 30;
const TAN = Math.tan((FOV * Math.PI) / 360);
const H = 3.4;
const EZ = H / 2 / TAN;
const ASPECT = POSTER.w / POSTER.h;
const FOOT = [0.95, 0.98];
const CAM: Camera = {
  fov: FOV,
  eye: [-(2 * FOOT[0] - 1) * TAN * EZ * ASPECT, -(1 - 2 * FOOT[1]) * TAN * EZ, EZ],
  poster: POSTER,
};

/** The world point that lands at poster fraction (x, y), z nearer the camera. */
const at = (x: number, y: number, z = 0): V3 => {
  const half = TAN * (EZ - z);
  return [CAM.eye[0] + (2 * x - 1) * half * ASPECT, CAM.eye[1] + (1 - 2 * y) * half, z];
};
/** A limb through these points, its corners softened but not lost (Chaikin, twice). */
const limb = (pts: [number, number, number?][], r: number[], sprout?: Sprout) => {
  let p = pts.map(([x, y, z]) => [...at(x, y, z), 0] as number[]);
  p.forEach((q, k) => (q[3] = r[k]));
  for (let n = 0; n < 2; n++)
    p = [
      p[0],
      ...p.slice(1).flatMap((b, k) => {
        const a = p[k];
        const mixed = (t: number) => a.map((v, j) => v + (b[j] - v) * t);
        return [mixed(0.25), mixed(0.75)];
      }),
      p[p.length - 1],
    ];
  return { pts: p.map((q) => [q[0], q[1], q[2]] as V3), r: p.map((q) => q[3]), sprout };
};
type Sprout = NonNullable<NonNullable<GrowOpts["scaffold"]>[number]["sprout"]>;
const twigs = (every: number, len: number, up: V3): Sprout => ({
  every,
  len,
  depth: 4,
  up,
  wild: 0.9,
});

const TREE: GrowOpts = {
  seed: 20261021,
  maxDepth: 6,
  twigs: { from: 3, p: 0.6 },
  // Radii are widths at the scaffold's points; the trunk's foot flares into root buttresses
  // (components/landing/tree-scene.ts). Limbs taper unevenly, thickening a little at each node,
  // and the great limb dips and rises along its arc.
  scaffold: [
    // The trunk, leaning a little left; it ends where the two limbs part.
    limb(
      [
        [0.955, 1.04],
        [0.95, 0.93],
        [0.942, 0.82],
        [0.935, 0.72],
        [0.918, 0.615],
        [0.915, 0.58],
      ],
      [0.27, 0.23, 0.215, 0.205, 0.18, 0.1],
    ),
    // The great limb, up and left across the top.
    limb(
      [
        [0.918, 0.64],
        [0.875, 0.55, 0.1],
        [0.82, 0.45, 0.17],
        [0.785, 0.375, 0.2],
        [0.735, 0.32, 0.2],
        [0.69, 0.255, 0.2],
        [0.635, 0.2, 0.15],
        [0.59, 0.135, 0.12],
        [0.54, 0.105, 0.1],
        [0.46, 0.055],
        [0.37, 0.0],
      ],
      [0.16, 0.135, 0.125, 0.115, 0.095, 0.088, 0.07, 0.062, 0.05, 0.033, 0.02],
      twigs(0.28, 0.75, [-0.2, 0.7, 0.3]),
    ),
    // Off it: one bough to the left, past the end of the headline, and one up.
    limb(
      [
        [0.72, 0.285, 0.2],
        [0.66, 0.29, 0.28],
        [0.6, 0.31, 0.3],
        [0.53, 0.3, 0.3],
      ],
      [0.032, 0.026, 0.02, 0.012],
      twigs(0.2, 0.35, [-0.3, 0.6, 0.3]),
    ),
    limb(
      [
        [0.785, 0.385, 0.18],
        [0.795, 0.29, 0.14],
        [0.81, 0.21, 0.1],
        [0.84, 0.07],
        [0.87, -0.05],
      ],
      [0.06, 0.05, 0.045, 0.033, 0.024],
      twigs(0.25, 0.6, [0.1, 0.7, 0.3]),
    ),
    // The smaller limb, up and off the right edge.
    limb(
      [
        [0.928, 0.645],
        [0.945, 0.53, -0.08],
        [0.965, 0.44, -0.1],
        [0.99, 0.31, -0.1],
        [1.04, 0.15, -0.2],
      ],
      [0.13, 0.105, 0.098, 0.075, 0.055],
      twigs(0.3, 0.7, [0.3, 0.6, 0.3]),
    ),
    // Smaller branches off the trunk and the great limb, two of them broken short.
    limb(
      [
        [0.94, 0.76, 0.12],
        [0.975, 0.68, 0.2],
        [1.01, 0.62, 0.22],
      ],
      [0.04, 0.03, 0.022],
      twigs(0.15, 0.4, [0.3, 0.6, 0.3]),
    ),
    limb(
      [
        [0.945, 0.86, 0.12],
        [0.93, 0.85, 0.2],
      ],
      [0.045, 0.036],
    ),
    limb(
      [
        [0.84, 0.49, 0.2],
        [0.835, 0.455, 0.27],
      ],
      [0.034, 0.028],
    ),
    limb(
      [
        [0.66, 0.22, 0.18],
        [0.635, 0.15, 0.22],
        [0.62, 0.08, 0.22],
      ],
      [0.028, 0.02, 0.012],
      twigs(0.15, 0.35, [-0.1, 0.7, 0.3]),
    ),
  ],
  // One unit is about half a metre (the trunk is some 25 cm across): a petal 1.5 cm long, a
  // flower about 4 cm across, 2.6 petals.
  sprays: { from: 3, spacing: 0.02, size: 0.04, spread: 0.07, pale: 0.6 },
  air: {
    n: 170,
    x: [at(0, 0)[0], at(1, 0)[0]],
    y: [0, at(0, -0.1)[1]],
    z: [-1.5, 1],
    size: 0.031,
  },
  ground: { n: 700, x: [at(0.35, 1)[0], at(1.05, 1)[0]], z: [-1.4, 0] },
};

// Slightly out of focus, three metres nearer the camera: two blossoms at the top right.
const NEAR_DEPTH = 3;
const NEAR: [number, number, number, number][] = [
  [0.93, 0.05, 0.04, 0.4],
  [0.985, 0.3, 0.035, 1.3],
];

const inCopy = (p: V3) => {
  const s = project(p, CAM);
  return s.x / POSTER.w < COPY.x && s.y / POSTER.h > COPY.y;
};

function build() {
  const g = grow(TREE);
  const tree: Tree = {
    ...g,
    // Nothing crosses the copy: what would is cut back, as a gardener keeps a path clear.
    segments: g.segments.filter((s) => !inCopy(s.b)),
    flowers: g.flowers.filter((f) => !inCopy(f.p)),
    buds: g.buds.filter((b) => !inCopy(b)),
    near: NEAR.map(([x, y, size, spin]): Near => {
      const z = EZ - NEAR_DEPTH;
      const half = TAN * NEAR_DEPTH;
      return {
        p: [CAM.eye[0] + (2 * x - 1) * half * ASPECT, CAM.eye[1] + (1 - 2 * y) * half, z],
        size,
        spin,
        rgb: [0.99, 0.86, 0.9],
      };
    }),
  };
  // Poly Haven's CC0 "Sakura Bark" scan by Charlotte Baglioni (polyhaven.com/a/sakura_bark):
  // colour, OpenGL normal, and AO / roughness / metal, downscaled to 512 px webp.
  const bark = {
    map: "/hero/bark-diff.webp",
    normal: "/hero/bark-nor.webp",
    arm: "/hero/bark-arm.webp",
  };
  return { tree, cam: CAM, cover: [1, 1] as [number, number], bark };
}

/** The hero's tree, its camera, the poster's `cover` position (bottom right), and its bark. */
export const HERO = build();
