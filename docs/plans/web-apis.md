# Web platform APIs: implementation plan

**Status: executable plan, provisional.** Routes [RFC 0002](../rfcs/0002-web-platform-apis.md)
and design §17.262 (round 34, 2026-10-20) from the owner's request of 2026-10-19 and
[research/15](../../research/15-web-apis.md). The RFC decides the shapes; this plan decides
the order, the files, the tests and who does what. Nothing here is a date. Opus 5.5 writes
every line of code; GPT-6.1-Sol takes only the mechanical subtasks named per phase; the
owner accepts the RFC, tags and stabilizes. Each phase is sized for one Opus session:
one branch of work, one gate run, one push to `main`, one changelog entry.

## 1. Fixed inputs

- The RFC's inventory table (§1) and shapes (§2) are the scope; a phase does not add a
  signature the RFC does not name without a `design-question` in `compiler/REQUESTS.md`.
- Versions (design §17.263, §17.266): an additive phase ships in whichever release is
  next; phases 0, 1, 2, 3, R and 7a (JS and native) are in `v0.1.0`. Phases 4a and 4b (the
  breaking set, and any ABI version bump, which includes 7a's WASM step) are held as PR
  branches, rebased daily, each green on its own base (its own `test_bench.py` ABI
  assertion, §17.270 (2)), merged right after the `v0.1.0` tag in the order #5 (4b,
  1.5.0), #6 (7a-W, 1.6.0, stacked on #5), #4, then 4a (PR #7, stacked on #6, 1.7.0; §17.270 (3),
  §17.271),
  and on `main` before the `v1.0.0-beta.1` preparation commit (design §17.291 folds `v0.2.0` into the
  beta); phases 5–10 are additive and land before `v1.0.0-beta.1`; phase 11 is experimental; phase 12 closes each phase's docs and the
  STABILITY inventory rows.
- Gates per phase, from `compiler/`: `cargo fmt --all -- --check`, `cargo clippy
  --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`
  (with `SAKURA_REQUIRE_WASM=1`; add `--features native` when the phase touches
  `runtime/rt` or the native host), `cargo run -p agent_docs --locked -- --check`, the
  runtime's `node --test` suite; from `website/`: `bun run fmt:check`, `bun run lint`,
  `bun run typecheck`, `bun run build`, `bun scripts/generate-stdlib.mjs` when a signature
  changed; from the root: `python3 website/scripts/check-doc-sweep.py` when a page changed.
- Where code lives (verified at `7b4f55b5`): std modules in `compiler/std/*.sak`, their
  index generated into `compiler/std/INDEX.md` and `skills/sakura/references/stdlib.md`
  by `agent_docs`; JS externals in `compiler/runtime/js/sakura_runtime.js` (tests beside
  it, `*.test.js`); the WASM host in `compiler/runtime/js/sakura_wasm_host.js`; the shared
  WASM/native runtime in `compiler/runtime/rt/src/` (`builtins.rs`, `exports.rs`, …) with
  its contract in `compiler/runtime/rt/ABI.md`; the host import list in
  `compiler/crates/sakura_codegen_wasm/src/validate.rs` and the builtin table in
  `sakura_core::builtins`; the native host in `compiler/crates/sakura_host_native/src/`;
  fixtures in `compiler/tests/semantics/<slug>.sak` (+ `.stdout`, `.wasm.stdout` only for
  a declared difference) and `compiler/tests/ui/<code>_<slug>.sak`; parity in
  `compiler/crates/sakura_cli/tests/parity.rs`; the stdlib page is generated from the
  index; tour and cookbook pages in `website/content/docs/`.
- Fixture rule: every fixture of this plan carries, after the harness directives, one line
  `// Spec: <url>#<anchor>` naming the standard section it demonstrates, and a sibling
  negative fixture (`_rejects_<case>`) for each typed error the RFC names. The harness
  ignores plain comments; `// covers:` stays for grammar productions.
- Parity rule: a semantics fixture is the parity case. `.wasm.stdout` is written only for
  a difference the RFC declares (none expected in phases 1–6; compressed bytes and
  signatures are never printed).
- Pure modules are Sakura source and run on every backend with no runtime work. Hot codecs
  (Base64, UTF-16, hashes, AEAD, DEFLATE) are externals: one JS implementation over the
  host (`Buffer`, `TextDecoder`, `crypto.subtle`, `node:zlib`) and one Rust implementation
  in `runtime/rt` shared by WASM and native. The two agree by the vector fixtures.

## 2. Queue

