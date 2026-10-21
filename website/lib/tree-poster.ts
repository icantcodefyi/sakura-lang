import {
  CAMERA,
  LIGHT,
  POSTER,
  petalOutline,
  project,
  toHex,
  TREE,
  TREE_COLORS,
  type TreeTheme,
  type V3,
} from "@/lib/sakura-tree";

// Whole poster units: the file is a background image, drawn at most about 640px wide.
const i = (v: number) => Math.round(v);
const deg = (rad: number) => Math.round((rad * 180) / Math.PI);

/**
 * Limbs as filled outlines: one tapered quad per segment, written with relative steps. Segments
 * thinner than a pixel are left to the blossoms, which cover them in the scene too.
 */
function wood(lit: boolean) {
  let d = "";
  for (const s of TREE.segments) {
    const a = project(s.a);
    const c = project(s.b);
    const wa = s.r0 * a.scale;
    const wc = s.r1 * c.scale;
    if (wc < 0.6) continue;
    const len = Math.hypot(c.x - a.x, c.y - a.y) || 1;
    let nx = -(c.y - a.y) / len;
    let ny = (c.x - a.x) / len;
    // The lit side faces the light (up and left on the poster).
    if (nx * -LIGHT[0] + ny * -LIGHT[1] < 0) [nx, ny] = [-nx, -ny];
    // The lit strip is the outer third of the limb on the light's side.
    const [ka, kb] = lit ? [1, 0.35] : [1, -1];
    const pts = [
      [a.x + nx * wa * ka, a.y + ny * wa * ka],
      [c.x + nx * wc * ka, c.y + ny * wc * ka],
      [c.x + nx * wc * kb, c.y + ny * wc * kb],
      [a.x + nx * wa * kb, a.y + ny * wa * kb],
    ].map(([x, y]) => [i(x), i(y)]);
    if (lit && wa < 3) continue;
    d += `M${pts[0][0]} ${pts[0][1]}`;
    for (let k = 1; k < 4; k++) d += `l${pts[k][0] - pts[k - 1][0]} ${pts[k][1] - pts[k - 1][1]}`;
    if (!lit && wc > 2) {
      const r = i(wc);
      d += `M${i(c.x) - r} ${i(c.y)}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0`;
    }
  }
  return d;
}

/**
 * Discs as round-capped dots, grouped by colour and size: each disc is just `Mx yh0` in one
 * path, a tenth of the bytes of a `<circle>`.
 */
function circles(items: { x: number; y: number; r: number; fill: string }[]) {
  const by = new Map<string, string>();
  for (const c of items) {
    const r = i(c.r);
    if (r < 1) continue;
    const key = `${c.fill} ${r}`;
    by.set(key, `${by.get(key) ?? ""}M${i(c.x)} ${i(c.y)}h0`);
  }
  return [...by]
    .map(([key, d]) => {
      const [fill, r] = key.split(" ");
      return `<path stroke="${fill}" stroke-width="${2 * Number(r)}" stroke-linecap="round" d="${d}"/>`;
    })
    .join("");
}

const scale = (c: V3, k: number): V3 => [c[0] * k, c[1] * k, c[2] * k];
// Quantise colours so circles share a few fills.
const q = (c: V3) => toHex(c.map((v) => Math.round(v * 20) / 20) as V3);

/**
 * The hero tree as a standalone SVG file: what every visitor sees first, and the only tree with
 * reduced motion, without WebGL or on a phone. Served once per theme by app/tree-*.svg, so it
 * costs the page no HTML and only the shown theme is fetched. Each blossom cluster is a shaded
 * cloud (underside, body, lit top), with every flower of the scene on it as a dot.
 */
export function posterSvg(theme: TreeTheme) {
  const c = TREE_COLORS[theme];
  const k = c.bloom;
  const foot = project([CAMERA.eye[0] * 0.4, 0, 0]);
  const fallen = TREE.fallen
    .map((f) => {
      const p = project([f.x, 0, f.z]);
      return `<use href="#p" fill="${q(scale(f.rgb, k * 0.95))}" transform="translate(${i(p.x)} ${i(p.y)}) scale(${(0.06 * p.scale).toFixed(1)} ${(0.03 * p.scale).toFixed(1)}) rotate(${deg(f.spin)})"/>`;
    })
    .join("");
  const clouds = circles(
    TREE.clusters.flatMap((cl) => {
      const p = project(cl.c);
      const r = cl.r * p.scale;
      const s = cl.shade * k;
      return [
        {
          x: p.x + r * 0.08,
          y: p.y + r * 0.25,
          r: r * 0.72,
          fill: q(scale([0.85, 0.54, 0.64], s)),
        },
        { x: p.x, y: p.y, r: r * 0.66, fill: q(scale([0.96, 0.76, 0.83], s)) },
      ];
    }),
  );
  const blooms = circles(
    // Every other flower: at poster size the rest only add bytes.
    TREE.flowers
      .filter((_, n) => n % 2 === 0)
      .map((f) => {
        const p = project(f.p);
        return { x: p.x, y: p.y, r: f.size * p.scale * 0.8, fill: q(scale(f.rgb, k)) };
      }),
  );
  const g = project([0, 0, 0]);
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${POSTER.w} ${POSTER.h}">`,
    `<defs><path id="p" d="M${petalOutline(12)
      .map(([x, y]) => `${i(x * 10)} ${i(y * 10)}`)
      .join("L")}Z" transform="scale(.1)"/></defs>`,
    `<ellipse cx="${i(foot.x)}" cy="${i(g.y + 8)}" rx="${i(2.6 * g.scale)}" ry="${i(0.32 * g.scale)}" fill="${c.ground}"/>`,
    fallen,
    `<path d="${wood(false)}" fill="${c.bark}"/>`,
    `<path d="${wood(true)}" fill="${c.barkLit}"/>`,
    clouds,
    blooms,
    `</svg>`,
  ].join("");
}

export const svgResponse = (body: string) =>
  new Response(body, {
    headers: {
      "Content-Type": "image/svg+xml; charset=utf-8",
      "Cache-Control": "public, max-age=86400, stale-while-revalidate=604800",
    },
  });
