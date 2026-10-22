/* k-nucleotide, the Benchmarks Game's task: from FASTA input read line by line, extract
   sequence THREE (upper-cased), write the 1- and 2-nucleotide frequencies (%.3f, by descending
   frequency then ascending key) and the counts of GGT, GGTA, GGTATT, GGTATTTTAATT and
   GGTATTTTAATTTATAGT. `update` counts one reading frame into a klib khash table (khash.h, MIT,
   vendored beside this file; the hash table the Benchmarks Game's C programs use), grown from
   empty; `count` combines all k frames. The input is `fasta n`'s output, generated in memory by
   bg_fasta's code (copied below) as part of the timed work. Keys are the nucleotides as codes
   0..3 packed base 4, as in the Sakura program. */
#pragma STDC FP_CONTRACT OFF
#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "khash.h"
#include "runner.h"

KHASH_MAP_INIT_INT64(kmer, int64_t)

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

struct seq { unsigned char *codes; size_t len; };

/* A nucleotide's code 0..3: A, C, G, T. */
static unsigned char code(char c) { return c == 'A' ? 0 : c == 'C' ? 1 : c == 'G' ? 2 : 3; }

/* Sequence THREE, read line by line from the FASTA text, upper-cased, as codes 0..3. */
static struct seq sequence_three(const char *input, size_t size) {
  struct seq s = {malloc(64), 0};
  size_t cap = 64;
  int inside = 0;
  for (size_t at = 0; at < size;) {
    const char *end = memchr(input + at, '\n', size - at);
    size_t line = end ? (size_t)(end - (input + at)) : size - at;
    const char *l = input + at;
    if (line > 0 && l[0] == '>') {
      inside = line >= 6 && memcmp(l, ">THREE", 6) == 0;
    } else if (inside) {
      if (s.len + line > cap) {
        while (s.len + line > cap) cap *= 2;
        s.codes = realloc(s.codes, cap);
      }
      for (size_t i = 0; i < line; i++) s.codes[s.len++] = code((char)toupper((unsigned char)l[i]));
    }
    at += line + 1;
  }
  return s;
}

/* Count the k-nucleotides starting at frame, frame + k, frame + 2k, ... into the table. */
static void update(khash_t(kmer) *table, const struct seq *s, size_t k, size_t frame) {
  for (size_t i = frame; i + k <= s->len; i += k) {
    int64_t key = 0;
    for (size_t j = 0; j < k; j++) key = key * 4 + s->codes[i + j];
    int absent;
    khiter_t it = kh_put(kmer, table, key, &absent);
    if (absent) kh_value(table, it) = 0;
    kh_value(table, it) += 1;
  }
}

/* Every k-nucleotide's count, all k reading frames combined. */
static khash_t(kmer) *count(const struct seq *s, size_t k) {
  khash_t(kmer) *table = kh_init(kmer);
  for (size_t frame = 0; frame < k; frame++) update(table, s, k, frame);
  return table;
}

struct entry { int64_t key, count; };

static int by_frequency(const void *a, const void *b) {
  const struct entry *x = a, *y = b;
  if (x->count != y->count) return x->count > y->count ? -1 : 1;
  return (x->key > y->key) - (x->key < y->key);
}

struct text { char buf[4096]; size_t len; };

static void letters(char *out, int64_t key, size_t k) {
  for (size_t j = k; j > 0; j--) {
    out[j - 1] = "ACGT"[key % 4];
    key /= 4;
  }
  out[k] = '\0';
}

/* The k-nucleotides with their percentage frequencies, by descending frequency then key. */
static void frequencies(struct text *t, const struct seq *s, size_t k) {
  khash_t(kmer) *table = count(s, k);
  struct entry entries[16];
  size_t n = 0;
  int64_t total = 0;
  for (khiter_t it = kh_begin(table); it != kh_end(table); ++it) {
    if (!kh_exist(table, it)) continue;
    entries[n++] = (struct entry){kh_key(table, it), kh_value(table, it)};
    total += kh_value(table, it);
  }
  kh_destroy(kmer, table);
  qsort(entries, n, sizeof entries[0], by_frequency);
  for (size_t i = 0; i < n; i++) {
    char name[3];
    letters(name, entries[i].key, k);
    t->len += (size_t)snprintf(t->buf + t->len, sizeof t->buf - t->len, "%s %.3f\n", name,
                               100.0 * (double)entries[i].count / (double)total);
  }
  t->len += (size_t)snprintf(t->buf + t->len, sizeof t->buf - t->len, "\n");
}

/* What the Benchmarks Game program writes for `fasta n` on its input. */
static void knucleotide(struct text *t, int64_t n) {
  size_t size;
  char *input = fasta(n, &size);
  struct seq s = sequence_three(input, size);
  free(input);
  t->len = 0;
  frequencies(t, &s, 1);
  frequencies(t, &s, 2);
  static const char *const wanted[] = {"GGT", "GGTA", "GGTATT", "GGTATTTTAATT", "GGTATTTTAATTTATAGT"};
  for (size_t w = 0; w < 5; w++) {
    size_t k = strlen(wanted[w]);
    int64_t key = 0;
    for (size_t i = 0; i < k; i++) key = key * 4 + code(wanted[w][i]);
    khash_t(kmer) *table = count(&s, k);
    khiter_t it = kh_get(kmer, table, key);
    int64_t found = it == kh_end(table) ? 0 : kh_value(table, it);
    kh_destroy(kmer, table);
    t->len += (size_t)snprintf(t->buf + t->len, sizeof t->buf - t->len, "%lld\t%s\n", (long long)found,
                               wanted[w]);
  }
  free(s.codes);
}

static int64_t run(int64_t n) {
  struct text t;
  knucleotide(&t, n);
  int64_t h = 0;
  for (size_t i = 0; i < t.len; i++) h = (h * 31 + (unsigned char)t.buf[i]) % 1000000007;
  return h;
}

int main(int argc, char **argv) {
  if (argc == 3 && strcmp(argv[1], "output") == 0) {
    struct text t;
    knucleotide(&t, atoll(argv[2]));
    fwrite(t.buf, 1, t.len, stdout);
    return 0;
  }
  return bench_main(argc, argv, run, 25000000);
}
