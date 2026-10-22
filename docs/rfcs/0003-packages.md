# RFC 0003: Packages

Sakura programs are single packages over std at 1.0. This RFC adds packages additively in a
1.x minor: a `[dependencies]` table, a resolver, a lock with content hashes, a static registry
on GitHub, a publishing path with the Elm rule, and the thing no other ecosystem's packages
carry: an **effect ceiling** and an **authority gate** per package, checked by the effect
system. The evidence is [research/17](../research/17-packages.md); the lanes are
[`docs/plans/packages.md`](../plans/packages.md); the decision to start is design §17.311.

## Metadata

- Number/title: RFC 0003, Packages.
- Champion: [@icantcodefyi](https://github.com/icantcodefyi) (owner); Fable drafted under the
  owner's 2026-10-22 direction to start milestone P now.
- Discussion and PR: the `main` commit that lands this file; a pitch issue is not opened
  separately (the design has carried §9's package goals since round 1).
- Decision status: Draft.
- Implementation status: Not started; lanes in [`docs/plans/packages.md`](../plans/packages.md).
- Stabilization status: Not stabilized. Packages ship **experimental** in the first minor that
  carries them (§12).
- FCP start/end: Not started.
- Decision date/owner: Not decided.

## Motivation

A language whose users are agents needs code reuse that an agent can reason about: which
version will be built (deterministically), what a dependency *can do* (its effects), what
authority it was given (handlers, never ambient), what an upgrade changes (API, effects,
contracts), and failures that are diagnostics, not prose. Today a second package can only be
copied into `src/`. The design has promised packages since §9 and deferred them from 1.0 under
§17.225 (7) "after their own research and plan"; both now exist.

Goals: additive to every 1.0 contract; no network in `check`/`build`/`run`/`test`; no install
or build scripts; per-package effect ceilings and the authority gate enforced by the compiler;
a registry with no paid infrastructure; publish-time semver from the compiler's own diff.

Non-goals: multiple majors of one package in a build (§4.3); scoped names; binary or
prebuilt packages; a hosted service; a web UI; crates.io-style search; packages for std (std
stays inside the compiler, §9); build scripts, ever.

## Concrete examples and guide-level explanation

A library package `retry_http` (`sakura.toml`):

```toml
[package]
name = "retry_http"
version = "1.2.0"
edition = "2026"
sakura = "1.2"                       # compiler/std floor (minor), §2.1
effects = ["Http", "Clock"]          # the ceiling: std effects its public API may require

[dependencies]
backoff = "0.4.1"                    # floor; same major (0.4.x) per §4.1
```

A consumer:

```toml
[package]
name = "ingest"
version = "0.1.0"
edition = "2026"

[dependencies]
retry_http = { version = "1.2.0", effects = ["Http", "Clock"] }   # written by `sakura add`
json_pointer = "2.0.0"                                            # a pure package: no ceiling line
local_util = { path = "../util" }
scraper = { git = "https://github.com/acme/scraper", rev = "9f2c0b7e1a4d3c6b8e5f7a9d0c1b2e3f4a5b6c7d" }

[permissions]
net = ["api.example.com:443"]
```

```sakura
import retry_http.{with_retry}
import std/http.{Http}

pub fn main() -> Unit ! {Http, Clock, Console} {
  let r = with_retry(3, fn() -> Http.get("https://api.example.com/v1"))
  Console.print(show(r.status))
}
```

What a human or agent must know: `main` still declares only std effects and the host still
grants only `[permissions]`. `retry_http` performs `Http` because its public function's row
says so, and it performs it through *the caller's* handler; it cannot call `Http.host()`
itself (§5.2). `sakura add retry_http` wrote `effects = ["Http", "Clock"]` from the package's
ceiling; a later `retry_http 1.3.0` whose ceiling adds `Fs` does not compile in `ingest` until
that line is edited (`V8032`). `sakura.lock` records the exact version, SHA-256 and ceiling of
every package; `check`/`build`/`run`/`test` read it and the local cache and never fetch.

Rejected uses: `[dependencies] foo = "*"` (no floor: `V8030`); `git = …, branch = "main"`
(`V8030`: `rev` must be a full commit hash); a dependency containing `@external` or a `host()`
call without `trusted = true` in the consumer's entry (`V6020`); a dependency whose `pub fn`
row exceeds its own declared `effects` (`V3030`, reported in the dependency's package when it
is checked, and refused by `publish`); a lock older than the manifest (`V8031`, fix: run
`sakura sync`).

## Normative semantics

### 1. Manifest

`sakura.toml` gains these keys and tables; everything else in design §9 and STABILITY's
`sakura.toml` rows is unchanged.

1.1 `[package]`:
- `sakura = "<major>.<minor>"` (optional): the minimum compiler and std minor the package
  needs. Resolution treats it as a floor like any dependency; a compiler older than the
  maximum floor in the graph reports `V8033 compiler_too_old` naming the package. Absent means
  the compiler that first built it, recorded by `publish`.
- `effects = [<EffectName>, …]` (optional for unpublished packages, **required by `publish`**):
  the ceiling (§5.1). Names are std effect names (`Http`, `Fs`, `Sql`, `Env`, `Clock`,
  `Random`, `Console`, `Log`, `Task`, `Serve`, `Llm`, `Cli`, `WebSocket`, `Worker`, …, the
  modules with a `host()` constructor; `Fail` and `Test` are never listed and never counted).
  `effects = []` is a pure package.
- `description`, `license` (SPDX expression), `repository` (URL), `keywords` (≤ 8), optional
  strings; `publish` requires `description` and `license`.

1.2 `[dependencies]`: `name = "<floor>"` or `name = { version = "<floor>", effects = [...],
trusted = <bool> }`, or `{ path = "<relative dir>" }`, or `{ git = "<https URL>", rev =
"<40 hex>" }`, each optionally with `effects` and `trusted`. `<floor>` is `MAJOR.MINOR.PATCH`;
pre-release and build metadata are refused (`V8030 dependency_invalid`). A name must satisfy
`[a-z][a-z0-9_]*` and not be `std` or `main` (`V8002`'s rule).
- `effects`: the consumer's accepted ceiling for this dependency (§5.1). Absent means "the
  dependency's declared ceiling, whatever it is, as long as it is empty"; i.e. a package with a
  non-empty ceiling needs an `effects` line, written by `sakura add`.
- `trusted = true`: the dependency may acquire authority (§5.2). Default `false`.

1.3 `[packages]` (new table; consumer-side settings):
- `registry = "<https or file URL>"` (default `https://registry.sakuralang.com`).
- `vendor = "<relative dir>"`: read packages from this directory (§8), never the cache or
  network.
- `min_age = "<n>d"` (default `"3d"`): `add`/`update` refuse a registry version published less
  than this long ago (§9); `"0d"` disables.

1.4 `V8005 dependencies_not_supported` keeps its identity and name. From this minor it is
emitted only for a `[dependencies]` entry whose *form* the compiler does not support (for
example `git` before lane P5 lands, or a source kind from a future RFC), at the entry's span,
with the message naming the form. A compiler without package support (a build with the
feature off, if one exists) keeps the current message. The 1.0 contract was "the table is
refused"; accepting it is the additive change RFC 0001 reserved room for.

### 2. Packages, modules, names

2.1 A package is a directory with `sakura.toml` and `src/`; module paths are relative to
`src/` as design §9 says. A dependency's modules are imported as `import <name>/<module>`;
`PackageInput.deps` (already in the HIR) lists the names in its `[dependencies]`; a package
sees only std and its direct dependencies (`V1005` otherwise, as today).

2.2 Exactly one version of a name exists in a build (§4.3). The user package's name, each
dependency's name and `std` are distinct; a dependency whose manifest `name` differs from the
key it was added under is `V8030`.

2.3 A dependency's package is checked with the *same* std and compiler as the root; there is
no per-package std. A dependency's `tests` are not run by the consumer's `sakura test` (its
`test` bodies are compiled out, as std's are today) and its `verify` obligations are not
re-verified (they are listed in `build --manifest` under the package, status as published).

### 3. Lockfile

3.1 `sakura.lock` is TOML, `version = 2`, written only by `add`, `remove`, `update`, `sync` and
`vendor`, committed. `version = 1` (today's empty file) is read as "no dependencies" and
rewritten as 2 by the first writing command; any other version is `V8031 lockfile_invalid`.

```toml
version = 2

[[package]]
name = "retry_http"
version = "1.2.0"
source = "registry+https://registry.sakuralang.com"
checksum = "sha256:3f8a…"          # of the canonical tarball
effects = ["Http", "Clock"]
trusted = false
dependencies = ["backoff 0.4.3"]
```

Entries are sorted by name; a `path` source has `source = "path+../util"` and no checksum (its
files are read live); a `git` source has `source = "git+https://…#<sha>"` and the checksum of
the canonical tarball built from that commit.

3.2 The lock is **authoritative** for every compiler command: `check`, `build`, `run`, `test`,
`verify`, `fmt`, `doc`, the LSP and MCP read `sakura.lock` and load each entry from the vendor
directory, else the cache (§7); they never resolve and never fetch. A manifest whose
`[dependencies]` is not satisfied by the lock (a name added, removed, a floor above the locked
version, a changed `path`/`git`/`rev`, a changed `effects`/`trusted` that the lock contradicts)
is `V8031 lockfile_stale` at the manifest line, fix "run `sakura sync`". A locked entry whose
files are absent is `V8034 package_missing` naming the entry, same fix. A cached tarball whose
hash differs from the lock is `V8035 checksum_mismatch` with both hashes; nothing is loaded.

### 4. Resolution

4.1 **Floors.** `"1.2.0"` means `>= 1.2.0, < 2.0.0`. For major 0, `"0.4.1"` means
`>= 0.4.1, < 0.5.0` (Cargo's convention; the breaking position is the minor).

4.2 **MVS.** Starting from the root's `[dependencies]`, the build list is computed as
[Cox's algorithm](https://research.swtch.com/vgo-mvs): walk the graph of manifests, and for
each name take the maximum of all floors reached. It reads only manifests, which the index
carries per version (§6), so resolution needs no tarballs. Yanked versions are skipped unless
the lock already names them (then kept, with `W8010 dependency_yanked`). Versions younger than
`min_age` are skipped by `add`/`update` (§9). The result is a pure function of the manifests
and the index state; the index state is recorded in the lock entries, so the lock is
reproducible from the index history.

4.3 **One major per name.** If two floors for one name have different majors (or different
minors at major 0), resolution fails with `V8036 dependency_major_conflict` naming the two
requirement paths (`ingest → retry_http 1.2.0 → backoff 0.4.1` and `ingest → other 2.0.0 →
backoff 0.5.0`). The fix is a human or agent decision (update one requirement); the compiler
never picks.

4.4 **Operations.** `add` adds a floor (the newest eligible version at add time) and resolves;
`remove` removes one; `update [name]` raises the floor of `name` (or all) to the newest
eligible and re-resolves (the only operation that moves anything else: transitive floors may
rise by MVS); `sync` resolves from the manifest without changing floors and fetches. Each
prints an `update_plan` record (§10) before writing; `--plan` prints without writing.

4.5 **Limits.** At most 1,024 packages and a dependency depth of 64 (`V8037 resolution_limit`).

### 5. Effect ceilings and the authority gate

5.1 **Ceiling.** For a package with `[package] effects = C`, the effect checker computes
`U = ⋃ { std effects in the row of f | f is pub in the package }` (user-defined effects are
excluded: they carry no authority, the consumer's handler decides). `U ⊆ C` is required;
otherwise `V3030 effect_ceiling_exceeded` at the offending `pub fn`, naming the effect and
the ceiling, phase EFFECTS, reported when that package is checked (as the root or as a
dependency being checked by `publish`). A package without an `effects` key has no check
(unpublished, path/git); `publish` requires the key. In the consumer, each dependency entry's
accepted `effects` (default `[]`) must contain the dependency's declared ceiling as recorded
in the lock; otherwise `V8032 dependency_effects_exceeded` at the consumer's dependency line,
naming the extra effects, with a `suggested` fix that rewrites the line (`authority_change`,
never `machine_applicable`, design §7.3). Transitively: a dependency's own `[dependencies]`
entries are subject to the same rule inside that dependency, so a ceiling is honest all the
way down; the consumer's line about `retry_http` bounds what `retry_http` *and everything it
calls* can require, because `retry_http`'s public rows already include what its callees
perform.

5.2 **Authority gate.** Authority enters a program at four kinds of site: a `host()`
constructor of a std effect module, an `@external` binding, `Secret.expose`/`Secret.map`, and
`Untrusted.trust`/`Html.raw`. In a dependency (any package other than the root), a call to a
`host()` constructor or an `@external` declaration is `V6020 dependency_acquires_authority` at
the site, unless the consumer's entry for *that* dependency says `trusted = true`, in which
case the site is listed in `sakura audit` and `build --manifest`'s `trusted` list under the
package name (design §6.6). `@trust` sites in dependencies stay legal and are listed by
`audit` per package (`audit --json` records gain a `package` field). The root package is
unchanged. Rationale: a `host()` call reads the root grants; without the gate a dependency
could use `Fs` without `Fs` in any public row. With it, the only authority a dependency has is
what its public rows require and what the consumer's handlers provide.

5.3 **Tests and std.** std is exempt from both rules (it is the trusted computing base, §6.6).
A dependency's `test` bodies are not compiled by consumers (§2.3), so a test's `Env.host()`
is not a gate violation for consumers.

### 6. Registry

6.1 **Identity.** Bare names, `[a-z][a-z0-9_]*`, ≤ 64 bytes, first-come, with a similarity
rule: a new name is refused if, after folding `_` and digits, it equals an existing name or is
within Damerau–Levenshtein distance 1 of one (the registry workflow enforces it; `publish`
checks it first). Reserved: `std`, `main`, `sakura`, `test`, the std module names.

6.2 **Index.** A git repository with `index/<n>/<name>` for names of length 1–2 under `1/`,
`2/`, and `<first two letters>/<name>` otherwise (Cargo's layout), each a JSON Lines file,
one object per version:

```json
{"schema_version":"1.0","name":"retry_http","version":"1.2.0","checksum":"sha256:…",
 "deps":[{"name":"backoff","floor":"0.4.1"}],"effects":["Http","Clock"],"trusted":false,
 "sakura":"1.2","source":{"repo":"https://github.com/acme/retry_http","commit":"<sha>"},
 "published":"2026-11-03T10:14:00Z","run":"https://github.com/<owner>/sakura-registry/actions/runs/<id>",
 "yanked":false}
```

plus `owners/<name>` (GitHub logins, one per line) and `names` (every name, one per line, for
`deps search`). Lines are **append-only**: a workflow check refuses a change to an existing
line other than `yanked` flipping. The index is served by GitHub Pages at the `[packages]
registry` URL; `file://` registries have the same layout (tests, mirrors, air-gapped use).

6.3 **Tarballs.** Assets of a GitHub Release of the registry repository tagged
`<name>/<version>`, named `<name>-<version>.tar.gz`, built by the registry workflow (never
uploaded by the publisher). The canonical tarball: entries sorted by path, mtime 0, uid/gid 0,
mode 0644 (0755 for directories), regular files only, containing exactly `sakura.toml`,
`src/**/*.sak`, and if present `README.md`, `CHANGELOG.md`, `LICENSE*`; nothing else (no
`sakura.lock`, no `target/`, no dot files). Limits: ≤ 16 MiB compressed, ≤ 64 MiB extracted,
≤ 4,096 entries, path ≤ 256 bytes and ≤ 16 segments, no `..`, no absolute path, no symlink or
hard link; a violation is `V8038 package_archive_invalid` naming the entry. `checksum` is the
SHA-256 of the compressed bytes. The client fetches `https://github.com/<owner>/sakura-registry/releases/download/<name>%2F<version>/<name>-<version>.tar.gz`
(the URL is a field of the index line, so the host can move), verifies the hash, then extracts
under the limits into the cache.

6.4 **Transparency.** The index's git history is the log for this RFC: every clone has every
state, and the append-only check makes a rewrite visible. A C2SP tlog-tiles mirror of the
checksums written by the same workflow is a later lane (plan P8), not a condition here.

### 7. Cache

`<cache>/sakuralang/packages/<name>/<version>-<first 16 hex of checksum>/` holds the extracted
tree and `.tarball` (the verified compressed bytes), `<cache>` as STABILITY's cache rule
(`SAKURA_CACHE_DIR`, else XDG/LocalAppData). The cache is content-addressed and may be deleted
at any time; `sync` refills it. `SAKURA_NO_CACHE` does not apply (packages must live
somewhere); a read-only cache is fine for compiler commands. Git sources are fetched by
shelling out to `git` on `PATH` (`V8039 git_missing` otherwise), checked out at `rev` into a
temporary directory, canonicalised into a tarball and cached like a registry package.

### 8. Vendoring and offline use

`sakura vendor` writes every locked package's extracted tree to `[packages] vendor` (default
`vendor/`) as `vendor/<name>-<version>/` plus `vendor/.checksums` (name, version, sha256 of
the canonical tarball) and sets the manifest key if absent. With `vendor` set, compiler
commands read only there and verify each tree's canonical tarball hash against the lock
(`V8035` on mismatch); `sync` verifies and fetches nothing. A `file://` registry plus a
vendor directory is the air-gapped configuration.

### 9. Publishing

9.1 `sakura publish [--dry-run] [--json]` in a package directory: requires `[package]`
`effects`, `description`, `license`, `repository`, a clean git tree at a commit that exists on
`repository` (checked via `git ls-remote`), and runs: `check` (no diagnostics), `test`
(green), the ceiling check (§5.1), the authority rule (the package may not contain `host()`
calls or `@external` unless it declares `trusted = true` in `[package]`, which the index
records and consumers must accept), the name rule (§6.1), and the **bump rule** (§9.2)
against the latest published version in the same major line (none for a first publish, which
must be `0.1.0` or `1.0.0`). It builds the canonical tarball, prints a `publish_check` record
(§10) with the computed bump, the ceiling, the tarball hash and the index line it proposes,
and, without `--dry-run`, opens a pull request to the registry repository containing that line
through the user's `gh` authentication (`V8039 gh_missing` if `gh` is absent or unauthenticated;
the record always prints, so the PR can be opened by hand).

9.2 **Bump rule.** `InterfaceDelta` between the previous version's public surface and the
candidate's classifies the minimum bump: MAJOR for a removed or changed public item, a widened
effect row, any error-union change, a strengthened `requires` or weakened `ensures`, a changed
`Schema` shape of a public type, a widened ceiling or a new `trusted` requirement; MINOR for an
added item, a narrowed row, a weakened `requires` or strengthened `ensures`, a narrowed
ceiling; PATCH otherwise. At major 0 the positions shift down one (MAJOR → minor, MINOR → patch).
A candidate version below the minimum is refused with `V8040 version_bump_too_small` listing the
items. Larger bumps are allowed. The rule decides what can break a build, not behaviour.

9.3 **The registry workflow** (in the registry repository, GitHub Actions) on each PR: the PR
author is in `owners/<name>` (or the name is new, then the author becomes its owner), the line
is append-only, the name rule holds, the commit exists on the repository named, the tarball it
builds from that commit with the released `sakura` has the stated checksum, `check` and the
bump rule pass against the previous index line, the `sakura` floor is satisfied by the released
compiler; then it uploads the release asset with a build attestation, commits the line and
`names`, merges, and Pages deploys. A failing check is a PR comment with the `V` code. Yank is a
PR flipping `yanked`, by an owner. There is no unpublish.

9.4 **Cooldown.** `add`/`update` skip versions younger than `[packages] min_age` (default three
days), as pnpm's `minimumReleaseAge` does. `sync` honours the lock regardless. A no-downgrade
rule: a version whose index line lacks `run` (an attestation) when the previous had one is
skipped by `add`/`update` with `W8011 provenance_downgrade`.

### 10. CLI and JSON

New commands, all under the existing exit codes (0; 1 for `V` diagnostics; 2 usage):

| Command | Does | Network |
|---|---|---|
| `sakura add <name>[@<floor>] [--path DIR \| --git URL --rev SHA] [--trusted] [--plan] [--json]` | adds the entry with its `effects` from the resolved ceiling, resolves, fetches, writes manifest and lock | yes |
| `sakura remove <name> [--json]` | removes the entry, re-resolves, writes | no |
| `sakura update [<name>] [--plan] [--json]` | raises floors to the newest eligible, prints the plan (versions, API delta, ceiling delta), writes | yes |
| `sakura sync [--json]` | resolves from the manifest without moving floors, fetches, writes the lock; the command every `V8031`/`V8034` fix names | yes |
| `sakura vendor [--json]` | §8 | yes (first time) |
| `sakura deps [tree \| why <name> \| capabilities \| search <text>] [--json]` | the build list as a tree; the requirement paths to a name; every package's ceiling, `trusted` flag and authority sites (design §6.5's `sakura deps capabilities`); names matching `<text>` from `names` | `search` only |
| `sakura publish [--dry-run] [--json]` | §9 | yes (not with `--dry-run`) |

JSON Lines record kinds, each `schema_version = "1.0"`, additive under RFC 0001: `dependency`
(one per build-list entry: name, version, source, checksum, effects, trusted, parents),
`update_plan` (per package: from, to, api `{added, removed, changed}` as `InterfaceDelta`
renders them, effects before/after), `publish_check` (bump, ceiling, checksum, the index line,
diagnostics), `deps_summary`. `check`'s `summary` is unchanged. Usage failures remain human
stderr with exit 2 (design §17.242 (3)). `cli.txt` is regenerated; the rows are additive.

### 11. Build integration

11.1 The whole graph is one program, as today: HIR lowers every package; types, effects and
Core see one module graph. The JS emitter writes `out/<package>/<module>.js` per package
module (today's single-package layout is unchanged for a program without dependencies); WASM
and native produce the same single artifacts as today. No ABI change: packages add modules,
not a boundary. A dependency's `@external` JS (only in a `trusted` package) is copied beside
its modules; `V8004` on WASM and native is unchanged.

11.2 The std analysis cache (`std_cache.rs`) is not extended by this RFC; a per-package
analysis cache keyed by checksum is plan lane P8 after a measurement.

11.3 `sakura new`/`init` keep writing `sakura.lock` with `version = 2` and no packages, and
`.gitignore` gains nothing (vendor directories are meant to be committed when used).

### 12. Experimental, then stable

The first minor that carries lanes P1–P4 ships packages labelled **experimental** in
STABILITY.md: the manifest keys, lock format, index schema, CLI and JSON kinds of this RFC may
change in the following minor with a CHANGELOG migration; `V` identities are allocated and
never reused regardless. Opting in is writing `[dependencies]`; programs without it are
unaffected. Stabilization is a separate owner decision when: the registry has served ≥ 20
published packages from ≥ 3 publishers for one minor; the six dogfood programs each consume at
least one registry package and upgraded across a minor from the notes alone; the fuzz targets
of plan P7 have ≥ 24 clean CPU-hours each; the lock v2 and index 1.0 schemas changed in no way
during the minor; the tlog-tiles mirror (P8) exists or is explicitly waived in the entry.

## Alternatives and prior art

Research/17 §2 and §5 hold the comparison; the rejected alternatives, each with its reason
there: a hosted registry (paid infrastructure); URL/hash-only dependencies (no names, no yank,
no semver gate; kept as the `git` source kind); npm as transport (namespace and semantics
governed elsewhere); multiple majors in one build (type identity and import collisions); a
backtracking solver (non-deterministic across index states, harder to explain); build scripts
(none exist, none will); scoped names (`@owner/name` collides with `owner/module` import
paths; the similarity rule and first-come with owners files give the anti-squatting value);
auto-fetch in `build` (an agent's build must not change meaning because a registry changed);
`host()` allowed in dependencies (ambient authority); ceilings computed-but-not-declared (an
upgrade could widen them silently, design §16 question 11 resolved toward declared).

## Drawbacks and costs

A new crate and a network client in the CLI; a second manifest parser surface (`[packages]`,
dependency tables) with its fixture burden; the registry workflow is code in another repository
that must be kept in step with the released `sakura`; the owners-file model has no account
recovery beyond GitHub's; the git-history log is weaker than a Merkle log until P8; the
cooldown delays security fixes by three days unless a user sets `min_age = "0d"`; one major
per name means some graphs do not build until someone upgrades. The bump rule cannot see
behaviour (Elm's limit) and says so in its message.

## Compatibility and migration

Everything is additive under RFC 0001: `[dependencies]`, `[packages]`, `[package] sakura`,
`effects`, `description`, `license`, `repository`, `keywords` and dependency `effects`/
`trusted` become accepted where they were `V8005`/`V8006` (a program that failed now compiles;
no program that compiled changes); `V8005` keeps its identity for unsupported dependency forms;
new codes `V3030`, `V6020`, `V8030`–`V8040`, `W8010`–`W8011` are allocated if free at landing
(Opus confirms in `codes.rs`); new JSON kinds at `1.0`; new CLI commands and flags, none
existing changed; `sakura.lock` `version = 2` reads `version = 1`; no source semantics, std
signature, exit code or ABI change; the MSRV is unchanged unless the HTTP client needs a newer
toolchain (then a minor, with the release note RFC 0001 requires). STABILITY.md's `[dependencies]`
row changes from "reserved" to "experimental, RFC 0003" in the landing minor; the `V8005` row's
text changes to the new meaning; the website's manifest and `V8005` pages follow (plan P6). A
CHANGELOG `Added` entry per lane. `tests/compat` replays the previous release's `V8005`
fixtures: the ones that now compile are listed in `allow.txt` with this RFC as the reason.

## Agent and tooling impact

`sakura add` records a dependency's ceiling at the moment of adding, so an agent's later
`check` fails on a silent widening with a one-line fix it must choose to apply (never
`machine_applicable`: it is an `authority_change`). `deps capabilities --json` is the authority
graph design §6.5 promised. `update --plan --json` is a reviewable upgrade record with the
`InterfaceDelta` the agent already knows from `check_patch`. The MCP server gains `deps` and
`update_plan` tools returning the same records (plan P6, design §17.242 (5c)); the LSP resolves
hover and definition into dependency sources (read-only, in the cache). `sakura doc` indexes
dependency symbols with a `package` field. Unknown names fail with `V8034`/`V8030` and no
suggestion (research/17 §2.13). `skills/sakura` gains a packages reference. Context cost: one
page.

## Security and resource limits

Trust boundaries: the index (HTTPS from GitHub Pages; integrity by append-only history and by
the checksum chain into the lock), tarballs (SHA-256 from the index line before extraction),
publishers (GitHub identity through the registry workflow; owners files), the registry workflow
itself (part of the TCB for *provenance*, not for *authority*: a malicious package still has
only the effects its rows declare and no `host()`), `git` and `gh` on `PATH` (shelled out,
arguments never from untrusted strings without `--`). No install or build scripts exist. Limits
are §6.3's and §4.5's, plus HTTP: 30 s connect/read timeouts, bodies bounded by the tarball
limit, redirects followed only within the same host, `https` only except `file://` and
loopback. Failure is always a named `V` with nothing written to the lock or cache. No unsafe
code.

## Tests and validation

- `tests/ui`: every new code, positive and negative, including `V3030` at the `pub fn`,
  `V6020` at a `host()` call in a dependency and its `trusted = true` twin, `V8032` with its
  `suggested` fix, `V8031` for each stale cause, `V8036` naming both paths.
- `compiler/tests/packages/` (new harness): multi-package projects with a `file://` registry
  fixture directory; MVS cases (diamond, floors raised by transitive requirements, major
  conflict, yanked kept when locked and skipped otherwise, `min_age`); lock round-trips;
  vendor; canonical tarball byte-for-byte determinism across platforms; extraction limits.
- `examples/packages_*`: a two-package project run on js, wasm and native with the same
  stdout (parity).
- Bump-rule table tests over `InterfaceDelta` fixtures in `tests/compat`-style pairs.
- `tests/compat`: the previous release's `V8005` fixtures listed in `allow.txt`.
- Fuzz (plan P7): tarball extraction, index line parsing, lock parsing, the resolver on random
  graphs (always terminates; same input same output).
- Not runnable until the registry repository exists: the workflow's end-to-end publish; it is
  tested against a fork first (plan P3).

## Implementation and rollout

Lanes P1–P8 in [`docs/plans/packages.md`](../plans/packages.md): P1 manifest, lock, path
dependencies, multi-package loading, ceilings and the gate (startable now); P2 the
`sakura_packages` crate (resolver, index client, tarball, cache, `add`/`remove`/`update`/
`sync`/`vendor`); P3 the registry repository and workflow (owner creates the repository and the
DNS record); P4 `publish` and the bump rule; P5 git sources; P6 docs, STABILITY, website,
skill, MCP tools; P7 fuzz targets and the dogfood package; P8 tlog-tiles mirror and the
package analysis cache. Each lands as its own CHANGELOG entry. Rollback: a lane's commit is
revertible on its own; nothing it adds is read by programs without `[dependencies]`.

## Unresolved questions

1. The registry's default URL depends on the owner's DNS record; until it exists the default
   is the Pages URL, changed in one constant.
2. Whether the registry workflow should also run `verify` on published packages (cost on
   hosted runners) — defer to P3's first month of data.
3. Whether `trusted` should be finer than a boolean (per `host()` effect) — a later RFC if a
   consumer asks.
4. Whether a `[workspace]` of several local packages is needed before publishing exists;
   `path` dependencies cover the dogfood cases, so not in this RFC.

## Decision record

Not decided. The maintainer records FCP dates and disposition here.
