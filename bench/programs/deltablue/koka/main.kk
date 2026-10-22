// DeltaBlue from Are We Fast Yet; see bench/programs/deltablue/LICENSE. Deviations are the Sakura port's:
// records in growable vectors by dense id, threaded through every planner call (Koka `ref`s would form
// reference-count cycles); Perceus updates the uniquely owned vectors in place. Stable merge sort.
import runner

val required = -800
val preferred = -400
val strong-default = -200
val default-strength = 0
val absolute-weakest = 10000

type dir { Undirected; Forward; Backward }
type kind { Edit; Stay; Equality; Scale(scale: int, offset: int) }

struct constraint
  kind: kind
  strength: int
  v1: int
  v2: int
  dir: dir
  satisfied: bool

struct variable
  value: int
  constraints: list<int> // newest first, so attaching is O(1); traversals reverse it
  determined-by: int
  walk: int
  stay: bool
  mark: int

// A growable vector: `items` holds `len` live entries and spare capacity.
struct pool<a>
  items: vector<a>
  len: int

fun pool/at(p: pool<a>, i: int): exn a
  p.items[i]

fun pool/set(p: pool<a>, i: int, x: a): exn pool<a>
  match p
    Pool(items, len) ->
      var xs := items
      xs[i] := x
      Pool(xs, len)

fun pool/push(p: pool<a>, x: a): exn pool<a>
  match p
    Pool(items, len) ->
      if len < items.length then
        var xs := items
        xs[len] := x
        Pool(xs, len + 1)
      else Pool(vector-init-total(2 * len + 16, fn(i) if i < len then items.at(i).default(x) else x), len + 1)

struct planner
  vars: pool<variable>
  cons: pool<constraint>
  mark: int

fun new-planner(): planner
  Planner(Pool([].vector, 0), Pool([].vector, 0), 1)

fun var-of(p: planner, i: int): exn variable
  p.vars.at(i)

fun con-of(p: planner, i: int): exn constraint
  p.cons.at(i)

// Destructure first so the vectors stay uniquely owned and update in place.
fun put-var(p: planner, i: int, v: variable): exn planner
  match p
    Planner(vars, cons, cur) -> Planner(vars.set(i, v), cons, cur)

fun put-con(p: planner, i: int, c: constraint): exn planner
  match p
    Planner(vars, cons, cur) -> Planner(vars, cons.set(i, c), cur)

fun new-var(p: planner, value: int): exn (planner, int)
  match p
    Planner(vars, cons, cur) ->
      // The id is read from the pushed pool: reading `vars` afterwards would share its vector.
      val pushed = vars.push(Variable(value, [], -1, absolute-weakest, True, 0))
      val i = pushed.len - 1
      (Planner(pushed, cons, cur), i)

fun value-of(p: planner, i: int): exn int
  var-of(p, i).value

fun set-value(p: planner, i: int, value: int): exn planner
  val x = var-of(p, i)
  put-var(p, i, x(value = value))

fun set-mark(p: planner, i: int, cur: int): exn planner
  val x = var-of(p, i)
  put-var(p, i, x(mark = cur))

fun new-mark(p: planner): (planner, int)
  match p
    Planner(vars, cons, cur) -> (Planner(vars, cons, cur + 1), cur + 1)

// ---- constraint queries

fun is-unary(c: constraint): bool
  match c.kind
    Edit -> True
    Stay -> True
    _ -> False

fun is-input(c: constraint): bool
  c.kind.is-edit

fun is-satisfied(c: constraint): bool
  if c.is-unary then c.satisfied else !c.dir.is-undirected

fun output(c: constraint): int
  if c.is-unary || !c.dir.is-forward then c.v1 else c.v2

// The variables a constraint reads, as AWFY's inputsDo.
fun inputs(c: constraint): list<int>
  val primary = if c.dir.is-forward then c.v1 else c.v2
  match c.kind
    Equality -> [primary]
    Scale(s, o) -> [primary, s, o]
    _ -> []

// The variables a constraint is attached to, as AWFY's addToGraph.
fun attached(c: constraint): list<int>
  match c.kind
    Equality -> [c.v1, c.v2]
    Scale(s, o) -> [c.v1, c.v2, s, o]
    _ -> [c.v1]

fun weakest(a: int, b: int): int
  if b > a then b else a

