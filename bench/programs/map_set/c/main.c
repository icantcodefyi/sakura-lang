// Open-addressing hash maps (linear probing, tombstones) with real Int, String and (Int, Int)
// keys plus an Int set; the retained Int-map snapshot is a full copy. As in the Rust and Go
// ports, final sums look up the original key sequence. String keys are formatted on the stack.
#include "runner.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

enum { EMPTY, FULL, DELETED };

static void *checked(void *p) {
  if (!p) {
    fprintf(stderr, "out of memory\n");
    exit(1);
  }
  return p;
}

static uint64_t mix(uint64_t x) { /* splitmix64 finalizer */
  x ^= x >> 30;
  x *= 0xbf58476d1ce4e5b9u;
  x ^= x >> 27;
  x *= 0x94d049bb133111ebu;
  return x ^ (x >> 31);
}

typedef struct {
  int64_t a, b;
} Pair;
typedef struct {
  char *s;
  size_t len;
} Str;

static uint64_t hash_int(int64_t k) { return mix((uint64_t)k); }
static bool eq_int(int64_t x, int64_t y) { return x == y; }
static uint64_t hash_pair(Pair k) { return mix((uint64_t)k.a * 0x9e3779b97f4a7c15u ^ (uint64_t)k.b); }
static bool eq_pair(Pair x, Pair y) { return x.a == y.a && x.b == y.b; }
static uint64_t hash_str(Str k) { /* FNV-1a */
  uint64_t h = 0xcbf29ce484222325u;
  for (size_t i = 0; i < k.len; i++) h = (h ^ (unsigned char)k.s[i]) * 0x100000001b3u;
  return h;
}
static bool eq_str(Str x, Str y) { return x.len == y.len && memcmp(x.s, y.s, x.len) == 0; }

/* One map type per key type. Capacity is a power of two; `used` counts FULL and DELETED. */
#define DEFINE_MAP(Map, K, hash, eq)                                                            \
  typedef struct {                                                                              \
    K *keys;                                                                                    \
    int64_t *vals;                                                                              \
    unsigned char *state;                                                                       \
    size_t cap, len, used;                                                                      \
  } Map;                                                                                        \
                                                                                                \
  static void Map##_alloc(Map *m, size_t cap) {                                                 \
    m->keys = checked(malloc(cap * sizeof *m->keys));                                           \
    m->vals = checked(malloc(cap * sizeof *m->vals));                                           \
    m->state = checked(calloc(cap, 1));                                                         \
    m->cap = cap;                                                                               \
    m->len = m->used = 0;                                                                       \
  }                                                                                             \
                                                                                                \
  static void Map##_free(Map *m) {                                                              \
    free(m->keys);                                                                              \
    free(m->vals);                                                                              \
    free(m->state);                                                                             \
  }                                                                                             \
                                                                                                \
  /* The slot holding k, or else the first free slot on its probe path. */                      \
  static size_t Map##_probe(const Map *m, K k, bool *found) {                                   \
    size_t mask = m->cap - 1, i = (size_t)hash(k) & mask, tomb = SIZE_MAX;                      \
    for (;; i = (i + 1) & mask) {                                                               \
      if (m->state[i] == EMPTY) {                                                               \
        *found = false;                                                                         \
        return tomb != SIZE_MAX ? tomb : i;                                                     \
      }                                                                                         \
      if (m->state[i] == DELETED) {                                                             \
        if (tomb == SIZE_MAX) tomb = i;                                                         \
      } else if (eq(m->keys[i], k)) {                                                           \
        *found = true;                                                                          \
        return i;                                                                               \
      }                                                                                         \
    }                                                                                           \
  }                                                                                             \
                                                                                                \
  static int64_t Map##_get(const Map *m, K k) {                                                 \
    bool found;                                                                                 \
    size_t i = Map##_probe(m, k, &found);                                                       \
    return found ? m->vals[i] : 0;                                                              \
  }                                                                                             \
                                                                                                \
  static void Map##_rehash(Map *m) {                                                            \
    Map old = *m;                                                                               \
    size_t cap = 16;                                                                            \
    while (cap < 2 * (old.len + 1)) cap *= 2;                                                   \
    Map##_alloc(m, cap);                                                                        \
    for (size_t j = 0; j < old.cap; j++) {                                                      \
      if (old.state[j] != FULL) continue;                                                       \
      bool found;                                                                               \
      size_t i = Map##_probe(m, old.keys[j], &found);                                           \
      m->keys[i] = old.keys[j];                                                                 \
      m->vals[i] = old.vals[j];                                                                 \
      m->state[i] = FULL;                                                                       \
    }                                                                                           \
    m->len = m->used = old.len;                                                                 \
    Map##_free(&old);                                                                           \
  }                                                                                             \
                                                                                                \
  /* The value slot for k, inserted with 0 if absent (*inserted then says so). */               \
  static size_t Map##_entry(Map *m, K k, bool *inserted) {                                      \
    if (4 * (m->used + 1) > 3 * m->cap) Map##_rehash(m);                                        \
    bool found;                                                                                 \
    size_t i = Map##_probe(m, k, &found);                                                       \
    *inserted = !found;                                                                         \
    if (!found) {                                                                               \
      if (m->state[i] == EMPTY) m->used++;                                                      \
      m->state[i] = FULL;                                                                       \
      m->keys[i] = k;                                                                           \
      m->vals[i] = 0;                                                                           \
      m->len++;                                                                                 \
    }                                                                                           \
    return i;                                                                                   \
  }                                                                                             \
                                                                                                \
  /* Removes k; returns its slot (key still readable there) or SIZE_MAX if absent. */           \
  static size_t Map##_remove(Map *m, K k) {                                                     \
    bool found;                                                                                 \
    size_t i = Map##_probe(m, k, &found);                                                       \
    if (!found) return SIZE_MAX;                                                                \
    m->state[i] = DELETED;                                                                      \
    m->len--;                                                                                   \
    return i;                                                                                   \
  }

