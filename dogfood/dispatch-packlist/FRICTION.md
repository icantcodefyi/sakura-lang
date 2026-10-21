# Friction log: dispatch-packlist on sakura 0.1.0

Author: Claude Opus 5.5, fresh session, no repository checkout. Sources: the installed
`sakura 0.1.0 (2014e821b87a0d30cb8061b6158f99723f0f676d)`, its `sakura prompt`, `sakura doc`,
the `sakura setup` skill (applied only in a throwaway scratch project), and the public website's
Markdown exports. Items are ordered roughly by impact.

## 2026-10-21 — `Cli.exit` is a defect on `--target wasm`, and the docs say WASM is complete

- Expected: `Cli.exit(1)` / `Cli.exit(2)` to work on both CLI targets. The installed skill's
  `references/stdlib.md` says "Every module runs on the JavaScript and WebAssembly backends,
  except `std/crypto`", and neither the website stdlib page nor the changes page mentions `Cli.exit`.
- Happened: `sakura run --target wasm -- --batch clean` prints the usage message, then
  `{"schema_version":"1.0","kind":"defect","code":"D090","message":"builtin cli_exit is not supported by the WASM backend yet"}`
  and exits 70. Every non-zero status the brief asks for (1 for rejections, 2 for bad arguments)
  becomes 70 on WASM. Exit 0 and the escaping-`FsError` exit 1 are fine.
- Got past it: I did not. There is no other way to choose an exit code: a typed failure escaping
  `main` always exits 1. Tests are unaffected (they never call `Cli.exit`), so `sakura test
  --target wasm` passes 24/24, but the brief's "CI on both js and wasm" acceptance cannot hold for
  CLI statuses on 0.1.0. Recorded in the source header.
- Triage (2026-10-21, Fable, design §17.292 (1)): bug, still on `main`; **beta-blocking**. The
  guest runtime already has the exit outcome and both hosts decode it; one `rt_b_cli_exit`
  builtin row is missing. No ABI change. Lane A.

## 2026-10-21 — `Json.parse`/`Json.decode` syntax errors quote the input

- Expected: a `DecodeError` safe to show, given the brief's "never dump raw rejected content".
- Happened: syntax errors carry the JS engine's message, which echoes the body:
  `DecodeError { path: "", message: "Unexpected token 'o', \"not json at\"... is not valid JSON" }`.
  The text is also V8's, so it presumably differs per backend.
- Got past it: `decode_raw` calls `Json.parse` first and reports only `malformed JSON` on failure;
  only shape errors (`missing field`, `expected a string, found an integer`, with a JSON pointer
  path) are shown. Test 8 asserts the body never appears in text or JSON output.
- Triage (2026-10-21, Fable, design §17.292 (2)): bug, still on `main` (the Rust runtime's text
  differs from V8's too); **beta-blocking**. Every syntax error becomes `malformed JSON` (or
  `JSON nested too deeply`) on every backend: no offset, no token, no quotation. Lane A.

## 2026-10-21 — `Cli.parse` cannot take lowercase choices for a variant type

- Expected: `type Format = Text | Json` as a flag type to accept `--format json`, or the usage to
  list the choices.
- Happened: only the exact constructor name works (`--format Json`); `json` fails with
  `--format: unknown constructor "json" of Format`, and the usage says `--format <value>`.
- Got past it: `type FormatFlag = String where it == "text" || it == "json"`, mapped to a variant
  by hand. Its error, `"xml" does not satisfy the where clause of FormatFlag`, still does not name
  the allowed values.
- Triage (2026-10-21, Fable, design §17.292 (3)): API gap, still on `main`; post-beta (beta.N,
  additive): case-insensitive constructors, the usage lists them, the refined-flag message
  appends the clause text item 4's fix provides.

## 2026-10-21 — `Violation.clause` is always `"where"`

- Expected: per `sakura doc Violation`, "`clause` is the source text of the failing conjunct"
  (example: `clause: "it <= 100"`).
- Happened: `Path.validate("../x")` and `Untrusted.validate` into my
  `Name = String where String.len(it) >= 1 && String.len(it) <= 64` both give `clause: "where"`.
