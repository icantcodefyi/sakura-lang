# Vow standard library index (v0)

Every public std signature, one module per file in `compiler/std/`. The signature listing
below the "Signatures" heading is extracted from the source; regenerate it after editing a
module (until `vow doc --index` exists, F regenerates it by hand from the source). The design
reference is `docs/design.md` §10; deviations are listed at the end and filed as
`design-question` entries in `compiler/REQUESTS.md`.

Status: every module checks clean and its `test` blocks pass under `vow test --std`.
Modules marked M3 type-check but need features from design §14.3 M3 to run.

## Prelude

In scope in every module without an import (design §9, with the additions marked +):

| Kind | Names |
|---|---|
| Compiler-known types | `Unit Bool Int Float String Bytes Never List Map Set Range Handler Secret Untrusted` |
| Std types | `Option Result` · + `Duration Instant Uuid Json Level` (prelude effects mention them; §15.1 and §15.4 use `Instant.format_iso`, `Level.Warn`, `Json.String` unimported) |
| Constructors | `Some None Ok Err` (other variants are used qualified: `Level.Warn`, `Json.String(s)`) |
| Effects | `Fail` (compiler-known) `Console Clock Random Env Log Task` · + `Test` |
| Functions | `show` · + `expect expect_eq expect_fail expect_fails_with expect_defect expect_output expect_snapshot` (they need `Test`, which only `test`/`property` bodies have; elsewhere they are `V3001`) |
| Tags (M3) | `sql html url shell` |

Not in the prelude, import them: `import std/retry.{Policy, retry}`, `import std/task.{Timeout, timeout}`,
`import std/gen` (`gen.int(0, 9)`), `import std/graph.{Graph, NodeId}`, and the M3 modules.

Home modules (design §4.9): `T.f(…)` and `x.f(…)` with `x: T` call `f` in the module that
declares `T`; for compiler-known types it is `std/<lowercase name>` (`List` → `std/list`). An
effect's home module is the one declaring it, and `Eff.f(…)` calls a home-module function
when `f` is not an operation (`Console.captured()`, `Clock.fixed(t)`, `Random.seeded(1)`).

## Conventions

- **Externals.** Functions marked `// @external` are bodyless and bound by
  `@external(js, "vow_runtime", "<module>_<fn>")` to `compiler/runtime/js`. Everything else is
  Vow. Externals are part of the trusted base (§6.6).
- **Defects, not failures,** for programming errors: `Option.expect` (`D030`),
  `String.slice` off a character boundary (`D031`), checked arithmetic (`D001`–`D003`), and
  `requires` clauses (`D020`, caller blamed).
- **Indexing never defects:** `List.get`/`xs[i]`, `Bytes.get`, `Map.get` return `Option`;
  `List.slice` and `String.slice` clamp their bounds.
- **Insertion order** for `Map`/`Set` iteration on every backend.
- **Time** is whole milliseconds (`Duration`, `Instant` since the Unix epoch, UTC).
- **Test defaults** (§5.8): `Clock.fixed(Instant.from_unix_millis(0))`, `Random.seeded(seed)`,
  `Env.of(Map.empty())`, `Console.captured()`, `Log.captured()`. Expectations record a
  failure and let the test continue; the runner fails the test at the end.
- **Show format** assumed by the std tests and the example outputs (pending a spec, see the
  design questions): valid Vow expression syntax — `[1, 2]`, `(1, "a")`, `Some("x")`,
  `User { id: 1, name: "a" }`, `Map.of([("a", 1)])`, `Set.of([1])`, floats always with a
  `.` or exponent (`2.0`); strings are quoted inside containers and raw at the top level of
  an interpolation.

## Module dependencies (non-test code)

Acyclic. Test blocks additionally use `std/test`, which depends only on `std/core`.

```
core  unit  option  result  set  int  float  duration       (no std dependencies)
list → set              map → list            string → list       bytes → list
json → map              uuid → string, list   instant → duration, string, list, int
test → core             console → test        clock → duration, instant, int
random → uuid, int, string, bytes, list       env → map, list
log → json, test, list, core                  gen → test, random, list, int, string
retry → duration, int   (+ Clock, Random)     task → duration, list (+ Clock)   [M3]
graph → map, list       untrusted               path → string, list
url → string, list      cli [M3]                fs → path, untrusted, map [M3]
request → url, json [M3]                       response → json, untrusted [M3]
http → request, response, url, duration [M3]  budget [M3]      prompt [M3]
llm → budget, core, http, prompt, task, untrusted [M3]          sql → fs, path [M3]
```

