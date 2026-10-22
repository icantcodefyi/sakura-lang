/* Queens from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/queens.js;
   Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's C++
   port: the four arrays are fields of the benchmark object, allocated by every queens() call,
   filled, written in place and freed directly. run(n) sums n benchmark() results, true = 1. */
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

#include "runner.h"

typedef struct {
  bool *free_maxs;
  bool *free_rows;
  bool *free_mins;
  int64_t *queen_rows;
} Queens;

static void *allocate(size_t count, size_t size) {
  void *p = malloc(count * size);
  if (!p) {
    fprintf(stderr, "out of memory\n");
    exit(1);
  }
  return p;
}

static bool get_row_column(const Queens *self, int64_t r, int64_t c) {
  return self->free_rows[r] && self->free_maxs[c + r] && self->free_mins[c - r + 7];
}

static void set_row_column(Queens *self, int64_t r, int64_t c, bool v) {
  self->free_rows[r] = v;
  self->free_maxs[c + r] = v;
  self->free_mins[c - r + 7] = v;
}

static bool place_queen(Queens *self, int64_t c) {
  for (int64_t r = 0; r < 8; r += 1) {
    if (get_row_column(self, r, c)) {
      self->queen_rows[r] = c;
      set_row_column(self, r, c, false);

      if (c == 7) return true;

      if (place_queen(self, c + 1)) return true;
      set_row_column(self, r, c, true);
    }
  }
  return false;
}

static bool queens(Queens *self) {
  self->free_rows = allocate(8, sizeof *self->free_rows);
  self->free_maxs = allocate(16, sizeof *self->free_maxs);
  self->free_mins = allocate(16, sizeof *self->free_mins);
  self->queen_rows = allocate(8, sizeof *self->queen_rows);
  for (int i = 0; i < 8; i += 1) self->free_rows[i] = true;
  for (int i = 0; i < 16; i += 1) self->free_maxs[i] = true;
  for (int i = 0; i < 16; i += 1) self->free_mins[i] = true;
  for (int i = 0; i < 8; i += 1) self->queen_rows[i] = -1;

  const bool result = place_queen(self, 0);

  free(self->free_rows);
  free(self->free_maxs);
  free(self->free_mins);
  free(self->queen_rows);
  return result;
}

static bool benchmark(Queens *self) {
  bool result = true;
  for (int64_t i = 0; i < 10; i += 1) result = result && queens(self);
  return result;
}

static int64_t run(int64_t n) {
  Queens bench = {NULL, NULL, NULL, NULL};
  int64_t sum = 0;
  for (int64_t i = 0; i < n; i += 1) sum += benchmark(&bench) ? 1 : 0;
  return sum;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 8000); }
