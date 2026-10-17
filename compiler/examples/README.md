# Example corpus

The design §15 programs as runnable projects (`sakura.toml`, `src/main.sakura`, and
`expected.stdout` for `mode: run`). Header directives in `src/main.sakura` follow the fixture
format in `compiler/REQUESTS.md` (F → E), plus `// args:` (arguments after `--`) and
`// stdout-regex: true` (each line of `expected.stdout` is a regex for the matching
stdout line). Programs also carry `test` blocks; CI runs `sakura test` on every project
whose milestone has landed.

| Example | Design | Milestone | Mode | Blocked on |
|---|---|---|---|---|
| `hello` | §15.1 | M2 | run (regex) + test | `Clock`, `Instant` in std |
| `contracts` | §15.2 | M3 | run + test | contract checks, refined types |
| `counter` | §15.3 | M2 | run + test | user effects, handler state |
| `typed_errors` | §15.4 | M2 | run + test | `try`/`catch`, `Fail` ops, `Log` |
| `json_api` | §15.5 | M3 | test | `SqlValue.from` in `sql"…"` (C), `Sql.memory` (D) |
| `word_count` | §15.6 | M3 | run + test | nothing (runs and passes) |
| `llm_extract` | §15.7 | M3 | test | `@llm` bodies (B or D) |
| `agent_loop` | §15.8 | M3 | check | `Tool.of` intrinsic (B/C) |
| `property` | §15.9 | M2 | test | runner: snapshot path, generating inputs from `requires` (E) |
| `concurrency` | §15.10 | M3 | test | `timeout` under virtual time raises `Timeout` (D) |
| `secrets` | §15.11 | M3 | check | nothing (checks clean) |
| `graph` | §15.12 | M1 | run + test | records, generics, imports |

Where a design example does not check as written (types whose home module is not the one
the design imports them from, handler order in a `with` list, a missing effect in `main`),
the program here is the checked version and its header comment says what changed.
