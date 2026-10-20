# Changelog

All notable changes to Sakura Lang are recorded here, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Releases follow the [stability policy](STABILITY.md). The `0.1.0-alpha.1` section also
retains earlier implementation history.

## [Unreleased]

### Changed

- WASM backend performance (design §17.285 (c), WS-W6; the native backend shares the runtime,
  unmeasured): building a string by
  repeated `+` or interpolation (`s = "{s}{w}"`) appends to the left string in place when
  nothing else holds it, growing its block geometrically, so the loop is linear instead of
  quadratic: `tokenizer` at n = 100,000 runs 90.6% fewer instructions on WASM-on-Node and
  `json_validate` at n = 30,000 75% fewer. No semantic change; the LIR's `concat` now consumes
  its left operand (an internal runtime convention, not an ABI change).
- Map performance (design §17.275): on the JS backend a `Map<Int, _>` or `Set<Int>` indexes
  its keys by their own bits instead of a hash, and on every backend `Map.get` consumed at once
  by `unwrap_or`, `is_some` or `expect` (and `Map.contains`) builds no `Option`. By
  instructions retired per call on the reference Mac (Node, n = 100,000), `map_set`'s `Int`
  case drops 38% (6.21× → 3.84× hand-written JavaScript); insertion order, `Show` and JSON are
  unchanged.
- Benchmark harness (design §17.282): every Node process it measures runs under
  `--predictable --min-semi-space-size=16 --max-semi-space-size=16`, a fixed young
  generation that removes a GC-phase alias which moved a counted cell with its artefact's
  directory (`shapes` n = 50,000: 8–19%; under the flags, 0.21%). Runs record the list as
  `meta.flags.node`; `compare` (verdict `flags`), `envelope` and `bless` refuse two files
  under different lists. Every Node count changes once, at the owner's re-bless.
- Benchmark harness (design §17.283): a counted `compare` gates peak RSS on the one-call
  processes' maximum, not the many-call processes', whose call count the file's own speed
  sets; the many-call maxima and `calls` are printed beside it as a diagnostic.
- JS backend performance (design §17.265 (6)): a function that returns a fresh list may now
  update the list it is given in place when the caller is done with it, and a `List<Bool>`
  built by an owned push loop is one array until a persistent update rebuilds it. By
  instructions retired per call on the reference Mac (Node, largest size), `nbody` drops 40%
  (11.90× → 7.23× hand-written JavaScript) and `sieve` 33% (4.62× → 3.08×); no program's
  output changes.
- JS backend performance (design §17.274): a defect on a path that is never taken (an `Int`
  overflow, an `expect` on a missing element, a division by zero) is thrown where it is
  detected rather than by calling a function that throws, and a contract predicate that can defect is a
  `try` whose catch reports `D011`. `spectral_norm` drops 26% by instructions retired per
  call on the reference Mac (Node, n = 400; 4.49× → 3.31× hand-written JavaScript). Every
  defect's code, message, spans, `--json` record, stderr line and exit status are unchanged.
- **Breaking for user handlers (0.2.0, RFC 0002 §2.8, design §17.262 (3)).** The `Clock`
  effect gains `monotonic_micros() -> Int` and the `Task` effect gains `cancel<T>(f: Fiber<T>)
  -> Unit` and `cancelled(s: Scope) -> Bool`, so a user `handler Clock` or `handler Task` must
  add them; `Clock.host`, `Clock.fixed` (`0`), `Clock.virtual` (its milliseconds × 1000),
  `Task.host` and `Task.deterministic` implement them. `Task.cancel` cancels a fiber, waits
  until it has unwound through its finalizers and uses it up: a later `Task.join` of it is
  the defect `D095` (`join on a cancelled fiber`) on every backend, where `D095` was
  unreachable before. `Task.cancelled(s)` is whether a child of `s` failed, so the scope is
  cancelling the others.
- The WASM runtime ABI is 1.5.0 (`sakura_abi_version` `0x010500`): the `sync` import
  `clock_monotonic_us() -> i64` backs `Clock.monotonic_micros` on `Clock.host`. A 1.4 host
  refuses a 1.5 module (`V8003`); the Node host and the native host implement the import.
- The WASM runtime ABI is 1.6.0 (`sakura_abi_version` `0x010600`; design §17.266 (1)):
  `std/crypto` runs on `--target wasm`, where its primitives were the defect `D090`. Its five
  primitives are `hosted` imports the Node host serves with `node:crypto`, as the JS backend
  does, so the NIST CAVP and RFC vectors agree on JS, WASM and native. Key material crosses
  as `Bytes`; the guest zeroes every copy of the argument tuple and the answer before freeing
  them, and the host zeroes its copies. A 1.5 host refuses a 1.6 module (`V8003`); a WASI command does not have the imports.

- **`--json` closing records have one `kind` per shape** (design §17.242). `check`, `fix`
  and `build` still end with `summary` 1.0, now with all eight fields always present
  (`snapshot`, `errors`, `warnings`, `notes`, `total`, `emitted`, `truncated`, `holes`).
  Three kinds are renamed: `sakura test --json` ends with `test_summary` 1.0 (`passed`,
  `failed`, `skipped`, `total`, `seed`); `sakura audit --json` with `audit_summary` 1.0
  (`snapshot`, `sites`, `by_site`); `sakura verify --json` with `verify_summary` 1.0 (the
  same fields as the old `summary` 1.1, but `schema_version` is `"1.0"`; verify's
  `obligation` and `diagnostic` records stay 1.1). A `sakura test --json` run that does not
  compile still ends with the build's `summary`. **To migrate**, a consumer that finds the
  end of a run by `kind == "summary"` matches `test_summary`, `audit_summary` or
  `verify_summary` for those three commands, and one that checks verify's
  `schema_version == "1.1"` reads `"1.0"` on `verify_summary`. Field names and meanings
  are unchanged.
- **The `defect` record and the regression file join the `--json` contract.** `defect` 1.0
  on stderr at exit 70 (`schema_version`, `kind`, `code`, `message`, optional `spans`) is
  unchanged on the wire. A file `sakura test --contracts` writes to
  `tests/regressions/<fn>.json` is now a `regression` 1.0 record: it gains
  `"kind": "regression"` beside `schema_version`, `function`, `seed`, `inputs` and
  `failure`. Files written by earlier versions, without `kind`, still replay.
- **MCP results are the CLI's records.** `symbol_query` returns the `symbol_query` record
  `sakura doc --json` prints, gaining `schema_version` and `kind`; `test`'s `summary` field
  holds the `test_summary` record; both are built by the same code as the CLI's.
- **Breaking (0.2.0, RFC 0002 §2.4, design §17.262 (3), §17.263 (4)).** `Request.body` and
  `Response.body` are a `Payload` (`Empty | Text(String) | Bytes(Bytes)`, in `std/request`),
  so a body may be binary on JS and WASM. Migration: `body: "s"` in a record becomes
  `body: Text("s")` (or `.with_payload(Text("s"))`), and a read of `r.body` becomes a `match`,
  or `Response.text`, `Response.json` or the new `Response.bytes(r, limit)` for a response.
  `Request.of`, `post`, `post_json`, `Response.status`, `ok_json` and `html` keep their
  signatures and build `Text` or `Empty`. `Http.host()`, `Serve.host()` and a replayed
  cassette deliver a body as `Bytes` (`Empty` with no bytes), so a test that compared a
  replayed `response.body` with a string compares `Response.bytes` or `Response.text`.
  `Method` gains `Options` and `Trace` (a user `match` on `Method` gains two arms); `Request`
  gains `redirect: Redirect` (`Follow`, `Manual`, `Error`; `Follow` from every builder) and
  the builders `with_payload`, `with_header` and `with_headers(h: Headers)`.
