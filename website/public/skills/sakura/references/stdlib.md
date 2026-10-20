# Sakura Lang standard library index (v0)

Every public std signature, one module per file in `compiler/std/`. The signature listing
below the "Signatures" heading is generated from the std sources by `cargo run -p agent_docs`
(in `compiler/`; CI fails when it is stale); the rest of this file is authored. The same file is
the `sakura` skill's `references/stdlib.md`. The design reference is `docs/design.md` §10; deviations are listed at the end and filed as
`design-question` entries in `compiler/REQUESTS.md`.

Status: every module checks clean and its `test` blocks pass under `sakura test --std`.
Every module runs on the JavaScript and WebAssembly backends. `std/crypto`'s primitives are
`node:crypto` on JavaScript, the Node host's on WebAssembly (`hosted` imports, ABI 1.6.0; tier
1b's, AES-GCM, ECDSA and ECDH over P-256, Ed25519 and X25519, at 1.9.0) and the RustCrypto and
dalek crates on native.

## Prelude

In scope in every module without an import (design §9, with the additions marked +):

| Kind | Names |
|---|---|
| Compiler-known types | `Unit Bool Int Float String Bytes Never List Map Set Range Handler Secret Untrusted` |
| Std types | `Option Result` · + `Duration Instant Uuid Json Level` (prelude effects mention them; §15.1 and §15.4 use `Instant.format_iso`, `Level.Warn`, `Json.String` unimported) |
| Constructors | `Some None Ok Err` (other variants are used qualified: `Level.Warn`, `Json.String(s)`) |
| Effects | `Fail` (compiler-known) `Console Clock Random Env Log Task` · + `Test` |
| Functions | `show` · + `expect expect_eq expect_fail expect_fails_with expect_defect expect_output expect_snapshot` (they need `Test`, which only `test`/`property` bodies have; elsewhere they are `V3001`) |
| Tags | `sql html url` (`shell` is reserved by the prelude; no `std/shell` exists yet) |

Not in the prelude, import them: `import std/retry.{Policy, retry}`, `import std/task.{Timeout, timeout}`,
`import std/gen` (`gen.int(0, 9)`), `import std/graph.{Graph, NodeId}`, and every other std module.

Home modules (design §4.9): `T.f(…)` and `x.f(…)` with `x: T` call `f` in the module that
declares `T`; for compiler-known types it is `std/<lowercase name>` (`List` → `std/list`). An
effect's home module is the one declaring it, and `Eff.f(…)` calls a home-module function
when `f` is not an operation (`Console.captured()`, `Clock.fixed(t)`, `Random.seeded(1)`).

## Conventions

- **Externals.** Functions marked `// @external` are bodyless and bound by
  `@external(js, "sakura_runtime", "<module>_<fn>")` to `compiler/runtime/js`. Everything else is
  Sakura Lang. Externals are part of the trusted base (§6.6).
- **Defects, not failures,** for programming errors: `Option.expect` (`D030`),
  `String.slice` off a character boundary (`D031`), checked arithmetic (`D001`–`D003`), and
  `requires` clauses (`D020`, caller blamed).
- **Indexing never defects:** `List.get`/`xs[i]`, `Bytes.get`, `Map.get` return `Option`;
  `List.slice` and `String.slice` clamp their bounds.
- **Insertion order** for `Map`/`Set` iteration on every backend.
- **Time** is whole milliseconds (`Duration`, `Instant` since the Unix epoch, UTC);
  `Clock.monotonic_micros()` is the one microsecond reading, for timing work.
- **Test defaults** (§5.8): `Clock.fixed(Instant.from_unix_millis(0))`, `Random.seeded(seed)`,
  `Env.of(Map.empty())`, `Console.captured()` (`Console.scripted(lines)` scripts a test's
  input), `Log.captured()`. Expectations record a
  failure and let the test continue; the runner fails the test at the end.
- **Show format** assumed by the std tests and the example outputs (pending a spec, see the
  design questions): valid Sakura Lang expression syntax — `[1, 2]`, `(1, "a")`, `Some("x")`,
  `User { id: 1, name: "a" }`, `Map.of([("a", 1)])`, `Set.of([1])`, floats always with a
  `.` or exponent (`2.0`); strings are quoted inside containers and raw at the top level of
  an interpolation.
- **Opaque types:** `Prompt`, `Query`, `Html` and `UrlPart` are `pub opaque` (design §17.10), and
  `Schema` only inside their modules (§17.130, `V2033`). `Uuid`, `Path` and `Decimal` are
  refinements, so they decode through their predicates.

## By task

| Task | Start here |
|---|---|
| Transform, filter or update collections | `List.map`, `filter_map`, `fold`, `set`; `Map.update`; `Set.of` |
| Parse text and validate data | `String.parse_int`, `Json.decode`, `Untrusted.validate`, `Result` |
| Read CLI arguments and files | `Cli.parse`, `Path`, `Dir`, `Fs.rooted`; test with `Cli.of`, `Fs.memory` |
| Call HTTP or dispatch a route | `Request`, `Response`, `Http`; `Router.handle` for server-free tests; `UrlPattern.compile`, `exec` for route patterns |
| Stream a body, a file or text in bounded pieces | `Source.from_list`, `map`, `lines`, `chunks`, `fold`, `to_list`; `Http.body_source`, `Fs.file_source`, `Fs.file_sink`, `Blob.source`, `Encoding.decode_source` |
| Talk to a WebSocket server | `WebSocket.connect`, `send`, `receive`, `close`, `messages`; `WebSocket.scripted` for tests |
| Read headers, build or parse a form | `Headers.of`, `get`, `entries`; `Form.encode_multipart`, `parse_multipart` with `FormLimits`; `Blob`, `FileData` |
| Query a database | `sql` templates, `Sql.query`; `Sql.memory` for tests |
| Call a model or run tools | `Prompt`, `Llm`, `Tool`, `Agent`, `Budget`; `Llm.scripted` for tests |
| Retry, time or parallelize work | `retry`, `Clock`, `Random`, `std/task.timeout`, `std/task.par_map`, `std/task.every`, `Task.cancel` |
| Test effects deterministically | `Test`, `Console.captured`, `Console.scripted`, `Clock.fixed`, `Http.scripted`, `Serve.scripted`, `WebSocket.scripted` |

## Complexity and behaviour notes

These authored notes complement the generated signatures and are copied into the
checked skill reference; they are not additional APIs.

- An `Int` key you allocate as `len(xs)` and never remove indexes a `List`;
  `Map<Int, V>` is for sparse, removable or externally chosen keys (§17.217).
- `List.set` makes an **O(log32 n) path copy**, sharing untouched branches (§17.126).
  Use it for indexed updates instead of rebuilding the list with `map` or slices.
  Its index must satisfy the generated `requires` clause; `List.get` returns `Option`.
- `String.len` counts **UTF-8 bytes**, not characters, and is **O(n) on JS**.
  `String.chars` returns Unicode scalars; `graphemes` returns grapheme clusters.
  `String.slice` uses byte offsets, clamps bounds and defects (`D031`) if an
  endpoint splits a character. Cache lengths in loops when the string is unchanged.
- `Untrusted.validate` is compiler-known (§17.71); validation checks the target
  refinement. `trust` requires an explicit `Trust` argument and does not validate.
