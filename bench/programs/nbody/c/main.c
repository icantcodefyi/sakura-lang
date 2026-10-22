/* Bodies are one array updated in place (Sakura copies records); the float operation order
   is the source's, and FP_CONTRACT OFF keeps the compiler from fusing multiply-adds. */
#pragma STDC FP_CONTRACT OFF
#include <math.h>

#include "runner.h"

struct body { double x, y, z, vx, vy, vz, mass; };

static const double pi = 3.141592653589793;
static const double days = 365.24;

static struct body planet(double x, double y, double z, double vx, double vy, double vz, double m) {
  double solar_mass = 4.0 * pi * pi;
  return (struct body){x, y, z, vx * days, vy * days, vz * days, m * solar_mass};
}

static int64_t run(int64_t n) {
  double solar_mass = 4.0 * pi * pi;
  struct body bs[5] = {
    {0, 0, 0, 0, 0, 0, solar_mass},
    planet(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
           1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05,
           9.54791938424326609e-04),
    planet(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
           -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05,
           2.85885980666130812e-04),
    planet(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
           2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05,
           4.36624404335156298e-05),
    planet(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
           2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05,
           5.15138902046611451e-05),
  };
  double px = 0, py = 0, pz = 0;
  for (int i = 0; i < 5; i++) {
    px += bs[i].vx * bs[i].mass;
    py += bs[i].vy * bs[i].mass;
    pz += bs[i].vz * bs[i].mass;
  }
  bs[0].vx = 0.0 - px / solar_mass;
  bs[0].vy = 0.0 - py / solar_mass;
  bs[0].vz = 0.0 - pz / solar_mass;

  for (int64_t step = 0; step < n; step++) {
    for (int i = 0; i < 5; i++) {
      struct body *b = &bs[i];
      for (int j = i + 1; j < 5; j++) {
        struct body *c = &bs[j];
        double dx = b->x - c->x, dy = b->y - c->y, dz = b->z - c->z;
        double d2 = dx * dx + dy * dy + dz * dz;
        double mag = 0.01 / (d2 * sqrt(d2));
        b->vx = b->vx - dx * c->mass * mag;
        b->vy = b->vy - dy * c->mass * mag;
        b->vz = b->vz - dz * c->mass * mag;
        c->vx = c->vx + dx * b->mass * mag;
        c->vy = c->vy + dy * b->mass * mag;
        c->vz = c->vz + dz * b->mass * mag;
      }
    }
    for (int i = 0; i < 5; i++) {
      bs[i].x = bs[i].x + 0.01 * bs[i].vx;
      bs[i].y = bs[i].y + 0.01 * bs[i].vy;
      bs[i].z = bs[i].z + 0.01 * bs[i].vz;
    }
  }

  double e = 0.0;
  for (int i = 0; i < 5; i++) {
    struct body *b = &bs[i];
    e = e + 0.5 * b->mass * (b->vx * b->vx + b->vy * b->vy + b->vz * b->vz);
    for (int j = i + 1; j < 5; j++) {
      struct body *c = &bs[j];
      double dx = b->x - c->x, dy = b->y - c->y, dz = b->z - c->z;
      e = e - b->mass * c->mass / sqrt(dx * dx + dy * dy + dz * dz);
    }
  }
  return llround(e * 1.0e9);
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 100000); }
