// DeltaBlue from Are We Fast Yet; see bench/programs/deltablue/LICENSE.
// Arena indices stand in for the JS object graph, as in the Rust and Go ports. The unsatisfied
// list is sorted stably by strength: qsort over (strength, list position) pairs.
#include "runner.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

enum kind { EDIT, STAY, EQUALITY, SCALE };
enum { NONE = -1 };

static void die(const char *message) {
  fprintf(stderr, "%s\n", message);
  exit(1);
}

static void *grow(void *items, size_t *cap, size_t size) {
  *cap = *cap ? *cap * 2 : 4;
  items = realloc(items, *cap * size);
  if (!items) die("out of memory");
  return items;
}

typedef struct {
  int *items;
  size_t len, cap;
} IntVec;

static void push(IntVec *v, int x) {
  if (v->len == v->cap) v->items = grow(v->items, &v->cap, sizeof *v->items);
  v->items[v->len++] = x;
}

typedef struct {
  enum kind kind;
  int64_t strength;
  int v1, v2, scale, offset;
  int dir; /* 1 forward (v1 -> v2), -1 backward, 0 unsatisfied */
  bool satisfied;
} Constraint;

typedef struct {
  int64_t value;
  IntVec constraints;
  int determined;
  int64_t walk;
  bool stay;
  int64_t mark;
} Variable;

typedef struct {
  Variable *vars;
  size_t nvars, capvars;
  Constraint *cons;
  size_t ncons, capcons;
  int64_t mark;
} Planner;

static bool unary(const Constraint *c) { return c->kind == EDIT || c->kind == STAY; }
static bool is_satisfied(const Constraint *c) { return unary(c) ? c->satisfied : c->dir != 0; }
static int output(const Constraint *c) { return unary(c) || c->dir != 1 ? c->v1 : c->v2; }
static int input(const Constraint *c) { return c->dir == 1 ? c->v1 : c->v2; }
static void unsatisfy(Constraint *c) {
  c->dir = 0;
  c->satisfied = false;
}

static size_t inputs(const Constraint *c, int out[3]) {
  if (unary(c)) return 0;
  out[0] = input(c);
  if (c->kind == EQUALITY) return 1;
  out[1] = c->scale;
  out[2] = c->offset;
  return 3;
}

static size_t attached(const Constraint *c, int out[4]) {
  out[0] = c->v1;
  if (unary(c)) return 1;
  out[1] = c->v2;
  if (c->kind == EQUALITY) return 2;
  out[2] = c->scale;
  out[3] = c->offset;
  return 4;
}

static int new_var(Planner *p, int64_t value) {
  if (p->nvars == p->capvars) p->vars = grow(p->vars, &p->capvars, sizeof *p->vars);
  p->vars[p->nvars] = (Variable){value, {0}, NONE, 10000, true, 0};
  return (int)p->nvars++;
}

static int64_t new_mark(Planner *p) { return ++p->mark; }

static void incremental_add(Planner *p, int id);
static void incremental_remove(Planner *p, int id);

static int add(Planner *p, enum kind kind, int v1, int v2, int64_t strength, int scale, int offset) {
  if (p->ncons == p->capcons) p->cons = grow(p->cons, &p->capcons, sizeof *p->cons);
  int id = (int)p->ncons++;
  p->cons[id] = (Constraint){kind, strength, v1, v2, scale, offset, 0, false};
  int vs[4];
  size_t count = attached(&p->cons[id], vs);
  for (size_t i = 0; i < count; i++) push(&p->vars[vs[i]].constraints, id);
  incremental_add(p, id);
  return id;
}

static void remove_graph(Planner *p, int id) {
  int vs[4];
  size_t count = attached(&p->cons[id], vs);
  for (size_t i = 0; i < count; i++) {
    Variable *v = &p->vars[vs[i]];
    size_t kept = 0;
    for (size_t j = 0; j < v->constraints.len; j++)
      if (v->constraints.items[j] != id) v->constraints.items[kept++] = v->constraints.items[j];
    v->constraints.len = kept;
    if (v->determined == id) v->determined = NONE;
  }
  unsatisfy(&p->cons[id]);
}

static void choose(Planner *p, int id, int64_t mark) {
  Constraint *c = &p->cons[id];
  const Variable *v1 = &p->vars[c->v1], *v2 = &p->vars[c->v2];
  if (unary(c)) {
    c->satisfied = v1->mark != mark && c->strength < v1->walk;
    return;
  }
  if (v1->mark == mark)
    c->dir = v2->mark != mark && c->strength < v2->walk ? 1 : 0;
  else if (v2->mark == mark)
    c->dir = v1->mark != mark && c->strength < v1->walk ? -1 : 0;
  else if (v1->walk > v2->walk)
    c->dir = c->strength < v1->walk ? -1 : 0;
  else
    c->dir = c->strength < v2->walk ? 1 : 0;
}

