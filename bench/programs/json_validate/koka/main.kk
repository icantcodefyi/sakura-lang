// Deviations: Koka 3.2.9 ships no JSON library, so a small recursive-descent parser is defined
// here; numbers with a fraction or exponent parse but count as non-integers, and lone UTF-16
// surrogate escapes are rejected (Koka strings hold Unicode scalars only).
import runner

type json
  JNull
  JBool(b: bool)
  JInt(i: int)
  JFrac
  JStr(s: string)
  JArr(items: list<json>)
  JObj(fields: list<(string, json)>) // most recent first, so lookup sees the last duplicate

fun fail(): exn a { throw("invalid JSON") }

fun next-char(s: sslice): exn (char, sslice)
  match s.next
    Just(p) -> p
    Nothing -> fail()

fun skip-ws(s: sslice): div sslice
  match s.next
    Just((c, rest)) | c == ' ' || c == '\t' || c == '\n' || c == '\r' -> skip-ws(rest)
    _ -> s

fun expect(s: sslice, c: char): <div,exn> sslice
  val (d, rest) = s.skip-ws.next-char
  if d == c then rest else fail()

fun keyword(s: sslice, word: list<char>): exn sslice
  match word
    Nil -> s
    Cons(w, ws) ->
      val (c, rest) = s.next-char
      if c == w then keyword(rest, ws) else fail()

fun hex4(s: sslice): exn (int, sslice)
  fun digit(c: char): exn int
    if c.is-digit then c.int - '0'.int
    elif c >= 'a' && c <= 'f' then c.int - 'a'.int + 10
    elif c >= 'A' && c <= 'F' then c.int - 'A'.int + 10
    else fail()
  val (a, s1) = s.next-char
  val (b, s2) = s1.next-char
  val (c, s3) = s2.next-char
  val (d, s4) = s3.next-char
  (((digit(a) * 16 + digit(b)) * 16 + digit(c)) * 16 + digit(d), s4)

fun unicode-escape(s: sslice): exn (char, sslice)
  val (hi, s1) = hex4(s)
  if hi >= 0xDC00 && hi <= 0xDFFF then fail()
  elif hi >= 0xD800 && hi <= 0xDBFF then
    val (lo, s2) = hex4(keyword(s1, ['\\', 'u']))
    if lo < 0xDC00 || lo > 0xDFFF then fail()
    ((0x10000 + (hi - 0xD800) * 0x400 + (lo - 0xDC00)).char, s2)
  else (hi.char, s1)

fun parse-string(s: sslice, acc: list<char>): <div,exn> (string, sslice)
  val (c, rest) = s.next-char
  if c == '"' then (acc.reverse.string, rest)
  elif c == '\\' then
    val (e, r) = rest.next-char
    match e
      '"' -> parse-string(r, Cons('"', acc))
      '\\' -> parse-string(r, Cons('\\', acc))
      '/' -> parse-string(r, Cons('/', acc))
      'b' -> parse-string(r, Cons(8.char, acc))
      'f' -> parse-string(r, Cons(12.char, acc))
      'n' -> parse-string(r, Cons('\n', acc))
      'r' -> parse-string(r, Cons('\r', acc))
      't' -> parse-string(r, Cons('\t', acc))
      'u' ->
        val (u, r2) = unicode-escape(r)
        parse-string(r2, Cons(u, acc))
      _ -> fail()
  elif c.int < 0x20 then fail()
  else parse-string(rest, Cons(c, acc))

fun digits(s: sslice, acc: int, count: int): div (int, int, sslice)
  match s.next
    Just((c, rest)) | c.is-digit -> digits(rest, acc * 10 + (c.int - '0'.int), count + 1)
    _ -> (acc, count, s)

fun parse-number(s: sslice): <div,exn> (json, sslice)
  val (negative, s1) = match s.next
    Just(('-', rest)) -> (True, rest)
    _ -> (False, s)
  val (magnitude, count, s2) = digits(s1, 0, 0)
  if count == 0 || (count > 1 && s1.next.map(fst) == Just('0')) then fail()
  val (fraction, s3) = match s2.next
    Just(('.', rest)) ->
      val (_, k, r) = digits(rest, 0, 0)
      if k == 0 then fail() else (True, r)
    _ -> (False, s2)
  val (exponent, s4) = match s3.next
    Just((e, rest)) | e == 'e' || e == 'E' ->
      val signed = match rest.next
        Just((sign, r)) | sign == '+' || sign == '-' -> r
        _ -> rest
      val (_, k, r) = digits(signed, 0, 0)
      if k == 0 then fail() else (True, r)
    _ -> (False, s3)
  if fraction || exponent then (JFrac, s4)
  else (JInt(if negative then 0 - magnitude else magnitude), s2)

