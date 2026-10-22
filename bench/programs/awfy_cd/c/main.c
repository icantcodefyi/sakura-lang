/* CD from Are We Fast Yet (benchmarks/JavaScript/cd.js and C++/src/cd.h at
   smarr/are-we-fast-yet@74306fec; Copyright (c) 2001-2010, Purdue University; Copyright (C) 2015
   Apple Inc.; see ../LICENSE). AWFY's RedBlackTree is a parent-pointer tree mutated in place, one
   node type for every map: the key is a union compared through the tree's compare function (the
   analogue of compareTo's dispatch), the value a union. SOM's Vector is a growable array; vectors
   are value structs. sin and cos are libm's. run(n) is cd(n - n % 2): AWFY's Simulator reads
   aircraft i + 1 for every even i, so it is defined for even n only. FP_CONTRACT OFF keeps the
   compiler from fusing multiply-adds. */
#pragma STDC FP_CONTRACT OFF
#include <math.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

#include "runner.h"

static void die(const char *message) {
  fprintf(stderr, "%s\n", message);
  exit(1);
}

static void *grow(void *items, size_t *cap, size_t size) {
  *cap = *cap ? *cap * 2 : 10;
  items = realloc(items, *cap * size);
  if (!items) die("out of memory");
  return items;
}

/* ---- vectors */

typedef struct { double x, y; } vec2;
typedef struct { double x, y, z; } vec3;

static vec2 plus2(vec2 a, vec2 b) { return (vec2){a.x + b.x, a.y + b.y}; }
static vec2 minus2(vec2 a, vec2 b) { return (vec2){a.x - b.x, a.y - b.y}; }

static int compare_numbers(double a, double b) {
  if (a == b) return 0;
  if (a < b) return -1;
  if (a > b) return 1;
  /* We say that NaN is smaller than non-NaN. */
  if (a == a) return 1;
  return -1;
}

static vec3 plus3(vec3 a, vec3 b) { return (vec3){a.x + b.x, a.y + b.y, a.z + b.z}; }
static vec3 minus3(vec3 a, vec3 b) { return (vec3){a.x - b.x, a.y - b.y, a.z - b.z}; }
static double dot(vec3 a, vec3 b) { return a.x * b.x + a.y * b.y + a.z * b.z; }
static double squared_magnitude(vec3 a) { return dot(a, a); }
static double magnitude(vec3 a) { return sqrt(squared_magnitude(a)); }
static vec3 times(vec3 a, double amount) { return (vec3){a.x * amount, a.y * amount, a.z * amount}; }

typedef struct {
  int64_t callsign;
  vec3 pos_one, pos_two;
} motion;

typedef struct {
  motion *items;
  size_t len, cap;
} motion_vec;

static void append_motion(motion_vec *v, motion m) {
  if (v->len == v->cap) v->items = grow(v->items, &v->cap, sizeof *v->items);
  v->items[v->len++] = m;
}

/* ---- red-black tree */

typedef union {
  vec2 voxel;
  int64_t callsign;
} key;

typedef union {
  motion_vec *motions;
  bool flag;
  vec3 position;
} value;

typedef struct node {
  key key;
  value value;
  struct node *left, *right, *parent;
  bool red;
} node;

typedef struct {
  node *root;
  int (*compare)(const key *a, const key *b);
} tree;

static int compare_voxels(const key *a, const key *b) {
  int result = compare_numbers(a->voxel.x, b->voxel.x);
  if (result) return result;
  return compare_numbers(a->voxel.y, b->voxel.y);
}

static int compare_call_signs(const key *a, const key *b) {
  return a->callsign == b->callsign ? 0 : a->callsign < b->callsign ? -1 : 1;
}

static node *tree_minimum(node *x) {
  node *current = x;
  while (current->left) current = current->left;
  return current;
}

static node *successor(node *x) {
  if (x->right) return tree_minimum(x->right);
  node *y = x->parent;
  while (y && x == y->right) {
    x = y;
    y = y->parent;
  }
  return y;
}

/* forEach: the first entry in key order, then successor(). */
static node *tree_first(tree *t) { return t->root ? tree_minimum(t->root) : NULL; }

static node *find_node(tree *t, const key *k) {
  for (node *current = t->root; current;) {
    int comparison = t->compare(k, &current->key);
    if (!comparison) return current;
    current = comparison < 0 ? current->left : current->right;
  }
  return NULL;
}

static value *tree_get(tree *t, const key *k) {
  node *n = find_node(t, k);
  return n ? &n->value : NULL;
}

