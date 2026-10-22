# P — packages and the package manager

**Status: executable plan, provisional; started 2026-10-22 under the owner's direction to
"start doing them all right now" (design §17.311).** The decisions are
[RFC 0003](../rfcs/0003-packages.md) (Draft; its FCP does not gate the lanes, as RFC 0001's
did not gate the betas, §17.291); the evidence is [research/17](../research/17-packages.md).
Nothing here is a date and nothing here gates `v1.0.0` (§17.225 (7): packages are 1.x,
additive). Lanes are bounded by files (§17.285) and run in parallel; "after" names a landing on
`main`. Packages ship **experimental** in the first minor that carries P1–P4 (RFC §12). Opus
codes; Sol does CI, workflows and the registry repository; Astra measures and writes docs;
Fable answers readings in `compiler/REQUESTS.md` and ratifies in §17; the owner creates the
registry repository, the DNS record, and blesses nothing (no counted baseline is touched).

## 1. Inputs and fixed decisions

- RFC 0003's normative text; where this plan and the RFC differ, the RFC rules and the plan is
  corrected.
- The HIR already lowers several packages (`PackageInput { name, deps, modules }`,
  `sakura_hir/src/lower.rs`); `sakura_analysis` loads one (`module_of`, `package_name`,
  `disk.rs`). `InterfaceDelta` (`sakura_analysis/src/specs.rs`) is the API diff.
- No new crates beyond what `compiler/Cargo.lock` already pins: `reqwest` + `rustls` (HTTP),
  `flate2` (gzip), `sha2`, `semver`. A `tar` crate is **not** in the lock: the canonical
  tarball writer and the limited reader are written in the new crate (ustar, regular files and
  directories only; ~300 lines) unless Opus files a REQUESTS entry for a crate with a reason.
  `git` and `gh` are shelled out to, never linked.
- Diagnostic codes proposed by the RFC: `V3030`, `V6020`, `V8030`–`V8040`, `W8010`, `W8011`.
  Opus confirms each is free in `codes.rs` at landing and takes the next free number in the
  family otherwise, correcting the RFC text in the same commit.
- Network only in `add`, `update`, `sync`, `vendor`, `deps search`, `publish`. A test that
  reaches the network fails the lane.
- Determinism: resolver, lock writer, tarball writer and index parser are pure functions with
  byte-for-byte fixtures on Linux, macOS and Windows.

## 2. Lanes

