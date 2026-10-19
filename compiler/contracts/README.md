# Interface baselines

`cli.txt` is generated from the real clap command tree, including hidden arguments,
possible values, defaults, built-in help, and the driver's exit constants. It also
lists literal `SAKURA_*` environment reads in compiler crate sources and runtime
hosts (including build-time stamps and internal controls). It never reads their
values. Test directories and JS test files are excluded. The small scanner covers
Rust `var`/`var_os`/`env!`/`option_env!`, the cache's `var` closure, JS `env` property
access and `bytesEnv`; a new environment access idiom must extend the scanner.
It does not promise compatibility for internal rows or capture arbitrary user
`Env.get` keys, human help prose, emitted program exit statuses, or every clap
argument constraint.

From `compiler/`, regenerate deliberately, then inspect the diff:

```sh
cargo test -p sakura_cli --bin sakura cli_surface::regenerate -- --ignored --exact
cargo test -p sakura_cli --bin sakura cli_surface::current_surface_matches_baseline -- --exact
```

The drift test also runs with `cargo test --workspace --locked`. The generator
lives alongside the other documentation generators in
[`tools/agent_docs/cli_surface.rs`](../tools/agent_docs/cli_surface.rs); it is a test
module in the CLI binary so introspection needs no public Rust API or dependency.
The draft classifications are in [STABILITY.md](../../STABILITY.md). This freshness
check is not the append-only history gate assigned to Sol in design §17.232, and
does not enforce experimental help labels assigned to Opus.