static void execute(Planner *p, int id) {
  const Constraint *c = &p->cons[id];
  Variable *vs = p->vars;
  int64_t value;
  switch (c->kind) {
  case EDIT:
  case STAY:
    return;
  case EQUALITY:
    value = vs[input(c)].value;
    break;
  default:
    value = c->dir == 1 ? vs[c->v1].value * vs[c->scale].value + vs[c->offset].value
                        : (vs[c->v2].value - vs[c->offset].value) / vs[c->scale].value;
  }
  vs[output(c)].value = value;
}

static void recalculate(Planner *p, int id) {
  const Constraint *c = &p->cons[id];
  int64_t walk;
  bool stay;
  if (unary(c)) {
    walk = c->strength;
    stay = c->kind != EDIT;
  } else {
    const Variable *in = &p->vars[input(c)];
    stay = in->stay;
    if (c->kind == SCALE) stay = stay && p->vars[c->scale].stay && p->vars[c->offset].stay;
    walk = c->strength > in->walk ? c->strength : in->walk;
  }
  Variable *out = &p->vars[output(c)];
  out->walk = walk;
  out->stay = stay;
  if (stay) execute(p, id);
}

/* Appends the satisfied constraints of v other than the one determining it. */
static void add_consuming(const Planner *p, int v, IntVec *out) {
  const Variable *var = &p->vars[v];
  for (size_t i = 0; i < var->constraints.len; i++) {
    int c = var->constraints.items[i];
    if (var->determined != c && is_satisfied(&p->cons[c])) push(out, c);
  }
}

static int satisfy(Planner *p, int id, int64_t mark) {
  choose(p, id, mark);
  Constraint c = p->cons[id];
  if (!is_satisfied(&c)) {
    if (c.strength == -800) die("Could not satisfy a required constraint");
    return NONE;
  }
  int ins[3];
  size_t count = inputs(&c, ins);
  for (size_t i = 0; i < count; i++) p->vars[ins[i]].mark = mark;
  int out = output(&c);
  int overridden = p->vars[out].determined;
  if (overridden != NONE) unsatisfy(&p->cons[overridden]);
  p->vars[out].determined = id;
  IntVec todo = {0};
  push(&todo, id);
  for (size_t head = 0; head < todo.len; head++) {
    int d = todo.items[head];
    int o = output(&p->cons[d]);
    if (p->vars[o].mark == mark) {
      incremental_remove(p, id);
      die("Cycle encountered");
    }
    recalculate(p, d);
    add_consuming(p, o, &todo);
  }
  free(todo.items);
  p->vars[out].mark = mark;
  return overridden;
}

static void incremental_add(Planner *p, int id) {
  int64_t mark = new_mark(p);
  for (int c = satisfy(p, id, mark); c != NONE; c = satisfy(p, c, mark)) {
  }
}

typedef struct {
  int64_t strength;
  size_t position;
  int id;
} Ranked;

static int by_strength(const void *a, const void *b) {
  const Ranked *x = a, *y = b;
  if (x->strength != y->strength) return x->strength < y->strength ? -1 : 1;
  return x->position < y->position ? -1 : x->position > y->position;
}

static void incremental_remove(Planner *p, int id) {
  int out = output(&p->cons[id]);
  unsatisfy(&p->cons[id]);
  remove_graph(p, id);
  p->vars[out].determined = NONE;
  p->vars[out].walk = 10000;
  p->vars[out].stay = true;
  IntVec unsatisfied = {0}, todo = {0}, consuming = {0};
  push(&todo, out);
  for (size_t head = 0; head < todo.len; head++) {
    int v = todo.items[head];
    const IntVec *cs = &p->vars[v].constraints;
    for (size_t i = 0; i < cs->len; i++)
      if (!is_satisfied(&p->cons[cs->items[i]])) push(&unsatisfied, cs->items[i]);
    consuming.len = 0;
    add_consuming(p, v, &consuming);
    for (size_t i = 0; i < consuming.len; i++) {
      recalculate(p, consuming.items[i]);
      push(&todo, output(&p->cons[consuming.items[i]]));
    }
  }
  Ranked *ranked = malloc((unsatisfied.len ? unsatisfied.len : 1) * sizeof *ranked);
  if (!ranked) die("out of memory");
  for (size_t i = 0; i < unsatisfied.len; i++)
    ranked[i] = (Ranked){p->cons[unsatisfied.items[i]].strength, i, unsatisfied.items[i]};
  qsort(ranked, unsatisfied.len, sizeof *ranked, by_strength);
  for (size_t i = 0; i < unsatisfied.len; i++) incremental_add(p, ranked[i].id);
  free(ranked);
  free(unsatisfied.items);
  free(todo.items);
  free(consuming.items);
}