- `File` is a resource (§17.70); use `Fs.with_file` to bound its lifetime.
- A `Source` is a value (§17.280): `pull` returns `Done` or `Next(item, rest)`. A pure source
  (`from_list`, `Blob.source`, and combinators over them) reads the same items each time it
  is pulled; a view (`Http.body_source`, `Fs.file_source`, `Fs.file_sink`) advances its
  resource and is `V6003` outside the callback that received it. Every bound is an argument:
  `to_list`'s `max_items`, `chunks`'s `size`, `lines`'s `max_line`.
- Each non-`UrlPart` interpolation in `url"…"` goes through `UrlPart.from`, which
  percent-encodes `show(value)` (§17.124). `Url.template` assembles those parts.
- Each non-`Html` interpolation in `html"…"` goes through `Html.from` and is escaped
  (§17.77); an `Html` value is already markup. Each `sql"…"` value goes through
  `SqlValue.from` and is a parameter (§4.11), not concatenated query text.
- `Http.recording` writes a schema-version line, refuses credential-bearing
  headers and redacts `set-cookie` (§17.122). `Cassette.open(c)` (also `c.open()`)
  opens a cassette (§17.125). Under `sakura test --record` it stores at most 16 MiB
  (16,777,216 bytes) of a response body, sent or streamed; past that `Http.send` or
  `Http.stream` fails with `HttpError` `"response body over 16 MiB: sakura test --record
  stores at most 16,777,216 bytes of a response body"` and nothing is appended (§17.205,
  §17.209). Replaying an existing larger cassette is not checked. A `Llm.replay` miss is
  `ProviderError` with guidance naming `Llm.recording` and `sakura test --record`; it does
  not call a provider.
- HTTP bodies are a `Payload` (`Empty | Text(String) | Bytes(Bytes)`, RFC 0002 §2.4):
  `Request.of`, `post`, `post_json`, `Response.status`, `ok_json` and `html` build `Text` or
  `Empty`, `Request.with_payload` any of the three, and `Http.host()`, `Serve.host()` and a
  cassette replay deliver `Bytes` (`Empty` with no bytes). `Response.text`, `json` and `bytes`
  read any of them; a `Request` body is a `match`. Every handler in `std/http` follows
  redirects with one Sakura loop under `Request.redirect` (`Follow` by default, `Manual`,
  `Error`): at most 20 hops, each its own send, so `Http.host()` checks `net` and
  `Http.scoped` its list on every hop; a hop to another origin drops `authorization` and
  every `secret_headers` entry (design §17.263 (4)). `Http.scripted` can answer a 3xx.
- `Http.host()`'s `send` reads at most `SAKURA_MAX_COPY_BYTES` (default 256 MiB) of a
  response body; past that, or with a declared `content-length` over it, the fetch is
  aborted and `send` fails with `HttpError` `"response too large"` (§17.205). On
  `--target wasm` the host also counts headers and queued answers against the same budget.
  The variable takes decimal digits from 1 to 2^53 - 1; any other value (empty, `0`, signs, fractions,
  exponents, spaces) is the defect `D091` at first use (on `--target wasm`, where the
  budget covers every host answer, the first answer), never an unlimited or zero budget
  (§17.209). The budget is a per-instance memory ceiling, not a byte-exact contract: near it
  the two backends may differ, so read at most about that many bytes.
- `Serve.host()` answers a request body over 1 MiB with `413` and closes the connection
  without running the handler, and answers a handler's failure, defect or invalid response
  with `500` and the record on stderr; the server keeps serving (§17.159 (6)). On
  `--target wasm` a request over `SAKURA_MAX_COPY_BYTES` (headers included) is that `413`
  and a response over it is that `500`, each with the defect `D091` on stderr (§17.216 (4)).
  A request the guest heap cannot take is `503` with `retry-after: 1` and the defect `D096`
  on stderr, and the server keeps serving. On both backends, once the server stops a request
  still arriving is `503` with the connection closed and the handler not run, and on
  `--target wasm` a request still waiting when the program ends is `500`. A runtime trap
  while serving answers that request `500`, stops the server and ends the program with the
  defect `D090` (exit 70) (§17.223). A request
  pipelined behind one refused with `413` may be served before the connection closes.

## Module dependencies (non-test code)

Acyclic. Test blocks additionally use `std/test`, which depends on `std/core` and (for
`Test.fixtures()`'s `Dir`) `std/fs`.

```text
core  unit  option  result  set  int  float  duration       (no std dependencies)
list → set              map → list            string → list       bytes → list
encoding → bytes, string, list, stream         searchparams → encoding, string, list, bytes
crypto → bytes, list, secret   (+ Random for `generate_key`)
headers → string, list, bytes                 blob → bytes, string, int, stream
form → blob, bytes, encoding, searchparams, string, list, instant   (+ Random for `boundary`)
json → map, string, list, int                 uuid → string, list   instant → duration, string, list, int
test → core, fs         console → test, string, list                  clock → duration, instant, int
random → uuid, int, string, bytes, list       env → map, list
log → json, test, list, core                  gen → test, random, list, int, string
retry → duration, int   (+ Clock, Random)     task → duration, list, int (+ Clock)
graph → map, list       untrusted → core        path → string, list
secret → core           decimal → string, list, int                 cli
url → string, list, int, bytes, secret, urlpart, searchparams      urlpart → string, list, bytes
urlpattern → url, string, list, bytes        stream → list, bytes, string
compress → stream, bytes, int
fs → path, untrusted, map, stream
request → url, json, encoding, headers         response → request, encoding, html, json, untrusted
html → core             router → request, response, url, urlpattern
serve → request, response, router, url, encoding
http → request, response, url, duration, fs, path, stream          budget   prompt
websocket → url, stream, string, bytes, list, map
llm → budget, core, fs, http, path, prompt, task, untrusted               sql → fs, path
tool → llm              agent → budget, core, llm, prompt, tool
```

The std modules' test handlers are written in Sakura Lang (`Http.scripted`, `Budget.limits`,
`Cli.of`, `Fs.memory`, `Llm.scripted`); their host handlers and `Schema` codecs are runtime
externals.

## Signatures

<!-- agent_docs: signatures begin (generated by `cargo run -p agent_docs`; do not edit) -->

### `std/agent`

```sakura fragment
type Agent<T, e> = { request: Request<T>, tools: List<Tool<e>>, max_steps: Int, stop: fn(Partial<T>) -> Bool }
fn run<T: Schema, e>(a: Agent<T, e>) -> T ! {Budget, Llm, Fail<LlmError | ToolError> | e} requires a.max_steps > 0
```

### `std/blob`

```sakura fragment
type Blob = { bytes: Bytes, media_type: String }
type FileData = { blob: Blob, name: String, last_modified: Instant }
fn of(bytes: Bytes, media_type: String) -> Blob
fn size(b: Blob) -> Int
fn slice(b: Blob, start: Int, end: Int, media_type: String) -> Blob
fn text(b: Blob) -> Result<String, Utf8Error>
fn source<e>(b: Blob, chunk: Int) -> Source<Bytes, e> requires chunk > 0
```

### `std/budget`