static void left_rotate(tree *t, node *x) {
  node *y = x->right;
  /* Turn y's left subtree into x's right subtree. */
  x->right = y->left;
  if (y->left) y->left->parent = x;
  /* Link x's parent to y. */
  y->parent = x->parent;
  if (!x->parent) {
    t->root = y;
  } else if (x == x->parent->left) {
    x->parent->left = y;
  } else {
    x->parent->right = y;
  }
  /* Put x on y's left. */
  y->left = x;
  x->parent = y;
}

static void right_rotate(tree *t, node *y) {
  node *x = y->left;
  /* Turn x's right subtree into y's left subtree. */
  y->left = x->right;
  if (x->right) x->right->parent = y;
  /* Link y's parent to x. */
  x->parent = y->parent;
  if (!y->parent) {
    t->root = x;
  } else if (y == y->parent->left) {
    y->parent->left = x;
  } else {
    y->parent->right = x;
  }
  x->right = y;
  y->parent = x;
}

/* Returns the new node, or NULL after replacing an existing entry's value (old in *old). */
static node *tree_insert(tree *t, const key *k, value v, value *old) {
  node *y = NULL;
  node *x = t->root;
  while (x) {
    y = x;
    int comparison = t->compare(k, &x->key);
    if (comparison < 0) {
      x = x->left;
    } else if (comparison > 0) {
      x = x->right;
    } else {
      *old = x->value;
      x->value = v;
      return NULL;
    }
  }
  node *z = malloc(sizeof *z);
  if (!z) die("out of memory");
  *z = (node){.key = *k, .value = v, .parent = y, .red = true};
  if (!y) {
    t->root = z;
  } else if (t->compare(k, &y->key) < 0) {
    y->left = z;
  } else {
    y->right = z;
  }
  return z;
}

/* AWFY's put: true (and the replaced value in *old) when the key was present. */
static bool tree_put(tree *t, const key *k, value v, value *old) {
  node *x = tree_insert(t, k, v, old);
  if (!x) return true;
  while (x != t->root && x->parent->red) {
    if (x->parent == x->parent->parent->left) {
      node *y = x->parent->parent->right;
      if (y && y->red) {
        /* Case 1 */
        x->parent->red = false;
        y->red = false;
        x->parent->parent->red = true;
        x = x->parent->parent;
      } else {
        if (x == x->parent->right) {
          /* Case 2 */
          x = x->parent;
          left_rotate(t, x);
        }
        /* Case 3 */
        x->parent->red = false;
        x->parent->parent->red = true;
        right_rotate(t, x->parent->parent);
      }
    } else {
      /* Same as "then" clause with "right" and "left" exchanged. */
      node *y = x->parent->parent->left;
      if (y && y->red) {
        /* Case 1 */
        x->parent->red = false;
        y->red = false;
        x->parent->parent->red = true;
        x = x->parent->parent;
      } else {
        if (x == x->parent->left) {
          /* Case 2 */
          x = x->parent;
          right_rotate(t, x);
        }
        /* Case 3 */
        x->parent->red = false;
        x->parent->parent->red = true;
        left_rotate(t, x->parent->parent);
      }
    }
  }
  t->root->red = false;
  return false;
}

static bool is_black(node *n) { return !n || !n->red; }

static void remove_fixup(tree *t, node *x, node *x_parent) {
  while (x != t->root && is_black(x)) {
    if (x == x_parent->left) {
      /* The text points out that w cannot be null: it follows from the red-black properties. */
      node *w = x_parent->right;
      if (w->red) {
        /* Case 1 */
        w->red = false;
        x_parent->red = true;
        left_rotate(t, x_parent);
        w = x_parent->right;
      }
      if (is_black(w->left) && is_black(w->right)) {
        /* Case 2 */
        w->red = true;
        x = x_parent;
        x_parent = x->parent;
      } else {
        if (is_black(w->right)) {
          /* Case 3 */
          w->left->red = false;
          w->red = true;
          right_rotate(t, w);
          w = x_parent->right;
        }
        /* Case 4 */
        w->red = x_parent->red;
        x_parent->red = false;
        if (w->right) w->right->red = false;
        left_rotate(t, x_parent);
        x = t->root;
        x_parent = x->parent;
      }
    } else {
      /* Same as "then" clause with "right" and "left" exchanged. */
      node *w = x_parent->left;
      if (w->red) {
        /* Case 1 */
        w->red = false;
        x_parent->red = true;
        right_rotate(t, x_parent);
        w = x_parent->left;
      }
      if (is_black(w->right) && is_black(w->left)) {
        /* Case 2 */
        w->red = true;
        x = x_parent;
        x_parent = x->parent;
      } else {
        if (is_black(w->left)) {
          /* Case 3 */
          w->right->red = false;
          w->red = true;
          left_rotate(t, w);
          w = x_parent->left;
        }
        /* Case 4 */
        w->red = x_parent->red;
        x_parent->red = false;
        if (w->left) w->left->red = false;
        right_rotate(t, x_parent);
        x = t->root;
        x_parent = x->parent;
      }
    }
  }
  if (x) x->red = false;
}

