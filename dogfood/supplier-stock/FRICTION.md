# FRICTION: supplier-stock on installed sakura 0.1.0

Author: Claude Opus 5.5, one fresh session with no repository checkout.
Toolchain: `sakura 0.1.0 (2014e821b87a0d30cb8061b6158f99723f0f676d)` at
`/opt/homebrew/bin/sakura`. Sources: `sakura prompt`, `sakura doc`, `sakura setup`
(dry run only), the public website's `.md` pages and BRIEF.md.

Final state: `sakura check` reports no problems, `sakura fmt --check` passes,
`sakura test` passes 27 of 27 on `js`, and `sakura test --target wasm` passes only
10 of 27 (item 12). The brief's requirement that both `js` and `wasm` pass is
**not met** on 0.1.0.

## Steps skipped

- **2026-10-21: The brief's version is not the installed one.** The brief assumes the
  installed `v0.1.0-alpha.1`, but `v0.1.0` was installed. I recorded no alpha.1
  failures because I never ran alpha.1. Test 18 checks immediate transport failure
  on 0.1.0; the alpha.1 cut-body bug does not apply to this build.
- **2026-10-21: Beta steps skipped (they need a beta binary).** I did no
  1.0.0-beta.1 migration and no MIGRATION.md-driven edits; that half of "Delivery
  and release evidence" needs a beta binary. Astra's review step is also out of
  scope.
- **2026-10-21: Built outside the repository.** The brief places the project at
  `dogfood/supplier-stock/` in the repository. This session had no checkout, so the
  project is at `/tmp/sakura-dogfood/supplier-stock/`.
- **2026-10-21: I could not read the setup skill.** The brief says Opus starts from
  "its setup skill". `sakura setup` without `--apply` only lists the paths it would
  create (`.claude/skills/sakura/SKILL.md`, …); it does not print their contents. I
  was limited to a dry run, so I never read the skill and relied on `sakura prompt`
  and `sakura doc`.

## Friction items

1. **2026-10-21: `sakura doc` needs a project and cannot list a module.**
   - Expected: `sakura doc std::http` (or `std/http`) to list the module's API from
     any directory.
   - Happened: in an empty directory, every query fails with
     `error: no .sak files found`. Inside a project, `std::http` and `std/http` give
     `no symbol matches`. Only fuzzy names (`Http`, `scripted`) and full symbol paths
     (`std::http::Incoming`) work.
   - Got past it: ran `sakura init` first, then queried about 80 names in a loop and
     grepped the `std::…` lines out of the output to build my own index.

2. **2026-10-21: The guide and website track main, not 0.1.0.**
   - Expected: docs that match the installed binary.
   - Happened: `sakura prompt` says it "tracks main, including Unreleased changes".
     The website's concurrency page lists `Task.cancel`/`Task.cancelled`, and its
     cassette page describes `Response.bytes` and `Payload` bodies. The 0.1.0
     `sakura doc` output shows none of these: `Response.body` is a `String`, and
     `Task` has no `cancel`.
   - Got past it: checked every signature against `sakura doc` before using it, and
     read the website's "Changes since 0.1.0" page to see which items are main-only.

3. **2026-10-21: `Url.to_string` does not take a `Url`.**
   - Expected: `Url.to_string(req.url)` to give the text of the URL.
   - Happened: `V2001 expected UrlParts, found Url`. `Url` is a refined `String`, and
     `to_string` is defined over `UrlParts`.
   - Got past it: interpolated it directly, `"{req.url}"`.

4. **2026-10-21: A refinement's `where` cannot start a new line.**
   - Expected: `pub type Sku = String` followed by an indented `where …` line to parse,
     since signatures may break before `requires`.
   - Happened: `V0001 expected an item …, found 'where'`. That parse failure then
     caused a misleading `V5003 … String is not [a refined type]` at each
     `Untrusted.validate` site. `sakura fmt` itself breaks long predicates across
     lines, but after `where`, at the `&&`s.
   - Got past it: put `where` on the `type` line.

5. **2026-10-21: `error` cannot be a record field name.**
   - Expected: a report field named `error`, the obvious JSON key.
   - Happened: `V0001 expected an error name, found ':'`, because `error` is a keyword
     there.
   - Got past it: named the fields `failure` (supplier lines) and `input_error`
     (input-failure JSON). Keywords therefore shape the report's JSON keys.