```sakura fragment
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

### `std/bytes`

```sakura fragment
fn len(b: Bytes) -> Int    // @external, @total
fn get(b: Bytes, i: Int) -> Option<Int>    // @external, @total
fn from_list(xs: List<Int>) -> Bytes requires List.all(xs, fn(x) { x >= 0 && x <= 255 })    // @external
fn slice(b: Bytes, start: Int, end: Int) -> Bytes    // @external
fn concat(parts: List<Bytes>) -> Bytes    // @external
type Alphabet = Standard | Url
error Base64Error(offset: Int, reason: String)
fn to_base64(b: Bytes, alphabet: Alphabet, padded: Bool) -> String
fn from_base64(text: String, alphabet: Alphabet) -> Result<Bytes, Base64Error>
fn from_base64_lenient(text: String) -> Result<Bytes, Base64Error>
```

### `std/cli`

```sakura fragment
effect Cli {
  fn args() -> List<String>
  fn exit(code: Int) -> Never requires code >= 0 && code <= 255
}
error CliError(message: String)
fn host() -> Handler<Cli>
fn of(args: List<String>) -> Handler<Cli>
fn parse<T: Schema>(args: List<String>) -> T ! {Fail<CliError>}    // @external
```

### `std/clock`

```sakura fragment
effect Clock {
  fn now() -> Instant
  fn monotonic() -> Duration
  fn sleep(d: Duration) -> Unit
  fn monotonic_micros() -> Int
}
fn host() -> Handler<Clock>    // @external
fn fixed(at: Instant) -> Handler<Clock>
fn virtual() -> Handler<Clock>
```

### `std/compress`

```sakura fragment
type Format = Gzip | Deflate | DeflateRaw
type CompressionReason = Malformed(Int) | OutputLimit | Truncated
error CompressionError(reason: CompressionReason)
fn compress(b: Bytes, format: Format, level: Int) -> Bytes requires level >= 0 && level <= 9
fn decompress(b: Bytes, format: Format, max_out: Int) -> Result<Bytes, CompressionError> requires max_out >= 0
fn compress_source<T, e>(src: Source<Bytes, e>, format: Format, level: Int, body: fn(Source<Bytes, e>) -> T ! e) -> T ! e requires level >= 0 && level <= 9
fn decompress_source<T, e>(src: Source<Bytes, e>, format: Format, max_out: Int, body: fn(Source<Bytes, {Fail<CompressionError> | e}>) -> T ! {Fail<CompressionError> | e}) -> T ! {Fail<CompressionError> | e} requires max_out >= 0
```

### `std/console`

```sakura fragment
effect Console {
  fn print(line: String) -> Unit
  fn read_line() -> Option<String>
}
fn host() -> Handler<Console>    // @external
fn captured() -> Handler<Console, {Test}>
fn scripted(lines: List<String>) -> Handler<Console, {Test}>
fn alert(message: String) -> Unit ! {Console}
fn confirm(message: String) -> Bool ! {Console}
fn prompt(message: String, default: Option<String>) -> Option<String> ! {Console}
```

### `std/core`

```sakura fragment
fn show<A: Show>(a: A) -> String    // @external
type Violation = { type_name: String, clause: String, value_summary: String }
type Trust
```

### `std/crypto`

```sakura fragment
type Digest = Sha256 | Sha384 | Sha512
type Curve = P256
type AesLength = Aes128 | Aes256
type Algorithm = Hmac(Digest) | Hkdf(Digest) | Pbkdf2(Digest) | AesGcm(AesLength) | Ecdsa(Curve) | Ecdh(Curve) | Ed25519 | X25519
type Usage = Sign | Verify | DeriveBits | Encrypt | Decrypt
type KeyFormat = Raw | Pkcs8 | Spki | Jwk
type KeyKind = SecretKey | PublicKey | PrivateKey
type KeyReason = NotExtractable | NoUsages | NotAllowed(Usage) | Empty | Format(KeyFormat) | Malformed
error KeyError(reason: KeyReason)
type CryptoReason = Length(Int) | Iterations(Int) | Info(Int) | Nonce(Int) | Tag | ZeroSecret
error CryptoError(reason: CryptoReason)
opaque type CryptoKey = { algorithm: Algorithm, kind: KeyKind, usages: List<Usage>, extractable: Bool, material: Secret<Bytes> }
type KeyPair = { public: CryptoKey, private: CryptoKey }
type HkdfParams = { salt: Bytes, info: Bytes }
type Pbkdf2Params = { salt: Bytes, iterations: Int }
type DeriveParams = HkdfWith(HkdfParams) | Pbkdf2With(Pbkdf2Params) | EcdhWith(CryptoKey)
fn algorithm(key: CryptoKey) -> Algorithm
fn kind(key: CryptoKey) -> KeyKind
fn usages(key: CryptoKey) -> List<Usage>
fn extractable(key: CryptoKey) -> Bool
fn allows(alg: Algorithm, usage: Usage) -> Bool    // @total
fn permits(key: CryptoKey, usage: Usage) -> Bool    // @total
fn digest(alg: Digest, data: Bytes) -> Bytes
fn import_key(format: KeyFormat, alg: Algorithm, usages: List<Usage>, extractable: Bool, material: Secret<Bytes>) -> Result<CryptoKey, KeyError>
fn export_key(key: CryptoKey, format: KeyFormat) -> Result<Bytes, KeyError>
fn generate_key(alg: Algorithm, usages: List<Usage>, extractable: Bool) -> CryptoKey ! {Random} requires (is_hmac(alg) || is_aes(alg)) && usages_error(alg, SecretKey, usages) == None
fn generate_key_pair(alg: Algorithm, usages: List<Usage>, extractable: Bool) -> KeyPair ! {Random} requires curve_of(alg) != None && List.all(usages, fn(u) { allows(alg, u) }) && List.any(usages, fn(u) { u != Verify })
fn hmac(key: CryptoKey, data: Bytes) -> Bytes requires is_hmac(key.algorithm) && permits(key, Sign)
fn sign(key: CryptoKey, data: Bytes) -> Bytes ! {Random} requires signs(key.algorithm) && permits(key, Sign)
fn verify(key: CryptoKey, signature: Bytes, data: Bytes) -> Bool requires signs(key.algorithm) && permits(key, Verify)
fn encrypt(key: CryptoKey, nonce: Bytes, aad: Bytes, plaintext: Bytes) -> Result<Bytes, CryptoError> requires is_aes(key.algorithm) && permits(key, Encrypt)
fn decrypt(key: CryptoKey, nonce: Bytes, aad: Bytes, ciphertext: Bytes) -> Result<Bytes, CryptoError> requires is_aes(key.algorithm) && permits(key, Decrypt)
fn derive_bits(key: CryptoKey, params: DeriveParams, bits: Int) -> Result<Bytes, CryptoError> requires permits(key, DeriveBits) && fits(key, params)
```

### `std/decimal`

```sakura fragment
type DecimalRepr = { whole: Int, frac: List<Int> }
type Decimal = DecimalRepr where is_canonical(it)
fn is_canonical(d: DecimalRepr) -> Bool    // @total
error DecimalError(input: String)
fn from_int(n: Int) -> Decimal
fn zero() -> Decimal
fn parse(text: String) -> Result<Decimal, DecimalError>
fn to_string(d: Decimal) -> String
fn add(a: Decimal, b: Decimal) -> Decimal
fn sub(a: Decimal, b: Decimal) -> Decimal
fn neg(a: Decimal) -> Decimal
fn mul(a: Decimal, b: Decimal) -> Decimal
fn div_checked(a: Decimal, b: Decimal, places: Int) -> Option<Decimal> requires places >= 0
fn round(d: Decimal, places: Int) -> Decimal requires places >= 0
```

### `std/duration`

```sakura fragment
type Duration = Duration(Int)
fn millis(n: Int) -> Duration
fn seconds(n: Int) -> Duration
fn minutes(n: Int) -> Duration
fn to_millis(d: Duration) -> Int
```

### `std/encoding`

```sakura fragment
type Encoding = Utf8 | Utf16Le | Utf16Be | Windows1252
type Errors = Fatal | Replace
type Bom = Strip | Keep
type EncodingReason = Unsupported(String) | Malformed(Int)
error EncodingError(reason: EncodingReason)
opaque type Decoder = { encoding: Encoding, errors: Errors, pending: Bytes, offset: Int, started: Bool }
fn parse(label: String) -> Result<Encoding, EncodingError>
fn decode(b: Bytes, enc: Encoding, errors: Errors) -> Result<String, EncodingError>
fn decode_with(b: Bytes, enc: Encoding, errors: Errors, bom: Bom) -> Result<String, EncodingError>
fn new(enc: Encoding, errors: Errors) -> Decoder
fn push(d: Decoder, chunk: Bytes) -> Result<(Decoder, String), EncodingError>
fn finish(d: Decoder) -> Result<String, EncodingError>
fn decode_source<e>(src: Source<Bytes, e>, enc: Encoding, errors: Errors) -> Source<String, {Fail<EncodingError> | e}>
fn encode_source<e>(src: Source<String, e>, enc: Encoding) -> Source<Bytes, e> requires encodable(enc)
fn encode(s: String, enc: Encoding) -> Bytes requires encodable(enc)
fn encodable(enc: Encoding) -> Bool    // @total
```

### `std/env`

```sakura fragment
effect Env {
  fn get(key: String) -> Option<String>
  fn secret(key: String) -> Option<Secret<String>>
}
fn host() -> Handler<Env>
fn of(vars: Map<String, String>) -> Handler<Env>
fn only(keys: List<String>) -> Handler<Env, {Env}>
```

### `std/float`

```sakura fragment
fn round(f: Float) -> Int    // @external
fn to_int_checked(f: Float) -> Option<Int>    // @external
fn sqrt(f: Float) -> Float    // @external
fn format(f: Float, decimals: Int) -> String requires decimals >= 0    // @external
fn is_nan(f: Float) -> Bool
```

### `std/form`

```sakura fragment
type Part = Text(String) | File(FileData)
type Form = { entries: List<(String, Part)> }
type FormLimits = { max_parts: Int, max_bytes: Int, max_field_bytes: Int }
type FormLimit = MaxParts | MaxBytes | MaxFieldBytes
type FormReason = Limit(FormLimit) | ContentType | Malformed(Int)
error FormError(reason: FormReason)
fn encode_urlencoded(f: Form) -> String
fn parse_urlencoded(text: String) -> Form
fn encode_multipart(f: Form, boundary: String) -> Bytes requires is_boundary(boundary)
fn boundary() -> String ! {Random}
fn is_boundary(s: String) -> Bool    // @total
fn parse_multipart(body: Bytes, content_type: String, limits: FormLimits) -> Result<Form, FormError>
```

### `std/fs`

```sakura fragment
type Dir
type Entry = { name: String, is_dir: Bool }
type Mode = Read | Write | Append
opaque type File = { id: Int, path: Path, mode: Mode }    // @resource
error FsError(path: String, reason: String)
effect Fs {
  fn root() -> Dir
  fn read(path: Path) -> Bytes ! {Fail<FsError>}
  fn read_text(path: Path) -> Untrusted<String> ! {Fail<FsError>}
  fn write(path: Path, data: Bytes) -> Unit ! {Fail<FsError>}
  fn list(dir: Path) -> List<Entry> ! {Fail<FsError>}
  fn exists(path: Path) -> Bool
  fn with_file<T, e>(path: Path, mode: Mode, body: fn(File) -> T ! e) -> T ! {Fail<FsError> | e}
  fn read_chunk(file: File, max: Int) -> Bytes ! {Fail<FsError>} requires max > 0
  fn write_chunk(file: File, data: Bytes) -> Unit ! {Fail<FsError>}
}
fn host() -> Handler<Fs>
fn rooted(dir: Dir) -> Handler<Fs>
fn subdir(d: Dir, name: String) -> Dir ! {Fail<FsError>}
fn memory(files: List<(Path, Bytes)>) -> Handler<Fs>
fn read_all(file: File) -> Bytes ! {Fs, Fail<FsError>}
fn file_source(file: File, chunk: Int) -> Source<Bytes, {Fs, Fail<FsError>}> requires chunk > 0
fn file_sink(file: File) -> Sink<Bytes, {Fs, Fail<FsError>}>
```

### `std/gen`

```sakura fragment
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