fun parse-members(s: sslice, fields: list<(string, json)>): <div,exn> (json, sslice)
  val (key, r1) = parse-string(expect(s, '"'), Nil)
  val (value, r2) = parse-value(expect(r1, ':'))
  val fs = Cons((key, value), fields)
  val (c, r3) = r2.skip-ws.next-char
  if c == ',' then parse-members(r3, fs)
  elif c == '}' then (JObj(fs), r3)
  else fail()

fun parse-items(s: sslice, items: list<json>): <div,exn> (json, sslice)
  val (value, r1) = parse-value(s)
  val xs = Cons(value, items)
  val (c, r2) = r1.skip-ws.next-char
  if c == ',' then parse-items(r2, xs)
  elif c == ']' then (JArr(xs.reverse), r2)
  else fail()

fun parse-value(s0: sslice): <div,exn> (json, sslice)
  val s = s0.skip-ws
  val (c, rest) = s.next-char
  match c
    '{' -> match rest.skip-ws.next
      Just(('}', r)) -> (JObj(Nil), r)
      _ -> parse-members(rest, Nil)
    '[' -> match rest.skip-ws.next
      Just((']', r)) -> (JArr(Nil), r)
      _ -> parse-items(rest, Nil)
    '"' ->
      val (str, r) = parse-string(rest, Nil)
      (JStr(str), r)
    't' -> (JBool(True), keyword(rest, "rue".list))
    'f' -> (JBool(False), keyword(rest, "alse".list))
    'n' -> (JNull, keyword(rest, "ull".list))
    _ -> if c == '-' || c.is-digit then parse-number(s) else fail()

fun parse(text: string): <div,exn> json
  val (value, rest) = parse-value(text.slice)
  if rest.skip-ws.is-empty then value else fail()

struct user { id: int; name: string; age: int }

// The refinements of the Sakura `User`: safe-integer id, name of 1..32 UTF-8 bytes, age 0..150.
fun decode(text: string): div maybe<user>
  match try({ parse(text) })
    Error -> Nothing
    Ok(JObj(fields)) ->
      fun field(key: string) fields.lookup(fn(k) k == key)
      match (field("id"), field("name"), field("age"))
        (Just(JInt(id)), Just(JStr(name)), Just(JInt(age)))
          | id.abs <= 9007199254740991 && name.bytes.length > 0 && name.bytes.length <= 32
            && age >= 0 && age <= 150 -> Just(User(id, name, age))
        _ -> Nothing
    _ -> Nothing

fun escape(c: char): string
  match c
    '"' -> "\\\""
    '\\' -> "\\\\"
    _ | c.int == 0x08 -> "\\b"
    '\n' -> "\\n"
    '\r' -> "\\r"
    '\t' -> "\\t"
    _ | c.int == 0x0C -> "\\f"
      | c.int < 0x20 -> "\\u" ++ c.int.show-hex(4, False, "")
      | True -> c.string

fun encode(u: user): string
  val User(id, name, age) = u
  "{\"id\":" ++ id.show ++ ",\"name\":\"" ++ name.list.map(escape).join ++ "\",\"age\":" ++ age.show ++ "}"

fun line(i: int): string
  val name = if i % 17 == 0 then "" else "user" ++ i.show
  val age = if i % 13 == 0 then 200 else i % 90
  "{\"id\": " ++ i.show ++ ", \"name\": \"" ++ name ++ "\", \"age\": " ++ age.show ++ "}"

fun json-validate(n: int): div int
  val text = list(0, n - 1).map(line).join("\n")
  var valid := 0
  var encoded := 0
  var ids := 0
  var invalid := 0
  text.split("\n").foreach fn(l)
    if l.is-notempty then
      match decode(l)
        Just(u) ->
          valid := valid + 1
          encoded := encoded + u.encode.bytes.length
          ids := ids + u.user/id
        Nothing -> invalid := invalid + 1
  valid + encoded + ids + 1000 * invalid

fun main() { bench-main(json-validate, 30000) }
