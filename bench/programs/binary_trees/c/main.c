/* Every node is malloc'd and every tree freed after its check (no pool or arena). */
#include <stdlib.h>

#include "runner.h"

struct node { struct node *l, *r; };

static struct node *bottom_up(int64_t d) {
  struct node *t = malloc(sizeof *t);
  t->l = d > 0 ? bottom_up(d - 1) : NULL;
  t->r = d > 0 ? bottom_up(d - 1) : NULL;
  return t;
}

static int64_t check(const struct node *t) { return t ? 1 + check(t->l) + check(t->r) : 0; }

static void release(struct node *t) {
  if (!t) return;
  release(t->l);
  release(t->r);
  free(t);
}

static int64_t check_and_release(struct node *t) {
  int64_t c = check(t);
  release(t);
  return c;
}

static int64_t run(int64_t n) {
  int64_t min_depth = 4, max_depth = n > min_depth + 2 ? n : min_depth + 2;
  int64_t total = check_and_release(bottom_up(max_depth + 1));
  struct node *long_lived = bottom_up(max_depth);
  for (int64_t d = min_depth; d <= max_depth; d += 2) {
    int64_t iterations = (int64_t)1 << (max_depth - d + min_depth);
    for (int64_t i = 0; i < iterations; i++) total += check_and_release(bottom_up(d));
  }
  return total + check_and_release(long_lived);
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 14); }
