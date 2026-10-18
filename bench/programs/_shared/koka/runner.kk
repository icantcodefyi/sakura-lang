module runner
import std/os/env
import std/core/debug
import std/time/timer
import std/time/duration
import std/num/float64

pub fun bench-main(workload: int -> <div,exn,ndet,console> int, maximum: int): <console,ndet,exn,div> () {
  val args = get-args()
  val mode = args.head("")
  val rest = args.tail
  val stable = rest.any(fn(x){ x == "--stable" })
  val sizes = rest.filter(fn(x){ x != "--stable" }).map(fn(x){
    val n = x.parse-int().default(-1)
    if (n < 0 || n > maximum) then impossible("size outside oracle resource bound")
    n
  })
  if (sizes.is-empty) then impossible("missing size")
  if (mode == "values") then {
    val pairs = sizes.map(fn(n){ "\"" ++ n.show ++ "\":\"" ++ workload(n).show ++ "\"" })
    println("{\"values\":{" ++ pairs.join(",") ++ "}}")
  }
  else {
    if (mode != "measure" || sizes.length != 1) then impossible("measure <n> [--stable]")
    val n = sizes.head(0)
    var sink := 0
    val start = ticks()
    val warmup = if stable then 2000000000 else 250000000
    while(fn(){ (ticks() - start).nano-seconds < warmup }, fn(){ sink := workload(n) })
    var count := 1
    var calibrated := False
    while(fn(){ !calibrated }, fn(){
      val t = ticks()
      for(count, fn(_){ sink := workload(n) })
      if ((ticks() - t).nano-seconds >= 30000000 || count >= 1048576)
        then calibrated := True
        else count := count * 2
    })
    for(9, fn(_){
      val t = ticks()
      for(count, fn(_){ sink := workload(n) })
      val ms = (ticks() - t).nano-seconds.float64 / 1000000.0 / count.float64
      println("{\"n\":" ++ n.show ++ ",\"ms\":" ++ ms.show ++ ",\"checksum\":\"" ++ sink.show ++ "\"}")
    })
  }
}
