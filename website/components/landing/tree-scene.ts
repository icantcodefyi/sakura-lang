// The live sakura tree: lib/hero-tree.ts in three.js, through the poster's camera. Whole limbs
// in scanned bark, lit by a key from the top left; young twigs; five-petal flowers in bunches;
// buds; a wind that moves the tips more than the trunk; petals that tumble down on varied paths
// and settle on the ground for a while. Loaded only by dynamic import (tree-live.tsx), so
// three.js never reaches the initial bundle. Rendered once, it is also the hero's poster
// (scripts/hero-posters.sh).
import {
  BufferAttribute,
  BufferGeometry,
  CanvasTexture,
  Color,
  CylinderGeometry,
  DirectionalLight,
  DoubleSide,
  Euler,
  Float32BufferAttribute,
  Group,
  HemisphereLight,
  IcosahedronGeometry,
  InstancedMesh,
  LoadingManager,
  type Material,
  Matrix4,
  Mesh,
  MeshBasicMaterial,
  MeshLambertMaterial,
  MeshStandardMaterial,
  Object3D,
  PerspectiveCamera,
  PlaneGeometry,
  Quaternion,
  RepeatWrapping,
  Scene,
  Shape,
  ShapeGeometry,
  SRGBColorSpace,
  TextureLoader,
  Vector2,
  Vector3,
  WebGLRenderer,
} from "three";
import {
  type Camera,
  flowerOutline,
  petalOutline,
  type Tree,
  TREE_COLORS,
  type V3,
} from "@/lib/sakura-tree";

const theme = () => (document.documentElement.classList.contains("dark") ? "dark" : "light");
const shapeOf = (pts: [number, number][]) =>
  // Outlines are in SVG's y-down space; three.js is y-up.
  new Shape(pts.map(([x, y]) => new Vector2(x, -y)));

/**
 * A shape out of focus, white on clear, for tinting: drawn off the canvas so only its blurred
 * shadow lands on it (Safari has no ctx.filter). The shape spans 0.31 of the tile.
 */
function soft(outline: [number, number][], blur: number) {
  const c = document.createElement("canvas");
  c.width = c.height = 128;
  const g = c.getContext("2d")!;
  g.shadowColor = "#ffffff";
  g.shadowBlur = blur;
  g.shadowOffsetX = 256;
  g.beginPath();
  for (const [x, y] of outline) g.lineTo(64 + x * 40 - 256, 64 + y * 40);
  g.fill();
  const t = new CanvasTexture(c);
  t.colorSpace = SRGBColorSpace;
  return t;
}
const softMat = (map: CanvasTexture) =>
  new MeshBasicMaterial({
    map,
    transparent: true,
    opacity: 0.75,
    depthWrite: false,
    side: DoubleSide,
  });

/** The bark's maps: colour, OpenGL normal, and ambient occlusion / roughness / metal. */
export type BarkMaps = { map: string; normal: string; arm: string };

/** The smallest signed difference between two angles. */
const turn = (a: number, b: number) => Math.atan2(Math.sin(a - b), Math.cos(a - b));

/**
 * The big wood as whole limbs, each one tube along its points: not round but fluted by
 * a few low harmonics that twist slowly along it, the first limb (the trunk) swelling at its foot
 * into root buttresses and carrying a burl or two, the others thickening into a collar where
 * they leave their parent and starting inside it, so they grow out of it rather than poke into
 * it. `aAge` is 1 on old, thick wood and 0 on young limbs. The bark tiles every 0.8 units, a
 * whole number of times around.
 */
