# Sakura Lang: Bun Web APIs coverage and implementation research

Research date: 2026-10-19. Repository inspected: `/Users/aniruddhdubge/Desktop/ani/sakura-lang`, HEAD `ba24a6ca4cb51f25d74d42e8ca7ddd7eb03b0f88`. Read-only repository research; this report is outside the repository. AI-assisted research by Codex (gpt-6-astra); proposals below require human review and are not accepted language design.

## Summary and exact page inventory

The [Bun Web APIs page](https://bun.com/docs/runtime/web-apis) lists **49 explicitly named entries**, in the groups below, plus an unspecified set of associated stream classes. This inventory excludes site navigation. The page describes partial or complete support, not a conformance guarantee. Identifiers are enumerated below; descriptions and assessments are original analysis.

| Page group | Exact named entries | Sakura coverage today | Remaining cross-backend size |
|---|---|---|---|
| HTTP | `fetch`, `Response`, `Request`, `Headers`, `AbortController`, `AbortSignal` | Typed HTTP client, request/response records, header lists, scoped cancellation; incomplete web semantics; native HTTP unported | L |
| URLs | `URL`, `URLSearchParams` | Lightweight `Url` refinement and raw query lookup | M |
| Web Workers | `Worker`, `self.postMessage`, `structuredClone`, `MessagePort`, `MessageChannel`, `BroadcastChannel` | Scoped fibers and value semantics are related, but no workers or channels | L |
| Streams | `ReadableStream`, `WritableStream`, `TransformStream`, `ByteLengthQueuingStrategy`, `CountQueuingStrategy` | HTTP/file chunk operations; no general stream abstraction | L |
| Blob | `Blob` | Immutable `Bytes` is a building block | M |
| WebSockets | `WebSocket` | None | L |
| Encoding and decoding | `Uint8Array`, `Uint8Array.prototype.toBase64()`, `Uint8Array.fromBase64()`, `TextEncoder`, `TextDecoder`, `atob`, `btoa` | Immutable bytes and UTF-8 conversion; no Base64 or general decoder | S–M |
| JSON | `JSON` | Typed `Json` parse/decode/encode | S for adapting current functionality |
| Timeouts | `setTimeout`, `clearTimeout` | `Clock.sleep`, task scope cancellation and `timeout` | S–M |
| Intervals | `setInterval`, `clearInterval` | Compose clock/task operations; no interval API | M |
| Crypto | `crypto`, `SubtleCrypto`, `CryptoKey` | `Random.bytes`, `Random.uuid`; no crypto module/key type | L |
| Debugging | `console`, `performance` | `Console`, `Log`, millisecond `Clock.monotonic` | S–M |
| Microtasks | `queueMicrotask` | Scheduler internal; `Task.yield_now` has different semantics | M |
| Errors | `reportError` | Structured defects and logging; no web error event reporting | M |
| User interaction | `alert`, `confirm`, `prompt` | `Console.print/read_line` building blocks | S |
| Realms | `ShadowRealm` | None | L; JS-engine interop only recommended |
| Events | `EventTarget`, `Event`, `ErrorEvent`, `CloseEvent`, `MessageEvent` | No web event types or listener API | S for records, L for event dispatch |

**Not explicitly listed in that table:** `File`, `FormData`, `URLPattern`, `TextEncoderStream`, `TextDecoderStream`, `CompressionStream`, `DecompressionStream`, `CustomEvent`, `DOMException`. These nine are covered separately below because the brief names them and several are necessary dependencies. Their omission is not evidence that Bun lacks them. The page also mentions the DOM and History APIs as examples outside its server-runtime focus; those are exclusions, not supported inventory entries. `crypto.getRandomValues` and `crypto.randomUUID` are assessed as members of the listed `crypto` object. Eight stream reader/writer/controller classes are expanded explicitly below as the natural scope of the page's associated-classes clause; this is our interpretation, not an additional enumerated Bun list.

`Uint8Array`, its Base64 methods, and `JSON` are ECMAScript facilities. `ShadowRealm` is a TC39 proposal. They do not acquire WHATWG/W3C specifications by appearing on this page.

**Recommendation:** pursue portable functionality through typed std modules and explicit handlers, with an optional web interoperability layer. Literal JS object compatibility and Sakura-native value semantics are distinct deliverables. Host delegation saves protocol implementation work; it does not solve cancellation, authority, lifetime, error, or scheduling integration.

Sizes estimate remaining engineering for the proposed portable shape, including adapters and focused conformance tests: **S** = a small wrapper/value utility using existing infrastructure; **M** = a new data model or bounded state machine; **L** = runtime/ABI/security work or a large protocol surface. Sizes are not elapsed-time commitments and do not add linearly: streams, scheduling, codecs and resource handles are shared prerequisites. Exact JS compatibility can be much larger than an idiomatic equivalent.

## Evidence baseline: what Sakura actually provides

Repository paths below are relative to the repository root. The principal sources read were `AGENTS.md`, `compiler/AGENTS.md`, `CONTRIBUTING.md`, `STABILITY.md`, `docs/plans/handoff-2026-10-19.md`, `docs/design.md` §§5–6, 8, 10, 13 and relevant decision notes, `compiler/std/INDEX.md`, the relevant `compiler/std/*.sak` files, `website/content/docs/tour/capabilities.mdx`, and runtime implementations.

### Effects, permissions, values and failures

- Empty effect rows are pure by default. Expected failures use nominal error tags in `Fail<E>`; `try { ... }` materializes `Result<T,E>`. A new wrapper should map expected host exceptions/rejections into stable typed errors. It must preserve uncaught Sakura defects as defects, rather than turning all faults into ordinary errors. See `docs/design.md` §§5.1–5.4 and `compiler/std/result.sak`.
- Effects describe operations; installed handlers hold authority. Existing root permission keys are `fs.read`, `fs.write`, `net`, `env`; `process = []` is reserved and grants nothing. There are no current `crypto`, `worker`, `broadcast`, `timer`, `console` or `stream` permission keys, and no separate `Net` effect. Proposed effects/permissions below are explicitly new design work, not existing facilities.
- The absence of a permissions table currently means no host limits; direct execution without `SAKURA_PERMISSIONS` does not establish the CLI ceiling. An explicitly empty grant is different. Tests install empty grants and deterministic/fake handlers. Do not describe present Sakura as universally deny-by-default or an OS sandbox. `website/content/docs/tour/capabilities.mdx` and `docs/design.md` §6.2 document this distinction.
- `Http.scoped(hosts)` attenuates an outer handler by data. `Fs.rooted(dir)` uses a real directory capability. Imports confer no authority. A future worker must receive an explicitly bounded authority configuration; copying a JS object containing a host handle is not authority delegation.
- Resources are scope-bound, with finalization on normal return, failure, defect and cancellation; no general static affine-use-count checker exists today. `V6002`/`V6003` prevent capability/resource escape. `docs/design.md` §§6.3, 8.4 and `compiler/std/fs.sak`, `compiler/std/http.sak` are the patterns for streams, sockets, readers and key handles.
- Values have no observable aliases, closures capture by value, and safe owning heaps are acyclic. `Graph` uses node IDs for explicit cycles. Handler state is confined and serialized. Every current backend executes one Sakura instance on one thread; native host services may internally use helper threads. Channels, detached tasks and shared mutable memory are not current language facilities (`docs/design.md` §§8.1–8.7).
- There is **no `Async` effect**. Suspension is a handler implementation detail. New streaming, crypto and task APIs should name their actual effects, not invent `Async` merely because JS returns promises.
- `Secret<T>` must remain non-serializable/non-loggable; new keys or messages must not bypass it. `Untrusted<T>` is the existing external-data boundary. `Crypto.hmac` appears in design discussion, but there is **no `compiler/std/crypto.sak`**: it is not implemented coverage.

### Backend evidence and common implementation rules

**JS today:** `compiler/runtime/js/sakura_runtime.js` implements `http_send`, `http_stream_open/read/closing`, `random_bytes`, `random_uuid`, `clock_*`, `console_*`, `log_*`. It uses host `fetch`, `Headers`, `AbortController`, `TextEncoder/Decoder`, `crypto`, and timers internally. Internal use does not make the corresponding JS classes public Sakura types. The default documented execution host is Node 26, not Bun-only.

**WASM today:** `compiler/runtime/js/sakura_wasm_host.js` forwards HTTP imports to that same JS runtime. `compiler/runtime/rt/ABI.md` defines versioned imports, copied data, asynchronous request completion and scoped handles. WASM itself supplies none of these web APIs. A browser/Node embedding can import them; a non-JS embedder must implement equivalent services. Pure utilities can instead execute inside the guest. Any new host handles/imports must follow ABI versioning, allocation limits, generation checks, cancellation and teardown; do not retain raw views into growable guest memory across suspension.

**Native today:** `compiler/crates/sakura_host_native/src/lib.rs` supplies Console, Clock, Random, Fs, Env and other services using a current-thread Tokio executor. Its opening comment explicitly says Http/Serve/Llm still answer `D091` through host defaults. `compiler/crates/sakura_host_native/Cargo.toml` declares reqwest/rustls/hyper dependencies, but declarations are not a working HTTP port. Random currently reads `/dev/urandom`; UUID generation exists. Pure operations run in `compiler/runtime/rt/src/builtins.rs`, `json.rs` and related runtime files. The native target remains feature-gated and experimental in `STABILITY.md`.

**Proposed common rule:** JS entries below mean trusted std adapters that copy/normalize values and preserve handler context, not exposing arbitrary host objects. WASM entries mean validated imports or pure guest implementations. Native entries are implementation candidates, not claims that crates provide full web conformance. Do not implement web listener callbacks by passing arbitrary effectful Sakura closures directly into synchronous JS FFI; dispatch through the scheduler with the performer's handler environment.

## 1. HTTP and cancellation

Semantics references: [WHATWG Fetch](https://fetch.spec.whatwg.org/#fetch-api) and [WHATWG DOM aborting](https://dom.spec.whatwg.org/#aborting-ongoing-activities). Parsing/building in-memory requests is distinct from sending them.

### `fetch` — L

- **Existing:** `compiler/std/http.sak`: `Http.send`, `Http.get`, `Http.post_json`, `Http.stream`, `Http.read_chunk`, `host`, `scripted`, `recording`, `scoped`. JS/WASM transport exists; native transport does not.
- **Semantics:** fetch resolves HTTP error statuses; network/abort failures reject. Redirects, body consumption, request mode/credentials/cache and streaming are part of its contract. [Fetch](https://fetch.spec.whatwg.org/#fetch-method).
- **Backends:** JS extend current fetch adapter; WASM extend current HTTP imports and body handles; native implement reqwest/hyper plus TLS, DNS, redirect policy, bounded streaming and cancellation. An HTTP client crate is not a Fetch implementation.
- **Effects/shape:** retain `Http` plus `Fail<HttpError>`, manifest `net`, attenuating handlers, typed request/response values and scoped bodies. Keep response status a value; offer explicit status validation. `Http.get` currently additionally maps 429 into `RateLimited`; `Http.send` preserves all statuses. Scripted transports and bounded recordings remain the deterministic route. Size includes native integration and policy parity.

### `Request` — M

- **Existing:** `compiler/std/request.sak`: `Request`, `Method`, `of`, `post`, `post_json`, `json`, `param`, `header`. Current method variants are Get/Head/Post/Put/Patch/Delete; body is `String`; headers and secret headers are separate lists.
- **Semantics:** Request carries metadata and a potentially single-use body; cloning interacts with stream state. [Fetch Request](https://fetch.spec.whatwg.org/#request-class).
- **Backends:** JS translate typed values into host Request/init; WASM encode the value and body handle; native typed Rust request plus streaming body source.
- **Effects/shape:** pure validated construction; body reads use the source's effects. Add typed method/body/options variants with `Result<...,RequestError>` validation, preserving `secret_headers`. Avoid mutable request aliases and implicit body cloning. Expand method coverage deliberately rather than claiming the six existing variants are complete.

### `Response` — M

- **Existing:** `compiler/std/response.sak`: `Response`, `Status`, `status`, `ok_json`, `html`, `json`, `text`; `compiler/std/http.sak`: `Incoming` and resource `Body`. Current buffered response body is text and `Status` permits 100–599.
- **Semantics:** response types include network-error/opaque cases with status zero; body reads are stateful. Constructor and null-body status rules differ from arbitrary numeric status records. [Fetch Response](https://fetch.spec.whatwg.org/#response-class).
- **Backends:** JS host Response adapter; WASM head/value plus scoped reader handle; native typed head and bounded byte stream.
- **Effects/shape:** separate immutable response metadata, buffered `Bytes`, and scoped live bodies. Decode into `Result<T,DecodeError>` or existing `Fail`; retain `Untrusted` for raw external text. Existing `Response.text/json` reject non-2xx statuses, unlike web body decoders; document or separate that convenience. Do not force web status-zero cases through `Status.from`, which would defect.

### `Headers` — M

- **Existing:** no Headers type. `Request.headers`, `Response.headers` are `List<(String,String)>`; `Request.header` is case-insensitive first-match lookup (`compiler/std/request.sak`). JS `fetchArgs` creates a host Headers internally.
- **Semantics:** normalized names, value validation, duplicate handling, guards, and special Set-Cookie treatment matter. [Fetch Headers](https://fetch.spec.whatwg.org/#headers-class).
- **Backends:** JS host validation with normalized immutable output; WASM pure list operations plus boundary checks; native validated ordered multimap, not a plain single-value HashMap.
- **Effects/shape:** pure `Headers` value; update returns a new value and invalid inputs return a typed error. Keep secrets in a separate sink-only representation. Host-specific forbidden-header behavior must not silently change portable results.

### `AbortController` — M

- **Existing:** no public type. `compiler/std/task.sak`: `Task.scope`, `race`, `timeout`; JS `fetchArgs` creates an internal AbortController, and cancellation calls `abort`.
- **Semantics:** controller triggers one-way abort with a reason; repeated abort does not reset the signal. [DOM aborting](https://dom.spec.whatwg.org/#interface-abortcontroller).
- **Backends:** JS bridge scope cancellation to host abort; WASM cancellation request/handle retirement; native cancellation state and wakeup through executor, with transport teardown.
- **Effects/shape:** prefer existing scoped task cancellation. If manual cancellation is needed, propose a scoped cancellation-source resource under `Task`, with typed reason. No ambient permission is needed for one's own task, but authority to cancel another task must be an explicit handle. Mutable shared controller aliases are unsuitable as ordinary values.

### `AbortSignal` — M

- **Existing:** none publicly; scheduler cancellation and `timeout` cover related behavior in `compiler/std/task.sak` and JS runtime.
- **Semantics:** observable aborted state/reason; `any`, `timeout`, `throwIfAborted` compose or inspect cancellation. [DOM AbortSignal](https://dom.spec.whatwg.org/#interface-abortsignal).
- **Backends:** JS wrap signal handles; WASM opaque scoped handle with safe completion; native read-side cancellation token plus deadline/composition implementation.
- **Effects/shape:** read-only scoped cancellation token; inspection/subscription is `Task`-effectful, timed creation also `Clock`. Convert expected abort to a declared cancellation outcome, not arbitrary thrown reason. Preserve Sakura's existing timeout cancel-and-join/finalizer guarantees; web abort notification alone does not guarantee work has stopped.

**Permission integration finding:** `sakura_runtime.js::fetchArgs` checks the initial URL's host/port, then calls host fetch without an explicit redirect policy. Source inspection does not show per-hop reauthorization here. This is an untested potential redirect-authority gap, not a demonstrated exploit. The expanded client should explicitly check every redirect and credential-forwarding boundary, canonicalize the same authority representation used for permission matching, and define DNS/IP policy. Do not claim existing `Http.scoped` alone enforces redirect destinations.

## 2. URLs

### `URL` — M

- **Existing:** `compiler/std/url.sak`: `Url`, `is_valid`, `host`, `path`, `query_param`, `template`, `encode_component`; `compiler/std/urlpart.sak`: `UrlPart.from/to_string`. `is_valid` only checks http/https/origin-form prefix and absence of spaces; component extraction splits strings.
- **Semantics:** base resolution, canonicalization, special schemes, credentials, IDNA, IPv4/IPv6, escaping and origin calculation. [WHATWG URL](https://url.spec.whatwg.org/#api).
- **Backends:** JS use host URL then snapshot components; WASM import parsing or compile a shared parser; native Rust `url` is a candidate. Both need common tests.
- **Effects/shape:** pure opaque parsed URL plus `Result<Url,UrlError>`; setters return new values. A URL value grants no network or file access. Preserve explicit safe component interpolation. Object-URL registration/revocation is separate stateful resource work (Blob lifetime), not a pure parser method.

### `URLSearchParams` — M

- **Existing:** no matching type; `Url.query_param` and `Request.param` return a first raw value without percent-decoding (`compiler/std/url.sak`, `request.sak`).
- **Semantics:** ordered repeated keys, form percent-encoding, plus-as-space, sorting and live coupling to URL.search in JS. [URLSearchParams](https://url.spec.whatwg.org/#interface-urlsearchparams).
- **Backends:** JS snapshot host params; WASM pure ordered-pair codec or host import; native `url::form_urlencoded` plus ordered operations and specified sort behavior.
- **Effects/shape:** pure `QueryParams` ordered multimap; `get_all`, functional append/set/remove, explicit attach-to-URL. Reject live aliasing with a mutable parent URL. Parsing has no `Http` effect and needs no grant. Keep query encoding distinct from path-component encoding.

## 3. Binary data, codecs and JSON

### `Blob` — M

- **Existing:** no Blob type; `Bytes` and `compiler/std/bytes.sak::{len,get,from_list}`, `String.to_bytes/from_bytes` in `compiler/std/string.sak` are building blocks. Filesystem resources are separate.
- **Semantics:** immutable bytes with MIME metadata, slicing, byte/text/stream reads; text decoding is not arbitrary charset inference. [W3C File API](https://www.w3.org/TR/FileAPI/#blob-section).
- **Backends:** JS construct/snapshot host Blob; WASM bounded copied bytes or scoped backing handle; native reference-counted byte storage plus MIME metadata and stream adapter.
- **Effects/shape:** pure in-memory `Blob { bytes, media_type }`; file-backed lazy blobs must be scoped and retain `Fs` authority. Avoid platform-dependent newline normalization defaults. Object URLs require a bounded scoped registry, teardown, and distinct authority; no implicit file access.

### `Uint8Array` — S for byte-value coverage; L for full JS memory-object semantics

- **Existing:** compiler-known immutable `Bytes`, `compiler/std/bytes.sak`; JS representation uses Uint8Array internally.
- **Semantics:** mutable typed-array views, buffer sharing, offsets, copy/view distinctions, detachment and resizable-buffer interactions are ECMAScript behavior. [ECMAScript typed arrays](https://tc39.es/ecma262/multipage/indexed-collections.html#sec-typedarray-objects).
- **Backends:** JS copy at boundary; WASM checked byte spans/copied buffers; native owned byte vectors/slices under runtime ownership rules.
- **Effects/shape:** extend `Bytes` with pure slice/concat/update operations. Do not expose aliasable mutable buffers or detachment in core Sakura. Separate borrowed FFI buffers need explicit lifetimes; never let an imported callback retain a guest-memory view.

### `Uint8Array.prototype.toBase64()` — S

- **Existing:** none; `Bytes` supplies the input representation.
- **Semantics:** selectable Base64/base64url alphabet and padding omission. [ECMAScript toBase64](https://tc39.es/ecma262/multipage/indexed-collections.html#sec-uint8array.prototype.tobase64).
- **Backends:** JS host method when available, otherwise a tested codec; WASM pure guest codec or copied import; native small Rust codec with matching options.
- **Effects/shape:** pure `Bytes.to_base64(options) -> String`; explicit alphabet/padding enum and output-size checks. It is encoding, not encryption; do not add a Crypto or Random effect.

### `Uint8Array.fromBase64()` — S

- **Existing:** none.
- **Semantics:** alphabet selection, whitespace and last-chunk handling affect acceptance; malformed input may throw. [ECMAScript fromBase64](https://tc39.es/ecma262/multipage/indexed-collections.html#sec-uint8array.frombase64).
- **Backends:** JS host method/polyfill; WASM shared guest decoder or import; native codec configured to the same acceptance rules.
- **Effects/shape:** pure `decode_base64 -> Result<Bytes,Base64Error>`, explicit strict/loose policy and decoded-size budget. Do not accidentally accept malformed data merely because a host Buffer decoder does.

### `TextEncoder` — S

- **Existing:** `compiler/std/string.sak::to_bytes` is UTF-8 encoding; JS uses host TextEncoder in `sakura_runtime.js::string_to_bytes`.
- **Semantics:** UTF-8 encoding; `encodeInto` counts input UTF-16 code units and output bytes. [WHATWG Encoding](https://encoding.spec.whatwg.org/#interface-textencoder).
- **Backends:** JS existing adapter; WASM existing runtime UTF-8 bytes or host import; native UTF-8 byte copy/view under ownership rules.
- **Effects/shape:** preserve pure `String.to_bytes`. For bounded output, return consumed/output counts with clearly documented units; Sakura strings use UTF-8 byte indexing, so blindly exposing JS counts would be misleading. No encoder object or permission is needed for a stateless conversion.

### `TextDecoder` — M

- **Existing:** `compiler/std/string.sak::from_bytes -> Result<String,Utf8Error>` covers strict whole-buffer UTF-8 only.
- **Semantics:** encoding labels, replacement versus fatal errors, BOM policy and incremental buffering. [WHATWG decoder](https://encoding.spec.whatwg.org/#interface-textdecoder).
- **Backends:** JS TextDecoder with pinned options; WASM host import or shared codec; native `encoding_rs` plus explicit state/flush policy.
- **Effects/shape:** pure whole-buffer decode to Result. Incremental decode either threads an immutable decoder state `(next_state, text)` or uses a scoped resource/effect. UTF-8 strict behavior should remain a stable convenience; do not silently change existing `String.from_bytes` to replacement decoding.

### `atob` — S

- **Existing:** none.
- **Semantics:** forgiving Base64 decode returns a JS binary string, not UTF-8 text. [HTML Base64 utilities](https://html.spec.whatwg.org/multipage/webappapis.html#atob).
- **Backends:** JS atob or shared codec; WASM codec/import; native same Base64 decoder with HTML acceptance rules.
- **Effects/shape:** pure decode to `Result<Bytes,Base64Error>`. Keep a binary-string conversion only in JS interop; byte values 128–255 are not a UTF-8 string. Separate from typed-array decoder options where rules differ.

### `btoa` — S

- **Existing:** none.
- **Semantics:** encodes byte-valued string characters; non-byte characters cause an error. [HTML Base64 utilities](https://html.spec.whatwg.org/multipage/webappapis.html#btoa).
- **Backends:** JS btoa on validated binary input or shared codec; WASM codec/import; native encode bytes directly.
- **Effects/shape:** pure `Bytes.to_base64`, avoiding a misleading String API. For text, require the explicit composition `String.to_bytes` then encode. An interop wrapper for JS strings returns a typed range/input error.

### `JSON` — S for current typed coverage; M for additional interoperability options

- **Existing:** `compiler/std/json.sak`: `Json`, `parse`, `decode<T: Schema>`, `encode<T: Schema>`, `schema`, `get`, `get_string`, `DecodeError`. Runtime support in `compiler/runtime/js/sakura_runtime.js` and `compiler/runtime/rt/src/json.rs`.
- **Semantics:** grammar, numbers, property ordering, parse/stringify hooks and treatment of unsupported JS values matter; JSON cannot preserve arbitrary cycles. [ECMAScript JSON](https://tc39.es/ecma262/multipage/structured-data.html#sec-json-object).
- **Backends:** JS current schema codec/host JSON; WASM existing runtime/host codec path; native existing codec, with shared number/order fixtures rather than independently chosen serialization conventions.
- **Effects/shape:** keep typed pure encode/decode and Result errors. Do not introduce arbitrary JS `toJSON` methods, mutable revivers or arbitrary objects; provide typed explicit transforms. Fix portable number/duplicate-key policy and size/depth limits. Schema validation does not turn JSON serialization into structured clone.

## 4. Streams and their associated classes

No general stream module exists in `compiler/std`. Closest pieces are `Http.stream/read_chunk` (`compiler/std/http.sak`) and `Fs.with_file/read_chunk/write_chunk` (`compiler/std/fs.sak`). These are scoped, effectful resource operations, not Web Streams implementations.

The [WHATWG Streams standard](https://streams.spec.whatwg.org/) defines queueing/backpressure, exclusive locks, cancellation, close/error state, piping, teeing, and byte-stream BYOB ownership. The class-specific semantics below identify the relevant responsibility; the proposed Sakura shapes are design recommendations.

**Shared backend plan for every class below:** JS wraps the corresponding host class inside a trusted scheduler adapter; WASM uses generational scoped handles and asynchronous read/write completions, copying chunks safely; native builds a stream state machine and bounded queues over the existing executor, with Rust AsyncRead/AsyncWrite adapters where suitable. Tokio I/O traits alone do not supply web locks/tee/BYOB semantics. Pure transforms may compile directly on all backends.

**Shared effect/authority rule:** an in-memory immutable sequence needs no I/O permission. Reading/writing a live source/sink carries its effects (`Http`, `Fs`, proposed socket effect) and typed errors. Scheduling uses `Task` where needed; cancellation is tied to the acquiring scope. If a generic stream effect is introduced, it must not hide or enlarge the source/sink authority. No current `Stream` root effect exists. Bound queued bytes and items, total output, tee lag, pending operations and callback reentrancy.

| API | Existing coverage | Semantics to preserve | Proposed shape / API-specific backend work | Size |
|---|---|---|---|---|
| `ReadableStream` | Partial: `Http.Body`/`Http.read_chunk`, `Fs.read_chunk` | Pulling, locking, cancellation and teeing. | Scoped typed reader returning end-of-stream explicitly; JS bridge reader, WASM read import, native pull state machine. An end marker should not conflate an empty data chunk with EOF. | L |
| `WritableStream` | Partial: `Fs.write_chunk` | Ordered writes, backpressure, close versus abort. | Scoped typed sink with write completion; JS writer adapter, WASM write import, native bounded sink. Errors remain declared. | L |
| `TransformStream` | None; collection mapping is only related | Paired endpoints and propagated backpressure/errors. | Typed transformation with callback effect row; JS transform bridge, WASM two endpoint handles, native pump/state machine. Avoid unbounded buffering or orphaned pump tasks. | L |
| `ByteLengthQueuingStrategy` | None | Queue size measured in bytes. | Immutable validated byte-budget policy; JS translate to host strategy, WASM encode limits, native maintain byte accounting. Pure construction, used by stream handlers. | S |
| `CountQueuingStrategy` | None | Queue size measured in chunks. | Immutable item-budget policy, same three adapters; combine with byte cap because one chunk may be huge. Pure construction. | S |
| `ReadableStreamDefaultReader` | Partial scoped chunk reader pattern | Exclusive read lock and lock release. | Scoped reader handle; no copying the logical read cursor. JS `getReader`, WASM handle, native cursor owner. | M |
| `ReadableStreamBYOBReader` | None | Caller-provided read buffers and view ownership. | Use owned byte chunks first; exact BYOB needs explicit buffer lifetime/transfer support. JS copy boundary, WASM checked buffer lease, native owned buffer pool. | L |
| `ReadableStreamDefaultController` | None | Enqueue, close, error and desired queue size. | Internal producer endpoint; JS controller hidden in adapter, WASM enqueue/close imports, native producer state. Do not give escaping controller objects to user callbacks. | M |
| `ReadableByteStreamController` | None | Byte queues and BYOB requests. | Internal bounded byte producer with common byte-copy rules. JS byte controller, WASM lease validation, native byte queue and ownership checks. | L |
| `ReadableStreamBYOBRequest` | None | Completing a pending buffer request. | Internal single-completion request token; JS respond bridge, WASM generation/count validation, native completion state. Runtime validates consumption; current scope checks do not prove one-shot use. | L |
| `WritableStreamDefaultWriter` | Partial `Fs` writes | Writer lock, readiness and completion. | Scoped writer; JS ready/write bridge, WASM pending write IDs, native serialized sink and wakeups. | M |
| `WritableStreamDefaultController` | None | Erroring the writable and observing abort. | Hidden sink controller; JS abort integration, WASM cancellation import, native cancellation state. | M |
| `TransformStreamDefaultController` | None | Enqueue, error, terminate. | Internal typed output endpoint; JS transform callback adapter, WASM safe completion, native transform state. Termination must finalize both sides. | M |

The associated classes are dependencies of the general stream work; their sizes should not be summed as independent projects. A public compatibility layer can expose locks/controllers later, but the idiomatic layer should prefer `with_reader`, `with_writer`, typed folds/maps and scoped piping. These names are proposals, not existing signatures.

## 5. WebSockets

### `WebSocket` — L

- **Existing:** none. `compiler/std/http.sak` and `serve.sak` do not expose a WebSocket upgrade/client API.
- **Semantics:** handshake/protocol selection, text/binary messages, ready states, buffered sends and close negotiation; the classic interface lacks pull-based receive backpressure. [WHATWG WebSockets](https://websockets.spec.whatwg.org/).
- **Backends:** JS host WebSocket with bounded event-to-reader adapter; WASM socket handle and send/receive/close imports; native tokio-tungstenite/tungstenite plus rustls and executor integration. Client and server upgrade are separate work.
- **Effects/shape:** proposed `WebSocket` or `Socket` effect with scoped resource, `net` host/port checks for ws/wss, secret-bearing handshake sinks, typed `Text | Binary` messages and close reason/result. Event handlers must not escape authority scopes. Explicit receive/send limits, slow-consumer policy and scripted peer are needed. Do not assume `Http` authority already authorizes a newly introduced socket root handler.

## 6. Cryptography

### `crypto` (including `getRandomValues`, `randomUUID`, `subtle`) — S for randomness adapters, L for the complete family

- **Existing:** `compiler/std/random.sak::{Random.bytes,Random.uuid,Random.host,Random.seeded}` and `compiler/std/uuid.sak::{Uuid,parse,from_canonical}`. No Crypto object. JS `random_bytes` chunks host draws into 65,536-byte pieces; `random_uuid` delegates to the host. Native Random uses OS entropy.
- **Semantics:** `getRandomValues` fills integer typed arrays with cryptographic randomness and rejects requests over 65,536 bytes; `randomUUID` creates random v4 UUIDs. `subtle` exposes cryptographic operations. [W3C Web Crypto](https://www.w3.org/TR/webcrypto/).
- **Backends:** JS existing crypto adapter; WASM Random imports; native existing entropy/UUID implementation, extended to a portable OS CSPRNG where required.
- **Effects/shape:** randomness remains `Random`, returning immutable Bytes/Uuid. Existing `Random.bytes` deliberately supports more than a single web draw, so compatibility and convenience limits differ. No new manifest key is required by today's model. **Security design decision:** a freely replaceable `Random.seeded` handler is useful in tests but unsuitable for production key generation; distinguish secure entropy authority from simulation in the crypto RFC.

### `SubtleCrypto` — L

- **Existing:** none; no crypto standard module. Design mention of HMAC is aspirational.
- **Semantics:** digest, encrypt/decrypt, sign/verify, key generation, derivation, import/export, wrap/unwrap; algorithm parameters, usages, extractability and encoding are observable. [W3C SubtleCrypto](https://www.w3.org/TR/webcrypto/#subtlecrypto-interface).
- **Backends:** JS trusted `crypto.subtle` adapter with typed algorithm enum; WASM byte/key handles and asynchronous completions; native audited Rust crypto libraries or OpenSSL EVP through Rust bindings/C ABI. Native needs Web Crypto-compatible key formats and signature encodings, not just matching algorithm names.
- **Effects/shape:** deterministic public-data digest/verification can be pure Result operations. Key generation and randomized operations require secure entropy; key-store/HSM operations require a new explicit capability/effect. Treat secret-key operations as a typed crypto service if policy/audit is needed. Choose an algorithm support matrix and report UnsupportedAlgorithm explicitly. Map authentication/format errors, never host exceptions or key material, into public errors. Secure contexts and algorithm availability differ by embedding.

### `CryptoKey` — L

- **Existing:** none; `compiler/std/secret.sak::Secret` is a secrecy wrapper, not a non-extractable key handle.
- **Semantics:** key kind, algorithm, allowed usages and extractability constrain operations. [W3C CryptoKey](https://www.w3.org/TR/webcrypto/#cryptokey-interface).
- **Backends:** JS scoped host CryptoKey handle; WASM typed generation-checked handle; native owned provider key/EVP handle with destruction and import/export validation.
- **Effects/shape:** opaque key type, no Show/Schema/default clone; typed usage permissions and explicit export policy. Creation and release obey resource scope. A Secret-wrapped byte array alone cannot enforce non-extractability. Do not allow generic structured clone/message serialization to transfer key authority accidentally; define a dedicated delegation rule if needed.

## 7. Time, scheduling, diagnostics and interaction

### `setTimeout` — S for idiomatic delay, M for exact callback compatibility

- **Existing:** `compiler/std/clock.sak::Clock.sleep`; `compiler/std/task.sak::timeout`, `Task.scope/spawn`; JS `sleep_chain` supports long delays with cancellable host timers.
- **Semantics:** callback scheduling after a delay, with timer nesting/clamping and event-loop ordering; deadlines are not exact execution times. [HTML timers](https://html.spec.whatwg.org/multipage/timers-and-user-prompts.html#timers).
- **Backends:** JS existing timer bridge; WASM Clock completion imports; native existing Tokio timer host.
- **Effects/shape:** `Clock.sleep`, or a scoped child that sleeps then runs a callback under its inherited effects. No detached callbacks, string-eval timers or ambient timer IDs. Use `Clock.virtual` for reproducible tests; delay alone does not promise JS event-loop equivalence.

### `clearTimeout` — M

- **Existing:** none publicly; task cancellation clears timers internally in `sakura_runtime.js::clock_sleep_ms/sleep_chain`.
- **Semantics:** clears an active timeout handle; missing/inactive handles have no effect. [HTML timers](https://html.spec.whatwg.org/multipage/timers-and-user-prompts.html#timers).
- **Backends:** JS cancellation-to-clearTimeout; WASM retire pending timer; native abort executor timer.
- **Effects/shape:** cancel a scoped timer/task resource under `Task` or a proposed timer effect, preserving idempotence and finalization. No global integer cancellation namespace. Scope exit already covers the common cleanup need; do not pretend a public `Task.cancel` exists today.

### `setInterval` — M

- **Existing:** no interval operation. `Clock.sleep` and scoped tasks can build periodic work.
- **Semantics:** repeated timer scheduling; callbacks may be late and overlap with asynchronous work launched by earlier callbacks. [HTML timers](https://html.spec.whatwg.org/multipage/timers-and-user-prompts.html#timers).
- **Backends:** JS host interval or an explicit scheduler; WASM periodic scheduling import; native Tokio interval/deadline loop configured to the portable policy.
- **Effects/shape:** scoped periodic task/tick source with `Clock`, `Task`, callback effects, cancellation, and an explicit missed-tick policy. Serialize callback execution by default and document the semantic departure. Test through virtual time.

### `clearInterval` — M

- **Existing:** none publicly; scoped task termination is related.
- **Semantics:** stops future repetitions; does not retroactively undo callback work. [HTML timers](https://html.spec.whatwg.org/multipage/timers-and-user-prompts.html#timers).
- **Backends:** JS clear interval, WASM cancel/retire periodic handle, native stop timer and wake pending wait.
- **Effects/shape:** scoped cancellation with join/finalization is stronger and more useful than dropping a raw interval ID. Define whether already-running work finishes or is cooperatively cancelled; use `Task`, no new ambient grant.

### `queueMicrotask` — M

- **Existing:** no public API. `compiler/std/task.sak::Task.yield_now` is a cancellation/scheduler point; JS live implementation uses a zero-delay timer, not a microtask.
- **Semantics:** FIFO microtask checkpoint execution, recursive queueing and exception reporting. [HTML microtasks](https://html.spec.whatwg.org/multipage/webappapis.html#microtask-queuing).
- **Backends:** JS queue host microtask but re-enter the Sakura scheduler safely; WASM enqueue an instance-local callback completion; native add a distinct bounded checkpoint queue if compatibility is required.
- **Effects/shape:** prefer scoped `Task` operations. A proposed defer operation must carry callback effects/authority and remain in scope. Do not expose event-loop phase observation as a pure function or claim `yield_now` is equivalent. Bound starvation from self-enqueueing.

### `console` — S for logging adapters, M for full method family

- **Existing:** `compiler/std/console.sak::Console.print/read_line`; `compiler/std/log.sak::Log.event`, `stderr`, `json`, `captured`, `Level`.
- **Semantics:** log levels, assertions, formatting, counts, groups, timers and traces; presentation is host-defined. [WHATWG Console](https://console.spec.whatwg.org/).
- **Backends:** JS map typed output to host console or existing process output; WASM output imports; native stdout/stderr formatter plus scoped counters/groups if desired.
- **Effects/shape:** preserve `Console` and structured `Log`; timings additionally use `Clock`. No current permission key. Disallow arbitrary object inspection of secrets/capabilities. Keep portable logs deterministic and machine-readable; browser inspector rendering is not a portable contract. `console.assert` reporting must not silently become a Sakura contract defect.

### `performance` — M

- **Existing:** `compiler/std/clock.sak::Clock.monotonic`, whole-millisecond `Duration`; JS runtime uses performance.now internally and floors it. No marks/measures/observer API.
- **Semantics:** monotonic high-resolution time relative to a time origin; marks/measures create timeline state. [High Resolution Time](https://www.w3.org/TR/hr-time-3/), [User Timing](https://www.w3.org/TR/user-timing/).
- **Backends:** JS performance adapter; WASM clock/measurement imports; native std::time::Instant plus bounded measurement storage and observer scheduling if added.
- **Effects/shape:** keep `Clock` for reads; add typed measurement/log records or a proposed Metrics effect for timeline state. Select precision/rounding explicitly. Browser navigation/resource timelines are unavailable without those sources; do not fabricate them. Tests use virtual clocks and scripted metrics, no ambient nondeterministic pure calls.

### `reportError` — M

- **Existing:** none. Related: runtime Defect reporting, structured diagnostic contracts, `Log.event`; `compiler/runtime/js/sakura_runtime.js`, `compiler/runtime/rt/ABI.md`, `compiler/std/log.sak`.
- **Semantics:** reports an exception through the host error-reporting mechanism; this is not a normal throw/return. [HTML reportError](https://html.spec.whatwg.org/multipage/webappapis.html#dom-reporterror).
- **Backends:** JS host reportError if present, otherwise explicitly defined reporter; WASM host diagnostic import; native structured reporter with defined process/task policy.
- **Effects/shape:** a declared diagnostic/log operation carrying safe typed error data. Do not convert expected failures into detached uncaught events, and do not suppress fatal defects by routing them through Log. Redact secrets and preserve versioned defect contracts. Node's documented globals do not guarantee this host function.

### `alert` — S

- **Existing:** building blocks `Console.print/read_line` in `compiler/std/console.sak`; no alert function.
- **Semantics:** display and acknowledgement; Bun treats it as CLI interaction. [HTML simple dialogs](https://html.spec.whatwg.org/multipage/timers-and-user-prompts.html#simple-dialogs).
- **Backends:** JS Bun/Deno/browser dialog when appropriate or line I/O; WASM console/dialog host import; native terminal print/read.
- **Effects/shape:** explicit `Console` helper with scripted test handler and documented noninteractive behavior. No hidden prompt from a pure function or background agent tool.

### `confirm` — S

- **Existing:** no confirm function; same Console primitives.
- **Semantics:** asks a yes/no question. [HTML dialogs](https://html.spec.whatwg.org/multipage/timers-and-user-prompts.html#simple-dialogs).
- **Backends:** JS host confirm or line parser; WASM prompt import; native terminal input parser.
- **Effects/shape:** `Console` helper returning typed confirmation/decline/EOF outcome, or a documented Bool policy. Do not equate lack of input with approval. Console authority exists; no new manifest key is currently defined.

### `prompt` — S

- **Existing:** `Console.read_line -> Option<String>`; no message/default prompt helper.
- **Semantics:** text input with cancellation and optional default. [HTML dialogs](https://html.spec.whatwg.org/multipage/timers-and-user-prompts.html#simple-dialogs).
- **Backends:** JS host prompt or line I/O, WASM input import, native terminal read.
- **Effects/shape:** `Console` operation returning Option or typed Result; distinguish cancel/EOF if needed. User input should enter validation as Untrusted. Headless agents should receive a scripted handler rather than blocking unexpectedly.

## 8. Events

### `EventTarget` — L

- **Existing:** none. Confined handler state is a useful implementation mechanism but is not a public event bus.
- **Semantics:** listener identity, once/capture/passive/signal options, ordered synchronous dispatch, cancellation and reentrancy. [DOM EventTarget](https://dom.spec.whatwg.org/#interface-eventtarget).
- **Backends:** JS native target behind adapters; WASM per-instance listener registry and scheduler callback IDs; native confined registry with explicit dispatch order.
- **Effects/shape:** mutable listener lists do not fit ordinary immutable values. Prefer typed scoped subscriptions owned by a handler; event types are variants, not arbitrary strings plus `any`. A proposed Events effect accounts for registration/dispatch and callback effects. Exact synchronous DOM dispatch cannot simply run suspendable Sakura callbacks; choose restricted compatibility callbacks or an explicitly asynchronous typed service.

### `Event` — S for typed metadata; M with compatibility dispatch

- **Existing:** none.
- **Semantics:** type, target, timestamp, propagation/cancellation flags and default prevention. [DOM Event](https://dom.spec.whatwg.org/#interface-event).
- **Backends:** JS snapshot event fields, WASM copy typed metadata, native construct record; compatibility flags stay in dispatch-owned state.
- **Effects/shape:** immutable event payload, pure metadata construction when timestamp is supplied; automatic time acquisition uses Clock. Return an explicit dispatch decision instead of mutating a shared Event. Never fabricate trusted events from user constructors.

### `ErrorEvent` — S

- **Existing:** none; typed error tags and runtime diagnostic records are related.
- **Semantics:** message/source location plus an error payload for script errors. [HTML ErrorEvent](https://html.spec.whatwg.org/multipage/webappapis.html#the-errorevent-interface).
- **Backends:** JS normalize/redact host error; WASM serialize bounded error metadata; native produce equivalent typed record.
- **Effects/shape:** pure safe error data; reporting/dispatch uses Log/proposed Events. Do not import arbitrary error objects, stacks containing secrets, or catchable aliases of fatal Sakura defects.

### `CloseEvent` — S

- **Existing:** none.
- **Semantics:** close code, reason and clean-close status. [WebSockets CloseEvent](https://websockets.spec.whatwg.org/#the-closeevent-interface).
- **Backends:** JS copy host close fields; WASM socket completion record; native derive from close handshake result.
- **Effects/shape:** immutable `CloseInfo`; receiving it belongs to the socket effect. Keep protocol validation errors distinct from normal closure. UTF-8 reason limits and legal outbound close codes belong in the socket adapter.

### `MessageEvent` — S for typed envelope; M with full interop

- **Existing:** none.
- **Semantics:** data, origin, source and transferred ports describe a delivered message. [HTML MessageEvent](https://html.spec.whatwg.org/multipage/comms.html#the-messageevent-interface).
- **Backends:** JS validate payload/metadata at receipt; WASM decode bounded typed envelope and verify handles; native equivalent channel message record.
- **Effects/shape:** `Message<T>` with validated provenance and payload, scoped endpoints rather than serializable arbitrary `source` objects. Receipt uses a proposed messaging effect; pure record construction alone conveys no authority. Origin metadata must be set by trusted transport, not trusted merely because a sender supplied a string.

## 9. Workers, messaging, cloning and realms

### `Worker` — L

- **Existing:** none. `compiler/std/task.sak::Task.scope/spawn/join/all/race` provides structured concurrency within one instance, not separate JS globals or CPU-parallel shared-heap work.
- **Semantics:** separate execution environment, script/module loading, message transport, termination and error propagation. [HTML workers](https://html.spec.whatwg.org/multipage/workers.html).
- **Backends:** JS Bun/Deno/browser Worker or Node worker_threads adapter; WASM separate instance per worker, with host-owned message queues; native separate instance on an OS thread or process with serialized values and its own runtime context. Executing arbitrary JS worker scripts natively would require embedding a JS engine.
- **Effects/shape:** propose scoped `Worker<I,O>` or typed isolated task service, explicit module identity and attenuated grants. Never share runtime RC pointers/handler state across threads. Define thread-count, memory, CPU/time and message limits plus deterministic simulation. Worker module loading must not implicitly widen Fs/net authority. `process = []` does not authorize subprocess workers. SharedArrayBuffer/Atomics are not required by this inventory and should not enter core Sakura as a side effect of Worker support.

### `self.postMessage` — M after worker/channel foundation

- **Existing:** none; task results via join are the nearest primitive.
- **Semantics:** worker-to-parent message serialization, queued delivery and optional transfer list. [Dedicated worker messaging](https://html.spec.whatwg.org/multipage/workers.html#dedicated-workers-and-the-worker-interface).
- **Backends:** JS worker postMessage or Node parentPort, WASM parent endpoint import, native bounded parent queue.
- **Effects/shape:** explicit scoped parent endpoint and `send<T> -> Result<Unit,SendError>` under a proposed messaging/worker effect. No ambient `self`, arbitrary graphs or transfer of capabilities. Delivery order, queue overflow and shutdown failures must be specified; sent is not necessarily processed.

### `structuredClone` — L for compatibility; usually unnecessary for ordinary Sakura values

- **Existing:** no clone API. Value semantics (`docs/design.md` §8.1) already make assignment/capture independent; `compiler/std/graph.sak::Graph` models graph cycles by IDs. `Json.encode/decode` is not structured clone.
- **Semantics:** supported graphs retain cycles/repeated references; unsupported values fail; transfer can detach buffers; shared memory has special rules. [HTML structured data](https://html.spec.whatwg.org/multipage/structured-data.html#safe-passing-of-structured-data).
- **Backends:** JS native structuredClone only in an isolated foreign-value layer; WASM explicit typed wire codec, no arbitrary host graph inside RC heap; native typed serialization, or a separate graph arena/JS engine for literal JS graph behavior.
- **Effects/shape:** ordinary Sakura copy needs no deep-clone API. For transport, propose a deliberately restricted wire-value contract and Result codec. Exclude closures, capabilities, Secrets, scoped resources and provider keys by default. Copying pure serializable values is pure; transfer/detachment is an effectful ownership operation and would require new rules. Arbitrary cyclic JS objects violate current acyclic heap assumptions.

### `MessagePort` — L

- **Existing:** none; `docs/design.md` §8.6 explicitly has no channel type in v0.
- **Semantics:** entangled endpoint, start/close, queued delivery and transferability. [HTML ports](https://html.spec.whatwg.org/multipage/web-messaging.html#message-ports).
- **Backends:** JS host port, WASM opaque endpoint handle with completion queue, native bounded channel endpoint integrated with executor.
- **Effects/shape:** scoped typed sender/receiver; send/receive/cancel are explicit operations with typed errors. Possession of endpoint grants access; no ambient global channel registry. Transfer cannot bypass scope checks. Define whether closure drains, rejects or discards pending work.

### `MessageChannel` — L shared with MessagePort

- **Existing:** none.
- **Semantics:** creates two entangled ports. [HTML channels](https://html.spec.whatwg.org/multipage/web-messaging.html#message-channels).
- **Backends:** JS host MessageChannel, WASM host-created paired handles, native paired queues; all need bounds and lifecycle protocol.
- **Effects/shape:** proposed `with_channel<T>(capacity, callback)` under Task/new Channel effect; expose typed bounded endpoints. Construction must not create detached resources. Exact web unbounded queue behavior is a poor portable default; document bounded behavior as an intentional adaptation.

### `BroadcastChannel` — L

- **Existing:** none.
- **Semantics:** named-channel fan-out constrained by origin/storage context; asynchronous delivery without transferable ownership. [HTML broadcasting](https://html.spec.whatwg.org/multipage/web-messaging.html#broadcasting-to-other-browsing-contexts).
- **Backends:** JS host channel with runtime-specific namespace; WASM scoped broker subscription import; native explicit local broker/IPC service if cross-process delivery is required. Process-local queues alone do not implement cross-process scope.
- **Effects/shape:** proposed topic capability plus typed messages and explicit publish/subscribe effect; deny ambient lookup by arbitrary name. Define namespace, participant scope, overflow, ordering and teardown; scripted broker for tests. Existing `net` permissions do not automatically control an in-process broadcast registry, and no broadcast permission exists today.

### `ShadowRealm` — L; recommend optional foreign-runtime feature

- **Existing:** none.
- **Semantics:** separate JS realm/intrinsics, evaluation/import and restricted cross-realm values/callables; not a general security sandbox. This is [TC39's ShadowRealm proposal](https://tc39.es/proposal-shadowrealm/), not a WHATWG/W3C standard.
- **Backends:** JS feature-detected host support, not portable to every JS engine; WASM host JS realm import only when embedding supports it; native requires a JS engine with the proposal or an explicit UnsupportedFeature result. Rust scopes/threads are not ShadowRealm.
- **Effects/shape:** expose only under an explicitly trusted foreign-evaluation service with import authority and resource limits. Dynamic source evaluation conflicts with static effects/contracts and agent-reviewable behavior. Prefer statically compiled isolated Sakura modules; that meets isolation needs without claiming ShadowRealm compatibility. No new Foreign/Realm effect is claimed as existing std coverage here.

## 10. Additional APIs named in the brief but absent from Bun's explicit table

These assessments are additional scope, not silently inserted into the 49-entry count.

### `File` — M

- **Existing:** no web File. `compiler/std/fs.sak::File` is an open scoped resource, not immutable named bytes; Fs chunk/read operations can supply content.
- **Semantics:** Blob plus filename and lastModified; a File object need not designate a live disk file. [W3C File](https://www.w3.org/TR/FileAPI/#file-section).
- **Backends:** JS construct/snapshot host File; WASM copy bytes/metadata or scoped source; native byte-backed record with Blob operations.
- **Effects/shape:** proposed `FileData` value to avoid collision with `Fs.File`. Explicit lastModified keeps construction pure; acquiring current time uses Clock. Reading disk uses Fs grants and scoped access. Do not infer filesystem authority from a filename.

### `FormData` — M

- **Existing:** none; request body is String and JSON helpers only.
- **Semantics:** ordered repeated names with string/File values; multipart encoding and boundary handling integrate with fetch. [WHATWG FormData](https://xhr.spec.whatwg.org/#interface-formdata).
- **Backends:** JS host FormData from typed entries; WASM typed list/file handles plus encoder import; native multipart encoder/parser preserving filenames, MIME metadata and repeated fields.
- **Effects/shape:** immutable `Form` with `Text | FileData` parts and Result parsing; pure buffered conversion, streaming inherits source effects. Require byte/part/header limits. Choose explicit boundary for deterministic encoded output or make boundary generation effectful; never auto-read a path embedded in a form value.

### `URLPattern` — M; L for complete regexp compatibility

- **Existing:** no URLPattern; related `compiler/std/router.sak::{Router,route,get,post,handle}` performs path route matching, not full URL-component patterns.
- **Semantics:** component patterns, base URL/canonicalization, named groups and regexp matching. [WHATWG URL Pattern](https://urlpattern.spec.whatwg.org/).
- **Backends:** JS host URLPattern when available, otherwise tested implementation; WASM import or compiled parser/matcher; native Rust `urlpattern` candidate plus an explicitly selected compatible regexp engine.
- **Effects/shape:** pure compiled pattern and `Result<Pattern,PatternError>`, `Option<Match>` result. Bound compilation/matching work. Rust regexp semantics cannot be assumed identical to JS regexp; a limited safe syntax must be advertised as a subset, not conformance. Matching does not grant URL access.

### `TextEncoderStream` — M after streams

- **Existing:** none; String.to_bytes covers non-streaming encoding.
- **Semantics:** text-to-UTF-8 transform with chunk-boundary handling. [Encoding encoder stream](https://encoding.spec.whatwg.org/#interface-textencoderstream).
- **Backends:** JS host encoder stream; WASM transform/codec import or guest state; native UTF-8 incremental encoder integrated with stream protocol.
- **Effects/shape:** typed text-to-bytes transform, source/sink effects preserved; pure chunks when no pending cross-chunk state is needed. JS lone-surrogate handling is an interop concern absent from ordinary valid Sakura strings. Bounded queues and finalization follow §4.

### `TextDecoderStream` — M after streams

- **Existing:** none; strict whole-buffer String.from_bytes is related. HTTP runtime uses a decoder internally but exposes no decoder stream.
- **Semantics:** incremental decoding retains partial characters; fatal/BOM policy and end flush matter. [Encoding decoder stream](https://encoding.spec.whatwg.org/#interface-textdecoderstream).
- **Backends:** JS host decoder stream; WASM codec handle plus chunk imports or guest codec; native encoding_rs decoder integrated with streams.
- **Effects/shape:** typed bytes-to-text transform with explicit encoding/options, typed DecodeError, bounded carry state. Preserve source effects and Untrusted status; stream decoding alone is not application validation.

### `CompressionStream` — M after streams

- **Existing:** none.
- **Semantics:** supported format, framing and final flush determine encoded output. [WHATWG Compression](https://compression.spec.whatwg.org/).
- **Backends:** JS host CompressionStream for a fixed common format set; WASM codec import or compiled compression library; native flate2/zlib-compatible implementation, with separate codecs only for additionally standardized/chosen formats.
- **Effects/shape:** pure whole-buffer compression or explicit state-threading; scoped streaming transform carries source/sink effects. Pin algorithm/options/version when byte-for-byte deterministic output matters; equivalent decompression does not imply identical compressed bytes. Bound input/output and CPU work.

### `DecompressionStream` — M after streams

- **Existing:** none.
- **Semantics:** malformed/truncated data, checksums, member/trailing-data rules and format-specific framing. [WHATWG Compression](https://compression.spec.whatwg.org/).
- **Backends:** JS host decompressor with limits; WASM host/guest codec; native flate2 or chosen codec with format-policy layer.
- **Effects/shape:** Result/Fail<CompressionError>, bounded output and expansion/work limits; never trust compressed input to set allocation sizes. Pure buffered transform is possible; streaming inherits I/O effects. Cancellation and final flush errors must propagate.

### `CustomEvent` — S for values; depends on EventTarget for dispatch

- **Existing:** none; nominal variants/records already model typed event payloads.
- **Semantics:** Event with an arbitrary detail payload. [DOM CustomEvent](https://dom.spec.whatwg.org/#interface-customevent).
- **Backends:** JS validate/copy detail; WASM typed payload codec; native record/variant. Host handles require the separate scoped interop layer.
- **Effects/shape:** `Event<T>`/domain variants, pure construction with explicit timestamp; proposed Events effect for dispatch. Arbitrary aliased `any` payloads, Secret serialization and unscoped listeners are rejected by the idiomatic model.

### `DOMException` — S

- **Existing:** none as a type. Related `compiler/std/result.sak`, nominal error tags, `DecodeError`, `HttpError`, `FsError`.
- **Semantics:** named web exception with message and legacy numeric code. [WHATWG Web IDL DOMException](https://webidl.spec.whatwg.org/#idl-DOMException).
- **Backends:** JS catch/normalize known DOMException values; WASM tagged error envelope; native map provider errors into the same documented tags.
- **Effects/shape:** map expected cases such as abort, invalid input, unsupported operation and quota into stable typed errors/Result; preserve original web name only as safe diagnostic metadata. Do not make arbitrary host exception names Sakura's stable error contract, and keep runtime defects separate.

## 11. Deno and Node exposure: useful portability differences

This is documentation-based comparison, not a version-by-version runtime conformance test. The fetched Node pages identify v26.12.0; Deno pages are rolling documentation. Feature detection and a pinned host support matrix remain necessary.

| Family | Deno exposure | Node exposure / difference |
|---|---|---|
| Fetch/cancellation | Global fetch, Request, Response, Headers, FormData; AbortController/Signal. [Fetch docs](https://docs.deno.com/api/web/fetch/), [platform docs](https://docs.deno.com/api/web/platform/) | Globals; fetch uses Undici. [Node globals](https://nodejs.org/api/globals.html) |
| URL/data/codecs | Global URL/SearchParams/URLPattern, Blob/File, encoding and Base64 helpers. [URL](https://docs.deno.com/api/web/url/), [File](https://docs.deno.com/api/web/file/), [encoding](https://docs.deno.com/api/web/) | Globals for URL types, Blob/File, encoders/decoders and atob/btoa; the latter two are marked legacy. [Node globals](https://nodejs.org/api/globals.html) |
| Streams | Global stream classes, strategies and compression transforms. [Streams](https://docs.deno.com/api/web/streams/) | Global classes and `node:stream/web`; distinct from classic Node streams. [Node Web Streams](https://nodejs.org/api/webstreams.html) |
| Crypto | Global crypto/SubtleCrypto/CryptoKey. [Deno crypto](https://docs.deno.com/api/web/) | Globals, dependent on crypto-enabled builds; `node:crypto` has additional APIs. [Node globals](https://nodejs.org/api/globals.html) |
| WebSocket/events | WebSocket plus EventTarget/Event/CustomEvent/ErrorEvent/MessageEvent and close-event support. [WebSocket](https://docs.deno.com/api/web/websockets/), [events](https://docs.deno.com/api/web/) | Global web classes, including ErrorEvent in this documented version; Node EventEmitter remains separate. [Node globals](https://nodejs.org/api/globals.html) |
| Timers/microtasks | Global timers and queueMicrotask. [Deno platform](https://docs.deno.com/api/web/platform/) | Global timers return Timeout objects with Node lifecycle methods; HTML's numeric timer handles are not a portable assumption. [Node timers](https://nodejs.org/api/timers.html) |
| Diagnostics/time | console, performance, reportError. [Platform](https://docs.deno.com/api/web/platform/), [performance](https://docs.deno.com/api/web/performance/) | console/performance globals; reportError is not listed in the fetched globals reference. Treat as unguaranteed. [Node globals](https://nodejs.org/api/globals.html) |
| Worker | Global Worker and worker-global postMessage. [Deno workers](https://docs.deno.com/api/web/workers/) | `Worker` from `node:worker_threads`; worker replies via `parentPort.postMessage`, not browser `self.postMessage`. Node Worker also has Node-specific lifecycle/options. [Worker threads](https://nodejs.org/api/worker_threads.html) |
| Channels/clone | MessageChannel/Port, BroadcastChannel, structuredClone. [Messaging](https://docs.deno.com/api/web/messaging/), [platform](https://docs.deno.com/api/web/platform/) | MessageChannel/Port and BroadcastChannel available globally and/or through worker_threads; structuredClone global. Broadcasting is within the process's worker context, not browser-origin storage topology. [Worker threads](https://nodejs.org/api/worker_threads.html), [globals](https://nodejs.org/api/globals.html) |
| Dialogs | Global CLI alert/confirm/prompt. [Deno platform](https://docs.deno.com/api/web/platform/) | No standard global dialog surface documented; use a terminal adapter such as readline. [Node globals](https://nodejs.org/api/globals.html) |
| ECMAScript entries | JSON/Uint8Array are engine facilities; verify the exact Base64 methods against pinned engine version. | Same distinction; inclusion of JSON/Uint8Array says nothing about every newly added method. |
| ShadowRealm | Not established as a supported public API by the reviewed Deno references. | Not in fetched globals reference; experimental engine flags would not establish portable standard support. |

Deno's permission model is relevant precedent: filesystem/network/environment access is gated, but its permissions are a runtime authority boundary rather than Sakura's static effect rows. Deno additionally warns about privilege implications of code running on the same thread. This research does not establish that every worker inherits or restricts permissions identically across runtime versions. Specify Sakura worker grants independently. [Deno security and permissions](https://docs.deno.com/runtime/fundamentals/security/).

Host compatibility is not effect compatibility. A JS global can exist while still being unsuitable for an unmediated Sakura call, particularly clocks, randomness, messaging and callbacks.

## 12. Which nearby Bun APIs are Bun-specific?

The Web APIs table is the requested inventory. Navigation links and methods illustrated on neighboring pages are not automatically additions to it.

| Bun documentation | Standard portion | Bun-specific portion / scope consequence |
|---|---|---|
| [Fetch](https://bun.com/docs/runtime/networking/fetch) | fetch and request/response/body concepts | Extensions such as proxy/TLS/Unix-socket options require separate adapter policy; they are not portable Fetch options. |
| [WebSockets](https://bun.com/docs/runtime/http/websockets) | Client WebSocket and wire protocol | `Bun.serve` upgrade hooks, `ServerWebSocket`, publish/subscribe and server callbacks are Bun APIs; a WebSocket client does not implement them. |
| [Workers](https://bun.com/docs/runtime/workers) | Worker/postMessage model | Runtime extensions such as worker lifecycle/ref behavior and Bun-specific options require separate compatibility decisions. |
| [Streams](https://bun.com/docs/runtime/streams) | Web readable/writable/transform streams | `Bun.ArrayBufferSink`, direct-stream extensions and `Bun.readableStreamTo*` helpers are not generic Web Streams classes. |
| [File I/O](https://bun.com/docs/runtime/file-io) | Blob/File-compatible data interfaces | `Bun.file`, `Bun.write`, file writers and path-backed lazy I/O are Bun filesystem APIs. A web File is not a portable path reader. |
| [Console](https://bun.com/docs/runtime/console) | console logging | Async iteration over console for stdin is a Bun extension. Sakura already has explicit line input. |
| [Bun APIs index](https://bun.com/docs/runtime/bun-apis) | May consume standard values | Bun.serve, spawn, TCP/UDP, SQLite/SQL, FFI, hashing/password helpers, transpilation, package tooling and other Bun namespace APIs are a separate product surface. |

The page's realm entry is a TC39 proposal rather than a Bun invention; JSON and byte arrays are ECMAScript rather than Bun-specific. Likewise, CLI adaptation of dialog APIs is host behavior around HTML names, not an implementation of browser modal UI.

## 13. Implementation order and design decisions

1. **Specify the supported portable contract.** Select standard versions/algorithms/encoding labels, clarify pure versus resource APIs, and define stable error tags. Decide whether the goal is compatible functionality or literal host-object interoperation; label subsets clearly.
2. **Complete pure data foundations.** Bytes utilities/Base64, correct URL parsing/query semantics, immutable headers, Blob/FileData/FormData, encoding options. Extend existing modules where coherent. Reuse existing Json functionality.
3. **Build shared scoped streaming infrastructure.** Reader/writer/resource handling, bounded queues, cancellation and runtime completion protocol. Then add body streaming, decoder/compression transforms and socket adapters.
4. **Finish HTTP backend parity and policy.** Native port, redirect authority, header/cookie policy, binary bodies, resource limits, malformed-input handling and secret-safe replay. A host-dependent CORS/cookie/cache model should be explicit; server runtimes are not browser security contexts.
5. **Add crypto with a reviewed entropy/key policy.** Start with well-scoped digest/verification and secure random values, then key lifecycle and algorithm matrix. Never imply general Web Crypto parity from SHA-256 alone.
6. **Keep concurrency expansion in an RFC.** Typed channels, workers, cross-instance values and topic authority change current no-channel/single-thread assumptions. Shared memory and arbitrary graph transfer need separate decisions. EventTarget and ShadowRealm should not dictate the core language model.

Proposed public names throughout this report are sketches, not compilable Sakura declarations. New semantics/protocol/permission work should follow `docs/rfcs/README.md`; host architecture should use ADRs. Semantics/security changes require named maintainer review per `AGENTS.md`/`CONTRIBUTING.md`. No approval, edit, commit, issue, PR, deployment or message to others was performed for this research.

### Native implementation candidates and their limits

- [Rust url](https://docs.rs/url/latest/url/) supplies WHATWG-oriented parsing and form-urlencoding; still test normalization and permission comparison together.
- [encoding_rs](https://docs.rs/encoding_rs/latest/encoding_rs/) implements Encoding Standard codecs and incremental conversion; Sakura still needs state/error/lifetime adapters.
- [reqwest](https://docs.rs/reqwest/latest/reqwest/) is a useful HTTP client foundation; existing dependency declarations do not implement Fetch semantics or Sakura authority checks.
- [tokio-tungstenite](https://docs.rs/tokio-tungstenite/latest/tokio_tungstenite/) provides asynchronous WebSocket transport; Sakura still needs typed lifecycle, limits and replay.
- [flate2](https://docs.rs/flate2/latest/flate2/) supplies DEFLATE/gzip/zlib codecs; format acceptance and deterministic-output policy remain Sakura responsibilities.
- [OpenSSL bindings](https://docs.rs/openssl/latest/openssl/) are one broad native crypto option; audited pure Rust implementations may be preferable for selected algorithms. Provider choice, algorithm support, key formats and platform support need deliberate review.
- [Rust urlpattern](https://docs.rs/urlpattern/latest/urlpattern/) is a candidate parser/matcher foundation; regexp compatibility and resource limits need independent assessment.

These links establish available implementation components, not endorsements of a particular pinned version or proof of web conformance. No dependencies were installed.

## 14. Verification and limits of this report

Performed: repository source/document reads, targeted symbol searches, direct retrieval of the Bun page and official standards/runtime documentation, inventory cross-check, and report-content/path checks. The check found all 49 listed entries, all nine additional APIs, and all eight expanded stream classes. All 26 referenced existing repository file paths checked were present; the separately named `compiler/std/crypto.sak` was confirmed absent, as reported. Git porcelain status matched the research snapshot after writing this file. No executable Sakura or runtime conformance tests were run. Existing tests and documentation are evidence of intended/current behavior, not newly verified passes.

Suggested acceptance evidence for implementation:

- Spec-linked positive/negative vectors: URLs and encodings, duplicate headers/query keys, malformed Base64/JSON/multipart, abort races, stream locks/EOF/partial reads, crypto key usages and invalid tags, WebSocket close errors, channel shutdown.
- JS/WASM/native parity over values, typed errors, effect traces and finalization. Compare encoded representations only when the contract promises canonical bytes.
- Permission denial at initial access and redirected/reconnected access, worker grant attenuation, no secret leakage in errors/logs/cassettes, no resource/capability escape.
- Deterministic fake Clock/Random/Http/task/channel handlers; bounded queues, bytes, recursion, pending operations, workers and decompression output. Pin replay schema and diagnostic identities.
- When compiler implementation changes land, run the repository's prescribed fmt/clippy/test workspace gates and targeted conformance cases; website changes additionally require its documented checks. None is claimed passed here.

Documentation may evolve after the research date. The Bun table is an overview rather than an exhaustive method list; this report covers every named entry, explicitly expands stream-associated classes, and separates the brief's additional examples. A full Web Platform Tests campaign and complete algorithm-by-algorithm Web Crypto compatibility study remain implementation-stage work.