| Lane | What | Owner | Files | Starts | Acceptance |
|---|---|---|---|---|---|
| **P1** | **Manifest, lock, path dependencies, multi-package loading, ceilings and the gate** (RFC §1, §2, §3.1–3.2 for `path` only, §5, §10's `deps tree\|capabilities`) — the brief is §3 below | Opus | `sakura_analysis/src/manifest.rs` (dependency tables, `[packages]`, `[package]` new keys), new `sakura_analysis/src/lock.rs` (lock v2 read/write, v1 read), `sakura_analysis/src/disk.rs` and `lib.rs` (load every locked package's `src/` as its own `PackageInput`; `module_of` by package root; `V8031`/`V8034`), `sakura_hir/src/lower.rs` (only if the unknown-package hint or `deps` plumbing needs it), `sakura_types`/the effects checker (`V3030` ceiling; `V6020` gate at `host()` calls and `@external` in non-root packages), `sakura_cli/src/main.rs` + new `sakura_cli/src/deps.rs` (`deps tree`, `deps capabilities`, `--json`), `sakura_cli/src/new.rs` (lock `version = 2`), `sakura_diagnostics/src/codes.rs`, `tests/ui/`, new `compiler/tests/packages/` harness + `sakura_cli/tests/packages.rs`, `examples/packages_two/`, `compiler/contracts/cli.txt`, `tests/compat/allow.txt`, CHANGELOG | **now** | §3's list; `cargo test --workspace --locked` green; a two-package example runs identically on js, wasm and native; LSP hover/definition into the path dependency; no network code in the lane |
| **P2** | **`sakura_packages` crate**: MVS resolver over index manifests (RFC §4), index client for `file://` and `https` (§6.2), canonical tarball writer and limited reader (§6.3), cache layout (§7), `sakura add`/`remove`/`update [--plan]`/`sync`/`vendor` (§8, §10), `min_age` and the yank/provenance rules (§4.2, §9.4), `update_plan` and `dependency` records | Opus | new `crates/sakura_packages/` (pure parts may start now in the crate alone), `compiler/Cargo.toml` (member; `reqwest`/`rustls` features for the CLI), `sakura_cli/src/{main.rs,packages.rs}` (new file for the commands), `sakura_analysis/src/lock.rs` (registry/git entries added to the shape P1 fixed), `compiler/tests/packages/registry/` (a `file://` index fixture with tarballs), `tests/ui/`, `cli.txt`, `json.txt` field baseline, CHANGELOG | pure parts now; CLI integration after P1 | MVS cases (diamond, raised transitive floor, major conflict `V8036` naming both paths, yanked kept-if-locked, `min_age` skip); tarball bytes identical across the three OS runners; extraction refuses each limit with `V8038`; `sync` on a stale lock repairs it; a cache wipe + `sync` rebuilds; no network in tests (the fixture is `file://`); `add` writes the dependency's `effects` line |
| **P3** | **The registry repository and workflow** (RFC §6, §9.3): `index/`, `owners/`, `names`, the append-only check, the publish PR workflow (ownership, clone at commit, build the tarball with the released `sakura`, checksum, `check`, bump rule, release asset + `actions/attest-build-provenance`, merge), Pages | Sol (workflows), owner (repository, Pages, DNS) | a new repository `sakura-registry` (not this one); in this repo only `docs/release.md` (one section on the registry's operation) | repository skeleton now; the workflow when a release carries P2 and P4 (`publish --dry-run` output is the workflow's input) | an end-to-end publish of a test package from a fork, then from `main`; a rewrite of an existing line refused by CI; a second publisher refused for an owned name; the Pages URL serves the index with correct `Content-Type`; the default registry URL constant updated in one commit |
| **P4** | **`sakura publish` and the bump rule** (RFC §9.1–9.2): `InterfaceDelta` → bump classifier (`specs.rs`), the ceiling/authority/name/clean-tree checks, `publish_check` record, PR creation via `gh` | Opus | `sakura_analysis/src/specs.rs` (`bump()` over `InterfaceDelta`; error-union and `Schema` shape in `InterfaceEntry` if not already rendered), `sakura_cli/src/publish.rs` (new), `sakura_packages` (tarball reuse), `tests/compat`-style pairs for the bump table, `tests/ui/` for `V8040`, `cli.txt`, CHANGELOG | after P1 (ceilings) and P2's tarball | every row of RFC §9.2's table has a positive and a negative pair; major-0 shift tested; `--dry-run` prints the exact index line P3's workflow accepts; no network with `--dry-run` |
| **P5** | **Git sources** (RFC §1.2, §7): `{ git, rev }` with a 40-hex `rev`, shell-out to `git`, canonicalised and cached like a registry package; `V8039` | Opus | `sakura_packages/src/git.rs` (new), `sakura_analysis/src/lock.rs` (git entries), `tests/packages/` (a local bare repository fixture), `tests/ui/` | after P2 | a `git` dependency from a local bare repo locks with the tarball checksum; a branch name is `V8030`; a missing `git` is `V8039`; `V8005` for `git` entries disappears in this commit |
| **P6** | **Docs and tooling surface**: website `manifest.mdx` (new keys, `[packages]`, `[dependencies]` no longer reserved), `cli.mdx`, a new `packages.mdx` (the model: ceilings, the gate, the lock, the registry), error pages for every new code, `V8005`'s page rewritten, STABILITY rows (experimental), `skills/sakura/references/packages.md`, `docs/llms.txt`, MCP `deps` and `update_plan` tools (`docs/mcp.md`), design §9/§12.1 text aligned with the RFC | Astra (docs), Opus (MCP tools) | `website/content/docs/**`, `STABILITY.md`, `skills/sakura/**`, `docs/llms.txt`, `docs/mcp.md`, `sakura_mcp/src/`, `docs/design.md` §9 and §12.1 (one sentence each pointing at RFC 0003) | each part after its lane lands; STABILITY rows at the preparation commit of the first minor carrying P1–P4 | `check-doc-links.py` green; `agent_docs --check` green; every new `V`/`W` has an `explain` text and a page |
| **P7** | **Fuzz and dogfood**: fuzz targets for tarball extraction, index line parsing, lock parsing and the resolver (terminates; deterministic); one dogfood program's shared code extracted into a package published to the registry (the first real package), the other five consuming it over one minor | Opus (fuzz), Astra (dogfood) | `compiler/fuzz/` (four targets, `fuzz.yml` matrix rows, the ledger section), the dogfood repositories, `evals/`'s notes | fuzz after P2; dogfood after P3's first publish | ledger rows at 0 CPU-h created; the dogfood upgrade recorded as RFC §12 asks |
| **P8** | **Later**: a C2SP tlog-tiles mirror of the index's checksums written by the registry workflow (RFC §6.4); a per-package analysis cache keyed by checksum (`std_cache.rs`'s pattern) after a measurement shows dependency analysis dominates `check` on a 10-package project | Sol (tiles), Opus (cache) | registry repository; `sakura_analysis/src/std_cache.rs` generalised | after P3 is live; after a measurement | tiles verify with an independent client; the cache cuts `check` on the 10-package project by ≥ 50 % with identical diagnostics |

