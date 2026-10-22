// mandelbrot, the Benchmarks Game's task: an n-by-n PBM P4 bitmap of [-1.5-i, 0.5+i], 50
// iterations, escape when |z|^2 > 4 (the reference C program, contributed by Greg Buchholz).
// The output is a `vector<int>` of byte values written in place under unique ownership, then
// digested (run) or converted to `bytes` and written raw (`main output n`). Deviations: the
// escape loop is a tail-recursive helper (same float operations in the same order), bits are
// packed with arithmetic (`acc * 2 + bit`) as in Sakura, and since Koka's console prints only
// strings, the raw write is a small inline C `fwrite` (`write-raw`).
import std/num/float64
import std/os/env
import runner

extern write-raw(^b: bytes): console ()
  c inline "({ kk_ssize_t len; const uint8_t* p = kk_bytes_buf_borrow(#1, &len, kk_context()); fwrite(p, 1, (size_t)len, stdout); fflush(stdout); kk_Unit; })"

// The reference's inner loop: whether c = cr + ci·i stays bounded for 50 iterations.
fun inside(cr: float64, ci: float64): div bool {
  fun iterate(i: int, zr: float64, zi: float64, tr: float64, ti: float64): div bool {
    if i < 50 && tr + ti <= 4.0 then {
      val zi2 = 2.0 * zr * zi + ci
      val zr2 = tr - ti + cr
      iterate(i + 1, zr2, zi2, zr2 * zr2, zi2 * zi2)
    } else tr + ti <= 4.0
  }
  iterate(0, 0.0, 0.0, 0.0, 0.0)
}

fun bitmap(n: int): <div,exn> vector<int> {
  val hbytes = ("P4\n" ++ n.show ++ " " ++ n.show ++ "\n").bytes
  var out := vector(hbytes.length + (n + 7) / 8 * n, 0)
  for(hbytes.length, fn(i) { out[i] := hbytes[i] })
  var pos := hbytes.length
  var acc := 0
  var bits := 0
  val nf = n.float64
  for(n, fn(y) {
    val ci = 2.0 * y.float64 / nf - 1.0
    for(n, fn(x) {
      val cr = 2.0 * x.float64 / nf - 1.5
      acc := acc * 2 + (if inside(cr, ci) then 1 else 0)
      bits := bits + 1
      if bits == 8 then {
        out[pos] := acc
        pos := pos + 1
        acc := 0
        bits := 0
      } elif x == n - 1 then {
        out[pos] := acc * 2.pow(8 - n % 8)
        pos := pos + 1
        acc := 0
        bits := 0
      }
    })
  })
  out
}

fun digest(n: int): <div,exn> int {
  var h := 0
  bitmap(n).foreach(fn(b) { h := (h * 31 + b) % 1000000007 })
  h
}

fun main() {
  val args = get-args()
  match args {
    Cons("output", Cons(size, Nil)) -> write-raw(bitmap(size.parse-int.default(0)).bytes)
    _ -> bench-main(digest, 16000)
  }
}
