/* Havlak loop recognition from Are We Fast Yet (smarr/are-we-fast-yet@74306fec
   benchmarks/JavaScript/havlak.js and C++/src/havlak.h; Apache-2.0, Copyright 2011 Google Inc.;
   the som collections MIT, Copyright (c) 2015-2016 Stefan Marr; see ../LICENSE). run(n) is
   LoopTesterApp.main(n, 50, 10, 10, 5) as loops * 1000000 + nodes.
   Objects are malloc'd structs linked by pointers, as in AWFY's C++ port, and freed by their
   owners (the CFG its blocks and edges, the loop structure its loops, the finder its own state).
   som's Vector is a doubling array with a first index (one for pointers, one for ints), Set and
   IdentitySet wrap one, IdentityDictionary is som's chained hash table keyed by the block pointer
   with the block's customHash (its name). C has no closures: forEach and hasSome are loops. */
#include "runner.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

enum { INITIAL_SIZE = 10, INITIAL_CAPACITY = 16 };
static const int64_t UNVISITED = 2147483647;            /* Marker for uninitialized nodes. */
static const int64_t MAXNONBACKPREDS = 32 * 1024; /* Safeguard against pathological behavior. */

static void *xmalloc(size_t size) {
  void *p = malloc(size);
  if (!p) {
    fprintf(stderr, "out of memory\n");
    exit(1);
  }
  return p;
}

static void *xcalloc(size_t count, size_t size) {
  void *p = calloc(count, size);
  if (!p) {
    fprintf(stderr, "out of memory\n");
    exit(1);
  }
  return p;
}

/* ---- som: Vector (of pointers, and of ints) */

typedef struct {
  void **storage;
  int64_t capacity, first, last;
} Vector;

static Vector vector_new(void) { return (Vector){NULL, 0, 0, 0}; }

static void *vector_at(const Vector *v, int64_t idx) {
  if (v->storage == NULL || idx >= v->last - v->first) return NULL;
  return v->storage[v->first + idx];
}

static void vector_grow(Vector *v, int64_t capacity) {
  void **storage = xcalloc((size_t)capacity, sizeof *storage);
  for (int64_t i = 0; i < v->last; i++) storage[i] = v->storage[i];
  free(v->storage);
  v->storage = storage;
  v->capacity = capacity;
}

static void vector_at_put(Vector *v, int64_t idx, void *val) {
  if (v->storage == NULL) {
    v->capacity = idx + 1 > INITIAL_SIZE ? idx + 1 : INITIAL_SIZE;
    v->storage = xcalloc((size_t)v->capacity, sizeof *v->storage);
  } else if (idx >= v->last - v->first) {
    int64_t length = v->capacity;
    while (length <= idx + v->first) length *= 2;
    if (length != v->capacity) vector_grow(v, length);
  }
  v->storage[v->first + idx] = val;
  if (v->last < v->first + idx + 1) v->last = v->first + idx + 1;
}

static void vector_append(Vector *v, void *elem) {
  if (v->storage == NULL) {
    v->storage = xcalloc(INITIAL_SIZE, sizeof *v->storage);
    v->capacity = INITIAL_SIZE;
  } else if (v->last >= v->capacity) {
    vector_grow(v, v->capacity * 2);
  }
  v->storage[v->last++] = elem;
}

static bool vector_is_empty(const Vector *v) { return v->last == v->first; }
static int64_t vector_size(const Vector *v) { return v->last - v->first; }

static void *vector_remove_first(Vector *v) {
  if (vector_is_empty(v)) return NULL;
  v->first++;
  return v->storage[v->first - 1];
}

static void vector_remove_all(Vector *v) {
  v->first = 0;
  v->last = 0;
  if (v->storage != NULL) {
    free(v->storage);
    v->storage = xcalloc((size_t)v->capacity, sizeof *v->storage);
  }
}

static void vector_free(Vector *v) { free(v->storage); }

/* som's hasSome with the closure `e == obj`. */
static bool vector_has(const Vector *v, const void *obj) {
  for (int64_t i = v->first; i < v->last; i++)
    if (v->storage[i] == obj) return true;
  return false;
}

typedef struct {
  int64_t *storage;
  int64_t capacity, first, last;
} IntVector;

static IntVector *int_vector_new(void) {
  IntVector *v = xmalloc(sizeof *v);
  *v = (IntVector){NULL, 0, 0, 0};
  return v;
}

