// No deviation. Koka strings are UTF-8: `t.bytes.length` is the UTF-8 byte count (the C
// backend shares the string buffer) and `t.count` the Unicode scalar count.
import runner

val words = ["alpha", "naïve", "日本語", "🌸", "x1", "Grüße", "ok", "mañana"].vector

fun tokenizer(n: int): exn int {
  var s := ""
  for(n, fn(i){ s := s ++ words[i % 8] ++ (if i % 5 == 4 then ", " else " ") })
  var tokens := 0
  var utf8 := 0
  var scalars := 0
  var commas := 0
  s.split(" ").foreach(fn(t){
    if t.is-notempty then {
      tokens := tokens + 1
      utf8 := utf8 + t.bytes.length
      scalars := scalars + t.count
      if t.ends-with(",").is-just then commas := commas + 1
    }
  })
  tokens + 3 * utf8 + 7 * scalars + 11 * commas
}

fun main() { bench-main(tokenizer, 100000) }
