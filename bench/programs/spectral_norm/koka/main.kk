// Spectral norm with float64 in the Sakura operation order. Deviation: `vector`s built with
// vector-init replace Sakura's persistent List.push; the result rounds half away from zero.
import std/num/float64
import runner

fun a(i: int, j: int): float64 { 1.0 / ((i + j) * (i + j + 1) / 2 + i + 1).float64 }

fun mul-av(v: vector<float64>, n: int): exn vector<float64> {
  vector-init(n, fn(i) {
    var sum := 0.0
    for(n, fn(j) { sum := sum + a(i, j) * v[j] })
    sum
  })
}

fun mul-atv(v: vector<float64>, n: int): exn vector<float64> {
  vector-init(n, fn(i) {
    var sum := 0.0
    for(n, fn(j) { sum := sum + a(j, i) * v[j] })
    sum
  })
}

fun mul-atav(v: vector<float64>, n: int): exn vector<float64> { mul-atv(mul-av(v, n), n) }

// Sakura's Float.round: half away from zero (Koka's `round` is half to even).
fun round-away(x: float64): int {
  val t = x.truncate
  (if (abs(x - t) >= 0.5) then (if (x < 0.0) then t - 1.0 else t + 1.0) else t).int
}

fun spectral-norm(n: int): exn int {
  if (n == 0) then return 0
  var u := vector(n, 1.0)
  var v := u
  for(10, fn(_) {
    v := mul-atav(u, n)
    u := mul-atav(v, n)
  })
  var vbv := 0.0
  var vv := 0.0
  for(n, fn(m) {
    vbv := vbv + u[m] * v[m]
    vv := vv + v[m] * v[m]
  })
  round-away(sqrt(vbv / vv) * 1.0e9)
}

fun main() { bench-main(spectral-norm, 400) }
