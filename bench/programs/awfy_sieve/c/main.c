/* Sieve from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/sieve.js;
   Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's C++
   port: the 5000 flags are a stack array filled with true and written in place. run(n) sums n
   benchmark() results. */
#include <stdbool.h>

#include "runner.h"

static int64_t sieve(bool *flags, int64_t size) {
  int64_t prime_count = 0;

  for (int64_t i = 2; i <= size; i += 1) {
    if (flags[i - 1]) {
      prime_count += 1;
      int64_t k = i + i;
      while (k <= size) {
        flags[k - 1] = false;
        k += i;
      }
    }
  }
  return prime_count;
}

static int64_t benchmark(void) {
  bool flags[5000];
  for (int i = 0; i < 5000; i += 1) flags[i] = true;
  return sieve(flags, 5000);
}

static int64_t run(int64_t n) {
  int64_t sum = 0;
  for (int64_t i = 0; i < n; i += 1) sum += benchmark();
  return sum;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 3000); }