M3 modules type-check today. Their test handlers written in Vow (`Http.scripted`,
`Budget.limits`, `Cli.of`, `Fs.memory`, `Llm.scripted`) run as soon as the runtime has the
externals they touch; their host handlers and `Schema` codecs are runtime externals (M3).

## Planned, not yet in source (design §10, milestone M3)

`std/router`, `std/serve`, `std/html`, `std/tool`, `std/agent`, `std/decimal`, `std/secret`
(`Secret.expose`), `Untrusted.validate/check/trust` (need the compiler-known `Trust`),
`Fs.with_file`/`File`, `Http.scoped`/`recording`, `Llm.stream`/`replay`, `Sql.postgres`, the
`url"…"`/`html"…"` tags, `List.par_map` (written as `std/task.par_map` until `Task` runs),
`Env.host(scope)`, and `pub opaque` on `Uuid`, `Path`, `Prompt` and `Query` (the parser does
not accept `opaque` yet). Their signatures are the ones in design §10.2–§10.7.

## Signatures

### `std/core`

```vow
fn show<A: Show>(a: A) -> String    // @external
type Violation = { type_name: String, clause: String, value_summary: String }
```

### `std/unit`

```vow
fn ignore<A>(a: A) -> Unit
```

### `std/option`

```vow
type Option<A> = None | Some(A)
fn map<A, B, e>(o: Option<A>, f: fn(A) -> B ! e) -> Option<B> ! e
fn unwrap_or<A>(o: Option<A>, default: A) -> A
fn or_fail<A, E>(o: Option<A>, err: E) -> A ! {Fail<E>}
fn expect<A>(o: Option<A>, msg: String) -> A    // @external
fn is_some<A>(o: Option<A>) -> Bool    // @total
```

### `std/result`

```vow
type Result<A, E> = Ok(A) | Err(E)
fn map<A, B, E, e>(r: Result<A, E>, f: fn(A) -> B ! e) -> Result<B, E> ! e
fn map_err<A, E, F>(r: Result<A, E>, f: fn(E) -> F) -> Result<A, F>
fn unwrap_or<A, E>(r: Result<A, E>, default: A) -> A
fn raise<A, E>(r: Result<A, E>) -> A ! {Fail<E>}
fn is_ok<A, E>(r: Result<A, E>) -> Bool    // @total
fn is_err<A, E>(r: Result<A, E>) -> Bool    // @total
```

### `std/list`

```vow
fn len<A>(xs: List<A>) -> Int    // @external, @total
fn get<A>(xs: List<A>, i: Int) -> Option<A>    // @external, @total
fn push<A>(xs: List<A>, x: A) -> List<A>    // @external
fn slice<A>(xs: List<A>, start: Int, end: Int) -> List<A>    // @external
fn empty<A>() -> List<A>
fn concat<A>(a: List<A>, b: List<A>) -> List<A>
fn map<A, B, e>(xs: List<A>, f: fn(A) -> B ! e) -> List<B> ! e
fn filter<A, e>(xs: List<A>, f: fn(A) -> Bool ! e) -> List<A> ! e
fn filter_map<A, B, e>(xs: List<A>, f: fn(A) -> Option<B> ! e) -> List<B> ! e
fn flat_map<A, B, e>(xs: List<A>, f: fn(A) -> List<B> ! e) -> List<B> ! e
fn fold<A, B, e>(xs: List<A>, init: B, f: fn(B, A) -> B ! e) -> B ! e
fn find<A, e>(xs: List<A>, f: fn(A) -> Bool ! e) -> Option<A> ! e
fn all<A, e>(xs: List<A>, f: fn(A) -> Bool ! e) -> Bool ! e    // @total
fn any<A, e>(xs: List<A>, f: fn(A) -> Bool ! e) -> Bool ! e    // @total
fn sort<A: Ord>(xs: List<A>) -> List<A>
fn sort_by<A, K: Ord>(xs: List<A>, key: fn(A) -> K) -> List<A>
fn zip<A, B>(a: List<A>, b: List<B>) -> List<(A, B)>
fn enumerate<A>(xs: List<A>) -> List<(Int, A)>
fn reverse<A>(xs: List<A>) -> List<A>
fn join(xs: List<String>, sep: String) -> String
fn sum(xs: List<Int>) -> Int
fn first<A>(xs: List<A>) -> Option<A>    // @total
fn last<A>(xs: List<A>) -> Option<A>    // @total
fn contains<A: Eq>(xs: List<A>, x: A) -> Bool    // @total
fn unique<A: Hash>(xs: List<A>) -> List<A>
fn range(start: Int, end: Int) -> List<Int>
```

