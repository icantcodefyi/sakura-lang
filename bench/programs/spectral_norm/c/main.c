/* Three buffers reused across the ten iterations (Sakura builds fresh lists); the float
   operation order is the source's, and FP_CONTRACT OFF prevents fused multiply-adds. */
#pragma STDC FP_CONTRACT OFF
#include <math.h>
#include <stdlib.h>

#include "runner.h"

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

static int64_t run(int64_t n) {
  if (n == 0) return 0;
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
  return llround(sqrt(vbv / vv) * 1.0e9);
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 400); }
