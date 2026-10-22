/* Shape is a tagged struct and Option<Int> a flag plus value; each round maps the items
   into a freshly malloc'd array and frees the previous one. */
#include <stdbool.h>
#include <stdlib.h>

#include "runner.h"

enum kind { CIRCLE, RECT, SQUARE };
struct shape { enum kind kind; int64_t a, b; };
struct item { int64_t id; struct shape shape; bool has_tag; int64_t tag; };

static int64_t area(struct shape s) {
  switch (s.kind) {
  case CIRCLE: return 3 * s.a * s.a;
  case RECT: return s.a * s.b;
  default: return s.a * s.a;
  }
}

static struct item grow(struct item it) {
  struct shape s = it.shape;
  switch (s.kind) {
  case CIRCLE: it.shape = (struct shape){RECT, s.a, s.a + 1}; break;
  case RECT: it.shape = (struct shape){SQUARE, (s.a + s.b) % 100, 0}; break;
  default: it.shape = (struct shape){CIRCLE, s.a % 50 + 1, 0}; break;
  }
  if (!it.has_tag) it.has_tag = true, it.tag = 0;
  else if (it.tag > 5) it.has_tag = false;
  else it.tag = it.tag + 1;
  return it;
}

static int64_t run(int64_t n) {
  struct item *items = malloc((size_t)n * sizeof *items);
  int64_t seed = 42;
  for (int64_t i = 0; i < n; i++) {
    seed = seed * 48271 % 2147483647;
    int64_t k = seed % 100;
    struct shape s = k < 33 ? (struct shape){CIRCLE, k, 0}
                   : k < 66 ? (struct shape){RECT, k, k % 7 + 1}
                            : (struct shape){SQUARE, k % 13, 0};
    items[i] = (struct item){i, s, k % 4 != 0, k % 9};
  }
  int64_t total = 0;
  for (int r = 0; r < 5; r++) {
    struct item *next = malloc((size_t)n * sizeof *next);
    for (int64_t i = 0; i < n; i++) next[i] = grow(items[i]);
    free(items);
    items = next;
    for (int64_t i = 0; i < n; i++)
      total += area(items[i].shape) + (items[i].has_tag ? items[i].tag : 0) + items[i].id % 3;
  }
  free(items);
  return total;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 100000); }
