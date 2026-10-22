/* List from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/list.js;
   Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's C++
   port: heap elements whose `next` is NULL at the end, the three lists are freed after each
   benchmark() (tail() returns a suffix of one of them). run(n) sums n benchmark() results. */
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

#include "runner.h"

typedef struct Element {
  int64_t val;
  struct Element *next;
} Element;

static int64_t element_length(const Element *e) {
  if (e->next == NULL) return 1;
  return 1 + element_length(e->next);
}

static void element_free(Element *e) {
  while (e != NULL) {
    Element *next = e->next;
    free(e);
    e = next;
  }
}

static Element *make_list(int64_t length) {
  if (length == 0) return NULL;
  Element *e = malloc(sizeof *e);
  if (!e) {
    fprintf(stderr, "out of memory\n");
    exit(1);
  }
  e->val = length;
  e->next = make_list(length - 1);
  return e;
}

static bool is_shorter_than(const Element *x, const Element *y) {
  const Element *x_tail = x;
  const Element *y_tail = y;

  while (y_tail != NULL) {
    if (x_tail == NULL) return true;
    x_tail = x_tail->next;
    y_tail = y_tail->next;
  }
  return false;
}

static Element *tail(Element *x, Element *y, Element *z) {
  if (is_shorter_than(y, x)) {
    return tail(tail(x->next, y, z), tail(y->next, z, x), tail(z->next, x, y));
  }
  return z;
}

static int64_t benchmark(void) {
  Element *x = make_list(15);
  Element *y = make_list(10);
  Element *z = make_list(6);

  Element *result = tail(x, y, z);
  const int64_t length = element_length(result);

  element_free(x);
  element_free(y);
  element_free(z);
  return length;
}

static int64_t run(int64_t n) {
  int64_t sum = 0;
  for (int64_t i = 0; i < n; i += 1) sum += benchmark();
  return sum;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 6000); }
