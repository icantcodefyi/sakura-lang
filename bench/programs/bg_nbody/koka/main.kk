// n-body, the Benchmarks Game's task: the Java program's symplectic integrator, printing the
// energy before and after n steps as `%.9f` lines (`show-fixed`, C's snprintf). Deviation: the
// bodies are a `vector` of immutable records updated with copy syntax and written in place under
// unique ownership, the analogue of Sakura's owned List.set. `main output <n>` writes the
// program's output; run(n) is the digest of the same bytes (bench/README.md), read as chars
// because the output is ASCII.
import std/num/float64
import std/os/env
import runner

struct body { x: float64; y: float64; z: float64; vx: float64; vy: float64; vz: float64; mass: float64 }

val solar-mass = 4.0 * pi * pi
val days = 365.24

fun planet(x: float64, y: float64, z: float64, vx: float64, vy: float64, vz: float64, m: float64): body {
  Body(x, y, z, vx * days, vy * days, vz * days, m * solar-mass)
}

fun system(): vector<body> {
  [
    Body(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, solar-mass),
    planet(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
           1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05,
           9.54791938424326609e-04),
    planet(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
           -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05,
           2.85885980666130812e-04),
    planet(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
           2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05,
           4.36624404335156298e-05),
    planet(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
           2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05,
           5.15138902046611451e-05),
  ].vector
}

fun offset-momentum(start: vector<body>): exn vector<body> {
  var px := 0.0
  var py := 0.0
  var pz := 0.0
  start.foreach(fn(b) {
    px := px + b.vx * b.mass
    py := py + b.vy * b.mass
    pz := pz + b.vz * b.mass
  })
  var bs := start
  val sun = bs[0]
  bs[0] := sun(vx = 0.0 - px / solar-mass, vy = 0.0 - py / solar-mass, vz = 0.0 - pz / solar-mass)
  bs
}

fun energy(bs: vector<body>): exn float64 {
  val n = bs.length
  var e := 0.0
  for(n, fn(i) {
    val b = bs[i]
    e := e + 0.5 * b.mass * (b.vx * b.vx + b.vy * b.vy + b.vz * b.vz)
    for(i + 1, n - 1, fn(j) {
      val c = bs[j]
      val dx = b.x - c.x
      val dy = b.y - c.y
      val dz = b.z - c.z
      e := e - b.mass * c.mass / sqrt(dx * dx + dy * dy + dz * dz)
    })
  })
  e
}

fun advance(start: vector<body>, dt: float64): <div,exn> vector<body> {
  val n = start.length
  var bs := start
  for(n, fn(i) {
    // The inner j loop threads the updated body i as an accumulator.
    fun pairs(j, b: body) {
      if (j >= n) then b else {
        val c = bs[j]
        val dx = b.x - c.x
        val dy = b.y - c.y
        val dz = b.z - c.z
        val d2 = dx * dx + dy * dy + dz * dz
        val mag = dt / (d2 * sqrt(d2))
        val b2 = b(vx = b.vx - dx * c.mass * mag, vy = b.vy - dy * c.mass * mag, vz = b.vz - dz * c.mass * mag)
        bs[j] := c(vx = c.vx + dx * b2.mass * mag, vy = c.vy + dy * b2.mass * mag, vz = c.vz + dz * b2.mass * mag)
        pairs(j + 1, b2)
      }
    }
    bs[i] := pairs(i + 1, bs[i])
  })
  bs.map(fn(b) { b(x = b.x + dt * b.vx, y = b.y + dt * b.vy, z = b.z + dt * b.vz) })
}

// What the Benchmarks Game program writes for n.
fun text(n: int): <div,exn> string {
  var bs := offset-momentum(system())
  val before = energy(bs).show-fixed(9)
  for(n, fn(_) { bs := advance(bs, 0.01) })
  before ++ "\n" ++ energy(bs).show-fixed(9) ++ "\n"
}

// h = (31·h + byte) mod 1,000,000,007 over every byte.
fun digest(s: string): int {
  var h := 0
  s.foreach(fn(c) { h := (h * 31 + c.int) % 1000000007 })
  h
}

val max-size = 50000000

fun main() {
  match get-args() {
    Cons("output", Cons(arg, Nil)) -> {
      val n = arg.parse-int.default(-1)
      if (n < 0 || n > max-size) then impossible("size outside resource bound")
      print(text(n))
    }
    _ -> bench-main(fn(n) { digest(text(n)) }, max-size)
  }
}