// ---- constraint operations

fun unsatisfied(c: constraint): constraint
  c(dir = Undirected, satisfied = False)

fun add-to-graph(p0: planner, cid: int): exn planner
  val c = con-of(p0, cid)
  val p = c.attached.foldl(p0) fn(p, v)
    val x = var-of(p, v)
    put-var(p, v, x(constraints = Cons(cid, x.constraints)))
  put-con(p, cid, c.unsatisfied)

fun remove-from-graph(p0: planner, cid: int): exn planner
  val c = con-of(p0, cid)
  val p = c.attached.foldl(p0) fn(p, v)
    val x = var-of(p, v)
    val determined-by = if x.determined-by == cid then -1 else x.determined-by
    put-var(p, v, x(constraints = x.constraints.filter(fn(k) k != cid), determined-by = determined-by))
  put-con(p, cid, c.unsatisfied)

fun choose-method(p: planner, cid: int, cur: int): exn planner
  val c = con-of(p, cid)
  if c.is-unary then
    val out = var-of(p, c.v1)
    put-con(p, cid, c(satisfied = out.mark != cur && c.strength < out.walk))
  else
    val v1 = var-of(p, c.v1)
    val v2 = var-of(p, c.v2)
    val dir =
      if v1.mark == cur then (if v2.mark != cur && c.strength < v2.walk then Forward else Undirected)
      elif v2.mark == cur then (if v1.mark != cur && c.strength < v1.walk then Backward else Undirected)
      elif v1.walk > v2.walk then (if c.strength < v1.walk then Backward else Undirected)
      elif c.strength < v2.walk then Forward
      else Undirected
    put-con(p, cid, c(dir = dir))

fun execute(p: planner, cid: int): exn planner
  val c = con-of(p, cid)
  match c.kind
    Equality ->
      if c.dir.is-forward then
        val x = value-of(p, c.v1)
        set-value(p, c.v2, x)
      else
        val x = value-of(p, c.v2)
        set-value(p, c.v1, x)
    Scale(s, o) ->
      if c.dir.is-forward then
        val x = value-of(p, c.v1) * value-of(p, s) + value-of(p, o)
        set-value(p, c.v2, x)
      else
        val x = (value-of(p, c.v2) - value-of(p, o)) / value-of(p, s)
        set-value(p, c.v1, x)
    _ -> p

fun recalculate(p0: planner, cid: int): exn planner
  val c = con-of(p0, cid)
  val out-id = c.output
  val out = var-of(p0, out-id)
  val p = if c.is-unary then put-var(p0, out-id, out(walk = c.strength, stay = !c.is-input))
    else
      val ihn = var-of(p0, if c.dir.is-forward then c.v1 else c.v2)
      val stay = match c.kind
        Scale(s, o) -> ihn.stay && var-of(p0, s).stay && var-of(p0, o).stay
        _ -> ihn.stay
      put-var(p0, out-id, out(walk = weakest(c.strength, ihn.walk), stay = stay))
  if var-of(p, out-id).stay then execute(p, cid) else p

fun inputs-known(p: planner, c: constraint, cur: int): exn bool
  if c.is-unary then True
  else
    val v = var-of(p, if c.dir.is-forward then c.v1 else c.v2)
    v.mark == cur || v.stay || v.determined-by < 0

// AWFY's satisfy: the constraint it overrides, or -1.
fun satisfy(p0: planner, cid: int, cur: int): <div,exn> (planner, int)
  val p1 = choose-method(p0, cid, cur)
  val c = con-of(p1, cid)
  if !c.is-satisfied then
    if c.strength == required then throw("Could not satisfy a required constraint") else (p1, -1)
  else
    val p2 = c.inputs.foldl(p1, fn(p, i) set-mark(p, i, cur))
    val out-id = c.output
    val overridden = var-of(p2, out-id).determined-by
    val p3 = if overridden >= 0 then
        val o = con-of(p2, overridden)
        put-con(p2, overridden, o.unsatisfied)
      else p2
    val x = var-of(p3, out-id)
    val (q, ok) = add-propagate(put-var(p3, out-id, x(determined-by = cid)), cid, cur)
    if !ok then throw("Cycle encountered")
    (set-mark(q, out-id, cur), overridden)

// ---- planner

