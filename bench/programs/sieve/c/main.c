/* One flag array written in place (the Sakura source's List.set, design §17.126). */
#include <stdbool.h>
#include <stdlib.h>

#include "runner.h"

static int64_t run(int64_t n) {
  bool *flags = malloc((size_t)n * sizeof *flags);
  for (int64_t i = 0; i < n; i++) flags[i] = i >= 2;
  for (int64_t p = 2; p * p < n; p++)
    if (flags[p])
      for (int64_t j = p * p; j < n; j += p) flags[j] = false;
  int64_t count = 0;
  for (int64_t i = 0; i < n; i++) count += flags[i];
  free(flags);
  return count;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 200000); }
