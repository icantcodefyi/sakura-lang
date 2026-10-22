/* mandelbrot, the Benchmarks Game's task: an n-by-n PBM P4 bitmap of [-1.5-i, 0.5+i], 50
   iterations, escape when |z|^2 > 4, after the reference C program contributed by Greg Buchholz.
   The whole output is built in one buffer, then digested (run) or written raw (`main output n`).
   FP_CONTRACT OFF keeps the compiler from fusing multiply-adds, so every port computes the same
   floats. */
#pragma STDC FP_CONTRACT OFF
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "runner.h"

static unsigned char *bitmap(int64_t n, size_t *len) {
  char header[64];
  int h = snprintf(header, sizeof header, "P4\n%lld %lld\n", (long long)n, (long long)n);
  size_t row_bytes = (size_t)(n + 7) / 8;
  unsigned char *out = malloc((size_t)h + row_bytes * (size_t)n);
  if (!out) {
    fprintf(stderr, "out of memory\n");
    exit(1);
  }
  memcpy(out, header, (size_t)h);
  size_t pos = (size_t)h;
  for (int64_t y = 0; y < n; y++) {
    double ci = 2.0 * (double)y / (double)n - 1.0;
    unsigned acc = 0;
    int bits = 0;
    for (int64_t x = 0; x < n; x++) {
      double cr = 2.0 * (double)x / (double)n - 1.5;
      double zr = 0.0, zi = 0.0, tr = 0.0, ti = 0.0;
      for (int i = 0; i < 50 && tr + ti <= 4.0; i++) {
        zi = 2.0 * zr * zi + ci;
        zr = tr - ti + cr;
        tr = zr * zr;
        ti = zi * zi;
      }
      acc = (acc << 1) | (tr + ti <= 4.0);
      if (++bits == 8) {
        out[pos++] = (unsigned char)acc;
        acc = 0;
        bits = 0;
      } else if (x == n - 1) {
        out[pos++] = (unsigned char)(acc << (8 - n % 8));
        acc = 0;
        bits = 0;
      }
    }
  }
  *len = pos;
  return out;
}

static int64_t run(int64_t n) {
  size_t len;
  unsigned char *out = bitmap(n, &len);
  int64_t h = 0;
  for (size_t i = 0; i < len; i++) h = (h * 31 + out[i]) % 1000000007;
  free(out);
  return h;
}

int main(int argc, char **argv) {
  if (argc == 3 && strcmp(argv[1], "output") == 0) {
    size_t len;
    unsigned char *out = bitmap(atoll(argv[2]), &len);
    fwrite(out, 1, len, stdout);
    free(out);
    return 0;
  }
  return bench_main(argc, argv, run, 16000);
}
