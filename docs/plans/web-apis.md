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
- Versions (design §17.263): an additive phase ships in whichever release is next; phases
  0 and 1 are in `v0.1.0`, and 2, 3, 7a and R land on `main` before the tag if their gates
  are green first. Phases 4a and 4b (the breaking set, and any ABI version bump) are held
  as PR branches, rebased daily, merged right after the `v0.1.0` tag and on `main` before
  the `v0.2.0` preparation commit; phases 5–10 are additive and land before
  `v1.0.0-rc.1`; phase 11 is experimental; phase 12 closes each phase's docs and the
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
| 2 | `std/url` parser, `std/searchparams` | next release | Opus | 1 |
| 3 | `std/headers`, `std/blob`, `std/form` | next release | Opus | 2 |
| 4a | HTTP 0.2.0 shape: `Payload`, `Method`, `redirect`, `Response.bytes`; the redirect loop moves into `std/http` | 0.2.0; PR branch until the `v0.1.0` tag | Opus | 3, tag |
| 4b | Effect additions: `Clock.monotonic_micros`, `Task.cancel`/`cancelled`, `Task.every`, console dialogs, `Json.render` | 0.2.0; PR branch until the `v0.1.0` tag | Opus | 0, tag |
| 5 | `std/stream` and the `Http`/`Fs` adapters | pre-rc | Opus | 4a |
| 6 | `std/compress` | pre-rc | Opus | 5 |
| 7a | `std/crypto` tier 1a: `CryptoKey`, digests, HMAC, HKDF, PBKDF2, import/export | next release | Opus | 1 |
| 7b | `std/crypto` tier 1b: AES-GCM, ECDSA/ECDH P-256, Ed25519, X25519, JWK | pre-rc | Opus | 7a |
| 7c | `std/crypto` tier 2 on JS and WASM-on-JS (RSA, P-384/521, AES-CBC/CTR/KW) | pre-rc, optional | Opus | 7b |
| 8 | `std/websocket` client | pre-rc | Opus | 4a, 5 |
| 9 | `std/urlpattern` and `Router` on it | pre-rc | Opus | 2 |
| 10 | `std/channel` | pre-rc | Opus | 4b |
| 11 | `std/worker` (experimental) | first green minor | Opus | 10 |
| 12 | Docs, skill, inventory, prompt budget | with each phase; final pass pre-rc | Sol (mechanical), Opus (pages with code) | each phase |
| R | Land research/15 and the fixture-vector tables | now | Sol | — |

"After" names what must be on `main`. 4b and 7a can run beside 1–3 in separate worktrees.

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
- **Tests.** Fixture `url_parse_whatwg.sak` driven by a curated 150-case subset of WHATWG
  `urltestdata.json` checked in as `compiler/tests/vectors/url.jsonl` (input, base,
  expected href or failure), read with `Test.fixtures()`; `url_resolve_relative.sak`;
  `url_rejects_idna.sak`; `url_credentials_are_secret.sak` (the password never shows);
  `searchparams_order_and_repeats.sak`; `searchparams_plus_and_percent.sak`;
  `searchparams_sort_stable.sak`. Std `test` blocks for each `with_*`.
- **Docs.** Stdlib regenerated; `cookbook/http_retry.mdx` or a new
  `cookbook/url_build.mdx` showing `Url.parse` → `with_search` → `to_url`.
- **Who.** Opus; Sol: the `url.jsonl` subset with the upstream commit hash in its header.
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

### Phase 4a: HTTP 0.2.0 shape

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

### Phase 5: `std/stream`

- **Scope.** `Source`, `Sink`, `Limit`, and the combinators of RFC §2.5; `Http.body_source`,
  `Fs.file_source`, `Fs.file_sink`. Pure Sakura except nothing: `buffer` and `tee` use
  `Task` and handler state.
- **Files.** New `compiler/std/stream.sak`; `http.sak`, `fs.sak` adapters; `INDEX.md` graph
  (`stream → list, bytes, string, task`); design §8.4 one sentence (closure records as
  cursors) in phase 12.
- **Tests.** `stream_pull_end_once.sak` (`None` once; an empty chunk is data),
  `stream_map_filter_take_fold.sak`, `stream_lines_split_across_chunks.sak`,
  `stream_chunks_bytes.sak`, `stream_buffer_items_backpressure.sak` (under
  `Task.deterministic`: the producer waits when full; effect trace order),
  `stream_buffer_bytes.sak`, `stream_tee_lag_bound.sak`, `stream_pipe_to_file_sink.sak`
  (`Fs.memory`), `http_body_source_scripted.sak`, `stream_to_list_limit_rejects.sak`.
- **Docs.** Stdlib regenerated; `cookbook/stream_lines.mdx`; `tour/memory.mdx` one
  paragraph on cursors and copies.
- **Who.** Opus.
- **Acceptance.** Fixtures green on three backends with identical effect traces; no
  combinator allocates a queue without a bound (reviewed by reading the module).

### Phase 6: `std/compress`

- **Scope.** RFC §2.6: `Format`, `compress`, `decompress`, `compress_source`,
  `decompress_source`, `CompressionError`.
