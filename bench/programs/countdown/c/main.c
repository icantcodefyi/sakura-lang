/* Matched lane allows loops here (plan §2.1), as in the JS and Go controls: the self-tail
   sum and the mutual-recursion evenness become two loops with the same sums in the same order. */
#include <stdbool.h>

#include "runner.h"

static int64_t run(int64_t n) {
  int64_t acc = 0;
  for (int64_t i = n; i > 0; i--) acc += i;
  bool even = true;
  for (int64_t i = n; i > 0; i--) even = !even;
  return acc + (even ? 1 : 0);
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 1000000); }