- **Redirects are followed in `std/http`, not in the host** (design §17.263 (4)). Every
  `Http.host()` send reaches `fetch` with `redirect: "manual"`, and one Sakura function,
  shared by `host()`, `scoped()`, `scripted()` and `recording()`, follows at most 20 hops,
  each its own send: `Location` is resolved with `Url.resolve`, `net` is checked by the host
  and `Http.scoped`'s list by `scoped` on every hop (before, a hop was checked against `net`
  only), `authorization` and every `secret_headers` entry are dropped when the scheme, host
  or port changes, and a 303 (or a 301/302 after a `POST`) becomes a body-less `GET`, as
  phase 0 did in the host. `Manual` returns the 3xx response and `Error` fails `HttpError`
  with its status. The messages are phase 0's (`too many redirects`, `redirect to <scheme>:
  refused`, `invalid redirect location <location>`). `Http.scripted` can now answer a 3xx,
  which is followed like a host's; `Http.recording` records each hop as an exchange, and a
  cassette without 3xx exchanges replays as before. `Http.stream` closes a 3xx body before
  it opens the next hop. The host's own hop loop is gone.
- Cassettes store a request or response body that is not UTF-8 as `body_base64` or
  `response_body_base64` (the field `response_body_base64` already existed for streamed
  bodies); a `Text` and a `Bytes` request body with the same bytes match the same exchange.
  An exchange with a `*_base64` field that is not Base64 now fails `open` naming the line.
- The WASM runtime ABI is 1.7.0 (`sakura_abi_version` `0x010700`; design §17.270 (3)):
  `http_send` and `http_stream_open` carry `Payload` bodies and the `redirect` field, send
  with `redirect: "manual"` and answer a 3xx as it is; `serve_listen` carries `Payload`
  bodies. The import names are unchanged; a 1.6 host refuses a 1.7 module (`V8003`).
- Experimental `--target native` (RFC 0002 §7, design §17.271 (9)): a program whose `main`
  declares `Http`, `Serve` or `Llm`, which the native host does not implement, exits 75 at
  startup with one stderr line, "`Http` is not available on target `native`", before any of
  its code runs, instead of the defect `D091` at first use. A `main` that handles the effect
  itself (`Http.scripted`) still runs. The `secrets` example is refused on native this way.

### Added

- `std/websocket` (RFC 0002 §1, §3, §6, §7; plan `web-apis.md` phase 8): a WebSocket client as
  the effect `WebSocket`. `WebSocket.connect(url, options, body)` opens a `ws` or `wss` URL and
  runs `body` with a `Socket` that cannot leave it (`V6003`), closing it with 1001 on every exit;
  `send(sock, Text(s) | Binary(b))`, `receive(sock)` (`Data(message)` or
  `Closed(CloseInfo { code, reason, clean })`, the close returned again by later receives),
  `close(sock, code, reason)` (1000 or 3000–4999, a reason of at most 123 bytes; anything else
  is `D020`) and `messages(sock)`, the messages as a `std/stream` source. `Options` carries
  subprotocols, extra headers and `max_message` (16 MiB by default), which bounds both
  directions: a larger send fails `SocketError(MessageTooLarge)`, a larger received message
  closes the connection with 1009. `WebSocket.host()` speaks RFC 6455 itself on JS and through
  the WASM host: `net` is checked before connecting (`ws` port 80, `wss` 443), the handshake
  follows no redirect, at most 64 received messages, and at most `max_message` bytes of them,
  wait before it stops reading the connection, a ping is answered only while the socket's write
  buffer needs no drain (so a ping flood cannot grow it; design §17.286 (4)), and a close the peer does not answer ends after 5 seconds as 1006.
  `WebSocket.scripted([Reply(…), Push(…), Close(…)])` is the deterministic peer for tests. A
  `main` declaring `WebSocket` exits 75 at startup with an empty `net` list, and on
  `--target native` with "`WebSocket` is not available on target `native`". WASM runtime ABI
  1.8.0 adds the `ws_*` imports, so a host built for ABI 1.7 refuses a module from this
  version (`V8003`). A cookbook recipe, `websocket_client`.
- Web timer and dialog idioms (RFC 0002 §1, §2.8; plan `web-apis.md` phase 4b):
  `std/task.every(period, body)` (`setInterval`: serialized ticks, a tick due while `body` runs
  is skipped and the next call receives the count, the loop ends when `body` returns `false`
  or its task is cancelled), `Console.alert`, `Console.confirm` (`true` only for `y` or `yes`
  in any case; end of input is `false`) and `Console.prompt` (the line, the default for an
  empty line, `None` at end of input), written in std over `print` and `read_line` so every
  `Console` handler serves them, and `Json.render(j, indent)` (`JSON.stringify` with an
  indent of at most 10; compact for `None`). `Console.scripted(lines)` is a test handler whose
  `read_line` answers `lines` in order and then end of input (the dialogs need scripted
  input, which `Console.captured` does not take). The concurrency tour shows the
  `setTimeout`, `setInterval` and `clearTimeout` idioms.
- A cookbook recipe, `sign_webhook`: a router endpoint that checks a delivery's Base64
  HMAC-SHA-256 `X-Signature` with a verify-only `CryptoKey` (`key.verify`, constant time),
  tested with a changed body, another secret and a missing or malformed header.
- `std/urlpattern` (RFC 0002 §2.2; plan `web-apis.md` phase 9): `UrlPattern.compile(pattern)`
  and `UrlPattern.exec(p, url) -> Option<UrlMatch>`, the web's `URLPattern` without regular
  expressions, so a pattern matches alike on JS, WASM and native. A pattern is a path or an
  absolute URL with a literal scheme, host and port; each segment is literal, a `:name`
  group, `*` (one segment) or `**` (any number, greedy). `UrlMatch.groups` lists the groups
  in pattern order, unnamed ones as `"0"`, `"1"`, …. Regexp groups, `{…}` groups, modifiers,
  escapes and a group inside a segment are `PatternError(Unsupported(text))`, a repeated name
  `Duplicate(name)`, and a pattern over 4,096 bytes or 64 groups `Limit`.
- `Router` routes are `UrlPattern` patterns, matched through `UrlPattern.exec`, so `*` and
  `**` routes work, their segments in `Request.param(req, "0")`, `"1"`, …. A `:name` route
  matches as before. A route pattern `UrlPattern.compile` rejects (a `(…)`, `{…}`, `?`, `+`
  or `\` in a segment, a `:` or `*` inside one, a name that is not an identifier) is now the
  caller defect `D020` at `Router.route`/`get`/`post`/`put`/`delete`, where it was a literal
  segment before.
- `std/stream` (RFC 0002 §2.5; design §17.280; plan `web-apis.md` phase 5): `ReadableStream`
  and `WritableStream` as values. A `Source<T, e>` is `{ pull }`, and pulling returns `Done`
  or `Next(item, rest)`; a `Sink<T, e>` is `{ write, close }`, `write` returning the next
  sink. A pure source reads the same items every time it is pulled. Combinators `empty`,
  `from_list`, `map`, `filter`, `take`, `fold`, `each`, `pipe`, `chunks(src, size)` (bytes
  re-cut to exactly `size`), `lines(src, max_line)` (LF, a CR before it dropped) and
  `to_list(src, max_items)`; past a bound, `StreamError(MaxItems | MaxLine)`. Adapters:
  `Encoding.decode_source`/`encode_source` (`TextDecoderStream`/`TextEncoderStream`),
  `Blob.source(b, chunk)` (`blob.stream()`), and the views `Http.body_source(body, chunk)`,
  `Fs.file_source(file, chunk)` and `Fs.file_sink(file)`, which advance their body or file
  and, like it, cannot leave the callback that received it (`V6003`). `buffer` and `tee`
  arrive with `std/channel`. A cookbook recipe, `stream_lines`, counts a file's lines in
  bounded reads.
- `std/crypto` tier 1b (RFC 0002 §2.7; plan `web-apis.md` phase 7b; design §17.285 (2d)):
  AES-GCM (`Algorithm.AesGcm(Aes128 | Aes256)`, `encrypt`/`decrypt` with a caller's 12-byte
  nonce and additional data, `CryptoError` `Nonce(n)` or `Tag`, no plaintext from a ciphertext
  that does not authenticate), ECDSA P-256 with SHA-256 (`sign`, which performs `Random`, and
  `verify`; signatures are `r || s`, deterministic RFC 6979 on native and randomized on JS, so
  they are compared with `verify`, never by value), ECDH P-256 and X25519 (`derive_bits` with
  `EcdhWith(peer)`; an X25519 peer of small order is `ZeroSecret`), Ed25519, and
  `generate_key_pair` (`KeyPair`, Web Crypto's `CryptoKeyPair`). `CryptoKey` gains `kind`
  (`SecretKey | PublicKey | PrivateKey`); `KeyFormat` gains `Pkcs8`, `Spki` and `Jwk`, written
  and read in std, so each is the same bytes on every backend; `KeyReason` gains `Format(f)`
  and `Malformed`. The primitives are `node:crypto` on JS and, on native, `aes-gcm` 0.11.1,
  `p256` 0.14.0, `ed25519-dalek` 3.0.0 and `x25519-dalek` 3.0.0 at exact pins. NIST CAVP GCM,
  RFC 8032, RFC 7748, RFC 5903 and RFC 6979 vectors in `compiler/tests/vectors` hold the two
  equal. On `--target wasm` a tier 1b primitive is the defect `D090` until its host imports
  (ABI 1.9.0) follow phase 8. No ABI change. The new variants of `Algorithm`, `Usage`,
  `KeyFormat`, `KeyReason` and `CryptoReason`, which RFC 0002 §2.7 reserved for this tier,
  make an exhaustive `match` over one of them in a program written against `v0.1.0`
  non-exhaustive; std signatures are stable from 0.2.0 (STABILITY.md).
- `Request.bytes(req, limit)` and `Request.text(req, limit)` (design §17.271): a server's
  readers of a request body, `BadRequest` past `limit`; `text` is strict UTF-8
  (`BadRequest("body is not UTF-8")`) and `Request.json` decodes `text`.

### Fixed

- An effect operation's `requires` is now checked at every perform site, on every backend
  (design §7.2, §17.286 (8)): `Cli.exit(256)`, `Random.int(3, 2)`, `Random.bytes(-1)`,
  `Fs.read_chunk(f, 0)`, `Http.read_chunk(b, 0)`, `WebSocket.connect` with `max_message` 0
  and `WebSocket.close` with a code outside `closable` are `D020` blaming the perform, before
  any handler runs, where they previously reached the handler unchecked (or, for
  `WebSocket`, defected inside `std`). An operation's `ensures` is checked when the perform
  returns normally (`D021`). A handler never re-checks; a handler arm that performs the
  operation again is checked again.
- `std/stream` (design §17.284): `to_list` requires `max_items >= 0` (a negative bound is
  `D020`), and `lines` fails `MaxLine` on a final line without an LF longer than `max_line`
  (its CR counted), so no yielded line exceeds the bound.
- Native target (experimental): `Http.stream` under `Http.scripted`, and `Http.body_source`
  over its `Body`, run natively as on JS instead of stopping with the defect `D091`; the
  native host keeps the scripted body's bytes and the runtime reads and closes it.
- `sakura fmt` no longer refuses (`V9001`) a line comment that ends in a bare `\r` followed by
  spaces or tabs, such as `//\r `: the trailing `\r` is trimmed with the spaces, where it
  would otherwise end the line before the comment's text and change it.

### Security

- `Http.scoped(hosts)` checks every redirect hop against its list, not only the first URL
  (design §17.263 (4)): the hops are now followed by `std/http` (see Changed), so a listed
  host that redirects to an unlisted one fails `HttpError` (`<host:port> is outside
  Http.scoped`) and the unlisted host is never sent anything. Security-sensitive: the owner
  reviews the relocated redirect loop.

## [0.1.0] - 2026-10-20

The first non-alpha preview: the WebAssembly backend is complete. Sakura Lang is still pre-1.0: read [STABILITY.md](STABILITY.md) before
depending on anything below. The Rust MSRV is unchanged (1.99).

**What is stable and what is not**

- **No stability promise yet.** There is no stable language, std, CLI, `--json` or ABI
  contract in `0.1.0`. [RFC 0001](docs/rfcs/0001-stability-guarantees.md) (Draft) proposes
  the guarantees, and the draft inventory in STABILITY.md names what each covers; if the owner
  accepts it, its promises begin at `v0.2.0`. Every `--json` record is versioned (`kind`,
  `schema_version`), and `compiler/contracts/json.txt` lists its fields.
- **WASM backend: done.** `--target wasm` builds, runs, tests (with `--contracts`) and
  verifies programs as `--target js` does; CI's parity suite compares the two backends'
  output, traces and defect records on Linux x64 and ARM64, macOS ARM64 and Intel, and
  Windows x64. `sakura test --record` still needs `--target js`. Modules carry runtime ABI
  1.4.0.
- **`--target native`: experimental.** It exists only in a compiler built from source with
  the Cargo `native` feature; the release binaries and packages do not include it. It is
  outside every compatibility promise and stays experimental at `v0.2.0`. Natively, `Http`,
  `serve` and `Llm` still stop with a `D090` or `D091` defect.
- **Packages: not supported.** `[dependencies]` in `sakura.toml` is reserved and refused with
  `V8005 dependencies_not_supported`; nothing is fetched. Packages are planned
  for a 1.x minor release.
- **Platforms.** A packaging target is called supported for this release only after its
  published-install smoke passes on every applicable channel; the
  [install page](website/content/docs/install.mdx) states each platform's level.
