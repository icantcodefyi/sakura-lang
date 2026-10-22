/* Mandelbrot from Are We Fast Yet (benchmarks/JavaScript/mandelbrot.js; revised BSD licence,
   Copyright © 2004-2013 Brent Fulgham; Benchmarks Game program contributed by Karl von
   Laudermann, modified by Jeremy Echols, Detlef Reichl, Joseph LaFata and Peter Zotov).
   Same loop as the JS; the shifts and XOR are C's operators (Sakura multiplies and loops). */
#pragma STDC FP_CONTRACT OFF
#include "runner.h"

static int64_t mandelbrot(int64_t size) {
  int64_t sum = 0;
  int64_t byte_acc = 0;
  int64_t bit_num = 0;

  int64_t y = 0;

  while (y < size) {
    double ci = ((2.0 * (double)y) / (double)size) - 1.0;
    int64_t x = 0;

    while (x < size) {
      double zrzr = 0.0;
      double zi = 0.0;
      double zizi = 0.0;
      double cr = ((2.0 * (double)x) / (double)size) - 1.5;

      int64_t z = 0;
      int not_done = 1;
      int64_t escape = 0;
      while (not_done && z < 50) {
        double zr = zrzr - zizi + cr;
        zi = 2.0 * zr * zi + ci;

        /* preserve recalculation */
        zrzr = zr * zr;
        zizi = zi * zi;

        if (zrzr + zizi > 4.0) {
          not_done = 0;
          escape = 1;
        }
        z += 1;
      }

      byte_acc = (byte_acc << 1) + escape;
      bit_num += 1;

      if (bit_num == 8) {
        sum ^= byte_acc;
        byte_acc = 0;
        bit_num = 0;
      } else if (x == size - 1) {
        byte_acc <<= (8 - bit_num);
        sum ^= byte_acc;
        byte_acc = 0;
        bit_num = 0;
      }
      x += 1;
    }
    y += 1;
  }
  return sum;
}

int main(int argc, char **argv) { return bench_main(argc, argv, mandelbrot, 1000); }
