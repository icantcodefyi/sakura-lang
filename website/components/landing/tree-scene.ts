// The live sakura tree: lib/sakura-tree.ts in three.js, through the poster's camera. Bark with
// lenticels, lit from above; shaded blossom clouds of five-petal flowers; buds; a wind that moves
// the tips more than the trunk; petals that tumble down on varied paths and settle on the ground
// for a while. Loaded only by dynamic import (tree-live.tsx), so three.js never reaches the
// initial bundle.
import {
  BufferAttribute,
  CanvasTexture,
  CircleGeometry,
  Color,
  CylinderGeometry,
  DirectionalLight,
  DoubleSide,
  Euler,
  Group,
  HemisphereLight,
  IcosahedronGeometry,
  InstancedMesh,
  type Material,
  Matrix4,
  Mesh,
  MeshBasicMaterial,
  MeshLambertMaterial,
  Object3D,
  PerspectiveCamera,
  Quaternion,
  RepeatWrapping,
  Scene,
  Shape,
  ShapeGeometry,
  SRGBColorSpace,
  Vector2,
  Vector3,
  WebGLRenderer,
} from "three";
import { CAMERA, flowerOutline, petalOutline, TREE, TREE_COLORS, type V3 } from "@/lib/sakura-tree";

const FALLING = 160;
const theme = () => (document.documentElement.classList.contains("dark") ? "dark" : "light");
const shapeOf = (pts: [number, number][]) =>
  // Outlines are in SVG's y-down space; three.js is y-up.
  new Shape(pts.map(([x, y]) => new Vector2(x, -y)));

/** Bark: dark, streaked lengthwise, with pale horizontal lenticels. Drawn once on a canvas. */
function barkTexture() {
  const c = document.createElement("canvas");
  c.width = 64;
  c.height = 128;
  const g = c.getContext("2d")!;
  g.fillStyle = "#ffffff";
  g.fillRect(0, 0, 64, 128);
  let s = 3;
  const r = () => ((s = (s * 16807) % 2147483647) - 1) / 2147483646;
  for (let k = 0; k < 40; k++) {
    g.fillStyle = `rgba(40,20,25,${0.08 + r() * 0.18})`;
    g.fillRect(r() * 64, 0, 1 + r() * 3, 128);
  }
  for (let k = 0; k < 26; k++) {
    g.fillStyle = `rgba(255,236,230,${0.25 + r() * 0.25})`;
    g.fillRect(r() * 60, r() * 126, 4 + r() * 9, 1.2);
  }
  const t = new CanvasTexture(c);
  t.colorSpace = SRGBColorSpace;
  t.wrapS = t.wrapT = RepeatWrapping;
  return t;
}

/**
 * Wind, in the vertex shader of every swaying material: an offset that grows with height and
 * with distance from the trunk, so tips move and the trunk barely does.
 */