- **Performance.** These notes quote no benchmark ratios. The owner's blessed
  instruction-count baseline (`bench/baselines/`) is the reference later releases are
  compared with; the [benchmarks page](https://www.sakuralang.com/docs/benchmarks) shows it.

**Semantics changes for owner review** (rows 8–10 of
[the release checklist](docs/plans/m4-exit-checklist.md)):

- `Serve.host()` on `--target wasm` answers a request the heap cannot take `503` with a
  `D096` record, a request still waiting when the program ends `500`, and a trap `500` before
  the server stops and the program exits 70; on both backends a request arriving after the
  server stops is `503` (Fixed below; design §17.223).
- A response body cut short in `Http.host()`'s `send` is `Err(HttpError)` on both backends
  instead of a defect or a crash (Changed below, **Breaking**; design §17.209 (2)).
- Symbolic links in an `fs.write` grant: a write through a dangling link is refused, and the
  JS and WASM hosts open the resolved path with `O_NOFOLLOW`, so a link planted during the
  call is refused within the limits ABI §8.3 states (Security below; design §17.233). The dangling-link hole shipped in
  `v0.1.0-alpha.1`.

Entries marked **Breaking** can change what a program that worked on `v0.1.0-alpha.1` does.

### Added

- `std/bytes` and the new `std/encoding` (RFC 0002 §2.1, web-APIs plan phase 1): the web's
  `Uint8Array.toBase64`/`fromBase64`, `atob`/`btoa`, `TextDecoder` (streaming too) and
  `TextEncoder` for UTF-16, as values with typed errors. `Bytes.slice` (clamped like
  `List.slice`) and `Bytes.concat` (one copy, linear); `Bytes.to_base64(b, alphabet, padded)`,
  strict `Bytes.from_base64(text, alphabet)` and `Bytes.from_base64_lenient(text)` (HTML's
  forgiving Base64), each failure `Base64Error { offset, reason }`; `Encoding` (`Utf8`,
  `Utf16Le`, `Utf16Be`, `Windows1252`) with `Encoding.parse(label)` over the WHATWG labels
  (anything else is `Unsupported(label)`), `decode`/`decode_with` with `Errors.Fatal`
  (`Malformed(offset)`) or `Errors.Replace` (U+FFFD) and the byte order mark stripped or kept,
  an immutable `Decoder` (`Encoding.new`, `push`, `finish`) that joins a character split
  across chunks, and `Encoding.encode` for UTF-16. The codecs are runtime externals in the JS
  runtime and in `runtime/rt` (WASM and native); RFC 4648 and Encoding Standard vectors in
  `compiler/tests/vectors` hold the two equal. Additive; no ABI change.
- `std/url` gains the WHATWG URL parser and the new `std/searchparams` (RFC 0002 §2.2,
  web-APIs plan phase 2): the web's `URL` and `URLSearchParams` as values. `Url.parse(s)` and
  `Url.resolve(base, relative)` (`new URL(relative, base)`) give `UrlParts { scheme, username,
  password, host, port, path, query, fragment }` or `UrlError { reason }` with `Idna`,
  `InvalidScheme`, `InvalidHost`, `InvalidPort`, `Relative` or `OpaquePath`; special schemes
  get the standard's host parsing (IPv4 in every radix, IPv6 compressed), default ports, dot
  segments and percent-encode sets, other schemes opaque hosts and paths. `to_string` is the standard's
  serialization, `to_url` the `Url` for `Request.of`, `origin` the origin; `with_scheme`,
  `with_host`, `with_port`, `with_path`, `with_query`, `with_fragment` and `with_credentials`
  are the setters, returning a new value; the ones that can refuse return `Result`, where the
  standard's setters ignore the input (`with_path` on an opaque-path URL such as `mailto:x` is
  `OpaquePath`, design §17.268 (3)). The password is a `Secret`, which `to_string`,
  `to_url` and `origin` leave out. A host that needs IDNA (non-ASCII, or an `xn--` label) is
  `Idna` until punycode lands. `SearchParams` is an ordered multimap (`parse`, `get`,
  `get_all`, `has`, `append`, `set`, `remove`, `sort` in UTF-16 code unit order, `entries`,
  `to_string`) over `application/x-www-form-urlencoded`; `Url.search` and `Url.with_search`
  read and replace a URL's query. Pure Sakura on every backend: 150 WPT `urltestdata.json`
  cases in `compiler/tests/vectors/url.jsonl` generate a semantics fixture. `Url` and
  `Url.is_valid` are unchanged; a cookbook recipe (`url_build`) shows parse, `with_search`,
  `to_url` and `resolve`. Additive; no ABI change.

- `std/crypto` (RFC 0002 §2.7, web-APIs plan phase 7a): the Web Crypto API's SHA-2 digests,
  HMAC, HKDF and PBKDF2 as pure functions. It has no effect and no manifest key; key generation
  performs `Random`. `crypto.digest(alg, data)` for SHA-256/384/512 (no SHA-1 or MD5); an
  opaque `CryptoKey` whose material is a `Secret<Bytes>`, so a key is never shown, logged or
  serialized (`V2031`); `import_key`/`export_key` in `Raw` format with `KeyError` (`NoUsages`,
  `NotAllowed(usage)`, `Empty`, `NotExtractable`); `generate_key` for HMAC from `Random`;
  `hmac`, and `verify`, which compares in constant time; `derive_bits` for HKDF and PBKDF2,
  with every bound an argument (`bits`, `iterations`) and `CryptoError` (`Length`,
  `Iterations`, `Info`) for an out-of-range one: HKDF `info` over 1,024 bytes, iterations
  outside `1..=2^31 − 1`, HKDF output over 255 hash lengths and PBKDF2 output over 1 MiB, each
  checked before the primitive and so the same error on every backend, where an oversized
  PBKDF2 output used to be the defect `D096` on native and `D090` on JS (design §17.266 (7)).
  A key without the operation's usage is `D020`; an imported HKDF or PBKDF2 key is
  extractable when imported so, where Web Crypto imports those non-extractable.
  The primitives are `node:crypto` on JS and the RustCrypto crates (`sha2` 0.11.1, `hmac`,
  `hkdf` and `pbkdf2` 0.13.0, exact pins) in `runtime/rt` on native. NIST CAVP SHA-2, RFC 4231,
  RFC 5869 and RFC 7914 §11 vectors in `compiler/tests/vectors` hold the two equal. On
  `--target wasm` a primitive is the defect `D090` until its host imports, an ABI MINOR
  revision, follow the `v0.1.0` tag. Additive; no ABI change.

- `std/headers`, `std/blob` and `std/form` (RFC 0002 §2.3, web-APIs plan phase 3): the web's
  `Headers`, `Blob`, `File` and `FormData` as values. `Headers.of(list)` validates names (HTTP
  tokens) and trims values per Fetch, failing with `HeaderError { name, reason }`
  (`InvalidName`, `InvalidValue`; the value is never repeated); `get` joins a name's values with
  `", "` except `set-cookie`, which only `get_set_cookie` returns; `get_all`, `has`, `append`,
  `set`, `remove`, `entries` (Fetch's sort and combine) and `to_list` complete it. It never holds
  a `Secret`: credentials stay in `Request.secret_headers`. `Blob { bytes, media_type }` with
  `of`, `size`, `slice` (File API offsets) and `text`, strict UTF-8 (`Utf8Error`) with a byte
  order mark removed; `FileData { blob, name, last_modified }` names no file on disk. `Form {
  entries }` of `Text` and `File` parts: `encode_urlencoded` and `encode_multipart(f, boundary)`
  follow HTML's form-submission algorithms (CRLF newlines, `%22`/`%0D`/`%0A` in names),
  `boundary()` draws one from `Random`, `is_boundary` is RFC 2046's rule, `parse_urlencoded`
  reads `SearchParams`, and `parse_multipart(body, content_type, limits)` reads RFC 7578 with a
  required `FormLimits { max_parts, max_bytes, max_field_bytes }` and `FormError`
  (`Limit(MaxParts | MaxBytes | MaxFieldBytes)`, `ContentType`, `Malformed(offset)`). Pure
  Sakura on every backend: 56 WPT form-submission cases in `compiler/tests/vectors/form.jsonl`
  generate a semantics fixture; a cookbook recipe (`form_upload`) sends a multipart upload and
  parses it back. `Request` and `Response` are unchanged. Additive; no ABI change.

- `compiler/fuzz`: an `exec` differential target. It mutates `tests/semantics` programs with
  typed edits (literals, operators, match-arm and handler order, `List` and `Map` operations)
  and runs each well-typed result on the reference evaluator, the JS backend, the LIR
  evaluator and the WASM backend on Node. Any disagreement on stdout, the ending, the defect
  record or the effect trace fails the run. The nightly fuzz workflow now carries each
  target's corpus between runs as an artifact, and a manual run takes `max_total_time`, the
  seconds per target.
- `bench/bench run --instructions` and `bench/bench compile --instructions` (macOS): the
  benchmark harness counts instructions retired per call through `/usr/bin/time -l`, the
  primary metric of a baseline from now on (design §17.264, the owner's decision of
  2026-10-19). Each cell takes a call count calibrated on the counter and five pairs of fresh
  processes (one call, and 1 + k calls), and reports the median per call with its spread;
  wall time per call is kept beside it as information. `bless` accepts a counted run without
  a quiet host, and `compare` and `envelope` read instructions when every file is counted.
  The quiet benchmark window no longer gates a release: the gate is a blessed
  instruction-count baseline from the reference Mac (`docs/release.md` §2). The blessed counted
  runs are on the website's [benchmarks page](https://www.sakuralang.com/docs/benchmarks).

- The npm packages carry a README, keywords and an issues link, so the npm page shows how to
  install and start. Each platform package's README points to `sakuralang`.

- Experimental `sakura build --target native` and `sakura run --target native` in a compiler
  built with the `native` feature: Cranelift compiles the program to an
  executable `main` linked with the native runtime, whose defect records carry the same spans
  as JS and WASM. Native stays experimental at `v0.2.0`; release builds do not include it.
  Native executables carry unwind tables and a DWARF line table, so `lldb`, `gdb` and
  profilers show Sakura frames at their `.sak` lines, and a `sakura.native` record that the
  runtime checks before running (`V8003` for another target, layout profile or ABI).
  `sakura build --target native --native-checks` builds a checked executable that stops at
  any generated heap access outside its object.

- `sakura run --target native` (a compiler built with the `native` feature) runs `Json.encode`,
  `Json.decode`, `Json.parse`, `Json.schema`, `Cli.args` and `Cli.parse`. The JSON
  codec is written once in the Rust runtime and matches the JS runtime's text and decode
  messages, `where` clauses included. Programs that use `Http`, `serve` or `Llm` still stop
  with a `D090` or `D091` defect natively.

- `Sql.memory()` and `Sql.sqlite(path)` run natively on bundled SQLite, each database on its
  own worker thread, with rows, decode errors and SQLite's messages matching the JS runtime.
  Cancelling a running statement (a lost `Task.race`, a `timeout`) interrupts it, and a write
  that committed before stays committed.

- The WASM runtime's `stats()` reports four more counters, recorded by `SAKURA_WASM_STATS`,
  `SAKURA_NATIVE_STATS` and `bench/harness` (runtime ABI 1.3.0, design §17.176 (1)): `reuse_attempts` and `reuses` (an owned `List`, `Map` or `Set` update testing whether it
  may write its wrapper and each node on its path in place, and the tests that could),
  `borrow_candidates` (executed reference-count increments before a call that only reads the
  value, which stays live after it) and `borrows` (always 0 until borrowing lands). Benchmark
  records add `reuse_rate` (`null` when nothing was attempted) and the module's `abi_version`.

- `sakura build --target wasm --dump owned` prints the ownership facts as the LIR computes them
  from the committed Core snapshot (design §17.176 (2)); they match the JS backend's dump.

- The WASM module's `stats()` export reports an eleventh counter, `restarts`: the calls of a
  direct function run again through frames after its native twin ran out of stack gauge
  (runtime ABI 1.2.0, design §17.160 (2)). ABI 1.2.0 also lists the host imports and handle
  kinds as built; a 1.1 host refuses a 1.2 module (`V8003`), so the bundled host and the
  module move together.

- A tree-sitter grammar in `editors/tree-sitter-sakura/` (not published) with highlight,
  locals, indent and injection queries and Rust and Node bindings. CI parses every `.sak`
  file in the repository with it and checks that, wherever the compiler's parser reports no
  syntax error, its tokens, items, statements and match arms have the compiler's spans, with
  LF and CRLF line endings. It is for editors and tooling; the compiler's parser stays the parser of record.

- `bench/bench run --suite wasm` times the benchmark suite built with `--target wasm` on Node
  and Wasmtime against hand JS on Node, after the same oracle, recording each row's `backend`
  and the module's `stats()` counters; its numbers are advisory
  (design §17.191 (2)). The nightly benchmark workflow runs it as its own job.

- `V4004 match_too_large` (pattern phase): a `match`, `catch` or `let` pattern whose decision
  tree passes 1,048,576 nodes, counting the rows its or-patterns expand into and the matches in
  its guards, is an error from `sakura build`, `run` and `test` instead of a build that runs away
  in memory and time (design §17.193). The arm count itself is not limited.

- `sakura new` and `sakura init` write `.gitattributes` with `*.sak text eol=lf`, so Windows
  checkouts of sources stay LF, and `*.snap -text`, so git never converts a snapshot's line
  endings and a value with `\r\n` compares byte for byte (design §17.180 (4b), §17.196 (1)).
  A snapshot that differs only in line endings names `*.snap -text` and
  `sakura test --update-snapshots`.

- `sakura verify --json` writes `schema_version: "1.1"` on every record, and its `summary`
  counts `reused`: the obligations whose record was rebuilt from an authenticated cache entry
  instead of being solved; the human summary prints `(n reused)`. Consumers check the major
  version and ignore unknown fields. An `unknown` whose reason is `resource_limit` is now
  cached and reused by `sakura verify` (the `rlimit` is in the key), so a second run does not
  spend the full budget on it again; it never removes a check. Proof cache entries move to
  format 2, so each obligation is solved once more after upgrading (design §17.164 (1), (2)).

- `sakura verify --target wasm` replays a `disproved` obligation on the LIR evaluator, whose
  `Int` is WASM's i64, and marks a counterexample that defects as predicted `confirmed`, as
  the JS profile already did on the Core evaluator (design §17.164 (4)).

- `sakura build --target wasm --manifest` writes `target/sakura/manifest.json` instead of
  refusing. The manifest is schema 1.1 and gains `target` (`js` or `wasm`); a WASM manifest
  lists `sakura_rt.wasm`, `sakura_wasm_host.js` and `sakura_runtime.js` as its runtime, and
  every clause stays `runtime_checked` because WASM builds drop no checks yet
  (design §17.163 (3)).

- Release candidate checks enforce deprecation notice periods, major-only removals
  and unchanged Rust MSRV in patch releases; the deprecation registry starts empty.

- Draft grammar and diagnostic specification in `docs/spec/`, indexed by fixtures,
  with a generated diagnostic identity table and a Python drift check; lexical boundary,
  grammar-alternative and tree-sitter divergence fixtures back its rules, and `V8003` has an
  emission test.

- Draft RFC 0001 proposes stability guarantees and identifies enforcement gaps for
  owner acceptance at `v0.2.0`; current compatibility policy is unchanged.

- Intel macOS archives, npm and Homebrew packaging with public Intel-hosted suites,
  parity and release smoke; both Darwin architectures enter the existing signing job.
  Release-specific support still requires published-channel acceptance.

- Linux ARM64 glibc release archives and the `sakuralang-linux-arm64-gnu` npm package,
  built and smoke-tested on public GitHub-hosted ARM runners. CI runs the Rust suites
  with pinned Z3/cvc5 and both JS/WASM parity lanes on Linux ARM64. The Homebrew
  formula directs Linux ARM64 users to the release installer or npm.

- Windows x64 MSVC ZIP and npm packaging, a checksum-verifying PowerShell installer,
  hosted Windows suites/parity, and native `sakura.exe` selection in VS Code.
  Release-specific supported status still requires the published smoke.

- Published-channel acceptance workflow with exact version/commit checks, a deliberate
  installer target override, and a platform-report issue form for community confirmations.

- Per-OS website install instructions with release-specific platform claims, Node 26
  setup for Raspberry Pi users; Windows x64 and Intel macOS are published for 0.1.0,
  supported once their published-install check is green.

### Changed

- A digit separator in a numeric literal is one `_` between two digits of one digit run
  (design §2.2, §17.243 (2)). `1_`, `1__000`, `0x_ff`, `0xff_`, `1_.5`, `1.5_` and `1.5_e3`,
  which the lexer used to accept, are now `V0006 malformed_number` at the misplaced `_`;
  `1_000`, `0xff_ff` and `1_000.000_5` are unchanged.
- `sakura test` relays its test runner's exit codes `0`, `2`, `3` and `70` unchanged (design
  §17.243 (5)). A module of another ABI under `sakura test --target wasm` now exits `2` after
  the runner's `V8003` line instead of `70` with "the test runner crashed"; only another
  status, a signal or the time limit is reported by the CLI itself.