static void tree_remove(tree *t, const key *k) {
  node *z = find_node(t, k);
  if (!z) return;

  /* Y is the node to be unlinked from the tree. */
  node *y = !z->left || !z->right ? z : successor(z);

  /* X is the child of y which might potentially replace y in the tree; it may be null. */
  node *x = y->left ? y->left : y->right;
  node *x_parent;
  if (x) {
    x->parent = y->parent;
    x_parent = x->parent;
  } else {
    x_parent = y->parent;
  }
  if (!y->parent) {
    t->root = x;
  } else if (y == y->parent->left) {
    y->parent->left = x;
  } else {
    y->parent->right = x;
  }

  if (y != z) {
    if (!y->red) remove_fixup(t, x, x_parent);
    y->parent = z->parent;
    y->red = z->red;
    y->left = z->left;
    y->right = z->right;
    if (z->left) z->left->parent = y;
    if (z->right) z->right->parent = y;
    if (z->parent) {
      if (z->parent->left == z) {
        z->parent->left = y;
      } else {
        z->parent->right = y;
      }
    } else {
      t->root = y;
    }
  } else if (!y->red) {
    remove_fixup(t, x, x_parent);
  }
  free(z);
}

static void free_nodes(node *n) {
  if (!n) return;
  free_nodes(n->left);
  free_nodes(n->right);
  free(n);
}

/* ---- the simulation */

#define MIN_X 0.0
#define MIN_Y 0.0
#define MAX_X 1000.0
#define MAX_Y 1000.0
#define MIN_Z 0.0
#define MAX_Z 10.0
#define PROXIMITY_RADIUS 1.0
#define GOOD_VOXEL_SIZE (PROXIMITY_RADIUS * 2.0)

typedef struct {
  int64_t aircraft_a, aircraft_b;
  vec3 position;
} collision;

typedef struct {
  collision *items;
  size_t len, cap;
} collision_vec;

typedef struct {
  int64_t callsign;
  vec3 position;
} aircraft;

static bool is_in_voxel(vec2 voxel, const motion *m) {
  if (voxel.x > MAX_X || voxel.x < MIN_X || voxel.y > MAX_Y || voxel.y < MIN_Y) return false;

  vec3 init = m->pos_one;
  vec3 fin = m->pos_two;

  double v_s = GOOD_VOXEL_SIZE;
  double r = PROXIMITY_RADIUS / 2.0;

  double v_x = voxel.x;
  double x0 = init.x;
  double xv = fin.x - init.x;

  double v_y = voxel.y;
  double y0 = init.y;
  double yv = fin.y - init.y;

  double low_x = (v_x - r - x0) / xv;
  double high_x = (v_x + v_s + r - x0) / xv;

  if (xv < 0.0) {
    double tmp = low_x;
    low_x = high_x;
    high_x = tmp;
  }

  double low_y = (v_y - r - y0) / yv;
  double high_y = (v_y + v_s + r - y0) / yv;

  if (yv < 0.0) {
    double tmp = low_y;
    low_y = high_y;
    high_y = tmp;
  }

  return ((xv == 0.0 && v_x <= x0 + r && x0 - r <= v_x + v_s) /* no motion in x */
          || ((low_x <= 1.0 && 1.0 <= high_x) || (low_x <= 0.0 && 0.0 <= high_x) ||
              (0.0 <= low_x && high_x <= 1.0))) &&
         ((yv == 0.0 && v_y <= y0 + r && y0 - r <= v_y + v_s) /* no motion in y */
          || ((low_y <= 1.0 && 1.0 <= high_y) || (low_y <= 0.0 && 0.0 <= high_y) ||
              (0.0 <= low_y && high_y <= 1.0))) &&
         (xv == 0.0 || yv == 0.0 /* no motion in x or y or both */
          || (low_y <= high_x && high_x <= high_y) || (low_y <= low_x && low_x <= high_y) ||
          (low_x <= low_y && high_y <= high_x));
}

