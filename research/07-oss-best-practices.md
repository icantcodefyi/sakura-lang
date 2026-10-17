# Vow: engineering and open-source practices

Research date: **2026-10-17**. Scope: `github.com/icantcodefyi/vow`, a Rust compiler and bun/Next.js/Fumadocs website in one repository.

Repository observations below use default-branch commits dated on or before this date; the source register identifies those revisions. Recommendations are proposals for Vow, not claims about infrastructure already implemented. Public workflows reveal jobs, but do not establish which checks a repository requires through private branch settings. Tool documentation is linked separately.

## 1. Summary

- Keep one repository and one root Cargo workspace; begin with a compiler library, CLI, and small task runner. Add crates when architectural boundaries become real.
- Treat diagnostics as a public interface: test structured error data separately from terminal rendering, and version machine-readable output.
- Make every language rule traceable to specification text and positive/negative conformance cases; snapshots alone cannot establish correctness.
- Use fast deterministic tests on every PR, then scheduled fuzzing, broader compatibility checks, and controlled performance measurements.
- Pin development toolchains, commit lockfiles, declare and test the Rust MSRV, and inherit dependency versions and lints explicitly.
- Keep CI readable: a three-OS test matrix, Linux quality/security checks, website checks, and an aggregate required gate are enough initially.
- Use Conventional Commit PR titles and a reviewed release PR; let release-please manage versions/notes and cargo-dist package binaries.
- Separate research, accepted design, normative specification, tutorials, and implementation documentation; generate agent-readable docs from the same sources.
- Publish governance and AI contribution rules early. An agent-first product still needs accountable maintainers and reviewable, bounded contributions.
- Start with dual MIT/Apache-2.0 licensing and explicit pre-1.0 compatibility rules; defer editions and elaborate release infrastructure until they solve demonstrated problems.

## 2. Per-project observations

### Gleam

Gleam offers an approachable Rust compiler model: its [workspace][gleam-cargo] separates compiler core, CLI, language server, WASM, benchmarks, and integration-test helpers; shared dependencies include `insta`. [CONTRIBUTING][gleam-contrib] distinguishes compiler tests from language integration tests and makes the required external runtimes explicit. Its [CI][gleam-ci] uses platform/target matrices and `Swatinem/rust-cache`; [deny.toml][gleam-deny] makes dependency policy reviewable. A short [release checklist][gleam-release] connects documentation, version changes, tests, tagging, and a CI-created draft release. Borrow the coherent developer experience, including its separate [tree-sitter grammar][gleam-tree] and [VS Code extension][gleam-editor]. Its contribution guide rejects AI-agent contributions: Vow should study the engineering without assuming policy alignment.

### Rust

The [repository layout][rust-tree] distinguishes compiler, standard library, tools, and tests. Its [CI entry point][rust-ci] delegates job selection to `src/ci/citool` and a job-definition file: useful at Rust's scale, excessive for Vow initially. [Compiletest][compiletest] supports distinct suites for diagnostics, execution, incremental compilation, and code generation. [UI tests][rust-ui] pair source with expected stdout/stderr and explicitly reviewed `--bless` updates. The transferable principle is observable behavior with minimal reproducers, not copying rustc's bootstrap machinery. [RFCs][rust-rfcs] separate substantive design approval from implementation, with a ten-calendar-day final comment period. Rust's [agent instructions][rust-agents] also demonstrate that agent guidance can encode strict contribution-policy boundaries.

### Roc

The inspected [tree][roc-tree] is now Zig-based (`build.zig`, `src/`, `test/`), so older descriptions of Roc as a Rust Cargo workspace are unsuitable templates. Its [contributor guide][roc-contrib] documents build-step naming and machine-readable local CI reports. The [snapshot tool][roc-snapshots] separates canonical diagnostic semantics from presentation snapshots, particularly relevant to agent-facing error output. [Fuzzing documentation][roc-fuzz] explains AFL++ targets, extracting corpus seeds from snapshots, crash reproduction, minimization, and promoting failures into regression cases. Its [benchmark workflow][roc-bench] compares main and PR binaries and verifies the benchmark corpus. Borrow the testing discipline, not its current implementation language.

### Zig

The [GitHub README][zig-readme] explicitly says the project moved to Codeberg and that GitHub is **not mirrored**. Consequently, this report's GitHub observations are historical, not claims about current Zig CI. The retained [test harness][zig-tests] systematically varies targets, optimization modes, libc linkage, and backend settings; [build.zig.zon][zig-manifest] brings standalone and linking cases into the test build. This illustrates compiler validation across configurations rather than only host-machine unit tests. Vow should maintain an explicit distinction between targets it can compile for and targets whose binaries it actually executes in CI.

### Deno

Deno's [Cargo workspace][deno-cargo] and [contributor-facing agent guide][deno-agents] expose CLI, runtime, extensions, libraries, and several test layers. Spec tests use `__test__.jsonc` plus expected output; runtime, Node compatibility, and Web Platform Tests address different contracts. Crucially, the guide warns that some tests need Cargo's harness setup rather than direct runtime invocation. Its [generated CI][deno-ci] includes build and benchmark orchestration, while the [release guide][deno-release] gives a maintainer entry point. Vow should likewise provide one supported command for each test family, with fixtures and environment setup owned by that command.

### Biome

Biome's [manifest][biome-cargo] centralizes workspace dependencies and Rust/Clippy lints; `crates/`, `packages/`, and `xtask/` separate Rust services, distribution interfaces, and generation. [PR CI][biome-ci] runs linting, cargo-deny, tests, and documentation checks. Separate [parser-conformance CI][biome-conformance] and [fuzz harnesses][biome-fuzz] complement snapshots. [AGENTS.md][biome-agents] identifies generators, test selection, and changeset requirements rather than merely describing the project. [RELEASES.md][biome-release] distinguishes previews, betas, stable binaries, and publication of internal crates. Vow should similarly avoid treating its internal Rust crates as a stable SDK just because they share a repository with the CLI.

