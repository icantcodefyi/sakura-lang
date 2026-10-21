# Friction log: parcel-quotes

Author: Claude Opus 5.5, in one fresh session with no repository checkout. The release was
`sakura 0.1.0 (2014e821b87a0d30cb8061b6158f99723f0f676d)` on Node v26.12.0 (macOS, arm64).
The only sources were `sakura --help`, `sakura prompt`, `sakura doc`, https://www.sakuralang.com
(`llms-full.txt`, `/docs/stdlib.md`, `/docs/agents/cassettes.md`, `/docs/tour/capabilities.md`)
and BRIEF.md.

## 2026-10-21: G02 is still open, so the Node pull-boundary consumer is not built (blocker)

- **Expected:** a public recipe for loading the emitted `main.wasm` from a Node application.
  It would cover calling a library entry such as `quote` with a typed value, servicing a
  pending `Http` operation in the host, resuming the call, getting the typed result or error,
  setting `SAKURA_PERMISSIONS`, and releasing the invocation.
- **What happened:** I searched `llms-full.txt` (680 KB) for embedding, pull boundary,
  suspend, library entry and host import. None of it documents such an API. The backends page
  only says that WASM builds write `main.wasm` and the Node host files, and that `run` and
  `test` execute through Node. I ran `sakura build --target wasm` and looked at the output
  directory listing and at the header comment of the emitted `sakura_wasm_host.js`. The comment
  describes a whole-program runner: `node sakura_wasm_host.js <module.wasm> [args…]` runs
  `main`. The file's exports include `runMain` and `runTests`. Nothing documents how to invoke
  a `pub fn` that is not `main`. The brief says not to guess export names or read runtime
  sources. I did not read the host beyond its header and its `export` lines, and I built
  nothing on them.
- **How I got past it:** I didn't. The Sakura library, its 31 tests on `js` and `wasm` and an
  offline sample runner are done. The Node consumer (quote round trip, observed suspension
  and resumption, no-HTTP invalid input, host failure translation, two isolated invocations,
  denied network) is **not implemented and not verified**. The sample runner is a CLI
  `main`, which the brief says does not count as a library consumer.
- Triage (2026-10-21, Fable, design §17.293 PQ-1): API gap, still on `main` (the host has
  `runMain`/`runTests` and the encoders; nothing starts a `pub fn` with host-encoded
  arguments). In 1.0, **not a beta.1 gate**: "library entries" is a beta.N ABI MINOR after
  phase 10 (`entries()` and `start_entry` guest exports, one public host function
  `callEntry(host, name, args)`, one page), and this program's Node consumer is written
  against it as 1.0 evidence. Now the backends page says plainly that no such call exists
  yet. The owner may cut it from 1.0 instead.

## 2026-10-21: on WASM, `timeout` around `Http.stream` + `Http.read_chunk` fires at once under `Clock.virtual()` (backend parity bug)

- **Expected:** a fetch from `Http.scripted` that reads its body inside a 5-second
  `timeout` finishes, as it does on `--target js`.
- **What happened:** under `Clock.virtual()`, every test whose fetch read a body failed on
  `--target wasm` with `Timeout { limit: Duration(5000) }`. That was 23 of 32 tests. They all
  passed on JS. I reduced it in a scratch project to
  `timeout(Duration.seconds(5), fn() { Http.stream(req, fn(i) { Bytes.len(Http.read_chunk(i.body, 100)) }) })`.
  The results:

  | Case | JS | WASM |
  | --- | --- | --- |
  | `Clock.virtual()` + `Task.deterministic(7)` | ok | Timeout |
  | `Clock.virtual()` + `Task.host()` | ok | Timeout |
  | Same `timeout` around `Http.stream` that does not call `read_chunk` | ok | ok |
  | Same `timeout` around `Http.send` | ok | ok |
  | Same `timeout` around pure work | ok | ok |
  | `Clock.host()` + `Task.deterministic(7)` | ok | ok |

  It looks as if the WASM scheduler treats `read_chunk` as idle time, so the virtual clock
  jumps to the timer.
