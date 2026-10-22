// Bounce from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/bounce.js and
// som.js's Random; Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks).
// As the Sakura source: Random and the balls are immutable structs, next() returns the new
// Random with the value, bounce() returns the moved ball with the flag, and the balls are a
// vector written in place while uniquely owned (AWFY writes fields in place). Random's seed is an
// int64 so it keeps the bitwise `and` (Koka's int has none; Sakura takes `% 65536`, the same
// value). run(n) sums n benchmark() results.
import std/num/int64
import runner

// som.js's Random.
struct random
  seed: int64

fun next(r: random): (random, int)
  val seed = (r.seed * 1309.int64 + 13849.int64).and(65535.int64)
  (Random(seed), seed.int)

struct ball
  x: int
  y: int
  x-vel: int
  y-vel: int

fun new-ball(r0: random): (random, ball)
  val (r1, x) = r0.next
  val (r2, y) = r1.next
  val (r3, x-vel) = r2.next
  val (r4, y-vel) = r3.next
  (r4, Ball(x % 500, y % 500, (x-vel % 300) - 150, (y-vel % 300) - 150))

fun bounce(start: ball): (ball, bool) {
  val x-limit = 500
  val y-limit = 500
  var b := start
  var bounced := False

  b := b(x = b.x + b.x-vel, y = b.y + b.y-vel)

  if b.x > x-limit then {
    b := b(x = x-limit, x-vel = 0 - b.x-vel.abs)
    bounced := True
  }

  if b.x < 0 then {
    b := b(x = 0, x-vel = b.x-vel.abs)
    bounced := True
  }

  if b.y > y-limit then {
    b := b(y = y-limit, y-vel = 0 - b.y-vel.abs)
    bounced := True
  }

  if b.y < 0 then {
    b := b(y = 0, y-vel = b.y-vel.abs)
    bounced := True
  }

  (b, bounced)
}

fun benchmark(): <div,exn> int {
  var random := Random(74755.int64)
  val ball-count = 100
  var bounces := 0
  var balls := vector(ball-count, Ball(0, 0, 0, 0))

  for(ball-count, fn(i) {
    val (r, b) = new-ball(random)
    random := r
    balls[i] := b
  })

  for(50, fn(_) {
    for(ball-count, fn(j) {
      val (b, bounced) = bounce(balls[j])
      balls[j] := b
      if bounced then bounces := bounces + 1
    })
  })
  bounces
}

fun run(n: int): <div,exn> int {
  var sum := 0
  var i := 0
  while { i < n } {
    sum := sum + benchmark()
    i := i + 1
  }
  sum
}

fun main() { bench-main(run, 10000) }
