#include "runner.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static double now(void) {
  struct timespec t;
  clock_gettime(CLOCK_MONOTONIC, &t);
  return (double)t.tv_sec + (double)t.tv_nsec / 1e9;
}

static int64_t parse_size(const char *s, int64_t maximum) {
  char *end;
  long long n = strtoll(s, &end, 10);
  if (*s == '\0' || *end != '\0' || n < 0 || n > maximum) {
    fprintf(stderr, "size outside oracle resource bound: %s\n", s);
    exit(2);
  }
  return n;
}

int bench_main(int argc, char **argv, int64_t (*run)(int64_t n), int64_t maximum) {
  if (argc < 3) {
    fprintf(stderr, "values <n>... or measure <n> [--stable]\n");
    return 2;
  }
  if (strcmp(argv[1], "values") == 0) {
    int64_t *sizes = malloc(sizeof(int64_t) * (size_t)argc);
    for (int i = 2; i < argc; i++) sizes[i] = parse_size(argv[i], maximum);
    printf("{\"values\":{");
    for (int i = 2; i < argc; i++)
      printf("%s\"%lld\":\"%lld\"", i > 2 ? "," : "", (long long)sizes[i], (long long)run(sizes[i]));
    printf("}}\n");
    free(sizes);
    return 0;
  }
  int stable = argc == 4 && strcmp(argv[3], "--stable") == 0;
  if (strcmp(argv[1], "measure") != 0 || (argc != 3 && !stable)) {
    fprintf(stderr, "measure <n> [--stable]\n");
    return 2;
  }
  int64_t n = parse_size(argv[2], maximum);
  /* volatile keeps every call and its checksum live */
  volatile int64_t size = n, sink = 0;
  double start = now();
  while (now() - start < (stable ? 2.0 : 0.25)) sink = run(size);
  long count = 1;
  for (;;) {
    double t = now();
    for (long i = 0; i < count; i++) sink = run(size);
    if (now() - t >= 0.03 || count >= (1L << 20)) break;
    count *= 2;
  }
  for (int s = 0; s < 9; s++) {
    double t = now();
    for (long i = 0; i < count; i++) sink = run(size);
    double ms = (now() - t) * 1000.0 / (double)count;
    printf("{\"n\":%lld,\"ms\":%.12g,\"checksum\":\"%lld\"}\n", (long long)n, ms, (long long)sink);
  }
  return 0;
}
