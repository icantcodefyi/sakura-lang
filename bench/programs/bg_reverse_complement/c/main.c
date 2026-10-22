/* reverse-complement, the Benchmarks Game's task: read FASTA line by line and, for each
   sequence, write its header line and its reverse complement in 60-character lines (the IUB
   complement table; lower-case input gets the upper-case complement). The input is `fasta n`'s
   output, generated in memory by bg_fasta's code (copied below) as part of the timed work. A
   sequence's lines are gathered into a buffer grown by doubling; the whole output is built in
   one buffer, then digested (run) or written raw (`main output n`). */
#pragma STDC FP_CONTRACT OFF
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "runner.h"

static const char alu[] =
    "GGCCGGGCGCGGTGGCTCACGCCTGTAATCCCAGCACTTTGG"
    "GAGGCCGAGGCGGGCGGATCACCTGAGGTCAGGAGTTCGAGA"
    "CCAGCCTGGCCAACATGGTGAAACCCCGTCTCTACTAAAAAT"
    "ACAAAAATTAGCCGGGCGTGGTGGCGCGCGCCTGTAATCCCA"
    "GCTACTCGGGAGGCTGAGGCAGGAGAATCGCTTGAACCCGGG"
    "AGGCGGAGGTTGCAGTGAGCCGAGATCGCGCCACTGCACTCC"
    "AGCCTGGGCGACAGAGCGAGACTCCGTCTCAAAAA";

struct acid { char c; double p; };

static struct acid iub[] = {
    {'a', 0.27}, {'c', 0.12}, {'g', 0.12}, {'t', 0.27}, {'B', 0.02}, {'D', 0.02},
    {'H', 0.02}, {'K', 0.02}, {'M', 0.02}, {'N', 0.02}, {'R', 0.02}, {'S', 0.02},
    {'V', 0.02}, {'W', 0.02}, {'Y', 0.02},
};

static struct acid homo_sapiens[] = {
    {'a', 0.3029549426680}, {'c', 0.1979883004921}, {'g', 0.1975473066391}, {'t', 0.3015094502008},
};

enum { WIDTH = 60, IM = 139968, IA = 3877, IC = 29573 };

struct writer { char *buf; size_t pos; };

static void text(struct writer *w, const char *s) {
  size_t len = strlen(s);
  memcpy(w->buf + w->pos, s, len);
  w->pos += len;
}

/* Cumulative probabilities, summed in table order. */
static void cumulative(const struct acid *table, struct acid *out, int n) {
  double p = 0.0;
  for (int i = 0; i < n; i++) {
    p = p + table[i].p;
    out[i] = (struct acid){table[i].c, p};
  }
}

static void repeat_fasta(struct writer *w, const char *header, const char *s, int64_t count) {
  text(w, header);
  size_t len = strlen(s);
  char *doubled = malloc(2 * len);
  memcpy(doubled, s, len);
  memcpy(doubled + len, s, len);
  size_t pos = 0;
  while (count > 0) {
    size_t line = count < WIDTH ? (size_t)count : WIDTH;
    memcpy(w->buf + w->pos, doubled + pos, line);
    w->pos += line;
    w->buf[w->pos++] = '\n';
    pos = pos + line;
    if (pos >= len) pos = pos - len;
    count = count - (int64_t)line;
  }
  free(doubled);
}

static int64_t random_fasta(struct writer *w, const char *header, const struct acid *table, int n,
                            int64_t count, int64_t seed) {
  text(w, header);
  struct acid cum[16];
  cumulative(table, cum, n);
  while (count > 0) {
    int line = count < WIDTH ? (int)count : WIDTH;
    for (int i = 0; i < line; i++) {
      seed = (seed * IA + IC) % IM;
      double r = (double)seed / (double)IM;
      char c = cum[n - 1].c;
      for (int j = 0; j < n; j++) {
        if (r < cum[j].p) {
          c = cum[j].c;
          break;
        }
      }
      w->buf[w->pos++] = c;
    }
    w->buf[w->pos++] = '\n';
    count = count - line;
  }
  return seed;
}

static const char *headers[] = {">ONE Homo sapiens alu\n", ">TWO IUB ambiguity codes\n",
                                ">THREE Homo sapiens frequency\n"};