### `std/map`

```vow
fn empty<K: Hash, V>() -> Map<K, V>    // @external
fn get<K: Hash, V>(m: Map<K, V>, k: K) -> Option<V>    // @external, @total
fn insert<K: Hash, V>(m: Map<K, V>, k: K, v: V) -> Map<K, V>    // @external
fn remove<K: Hash, V>(m: Map<K, V>, k: K) -> Map<K, V>    // @external
fn len<K: Hash, V>(m: Map<K, V>) -> Int    // @external, @total
fn entries<K: Hash, V>(m: Map<K, V>) -> List<(K, V)>    // @external
fn of<K: Hash, V>(pairs: List<(K, V)>) -> Map<K, V>
fn contains<K: Hash, V>(m: Map<K, V>, k: K) -> Bool    // @total
fn update<K: Hash, V, e>(m: Map<K, V>, k: K, f: fn(Option<V>) -> V ! e) -> Map<K, V> ! e
fn keys<K: Hash, V>(m: Map<K, V>) -> List<K>
fn values<K: Hash, V>(m: Map<K, V>) -> List<V>
```

### `std/set`

```vow
fn empty<A: Hash>() -> Set<A>    // @external
fn insert<A: Hash>(s: Set<A>, x: A) -> Set<A>    // @external
fn contains<A: Hash>(s: Set<A>, x: A) -> Bool    // @external, @total
fn len<A: Hash>(s: Set<A>) -> Int    // @external, @total
fn to_list<A: Hash>(s: Set<A>) -> List<A>    // @external
fn of<A: Hash>(xs: List<A>) -> Set<A>
```

### `std/string`

```vow
error Utf8Error(offset: Int)
fn len(s: String) -> Int    // @external, @total
fn chars(s: String) -> List<String>    // @external
fn graphemes(s: String) -> List<String>    // @external
fn split(s: String, sep: String) -> List<String>    // @external
fn trim(s: String) -> String    // @external
fn starts_with(s: String, prefix: String) -> Bool    // @external, @total
fn ends_with(s: String, suffix: String) -> Bool    // @external, @total
fn contains(s: String, needle: String) -> Bool    // @external, @total
fn replace(s: String, from: String, to: String) -> String    // @external
fn to_lower(s: String) -> String    // @external
fn to_upper(s: String) -> String    // @external
fn parse_int(s: String) -> Option<Int>    // @external
fn parse_float(s: String) -> Option<Float>    // @external
fn to_bytes(s: String) -> Bytes    // @external
fn from_bytes(b: Bytes) -> Result<String, Utf8Error>    // @external
fn slice(s: String, start: Int, end: Int) -> String    // @external
fn repeat(s: String, n: Int) -> String requires n >= 0
fn pad_left(s: String, width: Int, fill: String) -> String requires len(fill) > 0
```

### `std/int`

```vow
fn to_float(i: Int) -> Float    // @external
fn to_string(i: Int) -> String    // @external
fn checked_add(a: Int, b: Int) -> Option<Int>    // @external
fn wrapping_add(a: Int, b: Int) -> Int    // @external
fn abs(i: Int) -> Int    // @total
fn min(a: Int, b: Int) -> Int    // @total
fn max(a: Int, b: Int) -> Int    // @total
fn clamp(i: Int, lo: Int, hi: Int) -> Int requires lo <= hi
fn pow(base: Int, exp: Int) -> Int requires exp >= 0
```

### `std/float`

```vow
fn round(f: Float) -> Int    // @external
fn to_int_checked(f: Float) -> Option<Int>    // @external
fn sqrt(f: Float) -> Float    // @external
fn format(f: Float, decimals: Int) -> String requires decimals >= 0    // @external
fn is_nan(f: Float) -> Bool
```

### `std/bytes`

