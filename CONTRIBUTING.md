# Contributing to Sakura Lang

Sakura Lang is in **pre-alpha / design phase**. Research, design discussion, documentation,
and the website are useful contributions today. The Rust compiler workspace in `compiler/` builds from source and executes on JavaScript.
There is no supported release or stable compatibility guarantee yet.
Start with the [design](docs/design.md), [research](research/),
[stability policy](STABILITY.md), and [Code of Conduct](CODE_OF_CONDUCT.md).

## Git hooks

Run `git config core.hooksPath .githooks` once after cloning. The pre-push hook checks that
`compiler/Cargo.lock` matches the committed manifests, the same check CI runs with `--locked`.

## Set up the checkout

```sh
git clone https://github.com/icantcodefyi/sakuralang.git
cd sakuralang
cd website
bun install --frozen-lockfile
bun run dev
```

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

Match checks to the behavior changed. Documentation changes need accurate links,
examples, and explicit pseudocode labels; website changes need lint/build plus
relevant manual checks. Workflow changes need YAML validation and `actionlint`.
Use the existing compiler test targets; do not claim a milestone is complete from one suite.

Install rustup and Node 26, then use the pinned development toolchain:

```sh
cd compiler
cargo build -p sakura_cli
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The lockfile is `compiler/Cargo.lock`. For documentation claims, also exercise the
newly built `target/debug/sakura` and `cargo test -p sakura_cli --test examples`.
The twelve examples have distinct run/test/check modes; passing the corpus does
not imply every example executes. See the [completion plan](docs/plans/m3-completion-and-m4.md).

Compiler changes should cover positive, rejection, boundary, and cross-feature
cases tied to the relevant spec section. Add deterministic regression cases for
bugs; malformed input must report an error without crashing or hanging. Check
structured diagnostic codes, spans, and fixes separately from terminal rendering.

Review **every snapshot diff** before accepting it. Explain intentional output
changes in the PR. Normalize only irrelevant nondeterminism such as temporary
paths, never semantic content or diagnostic codes. New snapshots must fail CI
until reviewed and committed. Use the repository's snapshot harness when it exists;
the compiler uses checked-in fixtures and snapshots.

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

CI runs on pushes to `main`, PRs, and merge groups. `ci-required` aggregates
website, Rust, native, parity, and VS Code checks. The Rust jobs run in `compiler/`
using its pinned toolchain: fmt, clippy, workspace tests with cargo-nextest,
`cargo test --workspace --doc --locked`, and the agent-doc freshness check.
The nextest filter excludes only `sakura_cli::parity`: the required Parity matrix
runs that identical target for JS and WASM on both operating systems.
Ubuntu tests use two nextest hash partitions; fmt, clippy, doctests, and agent-doc
freshness run in the first. macOS runs the full filtered suite in one job.
`ci-required` requires every Rust matrix child, including both Ubuntu partitions.
Nextest runs test binaries in parallel; its configuration serializes tests that
share solver scripts or WASM example output. The local gate remains
`cargo test --workspace --locked` (above).

During bootstrap, direct pushes are allowed: **PR reviews and required status
checks are not enforced**. The active `Protect the default branch` ruleset blocks force pushes
and deletion of `main`, with no bypass actors. When the project leaves
bootstrap, enable required `ci-required` checks and PR reviews before accepting
routine changes. A green gate is still expected now; investigate failures.

Actions are pinned to major versions for bootstrap; **reviewed full commit SHA
pinning is required before the first release**. Dependabot checks Actions and the
website's native Bun ecosystem weekly. GitHub supports the text `bun.lock` file
([supported ecosystems](https://docs.github.com/en/code-security/reference/supply-chain-security/supported-ecosystems-and-repositories)).
Cargo dependency updates are not yet configured in `.github/dependabot.yml`.
When added, they must target `/compiler`, where the manifest and lockfile live.
Do not assume the pinned development toolchain establishes a separately tested MSRV.

## License for contributions

Unless you explicitly state otherwise and the maintainer agrees before inclusion,
contributions intentionally submitted for inclusion are licensed under
**MIT OR Apache-2.0**, at the recipient's choice, without additional terms.
You retain copyright in your contributions. Preserve third-party notices and
identify material with different terms. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE). No separate contributor agreement is required.