function sway(mat: Material, time: { value: number }) {
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
float reach = max(wp.y - 0.9, 0.0) * 0.35 + dot(wp.xz, wp.xz) * 0.05;
float gust = 0.6 + 0.4 * sin(uTime * 0.31);
wp.x += sin(uTime * 1.05 + wp.x * 0.7 + wp.z * 0.5) * 0.045 * reach * gust;
wp.z += cos(uTime * 0.83 + wp.x * 0.4) * 0.03 * reach * gust;
wp.y += sin(uTime * 1.9 + wp.x * 1.7 + wp.z * 1.3) * 0.012 * reach;
vec4 mvPosition = modelViewMatrix * wp;
gl_Position = projectionMatrix * mvPosition;`,
      );
  };
}

export function mount(canvas: HTMLCanvasElement, onFirstFrame: () => void) {
  const renderer = new WebGLRenderer({ canvas, antialias: true, alpha: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  const camera = new PerspectiveCamera(CAMERA.fov, 1, 0.1, 80);
  camera.position.set(...CAMERA.eye);
  const scene = new Scene();
  const tree = new Group();
  scene.add(tree);
  const time = { value: 0 };

  const sky = new HemisphereLight("#fff6f8", "#6b4f57", 1.3);
  const sun = new DirectionalLight("#ffffff", 1.3);
  sun.position.set(-4.5, 10, 4);
  scene.add(sky, sun);

  // Ground: a soft shadow under the crown, and petals already fallen.
  const groundGeo = new CircleGeometry(1, 48);
  const groundMat = new MeshBasicMaterial();
  const ground = new Mesh(groundGeo, groundMat);
  ground.rotation.x = -Math.PI / 2;
  ground.position.x = CAMERA.eye[0] * 0.4;
  ground.scale.set(2.6, 0.32 / (CAMERA.eye[1] / CAMERA.eye[2]), 1);
  scene.add(ground);

  // Limbs: instanced, slightly tapered cylinders per segment and a ball at each joint.
  const bark = barkTexture();
  const barkMat = new MeshLambertMaterial({ map: bark });
  sway(barkMat, time);
  const limbGeo = new CylinderGeometry(0.85, 1, 1, 10, 1, true);
  const jointGeo = new IcosahedronGeometry(1, 1);
  const n = TREE.segments.length;
  const limbs = new InstancedMesh(limbGeo, barkMat, n);
  const joints = new InstancedMesh(jointGeo, barkMat, n);
  const m = new Matrix4();
  const q = new Quaternion();
  const up = new Vector3(0, 1, 0);
  TREE.segments.forEach((s, i) => {
    const a = new Vector3(...s.a);
    const b = new Vector3(...s.b);
    const dir = b.clone().sub(a);
    const len = dir.length();
    q.setFromUnitVectors(up, dir.normalize());
    m.compose(a.clone().lerp(b, 0.5), q, new Vector3(s.r0, len, s.r0));
    limbs.setMatrixAt(i, m);
    m.compose(b, q, new Vector3(s.r1, s.r1, s.r1));
    joints.setMatrixAt(i, m);
  });
  tree.add(limbs, joints);

  // Blossom clouds: a shaded body per cluster behind its flowers, so the crown reads dense.
  const cloudGeo = new IcosahedronGeometry(1, 2);
  // Mostly self-lit, so the cloud reads as soft blossom mass rather than a glossy ball.
  const cloudMat = new MeshLambertMaterial({ color: "#c98a9c", emissive: "#e9a9bb" });
  sway(cloudMat, time);
  const clouds = new InstancedMesh(cloudGeo, cloudMat, TREE.clusters.length);
  const o = new Object3D();
  TREE.clusters.forEach((c, i) => {
    o.position.set(...c.c);
    o.rotation.set(0, 0, 0);
    o.scale.set(c.r * 0.62, c.r * 0.45, c.r * 0.62);
    o.updateMatrix();
    clouds.setMatrixAt(i, o.matrix);
  });
  tree.add(clouds);

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
  sway(flowerMat, time);
  const flowers = new InstancedMesh(flowerGeo, flowerMat, TREE.flowers.length);
  const z = new Vector3(0, 0, 1);
  TREE.flowers.forEach((f, i) => {
    o.position.set(...f.p);
    o.quaternion.setFromUnitVectors(z, new Vector3(...f.n));
    o.rotateZ(f.spin);
    o.scale.setScalar(f.size);
    o.updateMatrix();
    flowers.setMatrixAt(i, o.matrix);
  });
  tree.add(flowers);

  // Buds: small, dark and closed.
  const budGeo = new IcosahedronGeometry(1, 1);
  const budMat = new MeshLambertMaterial();
  sway(budMat, time);
  const buds = new InstancedMesh(budGeo, budMat, TREE.buds.length);
  TREE.buds.forEach((p, i) => {
    o.position.set(...p);
    o.rotation.set(0, 0, 0);
    o.scale.set(0.018, 0.026, 0.018);
    o.updateMatrix();
    buds.setMatrixAt(i, o.matrix);
  });
  tree.add(buds);

  // Petals on the ground, and petals in the air that tumble down and settle for a while.
  const petalGeo = new ShapeGeometry(shapeOf(petalOutline(16)));
  const petalMat = new MeshBasicMaterial({ side: DoubleSide });
  const settled = new InstancedMesh(petalGeo, petalMat, TREE.fallen.length);
  TREE.fallen.forEach((f, i) => {
    o.position.set(f.x, 0.005, f.z);
    o.rotation.set(-Math.PI / 2, 0, f.spin);
    o.scale.setScalar(0.06);
    o.updateMatrix();
    settled.setMatrixAt(i, o.matrix);
  });
  scene.add(settled);

  const petals = new InstancedMesh(petalGeo, petalMat, FALLING);
  scene.add(petals);
  let seed = 7;
  const rand = () => ((seed = (seed * 16807) % 2147483647) - 1) / 2147483646;
  const drift = Array.from({ length: FALLING }, () => ({
    p: new Vector3(),
    fall: 0,
    flutter: 0,
    phase: 0,
    spin: new Vector3(),
    rest: 0,
  }));
  const spawn = (d: (typeof drift)[number], anywhere: boolean) => {
    const f = TREE.flowers[Math.floor(rand() * TREE.flowers.length)];
    d.p.set(f.p[0], anywhere ? rand() * f.p[1] : f.p[1], f.p[2]);
    d.fall = 0.16 + rand() * 0.2;
    d.flutter = 0.6 + rand() * 1.6;
    d.phase = rand() * Math.PI * 2;
    d.spin.set((rand() - 0.5) * 5, (rand() - 0.5) * 5, (rand() - 0.5) * 3);
    d.rest = 0;
  };
  drift.forEach((d) => spawn(d, true));
  const euler = new Euler();

  const paint = () => {
    const c = TREE_COLORS[theme()];
    barkMat.color = new Color(c.bark).lerp(new Color("#ffffff"), theme() === "dark" ? 0.4 : 0.15);
    groundMat.color = new Color(c.ground);
    budMat.color = new Color(c.bud);
    cloudMat.emissive = new Color("#e9a9bb").multiplyScalar(c.bloom);
    const tint = new Color();
    TREE.flowers.forEach((f, i) =>
      flowers.setColorAt(i, tint.setRGB(...(f.rgb.map((v) => v * c.bloom) as V3), SRGBColorSpace)),
    );
    if (flowers.instanceColor) flowers.instanceColor.needsUpdate = true;
    TREE.fallen.forEach((f, i) => settled.setColorAt(i, tint.setRGB(...f.rgb, SRGBColorSpace)));
    if (settled.instanceColor) settled.instanceColor.needsUpdate = true;
    drift.forEach((_, i) =>
      petals.setColorAt(i, tint.setRGB(...TREE.fallen[i % TREE.fallen.length].rgb, SRGBColorSpace)),
    );
    if (petals.instanceColor) petals.instanceColor.needsUpdate = true;
    sky.intensity = theme() === "dark" ? 1.1 : 1.3;
  };
  paint();
  const themeWatch = new MutationObserver(paint);
  themeWatch.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });

  const resize = () => {
    const { clientWidth: w, clientHeight: h } = canvas;
    if (!w || !h) return;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  };
  const sizer = new ResizeObserver(resize);
  sizer.observe(canvas);
  resize();

  // The cursor, -1..1 across the window; the tree turns a little toward it, eased.
  const pointer = { x: 0, y: 0 };
  const onPointer = (e: PointerEvent) => {
    pointer.x = (e.clientX / window.innerWidth) * 2 - 1;
    pointer.y = (e.clientY / window.innerHeight) * 2 - 1;
  };
  window.addEventListener("pointermove", onPointer, { passive: true });

  let frame = 0;
  let last = 0;
  let first = true;
  const tick = (now: number) => {
    const dt = Math.min(0.05, last ? (now - last) / 1000 : 0);
    last = now;
    const t = (time.value += dt);
    tree.rotation.y += (pointer.x * 0.16 - tree.rotation.y) * Math.min(1, dt * 1.5);
    tree.rotation.x += (pointer.y * 0.02 - tree.rotation.x) * Math.min(1, dt * 1.5);
    const gust = 0.6 + 0.4 * Math.sin(t * 0.31);
    drift.forEach((d, i) => {
      if (d.p.y <= 0.006) {
        // Settled: lie flat for a few seconds, then come back as a new petal.
        d.rest += dt;
        if (d.rest > 3 + (i % 5)) spawn(d, false);
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
      o.scale.setScalar(0.06);
      o.updateMatrix();
      petals.setMatrixAt(i, o.matrix);
    });
    petals.instanceMatrix.needsUpdate = true;
    renderer.render(scene, camera);
    if (first) {
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
      window.removeEventListener("pointermove", onPointer);
      for (const g of [groundGeo, limbGeo, jointGeo, cloudGeo, flowerGeo, budGeo, petalGeo])
        g.dispose();
      for (const mat of [groundMat, barkMat, cloudMat, flowerMat, budMat, petalMat]) mat.dispose();
      for (const mesh of [limbs, joints, clouds, flowers, buds, settled, petals]) mesh.dispose();
      bark.dispose();
      renderer.dispose();
    },
  };
}
