# 17 — Packages and a package manager for Sakura Lang

Research date: **2026-10-22**. Repository baseline: `377f7e9c` (`main`, the day
`v1.0.0-beta.5` is being cut). Audience: **Fable** (who decides) and the Opus lanes that
implement [RFC 0003](../rfcs/0003-packages.md) through [the plan](../plans/packages.md).
**Research, not decisions.** The decisions are in the RFC; this file is the evidence behind
them and the alternatives weighed. The existing numbered series is in
[`research/`](../../research/), ending at 15; [16](16-native-and-agent-competitiveness.md)
continues it here, and so does this file.

AI involvement: Claude Fable 5.1 researched the sources below, read the compiler's manifest,
module and interface-delta code, and wrote this document. The human contributor owns the
result.

## 1. What Sakura has today

- `sakura.toml` has three tables, `[package]`, `[permissions]` and `[verify]`
  ([`manifest.rs`](../../compiler/crates/sakura_analysis/src/manifest.rs)). `[dependencies]` in
  any form is `V8005 dependencies_not_supported` before anything runs; any other table or key is
  `V8006 manifest_invalid` (design §17.232, §17.239; RFC 0001 "Reserved packages"). Both are
  stable contracts from `1.0.0-beta.1`.
- `sakura new`/`init` write a `sakura.lock` that says only `version = 1`
  ([`new.rs`](../../compiler/crates/sakura_cli/src/new.rs)). The website documents it as
  "with no dependencies it records only its format version".
- The module graph already has packages. `PackageInput { name, deps, modules }`
  ([`lower.rs`](../../compiler/crates/sakura_hir/src/lower.rs)) resolves an import's first
  segment among the importing package's `deps` plus `std`, and reports an unknown package with
  the hint "add it to `[dependencies]` in sakura.toml". `std` is an ordinary package named `std`
  whose sources ship inside the compiler (`sakura:///std/<path>`,
  [`lib.rs`](../../compiler/crates/sakura_analysis/src/lib.rs)). What is missing is everything
  before lowering: there is one user package, loaded from `src/` under the manifest.
- Authority comes from exactly one place: the host installs root handlers for the std effects
  `main` declares, capped by `[permissions]` (design §6.2); a declared effect without a grant is
  exit 75 at startup. Thirteen std modules have a `pub fn host()` constructor (`Fs.host()`,
  `Env.host()`, …) that any module may call today, and it reads the same `SAKURA_PERMISSIONS`
  grants. `@external` bindings are the JS FFI (`V8004` on other targets). `@trust` sites and
  `@external` bindings are listed by `sakura audit` and in `build --manifest`'s `trusted` list.
- The interface-delta machinery exists: `InterfaceDelta { added, removed, changed,
  permissions }` with `effects_changed` and `contracts_changed` per change
  ([`specs.rs`](../../compiler/crates/sakura_analysis/src/specs.rs)), used by
  `check --frozen-specs --since` (`V8020`) and the MCP `check_patch` tool. It compares two
  snapshots of the user package; it does not yet classify a delta as a semver bump.
- The compatibility corpus (`tests/compat`, design §17.232 (e)) replays the previous release's
  fixtures under the current compiler. Lockfile and manifest fixtures can join it.
- Design §9 already fixes several answers: MVS within a major, `sakura.lock` with exact versions
  and content hashes committed, immutable registry packages, `sakura publish` computing the
  API/effect/error/schema diff and enforcing the bump (the Elm rule). Design §6.5: packages
  declare the effects their public functions use (computed) and the capabilities their handlers
  need; an application grants authority only by installing a handler; build scripts do not
  exist. Design §16 question 11: capability declarations computed and shown by default,
  declared ceilings mandatory for published packages. [Research 05 §3](../../research/05-platform-and-stdlib.md)
  proposed the CLI shape (`add`, `remove`, `update --plan`, `sync --locked`, `deps
  tree|why|capabilities`, `vendor`, `publish --dry-run`) and the lock contents.