fun add-constraint(p0: planner, c: constraint): <div,exn> (planner, int)
  match p0
    Planner(vars, cons, cur) ->
      val pushed = cons.push(c)
      val cid = pushed.len - 1
      val p = add-to-graph(Planner(vars, pushed, cur), cid)
      (incremental-add(p, cid), cid)

fun destroy-constraint(p0: planner, cid: int): <div,exn> planner
  val p = if con-of(p0, cid).is-satisfied then incremental-remove(p0, cid) else p0
  remove-from-graph(p, cid)

fun incremental-add(p0: planner, cid: int): <div,exn> planner
  val (p1, cur) = new-mark(p0)
  fun loop(p: planner, overridden: int): <div,exn> planner
    if overridden < 0 then p
    else
      val (q, next) = satisfy(p, overridden, cur)
      loop(q, next)
  val (p2, first) = satisfy(p1, cid, cur)
  loop(p2, first)

fun incremental-remove(p0: planner, cid: int): <div,exn> planner
  val c = con-of(p0, cid)
  val out = c.output
  val p1 = remove-from-graph(put-con(p0, cid, c.unsatisfied), cid)
  val (p2, todo) = remove-propagate-from(p1, out)
  todo.foldl(p2, incremental-add)

fun consuming(p: planner, v: int): exn list<int>
  val x = var-of(p, v)
  x.constraints.reverse.filter(fn(k) k != x.determined-by && con-of(p, k).is-satisfied)

// The queues below are FIFO: a front list and a reversed back list.
fun add-propagate(p0: planner, cid: int, cur: int): <div,exn> (planner, bool)
  fun go(p: planner, front: list<int>, back: list<int>): <div,exn> (planner, bool)
    match front
      Cons(d, rest) ->
        if var-of(p, con-of(p, d).output).mark == cur then (incremental-remove(p, cid), False)
        else
          val q = recalculate(p, d)
          val next = consuming(q, con-of(q, d).output)
          go(q, rest, next.reverse-append(back))
      Nil -> if back.is-nil then (p, True) else go(p, back.reverse, Nil)
  go(p0, [cid], Nil)

fun merge(xs: list<(int, int)>, ys: list<(int, int)>): div list<(int, int)>
  match xs
    Nil -> ys
    Cons(x, xx) -> match ys
      Nil -> xs
      Cons(y, yy) -> if y.fst < x.fst then Cons(y, merge(xs, yy)) else Cons(x, merge(xx, ys))

// Stable merge sort on the first component.
fun sort-by-key(xs: list<(int, int)>): div list<(int, int)>
  match xs
    Nil -> xs
    Cons(_, Nil) -> xs
    _ ->
      val (a, b) = xs.split(xs.length / 2)
      merge(sort-by-key(a), sort-by-key(b))

fun remove-propagate-from(p0: planner, out: int): <div,exn> (planner, list<int>)
  val x = var-of(p0, out)
  val p1 = put-var(p0, out, x(determined-by = -1, walk = absolute-weakest, stay = True))
  fun go(p: planner, front: list<int>, back: list<int>, unsat: list<int>): <div,exn> (planner, list<int>)
    match front
      Cons(v, rest) ->
        val unsat2 = var-of(p, v).constraints.foldr(unsat) fn(c, acc)
          if con-of(p, c).is-satisfied then acc else Cons(c, acc)
        val (q, back2) = consuming(p, v).foldl((p, back)) fn(acc, c)
          val (q0, b) = acc
          val q1 = recalculate(q0, c)
          (q1, Cons(con-of(q1, c).output, b))
        go(q, rest, back2, unsat2)
      Nil -> if back.is-nil then (p, unsat.reverse) else go(p, back.reverse, Nil, unsat)
  val (p, unsat) = go(p1, [out], Nil, Nil)
  // Strongest first; stable (AWFY uses SOM's in-place sort; see ../LICENSE).
  val keyed = unsat.map(fn(c) (con-of(p, c).strength, c))
  (p, sort-by-key(keyed).map(snd))

