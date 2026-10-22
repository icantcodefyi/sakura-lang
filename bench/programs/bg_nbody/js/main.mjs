// n-body, the Benchmarks Game's task: the energy before and after n steps as `%.9f` lines
// (toFixed: ties cannot occur at 9 digits of these energies). Same integrator and operation order
// as the Sakura program; bodies are mutable objects updated in place. run(n) is the digest of the
// output bytes (bench/README.md); output(n) is the output as base64, as Sakura's is.
const PI = 3.141592653589793;
const SOLAR_MASS = 4 * PI * PI;
const DAYS = 365.24;

function planet(x, y, z, vx, vy, vz, m) {
  return { x, y, z, vx: vx * DAYS, vy: vy * DAYS, vz: vz * DAYS, mass: m * SOLAR_MASS };
}

function system() {
  return [
    { x: 0, y: 0, z: 0, vx: 0, vy: 0, vz: 0, mass: SOLAR_MASS },
    planet(4.8414314424647209, -1.16032004402742839, -1.03622044471123109e-1,
      1.66007664274403694e-3, 7.69901118419740425e-3, -6.90460016972063023e-5, 9.54791938424326609e-4),
    planet(8.34336671824457987, 4.12479856412430479, -4.03523417114321381e-1,
      -2.76742510726862411e-3, 4.99852801234917238e-3, 2.30417297573763929e-5, 2.85885980666130812e-4),
    planet(1.2894369562139131e1, -1.51111514016986312e1, -2.23307578892655734e-1,
      2.96460137564761618e-3, 2.3784717395948095e-3, -2.96589568540237556e-5, 4.36624404335156298e-5),
    planet(1.53796971148509165e1, -2.59193146099879641e1, 1.79258772950371181e-1,
      2.68067772490389322e-3, 1.62824170038242295e-3, -9.5159225451971587e-5, 5.15138902046611451e-5),
  ];
}

function offsetMomentum(bs) {
  let px = 0, py = 0, pz = 0;
  for (const b of bs) {
    px += b.vx * b.mass;
    py += b.vy * b.mass;
    pz += b.vz * b.mass;
  }
  bs[0].vx = 0 - px / SOLAR_MASS;
  bs[0].vy = 0 - py / SOLAR_MASS;
  bs[0].vz = 0 - pz / SOLAR_MASS;
}

function energy(bs) {
  let e = 0;
  for (let i = 0; i < bs.length; i++) {
    const b = bs[i];
    e += 0.5 * b.mass * (b.vx * b.vx + b.vy * b.vy + b.vz * b.vz);
    for (let j = i + 1; j < bs.length; j++) {
      const c = bs[j];
      const dx = b.x - c.x, dy = b.y - c.y, dz = b.z - c.z;
      e -= b.mass * c.mass / Math.sqrt(dx * dx + dy * dy + dz * dz);
    }
  }
  return e;
}

function advance(bs, dt) {
  for (let i = 0; i < bs.length; i++) {
    const b = bs[i];
    for (let j = i + 1; j < bs.length; j++) {
      const c = bs[j];
      const dx = b.x - c.x, dy = b.y - c.y, dz = b.z - c.z;
      const d2 = dx * dx + dy * dy + dz * dz;
      const mag = dt / (d2 * Math.sqrt(d2));
      b.vx -= dx * c.mass * mag;
      b.vy -= dy * c.mass * mag;
      b.vz -= dz * c.mass * mag;
      c.vx += dx * b.mass * mag;
      c.vy += dy * b.mass * mag;
      c.vz += dz * b.mass * mag;
    }
  }
  for (const b of bs) {
    b.x += dt * b.vx;
    b.y += dt * b.vy;
    b.z += dt * b.vz;
  }
}

function text(n) {
  const bs = system();
  offsetMomentum(bs);
  const before = energy(bs).toFixed(9);
  for (let i = 0; i < n; i++) advance(bs, 0.01);
  return `${before}\n${energy(bs).toFixed(9)}\n`;
}

// h = (31·h + byte) mod 1,000,000,007 over every byte.
function digest(bytes) {
  let h = 0;
  for (let i = 0; i < bytes.length; i++) h = (h * 31 + bytes[i]) % 1000000007;
  return h;
}

const encoder = new TextEncoder();
export function run(n) {
  return digest(encoder.encode(text(n)));
}

export function output(n) {
  return Buffer.from(text(n)).toString("base64");
}