- Got past it: per-field reasons are chosen by the program (`Untrusted.len` decides "empty after
  trimming" vs "longer than 64 UTF-8 bytes") instead of being read from the violation.
- Triage (2026-10-21, Fable, design §17.292 (4)): bug, still on `main` (Core hard-codes the
  string); **beta-blocking**. `clause` becomes the failing top-level `&&` conjunct's source text,
  the whole predicate when there is no chain; `Violation` joins the prelude with it. Lane B.

## 2026-10-21 — `Path.validate` accepts paths that are not plain relative segments

- Expected: the relative-path refinement to be enough to validate a batch argument.
- Happened: it rejects `/x`, `../x`, `a/../b` and `""`, but accepts `a\..\b`, `C:\x`, `a/./b`,
  `a//b` and `a/`.
- Got past it: after `Path.validate`, `batch_segments` splits on `/` and rejects empty, `.`, `..`
  and backslash segments (test 20). `Dir.subdir` itself refuses `../c` and `/etc` with `FsError`,
  so this is defence in depth, but the argument error should be a usage error (status 2), not I/O.
- Triage (2026-10-21, Fable, design §17.292 (5)): bug, security-relevant, still on `main`;
  **beta-blocking**, Breaking since 0.1.0. `is_relative` refuses `\` and `:` (the host's
  containment check already stops the escape, but the refinement's promise is false on
  Windows); `a/./b`, `a//b` and `a/` stay accepted by design (`of`/`join` normalise). Lane B.

## 2026-10-21 — `Dir.subdir`'s name semantics are undocumented, and stepwise narrowing breaks under a narrower grant

- Expected: the docs to say whether `Dir.subdir(d, name)` takes one segment or a relative path.
- Happened: by experiment it takes a multi-segment path (`"data/clean"`) and refuses `..` and
  absolute names. My first version narrowed in two steps (`subdir(root, "data")`, then the batch).
  With the manifest narrowed to `fs.read = ["./data/clean"]`, the first step already fails
  (`FsError { path: "data", reason: "not in the manifest's fs.read list" }`) even though only
  `data/clean` is ever read.
- Got past it: one `Dir.subdir(Fs.root(), "data/" + batch)` call, which works under both the
  production `./data` grant and a batch-only grant.
- Triage (2026-10-21, Fable, design §17.292 (6)): doc gap (multi-segment `name`); the grant
  check on the opened directory is by design. One doc line, rides with item 5's commit.

## 2026-10-21 — `Fs.memory` cannot represent an empty directory

- Expected: some way to give acceptance test 22 an empty batch directory.
- Happened: `Fs.memory` takes only `(Path, Bytes)` files; `data/b/`, `data/b/.` and `data/b` all
  make a file, and `Fs.list(Path.of("data/b"))` then fails `not a directory`.
- Got past it: test 22 checks `assemble` over an empty listing, plus a directory whose only entry
  is an ignored subdirectory. The real empty case was run with the binary (`data/empty`: zero
  counts, exit 0).
- Triage (2026-10-21, Fable, design §17.292 (7)): API gap, still on `main`; post-beta (beta.N,
  additive constructor that also takes directories).

## 2026-10-21 — "Permuting the filesystem listing" cannot be done through `Fs.list`

- Expected: a way to feed a permuted listing (acceptance test 6).
- Happened: `Fs.list` is documented and observed as "Entries sorted by name", including under
  `Fs.memory`, whatever order the files are given in.
- Got past it: test 6 runs the batch with the `Fs.memory` list reversed and then calls the pure
  `assemble` with reversed and rotated outcome lists; `assemble` sorts by filename itself.
- Triage (2026-10-21, Fable, design §17.292 (8)): by design; sorted listings are the
  determinism contract. The workaround is the intended test.

## 2026-10-21 — No stderr in `Console`

- Expected: a way to print a usage error to stderr.
- Happened: `Console` has only `print` and `read_line`; `sakura doc --search stderr` finds nothing.
- Got past it: `Log.event(Level.Error, msg, [])` under `main`'s root `Log` handler, which prints
  `Error <message>` on stderr. The `Error ` prefix cannot be changed, and in tests these lines
  appear in the same captured output as `Console`.
- Triage (2026-10-21, Fable, design §17.292 (9)): API gap, still on `main`; **beta-blocking**
  because a new operation on an existing effect cannot land after the preparation commit:
  `Console.eprint(line)`, one ABI MINOR for the host import, test handlers capture it with
  `print`. Lane A.

## 2026-10-21 — Statuses 1 (rejections) and "I/O failure" share exit 1

- Expected: a way to keep `FsError` distinguishable at the process boundary.
- Happened: a typed failure escaping `main` always exits 1 with
  `error: main failed with FsError { ... }`, the same code the brief gives a batch with
  rejections. Choosing another code would need `Cli.exit`, which is the WASM defect above.
- Got past it: kept the `FsError` propagation (the stderr record and the absent stdout report
  tell the cases apart) and documented it in the header. This follows from the brief's three codes.
- Triage (2026-10-21, Fable, design §17.292 (10)): by design and already in STABILITY.md's
  exit-code row (`1` is a failed `main`); item 1's fix is the real answer, and `Cli.exit`'s doc
  gains the sentence that says so.

## 2026-10-21 — `sakura doc` lookups are inconsistent and need a project

- Expected: `sakura doc Fs.rooted` to work like `sakura doc Fs.with_file`, from any directory.
- Happened: before `sakura init`, every lookup failed with `Error: no .sak files found`. After it,
  effect operations resolve (`Fs.with_file`, `Fs.list`, `Cli.exit`), but module functions do not:
  `Fs.rooted`, `Cli.parse`, `String.len`, `Json.encode`, `Untrusted.check`, `Dir.subdir` and
  `std/fs` all give `no symbol matches`. `std::fs::rooted` and `std::string::len` work, and the bare
  name `parse` finds `std::cli::parse`. `Path.validate` (generated for a refinement) has no entry
  in any form, nor does `Name.validate`. `sakura doc Fs` also lists unrelated std tests
  (`std::bytes::test:strict base64 ...`).
- Got past it: used `std::module::name` and the website stdlib page.
- Triage (2026-10-21, Fable, design §17.292 (11)): bug, still on `main`; post-beta, first in the
  beta.N queue (`Module.fn` names, generated `validate`/`from` as symbols, std lookups without
  a project, no std tests under `Fs`).

## 2026-10-21 — `Violation` is not in the prelude

- Expected: to annotate `Result<Path, Violation>` without an import, since every refined type's
  `validate` and `Untrusted.validate` return it and the prelude has `Untrusted`.
- Happened: `V1003 cannot find type Violation`.
- Got past it: the diagnostic's help (`Add import std/core.{Violation}`) was right. The website's
  `std/untrusted` signatures never say which module `Violation` lives in.
- Triage (2026-10-21, Fable, design §17.292 (12)): API gap, still on `main`; rides with item 4's
  commit (one prelude line; the explicit import stays valid, this program is not edited).

## 2026-10-21 — Duplicate JSON keys are accepted silently

- Expected: a rejection, or at least documented behaviour.
- Happened: `{"id":"a","id":"b",...}` decodes with `id: "b"` (last wins).
- Got past it: not worked around. A volunteer file with two `id` keys counts under the last
  one. Noted here rather than hand-parsing JSON.
- Triage (2026-10-21, Fable, design §17.292 (13)): by design, and the same on every backend
  (last value at the first position, as `JSON.parse` does); a doc sentence on `Json.parse` and
  `Json.decode` lands with lane A. No strict mode is planned.

## 2026-10-21 — Docs and brief target a different release than the installed binary

- Expected: `sakura prompt` to describe 0.1.0.
- Happened: it says "This reference tracks main, including Unreleased changes after the published
  alpha.1 preview", and the website says its docs track main, with a separate changes page. The
  brief says to start on `v0.1.0-alpha.1` and implement at `dogfood/dispatch-packlist/`, but only
  0.1.0 is installed and the task said to work in `/tmp/sakura-dogfood/dispatch-packlist/`.
- Got past it: built against 0.1.0 only, in the task's directory. No alpha.1 run took place, so no
  alpha.1 → 0.1.0 comparison was captured. I trusted the binary over the website where they
  differ. The WASM `Cli.exit` gap above was found by running the binary, not from either doc.
- Triage (2026-10-21, Fable, design §17.292 (14)): doc gap; `sakura prompt` names the binary's
  own version in the `0.2.0` → `1.0.0-beta.1` sweep (§17.291 (3f)); the website keeps
  "tracks `main`" with the changes page beside it.

## 2026-10-21 — Small things

- `sakura init` writes `AGENTS.md`, `CLAUDE.md`, `.gitattributes` and `.gitignore`, and
  `sakura setup --apply` would add about 40 files across `.claude/`, `.agents/`, `.cursor/` and
  `.codex/`. I removed the first three and applied the skill only in a scratch project, to keep
  this directory to the files the brief implies.
- A failing `expect_output` prints the expected and actual lists with Sakura-escaped braces
  (`Violation \{ type_name: ... \}`), which is hard to read.
- `sakura fmt` reorders error-union members alphabetically (`FsError | BatchTooLarge` becomes
  `BatchTooLarge | FsError`). This is fine, but it is not mentioned anywhere I read.
- Positive: `check`, `test` and `run` each took 0.1 to 0.4 s; the program checked clean on the
  first try, and 23 of 24 tests passed on the first run (the 24th only needed its new snapshot).
- Triage (2026-10-21, Fable, design §17.292, small things): `init`'s files are by design (the
  templates); the escaped braces are a cosmetic bug, beta.N; the union ordering is by design and
  gets one sentence on the `fmt` page in the sweep.

## 2026-10-21 — Upgrade to 1.0.0-beta.1 (Claude Opus 5.5)

- The migration list predicted no change and none was needed; see MIGRATION.md. Two of
  this log's beta-blocking items are visibly fixed on the published beta: `Cli.exit` gives
  1 and 2 on `--target wasm` (was D090, exit 70), and `--batch '..\x'` is refused by
  `Path.validate` itself.
- The list is about breaks only, so it does not say which old workarounds are now
  redundant (`decode_raw`'s `Json.parse` pre-check, the backslash segment check, the
  `import std/core.{Violation}` now that `Violation` is in the prelude). They stay valid;
  a reader finds them only by reading the `Fixed` section against their own code.
- The header's "statuses hold on js only" sentence became false with no compile signal; the
  only fix is noticing it. Small, but it is the kind of drift an upgrade leaves behind.
