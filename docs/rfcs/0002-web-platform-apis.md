# RFC 0002: Web platform APIs

Sakura programs should be able to do everything a program on a modern JavaScript runtime
can do with the web platform APIs: the whole inventory of
[Bun's Web APIs page](https://bun.com/docs/runtime/web-apis), plus the nine related APIs and
the eight stream classes that page implies. This RFC fixes the Sakura shape of every entry:
typed values, `Result` and `Fail<E>` instead of exceptions, explicit effects and manifest
permissions, bounded resources, and one deterministic test handler per effect. "All of them"
means every capability is delivered on every backend or refused with a typed, visible error;
it does not mean JavaScript object semantics are copied. Where the web shape conflicts with
Sakura's principles (mutable `EventTarget`, `structuredClone` of arbitrary graphs,
shared-memory workers, dynamic code in `ShadowRealm`), the Sakura form is decided here and
the reason recorded. The implementation order is
[docs/plans/web-apis.md](../plans/web-apis.md); the decision is design §17.262.

## Metadata

- Number/title: 0002, Web platform APIs.
- Champion: @icantcodefyi (owner; the request of 2026-10-19 "check bun's web APIs, we
  should include all of them in our language").
- Authoring: design role, Claude Fable 5.1, from GPT-6-Astra's research
  [research/15-web-apis.md](../../research/15-web-apis.md) (verified against the code at
  `7b4f55b5` before use). The human contributor owns acceptance.
- Discussion and PR: not opened; drafted on `main` at the owner's request. The pitch is the
  owner's message and design §17.262 (round 34).
- Decision status: Draft.
- Implementation status: Phase 0 (`7dd9dd47`) and phase 1 (`d0222605`) are on `main` and
  ship in `v0.1.0`; phases 2–3 and 7a may follow before the tag, 4a/4b are held for it
  (design §17.263). Phases and owners are in [docs/plans/web-apis.md](../plans/web-apis.md).
- Stabilization status: Not stabilized. §Compatibility states which parts are intended
  for the `v0.2.0` contract and which stay experimental.
- FCP start/end: Not started; the maintainer records seven calendar days under the
  [RFC process](README.md) before acceptance.
- Decision date/owner: Not decided; @icantcodefyi.

## Motivation

Agents write programs that fetch, parse URLs, decode text, hash and sign, stream bodies,
talk over WebSockets and run work in parallel. Today Sakura's std covers HTTP (`Http`,
`Request`, `Response`), a lightweight `Url` refinement, strict UTF-8 conversion, typed JSON,
`Random.bytes`/`Random.uuid`, `Clock`, `Console`/`Log` and structured concurrency
(`Task`); the gaps are the rest of Bun's page (see the inventory table below, verified
against `compiler/std/INDEX.md` and `compiler/runtime/js/sakura_runtime.js` at `7b4f55b5`).
An agent that reaches for `TextDecoder`, `URLSearchParams`, `crypto.subtle.digest` or
`WebSocket` and finds nothing either gives up or re-implements the web standard badly in
user code. The std is the right place: once, tested against the standards' vectors, on
every backend, under the capability model.

Goals:

1. Every entry of the Bun page has a Sakura counterpart that delivers the same capability,
   listed in one table with its module, effect row, permission and backend status.
2. The counterpart is idiomatic: immutable values, typed errors, explicit effects, scoped
   resources, bounded queues and sizes, deterministic test handlers.
3. Parity: the same program gives the same stdout, effect trace and typed errors on JS,
   WASM and native, except where this RFC states a declared difference (compressed bytes,
   randomized signatures).
4. Nothing is silent: a backend that lacks a service refuses the program at startup with a
   message naming the effect and the target; an unsupported algorithm, encoding or pattern
   feature is a typed error naming it.

Non-goals:

- Source-level compatibility with JavaScript (`new Headers()`, `addEventListener`,
  promises, `any`). Sakura has no JS interop layer and this RFC adds none.
- Bun-specific APIs (`Bun.serve` upgrade hooks, `Bun.file`, `ArrayBufferSink`, TCP/UDP,
  FFI). They are a separate product surface (research/15 §12).
- The browser-only entries: the DOM, History, object URLs (`URL.createObjectURL`), and
  the modal dialogs as UI. `alert`/`confirm`/`prompt` are delivered as console helpers,
  as Bun does.
- A WebSocket **server** (`Serve` upgrade): not on the page; a follow-up to `std/serve`.
- Shared memory (`SharedArrayBuffer`, `Atomics`): not on the page and excluded by §8.1.

## Concrete examples and guide-level explanation

Every signature below is proposed, not implemented. Fences are labelled `fragment`.

A JSON API call with a parsed URL, validated headers and a digest of the body:

```sakura fragment
import std/url.{UrlParts}
import std/headers.{Headers}
import std/crypto

fn upload(base: Url, name: String, data: Bytes) -> String ! {Http, Fail<HttpError | UrlError>} {
  let parts = try Url.parse(base)
  let target = parts.with_path("/v1/files/" ++ Url.encode_component(name)).to_url()
  let digest = Bytes.to_base64(Crypto.digest(Sha256, data), Standard, true)
  let req = Request.of(Post, target)
    .with_header("content-type", "application/octet-stream")
    .with_header("x-digest", "sha-256=" ++ digest)
    .with_payload(Bytes(data))
  let resp = Http.send(req)
  let etag = Headers.of(resp.headers).get("etag")
  etag.unwrap_or("")
}
```

Decoding text in a declared encoding, with the error policy in the type:

```sakura fragment
import std/encoding.{Encoding, Errors, decode}

fn latin1_to_string(b: Bytes) -> String ! {Fail<EncodingError>} {
  try decode(b, Windows1252, Errors.Fatal)      // Replace substitutes U+FFFD instead
}
```

A WebSocket client under the manifest's `net` grant, with a scripted peer in the test:

```sakura fragment
import std/websocket.{WebSocket, Message, Received, CloseInfo}

fn echo_once(url: Url) -> String ! {WebSocket, Fail<SocketError>} {
  WebSocket.connect(url, WebSocket.options(), fn(sock) {
    WebSocket.send(sock, Text("ping"))
    match WebSocket.receive(sock) {
      Text(s) -> s,
      Binary(_) -> "binary",
      Closed(info) -> "closed " ++ show(info.code),
    }
  })
}

test "echo" {
  with WebSocket.scripted([Reply(Text("ping"), Text("pong"))]) {
    expect_eq(echo_once(url"wss://example.test/echo"), "pong")
  }
}
```

A pull-based stream pipeline: an HTTP body, gunzipped, decoded as UTF-8 and split into
lines, with every bound explicit:

```sakura fragment
import std/stream.{Source}
import std/compress

fn count_lines(url: Url) -> Int ! {Http, Fail<HttpError | CompressionError | EncodingError>} {
  Http.stream(Request.of(Get, url), fn(incoming) {
    let bytes = Http.body_source(incoming.body, 65_536)
    let text = Encoding.decode_source(Compress.gunzip_source(bytes, 64 * 1_048_576), Utf8, Fatal)
    Stream.fold(Stream.lines(text), 0, fn(n, _line) { n + 1 })
  })
}
```

A worker with attenuated authority, typed messages and a deterministic in-process test
handler (experimental, see §Compatibility):

```sakura fragment
import std/worker.{Worker, Grants}

@worker
fn resize(inbox: Inbox<Bytes>, outbox: Outbox<Int>) -> Unit ! {Worker} {
  loop { match Worker.recv(inbox) { Some(b) -> Worker.send(outbox, Bytes.len(b)), None -> break } }
}

fn main() -> Unit ! {Console, Worker} {
  Worker.spawn(resize, Grants.none(), fn(w) {
    Worker.send(w, Bytes.from_list([1, 2, 3]))
    Console.print(show(Worker.recv(w)))        // Some(3)
  })
}
```

Rejected uses, each a diagnostic or a typed error rather than a surprise:

- `Headers.of([("content length", "3")])` is `Err(HeaderError(InvalidName("content length")))`.
- `Bytes.from_base64("ab=c", Standard)` is `Err(Base64Error(...))`; the lenient decoder
  (`atob` semantics) is a different function, `Bytes.from_base64_lenient`.
- `Crypto.digest(Sha1, b)` is `V1002 unresolved_name` at check time, because
  `Sha1` is not a `Digest` variant in 0.2.0; `Crypto.sign(key, b)` with a key whose
  `usages` lack `Sign` is `Err(KeyUsage)`.
- A `WebSocket.connect` to a host outside `net` is `Err(SocketError("… is not in the
  manifest's net list"))`, exactly as `Http.host()` answers today.
- `Compress.gunzip(b, max_out)` whose output would exceed `max_out` is
  `Err(CompressionError(OutputLimit))`; there is no unbounded form.
- A program whose `main` declares `WebSocket` run with `--target native` before the native
  host has the service exits 75 at startup: "`WebSocket` is not available on target
  `native`"; it never reaches first use.

What a human or agent must know: there are no event listeners. Every web "event" is a
value returned by a receive operation (`WebSocket.receive`, `Worker.recv`,
`Channel.recv`), so control flow is a `match`, cancellation is the scope's, and nothing
runs after the scope ends. There is no `structuredClone`: values are already copied by
assignment (§8.1), and the only serialization that crosses an instance boundary is the
`Schema` codec. There is no `Async`: suspension stays a handler property (§5.6).

## Normative semantics

### 1. Inventory and mapping

The table lists each entry of Bun's page, the nine related APIs the research names, and the
eight stream classes collapsed into `std/stream`. "Today" is verified coverage at
`7b4f55b5`. "Shape" names the proposed module and type. "Row" is the effect row of the
operations beyond `Fail<E>`; `pure` means an empty row. "Perm" is the manifest key.
Backends: `JS`, `WASM`, `native`; `✓` planned in this RFC, `host` means the WASM module
imports it from its host (ABI §8.2 `hosted`/`async` shapes), `rt` means a shared
`runtime/rt` implementation used by WASM and native, `refuse` means the backend refuses
the program at startup (rule §7), `gap` means deliberately not provided, with the reason in
§2.

| Web API | Today | Shape | Row | Perm | JS | WASM | native |
|---|---|---|---|---|---|---|---|
| `fetch` | `Http.send/get/post_json/stream` | `std/http` unchanged; redirect policy §2.4 | `Http` | `net` | ✓ | host | refuse until N1b |
| `Request` | `std/request` (text body, 6 methods) | `Request` gains `Payload`, `Options`/`Trace`, `redirect` (0.2.0) | pure | — | ✓ | ✓ | ✓ |
| `Response` | `std/response` (text body) | `Response.body: Payload`; `bytes(r, limit)` | pure | — | ✓ | ✓ | ✓ |
| `Headers` | `List<(String, String)>` fields | `std/headers.Headers` validated view | pure | — | ✓ | ✓ | ✓ |
| `AbortController` | scope cancellation only | `Task.cancel(f: Fiber<T>)` | `Task` | — | ✓ | ✓ | ✓ |
| `AbortSignal` | `std/task.timeout`, `race` | `timeout`, `race`, `Task.cancelled(s: Scope)` | `Task` | — | ✓ | ✓ | ✓ |
| `URL` | `Url` refinement, string splitting | `std/url.UrlParts` WHATWG parser; `Url` stays | pure | — | ✓ | ✓ | ✓ |
| `URLSearchParams` | `Url.query_param` (raw) | `std/searchparams.SearchParams` | pure | — | ✓ | ✓ | ✓ |
| `Worker` | none | `std/worker` (experimental) | `Worker` | inherited subset | ✓ | host | ✓ (thread) |
| `self.postMessage` | none | `Worker.send(outbox, m)` | `Worker` | — | ✓ | host | ✓ |
| `structuredClone` | value semantics | gap by design; `Schema` codec is the wire form | — | — | gap | gap | gap |
| `MessagePort` | none | `std/channel.{Sender, Receiver}` | `Task` | — | ✓ | ✓ | ✓ |
| `MessageChannel` | none | `Channel.with(capacity, body)` | `Task` | — | ✓ | ✓ | ✓ |
| `BroadcastChannel` | none | `Worker.topic(name)` in-process fan-out | `Worker` | — | ✓ | host | ✓ |
| `ReadableStream` | `Http.read_chunk`, `Fs.read_chunk` | `std/stream.Source<T, e>` | source's row | source's | ✓ | ✓ | ✓ |
| `WritableStream` | `Fs.write_chunk` | `std/stream.Sink<T, e>` | sink's row | sink's | ✓ | ✓ | ✓ |
| `TransformStream` | none | `fn(Source<A, e>) -> Source<B, e>` | pure | — | ✓ | ✓ | ✓ |
| `ByteLengthQueuingStrategy` | none | `Stream.buffer(src, Limit.bytes(n))` | `Task` | — | ✓ | ✓ | ✓ |
| `CountQueuingStrategy` | none | `Stream.buffer(src, Limit.items(n))` | `Task` | — | ✓ | ✓ | ✓ |
| 8 reader/writer/controller classes | none | internal to `Source`/`Sink`; BYOB is a gap | — | — | — | — | — |
| `Blob` | `Bytes` | `std/blob.Blob { bytes, media_type }` | pure | — | ✓ | ✓ | ✓ |
| `WebSocket` (client) | none | `std/websocket`, effect `WebSocket` | `WebSocket` | `net` | ✓ | host | refuse until ported |
| `Uint8Array` | `Bytes` (immutable) | `Bytes.slice`, `concat`; mutation is a gap | pure | — | ✓ | rt | rt |
| `Uint8Array.prototype.toBase64` | none (internal only) | `Bytes.to_base64(b, Alphabet, padded)` | pure | — | ✓ | rt | rt |
| `Uint8Array.fromBase64` | none | `Bytes.from_base64(text, Alphabet)` strict | pure | — | ✓ | rt | rt |
| `TextEncoder` | `String.to_bytes` (UTF-8) | unchanged; `Encoding.encode(s, enc)` for UTF-16 | pure | — | ✓ | rt | rt |
| `TextDecoder` | `String.from_bytes` (strict UTF-8) | `std/encoding.decode`, `Decoder` state | pure | — | ✓ | rt | rt |
| `atob` | none | `Bytes.from_base64_lenient(text)` | pure | — | ✓ | rt | rt |
| `btoa` | none | `Bytes.to_base64` (compose with `String.to_bytes`) | pure | — | ✓ | rt | rt |
| `JSON` | `std/json` typed | unchanged; `Json.render(j, indent)` | pure | — | ✓ | ✓ | ✓ |
| `setTimeout` | `Clock.sleep` + `Task.spawn` | idiom; no new API | `Clock`, `Task` | — | ✓ | ✓ | ✓ |
| `clearTimeout` | none | `Task.cancel(f)` | `Task` | — | ✓ | ✓ | ✓ |
| `setInterval` | none | `Task.every(period, body)` | `Clock`, `Task` | — | ✓ | ✓ | ✓ |
| `clearInterval` | none | `body` returns `false`, or `Task.cancel(f)` | `Task` | — | ✓ | ✓ | ✓ |
| `queueMicrotask` | `Task.yield_now` (different) | idiom: `Task.spawn` in the current scope | `Task` | — | ✓ | ✓ | ✓ |
| `crypto.getRandomValues` | `Random.bytes` | unchanged | `Random` | — | ✓ | ✓ | ✓ |
| `crypto.randomUUID` | `Random.uuid` | unchanged | `Random` | — | ✓ | ✓ | ✓ |
| `SubtleCrypto` | none | `std/crypto`: digest, hmac, aead, kdf, sign/verify, ecdh | pure; `Random` for keys and signing | — | ✓ | host (0.2.0), rt later | rt |
| `CryptoKey` | none | `std/crypto.CryptoKey` opaque value over `Secret<Bytes>` | pure | — | ✓ | ✓ | ✓ |
| `console` | `Console`, `Log` | unchanged; `console.error` is `Log.event(Error, …)` | `Console`, `Log` | — | ✓ | ✓ | ✓ |
| `performance` | `Clock.monotonic` (ms) | `Clock.monotonic_micros()` (0.2.0) | `Clock` | — | ✓ | ✓ | ✓ |
| `reportError` | `Log.event` | idiom; no new API | `Log` | — | ✓ | ✓ | ✓ |
| `alert` | `Console.print/read_line` | `Console.alert(msg)` | `Console` | — | ✓ | ✓ | ✓ |
| `confirm` | same | `Console.confirm(msg) -> Bool` | `Console` | — | ✓ | ✓ | ✓ |
| `prompt` | same | `Console.prompt(msg, default) -> Option<String>` | `Console` | — | ✓ | ✓ | ✓ |
| `ShadowRealm` | none | gap by design; isolation is a `Worker` with `Grants.none()` | — | — | gap | gap | gap |
| `EventTarget` | none | gap by design; the idiom is a list of callbacks | — | — | gap | gap | gap |
| `Event` | none | typed variants returned by receive operations | — | — | ✓ | ✓ | ✓ |
| `ErrorEvent` | none | `Worker.recv` returns `Failed(WorkerError)` | `Worker` | — | ✓ | host | ✓ |
| `CloseEvent` | none | `std/websocket.CloseInfo` | — | — | ✓ | host | ✓ |
| `MessageEvent` | none | the typed message itself (`Received`, `Option<O>`) | — | — | ✓ | ✓ | ✓ |
| `File` | `Fs.File` is a resource | `std/blob.FileData { blob, name, last_modified }` | pure | — | ✓ | ✓ | ✓ |
| `FormData` | none | `std/form.Form`, multipart and urlencoded codecs | pure | — | ✓ | ✓ | ✓ |
| `URLPattern` | `Router` `:name` segments | `std/urlpattern.UrlPattern` (subset, no regexp) | pure | — | ✓ | ✓ | ✓ |
| `TextEncoderStream` | none | `Encoding.encode_source(src, enc)` | source's | — | ✓ | ✓ | ✓ |
| `TextDecoderStream` | none | `Encoding.decode_source(src, enc, errors)` | source's | — | ✓ | ✓ | ✓ |
| `CompressionStream` | none | `std/compress.{gzip, deflate, deflate_raw}` + `_source` | pure / source's | — | ✓ | rt | rt |
| `DecompressionStream` | none | `Compress.{gunzip, inflate, inflate_raw}(b, max_out)` | pure / source's | — | ✓ | rt | rt |
| `CustomEvent` | none | a user variant; no API | — | — | — | — | — |
| `DOMException` | typed errors | each module's `error` tags; no shared type | — | — | — | — | — |

Every `gap` row is also a documentation row: the stdlib page and the skill name the
capability, the Sakura form that covers the need, and the reason.

### 2. Shapes by group

#### 2.1 Bytes, Base64 and text encodings (`std/bytes`, `std/encoding`)

- `Bytes.slice(b, start, end) -> Bytes` clamps like `List.slice`; `Bytes.concat(parts:
  List<Bytes>) -> Bytes` (§17.206 (4) held it for a second caller; `Blob`, `Form` and the
  stream combinators are those callers). No in-place update exists: a "mutable typed array"
  is `Bytes.from_list(List.set(...))` or a `Sink`.
- `type Alphabet = Standard | Url`. `to_base64(b, alphabet, padded: Bool) -> String`.
  `from_base64(text, alphabet) -> Result<Bytes, Base64Error>` is strict: no whitespace,
  canonical padding for `Standard`, padding optional for `Url`, a non-zero trailing-bit
  remainder is an error (ECMAScript `lastChunkHandling: "strict"`). `from_base64_lenient(text)`
  implements HTML's forgiving-base64 (`atob`): ASCII whitespace removed, padding optional.
  `error Base64Error(offset: Int, reason: String)`. `btoa`'s byte-string input form is not
  provided: text goes through `String.to_bytes` first.
- `type Encoding = Utf8 | Utf16Le | Utf16Be | Windows1252`; `Encoding.parse(label) ->
  Result<Encoding, EncodingError>` accepts the WHATWG labels of those four (`latin1`,
  `iso-8859-1` and `ascii` map to `Windows1252` as the standard requires); any other label
  is `Err(EncodingError(Unsupported(label)))`, never a fallback. `type Errors = Fatal |
  Replace`; `type Bom = Strip | Keep`. `decode(b, enc, errors) -> Result<String,
  EncodingError>` strips a BOM; `decode_with(b, enc, errors, bom)` chooses. Incremental
  decoding threads an immutable state: `Decoder.new(enc, errors) -> Decoder`,
  `Decoder.push(d, chunk) -> Result<(Decoder, String), EncodingError>`,
  `Decoder.finish(d) -> Result<String, EncodingError>`. `encode(s, enc) -> Bytes` for
  `Utf16Le`/`Utf16Be`, declared `requires encodable(enc)`: Windows-1252 cannot encode every
  string, so it is decode-only and passing it is the caller defect `D020`, a fact about the
  program, not its input (`TextEncoder` is UTF-8 only; §17.263 (3)); `String.to_bytes` stays
  the UTF-8 path. `encodeInto` is a gap: there
  is no caller-owned buffer to write into.
- `String.from_bytes` keeps its strict whole-buffer UTF-8 contract.

#### 2.2 URLs (`std/url`, `std/searchparams`, `std/urlpattern`)

- `Url` stays `String where is_valid(it)` with today's predicate: the `url"…"` template,
  `Request.url` and every existing program keep their contract. The parsed form is a new
  record, `UrlParts { scheme, username, password: Option<Secret<String>>, host, port:
  Option<Int>, path, query: Option<String>, fragment: Option<String> }`, produced by
  `Url.parse(s: String) -> Result<UrlParts, UrlError>` following the WHATWG URL parser for
  the special schemes (`http`, `https`, `ws`, `wss`, `file`) and opaque paths for others.
  `Url.resolve(base: UrlParts, relative: String) -> Result<UrlParts, UrlError>` is base
  resolution. `UrlParts.to_url(p) -> Url`, `to_string`, `origin(p) -> String`, and one
  `with_<component>` per component returning a new value. Serialization is the standard's.
  Hosts are ASCII-lowercased; a non-ASCII host is `Err(UrlError(Idna))` in 0.2.0, never
  mangled; punycode is a later additive change. A URL value grants nothing: `net` is checked
  by the handler at send time.
- `SearchParams` is an ordered multimap: `SearchParams.parse(query) -> SearchParams` (never
  fails; `application/x-www-form-urlencoded`, `+` is a space), `get`, `get_all`, `has`,
  `append`, `set`, `remove`, `sort` (stable, by name, code-unit order as the standard says),
  `entries`, `to_string`. `UrlParts.search(p) -> SearchParams` and `with_search(p, sp)`
  attach it. No live aliasing with a parent URL.
- `UrlPattern.compile(pattern) -> Result<UrlPattern, PatternError>` with the subset
  `:name` segments, `*` and `**` wildcards, and optional pathname-only or full-URL patterns;
  regexp groups `(…)` are `Err(PatternError(Unsupported(…)))`, because no JS regexp engine
  exists on WASM or native. `UrlPattern.exec(p, url) -> Option<UrlMatch>` with named
  groups. `std/router` matches through it, so the two never disagree.

#### 2.3 Headers, Blob, FileData, Form (`std/headers`, `std/blob`, `std/form`)

- `Headers` is a validated, immutable, ordered multimap. `Headers.of(list) ->
  Result<Headers, HeaderError>` validates names (token) and values (no CR/LF, trimmed) per
  Fetch; `get(h, name)` joins repeats with `", "` as Fetch does, except `set-cookie`, which
  only `get_set_cookie(h) -> List<String>` returns; `get_all`, `has`, `append`, `set`,
  `remove`, `entries` (lowercase names, sorted, combined), `to_list`. Secret headers stay in
  `Request.secret_headers`; `Headers` never holds a `Secret`. Forbidden-header rules are
  browser policy and do not apply.
- `Blob { bytes: Bytes, media_type: String }` with `Blob.of(bytes, media_type)`, `size`,
  `slice(b, start, end, media_type)`, `text(b) -> Result<String, Utf8Error>` (UTF-8 only,
  as the File API says), `source(b, chunk)` for streams. `FileData { blob: Blob, name:
  String, last_modified: Instant }` is the `File` interface; it is a value and names no file
  on disk. Reading a disk file into it is `Fs` work under the grant.
- `Form` is an ordered list of `(String, Part)`, `Part = Text(String) | File(FileData)`.
  `Form.encode_urlencoded(f) -> String`; `Form.encode_multipart(f, boundary) -> Bytes`
  with an explicit boundary for deterministic output and `Form.boundary() -> String !
  {Random}` for a fresh one; `Form.parse_urlencoded(text) -> Form`;
  `Form.parse_multipart(bytes, content_type, limits) -> Result<Form, FormError>` with
  `FormLimits { max_parts, max_bytes, max_field_bytes }` required. A part's filename is
  data; nothing is read from disk.

#### 2.4 HTTP in 0.2.0 (`std/request`, `std/response`, `std/http`)

- `type Payload = Empty | Text(String) | Bytes(Bytes)`. `Request.body` and `Response.body`
  become `Payload`. `Request.of`, `post`, `post_json`, `Response.status`, `ok_json`,
  `html` keep their signatures and produce `Text`/`Empty`. `Response.bytes(r, limit) ->
  Bytes ! {Fail<HttpError>}` joins `text` and `json`. `Request.with_payload`,
  `with_header`, `with_headers(h: Headers)` are the builders.
- `Method` gains `Options` and `Trace`. `Connect` is a proxy verb and is not added.
- `Request.redirect: Redirect = Follow | Manual | Error` (default `Follow` from the
  builders). Under `Follow`, `std/http` follows at most 20 hops in one Sakura loop shared
  by `Http.host()` and `Http.scoped()`: every send reaches the host with `redirect: "manual"`,
  each hop is its own send, so **every hop's `host:port` is checked against `net` before it is
  fetched** and against `scoped`'s list when there is one, and a refused hop fails
  `HttpError` with the same message as a refused first request; `authorization` and every
  `secret_headers` entry are dropped when the hop changes origin, as Fetch does.
  `Manual` returns the 3xx response. `Error` fails `HttpError(status: Some(3xx), …)`. Today
  the JS host calls `fetch` with its default `redirect: "follow"` after checking only the
  initial URL (`sakura_runtime.js` `fetchArgs`), so a granted host can redirect the program
  to an ungranted one. That is a security fix independent of the field: it lands first,
  as plan phase 0, in whichever version is next.
- `Http.body_source(body: Body, chunk: Int) -> Source<Bytes, {Http, Fail<HttpError>}>`
  adapts a streamed body. Nothing else in `std/http` changes.

#### 2.5 Streams (`std/stream`)

- `type Source<T, e> = { pull: fn() -> Option<T> ! e, close: fn() -> Unit ! e }` and
  `type Sink<T, e> = { write: fn(T) -> Unit ! e, close: fn() -> Unit ! e, abort:
  fn(String) -> Unit ! e }` are plain records of closures. Pull returns `None` exactly once
  at the end; an empty chunk is data. A transform is a function from a source to a source.
  There are no locks: one instance runs on one thread (§17.171) and a cursor pulled from two
  copies interleaves deterministically; the docs say so.
- Combinators (pure unless stated): `from_list`, `to_list(src, max_items)`, `map`,
  `filter`, `take`, `fold`, `each`, `chunks(src, size)`, `lines(src)`, `pipe(src, sink)`,
  `tee(src, max_lag)` (`Task`; the slower branch is at most `max_lag` items behind, then
  the faster one waits), `buffer(src, limit)` (`Task`; a producer fiber prefetches up to
  `Limit.items(n)` or `Limit.bytes(n)`, the two queuing strategies). Every bound is an
  argument; no combinator buffers without one.
- `Fs.file_source(file, chunk)` and `Fs.file_sink(file)` adapt files. BYOB readers and byte
  controllers are a gap: there is no caller-owned buffer in Sakura.

#### 2.6 Compression (`std/compress`)

