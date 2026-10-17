# WS7 Phase B documentation verification

Initially verified 2026-10-18 against `15179e8`, then rebased onto `202ec6d`
and rebuilt/rechecked before publishing, in the
`wt/docs2` worktree. Documentation and verification scripts were AI-assisted by
GPT-6-Astra. No compiler implementation was changed. This is local evidence on
macOS, not a declaration that the plan's cross-platform M3 exit gates have passed.

## Build and executable documentation

From `compiler/`, built the CLI with:

```sh
CARGO_TARGET_DIR=/Users/aniruddhdubge/.t3/scratch/sakura-worktrees/docs2/compiler/target cargo build -p sakura_cli
```

From the repository root:

```sh
python3 website/scripts/check-doc-samples.py
```

Result: **19 Sakura fenced snippets checked; 5 additional run/test invocations;
0 unexpected failures**. The two negative snippets deliberately produce `V6001`
and `V6003`; the typed-hole example is check-only. Declaration/function examples
are type-checked, not presented as runtime coverage. The generator's std signature
blocks are API listings, not standalone programs; std checking and tests cover
their owning modules.

The script isolates each snippet in a temporary project under this worktree's
`compiler/target`, invokes the built binary, and removes its own temporary files.
The cassette snippet uses a copy of `examples/secrets/tests/fixtures`.

## CLI and std evidence

Run from `compiler/`, with the same target directory:

```sh
CARGO_TARGET_DIR=/Users/aniruddhdubge/.t3/scratch/sakura-worktrees/docs2/compiler/target cargo test -p sakura_cli --test examples --test cli
target/debug/sakura test --std --json
```

- CLI integration target: **19 passed**. This includes audit/doc, build manifest,
  recording permissions/fixture writes, startup permission refusal, frozen specs
  and scaffolding checks.
- Examples target: **2 Rust tests passed**, including the complete twelve-project
  corpus: seven run plus test, five test, none check-only. The fixture provenance
  READMEs identify the `llm_extract`, `agent_loop` and `secrets` cassettes as synthetic.
- Std: **101 passed, 0 failed, 0 skipped**.
- Direct CLI probes confirmed `audit src`, `audit --json src`,
  `doc --search "fn(List<A>) -> Int"`, `doc std::list::len`,
  `doc --json std::list::len`, empty search results, and `build --manifest src`.
  The documented audit example produces exactly `total`, `from`, `trust` sites.
  `--out-dir custom-js` moves JS output but leaves the manifest at
  `target/sakura/manifest.json`. Local `compiler.git` is null; current obligations
  are `runtime_checked`.
- `test --help` includes `--record`, but not `--contracts`.
  `test --contracts` exits **2**, reporting an unexpected argument.
  `build --help` exposes only the `js` target and no `--release` flag.
- A direct synthetic recording probe used `Http.scripted` outside `Http.recording`:
  `test --record` without network grants exited **75**; with a declared host it
  wrote a cassette and passed. The cassette contained neither the test secret
  marker nor `secret_headers`. Replacing the scripted responses with `[]` and
  running ordinary `test` replayed it successfully. No live request was needed.

## Language evidence

Direct `sakura check --json` runs on the existing UI fixtures observed:

- `V6001_capability_construction.sakura`: `V6001`, exit 1.
- `V6002_scoped_value_escape.sakura` and
  `V6002_spawn_captures_inner_with.sakura`: `V6002`, exit 1.
- `V6003_resource_escape.sakura` and
  `V6003_route_handler_returns_file.sakura`: `V6003`, exit 1.
- `V5003_validate_into_plain_type.sakura`: `V5003`, exit 1.
- `V5004_literal_violates_refinement.sakura`: `V5004`, exit 1.
- `W5005_linear_contract_on_hot_path.sakura`: `W5005`, exit 0.

Direct `check` and `run` on the semantic fixtures established:

- `literal_refinement_decided.sakura`: no warnings; prints
  `notes/today.txt 404 ada@example.com`.
- `literal_refinement_checked_at_startup.sakura`: check succeeds without warnings;
  run exits **70** with `D010`, before the first `main` print.
- `literal_refinement_passes_once.sakura`: checks quietly and prints `sakura`
  three times. The once-at-initialization lowering is also specified by §17.87;
  output alone does not measure the number of checks.
- `validate_untrusted_into_refined.sakura`: checks and runs, producing the expected
  success and rejection cases for `Percent` and `Slug`.

Additional isolated checker probes confirmed `V5003` for an unknown target, a
wrong-base target and a non-call use of `Untrusted.validate`; `V5004` for a literal
whose refinement divides by zero; no warning for the canonical `Email`; and
`V6003` for a callback taking a record containing a `File`.

