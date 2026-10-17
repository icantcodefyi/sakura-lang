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

All twelve examples pass their configured modes: seven run plus test and five test;
none is check-only. `llm_extract`, `agent_loop` and `secrets` replay committed synthetic
cassettes with the test network denied. These are not live-provider recordings, and
passing the example corpus does not close every M3 gate.

Recent additions include [trust audits](website/content/docs/agents/audit.mdx),
[offline API search](website/content/docs/agents/doc.mdx),
[build manifests](website/content/docs/agents/build-manifest.mdx), and
[HTTP/LLM cassettes with `--record`](website/content/docs/agents/cassettes.mdx).
Scoped escape checks (`V6002`/`V6003`), foreign-capability FFI rejection (`V6001`),
refined-target `Untrusted.validate` (`V5003`), bounded literal evaluation (`V5004`,
`V5002` retired), collection-only `W5005`, and `Html.from`/nested HTML templates have landed.

`--contracts` and streaming HTTP remain planned. Audits currently omit undecided literal
conversions, and direct `Untrusted<String>` SQL interpolation is rejected; the guides record
these limitations. The M4 plan allows isolated runtime (R1–R3), LIR (L1–L2) and verifier
(V1–V2) work before the M3 gates, with CLI/backend integration still gated.

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
