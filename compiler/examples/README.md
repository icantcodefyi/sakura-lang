# Example corpus

The design §15 programs as runnable projects (`vow.toml`, `src/main.vow`, and
`expected.stdout` for `mode: run`). Header directives in `src/main.vow` follow the fixture
format in `compiler/REQUESTS.md` (F → E), plus `// args:` (arguments after `--`) and
`// stdout-regex: true` (each line of `expected.stdout` is a regex for the matching
stdout line). Programs also carry `test` blocks; CI runs `vow test` on every project
whose milestone has landed.

| Example | Design | Milestone | Mode | Blocked on |
|---|---|---|---|---|
| `hello` | §15.1 | M2 | run (regex) + test | `Clock`, `Instant` in std |
| `contracts` | §15.2 | M3 | run + test | contract checks, refined types |
| `counter` | §15.3 | M2 | run + test | user effects, handler state |
| `typed_errors` | §15.4 | M2 | run + test | `try`/`catch`, `Fail` ops, `Log` |
| `json_api` | §15.5 | M3 | test | `std/router`, `std/serve`, `SqlValue.from` in `sql"…"`, `Schema` codecs |
| `word_count` | §15.6 | M3 | run + test | `Cli.parse`, `cli_*`/`fs_*`/`untrusted_*` runtime externals |
| `llm_extract` | §15.7 | M3 | test | `@llm` desugaring, `Schema` codecs, `Untrusted.of` in the runtime |
| `agent_loop` | §15.8 | M3 | check | `std/agent`, `std/tool`, `url"…"`; scripted tool calls unspecified |
| `property` | §15.9 | M2 | test | `property` blocks, snapshots |
| `concurrency` | §15.10 | M3 | test | `Task` handlers in the runtime, `Fail` escaping a test (§17.30) |
| `secrets` | §15.11 | M3 | check | `Schema` codecs, `http_send` |
| `graph` | §15.12 | M1 | run + test | records, generics, imports |

Where a design example does not check as written (types whose home module is not the one
the design imports them from, handler order in a `with` list, a missing effect in `main`),
the program here is the checked version and its header comment says what changed.