- The Node WASM host reads a module's ABI version from its `sakura.debug` section before
  compiling or linking it (design §17.243 (4), ABI §0 erratum, no ABI version change), so a
  module of another ABI is `V8003` with exit `2` even when its imports differ, where it was a
  `LinkError`. A module this host cannot link is now the defect `D090` "the host could not
  link the module" (exit `70`) from `run` and `test` alike. A stripped module is still
  compared after instantiation.
- A source file that is not UTF-8 is now the diagnostic `V0015 source_not_utf8` (design
  §17.243 (3), §17.253 (1)) instead of a usage error with exit `2`: `check`, `build`, `run`,
  `test`, `verify`, `fmt` and the LSP report it at the first invalid byte (the message names
  the byte and its offset), `--json` carries it like any `V` with exit `1`, the file's other
  diagnostics are suppressed and the other files are checked. `sakura fmt` and edits never
  rewrite such a file. `sakura.toml` that is not UTF-8 is still a usage error.
- `[permissions] process` in `sakura.toml` is reserved (design §17.253 (3)): `process = []`
  is accepted and grants nothing, the same as leaving the key out, and a non-empty list,
  which used to be accepted and granted nothing either, is `V8006 manifest_invalid` at the
  value: "`process` grants nothing in this version: no std effect starts processes; the key
  is reserved".

- Diagnostic, explanation and generated-page text no longer names milestones, design-log
  entries or another release: `V8005` reads "Sakura Lang does not support packages yet",
  `V9002`'s root-handler message says which effects get root handlers, and `D096` no longer
  names `SAKURA_MAX_HEAP`, which the WASM host does not read. Codes, `kind`s and JSON fields
  are unchanged; occurrence ids, which hash the message, change with it.
- Every `--json` record carries `kind` and `schema_version` (design §17.232 (b)):
  `sakura doc --json` gains `"kind": "symbol_query"` and `"schema_version": "1.0"`, and
  `target/sakura/manifest.json` gains `"kind": "manifest"` (still schema 1.1). `sakura verify
  --json` on a program that does not compile now writes its diagnostics and summary at schema
  1.1, like every other `verify` record, instead of 1.0. The fields of every kind are listed in
  `compiler/contracts/json.txt`, which a test checks every command's output against.

- **Breaking:** `sakura.toml` refuses `[dependencies]` with `V8005 dependencies_not_supported` (RFC 0001,
  design §17.232). Packages arrive in a later minor release and the key is reserved: an empty
  table, entries, an inline `dependencies = {}` or a dotted key is an error at the key from
  `check`, `build`, `run`, `test` and the LSP before anything runs, and nothing is fetched. Remove
  the table.

- **Breaking:** a `sakura.toml` that is not a valid manifest is `V8006 manifest_invalid` (design §17.239): an
  unknown table or key, a value of the wrong type, a missing `[package] name` or malformed TOML is
  reported by `check`, `build`, `run`, `test`, `verify` and the LSP at the place in `sakura.toml`,
  with the parser's message, `--json` records and exit 1. It replaces the human-only
  `sakura.toml: …` usage error (exit 2) that only `run`, `test --record` and `verify` printed. Beside
  `[dependencies]` both `V8005` and `V8006` are reported.

- **Breaking:** a connection that drops while `Http.host()`'s `send` reads the response body fails with
  `HttpError` (`status: None`, the reason from the host), as a failed connection already did,
  on both backends: the JS backend reported the defect `D091`, and `--target wasm` ended the
  process with an unhandled promise rejection, also when a task was cancelled while its `send`
  read a body (design §17.209 (2)). A program that retries on `HttpError` now retries a cut
  body too; nothing else changes. This is a semantics change: owner review requested
  (§17.209 (2)).

- **Breaking:** a program means the same whatever its checkout's line endings (design §2,
  §17.180 (4b)): a `\r\n` inside a string literal is `\n`, and a line comment ends before the
  `\r`, so a CRLF file no longer prints a longer string. `sakura fmt` writes LF (a bare `\r` in
  a string of a CRLF file becomes the `\r` escape), and `sakura fmt --check` names CRLF line
  endings as the reason a file is listed. `sakura test` reports a snapshot that differs only
  in `\r\n` against `\n` as a line-ending mismatch.

- WASM modules record runtime ABI `0x010400` (1.4.0: 1.3.0's counters and 1.4.0's native
  profile text, design §17.229 (1)). A 1.2 host refuses them with `V8003`, as the ABI's MINOR
  rule says, since a module now writes counters an older host does not read; the 1.4 host still
  runs 1.1 and 1.2 modules.

- Release flags follow the version string (design §17.249): a plain `X.Y.Z` such as `0.1.0` is a
  GitHub release marked latest and publishes to npm `latest`; only `-alpha.N` or `-rc.N` is a
  GitHub prerelease under the `alpha` or `rc` dist-tag. The release notes call every 0.x version
  a pre-1.0 preview with no stability promise yet (RFC 0001, STABILITY.md).

- The documented npm install is the plain `npm install --global sakuralang`: the packages ship
  no lifecycle scripts, and `release.py` now refuses to pack one that declares `preinstall`,
  `install`, `postinstall` or `prepare`.

- The JS and WASM hosts answer an OS error by its class, as the native host does (design
  §17.229 (5)): a permission error is `Err("permission denied")`, not Node's `EACCES:
  permission denied, open '…'`; likewise `already exists`, `is a directory` and `not a
  directory`. On Node a sleep longer than 2^31 − 1 ms (about 24.8 days) now waits its full
  duration instead of firing after 1 ms.

- An `Int` interpolated into `sql"…"` is bound as an SQLite INTEGER on the JS and WASM hosts,
  as natively. It was bound as REAL, so `sql"select {7} / 2"` gave `3.5` (and an `Int` row
  failed to decode) where SQLite's integer division gives `3`, and `typeof(?)` said `real`.

- `Json.decode`, `Cli.parse` and `Sql.query` into a tuple type the program never builds (say
  `(Int, Float, Bool)`) work on `--target wasm`; they stopped with `D090 a malformed value
  crossed the host boundary`.

- `sakura check`, `build`, `run`, `test`, the LSP and the MCP server read std's analysis from a
  cache on disk instead of analysing std in every process (design §17.195 (3), §17.214 (5)). The
  cache holds one file per compiler build in `<cache>/sakuralang/std`, `<cache>` being
  `$SAKURA_CACHE_DIR`, else `$XDG_CACHE_HOME` or `~/.cache` on Unix and `%LOCALAPPDATA%` on
  Windows; `SAKURA_NO_CACHE=1` turns it off. Output is unchanged with and without it; a damaged
  or missing file is recomputed and rewritten. The files carry no compatibility promise
  (STABILITY.md).

- A process that analyses several programs (the LSP and MCP servers, test suites) analyses
  the std library once and reuses it for every program after the first; a single `sakura
  check` is unchanged. Snapshot files now list std first, so `FileId`s in `--dump core` and
  the Core snapshot's `files` change order, and the snapshot hash changes (design §17.195).

- An analysis that reuses std carries std's symbols, interface hashes and evaluated names and
  its lowered items instead of walking its 45 trees again (design §17.204 (2)).

- `V1002` on `Type.member` suggests the one exported function that is a prefix of the typed
  name or extends it (`List.lenght` → `len`, `List.fil` → `filter`) when no near spelling
  exists, as a `suggested` fix, never `machine_applicable`; `sakura explain V1002` names
  std's `len` convention (design §17.182 (2)).

- `V1002` on `Type.member` no longer falls back to the one function within two edits when the
  usual did-you-mean finds none: `List.size` gets no `List.slice` fix, and `List.lenth` →
  `List.len` comes from the prefix rule as `suggested`, not `machine_applicable` (design §17.187,
  withdrawing §17.165 (2)'s fallback, which shipped in `v0.1.0-alpha.1`).

- `V2002` names a `Type.member` callee as written: ``List.map` (std/list) takes 2 arguments,
  this call passes 1`` rather than the bare `map` (design §17.182 (3)).

- `--target wasm` runs a `perform` bound to a handler literal in the same function (design
  §17.113 (a)) as a direct call of its clause, and gives such functions and their clauses
  direct twins, so a loop performing a local handler's operation allocates no continuation
  frames: `handled_d1`, `_d4` and `_d16` run 4 frames per `run(1000)` where they ran 1,005
  (design §17.211 (1)). Output, traces and defects are unchanged.