### `std/graph`

```sakura fragment
type NodeId = NodeId(Int)
type Graph<N> = { nodes: Map<Int, N>, edges: Map<Int, List<NodeId>>, next: Int }
fn empty<N>() -> Graph<N>
fn insert<N>(g: Graph<N>, n: N) -> (Graph<N>, NodeId)
fn get<N>(g: Graph<N>, id: NodeId) -> Option<N>    // @total
fn connect<N>(g: Graph<N>, a: NodeId, b: NodeId) -> Graph<N> requires get(g, a).is_some() && get(g, b).is_some()
fn neighbors<N>(g: Graph<N>, id: NodeId) -> List<NodeId>
fn len<N>(g: Graph<N>) -> Int
```

### `std/headers`

```sakura fragment
type HeaderReason = InvalidName | InvalidValue
error HeaderError(name: String, reason: HeaderReason)
opaque type Headers = Headers(List<(String, String)>)
fn of(list: List<(String, String)>) -> Result<Headers, HeaderError>
fn get(h: Headers, name: String) -> Option<String>
fn get_set_cookie(h: Headers) -> List<String>
fn get_all(h: Headers, name: String) -> List<String>
fn has(h: Headers, name: String) -> Bool
fn append(h: Headers, name: String, value: String) -> Result<Headers, HeaderError>
fn set(h: Headers, name: String, value: String) -> Result<Headers, HeaderError>
fn remove(h: Headers, name: String) -> Headers
fn entries(h: Headers) -> List<(String, String)>
fn to_list(h: Headers) -> List<(String, String)>
```

### `std/html`

```sakura fragment
opaque type Html = Html(String)
fn text(s: String) -> Html
fn untrusted(u: Untrusted<String>) -> Html
fn raw(markup: String, reason: Trust) -> Html
fn from<T: Show>(v: T) -> Html
fn concat(parts: List<Html>) -> Html
fn to_string(h: Html) -> String
fn template(parts: List<String>, args: List<Html>) -> Html
```

### `std/http`

