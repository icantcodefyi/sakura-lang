# Vow

**The signature is the contract.**

![Status: pre-alpha / design phase](https://img.shields.io/badge/status-pre--alpha%20%2F%20design%20phase-orange)

Vow is an **agent-first programming language** being designed around explicit,
reviewable promises. Humans review signatures; the compiler is intended to check
that implementations keep their vows.

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

These are **design goals**, not shipped capabilities or benchmark claims. There is
no installable Vow compiler yet. Syntax, verification boundaries, runtime details,
and the library surface are still being worked out in [the design](docs/design.md).

## A small vow

Illustrative proposed syntax; this example cannot be compiled yet:

```vow
fn add(a: Int, b: Int) -> Int {
  a + b
}
```

The direction is to extend signatures with explicit error/effect information,
contracts, and capability requirements. The exact syntax and guarantees belong
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
| `compiler/` (planned) | Rust compiler library and CLI in a root Cargo workspace |

Run the website with Bun **1.4.3**:

```sh
cd website
bun install --frozen-lockfile
bun run dev
```

The [contribution guide](CONTRIBUTING.md) covers checks and the planned compiler
layout. The initial maintainer is [@icantcodefyi](https://github.com/icantcodefyi);
see [governance](GOVERNANCE.md), [conduct](CODE_OF_CONDUCT.md), and
[security reporting](SECURITY.md).

## License

Original Vow code and documentation are dual-licensed under **MIT OR Apache-2.0**,
at your choice. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
Third-party material retains its own notices and terms. Compiling a program does
not by itself place that program under the compiler's license; any future embedded
runtime code will have its licensing documented separately.