function barkGeometry(limbs: { pts: V3[]; r: number[] }[]) {
  const S = 48;
  const pos: number[] = [];
  const uv: number[] = [];
  const age: number[] = [];
  const index: number[] = [];
  const seams: [number, number][] = [];
  let seed = 5;
  const rnd = () => ((seed = (seed * 16807) % 2147483647) - 1) / 2147483646;
  limbs.forEach((limb, li) => {
    const trunk = li === 0;
    const P = limb.pts.map((p) => new Vector3(...p));
    const R = [...limb.r];
    if (!trunk) {
      const back = P[0].clone().sub(P[1]).normalize();
      P.unshift(P[0].clone().addScaledVector(back, R[0] * 0.9));
      R.unshift(R[0]);
    }
    const phase = Array.from({ length: 7 }, () => rnd() * Math.PI * 2);
    const amp = [0, 0, 0.07, 0.055, 0.04, 0.03, 0.02];
    const lobes = trunk ? [0.3, 1.9, 3.3, 4.8].map((v) => v + rnd() * 0.5) : [];
    const burls = trunk
      ? [
          [0.55, 2.4, 0.16],
          [1.05, 5.2, 0.1],
        ]
      : [];
    const tiles = Math.max(1, Math.round((2 * Math.PI * R[0]) / 0.8));
    const young = Math.min(1, Math.max(0, (R[0] - 0.02) / 0.12));
    let T = P[1].clone().sub(P[0]).normalize();
    const N = Math.abs(T.z) > 0.9 ? new Vector3(1, 0, 0) : new Vector3(0, 0, 1);
    N.addScaledVector(T, -N.dot(T)).normalize();
    const q = new Quaternion();
    const B = new Vector3();
    const dir = new Vector3();
    let len = 0;
    const first = pos.length / 3;
    P.forEach((c, i) => {
      if (i > 0) {
        len += c.distanceTo(P[i - 1]);
        const next = P[Math.min(P.length - 1, i + 1)]
          .clone()
          .sub(P[i - 1])
          .normalize();
        q.setFromUnitVectors(T, next);
        N.applyQuaternion(q);
        T = next;
      }
      B.crossVectors(T, N);
      const foot = trunk ? Math.exp(-Math.max(0, c.y) / 0.45) : 0;
      const collar = trunk ? 0 : 0.45 * Math.exp(-len / (R[0] * 1.6));
      for (let j = 0; j <= S; j++) {
        const a = (j / S) * Math.PI * 2;
        let k = 1;
        for (let h = 2; h < 7; h++)
          k += amp[h] * Math.sin(h * (a + len * 0.35) + phase[h] + len * 0.9);
        let lobe = 0;
        for (const l of lobes) lobe += Math.exp(-(turn(a, l) ** 2) / 0.12);
        k *= 1 + foot * (0.2 + 0.85 * lobe) + collar;
        for (const [at, around, size] of burls)
          k += size * Math.exp(-(((len - at) / 0.1) ** 2) - turn(a, around) ** 2 / 0.12);
        const r = R[i] * k;
        dir.copy(N).multiplyScalar(Math.cos(a)).addScaledVector(B, Math.sin(a));
        pos.push(c.x + dir.x * r, c.y + dir.y * r, c.z + dir.z * r);
        uv.push((j / S) * tiles, len / 0.8);
        age.push(trunk ? 1 : young);
        if (i > 0 && j < S) {
          const v = first + i * (S + 1) + j;
          index.push(v - S - 1, v - S, v, v - S, v + 1, v);
        }
      }
      seams.push([first + i * (S + 1), first + i * (S + 1) + S]);
    });
    // Close the end: a broken stub shows its blunt end, a fine one ends inside the blossom.
    const end = P[P.length - 1];
    const tip = pos.length / 3;
    pos.push(
      end.x + T.x * R[R.length - 1] * 0.3,
      end.y + T.y * R[R.length - 1] * 0.3,
      end.z + T.z * R[R.length - 1] * 0.3,
    );
    uv.push(0.5, len / 0.8);
    age.push(trunk ? 1 : young);
    const last = first + (P.length - 1) * (S + 1);
    for (let j = 0; j < S; j++) index.push(last + j, last + j + 1, tip);
  });
  const g = new BufferGeometry();
  g.setAttribute("position", new Float32BufferAttribute(pos, 3));
  g.setAttribute("uv", new Float32BufferAttribute(uv, 2));
  g.setAttribute("aAge", new Float32BufferAttribute(age, 1));
  g.setIndex(index);
  g.computeVertexNormals();
  // The seam's two copies of each vertex share one normal, so it does not show.
  const n = g.attributes.normal;
  for (const [a, b] of seams) {
    const v = new Vector3(
      n.getX(a) + n.getX(b),
      n.getY(a) + n.getY(b),
      n.getZ(a) + n.getZ(b),
    ).normalize();
    n.setXYZ(a, v.x, v.y, v.z);
    n.setXYZ(b, v.x, v.y, v.z);
  }
  return g;
}

/**
 * The big wood's material: Poly Haven's CC0 "Sakura Bark" scan (Charlotte Baglioni,
 * polyhaven.com/a/sakura_bark; colour, normal, AO and roughness maps, downscaled to 512 px in
 * public/hero), varied by age and place. Old wood is a cooler grey-brown, young limbs a
 * warmer red-brown with more sheen; the foot is darker in the grooves, with green-grey lichen on
 * the side away from the light.
 */
