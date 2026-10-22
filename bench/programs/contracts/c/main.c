/* The requires/ensures clauses as explicit checks that abort, in the places the Rust and Go
   ports check them; contracts_plain is this file without the checks. */
#include <stdlib.h>

#include "runner.h"

static int64_t increment(int64_t x) {
  if (!(x >= 0)) abort(); /* D020: requires x >= 0 */
  int64_t result = x + 1;
  if (!(result > x)) abort(); /* D021: ensures result > x */
  return result;
}

static int64_t total(const int64_t *xs, int len) {
  for (int k = 0; k < len; k++)
    if (!(xs[k] >= 0)) abort(); /* D020: requires List.all(xs, x >= 0) */
  int64_t sum = 0;
  for (int k = 0; k < len; k++) sum += xs[k];
  return sum;
}

static int64_t run(int64_t n) {
  int64_t xs[16];
  for (int k = 0; k < 16; k++) xs[k] = k;
  int64_t i = 0, acc = 0;
  while (i < n) {
    i = increment(i);
    if (i % 16 == 0) acc += total(xs, 16);
  }
  return acc + i;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 1000000); }
