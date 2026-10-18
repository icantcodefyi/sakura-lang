# Agent DX programme (round 10, §17.140–§17.148)

Status: **plan**, 2026-10-18. Owner: language design (Fable). Decisions: `docs/design.md`
§17.140–§17.148. Evidence: `research/11-agent-dx.md` (Astra, `7e14bf7`). Nothing here is
released, published or installed by itself; §17.140's five principles (one source with mechanical
adapters; compiler-matched; executable; nothing publishes or escalates by itself; measured, not
assumed) bind every workstream below.

Model roles: **Opus** codes (generator, CLI, harnesses, runner, workflows, editor client);
**Sol** does the mechanical parts (mirrors, adapters, CI wiring, website routes, fixed-program
companions, ports); **Astra** authors prose and designs experiments. The owner
([@icantcodefyi](https://github.com/icantcodefyi)) reviews semantics and security changes and is
the only one who publishes anything. Implementers post deviations in §4 and questions in
`compiler/REQUESTS.md`.

## 1. Workstreams

| ID | Workstream (decision) | Owner | Paths | Starts after | Acceptance |
|---|---|---|---|---|---|
| **DX0** | Correct the agent-facing surfaces (§17.141) | Astra audits and writes each sentence; Sol applies; Opus for `compiler_info` | `compiler/crates/sakura_mcp/src/lib.rs` (`compiler_info.features` from the build's facts), `docs/mcp.md`, `docs/llms.txt` (status section), `AGENTS.md` (root: the workspace and toolchain exist), `STABILITY.md`, `website/lib/source.ts`, `website/content/docs/agents/{index,llms-txt,mcp,diagnostics,verify}.mdx` | now | `mcp.rs` asserts `compiler_info` against the binary's capabilities (`backends` includes `wasm` when compiled in, `verify: true` with solver status); no agent page calls an exercised feature planned/future/unsettled, and `--tools`/HTTP export is labelled planned; `frozen_specs` documented optional; website `fmt:check`, `lint`, `typecheck`, `build`; every local link resolves |
| **DX1** | The portable `sakura` skill, its generator and its CI test (§17.142) | Astra authors `SKILL.md` and the repair table; Opus writes `agent_docs` and `agent_docs.rs`; Sol the mirror, ZIP, checksum, links | `skills/sakura/{SKILL.md,references/,assets/}` (new top-level directory; ADR 0002 amended), `compiler/tools/agent_docs/` (workspace member), `compiler/crates/sakura_cli/src/main.rs` (embedded bundle, version and commit filled at build), `compiler/crates/sakura_cli/tests/agent_docs.rs`, `compiler/tests/agent_docs/`, `compiler/std/INDEX.md` (now generated), `website/public/skills/sakura/` + `sakura-skill.zip` + `.sha256` + `manifest.json`, `website/content/docs/agents/index.mdx`, `website/app/llms.txt` index, `.github/workflows/ci.yml` (`agent_docs --check`) | DX0's sentences for the status lines; otherwise now | body < 500 lines and about 2,000 tokens (count reported in bytes, lines and the pilot model's tokens); every code fence classified (`check`/`test`/`run`/`wrong:<code>`/`fragment`/`hole-step`) and checked by the harness, none skipped; `references/language.md` byte-equal to `docs/llms.txt`; `stdlib.md` regenerates clean and `INDEX.md`'s hand-regeneration note is gone; raw `/skills/sakura/SKILL.md` starts with the frontmatter and relative links resolve; ZIP and mirror hashes match the source tree; `cargo run -p agent_docs -- --check` is a required CI step |
| **DX2** | `new`/`init` write `AGENTS.md` + `CLAUDE.md`; `sakura setup` installs the skill and client adapters (§17.143) | Opus (`new.rs`, `setup.rs`, MCP root resolution); Sol (adapter templates); Astra (template prose) | `compiler/crates/sakura_cli/src/{new.rs,setup.rs}`, `compiler/crates/sakura_mcp/src/lib.rs` (root = nearest ancestor `sakura.toml`; usage error outside a project), `skills/sakura/assets/{AGENTS.md,CLAUDE.md,mcp-claude.json,mcp-codex.toml,mcp-cursor.json,sakura.mdc}`, scaffold tests, `website/content/docs/quickstart.mdx` | DX1's embedded bundle | scaffold tests: four templates with the two new files; existing files preserved; `setup` dry run writes nothing; `--apply` on a fresh scaffold; `--apply` twice is idempotent; each client's entry merged into an existing config; malformed existing config prints a patch and writes nothing; path with spaces; `mcp serve` from a subdirectory and from outside the project; no write outside the project in any test (asserted by a tempdir diff); three documented client smoke runs (research/11 §4) with versions recorded in §4 before the quickstart advertises `setup` |
| **DX3** | Generated error-code reference pages (§17.144) | Opus (registry export in `agent_docs`); Sol (fixed companions, index, website wiring); Astra (explanations that are fragments today) | `compiler/tools/agent_docs/` (registry renderer), `website/content/docs/errors/{index,<code>}.mdx` (committed, generated), `compiler/tests/ui/<code>_<slug>.fixed.sakura` (companions, most frequent codes first), `compiler/crates/sakura_cli/tests/ui.rs` (fixed companions exit 0), `sakura explain` (prints the page URL after its text) | DX1's generator | `--check` fails on a code without a page or a page without a code; retired codes marked; every fixed companion checks clean in CI; a page without a companion says so visibly; link check over the index; `.md` export of a page works through the existing rewrite |
| **DX4** | Cookbook recipes as harness-run examples (§17.145) | Astra writes recipes; Sol extracts and renders; Opus only for a missing tested example | `compiler/examples/<recipe>/` (eight first recipes: JSON into a refined record; CLI over files; HTTP with retry and cassette; SQLite `sql"…"`; router endpoint tested directly; scripted LLM tool; budgeted agent loop; indexed update with `List.set`), `website/content/docs/cookbook/`, `skills/sakura/references/recipes.md` | DX1 | every recipe passes its harness mode offline in CI; pages regenerate clean; no recipe uses an unfinished API; no recipe is an evaluation task of DX5 |
| **DX5** | Agent evaluation pilot (§17.146) | Opus (runner, isolation, accounting); Astra (tasks, protocol, report); Sol (fixtures, workflow, environment) | `evals/agent-dx/{README.md,tasks/,harness/,reports/}` (ADR 0002 amended), `.github/workflows/agent-eval.yml` (`workflow_dispatch`, protected environment with the owner as required reviewer, budget and model inputs), `.github/workflows/ci.yml` (offline parts) | DX1 and DX2 for the skill and bootstrap conditions; task authoring may start now | offline in CI without secrets: schema check, gold solutions pass and hollow solutions fail every oracle, canned-transcript replay reports accounting, timeouts and termination; the runner refuses to start without `--budget-usd` and `--approved-by`, estimates spend first and stops at the cap; paid pilot (48 episodes, caps as in §17.146) runs only when the owner approves the workflow; the report gives per-task outcomes, raw counts, intervals, cost per accepted task, activation and retrieval failures, and lists every failed episode |
| **DX6** | First release prepared, awaiting the owner (§17.147) | Opus (workflow, installer logic); Sol (matrix, formula, wrapper, notices); Astra (`cargo-dist` evaluation, release docs) | `.github/workflows/release.yml` (tag `v*` or dry-run dispatch; no release created by a dry run), `tools/install/` or the `cargo-dist` configuration, `docs/release.md`, Homebrew tap formula and npm wrapper as reviewed files in the repository, `STABILITY.md` (owner), `CHANGELOG.md` | DX2 (the binary must carry `setup` and the skill) | a dry run produces the matrix's archives, checksums and the skill ZIP as workflow artifacts; on a fresh host per platform: download → `--version` → `new cli demo` → `check` → `test` → `build` → `mcp serve` smoke → `setup --apply`; the JS host prerequisite is stated; nothing is published by the workflow without the owner's tag and credentials |
| **DX7** | VS Code client over the LSP; tree-sitter after it (§17.148) | Opus (client, grammar); Sol (packaging); Astra (setup docs) | `editors/vscode/` (client, TextMate grammar converted from `website/lib/sakura/grammar.ts`, tests), later `editors/tree-sitter-sakura/` with a corpus from `compiler/tests/` | DX6's executable story (a configured or `PATH` `sakura`) | diagnostics with `/docs/errors/<code>` links, hole context, formatting and quick fixes with impact classes; tests for stdio start, root selection, UTF-16 positions, stale fixes, missing executable; installable from a local `vsce package`; marketplace publication only by the owner; the tree-sitter corpus agrees with the CST on `tests/` |
| **DX8** | Plugin packaging, playground, Linguist (§17.148) | Sol (plugin, catalogue); Opus (browser host, WASM checker or sandboxed service); Astra (Linguist evidence when real) | `plugins/sakura/`, `.claude-plugin/marketplace.json`; `website/app/playground/` and a browser host adapter; upstream Linguist files | plugin after DX1 has been stable for one release; playground after W2 parity and M4-OPT (`m3-completion-and-m4.md` §3.7 items 3 and 7); Linguist after the usage threshold is met | `claude plugin validate` passes and the plugin wraps the DX1 files unchanged; playground is bounded (worker, cancellation, memory limit), pure effects and `Console.captured` only, no Node bindings, pure examples and diagnostics first; no Linguist submission without genuine usage evidence |

## 2. Order and relation to M4

Dependencies in one line: DX0 now; DX1 now (its status lines after DX0); DX2 after DX1; DX3 and
DX4 with DX1's generator; DX5 task authoring now, conditions after DX1 and DX2, paid runs when
approved; DX6 after DX2; DX7 after DX6; DX8 as stated. The programme touches no crate M4 is
editing (`sakura_lir`, `sakura_codegen_wasm`, `runtime/rt`, `sakura_verify`); it touches
`sakura_cli` (`new.rs`, `setup.rs`, `main.rs`, tests), `sakura_mcp`, the website, `docs/`,
`skills/`, `evals/`, `editors/` and workflows. Opus's capacity is the only shared constraint:
`m3-completion-and-m4.md` §3.7 keeps its order (W2, L4, close-out, M4-OPT …), and DX1/DX2 are
scheduled beside W2 as the first agent-DX items, DX5's runner and DX6's workflow after L4.

## 3. Questions only the owner can answer

1. DX6: the supported platform matrix, the version string of the first tag (`v0.1.0-alpha.1`
   proposed), signing, the Homebrew tap and npm package names, and the credentials for each
   channel. Nothing publishes until these are given.
2. DX5: the pilot budget (dollars) and the two pinned model identifiers at run time; whether a
   replication is funded after the pilot's report.
   **Owner answer (2026-10-18):** up to **$10 total** on the owner's Anthropic API key
   (Anthropic models only), provided through `request_secret` when the runner is ready; no
   other paid runs. Build the tasks, oracles, transcript replay and the offline CI parts first.
3. DX7/DX8: the VS Code marketplace publisher and the plugin marketplace namespace.
4. DX2: whether `sakura new` should offer `--setup <client>` as a shortcut once `setup` exists
   (proposed: yes, same code path, same project-local rule).

**Owner answers, 2026-10-18 (items 1 and 3):**

- Platforms: matrix **A** first, macOS arm64 (`aarch64-apple-darwin`) and Linux x86-64
  (`x86_64-unknown-linux-gnu`); then **B** (+ Linux arm64), then **C** (+ Windows x86-64, macOS
  x86-64), each after the previous one ships.
- First tag: **`v0.1.0-alpha.1`**.
- Names: **`sakuralang`** wherever it can be had. The repository stays under the owner's account
  and is renamed `icantcodefyi/sakuralang` (done 2026-10-18; no GitHub organisation for now).
  crates.io `sakuralang`, npm `sakuralang`, Homebrew tap `icantcodefyi/homebrew-sakuralang`
  (formula `sakuralang`, installing the `sakura` binary), VS Code Marketplace and Open VSX
  publisher `sakuralang`, Claude plugin marketplace in this repository.
- CI runners: the owner proposes Blacksmith (free tier) for faster Actions; it needs the owner to
  install its GitHub App. Evaluate and switch the Linux jobs once installed.
- Signing and credentials: still open.

## 4. Deviations recorded during implementation

### DX0 (Opus, `compiler_info`)

1. `sakura_mcp::Config` gains `backends` (the CLI passes `TARGETS`, the same constant its
   `--target` flags accept) and `solver_status` (the CLI's `verify::solver_status`).
   `features.verify` is `true` whenever the binary has `sakura verify`; the solver status is a
   separate `features.solver` object: `name` (`z3`), `pinned` (`5.3.1`), `available`, and
   `version` + `path` when the pinned Z3 is on `PATH`, else `detail` (the reason `sakura verify`
   would give). It is probed on each `compiler_info` call (one `z3 --version`).
2. `tests/mcp.rs` reads the `--target` values from `sakura build --help` and compares them with
   `features.backends`, and compares `features.solver.available` with its own `z3 --version`.

### DX1 (Opus, generator and harness)

1. **Fence convention.** A Sakura fence's info string is `sakura <mode> id=<fixture-id>`, mode
   `check`, `test`, `run`, `hole-step`, `wrong:<code>` or `fragment` (no `id` needed). Other
   fences need a language; `json` and `toml` bodies must parse; a bare fence fails.
   Expectations live beside the harness in `compiler/tests/agent_docs/`: `<id>.stdout` (`run`),
   `<id>.span` (the text under a `wrong` sample's primary span) and `<id>.fixed.sakura` (a fix
   that is not itself a fence; otherwise the fix is the fence with the same `id`). Unused files
   there fail the test; `SAKURA_BLESS=1` writes `.stdout`/`.span`. The header of
   `sakura_cli/tests/agent_docs.rs` is the reference.
2. **`references/stdlib.md` is byte-equal to `compiler/std/INDEX.md`.** The generator rewrites
   only INDEX's "Signatures" section (between two `agent_docs` comment markers) from the
   analysis's symbol records over the embedded std (`std_sources()`, the records `sakura doc`
   prints): modules in path order, public items in source order, contracts and attribute names
   (`// @external, @total`). The prelude, conventions, dependency and deviation sections stay
   authored. Compared with the hand listing it adds `std/http`'s `type Body` and
   `read_chunk`'s `requires`, renders rows in the checker's order (`{Budget, Http}`) and drops
   the free-text notes (`List.set`'s complexity); a task-oriented index is not generated (the
   authored prelude/conventions sections serve it until Astra asks for more).
3. **Commit fill.** `skill_bundle()` in `sakura_cli/src/main.rs` rewrites `SKILL.md`'s
   `sakura-compiler-version` (crate version) and `sakura-compiler-git` (`SAKURA_GIT_COMMIT` at
   build time; `unknown` in a local build) inside the
   frontmatter. No `sakura` command writes the bundle until DX2's `setup`
   (`expect(dead_code)` marks it); a unit test pins it to the source tree.
4. **The harness found a broken sample**: `docs/llms.txt`'s syntax screen failed `V3001` (`main`
   lacked `Fail<UserError>`) and declared a `Sql` it never performed (`sakura test` then fails
   `V9002`). Its `main` row is now `{Console, Log, Fail<UserError>}`, the fence is
   `sakura test id=llms-syntax-screen` and the std listing fence is `text`.
5. Fixture projects have an empty `[permissions]` and no cassettes; a sample that needs either
   gets a sidecar when it exists. ZIP, checksum, mirror and `manifest.json` checks join
   `agent_docs.rs` with Sol's mirror. Astra's `SKILL.md` used `sakura id=X mode=test`; its four
   fences were rewritten to `sakura test id=X` (her entry asked for a mechanical adaptation), and
   her `@SAKURA_COMPILER_VERSION@`/`@SAKURA_COMPILER_GIT@` placeholders stay in the source.
6. `cargo test --workspace` also fails on stale outputs (`agent_docs`'s `outputs_are_fresh`),
   beside the CI step `cargo run -p agent_docs --locked -- --check`.

### DX1 (Sol, website distribution)

1. `agent_docs` writes `website/public/skills/sakura/` byte-for-byte, plus sibling
   `sakura-skill.zip`, `sakura-skill.sha256` and `manifest.json`. ZIP32 uses STORE,
   sorted UTF-8 paths under `sakura/`, 1980-01-01 timestamps and no host attributes,
   extras or comments. It rejects ZIP32 size/count limits; no compression dependency.
   The source placeholders remain unchanged in the raw mirror and ZIP; the CLI's
   build-time metadata fill remains separate, as required by byte equality.
2. **Content-only distribution (manifest-history fix).** The committed manifest
   omits `commit`; the old last-canonical-input commit changed after rebasing over
   source edits, even when final bundle bytes were unchanged. `compiler_version`
   comes from the workspace package version; `files` maps every skill-relative path
   to its SHA-256, and `sha256` hashes the archive. Schema version remains 1 in this
   pre-alpha; consumers must use hashes rather than the removed `commit` field.
   Generation and drift checks never query Git and need no history. Regenerate
   whenever content changes, in the same commit as sources if desired. Release
   provenance belongs at release/build time, as with the CLI's `SAKURA_GIT_COMMIT`.
   No per-commit website copies or release assets are published.
3. `agent_docs --check` compares binary outputs and rejects orphan public files;
   regeneration removes them. The CLI harness independently checks ZIP layout,
   mirror bytes, hashes and manifest metadata. Website formatting excludes this
   generated directory so it cannot rewrite the canonical skill's bytes.

### DX4 (GPT-6-Astra, recipes and extraction/rendering)

1. **Ownership override.** The owner assigned both authoring and extraction/rendering
   to Astra in `wt/dx4`. Eight new projects are `json_refined`, `cli_files`,
   `http_retry`, `sqlite_query`, `router_endpoint`, `scripted_tool`, `budgeted_agent`
   and `indexed_update`. All use supported APIs; none was dropped, skipped or
   borrowed from the DX5 evaluation tasks. The first, second and last use `run`
   plus tests; the other five use `test`. SQLite is a real fresh in-memory database;
   the router is invoked directly without a server.
2. **One authored source.** Each project's `recipe.md`, manifest, source and sidecars
   feed `agent_docs::cookbook`; `compiler/examples/cookbook.md` records the initial
   API baseline and regeneration instructions. The generator writes nine committed
   cookbook pages, navigation metadata and `references/recipes.md`, rejects orphan
   pages and refuses recipes without tests or with a skipped harness mode. This
   avoids Git-HEAD-dependent generated bytes: the initial API baseline is explicit,
   while CI checks the current compiler and installed bundle metadata identifies
   its build. The compiler bundle includes the recipe reference.
3. **Project-aware skill fences.** Complete recipe fences use
   `sakura test id=cookbook-<name> project=<name>`. The skill harness checks exact
   source equality, copies only the manifest/data/fixture sidecars into a fresh
   project, then checks and tests it. Names cannot escape `compiler/examples/`;
   other fence modes reject project metadata. This extends DX1's convention because
   a bare source file cannot replay a cassette. The examples harness independently
   exercises each project's declared mode and expected stdout.
4. **Finite budgets need HTTP-level replay.** `Llm.replay` and `Llm.recording`
   install `Budget.unlimited` internally. The finite-budget recipe instead wraps
   the provider with `Budget.limits`, `Http.scripted([])` and `Http.recording`;
   it uses a placeholder `Secret`, no API key or host HTTP handler. Tests verify
   successful accounting and a second-call rejection, an impossible token reserve
   and the final-turn rule. The provider and agent hold overlapping reservations:
   two completed calls need a transient call limit of three. This existing behavior
   is documented rather than changed. All cassettes are hand-authored synthetic
   exchanges, and no recipe grants net/env access.
5. **Integration.** Generator and harness coordination is recorded in
   `compiler/REQUESTS.md`; DX3 and the website skill mirror are preserved on rebase.
   The website uses its existing static MDX/search/Markdown-export pipeline. No new
   dependencies, runtime APIs, snapshots or language contracts are introduced.
   AI assistance: GPT-6-Astra authored recipes, prose, generation and checks; the
   human contributor owns acceptance.
6. **Mirror rebase.** Integrated Sol's `d552545` binary-output pipeline and independent
   ZIP/mirror/hash checks. Cookbook outputs enter the in-memory skill before the
   distribution is rendered, so a single invocation includes the fresh recipes.
   The recipe reference's canonical inputs are under `compiler/examples`. The
   original source-first, separate-regeneration commit procedure is superseded by
   DX1's content-only manifest fix above; a single commit can include both.

### DX3 (Opus, also the Sol and Astra parts)

1. **90 pages, not 73.** §17.144 counted the `V`/`W` codes; the registry also holds 17 runtime
   defects (`D…`), which get pages too. A defect page says there is no compile-time fixture
   instead of showing an empty minimal pair; the retired `V5002` says it is never reported.
   `index.mdx` lists the codes by phase (the §12.3 digit groups, then warnings, then defects);
   `meta.json` is generated as well, so every file in `website/content/docs/errors/` is an
   output, and a file there that the generator does not write (a page without a code) fails
   `--check` and `outputs_are_fresh`. The pages live under "Reference" in the sidebar, after
   the standard library. The "generated" note is a YAML comment in the frontmatter, because an
   MDX comment leaked into the `.md` export.
2. **Which fixture.** A code with several `tests/ui` fixtures shows the first (by name) that has
   a `.fixed.sakura` companion, else the first, and links the others. A directory fixture
   (`V1030_import_cycle/`) shows each file and says the ui suite does not run it yet; so does
   `W8001` (milestone M5). The wrong program is shown without its harness directives (other
   leading comments stay). The diagnostic is the fixture's `.jsonl` record for the code,
   pretty-printed in the contract's key order; fixtures run through `// command:` (no `.jsonl`)
   say which command reports the code. `V0008`'s 10,000-byte nesting is linked, not inlined (a
   page inlines up to 2,000 bytes).
3. **Companions: 64**, one for every code whose fixture the ui suite runs (all of them; no
   frequency data exists to order them, so none was left out). Each is the wrong fixture with
   the minimal repair, `sakura fmt`-clean (`fmt_clean.rs` covers `tests/ui`), and `ui.rs` runs
   it with the wrong fixture's command (`check`, `run`, `build`, `test --contracts`) and
   requires exit 0 with no error or warning. A companion without its wrong fixture fails the
   suite. Missing: `V1030` (directory fixture), `W8001` (not reported before M5), and the codes
   with no fixture (`V5002`, `V8002`–`V8004`, `V8020`, `V9001`, `W7002`, the defects).
4. **Link check.** Index links are built from the same registry walk that writes the pages, and
   `--check` fails on any page/code mismatch, so a dangling index link cannot be committed; no
   separate crawler was added. The website build renders every page (MDX escaping of `{`, `}`,
   `<`, `>` and `*` outside inline code is the generator's job; a unit test covers it).
5. **`sakura explain` prints `https://www.sakuralang.com/docs/errors/<code>`** last, the
   canonical `site.url` of `website/lib/site.ts` (the live domain since `f1de9f4`). `cli.rs` asserts
   the printed base appears in `site.ts`, so a domain change fails a test until both move.
   `explanation_uri` is unchanged (`sakura://explain/<code>`).
6. **Explanations expanded (32 codes)** that had no repair or minimal pair: `V0004`, `V1001`,
   `V1003`–`V1005`, `V1011`, `V1012`, `V1020`, `V1041`, `V2002`–`V2005`, `V3002`, `V3010`–`V3012`,
   `V3020`, `V4003`, `V5001`, `V6010`, `V9002`, `W1002`, `W4010`, `W8001`, `D004`, `D011`, `D020`,
   `D021`, `D030`, `D031`, `D091`. The pairs follow the fixtures and companions (each sample
   was parsed with the CLI); `skills/sakura/references/diagnostics.md` regenerates with them.

7. **Manifest history follow-up (GPT-6.1-Sol).** The registry edits exposed DX1's
   history-dependent `commit` field: rebases changed its expected value without
   changing bundle bytes. DX1's content-only distribution now removes that field
   and lookup; the CLI harness still checks every mirror byte, ZIP layout and
   file/archive hash, and rejects a committed `commit` field. Website and skill
   docs distinguish this content manifest from CLI build-time provenance. History
   verification uses an unrelated throwaway commit and a bundle-source edit/revert
   pair, requiring unchanged distribution bytes and a green `agent_docs --check`.
