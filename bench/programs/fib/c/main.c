#include "runner.h"

static int64_t fib(int64_t n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }

static int64_t run(int64_t n) { return fib(n); }

int main(int argc, char **argv) { return bench_main(argc, argv, run, 30); }
