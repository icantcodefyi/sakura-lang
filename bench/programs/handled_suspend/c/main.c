/* Go's resumable state closure in C: the loop's state lives in a struct, resume() returns
   false when it parks at n/2, and the caller advances the virtual clock 1 ms and resumes once. */
#include <stdbool.h>
#include <stdlib.h>

#include "runner.h"

typedef struct { int64_t (*next)(void *env); void *env; } tick;

static int64_t counter_next(void *env) { return ++*(int64_t *)env; }

struct task { tick t; int64_t n, i, acc; bool suspended; };

static bool resume(struct task *task) {
  while (task->i < task->n) {
    task->acc += task->t.next(task->t.env) % 7;
    int64_t at = task->i++;
    if (at == task->n / 2 && !task->suspended) {
      task->suspended = true;
      return false;
    }
  }
  return true;
}

static int64_t run(int64_t n) {
  int64_t value = 0, elapsed = 0;
  struct task task = {{counter_next, &value}, n, 0, 0, false};
  if (!resume(&task)) {
    elapsed += 1;
    if (!resume(&task)) abort(); /* only one suspension */
  }
  return task.acc + elapsed;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 1000000); }
