/* fannkuch-redux, the Benchmarks Game's task: the task description's sequential algorithm
   (Oleg Mazurov's count[] order, as the task page credits him) over int
   arrays updated in place, where the Sakura program updates persistent Lists; same permutation
   order, flips and checksum. For n = 0 the checksum and maximum are 0, as in the Sakura program.
   `main output <n>` writes the program's output; run(n) is the digest of the same bytes
   (bench/README.md). */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "runner.h"

#define MAXIMUM 12

/* Sets *checksum and returns the maximum flip count over all n! permutations. */
static int fannkuch(int n, int *checksum) {
  *checksum = 0;
  if (n == 0) return 0;
  int perm[MAXIMUM], perm1[MAXIMUM], count[MAXIMUM] = {0};
  int max_flips = 0, perm_count = 0;
  for (int i = 0; i < n; i++) perm1[i] = i;
  int r = n;
  for (;;) {
    while (r != 1) {
      count[r - 1] = r;
      r--;
    }
    for (int i = 0; i < n; i++) perm[i] = perm1[i];
    int flips = 0, k;
    while ((k = perm[0]) != 0) {
      int k2 = (k + 1) >> 1;
      for (int i = 0; i < k2; i++) {
        int t = perm[i];
        perm[i] = perm[k - i];
        perm[k - i] = t;
      }
      flips++;
    }
    if (flips > max_flips) max_flips = flips;
    *checksum += perm_count % 2 == 0 ? flips : -flips;
    for (;;) {
      if (r == n) return max_flips;
      int perm0 = perm1[0];
      for (int i = 0; i < r; i++) perm1[i] = perm1[i + 1];
      perm1[r] = perm0;
      count[r]--;
      if (count[r] > 0) break;
      r++;
    }
    perm_count++;
  }
}

/* What the Benchmarks Game program writes for n, into out (64 bytes); returns its length. */
static size_t text(int64_t n, char *out) {
  int checksum;
  int max_flips = fannkuch((int)n, &checksum);
  return (size_t)snprintf(out, 64, "%d\nPfannkuchen(%d) = %d\n", checksum, (int)n, max_flips);
}

/* h = (31·h + byte) mod 1,000,000,007 over every byte. */
static int64_t digest(const char *s, size_t len) {
  int64_t h = 0;
  for (size_t i = 0; i < len; i++) h = (h * 31 + (unsigned char)s[i]) % 1000000007;
  return h;
}

static int64_t run(int64_t n) {
  char out[64];
  size_t len = text(n, out);
  return digest(out, len);
}

int main(int argc, char **argv) {
  if (argc == 3 && strcmp(argv[1], "output") == 0) {
    long long n = strtoll(argv[2], NULL, 10);
    if (n < 0 || n > MAXIMUM) return 2;
    char out[64];
    size_t len = text(n, out);
    fwrite(out, 1, len, stdout);
    return 0;
  }
  return bench_main(argc, argv, run, MAXIMUM);
}
