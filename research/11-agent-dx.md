# Agent skills and the adoption path for Sakura Lang

Research and source access date: **2026-10-18**. Audience: **Fable, language designer**.
Repository inspected: `ffca8a4bf7ad45189e57aa1370c156a44042c834` on `wt/agentdx`.
This is research and a proposed implementation sequence, **not an accepted language,
CLI, release, or security contract**. All external sources below were fetched on
2026-10-18; client behavior must be rechecked before shipping. AI-assisted research
and prose by GPT-6-Astra; the human contributor owns review and acceptance.

## 1. What exists first

The fastest improvement is to package and accurately describe the existing compiler,
then measure whether agents use it. Sakura does not need a second compiler-facing
MCP server or a second language reference.

The requested design sections were inspected: [§0, §11 and §12](../docs/design.md).
In this revision, §11 describes language-level AI features; **CLI, diagnostics,
holes and compiler MCP are in §12**. The design is provisional under
[STABILITY.md](../STABILITY.md). Root [AGENTS.md](../AGENTS.md) still says there is
no Rust workspace; [CONTRIBUTING.md](../CONTRIBUTING.md),
[compiler/AGENTS.md](../compiler/AGENTS.md) and the actual tree show one, pinned by
`compiler/rust-toolchain.toml`. That documentation disagreement is itself an agent
onboarding defect. This audit follows implementation for availability claims and
labels design goals separately.

| Surface | Verified in source today | Boundary or adoption gap |
| --- | --- | --- |
| Source installation | `cargo install --locked --path crates/sakura_cli` from `compiler/`; std is embedded. [Quickstart](../website/content/docs/quickstart.mdx). | No supported compiler release/installer is documented; no release workflow was found. Do not advertise a registry package or supported binary matrix. |
| Project scaffolds | [new.rs](../compiler/crates/sakura_cli/src/new.rs): `new <cli|lib|agent|api> <name>` and `init --kind <kind>` (default `cli`) create `sakura.toml`, `sakura.lock`, `src/main.sak`, `.gitignore` containing `/target/`. Each template has a test. | No `AGENTS.md`, `CLAUDE.md`, skill or client MCP configuration. `new` refuses nonempty destinations; `init` preserves existing individual files but **refuses an existing `sakura.toml`**. It is not a repeatable setup updater. Neither initializes Git. |
| Permissions in scaffolds | Manifest includes `[permissions]`; cli/lib leave it empty; api grants its loopback listener; agent grants the provider endpoint and an environment-key name. | Do not describe all scaffolds as zero-authority. The agent template contains a particular provider/model string; it is an implementation default, not a verified current provider recommendation. |
| Language reference | [docs/llms.txt](../docs/llms.txt) is embedded into [CLI main.rs](../compiler/crates/sakura_cli/src/main.rs). `sakura prompt` already prints it. | `--budget` uses four bytes per estimated token and drops trailing whole sections; it is not a tokenizer or retrieval of requested signatures. The design's extra-signature behavior is not this implementation. |
| Offline lookup and explanations | [doc.rs](../compiler/crates/sakura_cli/src/doc.rs) implements symbol/type search; `doc --json` is one JSON object. `explain` looks up [codes::REGISTRY](../compiler/crates/sakura_diagnostics/src/codes.rs), case-insensitively, and prints title, phase and explanation; unknown codes exit 2. | No `sakura doc --index`. Many registry explanations have wrong/fixed pairs, but some are advice or fragments; do not assume every explanation contains a compilable program. |
| Check/repair loop | `check --json`, typed holes, formatting, fixes and frozen-spec comparison exist. JSON diagnostics include schema, snapshot/content hashes, byte spans and fix impact. [Diagnostics](../website/content/docs/agents/diagnostics.mdx), [holes](../website/content/docs/agents/typed-holes.mdx). | `check` accepts holes, so compilation-only acceptance is insufficient. Freeze signatures/tests/permissions against a baseline; a successful repair must not weaken the task. |
| Compiler MCP | [sakura_mcp](../compiler/crates/sakura_mcp/src/lib.rs) serves **stdio** through `sakura mcp serve [--root PATH]`: `compiler_info`, `check`, `symbol_query`, `hole_context`, `docs_query`, `explain_diagnostic`, `format`, `check_patch`, `apply_patch`, `test`. [docs/mcp.md](../docs/mcp.md) already has client setup and a workflow. | There is no compiler HTTP transport or `--tools` function-export flag in the inspected CLI. MCP result objects are not CLI JSONL streams. `frozen_specs` is optional on patch calls: callers must supply it to enforce freezing. |
| MCP retrieval | `docs_query` ranks embedded `docs/llms.txt`, `compiler/std/INDEX.md` and diagnostic registry explanations; it optionally retrieves symbols. | Its default 1,500-token budget is also a byte estimate. `compiler_info` advertises JS and `verify: false`; the CLI has newer WASM/verify capabilities. Treat the field as the adapter's current advertised surface, not a complete global feature inventory. |
| LSP | [sakura_lsp](../compiler/crates/sakura_lsp/src/lib.rs): stdio diagnostics, hover, definitions, completion, formatting, quick fixes, `sakura/holeContext`; UTF-8 negotiation with UTF-16 fallback. | No standalone VS Code extension or tree-sitter package was found. The website has a highlighting grammar, not an editor integration. |
| Assurance tools | Implemented [audit](../website/content/docs/agents/audit.mdx), [offline doc](../website/content/docs/agents/doc.mdx), [verify preview](../website/content/docs/agents/verify.mdx), [build manifest](../website/content/docs/agents/build-manifest.mdx). | Audit is an inventory, not certification. Verify can exit 0 with unknown/unsupported obligations; inspect statuses. Build manifests describe the actual build and can be stale after a failed build. |
| WASM | [wasm.rs](../compiler/crates/sakura_cli/src/wasm.rs), `sakura_codegen_wasm`, `sakura_lir` and `sakura_rt` exist; emitted artifacts include a **Node host**. | A WASM program backend does not imply a compiler running in a browser or browser-compatible host effects. Playground work is not simply waiting for the first backend commit. |
| Reusable examples/tests | `compiler/examples/`, `compiler/tests/ui`, `compiler/tests/semantics`; [examples harness](../compiler/crates/sakura_cli/tests/examples.rs), CLI/LSP/MCP integration tests. The scaffold test checks/formats/tests all four templates. | Existing fixture modes, milestones and known-bug skips matter. `compiler/tools/sakura_eval` is a Core/LIR evaluator, **not an LLM agent benchmark**. No agent task/evaluation suite or distributable `SKILL.md` was found. |
| Repository instructions | Root `AGENTS.md`, compiler/website guidance, root `CLAUDE.md` importing `@AGENTS.md`, and [REQUESTS](../compiler/REQUESTS.md). | These teach contributors how to change Sakura itself. They are not a project template for agents writing applications in Sakura. |