static void int_vector_append(IntVector *v, int64_t elem) {
  if (v->storage == NULL) {
    v->storage = xcalloc(INITIAL_SIZE, sizeof *v->storage);
    v->capacity = INITIAL_SIZE;
  } else if (v->last >= v->capacity) {
    int64_t *storage = xcalloc((size_t)v->capacity * 2, sizeof *storage);
    for (int64_t i = 0; i < v->last; i++) storage[i] = v->storage[i];
    free(v->storage);
    v->storage = storage;
    v->capacity *= 2;
  }
  v->storage[v->last++] = elem;
}

static int64_t int_vector_size(const IntVector *v) { return v->last - v->first; }

static void int_vector_free(IntVector *v) {
  free(v->storage);
  free(v);
}

/* ---- som: Set (of ints) and IdentitySet (of pointers) */

typedef struct {
  IntVector *items;
} Set;

static Set *set_new(void) {
  Set *s = xmalloc(sizeof *s);
  s->items = int_vector_new();
  return s;
}

static bool set_contains(const Set *s, int64_t obj) {
  for (int64_t i = s->items->first; i < s->items->last; i++)
    if (s->items->storage[i] == obj) return true;
  return false;
}

static void set_add(Set *s, int64_t obj) {
  if (!set_contains(s, obj)) int_vector_append(s->items, obj);
}

static void set_free(Set *s) {
  int_vector_free(s->items);
  free(s);
}

typedef struct {
  Vector items;
} IdentitySet;

static bool identity_set_contains(const IdentitySet *s, const void *obj) {
  return vector_has(&s->items, obj);
}

static void identity_set_add(IdentitySet *s, void *obj) {
  if (!identity_set_contains(s, obj)) vector_append(&s->items, obj);
}

/* ---- BasicBlock */

typedef struct BasicBlock {
  Vector in_edges, out_edges;
  int64_t name;
} BasicBlock;

static BasicBlock *basic_block_new(int64_t name) {
  BasicBlock *bb = xmalloc(sizeof *bb);
  bb->in_edges = vector_new();
  bb->out_edges = vector_new();
  bb->name = name;
  return bb;
}

static int64_t basic_block_num_pred(const BasicBlock *bb) { return vector_size(&bb->in_edges); }
static uint32_t basic_block_custom_hash(const BasicBlock *bb) { return (uint32_t)bb->name; }

/* ---- som: IdentityDictionary from a basic block to an int */

typedef struct DictEntry {
  uint32_t hash;
  const BasicBlock *key;
  int64_t value;
  struct DictEntry *next;
} DictEntry;

typedef struct {
  DictEntry **buckets;
  int64_t length, size;
} IdentityDictionary;

static uint32_t hash_fn(const BasicBlock *key) {
  if (key == NULL) return 0;
  uint32_t hash = basic_block_custom_hash(key);
  return hash ^ (hash >> 16);
}

static void dict_init(IdentityDictionary *d) {
  d->length = INITIAL_CAPACITY;
  d->buckets = xcalloc(INITIAL_CAPACITY, sizeof *d->buckets);
  d->size = 0;
}

static int64_t dict_bucket_idx(const IdentityDictionary *d, uint32_t hash) {
  return (int64_t)((uint32_t)(d->length - 1) & hash);
}

static const int64_t *dict_at(const IdentityDictionary *d, const BasicBlock *key) {
  uint32_t hash = hash_fn(key);
  for (DictEntry *e = d->buckets[dict_bucket_idx(d, hash)]; e; e = e->next)
    if (e->hash == hash && e->key == key) return &e->value;
  return NULL;
}

static DictEntry *dict_new_entry(const BasicBlock *key, int64_t value, uint32_t hash) {
  DictEntry *e = xmalloc(sizeof *e);
  *e = (DictEntry){hash, key, value, NULL};
  return e;
}

static void dict_insert_bucket_entry(IdentityDictionary *d, const BasicBlock *key, int64_t value,
                                     uint32_t hash, DictEntry *head) {
  DictEntry *current = head;
  while (true) {
    if (current->hash == hash && current->key == key) {
      current->value = value;
      return;
    }
    if (!current->next) {
      d->size++;
      current->next = dict_new_entry(key, value, hash);
      return;
    }
    current = current->next;
  }
}