```vow
fn len(b: Bytes) -> Int    // @external, @total
fn get(b: Bytes, i: Int) -> Option<Int>    // @external, @total
fn from_list(xs: List<Int>) -> Bytes requires List.all(xs, fn(x) { x >= 0 && x <= 255 })    // @external
```

### `std/duration`

```vow
type Duration = Duration(Int)
fn millis(n: Int) -> Duration
fn seconds(n: Int) -> Duration
fn minutes(n: Int) -> Duration
fn to_millis(d: Duration) -> Int
```

### `std/instant`

```vow
type Instant = Instant(Int)
error TimeError(input: String)
fn from_unix_millis(ms: Int) -> Instant
fn to_unix_millis(i: Instant) -> Int
fn add(i: Instant, d: Duration) -> Instant
fn diff(a: Instant, b: Instant) -> Duration
fn format_iso(i: Instant) -> String
fn parse(text: String) -> Result<Instant, TimeError>
```

### `std/uuid`

```vow
type Uuid = Uuid(String)
error UuidError(input: String)
fn to_string(u: Uuid) -> String
fn parse(text: String) -> Result<Uuid, UuidError>
```

### `std/json`

```vow
type Json = Null | Bool(Bool) | Number(Float) | String(String) | Array(List<Json>) | Object(Map<String, Json>)
error DecodeError(path: String, message: String)
fn encode<T: Schema>(value: T) -> String    // @external
fn decode<T: Schema>(text: String) -> Result<T, DecodeError>    // @external
fn parse(text: String) -> Result<Json, DecodeError>    // @external
fn get(j: Json, key: String) -> Option<Json>
fn get_string(j: Json, key: String) -> Option<String>
```

### `std/test`

```vow
effect Test {
  fn record_failure(message: String) -> Unit
  fn failures() -> Int
  fn output(line: String) -> Unit
  fn output_lines() -> List<String>
  fn snapshot(name: String, rendered: String) -> Unit
}
fn expect(cond: Bool) -> Unit ! {Test}
fn expect_eq<A: Eq + Show>(actual: A, expected: A) -> Unit ! {Test}
fn expect_fail<T, E: Eq + Show, e>(body: fn() -> T ! {Fail<E> | e}, err: E) -> Unit ! {Test | e}
fn expect_fails_with<T, E: Show, e>(body: fn() -> T ! {Fail<E> | e}, pred: fn(E) -> Bool) -> Unit ! {Test | e}
fn expect_defect<T, e>(body: fn() -> T ! e) -> Unit ! {Test | e}    // @external
fn expect_output(lines: List<String>) -> Unit ! {Test}
fn expect_snapshot<A: Show>(name: String, value: A) -> Unit ! {Test}
```

### `std/console`

```vow
effect Console {
  fn print(line: String) -> Unit
  fn read_line() -> Option<String>
}
fn host() -> Handler<Console>    // @external
fn captured() -> Handler<Console, {Test}>
```

### `std/clock`

```vow
effect Clock {
  fn now() -> Instant
  fn monotonic() -> Duration
  fn sleep(d: Duration) -> Unit
}
fn host() -> Handler<Clock>    // @external
fn fixed(at: Instant) -> Handler<Clock>
fn virtual() -> Handler<Clock>
```

### `std/random`

```vow
effect Random {
  fn int(lo: Int, hi: Int) -> Int requires lo <= hi
  fn float() -> Float
  fn bytes(n: Int) -> Bytes requires n >= 0
  fn uuid() -> Uuid
}
fn host() -> Handler<Random>    // @external
fn seeded(seed: Int) -> Handler<Random>
```

### `std/env`

```vow
effect Env {
  fn get(key: String) -> Option<String>
  fn secret(key: String) -> Option<Secret<String>>
}
fn host() -> Handler<Env>
fn of(vars: Map<String, String>) -> Handler<Env>
fn only(keys: List<String>) -> Handler<Env, {Env}>
```

### `std/log`

```vow
type Level = Debug | Info | Warn | Error
effect Log {
  fn event(level: Level, message: String, fields: List<(String, Json)>) -> Unit
}
fn stderr() -> Handler<Log>    // @external
fn host() -> Handler<Log>
fn json() -> Handler<Log>    // @external
fn captured() -> Handler<Log, {Test}>
```

### `std/gen`