static void put_into_map(tree *voxel_map, vec2 voxel, const motion *m) {
  key k = {.voxel = voxel};
  value *found = tree_get(voxel_map, &k);
  motion_vec *vec;
  if (found) {
    vec = found->motions;
  } else {
    vec = calloc(1, sizeof *vec);
    if (!vec) die("out of memory");
    value old;
    tree_put(voxel_map, &k, (value){.motions = vec}, &old);
  }
  append_motion(vec, *m);
}

static const vec2 horizontal = {GOOD_VOXEL_SIZE, 0.0};
static const vec2 vertical = {0.0, GOOD_VOXEL_SIZE};

static void recurse(tree *voxel_map, tree *seen, vec2 next_voxel, const motion *m) {
  if (!is_in_voxel(next_voxel, m)) return;
  value old;
  if (tree_put(seen, &(key){.voxel = next_voxel}, (value){.flag = true}, &old) && old.flag) return;

  put_into_map(voxel_map, next_voxel, m);

  recurse(voxel_map, seen, minus2(next_voxel, horizontal), m);
  recurse(voxel_map, seen, plus2(next_voxel, horizontal), m);
  recurse(voxel_map, seen, minus2(next_voxel, vertical), m);
  recurse(voxel_map, seen, plus2(next_voxel, vertical), m);
  recurse(voxel_map, seen, minus2(minus2(next_voxel, horizontal), vertical), m);
  recurse(voxel_map, seen, plus2(minus2(next_voxel, horizontal), vertical), m);
  recurse(voxel_map, seen, minus2(plus2(next_voxel, horizontal), vertical), m);
  recurse(voxel_map, seen, plus2(plus2(next_voxel, horizontal), vertical), m);
}

static vec2 voxel_hash(vec3 position) {
  int32_t x_div = (int32_t)(position.x / GOOD_VOXEL_SIZE);
  int32_t y_div = (int32_t)(position.y / GOOD_VOXEL_SIZE);

  vec2 result = {GOOD_VOXEL_SIZE * x_div, GOOD_VOXEL_SIZE * y_div};

  if (position.x < 0) result.x -= GOOD_VOXEL_SIZE;
  if (position.y < 0) result.y -= GOOD_VOXEL_SIZE;

  return result;
}

static void draw_motion_on_voxel_map(tree *voxel_map, const motion *m) {
  tree seen = {NULL, compare_voxels};
  recurse(voxel_map, &seen, voxel_hash(m->pos_one), m);
  free_nodes(seen.root);
}

typedef struct {
  motion_vec **items;
  size_t len, cap;
} reduced_vec;

static reduced_vec reduce_collision_set(const motion_vec *motions) {
  tree voxel_map = {NULL, compare_voxels};
  for (size_t i = 0; i < motions->len; i++) draw_motion_on_voxel_map(&voxel_map, &motions->items[i]);

  reduced_vec result = {0};
  for (node *e = tree_first(&voxel_map); e; e = successor(e)) {
    motion_vec *vec = e->value.motions;
    if (vec->len > 1) {
      if (result.len == result.cap) result.items = grow(result.items, &result.cap, sizeof *result.items);
      result.items[result.len++] = vec;
    } else {
      free(vec->items);
      free(vec);
    }
  }
  free_nodes(voxel_map.root);
  return result;
}

static bool find_intersection(const motion *m1, const motion *m2, vec3 *out) {
  vec3 init1 = m1->pos_one;
  vec3 init2 = m2->pos_one;
  vec3 vec1 = minus3(m1->pos_two, m1->pos_one);
  vec3 vec2_ = minus3(m2->pos_two, m2->pos_one);
  double radius = PROXIMITY_RADIUS;

  /* a = (V2 - V1)^T * (V2 - V1) */
  double a = squared_magnitude(minus3(vec2_, vec1));

  if (a != 0.0) {
    /* b = 2 * <I1-I2, V1-V2> */
    double b = 2.0 * dot(minus3(init1, init2), minus3(vec1, vec2_));
    /* c = -r^2 + (I2 - I1)^T * (I2 - I1) */
    double c = -radius * radius + squared_magnitude(minus3(init2, init1));

    double discr = b * b - 4.0 * a * c;
    if (discr < 0.0) return false;

    double v1 = (-b - sqrt(discr)) / (2.0 * a);
    double v2 = (-b + sqrt(discr)) / (2.0 * a);

    if (v1 <= v2 && ((v1 <= 1.0 && 1.0 <= v2) || (v1 <= 0.0 && 0.0 <= v2) || (0.0 <= v1 && v2 <= 1.0))) {
      /* Pick a good "time" at which to report the collision. */
      double v = v1 <= 0.0 ? 0.0 : v1;

      vec3 result1 = plus3(init1, times(vec1, v));
      vec3 result2 = plus3(init2, times(vec2_, v));

      vec3 result = times(plus3(result1, result2), 0.5);
      if (result.x >= MIN_X && result.x <= MAX_X && result.y >= MIN_Y && result.y <= MAX_Y &&
          result.z >= MIN_Z && result.z <= MAX_Z) {
        *out = result;
        return true;
      }
    }
    return false;
  }

  /* The planes have the same speeds and are moving in parallel (or not moving at all). */
  double dist = magnitude(minus3(init2, init1));
  if (dist <= radius) {
    *out = times(plus3(init1, init2), 0.5);
    return true;
  }
  return false;
}