static void dict_split_bucket(IdentityDictionary *d, int64_t old_length, int64_t i, DictEntry *head) {
  DictEntry *lo_head = NULL, *lo_tail = NULL, *hi_head = NULL, *hi_tail = NULL;
  for (DictEntry *current = head; current; current = current->next) {
    if ((current->hash & (uint32_t)old_length) == 0) {
      if (!lo_tail) lo_head = current;
      else lo_tail->next = current;
      lo_tail = current;
    } else {
      if (!hi_tail) hi_head = current;
      else hi_tail->next = current;
      hi_tail = current;
    }
  }
  if (lo_tail) {
    lo_tail->next = NULL;
    d->buckets[i] = lo_head;
  }
  if (hi_tail) {
    hi_tail->next = NULL;
    d->buckets[i + old_length] = hi_head;
  }
}

static void dict_transfer_entries(IdentityDictionary *d, DictEntry **old, int64_t old_length) {
  for (int64_t i = 0; i < old_length; i++) {
    DictEntry *current = old[i];
    if (current) {
      old[i] = NULL;
      if (!current->next) d->buckets[current->hash & (uint32_t)(d->length - 1)] = current;
      else dict_split_bucket(d, old_length, i, current);
    }
  }
}

static void dict_resize(IdentityDictionary *d) {
  DictEntry **old = d->buckets;
  int64_t old_length = d->length;
  d->length = old_length * 2;
  d->buckets = xcalloc((size_t)d->length, sizeof *d->buckets);
  dict_transfer_entries(d, old, old_length);
  free(old);
}

static void dict_at_put(IdentityDictionary *d, const BasicBlock *key, int64_t value) {
  uint32_t hash = hash_fn(key);
  int64_t i = dict_bucket_idx(d, hash);
  DictEntry *current = d->buckets[i];
  if (!current) {
    d->buckets[i] = dict_new_entry(key, value, hash);
    d->size++;
  } else {
    dict_insert_bucket_entry(d, key, value, hash, current);
  }
  if (d->size > d->length) dict_resize(d);
}

static void dict_free_entries(IdentityDictionary *d) {
  for (int64_t i = 0; i < d->length; i++) {
    DictEntry *e = d->buckets[i];
    while (e) {
      DictEntry *next = e->next;
      free(e);
      e = next;
    }
  }
}

static void dict_remove_all(IdentityDictionary *d) {
  dict_free_entries(d);
  free(d->buckets);
  d->buckets = xcalloc((size_t)d->length, sizeof *d->buckets);
  d->size = 0;
}

static void dict_free(IdentityDictionary *d) {
  dict_free_entries(d);
  free(d->buckets);
}

/* ---- BasicBlockEdge, ControlFlowGraph */

typedef struct {
  Vector basic_block_map;
  BasicBlock *start_node;
  Vector edge_list;
} ControlFlowGraph;

typedef struct {
  BasicBlock *from, *to;
} BasicBlockEdge;

static void cfg_init(ControlFlowGraph *cfg) {
  cfg->basic_block_map = vector_new();
  cfg->start_node = NULL;
  cfg->edge_list = vector_new();
}

static int64_t cfg_num_nodes(const ControlFlowGraph *cfg) {
  return vector_size(&cfg->basic_block_map);
}

static BasicBlock *cfg_create_node(ControlFlowGraph *cfg, int64_t name) {
  BasicBlock *node;
  if (vector_at(&cfg->basic_block_map, name) != NULL) {
    node = vector_at(&cfg->basic_block_map, name);
  } else {
    node = basic_block_new(name);
    vector_at_put(&cfg->basic_block_map, name, node);
  }
  if (cfg_num_nodes(cfg) == 1) cfg->start_node = node;
  return node;
}

static void basic_block_edge_new(ControlFlowGraph *cfg, int64_t from_name, int64_t to_name) {
  BasicBlockEdge *edge = xmalloc(sizeof *edge);
  edge->from = cfg_create_node(cfg, from_name);
  edge->to = cfg_create_node(cfg, to_name);
  vector_append(&edge->from->out_edges, edge->to);
  vector_append(&edge->to->in_edges, edge->from);
  vector_append(&cfg->edge_list, edge);
}

