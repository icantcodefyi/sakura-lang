<p align="center">
  <a href="https://www.sakuralang.com">
    <img src="website/public/brand/sakura-mark.svg" alt="The Sakura Seal" width="96" height="96">
  </a>
</p>

<h1 align="center">Sakura Lang</h1>

<p align="center"><sub>AGENT-FIRST</sub><br><strong>The programming language built for agents.</strong></p>

<p align="center">
  <a href="https://github.com/icantcodefyi/sakuralang/actions/workflows/ci.yml"><img src="https://github.com/icantcodefyi/sakuralang/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <a href="https://www.npmjs.com/package/sakuralang"><img src="https://img.shields.io/npm/v/sakuralang?color=F386A1&label=npm" alt="npm version"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-F386A1" alt="License: MIT OR Apache-2.0"></a>
  <a href="https://www.sakuralang.com/docs"><img src="https://img.shields.io/badge/docs-sakuralang.com-F386A1" alt="Docs"></a>
</p>

Sakura Lang is an **agent-first programming language** built around explicit,
reviewable promises. A function's signature states what it can do (its effects),
how it can fail (its errors) and what it guarantees (its contracts). The compiler
checks types and effects, checks contracts at runtime and can prove them with an
SMT solver. Humans review signatures; agents get precise, structured feedback for
small changes. The compiler is written in Rust and targets JavaScript and
WebAssembly.

