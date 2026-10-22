/* Storage from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/storage.js
   and som.js's Random; Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM
   benchmarks). Follows AWFY's C++ port: an array is a heap block of ArrayTree slots, each with a
   `children` pointer (NULL in a leaf array's k slots, AWFY's `new Array(k)`), and the tree is
   freed from the root after each benchmark(). Random keeps C's `&` (Sakura takes `% 65536`, the
   same value). run(n) sums n benchmark() results. */
#include <stdio.h>
#include <stdlib.h>

#include "runner.h"

typedef struct {
  int64_t seed;
} Random;

static int64_t random_next(Random *random) {
  random->seed = ((random->seed * 1309) + 13849) & 65535;
  return random->seed;
}

typedef struct ArrayTree {
  struct ArrayTree *children;
  int64_t length; /* the slots in `children`, for freeing */
} ArrayTree;

typedef struct {
  ArrayTree *items;
  int64_t length;
} Array;

static Array new_array(int64_t length) {
  ArrayTree *items = calloc((size_t)length, sizeof *items);
  if (!items) {
    fprintf(stderr, "out of memory\n");
    exit(1);
  }
  return (Array){items, length};
}

static void free_array(Array arr) {
  for (int64_t i = 0; i < arr.length; i += 1)
    if (arr.items[i].children) free_array((Array){arr.items[i].children, arr.items[i].length});
  free(arr.items);
}

typedef struct {
  int64_t count;
} Storage;

static Array build_tree_depth(Storage *self, int64_t depth, Random *random) {
  self->count += 1;
  if (depth == 1) return new_array((random_next(random) % 10) + 1);

  Array arr = new_array(4);
  for (int64_t i = 0; i < 4; i += 1) {
    Array child = build_tree_depth(self, depth - 1, random);
    arr.items[i].children = child.items;
    arr.items[i].length = child.length;
  }
  return arr;
}

static int64_t benchmark(Storage *self) {
  Random random = {74755};
  self->count = 0;
  free_array(build_tree_depth(self, 7, &random));
  return self->count;
}

static int64_t run(int64_t n) {
  Storage bench = {0};
  int64_t sum = 0;
  for (int64_t i = 0; i < n; i += 1) sum += benchmark(&bench);
  return sum;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 2000); }