- `--target wasm` calls a closure inside a direct function through the closure's own direct
  twin when it has one, so `List.map`, `fold`, `filter` and the other std functions taking a
  callback run their loop without continuation frames: `contracts_plain`, `pipeline`,
  `shapes`, `map_set` and `nbody` run 4 frames at their smallest size where they ran 1,121,
  2,508, 5,010, 2,005 and 7,005 (design §17.211 (2)). A callback that performs still runs
  through frames. Output, traces and defects are unchanged.

- On `--target wasm`, a fused `xs[i].expect(…)` or `map[k].expect(…)` (design §17.151) reads
  the element in place instead of building and dropping an `Option`; output and defects are
  unchanged.

- `sakura test --contracts --target wasm` runs the synthesized contract properties on WASM
  instead of refusing, with the counterexamples, shrinks, stored regressions and exit code JS
  reports for the same seed (design §17.163 (4)). `sakura test --record --target wasm` is
  refused with `` `--record` needs `--target js` `` before the permission check.

- `sakura build --target wasm` drops the checks of obligations `sakura verify --target wasm`
  proved on the `i64` profile, as the JS build does for the `js` profile, and its manifest
  says `proved` for those clauses (design §17.163 (4)). The JS modules a WASM build
  writes for decoding keep every check.

- `sakura build` deletes a proved check's predicate along with the check: an `ensures` or
  `where` built from comparisons, arithmetic, projections and constructors is no longer
  evaluated once `sakura verify` proved it. A predicate that calls a function is still
  evaluated, so a defect inside that function is still reported (design §17.98, §17.118).

- `==` and `!=` on an enumeration whose constructors have no fields (`type Dir = North | South`),
  or against a field-less constructor (`shape == Empty`, `o == None`), compare constructor tags
  instead of walking the values, on JS and WASM; results, traces and defects are unchanged and
  `--dump core` shows the comparison as `tag_eq` (design §17.183).

- Exhaustiveness checking costs each `match` arm the earlier arms with the same head constructor,
  not every earlier arm (design §17.200 (8)), so a long `match` of guarded rows alternating with
  literal rows no longer checks in quadratic time. Diagnostics are unchanged.

- The exhaustiveness index recurses into every column a `match` specialises (design §17.200 (8)),
  so thousands of arms under one constructor (`Some(1)`, `Some(2)`, …), over a tuple of literals
  or over nested constructors no longer cost each arm every earlier arm. Diagnostics are
  unchanged.

- A loop body with more than eight CPS steps keeps the shared-continuation form instead of
  the resumable `for (;;)` loop, which copies the rest of an iteration once per step;
  `sakura build --dump direct` lists each such loop as `resumable` or
  `shared(too-many-steps(<n>))` (design §17.181 (1)).

- The Sakura skill keeps its examples, syntax list, and edit loop in a smaller entry
  point, with repair and completion detail in its references. Its 7,500-byte ASCII
  limit now measures release metadata after template fill, with a 7,000-byte source
  working target (design §17.188).

- The agent-DX offline fixture validator runs independent tasks in a thread pool,
  with `--jobs 1` for serial execution and byte-identical task-ordered reports
  (design §17.197).

- The agent-eval runner caps the episodes of one approved run with `--max-episodes` (default
  48) instead of 48 across all runs, leaving the dollar ledger as the only cap across runs;
  `agent-eval.yml` gains `seed` and `max_episodes` inputs (design §17.182 (4)).

### Removed

- The unused `[targets]` manifest table. It was parsed and never read; it is now an unknown key
  and so `V8006`.

- The native host's `SAKURA_BLOCKING_THREADS`: nothing used its blocking pool (design §17.229
  (4)), and `Sql` runs on per-database workers instead.

### Fixed

- `String.from_bytes` on the JS backend reported the wrong `Utf8Error` offset for a truncated
  or ill-formed sequence before other bytes (`[0x61, 0xC3, 0x28]` gave offset `2`, WASM and
  native `1`); it now reports the start of the first ill-formed sequence on every backend.
- `Task.scope` no longer ends its task as cancelled when a child failed after an earlier
  sibling: at the scope's end a sibling cancelled because of the failure was taken for the
  scope's own outcome, so the failure never reached a surrounding `try` and `main` ended with
  a defect record without a code. The first unjoined failure or defect propagates, as the
  runtime ABI says, on the JS, WASM and native backends alike.
- `String.repeat(s, n)` makes O(log n) concatenations by doubling instead of `n`: a 16 MiB
  `repeat("a", 16_777_216)` took about a second of garbage collection on JS. The result is
  unchanged.
- `sakura test --record` holds a recorded streamed body as its chunks, joined once with
  `Bytes.concat` (design §17.205 (4)), not as one boxed `Int` per byte.
- `sakura test`'s runner stops quietly with status `141` when the CLI that reads its output is
  gone (killed, for example at a time limit), instead of Node's unhandled `EPIPE` trace.

- `Serve.host()` on `--target wasm` answers a request the guest heap cannot take with `503`
  and `retry-after: 1`, writing the defect `D096` to stderr, where it was a `500` with nothing
  on stderr; the server keeps serving. A request still waiting when the program ends is now
  answered `500` (it got no answer), with one `D090` "server stopped" record unless a defect
  ended the program. On both backends, once the server stops, a request still arriving on an
  open connection is `503` with the connection closed and its handler never runs (design
  §17.223, ABI 1.2.3).

- A runtime trap while `Serve.host()` serves on `--target wasm` no longer ends the process
  with an unhandled exception: the request is answered `500`, the server stops (later
  requests are `503`), and the program exits 70 with one `D090` record that names the
  function and the PC (design §17.223 (2), ABI 1.2.3). `sakura test` reports the test as a
  defect and runs the rest on a fresh instance, as for any trap.

- `Serve.host()` on `--target wasm` no longer ends the process when a request is larger than
  `SAKURA_MAX_COPY_BYTES`: the request is answered `413` with the connection closed and the
  handler not run, as a body over 1 MiB already is, and a handler's response over the budget is
  `500`; each writes the defect `D091` to stderr, and the server keeps serving (design §17.216
  (4)). The JS backend copies nothing into a guest and is unchanged.

- On `--target wasm`, running `main` again on one instance (the benchmark harness's Node cells
  do, once per sample) no longer keeps 4 objects per run: the Node host releases each finished
  root task through the new module export `task_free`, and the clauses of a host handler
  (`Console.host()`, `Clock.host()` and the other `*.host()` handlers) are freed with it instead
  of being static (design §17.212, ABI 1.2.2).

- A WASM function with more than about 16,000 `&&`-guarded `match` rows no longer aborts Node
  after printing its output (design §17.236). Past 256 jumps back to a function's
  dispatch loop, each 256 jumps branch through a stub, so the loop header has at most
  ⌈jumps/256⌉ + 1 predecessors; V8's optimizing tier failed past 65,535. 16,384 and 32,768 rows
  run on Node and Wasmtime; functions of at most 256 jumps are byte-identical.

- Type arguments on the first alternative of a variant list (`type T = Some<Int> | Nada`) are
  `V0001`, as on a later alternative. They were accepted, and `sakura fmt` printed the
  declaration as `Some | Nada`. A lone `Name<Args>` is still an alias.

- The WASM test runner (`sakura test --target wasm`) reports a module of another ABI as
  `error[V8003]` and exits 2, as `runMain` does, instead of throwing.

- A syntax error now hides only the diagnostics that point into the body holding it (design
  §17.207 (1)). Before, every diagnostic raised while that body was checked was dropped, so
  `sakura check` could miss an error in another module, such as a bad signature (`V2002`)
  first used from the broken body, that an analysis after an edit then reported. Diagnostics
  inside the broken body, and a `V3001` about its calls, stay suppressed as before.

- After an edit, a literal into a refined type is checked again when the edited function is
  reached from the refinement only through a `requires` or `ensures` clause, in the LSP and MCP
  servers and any other process that analyses more than once (design §17.87). Before, the
  literal kept its old verdict, so a `V5004` could be missing. Only the names a refinement's
  evaluation can call now make a function's body interface, so editing a function that merely
  shares its name with a parameter or local of such a call chain rechecks its own module only.

- After an edit, an unchanged module's `test` block no longer reports a `V5004` (a literal
  that breaks a refinement) twice, or a typed hole twice, in the LSP and MCP servers and any
  other process that analyses more than once: a reused module's tests, types and effects are
  no longer checked again. Type diagnostics within a file are listed by position
  in `Analysis` results, so a reusing analysis lists them as a fresh one does; the human and
  JSON output already sorted them.

- A long flat `match` no longer overflows the compiler's stack (design §17.193): a 3,000-arm
  literal `match` crashed a debug `sakura run`, because its decision tree is an `if` chain as
  deep as its arms. Every recursive walk over an IR (the match compiler, Core passes, the LIR
  lowering, the snapshot converter, the evaluator) grows its stack on the heap when it runs
  low, so no number of arms, `let`s or constructors bounds a pass. The JS output and
  `--dump core` stop indenting past 32 levels, so deep code is linear in size (the 3,000-arm
  program's JS falls from 406 MB to 5.4 MB), and `sakura check` indexes a `type` of thousands of
  variants in linear time. Such a program still exceeds node's limits when it runs: its JS nests
  a labeled block per arm, and its WASM a `block` per LIR block.

- A 3,000-arm literal `match` now runs on node with both backends (design §17.200): a `match`
  on `Int` or `String` literals is one `switch` over its literals instead of a chain of tests
  (`--dump core` shows it) whose cases hold the arms that one row reaches, wildcard rows are
  compiled once below the literals before them, an `else` no longer nests, a join past 64
  nested labeled blocks is an arrow function, and the rows after a failed guard continue in a
  join, so the emitted JS nests as deep as the source plus a constant. On WASM a function past 256 blocks dispatches
  through two levels of `br_table` and its blocks share locals, and an `Int` switch of more
  than eight arms is a jump by index arithmetic when its keys are dense or a binary search:
  the module no longer traps at compile, and V8 no longer runs out of memory optimizing a
  function of thousands of blocks. The snapshot writer grows its stack like its reader.

- A long run of guarded `match` rows now runs on node with both backends (design §17.220):
  the rows after a failed guard follow in its `else` rather than in a join, so the JS is a
  flat sequence of `if`s and WASM has one block fewer per guard. 3,000 guarded rows in a loop
  overflowed node's parser and about 10,000 outside one exhausted its stack; `deep_ir.rs` now
  runs 3,000 in a direct-style and a performing loop and 20,000 outside one. Past 256 nested
  blocks the rest of the run starts a new chunk, and the chunks are joined as a balanced tree,
  so Core stays within the snapshot's depth bound and JS nests only log₂ of the chunks.

- Core is a function of the source: type and row parameters are numbered per definition, so
  `sakura build --dump core`, the Core snapshot and the emitted JS of a program no longer
  depend on what an earlier analysis in the same process (an LSP or MCP server, a test run)
  had checked. `sakura verify` solves the obligations of generic definitions once more, since
  their cache keys hash the renumbered Core (design §17.194).

- Tail calls around a cycle of up to four monomorphic functions without contract checks
  (`is_even`/`is_odd`, a state machine) run as one loop on both backends instead of through
  continuation frames, so they need constant stack: each function of the cycle carries a copy
  of the others (design §17.160 (5)). Output, effect order, defect messages and blame
  sites are unchanged. A generic member, or one that checks a contract, is not copied (a
  contracted function still runs the others in its own loop); non-tail calls stay calls.

- A call in tail position under an `else if`, or under an `if` nested in an `if` branch, is a
  tail call: self-recursive and mutually recursive functions written that way run as loops
  too, where they ran through frames before. A terminating program's output, effects and
  defects are unchanged.

- A tail-call cycle whose functions cannot be nested inside each other, such as three
  functions that each call both others, also runs as one loop per function: the loop
  dispatches on a state number, with one `switch` (§17.208 (1)), to its copies of the other
  functions (design §17.192 (2)). The earlier limits still apply (four functions, 1,024 copied expressions), and output,
  effects, defect messages and blame sites are unchanged.

- Terminal output names a source outside the working directory by its OS path
  (`C:\other\main.sak`, `/other/a b.sak`) instead of the URI's path (`/C:/other/main.sak`,
  `/other/a%20b.sak`) (design §17.196 (3)).