| # | Phase | Version | Who | After |
|---|---|---|---|---|
| 0 | Redirect authority fix | 0.1.0 (`7dd9dd47`) | Opus | landed |
| 1 | `std/bytes` growth and Base64; `std/encoding` | 0.1.0 (`d0222605`) | Opus | landed |
| 2 | `std/url` parser, `std/searchparams` | 0.1.0 (`3481039f`) | Opus | landed |
| 3 | `std/headers`, `std/blob`, `std/form` | 0.1.0 (`d506e2b1`; §17.269) | Opus | landed |
| 4a | HTTP 1.0.0-beta.1 shape: `Payload`, `Method`, `redirect`, `Response.bytes`, `Request.bytes`/`text`; the redirect loop moves into `std/http` | 1.0.0-beta.1; PR #7 (`hold/webapis-4a`, `1f7f0ea9`) stacked on #6, ABI 1.7.0, merged last (§17.270 (3)); ratified §17.271 with two additions before merge | Opus | 3, #6, tag |
| B | Backend coverage refusal (RFC §7): a native `main` whose row leaves `Http`, `Serve` or `Llm` to the runtime exits 75 at startup | pre-rc (`796e553e`; §17.271 (9)) | Opus | landed |
| 4b | Effect additions: `Clock.monotonic_micros`, `Task.cancel`/`cancelled`, `Task.every`, console dialogs, `Json.render` | 1.0.0-beta.1; PR #5 (ABI 1.5.0) until the `v0.1.0` tag; its scope fix `ff66cded` moves to `main` first (§17.267) | Opus | 0, tag |
| 5 | `std/stream` (`Step`/`Source`/`Sink` as values, §17.280), `Encoding.decode_source`/`encode_source`, `Blob.source`, the `Http`/`Fs` views | pre-rc (`9139a178`; ratified §17.284 with one `fix(std)` commit to follow: `to_list`'s `requires`, the `Done`-arm `MaxLine` check) | Opus | landed |
| 6 | `std/compress` (streaming forms callback-shaped, §17.280 (4)) | pre-rc (no ABI MINOR; `miniz_oxide` 0.9.1 in `rt`; §17.286 (12); readings in `compiler/REQUESTS.md` 2026-10-21) | Opus | landed |
| 7a | `std/crypto` tier 1a: `CryptoKey`, digests, HMAC, HKDF, PBKDF2, import/export | 0.1.0 on JS and native (`3eab0003`); WASM `D090` | Opus | landed |
| 7a-W | `std/crypto` on WASM: the `crypto_*` `hosted` imports, `KNOWN` emptied, `cookbook/sign_webhook` | 1.0.0-beta.1; PR #6 stacked on #5, ABI 1.6.0, merged right after #5 (§17.266 (1)); its zeroing of every freed copy of a kind-`secret` import's arguments moves to `main` first (§17.270 (1)) | Opus | 7a, #5, tag |
| 7b | `std/crypto` tier 1b on JS and native: AES-GCM, ECDSA/ECDH P-256, Ed25519, X25519, JWK | pre-rc (`d401d073` vectors, `2a12eee4`; ratified §17.288; `rt`'s key copies zeroed `94ac097d`, THIRD_PARTY.md with its test `56aabd32`) | Opus | landed |
| 7b-W | tier 1b on WASM: the seven new `crypto_*` hosted imports, ABI 1.9.0 (§17.288 (10)), `KNOWN` emptied | pre-rc (ABI 1.9.0) | Opus | landed |
| 7c | `std/crypto` tier 2 on JS and WASM-on-JS (RSA, P-384/521, AES-CBC/CTR/KW) | pre-rc (ABI 1.11.0; native a typed `Unavailable`/`Unsupported`; readings in `compiler/REQUESTS.md` 2026-10-21) | Opus | landed |
| 8 | `std/websocket` client | pre-rc (`6b633a99`, ABI 1.8.0; §17.286) | Opus | landed |
| 8s | `std/websocket` server: `WebSocket.accept` inside a `Serve` handler, pure `handshake`, `origins`, `SAKURA_MAX_SOCKETS`, topics (`subscribe`/`unsubscribe`/`publish`) in the host (RFC §2.10; §17.289) | pre-rc, in the 1.0 contract (ABI 1.10.0, the first of 8s and 10 to be ready, §17.289 (11); readings and the owner's review list in `compiler/REQUESTS.md` 2026-10-21) | Opus | landed |
| 9 | `std/urlpattern` and `Router` on it | pre-rc (`b2fde35d`; §17.281) | Opus | landed |
| 10 | `std/channel` (`with_channel`, since `with` is a keyword), and `Stream.buffer`/`tee` over it (§17.280 (4)) | pre-rc, in the 1.0 contract (ABI 1.12.0, the next free MINOR after 7c's 1.11.0: tag 19 `Channel`, `rt_code_channel_end`; native as JS and WASM through `rt`; readings in `compiler/REQUESTS.md` 2026-10-21) | Opus | landed |
| 11 | `std/worker` (experimental) | first green minor; 11a (JS, inline, std; no ABI change) and 11b (WASM, native, topics in `rt`, `Worker.close`; ABI 1.15.0) landed | Opus | 10 |
| 12 | Docs, skill, inventory, prompt budget | with each phase; final pass pre-rc | Sol (mechanical), Opus (pages with code) | each phase |
| R | Land research/15 and the fixture-vector tables | 0.1.0 (`3a027001`) | Sol | landed |

"After" names what must be on `main`. 4b and 7a can run beside 1–3 in separate worktrees.

**Order of the remaining phases (design §17.279).** 5, 9, 8, 6, 10, 7b, 11, 7c (optional,
last), with 12 beside each. Phases 5 and 9 are pure Sakura and start now in the web lane's
worktree beside the perf lane's WS-P20 and WS-P15 (§17.276); phase 8 starts when WS-P20 is on
`main` and rebases over WS-P15, since both edit `sakura_runtime.js`; 6 and 10 follow 8. A
WebSocket server (`Serve` upgrade) stayed outside the RFC until round 43 (phase 8s, below). **Round 41:** phase 9 landed
first (`b2fde35d`, §17.281) while phase 5 waited on its design question; §17.280 answers it
(a stream is a value), so phase 5 is next and pure Sakura as before, then 8 as ordered. Phase 5
then landed as specified (`9139a178`, ratified §17.284); 8 is next, when WS-P20 is on `main`. **Round 42 (design §17.285):** WS-P20 is on `main` and phase 8 is in flight (it holds ABI
MINOR 1.8.0). Lanes are bounded by files, so **7b runs now on JS and native** as 7a did
(`crypto.sak`, `sakura_core::builtins`, the crypto section of `sakura_runtime.js`, the rt
crypto module and `sakura_host_native`'s table, `Cargo.toml`/`Cargo.lock`, vectors, fixtures;
the new operations `D090` on WASM under `parity.rs` `KNOWN` rows and the "waits" sentences);
its WASM step is **7b-W** (`sakura_wasm_host.js`, `WASM_HOSTED`, `validate.rs`, ABI.md §8),
after phase 8 is on `main`, at ABI 1.9.0. Phase 6 follows phase 8 (the scheduler) and 7b's
lock; phase 10 follows 6 at the next MINOR; 11 after 10; 7c after 7b-W, optional; 12 beside
each. A phase that must touch a running lane's file files the change in `REQUESTS.md` for
that lane or waits for it and rebases; a shared append-only file keeps both hunks.
**Phase 8 landed (`6b633a99`, ABI 1.8.0, after WS-P20 `fe236029`; ratified §17.286).** Its
triggers have fired: the scheduler is free, so **phase 6 starts now** in the web lane, after
three small commits the entry orders first (`http_body_of` natively, operation `requires`
checked at the perform site, the receive queue's byte bound with the pong rule); phase 6 needs
no ABI MINOR (its codec is `miniz_oxide` in `rt`) and its `Cargo.toml`/`Cargo.lock` hunk is the
lane's last commit, rebased over whatever lock `main` has, adding only its own crate
(`cargo update -p <crate> --precise <pin>`, every other line unchanged, `--locked` green), so
it no longer waits for 7b's lock. MINOR 1.9.0 is free: the first of 7b-W (after 7b is on
`main`) or phase 10 (after 6) to be ready takes it, the other 1.10.0 after it lands
(§17.266 (1)). N1b's remainder may take the next free line (`m5-native.md` §2). The rest of the
order stands. **Phase 7b landed (`d401d073`, `2a12eee4`; ratified §17.288).** 7b-W is the
ready one, so it **starts now at ABI 1.9.0** in the web lane, after two small commits the
entry orders first (`fix(rt)`: every key copy `rt::crypto` makes is zeroed before it is
freed; `docs(native)`: THIRD_PARTY.md's nine crypto rows with their notice files and the
test that keeps the table complete); phase 10 takes 1.10.0 after 7b-W lands. **7b-W
landed (ABI 1.9.0)**, after `94ac097d` and `56aabd32`. Phase 7c is no
longer optional-before-the-rc: a variant added to a `pub` std sum after the beta's preparation
commit breaks exhaustive matches, so 7c lands before the `v1.0.0-beta.1` preparation commit or its variants are not
in 1.0 (§17.288 (6)). **Round 43 (design §17.289, the owner's request of 2026-10-21):** the
WebSocket server is in the RFC and in 1.0 as **phase 8s** (`WebSocket.accept` inside a `Serve`
handler). Its files (`websocket.sak`, `serve.sak`'s comments, the WebSocket section and
`serveHttp` of `sakura_runtime.js`, `served` in `sakura_wasm_host.js`, `WASM_HOSTED`,
`validate.rs`, ABI.md, fixtures, `websocket.test.js`, `tests/wasm.rs`) are disjoint from phase
6's and 10's but for `sakura_runtime.js` by section, so **8s starts now in its own lane if an
implementer is free, else after phase 6 in the web lane; before 10 in either case**; the web
lane's order reads 7b-W, 6, 8s, 10, 11. One ABI MINOR inside its commit: the first of 8s and
phase 10 to be ready takes 1.10.0 and the other 1.11.0 after it lands (§17.266 (1), as
§17.286 (12) ordered 7b-W and 10); 1.9.0 stays 7b-W's. **7c runs in its own lane now** under
its deadline, its lock hunk by the shared append-only rule.

## 3. Phases

### Phase 0: redirect authority fix

- **Scope.** `Http.host()` follows at most 20 redirect hops; each hop's `host:port` is
  checked against `net` before it is fetched; a refused hop fails `HttpError` with the
  existing message form (`<host:port> is not in the manifest's net list`);
  `authorization` and every `secret_headers` entry are dropped on a hop that changes
  origin; the 21st hop is `HttpError("too many redirects")`. No signature changes. Under
  `Http.scoped(hosts)` the hops are checked against `net` but not against the scoped list,
  which the host cannot see; phase 4a closes that (design §17.263 (4)).
- **Files.** `compiler/runtime/js/sakura_runtime.js` (`fetchArgs`, the `http_send` and
  `http_stream_open` paths: `redirect: "manual"`, a hop loop), `sakura_wasm_host.js` if it
  wraps those paths, `compiler/runtime/js/*.test.js`, `compiler/std/http.sak` doc
  comments, `CHANGELOG.md` `[Unreleased]` → Security.
- **Tests.** Runtime tests with a scripted `fetch`: granted → granted hop follows; granted
  → ungranted hop fails with the message and does not fetch; `authorization` and a secret
  header are absent on a cross-origin hop and present on a same-origin hop; 21 hops fail.
  One semantics fixture `http_redirect_granted.sak` under `Http.scripted` is not possible
  (scripted has no redirects), so the runtime tests are the evidence; say so in the PR.
- **Docs.** `website/content/docs/tour/capabilities.mdx`: one sentence that `net` is
  checked on every redirect hop; `manifest.mdx` likewise.
- **Who.** Opus.
- **Acceptance.** The four runtime cases pass on JS and through the WASM host; gates
  green; changelog entry under Security.

### Phase 1: `std/bytes` growth and Base64; `std/encoding`

- **Scope.** `Bytes.slice`, `Bytes.concat`; `Alphabet`, `to_base64`, `from_base64`,
  `from_base64_lenient`, `Base64Error`; `std/encoding` with `Encoding`, `Encoding.parse`,
  `Errors`, `Bom`, `EncodingError`, `decode`, `decode_with`, `Decoder.{new, push,
  finish}`, `encode` (`requires encodable(enc)`: Windows-1252 is decode-only, design
  §17.263 (3)). `String.from_bytes` and `String.to_bytes` unchanged.
- **Files.** `compiler/std/bytes.sak`, new `compiler/std/encoding.sak`, `compiler/std/INDEX.md`
  (authored dependency graph line; signatures regenerated), `sakura_core::builtins` (new
  externals), `sakura_runtime.js` (`bytes_slice`, `bytes_concat`, `bytes_to_base64`,
  `bytes_from_base64`, `bytes_from_base64_lenient`, `encoding_decode`, `encoding_encode`;
  `Decoder.push`/`finish` are Sakura over the one stateless decode external, holding an
  unfinished sequence between pushes, design §17.263 (5)),
  `runtime/rt/src/builtins.rs` (+ a new `codec.rs`; `base64` and UTF-16 by hand, no
  dependency, or `base64` crate `no_std` if its MSRV fits; Windows-1252 as a 32-entry
  table), `sakura_codegen_wasm/src/validate.rs` (`hosted` is not used: these run in the
  guest), `ABI.md` §3/§8 if a new builtin kind is needed, native: nothing beyond `rt`.
- **Tests.** Std `test` blocks for each function. Fixtures: `bytes_base64_vectors.sak`
  (RFC 4648 §10 vectors, both alphabets, padded and not), `bytes_base64_rejects_padding.sak`,
  `bytes_base64_lenient_whitespace.sak`, `encoding_decode_utf16.sak` (BOM strip and keep),
  `encoding_decode_windows1252.sak`, `encoding_rejects_unsupported_label.sak`,
  `encoding_decoder_split_sequence.sak` (a 4-byte UTF-8 scalar split across three pushes),
  `encoding_fatal_vs_replace.sak`. Runtime `.test.js` and `rt` unit tests on the same
  vectors (Sol lands the vector table, below). Parity on all three backends.
- **Docs.** Stdlib page regenerated; `tour/types.mdx` or `tour/basics.mdx` gains one
  Base64/decoding example; skill reference regenerated by `agent_docs`.
- **Who.** Opus; Sol: `compiler/tests/vectors/base64.txt` and `encoding.txt` tables copied
  from RFC 4648 and the Encoding Standard with source URLs in the header (mechanical).
- **Acceptance.** Every vector passes on JS, WASM and native; negative fixtures assert the
  typed error; `Bytes.concat` of 10,000 one-byte values runs in linear time (a `rt` unit
  test with a count, not a time).

### Phase 2: `std/url` parser and `std/searchparams`

- **Scope.** `UrlParts`, `Url.parse`, `Url.resolve`, `UrlParts.{to_url, to_string, origin,
  with_scheme, with_host, with_port, with_path, with_query, with_fragment, with_credentials,
  search, with_search}`, `UrlError` with reasons (`Idna`, `InvalidScheme`, `InvalidHost`,
  `InvalidPort`, `Relative`); `SearchParams` with `parse`, `get`, `get_all`, `has`,
  `append`, `set`, `remove`, `sort`, `entries`, `to_string`. `Url.is_valid` unchanged.
  Pure Sakura, no externals.
- **Files.** `compiler/std/url.sak`, new `compiler/std/searchparams.sak`, `INDEX.md`
  dependency graph (`searchparams → string, list, bytes`; `url → … searchparams`).
- **Tests.** Fixture `url_parse_whatwg.sak` and its `.stdout`, generated from the curated
  150-case subset of WHATWG `urltestdata.json` checked in as
  `compiler/tests/vectors/url.jsonl` (input, base, expected href or failure) by the
  semantics test `url_vectors`, which fails when they are stale (`Test.fixtures()` is the
  package cassette directory, which a single-file fixture does not have; design §17.268
  (5)); `url_resolve_relative.sak`;
  `url_rejects_idna.sak`; `url_credentials_are_secret.sak` (the password never shows);
  `searchparams_order_and_repeats.sak`; `searchparams_plus_and_percent.sak`;
  `searchparams_sort_stable.sak`. Std `test` blocks for each `with_*`.
- **Docs.** Stdlib regenerated; `cookbook/http_retry.mdx` or a new
  `cookbook/url_build.mdx` showing `Url.parse` → `with_search` → `to_url`.
- **Who.** Opus; Sol: the `url.jsonl` subset with the upstream commit hash in its header.
- **Landed (`3481039f`; design §17.268).** As built, endorsed: the password never
  serializes; a host that needs IDNA (`xn--` labels included) is `Idna`; `ftp` is special;
  `with_scheme`/`with_host`/`with_port`/`with_credentials` return `Result`, and
  `with_path` joins them on `main`: `Result<UrlParts, UrlError>` with the new
  `UrlReason.OpaquePath` for an opaque-path URL, pinned by `url_rejects_opaque_path.sak`
  (§17.268 (3)). The 896-tuple literal that compiled to a 6 MB
  `main.wasm` is WS-P19 in the performance push, after WS-P18 and the tag (§17.268 (6)).
- **Acceptance.** All curated cases agree with the upstream expectation on three
  backends; `url"…"` templates and `Request.url` fixtures are byte-identical to before.

### Phase 3: `std/headers`, `std/blob`, `std/form`

- **Scope.** As RFC §2.3. Pure Sakura. `Form.boundary()` performs `Random`.
- **Files.** New `compiler/std/headers.sak`, `blob.sak`, `form.sak`; `INDEX.md` graph.
- **Tests.** `headers_validate_and_combine.sak` (Fetch's name/value rules, combining,
  `set-cookie` separation, sorted lowercase iteration), `headers_rejects_invalid_name.sak`,
  `blob_slice_and_text.sak`, `blob_text_rejects_invalid_utf8.sak`, `form_multipart_roundtrip.sak`
  (fixed boundary; RFC 7578 example bytes as the expected stdout), `form_urlencoded_roundtrip.sak`,
  `form_parse_rejects_limits.sak` (parts, bytes, field size), `form_filename_is_data.sak`
  (no `Fs` in the row).
- **Docs.** Stdlib regenerated; `cookbook/form_upload.mdx` (build a multipart body, send it
  with `Http.scripted`).
- **Who.** Opus.
- **Acceptance.** Fixtures green on three backends; `Form.parse_multipart` on a 1 MiB
  body with the limits set runs within the std test's default budget.
- **As built (`753d00fd`, `d506e2b1`, design §17.269).** Every reading endorsed and now
  RFC §2.3 text: `HeaderReason`, `FormLimit`, `FormReason`, `is_boundary` with
  `encode_multipart`'s `requires`, `Form` as a transparent record, `get(h, "set-cookie")`
  is `None`, Fetch's byte-sequence value rule, no credential names refused, `Blob.text`
  strict and `parse_multipart` substituting, `----SakuraFormBoundary` + 96 bits.
  `Blob.source` waits for phase 5; a binary file in an HTTP body waits for 4a.

### Phase 4a: HTTP 1.0.0-beta.1 shape

- **Scope.** `Payload`, `Request.body: Payload`, `Response.body: Payload`, `Method.Options`,
  `Method.Trace`, `Request.redirect: Redirect`, `Request.{with_payload, with_header,
  with_headers}`, `Response.bytes`. Redirect following moves into `std/http` (design
  §17.263 (4)): every `host()` and `scoped()` send reaches the host with `redirect:
  "manual"`; one Sakura function follows at most 20 hops for both, resolves `Location`
  against the request URL with phase 2's parser, checks the hop against `scoped`'s list
  (the host checks `net` on each send), drops `authorization` and `secret_headers` when
  scheme, host or port change, returns the 3xx under `Manual` and fails `HttpError` under
  `Error`; `Http.stream` closes a 3xx body and reopens. The host's hop loop from
  `7dd9dd47` is deleted in the same commit. `Http.recording` records each hop as an
  exchange; a 0.1.0 cassette still replays (its request meets no 3xx). Binary bodies sent and received on JS and WASM (the
  WASM `http_send` answer already carries bytes; the text decode moves to `Response.text`).
  `Http.recording` stores binary bodies as `response_body_base64`, which the cassette
  schema already has (§17.209); request bodies likewise. `Serve` passes `Payload` through.
- **Files.** `compiler/std/request.sak`, `response.sak`, `http.sak`, `serve.sak`,
  `router.sak`, `llm.sak` (its `Request` building), `sakura_runtime.js` (`http_send`,
  `http_stream_open`, recording), `sakura_wasm_host.js`, `runtime/rt` request/response
  layouts if they name the body field, `ABI.md` (MINOR bump if a layout changes),
  every fixture and example that writes `body: "…"` or reads `.body` (sed by hand, review
  each), `CHANGELOG.md` `[Unreleased]` → Changed with the migration sentence,
  `dogfood/*` migration list entry (Astra's list; Opus applies).
- **Tests.** `request_payload_bytes_roundtrip.sak` (scripted), `response_bytes_limit.sak`,
  `request_method_options_trace.sak`; with the loop in Sakura, `Http.scripted` can answer
  a 3xx, so the redirect cases are semantics fixtures: `http_redirect_follows.sak`,
  `http_redirect_manual_returns_3xx.sak`, `http_redirect_error_fails.sak`,
  `http_redirect_drops_secrets_cross_origin.sak`, `http_redirect_scoped_rejects_hop.sak`,
  `http_redirect_21_hops.sak`; the per-send `net` check keeps its runtime test; the
  existing HTTP fixtures updated and reviewed one by one; cassette fixtures with a binary
  body.
- **Docs.** Stdlib regenerated; `tour/capabilities.mdx`, `cookbook/http_retry.mdx`,
  `cookbook/router_endpoint.mdx` updated for `Payload`; `agents/cassettes.mdx` for binary
  bodies.
- **Who.** Opus.
- **Acceptance.** Corpus and parity green; `sakura test --record` round-trips a binary
  body; the changelog names the migration and the moved redirect loop; no `.wasm.stdout`
  added; the owner reviews the PR as security reviewer, since it relocates phase 0.
- **Branch (design §17.270 (3)).** `hold/webapis-4a`, started from `hold/webapis-7a-wasm`
  (PR #6) and opened as a PR against it; ABI 1.7.0 (`abi.rs`, the host's `ABI_VERSION`,
  ABI.md §0 and §8.2, `test_bench.py` `0x010700` in one commit), since the host reads a
  `Payload` body and `redirect` on every send; if no §8.2 line changes, the number stays
  1.6.0 and the base still holds. It rebases with the stack and onto `main` after #4 merges.
- **Built (design §17.271).** PR #7, `1f7f0ea9`, one commit on #6 at ABI 1.7.0; every
  reading ratified. Before merge, Opus adds `Request.bytes(req, limit) -> Bytes !
  {Fail<BadRequest>}` and `Request.text(req, limit) -> String ! {Fail<BadRequest>}` (strict
  UTF-8, `BadRequest("body is not UTF-8")`; `json` reads through `text`), moves
  `sign_webhook` and `form_upload` onto `Request.bytes`, regenerates `INDEX.md`, the stdlib
  page and the skill, adds the changelog line, drops the PR's `REQUESTS.md` hunk (the question
  and its answer are on `main`) and cites §17.271 in `follow`'s doc comment and ABI §0.
  The eval task `07-health-route` is revision 3 in the same PR (§17.271 (8)).

### Row B: backend coverage refusal (RFC §7)

- **Scope.** A native binary whose `main`'s effect row, after handling, leaves `Http`,
  `Serve` or `Llm` to the runtime exits 75 at startup, before any user code runs, with one
  stderr line naming the effect first ("`Http` is not available on target `native`").
  `D091` stays for any other use. A program that handles the effect inside `main`
  (`Http.scripted`, every redirect fixture) is not refused. Not a build-time diagnostic and
  no new diagnostic code (design §17.271 (9)). `WebSocket` joins the set in phase 8.
- **Files.** The native driver's entry (where `main`'s row is known at compile time, so the
  check is emitted into the binary's startup), `STABILITY.md`'s native row, the native page.
- **Tests.** One run-mode fixture under `compiler/tests/semantics` with a native-only
  expectation through `parity.rs`'s mechanism for declared differences, cited in the
  fixture header; JS and WASM unchanged. `CHANGELOG.md` `[Unreleased]` → Changed.
- **Who.** Opus, right after 4a merges and before phase 5.
- **Acceptance.** The fixture exits 75 with the message on native and runs on JS and WASM;
  gates green.
- **As built (`796e553e`).** The compiler lists `main`'s root effects (the analysis's
  `main_effects`) in the native module descriptor, which grows to 96 bytes, so the entry is
  renamed `sakura_native_main_v4`; the driver refuses `Http`, `Llm` or `Serve` after the
  `sakura.native` record's check and before the instance exists, naming the first in sorted
  order; the line has no `error:` prefix. A run-mode fixture without `sakura.toml` cannot
  declare `Http`: `sakura run` denies it on every backend (exit 75, no `net` grant, design
  §6.2) before the program starts. So the positive case is the `secrets` example (`main`
  declares `Http`, `net` granted), with `expect-exit@native: 75` and
  `expect-stderr@native` in its header in place of its `KNOWN_NATIVE` entry; the negative
  case is `tests/semantics/native_coverage_handled_http_runs.sak` (`Http.scripted` inside
  `main`), and `sakura_host_native`'s `the_driver_refuses_a_root_effect_native_lacks`
  holds the driver to the line and code. `Llm` in `main`'s row is `V9002` at build on every
  backend (no parameterless `Llm.host()`), so the driver's `Llm` entry is unreachable today.
- **Readings (design §17.277).** The example as the fixture, the unprefixed one-effect line
  and the unreachable `Llm` entry stand. Opus runs a native binary installing `Http.host()`
  and one installing `Serve.host()` and records the code each stops with in the native CLI
  parity note; `changes.mdx` and `roadmap.mdx` then name that code instead of "`D090` or
  `D091`".
- **Read (2026-10-20, aarch64-apple-darwin, release CLI with `native`).** A `main` whose row
  is `{Console}` and that runs `with Http.host() { Http.send(…) }`, and one that runs
  `with Serve.host() { Serve.serve(…) }`, both build and stop at the first operation with
  `{"schema_version":"1.0","kind":"defect","code":"D090","message":"a host operation the
  native host does not provide yet"}` on stderr and exit 70: the runtime's
  `rt_code_unsupported`, not the `Host` defaults' `D091`. `changes.mdx`, `roadmap.mdx`,
  `tour/memory.mdx`, the rt `ABI.md` §1 and the driver's comment now say `D090`.

### Phase 4b: effect additions and small functions

- **Scope.** `Clock.monotonic_micros`; `Task.cancel(f)`, `Task.cancelled(s)`;
  `std/task.every`; `Console.{alert, confirm, prompt}`; `Json.render(j, indent:
  Option<Int>)`. `Clock.fixed`, `Clock.virtual`, `Task.host`, `Task.deterministic`,
  `Console.host`, `Console.captured` implement the new operations.
- **Files.** `compiler/std/clock.sak`, `task.sak`, `console.sak`, `json.sak`,
  `sakura_runtime.js` (scheduler: cancel one fiber; `clock_monotonic_micros`),
  `runtime/rt/src/` scheduler (`task_cancel`), `ABI.md` (the task record's cancel flag;
  MINOR bump), `sakura_codegen_wasm/src/validate.rs`, native host clock, `CHANGELOG.md`.
- **Tests.** `task_cancel_fiber_runs_finalizers.sak` (effect trace shows the finalizer),
  `task_cancel_then_join_is_d095.sak` (`expect-defect: D095`), `task_cancel_finished_noop.sak`,
  `task_every_virtual_clock.sak` (ticks, a skipped tick's count, stop on `false`),
  `task_every_stops_on_scope_cancel.sak`, `console_confirm_cases.sak` (`y`, `YES`, `n`,
  empty, EOF under `Console.captured` with scripted input), `console_prompt_default.sak`,
  `json_render_indent.sak`, `clock_micros_monotonic.sak` (two reads, non-decreasing).
- **Docs.** `tour/concurrency.mdx` gains the `setTimeout`/`setInterval`/`clearTimeout`
  idioms with the web names in the text so searches find them; stdlib regenerated.
- **Who.** Opus.
- **Acceptance.** Fixtures green on three backends; `Console.captured` needs scripted input
  lines, so if it lacks them today the phase adds `Console.scripted(lines)` and says so in
  the changelog (a `design-question` is not needed: the RFC's dialogs require it).
- **As built (PR #5 `24e944a2`, design §17.267).** The dialogs are std functions over
  `print` and `read_line`, not operations, so a user `handler Console` serves them
  unchanged; `Console.scripted(lines)` is the test handler; `confirm` prints `msg [y/N]`
  and `prompt` prints `msg [default]`. PR #5's first commit `ff66cded` (a scope's failure
  propagates, not a sibling it cancelled) is not 4b: it lands on `main` before the tag as
  its own commit and #5 rebases onto it.

### Phase 5: `std/stream`

Stopped before code at `17d3b20d` (a closure cannot hold a cursor, `V2041`); answered by
design §17.280, which is the specification: a stream is a value. Landed as `9139a178`;
its readings are ratified in §17.284, which adds `to_list`'s `requires max_items >= 0` and
the `Done`-arm `MaxLine` check (one `fix(std)` commit). The evaluator's `unsupported` rows
and the two bare-corpus WASM `KNOWN` rows need no plan row (§17.284 (6), (7)).

- **Scope.** `Step<T, e> = Done | Next(T, Source<T, e>)`, `Source<T, e> = { pull }`,
  `Sink<T, e> = { write -> Sink, close }`, `StreamError(MaxItems | MaxLine)` and the
  combinators of §17.280 (3): `empty`, `from_list`, `map`, `filter`, `take`, `fold`, `each`,
  `to_list(src, max_items)`, `chunks(src, size)`, `lines(src, max_line)`, `pipe`;
  `Encoding.decode_source`/`encode_source` over `Decoder`; `Blob.source` (phase 3's
  deferral); the views `Http.body_source`, `Fs.file_source`, `Fs.file_sink`. Pure Sakura, no
  runtime work. **Not here:** `buffer`, `tee` and any `Limit` type (phase 10, over `Channel`).
- **Files.** New `compiler/std/stream.sak`; `encoding.sak`, `blob.sak`, `http.sak`,
  `fs.sak`; `INDEX.md` graph (`stream → list, bytes, string`; `encoding → stream`); design
  §8.4 one sentence (a source is a value; a view is scoped with its resource) in phase 12.
- **Tests** (§17.280 (5)). `stream_pull_persistent.sak` (the same pure source value pulled
  twice yields the same items; `Done` is stable), `stream_from_list_take_map_filter_fold.sak`,
  `stream_chunks_bytes.sak` (an empty `Bytes` item is data; re-sizing across items),
  `stream_lines_split_across_chunks.sak` (a line across two items, CRLF, a final
  unterminated line), `stream_lines_rejects_max_line.sak`,
  `stream_to_list_rejects_max_items.sak`, `encoding_decode_source_split_sequence.sak`,
  `stream_pipe_to_file_sink.sak` (`Fs.memory`), `http_body_source_scripted.sak`,
  `blob_source.sak`, `stream_view_cannot_escape.sak` (UI, `V6003`; if the escape check does
  not reach a closure's captured resource, file a `design-question`, do not work around it).
- **Docs.** Stdlib regenerated; `cookbook/stream_lines.mdx`; `tour/memory.mdx` one
  paragraph: a source is a value, a view is a resource's position; the module header states
  the `V2041` reason.
- **Who.** Opus.
- **Acceptance.** Fixtures green on three backends with identical effect traces, no
  `.wasm.stdout`; no combinator holds more than its argument names (reviewed by reading the
  module); no ABI, JSON or diagnostic change.

### Phase 6: `std/compress`

- **Scope.** RFC §2.6: `Format`, `compress`, `decompress`, `CompressionError`; the
  streaming forms are callback-shaped views over a scoped codec state (§17.280 (4)):
  `decompress_source(src, format, max_out, body)` and `compress_source(src, format, level,
  body)`, the body receiving a `Source<Bytes, …>` valid inside it; a codec state is never a
  pure source.
- **Files.** New `compiler/std/compress.sak`; `sakura_core::builtins`; `sakura_runtime.js`
  (`node:zlib` sync for whole-buffer, streaming via the zlib stream classes behind the
  scheduler); `runtime/rt` (`miniz_oxide` with `no_std`; a streaming state handle in the
  guest heap, no host import); `compiler/Cargo.toml`/`Cargo.lock` (workspace dependency,
  latest stable, exact pin as the native crates do); native: nothing beyond `rt`. Starts now
  (§17.286 (12)): no ABI MINOR; the `Cargo.toml`/`Cargo.lock` hunk is the lane's last commit,
  rebased over `main`'s lock and adding only its own crate.
- **Tests.** `compress_roundtrip_formats.sak` (prints decompressed text and lengths only),
  `decompress_rejects_output_limit.sak`, `decompress_rejects_truncated.sak`,
  `decompress_rejects_malformed.sak`, `compress_source_roundtrip.sak` (through
  `Stream.chunks`), `decompress_known_gzip_bytes.sak` (a checked-in gzip file from
  `Test.fixtures()` with its known plaintext: the one direction that is a contract).
  `rt` unit tests on the same files.
- **Docs.** Stdlib regenerated; the cookbook stream page extends to gunzip.
- **Who.** Opus.
- **Acceptance.** Round trips agree on three backends; decompression of the checked-in
  files is byte-identical; no fixture prints compressed bytes; the dependency is pinned and
  its licence listed where the other runtime crates are.
- **Landed.** As scoped, on JS, WASM and native, with
  `compress_rejects_level.sak` and `tests/ui/compress_view_cannot_escape.sak` (`V6003`) added
  to the tests. `Test.fixtures()` cannot be reached from a run-mode fixture, so
  `decompress_known_gzip_bytes.sak` embeds the Base64 of `compiler/tests/vectors/compress/`.
  `runtime/rt/tests/compress.rs` decodes the same files and fails if the two copies drift
  apart. Headers, trailers and checksums are parsed in Sakura, so errors and offsets agree on
  every backend. Readings beyond the RFC, including `decompress_source`'s `Fail` row and a
  `V6003` gap, are listed in `compiler/REQUESTS.md` (2026-10-21) for the designer.

### Phase 7a: `std/crypto` tier 1a

- **Scope.** `CryptoKey` (opaque, `Secret<Bytes>` material), `Algorithm`, `Usage`,
  `KeyFormat`, `Digest`, `digest`, `hmac`, `derive_bits` for `Hkdf` and `Pbkdf2`,
  `import_key`/`export_key` for `Raw` (HMAC, HKDF, PBKDF2 keys), `generate_key` for HMAC,
  `KeyError`, `CryptoError`. `verify` for HMAC (constant-time compare).
- **Files.** New `compiler/std/crypto.sak`; `sakura_core::builtins`; `sakura_runtime.js`
  (`node:crypto`'s synchronous `createHash`, `createHmac`, `timingSafeEqual`, `hkdfSync`,
  `pbkdf2Sync`; builtins are direct calls, so `crypto.subtle` and an import cache are
  struck, §17.266 (9)); `runtime/rt`: RustCrypto `sha2`, `hmac`, `hkdf`, `pbkdf2` at exact
  pins behind the `native` feature (with `ctutils`/`cmov` transitively; the wasm32 object
  is bare `rustc` and carries none of them), `Cargo.toml`/`Cargo.lock`.
- **WASM step (7a-W, held; design §17.266 (1)).** `sakura_wasm_host.js` + `validate.rs`
  `hosted` imports (`crypto_*`), `ABI.md` §0 and §8.2 (MINOR, 1.6.0; `sakura_abi_version`
  `0x010600` on both hosts), the seven `KNOWN` rows in `parity.rs` and the three "waits"
  sentences (`INDEX.md`, the skill, the capabilities tour) deleted, `cookbook/sign_webhook`
  and its example project. Branch from `hold/webapis-4b`, rebase with it, merge right after
  it. A non-JS WASM embedder can switch to the guest implementation later (a one-line
  `validate.rs` change plus a corpus run, once the wasm32 object builds with dependencies).
- **Tests.** `crypto_sha2_cavp.sak` (NIST short-message vectors, Sol's table),
  `crypto_hmac_rfc4231.sak`, `crypto_hkdf_rfc5869.sak` (A.1–A.3; A.4–A.7 are SHA-1),
  `crypto_pbkdf2_rfc7914.sak` (§11, PBKDF2-HMAC-SHA-256; RFC 6070 is SHA-1 and SHA-1 is not
  offered), `crypto_derive_rejects_bounds.sak` (`Length`, `Iterations`, `Info`, including
  the 2^20-byte PBKDF2 output bound of §17.266 (7)), `crypto_verify_rejects_tampered.sak`,
  `crypto_key_never_shows.sak` (a `CryptoKey` in `show` is `V2031`, a UI fixture),
  `crypto_export_rejects_not_extractable.sak`, `crypto_usage_rejected.sak`,
  `crypto_generate_under_seeded_random_is_deterministic.sak` (and the docs sentence that
  this is why `Random.seeded` is test-only).
- **Docs.** Stdlib regenerated; `tour/capabilities.mdx` section "Secrets and keys" (a
  tested HMAC webhook check); `cookbook/sign_webhook.mdx` (HMAC verify of an incoming
  request) in 7a-W, because example projects run on WASM with no `KNOWN`.
- **Who.** Opus; Sol: `compiler/tests/vectors/sha2.txt`, `hmac.txt`, `hkdf.txt`,
  `pbkdf2.txt` from the RFCs/CAVP with source URLs.
- **Acceptance.** Every vector passes on three backends (WASM through the host, native
  through `rt`), met when 7a-W merges; the UI fixture pins the no-`Show` rule; `cargo
  deny`-style licence check if the workspace has one, else the licences listed in the PR;
  the owner reviews the crate set as security reviewer. SHA-384/512 HKDF and PBKDF2 have
  no published vectors: JS (OpenSSL) = native (RustCrypto) parity on literal inputs is the
  evidence (§17.266 (5)).
- **Landed (`3a027001`, `3eab0003`; design §17.266).** JS and native; WASM `D090` until
  7a-W. Beyond the RFC's list, endorsed and now RFC text: `algorithm`, `usages`,
  `extractable`, `allows`, `permits` (`@total`), `HkdfParams`, `Pbkdf2Params`,
  `DeriveParams`, `KeyReason`, `CryptoReason`; a key without the operation's usage is
  `D020`; `crypto.verify`; HKDF and PBKDF2 keys extractable when imported so; the host-floor
  limits as typed errors on every backend. Still to do on `main` now: the 2^20-byte PBKDF2
  output bound as `Err(CryptoError(Length(bits)))`, replacing native `D096` / JS `D090`.
- **7a-W built (held branch `hold/webapis-7a-wasm`, stacked on #5).** The five `crypto_*`
  builtins are `hosted` imports of an ABI module (`sakura_lir::image::WASM_HOSTED`, outside
  the `HOSTED` table native shares, since native has them as `rt_b_crypto_*`); the guest zeroes
  the argument tuple before freeing it and the Node host zeroes its copied slice, the decoded
  `Bytes` and its answer. A WASI command (the pure corpus on Wasmtime) has no JS host, so the
  seven fixtures stay in the corpus test's `KNOWN` for `Host::Wasi` only; `parity.rs`'s
  `KNOWN` is empty.

### Phase 7b: `std/crypto` tier 1b

- **Scope.** `AesGcm` encrypt/decrypt with caller nonce and AAD; `Ecdsa(P256)` sign/verify
  (RFC 6979 on `rt`, Web Crypto on JS); `Ecdh(P256)` and `X25519` `derive_bits`;
  `Ed25519` sign/verify; `KeyFormat::{Pkcs8, Spki, Jwk}` import/export for these.
- **Files.** As 7a plus `aes-gcm`, `p256`, `ed25519-dalek`, `x25519-dalek` in `rt`.
- **Tests.** `crypto_aes_gcm_vectors.sak` (NIST GCM), `crypto_aes_gcm_rejects_tag.sak`,
  `crypto_ed25519_rfc8032.sak` (canonical signature bytes compared), `crypto_x25519_rfc7748.sak`,
  `crypto_ecdsa_sign_verify_roundtrip.sak` (never prints the signature),
  `crypto_ecdh_shared_secret.sak`, `crypto_jwk_roundtrip.sak`, `crypto_spki_import_rejects_garbage.sak`.
- **Docs.** Stdlib regenerated; the capabilities tour's key section extended.
- **Who.** Opus; Sol: the vector tables.
- **Acceptance.** As 7a; the ECDSA fixture passes without a `.wasm.stdout`.
- **Landed (`d401d073`, `2a12eee4`; design §17.288).** JS and native; WASM `D090` under
  eight `KNOWN` rows until 7b-W. Ratified as built: ECDSA P-256 with SHA-256 only, `r || s`;
  `sign` keeps `Random` and the backends keep their nonces (RFC 6979 on native, OpenSSL's on
  JS; compared through `verify` only); a 12-byte nonce; `kind: KeyKind`, `KeyPair`,
  `generate_key_pair`, `EcdhWith`, `Format`/`Malformed`, `Nonce`/`Tag`/`ZeroSecret`; the
  private material layout std-private; imports read only the DER forms `export_key` writes.
  The new variants are 1.0.0-beta.1's. Two small commits follow in the web lane before 7b-W:
  `fix(rt)`, every `Vec` `take_bytes` and `rt::crypto` make zeroed before it is freed
  (§17.288 (7)); `docs(native)`, THIRD_PARTY.md's nine crypto rows (7a's four, 7b's four,
  `curve25519-dalek`) with unmodified notice files and a `sakura_host_native` test that every
  `native` dependency has a row (§17.288 (9)). The `import_key` reachability cost is accepted;
  7b-W's REQUESTS answer line records the WASM and JS bytes of `crypto_hmac_rfc4231` at
  `2a12eee4` against its parent (§17.288 (8)). The heap's secret hygiene is `m5-native.md`
  row N-S, for the 1.0 security review.
- **7b-W (now, ABI 1.9.0; §17.288 (10)).** As 7a-W: the seven new `crypto_*` builtins as
  `hosted` imports (`WASM_HOSTED`, `sakura_wasm_host.js`, `validate.rs`), ABI.md §0 and §8.2
  at 1.9.0, `sakura_abi_version` `0x010900` on both hosts and in `test_bench.py`, the guest's
  argument tuple wiped, the eight `KNOWN` rows emptied (`Host::Wasi`-only rows stay), the
  three "waits" sentences deleted; one commit, green on its own base. **Landed** (ABI 1.9.0;
  the Node host also zeroes `crypto_aes_gcm_open`'s `Some` plaintext once encoded).

### Phase 7c: tier 2 on JS and WASM-on-JS (before the `v1.0.0-beta.1` preparation commit)

- **Scope.** `RsaOaep`, `RsaPss`, `RsassaPkcs1`, `P384`, `P521`, `AesCbc`, `AesCtr`,
  `AesKw` through `crypto.subtle`; native answers `Err(CryptoError(Unsupported(name)))`.
- **Files.** `crypto.sak` variants; `sakura_runtime.js`; native `rt` returns the typed error.
- **Tests.** One vector fixture per algorithm with a `.native.stdout`-style declared
  difference only if the harness supports it; otherwise a `// needs: tier 2 on native`
  header and the fixture skipped on native with the reason printed by the harness.
- **Who.** Opus. **Acceptance.** Typed `Unsupported` on native is a fixture, not a gap.
- **Deadline (design §17.288 (6)).** Its variants (`Algorithm`, `Curve`, `CryptoReason`'s
  `Unsupported`) grow `pub` std sums, which after the beta's preparation commit is not an additive change (an
  exhaustive `match` stops compiling), so 7c is on `main` before the `v1.0.0-beta.1` preparation commit
  or tier 2 is not in 1.0; its variants are not declared ahead of its implementation.
- **Landed (ABI 1.11.0).** JS through synchronous `node:crypto` (`createCipheriv`, `sign`,
  `verify`, `privateDecrypt`, and `publicEncrypt` without padding under std's own EME-OAEP so
  the seed is the caller's nonce); WASM-on-JS through two new `hosted` imports,
  `crypto_cipher` and `crypto_tier2`; native's `crypto_tier2` is `false`, so std answers
  `KeyError(Unavailable(name))` from `import_key` and `CryptoError(Unsupported(name))` from
  `encrypt`/`decrypt`, and `rt` refuses a tier-2 code with `D090` should one reach it. The
  harness now honours `<name>.native.stdout` as the declared difference, so each of the nine
  vector fixtures runs on native and prints its typed errors. Vector tables in
  `compiler/tests/vectors/` (NIST CAVP, RFC 3686, 3394, 6979, 5903; Wycheproof for RSA).
  RSA keys are imported only. Readings and the open questions are in `compiler/REQUESTS.md`
  (2026-10-21).

### Phase 8: `std/websocket` client

- **Scope.** Effect `WebSocket` with `connect`, `send`, `receive`, `close`; `Options`
  (subprotocols, `max_message`, `max_frame`, headers), `Message = Text(String) |
  Binary(Bytes)`, `Received = Text | Binary | Closed(CloseInfo)`, `CloseInfo { code,
  reason, clean }`, `SocketError` reasons; `WebSocket.host()` under `net` (`ws` 80, `wss`
  443); `WebSocket.scripted(script)` with `Reply(expect, answer)`, `Push(message)`,
  `Close(code, reason)` steps; the startup refusal rule on native (RFC §7) implemented in
  the native driver for `Http`, `Serve`, `Llm` and `WebSocket` alike.
- **Files.** New `compiler/std/websocket.sak`; `sakura_core::builtins`; `sakura_runtime.js`
  (the global `WebSocket`, Node 22+; a bounded receive queue feeding the scheduler);
  `sakura_wasm_host.js` + `validate.rs` (`async` imports `ws_connect`, `ws_send`,
  `ws_receive`, `ws_close` + `sync` `ws_free`), `ABI.md` §8.2/§8.4/§8.5 (MINOR bump; the
  handle kind, cancellation on scope exit); `sakura_native_driver` startup check; the
  permissions check shared with `fetchArgs`; `CHANGELOG.md`.
- **Tests.** `websocket_scripted_echo.sak`, `websocket_scripted_close_info.sak`,
  `websocket_rejects_message_too_large.sak`, `websocket_denied_host.sak` (manifest fixture
  under `sakura run` with `net = []`, exit 75), `websocket_redirect_not_followed.sak` (no
  redirects on upgrade), `ui`/driver test: native startup refusal message; runtime tests
  with a local `ws` server (Node's `http` + the `ws`-less handshake? use Node 26's built-in
  server upgrade handling) for the live handler: text, binary, fragmented frames, ping/pong,
  close codes 1000/1009, cancellation on scope exit closes with 1001.
- **Docs.** Stdlib regenerated; `tour/capabilities.mdx` and `manifest.mdx` (`net` covers
  `ws`/`wss`); `cookbook/websocket_client.mdx`.
- **Who.** Opus.
- **Acceptance.** Scripted fixtures green on JS and WASM; the live runtime tests green on
  JS and through the WASM host; native refuses at startup with the message; the exit-75
  rule for `Http`/`Serve`/`Llm` on native replaces today's `D091` at first use and the
  native CLI parity count notes it.
- **Landed (`6b633a99`, ABI 1.8.0; readings ratified in design §17.286, which also adds the
  receive queue's byte bound and the pong rule, moves operation `requires` to the perform site
  and lists the owner's review items for the new `net` path).**
  `std/websocket` with `connect`/`send`/`receive`/`close`, `messages` (a view), `scripted` and
  `host()`; `Received = Data(Message) | Closed(CloseInfo)` (one module cannot declare `Text`
  twice); `connect` takes a `String` (the `Url` refinement admits only `http(s)`); `Options`
  has no `max_frame` (the host bounds every frame by `max_message` before holding it) and no
  secret headers yet. The runtime speaks RFC 6455 over `node:http`'s upgrade rather than the
  global `WebSocket`, which can neither pause reading nor bound a frame; a socket is an `Int`
  in the file table, closed by `file_closing`, so no handle kind or finalizer code was added.
  Evidence: five semantics fixtures and one UI fixture (`V6003`) on three backends,
  `runtime/js/test/websocket.test.js` (text, binary, fragmented, ping/pong, 1000, 1001 on
  scope exit, 1002, 1007, 1009, the 5 s close timeout, the 64-message queue holding the peer
  back over TCP, no redirect, `net`), `tests/wasm.rs::websocket_host_on_wasm_as_on_js` (one
  program against a live peer, same output and trace on JS and WASM), `cli.rs`'s exit 75 with
  `net = []` (`websocket_denied_host`) and the native driver's refusal. `websocket_redirect_not_followed`
  is a runtime test, since a semantics fixture has no live server to redirect.

### Phase 8s: `std/websocket` server (`WebSocket.accept`; RFC §2.10, design §17.289)

- **Scope.** The `WebSocket` effect gains `accept<e>(req: Request, options: Upgrade, body:
  fn(Socket) -> Unit ! e) -> Response ! {Fail<SocketError> | e}` (`requires
  options.max_message > 0`), `subscribe(sock, topic)`, `unsubscribe(sock, topic)` and
  `publish(topic, message, except: Option<Socket>) -> Int` (the topic at most 256 bytes by
  `requires`); `Upgrade = { subprotocol: Option<String>, origins: List<String>, headers:
  List<(String, String)>, max_message: Int }` with `WebSocket.upgrade()`; the pure
  `handshake(req) -> Result<Handshake, String>` with `Handshake = { key, subprotocols,
  origin }` (RFC 6455 §4.2.1 strictly; body `Empty`; `origin` lowercased); `SocketReason`
  gains `Accept(String)`; `upgrade_request(path, subprotocols) -> Request` for tests;
  `host()` and `scripted` implement the four new operations (`scripted` runs the same
  `handshake`, origin and subprotocol checks, plays `script` as the peer, and `publish` meets
  each subscribed scripted socket as a send). The accepted socket's `send`, `receive`,
  `close`, `messages` and `protocol` are the client's, unchanged. `accept` returns the 101
  `Response`; the server discards the handler's return for an upgraded connection.
- **Rules (the entry's numbers).** `origins` empty refuses every browser, no wildcard, a
  request without `origin` admitted (5); the subprotocol must be offered, `headers` filtered
  as the client's (6); no extension negotiated, server frames unmasked, an unmasked client
  frame 1002, otherwise the client's wire rules, no runtime ping (7); `SAKURA_MAX_SOCKETS`
  (default 1,024; a positive integer or a startup refusal) counting both roles, `accept` and
  `connect` refused at the limit with the same string; no idle timer (`Task.timeout` around
  `receive`); `setKeepAlive(true)` on every socket (8); `publish` never waits, skips a
  subscriber whose write buffer holds `max_message` bytes or more or whose `max_message` the
  message exceeds, returns the count, subscriptions end with the socket (9); a refused upgrade
  is the handler's `Response` written by the host on the raw connection with `content-length`
  and `connection: close`, the 503 and 405 paths as `serveHttp`'s, a defect after the 101
  reported on stderr and the socket closed 1001 (10).
- **Files.** `compiler/std/websocket.sak` (imports `std/request.{Empty, Request}` and
  `std/response.{Response, Status}`; the header's "client only" sentence rewritten);
  `compiler/std/serve.sak` (comments: upgrade requests reach `handle`; `WebSocket.accept`);
  `sakura_core::builtins`; `sakura_runtime.js` (the WebSocket section: a role flag on the
  table entry, `wsFrame(op, payload, masked)`, unmasking in `wsData`, `ws_accept` keyed by
  `ctx.t`, the topic map, `ws_subscribe`/`ws_unsubscribe`/`ws_publish`, the socket counter
  and `SAKURA_MAX_SOCKETS` read as `SAKURA_MAX_COPY_BYTES` is; `serveHttp`: the `upgrade`
  listener building the same `Request`, the raw connection held by task, the host-written
  refusal); `sakura_wasm_host.js` (`served` holds the raw connection; the four `hosted`
  imports, `ws_accept(task, ptr, len)` keyed by the served task as `serve_respond` is);
  `sakura_lir::image::WASM_HOSTED`, `validate.rs`; `ABI.md` §0 and §8.2's WebSocket
  paragraph at the MINOR; `sakura_abi_version` on both hosts and `test_bench.py`'s assertion
  (§17.270 (2)); `CHANGELOG.md`; `std/INDEX.md` regenerated. No dependency.
- **Tests.** Semantics: `websocket_accept_scripted_echo.sak` (`Serve.scripted([upgrade_request("/ws", [])])`
  around `WebSocket.scripted`, output `Get /ws -> 101 ` and the body's `Test.output`),
  `websocket_accept_rejects_handshake.sak` (method, `upgrade`, `connection`, key, version,
  body: one `Accept` string each), `websocket_accept_rejects_origin.sak`,
  `websocket_accept_rejects_subprotocol.sak`, `websocket_accept_rejects_max_message.sak`
  (`expect-defect: D020`, the perform span), `websocket_publish_scripted_fanout.sak` (two
  accepted sockets, `except`, the `Int`), `websocket_publish_rejects_topic_length.sak`
  (`D020`); UI: `V6003` for a socket escaping `accept`'s body; each with `// Spec:
  https://www.rfc-editor.org/rfc/rfc6455#section-…`. Runtime (`websocket.test.js`): a
  Sakura-served server with Node's global `WebSocket` and a raw client (`ws_peer.mjs`'s kind)
  as peers: text, binary, fragmented, ping/pong, an unmasked client frame 1002, 1009, 1007,
  close 1000 both directions, 1001 on `body`'s return, the refusal path (`Upgrade: h2c` gets
  the handler's response with `connection: close`; a plain `GET` on the route gets its
  response), the 426 with `sec-websocket-version: 13`, an origin refused and one admitted,
  the subprotocol echoed, an extension offer not echoed, `SAKURA_MAX_SOCKETS=1` → the second
  accept `Accept(…)` → 503, two subscribed clients with `except`, the backpressure skip (the
  predicate on `writableLength` unit-tested if the TCP buffer cannot be filled
  deterministically). `tests/wasm.rs::websocket_accept_on_wasm_as_on_js`: one served
  program, same output and trace on JS and WASM. `cli.rs`: `net = []` exit 75 unchanged.
- **Docs (phase 12).** Stdlib regenerated; `cookbook/websocket_server.mdx` (Opus: the route,
  `handshake` then `accept`, `origins`, the 426/403/503 answers, a `Task.timeout` idle);
  `manifest.mdx` one sentence (`net` covers the listener; `accept` adds no entry); the RFC's
  gap sentence for `Bun.serve` rewritten as delivered (Sol).
- **Who.** Opus, one commit, one MINOR. **Acceptance.** Every fixture above green on three
  backends (native: the startup refusal unchanged); the runtime tests green on JS and through
  the WASM host; the owner's review of the served-upgrade path recorded as a dated line under
  the REQUESTS entry (§17.289's list (a)–(j)); no new diagnostic code; no dependency.

### Phase 9: `std/urlpattern`

- **Scope.** `UrlPattern.compile`, `exec`, `UrlMatch`, `PatternError`; `Router.route`
  patterns compiled through it (behaviour-preserving for `:name` segments; `*` and `**`
  become available to routes).
- **Files.** New `compiler/std/urlpattern.sak`; `router.sak`; `INDEX.md` graph.
- **Tests.** `urlpattern_named_groups.sak`, `urlpattern_wildcards.sak`,
  `urlpattern_rejects_regexp_group.sak`, `urlpattern_rejects_limits.sak`,
  `router_matches_through_urlpattern.sak` (the existing router fixtures unchanged).
- **Docs.** Stdlib regenerated; `cookbook/router_endpoint.mdx` one wildcard example.
- **Who.** Opus. **Acceptance.** Existing router fixtures byte-identical; new ones green.

### Phase 10: `std/channel`

- **Scope.** `Channel.with`, `Sender`, `Receiver`, `send`, `try_send`, `recv`, `close`,
  `SendError = Full | Closed`; external scoped types registered like `Scope`/`Fiber`
  (`V6001`/`V6002`/`V6003` apply). Then, in `std/stream` over it (§17.280 (4)):
  `buffer(src, capacity, body)` (a producer fiber fills a channel and waits when full; the
  body's source is a view over the `Receiver`) and `tee(src, max_lag, body)` (two channels
  of `max_lag`); fixtures `stream_buffer_backpressure.sak` (effect trace order under
  `Task.deterministic`) and `stream_tee_lag_bound.sak` join this phase.
- **Files.** New `compiler/std/channel.sak`; `sakura_types` external-type list if it is a
  list; `sakura_runtime.js` scheduler (wait queues); `runtime/rt` scheduler; `ABI.md`
  (MINOR bump); design §8.6 sentence in phase 12.
- **Tests.** `channel_bounded_backpressure.sak` (effect trace order under
  `Task.deterministic`), `channel_close_drains_then_none.sak`, `channel_try_send_full.sak`,
  `channel_sender_cannot_escape.sak` (UI, `V6003`), `channel_cancel_wakes_waiters.sak`.
- **Docs.** Stdlib regenerated; `tour/concurrency.mdx` one example.
- **Who.** Opus. **Acceptance.** Green on three backends with identical traces.
- **Landed** (ABI 1.12.0). `std/channel`: `with_channel(capacity, body)` (`requires capacity >
  0`), `Sender`/`Receiver` as `@resource` external types, `send`, `try_send`, `recv`, `close`,
  `SendError = Full | Closed`, every function with `Task` in its row; the channel is closed and
  emptied by a finalizer on every exit of the callback. `Source.buffer` and `Source.tee` run the
  producer and the body under `Task.all`, the body closing the channels when it returns. The
  channel lives in `sakura_runtime.js` and in `rt` (`channel.rs`, tag 19, shared by WASM and
  native), so native runs it as JS does. A run in which every task waits is `D090` on all
  three (`runMain` gained the `beforeExit` check), and the JS scheduler now delivers a resumed
  fiber's end at once, as `rt` does, so a channel wake and a fiber's end order alike. Fixtures
  as listed plus `channel_rejects_capacity` (`D020`), `channel_recv_every_task_waiting`
  (`D090`), `stream_tee_rejects_max_lag` (`D020`), `stream_buffer_propagates_failure` and the
  UI case `channel_receiver_cannot_be_handler_state`; readings beyond the RFC (the name, the
  rows, a `buffer` body's view not reached by `V6003`) are the REQUESTS entry.

### Phase 11: `std/worker` (experimental)

- **Scope.** `@worker` attribute on a top-level `fn(Inbox<I>, Outbox<O>) -> Unit ! {Worker
  | e}`; `Worker.spawn`, `send`, `recv`, `Received`, `WorkerHandle`, `Grants` (`none`,
  `of(table)`, subset check), `WorkerError` (`GrantExceeded`, `Limit`, `Defect(code)`,
  `Codec`), `Worker.topic`/`publish`/`subscribe`, `Worker.host()`, `Worker.inline()`.
  Marked experimental in the module header, the index, the stdlib page and STABILITY.
- **Files.** `sakura_syntax`/`sakura_hir` (the attribute; `V6001`-class check that the
  entry's row is `Worker`-only plus what the grants allow is **not** static: grants are
  runtime data, so the check is the ordinary effect check), `sakura_core` (secondary entry
  points), `sakura_codegen_js` (export each `@worker` entry; `worker_threads` bootstrap in
  `sakura_runtime.js` with `SAKURA_PERMISSIONS` = the grant subset), `sakura_codegen_wasm`
  + `sakura_wasm_host.js` (a second instance in a Node `Worker`, messages as `sakura-vt1`
  bytes), `runtime/rt` + `sakura_host_native` (a second `Instance` on an OS thread, an
  `mpsc` pair, the `Schema` codec at the boundary), `ABI.md` (MINOR bump; the worker entry
  export and message imports), new `compiler/std/worker.sak`, `CHANGELOG.md`, STABILITY
  inventory row (`experimental`, `—`).
- **Tests.** `worker_inline_roundtrip.sak` (deterministic), `worker_grants_subset_rejected.sak`,
  `worker_none_grants_cannot_fetch.sak` (the worker's `Http` fails closed at its startup),
  `worker_defect_reported_as_failed.sak`, `worker_limit_16.sak`, `worker_topic_fanout.sak`,
  `worker_secret_not_schema.sak` (UI: a `Secret` message type is `V2032`), driver tests for
  the live handler on JS and WASM-on-Node, native behind `--features native`.
- **Docs.** Stdlib regenerated with the experimental label; `tour/concurrency.mdx` "Workers
  (experimental)"; `stability.mdx` row.
- **Who.** Opus (two sessions are acceptable: 11a JS + inline + std; 11b WASM + native).
- **Acceptance.** Inline fixtures green on three backends; live JS and WASM green; native
  green or refusing at startup with the message; the experimental label present in every
  place phase 12 lists.
- **11a landed** (no ABI change). `std/worker`: `Port<S, R>` with `WorkerHandle`, `Inbox` and
  `Outbox` as its aliases, `spawn -> Result<R, WorkerError>` sharing the entry's row, `send`,
  `recv`, `Grants` over `Permissions`, `WorkerReason = GrantExceeded | Limit | Defect(String) |
  Codec`, topics, `inline()` in Sakura over `std/channel` (every target), `host()` on
  `node:worker_threads` (JS); `@worker` with `V1010` at a `spawn` of anything else, and Core's
  `$worker$<name>` entries registered by `__main__.js`. WASM and native refuse a `main` that
  declares `Worker` at startup (exit 75, "`Worker` is not available on target `…`"), and
  topics are runtime builtins on JS only (`D090` elsewhere, a `KNOWN` row) until 11b. Fixtures
  as listed but `worker_none_grants_cannot_fetch` and `worker_defect_reported_as_failed`,
  which are `crates/sakura_cli/tests/workers.rs` (live threads), plus `worker_rejects_codec`,
  `worker_host_refused_off_js` and `V1010_worker_spawn_untagged`; readings beyond the RFC (the
  shared port type, the shared row, the empty inline table, topics' counting, no `close`) are
  the REQUESTS entry.
- **11b landed** (ABI 1.15.0, after MT-3's 1.14.0; design §17.299). WASM: `Worker.host()` is
  a second instance of the module in a Node `Worker` thread over the JS runtime's links,
  messages as the `sakura-vt1` bytes of their `String`, the entry found through the module's
  `workers()` table by the code the function value holds. Native: a second `Instance` on an
  OS thread (the driver's launcher, descriptor layout 5 with the same table), a bounded
  `mpsc` pair of codec text per link, grants as the worker host's permissions, the parent's
  `Limits`. Topics are `rt` builtins on every target (a registry of `Channel`s, the host
  imports `topic_listen`/`topic_post` and the export `topic_deliver`); the four
  `worker_topic_fanout` rows are gone. `Worker.close(p)` (additive) ends one direction. A
  thread's environment is the `SAKURA_*` variables and its `env` grant's names; its heap is
  bounded as its parent's (`resourceLimits` on Node). `WorkerEntry.key` names; the list
  dispatch stays. RFC §2.9's fragment, `spawn -> Result` and the fixed 64 per direction are
  corrected. Evidence: `crates/sakura_cli/tests/workers.rs` on JS, WASM and native (a grant
  test per backend), `worker_close_finishes_inbox`, `worker_host_root_installed` (was
  `worker_host_refused_off_js`). Readings and the beta.N items left (the inline counter, the
  `@worker` shape check) are the REQUESTS entry.

### Phase 12: docs, skill, inventory, prompt budget

- **Scope.** Per phase: the stdlib page regenerated, the skill reference regenerated by
  `agent_docs`, `docs/llms.txt` one line per module, the gap rows of RFC §1 written into
  the stdlib page's "Current scope and limitations" (one sentence each: the web name, the
  Sakura form, the reason), the STABILITY inventory row for the module, the changelog
  entry, and the design sections RFC §8 names (one sentence each, by Fable in the next
  round or by Opus quoting §17.262). Final pass before the rc: `sakura prompt --budget`
  re-measured and recorded in `docs/plans/agent-dx.md`'s table; `research/15` cross-links.
- **Who.** Sol for the mechanical items (index lines, inventory rows, llms.txt lines,
  changelog lists, gap sentences from the RFC table verbatim); Opus for any page that
  carries a code fence (the sweep compiles them).
- **Acceptance.** `check-doc-sweep.py`, `agent_docs --check` and the website gates green;
  no page names a milestone (§17.256); every module of the RFC table has an inventory row.

### Row R: land the research and the vector tables

- **Scope.** `research/15-web-apis.md` is landed with this plan (done in the same commit as
  the RFC). Sol adds `compiler/tests/vectors/` tables as each phase above names them, with
  a header line per file giving the source URL and the date fetched; no vector is
  hand-edited.
- **Who.** Sol. **Acceptance.** Each table is referenced by at least one fixture.

## 4. Decisions implementers may need, already made

- Externals vs Sakura: hot codecs are externals in both runtimes; everything else is
  Sakura source (RFC §2, plan §1). Do not add a `hosted` import for something `rt` can do.
- No new manifest key, no new diagnostic code is expected through phase 10. A phase that
  needs one files a `design-question` in `compiler/REQUESTS.md` before using it.
- Breaking changes are phases 4a and 4b only. They do not touch `main` before the
  `v0.1.0` tag (held as PR branches, like PR #4) and must be on `main` before the `v1.0.0-beta.1`
  preparation commit; if that window closes first, they wait for the owner's decision, they
  do not land after the tag. Additive phases never wait for a tag (design §17.263 (2)).
- `.wasm.stdout` is a declared difference, never a convenience.
- Nothing prints compressed bytes or signatures; nothing shows, logs or records a key.
- One held PR per ABI revision, stacked in merge order (#5 at 1.5.0, 7a-W at 1.6.0); a
  MINOR number is never shared between two PRs (§17.266 (1)).
- A limit is a typed error on every backend, checked in Sakura before the external; a
  defect whose code differs by backend is a parity bug (§17.266 (7)).
- A runtime fix that brings a backend to its own written rule, changes no contract text
  and carries a three-backend fixture is a `fix` commit on `main`, even while the tag is
  pending; a held PR never carries one (§17.267 (2)).
- A fixture generated from a vector table is committed like any fixture, names its table
  and its generating test in its header, and that test is the staleness gate (§17.268 (5)).
- A setter the standard would silently ignore returns `Err` with a typed reason; a setter
  that applies to every value returns the value (§17.268 (3)).
- A served request's connection is the task's: the host finds it by the performing task
  (`ctx.t` on JS, the served task on WASM), never by a field of `Request` (§17.289 (10)).

## 5. Open items for Fable

Queue them in `compiler/REQUESTS.md` as `design-question` entries, as before. Closed:
`Console.captured` scripted input (phase 4b, `Console.scripted`, §17.267); `Router` does not
expose `UrlMatch`, `Request.params` is the surface (§17.281 (3)); where a stream's cursor
lives (§17.280: nowhere, a stream is a value); phase 5's six readings (§17.284); phase 7b's
seven readings and the four items beside them (§17.288); the WebSocket server's shape, lane
and number (§17.289). Phase 9's follow-ups: the `route` doc-comment
line with the next `router.sak` change (§17.281 (2)) and two gap rows in phase 12
(§17.281 (4)).