- **How I got past it:** tests whose fetch reads a body install `Clock.host()` through one
  helper, `body_clock()` in `src/quote_tests.sak`, with a comment pointing here. The brief
  asks for virtual time, so this is a deviation. An in-memory fetch never gets near 5 s of
  wall time, but these tests no longer use a virtual clock. Test 24 (the timeout itself) still
  runs under `Clock.virtual()`. It passes on both backends because its delay comes before the
  stream. I deleted a planned case, "a 4 s delayed fetch still completes in virtual time". It
  cannot pass on WASM 0.1.0.
- Triage (2026-10-21, Fable, design §17.293 PQ-2): bug (JS/WASM divergence), still on
  `main`; **beta-blocking**. Root cause: the guest scheduler (`rt/src/exec.rs` `next`)
  advances virtual time whenever no task is runnable, even while a host request such as the
  async `http_stream_read` is in flight; the JS runtime ticks only after every resolved
  promise. Fix: a sleeper pops only when every live request is a virtual sleeper, else the
  guest reports `PENDING` and the host delivers its completions first. No ABI change; parity
  fixture `virtual_timeout_stream_read.sak`. Lane C. The `Clock.host()` workaround here
  stays until this program's migration.

## 2026-10-21: `Cli.exit` is a defect on `--target wasm` in 0.1.0

- **Expected:** `sakura run --target wasm` returns the code `Cli.exit` gave. The header of
  the emitted `sakura_wasm_host.js` says "or the code `Cli.exit` gave", and the website says
  WASM builds, runs and tests programs as JS does.
- **What happened:** every nonzero exit from the sample runner printed
  `{"kind":"defect","code":"D090","message":"builtin cli_exit is not supported by the WASM backend yet"}`
  and exited 70. Stdout was correct.
- **How I got past it:** I didn't work around it. The runner's exit codes (2, 3, 4 and 64)
  are verified on JS only. On WASM its output matches but the exit code is 70. The library and
  its tests do not use `Cli`.
- Triage (2026-10-21, Fable, design §17.293 PQ-3): duplicate of §17.292 (1); bug, still on
  `main`, **beta-blocking** there (one builtin row, no ABI change). Lane A.

## 2026-10-21: the website stdlib documents `main`, not 0.1.0 (`Payload`, redirects)

- **Expected:** the stdlib page linked from the brief matches the installed release, or marks
  which entries are main-only.
- **What happened:** `/docs/stdlib` lists `type Payload = Empty | Text(String) | Bytes(Bytes)`
  in `std/request` and `Response.body: Payload`. On 0.1.0, `import std/request.{Payload}` is
  `V1002`, and `sakura doc std::response::Response` shows `body: String`. Only the "Breaking on
  main" section of the changes page says this. As a result I could not script a response body
  that is invalid UTF-8. The capabilities tour also says `std/http` follows redirects and checks
  `Http.scoped` on every hop. On 0.1.0, `Request` has no `redirect` field, and a scripted 302
  comes back to the caller as a 302.
- **How I got past it:** test 19 uses a small test handler, `raw_body`, that forwards
  `stream` to the outer scripted handler and replaces the bytes `read_chunk` returns. Test 25b
  asserts the 0.1.0 behaviour: a redirect is the explicit failure `CarrierStatus { status: 302 }`.
  **Open question I could not check offline:** does `Http.host()` on 0.1.0 let Node's `fetch`
  follow redirects itself? If it does, a deployment's `Http.scoped` list would not be checked
  on later hops.
- Triage (2026-10-21, Fable, design §17.293 PQ-4): fixed on `main` (`Payload` and
  `Request.redirect` are CHANGELOG migration items 1 and 3; test 25b's 302 assertion is a
  migration hunk, `Redirect.Manual` keeps it); the docs-track-`main` half is §17.292 (14).
  The open question is answered: on 0.1.0 `Http.host()` fetched with `redirect: "manual"`
  and its own loop checked every hop's `host:port` against the manifest's `net` before
  fetching it (phase 0, design §17.262 (4)); `Http.scoped` saw only the first URL there,
  and on `main` checks every hop in the Sakura loop (§17.263 (4)). No unchecked hop.

## 2026-10-21: `sakura doc` cannot find `Http.scoped` by the name code uses

