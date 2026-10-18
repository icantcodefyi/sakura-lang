# M3 documentation verification

Follow-up to [gate audit item 5](m3-gate-report.md#follow-ups), 2026-10-18.
Verified against the source-built JavaScript CLI on `d300df0` in worktree `docs3`.
AI assistance: GPT-6-Astra audited claims, wrote the documentation and ran these checks;
the human contributor owns the result. No compiler implementation changed.

## Executed examples

Scratch packages were created under `/tmp`. The examples published in the tours and cassette
guide were executed, not just type checked:

- **Contract properties:** `sakura test --contracts --seed 42` on the documented broken
  `increment` exits 3 with `D021`, shrinks to `x = 0`, and writes the exact documented
  `tests/regressions/increment.json`. A second run reports `(stored regression replayed)`.
  Changing `{ x }` to `{ x + 1 }` passes, with the saved regression retained.
- **Recording and LLM replay:** the synthetic `Llm.recording` example creates a one-row
  `hello.jsonl` cassette with `sakura test --record`. Ordinary `sakura test` passes even after
  replacing the outer HTTP handler with `Http.scripted([])`. The placeholder secret is absent
  from the cassette. A separate two-row cassette with identical requests returned `hello`,
  `second`, `second` under both `Llm.recording` and `Llm.replay` in one installation.
- **Streaming HTTP:** the scripted `hello` body yields `hel`, then `lo`, then EOF when read
  with `Http.read_chunk(body, 3)` inside `Http.stream`.
- **Memory filesystem:** `Dir.subdir(Fs.root(), "notes")` plus `Fs.rooted` reads and writes
  the same store as the enclosing `Fs.memory`, with paths relative to the nested directory.
- **Untrusted templates:** SQL finds the ordinary title and returns zero rows for an injection
  string. HTML escapes `<script>` directly without the `Untrusted` wrapper. A separate
  `Untrusted<Html>` probe renders escaped shown text, not markup.
- **Generic refinements:** `NonEmpty<Int>` passes to `List.fold`; validating an empty
  `List<String>` into `NonEmpty<String>` returns `Err`.

Additional CLI probes confirmed that a contract's typed `Fail` is allowed, a zero-argument
`Log.captured()` contract handler works, a generic public function is omitted from synthesis,
and an unsatisfied `requires x == 123_456` gives up with `W7001` and exit 3.
Direct `Untrusted<String>` in a URL template still fails with `V2001`.

## Repeatable checks

From `compiler/`:

```sh
CARGO_TARGET_DIR=/Users/aniruddhdubge/.t3/scratch/sakura-worktrees/docs3/compiler/target cargo build -p sakura_cli
CARGO_TARGET_DIR=/Users/aniruddhdubge/.t3/scratch/sakura-worktrees/docs3/compiler/target cargo test -p sakura_cli --test examples
```

Both passed. The example harness reports 2 Rust tests passed, including the twelve-package
example corpus; this is not a full compiler workspace test run.

From the repository root:

```sh
python3 website/scripts/check-doc-samples.py
git diff --check
```

Both passed. The documentation helper checks 24 snippets and executes 14 run/test invocations,
including expected rejections, recording followed by offline replay, and the contract bug,
regression JSON comparison, replay and fix. Other testing-tour snippets that depend on earlier
fragments are outside this helper's coverage.

From `website/`, each command passed:

```sh
bun install --frozen-lockfile
bun run fmt:check
bun run lint
bun run typecheck
bun run build
```

The production build generated 88/88 pages. It emitted two CSS optimization warnings about
Fumadocs `::highlight` selectors. An HTML-parser check of the generated pages resolved 1,346
internal `/docs` links, including fragments, against rendered routes and element IDs with no
failures. Per-page `.md` rewrite links and external URLs were not included in that count.

## Limits retained in the guides

No live provider, real credentials, or host-network streaming was exercised. Recording uses
synthetic HTTP responses; this does not establish live-provider compatibility. Contract tests
remain testing, not proof, and the documented eligibility, filtering and filename limits remain.
The historical implementation plan and design proposals are not rewritten as current status.
This follow-up closes the stale documentation item, not the remaining M3 owner-review gates.
