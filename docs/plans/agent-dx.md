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

(none yet; append per workstream as `performance-r1.md` §8 does)
