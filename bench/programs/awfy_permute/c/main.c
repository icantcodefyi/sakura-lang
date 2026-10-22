/* Permute from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/permute.js;
   Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's C++
   port: the benchmark object's `count` and the six-element array `v` are fields, the array
   allocated per benchmark() and freed directly; swaps write it in place. run(n) sums n
   benchmark() results. */
#include <stdio.h>
#include <stdlib.h>

#include "runner.h"

typedef struct {
  int64_t count;
  int64_t *v;
} Permute;

static void swap(Permute *self, int64_t i, int64_t j) {
  const int64_t tmp = self->v[i];
  self->v[i] = self->v[j];
  self->v[j] = tmp;
}

static void permute(Permute *self, int64_t n) {
  self->count += 1;
  if (n != 0) {
    const int64_t n1 = n - 1;
    permute(self, n1);
    for (int64_t i = n1; i >= 0; i -= 1) {
      swap(self, n1, i);
      permute(self, n1);
      swap(self, n1, i);
    }
  }
}

static int64_t benchmark(Permute *self) {
  self->count = 0;
  self->v = calloc(6, sizeof *self->v);
  if (!self->v) {
    fprintf(stderr, "out of memory\n");
    exit(1);
  }
  permute(self, 6);
  free(self->v);
  self->v = NULL;
  return self->count;
}

static int64_t run(int64_t n) {
  Permute bench = {0, NULL};
  int64_t sum = 0;
  for (int64_t i = 0; i < n; i += 1) sum += benchmark(&bench);
  return sum;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 4000); }