```vow
type Gen<A> = { sample: fn(Int) -> A ! {Random} }
fn int(lo: Int, hi: Int) -> Gen<Int> requires lo <= hi
fn bool() -> Gen<Bool>
fn one_of<A>(choices: List<A>) -> Gen<A> requires List.len(choices) > 0
fn list<A>(item: Gen<A>, max_len: Int) -> Gen<List<A>> requires max_len >= 0
fn string(max_len: Int) -> Gen<String> requires max_len >= 0
fn map<A, B>(g: Gen<A>, f: fn(A) -> B) -> Gen<B>
fn pair<A, B>(a: Gen<A>, b: Gen<B>) -> Gen<(A, B)>
fn for_all<A: Show, e>(g: Gen<A>, runs: Int, prop: fn(A) -> Unit ! {Test | e}) -> Unit ! {Random, Test | e}
```

### `std/retry`

```vow
type Policy<E> = { retry_on: fn(E) -> Bool, max_attempts: Int, backoff: Duration, jitter: Bool, deadline: Option<Duration> }
fn policy<E>(retry_on: fn(E) -> Bool, max_attempts: Int) -> Policy<E> requires max_attempts >= 1
fn retry<T, E, e>(p: Policy<E>, action: fn() -> T ! {Fail<E> | e}) -> T ! {Clock, Random, Fail<E> | e}
```

### `std/task`

```vow
type Scope
effect Task {
  fn scope<T, e>(body: fn(Scope) -> T ! e) -> T ! e
  fn spawn<T, e>(s: Scope, body: fn() -> T ! e) -> Fiber<T> ! e
  fn join<T>(f: Fiber<T>) -> T
  fn all<T, e>(bodies: List<fn() -> T ! e>) -> List<T> ! e
  fn race<T, e>(bodies: List<fn() -> T ! e>) -> T ! e
  fn yield_now() -> Unit
}
fn host() -> Handler<Task>    // @external
fn deterministic(seed: Int) -> Handler<Task>    // @external
error Timeout(limit: Duration)
fn timeout<T, e>(limit: Duration, action: fn() -> T ! e) -> T ! {Clock, Task, Fail<Timeout> | e}
fn par_map<A, B, e>(xs: List<A>, f: fn(A) -> B ! e) -> List<B> ! {Task | e}
type Fiber<T>
```

### `std/graph`

```vow
type NodeId = NodeId(Int)
type Graph<N> = { nodes: Map<Int, N>, edges: Map<Int, List<NodeId>>, next: Int }
fn empty<N>() -> Graph<N>
fn insert<N>(g: Graph<N>, n: N) -> (Graph<N>, NodeId)
fn get<N>(g: Graph<N>, id: NodeId) -> Option<N>    // @total
fn connect<N>(g: Graph<N>, a: NodeId, b: NodeId) -> Graph<N> requires get(g, a).is_some() && get(g, b).is_some()
fn neighbors<N>(g: Graph<N>, id: NodeId) -> List<NodeId>
fn len<N>(g: Graph<N>) -> Int
```

### `std/untrusted`

```vow
fn of<T>(value: T) -> Untrusted<T>    // @external
```

### `std/path`

```vow
type Path = Path(List<String>)
fn is_relative(s: String) -> Bool    // @total
fn of(s: String) -> Path requires is_relative(s)
fn join(p: Path, rest: String) -> Path requires is_relative(rest)
fn to_string(p: Path) -> String
```

### `std/fs`

```vow
type Dir
type Entry = { name: String, is_dir: Bool }
error FsError(path: String, reason: String)
effect Fs {
  fn root() -> Dir
  fn read(path: Path) -> Bytes ! {Fail<FsError>}
  fn read_text(path: Path) -> Untrusted<String> ! {Fail<FsError>}
  fn write(path: Path, data: Bytes) -> Unit ! {Fail<FsError>}
  fn list(dir: Path) -> List<Entry> ! {Fail<FsError>}
  fn exists(path: Path) -> Bool
}
fn host() -> Handler<Fs>
fn rooted(dir: Dir) -> Handler<Fs>
fn subdir(d: Dir, name: String) -> Dir ! {Fail<FsError>}
fn memory(files: List<(Path, Bytes)>) -> Handler<Fs>
```

### `std/cli`

```vow
effect Cli {
  fn args() -> List<String>
  fn exit(code: Int) -> Never requires code >= 0 && code <= 255
}
error CliError(message: String)
fn host() -> Handler<Cli>
fn of(args: List<String>) -> Handler<Cli>
fn parse<T: Schema>(args: List<String>) -> T ! {Fail<CliError>}    // @external
```

