/* NBody from Are We Fast Yet (benchmarks/JavaScript/nbody.js: the Benchmarks Game's n-body,
   contributed by Mark C. Lewis, modified slightly by Chad Whipkey; revised BSD licence). AWFY's
   Body and NBodySystem as a struct and functions over an array of bodies updated in place, in
   AWFY's operation order; FP_CONTRACT OFF keeps the compiler from fusing multiply-adds.
   run(n) is the energy after n advances times 10^16, rounded half away from zero (llround). */
#pragma STDC FP_CONTRACT OFF
#include <math.h>

#include "runner.h"

#define PI 3.141592653589793
#define SOLAR_MASS (4 * PI * PI)
#define DAYS_PER_YER 365.24
#define NUM_BODIES 5

struct body { double x, y, z, vx, vy, vz, mass; };

static struct body body_new(double x, double y, double z, double vx, double vy, double vz,
                            double mass) {
  return (struct body){x, y, z, vx * DAYS_PER_YER, vy * DAYS_PER_YER, vz * DAYS_PER_YER,
                       mass * SOLAR_MASS};
}

static void offset_momentum(struct body *b, double px, double py, double pz) {
  b->vx = 0.0 - (px / SOLAR_MASS);
  b->vy = 0.0 - (py / SOLAR_MASS);
  b->vz = 0.0 - (pz / SOLAR_MASS);
}

static void create_bodies(struct body *bodies) {
  bodies[0] = body_new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0); /* sun */
  bodies[1] = body_new(4.84143144246472090e+00, -1.16032004402742839e+00,      /* jupiter */
                       -1.03622044471123109e-01, 1.66007664274403694e-03,
                       7.69901118419740425e-03, -6.90460016972063023e-05,
                       9.54791938424326609e-04);
  bodies[2] = body_new(8.34336671824457987e+00, 4.12479856412430479e+00,       /* saturn */
                       -4.03523417114321381e-01, -2.76742510726862411e-03,
                       4.99852801234917238e-03, 2.30417297573763929e-05,
                       2.85885980666130812e-04);
  bodies[3] = body_new(1.28943695621391310e+01, -1.51111514016986312e+01,      /* uranus */
                       -2.23307578892655734e-01, 2.96460137564761618e-03,
                       2.37847173959480950e-03, -2.96589568540237556e-05,
                       4.36624404335156298e-05);
  bodies[4] = body_new(1.53796971148509165e+01, -2.59193146099879641e+01,      /* neptune */
                       1.79258772950371181e-01, 2.68067772490389322e-03,
                       1.62824170038242295e-03, -9.51592254519715870e-05,
                       5.15138902046611451e-05);

  double px = 0.0, py = 0.0, pz = 0.0;
  for (int i = 0; i < NUM_BODIES; i++) {
    px += bodies[i].vx * bodies[i].mass;
    py += bodies[i].vy * bodies[i].mass;
    pz += bodies[i].vz * bodies[i].mass;
  }
  offset_momentum(&bodies[0], px, py, pz);
}

static void advance(struct body *bodies, double dt) {
  for (int i = 0; i < NUM_BODIES; i++) {
    struct body *i_body = &bodies[i];
    for (int j = i + 1; j < NUM_BODIES; j++) {
      struct body *j_body = &bodies[j];
      double dx = i_body->x - j_body->x;
      double dy = i_body->y - j_body->y;
      double dz = i_body->z - j_body->z;

      double d_squared = dx * dx + dy * dy + dz * dz;
      double distance = sqrt(d_squared);
      double mag = dt / (d_squared * distance);

      i_body->vx -= dx * j_body->mass * mag;
      i_body->vy -= dy * j_body->mass * mag;
      i_body->vz -= dz * j_body->mass * mag;

      j_body->vx += dx * i_body->mass * mag;
      j_body->vy += dy * i_body->mass * mag;
      j_body->vz += dz * i_body->mass * mag;
    }
  }
  for (int i = 0; i < NUM_BODIES; i++) {
    bodies[i].x += dt * bodies[i].vx;
    bodies[i].y += dt * bodies[i].vy;
    bodies[i].z += dt * bodies[i].vz;
  }
}

static double energy(const struct body *bodies) {
  double e = 0.0;
  for (int i = 0; i < NUM_BODIES; i++) {
    const struct body *i_body = &bodies[i];
    e += 0.5 * i_body->mass *
         (i_body->vx * i_body->vx + i_body->vy * i_body->vy + i_body->vz * i_body->vz);
    for (int j = i + 1; j < NUM_BODIES; j++) {
      const struct body *j_body = &bodies[j];
      double dx = i_body->x - j_body->x;
      double dy = i_body->y - j_body->y;
      double dz = i_body->z - j_body->z;

      double distance = sqrt(dx * dx + dy * dy + dz * dz);
      e -= (i_body->mass * j_body->mass) / distance;
    }
  }
  return e;
}

static int64_t run(int64_t inner_iterations) {
  struct body system[NUM_BODIES];
  create_bodies(system);
  for (int64_t i = 0; i < inner_iterations; i++) advance(system, 0.01);
  return llround(energy(system) * 1e16);
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 1000000); }