### Website exports: reuse them, but preserve skill bytes

The inspected [llms-txt page](../website/content/docs/agents/llms-txt.mdx) correctly
separates the **site index/export** from the **compiler's language guide**. Implementation:

- `website/app/llms.txt/route.ts` returns `llmsTxt()` as UTF-8 plain text;
  `website/app/llms-full.txt/route.ts` concatenates processed documentation.
- [next.config.mjs](../website/next.config.mjs) rewrites `/docs.md` to
  `/llms.mdx/docs/content.md`, and `/docs/:slug*.md` or `.mdx` to
  `/llms.mdx/docs/:slug*/content.md`.
- `website/app/llms.mdx/docs/[[...slug]]/route.ts` removes the appended
  `content.md`, normalizes a trailing `index`, finds the Fumadocs page, and returns
  `text/markdown; charset=utf-8`; missing pages use `notFound()`.
- [source.ts](../website/lib/source.ts) reads processed MDX, flattens registered
  components, and prepends a title, URL, Markdown URL and description. This is a
  useful readable export, **not byte-preserving delivery of a skill with YAML
  frontmatter**.

Specific drift worth fixing before distribution: `source.ts` says the M3 std library
is not built; the llms-txt page simultaneously calls `prompt` implemented and future,
and calls `doc --search` planned; the [MCP page](../website/content/docs/agents/mcp.mdx)
calls transport/options and `compiler_info` fields unsettled despite the stdio
implementation. Its unconditional frozen-spec wording omits the optional parameter.
The diagnostics page says no phase-7 codes are named while the verify page documents
`W7002`. Reconcile these against source/tests; this research does not edit them.

## 2. Agent skills and instructions today

### Portable format and size

The [Agent Skills specification][skills-spec] requires `SKILL.md` with YAML
frontmatter followed by Markdown. Required fields: `name` (1–64 characters;
lowercase letters/digits/hyphens, no edge or consecutive hyphens, matching its
directory) and `description` (1–1,024 characters). Optional fields include
`license`, `compatibility` (up to 500 characters), string-valued `metadata`, and
experimental `allowed-tools`. `references/`, `scripts/`, and `assets/` are optional
conventions, not required folders.

Progressive disclosure means metadata first (approximately 100 tokens), the body
when activated (under 5,000 tokens recommended), and supporting files on demand.
Keep the body under 500 lines and link resources directly from it. These are
recommendations, not a universal runtime truncation rule. [Specification][skills-spec].

Proposed Sakura frontmatter, **an illustration, not an installed skill**:

```yaml
---
name: sakura
description: Write, check, repair, and test Sakura Lang programs. Use for .sak files, sakura.toml, typed effects, holes, and Sakura compiler diagnostics.
license: MIT OR Apache-2.0
compatibility: Requires a matching source-built Sakura CLI; running JS programs also requires the supported JavaScript host.
metadata:
  version: "0.1.0-draft.1"
  sakura-compiler-git: "ffca8a4bf7ad45189e57aa1370c156a44042c834"
  sakura-edition: "2026"
---
```

Those `metadata` keys are **our proposed convention**, not standard version
negotiation. Omit tool pre-approval and client-specific execution hooks from the
portable package. A language guide should not grant permissions merely by loading.

### How clients consume it

| Client | Instructions that anchor a project | Skills/distribution today |
| --- | --- | --- |
| Claude Code | `CLAUDE.md` supports `@AGENTS.md` imports. Current docs also describe built-in `AGENTS.md` support with version/settings caveats; keep the import for compatibility. | `.claude/skills/sakura/SKILL.md` in a project, `~/.claude/skills/` personally, or `<plugin>/skills/`. Automatic selection uses descriptions; `/sakura` invokes explicitly; plugin invocation is namespaced. [Skills][claude-skills], [memory][claude-memory]. |
| OpenAI Codex | `AGENTS.md` is directory-scoped; `AGENTS.override.md` takes precedence at a level. Project guidance has a default 32 KiB combined limit. | Skills **do exist**. Current docs scan `.agents/skills` from the working directory to repository root, and `~/.agents/skills` for user skills; explicit `$sakura` invocation is supported. `agents/openai.yaml` is optional UI/policy/dependency metadata, not needed for a portable guide. [Instructions][codex-agents], [skills][codex-skills]. |
| Cursor | Root/nested `AGENTS.md`, or scoped `.cursor/rules/*.mdc` with `description`, `globs`, `alwaysApply` frontmatter. | Current Cursor also supports Agent Skills under `.agents/skills` and `.cursor/skills`, with user equivalents. Local user skills do not automatically travel to every remote/cloud session. [Rules][cursor-rules], [skills][cursor-skills]. |

Codex's installed CLI here reports `codex-cli 0.163.0`; that is a local observation,
not a proposed minimum version. No client settings were changed or skills installed.
The official OpenAI URLs fetched during research redirected to ChatGPT Learn;
links below retain their canonical destination.

For Cursor, `.cursor/rules/sakura.mdc` could contain this **proposed adapter**,
not a replacement skill format:

```text
---
description: Use the project's Sakura workflow for Sakura source and manifests.
globs: "**/*.sak,**/sakura.toml"
alwaysApply: false
---
Read AGENTS.md and use the installed sakura skill before changing Sakura code.
```

Prefer a single short canonical `AGENTS.md` to three divergent language manuals.
Target **300–600 tokens of project instructions and 1,500–2,500 tokens in the
skill body** initially; these are proposed budgets to evaluate, not vendor limits.
The existing language guide can remain a separate reference. Count actual tokens
with the selected evaluation model's tokenizer, and separately report bytes/lines.

### Claude plugins and marketplaces

