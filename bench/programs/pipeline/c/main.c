/* Every stage writes a fresh malloc'd array, as Sakura's List.range/map/filter/fold do;
   the filter buffer is sized for the worst case rather than grown. */
#include <stdlib.h>

#include "runner.h"

static int64_t run(int64_t n) {
  int64_t *xs = malloc((size_t)n * sizeof *xs);
  for (int64_t i = 0; i < n; i++) xs[i] = i;
  int64_t *ys = malloc((size_t)n * sizeof *ys);
  for (int64_t i = 0; i < n; i++) ys[i] = xs[i] * 7 % 1000;
  int64_t *zs = malloc((size_t)n * sizeof *zs), len = 0;
  for (int64_t i = 0; i < n; i++)
    if (ys[i] % 2 == 0) zs[len++] = ys[i];
  int64_t acc = 0;
  for (int64_t i = 0; i < len; i++) acc += zs[i];
  free(xs);
  free(ys);
  free(zs);
  return acc;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 300000); }