/* What the Benchmarks Game fasta program writes for n: this program's input. */
static char *fasta(int64_t n, size_t *len) {
  size_t size = strlen(headers[0]) + strlen(headers[1]) + strlen(headers[2]) + 10 * (size_t)n +
                (size_t)(2 * n + WIDTH - 1) / WIDTH + (size_t)(3 * n + WIDTH - 1) / WIDTH +
                (size_t)(5 * n + WIDTH - 1) / WIDTH;
  struct writer w = {malloc(size), 0};
  if (!w.buf) {
    fprintf(stderr, "out of memory\n");
    exit(1);
  }
  repeat_fasta(&w, headers[0], alu, 2 * n);
  int64_t seed = random_fasta(&w, headers[1], iub, sizeof iub / sizeof iub[0], 3 * n, 42);
  random_fasta(&w, headers[2], homo_sapiens, sizeof homo_sapiens / sizeof homo_sapiens[0], 5 * n, seed);
  *len = w.pos;
  return w.buf;
}

struct bytes { unsigned char *buf; size_t len, cap; };

static void reserve(struct bytes *b, size_t extra) {
  if (b->len + extra <= b->cap) return;
  size_t cap = b->cap ? b->cap : 64;
  while (b->len + extra > cap) cap *= 2;
  b->buf = realloc(b->buf, cap);
  if (!b->buf) {
    fprintf(stderr, "out of memory\n");
    exit(1);
  }
  b->cap = cap;
}

static void append(struct bytes *b, const void *data, size_t len) {
  reserve(b, len);
  memcpy(b->buf + b->len, data, len);
  b->len += len;
}

/* Each byte's complement: the IUB codes and their lower-case forms map to the upper-case
   complement, every other byte to itself. */
static unsigned char complement[256];

static void complements(void) {
  const char *from = "ACGTUMRWSYKVHDBN", *to = "TGCAAKYWSRMBDHVN";
  for (int i = 0; i < 256; i++) complement[i] = (unsigned char)i;
  for (size_t i = 0; from[i]; i++) {
    complement[(unsigned char)from[i]] = (unsigned char)to[i];
    complement[(unsigned char)from[i] + 32] = (unsigned char)to[i];
  }
}

/* The reverse complement of a sequence in 60-character lines. */
static void reverse_complement(struct bytes *out, const struct bytes *seq) {
  reserve(out, seq->len + (seq->len + WIDTH - 1) / WIDTH);
  size_t i = seq->len;
  while (i > 0) {
    size_t line = i < WIDTH ? i : WIDTH;
    for (size_t j = 0; j < line; j++) out->buf[out->len++] = complement[seq->buf[--i]];
    out->buf[out->len++] = '\n';
  }
}

/* What the Benchmarks Game program writes for `fasta n` on its input. */
static struct bytes revcomp(int64_t n) {
  complements();
  size_t size;
  char *input = fasta(n, &size);
  struct bytes out = {0}, seq = {0};
  for (size_t at = 0; at < size;) {
    const char *end = memchr(input + at, '\n', size - at);
    size_t line = end ? (size_t)(end - (input + at)) : size - at;
    if (line > 0 && input[at] == '>') {
      reverse_complement(&out, &seq);
      append(&out, input + at, line);
      append(&out, "\n", 1);
      seq.len = 0;
    } else {
      append(&seq, input + at, line);
    }
    at += line + 1;
  }
  reverse_complement(&out, &seq);
  free(seq.buf);
  free(input);
  return out;
}

static int64_t run(int64_t n) {
  struct bytes out = revcomp(n);
  int64_t h = 0;
  for (size_t i = 0; i < out.len; i++) h = (h * 31 + out.buf[i]) % 1000000007;
  free(out.buf);
  return h;
}

int main(int argc, char **argv) {
  if (argc == 3 && strcmp(argv[1], "output") == 0) {
    struct bytes out = revcomp(atoll(argv[2]));
    fwrite(out.buf, 1, out.len, stdout);
    free(out.buf);
    return 0;
  }
  return bench_main(argc, argv, run, 25000000);
}