Order of landings: P1 → P2 (pure parts in parallel with P1) → P4 and P5 → P3's workflow (needs
a release with P2+P4; the repository skeleton earlier) → P7 → P8. P6 trails each. The first
minor carrying P1–P4 labels packages experimental; the stabilization entry is RFC §12's.

### Start conditions in one view

- **Now:** P1; P2's pure parts (`crates/sakura_packages/` alone: resolver, tarball, index
  parser, lock-entry types) in a second lane that does not touch any P1 file; P3's repository
  skeleton (owner + Sol, outside this repo).
- **When P1 lands:** P2's CLI integration; P4's classifier (`specs.rs` is not a P1 file, so the
  classifier itself may start now; its `publish` command waits for P2's tarball).
- **When P2 lands:** P5; P7's fuzz targets.
- **When a beta carries P2 and P4:** P3's workflow; then P7's dogfood publish.
- **Never before a measurement:** P8's cache.

## 3. Lane P1's brief

**Progress (2026-10-22): P1 landed on `main`** (the `feat(packages)` commits of that date): every
item below, with the readings and the places it went beyond the file list in
`compiler/REQUESTS.md`'s P1 entry. `V8031` is registered as `lockfile_invalid` for both its
causes; `V8005`'s git fixture is `tests/ui/V8005_git_dependency`, for P5 to flip. P2's CLI
integration may start: `sakura_analysis::lock` holds the lock shape and the `path` graph walk it
extends, and `sync` replaces `disk::lock_path_dependencies`, the stand-in that writes a path-only
project's lock.

Files: the P1 row above and nothing else (`sakura_hir` only if the plumbing needs it; say so
in the commit message). Everything is offline.

1. **Manifest** (`manifest.rs`). Accept `[dependencies]` entries of the forms RFC §1.2
   defines, parsed into `Dependency { name, source: Floor(semver) | Path(rel) | Git { url,
   rev }, effects: Vec<String>, trusted: bool }`; `[packages] { registry, vendor, min_age }`;
   `[package] { sakura, effects, description, license, repository, keywords }`. Validation
   errors are `V8030 dependency_invalid` at the entry's span (name shape, floor shape, missing
   `rev`, `rev` not 40 hex, both `version` and `path`); `V8005` stays for `git` entries until
   P5 (its message: "`git` dependencies are not supported in this version"), with a fixture
   that P5 flips. Keep `dependencies_key` for `V8005`'s span. `[dependencies]` absent or empty
   is a program over std exactly as today.
