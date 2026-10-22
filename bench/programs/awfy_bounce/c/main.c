/* Bounce from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/bounce.js
   and som.js's Random; Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM
   benchmarks). Follows AWFY's C++ port: the balls are mutable structs in a fixed array on the
   stack and Random's seed is a field. Random keeps C's `&` (Sakura has no bitwise operators and
   takes `% 65536`, the same value). run(n) sums n benchmark() results. */
#include <stdbool.h>
#include <stdlib.h>

#include "runner.h"

typedef struct {
  int64_t seed;
} Random;

static int64_t random_next(Random *random) {
  random->seed = ((random->seed * 1309) + 13849) & 65535;
  return random->seed;
}

typedef struct {
  int64_t x, y, x_vel, y_vel;
} Ball;

static void ball_init(Ball *ball, Random *random) {
  ball->x = random_next(random) % 500;
  ball->y = random_next(random) % 500;
  ball->x_vel = (random_next(random) % 300) - 150;
  ball->y_vel = (random_next(random) % 300) - 150;
}

static bool ball_bounce(Ball *ball) {
  const int64_t x_limit = 500;
  const int64_t y_limit = 500;
  bool bounced = false;

  ball->x += ball->x_vel;
  ball->y += ball->y_vel;

  if (ball->x > x_limit) {
    ball->x = x_limit;
    ball->x_vel = 0 - llabs(ball->x_vel);
    bounced = true;
  }

  if (ball->x < 0) {
    ball->x = 0;
    ball->x_vel = llabs(ball->x_vel);
    bounced = true;
  }

  if (ball->y > y_limit) {
    ball->y = y_limit;
    ball->y_vel = 0 - llabs(ball->y_vel);
    bounced = true;
  }

  if (ball->y < 0) {
    ball->y = 0;
    ball->y_vel = llabs(ball->y_vel);
    bounced = true;
  }

  return bounced;
}

static int64_t benchmark(void) {
  Random random = {74755};
  enum { BALL_COUNT = 100 };
  int64_t bounces = 0;
  Ball balls[BALL_COUNT];

  for (int64_t i = 0; i < BALL_COUNT; i += 1) ball_init(&balls[i], &random);

  for (int64_t i = 0; i < 50; i += 1)
    for (int64_t j = 0; j < BALL_COUNT; j += 1)
      if (ball_bounce(&balls[j])) bounces += 1;

  return bounces;
}

static int64_t run(int64_t n) {
  int64_t sum = 0;
  for (int64_t i = 0; i < n; i += 1) sum += benchmark();
  return sum;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 10000); }