static collision_vec handle_new_frame(tree *state, const aircraft *frame, size_t frame_len) {
  motion_vec motions = {0};
  tree seen = {NULL, compare_call_signs};

  for (size_t i = 0; i < frame_len; i++) {
    const aircraft *a = &frame[i];
    key k = {.callsign = a->callsign};
    value old;
    bool had = tree_put(state, &k, (value){.position = a->position}, &old);
    vec3 new_position = a->position;
    value ignored;
    tree_put(&seen, &k, (value){.flag = true}, &ignored);
    /* Treat newly introduced aircraft as if they were stationary. */
    vec3 old_position = had ? old.position : new_position;
    append_motion(&motions, (motion){a->callsign, old_position, new_position});
  }

  /* Remove aircraft that are no longer present. */
  int64_t *to_remove = NULL;
  size_t remove_len = 0, remove_cap = 0;
  for (node *e = tree_first(state); e; e = successor(e)) {
    value *was_seen = tree_get(&seen, &e->key);
    if (!was_seen || !was_seen->flag) {
      if (remove_len == remove_cap) to_remove = grow(to_remove, &remove_cap, sizeof *to_remove);
      to_remove[remove_len++] = e->key.callsign;
    }
  }
  for (size_t i = 0; i < remove_len; i++) tree_remove(state, &(key){.callsign = to_remove[i]});
  free(to_remove);
  free_nodes(seen.root);

  reduced_vec all_reduced = reduce_collision_set(&motions);
  collision_vec collisions = {0};

  for (size_t r = 0; r < all_reduced.len; r++) {
    motion_vec *reduced = all_reduced.items[r];
    for (size_t i = 0; i < reduced->len; i++) {
      const motion *motion1 = &reduced->items[i];
      for (size_t j = i + 1; j < reduced->len; j++) {
        const motion *motion2 = &reduced->items[j];
        vec3 position;
        if (find_intersection(motion1, motion2, &position)) {
          if (collisions.len == collisions.cap)
            collisions.items = grow(collisions.items, &collisions.cap, sizeof *collisions.items);
          collisions.items[collisions.len++] = (collision){motion1->callsign, motion2->callsign, position};
        }
      }
    }
    free(reduced->items);
    free(reduced);
  }
  free(all_reduced.items);
  free(motions.items);
  return collisions;
}

static void simulate(const int64_t *callsigns, size_t num_aircraft, double time, aircraft *frame) {
  for (size_t i = 0; i < num_aircraft; i += 2) {
    frame[i] = (aircraft){callsigns[i], {time, cos(time) * 2 + (double)(i * 3), 10}};
    frame[i + 1] = (aircraft){callsigns[i + 1], {time, sin(time) * 2 + (double)(i * 3), 10}};
  }
}

static int64_t cd(int64_t num_aircrafts) {
  const int num_frames = 200;
  int64_t *callsigns = malloc(sizeof *callsigns * (size_t)(num_aircrafts + 1));
  aircraft *frame = malloc(sizeof *frame * (size_t)(num_aircrafts + 1));
  if (!callsigns || !frame) die("out of memory");
  for (int64_t i = 0; i < num_aircrafts; i++) callsigns[i] = i;
  tree state = {NULL, compare_call_signs};

  int64_t actual_collisions = 0;
  for (int i = 0; i < num_frames; i++) {
    double time = i / 10.0;
    simulate(callsigns, (size_t)num_aircrafts, time, frame);
    collision_vec collisions = handle_new_frame(&state, frame, (size_t)num_aircrafts);
    actual_collisions += (int64_t)collisions.len;
    free(collisions.items);
  }
  free_nodes(state.root);
  free(frame);
  free(callsigns);
  return actual_collisions;
}

static int64_t run(int64_t n) { return cd(n - n % 2); }

int main(int argc, char **argv) { return bench_main(argc, argv, run, 1000); }