A marketplace repository contains `.claude-plugin/marketplace.json`; a plugin has
its own `.claude-plugin/plugin.json` and `skills/` beside that directory. A
marketplace lists plugins; it is not itself a skill. Claude's `/plugin` UI manages
installation; `/plugin marketplace add <owner>/<repo>` registers a catalog and
`/plugin install <plugin>@<marketplace>` opens its installation flow. CLI equivalents
include `claude plugin marketplace add` and `claude plugin install`.
[Marketplace docs][claude-marketplaces], [plugin reference][claude-plugins].

Illustrative **future** catalog at `.claude-plugin/marketplace.json`:

```json
{
  "name": "sakura-lang",
  "owner": { "name": "Sakura Lang maintainers" },
  "plugins": [{ "name": "sakura", "source": "./plugins/sakura" }]
}
```

Illustrative `plugins/sakura/.claude-plugin/plugin.json`:

```json
{
  "name": "sakura",
  "version": "0.1.0",
  "description": "Sakura Lang coding workflow",
  "author": { "name": "Sakura Lang maintainers" }
}
```

Generated content would be under `plugins/sakura/skills/sakura/`, giving
`/sakura:sakura`. These names/versions are examples; **no marketplace exists or is
published by this change**. Validate both directories with `claude plugin validate`
before offering installation. Keep the plugin version separate from the compiler
version and bump it for distributed content changes; do not expect a changed
Markdown file to invalidate every client's cached plugin automatically.

### Real projects, and what they demonstrate

