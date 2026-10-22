/* spectral-norm, the Benchmarks Game's task: the Java program's four functions (A, Av, Atv,
   AtAv), ten power iterations, the norm as a `%.9f` line. Three buffers reused across the ten
   iterations, as the Java program's arrays are (Sakura builds fresh lists); the float operation
   order is the Sakura program's, and FP_CONTRACT OFF prevents fused multiply-adds. For n = 0 the
   norm is taken as 0, as in the Sakura program. `main output <n>` writes the program's output;
   run(n) is the digest of the same bytes (bench/README.md). */
#pragma STDC FP_CONTRACT OFF
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "runner.h"

#define MAXIMUM 5500

static double a(int64_t i, int64_t j) { return 1.0 / (double)((i + j) * (i + j + 1) / 2 + i + 1); }

static void mul_av(const double *v, double *out, int64_t n) {
  for (int64_t i = 0; i < n; i++) {
    double sum = 0.0;
    for (int64_t j = 0; j < n; j++) sum = sum + a(i, j) * v[j];
    out[i] = sum;
  }
}

static void mul_atv(const double *v, double *out, int64_t n) {
  for (int64_t i = 0; i < n; i++) {
    double sum = 0.0;
    for (int64_t j = 0; j < n; j++) sum = sum + a(j, i) * v[j];
    out[i] = sum;
  }
}

static void mul_atav(const double *v, double *out, double *tmp, int64_t n) {
  mul_av(v, tmp, n);
  mul_atv(tmp, out, n);
}

static double approximate(int64_t n) {
  if (n == 0) return 0.0;
  double *u = malloc((size_t)n * sizeof *u);
  double *v = malloc((size_t)n * sizeof *v);
  double *tmp = malloc((size_t)n * sizeof *tmp);
  for (int64_t k = 0; k < n; k++) u[k] = 1.0;
  for (int i = 0; i < 10; i++) {
    mul_atav(u, v, tmp, n);
    mul_atav(v, u, tmp, n);
  }
  double vbv = 0.0, vv = 0.0;
  for (int64_t m = 0; m < n; m++) {
    vbv = vbv + u[m] * v[m];
    vv = vv + v[m] * v[m];
  }
  free(u);
  free(v);
  free(tmp);
  return sqrt(vbv / vv);
}

/* What the Benchmarks Game program writes for n, into out (64 bytes); returns its length. */
static size_t text(int64_t n, char *out) { return (size_t)snprintf(out, 64, "%.9f\n", approximate(n)); }

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