### Ruff

Ruff's [contributor guide][ruff-contrib], [agent guide][ruff-agents], and crate-specific guides make a large compiler/tooling workspace navigable. [Fuzz documentation][ruff-fuzz] describes parser/formatter properties and corpus maintenance; the [daily fuzz workflow][ruff-daily] makes continuing validation an operational responsibility. Its [distribution configuration][ruff-dist] pins cargo-dist and explicitly defines targets, installers, artifact attestations, and custom publication jobs. The [versioning policy][ruff-version] separates CLI behavior, preview features, internal crates, and source-build MSRV. Vow should copy the specificity of those contracts, without importing Ruff's release cadence or assuming its MSRV policy fits a new language.

### uv

uv's [workspace manifest][uv-cargo] combines a declared Rust version, shared dependencies, and workspace lints. Its concise [AGENTS.md][uv-agents] tells contributors to use existing snapshot helpers and normalize nondeterminism without concealing tested behavior. [Benchmark CI][uv-bench] separates building benchmark artifacts from measurement on designated hardware, with CodSpeed integration. [dist-workspace.toml][uv-dist] documents installers, attestations, target-specific details, and Linux compatibility baselines. The lesson for Vow is that distribution support includes ABI/OS minimums and installation behavior, while a performance claim needs reproducible workloads and environments—not only an attractive chart.

### rust-analyzer

The [architecture guide][ra-architecture] explains parser, syntax, semantic analysis, IDE features, server, editor client, and `xtask` responsibilities. Its [testing guide][ra-tests] uses compact textual fixtures, including multi-file scenarios; [CLAUDE.md][ra-agents] identifies `expect-test` snapshots and generated-code workflows. This is evidence against assuming every Rust project's snapshots use `insta`. The [release workflow][ra-release] packages both server binaries and platform-specific VS Code extensions. Its separate [AI policy][ra-ai] prohibits autonomous issue/PR submission. For Vow, reuse compiler analysis inside the LSP while keeping protocol tests separate from compiler tests.

### Bun

The inspected [contribution guide][bun-contrib] now describes Rust plus native dependencies; descriptions of Bun's old Zig implementation should not be presented as current. Its [agent guide][bun-agents] maps runtime, Node/web compatibility, CLI, bundler, and integration tests. Its most valuable warning: test the freshly built executable rather than the installed Bun, with a documented exception for declaration-only tests. The [Buildkite pipeline source][bun-ci] models build/test platforms and release/assertion/sanitizer profiles. Vow needs the same assurance that integration tests exercise the compiler produced by the checkout, and that release archives contain the binaries actually tested.

### Swift Evolution

Swift's [process][swift-process] separates a pitch from open community review and a recorded disposition. Experimental functionality is explicitly gated; implementation review is distinct from design approval. Its [repository][swift-tree] keeps proposals, templates, policies, releases, and vision documents discoverable. [Proposal-validation CI][swift-ci] treats proposal metadata as checkable infrastructure. Borrow explicit status, ownership, alternatives, compatibility discussion, and implementation tracking. Vow can do this within `docs/rfcs/`; it does not need Swift's workgroup structure or a separate repository on day one.

### Python PEPs

[PEP 1][pep1] distinguishes Standards Track, Informational, and Process proposals, assigns a champion, and records discussion and dissent. The PEP repository tests its documentation tooling through [test CI][pep-tests] and builds/publishes proposals through [render CI][pep-render]; prose infrastructure is maintained as software. [PEP 387][pep387] explicitly defines public compatibility and distinguishes soft deprecation from scheduled removal. Borrow those distinctions and durable decision records. Python's long deprecation timelines are evidence of a mature ecosystem's commitments, not a sensible schedule to impose unchanged on Vow's experimental releases.

### TypeScript

At the inspected revision, [.github/copilot-instructions.md][ts-agents] maps the native Go compiler to `tsc/internal`, JavaScript APIs to `packages/typescript`, and the editor extension to `packages/vscode-typescript`. Compiler fixtures live under `tsc/testdata/tests/cases`; generated local baselines are compared with committed references. Editor behavior requires language-server testing. [CI][ts-ci] separately checks builds, release packaging, extensions, and platform tests; [Herebyfile.mjs][ts-build] exposes their commands. [CONTRIBUTING][ts-contrib] allows disclosed, human-owned AI assistance but rejects bulk agent-driven contribution queues. Vow should preserve this separation between authoring assistance and maintainer accountability.

### Effect-TS

Effect's [package scripts][effect-package] separately expose runtime tests, type tests, documentation tests, builds, generated AI documentation, and performance tooling. Its [check workflow][effect-ci] checks multiple runtimes, declarations, docs, and generated outputs. [Changesets configuration][effect-changesets] defines a coordinated package version group; [release CI][effect-release] creates a release PR or publishes and avoids cancelling an active release. [.agents/AGENTS.md][effect-agents] maps each change type to appropriate validation and says `LLMS.md` is generated from `ai-docs/src`. Borrow tested examples and generated machine-readable documentation; retain bun for Vow rather than copying Effect's pnpm tooling.

## 3. Concrete recommendations for Vow

### Repository and workspace

Use this target layout, introducing deferred directories only when their implementation begins:

```text
vow/
  Cargo.toml, Cargo.lock, rust-toolchain.toml
  .cargo/config.toml
  .github/workflows/, ISSUE_TEMPLATE/, CODEOWNERS
  research/                      # evidence; not language authority
  docs/
    spec/                        # normative grammar and semantics
    book/                        # tutorials and worked examples
    reference/                   # CLI, config, diagnostics, standard library
    contributing/                # architecture, tests, releases
    rfcs/                        # README, template, numbered proposals
    adr/                         # internal engineering decisions
  compiler/
    vow-core/                    # lexer/parser/types/IR modules initially
    vow-cli/                     # binary named vow; thin orchestration
    vow-test-support/            # add when helpers need sharing
    vow-lsp/                     # add with first IDE milestone
  xtask/                         # portable repository commands
  tests/
    ui/                          # .vow inputs and reviewed diagnostics
    run/                         # executable behavior and exit status
    conformance/                 # normative, backend-independent cases
    fixtures/                    # small complete projects
  examples/                      # runnable, tested user examples
  fuzz/                          # separate nightly Cargo workspace
  benches/                       # datasets and performance methodology
  editors/vscode/                # deferred extension package
  grammar/tree-sitter-vow/        # deferred grammar and corpus tests
  website/                       # existing Next.js/Fumadocs app + bun.lock
```

Initially list only `compiler/vow-core`, `compiler/vow-cli`, and `xtask` as Cargo members. Keep a single root `Cargo.lock`; exclude `fuzz/` from the main workspace. Start compiler phases as modules rather than a dozen prematurely coupled crates. Put benchmark targets in `vow-core/benches/`; root `benches/` holds shared inputs and methodology. Register root `tests/` explicitly in the CLI integration-test harness: Cargo will not discover tests automatically at a virtual workspace root.

Use Rust edition 2024 with resolver 3. Centralize `edition`, `license`, `repository`, `version`, and `rust-version` in `[workspace.package]`; use `[workspace.dependencies]` and `[workspace.lints]`. Each member must opt in with `*.workspace = true` and `[lints] workspace = true`. Keep dependency features at the consuming crate unless genuinely universal. These inheritance mechanisms are documented in the [Cargo Book][cargo-workspaces].

Choose the oldest stable compiler that passes the first implementation's full build and record that exact MSRV; pin the development toolchain separately. Test the MSRV instead of inferring support from compilation on newer Rust. Permit MSRV increases in pre-1.0 minor releases, with release notes. Pin rustfmt/Clippy through the development toolchain; nightly is reserved for fuzzing or specific instrumentation. Start with `unsafe_code = "forbid"` in the safe frontend, `clippy::all` warnings, and a short reviewed lint list; isolate any future necessary unsafe runtime code behind a separately reviewed crate boundary.

### Root files and community configuration

Add these files incrementally, with real commands and contacts rather than placeholder promises:

- `README.md`: purpose, maturity, install/build commands, example, documentation and contribution links.
- `CONTRIBUTING.md`: setup, architecture map, tests, snapshot review, PR expectations, and AI policy.
- `AGENTS.md`: the operational outline below; `CLAUDE.md`: a short pointer/import to that canonical guidance, avoiding duplicated rules.
- `LICENSE-MIT`, `LICENSE-APACHE`: complete license texts; `COPYRIGHT`: ownership/third-party notices where needed.
- `CODE_OF_CONDUCT.md`: community rules and a monitored private reporting route.
- `SECURITY.md`: supported versions, private reporting, triage ownership, and disclosure process.
- `GOVERNANCE.md`: initial maintainer, decision authority, contributor promotion, conflict resolution, and release permissions.
- `CHANGELOG.md`, `RELEASING.md`, `STABILITY.md`: user-visible history, release procedure, and compatibility commitments.
- `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`: workspace metadata, reproducible resolution, and pinned development tools.
- `rustfmt.toml`, `clippy.toml`, `deny.toml`: minimal formatting/lint options and dependency-source/license/advisory policy. Keep lint levels in Cargo, not duplicated in `clippy.toml`.
- `.editorconfig`, `.gitattributes`, `.gitignore`: UTF-8/LF conventions, binary treatment, and generated/private build output exclusions.
- Before release: `dist-workspace.toml`, `release-please-config.json`, `.release-please-manifest.json`.

Also add `.cargo/config.toml` with an `xtask` alias; `.github/CODEOWNERS` for compiler/spec/site/workflows; bug and language-proposal issue forms; a PR template covering behavior, tests, compatibility, and AI involvement; and Dependabot configuration for Cargo, the website ecosystem it supports, and Actions. Preserve the committed bun lockfile when updating website dependencies.

**License recommendation:** use `MIT OR Apache-2.0` for original Vow code and documentation, with explicit inbound contribution terms. This follows Rust's repository convention and Biome's [workspace declaration][biome-cargo]. MIT is short and permissive; Apache-2.0 includes an explicit patent grant and associated conditions. Dual licensing gives recipients a choice—it does not force everyone to choose Apache. Preserve third-party obligations; generated programs should not inherit a compiler-code license merely by being compiled. If Vow embeds runtime code, document its licensing separately. [MIT text][mit]; [Apache-2.0 text][apache].

### CI workflows and required checks

Proposed commands below become supported interfaces only after implementation. The existing website currently has `dev`, `build`, `start`, and `lint`; add a `typecheck` script before CI references it.

