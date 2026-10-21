# Friction log: maintenance-triage

Author: Claude Opus 5.5, fresh session, no repository checkout. Built against
`sakura 0.1.0 (2014e821b87a0d30cb8061b6158f99723f0f676d)`. The brief names
`v0.1.0-alpha.1`, but the installed release is `0.1.0`. Sources: the
installed CLI, www.sakuralang.com and BRIEF.md.

Items are ordered by impact.

## 2026-10-21: `Llm.anthropic` reads `ANTHROPIC_BASE_URL` from the host process, even in tests

- **Expected:** `Llm.anthropic` would send to `https://api.anthropic.com/v1/messages`,
  as every cassette on the website shows. Under `sakura test`, the default
  `Env.of(Map.empty())` would hide host variables, and `sakura run` would read
  only the manifest's `env = ["ANTHROPIC_API_KEY"]`.
- **What happened:** This machine sets `ANTHROPIC_BASE_URL=http://127.0.0.1:8317`
  (a local proxy). Every cassette test failed with
  `ProviderError { status: 0, reason: "no recording for POST http://127.0.0.1:8317/v1/messages in two_turn.jsonl; run sakura test --record" }`.
  The adapter reads the process environment directly. It bypasses the test's
  `Env` handler, the program's `Env.only([...])` and the manifest's `env` list.
  The same happens under `sakura run --replay` (exit 4, cassette miss). Test
  results therefore depend on the machine. Neither the website, `sakura prompt`
  nor `sakura doc std::llm::anthropic` mentions the variable.
- **How I got past it:** I run every `sakura test` and `sakura run` with
  `env -u ANTHROPIC_BASE_URL`, and recorded this in both READMEs. A live run
  is still safe only because `net` allows `api.anthropic.com:443` alone, so a
  redirected base URL would be refused rather than reached. I did not check
  that, because a live call is not authorised.
