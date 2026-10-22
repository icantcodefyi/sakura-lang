/* Closure control like the Rust and Go ports: a fold over a fresh range array calls back
   into a function that ticks the stateful counter through its function pointer. */
#include <stdlib.h>

#include "runner.h"

typedef struct { int64_t (*next)(void *env); void *env; } tick;

static int64_t counter_next(void *env) { return ++*(int64_t *)env; }

static int64_t fold(const int64_t *xs, int64_t n, int64_t acc,
                    int64_t (*f)(void *env, int64_t acc, int64_t x), void *env) {
  for (int64_t i = 0; i < n; i++) acc = f(env, acc, xs[i]);
  return acc;
}

static int64_t step(void *env, int64_t acc, int64_t x) {
  (void)x;
  tick *t = env;
  return acc + t->next(t->env) % 7;
}

static int64_t run(int64_t n) {
  int64_t value = 0;
  tick t = {counter_next, &value};
  int64_t *xs = malloc((size_t)n * sizeof *xs);
  for (int64_t i = 0; i < n; i++) xs[i] = i;
  int64_t acc = fold(xs, n, 0, step, &t);
  free(xs);
  return acc;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 1000000); }