function barkMaterial(
  maps: BarkMaps,
  time: { value: number },
  centre: V3,
  manager: LoadingManager,
) {
  const load = new TextureLoader(manager);
  const tex = (url: string, srgb: boolean) => {
    const t = load.load(url);
    if (srgb) t.colorSpace = SRGBColorSpace;
    t.wrapS = t.wrapT = RepeatWrapping;
    t.anisotropy = 4;
    return t;
  };
  const arm = tex(maps.arm, false);
  const mat = new MeshStandardMaterial({
    map: tex(maps.map, true),
    normalMap: tex(maps.normal, false),
    normalScale: new Vector2(2, 2),
    aoMap: arm,
    aoMapIntensity: 1,
    roughnessMap: arm,
    metalness: 0,
  });
  sway(mat, time, centre);
  const swayed = mat.onBeforeCompile;
  mat.onBeforeCompile = (shader, renderer) => {
    swayed(shader, renderer);
    shader.vertexShader = shader.vertexShader
      .replace(
        "#include <common>",
        "#include <common>\nattribute float aAge;\nvarying float vAge;\nvarying vec3 vWorld;\nvarying vec3 vWorldNormal;",
      )
      .replace(
        "gl_Position = projectionMatrix * mvPosition;",
        "gl_Position = projectionMatrix * mvPosition;\nvAge = aAge;\nvWorld = wp.xyz;\nvWorldNormal = normal;",
      );
    shader.fragmentShader = shader.fragmentShader
      .replace(
        "#include <common>",
        "#include <common>\nvarying float vAge;\nvarying vec3 vWorld;\nvarying vec3 vWorldNormal;",
      )
      .replace(
        "#include <map_fragment>",
        `#include <map_fragment>
float grooves = texture2D(aoMap, vAoMapUv).r;
diffuseColor.rgb *= mix(vec3(1.12, 0.92, 0.86), vec3(0.95, 0.97, 1.0), vAge);
float foot = 1.0 - smoothstep(0.0, 1.0, vWorld.y);
diffuseColor.rgb *= mix(1.0, 0.55 + 0.45 * grooves, 0.6 + 0.4 * foot);
float shade = 1.0 - smoothstep(-0.3, 0.4, dot(normalize(vWorldNormal), vec3(-0.45, 0.8, 0.4)));
float lichen = foot * shade * smoothstep(0.35, 0.7, texture2D(map, vMapUv * 0.37 + 0.21).g * 2.2);
diffuseColor.rgb = mix(diffuseColor.rgb, vec3(0.42, 0.46, 0.37), lichen * 0.75);`,
      )
      .replace(
        "#include <roughnessmap_fragment>",
        "#include <roughnessmap_fragment>\nroughnessFactor = clamp(roughnessFactor * mix(0.6, 1.0, vAge), 0.25, 1.0);",
      );
  };
  return mat;
}

/**
 * Wind, in the vertex shader of every swaying material: an offset that grows with height and
 * with distance from the trunk, so tips move and the trunk barely does.
 */
function sway(mat: Material, time: { value: number }, centre: V3) {
  const c = `vec2(${centre[0].toFixed(3)}, ${centre[2].toFixed(3)})`;
  mat.onBeforeCompile = (shader) => {
    shader.uniforms.uTime = time;
    shader.vertexShader = shader.vertexShader
      .replace("#include <common>", "#include <common>\nuniform float uTime;")
      .replace(
        "#include <project_vertex>",
        `vec4 wp = vec4(transformed, 1.0);
#ifdef USE_INSTANCING
wp = instanceMatrix * wp;
#endif
vec2 rel = wp.xz - ${c};
float reach = max(wp.y - 0.9, 0.0) * 0.35 + dot(rel, rel) * 0.05;
float gust = 0.6 + 0.4 * sin(uTime * 0.31);
wp.x += sin(uTime * 1.05 + wp.x * 0.7 + wp.z * 0.5) * 0.045 * reach * gust;
wp.z += cos(uTime * 0.83 + wp.x * 0.4) * 0.03 * reach * gust;
wp.y += sin(uTime * 1.9 + wp.x * 1.7 + wp.z * 1.3) * 0.012 * reach;
vec4 mvPosition = modelViewMatrix * wp;
gl_Position = projectionMatrix * mvPosition;`,
      );
  };
}

/**
 * What to draw: a tree, its camera, the CSS `background-position` (0–1 per axis) the poster is
 * drawn with under `cover`, so the scene crops exactly as the poster does when the canvas does
 * not share its aspect, and the bark's texture files.
 */
