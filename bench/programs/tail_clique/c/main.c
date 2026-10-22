/* The three mutually tail-calling Sakura states as a state machine in one loop (as the JS
   and Go controls do); each step adds the weight of the edge it takes. */
#include "runner.h"

enum state { A, B, C };

static int64_t run(int64_t n) {
  enum state state = A;
  int64_t acc = 0;
  for (; n != 0; n--) {
    switch (state) {
    case A:
      if (n % 2 == 0) state = B, acc += 1;
      else state = C, acc += 2;
      break;
    case B:
      if (n % 3 == 0) state = A, acc += 3;
      else state = C, acc += 4;
      break;
    case C:
      if (n % 5 == 0) state = A, acc += 5;
      else state = B, acc += 6;
      break;
    }
  }
  return acc;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 10000000); }
