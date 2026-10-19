# Stability and compatibility

## Today: pre-alpha / design phase

Sakura Lang has a published `0.1.0-alpha.1` preview and a source-buildable compiler,
but no stable language/API contract. Release-specific platform evidence is listed
on the [install page](website/content/docs/install.mdx).
[The design](docs/design.md) is provisional; examples are illustrative unless
explicitly backed by a runnable test. Research and performance goals do not imply
verified soundness, production readiness, or measured efficiency.

## Before 1.0

The first preview is `0.1.0-alpha.1`; `0.1.0` is not yet published.
Preview behavior may change between prereleases.
[Draft RFC 0001](docs/rfcs/0001-stability-guarantees.md) proposes stronger guarantees
for owner acceptance at v0.2.0; it does not change this policy until accepted.
Once numbered non-prerelease versions exist, reserve **minor releases** for
deliberate breaking language/API changes and **patch releases** for compatible
fixes. Describe affected programs, rationale, and migration steps in release notes.
Security or correctness fixes may exceptionally break behavior in a patch; explain
why and document the smallest practical migration.

Experimental features must be opt-in and explicitly labeled outside compatibility
guarantees. RFC acceptance is not stabilization. Record implementation and
stabilization decisions separately and list the scope of any promised contract.
Internal compiler crates remain unpublished implementation details until an SDK
is deliberately designed; sharing a repository does not make their APIs stable.

The development toolchain is pinned in `compiler/rust-toolchain.toml`, and
the minimum supported Rust version (MSRV) is `workspace.package.rust-version`
in `compiler/Cargo.toml`. The required `msrv` CI job installs that toolchain,
checks all workspace targets with the lockfile, and runs workspace tests on Linux.
MSRV increases may occur in pre-1.0 minor releases and must be announced in notes.
Supported host/target and OS/ABI minimums will be stated only after native smoke
tests exist; there are no supported target guarantees yet.

The compiler caches std's analysis on disk, one file per compiler build, in
`<cache>/sakuralang/std/`: `<cache>` is `$SAKURA_CACHE_DIR`, else `$XDG_CACHE_HOME` or
`~/.cache` on Unix and `%LOCALAPPDATA%` on Windows. `SAKURA_NO_CACHE=1` turns it off. The
files are per-build artefacts with no compatibility promise: their format, name and location
may change in any release, any of them may be deleted at any time, and a missing, damaged or
foreign file only costs the analysis it would have saved.

## Contracts to define at 1.0

[RFC 0001](docs/rfcs/0001-stability-guarantees.md) drafts these guarantees for
owner acceptance at `v0.2.0`. It remains Draft and does not change this policy.

Before 1.0, enumerate guarantees for source semantics, standard-library APIs,
CLI flags and exit codes, configuration formats, structured diagnostic schemas,
and LSP extensions. Version machine protocols independently when necessary.
Diagnostic codes, structured fields, and fixes are machine interfaces; human
wording and presentation should remain improvable without implying a schema break.

For planned post-1.0 removals, provide deprecation warnings and a documented
migration for **at least two minor releases and six months**, satisfying both
conditions, before removal in a major release. An urgent security/correctness
exception must be explicitly justified, announced, and accompanied by mitigation.

## Evolution and distribution

Changes to these promises require the [RFC process](docs/rfcs/README.md).
Language editions are deferred until incompatible evolution needs coexistence;
there is no edition interoperability promise today. Compiler versions, any future
language editions, and machine protocol versions are distinct contracts.

Publish only tested immutable release artifacts, with checksums, license notices,
and documented targets. Never overwrite an existing version's binaries: issue a
corrected version. Programs do not acquire the compiler's license merely by being
compiled. Any embedded runtime code will need explicit licensing and compatibility
documentation before distribution.

New tagged releases require Developer ID signing and Apple notarization for macOS
binaries, pinned to team `P62B65A837`. The Homebrew archive and npm Darwin companion
contain the same signed binary. A bare Mach-O cannot carry a stapled ticket;
Gatekeeper may need network access to verify notarization. Ordinary workflow dry
runs skip signing and say so; Linux and Windows binaries remain unsigned. This policy does not
retroactively change existing immutable releases or establish language stability.
See [the release procedure](docs/release.md#macos-signing-and-notarization).