export type SceneOpts = { tree: Tree; cam: Camera; cover: [number, number]; bark: BarkMaps };

export function mount(
  canvas: HTMLCanvasElement,
  onFirstFrame: () => void,
  { tree, cam, cover, bark: maps }: SceneOpts,
) {
  const renderer = new WebGLRenderer({ canvas, antialias: true, alpha: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  const camera = new PerspectiveCamera(cam.fov, 1, 0.1, 80);
  camera.position.set(...cam.eye);
  const scene = new Scene();
  const crown = new Group();
  scene.add(crown);
  const time = { value: 0 };

  const sky = new HemisphereLight("#fff6f8", "#6b4f57");
  const sun = new DirectionalLight("#ffffff");
  sun.position.set(-4.5, 10, 4);
  scene.add(sky, sun);

  // The big wood: whole limbs in scanned bark. The first frame waits for its textures.
  const textures = new LoadingManager();
  let ready = false;
  textures.onLoad = () => (ready = true);
  const woodMat = barkMaterial(maps, time, tree.centre, textures);
  const wood = new Mesh(barkGeometry(tree.limbs), woodMat);
  crown.add(wood);
  // The grown twigs: instanced pieces, untapered at their mean width, with a ball at each joint,
  // in plain young red-brown bark.
  const barkMat = new MeshLambertMaterial();
  sway(barkMat, time, tree.centre);
  const twigs = tree.segments.filter((s) => s.depth > 0);
  const limbGeo = new CylinderGeometry(1, 1, 1, 10, 1, true);
  const jointGeo = new IcosahedronGeometry(1, 1);
  const n = twigs.length;
  const limbs = new InstancedMesh(limbGeo, barkMat, n);
  const joints = new InstancedMesh(jointGeo, barkMat, n);
  const m = new Matrix4();
  const q = new Quaternion();
  const up = new Vector3(0, 1, 0);
  twigs.forEach((s, i) => {
    const a = new Vector3(...s.a);
    const b = new Vector3(...s.b);
    const dir = b.clone().sub(a);
    const len = dir.length();
    q.setFromUnitVectors(up, dir.normalize());
    const w = (s.r0 + s.r1) / 2;
    m.compose(a.clone().lerp(b, 0.5), q, new Vector3(w, len, w));
    limbs.setMatrixAt(i, m);
    m.compose(b, q, new Vector3(s.r1, s.r1, s.r1));
    joints.setMatrixAt(i, m);
  });
  crown.add(limbs, joints);

  const o = new Object3D();
  // Flowers: five notched petals, white-pink at the rim and deeper pink at the heart (vertex
  // colours), each tinted by its baked shade from the tree.
  const flowerGeo = new ShapeGeometry(shapeOf(flowerOutline(30)));
  const pos = flowerGeo.attributes.position;
  const heart = new Float32Array(pos.count * 3);
  for (let k = 0; k < pos.count; k++) {
    const d = Math.hypot(pos.getX(k), pos.getY(k));
    const t = Math.min(1, d / 0.45);
    heart.set([1, 0.62 + 0.38 * t, 0.72 + 0.28 * t], k * 3);
  }
  flowerGeo.setAttribute("color", new BufferAttribute(heart, 3));
  const flowerMat = new MeshBasicMaterial({ side: DoubleSide, vertexColors: true });
  sway(flowerMat, time, tree.centre);
  const flowers = new InstancedMesh(flowerGeo, flowerMat, tree.flowers.length);
  const z = new Vector3(0, 0, 1);
  tree.flowers.forEach((f, i) => {
    o.position.set(...f.p);
    o.quaternion.setFromUnitVectors(z, new Vector3(...f.n));
    o.rotateZ(f.spin);
    o.scale.setScalar(f.size);
    o.updateMatrix();
    flowers.setMatrixAt(i, o.matrix);
  });
  crown.add(flowers);

  // Buds: small, dark and closed.
  const budGeo = new IcosahedronGeometry(1, 1);
  const budMat = new MeshLambertMaterial();
  sway(budMat, time, tree.centre);
  const buds = new InstancedMesh(budGeo, budMat, tree.buds.length);
  tree.buds.forEach((p, i) => {
    o.position.set(...p);
    o.rotation.set(0, 0, 0);
    o.scale.set(0.018, 0.026, 0.018);
    o.updateMatrix();
    buds.setMatrixAt(i, o.matrix);
  });
  crown.add(buds);

  // Petals on the ground, and petals in the air that tumble down and settle for a while.
  const petal = tree.air.size;
  const petalGeo = new ShapeGeometry(shapeOf(petalOutline(16)));
  const petalMat = new MeshBasicMaterial({ side: DoubleSide });
  const settled = new InstancedMesh(petalGeo, petalMat, tree.fallen.length);
  tree.fallen.forEach((f, i) => {
    o.position.set(f.x, 0.005, f.z);
    o.rotation.set(-Math.PI / 2, 0, f.spin);
    o.scale.setScalar(petal);
    o.updateMatrix();
    settled.setMatrixAt(i, o.matrix);
  });
  scene.add(settled);

  // Slightly out of focus nearer the camera: a couple of blossoms, and a few petals falling past.
  const softFlower = soft(flowerOutline(40), 8);
  const softPetal = soft(petalOutline(24), 5);
  const plane = new PlaneGeometry(1, 1);
  const nearMat = softMat(softFlower);
  sway(nearMat, time, tree.centre);
  const near = new InstancedMesh(plane, nearMat, tree.near.length);
  tree.near.forEach((f, i) => {
    o.position.set(...f.p);
    o.rotation.set(0, 0, f.spin);
    o.scale.setScalar(f.size * 3.2); // the shape spans 0.31 of its tile
    o.updateMatrix();
    near.setMatrixAt(i, o.matrix);
  });
  near.renderOrder = 2;
  scene.add(near);
  const BIG = 4;
  const bigMat = softMat(softPetal);
  const big = new InstancedMesh(plane, bigMat, BIG);
  big.renderOrder = 3;
  scene.add(big);

  const count = tree.air.n;
  const petals = new InstancedMesh(petalGeo, petalMat, count);
  scene.add(petals);
  let seed = 7;
  const rand = () => ((seed = (seed * 16807) % 2147483647) - 1) / 2147483646;
  const drift = Array.from({ length: count + BIG }, () => ({
    p: new Vector3(),
    fall: 0,
    flutter: 0,
    phase: 0,
    spin: new Vector3(),
    rest: 0,
  }));
  const span = (v: [number, number]) => v[0] + (v[1] - v[0]) * rand();
  const spawn = (d: (typeof drift)[number], anywhere: boolean, i: number) => {
    const air = tree.air;
    if (i >= count) {
      // These pass two to three metres nearer the camera, on the tree's side, clear of the copy.
      const zz = cam.eye[2] - 3 - rand();
      const half = Math.tan((cam.fov * Math.PI) / 360) * (cam.eye[2] - zz);
      d.p.set(
        cam.eye[0] + (0.15 + rand() * 0.85) * half * (cam.poster.w / cam.poster.h),
        cam.eye[1] + (anywhere ? rand() * 2 - 1 : 1.1) * half,
        zz,
      );
    } else d.p.set(span(air.x), anywhere ? span(air.y) : air.y[1], span(air.z));
    d.fall = 0.16 + rand() * 0.2;
    d.flutter = 0.6 + rand() * 1.6;
    d.phase = rand() * Math.PI * 2;
    d.spin.set((rand() - 0.5) * 5, (rand() - 0.5) * 5, (rand() - 0.5) * 3);
    d.rest = 0;
  };
  drift.forEach((d, i) => spawn(d, true, i));
  const euler = new Euler();

  const paint = () => {
    const c = TREE_COLORS[theme()];
    barkMat.color = new Color(c.twig);
    woodMat.color = new Color(c.bark);
    budMat.color = new Color(c.bud);
    const tint = new Color();
    tree.flowers.forEach((f, i) =>
      flowers.setColorAt(i, tint.setRGB(...(f.rgb.map((v) => v * c.bloom) as V3), SRGBColorSpace)),
    );
    if (flowers.instanceColor) flowers.instanceColor.needsUpdate = true;
    tree.fallen.forEach((f, i) => settled.setColorAt(i, tint.setRGB(...f.rgb, SRGBColorSpace)));
    if (settled.instanceColor) settled.instanceColor.needsUpdate = true;
    for (let i = 0; i < count; i++)
      petals.setColorAt(i, tint.setRGB(...tree.fallen[i % tree.fallen.length].rgb, SRGBColorSpace));
    if (petals.instanceColor) petals.instanceColor.needsUpdate = true;
    const blur = (f: { rgb: V3 }) =>
      tint.setRGB(...(f.rgb.map((v) => v * c.bloom) as V3), SRGBColorSpace);
    tree.near.forEach((f, i) => near.setColorAt(i, blur(f)));
    for (let i = 0; i < BIG; i++) big.setColorAt(i, blur(tree.fallen[i % tree.fallen.length]));
    for (const mesh of [near, big]) if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;
    // A soft sky and a stronger key from the top left, so the trunk has a lit side and a
    // shadow side.
    sky.intensity = theme() === "dark" ? 0.66 : 0.78;
    sun.intensity = theme() === "dark" ? 2 : 2.4;
  };
  paint();
  const themeWatch = new MutationObserver(paint);
  themeWatch.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });

  const resize = () => {
    const { clientWidth: w, clientHeight: h } = canvas;
    if (!w || !h) return;
    renderer.setSize(w, h, false);
    // The poster scaled to cover the canvas, offset by `cover`; render only the part on screen.
    const { w: pw, h: ph } = cam.poster;
    const s = Math.max(w / pw, h / ph);
    camera.aspect = pw / ph;
    camera.setViewOffset(
      pw,
      ph,
      ((pw * s - w) * cover[0]) / s,
      ((ph * s - h) * cover[1]) / s,
      w / s,
      h / s,
    );
    camera.updateProjectionMatrix();
  };
  const sizer = new ResizeObserver(resize);
  sizer.observe(canvas);
  resize();

  let frame = 0;
  let last = 0;
  let first = true;
  const tick = (now: number) => {
    const dt = Math.min(0.05, last ? (now - last) / 1000 : 0);
    last = now;
    const t = (time.value += dt);
    const gust = 0.6 + 0.4 * Math.sin(t * 0.31);
    drift.forEach((d, i) => {
      if (d.p.y <= 0.006) {
        // Settled: lie flat for a few seconds, then come back as a new petal.
        d.rest += dt;
        if (d.rest > 3 + (i % 5)) spawn(d, false, i);
        o.position.set(d.p.x, 0.006, d.p.z);
        o.rotation.set(-Math.PI / 2, 0, d.phase);
      } else {
        // Tumbling: falling faster edge-on, fluttering side to side, pushed by the gusts.
        const edge = Math.abs(Math.sin(t * d.spin.x + d.phase));
        d.p.y = Math.max(0.005, d.p.y - d.fall * (0.6 + 0.8 * edge) * dt);
        d.p.x += (Math.sin(t * d.flutter + d.phase) * 0.22 + 0.12 * gust) * dt;
        d.p.z += Math.cos(t * d.flutter * 0.7 + d.phase) * 0.12 * dt;
        o.position.copy(d.p);
        euler.set(t * d.spin.x + d.phase, t * d.spin.y, t * d.spin.z);
        o.rotation.copy(euler);
      }
      if (i < count) {
        o.scale.setScalar(petal);
        o.updateMatrix();
        petals.setMatrixAt(i, o.matrix);
      } else {
        // In front of the camera nothing settles: past the bottom of the view it starts over.
        if (d.p.y < cam.eye[1] - 2) spawn(d, false, i);
        o.scale.setScalar(petal * 3.2);
        o.updateMatrix();
        big.setMatrixAt(i - count, o.matrix);
      }
    });
    petals.instanceMatrix.needsUpdate = true;
    big.instanceMatrix.needsUpdate = true;
    renderer.render(scene, camera);
    if (first && ready) {
      first = false;
      onFirstFrame();
    }
    frame = requestAnimationFrame(tick);
  };

  return {
    /** Start or stop the frame loop: on screen and tab visible, or not. */
    run(on: boolean) {
      if (on && !frame) {
        last = 0;
        frame = requestAnimationFrame(tick);
      } else if (!on && frame) {
        cancelAnimationFrame(frame);
        frame = 0;
      }
    },
    dispose() {
      cancelAnimationFrame(frame);
      frame = 0;
      themeWatch.disconnect();
      sizer.disconnect();
      for (const g of [wood.geometry, limbGeo, jointGeo, flowerGeo, budGeo, petalGeo, plane])
        g.dispose();
      for (const mat of [woodMat, barkMat, flowerMat, budMat, petalMat, nearMat, bigMat])
        mat.dispose();
      for (const mesh of [limbs, joints, flowers, buds, settled, petals, near, big]) mesh.dispose();
      for (const t of [woodMat.map, woodMat.normalMap, woodMat.aoMap, softFlower, softPetal])
        t?.dispose();
      renderer.dispose();
    },
  };
}
