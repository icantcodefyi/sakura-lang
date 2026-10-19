# Contributing to Sakura Lang

Sakura Lang is in **pre-alpha / design phase**. Research, design discussion, documentation,
and the website are useful contributions today. The Rust compiler workspace in `compiler/` builds from source and executes on JavaScript.
The `0.1.0-alpha.1` preview is published; there is no stable compatibility guarantee yet.
Start with the [design](docs/design.md), [research](research/),
[stability policy](STABILITY.md), and [Code of Conduct](CODE_OF_CONDUCT.md).

**Contents:** [dev setup](#dev-setup-in-5-commands) ·
[website](#website) · [repository map](#repository-map) ·
[propose a change](#propose-a-change) · [tests and snapshots](#tests-and-snapshots) ·
[commits](#commits-and-pull-request-titles) · [AI assistance](#ai-assisted-contributions) ·
[automation](#automation-and-bootstrap-policy) · [license](#license-for-contributions)

## Dev setup in 5 commands

With rustup, Node 26 and Bun 1.4.3 installed:

```sh
git clone https://github.com/icantcodefyi/sakuralang.git && cd sakuralang
git config core.hooksPath .githooks
(cd compiler && cargo build -p sakura_cli && cargo test --workspace --locked)
python3 website/scripts/check-doc-sweep.py
(cd website && bun install --frozen-lockfile && bun run dev)
```

The pre-push hook checks that `compiler/Cargo.lock` matches the committed manifests,
the same check CI runs with `--locked`. The doc sweep uses `compiler/target/debug/sakura`
unless `SAKURA_BIN` names another build.

## Website

Use **Bun 1.4.3**, as pinned by `website/package.json`. The website uses Next.js
and Fumadocs; read [website/AGENTS.md](website/AGENTS.md) and the installed Next.js
guides before changing it. No credentials are required for local lint/build.
Commit `website/bun.lock` alongside intentional dependency changes; do not replace
it with another package manager's lockfile.

From `website/`, the current checks are:

```sh
bun run fmt:check   # oxfmt (run `bun run fmt` to fix)
bun run lint        # oxlint, warnings denied
bun run typecheck
bun run build
```

For browser behavior, also exercise the affected route, navigation, and responsive
states and describe what you actually checked.

## Repository map

- `research/`: evidence and prior-art analysis; recommendations are not language rules.
- `docs/design.md`: evolving language design, including open questions.
- `docs/rfcs/`: substantive language and policy proposals and their dispositions.
- `docs/adr/`: internal engineering decisions and their rationale.
- `website/`: Bun / Next.js / Fumadocs website and documentation presentation.
- `.github/`: CI, contribution forms, ownership, and dependency automation.
- `compiler/`: Rust compiler workspace, CLI, JS runtime, std and example corpus.
  The pinned toolchain is `compiler/rust-toolchain.toml`; run Cargo commands there.

The design is provisional. Future `docs/spec/` text will define normative grammar
and semantics; do not claim research or a website example is already a conformance
specification. See [ADR 0002](docs/adr/0002-monorepo-layout.md).

## Propose a change

Search existing issues and proposals first. Open a focused issue with the problem,
concrete examples, and alternatives, or send a small PR for a clear fix. Keep
unrelated edits separate. Explain behavior, validation, compatibility, and any
unresolved concerns in the PR template. Ask for review from
[@icantcodefyi](https://github.com/icantcodefyi); semantics and security changes
need a named maintainer reviewer. Never include credentials or private user data.

Syntax, semantics, effects, module/package rules, diagnostic protocol breaks, and
stability-policy changes follow the [RFC process](docs/rfcs/README.md). Start with
an issue pitch, then a numbered proposal PR. The maintainer announces a **seven
calendar day final comment period**, records objections, and decides accepted,
rejected, or postponed. Acceptance does not mean implemented or stabilized.
Internal architecture/build decisions use an [ADR](docs/adr/README.md).

## Tests and snapshots

### What to check

Match checks to the behavior changed. Documentation changes need accurate links,
examples, and explicit pseudocode labels; website changes need lint/build plus
relevant manual checks. Workflow changes need YAML validation and `actionlint`.
Use the existing compiler test targets; do not claim a milestone is complete from one suite.

### Compiler

Install rustup and Node 26, then use the pinned development toolchain:

```sh
cd compiler
cargo build -p sakura_cli
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The dev profile defaults to `opt-level = 1` (with an explicit `insta` override), retaining
debug assertions and overflow checks. This matters for running the integration
suites, which invoke the dev compiler many times. For parallel local tests, install
cargo-nextest and use `cargo nextest run --workspace --locked`, followed by
`cargo test --workspace --doc --locked` (nextest does not run doctests).

Isolate std analysis caches between concurrent jobs: set `SAKURA_CACHE_DIR` to a
job-specific temporary directory. The compiler appends `sakuralang/std`; remove the
directory after the run. `SAKURA_NO_CACHE=1` disables this disk cache for comparison.
This cache is separate from the verifier proof cache. Use a separate
`CARGO_TARGET_DIR` for concurrent worktrees too.

The lockfile is `compiler/Cargo.lock`. For documentation claims, also exercise the
newly built `target/debug/sakura` and `cargo test -p sakura_cli --test examples`.
All twelve original examples execute in their configured modes (seven run plus
test and five test); passing that corpus does not close every milestone gate. See the [completion plan](docs/plans/m3-completion-and-m4.md).

### Docs

For the docs sweep, set `SAKURA_BIN` to the built binary and run
`python3 website/scripts/check-doc-samples.py` and
`python3 website/scripts/check-doc-sweep.py` from the repository root. The latter
checks the guide fences and shared contexts; `SAKURA_DOC_TARGET=wasm` also exercises
their tests on WASM. Declaration/pseudocode fragments are explicitly labelled and
reported separately. Regenerate with `cargo run -p agent_docs --locked` in
`compiler/`, then `bun scripts/generate-stdlib.mjs` in `website/`. After a production
build and `bun run start`, run `python3 website/scripts/check-doc-links.py
http://localhost:3000` from the root to check local file links and rendered docs
routes/anchors. External websites are not fetched by that check.

### Compiler test cases

Compiler changes should cover positive, rejection, boundary, and cross-feature
cases tied to the relevant spec section. Add deterministic regression cases for
bugs; malformed input must report an error without crashing or hanging. Check
structured diagnostic codes, spans, and fixes separately from terminal rendering.

### Snapshots

Review **every snapshot diff** before accepting it. Explain intentional output
changes in the PR. Normalize only irrelevant nondeterminism such as temporary
paths, never semantic content or diagnostic codes. New snapshots must fail CI
until reviewed and committed. Use the repository's snapshot harness when it exists;
the compiler uses checked-in fixtures and snapshots.

### Diagnostic codes

Diagnostic identities are recorded in [the append-only baseline](compiler/diagnostics/codes.txt)
and `sakura_diagnostics::codes::REGISTRY`. Add the same code and snake_case title
to both. Use `reserved` before implementation, `active` while emitted, and
`retired` when emission stops. Retain retired entries in both files. Never change
an assigned title or reuse a retired code. The compiler tests compare the table
with source references; the required `diagnostic-codes` CI job compares it with
the PR base or previous pushed commit, so deleting a baseline row also fails.
Run `python3 -m unittest discover -s compiler/diagnostics -v` and
`python3 compiler/diagnostics/check_history.py --base <full-base-commit-sha>`
from the repository root to check history locally. Title equality guards the
machine-readable identity; review must still reject a changed meaning hidden
behind an unchanged title.

## Commits and pull request titles

Use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):
`feat(parser): add match expressions`, `fix(types): preserve source spans`, or
`docs: clarify effects`. Supported types are `feat`, `fix`, `docs`, `style`,
`refactor`, `perf`, `test`, `build`, `ci`, `chore`, and `revert`; a scope is optional.
Use `!` in the title or `BREAKING CHANGE:` in the body and provide migration notes
for incompatible public behavior. PR titles are checked and become squash-merge
commit titles; temporary working commits need not each pass a title check.

Update [CHANGELOG.md](CHANGELOG.md) for user-visible changes and
[STABILITY.md](STABILITY.md) when compatibility promises change. There are no
release or package-publication credentials delegated to automation yet.

## AI-assisted contributions

AI assistance is welcome. Disclose the tool/model and what it did in your PR,
including generated code, prose, tests, and substantial review. The submitting
human remains responsible for understanding the contribution, licensing it,
checking sources, reviewing generated output, and responding to feedback.
Do not submit unattended bulk issues or PRs. **Never fabricate test claims**:
list exact commands run, outcomes, and checks that could not run. An assistant's
assertion is not evidence that a test passed. Do not weaken contracts, broaden
effects, or grant capabilities merely to silence a compiler error.

## Automation and bootstrap policy

### CI policy

CI runs on pushes to `main`, PRs, merge groups, manual dispatch and a daily schedule.
The first `changes` job compares main against its last successful push CI run,
so cancelling a compiler run and then pushing docs still selects compiler checks.
PRs compare against their merge base; merge groups compare against their base.
Renames count both paths. Missing history, unavailable baseline API results and
unknown paths conservatively run everything. Diagnostic history uses the same
baseline and fails closed when it cannot obtain one.

- Compiler/runtime/std, benchmark programs and editor changes run the full Rust,
  parity, native, MSRV and editor checks. Compiler changes also rebuild the website
  because its generated references depend on the compiler.
- Website changes run website, documentation and Tree-sitter corpus checks. Docs
  (`docs/`, `research/` and Markdown) select the same checks without the Rust matrix.
  Documentation examples build the current CLI in a separate Linux job.
- Workflow, tool and packaging changes run every check. The nightly and default
  manual runs force all areas on, including the scheduled native checks. A manual
  run may supply `base_sha` to check a specific commit range without push
  cancellation. Use a temporary ref to pin the commit being checked, then delete
  that ref after verification. This override never applies to pushes or nightlies.
- A newer push to `main` cancels the previous push run; PR runs cancel per PR.
  Each nightly has its own concurrency group and is never superseded by a push
  or another nightly. All jobs use public GitHub-hosted runners.

`ci-required` retains its branch-protection name. Change detection must succeed;
other jobs may succeed or be intentionally skipped. Failure and cancellation
always fail the aggregate.

Rust caches include the OS, architecture, toolchain and dependency hashes. Rust
partitions and parity share the compiler cache; the first partition per OS saves
it and parity/docs only restore it. Native and MSRV have separate cache namespaces.
Pinned nextest binaries and checksum-verified solver installs are cached by version,
OS and architecture. Tool version bumps invalidate their caches.

When evaluating CI changes, watch each probe with `gh run watch <run-id>
--exit-status`. Record run wall time from creation to the final job's completion,
including runner queueing. Measure setup from the first `Set up job` step to the
first check/build/test step, separately from queue time. Compare matching runner
labels and report cold cache misses, failures and sample counts alongside timings.

`ci-required` aggregates
website, Rust, MSRV, native, parity, Tree-sitter, and VS Code checks. The Rust jobs run in `compiler/`
using its pinned toolchain: fmt, clippy, workspace tests with cargo-nextest,
`cargo test --workspace --doc --locked`, and the agent-doc freshness check.
The nextest filter excludes only `sakura_cli::parity`: the required Parity matrix
runs that identical target for JS and WASM on both operating systems.
Ubuntu and macOS tests each use two nextest hash partitions; fmt, clippy,
doctests, and agent-doc freshness run in the first partition on each OS.
`ci-required` requires every selected Rust matrix child, including both partitions on each OS.
Nextest runs test binaries in parallel; its configuration serializes tests that
share solver scripts or WASM example output. The local gate remains
`cargo test --workspace --locked` (above).

During bootstrap, direct pushes are allowed: **PR reviews and required status
checks are not enforced**. The active `Protect the default branch` ruleset blocks force pushes
and deletion of `main`, with no bypass actors. When the project leaves
bootstrap, enable required `ci-required` checks and PR reviews before accepting
routine changes. A green gate is still expected now; investigate failures.

Actions are pinned to reviewed full commit SHAs; preserve those pins when updating
CI. Dependabot checks Actions and the
website's native Bun ecosystem weekly. GitHub supports the text `bun.lock` file
([supported ecosystems](https://docs.github.com/en/code-security/reference/supply-chain-security/supported-ecosystems-and-repositories)).
Cargo dependency updates are not yet configured in `.github/dependabot.yml`.
When added, they must target `/compiler`, where the manifest and lockfile live.
The required `msrv` job reads `workspace.package.rust-version` from
`compiler/Cargo.toml`, explicitly installs that toolchain and the WASM target,
and runs `cargo +<msrv> check --workspace --all-targets --locked` plus workspace
nextest tests on one public Linux runner. It excludes the same parity target as
the Rust jobs; the required Parity matrix still tests both backends. This job is
separate from the pinned development-toolchain checks and is required by
`ci-required`. To reproduce it locally, install the declared toolchain, the WASM
target, Node 26, and the pinned solvers used by CI, then run those commands from
`compiler/`.

## License for contributions

Unless you explicitly state otherwise and the maintainer agrees before inclusion,
contributions intentionally submitted for inclusion are licensed under
**MIT OR Apache-2.0**, at the recipient's choice, without additional terms.
You retain copyright in your contributions. Preserve third-party notices and
identify material with different terms. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE). No separate contributor agreement is required.
