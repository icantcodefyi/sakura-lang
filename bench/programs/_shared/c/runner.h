#ifndef SAKURA_BENCH_RUNNER_H
#define SAKURA_BENCH_RUNNER_H
#include <stdint.h>

/* The ports' protocol (bench/README.md): `main values <n>...` prints one JSON line,
   `main measure <n> [--stable]` nine sample lines. Sizes outside 0..maximum exit 2 silently. */
int bench_main(int argc, char **argv, int64_t (*run)(int64_t n), int64_t maximum);

#endif