6. **2026-10-21: `Violation` needs an import.**
   - Expected: `Violation` to be usable with `Untrusted.validate`, since the guide
     uses `Result<V, Violation>` freely.
   - Happened: `V1003 cannot find type Violation`.
   - Got past it: the help line offered `import std/core.{Violation}`, and that
     worked.

7. **2026-10-21: `Violation.clause` is the literal `"where"`.**
   - Expected: per `sakura doc std::core::Violation`, "`clause` is the source text of
     the failing conjunct" (for example `it <= 100_000`).
   - Happened: on 0.1.0, `clause` is always `"where"` for my refinements, so messages
     read `Stock requires where, got 100001`.
   - Got past it: messages now say `100001 is not a valid Stock` and do not use
     `clause`. Tests assert that wording.

8. **2026-10-21: JSON decode messages are confusing and depend on the host.**
   - Expected: a clear message for an out-of-range integer, and the same syntax-error
     text on every backend.
   - Happened: decoding `1e300` into an `Int` field gives
     `expected an integer, found an integer`. A syntax error passes through V8's
     message (`Expected ',' or ']' after array element in JSON at position 5 …`), so
     the text depends on the host. Unknown object keys are accepted silently.
   - Got past it: test 12 asserts only the tag (`InvalidFeed`), and the
     wanted-file syntax test asserts only the `wanted file at '':` prefix.
   - Open gap: extra keys in a feed record are ignored, not rejected. The brief does
     not require rejecting them, and I did not add a strict-key check.

9. **2026-10-21: Handler order silently changes which `Clock` a user handler uses.**
   - Expected: a `Clock.sleep` inside my pass-through `handler Http` to use the
     test's `Clock.virtual()`.
   - Happened: with `with Http.scripted(…), spy(100), Clock.virtual() { … }`, the
     handler's `Clock` resolved to the test default `Clock.fixed`. The sleep returned
     at once, and `Clock.monotonic()` read 0 with no diagnostic. Handlers bind their
     own effects to whatever is outside them where they are installed.
   - Got past it: every test installs `Clock.virtual()`, `Task.deterministic(7)` and
     `Random.seeded(42)` first (outermost).

10. **2026-10-21: `timeout` fires immediately under the default test clock.**
    - Expected: `timeout(5s, …)` around a scripted `Http.stream` to return the body
      in a test that did not install a clock.
    - Happened: under the default `Clock.fixed`, `sleep(5s)` returns immediately. Any
      action that suspends (the stream read) loses the race, so the test fails with
      `Timeout { limit: 5000 }`. This follows from the documented semantics, but
      nothing warns about it.
    - Got past it: `Clock.virtual()` in every test that reaches `fetch_offers`.

11. **2026-10-21: `Http.scripted` cannot replay a sequence.**
    - Expected: a way to script 429 then 200.
    - Happened: `Http.scripted` returns "the response of the first matching entry"
      every time.
    - Got past it: test 15 uses a cassette (`supplier-a-429-then-200.jsonl`);
      identical requests replay in recorded order. Test 16 (three 429s) uses
      `scripted`, which simply repeats.

12. **2026-10-21: On WASM, `Clock.virtual` treats a fiber blocked in `Http.read_chunk`
    as idle (a backend bug that blocks the wasm requirement).**
    - Expected: `sakura test --target wasm` to behave like `--target js`.
    - Happened: under `Clock.virtual()` on wasm, a fiber waiting in `Http.read_chunk`
      (from a scripted or cassette body) does not count as runnable. Virtual time
      jumps to the next sleeper first, so every `timeout(5s, …)` around the adapter
      fires at 5000 ms, and `Task.race` picks the sleeping branch. 17 of 27 tests
      fail on wasm, and each failure is this timeout; the 10 pure or input-only tests
      pass. JS passes all 27. A real `sakura run --target wasm` on the host clock
      works (the replay demo exits 0 with the same report). I reduced it in a scratch
      project:
      ```sakura
      fn get_one() -> Int ! {Http, Fail<HttpError>} {
        Http.stream(Request.of(Method.Get, Url.from("https://a.invalid/x")),
          fn(i) { Bytes.len(Http.read_chunk(i.body, 100)) })
      }
      test "stream timeout virtual" {   // js: ok; wasm: fails with Timeout { limit: Duration(5000) }
        with Clock.virtual(), Task.deterministic(7), Http.scripted([(Any, Response.status(200, "hi"))]) {
          expect_eq(timeout(Duration.seconds(5), fn() { get_one() }), 2)
        }
      }
      ```
      In the same reduction, `Http.send` inside `timeout` passes on wasm, and so does
      `Http.stream` whose callback does not call `read_chunk`. A scope with a stream
      reader and a 5 s sleeper joins the reader at `Duration(5000)` on wasm.
    - Got past it: **I did not.** I did not swap `Task.timeout` for a homemade race or
      drop the virtual clock: the brief requires both, and the race shows the same
      bug. The wasm half of "CI must pass on js and wasm" is reported as failing.