- **Files.** New `compiler/std/compress.sak`; `sakura_core::builtins`; `sakura_runtime.js`
  (`node:zlib` sync for whole-buffer, streaming via the zlib stream classes behind the
  scheduler); `runtime/rt` (`miniz_oxide` with `no_std`; a streaming state handle in the
  guest heap, no host import); `compiler/Cargo.toml`/`Cargo.lock` (workspace dependency,
  latest stable, exact pin as the native crates do); native: nothing beyond `rt`.
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

### Phase 7a: `std/crypto` tier 1a

- **Scope.** `CryptoKey` (opaque, `Secret<Bytes>` material), `Algorithm`, `Usage`,
  `KeyFormat`, `Digest`, `digest`, `hmac`, `derive_bits` for `Hkdf` and `Pbkdf2`,
  `import_key`/`export_key` for `Raw` (HMAC, HKDF, PBKDF2 keys), `generate_key` for HMAC,
  `KeyError`, `CryptoError`. `verify` for HMAC (constant-time compare).
- **Files.** New `compiler/std/crypto.sak`; `sakura_core::builtins`; `sakura_runtime.js`
  (`crypto.subtle` with a per-instance import cache keyed by material hash);
  `sakura_wasm_host.js` + `validate.rs` `hosted` imports (`crypto_*`), `ABI.md` §8.2
  (MINOR bump; the import list); `runtime/rt`: RustCrypto `sha2`, `hmac`, `hkdf`,
  `pbkdf2` (`no_std`, exact pins) so native has them now and a non-JS WASM embedder can
  switch to the guest implementation later (the plan records that switch as a one-line
  `validate.rs` change plus a corpus run); `Cargo.toml`/`Cargo.lock`.
- **Tests.** `crypto_sha2_cavp.sak` (NIST short-message vectors, Sol's table),
  `crypto_hmac_rfc4231.sak`, `crypto_hkdf_rfc5869.sak`, `crypto_pbkdf2_rfc6070.sak`,
  `crypto_key_never_shows.sak` (a `CryptoKey` in `show` is `V2031`, a UI fixture),
  `crypto_export_rejects_not_extractable.sak`, `crypto_usage_rejected.sak`,
  `crypto_generate_under_seeded_random_is_deterministic.sak` (and the docs sentence that
  this is why `Random.seeded` is test-only).
- **Docs.** Stdlib regenerated; `tour/capabilities.mdx` section "Secrets and keys";
  `cookbook/sign_webhook.mdx` (HMAC verify of an incoming request).
- **Who.** Opus; Sol: `compiler/tests/vectors/sha2.txt`, `hmac.txt`, `hkdf.txt`,
  `pbkdf2.txt` from the RFCs/CAVP with source URLs.
- **Acceptance.** Every vector passes on three backends (WASM through the host, native
  through `rt`); the UI fixture pins the no-`Show` rule; `cargo deny`-style licence check
  if the workspace has one, else the licences listed in the PR.

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

### Phase 7c: tier 2 on JS and WASM-on-JS (optional before the rc)

- **Scope.** `RsaOaep`, `RsaPss`, `RsassaPkcs1`, `P384`, `P521`, `AesCbc`, `AesCtr`,
  `AesKw` through `crypto.subtle`; native answers `Err(CryptoError(Unsupported(name)))`.
- **Files.** `crypto.sak` variants; `sakura_runtime.js`; native `rt` returns the typed error.
- **Tests.** One vector fixture per algorithm with a `.native.stdout`-style declared
  difference only if the harness supports it; otherwise a `// needs: tier 2 on native`
  header and the fixture skipped on native with the reason printed by the harness.
- **Who.** Opus. **Acceptance.** Typed `Unsupported` on native is a fixture, not a gap.

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
  (`V6001`/`V6002`/`V6003` apply).
- **Files.** New `compiler/std/channel.sak`; `sakura_types` external-type list if it is a
  list; `sakura_runtime.js` scheduler (wait queues); `runtime/rt` scheduler; `ABI.md`
  (MINOR bump); design §8.6 sentence in phase 12.
- **Tests.** `channel_bounded_backpressure.sak` (effect trace order under
  `Task.deterministic`), `channel_close_drains_then_none.sak`, `channel_try_send_full.sak`,
  `channel_sender_cannot_escape.sak` (UI, `V6003`), `channel_cancel_wakes_waiters.sak`.
- **Docs.** Stdlib regenerated; `tour/concurrency.mdx` one example.
- **Who.** Opus. **Acceptance.** Green on three backends with identical traces.

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
  `v0.1.0` tag (held as PR branches, like PR #4) and must be on `main` before the 0.2.0
  preparation commit; if that window closes first, they wait for the owner's decision, they
  do not land after the tag. Additive phases never wait for a tag (design §17.263 (2)).
- `.wasm.stdout` is a declared difference, never a convenience.
- Nothing prints compressed bytes or signatures; nothing shows, logs or records a key.

## 5. Open items for Fable

Queue them in `compiler/REQUESTS.md` as `design-question` entries, as before. Known ones:
`Console.captured` scripted input (phase 4b decides it; no question needed); whether
`Router` should expose `UrlMatch` (phase 9 keeps `Request.params` as the surface).
