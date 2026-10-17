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
| `json_api` | §15.5 | M3 | test | `std/http`, `std/sql`, `Schema` codecs |
| `word_count` | §15.6 | M3 | run + test | `std/cli`, `std/fs`, capabilities |
| `llm_extract` | §15.7 | M3 | test | `std/ai`, `@llm`, `Llm.scripted` |
| `agent_loop` | §15.8 | M3 | check | `Agent.run`, `@tool`; scripted tool calls unspecified |
| `property` | §15.9 | M2 | test | `property` blocks, snapshots |
| `concurrency` | §15.10 | M3 | test | `Task`, `Http.scripted` |
| `secrets` | §15.11 | M3 | check | `Secret` sinks, `std/http` |
| `graph` | §15.12 | M1 | run + test | records, generics, imports |

Where a design example does not follow the v0 grammar (call-site type arguments, named
arguments, `Retry.Policy`), the program here is the grammatical version and its header
comment says what changed.