### `std/url`

```vow
type Url = String where is_valid(it)
fn is_valid(s: String) -> Bool    // @total
fn host(u: Url) -> Option<String>
fn path(u: Url) -> String
fn query_param(u: Url, name: String) -> Option<String>
```

### `std/request`

```vow
type Method = Get | Head | Post | Put | Patch | Delete
type Request = { method: Method, url: Url, headers: List<(String, String)>, body: String, bearer: Option<Secret<String>> }
error BadRequest(detail: String)
fn of(method: Method, url: Url) -> Request
fn post(path: String, body: String) -> Request
fn post_json<T: Schema>(url: Url, body: T) -> Request
fn json<T: Schema>(req: Request, limit: Int) -> T ! {Fail<BadRequest>}
fn param(req: Request, name: String) -> Option<String>
fn header(req: Request, name: String) -> Option<String>
```

### `std/response`

```vow
type Status = Int where it >= 100 && it <= 599
type Response = { status: Status, headers: List<(String, String)>, body: String }
error HttpError(status: Option<Status>, reason: String)
fn status(code: Int, body: String) -> Response requires code >= 100 && code <= 599
fn ok_json<T: Schema>(value: T) -> Response
fn json<T: Schema>(r: Response, limit: Int) -> T ! {Fail<HttpError | DecodeError>}
fn text(r: Response, limit: Int) -> Untrusted<String> ! {Fail<HttpError>}
```

### `std/http`

```vow
type Matcher = Any | Path(String) | Method(Method)
error RateLimited(retry_after: Option<Duration>)
effect Http {
  fn send(req: Request) -> Response ! {Fail<HttpError>}
}
fn get(url: Url) -> Response ! {Http, Fail<HttpError | RateLimited>}
fn post_json<T: Schema>(url: Url, body: T) -> Response ! {Http, Fail<HttpError | RateLimited>}
fn bearer(req: Request, token: Secret<String>) -> Request
fn host() -> Handler<Http>
fn scripted(script: List<(Matcher, Response)>) -> Handler<Http>
```

### `std/budget`

```vow
type Limits = { tokens: Option<Int>, usd: Option<Float>, calls: Option<Int> }
type Cost = { tokens: Int, usd: Float, calls: Int }
type Reservation = { estimate: Cost }
type BudgetKind = Tokens | Usd | Calls
error BudgetExceeded(kind: BudgetKind)
effect Budget {
  fn reserve(est: Cost) -> Reservation ! {Fail<BudgetExceeded>}
  fn settle(r: Reservation, actual: Cost) -> Unit
}
fn limits(l: Limits) -> Handler<Budget>
fn unlimited() -> Handler<Budget>
fn zero() -> Cost
```

### `std/prompt`

```vow
type PromptPart = Text(String) | Untrusted(Untrusted<String>) | Image(Bytes)
type Prompt = Prompt(List<PromptPart>)
fn of(parts: List<PromptPart>) -> Prompt
fn text(s: String) -> Prompt
fn parts(p: Prompt) -> List<PromptPart>
```

### `std/llm`

```vow
type Model = Model(String)
type Request<T> = { system: String, user: Prompt, model: Model, max_tokens: Int, temperature: Float }
error Refused
error InvalidOutput(violations: List<Violation>)
error Truncated
error ProviderError(status: Int)
type LlmError = Refused | InvalidOutput | Truncated | RateLimited | Timeout | ProviderError
effect Llm {
  fn generate<T: Schema>(req: Request<T>) -> T ! {Fail<LlmError>}
  fn generate_text(req: Request<String>) -> Untrusted<String> ! {Fail<LlmError>}
}
fn request<T: Schema>(system: String, user: Prompt) -> Request<T>
fn of(name: String) -> Model
fn default() -> Model
fn anthropic(key: Secret<String>, model: Model) -> Handler<Llm, {Http, Budget}>    // @external
fn scripted(responses: List<String>) -> Handler<Llm>
```

### `std/sql`

