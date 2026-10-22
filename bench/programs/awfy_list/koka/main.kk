// List from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/list.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). As the Sakura
// source: an element is an immutable struct whose `next` is a `maybe` (null is Nothing), built
// with its tail; AWFY's `x.next` on a list known to be non-empty is `next-of(x)`, which throws on
// Nothing as JavaScript would. isShorterThan's while loop with an early return is a
// tail-recursive function (Koka has no `return` inside a loop body). run(n) sums n benchmark()
// results.
import runner

struct element
  value: int
  next: maybe<element>

fun element/length(e: element): div int
  match e.next
    Nothing -> 1
    Just(n) -> 1 + n.length

fun next-of(x: maybe<element>): exn maybe<element>
  match x
    Just(e) -> e.next
    Nothing -> throw("element")

fun make-list(length: int): div maybe<element>
  if length == 0 then Nothing else Just(Element(length, make-list(length - 1)))

fun is-shorter-than(x-tail: maybe<element>, y-tail: maybe<element>): div bool
  match y-tail
    Nothing -> False
    Just(y) -> match x-tail
      Nothing -> True
      Just(x) -> is-shorter-than(x.next, y.next)

fun tail(x: maybe<element>, y: maybe<element>, z: maybe<element>): <div,exn> maybe<element>
  if is-shorter-than(y, x) then tail(tail(x.next-of, y, z), tail(y.next-of, z, x), tail(z.next-of, x, y))
  else z

fun benchmark(): <div,exn> int
  match tail(make-list(15), make-list(10), make-list(6))
    Just(result) -> result.length
    Nothing -> throw("result")

fun run(n: int): <div,exn> int {
  var sum := 0
  var i := 0
  while { i < n } {
    sum := sum + benchmark()
    i := i + 1
  }
  sum
}

fun main() { bench-main(run, 6000) }