```sakura fragment
type Matcher = Any | Path(String) | Method(Method)
error RateLimited(retry_after: Option<Duration>)
type Body    // @resource
type Incoming = { status: Status, headers: List<(String, String)>, body: Body }
effect Http {
  fn send(req: Request) -> Response ! {Fail<HttpError>}
  fn stream<T, e>(req: Request, body: fn(Incoming) -> T ! e) -> T ! {Fail<HttpError> | e}
  fn read_chunk(body: Body, max: Int) -> Bytes ! {Fail<HttpError>} requires max > 0
}
fn body_source(body: Body, chunk: Int) -> Source<Bytes, {Http, Fail<HttpError>}> requires chunk > 0
fn get(url: Url) -> Response ! {Http, Fail<HttpError | RateLimited>}
fn post_json<T: Schema>(url: Url, body: T) -> Response ! {Http, Fail<HttpError | RateLimited>}
fn bearer(req: Request, token: Secret<String>) -> Request
fn host() -> Handler<Http>
fn scoped(hosts: List<String>) -> Handler<Http, {Http}>
fn recording(path: Path) -> Handler<Http, {Fs, Http}>
opaque type Cassette = { path: Path, id: Int }
fn cassette(path: Path) -> Cassette
fn open(c: Cassette) -> Cassette ! {Fs, Fail<HttpError>}
fn replaying(c: Cassette) -> Handler<Http, {Fs, Http}>
fn scripted(script: List<(Matcher, Response)>) -> Handler<Http>
```

### `std/instant`

```sakura fragment
type Instant = Instant(Int)
error TimeError(input: String)
fn from_unix_millis(ms: Int) -> Instant
fn to_unix_millis(i: Instant) -> Int
fn add(i: Instant, d: Duration) -> Instant
fn diff(a: Instant, b: Instant) -> Duration
fn format_iso(i: Instant) -> String
fn parse(text: String) -> Result<Instant, TimeError>
```

### `std/int`

```sakura fragment
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

### `std/json`

```sakura fragment
type Json = Null | Bool(Bool) | Number(Float) | String(String) | Array(List<Json>) | Object(Map<String, Json>)
error DecodeError(path: String, message: String)
fn encode<T: Schema>(value: T) -> String    // @external
fn decode<T: Schema>(text: String) -> Result<T, DecodeError>    // @external
fn parse(text: String) -> Result<Json, DecodeError>    // @external
fn schema<T: Schema>(witness: Option<T>) -> Json    // @external
fn render(j: Json, indent: Option<Int>) -> String
fn get(j: Json, key: String) -> Option<Json>
fn get_string(j: Json, key: String) -> Option<String>
```

### `std/list`

```sakura fragment
fn len<A>(xs: List<A>) -> Int    // @external, @total
fn get<A>(xs: List<A>, i: Int) -> Option<A>    // @external, @total
fn push<A>(xs: List<A>, x: A) -> List<A>    // @external
fn set<A>(xs: List<A>, i: Int, x: A) -> List<A> requires 0 <= i && i < len(xs)
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

### `std/llm`

```sakura fragment
type Model = Model(String)
type Request<T> = { system: String, user: Prompt, model: Model, max_tokens: Int, temperature: Float }
error Refused
error InvalidOutput(violations: List<Violation>)
error Truncated
error ProviderError(status: Int, reason: String)
type LlmError = Refused | InvalidOutput | Truncated | RateLimited | Timeout | ProviderError | BudgetExceeded
type ToolSpec = { name: String, description: String, schema: Json }
type ToolCall = { id: String, name: String, input: Json }
type ToolResult = { id: String, output: Json }
type Turn = Asked(List<ToolCall>) | Answered(List<ToolResult>)
type Step<T> = Answer(T) | Use(List<ToolCall>)
type Partial<T> = { text: String, step: Int, calls: List<ToolCall> }
effect Llm {
  fn generate<T: Schema>(req: Request<T>) -> T ! {Fail<LlmError>}
  fn generate_text(req: Request<String>) -> Untrusted<String> ! {Fail<LlmError>}
  fn stream<T: Schema, e>(req: Request<T>, on_partial: fn(Partial<T>) -> Unit ! e) -> T ! {Fail<LlmError> | e}
  fn step<T: Schema>(req: Request<T>, tools: List<ToolSpec>, transcript: List<Turn>) -> Step<T> ! {Fail<LlmError>}
}
fn request<T: Schema>(system: String, user: Prompt) -> Request<T>
fn of(name: String) -> Model
fn default() -> Model
fn anthropic(key: Secret<String>, model: Model) -> Handler<Llm, {Budget, Http}>    // @external
fn recording(path: Path, provider: Handler<Llm, {Budget, Http}>) -> Handler<Llm, {Fs, Http}>
fn replay(path: Path) -> Handler<Llm, {Fs}>
fn scripted(responses: List<String>) -> Handler<Llm>
```

### `std/log`

```sakura fragment
type Level = Debug | Info | Warn | Error
effect Log {
  fn event(level: Level, message: String, fields: List<(String, Json)>) -> Unit
}
fn stderr() -> Handler<Log>    // @external
fn host() -> Handler<Log>
fn json() -> Handler<Log>    // @external
fn captured() -> Handler<Log, {Test}>
```

### `std/map`

```sakura fragment
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

### `std/option`

```sakura fragment
type Option<A> = None | Some(A)
fn map<A, B, e>(o: Option<A>, f: fn(A) -> B ! e) -> Option<B> ! e
fn unwrap_or<A>(o: Option<A>, default: A) -> A
fn or_fail<A, E>(o: Option<A>, err: E) -> A ! {Fail<E>}
fn expect<A>(o: Option<A>, msg: String) -> A    // @external
fn is_some<A>(o: Option<A>) -> Bool    // @total
```

### `std/path`

```sakura fragment
type Path = String where is_relative(it)
fn is_relative(s: String) -> Bool    // @total
fn of(s: String) -> Path requires is_relative(s)
fn join(p: Path, rest: String) -> Path requires is_relative(rest)
fn to_string(p: Path) -> String
```

### `std/prompt`

```sakura fragment
type PromptPart = Text(String) | Untrusted(Untrusted<String>) | Image(Bytes)
opaque type Prompt = Prompt(List<PromptPart>)
fn of(parts: List<PromptPart>) -> Prompt
fn text(s: String) -> Prompt
fn parts(p: Prompt) -> List<PromptPart>
```

### `std/random`

```sakura fragment
effect Random {
  fn int(lo: Int, hi: Int) -> Int requires lo <= hi
  fn float() -> Float
  fn bytes(n: Int) -> Bytes requires n >= 0
  fn uuid() -> Uuid
}
fn host() -> Handler<Random>    // @external
fn seeded(seed: Int) -> Handler<Random>
```

### `std/request`

```sakura fragment
type Method = Get | Head | Post | Put | Patch | Delete | Options | Trace
type Payload = Empty | Text(String) | Bytes(Bytes)
type Redirect = Follow | Manual | Error
type Request = { method: Method, url: Url, headers: List<(String, String)>, body: Payload, redirect: Redirect, secret_headers: List<(String, Secret<String>)>, params: List<(String, String)> }
error BadRequest(detail: String)
fn of(method: Method, url: Url) -> Request
fn post(path: String, body: String) -> Request
fn post_json<T: Schema>(url: Url, body: T) -> Request
fn with_payload(req: Request, body: Payload) -> Request
fn with_header(req: Request, name: String, value: String) -> Request
fn with_headers(req: Request, h: Headers) -> Request
fn bytes(req: Request, limit: Int) -> Bytes ! {Fail<BadRequest>}
fn text(req: Request, limit: Int) -> String ! {Fail<BadRequest>}
fn json<T: Schema>(req: Request, limit: Int) -> T ! {Fail<BadRequest>}
fn param(req: Request, name: String) -> Option<String>
fn header(req: Request, name: String) -> Option<String>
```

### `std/response`

```sakura fragment
type Status = Int where it >= 100 && it <= 599
type Response = { status: Status, headers: List<(String, String)>, body: Payload }
error HttpError(status: Option<Status>, reason: String)
fn status(code: Int, body: String) -> Response requires code >= 100 && code <= 599
fn ok_json<T: Schema>(value: T) -> Response
fn html(h: Html) -> Response
fn json<T: Schema>(r: Response, limit: Int) -> T ! {Fail<DecodeError | HttpError>}
fn text(r: Response, limit: Int) -> Untrusted<String> ! {Fail<HttpError>}
fn bytes(r: Response, limit: Int) -> Bytes ! {Fail<HttpError>}
```

### `std/result`

```sakura fragment
type Result<A, E> = Ok(A) | Err(E)
fn map<A, B, E, e>(r: Result<A, E>, f: fn(A) -> B ! e) -> Result<B, E> ! e
fn map_err<A, E, F>(r: Result<A, E>, f: fn(E) -> F) -> Result<A, F>
fn unwrap_or<A, E>(r: Result<A, E>, default: A) -> A
fn raise<A, E>(r: Result<A, E>) -> A ! {Fail<E>}
fn is_ok<A, E>(r: Result<A, E>) -> Bool    // @total
fn is_err<A, E>(r: Result<A, E>) -> Bool    // @total
```

### `std/retry`

```sakura fragment
type Policy<E> = { retry_on: fn(E) -> Bool, max_attempts: Int, backoff: Duration, jitter: Bool, deadline: Option<Duration> }
fn policy<E>(retry_on: fn(E) -> Bool, max_attempts: Int) -> Policy<E> requires max_attempts >= 1
fn retry<T, E, e>(p: Policy<E>, action: fn() -> T ! {Fail<E> | e}) -> T ! {Clock, Random, Fail<E> | e}
```

### `std/router`

```sakura fragment
opaque type Router<e> = { routes: List<Route<e>> }
fn new<e>() -> Router<e>
fn route<e>(r: Router<e>, method: Method, pattern: String, h: fn(Request) -> Response ! e) -> Router<e> requires UrlPattern.compile(pattern).is_ok()
fn get<e>(r: Router<e>, pattern: String, h: fn(Request) -> Response ! e) -> Router<e> requires UrlPattern.compile(pattern).is_ok()
fn post<e>(r: Router<e>, pattern: String, h: fn(Request) -> Response ! e) -> Router<e> requires UrlPattern.compile(pattern).is_ok()
fn put<e>(r: Router<e>, pattern: String, h: fn(Request) -> Response ! e) -> Router<e> requires UrlPattern.compile(pattern).is_ok()
fn delete<e>(r: Router<e>, pattern: String, h: fn(Request) -> Response ! e) -> Router<e> requires UrlPattern.compile(pattern).is_ok()
fn handle<e>(r: Router<e>, req: Request) -> Response ! e
```

### `std/searchparams`

```sakura fragment
opaque type SearchParams = SearchParams(List<(String, String)>)
fn parse(query: String) -> SearchParams
fn get(sp: SearchParams, name: String) -> Option<String>
fn get_all(sp: SearchParams, name: String) -> List<String>
fn has(sp: SearchParams, name: String) -> Bool
fn append(sp: SearchParams, name: String, value: String) -> SearchParams
fn set(sp: SearchParams, name: String, value: String) -> SearchParams
fn remove(sp: SearchParams, name: String) -> SearchParams
fn sort(sp: SearchParams) -> SearchParams
fn entries(sp: SearchParams) -> List<(String, String)>
fn to_string(sp: SearchParams) -> String
```

### `std/secret`

```sakura fragment
fn of<T>(value: T) -> Secret<T>    // @external
fn expose<T>(s: Secret<T>, reason: Trust) -> T
fn map<T, U>(s: Secret<T>, f: fn(T) -> U, reason: Trust) -> Secret<U>
```

### `std/serve`

```sakura fragment
type Address = { host: String, port: Int }
error ServeError(reason: String)
effect Serve {
  fn serve<e>(handle: fn(Request) -> Response ! e, addr: Address) -> Never ! {Fail<ServeError> | e}
}
fn listen<e>(router: Router<e>, addr: Address) -> Never ! {Serve, Fail<ServeError> | e}
fn of(host: String, port: Int) -> Address requires port >= 0 && port <= 65_535
fn host() -> Handler<Serve>
fn scripted(requests: List<Request>) -> Handler<Serve, {Test}>
```

### `std/set`

```sakura fragment
fn empty<A: Hash>() -> Set<A>    // @external
fn insert<A: Hash>(s: Set<A>, x: A) -> Set<A>    // @external
fn contains<A: Hash>(s: Set<A>, x: A) -> Bool    // @external, @total
fn len<A: Hash>(s: Set<A>) -> Int    // @external, @total
fn to_list<A: Hash>(s: Set<A>) -> List<A>    // @external
fn of<A: Hash>(xs: List<A>) -> Set<A>
```

### `std/sql`

```sakura fragment
type SqlValue = Null | Int(Int) | Float(Float) | Text(String) | Blob(Bytes)
fn from<T: Schema>(value: T) -> SqlValue
opaque type Query<R> = { text: String, params: List<SqlValue> }
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

### `std/stream`

```sakura fragment
type Step<T, e> = Done | Next(T, Source<T, e>)
type Source<T, e> = { pull: fn() -> Step<T, e> ! e }
type Sink<T, e> = { write: fn(T) -> Sink<T, e> ! e, close: fn() -> Unit ! e }
type StreamReason = MaxItems | MaxLine
error StreamError(reason: StreamReason)
fn empty<T, e>() -> Source<T, e>
fn from_list<T, e>(xs: List<T>) -> Source<T, e>
fn map<A, B, e>(src: Source<A, e>, f: fn(A) -> B ! e) -> Source<B, e>
fn filter<T, e>(src: Source<T, e>, keep: fn(T) -> Bool ! e) -> Source<T, e>
fn take<T, e>(src: Source<T, e>, n: Int) -> Source<T, e> requires n >= 0
fn fold<T, A, e>(src: Source<T, e>, init: A, f: fn(A, T) -> A ! e) -> A ! e
fn each<T, e>(src: Source<T, e>, f: fn(T) -> Unit ! e) -> Unit ! e
fn to_list<T, e>(src: Source<T, e>, max_items: Int) -> List<T> ! {Fail<StreamError> | e} requires max_items >= 0
fn chunks<e>(src: Source<Bytes, e>, size: Int) -> Source<Bytes, e> requires size > 0
fn lines<e>(src: Source<String, e>, max_line: Int) -> Source<String, {Fail<StreamError> | e}>
fn pipe<T, e>(src: Source<T, e>, sink: Sink<T, e>) -> Unit ! e
```

### `std/string`

```sakura fragment
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

### `std/task`

```sakura fragment
type Scope
type Fiber<T>
effect Task {
  fn scope<T, e>(body: fn(Scope) -> T ! e) -> T ! e
  fn spawn<T, e>(s: Scope, body: fn() -> T ! e) -> Fiber<T> ! e
  fn join<T>(f: Fiber<T>) -> T
  fn all<T, e>(bodies: List<fn() -> T ! e>) -> List<T> ! e
  fn race<T, e>(bodies: List<fn() -> T ! e>) -> T ! e
  fn yield_now() -> Unit
  fn cancel<T>(f: Fiber<T>) -> Unit
  fn cancelled(s: Scope) -> Bool
}
fn host() -> Handler<Task>    // @external
fn deterministic(seed: Int) -> Handler<Task>    // @external
error Timeout(limit: Duration)
fn timeout<T, e>(limit: Duration, action: fn() -> T ! e) -> T ! {Clock, Task, Fail<Timeout> | e}
fn every<e>(period: Duration, body: fn(Int) -> Bool ! e) -> Unit ! {Clock, Task | e}
fn par_map<A, B, e>(xs: List<A>, f: fn(A) -> B ! e) -> List<B> ! {Task | e}
```

### `std/test`

```sakura fragment
effect Test {
  fn record_failure(message: String) -> Unit
  fn failures() -> Int
  fn output(line: String) -> Unit
  fn output_lines() -> List<String>
  fn snapshot(name: String, rendered: String) -> Unit
  fn fixtures() -> Dir
}
fn expect(cond: Bool) -> Unit ! {Test}
fn expect_eq<A: Eq + Show>(actual: A, expected: A) -> Unit ! {Test}
fn expect_fail<T, E: Eq + Show, e>(body: fn() -> T ! {Fail<E> | e}, err: E) -> Unit ! {Test | e}
fn expect_fails_with<T, E: Show, e>(body: fn() -> T ! {Fail<E> | e}, pred: fn(E) -> Bool) -> Unit ! {Test | e}
fn expect_defect<T, e>(body: fn() -> T ! e) -> Unit ! {Test | e}    // @external
fn expect_output(lines: List<String>) -> Unit ! {Test}
fn expect_snapshot<A: Show>(name: String, value: A) -> Unit ! {Test}
```

### `std/tool`

```sakura fragment
opaque type Tool<e> = { spec: ToolSpec, run: fn(Json) -> Json ! {Fail<ToolError> | e} }
error ToolError(tool: String, tag: String, payload: Json)
fn from<A: Schema, R: Schema, E: Schema + Show, e>(name: String, description: String, f: fn(A) -> R ! {Fail<E> | e}) -> Tool<e>
fn name<e>(t: Tool<e>) -> String
fn spec<e>(t: Tool<e>) -> ToolSpec
fn call<e>(t: Tool<e>, input: Json) -> Json ! {Fail<ToolError> | e}
```

### `std/unit`

```sakura fragment
fn ignore<A>(a: A) -> Unit
```

### `std/untrusted`

```sakura fragment
fn of<T>(value: T) -> Untrusted<T>    // @external
fn validate<T, V>(u: Untrusted<T>) -> Result<V, Violation>    // @external
fn trust<T>(u: Untrusted<T>, reason: Trust) -> T
fn check<T>(u: Untrusted<T>, clause: String, pred: fn(T) -> Bool) -> Result<T, Violation>
fn len(u: Untrusted<String>) -> Int
```

### `std/url`

```sakura fragment
type Url = String where is_valid(it)
fn is_valid(s: String) -> Bool    // @total
fn host(u: Url) -> Option<String>
fn path(u: Url) -> String
fn query_param(u: Url, name: String) -> Option<String>
fn template(parts: List<String>, args: List<UrlPart>) -> Url
fn encode_component(s: String) -> String
type UrlReason = Idna | InvalidScheme | InvalidHost | InvalidPort | Relative | OpaquePath
error UrlError(reason: UrlReason)
type UrlParts = { scheme: String, username: String, password: Option<Secret<String>>, host: Option<String>, port: Option<Int>, path: String, query: Option<String>, fragment: Option<String> }
fn parse(s: String) -> Result<UrlParts, UrlError>
fn resolve(base: UrlParts, relative: String) -> Result<UrlParts, UrlError>
fn to_string(p: UrlParts) -> String
fn to_url(p: UrlParts) -> Url requires is_valid(to_string(p))
fn origin(p: UrlParts) -> String
fn with_scheme(p: UrlParts, scheme: String) -> Result<UrlParts, UrlError>
fn with_host(p: UrlParts, host: String) -> Result<UrlParts, UrlError>
fn with_port(p: UrlParts, port: Option<Int>) -> Result<UrlParts, UrlError>
fn with_path(p: UrlParts, path: String) -> Result<UrlParts, UrlError>
fn with_query(p: UrlParts, query: Option<String>) -> UrlParts
fn with_fragment(p: UrlParts, fragment: Option<String>) -> UrlParts
fn with_credentials(p: UrlParts, username: String, password: Option<Secret<String>>) -> Result<UrlParts, UrlError>    // @trust
fn search(p: UrlParts) -> SearchParams
fn with_search(p: UrlParts, sp: SearchParams) -> UrlParts
```

### `std/urlpart`

```sakura fragment
opaque type UrlPart = UrlPart(String)
fn from<T: Show>(v: T) -> UrlPart
fn to_string(p: UrlPart) -> String
```

### `std/urlpattern`

```sakura fragment
type PatternReason = Unsupported(String) | Duplicate(String) | Limit
error PatternError(reason: PatternReason)
opaque type UrlPattern = { origin: Option<Origin>, parts: List<Part> }
type UrlMatch = { groups: List<(String, String)> }
fn compile(pattern: String) -> Result<UrlPattern, PatternError>
fn exec(p: UrlPattern, url: Url) -> Option<UrlMatch>
```

### `std/uuid`

```sakura fragment
type Uuid = String where is_canonical(it)
error UuidError(input: String)
fn to_string(u: Uuid) -> String
fn parse(text: String) -> Result<Uuid, UuidError>
fn from_canonical(text: String) -> Uuid requires is_canonical(text)
fn is_canonical(text: String) -> Bool    // @total
```

### `std/websocket`

```sakura fragment
type Message = Text(String) | Binary(Bytes)
type CloseInfo = { code: Int, reason: String, clean: Bool }
type Received = Data(Message) | Closed(CloseInfo)
type SocketReason = Connect(String) | MessageTooLarge | NotOpen | Failed(String) | Accept(String)
error SocketError(reason: SocketReason)
type Options = { subprotocols: List<String>, headers: List<(String, String)>, max_message: Int }
fn options() -> Options
type Upgrade = { subprotocol: Option<String>, origins: List<String>, headers: List<(String, String)>, max_message: Int }
fn upgrade() -> Upgrade
type Handshake = { key: String, subprotocols: List<String>, origin: Option<String> }
opaque type Socket = { id: Int, protocol: String, max_message: Int }    // @resource
fn protocol(sock: Socket) -> String
effect WebSocket {
  fn connect<T, e>(url: String, options: Options, body: fn(Socket) -> T ! e) -> T ! {Fail<SocketError> | e} requires options.max_message > 0
  fn send(sock: Socket, message: Message) -> Unit ! {Fail<SocketError>}
  fn receive(sock: Socket) -> Received ! {Fail<SocketError>}
  fn close(sock: Socket, code: Int, reason: String) -> Unit ! {Fail<SocketError>} requires closable(code, reason)
  fn accept<e>(req: Request, options: Upgrade, body: fn(Socket) -> Unit ! e) -> Response ! {Fail<SocketError> | e} requires options.max_message > 0
  fn subscribe(sock: Socket, topic: String) -> Unit ! {Fail<SocketError>} requires String.len(topic) <= 256
  fn unsubscribe(sock: Socket, topic: String) -> Unit ! {Fail<SocketError>} requires String.len(topic) <= 256
  fn publish(topic: String, message: Message, except: Option<Socket>) -> Int ! {Fail<SocketError>} requires String.len(topic) <= 256
}
fn handshake(req: Request) -> Result<Handshake, String>
fn upgrade_request(path: String, subprotocols: List<String>) -> Request
fn closable(code: Int, reason: String) -> Bool    // @total
fn messages(sock: Socket) -> Source<Message, {WebSocket, Fail<SocketError>}>
fn host() -> Handler<WebSocket>
type ScriptStep = Reply(Message, Message) | Push(Message) | Close(Int, String)
fn scripted(script: List<ScriptStep>) -> Handler<WebSocket>
```

<!-- agent_docs: signatures end -->

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
   `from_list` (`Random.bytes`), `Uuid.from_canonical`/`is_canonical` (for `Random.uuid()`), `Gen.*` combinators and
   `Gen.for_all`, `Console.captured`/`Log.captured` forwarding to `Test.output`.
8. Resolved (§17.79): **`List.par_map`** is `std/task.par_map` permanently: its row performs
   `Task`, and `std/list` stays pure. `Env.host(scope)` is gone (§17.59).
9. **`Log.host()` and `Env.host()`** exist because the JS entry point installs each declared
   std effect's `host()`; `Log.host()` is `stderr()`. `Env.host()` exposes only the manifest's
   `env` keys.
10. **`String.parse_int`/`parse_float`** follow Rust's `str::parse` (a leading `+` is accepted;
    floats accept `NaN`/`inf`), matching the runtime.
11. **One home module per type in the web and AI surface:** `std/request` (`Request`,
    `Method`, `BadRequest`), `std/response` (`Response`, `Status`, `HttpError`) and
    `std/prompt` (`Prompt`, `PromptPart`) are separate from `std/http` and `std/llm`, because
    `Request.json`/`Response.json`, `Request.post_json`/`Http.post_json` and
    `Prompt.of`/`Model.of` would otherwise be one name (§4.9). `std/budget` and `std/llm`
    replace the `std/ai` placeholder.
12. Resolved (§17.57): `Request.secret_headers: List<(String, Secret<String>)>` replaces
    `bearer`; host handlers send each entry, nothing shows, matches or records it.
13. **Parameterless `Fs.host()` and `Http.host()`** for the entry point (as item 9);
    `Fs.memory(files)` is `Fs.rooted` over an in-memory directory, so `Dir.subdir` and
    `Fs.rooted` compose over it as on the host (§17.119). `Cli.exit` under `Cli.of` is
    a defect (`D030`). `Llm.scripted` reports undecodable output as `InvalidOutput` with one
    `Violation` whose `clause` is the decoder's message and `value_summary` its JSON Pointer.
14. **`Trust` has its home in `std/core`**; it is compiler-known and in the prelude (§17.16),
    and `@trust("…")` is accepted only at a `Trust` parameter (`V6001`). `Untrusted.check(u, clause, pred)` takes the clause text for
    its `Violation`; `Secret.of` and `Secret.map` are additions for tests and derived
    credentials; `map` takes a `Trust` reason like `expose` (§17.159 (2)).
15. **`Decimal` is `DecimalRepr where is_canonical(it)`** over `{ whole: Int, frac: List<Int> }`
    (floor and fractional digits, no trailing zeros; a refinement since §17.130, so its JSON
    form is the record and decoding checks it), not `{ units: Int, scale: Int }`: derived `Ord` compares fields in order,
    which is numeric only for this form. `div_checked(a, b, places)` and `round` round half to
    even. Interpolation shows the record form; `Decimal.to_string` gives `-12.34`.
16. **`Tool<e>` and `Agent<T, e>`** (resolved, §17.49): a tool runs under the caller's
    handlers (§11.2), so its type carries the effects its function performs as a row
    parameter. `Agent.run`'s row gains the tools' `e` (§15.8's
    `research` already declares `Http` for that reason). `Tool.from(name, description, f)`
    is what `Tool.of` builds (§17.66); `ToolError` is also raised as `DecodeError` (bad
    arguments) and `UnknownTool`.
17. **`Llm.step` and the scripted tool-call format** (§16.15): the agent loop needs an
    operation that returns either the answer or tool calls (`Step<T> = Answer(T) |
    Use(List<ToolCall>)`), given the tool specs and the transcript so far. A scripted output
    that is a JSON object with a `tool_use` array is a tool-call turn. `Partial<T>` is
    `{ text, step, calls }`. `Llm.stream`'s callback may perform effects (`! e`), otherwise
    a partial could not be observed. `Agent.run` reserves `max_tokens` and one call per step
    and settles to zero, as a headroom check; the provider charges actual use.
18. **`Router<e>`** (resolved, §17.49) for the same reason as `Tool<e>`, and `Serve.listen<e>` (a function over the one operation `Serve.serve(handle, addr)`,
    §17.48) performs what the routes perform, so what is installed around `listen` serves
    every request (§10.4). `Router.route`, `put`, `delete` and `handle` are
    additions; `:name` path segments fill the new `Request.params`, which `Request.param`
    reads before the query. `Serve.scripted(requests)` is the test handler; `Address.of`
    lives in `std/serve`.
19. **`Html.raw(markup, @trust("…"))`** instead of `Html.raw(s: TrustedHtml)`: one trust type
    for every sink (§6.4). `Html.untrusted(u)` escapes untrusted text. In `html"…"` an `Html`
    value is markup and any other value goes through `Html.from` as escaped text (§17.77); an
    interpolated `Untrusted<T>` goes through `Html.from` as its wrapped value, so
    `Untrusted<String>` is the escaped raw text (§17.101).
20. Resolved (§17.48): `read_chunk`/`write_chunk` are `Fs` operations; a `with_file` body
    runs under its perform site's handlers.
21. **`Path` is `String where is_relative(it)`**, not `pub opaque type Path = Path(List<String>)`:
    `Cli.parse` and `Json.decode` read a refined type from a string and check it, while an
    opaque variant has no string form (§15.6 parses `--input notes.txt` into a `Path`). The
    guarantee is the same: no leading `/`, no `..` segment. `Path.of` drops `.` and empty
    segments; a decoded `Path` keeps them.
22. **No `Sql.postgres`**: design §10.2 names it the second provider; it needs a native driver
    or a runtime dependency and has no release assigned (owner decision); `Sql.sqlite` and
    `Sql.memory` cover the corpus.