- Triage (2026-10-21, Fable, design §17.293 MT-1): bug, security-relevant, still on `main`
  (`sakura_runtime.js` line 4870 reads the process environment around `Env`, the manifest's
  `env` grant and the test's empty environment, and the cassette key carries the URL);
  **beta-blocking**, Breaking since 0.1.0. The read is deleted; `Llm.anthropic` always sends
  to `api.anthropic.com` and the new `Llm.anthropic_at(base, key, model)` takes the endpoint
  as a value (read it under your own `Env` if you want it from the environment). Lane A.

## 2026-10-21: The provider answer shape differs from `Llm.scripted`, and the error message is confusing

- **Expected:** The final-answer text in a provider cassette would be the
  answer's JSON, as with `Llm.scripted(["...answer json..."])`.
- **What happened:** The adapter's `output_config` wraps the schema as
  `{"value": <T>}`. A bare record in the cassette failed with
  `InvalidOutput { violations: [Violation { type_name: "output", clause: "expected an object, found a undefined", ... }] }`.
  "found a undefined" is leaked JavaScript wording, and it does not say that a
  `value` field is missing. The website's only hint is the `{"value":4}` in the
  `Int` recipes. The wrapping is never explained, and `Llm.scripted` takes the
  unwrapped value.
- **How I got past it:** I read `output_config` out of a recorded request body,
  then wrapped provider answers with a `provider_answer` helper.
- Triage (2026-10-21, Fable, design §17.293 MT-2): bug (the message) and doc gap, still on
  `main`; rides with MT-1 in lane A. A missing `value` field fails `InvalidOutput` with
  `expected an object with a "value" field` ("a undefined" goes), and the doc comments of
  `anthropic` and `scripted` and the cassettes page say which shape each takes. The shapes
  themselves stay: the API needs the wrapper and `scripted` takes the Sakura-side value.

## 2026-10-21: Refinements are not sent to the model in the JSON Schemas

- **Expected:** The output schema and tool `input_schema` sent to the provider
  would carry the refinements that can be expressed, such as `maxLength: 500`
  on the summary, `maxItems: 5` on citations, and the ID's length and pattern.
- **What happened:** The recorded schemas contain only `{"type":"string"}` and
  `{"type":"array"}`. The refinements are enforced after the fact:
  `InvalidOutput` for the answer, `ToolError(tag: "DecodeError")` for tool
  arguments. That is correct and safe, but the model is never told the limits,
  so a live model is likelier to waste a turn.
- **How I got past it:** I restated the limits in the fixed system
  instruction. The checks themselves come from decoding (tests 7, 8 and 11).
- Triage (2026-10-21, Fable, design §17.293 MT-3): API gap, still on `main` (a refinement is
  a compiled predicate with no structure the schema could carry); post-beta (beta.N,
  additive): the compiler lowers recognisable `where` conjuncts to `maxLength`, `maxItems`,
  `minimum`, `pattern` and the like. Now: the MCP page's "refinements included" sentence is
  corrected, since the refinement is checked on decode, not sent to the model.

## 2026-10-21: Recording multi-turn synthetic cassettes needs a hand-written `Http` handler

- **Expected:** The cassette docs say the request keys were "generated by
  executing the program with a scripted outer HTTP response under recording
  mode". I expected `Http.scripted` to support that for a multi-turn agent.
- **What happened:** `Http.scripted` always returns the first matching entry,
  and `Matcher` matches only `Any`, a path or a method. Every agent turn is
  `POST /v1/messages`, so it cannot return a tool turn followed by an answer.
  No docs show how the multi-turn fixtures (`platform.jsonl`, `tasks.jsonl`)
  were made.
- **How I got past it:** I wrote `synthetic_provider(bodies)`, a test `Http`
  handler that keeps a counter and returns the bodies in order. A
  `regenerate ...` test runs under `sakura test --record --filter regenerate`.
  A cassette must be deleted before it is regenerated, because the in-order
  handler sees only misses. This is documented in `tests/fixtures/README.md`.
- Triage (2026-10-21, Fable, design §17.293 MT-4): API gap, still on `main`; post-beta
  (beta.N, additive): `Http.sequence(script)` answers each entry once, in order; `scripted`
  stays first-match because the retry recipes rely on it. Now, with the website sweep: the
  cassettes page shows the in-order handler recipe and the delete-before-regenerate step, and
  `agent_loop` and `llm_extract` become cookbook pages so the page links them on the site.

## 2026-10-21: `sakura doc` needs a project and cannot find effect operations

- **Expected:** `sakura doc std::agent` (or any std symbol) would work offline
  from any directory, and `sakura doc std::fs::with_file` or
  `std::log::event` would document those operations.
- **What happened:** Outside a project, every query printed
  `error: no .sak files found`. Inside one, operations are not symbols: `std::log::event`,
  `std::fs::with_file`, `std::fs::Fs::with_file` and `--search "with_file"` all
  return `no symbol matches`. Only `sakura doc std::fs::Fs` prints the effect,
  with all nine operations on one line. Module paths such as `std::log` and
  `std::prompt` also return `no symbol matches`, and `Tool.of` (std::tool::of)
  is in neither `sakura doc` nor the stdlib page's `std/tool` listing.
- **How I got past it:** I ran `sakura init` first, then looked up the effect
  declarations and read the website's `/docs/stdlib.md` for module listings.
- Triage (2026-10-21, Fable, design §17.293 MT-5): duplicate of §17.292 (11) (`sakura doc`
  on std without a project, `Module.op` name forms, module paths as queries); bug, still on
  `main`; post-beta, first in the beta.N queue.

## 2026-10-21: Agent loop failure tags are undocumented

- **Expected:** The docs would list the `ToolError.tag` values `Agent.run`
  produces for a tool the model invents, or for arguments that fail the
  parameter's refinement.
- **What happened:** These are `tag: "UnknownTool"` (payload `Null`) and
  `tag: "DecodeError"` (payload with `path` and `message`). I found them only
  by printing results in a probe test. The recipes show only a user error tag.
- **How I got past it:** I added a probe test, then wrote tests 11, 12 and 22
  against the observed tags.
- Triage (2026-10-21, Fable, design §17.293 MT-6): doc gap; the tags are `UnknownTool`
  (payload `Null`) and `DecodeError` (payload `path`, `message`), stable; one sentence in
  `Agent.run`'s doc comment names them. Lane B's rider (`std/agent.sak`).

## 2026-10-21: Budget estimates are not observable

- **Expected:** The brief asks for token and cost limits chosen against the
  fixture's request estimates, so I expected a way to see the adapter's
  per-request estimate.
- **What happened:** No API or doc gives the estimate. The budgeted recipe uses
  5,000 tokens and $0.10 without explaining how they were derived.
- **How I got past it:** I probed limits in a throwaway test. The two-turn
  fixture fails at 2,500 tokens and passes at 3,000, and fails at $0.02 and
  passes at $0.05. Test 30 records both rejections and shows that the chosen
  live limits (20,000 tokens, $0.25, 4 calls) admit the three-turn fixture.
  These numbers are tied to the adapter's estimator and may change between
  releases.
- Triage (2026-10-21, Fable, design §17.293 MT-7): doc gap. The estimate is
  `ceil(request bytes / 4) + max_tokens` tokens at the model's price; `Llm.anthropic`'s doc
  comment states it (with MT-1 in lane A) and the cassettes page shows a `Budget` handler
  that logs each reservation. No API: a wrapping handler is the Sakura way to observe it.

## 2026-10-21: No bounded read API

- **Expected:** The brief bounds every read (65,536 and 8,192 bytes). I
  expected something like `Fs.read_text(path, limit)`.
- **What happened:** `Fs.read` and `Fs.read_text` take no limit.
  (`Response.text` has one.)
- **How I got past it:** I wrote `read_bounded`, which loops
  `Fs.with_file(path, Mode.Read, ...)` and `Fs.read_chunk(file, limit + 1 - total)`
  and fails `TooLarge` past the limit, followed by `String.from_bytes`. That
  makes about 30 lines for a common need.
- Triage (2026-10-21, Fable, design §17.293 MT-8): API gap, still on `main`; post-beta
  (beta.N, additive): `Fs.read_up_to(path, limit) -> Bytes`, a std function over `with_file`
  and `read_chunk` that fails `FsError` past the limit. `read` and `read_text` keep their
  shape (an operation change breaks every user `Fs` handler and no fix can choose a limit).

## 2026-10-21: Log fields render as `Json` `Show`, and `Json` has no integer variant

- **Expected:** `Json.Int(n)` would exist, and log fields would print as
  `hits=3 id=drill-press-belt-slip`.
- **What happened:** `Json.Int` is `V1002`. The constructors are `Null`,
  `Bool`, `Number(Float)`, `String`, `Array` and `Object`, which the
  `sakura prompt` guide does not list. Captured and stderr log lines read
  `Info manual.search hits=Number(3.0)` and
  `Info manual.fetch id=String("drill-press-belt-slip")`, which an operator
  would find noisy.
- **How I got past it:** I used `Json.Number(Int.to_float(n))` and asserted
  the exact rendered strings in tests 2 and 29.
- Triage (2026-10-21, Fable, design §17.293 MT-9): `Json.Int` is by design (one number
  type, as JSON); the rendering is a bug, still on `main`, **beta-blocking** because its
  text is what tests assert and so it is now or never, Breaking since 0.1.0: `Log.stderr`
  and `Log.captured` render a field as compact JSON (`hits=3 id="drill-press-belt-slip"`);
  `Log.json` is unchanged. Migration: tests 2 and 29 match `hits=3`. Lane A.

## 2026-10-21: `Console` cannot write to stderr

- **Expected:** A way to print the concise failure line on stderr, leaving
  stdout for the JSON record only.
- **What happened:** `Console` has only `print` and `read_line`.
- **How I got past it:** The failure line goes through
  `Log.event(Level.Error, ...)`, which `sakura run` sends to stderr with an
  `Error ` prefix. The exit code still carries the failure class.
- Triage (2026-10-21, Fable, design §17.293 MT-10): duplicate of §17.292 (9),
  `Console.eprint`; **beta-blocking** there (an ABI MINOR on the chain). Lane A.

## 2026-10-21: Smaller API gaps and papercuts

- **`Result.ok` is missing.** There is no `Result -> Option`. I changed the
  handler to store `EntryId` values directly, so no re-validation is needed.
- **Error tags need explicit imports in `catch`.** Catching `LlmError` members
  in `catch` needed explicit imports of `Refused`, `InvalidOutput`,
  `Truncated`, `ProviderError` (std/llm), `RateLimited` (std/http), `Timeout`
  (std/task) and `BudgetExceeded` and `BudgetKind` (std/budget). The `V1002`
  and `V1003` help lines named each import exactly, which made this quick.
- **Escaped braces make JSON fixtures noisy.** JSON literals in strings need
  `\{` for every brace, so hand-written tool calls and answers are hard to
  read, and failure output shows Show-escaped braces (`ToolError \{ tool: ...`).
  I built answers with `Json.encode` of a test record type instead.
- **Cookbook links lead nowhere on the site.** The cassette page cites the
  `agent_loop` and `llm_extract` examples, but
  `/docs/cookbook/agent_loop(.md)` and `/docs/cookbook/llm_extract` are 404.
  Those examples exist only on GitHub, which is out of bounds without a
  checkout.
- **Docs track main, not 0.1.0.** Every tour page says "these docs track main
  ([changes since 0.1.0])". Recipe pages say "From the repository root, with
  this checkout's `sakura`", which assumes a checkout I did not have. Nothing I
  used was missing from 0.1.0, but I could not tell in advance.
- **`--record` prints a Node warning.** `sakura test --record` prints a Node
  `ExperimentalWarning: The flag --allow-net is under experimental phase` on
  stderr.
- Triage (2026-10-21, Fable, design §17.293 MT-11): `Result.ok` is an API gap, beta.N
  (additive `Result.ok`/`Result.err` to `Option`); the explicit imports are by design (design
  line 705; the help lines name them); the escaped braces are §17.292's cosmetic item,
  beta.N; the cookbook links are a doc gap fixed with MT-4 (the two examples become cookbook
  pages); "docs track main" is §17.292 (14), the `sakura prompt` and website version sweep;
  the Node warning is a cosmetic bug, `--disable-warning=ExperimentalWarning` beside
  `--allow-net` in `sakura_cli/src/testing.rs`, lane C's rider.

## 2026-10-21: Environment: the `sakura` CLI is a Node script, so a broken Node breaks even `--help`

- **Expected:** `sakura init --help` would work regardless of the JS runtime.
- **What happened:** `/opt/homebrew/bin/sakura` is a symlink to
  `lib/node_modules/sakuralang/bin/sakura.js`. While another process ran a
  Homebrew upgrade, `node` failed with
  `dyld: Library not loaded: libsimdutf.36.dylib`, and `sakura init --help`
  aborted with exit 134 and that dyld trace. This is not a Sakura bug, but
  nothing in the error says the CLI needs Node.
- **How I got past it:** I waited until Homebrew finished relinking Node
  (26.12.0), about a minute.
- Triage (2026-10-21, Fable, design §17.293 MT-12): by design; the npm `sakura` is a Node
  launcher, and the installer and the Homebrew tap ship the Rust binary, which `--help`s
  without Node. Not a Sakura defect; no change.

## Not friction

The compiler's own diagnostics were consistently good. `V3001` named the call
that introduced each tag and suggested `try`/`catch`. `V1002` and `V1003`
offered exact imports. The first full `sakura check` of about 400 lines
reported three distinct problems, all with direct fixes. `sakura check` runs
in under 0.1 s and `sakura test` (31 tests) in about 1.5 s on js. Wasm parity
held for all 31 tests and the CLI run.

## 2026-10-21 — Upgrade to 1.0.0-beta.1 (Claude Opus 5.5)

- The migration list's items 10 and 11 were the whole upgrade; see MIGRATION.md. The
  first item of this log is fixed: with `ANTHROPIC_BASE_URL` set, the replay and all 31
  tests pass, so the `env -u` workaround leaves both READMEs.
- Item 11's hunk was already on `main`, put there by the compiler change that made the
  break (`ab662859`), so the corpus on `main` was no longer "the program as written on
  v0.1.0". That change edited the two `expect_output` lists by hand and left the file failing
  `sakura fmt --check`; the upgrade had to run `sakura fmt` to get back to clean. A compiler
  change that edits `dogfood/` should run the program's `fmt --check` too, or leave the
  edit to the upgrade.
- `Console.eprint` now exists, so the `Log.event(Level.Error, …)` workaround for stderr
  (and its fixed `Error ` prefix) could go; not changed here, since the list does not require
  it and the CLI's stderr text is part of its tested behaviour.