DEFINE_MAP(IntMap, int64_t, hash_int, eq_int)
DEFINE_MAP(PairMap, Pair, hash_pair, eq_pair)
DEFINE_MAP(StrMap, Str, hash_str, eq_str)

static void int_set(IntMap *m, int64_t k, int64_t v) {
  bool inserted;
  size_t i = IntMap_entry(m, k, &inserted); /* may rehash: index before vals */
  m->vals[i] = v;
}

static void pair_set(PairMap *m, Pair k, int64_t v) {
  bool inserted;
  size_t i = PairMap_entry(m, k, &inserted);
  m->vals[i] = v;
}

/* The map owns its keys: a new key is copied, a removed one freed. */
static int64_t *str_entry(StrMap *m, Str k) {
  bool inserted;
  size_t i = StrMap_entry(m, k, &inserted);
  if (inserted) {
    m->keys[i].s = checked(malloc(k.len));
    memcpy(m->keys[i].s, k.s, k.len);
  }
  return &m->vals[i];
}

static void str_remove(StrMap *m, Str k) {
  size_t i = StrMap_remove(m, k);
  if (i != SIZE_MAX) free(m->keys[i].s);
}

static void str_free(StrMap *m) {
  for (size_t i = 0; i < m->cap; i++)
    if (m->state[i] == FULL) free(m->keys[i].s);
  StrMap_free(m);
}

static IntMap int_copy(const IntMap *m) {
  IntMap c;
  IntMap_alloc(&c, m->cap);
  memcpy(c.keys, m->keys, m->cap * sizeof *m->keys);
  memcpy(c.vals, m->vals, m->cap * sizeof *m->vals);
  memcpy(c.state, m->state, m->cap);
  c.len = m->len;
  c.used = m->used;
  return c;
}

/* "k<i>" in buf. */
static Str key(char buf[32], int64_t i) {
  return (Str){buf, (size_t)snprintf(buf, 32, "k%lld", (long long)i)};
}

static int64_t run(int64_t n) {
  IntMap mi, set;
  StrMap ms;
  PairMap mt;
  IntMap_alloc(&mi, 16);
  IntMap_alloc(&set, 16);
  StrMap_alloc(&ms, 16);
  PairMap_alloc(&mt, 16);
  char buf[32];
  for (int64_t i = 0; i < n; i++) {
    int_set(&mi, i, i % 1000);
    *str_entry(&ms, key(buf, i)) = i % 1000;
    pair_set(&mt, (Pair){i % 100, i / 100}, i % 7);
    int_set(&set, i % 97, 0);
  }
  IntMap snapshot = int_copy(&mi);
  int64_t found = 0;
  for (int64_t i = 0; i < 2 * n; i++) {
    found += IntMap_get(&mi, i);
    found += StrMap_get(&ms, key(buf, i));
    found += PairMap_get(&mt, (Pair){i % 100, i / 100});
  }
  for (int64_t i = 0; i < n; i += 2) {
    bool inserted;
    size_t slot = IntMap_entry(&mi, i, &inserted);
    mi.vals[slot] += 7;
    *str_entry(&ms, key(buf, i)) += 7;
  }
  for (int64_t i = 0; i < n; i += 3) {
    IntMap_remove(&mi, i);
    str_remove(&ms, key(buf, i));
    PairMap_remove(&mt, (Pair){i % 100, i / 100});
  }
  int64_t total = 0, snap = 0;
  for (int64_t i = 0; i < n; i++) total += IntMap_get(&mi, i);
  for (int64_t i = 0; i < n; i++) total += StrMap_get(&ms, key(buf, i));
  for (int64_t i = 0; i < n; i++) total += PairMap_get(&mt, (Pair){i % 100, i / 100});
  for (int64_t i = 0; i < n; i++) snap += IntMap_get(&snapshot, i);
  int64_t sizes = (int64_t)(mi.len + ms.len + mt.len + snapshot.len + set.len);
  IntMap_free(&mi);
  IntMap_free(&set);
  IntMap_free(&snapshot);
  str_free(&ms);
  PairMap_free(&mt);
  return found + total + snap + sizes;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 100000); }
