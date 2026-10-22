// Real JSON work, as the Rust port does with serde_json: a recursive-descent parser (all JSON
// value kinds, depth limit 128), the source's refinements, and re-encoding with JSON string
// escaping in id,name,age order. The lines are written straight into the joined text buffer.
#include "runner.h"

#include <math.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

enum { MAX_DEPTH = 128 };

typedef struct {
  char *data;
  size_t len, cap;
} Buf;

static void reserve(Buf *b, size_t extra) {
  if (b->len + extra <= b->cap) return;
  while (b->len + extra > b->cap) b->cap = b->cap ? b->cap * 2 : 64;
  b->data = realloc(b->data, b->cap);
  if (!b->data) {
    fprintf(stderr, "out of memory\n");
    exit(1);
  }
}

static void append(Buf *b, const char *s, size_t n) {
  reserve(b, n);
  memcpy(b->data + b->len, s, n);
  b->len += n;
}

static void appendf(Buf *b, const char *format, long long x) {
  char tmp[32];
  int n = snprintf(tmp, sizeof tmp, format, x);
  append(b, tmp, (size_t)n);
}

typedef enum { J_NULL, J_BOOL, J_NUM, J_STR, J_ARR, J_OBJ } Kind;
typedef struct Member Member;
typedef struct Value {
  Kind kind;
  bool boolean;
  double num;
  Buf str;              /* J_STR */
  struct Value *items;  /* J_ARR */
  Member *members;      /* J_OBJ */
  size_t count, cap;
} Value;
struct Member {
  Buf key;
  Value value;
};

static void value_free(Value *v) {
  free(v->str.data);
  for (size_t i = 0; i < v->count; i++) {
    if (v->kind == J_ARR) value_free(&v->items[i]);
    if (v->kind == J_OBJ) {
      free(v->members[i].key.data);
      value_free(&v->members[i].value);
    }
  }
  free(v->items);
  free(v->members);
}

typedef struct {
  const char *s, *end;
} Parser;

static void skip_ws(Parser *p) {
  while (p->s < p->end && (*p->s == ' ' || *p->s == '\t' || *p->s == '\n' || *p->s == '\r')) p->s++;
}

static bool literal(Parser *p, const char *word) {
  size_t n = strlen(word);
  if ((size_t)(p->end - p->s) < n || memcmp(p->s, word, n) != 0) return false;
  p->s += n;
  return true;
}

static void put_utf8(Buf *b, unsigned cp) {
  char u[4];
  size_t n;
  if (cp < 0x80) {
    u[0] = (char)cp;
    n = 1;
  } else if (cp < 0x800) {
    u[0] = (char)(0xC0 | cp >> 6);
    u[1] = (char)(0x80 | (cp & 0x3F));
    n = 2;
  } else if (cp < 0x10000) {
    u[0] = (char)(0xE0 | cp >> 12);
    u[1] = (char)(0x80 | (cp >> 6 & 0x3F));
    u[2] = (char)(0x80 | (cp & 0x3F));
    n = 3;
  } else {
    u[0] = (char)(0xF0 | cp >> 18);
    u[1] = (char)(0x80 | (cp >> 12 & 0x3F));
    u[2] = (char)(0x80 | (cp >> 6 & 0x3F));
    u[3] = (char)(0x80 | (cp & 0x3F));
    n = 4;
  }
  append(b, u, n);
}

static bool hex4(Parser *p, unsigned *out) {
  if (p->end - p->s < 4) return false;
  unsigned x = 0;
  for (int i = 0; i < 4; i++) {
    char c = *p->s++;
    unsigned d = c >= '0' && c <= '9'   ? (unsigned)(c - '0')
                 : c >= 'a' && c <= 'f' ? (unsigned)(c - 'a' + 10)
                 : c >= 'A' && c <= 'F' ? (unsigned)(c - 'A' + 10)
                                        : 16;
    if (d == 16) return false;
    x = x * 16 + d;
  }
  *out = x;
  return true;
}

/* At the opening quote. Decodes escapes into out; rejects raw control characters. */
static bool parse_string(Parser *p, Buf *out) {
  p->s++;
  for (;;) {
    if (p->s == p->end) return false;
    unsigned char c = (unsigned char)*p->s++;
    if (c == '"') return true;
    if (c < 0x20) return false;
    if (c != '\\') {
      append(out, (const char *)&c, 1);
      continue;
    }
    if (p->s == p->end) return false;
    char e = *p->s++;
    const char *simple = strchr("\"\\/bfnrt", e);
    if (e != '\0' && e != 'u' && simple) {
      append(out, &"\"\\/\b\f\n\r\t"[simple - "\"\\/bfnrt"], 1);
      continue;
    }
    unsigned cp;
    if (e != 'u' || !hex4(p, &cp)) return false;
    if (cp >= 0xD800 && cp < 0xDC00) {
      unsigned low;
      if (!literal(p, "\\u") || !hex4(p, &low) || low < 0xDC00 || low >= 0xE000) return false;
      cp = 0x10000 + ((cp - 0xD800) << 10) + (low - 0xDC00);
    } else if (cp >= 0xDC00 && cp < 0xE000) {
      return false;
    }
    put_utf8(out, cp);
  }
}

static bool digits(Parser *p) {
  const char *start = p->s;
  while (p->s < p->end && *p->s >= '0' && *p->s <= '9') p->s++;
  return p->s > start;
}

/* JSON number grammar checked here; strtod converts the validated span. */
static bool parse_number(Parser *p, double *out) {
  const char *start = p->s;
  if (*p->s == '-') p->s++;
  if (p->s < p->end && *p->s == '0')
    p->s++;
  else if (!digits(p))
    return false;
  if (p->s < p->end && *p->s == '.') {
    p->s++;
    if (!digits(p)) return false;
  }
  if (p->s < p->end && (*p->s == 'e' || *p->s == 'E')) {
    p->s++;
    if (p->s < p->end && (*p->s == '+' || *p->s == '-')) p->s++;
    if (!digits(p)) return false;
  }
  char tmp[64];
  size_t n = (size_t)(p->s - start);
  if (n >= sizeof tmp) {
    char *big = malloc(n + 1);
    if (!big) return false;
    memcpy(big, start, n);
    big[n] = '\0';
    *out = strtod(big, NULL);
    free(big);
  } else {
    memcpy(tmp, start, n);
    tmp[n] = '\0';
    *out = strtod(tmp, NULL);
  }
  return true;
}

static bool parse_value(Parser *p, Value *v, int depth) {
  memset(v, 0, sizeof *v);
  skip_ws(p);
  if (p->s == p->end) return false;
  switch (*p->s) {
  case '"':
    v->kind = J_STR;
    return parse_string(p, &v->str);
  case 't':
    v->kind = J_BOOL;
    v->boolean = true;
    return literal(p, "true");
  case 'f':
    v->kind = J_BOOL;
    return literal(p, "false");
  case 'n':
    v->kind = J_NULL;
    return literal(p, "null");
  case '[':
  case '{':
    break;
  default:
    v->kind = J_NUM;
    return parse_number(p, &v->num);
  }
  if (depth >= MAX_DEPTH) return false;
  bool object = *p->s++ == '{';
  v->kind = object ? J_OBJ : J_ARR;
  char close = object ? '}' : ']';
  skip_ws(p);
  if (p->s < p->end && *p->s == close) {
    p->s++;
    return true;
  }
  for (;;) {
    if (v->count == v->cap) {
      v->cap = v->cap ? v->cap * 2 : 4;
      void *grown = object ? realloc(v->members, v->cap * sizeof *v->members)
                           : realloc(v->items, v->cap * sizeof *v->items);
      if (!grown) return false;
      if (object) v->members = grown; else v->items = grown;
    }
    Value *slot;
    if (object) {
      Member *m = &v->members[v->count++];
      memset(m, 0, sizeof *m);
      skip_ws(p);
      if (p->s == p->end || *p->s != '"' || !parse_string(p, &m->key)) return false;
      skip_ws(p);
      if (p->s == p->end || *p->s++ != ':') return false;
      slot = &m->value;
    } else {
      slot = &v->items[v->count++];
    }
    if (!parse_value(p, slot, depth + 1)) return false;
    skip_ws(p);
    if (p->s == p->end) return false;
    char c = *p->s++;
    if (c == close) return true;
    if (c != ',') return false;
  }
}

/* The whole text must be one value. On failure v still owns whatever was parsed. */
static bool parse(const char *s, size_t n, Value *v) {
  Parser p = {s, s + n};
  if (!parse_value(&p, v, 0)) return false;
  skip_ws(&p);
  return p.s == p.end;
}

/* The last member wins for duplicate keys, as with serde_json and JSON.parse. */
static const Value *get(const Value *v, const char *key) {
  if (v->kind != J_OBJ) return NULL;
  size_t n = strlen(key);
  for (size_t i = v->count; i-- > 0;)
    if (v->members[i].key.len == n && memcmp(v->members[i].key.data, key, n) == 0)
      return &v->members[i].value;
  return NULL;
}

static bool safe_integer(const Value *v) {
  return v && v->kind == J_NUM && isfinite(v->num) && trunc(v->num) == v->num &&
         fabs(v->num) <= 9007199254740991.0;
}

static void encode_string(Buf *out, const Buf *s) {
  append(out, "\"", 1);
  for (size_t i = 0; i < s->len; i++) {
    unsigned char c = (unsigned char)s->data[i];
    const char *esc = c == '"' ? "\\\"" : c == '\\' ? "\\\\" : c == '\b' ? "\\b" : c == '\f' ? "\\f"
                    : c == '\n' ? "\\n" : c == '\r' ? "\\r" : c == '\t' ? "\\t" : NULL;
    if (esc)
      append(out, esc, 2);
    else if (c < 0x20)
      appendf(out, "\\u%04llx", c);
    else
      append(out, (const char *)&c, 1);
  }
  append(out, "\"", 1);
}

static int64_t run(int64_t n) {
  Buf text = {0}, encoded = {0};
  for (int64_t i = 0; i < n; i++) {
    if (i > 0) append(&text, "\n", 1);
    appendf(&text, "{\"id\": %lld, \"name\": \"", i);
    if (i % 17 != 0) appendf(&text, "user%lld", i);
    appendf(&text, "\", \"age\": %lld}", i % 13 == 0 ? 200 : i % 90);
  }
  int64_t valid = 0, bytes = 0, ids = 0, invalid = 0;
  for (size_t start = 0; start < text.len;) {
    const char *nl = memchr(text.data + start, '\n', text.len - start);
    size_t end = nl ? (size_t)(nl - text.data) : text.len;
    if (end > start) {
      Value v;
      bool ok = parse(text.data + start, end - start, &v);
      const Value *id = ok ? get(&v, "id") : NULL, *name = ok ? get(&v, "name") : NULL,
                  *age = ok ? get(&v, "age") : NULL;
      if (safe_integer(id) && name && name->kind == J_STR && name->str.len > 0 &&
          name->str.len <= 32 && safe_integer(age) && age->num >= 0 && age->num <= 150) {
        valid++;
        encoded.len = 0;
        appendf(&encoded, "{\"id\":%lld,\"name\":", (long long)id->num);
        encode_string(&encoded, &name->str);
        appendf(&encoded, ",\"age\":%lld}", (long long)age->num);
        bytes += (int64_t)encoded.len;
        ids += (int64_t)id->num;
      } else {
        invalid++;
      }
      value_free(&v);
    }
    start = end + 1;
  }
  free(text.data);
  free(encoded.data);
  return valid + bytes + ids + 1000 * invalid;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 30000); }
