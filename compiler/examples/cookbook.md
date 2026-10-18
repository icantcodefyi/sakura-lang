These eight projects teach supported Sakura APIs on the JavaScript backend. Each
project is exercised offline by `cargo test -p sakura_cli --test examples`;
HTTP and LLM exchanges are synthetic committed cassettes, with no network or API keys.
Use Node 26 for execution, including its built-in SQLite implementation.

The initial API baseline is compiler revision
[5cae349516c21714d90d36e87f1cfd6debe980c6](https://github.com/icantcodefyi/sakuralang/commit/5cae349516c21714d90d36e87f1cfd6debe980c6).
CI reruns these projects against the compiler in the same checkout. Build that
checkout's CLI and run `sakura --version` and `sakura prompt` before adapting a recipe;
pre-alpha APIs can change. An installed skill's frontmatter records its compiler
version and build revision. These teaching examples are separate from the agent
evaluation tasks.

The source of each page is its example's `recipe.md`, `sakura.toml`,
`src/main.sak` and fixture files. Run `cargo run -p agent_docs` from `compiler/`
to regenerate the cookbook and skill reference. Edit those sources, not the rendered pages.

## Audit and verify

`sakura audit --json` inventories trust boundaries; it is not certification.
`sakura verify --json` requires the pinned solver and reports each obligation.
Exit 0 can include `unknown` or `unsupported`; inspect statuses and
`path_feasible`, including `W7002` for vacuity. A proof is conditional on its
recorded assumptions and target profile, not a whole-program guarantee.