```vow
type SqlValue = Null | Int(Int) | Float(Float) | Text(String) | Blob(Bytes)
type Query<R> = { text: String, params: List<SqlValue> }
error SqlError(message: String)
effect Sql {
  fn query<R: Schema>(q: Query<R>) -> List<R> ! {Fail<SqlError>}
  fn execute(q: Query<Unit>) -> Int ! {Fail<SqlError>}
  fn transaction<T, e>(body: fn() -> T ! {Sql | e}) -> T ! {Fail<SqlError> | e}
}
fn template<R>(parts: List<String>, args: List<SqlValue>) -> Query<R>
fn migrate(statements: List<Query<Unit>>) -> Unit ! {Sql, Fail<SqlError>}
fn sqlite(path: Path) -> Handler<Sql, {Fs}>    // @external
fn memory() -> Handler<Sql>    // @external
```


## Deviations from design §10 (each has a `design-question` in `compiler/REQUESTS.md`)

1. **One module per type and effect** (`std/instant`, `std/duration`, `std/console`, …)
   instead of the grouped `std/core`, `std/effects`, `std/time`: the home-module rule (§4.9)
   would otherwise put `Console.host` and `Clock.host` in one namespace.
2. **Milliseconds** for `Instant` and `Duration`: nanoseconds since 1970 exceed 2^53 and
   would be `D003` on the JS backend. Additions: `Instant.from_unix_millis`, `to_unix_millis`.
3. **`retry` takes the error type:** `retry<T, E, e>(p: Policy<E>, action: fn() -> T ! {Fail<E> | e})`.
   §5.9's `retry<T, e>(policy: Retry.Policy, …)` cannot type `retry_on: fn(E) -> Bool`, and
   `Retry.Policy` is not a valid `TypePath` (the qualifier must be a lowercase module). The
   policy type is `Policy<E>` in `std/retry`, plus a `policy(retry_on, max_attempts)` helper.
4. **`Timeout`** is `std/task.Timeout(limit: Duration)`, reused by `LlmError` (resolved,
   design §17.6).
5. **`Clock.fixed(at: Instant)`** as in §10.2 (§5.5 passes a string). **`Clock.virtual()`**
   advances on `sleep`, since a handler cannot expose an extra "advance" operation.
6. **`Test` effect operations** are `record_failure`, `failures`, `output`, `snapshot`; §5.8
   only says "`Test` provides `expect`". `expect_defect` is an external (the runtime runs the
   body in a child task).
7. **Additions used by §15 or needed by handlers:** `Result.is_ok`/`is_err` (§15.2),
   `List.filter_map` and `Graph.neighbors` (§15.12), `Graph.empty`/`len`, `Bytes.len`/
   `from_list` (`Random.bytes`), `Uuid.Uuid` constructor, `Gen.*` combinators and
   `Gen.for_all`, `Console.captured`/`Log.captured` forwarding to `Test.output`.
8. **`List.par_map`** lives in `std/task` until `Task` exists, so `std/list` stays M1.
9. **`Log.host()` and `Env.host()`** exist because the JS entry point installs each declared
   std effect's `host()`; `Log.host()` is `stderr()`. `Env.host()` reads the whole process
   environment until `[permissions]` scoping lands (M3).
10. **`String.parse_int`/`parse_float`** follow Rust's `str::parse` (a leading `+` is accepted;
    floats accept `NaN`/`inf`), matching the runtime.
11. **One home module per type in the web and AI surface:** `std/request` (`Request`,
    `Method`, `BadRequest`), `std/response` (`Response`, `Status`, `HttpError`) and
    `std/prompt` (`Prompt`, `PromptPart`) are separate from `std/http` and `std/llm`, because
    `Request.json`/`Response.json`, `Request.post_json`/`Http.post_json` and
    `Prompt.of`/`Model.of` would otherwise be one name (§4.9). `std/budget` and `std/llm`
    replace the `std/ai` placeholder.
12. **`Request.bearer: Option<Secret<String>>`** keeps a credential added by `Http.bearer`
    out of the `Show`-able headers; the host handler sends it.
13. **Parameterless `Fs.host()` and `Http.host()`** for the entry point (as item 9);
    `Fs.memory`'s `root()` is a directory without host access. `Cli.exit` under `Cli.of` is
    a defect (`D030`). `Llm.scripted` reports undecodable output as `InvalidOutput` with one
    `Violation` whose `clause` is the decoder's message and `value_summary` its JSON Pointer.
14. **`Fiber<T>`** is the last item of `std/task`: a line ending in `>` continues (§2.4), so
    a bodiless generic type followed by another item does not parse.
