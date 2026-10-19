# Example corpus

The design §15 programs as runnable projects (`sakura.toml`, `src/main.sak`, and
`expected.stdout` for `mode: run`). Header directives in `src/main.sak` follow the fixture
format in `compiler/REQUESTS.md` (F → E), plus `// args:` (arguments after `--`) and
`// stdout-regex: true` (each line of `expected.stdout` is a regex for the matching
stdout line). Programs also carry `test` blocks; `cargo test -p sakura_cli --test examples` runs every
project through M3 (all pass; a `// known-bug:` line marks one blocked elsewhere).

| Example | Design | Milestone | Mode |
|---|---|---|---|
| `hello` | §15.1 | M2 | run (regex) + test |
| `contracts` | §15.2 | M3 | run + test |
| `counter` | §15.3 | M2 | run + test |
| `typed_errors` | §15.4 | M2 | run + test |
| `json_api` | §15.5 | M3 | test |
| `word_count` | §15.6 | M3 | run + test |
| `llm_extract` | §15.7 | M3 | test (cassette) |
| `agent_loop` | §15.8 | M3 | test (cassette) |
| `property` | §15.9 | M2 | test |
| `concurrency` | §15.10 | M3 | test |
| `secrets` | §15.11 | M3 | run + test (cassette) |
| `graph` | §15.12 | M1 | run + test |

Where a design example does not check as written (types whose home module is not the one
the design imports them from, handler order in a `with` list, a missing effect in `main`),
the program here is the checked version and its header comment says what changed.

Cassettes (`tests/fixtures/*.jsonl`, replayed by `Http.recording` and `Llm.replay` with the
network denied) are synthetic: hand-authored responses in the real response shapes, never
recorded from a provider. Each directory's `tests/fixtures/README.md` says how they were made.

## Cookbook (design §17.145)

The ten additional projects below are discovered by the same harness, all at M3
without skips. Their `recipe.md` prose and project files generate the website
cookbook and `skills/sakura/references/recipes.md` through `agent_docs`.

| Recipe | Harness mode |
|---|---|
| `json_refined` | run + test |
| `cli_files` | run + test |
| `http_retry` | test (cassette, virtual clock) |
| `sqlite_query` | test (in-memory SQLite) |
| `router_endpoint` | test (direct router calls) |
| `scripted_tool` | test (script and cassette) |
| `budgeted_agent` | test (finite budget and cassette) |
| `indexed_update` | run + test |
| `url_build` | run + test |
| `form_upload` | test (script and router) |

See [cookbook.md](cookbook.md) for the compiler baseline and regeneration command.