| Official source | Artifact observed | Useful lesson and limit |
| --- | --- | --- |
| [Svelte AI overview][svelte-ai], [instructions][svelte-instructions], [skills][svelte-skills], [actual SKILL.md][svelte-skill-source] | An AGENTS.md starter, MCP documentation/static-analysis tools, on-demand skills, plugins and downloadable skill releases. The code-writer skill teaches documentation lookup and validation CLI commands. | Closest model for Sakura: small instructions plus a language-aware tool loop. Existence is not evidence of a particular success rate. |
| [Svelte's Markdown export][svelte-llms] | A readable `/llms.txt` export linked from its official docs. | Documentation retrieval and skill installation are separate entry points. |
| [Vercel's official skills][vercel-skills], [React skill source][vercel-react] | A real `SKILL.md` with targeted triggers and a prioritized rule index, backed by focused rule files. | Copy the selective reference structure and concrete good/bad examples, not React-specific rules or an enormous always-loaded checklist. |
| [Rust compiler AGENTS.md][rust-agents] | Repository contributor instructions linked to project contribution policy. | Evidence that language projects publish agent guidance; **not** a skill for teaching application authors Rust. Keep Sakura's compiler-contributor guide separate from its language-user guide. |

The [llms.txt proposal][llms-proposal] is a Markdown navigation convention. It
neither installs skills nor guarantees that every coding agent discovers a site.
Link the skill download and versioned references explicitly from the website's
agent landing page and llms index.

### What makes the Sakura skill useful: evidence versus hypotheses

**Measured in another setting:** Vercel's Next.js evaluation reported 53% passing
without docs, 53% with default skill availability, 79% with explicit skill-use
instructions, and 100% with an always-available docs index. The skill was not
invoked in 56% of cases; the index was compressed to 8 KB. This is one vendor's
framework/task/harness result, not a general proof that AGENTS.md beats skills.
It motivates measuring **discovery and activation**, not just testing a body
forcibly injected into context. [Evaluation report][vercel-eval].

**Research caution:** the SWE-Skills-Bench preprint pairs skills with pinned
repositories and executable requirements. It reports little average benefit and
some degradation from version-mismatched guidance. Its tasks are not Sakura;
use its paired with/without-skill methodology, not its scores as a prediction.
[SWE-Skills-Bench][swe-skills].

**Proposed content, to test:**

1. A one-screen syntax/prelude cheat sheet prevents TS/Rust syntax transfer:
   explicit top-level signatures, pure omission of `!`, positional calls, binding
   annotations instead of call-site generics, explicit non-prelude imports.
2. A numbered workflow makes tools actionable: identify compiler → retrieve
   signatures → write signatures/holes → check → fill → test → build → report.
   Teach CLI fallback when MCP is absent; prefer querying symbols over guessing.
3. Short complete examples teach interactions that a grammar cannot: `Fail`
   recovery, local handlers, refinement entry checks, deterministic fake effects.
   Link larger recipes instead of placing all examples in the body.
4. An error→cause→repair table starts with frequent codes from actual evaluation
   failures, backed by `sakura explain` and tested minimal pairs. For `V3001`,
   explain the distinction between handling locally and changing the signature;
   do not always recommend widening the row. `V2050` means unfinished code, and
   `V8020` means change the implementation rather than the frozen task.
5. Completion requires no holes, passing independent tests and no unauthorized
   contract/permission change. Report audit/verification evidence accurately.

The Agent Skills [authoring guidance][skills-best] supports actionable steps,
examples, selective references and evaluation. It does **not** establish the ideal
cheat-sheet/example/error-table ratio for a new language. Ablate those components
in §5 rather than presenting preference as a result.

## 3. Distribution and compiler-version synchronization

### One source package, several delivery adapters

Proposed source tree (all new paths below are design suggestions):

```text
skills/sakura/
  SKILL.md
  references/language.md
  references/stdlib.md
  references/diagnostics.md
  references/recipes.md
  assets/AGENTS.md
  assets/CLAUDE.md
  assets/mcp-setup.md
```

Start without executable install scripts in the skill. The installed compiler
already supplies deterministic checking, explaining, formatting and testing.
A `scripts/` directory becomes justified only for a repeated workflow not already
covered by that CLI. Do not use a skill hook to download or execute code implicitly.

Generate a byte-identical mirror under `website/public/skills/sakura/`, so
`/skills/sakura/SKILL.md` starts with YAML frontmatter and sibling relative links
work. The existing `/docs/*.md` transformation must not wrap it. Add a complete
ZIP preserving the `sakura/` directory and references; downloading only SKILL.md
would leave a broken package. Serve checksums and immutable URLs such as
`/skills/by-commit/<git-sha>/sakura/SKILL.md` during pre-alpha; after actual releases,
add `/skills/by-version/<compiler-version>/sakura/`. The unversioned URL is a
convenience pointer with explicit provenance, not a reproducibility guarantee.

A user can extract/copy the same package into `.agents/skills/sakura/` for
Codex/Cursor or `.claude/skills/sakura/` for Claude. Maintain one source and
mechanically produce adapters, rather than hand-editing three copies. Plugin
packaging is a later convenience layer. Website hosting a SKILL.md alone is not
an auto-install protocol, and no universal `sakura skills install` command exists.

### What should be generated and what should stay authored

| Input already in repo | Proposed output/use | Guard against drift |
| --- | --- | --- |
| `docs/llms.txt` | Compiler-matched language reference; a curated short skill body links to it. | Bundle the exact source revision; compare to `sakura prompt` from that build. Do not fetch `master` in a pinned project. |
| `compiler/std/INDEX.md` and public std declarations | `references/stdlib.md`, indexed by module and task; retrieve full signatures through `doc`/MCP. | INDEX says its listing is regenerated by hand today. Add deterministic extraction/checking; copying the index alone does not prove it matches std. |
| `sakura_diagnostics::codes::REGISTRY` / `sakura explain` | Complete versioned code-reference pages and a curated short repair table. | Export through the Rust registry rather than regex-parsing Rust string literals. Preserve retired codes; record which examples are fragments. |
| `compiler/examples/` and checked fixtures | Runnable recipe snippets and negative/fixed pairs. | Preserve existing run/test/check modes, required permissions and prerequisites. No silently skipped example may be advertised as working. |
| Compiler version, Git SHA, edition, diagnostic schema | Bundle provenance and a compatibility page. | Crate version alone does not distinguish rapidly changing unreleased commits; edition `2026` is not a precise compiler compatibility selector. |

A small repository generator, proposed at `tools/agent-docs/`, can call an internal
registry exporter and assemble the bundle. Do not add a public JSON flag to
`explain` merely because the generator needs structured input. Future release
automation should package compiler, docs, skill and examples from **one commit**.
Generated output must be deterministic and checked for a clean regeneration diff.
A mismatch should tell the user which docs to obtain, not silently rewrite source
or upgrade their compiler.

### Acceptance test: examples compile, and repairs preserve intent

Proposed `compiler/crates/sakura_cli/tests/agent_docs.rs`, reusing its existing
`common` helpers and fixture conventions, plus `compiler/tests/agent_docs/`:

- Every advertised complete Sakura code block has a fixture ID, source revision
  and expected mode. Prefer rendering docs from those fixtures. Track fragments,
  intentional holes, and pseudocode explicitly; reject unclassified code fences.
- Run **`sakura check --json <fixture>`** on every positive complete example.
  Require exit 0, no error diagnostics, no unexpected holes, and a final summary.
  A hole tutorial expects holes at its intermediate step and none in its solution.
- For wrong/fixed pairs, assert the wrong example's expected diagnostic code and
  relevant span; check the repaired example and run its behavior tests. Do not
  normalize away new errors, contracts or permissions to keep a snapshot green.
- Run examples promising behavior with `sakura test --json` and, where appropriate,
  build/run with the fixture's manifest. Networked examples use scripted/replayed
  handlers; source installation and checking do not require a provider API key.
- Validate frontmatter, relative references, JSON/TOML client snippets, ZIP layout,
  checksums and byte equality between package and website download. Check generated
  error-page coverage against the registry and unknown-code handling.
- Make these deterministic tests part of ordinary CI. Paid model evaluation is a
  separate gate (§5). Existing examples/CLI suites provide scaffolding, not proof
  that the future skill samples already pass.

## 4. What `sakura init` / `new` should give an application project

**Recommendation for Fable to decide:** add a short application-specific
`AGENTS.md` and `CLAUDE.md` import to new projects, together with an offline
`docs/agent-setup.md` containing exact client snippets and compiler provenance.
Keep existing user files intact. An explicit agent-setup option can install selected
project-local adapters; do not alter global dotfiles, enable plugins, grant trust,
or replace other MCP servers as a side effect of scaffolding.

No new flag name is specified here. An updater must be a separately accepted CLI
behavior because today's `init` refuses an existing manifest. Tests should cover
fresh scaffolds, existing instruction files, repeated setup, malformed configs,
name collisions, paths with spaces, and a client started outside the project.
Merge only the Sakura-owned server entry when requested; otherwise print a patch.

Suggested always-loaded project guidance, **proposed template prose**:

```text
This project uses Sakura Lang. Check the installed compiler version and read
`sakura prompt` before writing source. Use the sakura skill when available.
Look up unfamiliar APIs with `sakura doc` or MCP `symbol_query`.

Run `sakura check --json`, `sakura fmt --check`, and `sakura test --json`.
A successful check may still contain holes; finish them before the final build.
For repairs, freeze the agreed signatures, contracts, tests, and permissions
against the task's baseline commit. Use `sakura explain CODE` for diagnostics.
Do not weaken contracts or broaden authority to pass. Report interface changes.
Audit inventories trust; verify statuses describe individual obligations.
Neither a successful check nor exit 0 from verify certifies the whole program.
```

`CLAUDE.md` should contain the actual import `@AGENTS.md`, not just an English
request that the model may ignore. This is already the pattern in Sakura's own
repository; current Claude documentation explicitly supports it. [Memory][claude-memory].

### Exact current MCP client formats

These are **config examples for existing `sakura mcp serve`**, not claims that
`init` already writes them. `sakura` must be on the client's PATH (or `command`
must name the installed executable). `/absolute/path/to/project` is a placeholder
to substitute, not a magic client variable. The compiler `--root` flag makes the
workspace explicit; clients should verify `compiler_info.workspace_root` after
connecting. These examples contain no credentials.

**Claude Code — project `.mcp.json`:**

```json
{
  "mcpServers": {
    "sakura": {
      "type": "stdio",
      "command": "sakura",
      "args": ["mcp", "serve", "--root", "/absolute/path/to/project"]
    }
  }
}
```

Or, from the chosen project, the current CLI form is:

```sh
claude mcp add --transport stdio --scope project sakura -- sakura mcp serve --root /absolute/path/to/project
```

Project settings are `.mcp.json`; local/user MCP entries live in `~/.claude.json`,
not `~/.claude/settings.json`. Interactive project servers have an approval flow;
setup should not preapprove it. Claude's `${CLAUDE_PROJECT_DIR}` is set in the
spawned process; ordinary `.mcp.json` expansion cannot simply assume it exists in
the parent's environment. An absolute-root snippet avoids that ambiguity.
[Official MCP docs][claude-mcp].

**Codex — project `.codex/config.toml` (trusted projects), or user
`~/.codex/config.toml`:**

```toml
[mcp_servers.sakura]
command = "sakura"
args = ["mcp", "serve", "--root", "/absolute/path/to/project"]
```

The documented CLI alternative is `codex mcp add sakura -- sakura mcp serve --root
/absolute/path/to/project` (one shell command). Do not translate the JSON
`mcpServers` shape into this file or assume Cursor's interpolation works here.
Use project-local configuration for project-specific roots. [Official MCP docs][codex-mcp].

**Cursor — project `.cursor/mcp.json`, or user `~/.cursor/mcp.json`:**

```json
{
  "mcpServers": {
    "sakura": {
      "type": "stdio",
      "command": "sakura",
      "args": ["mcp", "serve", "--root", "${workspaceFolder}"]
    }
  }
}
```

`${workspaceFolder}` is documented for project configuration and resolves to the
folder containing `.cursor/mcp.json`. Use an explicit root for a global entry
when a project root is unavailable. Cursor's current stdio field table requires
`type: "stdio"`, although some introductory examples omit it; include it.
[Official MCP docs][cursor-mcp].

For generated, shareable Claude/Codex configurations, Fable must choose between
an explicit machine-local root, a documented project-root launch convention, or a
small launcher that reliably supplies a root. Do not invent cross-client variables
or commit an individual developer's absolute path into a portable template.
First release can ship snippets and a copy/setup workflow while this is decided.

Smoke acceptance for all three clients: start a trusted scratch project with its
supported client version, list tools, inspect `compiler_info`, check a file with a
hole, retrieve its context, make a local checked patch and run its deterministic
test. Also start from a different directory to catch accidental-root selection.
Record versions and outcomes. JSON/TOML parsing alone is not an integration test.

## 5. A small agent evaluation suite

### What to measure and prior art

“Agent-first” should mean **more independently correct changes per bounded cost
with fewer repairs and fewer unauthorized specification changes**. Syntax validity
alone misses holes, failing tests, widened effects and authority escalation.

- [HumanEval and its harness][humaneval] distinguish independently sampled
  candidates and functional execution. For `n` independent candidates, of which
  `c` pass, estimate `pass@k = 1 - C(n-c,k)/C(n,k)` for `k <= n`; average across
  tasks. An iterative repair episode is not `k` independent candidates.
- [MultiPL-E][multiple] translates common tasks across languages, supporting a
  paired baseline while exposing translation/library confounds. Its original
  scope is Python plus 18 translations, not a novel-language repository benchmark.
- [CangjieBench][cangjie], a 2026 preprint with 248 manually translated
  HumanEval/ClassEval samples, compares direct, syntax-constrained, retrieval and
  agent settings in a low-resource general-purpose language. It reports a
  cost/accuracy tradeoff and negative transfer in some code-to-code tasks. This
  makes it closer to Sakura's problem than a Python-only leaderboard; it still
  does not establish Sakura performance or eliminate contamination of algorithms.
- [No Resource, No Benchmarks, No Problem?][no-resource] introduces three
  generation benchmarks around two recent languages and studies prompting and
  training interventions. It is relevant prior art for unfamiliar syntax, not
  evidence that Sakura should fund model training before testing a docs/tool loop.
- [SWE-Skills-Bench][swe-skills] offers paired requirements and executable
  acceptance. [research/04](04-agent-ergonomics.md) contains broader type/feedback
  and language-learning evidence; this proposal adds distribution and activation
  experiments rather than replacing that analysis.

### Proposed repository layout and initial tasks

Keep the suite separate from the existing semantic evaluator:

```text
evals/agent-dx/
  README.md
  tasks/<task-id>/prompt.md
  tasks/<task-id>/starter/
  tasks/<task-id>/oracle/
  tasks/<task-id>/task.json
  harness/
  reports/
```

`task.json` is a **proposed internal manifest, not an external standard**. Document
its schema before implementation: task ID/revision, category, supported compiler
revision/backend, starter hash, editable files, frozen baseline, oracle command,
expected diagnostics, episode budgets, and comparison eligibility. Keep oracle
files outside the agent-visible filesystem. Commit a baseline inside each scratch
project so `--frozen-specs all --since <baseline>` has a real Git reference.

Start with **12 tasks**, two in each category, all using implemented JS behavior:

| Category | Two concrete tasks | Independent acceptance |
| --- | --- | --- |
| Learn syntax/data | Complete bounded clamp; transform/filter a list into a nominal result record. | Boundary/empty-input tests; no holes; original contracts intact. |
| Diagnose and repair | Fix missing import/type mismatch; repair effect error with a local handler under a frozen signature. | Expected initial codes, correct output and no new effect/permission grant. |
| Holes and retrieval | Fill a hole requiring a std function lookup; fill typed-error recovery using available constructors. | Type/effect constraints plus hidden success/failure tests. |
| Small application change | Add `/health` behavior to a router; extend a CLI argument transformation. | Call router directly; test pure CLI logic without live network. |
| Deterministic effects | Repair a scripted LLM test; process JSON/HTTP data with fake handlers and validation. | No provider call; invalid input and failed effect handled as specified. |
| Preserve assurance | Repair a contract violation without weakening it; repair permission-sensitive code using existing granted authority. | Frozen-spec gate plus out-of-band file/manifest checks and negative tests. |

Author and validate gold solutions before model runs; include deliberately bad
solutions to prove oracles reject hollow fixes, removed tests, `.trust` shortcuts,
permission broadening and unfinished holes. These are task-oracle tests, not
repetitions of implementation internals. Split recipes from held-out evaluation
variants so teaching the recipe does not simply reveal the test solution.

### Conditions, models and fairness

Run staged experiments rather than every Cartesian product:

1. **Baseline:** compiler/help and the same initial task context, without a Sakura
   skill; record which docs are available. A no-docs condition is a cold-start stress
   test, not the only fair competitor to a trained-in language.
2. **Guide:** baseline plus compiler-matched `sakura prompt` reference.
3. **Skill:** guide plus the proposed workflow/examples. Compare forced loading
   (content effectiveness) with normal discovery (activation effectiveness).
4. **Bootstrap:** skill plus short `AGENTS.md` directing retrieval/activation.
5. **MCP:** the best prior condition plus compiler MCP instead of the equivalent
   CLI access. Give equivalent semantic information and tool budgets, and record
   transport overhead. Do not claim an MCP benefit merely from giving it more docs.

Then ablate cheat sheet, examples and repair table one at a time on the failing
categories. Report task-level outcomes, not only a favorable combined score.

Use a pinned Opus-family coding model and a pinned GPT-6-Sol-family coding model
for the first cross-family replication; add an Astra-family model for a small
reasoning sensitivity check only if useful. **These are selection roles, not API
model IDs.** Record exact available provider model identifiers, revisions,
reasoning settings, sampling controls, client/harness versions and date. Select
supported identifiers at run time; no model availability or price is assumed here.

For TS/Python comparison, initially use the same **six portable tasks** drawn from
syntax/data, small-application and data-validation work, with matching I/O and
behavioral tests, explicit pinned runtimes and equivalent library availability.
Review translations manually and account for integer ranges, ordering and error
semantics. Supply matched documentation budgets and the same agent/tool budget;
also report normal ecosystem-assisted conditions separately. Keep Sakura-specific
hole/effect/frozen-spec tasks as a separate score, since neither TS nor Python has
the same native contracts. Do not infer intrinsic language superiority from
training exposure or one framework's HTTP scaffolding cost.

### Metrics, execution limits and CI

Record each fresh episode: task/condition/model IDs, source and bundle hashes,
initial/final diagnostic sets, hole counts, compiler/test calls, patch attempts,
retrieved documents, whether the skill activated, tokens (including cached usage),
latency, billed cost and termination reason. Store provider messages privately
unless reviewed/redacted; checked-in reports should not contain credentials.

Primary metrics:

- **One-shot pass@1** and **budgeted episode success** are separate. On-demand
  `pass@3` needs at least three independent fresh episodes per task; repair turns
  inside an episode never increase `k`.
- **Iterations-to-green:** edit→check/test cycles until independent acceptance.
  Report success fraction alongside conditional median/p90; count timeouts and
  exhausted episodes as failures/censored observations, not zero iterations.
- **Diagnostic-fix success:** proportion of seeded defects repaired with the
  expected code gone, hidden behavior tests passing and specs/authority preserved;
  track new diagnostics and recurrence separately.
- **Cost per accepted task**, tokens per acceptance, time to first green,
  activation/retrieval failures, unauthorized edits, and a small human review-time
  sample. Cost per accepted task is total spend (including failed episodes) divided
  by accepted tasks; report it as undefined when no task succeeds.

Proposed pilot caps: 12 tasks × 2 conditions × 2 models × 1 episode = **48 episodes**;
maximum 8 repair cycles, 12,000 model output tokens, 60,000 aggregate input tokens,
10 minutes, 2 concurrent workers per run, and a separately authorized dollar cap.
These are engineering starting points, not measured sufficient limits. Estimate
spend from current provider prices before execution; stop scheduling when the cap
cannot cover another episode. No unbounded retries or automatic model escalation.
After harness validation, a 3-sample replication of that matrix is 144 episodes.

Run each episode in a disposable filesystem/process sandbox with fixed toolchain,
limits and network denied for generated programs. The model API broker lives
outside the agent's project and owns credentials. External oracle snapshots/hash
checks supplement `--frozen-specs`: an adversarial agent must not be able to edit
its acceptance tests or baseline Git refs and still pass. `check`'s soft budget is
not a process timeout; the harness must kill runaway tools itself.

**Every PR / regular CI:** bundle generation/schema/link checks; compile/test all
skill fixtures; gold/negative oracle checks; MCP protocol smoke via existing test
patterns; replay a canned transcript to test accounting, timeouts and reporting.
No paid provider calls or secrets on untrusted PRs.

**On demand / approved release qualification:** model runs, cross-language runs,
client UI smoke and held-out tasks. Freeze the evaluation set before comparison;
use paired task outcomes and uncertainty intervals, retain raw counts and failed
runs. Twelve tasks make a diagnostic pilot, not a defensible public leaderboard.
A future scheduled paid suite needs a separate owner decision and budget.

## 6. Remaining adoption gaps and relative value

Impact/effort below is a judgment for **agents adopting this repo now**, not a
measured ranking. S/M/L mean a focused change, several coordinated changes, or a
substantial platform effort; they are not calendar estimates.

| Item | Agent impact / effort | Existing foundation and proposed next step |
| --- | --- | --- |
| Correct status/docs and unify entry points | Very high / S | Fix contradictory agent pages and llms index first; expose one tested start→lookup→repair→test path. |
| Portable skill + bootstrap instructions | Very high / M | Package the current reference and tools; add small project guidance and client setup without duplicating policy. |
| Executable docs / error-reference site | High / M | Use `codes::REGISTRY`, `explain`, fixtures and MD exports to generate `/docs/errors/<code>` with `.md` exports. Keep `sakura://explain/<code>` as the existing diagnostic URI; map it to an HTTP page in docs/editors without silently changing the protocol. |
| Task cookbook | High / M | Adapt checked examples into recipes: parse/validate JSON, CLI+files, HTTP+retry, SQLite query, router endpoint, scripted LLM tool, budgeted agent loop. Include imports, permissions, failure branches, tests and compiler revision. Exclude unfinished APIs or label them explicitly. |
| Agent eval pilot | High / M | Existing diagnostic/example harnesses provide components; add task oracles and bounded model episodes, not a new compiler evaluator. |
| Immutable release binaries | Very high for first install / M–L | Source build exists. Owner must first approve supported OS/CPU/ABI matrix and release policy. Build/test artifacts, checksums, notices, std/runtime inclusion and same-version skill bundle. A Rust CLI binary still needs its JS host for JS execution; document that dependency. |
| Download/install script | High / M, after binaries | Make versions pin-able, verify checksums, detect platform, support noninteractive prefix/PATH instructions and clean failure. Do not require `curl | sh`; offer reviewed download/manual extraction. [uv's distribution configuration][uv-dist] is real prior art for coordinated installers and artifacts. |
| VS Code extension over existing LSP | Medium–high / M | Thin client starts `sakura lsp`, registers `.sak`, exposes diagnostic links/holes and executable selection. Test stdio, root selection, edits/UTF-16 positions, formatting and stale fixes. Follow the official [language-server extension guide][vscode-lsp]. Source extension first; marketplace publication needs authorization. |
| Homebrew | Medium / S–M after release | A maintained tap formula can point at immutable archives with SHA-256 and a smoke test; do not assume admission to core. Formula structure and tests are documented in the [cookbook][brew]. Owner controls namespace/credentials and publishing. |
| npm wrapper | Medium / M after release | Optional convenience for JS-centric agents: a `bin` launcher and explicit platform/architecture packages or a verified installer. Account for unsupported targets, offline installs and disabled install scripts. `bin`, `os`, `cpu`, `optionalDependencies` are documented [package fields][npm-package]; they do not themselves implement binary selection. No registry package/name is claimed. |
| Tree-sitter grammar | Medium / M | Useful for editor highlighting, structural navigation and external tooling; the existing `website/lib/sakura/grammar.ts` is syntax highlighting, not a tree-sitter parser. Start a corpus tied to the compiler lexer/parser cases; test holes, tagged strings, comments and newline rules. [Tree-sitter parser guide][tree-sitter]. |
| Plugin marketplace | Medium / S–M after bundle | Convenient Claude installation/version updates; can wrap the same tested skill. Keep compiler MCP setup project-specific rather than auto-starting a server in the wrong workspace. |
| Browser playground | Medium / L | First implement browser-compatible host imports, bounded worker execution/cancellation and either a browser-compiled checker/compiler or a separate sandboxed compilation service. Begin with pure examples, diagnostics and reset/share support; do not expose Node filesystem/network host bindings. WASM backend presence alone is insufficient. |
| GitHub Linguist recognition | Low immediate agent impact / blocked on adoption | Existing highlighting grammar may help, but recognition is a distinct upstream contribution. Current requirements include 2,000 indexed files per normal repeatable extension over the last year excluding forks (200 for once-per-repo filenames), distribution across users/repos, a compatible grammar, licensed representative samples and tests. Do not manufacture usage to pass the threshold. [Linguist contribution requirements][linguist]. |

Publishing compiler releases, Homebrew/npm/extension packages, a plugin marketplace,
or a website deployment requires owner authorization. This document requests design
decisions; this task publishes none of those artifacts. A GitHub Linguist PR also
needs explicit scope/authorization and evidence of real adoption.

## 7. Sources and validation boundaries

All URLs below were accessed **2026-10-18**. Vendor documentation is live and can
change independently of Sakura; archive/pin the version used by a future release.
Examples above are original proposed adapters using the documented formats, not
copied third-party skill bodies. Official project examples establish practice;
preprints/vendor evaluations are attributed evidence with limited generality.

- Portable format/authoring: [Agent Skills specification][skills-spec], [best practices][skills-best].
- Claude: [skills][claude-skills], [memory/imports][claude-memory], [marketplaces][claude-marketplaces], [plugin reference][claude-plugins], [MCP][claude-mcp].
- Codex: [skills][codex-skills], [AGENTS.md][codex-agents], [MCP][codex-mcp].
- Cursor: [rules][cursor-rules], [skills][cursor-skills], [MCP][cursor-mcp].
- Project practice: [Svelte AI][svelte-ai], [instructions][svelte-instructions], [skills][svelte-skills], [skill source][svelte-skill-source], [Markdown export][svelte-llms]; [Vercel skills][vercel-skills], [React skill][vercel-react]; [Rust contributor instructions][rust-agents]; [llms.txt proposal][llms-proposal].
- Evaluation: [Vercel report][vercel-eval], [SWE-Skills-Bench][swe-skills], [HumanEval harness][humaneval], [MultiPL-E][multiple], [CangjieBench][cangjie], [no-resource language benchmarks][no-resource].
- Distribution/editors: [uv distribution][uv-dist], [Homebrew][brew], [npm][npm-package], [VS Code LSP][vscode-lsp], [tree-sitter][tree-sitter], [Linguist][linguist].

[skills-spec]: https://agentskills.io/specification
[skills-best]: https://agentskills.io/skill-creation/best-practices
[claude-skills]: https://code.claude.com/docs/en/skills
[claude-memory]: https://code.claude.com/docs/en/memory
[claude-marketplaces]: https://code.claude.com/docs/en/plugin-marketplaces
[claude-plugins]: https://code.claude.com/docs/en/plugins-reference
[claude-mcp]: https://code.claude.com/docs/en/mcp
[codex-skills]: https://learn.chatgpt.com/docs/build-skills
[codex-agents]: https://learn.chatgpt.com/docs/agent-configuration/agents-md
[codex-mcp]: https://learn.chatgpt.com/docs/extend/mcp?surface=cli
[cursor-rules]: https://cursor.com/docs/rules
[cursor-skills]: https://cursor.com/docs/skills
[cursor-mcp]: https://cursor.com/docs/mcp
[svelte-ai]: https://svelte.dev/docs/ai/overview
[svelte-instructions]: https://svelte.dev/docs/ai/instructions
[svelte-skills]: https://svelte.dev/docs/ai/skills
[svelte-skill-source]: https://github.com/sveltejs/ai-tools/blob/main/tools/skills/svelte-code-writer/SKILL.md
[svelte-llms]: https://svelte.dev/docs/ai/overview/llms.txt
[vercel-skills]: https://github.com/vercel-labs/agent-skills
[vercel-react]: https://github.com/vercel-labs/agent-skills/blob/main/skills/react-best-practices/SKILL.md
[rust-agents]: https://github.com/rust-lang/rust/blob/main/AGENTS.md
[llms-proposal]: https://llmstxt.org/
[vercel-eval]: https://vercel.com/blog/agents-md-outperforms-skills-in-our-agent-evals
[swe-skills]: https://arxiv.org/abs/2603.15401
[humaneval]: https://github.com/openai/human-eval
[multiple]: https://arxiv.org/abs/2208.08227
[cangjie]: https://arxiv.org/abs/2603.14501
[no-resource]: https://arxiv.org/abs/2606.16827
[uv-dist]: https://github.com/astral-sh/uv/blob/main/dist-workspace.toml
[brew]: https://docs.brew.sh/Formula-Cookbook
[npm-package]: https://docs.npmjs.com/cli/v11/configuring-npm/package-json/
[vscode-lsp]: https://code.visualstudio.com/api/language-extensions/language-server-extension-guide
[tree-sitter]: https://tree-sitter.github.io/tree-sitter/creating-parsers/1-getting-started.html
[linguist]: https://github.com/github-linguist/linguist/blob/main/CONTRIBUTING.md

Validation for this research: all **34 external reference URLs returned HTTP 200**;
all **28 local links** resolved; all reference labels and fence pairs were checked;
**four JSON blocks, one TOML block and two YAML frontmatter examples** parsed with
Python JSON/TOML parsers and PyYAML. These checks supplement source inspection;
they do not establish client interoperability. No executable
Sakura sample is newly asserted by this document: proposed samples are fixture
requirements, and the existing suites are identified as evidence available to
implementers, not claimed as run here. No paid agent evaluation, client MCP
connection, installer, website build or compiler test suite was run for this
research-only change. Those remain explicit acceptance work below.

## 8. Prioritized decisions for Fable

Model suggestions are ownership roles from this task: **Opus codes, Sol handles
mechanical work, Astra owns docs/research**. Fable decides interfaces and ordering;
[@icantcodefyi](https://github.com/icantcodefyi) remains the named human maintainer
for semantics/security review and publication. No recommendation grants a model
publication rights or changes the existing RFC/ADR process.

| Priority / decision | Paths (new paths are proposed) | Suggested owner | Dependencies | Acceptance |
| --- | --- | --- | --- | --- |
| **P0 — correct the agent entry points** | `AGENTS.md`, `STABILITY.md`, `website/lib/source.ts`, `website/content/docs/agents/{llms-txt,mcp,diagnostics}.mdx`, `docs/mcp.md` | Astra audits/prose; Sol applies mechanical status updates. | Current compiler source and CLI/MCP smoke evidence; Fable confirms capability wording. | No implemented/future contradictions; exact stdio/frozen-spec behavior; checked local links and website gates for affected files. |
| **P0 — approve one portable Sakura skill and a short project bootstrap** | `skills/sakura/`, `docs/llms.txt`, `compiler/crates/sakura_cli/src/new.rs`, CLI scaffold tests | Astra authors workflow; Opus implements safe scaffolding; Sol handles adapters. | Decide default instruction files, opt-in client configuration, and behavior for already-initialized projects. | Matching compiler/bundle provenance; preserves existing files/config entries; fresh-project check/test/build; three documented client smoke runs and a no-MCP fallback. |
| **P0 — make the bundle executable and reproducible** | `tools/agent-docs/`, `compiler/crates/sakura_cli/tests/agent_docs.rs`, `compiler/tests/agent_docs/`, `compiler/std/INDEX.md`, `.github/workflows/ci.yml` | Opus exporter/harness; Sol deterministic mirrors/checks. | Skill content and fixture classification; registry/std owners review extraction interfaces. | All positive samples check without unexpected holes; negative/fixed pairs assert codes and behavior; no hidden skips; clean regeneration, parseable configs and valid relative links. |
| **P1 — serve the tested skill and code references** | `website/public/skills/`, `website/content/docs/errors/`, agent landing page; registry exporter | Sol packaging/routes; Astra repair guidance; Opus only where exporter logic needs it. | P0 bundle checks; choose immutable URL/version convention. | Raw SKILL.md starts with frontmatter; ZIP and website match hashes; references resolve offline/online; registry page coverage includes retired codes. Deployment separately authorized. |
| **P1 — establish the 12-task agent-DX baseline** | `evals/agent-dx/{tasks,harness,reports}/`, separate on-demand workflow | Opus runner/isolation; Astra experiment design/report; Sol fixtures/accounting. | Gold/negative oracles, frozen baseline, approved provider budget and model IDs. | Offline CI works without secrets; bounded 48-episode pilot reports failures/cost/activation and per-task acceptance; optional replicated TS/Python subset is separately labeled. |
| **P1 — publish a tested cookbook** | `compiler/examples/`, `website/content/docs/cookbook/`, `skills/sakura/references/recipes.md` | Astra recipes; Sol extraction; Opus only for missing tested examples. | P0 fixture pipeline and current implemented APIs. | Recipes state imports/permissions/errors/tests and compiler revision, execute offline with fakes, and differ from held-out eval tasks. |
| **P1 — design first-release installation** | proposed `.github/workflows/release.yml`, `tools/install/`, quickstart and release docs | Opus artifact/install logic; Sol matrix/package work; Astra docs. | Owner approves target/ABI support, versioning, signing/checksums and all publishing. | Fresh-host binary→version→new→check→test→MCP smoke; JS runtime prerequisite explicit; identical revision of docs/skill; no mutable published release. |
| **P2 — make existing LSP usable in editors** | proposed `editors/vscode/`, existing `sakura_lsp`, website highlighting grammar | Opus client integration; Sol packaging; Astra setup docs. | Tested installed executable and stable-enough protocol expectations. | Source-installable extension; diagnostics/navigation/formatting/hole context work; UTF-16/root handling tested; no marketplace publish without approval. |
| **P2 — add convenience channels only after the base path works** | proposed `plugins/sakura/`, `.claude-plugin/marketplace.json`, Homebrew tap, npm wrapper package | Sol wrappers/catalogs; Opus installer/platform logic. | Versioned bundle/binaries; owner-controlled namespaces and authorization. | Validate plugins, cache/version upgrades, formula smoke and wrapper platform/offline behavior; one source of language instructions across channels. |
| **P2 — tree-sitter; P3 — playground and Linguist** | proposed `editors/tree-sitter-sakura/`, playground app/host adapter; eventual upstream Linguist files | Opus grammar/browser engineering; Sol packaging/corpus; Astra docs. | Grammar corpus first; browser compiler/host/limits before playground; genuine usage threshold before Linguist proposal. | Parser fixtures match intended syntax; playground is cancellable and bounded with browser-safe effects; Linguist submission satisfies current upstream requirements. No premature support claim. |
