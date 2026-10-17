# The Sakura Lang MCP server

> **Pre-alpha.** Tool names, parameters and result shapes may change between commits until the
> first release (see [STABILITY.md](../STABILITY.md)). Design: [design.md §12.6](design.md).

`sakura mcp serve` runs the compiler as a [Model Context Protocol](https://modelcontextprotocol.io)
server on stdin/stdout, so an agent can check, query, patch and test a Sakura Lang workspace through
structured tools instead of parsing terminal output.

## Install

No release is published yet. Build the CLI from a checkout:

```sh
cargo install --locked --path compiler/crates/sakura_cli
sakura mcp serve --help
```

The server works on the workspace in its current directory, or the one named by
`--root <dir>`. It writes files only inside that directory, and only through `apply_patch`.

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

## Workflow

1. `check` with no `snapshot` reads the workspace from disk and returns a `snapshot` id.
   Diagnostic spans carry each file's `content_hash`.
2. Propose a change with `check_patch(snapshot, edits)`. Offsets are zero-based UTF-8 bytes.
   Each edit of an existing file carries that file's `content_hash`, and a mismatch is a
   `conflict`. Edits apply all together or not at all.
3. Read the new diagnostics and `interface_delta`. Pass `frozen_specs: "all"` (or a list of
   symbol names) to have signature, effect-row, contract, test and `[permissions]` changes
   reported as `V8020`.
4. `apply_patch(snapshot, patch_id)` writes the files. If a file changed on disk since the
   snapshot, nothing is written and the result is a `conflict`: run `check` again.
5. `test` to run the tests against the new snapshot.

A diagnostic fix id (`f:…`) can be passed to `apply_patch` directly. Fixes whose impact is
`contract_change` or `authority_change` are refused there; make those changes with
`check_patch` so they are reviewed.