static void cfg_free(ControlFlowGraph *cfg) {
  for (int64_t i = cfg->basic_block_map.first; i < cfg->basic_block_map.last; i++) {
    BasicBlock *bb = cfg->basic_block_map.storage[i];
    if (bb) {
      vector_free(&bb->in_edges);
      vector_free(&bb->out_edges);
      free(bb);
    }
  }
  vector_free(&cfg->basic_block_map);
  for (int64_t i = cfg->edge_list.first; i < cfg->edge_list.last; i++) free(cfg->edge_list.storage[i]);
  vector_free(&cfg->edge_list);
}

/* ---- SimpleLoop, LoopStructureGraph */

typedef struct SimpleLoop {
  IdentitySet basic_blocks, children;
  struct SimpleLoop *parent;
  BasicBlock *header;
  bool is_reducible, is_root;
  int64_t nesting_level, counter, depth_level;
} SimpleLoop;

static SimpleLoop *simple_loop_new(BasicBlock *bb, bool is_reducible) {
  SimpleLoop *loop = xmalloc(sizeof *loop);
  *loop = (SimpleLoop){{vector_new()}, {vector_new()}, NULL, bb, is_reducible, false, 0, 0, 0};
  if (bb) identity_set_add(&loop->basic_blocks, bb);
  return loop;
}

static void simple_loop_set_parent(SimpleLoop *loop, SimpleLoop *parent) {
  loop->parent = parent;
  identity_set_add(&parent->children, loop);
}

static void simple_loop_set_nesting_level(SimpleLoop *loop, int64_t level) {
  loop->nesting_level = level;
  if (level == 0) loop->is_root = true;
}

typedef struct {
  SimpleLoop *root;
  Vector loops;
  int64_t loop_counter;
} LoopStructureGraph;

static void lsg_init(LoopStructureGraph *lsg) {
  lsg->loop_counter = 0;
  lsg->loops = vector_new();
  lsg->root = simple_loop_new(NULL, true);
  simple_loop_set_nesting_level(lsg->root, 0);
  lsg->root->counter = lsg->loop_counter;
  lsg->loop_counter++;
  vector_append(&lsg->loops, lsg->root);
}

static SimpleLoop *lsg_create_new_loop(LoopStructureGraph *lsg, BasicBlock *bb, bool is_reducible) {
  SimpleLoop *loop = simple_loop_new(bb, is_reducible);
  loop->counter = lsg->loop_counter;
  lsg->loop_counter++;
  vector_append(&lsg->loops, loop);
  return loop;
}

static void lsg_calculate_nesting_level_rec(SimpleLoop *loop, int64_t depth) {
  loop->depth_level = depth;
  for (int64_t i = loop->children.items.first; i < loop->children.items.last; i++) {
    SimpleLoop *liter = loop->children.items.storage[i];
    lsg_calculate_nesting_level_rec(liter, depth + 1);
    int64_t level = 1 + liter->nesting_level;
    simple_loop_set_nesting_level(loop, loop->nesting_level > level ? loop->nesting_level : level);
  }
}

static void lsg_calculate_nesting_level(LoopStructureGraph *lsg) {
  /* link up all 1st level loops to artificial root node. */
  for (int64_t i = lsg->loops.first; i < lsg->loops.last; i++) {
    SimpleLoop *liter = lsg->loops.storage[i];
    if (!liter->is_root && !liter->parent) simple_loop_set_parent(liter, lsg->root);
  }
  /* recursively traverse the tree and assign levels. */
  lsg_calculate_nesting_level_rec(lsg->root, 0);
}

static void lsg_free(LoopStructureGraph *lsg) {
  for (int64_t i = lsg->loops.first; i < lsg->loops.last; i++) {
    SimpleLoop *loop = lsg->loops.storage[i];
    vector_free(&loop->basic_blocks.items);
    vector_free(&loop->children.items);
    free(loop);
  }
  vector_free(&lsg->loops);
}

/* ---- UnionFindNode */

typedef struct UnionFindNode {
  struct UnionFindNode *parent;
  BasicBlock *bb;
  SimpleLoop *loop;
  int64_t dfs_number;
} UnionFindNode;

static void ufn_init_node(UnionFindNode *node, BasicBlock *bb, int64_t dfs_number) {
  node->parent = node;
  node->bb = bb;
  node->dfs_number = dfs_number;
  node->loop = NULL;
}

/* The find routine, with path compression: the nodes on the path whose parent is not a root
   are pointed at this node's parent. */