The RFC therefore does not invent a module system or an authority model. It adds loading of
more than one user package, a resolver, a lock format with content, a transport, a publishing
path, and the checks that make the already-designed ceilings real.

## 2. What the others do

### 2.1 Cargo (Rust)

Two registry protocols: a git index and, since Rust 1.68, a **sparse index** fetched per crate
over plain HTTP from `sparse+https://index.crates.io/` (one small file per crate, cacheable;
HTTP/2 helps). Index lines carry `cksum` (SHA-256 of the `.crate`) and `yanked`; `Cargo.lock`
pins versions and checksums and embeds the registry URL, which is why a protocol migration had
to rewrite lock files ([Registry Index](https://doc.rust-lang.org/cargo/reference/registry-index.html),
[sparse registry call for testing](https://blog.rust-lang.org/2022/06/22/sparse-registry-testing/),
[Cargo team notes](https://hackmd.io/@rust-cargo-team/B13O52Zko)). Yank keeps the bytes but
removes the version from resolution for anyone not locked to it. Resolution is a backtracking
solver over caret ranges; multiple majors of one crate coexist in a build. `build.rs` and
proc macros run arbitrary code at build time, the ecosystem's main supply-chain surface
([cackle](https://github.com/cackle-rs/cackle) exists to ACL it, below).
[The SemVer reference](https://doc.rust-lang.org/cargo/reference/semver.html) is a long,
hand-maintained list of what counts as major, minor and "possibly breaking";
[cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks) mechanises 245 of those
rules from rustdoc JSON, with the stated 2026 goal that `cargo publish` requires compliance by
default with an override flag ([project goal](https://rust-lang.github.io/rust-project-goals/2026/cargo-semver-checks.html)).
Its known limitation is false negatives, and items re-exported from other crates it cannot see.

Transfers: the sparse index as static files; checksum in the index and the lock; yank as a flag,
never a deletion; the publish-time semver gate. Does not transfer: build scripts (Sakura has
none by construction), the backtracking solver (MVS is simpler and deterministic), multiple
majors in one build (Sakura's import first segment is the package name, so two majors would
collide; §4 below).

### 2.2 npm and pnpm

The 2025 **Shai-Hulud** worm compromised 500+ packages through stolen maintainer tokens and
spread via `preinstall`/`postinstall` scripts; a second wave hit 300+ in November 2025 and a
2026 variant published through a project's own CI with valid provenance
([trigger.dev post-mortem](https://trigger.dev/blog/shai-hulud-postmortem),
[PostHog post-mortem](https://posthog.com/blog/nov-24-shai-hulud-attack-post-mortem),
[Tensorlake case](https://www.matthewswong.com/en/blog/tensorlake-npm-shai-hulud-ai-sdk-worm/)).
The responses that stuck: install scripts off by default (pnpm 10, Bun; pnpm 11's `allowBuilds`
allowlist), **release-age cooldowns** (`minimumReleaseAge`; pnpm 11 defaults to one day, npm's
`min-release-age` is off by default), `trustPolicy: no-downgrade` (a version that loses
provenance a previous version had is refused), trusted publishing over GitHub Actions OIDC
instead of long-lived tokens, and frozen lockfiles in CI
([hardening guide](https://www.nodejs-security.com/blog/hardening-your-npm-pnpm-config-for-shai-hulud),
[Snyk](https://snyk.io/articles/npm-security-best-practices-shai-hulud-attack/)).
[npm provenance](https://github.blog/security/supply-chain-security/introducing-npm-package-provenance/)
is an in-toto/SLSA statement signed through Sigstore's Fulcio with the runner's OIDC identity
and logged in Rekor; `npm audit signatures` verifies it on the consumer side.

Transfers: no install scripts (already the design), cooldown as a default of `add`/`update`,
no-downgrade of trust signals, OIDC-identified publishing, lockfile-only installs for the
compiler. Does not transfer: reusing npm itself as Sakura's registry (§5.3).

### 2.3 Go modules

**Minimal version selection** ([Cox, 2018](https://research.swtch.com/vgo-mvs)): every
`require` is a floor; the build list is the per-module maximum of all floors reachable from the
root, computed by a graph walk with no SAT solving, so the same manifests always give the same
build and the chosen versions are the oldest that satisfy everyone, which keeps builds close to
what each author tested. Upgrades and downgrades are defined as operations on the build list.
Majors are different module paths (semantic import versioning). `go.sum` holds two SHA-256
hashes per module version, the zip and the `go.mod` alone, the latter so MVS can read
requirements without downloading bodies ([reference](https://go.dev/ref/mod)). The
**checksum database** `sum.golang.org` is a Merkle transparency log (Trillian) of every
public module's hashes; the `go` command verifies a first-seen hash against it with an inclusion
proof, so a proxy cannot serve one person different bytes
([design](https://go.googlesource.com/proposal/+/master/design/25530-sumdb.md),
[launch post](https://go.dev/blog/module-mirror-launch)). Criticism: `go.sum` merge conflicts
in fast-moving repositories.

Transfers: MVS (already the design), the separate manifest hash so resolution reads only
manifests, the transparency-log idea in a static form (§2.11). Does not transfer: majors in
the import path (an edition-like scar on every import), Trillian as infrastructure.

### 2.4 Deno and JSR

[JSR](https://deno.com/blog/how-we-built-jsr) is ESM-only, TypeScript-first, runs no install
scripts, publishes immutable versions, attaches Sigstore provenance when publishing from GitHub
Actions with OIDC, and scores packages on documentation, "no slow types" and provenance
([Why JSR](https://jsr.io/docs/why)). It transpiles for npm consumers on the fly. Deno's
permission model is process-wide, scoped by value (`--allow-net=api.example.com:443`), with
deny overriding allow; **there is no per-package grant**, a request open since 2018
([#171](https://github.com/denoland/deno/issues/171),
[#6657](https://github.com/denoland/deno/issues/6657)), and the maintainers name the nested
dependency case as the design hurdle: if A may use the network and calls B, who is answerable?
([security docs](https://docs.deno.com/runtime/fundamentals/security/)).

Transfers: token-less CI publishing with provenance, immutability, a score that rewards
documented and well-typed packages (Sakura's `sakura doc` index and effect ceilings can feed
one later). The per-package permission gap is exactly what Sakura's effect system closes:
authority is a value a handler closes over, not a process flag, so "who is answerable" has a
typed answer (§3).

### 2.5 Gleam and Hex

Gleam uses Hex. `gleam add wibble@2` writes a range `>= 2.x and < 3.0.0`; `manifest.toml`
locks versions with `outer_checksum` per package and `source = "hex"`
([command reference](https://gleam.run/documentation/command-line-reference/),
[an example manifest](https://github.com/gleam-lang/erlang/blob/main/manifest.toml)). Hex's
registry is a set of **signed protobuf resources**, `names`, `versions`, one `package` file per
package, verified with the repository's public key, so mirrors need no trust
([specification](https://github.com/hexpm/specifications/blob/main/endpoints.md)); a
self-hosted Hex is `mix hex.registry build`, producing `public_key`, `names`, `versions` and a
`tarballs/` directory, served by any static host ([self-hosting](https://hex.pm/docs/self-hosting)).
A 2026 CVE in Gleam ([CVE-2026-59247](https://cna.erlef.org/cves/CVE-2026-59247.html)) came from
recording the checksum from the *unsigned* API response instead of the signed registry
resource; builds reusing a known-good manifest were unaffected.

Transfers: the signed static registry is the closest existing model to what §5 proposes; the
CVE is the exact lesson that every byte the lock records must come from the verified
resource, not a convenience endpoint.

### 2.6 Zig

No registry. `build.zig.zon` lists dependencies as URL plus content hash; `zig fetch --save`
computes the hash; every manifest in the tree pins its direct dependencies by hash, so there is
no resolution step and two hashes of the same upstream both exist; a dependency that is never
referenced is fetched lazily ([Unboxed: Zig](https://nesbitt.io/2026/07/09/unboxed-zig.html)).
Criticism: without a registry, no name authority, no yank, no one authoritative index, and
ecosystem tooling (SBOM, vulnerability scanners) does not know the format
([M×N problem](https://nesbitt.io/2026/01/29/zig-and-the-mxn-supply-chain-problem.html)).

Transfers: the canonical hash of the file tree, offline builds once fetched. Does not
transfer: hashes instead of versions (no compatible-upgrade story, no semver gate), URLs as
identity (an import's first segment must be a name).

### 2.7 Swift Package Manager

[SE-0292](https://github.com/swiftlang/swift-package-manager/blob/main/Documentation/PackageRegistry/Registry.md)
is a registry *service specification* (scoped identifiers `scope.name`, ABNF, URI templates)
that any server may implement; source-control dependencies pinned in `Package.resolved`
coexist with registry ones, and the registry maps URLs to identifiers for migration. Publishing
endpoints came in a later proposal. Transfers: a written, versioned protocol that a static host
can satisfy, and registry entries beside git entries in one lock.

### 2.8 Elm

Versions are **computed from the API**: every package starts at 1.0.0; `elm diff` compares the
current API with the last published one; `elm bump` picks PATCH (same API), MINOR (additions
only) or MAJOR (changes or removals); the server refuses an upload whose bump is too small
([original README](https://github.com/avh4/elm-dlang/tree/master/elm-package),
[Elm Radio](https://elm-radio.com/episode/publishing-packages/)). Nothing is unpublished. The
honest criticism ([elm-lang.org #868](https://github.com/elm/elm-lang.org/issues/868)): a
PATCH can change behaviour behind an identical signature, so the guarantee is "a minor cannot
break your build", not "a minor cannot break your program".

Transfers: the whole rule, extended as design §9 says to effect rows, error unions, contracts
and schemas, which Sakura's signatures carry and Elm's do not; the same honesty about
behaviour.

### 2.9 Unison

Definitions are content-addressed by the hash of their syntax tree; names are metadata;
dependencies are hashes, so there is no diamond problem and no version conflict, at the price
that a bug fix is not inherited until a definition is re-pointed
([The big idea](https://www.unison-lang.org/docs/the-big-idea/),
[a 2026 hands-on](https://softwaremill.com/trying-out-unison-part-1-code-as-hashes/)).
Unison Share is the codebase host. Transfers: content hashes as the identity of *bytes* (the
lock), a perfect compilation cache keyed by content (a later cache lane). Does not transfer:
hashes as the identity of *APIs*; Sakura's users and agents name things.

### 2.10 Roc

A Roc application header names its platform and packages as **URLs whose file name is a
base58 BLAKE3 hash of the bundle** (`https://github.com/roc-lang/http/releases/download/1.0.0/6ZUw….tar.zst`);
bundles live on GitHub Releases, the cache is `$XDG_CACHE_HOME/roc/packages/<hash>/`, and an
open issue asks for a documented offline directory
([#10265](https://github.com/roc-lang/roc/issues/10265),
[#12105](https://github.com/roc-lang/roc/issues/12105),
[platforms](https://www.roc-lang.org/platforms)). Transfers: GitHub Releases as a free,
unlimited, bandwidth-free tarball store; safe extraction with path and size limits as a
spelled-out rule. Does not transfer: hashes in source files (Sakura's imports stay names; the
hash lives in the lock).

### 2.11 Transparency without a server

[C2SP tlog-tiles](https://c2sp.org/tlog-tiles@main) specifies a Merkle log served as a finite
set of **static files** (a signed `checkpoint` plus `tile/<L>/<N>` hash tiles and entry
bundles), the encoding behind the static CT API, with implementations in Go and Rust. A
checksum log for a small registry can therefore be a directory on GitHub Pages, written by the
publishing workflow, with no Trillian. Until that lane exists, an append-only git history of the
index (enforced by a CI check that lines are only added or have `yanked` flipped) is a weaker
but real log: every historical index state is fetchable, and a rewritten history is visible to
every clone.

### 2.12 Per-package authority in other ecosystems

[LavaMoat](https://www.npmjs.com/package/lavamoat) runs each npm package in a SES Compartment
with a `policy.json` listing the globals, builtins and packages it may reach; policy is
generated from an observed run and committed; production use at MetaMask. Its author calls it
"not quite object capabilities" and it needs a Compartment-capable engine.
[cackle](https://github.com/cackle-rs/cackle) ACLs Rust crates by API class (`fs`, `net`,
`process`) through a `cackle.toml` and a wrapped `rustc`; its own documentation says
circumvention is possible and `unsafe` is a master permission
([Lattimore, 2023](https://davidlattimore.github.io/posts/2023/10/09/making-supply-chain-attacks-harder.html)).
Both are after-the-fact overlays on languages whose code has ambient authority. The Rust
internals thread ["Effect System + Crate Permissions?"](https://internals.rust-lang.org/t/effect-system-crate-permissions/23529)
asks for what Sakura has: permissions that the type system carries.

[OpenSSF's Principles for Package Repository Security](https://repos.openssf.org/principles-for-package-repository-security.html)
(with CISA, 2024) grade repositories on authentication, authorization, general capabilities
and CLI tooling, levels 0–3; level 1 already asks the CLI to install from hash- and
version-pinned lockfiles, level 3 asks for build provenance. §5's design reaches level 3 on the
capabilities and CLI tracks without accounts of its own, because authentication is GitHub's.

### 2.13 Hallucinated names

The USENIX Security 2025 study of 2.23 million generated samples found 19.7 % contained at
least one hallucinated package name, 205,474 unique fabrications, 43 % of them repeated on every
re-run of the same prompt, and 8.7 % of hallucinated Python names existing on npm
([Trend Micro summary](https://www.trendmicro.com/vinfo/us/security/news/cybercrime-and-digital-threats/slopsquatting-when-ai-agents-hallucinate-malicious-packages),
[CSA note](https://labs.cloudsecurityalliance.org/research/csa-research-note-slopsquatting-ai-supply-chain-20260419-csa/)).
Agents with live validation hallucinate about half as often but still do. For an agent-first
language the consequences are concrete: an unknown name must fail fast with a machine-readable
code and no fallback to another registry; the registry must not auto-register on first `add`;
new names close to existing ones must be refused at publish; and `sakura add` must show what
the package *is* (its ceiling and its API summary) before the lock changes.

## 3. The agent-first concerns, and Sakura's answer

1. **Deterministic resolution.** MVS over manifests is a pure function; the lock makes it
   explicit; the compiler commands read the lock and the cache and never resolve or fetch.
   An agent's `check`/`build`/`test` therefore never changes meaning between two runs because
   a registry changed. Only `add`, `remove`, `update`, `sync`, `vendor` and `publish` touch the
   network, and each prints what it changed as JSON.
2. **Machine-readable errors.** Every failure is a `V8xxx` diagnostic in the `--json` protocol
   with a span in `sakura.toml` or `sakura.lock` (the existing `V8005`/`V8006` pattern): a
   major conflict names the two requirement paths; a stale lock names the manifest line that
   changed; a checksum mismatch names the package and both hashes; an unknown name says so and
   suggests nothing.
3. **"Package X needs effects Y."** Sakura already computes, for every function, the effects it
   may perform, and authority is a capability value a handler closes over (design §6.1). So a
   package's requirements are a *fact of its types*: the union of the std effects in its public
   functions' rows. The RFC makes that union the package's **effect ceiling**, declared in its
   manifest and checked by the effect checker (a public function whose row exceeds the ceiling
   is an error in the package itself), recorded in the consumer's manifest by `sakura add` and
   in the lock, and enforced on upgrade: a version that needs more than the consumer accepted
   does not compile until the consumer edits one line. The second half is the **authority
   gate**: a dependency may not call a `host()` constructor or declare `@external` unless the
   consumer marks it `trusted`, because those are the only ways to acquire authority rather
   than require it. With both, Deno's unanswered question has a typed answer: if A may use the
   network and calls B, B's row says `Http`, A's ceiling includes it, and the handler A's caller
   installed decides which hosts. No other mainstream ecosystem's packages carry this; it is
   the differentiator the RFC builds on.
4. **Reviewable upgrades.** `sakura update --plan` prints the version, API (added, removed,
   changed with effects/contracts flags, from `InterfaceDelta`) and ceiling differences before
   writing anything, as a JSON record an agent can quote in a pull request.

## 4. Why one version per package per build

Cargo allows two majors of one crate in a build; Go gives majors different import paths;
Unison has no versions. In Sakura the import's first segment *is* the package name
(`import acme/billing`), the prelude and `Schema` names are by module path, and opaque types
are identified by their declaring module. Two majors of `acme` would need either a renamed
import path for one of them (Go's scar on every import) or an invisible type-identity split
(two `acme/Invoice` types that do not unify, with the error messages that follow). MVS with
"one major per package, a conflict is an error naming both requirement paths" keeps resolution
explainable and the type universe simple. The cost, that a graph with a major split cannot
build until someone upgrades, is what Elm and Go accepted, and it is the cost an agent can act
on (the error names the two paths). The RFC takes it.

## 5. Registry options under the no-paid-infrastructure constraint

### 5.1 A hosted registry service

crates.io, npm, Hex, JSR: a database, an API, accounts, moderation, uptime. Rejected: it is
exactly the paid infrastructure and the operational commitment the constraint forbids, and a
one-maintainer project cannot promise its availability.

### 5.2 Git- or URL-based only (Zig, Roc)

No infrastructure at all. Rejected as the *only* mechanism: no name authority (so no typosquat
rule and no unique import segment), no yank, no cooldown, no publish-time semver gate, and
availability at the mercy of each publisher's repository. Kept as a *source kind*: a dependency
may be `{ git = "...", rev = "<40-hex sha>" }` (exact commit, never a branch) or
`{ path = "..." }`, both locked by content hash, for private code and pre-publication.

### 5.3 npm as transport

Publish Sakura tarballs under an npm scope and resolve through npm's API. It is free and has
provenance and 2FA. Rejected: Sakura's namespace would be governed by npm's account and
unpublish policies; the metadata is JS-shaped and would confuse both agents and the 8.7 %
cross-registry hallucination case above; `install` semantics (scripts, `node_modules`) would
be a constant temptation to lean on; and the compiler would depend on npm's rate limits and
API stability for every `sync`. A mirror of the index to npm is possible later; not a design
input now.

### 5.4 A static, signed index on GitHub, tarballs on GitHub Releases (chosen)

The Hex self-hosting and Cargo sparse-index model with GitHub as the only operator:

- **Index** = a git repository (`sakura-registry`, under the owner's GitHub account) with one
  JSON Lines file per package, one line per version: name, version, SHA-256 of the canonical
  tarball, dependency floors, the effect ceiling and `trusted` flag, compiler floor, source
  repository and commit, the publishing workflow run, `yanked`. Served as static files by
  GitHub Pages (free; 1 GB site, 100 GB/month soft bandwidth; a CNAME to a subdomain of the
  existing domain is free). Clients fetch one file per package (sparse) and may `git clone` for
  history.
- **Tarballs** = assets on GitHub Releases *of the registry repository* (one release per
  package version), not of the publisher's repository, so the publisher cannot replace them;
  each asset is under 2 GiB, releases have no total size or bandwidth cap
  ([about releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases)).
  The client verifies the SHA-256 against the index line before extraction.
- **Publishing** = a pull request to the index repository opened by `sakura publish` (through
  the user's own `gh` authentication; no Sakura accounts, no tokens stored). The registry's
  GitHub Actions workflow checks ownership (first publisher of a name is recorded as its
  owner; later versions must come from an owner), clones the named commit, builds the canonical
  tarball with the released `sakura`, checks the checksum, `check`s the package, computes the
  API diff against the previous version and refuses an undersized bump (Elm's rule,
  server-side), uploads the asset, attests the build
  ([artifact attestations](https://github.blog/security/supply-chain-security/slsa-3-compliance-with-github-actions/)),
  and merges. An append-only check refuses any PR that edits or removes an existing line other
  than flipping `yanked`.
- **Why it satisfies the constraint**: GitHub Pages, Releases and Actions on a public
  repository cost nothing; the website's Vercel Hobby plan is not on the path (its 100 MB
  per-deployment static limit and non-commercial terms would bind;
  [Vercel limits](https://vercel.com/docs/limits)). The owner's steps are creating the
  repository, enabling Pages, and one DNS record.
- **What it does not give**: a Trillian-grade transparency log (the git history is the log
  until the tlog-tiles lane), search (the index is small; `sakura deps search` can grep a
  names file), download counts, and a web UI (the website can render the index later).

## 6. Semver from the compiler's own diff

`InterfaceDelta` already answers "what changed in the public surface" between two snapshots,
with per-item `effects_changed` and `contracts_changed`. A bump classifier over it is small:

| Change in a `pub` item | Bump |
|---|---|
| Item removed, type/signature changed, type's constructors or fields changed (non-opaque), variant added to a non-opaque sum type | MAJOR |
| Effect row of a function widened (new std or user effect) | MAJOR (callers' declared rows no longer suffice) |
| Effect row narrowed | MINOR |
| Error union changed in either direction | MAJOR (exhaustive matches and `Fail` handlers name the tags) |
| `requires` strengthened or `ensures` weakened | MAJOR; the other direction MINOR |
| `Schema` shape of a `pub` type changed | MAJOR (wire compatibility) |
| Manifest effect ceiling widened; `trusted` requirement appears | MAJOR (authority, design §9's extension) |
| Item added; ceiling narrowed; doc-only change | MINOR / PATCH |

Pre-1.0 packages follow Cargo's convention (a `0.x` minor is the breaking position). The gate
is applied twice, by `sakura publish` locally and by the registry workflow, and is stated as
Elm states it: it decides what can break a *build*, never what changes *behaviour*. Behaviour
remains the package's tests' and contracts' business; `sakura test` runs in the registry
workflow but is not a bump input.

## 7. Resource limits that must be explicit

Tarball at most 16 MiB compressed and 64 MiB extracted, at most 4,096 entries, paths of at
most 256 bytes and 16 segments, regular files only (no symlinks, hard links, devices, `..`,
absolute paths), fixed mtime, mode and ownership for canonicality; an index file at most
1 MiB; a resolution of at most 1,024 packages and a depth of at most 64; HTTP responses
bounded and timed out (the host already has `read_body`'s 16 MiB rule and a redirect policy).
Everything over a limit is a named `V8xxx`, never a truncation.

## 8. Open questions the RFC answers

Namespacing (bare names with a similarity rule, or scopes), the cooldown default, whether
`check`/`build` may ever fetch, where the ceiling is checked, the lock's exact shape, the
first code numbers, and what ships experimental. See [RFC 0003](../rfcs/0003-packages.md).

## 9. Sources

- Go: [MVS](https://research.swtch.com/vgo-mvs), [modules reference](https://go.dev/ref/mod),
  [sumdb design](https://go.googlesource.com/proposal/+/master/design/25530-sumdb.md),
  [mirror and checksum database launch](https://go.dev/blog/module-mirror-launch).
- Cargo: [registry index](https://doc.rust-lang.org/cargo/reference/registry-index.html),
  [sparse registry](https://blog.rust-lang.org/2022/06/22/sparse-registry-testing/),
  [SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html),
  [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks),
  [2026 project goal](https://rust-lang.github.io/rust-project-goals/2026/cargo-semver-checks.html).
- npm/pnpm: [Shai-Hulud post-mortem (trigger.dev)](https://trigger.dev/blog/shai-hulud-postmortem),
  [PostHog post-mortem](https://posthog.com/blog/nov-24-shai-hulud-attack-post-mortem),
  [Tensorlake](https://www.matthewswong.com/en/blog/tensorlake-npm-shai-hulud-ai-sdk-worm/),
  [hardening npm and pnpm](https://www.nodejs-security.com/blog/hardening-your-npm-pnpm-config-for-shai-hulud),
  [npm provenance](https://github.blog/security/supply-chain-security/introducing-npm-package-provenance/),
  [provenance walkthrough 2026](https://safeguard.sh/resources/blog/npm-provenance-attestations-walkthrough-2026).
- Deno/JSR: [How we built JSR](https://deno.com/blog/how-we-built-jsr), [Why JSR](https://jsr.io/docs/why),
  [Deno security](https://docs.deno.com/runtime/fundamentals/security/),
  [deno #171](https://github.com/denoland/deno/issues/171), [deno #6657](https://github.com/denoland/deno/issues/6657).
- Gleam/Hex: [command-line reference](https://gleam.run/documentation/command-line-reference/),
  [Hex specifications](https://github.com/hexpm/specifications/blob/main/endpoints.md),
  [Hex self-hosting](https://hex.pm/docs/self-hosting), [CVE-2026-59247](https://cna.erlef.org/cves/CVE-2026-59247.html).
- Zig: [Unboxed: Zig](https://nesbitt.io/2026/07/09/unboxed-zig.html),
  [M×N supply chain](https://nesbitt.io/2026/01/29/zig-and-the-mxn-supply-chain-problem.html).
- Swift: [SE-0292 registry specification](https://github.com/swiftlang/swift-package-manager/blob/main/Documentation/PackageRegistry/Registry.md),
  [usage](https://github.com/swiftlang/swift-package-manager/blob/main/Documentation/PackageRegistry/PackageRegistryUsage.md).
- Elm: [elm-package README](https://github.com/avh4/elm-dlang/tree/master/elm-package),
  [Elm Radio: publishing](https://elm-radio.com/episode/publishing-packages/),
  [elm-lang.org #868](https://github.com/elm/elm-lang.org/issues/868).
- Unison: [The big idea](https://www.unison-lang.org/docs/the-big-idea/),
  [code as hashes](https://softwaremill.com/trying-out-unison-part-1-code-as-hashes/).
- Roc: [platforms](https://www.roc-lang.org/platforms), [roc #10265](https://github.com/roc-lang/roc/issues/10265),
  [roc #12105](https://github.com/roc-lang/roc/issues/12105).
- Supply chain: [tlog-tiles](https://c2sp.org/tlog-tiles@main),
  [SLSA 3 with Actions and Sigstore](https://github.blog/security/supply-chain-security/slsa-3-compliance-with-github-actions/),
  [OpenSSF principles](https://repos.openssf.org/principles-for-package-repository-security.html),
  [slopsquatting (Trend Micro)](https://www.trendmicro.com/vinfo/us/security/news/cybercrime-and-digital-threats/slopsquatting-when-ai-agents-hallucinate-malicious-packages),
  [CSA note](https://labs.cloudsecurityalliance.org/research/csa-research-note-slopsquatting-ai-supply-chain-20260419-csa/),
  [LavaMoat](https://www.npmjs.com/package/lavamoat), [cackle](https://github.com/cackle-rs/cackle),
  [Rust internals: effects and crate permissions](https://internals.rust-lang.org/t/effect-system-crate-permissions/23529).
- Hosting: [GitHub releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases),
  [Vercel limits](https://vercel.com/docs/limits).