13. **2026-10-21: Snapshot names are checked only at run time.**
    - Expected: `expect_snapshot("healthy report", …)` to be accepted, or rejected by
      `check`.
    - Happened: `check` passed, and the test failed at run time with
      `snapshot name … must be letters, digits, '_', '-' or '.'`.
    - Got past it: renamed it to `healthy-report`.

14. **2026-10-21: Invalid UTF-8 cannot be scripted on 0.1.0.**
    - Expected: an `Http.scripted` response whose body holds invalid bytes.
    - Happened: on 0.1.0, `Response.body` is a `String`, so a scripted body is always
      valid UTF-8.
    - Got past it: a cassette line with `response_body_base64` (bytes `FF FE`). That
      field is documented on the main-tracking website page, and 0.1.0 replays it
      through `Http.stream` as the raw bytes. Test 11 covers it.

15. **2026-10-21: A scope refusal looks like any other transport failure.**
    - Expected: some typed way to tell a refused request apart.
    - Happened: `Http.scoped` refuses with `HttpError { status: None, reason: "… is
      outside Http.scoped" }`, the same shape as a DNS or connection failure or a
      cassette miss. The only difference is the reason text.
    - Got past it: the adapter maps all of them to `TransportFailed { reason }`.
      Test 22 checks the reason substring and that the permissive outer handler logs
      nothing.

16. **2026-10-21: No latch or condition primitive for the "gated" test.**
    - Expected: test 20 asks for "a gated response". I expected a latch or condition
      to build that gate on.
    - Happened: 0.1.0 has no channel or condition primitive, and a handler can only
      wait by sleeping or yielding.
    - Got past it: the gate is a pass-through `handler Http` that logs `start`, holds
      the request 100 ms on virtual time, then logs `release` and forwards. Test 20
      asserts that all three `start` lines come before any `release`, and that total
      virtual time is 100 ms rather than 300. The gate releases on a timer, not on a
      signal.

17. **2026-10-21: No bounded `read_text`.**
    - Expected: a way to read at most N bytes of a file as text.
    - Happened: `Fs.read_text` returns `Untrusted<String>`, but it reads the whole
      file. A bounded read needs `Fs.with_file` + `Fs.read_chunk`, which return
      `Bytes`.
    - Got past it: `read_wanted` reads at most 65,537 bytes, and `parse_wanted`
      decodes UTF-8 with `String.from_bytes`. Fields go through `Untrusted.validate`
      after decoding into raw records, not through `read_text`'s `Untrusted` wrapper.

18. **2026-10-21: Brief items that live in the guide only, not in the CLI.**
    - Expected: CLI support for the brief's release-provenance and budget items.
    - Happened: the gap register `G01`, `G06` and `G07` that the brief cites is a
      repository file (`../website-gaps.md`); I could not read it without a checkout.
      The program uses only its own explicit byte limits, as the brief asks.
    - Got past it: did not consult the register.

## Design notes (not toolchain friction)

- **The reason codes do not distinguish "no supplier answered".** When every feed
  fails, each item's reason is `not_offered`. `complete: false` and
  `failed_suppliers` keep this from looking like zero stock (test 23), but the
  per-item reason does not separate "nobody lists it" from "nobody answered".
- **`--replay` is an addition to the brief.** It serves each supplier from
  `data/cassettes/<id>.jsonl` (inside the `./data` read grant), wrapped in
  `Http.scripted([])` so nothing reaches the network. It exists so the real process
  can show exit 0 with the synthetic `.invalid` endpoints. A live run against them
  exits 1 with three `transport` entries (`getaddrinfo ENOTFOUND`).
