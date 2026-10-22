// The text is appended into one growing buffer (amortized doubling), where the Rust and Go ports
// copy the whole string per word; the bytes are identical. Tokens are spans between spaces;
// scalars are counted by stepping over each UTF-8 sequence by its lead byte.
#include "runner.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char *const words[] = {"alpha", "naïve", "日本語", "🌸", "x1", "Grüße", "ok", "mañana"};

static size_t sequence_length(unsigned char lead) {
  return lead < 0x80 ? 1 : lead < 0xE0 ? 2 : lead < 0xF0 ? 3 : 4;
}

static int64_t run(int64_t n) {
  char *s = NULL;
  size_t len = 0, cap = 0;
  for (int64_t i = 0; i < n; i++) {
    const char *w = words[i % 8], *sep = i % 5 == 4 ? ", " : " ";
    size_t wl = strlen(w), sl = strlen(sep);
    if (len + wl + sl > cap) {
      cap = cap ? cap * 2 : 64;
      if (len + wl + sl > cap) cap = len + wl + sl;
      s = realloc(s, cap);
      if (!s) {
        fprintf(stderr, "out of memory\n");
        exit(1);
      }
    }
    memcpy(s + len, w, wl);
    memcpy(s + len + wl, sep, sl);
    len += wl + sl;
  }
  int64_t tokens = 0, bytes = 0, scalars = 0, commas = 0;
  for (size_t start = 0; start <= len;) {
    const char *space = len > start ? memchr(s + start, ' ', len - start) : NULL;
    size_t end = space ? (size_t)(space - s) : len;
    if (end > start) {
      tokens++;
      bytes += (int64_t)(end - start);
      for (size_t i = start; i < end; i += sequence_length((unsigned char)s[i])) scalars++;
      if (s[end - 1] == ',') commas++;
    }
    start = end + 1;
  }
  free(s);
  return tokens + 3 * bytes + 7 * scalars + 11 * commas;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 100000); }