static UnionFindNode *ufn_find_set(UnionFindNode *self) {
  Vector node_list = vector_new();
  UnionFindNode *node = self;
  while (node != node->parent) {
    if (node->parent != node->parent->parent) vector_append(&node_list, node);
    node = node->parent;
  }
  for (int64_t i = node_list.first; i < node_list.last; i++) {
    UnionFindNode *iter = node_list.storage[i];
    iter->parent = self->parent;
  }
  vector_free(&node_list);
  return node;
}

/* ---- HavlakLoopFinder */

enum bb_class { BB_TOP, BB_NONHEADER, BB_REDUCIBLE, BB_SELF, BB_IRREDUCIBLE, BB_DEAD };

typedef struct {
  ControlFlowGraph *cfg;
  LoopStructureGraph *lsg;
  Vector non_back_preds; /* of Set* */
  Vector back_preds;     /* of IntVector* */
  IdentityDictionary number;
  int64_t max_size;
  int64_t *header;
  enum bb_class *type;
  int64_t *last;
  UnionFindNode **nodes;
} HavlakLoopFinder;

static void finder_init(HavlakLoopFinder *f, ControlFlowGraph *cfg, LoopStructureGraph *lsg) {
  f->non_back_preds = vector_new();
  f->back_preds = vector_new();
  dict_init(&f->number);
  f->max_size = 0;
  f->header = NULL;
  f->type = NULL;
  f->last = NULL;
  f->nodes = NULL;
  f->cfg = cfg;
  f->lsg = lsg;
}

static void finder_free_preds(HavlakLoopFinder *f) {
  for (int64_t i = f->non_back_preds.first; i < f->non_back_preds.last; i++)
    set_free(f->non_back_preds.storage[i]);
  for (int64_t i = f->back_preds.first; i < f->back_preds.last; i++)
    int_vector_free(f->back_preds.storage[i]);
}

static void finder_free(HavlakLoopFinder *f) {
  finder_free_preds(f);
  vector_free(&f->non_back_preds);
  vector_free(&f->back_preds);
  dict_free(&f->number);
  free(f->header);
  free(f->type);
  free(f->last);
  for (int64_t i = 0; i < f->max_size; i++) free(f->nodes[i]);
  free(f->nodes);
}

static bool is_ancestor(const HavlakLoopFinder *f, int64_t w, int64_t v) {
  return w <= v && v <= f->last[w];
}

static int64_t do_dfs(HavlakLoopFinder *f, BasicBlock *current_node, int64_t current) {
  ufn_init_node(f->nodes[current], current_node, current);
  dict_at_put(&f->number, current_node, current);
  int64_t last_id = current;
  const Vector *outer_blocks = &current_node->out_edges;
  for (int64_t i = 0; i < vector_size(outer_blocks); i++) {
    BasicBlock *target = vector_at(outer_blocks, i);
    if (*dict_at(&f->number, target) == UNVISITED) last_id = do_dfs(f, target, last_id + 1);
  }
  f->last[current] = last_id;
  return last_id;
}

static void init_all_nodes(HavlakLoopFinder *f) {
  const Vector *blocks = &f->cfg->basic_block_map;
  for (int64_t i = blocks->first; i < blocks->last; i++)
    dict_at_put(&f->number, blocks->storage[i], UNVISITED);
  do_dfs(f, f->cfg->start_node, 0);
}

static void process_edges(HavlakLoopFinder *f, BasicBlock *node_w, int64_t w) {
  if (basic_block_num_pred(node_w) > 0) {
    for (int64_t i = node_w->in_edges.first; i < node_w->in_edges.last; i++) {
      int64_t v = *dict_at(&f->number, node_w->in_edges.storage[i]);
      if (v != UNVISITED) {
        if (is_ancestor(f, w, v)) int_vector_append(f->back_preds.storage[w], v);
        else set_add(f->non_back_preds.storage[w], v);
      }
    }
  }
}

static void identify_edges(HavlakLoopFinder *f, int64_t size) {
  for (int64_t w = 0; w < size; w++) {
    f->header[w] = 0;
    f->type[w] = BB_NONHEADER;
    BasicBlock *node_w = f->nodes[w]->bb;
    if (!node_w) f->type[w] = BB_DEAD;
    else process_edges(f, node_w, w);
  }
}

static void step_d(HavlakLoopFinder *f, int64_t w, Vector *node_pool) {
  const IntVector *preds = vector_at(&f->back_preds, w);
  for (int64_t i = preds->first; i < preds->last; i++) {
    int64_t v = preds->storage[i];
    if (v != w) vector_append(node_pool, ufn_find_set(f->nodes[v]));
    else f->type[w] = BB_SELF;
  }
}

