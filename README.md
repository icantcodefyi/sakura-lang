# Sakura Lang

**The signature is the contract.**

Pre-alpha, source-buildable Rust compiler with a JavaScript backend. M3 is in progress; no release or stable language/API contract exists.

![Status: pre-alpha / design phase](https://img.shields.io/badge/status-pre--alpha%20%2F%20design%20phase-orange)

Sakura Lang is an **agent-first programming language** built around explicit,
reviewable promises. Humans review signatures; the compiler checks types and effects,
and checks contracts at runtime.

The design brings together:

- **Typed effects and errors:** a function states what it can do and how it can fail.
- **Efficient automatic memory management:** Perceus-style reference counting and
  reuse aim for Rust-like memory efficiency without a user-facing borrow checker.
- **Contracts and refinements:** express useful invariants and distinguish static
  proof, runtime checks, and assumptions.
- **Capabilities:** make access to files, networks, and other authority explicit.
- **Tools built for agents:** precise structured diagnostics, discoverable signatures,
  and compiler feedback for small, independently validated changes.
- **Batteries included:** one coherent compiler, formatter, test runner, package
  workflow, and standard-library direction rather than assembly from unrelated tools.

The JS backend implements typed effects/errors, runtime contracts, std handlers,
permissions, tests, LSP and MCP. WASM/static verification (M4), native Perceus reuse
(M5), and package management remain planned. See the [roadmap](website/content/docs/roadmap.mdx)
and [completion plan](docs/plans/m3-completion-and-m4.md); the [design](docs/design.md) remains provisional.

Build with the pinned Rust toolchain and run with Node 26:

```sh
cd compiler
cargo build -p sakura_cli
target/debug/sakura run examples/contracts
cargo test -p sakura_cli --test examples
```

All twelve examples pass their configured modes: six run plus test, four test,
and two check-only (`agent_loop`, `secrets`). This does not yet meet the M3 gate
requiring execution or offline replay for every example.

Recent additions include `Tool.of` for `@tool` functions, `sakura new api`,
`SAKURA_PERMISSIONS` enforcement by host handlers, per-module rechecking when public
interfaces hold, `Tool<e>` / `Agent<T, e>` / `Router<e>` row parameters, opaque access
diagnostics (`V1006`), prelude `Trust` and sink-only `@trust` (`V6001`), automatic
`SqlValue.from` conversions in `sql"…"`, escaping typed failures reported as failed
tests, and one `Llm.anthropic` streaming callback per SSE text delta. Provider behavior
is covered by local mocks; this is not a claim of live-provider verification.

## A small promise

This pure function checks with the current compiler:

```sakura
fn add(a: Int, b: Int) -> Int {
  a + b
}
```

Signatures can also declare errors/effects, contracts, and capability requirements. The exact syntax and guarantees belong
in the evolving design, not in assumptions made from this small example.

## Explore the project

Start with [the language design](docs/design.md), [the research](research/), and
[the website and documentation app](website/). Questions about prior art and
unresolved tradeoffs are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) to help,
[the RFC process](docs/rfcs/README.md) to propose language changes, and
[STABILITY.md](STABILITY.md) for compatibility expectations.

| Path | Purpose |
| --- | --- |
| `research/` | Evidence, prior art, and engineering recommendations |
| `docs/` | Evolving language design, RFCs, and architecture decisions |
| `website/` | Bun / Next.js / Fumadocs website and documentation |
| `.github/` | CI, dependency updates, issue forms, and review ownership |
| `compiler/` | Rust compiler workspace, CLI, JS runtime, std and examples |

Run the website with Bun **1.4.3**:

```sh
cd website
bun install --frozen-lockfile
bun run dev
```

The [contribution guide](CONTRIBUTING.md) covers checks and the compiler
layout. The initial maintainer is [@icantcodefyi](https://github.com/icantcodefyi);
see [governance](GOVERNANCE.md), [conduct](CODE_OF_CONDUCT.md), and
[security reporting](SECURITY.md).

## License

Original Sakura Lang code and documentation are dual-licensed under **MIT OR Apache-2.0**,
at your choice. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
Third-party material retains its own notices and terms. Compiling a program does
not by itself place that program under the compiler's license; any future embedded
runtime code will have its licensing documented separately.