- `file://` URIs are RFC 8089 on every OS (design §17.180 (4a)): on Windows a source is
  `file:///C:/dir/src/main.sak` instead of `file://C:%5Cdir%5Csrc%5Cmain.sak`, so its module
  name, the `sakura.toml` lookup, terminal paths relative to the working directory and the
  MCP `paths` filter work there. Unix URIs are unchanged. The MCP `paths` filter accepts a
  client's spelling of a `file:` URI (`file:///c%3A/…`).

- `sakura run` exits 70 with the raw code on stderr when the program's exit code is outside
  `0..=255` (a Windows NTSTATUS such as Ctrl-C's), instead of truncating it to a byte. On
  Windows, `node` is found through `PATHEXT` (a `node.cmd` shim works), solvers are found as
  `z3.exe` and `cvc5.exe`, and test permission grants carry no `\\?\` prefix (design §17.180
  (4c)). Windows is not yet a supported platform.

- `agent_docs` normalizes Windows path separators so the skill mirror, manifest,
  hashes, and ZIP are byte-identical to Unix output (design §17.180 (5)).

- The agent skill entry point is at most 7,500 bytes of ASCII, pinned by a `sakura_cli` test,
  so it fits below the eval harness tool-output limit with its metadata filled in. Its four
  examples stay; the provenance details moved to `references/repairs.md` and the `audit` and
  `verify` notes to the cookbook (design §17.182 (1)).

### Security

- The WASM guest now also zeroes the intermediate copies of a kind-`secret` import's encoded
  arguments (`http_send`, `http_stream_open`, `js_handler`; ABI §8.1 erratum, design §17.270
  (1)): the blocks the encoder's growth freed and the `String` it copied into the transport
  `Bytes` kept a secret's value in freed heap memory until reused. Hygiene under the trust
  model, not a defence against the host; no import, export, byte or ABI version changes.

- `Http.host()` checks the manifest's `net` list on every redirect hop (design §17.262 (4),
  RFC 0002 §2.4). The JS host, and the WASM host that shares its code, checked only the first
  URL and let `fetch` follow redirects, so a granted host could send the program to any host.
  Redirects are now followed for at most 20 hops, each hop's `host:port` checked before it
  is fetched; a refused hop fails `HttpError` with the first request's message
  (`<host:port> is not in the manifest's net list`), the 21st hop fails with
  `too many redirects`, and a redirect to a scheme other than `http` or `https` fails with
  `redirect to <scheme>: refused`. A hop to another origin drops `authorization` and every
  `secret_headers` entry for the rest of the chain. No signature changes. `Http.scoped`'s
  list is still checked against the request's URL only. The native backend has no `Http`.

- On the JS backend and `--target wasm`, a symbolic link that another process plants at a
  write's path between the grant check and the open is refused with `"escapes the
  directory"` and its target is never created. Before, the host checked the path and then
  opened it in a separate call, which followed the new link. `Fs.write`, `Fs.read`, a `File`
  opened to read, write or append, and `sakura test --record`'s cassette now open the
  resolved path with `O_NOFOLLOW` and do their I/O through the descriptor (design §17.233).
  A link inside the directory that exists before the call still works. Known limit, stated
  in ABI §8.3: a parent directory swapped for a link in that window, `Sql.sqlite`'s database
  file, the directory `Fs.list` reads (entry names only) and Windows are not covered on the
  JS hosts. The native host has no window.

- A write through a dangling symbolic link inside a directory the manifest's `fs.write` grants
  is refused with `"escapes the directory"` on the JS backend and `--target wasm`, as the
  native host already refuses it. Before, `Fs.write`, a `File` opened to write or append,
  `sakura test --record`'s cassette and `Sql.sqlite` followed the link and created its target
  wherever it pointed, outside the grant (shipped in `v0.1.0-alpha.1`; N1a review). A dangling
  link pointing back inside the directory is refused too. To migrate, write to the link's
  target path instead of the link.

- **Breaking:** `SAKURA_MAX_COPY_BYTES` must be decimal digits with a value from 1 to
  2^53 - 1: any other value (empty, `0`, a sign, a fraction, an exponent such as `1e3`, hex,
  spaces such as `" 42"`, words) is the defect `D091` naming the variable and the value, at
  first use: `Http.host()`'s `send` on the JS backend, and on `--target wasm`, where the budget
  covers every host answer, the first answer or `Serve.host()` starting. A non-numeric value turned the limit off, an empty one refused
  every response body, and forms like `1e3` were accepted (design §17.209 (1)). To migrate,
  set the variable to a plain byte count or unset it for the 256 MiB default.

- `Http.host()`'s `send` on the JS backend reads at most `SAKURA_MAX_COPY_BYTES` (default
  256 MiB) of a response body, as `--target wasm` already did: a larger body, or a declared
  `content-length` over the limit, aborts the fetch and fails with `HttpError`
  `"response too large"`, where the whole body was read with no limit (design §17.205 (3)).
  To migrate, raise `SAKURA_MAX_COPY_BYTES` or read a large response with `Http.stream`.

- `sakura test --record` stores at most 16 MiB (16,777,216 bytes) of a response body, sent or
  streamed: past that, `Http.send` or `Http.stream` under `Http.recording` fails with
  `HttpError` `"response body over 16 MiB: sakura test --record stores at most 16,777,216
  bytes of a response body"` and the cassette is not appended to. A larger streamed body was
  held whole, one list element per byte, until the test runner's heap ran out (security
  review C2, design §17.205 (2)); a sent body was appended whole (design §17.209 (4), which
  also changed the message's last words from "of a streamed body"). To migrate, record a
  smaller response, or read a large one outside `Http.recording`.

- New macOS tag releases require Developer ID signing with team `P62B65A837` and
  Apple notarization before checksums, Homebrew archives and npm packages are
  assembled. Ordinary dry runs skip signing. A separate protected `sign_test`
  dispatch exercises fresh signed binaries without publishing; bare Mach-O
  executables cannot carry a stapled notarization ticket.

### Performance

- Ownership facts follow a collection through records, tuple results, borrowing
  parameters and closures, and through recursive calls (design §17.201 (3)):
  a def that threads a record of lists, such as DeltaBlue's `Planner`, updates them in
  place when its caller's record is owned, on the direct and the CPS ABI. `--dump owned`
  gains the reasons `param`, `field` and `tuple`, and prints each consuming def's
  owned-entry verdict beside the plain one. Outputs are unchanged.

- The DeltaBlue benchmark stores dense variable and constraint ids in `List` instead
  of `Map`, following design §17.217. The std index and Sakura skill
  reference document this collection choice. Oracle checksums are unchanged.

- `SAKURA_TRACE` effect traces are written in batches on JS and WASM rather than one write per
  record. The lines are unchanged; a run killed by a signal can lose the tail of its trace.

- `--target wasm` gives each variable of a function its own local only up to 256 variables;
  past that, a block's variables share locals by liveness (design §17.219 (3)), so
  V8 no longer needs memory in proportion to the variable count to compile a function of
  thousands of `let`s. Functions of at most 256 variables compile to the same bytes.

### Documentation

- CI requires authored guide examples and rendered documentation links to pass,
  reusing the existing compiler and website builds.

- Draft public-interface inventory in `STABILITY.md`, with a generated clap/environment/exit-code
  baseline and workspace drift test; classifications await RFC 0001 acceptance at v0.2.0.

- Rechecked the guides against the compiler, repaired runnable tour examples, and
  documented current-main migration notes, runtime limits, caching and release status.
  Added a broad sample gate and rendered-route/link checks; refreshed generated agent
  references and kept the skill entry point below 7,000 bytes.

## [0.1.0-alpha.1] - 2026-10-18

The first published preview (GitHub prerelease, Homebrew tap and npm). This section is the
development log as it stood at the tag, including the implementation history before it.

### Security

- The proof key is refused when its directory is writable by other users or owned by another
  user, as a readable key file already was; a key inside the project root or the proofs
  directory is refused from any working directory; `sakura build` only reads the key and
  never creates it; and a proof cache entry carries a `format` under its MAC, so an entry of
  another format is solved again (security review K1–K3, design §17.164 (3)).

- `--target wasm` checks what its JS host answers instead of trusting it (security review
  W1–W4, W7–W11; ABI 1.1.2): an operation index or a builtin result of the wrong shape is a
  `D090` defect, never a heap read, and a completion payload must be a fresh `Bytes`. A
  response over `SAKURA_MAX_COPY_BYTES` (256 MiB) is `Err("response too large")` from `Http`
  and a `D091` defect elsewhere, instead of `D096`; a full request table is `D091` for
  `Clock.sleep` (which returned at once) and `serve_listen`. A database from `Sql.sqlite` or
  `Sql.memory()` and an `Llm.anthropic` handler, with its key, are released when their handler
  ends rather than at exit. `sakura test --target wasm` runs the tests after a host fault on a
  fresh instance. On both backends `Sql.sqlite`'s `Fs.root()` runs in the clause's task, so it
  no longer spends a task number in `SAKURA_TRACE` on JS (design §17.159 (5)).

- **Breaking:** `Secret.map(s, f, reason: Trust)` takes a `@trust("…")` reason, and
  `sakura audit` lists each call beside `Secret.expose`: a pure `f` cannot perform, but a
  defect in it can print the clear value (security review S1, design §17.159 (2)).
  `Http.bearer` is unchanged for callers; std's own site is not listed.

- `Serve.host()` reads at most 1 MiB of a request body: a larger body is answered `413` and
  its connection closed without running the handler. A reply whose status or headers the
  server cannot send is reported as a `D091` defect and answered `500`, and the server keeps
  running instead of stopping (security review H1, H2).

- `--target wasm` now sends `Secret` HTTP headers (`Http.bearer`, and `Llm.anthropic` over
  `Http.host()`) instead of refusing the request, as gate 10 approved (security review W0):
  `http_send` and `http_stream_open` are kind-`secret` host imports beside `js_handler`, and
  cassettes still see the header redacted. The guest and the host overwrite their transport
  copies of the value once it has crossed (ABI 1.1.1 erratum, §8.1 and §8.4).

- The installer verifies GitHub artifact attestations when `gh` is available and fails
  on verification errors; otherwise it reports "integrity verified, not attested".
  Installation now requires an absolute prefix and uses a securely created staging file.

- M3 exit gate 10 passed on 2026-10-18: the [security review](docs/reviews/m3-security-review.md)
  of the escape rules, the cassette format and the `Untrusted` template rule is signed by
  Claude Fable 5.1 under the owner's written delegation (design §17.159; not the owner's
  personal review). M3 is complete and the reviewed code is no longer frozen. The review
  records follow-up findings as plan WS12: `Secret.map` will take a `Trust` reason, plain
  `sakura test` will pass empty permissions to the runtime, `Serve.host()` gains a request body
  cap, the proof key's directory mode is checked, non-release workflows pin action SHAs, and
  WASM may send `Secret` headers once the approved transport change lands.
- `sakura test` always passes `SAKURA_PERMISSIONS`: without `--record` its grants are empty,
  so a test that installs `Env.host()` reads no runner variable and `Sql.sqlite` under
  `Fs.host()` cannot create a database file (WS12 item 2, review R1/R2). `Test.fixtures()`
  keeps its own grants.