- **Expected:** `sakura doc Http.scoped` (or `std::http::Http.scoped`) shows the function,
  because code calls it as `Http.scoped(...)`. Operations work that way:
  `sakura doc std::http::Http.stream` works.
- **What happened:** `Http.scoped`, `std::http::Http.scoped`, `Http.recording` and
  `Http.scripted` all print `no symbol matches`. So do `--search scoped` and
  `--search "List<String>"`. A function in an effect's module that is not an operation is
  documented as `std::http::scoped`. `--search` only matches whole type patterns: the exact
  `"fn(List<String>) -> Handler<Http, {Http}>"` finds it.
- **How I got past it:** I read the website's `/docs/stdlib.md`, then used
  `sakura doc std::http::scoped`.
- Triage (2026-10-21, Fable, design §17.293 PQ-5): duplicate of §17.292 (11) (`Module.fn`
  resolves through the mapping hole candidates already use); bug, still on `main`;
  post-beta, first in the beta.N queue.

## 2026-10-21: importing names from a module does not bind the module, and the fix hint is wrong

- **Expected:** after `import parcel_quotes/policy.{InputError, …}`, a call to
  `policy.validate_parcel(...)` resolves, or the diagnostic suggests
  `import parcel_quotes/policy`.
- **What happened:** the call failed with `V1002 cannot find policy`. The help line said
  ``Add `import std/retry.{policy}` ``, which points at an unrelated std function called
  `policy`. For `carrier` it gave no hint at all.
- **How I got past it:** I import each of my modules twice: `import parcel_quotes/policy` and
  `import parcel_quotes/policy.{…}`.
- Triage (2026-10-21, Fable, design §17.293 PQ-6): the binding rule is by design (design
  line 705: `import m.{…}` binds the names, `import m` the module); the hint is a bug, still
  on `main` (`std_import_fix` never asks whether the name is one of the program's own
  modules, so a `MachineApplicable` fix is wrong and `sakura fix` would apply it);
  post-beta, beta.N with §17.292 (11): a package module match wins and offers
  `import parcel_quotes/policy`. Lane B after its gates.

## 2026-10-21: `sakura prompt` points at repository files and "main", not the installed release

- **Expected:** a guide that matches the installed 0.1.0 binary, as `sakura prompt --help`
  says ("matched to this compiler").
- **What happened:** the guide says it "tracks main, including Unreleased changes after the
  published alpha.1 preview". It tells the reader to look up signatures in
  `compiler/std/INDEX.md` and design decisions in `docs/design.md` and `docs/plans/*.md`. An
  installed user has none of these files. It was not clear which of the APIs it lists exist
  in 0.1.0. `Payload` and redirect following do not (see above).
- **How I got past it:** I checked every API with `sakura doc` and `sakura check` before
  relying on it.
- Triage (2026-10-21, Fable, design §17.293 PQ-7): duplicate of §17.292 (14), extended: the
  prompt (`docs/llms.txt`) opens with the binary's version and sends the reader to
  `sakura doc` and the website where it names `compiler/std/INDEX.md`, `docs/design.md` and
  `docs/plans/*.md`. Doc gap, with the beta's website sweep (lane D).

## 2026-10-21: the test-default `Clock.fixed` makes any `timeout` that yields fire immediately

- **Expected:** a library function that wraps its fetch in `timeout` can be tested under the
  default test handlers once `Http` and `Task` are installed.
- **What happened:** under the default `Clock.fixed`, the timer's `sleep` returns at once. So
  `timeout(5 s, stream + read_chunk)` fails with `Timeout` on both backends. This follows from
  "sleep returns at once under `Clock.fixed`", but the testing and concurrency pages do not
  warn about it.
- **How I got past it:** every test that calls `quote` installs a clock: `Clock.virtual()`, or
  `Clock.host()` for the WASM reason above.
- Triage (2026-10-21, Fable, design §17.293 PQ-8): by design (`timeout` is a race against
  `Clock.sleep`, which returns at once under `fixed`); doc gap: `timeout`'s doc comment
  (lane B, `std/task.sak`) and the testing page's `Clock` default row say so and point at
  `Clock.virtual()`.

## 2026-10-21: per-field typed errors from an `Untrusted` record need `trust`