1. **`ci.yml`, PR/push/merge-group:** `changes` computes affected components; `rust-quality` runs `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings`; `rust-test` runs `cargo test --workspace --locked` on Linux, macOS, and Windows. Add supported feature combinations explicitly; do not assume `--all-features` is meaningful if features conflict.
2. **Same workflow:** `msrv` runs workspace checks/tests on the declared floor; `docs-and-generated` runs `cargo doc`, executable documentation examples, link checks, and `cargo xtask codegen --check`; `dependencies` runs `cargo deny check`.
3. **Website job:** from `website/`, use the package-manager version pinned in its manifest, `bun install --frozen-lockfile`, `bun run lint`, the new `bun run typecheck`, and `bun run build`. Add one browser smoke test for documentation navigation/search and Markdown endpoints when those features exist.
4. **`ci-required`:** always run an aggregate job that fails if any applicable prerequisite fails or is cancelled. Account for intentional skips explicitly. Require this check and review through repository rules; avoid workflow-level path filters that leave required checks pending.
5. **`nightly.yml`:** cargo-fuzz with a bounded budget and retained crash artifacts, broader feature/target coverage, dependency advisories, and coverage reporting via [cargo-llvm-cov][llvm-cov]. Build fuzz harnesses on relevant PRs; keep long fuzz campaigns outside the fast gate.
6. **`perf.yml`:** benchmark main and candidate commits on the same controlled runner; initially report results without flaky automatic rejection.
7. **`release-please.yml` and generated `release.yml`:** prepare reviewed release PRs, then invoke the pinned cargo-dist pipeline to build, smoke-test, and publish artifacts. Website deployment remains independent of compiler versioning.

Use [Swatinem/rust-cache][rust-cache] with distinct target/toolchain keys; lockfile-sensitive website caching; cancellation of superseded PR runs; timeouts; and failure artifacts. Pin third-party Actions to reviewed SHAs. Default permissions to read-only and grant publication rights only to release jobs. Do not run untrusted PR code with publishing credentials. Begin on hosted runners; dedicated performance machines are a later investment. Gleam's [workflow][gleam-ci] is a more suitable starting scale than Rust's dynamic CI machinery.

### Compiler testing strategy

**Unit and property tests:** cover tokenization, source offsets, parser recovery, unification, scope handling, and IR invariants. Include Unicode, truncated input, nesting limits, and malformed programs. An invalid program should produce an error rather than crash or hang. Formatter idempotence and parse/print/parse equivalence need a defined semantic normalization, not raw AST equality when trivia changes.