### Changed

- **Breaking:** Sakura Lang source files end in `.sak` instead of `.sakura`, a clean switch
  with no fallback (owner decision, design §17.179). Rename sources with
  `git mv main.sakura main.sak`; `sakura` names the rename when given a `.sakura` file or a
  tree with only `.sakura` sources. The VS Code extension, the skill bundle, the agent-DX
  tasks (revision 2) and every document follow. `sakura.toml`, the `sakura:///std/…` URIs
  and the `sakura` language id are unchanged. <!-- rename:keep -->

- Design round 13 (§17.169–§17.178) defines M5's offset32 native ABI, native host coverage,
  single-thread execution and cancellation, proven-copy warnings, four memory-checking
  lanes, dependency pins and staged platform matrix; regions and representation changes
  remain deferred pending measurements.
- N0a adds opt-in native codegen/host skeletons, exact toolchain dependency pins and a
  platform-A CI job that links and executes a constant object and a test-only runtime
  staticlib wrapper. Real runtime staticlib linkage and the ABI export move to N1a;
  native compilation and execution of Sakura programs remain unimplemented.
- `V2002` on a call to a declared function names the callee and its home module and fills
  `expected`/`actual` with the two counts (`` `Env.get` (std/env) takes 1 argument, this call
  passes 2 ``); a call through a function value keeps the type form (design §17.165).
- `V1002` on `Type.member` suggests the one function within two edits when the usual
  did-you-mean finds none (`List.lenth` → `List.len`, machine-applicable) (§17.165).
- `V3001`'s `with` fix names the effect's one deterministic `std` handler when there is one
  (`with Env.of(?vars)`, still `incomplete`), and `sakura explain V3001` says how to recover
  from a residual `Fail<E>` (§17.165).
- The agent-eval harness (version 2) records each lookup with its size and success, splits
  reported diagnostics from codes quoted in documents, reports the remaining input and the
  refused request, and pairs input-cap arms (`--input-caps 40000,60000`) (§17.165).

- Design round 12 (§17.160–§17.168): design §12.1 now lists only the `sakura build` flags that
  exist (`--release` is removed; every build keeps its checks) and marks `sakura mcp serve --tools`
  as planned; `sakura verify --json` will report `summary.reused` under schema 1.1 and reuse
  `unknown(resource_limit)` results; WASM defect records will match JS in code, message and site
  before M4 is called complete; the rest of M4 is ordered in plan `m3-completion-and-m4.md` §3.8,
  M5 planning opens in `docs/plans/m5-native.md`, and `docs/plans/road-to-1.0.md` is a provisional
  outline.

### Fixed

- `--target wasm` defect records now match `--target js` in code, message and site (design
  §17.162): a contract defect says ``precondition `nonzero` of positive violated`` (not
  ``contract `nonzero` violated``) and carries the same `spans`, the blamed call first and the
  clause as `definition`; `Option.expect` reports the program's own message, and `D004`,
  `D011` and `D031` name the value, the inner defect and the offset as JS does. The spans come
  from the module's `sakura.debug` section (ABI 1.1.3, wording only), so `--strip` drops them.

- A cassette line that lacks an exchange field (or has one of the wrong type) now fails
  `Http.recording`'s open with `<path>:<line>: not a cassette exchange (missing <field>)`
  instead of a `D090` at replay (security review C1).

- The Sakura skill starts with a bounded guide read and recommends targeted lookup
  to avoid repeated full-guide input costs. The first paid DX5 pilot now has a
  public report; no additional episodes are authorized.

- A `requires` violated inside a self-tail-recursive function now blames the self call
  (`down2(n - 2)`) instead of the outer call that entered the loop. Contracted functions
  take their blame site as a trailing parameter on the JS backend, and predicates that
  cannot defect skip the `D011` bracket (design §17.115); defect codes are unchanged.

- M4 documentation now describes source-built WASM execution and verification,
  CI parity, the pending Secret-header transport review, and implemented versus
  planned backend and agent tooling. Added the static-verification tour.

- The committed skill manifest omits its history-dependent `commit` field; file and
  ZIP SHA-256 hashes remain. Identical contents stay fresh across commits and rebases,
  and generation no longer requires Git history. CLI commit metadata stays build-time.

- Restored std index complexity and behaviour notes and added a task-oriented lookup
  table; the checked skill reference and downloadable bundle include both.

- Website production builds preserve Fumadocs search highlights without Lightning CSS's
  false positive `::highlight` warnings; Turbopack still minifies the emitted CSS.

- Native benchmark builds include the OCaml shared modules and Unix library and
  resolve Koka shared imports from the entry directory, matching the independent
  verifier's C build configuration without changing port algorithms or toolchain pins.

- Benchmark nbody uses persistent `List.set`, and the countdown hand-JS control
  checks arithmetic overflow in source operation order (§17.150). Checksums are
  unchanged; pre-change ratios are excluded across these definition boundaries.
  The WS-S10 evidence records the remaining controls' arithmetic-check gaps.

### Added

- `--target wasm` runs direct code without continuation frames: each direct top-level
  function whose body is first-order gets a native twin with `Int`/`Float` arguments in
  registers, and deep recursion falls back to frames past a stack gauge (design §17.118).
  `fib` 30 runs about 4× faster on Node; output, defects and traces are unchanged.
  `--no-direct` builds the all-frame module.
- The WASM module's `stats()` export also reports frames, `Int`/`Float` boxes, `Dup`s and
  `Drop`s (runtime ABI 1.1.0), and `SAKURA_WASM_STATS=<path>` makes the WASM host append them
  at exit. The LIR computes the same direct facts as `sakura build --dump direct`.

- `--target wasm` runs `Http.host()` sends and streams, `Sql.sqlite`, `Serve.host()`,
  `Llm.stream` through `Llm.anthropic` and `Log.json()` as the JS backend does; every
  semantics fixture and example agrees with JS on output and effect trace, with `@js`/`@wasm`
  fixture expectations where `Int` differs, and the parity harness reports which host
  imports the corpus exercised (plan M4 W2). Until the owner's review of `Secret` transport
  (design §17.155), a WASM request carrying `secret_headers` is refused rather than sent.
  JS traces no longer spend a task number on a refinement check inside a JSON decode, so task
  numbers compare exactly across backends.

- A VS Code extension in `editors/vscode/` (not published; build a `.vsix` locally):
  `.sak` highlighting generated from the website grammar, language configuration, and a
  client for `sakura lsp` per project (nearest `sakura.toml`) with error-page links,
  quick fixes showing their impact class, stale-fix refusal, Show Hole Context and format on
  save. `sakura lsp` diagnostics gain `codeDescription` links to `/docs/errors/<code>`, quick
  fixes carry `data.impact`/`data.applicability`, and a client supporting `documentChanges`
  gets fixes pinned to the document version. New docs page: Editor setup.

- Release preparation for `v0.1.0-alpha.1` (not published): the workspace version is
  `0.1.0-alpha.1`; `sakura --version` and MCP `compiler_info` (`commit`) report the source
  commit of a stamped build. `release.yml` builds macOS arm64 and Linux x86-64 archives
  with SHA-256 sums, a provenance record, the skill ZIP, `tools/install/install.sh`
  (pinned version, strict checksum verification), a Homebrew formula and npm packages;
  a manual run is a dry run, and every publishing step waits for the owner.

- Twelve agent-DX pilot tasks with isolated oracle fixtures, gold/hollow solutions,
  schema and frozen-baseline checks in ordinary Cargo CI. The protocol defines
  the Anthropic-only $10 pilot budget; paid execution and its runner remain deferred.

- The agent-DX episode runner (`evals/agent-dx/harness/runner.py`): isolated episodes,
  hidden oracles on frozen snapshots, JSONL transcripts, metrics, transcript replay
  and a scripted backend tested offline in CI. Live Anthropic runs require an owner
  approval, a budget and pinned prices, and stop at a committed $10-total spend ledger;
  `agent-eval.yml` is manual-dispatch only. No paid run has happened.

- Eight offline cookbook projects cover refined JSON, file CLI, HTTP retry/cassettes,
  SQLite parameters, direct router tests, scripted LLM tools, budgeted agents and
  persistent indexed updates. Cookbook pages and the embedded skill reference are
  generated from those projects; CI checks their source, fixtures and freshness.

- Downloadable Sakura skill with a byte-identical website mirror, deterministic ZIP,
  SHA-256 checksum and content manifest. `agent_docs --check` and the CLI harness
  reject distribution drift; the agents docs and `/llms.txt` link the bundle.

- Error-code reference pages at `/docs/errors/<code>`, generated by `agent_docs` for all 90
  registered codes: the explanation, the `tests/ui` program that triggers the code, its JSON
  diagnostic and, for 64 codes, a fixed program the ui suite checks clean. `sakura explain`
  prints the page URL after its text, and 32 explanations that lacked a repair gained one.

- `sakura new` and `sakura init` write `AGENTS.md` and a `CLAUDE.md` importing it when
  absent. `sakura setup [--client claude|codex|cursor|all] [--apply]` prints, and with
  `--apply` writes, the project-local `sakura` skill, the client's MCP entry (merged into an
  existing config; a config it cannot parse gets a printed entry and nothing is written) and
  Cursor's rule; `sakura new --setup <client>` runs it after scaffolding. `sakura mcp serve`
  without `--root` uses the nearest `sakura.toml` upward and refuses to start outside a
  project (§17.143).

- Canonical Sakura agent skill source with a compiler-driven edit loop, four offline
  examples and a fixture-seeded repair table. Generated references, embedded metadata
  and distribution are tracked separately in the agent DX plan.

- Benchmark results retain every process peak RSS, and comparisons report process medians
  alongside the unchanged maximum-based +25% RSS gate. The WS-P1 variance study and raw
  evidence are documented in `bench/README.md`.
- Benchmark runs record host load and a quiet-window verdict (`meta.host.quiet`);
  `bench bless` refuses runs that are not quiet, `bench compare` reports loaded-host runs as
  `advisory (host load L)`, `map_set` times its `Int`, `String` and tuple keys as separate
  cells, and the CI noise job runs weekly (design §17.153).

- Research on language design, agent ergonomics, memory management, effects,
  standard-library direction, and open-source engineering practices in `research/`.
- Bun / Next.js / Fumadocs website and documentation delivery in `website/`.
- Initial language design in `docs/design.md`, with proposed syntax and open decisions.
- Community governance, contribution and AI-assistance policies, security guidance,
  and dual MIT OR Apache-2.0 licensing.
- RFC and architecture decision records, compatibility policy, GitHub contribution
  forms, and bootstrap CI/dependency automation.
- `sakura mcp serve`: the compiler as an MCP server with check, symbol and hole queries,
  docs, formatting, checked patches and test runs. Setup: [docs/mcp.md](docs/mcp.md).
- `sakura lsp`: a language server with diagnostics, hover, go to definition, completion,
  formatting and quick fixes.
- `sakura fix [--preview] [--apply ids]` applies compiler fixes by impact class, and
  `sakura check --frozen-specs <symbols|all> [--since rev]` reports spec changes since a git
  revision as `V8020`.
- `sakura new <cli|lib|agent|api> <name>` and `sakura init` scaffold a project, and `sakura run` exits
  75 when `main` declares an effect that `[permissions]` does not grant.
- `sakura audit [--json] [paths]` lists every `@trust` (with its reason and sink), `@total`,
  `@external`, `@resource` and refined-type `T.from` site in your packages, as a table by file
  or as JSON Lines `audit` records plus a `summary`. It exits 0 after reporting sites;
  loading/usage failures remain errors. A literal whose refinement is undecidable at compile
  time is listed as a `from` site with `reason: "literal"`.
- `sakura doc [--search "fn(List<A>) -> Int"] [symbol] [--json]` prints signatures, effect
  rows, contracts and doc comments offline; `--json` returns the records of MCP `symbol_query`.
- `sakura test --contracts` also runs one property per `pub fn` with a refined parameter or a
  `requires` clause: inputs come from the parameter types, `requires` filters them, and `ensures`
  is checked. A property that discards more than 90% of its inputs fails and is reported as
  `W7001` at the function. A shrunk failure is saved to `tests/regressions/<fn>.json` and runs
  first next time. Functions with effects other than `Fail`, `Clock` and `Random` are skipped
  unless `@contract_test(handlers: [Effect.constructor()])` names their handlers.
- The `sakura explain` pages for `V5003`, `V5004`, `V6002`, `V6003`, `W5005` and `W7001`
  describe the M3 rules with minimal pairs; the retired `V5002` page remains for older reports.
- Test fixtures accept backend-suffixed expectations (`// expect-defect@js: D003`,
  `// expect-exit@wasm: …`) beside the unsuffixed default; the M4 harness will use them.
- `compiler/fuzz`: cargo-fuzz targets for parsing, checking, formatter idempotence and row
  unification laws, run nightly for ten minutes each, with minimized failing inputs uploaded as
  workflow artifacts.

- `Tool.of` lowers a named `@tool` function through `Tool.from` with synthesized argument schema.
- Effect-row parameters on types, including `Tool<e>`, `Agent<T, e>` and `Router<e>`.
- Opaque constructor/field access outside the defining module is rejected with `V1006`;
  compiler-known prelude `Trust` restricts `@trust("reason")` to trusted sink arguments (`V6001`).
- `sql"…"` interpolations convert through `SqlValue.from` and remain bound parameters.
- Typed failures escaping test bodies are reported as test failures.
- `sakura build --manifest` emits the trusted base and runtime contract obligations to
  `target/sakura/manifest.json`.
- `sakura build --emit lir` writes the WASM backend's validated low-level IR as text to
  `target/sakura/lir/main.lir` (M4 preview; no WASM is emitted yet). `--layout=niche` is
  accepted and refused until the `Option` niche layout lands.
- `sakura build --dump core` prints the user modules' Core, with each `Int` operation the JS
  backend's interval pass proves marked `proved` (design §17.114).
- `sakura build --dump owned` lists each collection update as `owned` (in place) or
  `shared(<reason>)` with reasons `origin`, `borrowed-after`, `captured`, `stored`,
  `escapes-call <name>` and `join`; `--dump core` marks owned updates (design §17.149).
- `sakura verify [--json] [--target js|wasm] [--rlimit N] [--solver PATH]` discharges
  contract, overflow and division obligations with Z3 5.3.1 (`proved`, `disproved` with a
  counterexample replayed through the evaluator, `unknown`, `unsupported`, and `W7002` for a
  proof over an impossible path) and records statuses in the manifest; `sakura build` then
  drops a proved `ensures`/`where` check only while its MAC-authenticated proof still matches the
  code. `[verify] rlimit` in `sakura.toml` sets the solver budget. The website now documents
  [verification](website/content/docs/agents/verify.mdx), with a captured CLI example,
  JSON Lines records, cache authentication and check-removal limits.
- `sakura test --record`, `Http.recording`, `Llm.replay` and `Test.fixtures` support
  scoped cassette recording and offline replay. Example fixtures are explicitly synthetic.
- Scoped escape diagnostics `V6002`/`V6003`, foreign-capability FFI rejection `V6001`,
  and refined-target `Untrusted.validate` with `V5003` for invalid targets.
- Bounded compile-time scalar refinement evaluation: invalid literals report `V5004`;
  undecidable literals retain startup checks without a warning.
- `Html.from` and HTML-valued template interpolation compose markup while escaping text.
- All twelve examples execute (seven run plus test, five test), including committed
  cassette replay in `llm_extract`, `agent_loop` and `secrets`. This does not close all M3 gates.

### Changed

- `sakura verify` no longer re-solves an obligation whose MAC-authenticated proof record
  still matches its key and was obtained with the same solvers: it rebuilds the record from
  the cached solver answers, so a second run on an unchanged tree prints the same records
  without solving. `unknown` results are still solved every time, and forged or foreign
  records are solved again. Proof records written before this change are not read; rerun
  `sakura verify` once so that `sakura build` drops proved checks again.

- Updated live repository links and clone instructions to `icantcodefyi/sakuralang`.

- Benchmark peak-RSS crossings above 25% now require one confirmation group on Node;
  Bun crossings warn only (design §17.136). Compare output retains both groups' maxima,
  medians and counts, and CI reuses the timing confirmation run.

- **Security (design §17.130):** an opaque type is `Schema` only inside its declaring module.
  Elsewhere, any `Schema` bound that reaches one (`Json.decode`, `Sql.query` rows,
  `Llm.generate`, `Cli.parse`, `Tool.of` arguments, `property` parameters) is the new error
  `V2033 opaque_not_schema`, so data can no longer forge a `Query<R>`, `Html` or `UrlPart`.
  `sakura test --contracts` skips a `pub fn` with such a parameter and names it. `Uuid` is now
  `String where is_canonical(it)` and `Decimal` is `DecimalRepr where is_canonical(it)`, so both
  decode with their check; programs that built `Uuid("…")` or matched on it use `Uuid.parse`.
- The JS backend emits `Int` arithmetic it can prove in range (loop counters under their
  guard, `n - 1` under `n >= 1`, a non-zero literal divisor) as the raw operator, and inlines
  the safe-integer test of every other `+`, `-` and `*`. Semantics are unchanged: every
  `D001`/`D002`/`D003` still happens where it did (design §17.114).
- A `sakura.toml` naming its package `std` or `main` is `V8002 reserved_package_name` (§17.131).
- A field assignment through a refined type (`x.n = -1` on `var x: Pos`, or `r.total.frac = …`
  through a `Decimal`) is the new error `V2034 refined_field_assignment` (design §17.135), in
  every module: the sugar rebuilt the base type and skipped the predicate. The suggested fix
  rewrites it to `x = Pos.from(PosRepr { ..x, n: -1 })`; use `Pos.validate(…)?` for data.
  Whole reassignment of a refined `var` is unchanged.
- `sakura fmt` keeps an `if … else` on one line only when every branch fits; otherwise every
  branch is a block (§17.133).
- Retired `V5002`; narrowed `W5005` to user clauses scanning collection parameters with
  `List.all`, `List.any` or `List.contains`. `String.contains` no longer triggers it.
- WS7 Phase B documents the landed CLI and language behavior, regenerates the website
  std signatures from `compiler/std/INDEX.md`, and describes early M4 R1–R3/L1–L2/V1–V2
  work separately from gated integration.
- Refreshed the tours, cassette guide and README for landed contract properties, streaming
  HTTP, `Llm.recording`, memory filesystem attenuation, untrusted templates and generic
  refinements, with [source-built CLI verification](docs/plans/m3-docs-verification.md).

- Renamed the language from Vow to Sakura Lang: binary `sakura`, sources `.sakura`, manifest `sakura.toml`, crates `sakura_*`, env `SAKURA_*`, URIs `sakura://`, repo `icantcodefyi/sakura-lang`. Diagnostic codes are unchanged. <!-- rename:keep -->

- Host handlers enforce manifest grants delivered by `sakura run` in `SAKURA_PERMISSIONS`.
- Analysis reuses unchanged modules' signatures and bodies; private-body edits lower and
  check only the edited module. Item/import/export collection remains global. Bodies reachable
  from refinement predicates participate in interface invalidation.
- Roadmap and source-build documentation now distinguish implemented M3 behavior from
  M3 exit gates, planned M4 WASM/verification and M5 native execution.
- M3 is complete pending the owner's security review (gate 10): the re-audit after the M3
  close-out passes gates 1–9 and 11 at `a843e60`
  ([gate report](docs/plans/m3-gate-report.md)). The reviewed escape, cassette and template
  code stays frozen until the [review packet](docs/reviews/m3-security-review.md) is signed.

- Faster effect handlers on the JS backend (design §17.113): an operation performed directly
  in the body of `with` over a handler literal, or over a call of a function that only builds
  one (`counter()`, `Clock.fixed(…)`), calls its clause directly instead of searching the
  handler stack. It stays a cancellation point and is traced as before, and a function whose
  only effects are handled that way runs without continuations. Other operations run a clause
  that performs nothing inline. Semantics, traces and output are unchanged.
- Direct-style JS no longer keeps a value used once by the next call alive for the rest of
  the function (design §17.138), which lowered `binary_trees`' peak memory on Node.
- Faster checked reads on the JS backend (design §17.151): `xs[i].expect(…)` and
  `Map.get(m, k).expect(…)`, when the `Option` is used only there, compile to one checked
  read that builds no `Option`. A miss is still `D030` with the same message and exit 70.
- Loops on the JS backend that perform effects whose handlers are found at run time now run
  as plain JS loops (design §17.156): an iteration whose handler answers directly continues
  in place, and only an iteration that suspends or reaches a handler that itself performs
  allocates a continuation. `handled_suspend` runs about 3× faster on Node and its peak memory
  there fell from ~120 MB to ~50–64 MB. Semantics, traces and output are unchanged.
- Faster collections on the JS backend (design §17.116): `List.map`, `filter`, `filter_map`,
  `flat_map`, `range`, `zip` and `enumerate` build their result in place before returning it,
  `for` over a list reads each leaf of the vector once, `for` over a `Map` or `Set` copies
  nothing, and string splitting and map inserts allocate less. `String.len` is documented as
  O(n) on JS: compute it once outside a loop.
- `List.set`, `List.push`, `Map.insert`, `Map.remove` and `Set.insert` update a collection
  in place when the compiler proves no other reference to it exists (design §17.149): built
  from a literal, a fresh constructor, an empty collection or an earlier such update, and only
  read (`len`, `get`, `contains`) before it is replaced. `sieve` no longer copies a trie path
  per write. Every other update stays persistent, so results are unchanged. The WASM runtime
  writes any path node with a reference count of one in place.

### Fixed

- Agent-facing status and MCP documentation now distinguish implemented CLI, stdio,
  WASM and verification capabilities from planned exports, and explain opt-in frozen specs.
  Contributor guidance points to the existing Rust workspace and its checks.

- `sakura fmt` keeps file header comments above sorted imports when an inline block comment
  precedes the first import, while moving that inline comment with its import.
- `sakura fmt` preserves U+FDD0 in strings and comments instead of decoding it as a newline,
  and keeps Unicode separators and control characters inside line comments until LF.
- `Llm.stream` under `Llm.anthropic` requests server-sent events and calls `on_partial` once per
  text delta instead of once with the whole reply.
- `sakura fmt` no longer drops comments in import lists, after `where`, or before a binary `-`,
  keeps one-line block comments inline in lists and statements, and lays out comments after
  `=` and `=>` instead of copying the statement verbatim. If formatting would still lose a
  comment or produce code that does not parse, the file is left unchanged and `V9001` reported.
- A generic refined type such as `type NonEmpty<T> = List<T> where List.len(it) > 0` is its
  base at its own arguments: `NonEmpty<Int>` now coerces to `List<Int>` for calls, methods,
  fields, patterns and constraints instead of reporting `V2032`/`V2001` about `T`.
- A non-exhaustive `match` on a refined list type reports `V4001` with the missing case instead
  of an internal compiler error.

[Unreleased]: https://github.com/icantcodefyi/sakuralang/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/icantcodefyi/sakuralang/compare/v0.1.0-alpha.1...v0.1.0
[0.1.0-alpha.1]: https://github.com/icantcodefyi/sakuralang/releases/tag/v0.1.0-alpha.1