static void step_e_process_non_back_preds(HavlakLoopFinder *f, int64_t w, Vector *node_pool,
                                          Vector *work_list, UnionFindNode *x) {
  const IntVector *preds = ((Set *)vector_at(&f->non_back_preds, x->dfs_number))->items;
  for (int64_t i = preds->first; i < preds->last; i++) {
    UnionFindNode *y = f->nodes[preds->storage[i]];
    UnionFindNode *ydash = ufn_find_set(y);
    if (!is_ancestor(f, w, ydash->dfs_number)) {
      f->type[w] = BB_IRREDUCIBLE;
      set_add(vector_at(&f->non_back_preds, w), ydash->dfs_number);
    } else if (ydash->dfs_number != w) {
      if (!vector_has(node_pool, ydash)) {
        vector_append(work_list, ydash);
        vector_append(node_pool, ydash);
      }
    }
  }
}

static void set_loop_attributes(HavlakLoopFinder *f, int64_t w, const Vector *node_pool,
                                SimpleLoop *loop) {
  f->nodes[w]->loop = loop;
  for (int64_t i = node_pool->first; i < node_pool->last; i++) {
    UnionFindNode *node = node_pool->storage[i];
    /* Add nodes to loop descriptor. */
    f->header[node->dfs_number] = w;
    node->parent = f->nodes[w];
    /* Nested loops are not added, but linked together. */
    if (node->loop) simple_loop_set_parent(node->loop, loop);
    else identity_set_add(&loop->basic_blocks, node->bb);
  }
}

static void find_loops(HavlakLoopFinder *f) {
  if (!f->cfg->start_node) return;
  int64_t size = cfg_num_nodes(f->cfg);
  finder_free_preds(f);
  vector_remove_all(&f->non_back_preds);
  vector_remove_all(&f->back_preds);
  dict_remove_all(&f->number);
  if (size > f->max_size) {
    f->header = xcalloc((size_t)size, sizeof *f->header);
    f->type = xcalloc((size_t)size, sizeof *f->type);
    f->last = xcalloc((size_t)size, sizeof *f->last);
    f->nodes = xcalloc((size_t)size, sizeof *f->nodes);
    f->max_size = size;
  }
  for (int64_t i = 0; i < size; i++) {
    vector_append(&f->non_back_preds, set_new());
    vector_append(&f->back_preds, int_vector_new());
    f->nodes[i] = xcalloc(1, sizeof *f->nodes[i]);
  }
  init_all_nodes(f);
  identify_edges(f, size);
  /* Start node is root of all other loops. */
  f->header[0] = 0;

  /* Step c: reverse DFS preorder, so inner loop headers come before the loops around them. */
  for (int64_t w = size - 1; w >= 0; w--) {
    Vector node_pool = vector_new(); /* 'P' in Havlak's paper */
    BasicBlock *node_w = f->nodes[w]->bb;
    if (node_w) {
      step_d(f, w, &node_pool);
      /* Copy nodePool to workList. */
      Vector work_list = vector_new();
      for (int64_t i = node_pool.first; i < node_pool.last; i++)
        vector_append(&work_list, node_pool.storage[i]);
      if (vector_size(&node_pool) != 0) f->type[w] = BB_REDUCIBLE;
      /* work the list... */
      while (!vector_is_empty(&work_list)) {
        UnionFindNode *x = vector_remove_first(&work_list);
        /* Step e: the algorithm has degenerated past the safeguard: give up. */
        int64_t non_back_size =
            int_vector_size(((Set *)vector_at(&f->non_back_preds, x->dfs_number))->items);
        if (non_back_size > MAXNONBACKPREDS) {
          vector_free(&work_list);
          vector_free(&node_pool);
          return;
        }
        step_e_process_non_back_preds(f, w, &node_pool, &work_list, x);
      }
      vector_free(&work_list);
      /* Collapse the nodes of each SCC found into one, with a loop descriptor linked in. */
      if (vector_size(&node_pool) > 0 || f->type[w] == BB_SELF) {
        SimpleLoop *loop = lsg_create_new_loop(f->lsg, node_w, f->type[w] != BB_IRREDUCIBLE);
        set_loop_attributes(f, w, &node_pool, loop);
      }
    }
    vector_free(&node_pool);
  }
}

