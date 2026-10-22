/* Closure control like the Rust and Go ports: a stateful counter behind a function pointer.
   Like those ports there is no handler stack, so the 4 handler frames are not modelled
   and this is handled_d1's loop. */
#include "runner.h"

typedef struct { int64_t (*next)(void *env); void *env; } tick;

static int64_t counter_next(void *env) { return ++*(int64_t *)env; }

static int64_t run(int64_t n) {
  int64_t value = 0;
  tick t = {counter_next, &value};
  int64_t acc = 0;
  for (int64_t i = 0; i < n; i++) acc += t.next(t.env) % 7;
  return acc;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 1000000); }