static void destroy(Planner *p, int id) {
  if (is_satisfied(&p->cons[id])) incremental_remove(p, id);
  remove_graph(p, id);
}

/* The plan for one edit constraint (AWFY extractPlanFromConstraints with a single source). */
static IntVec make_plan(Planner *p, int edit) {
  IntVec todo = {0}, plan = {0};
  if (p->cons[edit].kind == EDIT && is_satisfied(&p->cons[edit])) push(&todo, edit);
  int64_t mark = new_mark(p);
  for (size_t head = 0; head < todo.len; head++) {
    int id = todo.items[head];
    const Constraint *c = &p->cons[id];
    int out = output(c);
    const Variable *in = &p->vars[input(c)];
    bool known = unary(c) || in->mark == mark || in->stay || in->determined == NONE;
    if (p->vars[out].mark != mark && known) {
      push(&plan, id);
      p->vars[out].mark = mark;
      add_consuming(p, out, &todo);
    }
  }
  free(todo.items);
  return plan;
}

static void execute_plan(Planner *p, const IntVec *plan) {
  for (size_t i = 0; i < plan->len; i++) execute(p, plan->items[i]);
}

static void change(Planner *p, int v, int64_t value) {
  int edit = add(p, EDIT, v, v, -400, 0, 0);
  IntVec plan = make_plan(p, edit);
  for (int i = 0; i < 10; i++) {
    p->vars[v].value = value;
    execute_plan(p, &plan);
  }
  free(plan.items);
  destroy(p, edit);
}

static void planner_free(Planner *p) {
  for (size_t i = 0; i < p->nvars; i++) free(p->vars[i].constraints.items);
  free(p->vars);
  free(p->cons);
}

static int64_t run(int64_t n64) {
  int n = (int)n64;
  Planner p = {.mark = 1};
  int *vars = malloc(((size_t)n + 1) * sizeof *vars);
  if (!vars) die("out of memory");
  for (int i = 0; i <= n; i++) vars[i] = new_var(&p, 0);
  for (int i = 0; i < n; i++) add(&p, EQUALITY, vars[i], vars[i + 1], -800, 0, 0);
  add(&p, STAY, vars[n], vars[n], -200, 0, 0);
  int edit = add(&p, EDIT, vars[0], vars[0], -400, 0, 0);
  IntVec plan = make_plan(&p, edit);
  int64_t chain = 0;
  for (int64_t i = 0; i < 100; i++) {
    p.vars[vars[0]].value = i;
    execute_plan(&p, &plan);
    if (p.vars[vars[n]].value != i) die("Chain test failed");
    chain += p.vars[vars[n]].value;
  }
  destroy(&p, edit);
  chain += (int64_t)plan.len;
  free(plan.items);
  free(vars);
  planner_free(&p);
  if (n == 0) return chain;

  p = (Planner){.mark = 1};
  int scale = new_var(&p, 10), offset = new_var(&p, 1000);
  int *dests = malloc((size_t)n * sizeof *dests);
  if (!dests) die("out of memory");
  int src = 0, dst = 0;
  for (int i = 1; i <= n; i++) {
    src = new_var(&p, i);
    dst = new_var(&p, i);
    dests[i - 1] = dst;
    add(&p, STAY, src, src, 0, 0, 0);
    add(&p, SCALE, src, dst, -800, scale, offset);
  }
  change(&p, src, 17);
  if (p.vars[dst].value != 1170) die("Projection test 1 failed");
  change(&p, dst, 1050);
  if (p.vars[src].value != 5) die("Projection test 2 failed");
  change(&p, scale, 5);
  for (int i = 0; i < n - 1; i++)
    if (p.vars[dests[i]].value != (int64_t)(i + 1) * 5 + 1000) die("Projection test 3 failed");
  int64_t first = 0;
  for (int i = 0; i < n; i++) first += p.vars[dests[i]].value;
  change(&p, offset, 2000);
  for (int i = 0; i < n - 1; i++)
    if (p.vars[dests[i]].value != (int64_t)(i + 1) * 5 + 2000) die("Projection test 4 failed");
  int64_t last = 0;
  for (int i = 0; i < n; i++) last += p.vars[dests[i]].value;
  free(dests);
  planner_free(&p);
  return chain + 1170 + 5 + first + last;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 10000); }