fun make-plan(p0: planner, sources: list<int>): <div,exn> (planner, list<int>)
  val (p1, cur) = new-mark(p0)
  fun go(p: planner, front: list<int>, back: list<int>, plan: list<int>): <div,exn> (planner, list<int>)
    match front
      Cons(cid, rest) ->
        val c = con-of(p, cid)
        val out = c.output
        if var-of(p, out).mark != cur && inputs-known(p, c, cur) then
          val q = set-mark(p, out, cur)
          val next = consuming(q, out)
          go(q, rest, next.reverse-append(back), Cons(cid, plan))
        else go(p, rest, back, plan)
      Nil -> if back.is-nil then (p, plan.reverse) else go(p, back.reverse, Nil, plan)
  go(p1, sources, Nil, Nil)

fun extract-plan(p: planner, constraints: list<int>): <div,exn> (planner, list<int>)
  val sources = constraints.filter fn(k)
    val c = con-of(p, k)
    c.is-input && c.is-satisfied
  make-plan(p, sources)

fun execute-plan(p: planner, plan: list<int>): exn planner
  plan.foldl(p, execute)

fun unary(kind: kind, v: int, strength: int): constraint
  Constraint(kind, strength, v, v, Undirected, False)

fun binary(kind: kind, v1: int, v2: int, strength: int): constraint
  Constraint(kind, strength, v1, v2, Undirected, False)

fun change(p0: planner, v: int, new-value: int): <div,exn> planner
  val (p1, edit) = add-constraint(p0, unary(Edit, v, preferred))
  val (p2, plan) = extract-plan(p1, [edit])
  val p3 = list(1, 10).foldl(p2, fn(p, _) execute-plan(set-value(p, v, new-value), plan))
  destroy-constraint(p3, edit)

// ---- the AWFY tests

fun chain-test(n: int): <div,exn> int
  val p1 = list(0, n).foldl(new-planner(), fn(p, _) new-var(p, 0).fst)
  val p2 = list(0, n - 1).foldl(p1, fn(p, i) add-constraint(p, binary(Equality, i, i + 1, required)).fst)
  val (p3, _) = add-constraint(p2, unary(Stay, n, strong-default))
  val (p4, edit) = add-constraint(p3, unary(Edit, 0, preferred))
  val (p5, plan) = extract-plan(p4, [edit])
  val (p6, sum) = list(0, 99).foldl((p5, 0)) fn(acc, i)
    val (p, s) = acc
    val q = execute-plan(set-value(p, 0, i), plan)
    val x = value-of(q, n)
    if x != i then throw("Chain test failed!")
    (q, s + x)
  val _ = destroy-constraint(p6, edit)
  sum + plan.length

fun sum-values(p: planner, ids: list<int>): exn int
  ids.foldl(0, fn(acc, v) acc + value-of(p, v))

fun check-dests(p: planner, dests: list<int>, n: int, offset: int, message: string): exn ()
  dests.foreach-indexed fn(i, d)
    if i < n - 1 && value-of(p, d) != (i + 1) * 5 + offset then throw(message)

fun projection-test(n: int): <div,exn> int
  if n == 0 then return 0
  val (p0, scale) = new-var(new-planner(), 10)
  val (p1, offset) = new-var(p0, 1000)
  val (p2, src, dst, rdests) = list(1, n).foldl((p1, 0, 0, Nil)) fn(acc, i)
    val (p, _, _, ds) = acc
    val (q1, s) = new-var(p, i)
    val (q2, d) = new-var(q1, i)
    val (q3, _) = add-constraint(q2, unary(Stay, s, default-strength))
    val (q4, _) = add-constraint(q3, binary(Scale(scale, offset), s, d, required))
    (q4, s, d, Cons(d, ds))
  val dests = rdests.reverse
  val p3 = change(p2, src, 17)
  val a = value-of(p3, dst)
  if a != 1170 then throw("Projection test 1 failed!")
  val p4 = change(p3, dst, 1050)
  val b = value-of(p4, src)
  if b != 5 then throw("Projection test 2 failed!")
  val p5 = change(p4, scale, 5)
  check-dests(p5, dests, n, 1000, "Projection test 3 failed!")
  val c = sum-values(p5, dests)
  val p6 = change(p5, offset, 2000)
  check-dests(p6, dests, n, 2000, "Projection test 4 failed!")
  a + b + c + sum-values(p6, dests)

fun deltablue(n: int): <div,exn> int
  chain-test(n) + projection-test(n)

fun main() { bench-main(deltablue, 10000) }
