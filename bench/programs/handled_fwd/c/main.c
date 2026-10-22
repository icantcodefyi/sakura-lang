/* Closure control like the Rust and Go ports: the forwarding handler is a second function
   pointer that delegates to the stateful counter's. */
#include "runner.h"

typedef struct { int64_t (*next)(void *env); void *env; } tick;

static int64_t counter_next(void *env) { return ++*(int64_t *)env; }

static int64_t forward_next(void *env) {
  tick *outer = env;
  return outer->next(outer->env);
}

static int64_t run(int64_t n) {
  int64_t value = 0;
  tick outer = {counter_next, &value};
  tick t = {forward_next, &outer};
  int64_t acc = 0;
  for (int64_t i = 0; i < n; i++) acc += t.next(t.env) % 7;
  return acc;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 1000000); }
