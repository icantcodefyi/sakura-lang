# Dispatch Packlist: check a community pantry's delivery files

- Brief author: GPT-6-Astra (Codex family), 2026-10-19.
- Status: website-only brief, before implementation; no program or tests have run.
- Required domain: CLI over `Cli` and `Fs`.
- Intended program author: Claude Opus 5.5 (Claude family), in one fresh session with no repository checkout or path to one.
- Initial release: installed `v0.1.0-alpha.1`; actual version and build revision: pending implementation.
- Program header must record the actual author, fresh-session provenance and release built against. This brief is not program authorship.

## Purpose and user-visible behaviour

A pantry coordinator receives separate JSON packing lists from volunteers and needs
one trustworthy dispatch report before loading the van. The CLI reads only the
immediate files in a selected directory under `data`, validates each delivery, and
reports item totals, route totals and rejected files. It never edits the originals.

Each delivery has a unique delivery ID, a route and a list of item/quantity pairs.
Process filenames in ascending order; the first valid delivery with a given ID
counts, and subsequent duplicates are rejected. A malformed file does not hide
valid deliveries in other files. Render either readable text or a JSON report,
with the same information and deterministic ordering. Return success for an entirely
valid batch, status 1 for a completed batch containing rejections, and status 2 for
invalid arguments. Unrecoverable directory I/O fails explicitly without a success
report. These are application choices, not new language exit-code rules.

## Inputs, outputs and bounds

Arguments select a relative batch directory and `text` or `json` output. Input files
are UTF-8 JSON with IDs and route/item names of 1–64 UTF-8 bytes, quantities from
1 through 1,000 and at most 100 item lines. Permit at most 200 selected `.json`
files and 65,536 bytes per file. Ignore other extensions and subdirectories;
report the ignored count. Reject an over-limit batch before processing its files.

Bound file reads inside `Fs.with_file` with `Fs.read_chunk`; read at most the cap
plus one byte to distinguish an exact fit from oversize. Decode bytes only after
the bounded read, reporting invalid UTF-8 as a per-file rejection. Validate the
decoded record and collection bounds before aggregation. The maximum possible
aggregate is comfortably within both documented integer profiles.

Output includes accepted/rejected/ignored counts, sorted delivery IDs, item totals
per route, overall item totals and one concise rejection per rejected file. Never
dump raw rejected content. Preserve an input's existing value when constructing a
normalised record; trim names, reject an empty trimmed name, and retain case.

## Std surface and permissions

Use `Cli.args`/`Cli.parse`, `Fs.list`, bounded file callbacks, `Dir.subdir` and
`Fs.rooted`; `String.from_bytes`, `Json.decode`/`encode`, refinements, `List`/`Map`
and `Console` support parsing and reporting. Keep aggregation and rendering pure.
Translate expected parsing/validation failures into typed application errors and
per-file results; keep `CliError` and `FsError` distinguishable at their boundaries.
Use `Untrusted.of` plus a checked validation step where raw decoded text enters
the application, rather than an unexplained trust assertion.

The production manifest grants `fs.read` only to `./data`. No filesystem writes,
network, environment variables or subprocesses are needed. Narrow further to the
selected batch directory. Paths from arguments are validated, not forced through
a trusted constructor. This is the documented language-level boundary, not a claim
of OS sandboxing or protection against every host filesystem race.

## Acceptance: at least 20 separately named sakura test cases

Use `Cli.of`, `Fs.memory`, captured output and pure functions. Each numbered item
is a separate test, not merely another assertion inside one catch-all test.

1. One delivery with two items produces the exact route and overall totals.
2. Two routes sharing an item contribute to distinct route totals and one overall total.
3. Repeated item lines within one delivery are added together.
4. Duplicate delivery IDs in different files count only the first valid file in filename order.
5. An invalid earlier file does not reserve a delivery ID against a later valid file.
6. Permuting the filesystem listing leaves the full report unchanged.
7. A malformed JSON file is rejected while its valid neighbour still contributes.
8. A missing required field reports a field-oriented rejection without echoing the body.
9. Invalid UTF-8 produces a recoverable rejection and does not stop the batch.
10. A zero quantity is rejected, with no contribution to totals.
11. A quantity of 1,000 is accepted and 1,001 is rejected.
12. Empty or whitespace-only item names are rejected after trimming.
13. A multibyte name is measured in UTF-8 bytes at the 64-byte boundary.
14. A 100-line delivery passes; a 101-line delivery fails validation.
15. A file exactly at 65,536 bytes is admitted to decoding; a larger one is rejected before decoding.
16. A batch of 200 selected files passes the count guard; 201 fails before reading a file.
17. Subdirectories and non-JSON files are ignored and counted, without traversal.
18. A nonexistent selected directory returns `FsError`, with no dispatch-success output.
19. Missing or invalid CLI flags are rejected before filesystem operations.
20. An absolute or parent-traversing batch argument is rejected through validation without a contract defect.
21. Rooting the filesystem to a batch cannot read a sibling batch; a capability test verifies the refusal.
22. An empty directory produces a zero-count report with empty totals.
23. Text output has a reviewed snapshot and JSON output decodes to the same report values.
24. A mixed batch's application result requests status 1; a clean batch requests status 0, without invoking process exit inside the test.

Check actual CLI statuses and the denied-read manifest separately with the installed
binary; a simulated handler test is not proof of host permission enforcement.
There is no network service in this program, so network cassettes are inapplicable.

## Delivery and release evidence

Implement at `dogfood/dispatch-packlist/`, with at least 300 lines of `.sak` outside
tests and at least 20 tests. Use substantive parsing, validation, aggregation,
reporting and CLI separation, not padding. The fresh author may use only the
installed binary, its `sakura setup` skill and the website. Start on alpha.1 and
record failures honestly; do not substitute a source build. Acceptance requires CI
runs on both `js` and `wasm` against installed `v0.1.0`. Capture the alpha.1 → 0.1.0
changes against the published notes. After 1.0.0-beta.1 tags, Astra writes `MIGRATION.md`
from release notes alone before Opus changes the program; every migration hunk
must be accounted for. Astra's later reader review edits no program code.

## Non-goals and open friction

No delivery routing optimisation, file repair, recursive scanning, CSV parser,
database, live network, personal-address storage or filesystem-race security claim.
No padding to satisfy the corpus size. Website gaps G01 and G04 in
[the gap register](../website-gaps.md) affect release expectations and capability
documentation; alpha.1 does not gain main's filesystem fixes by reading this brief.

## Website evidence

Read on 2026-10-19, through the public site's Markdown exports:

- [CLI and files recipe](https://www.sakuralang.com/docs/cookbook/cli_files): argument decoding, directory narrowing and in-memory tests.
- [Refined JSON recipe](https://www.sakuralang.com/docs/cookbook/json_refined): recoverable schema/refinement rejection.
- [Std reference](https://www.sakuralang.com/docs/stdlib): file chunks, UTF-8 conversion, collections and CLI APIs.
- [Capabilities](https://www.sakuralang.com/docs/tour/capabilities): filesystem authority and untrusted data.
- [Effects](https://www.sakuralang.com/docs/tour/effects) and [testing](https://www.sakuralang.com/docs/tour/testing): typed errors and handler-based acceptance.
- [Install](https://www.sakuralang.com/docs/install), [changes](https://www.sakuralang.com/docs/changes) and [changelog](https://www.sakuralang.com/changelog.md): published baseline and migration caveats.

The workflow constraints above come from the explicitly supplied design decisions;
the language research comes only from these website pages. No repository language
sources were used to fill a website gap.