**Snapshots and UI tests:** use `insta` for AST/IR and structured diagnostics; run real CLI processes for `.vow` fixtures with expected stdout, stderr, and exit status. Test stable diagnostic codes, spans, notes, and machine-applicable fixes separately from color/terminal wrapping. Normalize temporary paths, platform separators, and nondeterministic IDs narrowly. Never normalize the error code or semantic content being asserted. CI fails on new snapshots; maintainers inspect every accepted diff. Learn the convention from [Rust UI tests][rust-ui] and [uv's snapshot guidance][uv-agents], without adopting rustc's entire harness. `trybuild` tests Rust compilation; it is not automatically a harness for Vow source.

**Executable and conformance tests:** give each case a spec-section identifier, expected success/failure phase, required features, and expected values/output. Follow [Test262's metadata approach][test262], but build a Vow-specific suite: JavaScript conformance cases do not define Vow semantics. Require positive, rejection, boundary, and cross-feature examples for new syntax/type rules. Keep implementation-specific IR snapshots outside normative conformance. Track skipped cases with reason, owner, and issue.

**Fuzz and differential tests:** use [cargo-fuzz][cargo-fuzz] with pinned nightly, strict time/memory budgets, and corpus seeds from real tests. Begin with lexer/parser no-panic properties; then formatter and typechecker invariants. Minimize every failure into a deterministic regression. Once two execution paths exist, compare an interpreter with native/WASM output and optimized with unoptimized builds on generated well-typed programs. Restrict comparisons to defined semantics; agreement between two implementations is not proof of correctness. Roc's [fuzz workflow documentation][roc-fuzz] is a strong model for the full crash-to-regression loop.

**Performance:** choose [Criterion][criterion] initially for lexer/parser/typecheck microbenchmarks; [Divan][divan] is a reasonable lighter alternative, not a second mandatory harness. Record compiler revision, toolchain, CPU, flags, corpus hash, warm/cold state, distributions, peak memory, and binary size. Add whole-project compile and LSP latency workloads. Run baseline/candidate repeatedly on the same machine and distinguish noise from repeatable regressions. uv's [build/measurement split][uv-bench] is the later model. Coverage identifies untested areas; it does not replace conformance or correctness arguments.

### Commits, changelog, RFCs, and documentation

Squash-merge PRs with Conventional Commit titles: `feat(parser): add match expressions`, `fix(types): retain the source span`, `docs(book): explain effects`. Validate PR titles rather than every temporary contributor commit. Require `!` or `BREAKING CHANGE:` plus migration notes for incompatible public changes.

Use **release-please's Rust strategy** for the initial root workspace and one Vow product version. Its [configuration guide][release-please] supports Rust workspaces; if crates later acquire independent release versions, use the documented [Cargo workspace plugin][release-manifest]. Review generated notes for user impact. Changesets, as used by Biome/Effect, become useful if Vow gains independently published JavaScript packages; do not make both tools manage the same product's changelog. Website-only work should not create compiler releases.

Store language RFCs in `docs/rfcs/NNNN-title.md`; internal build/architecture choices use `docs/adr/NNNN-title.md`. Require RFCs for syntax, semantics, effects, module/package rules, diagnostic protocol breaks, and stability-policy changes—not ordinary fixes. Use: discussion → draft PR → review → seven-calendar-day final comment period → accepted/rejected/postponed. That seven-day interval is a **Vow recommendation**, not Rust's ten-day rule. The initial maintainer records the decision and outstanding objections; do not pretend a solo project has committee consensus.

The template should include motivation, concrete examples, normative semantics, alternatives, drawbacks, migration, agent/tooling impact, security/resource limits, tests, and unresolved questions. Link implementation issues. Track acceptance, implementation, and stabilization separately. Preserve rejected proposals and explain supersession. Rust's [RFC template][rfc-template], [Swift's process][swift-process], and [PEP 1][pep1] supply complementary models.

Make `docs/spec/` authoritative, `docs/book/` instructional, and `docs/reference/` practical lookup material. Compiler behavior that disagrees with the spec needs an explicit bug or spec change. Fumadocs should consume these canonical sources rather than maintain a second manually copied documentation tree. Test runnable code fences, excluding pseudocode explicitly. Publish version-labeled `/llms.txt`, `/llms-full.txt`, and per-page Markdown through [Fumadocs' LLM integration][fumadocs-llms]; these are discovery/content formats, not guarantees that agents will obey documentation. Include stability labels and diagnostic-schema links.

### AGENTS.md outline and editor support

Keep the root agent file short and link deeper guidance:

1. Project purpose, current maturity, canonical spec, architecture map, and folder ownership.
2. Exact setup/build/check commands, required tool versions, and change-to-test mapping.
3. Scope rules: inspect existing helpers, preserve unrelated changes, and avoid speculative features or dependencies.
4. Compiler invariants: deterministic output, malformed-input recovery, explicit resource limits, and stable diagnostic contracts.
5. Generated files and their source generators; snapshot acceptance and conformance requirements.
6. Documentation/RFC obligations and compatibility/release-note requirements.
7. Contribution policy: human accountability, disclosed AI involvement, no fabricated test claims, and named review for semantics/security changes.
8. Permissions: no credentials in output, no autonomous publication, and explicit reporting of checks that could not run.

Add `compiler/AGENTS.md` and retain/adapt the existing `website/AGENTS.md` for local commands. The [AGENTS.md convention][agents-format] supports directory-scoped guidance. Use a pointer/import or verified synchronization for `CLAUDE.md`; do not assume every agent automatically reads every other tool's file. Effect's [guide][effect-agents], rust-analyzer's [guide][ra-agents], and Bun's [guide][bun-agents] demonstrate useful task-specific instructions. Vow should explicitly welcome reviewed AI assistance while limiting unattended issue/PR spam.

Ship a **TextMate grammar** first for inexpensive VS Code highlighting, brackets, comments, and snippets; VS Code's [syntax guide][vscode-syntax] explains this layer. Add a **tree-sitter grammar** with [corpus tests][tree-tests] for other editors and incremental syntax tooling. Share examples across grammars, but keep compiler parsing authoritative. Implement `vow lsp` over compiler analysis: diagnostics, hover, definition, formatting, then completion/rename. Test protocol sessions, incomplete edits, cancellation, and stale-result suppression. Pin extension/server compatibility and never implement a second typechecker in the extension.

### Release and stability plan

Start with `0.1.0-alpha.1`, then a deliberately scoped `0.1.0`. Publish immutable versioned archives for Linux x86-64, macOS arm64/x86-64, and Windows x86-64 only when native smoke tests pass. State OS/ABI minimums and unsupported targets. Keep internal crates `publish = false` until there is an intentional library API.

Use pinned [cargo-dist][cargo-dist] to generate archives/installers and the release workflow. Before publication: green checks at the release commit, reviewed notes/migrations, CLI version verification, example compilation from extracted archives, installer smoke tests, checksums, license notices, and artifact attestations. Maintain `RELEASING.md` with failed-release recovery: never overwrite an existing version's binaries; publish a corrected version. Wire release-please to cargo-dist explicitly: a tag created with GitHub's default workflow token may not trigger another workflow, so use a reviewed dispatch/job handoff or appropriately scoped GitHub App credentials. [GitHub workflow-trigger rules][workflow-triggers].

Before 1.0, reserve minor releases for deliberate language breaks and patch releases for compatible fixes; mark experimental features as opt-in and outside the stable contract. At 1.0, enumerate guarantees for source semantics, standard library, CLI flags/exit codes, configuration, diagnostic JSON, and LSP extensions. Keep human diagnostic wording improvable while codes/schema remain stable. Recommend deprecation warnings plus a documented migration for at least two minor releases and six months before a planned major-version removal, with explicitly justified security/correctness exceptions. This is Vow's proposed policy, not Python's timetable.

Defer **editions** until incompatible source evolution needs coexistence. Rust's [edition model][editions] demonstrates per-package opt-in migration and interoperability; Vow should not claim equivalent guarantees before its module/compiler architecture supports them. Version the compiler, language edition if introduced, and machine protocols as distinct contracts.

## 4. Prioritized checklist

### Now

- [ ] Add README, contribution/AI policy, dual licenses, governance, conduct/security contacts, and scoped agent instructions.
- [ ] Record the initial normative spec, RFC template/status rules, and architecture ADR.
- [ ] Establish the three-member Cargo workspace, pinned tools, lockfile, inherited lints, and tested MSRV.
- [ ] Implement parser/unit tests, structured diagnostic snapshots, CLI UI fixtures, and the first spec-linked conformance cases.
- [ ] Add platform CI, website checks, dependency policy, and one required aggregate gate.
- [ ] Enforce Conventional Commit PR titles; test website documentation examples and links.

### At first release

- [ ] Publish stability, supported-target, diagnostic-schema, and MSRV policies.
- [ ] Rehearse release-please → cargo-dist end to end; test extracted archives/installers on supported systems.
- [ ] Include reviewed notes, checksums, notices, provenance, and release recovery instructions.
- [ ] Establish bounded scheduled fuzzing and a minimized-crash regression process.
- [ ] Publish versioned docs and generated LLM endpoints; ship TextMate highlighting and basic LSP support.
- [ ] Record repeatable compiler performance baselines and initial tree-sitter corpus coverage.

### Later

- [ ] Introduce dedicated performance runners/CodSpeed and evidence-based regression gates.
- [ ] Add differential execution, additional backends, ecosystem compatibility runs, and sanitizer coverage where relevant.
- [ ] Split crates when ownership or dependency boundaries justify it; publish an SDK only with explicit guarantees.
- [ ] Add contributor promotion/release succession, merge queues, and independent package release tooling as team/package count grows.
- [ ] Consider editions only when compatibility requirements justify their long-term cost.

## 5. Sources and revision register

All links below were consulted directly as repository files, trees, or official documentation. Repository-file links are pinned; auxiliary tool documentation links can evolve. Some recursive GitHub tree requests were truncated/rate-limited, so targeted raw-file reads and directory pages were used; no claim of exhaustive repository auditing is made.

Inspected revisions: Gleam `0d9fa18ddb69`; Rust `158f79d5c24d`; Roc `5f126ac3ce5d`; Zig `738d2be9d6b6` (2025-11-27, frozen GitHub copy); Deno `a18ce33715e3`; Biome `cd735e9faac24`; Ruff `8d122c1fd2b8`; uv `00c97356501a`; rust-analyzer `70e0c46be740`; Bun `017b2439a7a7`; Swift Evolution `f006bf3cf259`; Python PEPs `6068058b8964`; TypeScript `2e9fb00f5f5c`; Effect `7f32606baac4`.

- **Gleam:** [Cargo.toml][gleam-cargo]; [CONTRIBUTING.md][gleam-contrib]; [.github/workflows/ci.yaml][gleam-ci]; [deny.toml][gleam-deny]; [RELEASE.md][gleam-release].
- **Rust:** [repository tree][rust-tree]; [.github/workflows/ci.yml][rust-ci]; [AGENTS.md][rust-agents].
- **Roc:** [repository tree][roc-tree]; [CONTRIBUTING/README.md][roc-contrib]; [src/snapshot_tool/README.md][roc-snapshots]; [CONTRIBUTING/fuzzing.md][roc-fuzz]; [.github/workflows/ci_zig_benchmarks.yml][roc-bench].
- **Zig:** [README.md][zig-readme]; [test/tests.zig][zig-tests]; [build.zig.zon][zig-manifest].
- **Deno:** [Cargo.toml][deno-cargo]; [CLAUDE.md][deno-agents]; [.github/workflows/ci.generated.yml][deno-ci]; [tools/cut_a_release.md][deno-release].
- **Biome:** [Cargo.toml][biome-cargo]; [.github/workflows/pull_request.yml][biome-ci]; [.github/workflows/parser_conformance.yml][biome-conformance]; [fuzz/README.md][biome-fuzz]; [AGENTS.md][biome-agents]; [RELEASES.md][biome-release].
- **Ruff:** [CONTRIBUTING.md][ruff-contrib]; [AGENTS.md][ruff-agents]; [fuzz/README.md][ruff-fuzz]; [.github/workflows/daily_fuzz.yaml][ruff-daily]; [dist-workspace.toml][ruff-dist]; [docs/versioning.md][ruff-version].
- **uv:** [Cargo.toml][uv-cargo]; [AGENTS.md][uv-agents]; [.github/workflows/bench.yml][uv-bench]; [dist-workspace.toml][uv-dist].
- **rust-analyzer:** [docs/book/src/contributing/architecture.md][ra-architecture]; [docs/book/src/contributing/testing.md][ra-tests]; [CLAUDE.md][ra-agents]; [.github/workflows/release.yaml][ra-release]; [AI_POLICY.md][ra-ai].
- **Bun:** [CONTRIBUTING.md][bun-contrib]; [CLAUDE.md][bun-agents]; [.buildkite/ci.ts][bun-ci].
- **Swift Evolution:** [process.md][swift-process]; [repository tree][swift-tree]; [.github/workflows/proposal_pull_request_swift.yml][swift-ci].
- **Python PEPs:** [peps/pep-0001.rst][pep1]; [.github/workflows/test.yml][pep-tests]; [.github/workflows/render.yml][pep-render]; [peps/pep-0387.rst][pep387].
- **TypeScript:** [.github/copilot-instructions.md][ts-agents]; [.github/workflows/ci.yml][ts-ci]; [Herebyfile.mjs][ts-build]; [CONTRIBUTING.md][ts-contrib].
- **Effect:** [package.json][effect-package]; [.github/workflows/check.yml][effect-ci]; [.changeset/config.json][effect-changesets]; [.github/workflows/release.yml][effect-release]; [.agents/AGENTS.md][effect-agents].

Additional primary sources:

- [Gleam tree-sitter][gleam-tree].
- [Gleam VS Code][gleam-editor].
- [Compiletest guide][compiletest].
- [Rust UI tests][rust-ui].
- [Rust RFC process][rust-rfcs].
- [Rust RFC template][rfc-template].
- [Cargo workspaces][cargo-workspaces].
- [Swatinem/rust-cache][rust-cache].
- [release-please strategies][release-please].
- [release-please workspace configuration][release-manifest].
- [cargo-dist README][cargo-dist].
- [cargo-llvm-cov][llvm-cov].
- [cargo-fuzz][cargo-fuzz].
- [Criterion maintained repository][criterion].
- [Divan][divan].
- [Test262 contribution and metadata guide][test262].
- [Fumadocs LLM integration][fumadocs-llms].
- [AGENTS.md format][agents-format].
- [VS Code highlighting][vscode-syntax].
- [tree-sitter corpus tests][tree-tests].
- [Rust editions][editions].
- [MIT license text][mit].
- [Apache-2.0 license text][apache].
- [GitHub workflow-trigger rules][workflow-triggers].

[gleam-cargo]: https://github.com/gleam-lang/gleam/blob/0d9fa18ddb690e34f220cc42ebe81f657ce0b574/Cargo.toml
[gleam-contrib]: https://github.com/gleam-lang/gleam/blob/0d9fa18ddb690e34f220cc42ebe81f657ce0b574/CONTRIBUTING.md
[gleam-ci]: https://github.com/gleam-lang/gleam/blob/0d9fa18ddb690e34f220cc42ebe81f657ce0b574/.github/workflows/ci.yaml
[gleam-deny]: https://github.com/gleam-lang/gleam/blob/0d9fa18ddb690e34f220cc42ebe81f657ce0b574/deny.toml
[gleam-release]: https://github.com/gleam-lang/gleam/blob/0d9fa18ddb690e34f220cc42ebe81f657ce0b574/RELEASE.md
[rust-tree]: https://github.com/rust-lang/rust/tree/158f79d5c24d262f820fa45da96e7b77bc29a338
[rust-ci]: https://github.com/rust-lang/rust/blob/158f79d5c24d262f820fa45da96e7b77bc29a338/.github/workflows/ci.yml
[rust-agents]: https://github.com/rust-lang/rust/blob/158f79d5c24d262f820fa45da96e7b77bc29a338/AGENTS.md
[roc-tree]: https://github.com/roc-lang/roc/tree/5f126ac3ce5dd3c52d4d3309d5dcf53c5f7ffed5
[roc-contrib]: https://github.com/roc-lang/roc/blob/5f126ac3ce5dd3c52d4d3309d5dcf53c5f7ffed5/CONTRIBUTING/README.md
[roc-snapshots]: https://github.com/roc-lang/roc/blob/5f126ac3ce5dd3c52d4d3309d5dcf53c5f7ffed5/src/snapshot_tool/README.md
[roc-fuzz]: https://github.com/roc-lang/roc/blob/5f126ac3ce5dd3c52d4d3309d5dcf53c5f7ffed5/CONTRIBUTING/fuzzing.md
[roc-bench]: https://github.com/roc-lang/roc/blob/5f126ac3ce5dd3c52d4d3309d5dcf53c5f7ffed5/.github/workflows/ci_zig_benchmarks.yml
[zig-readme]: https://github.com/ziglang/zig/blob/738d2be9d6b6ef3ff3559130c05159ef53336224/README.md
[zig-tests]: https://github.com/ziglang/zig/blob/738d2be9d6b6ef3ff3559130c05159ef53336224/test/tests.zig
[zig-manifest]: https://github.com/ziglang/zig/blob/738d2be9d6b6ef3ff3559130c05159ef53336224/build.zig.zon
[deno-cargo]: https://github.com/denoland/deno/blob/a18ce33715e30cd2b0d99c7e322ef11a65490e4d/Cargo.toml
[deno-agents]: https://github.com/denoland/deno/blob/a18ce33715e30cd2b0d99c7e322ef11a65490e4d/CLAUDE.md
[deno-ci]: https://github.com/denoland/deno/blob/a18ce33715e30cd2b0d99c7e322ef11a65490e4d/.github/workflows/ci.generated.yml
[deno-release]: https://github.com/denoland/deno/blob/a18ce33715e30cd2b0d99c7e322ef11a65490e4d/tools/cut_a_release.md
[biome-cargo]: https://github.com/biomejs/biome/blob/cd735e9faac24738783c1eb7c469862d4cdb3235/Cargo.toml
[biome-ci]: https://github.com/biomejs/biome/blob/cd735e9faac24738783c1eb7c469862d4cdb3235/.github/workflows/pull_request.yml
[biome-conformance]: https://github.com/biomejs/biome/blob/cd735e9faac24738783c1eb7c469862d4cdb3235/.github/workflows/parser_conformance.yml
[biome-fuzz]: https://github.com/biomejs/biome/blob/cd735e9faac24738783c1eb7c469862d4cdb3235/fuzz/README.md
[biome-agents]: https://github.com/biomejs/biome/blob/cd735e9faac24738783c1eb7c469862d4cdb3235/AGENTS.md
[biome-release]: https://github.com/biomejs/biome/blob/cd735e9faac24738783c1eb7c469862d4cdb3235/RELEASES.md
[ruff-contrib]: https://github.com/astral-sh/ruff/blob/8d122c1fd2b8f16d494ff8a8f8ee6f264bba7ceb/CONTRIBUTING.md
[ruff-agents]: https://github.com/astral-sh/ruff/blob/8d122c1fd2b8f16d494ff8a8f8ee6f264bba7ceb/AGENTS.md
[ruff-fuzz]: https://github.com/astral-sh/ruff/blob/8d122c1fd2b8f16d494ff8a8f8ee6f264bba7ceb/fuzz/README.md
[ruff-daily]: https://github.com/astral-sh/ruff/blob/8d122c1fd2b8f16d494ff8a8f8ee6f264bba7ceb/.github/workflows/daily_fuzz.yaml
[ruff-dist]: https://github.com/astral-sh/ruff/blob/8d122c1fd2b8f16d494ff8a8f8ee6f264bba7ceb/dist-workspace.toml
[ruff-version]: https://github.com/astral-sh/ruff/blob/8d122c1fd2b8f16d494ff8a8f8ee6f264bba7ceb/docs/versioning.md
[uv-cargo]: https://github.com/astral-sh/uv/blob/00c97356501aafa781b287d38feffe17f0dfa71f/Cargo.toml
[uv-agents]: https://github.com/astral-sh/uv/blob/00c97356501aafa781b287d38feffe17f0dfa71f/AGENTS.md
[uv-bench]: https://github.com/astral-sh/uv/blob/00c97356501aafa781b287d38feffe17f0dfa71f/.github/workflows/bench.yml
[uv-dist]: https://github.com/astral-sh/uv/blob/00c97356501aafa781b287d38feffe17f0dfa71f/dist-workspace.toml
[ra-architecture]: https://github.com/rust-lang/rust-analyzer/blob/70e0c46be740a471bcc95f2be290f4bbfdcffd87/docs/book/src/contributing/architecture.md
[ra-tests]: https://github.com/rust-lang/rust-analyzer/blob/70e0c46be740a471bcc95f2be290f4bbfdcffd87/docs/book/src/contributing/testing.md
[ra-agents]: https://github.com/rust-lang/rust-analyzer/blob/70e0c46be740a471bcc95f2be290f4bbfdcffd87/CLAUDE.md
[ra-release]: https://github.com/rust-lang/rust-analyzer/blob/70e0c46be740a471bcc95f2be290f4bbfdcffd87/.github/workflows/release.yaml
[ra-ai]: https://github.com/rust-lang/rust-analyzer/blob/70e0c46be740a471bcc95f2be290f4bbfdcffd87/AI_POLICY.md
[bun-contrib]: https://github.com/oven-sh/bun/blob/017b2439a7a75c82c7a86a294badd8ef2fc30e28/CONTRIBUTING.md
[bun-agents]: https://github.com/oven-sh/bun/blob/017b2439a7a75c82c7a86a294badd8ef2fc30e28/CLAUDE.md
[bun-ci]: https://github.com/oven-sh/bun/blob/017b2439a7a75c82c7a86a294badd8ef2fc30e28/.buildkite/ci.ts
[swift-process]: https://github.com/swiftlang/swift-evolution/blob/f006bf3cf2594bdd4a972f9126f93bd1743ed524/process.md
[swift-tree]: https://github.com/swiftlang/swift-evolution/tree/f006bf3cf2594bdd4a972f9126f93bd1743ed524
[swift-ci]: https://github.com/swiftlang/swift-evolution/blob/f006bf3cf2594bdd4a972f9126f93bd1743ed524/.github/workflows/proposal_pull_request_swift.yml
[pep1]: https://github.com/python/peps/blob/6068058b896441388bbf22f2607ab681193f6a3d/peps/pep-0001.rst
[pep-tests]: https://github.com/python/peps/blob/6068058b896441388bbf22f2607ab681193f6a3d/.github/workflows/test.yml
[pep-render]: https://github.com/python/peps/blob/6068058b896441388bbf22f2607ab681193f6a3d/.github/workflows/render.yml
[pep387]: https://github.com/python/peps/blob/6068058b896441388bbf22f2607ab681193f6a3d/peps/pep-0387.rst
[ts-agents]: https://github.com/microsoft/TypeScript/blob/2e9fb00f5f5c9186d09aabd78cced5ce2497456d/.github/copilot-instructions.md
[ts-ci]: https://github.com/microsoft/TypeScript/blob/2e9fb00f5f5c9186d09aabd78cced5ce2497456d/.github/workflows/ci.yml
[ts-build]: https://github.com/microsoft/TypeScript/blob/2e9fb00f5f5c9186d09aabd78cced5ce2497456d/Herebyfile.mjs
[ts-contrib]: https://github.com/microsoft/TypeScript/blob/2e9fb00f5f5c9186d09aabd78cced5ce2497456d/CONTRIBUTING.md
[effect-package]: https://github.com/Effect-TS/effect/blob/7f32606baac43a67d171ae93b77c7b29791c9692/package.json
[effect-ci]: https://github.com/Effect-TS/effect/blob/7f32606baac43a67d171ae93b77c7b29791c9692/.github/workflows/check.yml
[effect-changesets]: https://github.com/Effect-TS/effect/blob/7f32606baac43a67d171ae93b77c7b29791c9692/.changeset/config.json
[effect-release]: https://github.com/Effect-TS/effect/blob/7f32606baac43a67d171ae93b77c7b29791c9692/.github/workflows/release.yml
[effect-agents]: https://github.com/Effect-TS/effect/blob/7f32606baac43a67d171ae93b77c7b29791c9692/.agents/AGENTS.md
[gleam-tree]: https://github.com/gleam-lang/tree-sitter-gleam
[gleam-editor]: https://github.com/gleam-lang/vscode-gleam
[compiletest]: https://rustc-dev-guide.rust-lang.org/tests/compiletest.html
[rust-ui]: https://rustc-dev-guide.rust-lang.org/tests/ui.html
[rust-rfcs]: https://github.com/rust-lang/rfcs/blob/main/README.md
[rfc-template]: https://github.com/rust-lang/rfcs/blob/main/0000-template.md
[cargo-workspaces]: https://doc.rust-lang.org/cargo/reference/workspaces.html
[rust-cache]: https://github.com/Swatinem/rust-cache
[release-please]: https://github.com/googleapis/release-please/blob/main/docs/customizing.md
[release-manifest]: https://github.com/googleapis/release-please/blob/main/docs/manifest-releaser.md
[cargo-dist]: https://github.com/axodotdev/cargo-dist/blob/main/README.md
[llvm-cov]: https://github.com/taiki-e/cargo-llvm-cov
[cargo-fuzz]: https://github.com/rust-fuzz/cargo-fuzz
[criterion]: https://github.com/criterion-rs/criterion.rs
[divan]: https://github.com/nvzqz/divan/blob/main/README.md
[test262]: https://github.com/tc39/test262/blob/main/CONTRIBUTING.md
[fumadocs-llms]: https://www.fumadocs.dev/docs/integrations/llms
[agents-format]: https://agents.md/
[vscode-syntax]: https://code.visualstudio.com/api/language-extensions/syntax-highlight-guide
[tree-tests]: https://github.com/tree-sitter/tree-sitter/blob/master/docs/src/creating-parsers/5-writing-tests.md
[editions]: https://doc.rust-lang.org/edition-guide/editions/index.html
[mit]: https://opensource.org/license/mit
[apache]: https://opensource.org/license/apache-2.0



[workflow-triggers]: https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow
