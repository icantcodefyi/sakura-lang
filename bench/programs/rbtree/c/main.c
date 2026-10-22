/* Persistent (path-copying) Okasaki red-black tree: no node is mutated after creation.
   Versions share nodes, so every node is threaded on the run's allocation list and the
   whole list is freed when run() returns. */
#include <stdbool.h>
#include <stdlib.h>

#include "runner.h"

typedef struct tree {
  bool red;
  struct tree *l, *r;
  int64_t k;
  struct tree *allocated_next;
} tree;

static tree *allocated;

static tree *node(bool red, tree *l, int64_t k, tree *r) {
  tree *t = malloc(sizeof *t);
  *t = (tree){red, l, r, k, allocated};
  allocated = t;
  return t;
}

static bool is_red(const tree *t) { return t && t->red; }

static tree *balance(bool red, tree *l, int64_t k, tree *r) {
  if (!red) {
    if (is_red(l) && is_red(l->l))
      return node(true, node(false, l->l->l, l->l->k, l->l->r), l->k, node(false, l->r, k, r));
    if (is_red(l) && is_red(l->r))
      return node(true, node(false, l->l, l->k, l->r->l), l->r->k, node(false, l->r->r, k, r));
    if (is_red(r) && is_red(r->l))
      return node(true, node(false, l, k, r->l->l), r->l->k, node(false, r->l->r, r->k, r->r));
    if (is_red(r) && is_red(r->r))
      return node(true, node(false, l, k, r->l), r->k, node(false, r->r->l, r->r->k, r->r->r));
  }
  return node(red, l, k, r);
}

static tree *ins(tree *t, int64_t k) {
  if (!t) return node(true, NULL, k, NULL);
  if (k < t->k) return balance(t->red, ins(t->l, k), t->k, t->r);
  if (k > t->k) return balance(t->red, t->l, t->k, ins(t->r, k));
  return t;
}

static tree *insert(tree *t, int64_t k) {
  tree *s = ins(t, k);
  return node(false, s->l, s->k, s->r);
}

static bool member(const tree *t, int64_t k) {
  while (t) {
    if (k < t->k) t = t->l;
    else if (k > t->k) t = t->r;
    else return true;
  }
  return false;
}

static int64_t size(const tree *t) { return t ? size(t->l) + 1 + size(t->r) : 0; }

static int64_t black_height(const tree *t) { return t ? !t->red + black_height(t->l) : 0; }

static int64_t run(int64_t n) {
  tree *t = NULL;
  tree **kept = malloc((size_t)(n / 100 + 1) * sizeof *kept);
  int64_t nkept = 0;
  for (int64_t i = 0; i < n; i++) {
    t = insert(t, i * 7919 % (2 * n + 1));
    if (i % 100 == 0) kept[nkept++] = t;
  }
  int64_t sum = size(t);
  for (int64_t j = 0; j < nkept; j++) sum += black_height(kept[j]) + member(kept[j], j * 3);
  free(kept);
  while (allocated) {
    tree *next = allocated->allocated_next;
    free(allocated);
    allocated = next;
  }
  return sum;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 100000); }