/* ---- LoopTesterApp */

typedef struct {
  ControlFlowGraph cfg;
  LoopStructureGraph lsg;
} LoopTesterApp;

static int64_t build_diamond(LoopTesterApp *app, int64_t start) {
  int64_t bb0 = start;
  basic_block_edge_new(&app->cfg, bb0, bb0 + 1);
  basic_block_edge_new(&app->cfg, bb0, bb0 + 2);
  basic_block_edge_new(&app->cfg, bb0 + 1, bb0 + 3);
  basic_block_edge_new(&app->cfg, bb0 + 2, bb0 + 3);
  return bb0 + 3;
}

static void build_connect(LoopTesterApp *app, int64_t start, int64_t end) {
  basic_block_edge_new(&app->cfg, start, end);
}

static int64_t build_straight(LoopTesterApp *app, int64_t start, int64_t n) {
  for (int64_t i = 0; i < n; i++) build_connect(app, start + i, start + i + 1);
  return start + n;
}

static int64_t build_base_loop(LoopTesterApp *app, int64_t from) {
  int64_t header = build_straight(app, from, 1);
  int64_t diamond1 = build_diamond(app, header);
  int64_t d11 = build_straight(app, diamond1, 1);
  int64_t diamond2 = build_diamond(app, d11);
  int64_t footer = build_straight(app, diamond2, 1);
  build_connect(app, diamond2, d11);
  build_connect(app, diamond1, header);
  build_connect(app, footer, from);
  footer = build_straight(app, footer, 1);
  return footer;
}

static void app_find_loops(LoopTesterApp *app, LoopStructureGraph *loop_structure) {
  HavlakLoopFinder finder;
  finder_init(&finder, &app->cfg, loop_structure);
  find_loops(&finder);
  finder_free(&finder);
}

static void construct_simple_cfg(LoopTesterApp *app) {
  cfg_create_node(&app->cfg, 0);
  build_base_loop(app, 0);
  cfg_create_node(&app->cfg, 1);
  basic_block_edge_new(&app->cfg, 0, 2);
}

static void add_dummy_loops(LoopTesterApp *app, int64_t num_dummy_loops) {
  for (int64_t dummyloop = 0; dummyloop < num_dummy_loops; dummyloop++)
    app_find_loops(app, &app->lsg);
}

static void construct_cfg(LoopTesterApp *app, int64_t par_loops, int64_t ppar_loops,
                          int64_t pppar_loops) {
  int64_t n = 2;
  for (int64_t parlooptrees = 0; parlooptrees < par_loops; parlooptrees++) {
    cfg_create_node(&app->cfg, n + 1);
    build_connect(app, 2, n + 1);
    n++;
    for (int64_t i = 0; i < ppar_loops; i++) {
      int64_t top = n;
      n = build_straight(app, n, 1);
      for (int64_t j = 0; j < pppar_loops; j++) n = build_base_loop(app, n);
      int64_t bottom = build_straight(app, n, 1);
      build_connect(app, n, top);
      n = bottom;
    }
    build_connect(app, n, 1);
  }
}

static void app_main(LoopTesterApp *app, int64_t num_dummy_loops, int64_t find_loop_iterations,
                     int64_t par_loops, int64_t ppar_loops, int64_t pppar_loops, int64_t result[2]) {
  construct_simple_cfg(app);
  add_dummy_loops(app, num_dummy_loops);
  construct_cfg(app, par_loops, ppar_loops, pppar_loops);
  /* Performing Loop Recognition, 1 Iteration, then findLoopIteration */
  app_find_loops(app, &app->lsg);
  for (int64_t i = 0; i < find_loop_iterations; i++) {
    LoopStructureGraph lsg;
    lsg_init(&lsg);
    app_find_loops(app, &lsg);
    lsg_free(&lsg);
  }
  lsg_calculate_nesting_level(&app->lsg);
  result[0] = vector_size(&app->lsg.loops);
  result[1] = cfg_num_nodes(&app->cfg);
}

static int64_t run(int64_t n) {
  LoopTesterApp app;
  cfg_init(&app.cfg);
  lsg_init(&app.lsg);
  cfg_create_node(&app.cfg, 0);
  int64_t result[2];
  app_main(&app, n, 50, 10, 10, 5, result);
  lsg_free(&app.lsg);
  cfg_free(&app.cfg);
  return result[0] * 1000000 + result[1];
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 15000); }