The updated HTML composition and HTTP attenuation snippets execute as tests.
Std tests additionally cover direct untrusted HTML interpolation and `Html.from`.
The scalar evaluator's fragment and 10,000-step limit were cross-checked with
§17.87, the implementation and the plan's WS1b deviation notes; this is not SMT
verification.

## Verified gaps and documentation corrections

1. **Audit literal inventory:** both undecidable literal fixtures above yield
   zero audit sites. Design §17.78 requires a `from` site with `reason: "literal"`.
   The guide documents the omission instead of claiming this has landed.
2. **Direct untrusted SQL interpolation:** the old capability-tour `search`
   snippet fails with `V2032` (`Untrusted<String>` lacks `Schema`). The index's
   SQL deviation text overstates current support. A separate probe validating
   to a refined string before interpolation checks successfully. The tour now
   shows a checked string parameter and explains this boundary.
3. **Generic refinement sample:** the previous `top_score(NonEmpty<Int>)` example
   produces `V2032`/`V2001` around the unsubstituted `T`. The replacement uses the
   concrete `NonEmptyScores = List<Int> where ...` and checks with the expected
   `W5005` warning.
4. **Filesystem fake composition:** an attempted `Fs.memory` + `Dir.subdir` +
   `Fs.rooted` example type-checks but fails at runtime with `FsError` saying the
   directory has no host access. It was not published; the tour instead tests
   HTTP attenuation. This does not establish that host filesystem narrowing fails.
5. **Streaming HTTP:** `std/http.sakura` has only `send`; `Http.stream`,
   `Http.read_chunk`, `Body` and `Incoming` remain in the index's Planned section.
   The documentation leaves §17.88 planned. Existing LLM SSE callbacks are not
   represented as incremental HTTP transport support.
6. **Generated contract tests:** `--contracts` is absent. It stays explicitly
   planned, including in the M3 gate list.
7. **LLM recording:** `Llm.replay` installs an empty scripted outer HTTP handler;
   `--record` cannot make that handler fetch a missing exchange. Live recording
   requires an explicit provider/recording/host composition. The guide also
   discloses that each replay operation reopens the cassette.
8. **M4 status:** research and `runtime/rt/ABI.md` exist. The plan permits isolated
   R1–R3, L1–L2 and V1–V2 work before the M3 gates; it does not establish completed
   milestones or available `verify`, LIR-emission or WASM CLI commands.

## Rebase verification

Upstream advanced by eight commits during this work. Rebased onto `202ec6d`
without conflicts, rebuilt the CLI, and reran all 19 documentation snippets,
the five run/test invocations, CLI integration/examples, all 101 std tests and
the website checks. All passed again. The WS3 deviation notes also corrected an
old roadmap/changelog statement about whole-program lowering.

```sh
CARGO_TARGET_DIR=/Users/aniruddhdubge/.t3/scratch/sakura-worktrees/docs2/compiler/target cargo test -p sakura_analysis --test recheck
```

This additional test passed: a private-body edit parses/lowers/checks one module;
a body reachable from a refinement predicate invalidates dependent analysis.
No streaming HTTP or `--contracts` implementation arrived in those commits.

## Website and links

From `website/`, all exited 0:

```sh
bun install --frozen-lockfile
bun scripts/generate-stdlib.mjs
bun run fmt:check
bun run lint
bun run typecheck
bun run build
```

The std page copies the index's Prelude and Signatures sections, with a generated
header explaining limitations. It excludes implementation-deviation prose that
has not been validated. Regeneration preserves the exact signature text.

The build emitted two CSS optimizer warnings about the `::highlight(fd-search)`
pseudo-element; it still compiled and generated all routes successfully.

Served that production build on this task's own `127.0.0.1:43177`. An HTTP crawl
starting at `/docs` followed **1,478 internal link occurrences across 29 URLs**;
all returned 200 and every linked fragment existed in the rendered target HTML.
The crawl included all four new guides, both changed tours, the std page, roadmap,
`llms.txt`, `llms-full.txt`, and the linked Markdown export. External URLs were not
crawled. README relative file links were checked against this checkout.

Not run as part of this documentation change: full compiler clippy/workspace-test
suites, 2,000-program differential testing, ten-minute fuzz targets, Ubuntu CI,
live-provider recording, M4 implementation tests or named maintainer security
review. No M3 gate completion or live-provider guarantee is inferred from these
local results.

Publishing also invokes the repository's pre-push hook. It checks an export of
the committed compiler using locked offline Cargo metadata, `cargo fmt --all --
--check`, and `cargo check --workspace --all-targets --locked --offline`. Those
checks are distinct from the full compiler test and clippy gates above.