2. **Lock** (`lock.rs`). `version = 2` TOML as RFC §3.1, sorted entries, `path` entries without
   checksum; read `version = 1` as empty; `V8031 lockfile_invalid` for anything else. A
   staleness check: manifest `[dependencies]` vs lock entries (name set, floors ≤ locked
   version, `path` equal, `effects`/`trusted` equal) → `V8031 lockfile_stale` at the manifest
   line with the fix text "run `sakura sync`" (`sync` does not exist yet in P1: for `path`
   dependencies P1 writes the lock itself when `sakura.lock` is `version = 1` or absent and
   every dependency is `path`, so a P1 project needs no P2 command; this convenience is
   removed when P2 lands and `sync` exists; record it in the CHANGELOG line).
3. **Loading** (`disk.rs`, `lib.rs`). For each lock entry with a `path` source, load its
   `sakura.toml` and `src/**/*.sak` as a `PackageInput` named by its manifest `name` with
   `deps` from its `[dependencies]`; `module_of` resolves a file to its package by the nearest
   enclosing manifest, not "the last `src/`"; a dependency's `test` bodies are skipped as
   std's are; a missing directory is `V8034 package_missing` at the lock entry. The root's
   `deps` are its `[dependencies]` names. `V1005`'s hint text stays.
4. **Ceiling** (effects checker). With `[package] effects = C` present, `U ⊆ C` over `pub fn`
   rows restricted to std effect names, else `V3030 effect_ceiling_exceeded` at the `pub fn`
   with the effect and the ceiling in the message; absent `effects` → no check. Consumer side:
   for each dependency, declared ceiling (from its manifest; recorded in the lock entry)
   ⊆ the consumer's `effects` for it (default `[]`), else `V8032 dependency_effects_exceeded`
   at the consumer's dependency line with a `suggested`/`authority_change` fix rewriting the
   line.
5. **Gate** (effects checker or HIR lowering, whichever already sees `host()` resolution and
   `@external`). In a non-root package, a call to any std `host()` constructor or an
   `@external` declaration is `V6020 dependency_acquires_authority` at the site unless the
   root's entry for that package has `trusted = true`; when trusted, `audit` and `build
   --manifest` list the site under the package name (`audit --json` records gain a `package`
   field; `manifest` record's `trusted` entries gain `package`; both additive, `json.txt`
   baseline updated).
6. **CLI** (`deps.rs`). `sakura deps tree [--json]` (the lock's build list as a tree from the
   root; `dependency` records) and `sakura deps capabilities [--json]` (per package: ceiling,
   `trusted`, authority sites). No network. `cli.txt` regenerated.
7. **Scaffold** (`new.rs`). `sakura.lock` written as `version = 2`.
8. **Tests.** `tests/ui` for `V8030` (each cause), `V8031` (each stale cause and invalid lock),
   `V8034`, `V3030` (over and at the ceiling), `V8032` (with the fix), `V6020` (a `host()` call
   and an `@external` in a dependency; the `trusted = true` twins pass and appear in `audit`);
   the new `compiler/tests/packages/` harness with multi-directory projects (the `ui` harness
   is single-file; extend it or add the new one, as RFC 0001's row 330 allowed for manifests);
   `examples/packages_two/` (a root and a `path` dependency with a `Clock` ceiling) in the
   examples harness on js, wasm and native; `tests/compat/allow.txt` lists the previous
   release's `V8005` fixtures that now compile, reason "RFC 0003 P1"; LSP test: hover and
   definition into the dependency's source.
9. **Docs in the lane.** CHANGELOG `Added` entry; `V8005`'s `explain` text updated; the
   STABILITY `[dependencies]` row is **not** edited in P1 (P6 does it at the minor's
   preparation commit); `docs/design.md` is not edited (Fable records readings).

Acceptance: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked
-- -D warnings`, `cargo test --workspace --locked` green; the example's three-target parity;
`cli.txt` and `json.txt` diffs additive only; no `reqwest` or process spawn in any P1 file.
Readings (names of codes, the fix's exact text, where the gate lives) go to
`compiler/REQUESTS.md` dated, as the M5 lanes did; Fable answers in §17.

## 4. Owner-only items

- Create the `sakura-registry` repository (public), enable Pages from `main`, and add the DNS
  record for `registry.sakuralang.com` (or tell Fable the name to use). Until then the default
  registry URL is the Pages URL and `file://` registries carry every test.
- Nothing else: no bless, no tag waits for P (§17.311).
