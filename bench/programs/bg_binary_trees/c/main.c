/* binary-trees, the Benchmarks Game's task: the stretch tree, the long-lived tree and the
   bottom-up trees per depth, with the suite's report lines. Every node is malloc'd, leaves
   included, and every tree freed after its check (per-node allocation: no pool or arena).
   `main output <n>` writes the program's output; run(n) is the digest of the same bytes
   (bench/README.md). */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "runner.h"

#define MAXIMUM 21
#define TEXT_CAP 2048 /* the output for MAXIMUM is under 600 bytes */

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

/* What the Benchmarks Game program writes for n, into out (TEXT_CAP bytes); returns its length. */
static size_t text(int64_t n, char *out) {
  int64_t min_depth = 4, max_depth = n > min_depth + 2 ? n : min_depth + 2;
  int64_t stretch_depth = max_depth + 1;
  int len = snprintf(out, TEXT_CAP, "stretch tree of depth %lld\t check: %lld\n", (long long)stretch_depth,
                     (long long)check_and_release(bottom_up(stretch_depth)));
  struct node *long_lived = bottom_up(max_depth);
  for (int64_t d = min_depth; d <= max_depth; d += 2) {
    int64_t iterations = (int64_t)1 << (max_depth - d + min_depth);
    int64_t sum = 0;
    for (int64_t i = 0; i < iterations; i++) sum += check_and_release(bottom_up(d));
    len += snprintf(out + len, (size_t)(TEXT_CAP - len), "%lld\t trees of depth %lld\t check: %lld\n",
                    (long long)iterations, (long long)d, (long long)sum);
  }
  len += snprintf(out + len, (size_t)(TEXT_CAP - len), "long lived tree of depth %lld\t check: %lld\n",
                  (long long)max_depth, (long long)check_and_release(long_lived));
  return (size_t)len;
}

/* h = (31·h + byte) mod 1,000,000,007 over every byte. */
static int64_t digest(const char *s, size_t len) {
  int64_t h = 0;
  for (size_t i = 0; i < len; i++) h = (h * 31 + (unsigned char)s[i]) % 1000000007;
  return h;
}

static int64_t run(int64_t n) {
  char out[TEXT_CAP];
  size_t len = text(n, out);
  return digest(out, len);
}

int main(int argc, char **argv) {
  if (argc == 3 && strcmp(argv[1], "output") == 0) {
    long long n = strtoll(argv[2], NULL, 10);
    if (n < 0 || n > MAXIMUM) return 2;
    char out[TEXT_CAP];
    size_t len = text(n, out);
    fwrite(out, 1, len, stdout);
    return 0;
  }
  return bench_main(argc, argv, run, MAXIMUM);
}
