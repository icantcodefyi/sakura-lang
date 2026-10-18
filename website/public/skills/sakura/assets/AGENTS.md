# Agent instructions

This project uses Sakura Lang. Run `sakura --version` and read `sakura prompt`
before writing source; the guide in that executable matches the compiler. Use
the `sakura` skill when it is installed. Look up unfamiliar APIs with
`sakura doc` or the MCP tool `symbol_query` before guessing an import.

Run `sakura check --json`, `sakura fmt --check` and `sakura test --json`. A
successful check may still contain holes (`?name`); finish them before the
final `sakura build`. Use `sakura explain CODE` for any diagnostic.

For repairs, freeze the agreed signatures, contracts, tests and permissions
against the task's baseline commit (`--frozen-specs`). Do not weaken a contract
or widen `[permissions]` or an effect row to make a check pass; report any
interface change. Audit output inventories trust and verify statuses describe
single obligations: neither a clean check nor exit 0 from `sakura verify`
certifies the whole program.

When the MCP server `sakura` is connected, confirm that
`compiler_info.workspace_root` is this project before trusting its results.
