# The Sakura Lang MCP server

> **Pre-alpha.** Tool names, parameters and result shapes may change between commits until the
> first release (see [STABILITY.md](../STABILITY.md)). Design: [design.md §12.6](design.md).

`sakura mcp serve` runs the compiler as a [Model Context Protocol](https://modelcontextprotocol.io)
server on stdin/stdout, so an agent can check, query, patch and test a Sakura Lang workspace through
structured tools instead of parsing terminal output.

## Install

No release is published yet. Build the CLI from a checkout:

```sh
cd compiler
cargo install --locked --path crates/sakura_cli
sakura mcp serve --help
```

Without `--root`, the server finds the nearest ancestor of its working directory
containing `sakura.toml`; it refuses to start outside a project. `--root <dir>`
selects an explicit workspace. Source edits are written only inside that directory through `apply_patch`;
`test` executes the supplied test runner. Compiler HTTP transport and the
`--tools` function-export flag are planned, not supported options.

For project-local setup, `sakura setup --client claude|codex|cursor|all` prints the
proposed configuration; add `--apply` to write it. Run it inside the project.
The commands below are manual alternatives.

## Client setup

Each client starts the server itself. If a client does not start servers in the project
directory, pass `--root` with an absolute path.

**Claude Code**: from the project directory,

```sh
claude mcp add sakura -- sakura mcp serve
```

or commit a project-scoped `.mcp.json`:

```json
{ "mcpServers": { "sakura": { "command": "sakura", "args": ["mcp", "serve"] } } }
```

**Codex**: `codex mcp add sakura -- sakura mcp serve`, or in `~/.codex/config.toml`:

```toml
[mcp_servers.sakura]
command = "sakura"
args = ["mcp", "serve"]
```

**Cursor**: `.cursor/mcp.json` in the project:

```json
{
  "mcpServers": {
    "sakura": { "command": "sakura", "args": ["mcp", "serve", "--root", "${workspaceFolder}"] }
  }
}
```

## Tools

| Tool                 | Does                                                                                              |
| -------------------- | ------------------------------------------------------------------------------------------------- |
| `compiler_info`      | Compiler version, edition, schema version, tools and features, workspace root and manifest.       |
| `check`              | Diagnostics (stable ids, fixes), holes and a summary for the workspace or a `snapshot`.           |
| `symbol_query`       | Symbols by name, id (`std::list::map`), kind, or type shape (`fn(List<A>) -> Int`).               |
| `hole_context`       | Expected type, allowed effects and candidate bindings for a `?name` hole.                         |
| `docs_query`         | Language guide, std index and diagnostic sections for a topic or symbol, within a token budget.   |
| `explain_diagnostic` | The explanation of a code (`V3001`), or of one diagnostic occurrence.                             |
| `format`             | Canonical formatting as a patch (`patch_id`), not written to disk.                                |
| `check_patch`        | Applies edits to a snapshot in memory; returns new diagnostics, interface delta and a `patch_id`. |
| `apply_patch`        | Commits a `patch_id` or fix id; writes files only if they still match the snapshot on disk.       |
| `test`               | Runs `test`/`property` blocks with deterministic handlers, a seed and a time limit.               |

Tool failures are results with `isError: true` and a JSON body `{"error": kind, "message": …}`.
Kinds include `conflict`, `unknown_snapshot`, `unknown_patch`, `spec_frozen`,
`fix_requires_review` and `internal_error`. A compiler crash inside a tool is reported as
`internal_error` and the session continues.

`compiler_info` returns `compiler`, `version`, `commit` (the source commit a release build
was stamped with, else `null`), `schema_version`, `edition`, `tools`, `features`,
`workspace_root`, `manifest` and `lock_digest`. Capabilities describe the
running build: `backends` includes `wasm` when compiled in, `verify: true` reports
CLI verification support; the separate `features.solver` object reports `name`,
`pinned`, `available`, and either `version`/`path` or a failure `detail`.
`features.test_execution` reports whether the server has a test runner. A missing solver
does not mean verification is unimplemented; `verify` is not an MCP tool.

MCP results are JSON objects containing records and summaries, not the CLI
`--json` JSON Lines stream. Read `tools/list` for the running server's schemas.

## Workflow

1. `check` with no `snapshot` reads the workspace from disk and returns a `snapshot` id.
   Diagnostic spans carry each file's `content_hash`.
2. Propose a change with `check_patch(snapshot, edits)`. Offsets are zero-based UTF-8 bytes.
   Each edit of an existing file carries that file's `content_hash`, and a mismatch is a
   `conflict`. Edits apply all together or not at all.
3. Read the new diagnostics and `interface_delta`. Pass `frozen_specs: "all"` (or a
   comma-separated string of symbol names) on `check_patch` to have signature,
   effect-row, contract, test and `[permissions]` changes reported as `V8020`.
4. `apply_patch(snapshot, patch_id, frozen_specs: "all")` writes the files using the
   original base snapshot. `frozen_specs` is optional on **each** patch call; a check
   does not attach the freeze policy to the patch. Repeat it when applying.
   If a file changed on disk since the snapshot, nothing is written and the result is a `conflict`: run `check` again.
5. `test` to run the tests against the new snapshot.

A diagnostic fix id (`f:…`) can be passed to `apply_patch` directly. Fixes whose impact is
`contract_change` or `authority_change` are refused there; make those changes with
`check_patch` so they are reviewed.

## Current-main cache and diagnostics

The server reuses std analysis in memory and from the per-build disk cache at
`<cache>/sakuralang/std`. Set `SAKURA_CACHE_DIR` to isolate concurrent jobs, or
`SAKURA_NO_CACHE=1` to disable disk caching. This is not the verifier proof cache.
Missing, damaged or foreign entries are recomputed without changing diagnostics.

File URIs use RFC 8089 on Windows as well as Unix; `paths` accepts equivalent
spellings such as an encoded drive colon. Terminal output uses OS paths. Syntax
errors suppress diagnostics by their location in a broken body, not all diagnostics
raised while checking that body; an error at a signature in another module remains
visible. Rechecks include refinement dependencies reached through contracts.
See the [current-main guide](../website/content/docs/changes.mdx) for migration
notes and the distinction from the published alpha.1 package.