- `type Format = Gzip | Deflate | DeflateRaw`. Whole-buffer: `compress(b, format, level)
  -> Bytes`; `decompress(b, format, max_out) -> Result<Bytes, CompressionError>` with
  `max_out` required and `CompressionError = Malformed(offset) | OutputLimit | Truncated`.
  Streaming: `compress_source(src, format, level)` and `decompress_source(src, format,
  max_out)`. Decompressed bytes are a contract; compressed bytes are not (`zlib` on JS,
  the runtime's codec on WASM and native differ), so no fixture pins compressed output.

#### 2.7 Cryptography (`std/crypto`)

- `CryptoKey` is a `pub opaque` record `{ algorithm: Algorithm, usages: List<Usage>,
  extractable: Bool, material: Secret<Bytes> }`. It is a value: no `Show`, no `Schema`, no
  log and no cassette can carry it (`Secret` rules, §6.4). "Non-extractable" means
  `export_key` fails with `Err(KeyError(NotExtractable))`; it is a std-enforced policy, not
  a hardware boundary, and the docs say so.
- Tier 1, every backend: `Digest = Sha256 | Sha384 | Sha512`; `Hmac(Digest)`;
  `AesGcm(128 | 256)`; `Hkdf(Digest)`; `Pbkdf2(Digest)`; `Ecdsa(P256)`; `Ecdh(P256)`;
  `Ed25519`; `X25519`. Tier 2, JS and WASM-on-JS-host only in 0.2.0, native
  `Err(Unsupported)`: `RsaOaep`, `RsaPss`, `RsassaPkcs1`, `P384`, `P521`, `AesCbc`,
  `AesCtr`, `AesKw`. SHA-1 and MD5 are not offered.
- Operations: `digest(alg, b) -> Bytes` and `verify(key, signature, b) -> Bool` are pure.
  `hmac(key, b) -> Bytes` is pure. `encrypt(key, nonce, aad, plaintext) -> Result<Bytes,
  CryptoError>` and `decrypt` are pure with the nonce an argument (12 bytes for GCM;
  obtain it from `Random.bytes(12)`). `derive_bits(key, params, len)` is pure.
  `generate_key(alg, usages, extractable) -> CryptoKey ! {Random}` and `sign(key, b) ->
  Bytes ! {Random}` perform `Random` (ECDSA nonces and PSS salts; Ed25519 and HMAC signing
  ignore it). `import_key(format, alg, usages, extractable, material) -> Result<CryptoKey,
  KeyError>` and `export_key(key, format) -> Result<Bytes, KeyError>` with `KeyFormat = Raw
  | Pkcs8 | Spki | Jwk` (JWK as JSON text bytes).
- Entropy authority is `Random`: `Random.host()` is the OS CSPRNG on every backend;
  `Random.seeded` is for tests and the docs state that keys made under it are not secret.
  No `Crypto` effect and no manifest key: nothing here touches the network or disk.
- Signatures are not canonical bytes: ECDSA on JS is randomized (Web Crypto), on WASM and
  native deterministic (RFC 6979). Parity compares `verify` round trips, never signature
  bytes. Ed25519, HMAC and digests are canonical and are compared byte for byte.

#### 2.8 Time, scheduling and diagnostics (`std/clock`, `std/task`, `std/console`)

- `Clock` gains `monotonic_micros() -> Int` (microseconds since an arbitrary origin;
  `Clock.fixed` returns 0, `Clock.virtual` derives it from its millisecond state).
- `Task` gains `cancel(f: Fiber<T>) -> Unit` (cooperative; the fiber unwinds through its
  finalizers; the operation consumes the fiber, so a later `join(f)` is `D095`; cancelling
  a finished fiber is a no-op) and `cancelled(s: Scope) -> Bool`. `std/task` gains
  `every<e>(period: Duration, body: fn(Int) -> Bool ! e) -> Unit ! {Clock, Task | e}`:
  ticks are serialized, a tick that would start while the body runs is skipped and the
  count passed to the body says how many were skipped, the loop ends when the body returns
  `false` or the scope is cancelled. Under `Clock.virtual` ticks are reproducible.
- `setTimeout` is `Task.spawn(s, fn() { Clock.sleep(d); work() })`; `queueMicrotask` is
  `Task.spawn` without the sleep; `clearTimeout` is `Task.cancel(f)`. The docs show the
  idioms; no timer id namespace exists.
- `Console.alert(msg) -> Unit ! {Console}` prints `msg` and waits for a line; `confirm(msg)
  -> Bool` prints `msg [y/N]` and reads a line, where only `y`/`yes` (case-insensitive) is
  `true` and end of input is `false`; `prompt(msg, default: Option<String>) ->
  Option<String>` returns the line, the default on an empty line, `None` at end of input.
  Under `Console.captured` the input comes from the test's scripted lines. `reportError`
  and `console.error` are `Log.event(Level.Error, …)`; `console.time` is
  `Clock.monotonic_micros` plus a `Log.event`.

#### 2.9 Channels and workers (`std/channel`, `std/worker`; experimental)

- `Channel.with<T, R, e>(capacity: Int, body: fn(Sender<T>, Receiver<T>) -> R ! e) -> R !
  {Task | e}`: a bounded in-instance queue (`§16` item 13's "add bounded channels when
  needed"). `send(tx, v) -> Result<Unit, SendError>` waits while full, fails `Closed` after
  `close`; `recv(rx) -> Option<T>` waits while empty, `None` after close and drain.
  `Sender`/`Receiver` are external scoped types (§6.3): they cannot leave the callback.
  This is `MessageChannel`/`MessagePort` inside one instance.
- `Worker` is a **second Sakura instance** running a function marked `@worker` from the same
  program, with its own heap and handlers, connected by two typed bounded channels whose
  element types are `Schema` (the wire codec; closures, handlers, resources, `Secret` and
  `CryptoKey` cannot cross because they are not `Schema`). `Worker.spawn<I, O, R, e>(entry,
  grants: Grants, body: fn(WorkerHandle<I, O>) -> R ! e) -> R ! {Worker | e}`; `send(w, m)
  -> Result<Unit, SendError>`; `recv(w) -> Received<O>` where `Received<O> = Message(O) |
  Finished | Failed(WorkerError)`. `Grants` is a subset of the parent's `[permissions]`
  checked at spawn (`Err(WorkerError(GrantExceeded))` otherwise); `Grants.none()` is the
  isolation primitive that replaces `ShadowRealm`. A worker ends when its entry returns,
  when its handle's callback returns (cancelled and joined, like a fiber), or on its
  defect, which `recv` reports as `Failed`. Limits: at most 16 live workers per instance
  (`Err(WorkerError(Limit))`), channel capacity per spawn, the worker's heap bound by the
  host's `SAKURA_MAX_HEAP`. No value, RC pointer or handler crosses the boundary (§17.171,
  §16 item 22 stay true).
- `Worker.topic<T: Schema>(name: String) -> Topic<T>` with `publish`/`subscribe` is
  `BroadcastChannel` within one process; delivery is per-subscriber bounded queues with
  the same `SendError` on overflow. No cross-process delivery is claimed.
- `Worker.inline()` is the deterministic test handler: the entry runs as a fiber in the
  same instance through the same codec, so a worker test needs no thread.
- Dispatch of a `@worker` entry needs compiler support (a named secondary entry point in
  the emitted module); that is the one language-level change in this RFC and it stays
  experimental until the owner stabilizes `std/worker`.

### 3. Effects and permissions

- New effects: `WebSocket` (`std/websocket`) and `Worker` (`std/worker`). Everything else
  is pure or rides on `Http`, `Fs`, `Task`, `Clock`, `Random`, `Console`, `Log`.
- Manifest keys: `net` governs `Http`, `Serve` and now `WebSocket` (`ws` defaults to port
  80, `wss` to 443; the check is the same `host:port` string). No new key is added.
  `process` stays reserved (§17.253 (3)). A worker's `Grants` are a subset of the parent's
  table and never widen it; a worker's `main`-style root handlers are built from that
  subset exactly as `sakura run` builds the parent's (§6.2).
- A `main` that declares `WebSocket` with an empty `net` list fails at startup with exit 75,
  as `Http` does today.

### 4. Errors

Every expected failure is a nominal `error` tag in its module: `Base64Error`,
`EncodingError`, `UrlError`, `PatternError`, `HeaderError`, `FormError`,
`CompressionError`, `CryptoError`, `KeyError`, `SocketError`, `SendError`, `WorkerError`.
`HttpError` and `Utf8Error` are reused. Each carries a typed reason variant where the web
standard distinguishes cases (`Unsupported(name)`, `OutputLimit`, `NotExtractable`,
`GrantExceeded`, …). `DOMException` names (`AbortError`, `DataError`, `OperationError`)
appear only as documentation cross-references. Defects stay defects.

### 5. Determinism

Every pure function returns the same bytes on every backend for the same input, with two
declared exceptions: compressed output (§2.6) and randomized signatures (§2.7). Every
effect has a scripted or captured handler: `WebSocket.scripted`, `Worker.inline`,
`Console.captured`, `Clock.virtual`, `Random.seeded`, `Http.scripted`.

### 6. Resource limits

| Surface | Limit | Failure |
|---|---|---|
| Base64, encoding, URL, headers, form parsing | input is an in-memory `Bytes`/`String`; heap-bound | `D096` as today |
| `Form.parse_multipart` | `FormLimits` required | `FormError(Limit)` |
| `Compress.decompress*` | `max_out` required | `CompressionError(OutputLimit)` |
| `Stream.buffer`, `tee` | `Limit`/`max_lag` required | producer waits (backpressure) |
| `Http` bodies | unchanged: `SAKURA_MAX_COPY_BYTES`, 16 MiB cassettes | unchanged |
| `WebSocket` | `max_message` (default 16 MiB), `max_frame`, receive queue 64 messages then TCP backpressure, close handshake timeout 5 s | `SocketError(MessageTooLarge)`, `Closed(1009)` |
| Redirects | 20 hops | `HttpError("too many redirects")` |
| `Channel`, `Worker` queues | `capacity` per channel | `SendError(Full)` only on `try_send`; `send` waits |
| Workers | 16 live per instance; heap per worker `SAKURA_MAX_HEAP` | `WorkerError(Limit)`, worker `D096` reported as `Failed` |
| `UrlPattern.compile` | pattern length 4 KiB, 64 groups | `PatternError(Limit)` |

### 7. Backend coverage rule

A backend that lacks the host service of a std effect `main` declares refuses the program at
startup, exit 75, with a message naming the effect and the target ("`WebSocket` is not
available on target `native`"). This replaces the current native behaviour of answering
`D091` at first use for `Http`, `Serve` and `Llm`, and applies to them too. Pure modules
run everywhere by construction: std is Sakura source, and their externals live in the JS
runtime and in `runtime/rt`, which WASM and native share. `hosted` WASM imports (crypto
in 0.2.0) are a stated interim: a non-JS WASM embedder gets a link-time `V8003`-class
refusal, never a silent fallback.

### 8. Sections of the design that change

§6.2 (`net` covers `WebSocket`; worker grants), §8.4 (closure records as stream
cursors), §8.6 (bounded channels and workers lift the "no channel" and "no detached work"
lines for the experimental module only), §10.2–10.4 (new modules), §16 items 13 and 22
(answered for the experimental module), §17.206 (4) (`Bytes.concat` lands). The ABI gains a
MINOR revision per phase that adds imports.

## Alternatives and prior art

- **Do nothing / user code.** Agents re-implement URL parsing, Base64 and multipart in every
  program, wrongly. Rejected.
- **A JS interop layer exposing host objects.** Fast on JS, impossible on WASM-without-JS
  and native, and every web object is mutable and aliasable. Rejected (research/15
  recommendation).
- **A single `Web` effect.** Hides which authority is used; `net` could not be checked per
  operation. Rejected: effects name operations, handlers hold authority (§6.1).
- **Web Streams with locks and controllers.** Needed in JS because consumers alias one
  stream; not needed with value semantics and one thread. Collapsed into `Source`/`Sink`.
- **Threads with shared memory for workers.** Breaks §8.1 and the RC runtime. Workers are
  separate instances with a `Schema` wire, like Deno/Node workers with `postMessage`.
- **Prior art.** Deno (permissions as a runtime boundary; workers inherit or restrict
  grants), Node (`node:stream/web`, `worker_threads`, `MessagePort`), Rust (`url`,
  `encoding_rs`, RustCrypto, `miniz_oxide`), Gleam/OCaml (records of closures as stream
  cursors), Koka (effects as rows). Deno's model transfers best: permissions are data, a
  worker's grant is a subset.

## Drawbacks and costs

- About twelve new std modules and roughly 180 public signatures; the std index, the skill
  and `llms.txt` grow, and `sakura doc` becomes the way to find things rather than reading
  the index. The prompt budget (`sakura prompt --budget`) must be re-measured after each
  phase.
- Two Rust codec implementations (JS host, `runtime/rt`) must agree; the shared fixture
  vectors are the only guard.
- `Request`/`Response` body shape changes once, at 0.2.0, and every program constructing a
  record literal with `body: "…"` must change to `body: Text("…")` (helpers keep working).
- Workers add a second entry point to the emitted module and a thread to the native host:
  the largest runtime change since M4, which is why they are experimental.
- Evidence limits: no Web Platform Tests campaign is run; conformance is by the standards'
  published vectors and the fixtures named in the plan.

## Compatibility and migration

- **Before the `v0.1.0` tag** (design §17.263): additive phases land on `main` as their
  gates go green and ship in whatever release is next (phase 0 `7dd9dd47` and phase 1
  `d0222605` are in `v0.1.0`); 0.1.0 precedes RFC 0001's compatibility start, so an added
  module breaks no promise. Phases 4a and 4b and any ABI version bump are held as PR
  branches, rebased daily, and merged right after the tag.
- **`v0.2.0`** is the one breaking window before 1.0 (RFC 0001 starts within-minor
  compatibility at 0.2.0, and the roadmap goes from 0.2.0 to `1.0.0-rc.1`). Everything that
  changes an existing record, variant or effect lands there: `Payload` bodies, `Method`
  variants, `Request.redirect`, `Clock.monotonic_micros`, `Task.cancel`/`cancelled`.
  Migration: `body: "s"` becomes `body: Text("s")` and `r.body` reads become a `match` or
  `Response.text`; a user `match` on `Method` gains two arms; a user `handler Clock`/`Task`
  gains the new operations. Astra's migration list for the dogfood corpus (§17.255 (3))
  carries these; Opus applies them.
- **After `v0.2.0`, before the rc:** only additive modules (`std/stream`, `std/compress`,
  `std/crypto`, `std/websocket`, `std/urlpattern`, `std/channel`) and additive functions.
  Each enters the STABILITY inventory as `stable`, first honouring `1.0.0`, when it lands
  with its fixtures; the rc's "no contract change" rule then applies.
- **Experimental:** `std/worker` (and `@worker`) is labelled experimental in the inventory
  and the stdlib page, outside the 1.0 contract, stabilized by a separate owner decision
  (§Unresolved 1).
- CLI, `sakura.toml` and `--json` schemas: unchanged. `net` semantics extend to `ws`/`wss`
  without a key change. ABI: each phase that adds host imports bumps the MINOR revision;
  the optional-export rule (§17.222) covers older hosts. MSRV: unchanged unless a codec
  crate needs it; any bump is announced in notes. Native remains experimental and off the
  gates (§17.245 (2)); its refusal message is the only native behaviour this RFC requires.
- Release notes list each phase under `[Unreleased]` → the version it ships in.

## Agent and tooling impact

- Every signature is discoverable through `sakura doc` and the generated std index; each
  gap row of §1 has a stdlib-page sentence naming the Sakura form, so an agent searching
  for `TextDecoder` or `EventTarget` finds the answer rather than nothing.
- Typed holes and `V3001` fixes work unchanged: new effects are ordinary declarations, and
  the fix "add `WebSocket` to the signature" is `suggested` + `interface_change` like today.
- No diagnostic schema changes. No automatic repair grants authority: a missing `net`
  entry stays a startup refusal, never a suggested manifest edit.
- Context cost: the skill's `references/stdlib.md` grows; phase 12 re-measures
  `sakura prompt --budget` and trims the authored notes if needed.

## Security and resource limits

- Trust boundaries: `net` for `Http`/`WebSocket` (every redirect hop, every reconnect);
  worker grants ≤ parent; `Secret` for key material and URL passwords; `Untrusted` for
  decoded external text (`Response.text`, `Blob.text` of received data) is unchanged.
- The redirect gap (§2.4) is fixed first.
- Denial of service: every decoder with an expansion factor takes a required bound;
  every queue has a capacity; workers are capped; WebSocket messages and frames are capped;
  multipart parts are capped. Heap exhaustion stays `D096`.
- Unsafe code: none new in `sakura_rt` is expected; codec crates are pure Rust
  (`miniz_oxide`, RustCrypto) and `no_std`-capable. Any `unsafe` added must be listed in
  the phase's PR.
- Cryptography: tier 1 algorithms only from audited pure-Rust crates; no custom primitives;
  AES-GCM nonce is caller-supplied so misuse is visible in code review; keys never log,
  show, serialize or record.

## Tests and validation

- Positive and negative fixtures per module in `compiler/tests/semantics` and
  `compiler/tests/ui`, each with a `// Spec:` comment citing the standard section, plus
  `test` blocks in the std module (`sakura test --std`). Parity (`parity.rs`) runs every
  semantics fixture on JS, WASM and native, so each fixture is a parity case for free.
- Standard vectors: WHATWG URL `urltestdata.json` (a curated subset checked in as a fixture
  table), Encoding Standard samples, RFC 4648 Base64, RFC 4231 HMAC, NIST CAVP SHA-2 and
  GCM, RFC 5869 HKDF, RFC 6070 PBKDF2, RFC 8032 Ed25519, RFC 7748 X25519, RFC 6979 ECDSA,
  RFC 7578 multipart, RFC 6455 close codes.
- Permission tests: a refused redirect hop; a `ws://` host outside `net`; a worker grant
  wider than the parent's.
- Limits: every bound in §6 has one fixture that trips it.
- Determinism: runtime `.test.js` and `runtime/rt` unit tests for each codec; snapshots
  only for diagnostics, never for compressed bytes or signatures.
- Not run yet: everything above; no conformance claim exists until the phase lands and CI
  is green on it.

## Implementation and rollout

The order, files, owners and acceptance criteria are in
[docs/plans/web-apis.md](../plans/web-apis.md): phase 0 the redirect fix; phases 1–4 the
0.2.0 set (bytes/base64/encoding, URL/search params, headers/blob/form, the HTTP record
changes and effect additions); phases 5–10 additive modules before the rc (streams,
compression, crypto, WebSocket, URL patterns, channels); phase 11 workers (experimental);
phase 12 docs and the stability inventory. Opus writes the code; Sol does the mechanical
subtasks the plan names; the owner accepts the RFC and stabilizes.

## Unresolved questions

1. **Does 1.0 wait for `std/worker`?** Recommendation: no. Workers ship experimental in
   the first release whose matrix is green and stabilize in 1.x; everything else in this
   RFC is in the 1.0 contract. Owner accepted the recommendation on 2026-10-20.
2. **Tier 2 crypto on native (RSA, P-384/P-521, AES-CBC/CTR/KW).** Recommendation: not
   before 1.0; typed `Unsupported` is honest. Owner accepted the recommendation on
   2026-10-20; revisited at the rc only if asked.

Everything else is decided above; the plan carries the implementer-level choices.

## Decision record

Not decided. The maintainer records FCP dates, disposition, objections and follow-up links
here.