- **Expected:** validating an `Untrusted<ParcelInput>` reports which field failed and its
  value (`InvalidWeight { grams }`, `UnknownZone { zone }`) without an escape hatch.
- **What happened:** `Untrusted.check` gives `Result<T, Violation>` for one predicate over the
  whole record. `Untrusted.validate` needs a refined target type and reports only a
  `Violation` with a string summary. Neither lets me read the rejected field as data.
- **How I got past it:** `validate_parcel` checks the weight predicate first and works on the
  plain record after that. To echo a rejected weight it uses one audited
  `@trust("the weight is only echoed back in InvalidWeight, never used for pricing")`.
  `sakura audit` lists it.
- Triage (2026-10-21, Fable, design §17.293 PQ-9): by design. A typed error that carries a
  rejected untrusted value is the program's audited choice, which is what `@trust` with a
  reason and `sakura audit` are for; the capabilities page shows this pattern (lane D).

## 2026-10-21: `Json.decode` errors carry host-engine text and an empty path

- **Expected:** a stable, backend-neutral `DecodeError` for malformed JSON, with a location.
- **What happened:** truncated input gives `DecodeError { path: "", message: "Unexpected end
  of JSON input" }`. That is V8's `JSON.parse` message, and it was the same on JS and WASM in
  this run. A wrong shape gives `expected an object, found an array`, which reads well.
- **How I got past it:** `BadJson` passes `path` and `message` through. The sample runner
  leaves out an empty path. Tests match `BadJson { .. }` and do not check the message text.
- Triage (2026-10-21, Fable, design §17.293 PQ-10): duplicate of §17.292 (2); bug, still on
  `main`, **beta-blocking** there: every syntax error is `malformed JSON` on every backend,
  no location (V8 gives none a program can rely on). Lane A.

## 2026-10-21: smaller items

- `sakura init --kind lib` always writes `AGENTS.md` and `CLAUDE.md`, and has no flag to skip
  them. I deleted them at the end as instructed. It also silently derived the package name
  `parcel_quotes` from the directory `parcel-quotes`. That is fine, but it was not announced.
- `sakura test --record` prints Node's `ExperimentalWarning: The flag --allow-net is under
  experimental phase` on stderr.
- Not friction: check, fmt and test each took about 0.2–0.3 s, with no slow tooling.
  `Http.recording` + `--record` with a scripted outer handler produced a correct request key on
  the first try, as `/docs/agents/cassettes` describes.
- Triage (2026-10-21, Fable, design §17.293 PQ-11): `init`'s files are by design (§17.292's
  small item); printing the derived package name is cosmetic, beta.N; the Node warning is
  MT-11f of the maintenance-triage log, a one-line `--disable-warning` in lane C.

## 2026-10-21 — Upgrade to 1.0.0-beta.1 (Claude Opus 5.5)

- Item 3 (redirects followed) was the one required change, and it is a behaviour change
  that compiles: without the list, test 25b's failure (`CarrierUnreachable { reason:
  "rates.example:443 is outside Http.scoped" }`) is the only signal. A library with no
  redirect test would silently start following redirects. The list told me exactly what to do,
  but it says "choose `Manual`" without naming `std/request.{Redirect}` or the record-update
  form, since every builder sets `Follow` and none takes a `redirect` argument:
  `Request { ..Request.of(Method.Get, url), redirect: Redirect.Manual }`. One example line
  in item 3 would save the `sakura doc` lookup.
- The sample runner's malformed-JSON line now reads `rate document: malformed JSON: malformed
  JSON`: the runner prefixes "malformed JSON" and the decoder's message is now the same words
  (Fixed: "`Json.parse` and `Json.decode` fail any syntax error with … `malformed JSON`").
  No test pins it, so no hunk was needed; it is output drift that a Fixed entry caused and the
  migration list cannot mention. Left as is.
- The WASM virtual-clock fix works: `body_clock()` is `Clock.virtual()` again and all 31 tests
  pass on `--target wasm`. `Cli.exit` codes are right on WASM too (2, 4, 64).
- `W1003` found an unused import (`Quote`) that I missed when reading for item 14; the warning's
  help says exactly what to remove.