> [!NOTE]
> `0.1.0` is a published **preview** with no stable language or API
> contract yet. See [Status and roadmap](#status-and-roadmap) and [STABILITY.md](STABILITY.md).

## Install

`check` and `build` need nothing else; `run` and `test` need Node 26.

**npm**

```sh
npm install --global sakuralang@0.1.0
```

**Homebrew** (installs Node with it)

```sh
brew tap icantcodefyi/sakuralang && brew install sakuralang
```

**install.sh** (checks `SHA256SUMS`; verifies attestations when `gh` is installed)

```sh
curl -fsSLO https://github.com/icantcodefyi/sakuralang/releases/download/v0.1.0/install.sh
sh install.sh --version 0.1.0
```

| Platform | Target | Release |
| --- | --- | --- |
| macOS, Apple silicon | `aarch64-apple-darwin` | in v0.1.0 |
| Linux x64 (glibc 2.35+) | `x86_64-unknown-linux-gnu` | in v0.1.0 |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | in v0.1.0 |
| Windows x64 | `x86_64-pc-windows-msvc` | in v0.1.0 (npm or `install.ps1`) |
| macOS, Intel | `x86_64-apple-darwin` | in v0.1.0 |

No platform is called **Supported** until a release's published-install smoke
passes on a hosted runner of that OS and CPU. See the
[claim levels](website/content/docs/install.mdx#platform-claims) and the
[install guide](website/content/docs/install.mdx) for details and verification.

## Quickstart

```sh
sakura new cli hello && cd hello
sakura check            # no problems
sakura run -- Sakura    # hello, Sakura
sakura test             # 1 passed, 0 failed
```

`sakura new` writes `sakura.toml` (with an empty `[permissions]` table), a lockfile,
`src/main.sak` with one test, and `AGENTS.md`/`CLAUDE.md` for coding agents. Add
`--setup claude` (or `codex`, `cursor`, `all`) to install the `sakura` skill and MCP
entry for your agent. Continue with the [quickstart guide](website/content/docs/quickstart.mdx);
the [CLI reference](website/content/docs/cli.mdx) lists every command.

## A taste

Effects in `! {…}`, typed errors with `fail`, and contracts with `requires` and
`ensures`. This program checks with the current compiler:

```sakura
fn withdraw(acct: Account, amount: Cents)
  -> Account
  ! {Ledger, Fail<Overdrawn | Frozen>}
  requires positive: amount > 0
  ensures result.balance == acct.balance - amount
{
  if Ledger.frozen(acct.id) { fail Frozen }
  if amount > acct.balance { fail Overdrawn { balance: acct.balance } }
  let next = Account { id: acct.id, balance: Cents.from(acct.balance - amount) }
  Ledger.record(next)
  next
}

type Cents = Int where it >= 0
type Account = { id: String, balance: Cents }
error Overdrawn(balance: Cents)
error Frozen
effect Ledger {
  fn frozen(id: String) -> Bool
  fn record(acct: Account) -> Unit
}
```

A caller must handle `Ledger` and both errors, or declare them in its own
signature. The compiler rejects anything else.

## Why Sakura Lang

- **Built for agents.** `sakura setup` installs a skill and MCP entry; `sakura mcp serve`
  exposes `check`, `apply_patch`, `hole_context`, `test` and more; `sakura lsp` serves
  editors. `--json` emits versioned diagnostics whose fixes state their impact, and
  typed holes (`?receipt`) report the expected type, effects, locals and candidates.
- **Authority is explicit.** Files, network, clock and randomness are effects. `main`
  gets only what `[permissions]` in `sakura.toml` grants; a denied permission stops
  the program at startup.
- **Contracts you can prove.** Every contract is checked at runtime in every build.
  `sakura verify` asks Z3, cross-checked by cvc5 when it is installed, to prove each
  one; only proved checks drop out of the next build, and failures come back as
  counterexamples.
- **Deterministic by construction.** Every source of nondeterminism is an effect, so
  tests run with a fixed clock, seeded randomness and the network denied. `Map`/`Set`
  iteration order is the same on every backend.
- **Fast feedback.** `sakura check` reuses a per-build disk cache of the standard
  library's analysis, so only your own modules are analysed. `bench/bench compile`
  measures it on your machine; the blessed instruction-count baseline is on the
  [benchmarks page](https://www.sakuralang.com/docs/benchmarks).

## Status and roadmap

| Release | What it means | State |
| --- | --- | --- |
| `v0.1.0-alpha.1` | The language, std, LSP, MCP, the skill, WASM and the verify preview, on npm, Homebrew and install.sh | **shipped** 2026-10-18 |
| `v0.1.0` | The first non-alpha preview: WASM complete; the web platform's bytes, encoding, URL, headers, forms and crypto in std; a blessed instruction-count baseline; adds Linux ARM64, Windows x64 and Intel Mac | **shipped** 2026-10-20 |
| `v1.0.0-beta.1` | A beta of 1.0, not production: it may have bugs and may change before 1.0.0, every breaking change listed in the CHANGELOG with a migration; one `--json` summary kind per record shape, binary HTTP bodies, the web APIs | **shipped** 2026-10-21 (npm `beta`, `sakuralang-beta`) |
| `v1.0.0-beta.2` | The second beta, not production: `std/worker` (experimental), JSON Schema bounds for refined types, a set `SAKURA_PERMISSIONS` denies every key it leaves out; migrations in the CHANGELOG | **shipped** 2026-10-21 (npm `beta`, `sakuralang-beta`) |
| `v1.0.0-beta.3` | The third beta, not production: library entries a Node program calls (`callEntry` on WASM, `__entries__.js` on JS), WASM runtime ABI 1.16.0, `sakura verify` without exponential VC growth; migrations in the CHANGELOG | **shipped** 2026-10-21 (npm `beta`, `sakuralang-beta`) |
| `v1.0.0-beta.4` | The fourth beta, not production: `--target native` (experimental) in the macOS arm64, Linux x64 and Linux arm64 release binaries, `W8001` copy-on-write warnings, Perceus reuse and borrowed read-only calls, WASM runtime ABI 1.18.0; migrations in the CHANGELOG | **shipped** 2026-10-22 (npm `beta`, `sakuralang-beta`) |
| `v1.0.0-beta.5` | The fifth beta, not production: `--target native` (experimental) in the Intel macOS release binary too, a `match` on a tuple literal allocates nothing, runtime release fixes on out-of-memory paths, WASM runtime ABI unchanged at 1.18.0; migrations in the CHANGELOG | **shipped** 2026-10-22 (npm `beta`, `sakuralang-beta`) |
| `v1.0.0` | Production: the fuzz campaign's ledger (≥ 24 CPU-hours per target, zero soundness findings; design §17.306) and no P1 defect, then 1.0; packages follow in 1.x | planned |

The JavaScript and WebAssembly backends ship. The Cranelift native backend is
**experimental**: the macOS arm64, Linux x64 and Linux arm64 release binaries carry it from
`v1.0.0-beta.4` and the Intel macOS one from `v1.0.0-beta.5`, and under the current plan it gates no release. Packages arrive in 1.x; until then `[dependencies]` is refused. Releases are not
dated; see the [roadmap](website/content/docs/roadmap.mdx) and the
[road to 1.0](docs/plans/road-to-1.0.md). The [design](docs/design.md) is provisional.

## Build from source

Use the pinned Rust toolchain, and Node 26 to run programs:

```sh
cd compiler
cargo build -p sakura_cli
target/debug/sakura run examples/contracts
cargo test -p sakura_cli --test examples
```

The docs track `main`: [Unreleased](CHANGELOG.md#unreleased) changes need a source
build until the next release. Sources use `.sak`; rename older `.sakura` files
(there is no fallback). <!-- rename:keep -->

| Path | Purpose |
| --- | --- |
| `compiler/` | Rust compiler workspace, CLI, JS runtime, std and examples |
| `website/` | Bun / Next.js / Fumadocs website and documentation |
| `docs/` | Evolving language design, RFCs, plans and architecture decisions |
| `research/` | Evidence, prior art and engineering recommendations |
| `.github/` | CI, dependency updates, issue forms and review ownership |

## Links

- [Documentation](https://www.sakuralang.com/docs) and [what changed on main](https://www.sakuralang.com/docs/changes)
- [Contributing](CONTRIBUTING.md), [governance](GOVERNANCE.md) and the [Code of Conduct](CODE_OF_CONDUCT.md)
- [Stability policy](STABILITY.md) and the [RFC process](docs/rfcs/README.md)
- [Security policy](SECURITY.md) and [changelog](CHANGELOG.md)

The maintainer is [@icantcodefyi](https://github.com/icantcodefyi).

## License

Original Sakura Lang code and documentation are dual-licensed under **MIT OR Apache-2.0**,
at your choice. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
Third-party material retains its own notices and terms. Compiling a program does
not by itself place that program under the compiler's license; any future embedded
runtime code will have its licensing documented separately.

AI assistance: Sakura Lang is designed and built with AI agents (Claude and Codex)
under the maintainer's review, who owns the result. Contributions disclose AI
involvement; see [AI-assisted contributions](CONTRIBUTING.md#ai-assisted-contributions).
