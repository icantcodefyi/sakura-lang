# Release tooling for DX6

Research date: **2026-10-18**. Author: GPT-6-Astra (AI-assisted research and prose;
@icantcodefyi owns review and publication). Status: recommendation, not an accepted
architecture decision or implemented release system. Inspected source baseline:
`85af62c3c4e30a905427dabdbf34847db5a401f8`.

Read with [agent-DX plan](../docs/plans/agent-dx.md) DX6 and §§3–4,
[design](../docs/design.md) §17.147, [research/11](11-agent-dx.md) §§5–6,
[stability policy](../STABILITY.md), and the [draft procedure](../docs/release.md).
The evaluation pilot in research/11 §5 is separate: release qualification does
not authorize paid model runs or expand the owner's $10 Anthropic budget.

## 1. Recommendation and owner decisions

**Use a small hand-written release workflow and installer scripts for DX6.**
Keep one reviewed target/artifact manifest driving archives, installer selection,
Homebrew and npm packaging. Do not implement two release engines. Reconsider
cargo-dist when its installer verification and npm packaging match Sakura's
requirements; it is capable of building every requested target today.

The official [cargo-dist site](https://axodotdev.github.io/cargo-dist/) and
[changelog](https://axodotdev.github.io/cargo-dist/changelog/) list **0.33.0** as
current stable (release heading dated 2026-09-10). This evaluation also inspected
its **v0.33.0** source templates, rather than assuming the moving documentation
describes every verification path. Pin that exact version for any implementer
experiment; do not bootstrap from a mutable `latest` URL.

The decisive findings are:

- The [v0.33.0 shell template](https://github.com/axodotdev/cargo-dist/blob/v0.33.0/cargo-dist/templates/installer/installer.sh.j2)
  compares a known checksum when the required utility is present, but succeeds
  without verification if `sha256sum` is unavailable. This matters on a fresh
  macOS host. It also accepts an absent checksum. Sakura requires failure before
  installation when verification cannot be completed.
- The [v0.33.0 PowerShell template](https://github.com/axodotdev/cargo-dist/blob/v0.33.0/cargo-dist/templates/installer/installer.ps1.j2)
  downloads and extracts the archive without a checksum comparison. Generating
  checksum sidecars does not make that installer verify them.
- Its [npm installer](https://axodotdev.github.io/cargo-dist/book/installers/npm.html)
  downloads release archives; it does not implement §17.147's packages containing
  platform binaries and verified offline installation. The
  [download implementation](https://github.com/axodotdev/cargo-dist/blob/v0.33.0/cargo-dist/templates/installer/npm/binary-install.js)
  is a separate network/extraction path that would also need review.
- Stock publishing needs adaptation for owner approval at **each** channel. The
  [npm publishing template](https://github.com/axodotdev/cargo-dist/blob/v0.33.0/cargo-dist/templates/ci/github/partials/publish_npm.yml.j2)
  uses a token and `npm publish` without an explicit prerelease dist-tag; enabling
  prerelease publication alone would not enforce our proposed `alpha` channel.

These are source-inspection findings, not results of executing cargo-dist or its
installers. An alternative is cargo-dist for archives plus custom installers,
npm packaging and publication orchestration. That retains useful platform logic
but creates two integration layers for five eventual targets. For this alpha,
the smaller ownership boundary is one narrow implementation. This recommendation
does **not** justify a general release framework or relaxing §17.147.

Apply the owner's answers without reopening them:

- First version `0.1.0-alpha.1`, tag `v0.1.0-alpha.1`.
- Public repository `icantcodefyi/sakuralang`, under the owner's personal account.
- Public package name `sakuralang` on npm; crates.io is deferred for the alpha
  (§17.157). Homebrew repository
  `icantcodefyi/homebrew-sakuralang`, formula `sakuralang`, executable `sakura`.
  These are desired names, not evidence that registry ownership is secured.
- A ships first; B only after A ships; C only after B ships. Use GNU Linux targets,
  superseding §17.147's earlier *proposed* musl matrix.
- No tag, release, registry package, tap push, account or GitHub App installation
  is authorized by this research. Signing and channel credentials remain open.

## 2. Repository fit and packaging blockers

The Cargo workspace is [compiler/Cargo.toml](../compiler/Cargo.toml), not the
repository root. It currently has version `0.0.0`, `publish = false`, and path-only
internal dependencies. [sakura_cli](../compiler/crates/sakura_cli/Cargo.toml)
defines the `sakura` binary. Packaging names need not rename every internal crate.
Release archives should use `sakuralang-<version>-<target>` and contain `sakura`
(or `sakura.exe`), notices and installation/runtime instructions.

A Cargo binary built from the complete repository already embeds:

- std sources through [analysis/build.rs](../compiler/crates/sakura_analysis/build.rs)
  and `std_sources()`;
- the JS runtime through [codegen_js/src/lib.rs](../compiler/crates/sakura_codegen_js/src/lib.rs);
- `docs/llms.txt` and the skill tree through
  [CLI main.rs](../compiler/crates/sakura_cli/src/main.rs);
- the WASM runtime object and Node host adapter when
  [codegen_wasm/build.rs](../compiler/crates/sakura_codegen_wasm/build.rs) succeeds.
  That script substitutes an empty object on failure: install the pinned
  toolchain's `wasm32-unknown-unknown` target and explicitly test backend presence.

Copying the verified native executable is therefore viable. Test it after
extraction on a host without the checkout; no lookup of repository std/runtime/
skills may be required. A Sakura JS build emits its runtime beside generated
modules; retain that output directory when distributing the compiled program.

**Node 26 is the current repository qualification major.** `sakura run` and
`sakura test` execute Node for JS; the current WASM host also uses Node.
`SAKURA_NODE` can select its executable, otherwise `node` must be on PATH
([testing.rs](../compiler/crates/sakura_cli/src/testing.rs)). Node is not bundled.
Ordinary `check`, `build`, `doc` and MCP startup do not need Node; MCP tools that
execute programs do. Document missing-Node behavior and do not promise older
versions work. Solver installation for `verify` is separate; installers must not
silently fetch or bundle solvers.

### crates.io is a separate source-packaging problem

**Round-11 disposition (§17.157):** crates.io is deferred for `v0.1.0-alpha.1`.
Alpha channels are GitHub archives with the installer, Homebrew tap and npm.
Revisit for the first beta, or earlier on a `cargo install sakuralang` request,
through a new ADR, starting with the generated source-package option below.
The following is the packaging evaluation that informed that decision.

Neither cargo-dist nor a shell workflow makes `cargo publish -p sakura_cli` valid.
Cargo's [dependency rules](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)
require registry-resolvable non-dev dependencies: local paths do not upload
sibling crates. Sakura's `include_str!` and build scripts also refer to embedded
inputs outside individual package roots. Renaming the CLI to `sakuralang` and
changing `publish` solves neither issue.

Request a designer decision before preparing the crates.io artifact:

1. Prefer investigating a mechanically generated, self-contained **source**
   package named `sakuralang`, with internal code and embedded inputs inside its
   package root. Its module/build layout and licensing need an ADR and review;
   Cargo cannot simply publish the current nested path crates as one package.
2. Publishing the internal dependency graph with versioned registry dependencies
   is another option, but conflicts with STABILITY's unpublished internals and
   expands names and release ordering. It needs an explicit policy decision.
3. Defer crates.io if neither is ready. Do not upload a placeholder/name-reservation
   crate or a build script that downloads the compiler. The owner must explicitly
   accept reduced first-release channels before such a release.

Acceptance is an unpacked `.crate` building/installing without this repository,
with every embedded file and matching version/commit metadata. Inspect
`cargo package --list` and run `cargo publish --dry-run --locked` on the approved
package. The [Cargo publication guide](https://doc.rust-lang.org/cargo/reference/publishing.html)
describes package verification, account/token setup and immutable versions.

## 3. cargo-dist versus a hand-written workflow

Cargo-dist's [workspace model](https://axodotdev.github.io/cargo-dist/book/workspaces/structure.html)
can represent this monorepo with a root `dist-workspace.toml` whose member is
`cargo:compiler`. Opt in only `sakura_cli` despite `publish = false`; exclude
`agent_docs`, evaluators and runtime artifacts as independent applications.
A `dist` profile belongs in `compiler/Cargo.toml`. Do not mistake
`github-releases-submodule-path` for a working-directory setting: `compiler/` is
not a Git submodule.

| Concern | cargo-dist 0.33.0 | Hand-written DX6 implementation |
| --- | --- | --- |
| Build/archive matrix | Cargo discovery, target planning and archives; all A/B/C triples supported. | Explicit native runner/target list; locked Cargo builds from `compiler/`. |
| Names | Crate/bin names may differ; formula/npm names can be overridden. Inspect archive/application naming. | Public artifact/package prefix `sakuralang`; executable `sakura`. |
| Shell / PowerShell | Platform detection and prefix controls, with verification gaps above. | Pinned version, explicit prefix, strict hashes, manual extraction; Windows qualified at C. |
| Homebrew | Generates binary formulae with hashes for supported macOS/Linux targets. | Reviewed formula from the shared artifact inventory, Node dependency and smoke test. |
| npm | Renamable archive-downloading wrapper. | Launcher and exact-version platform packages containing binaries; no install-time downloads. |
| Dry run | Local planning/building; dispatch dry-run can upload Actions artifacts only. | Dispatch is artifact-only, without publishing credentials/OIDC; owner tag and channel gates publish. |
| Integrity | Archive checksums, GitHub attestations, platform signing integrations. | Explicit hashes and approved attestations covering all final assets. |
| Maintenance | Upstream owns platform logic; custom overrides require upgrade review. | We own OS detection, archive safety, download errors, hashes and tests. No release without these tests. |

Supporting official guides: [configuration](https://axodotdev.github.io/cargo-dist/book/reference/config.html),
[checksums](https://axodotdev.github.io/cargo-dist/book/artifacts/checksums.html),
[Homebrew](https://axodotdev.github.io/cargo-dist/book/installers/homebrew.html),
[shell](https://axodotdev.github.io/cargo-dist/book/installers/shell.html),
[PowerShell](https://axodotdev.github.io/cargo-dist/book/installers/powershell.html).
Homebrew does not give parallel prerelease/stable channels automatically: making
the alpha the tap's current formula needs an explicit owner choice.

Cargo-dist's [CI customization guide](https://axodotdev.github.io/cargo-dist/book/ci/customizing.html)
provides custom jobs/runners and action SHA overrides. Local `dist plan` inspects
the release graph; `dist build` builds artifacts without publishing. In generated
CI, `dispatch-releases = true` **replaces** tag-push triggering; it does not add both
triggers. Dispatch with tag `dry-run` skips publication, but a real version can
publish. `create-release = false` still uploads to and eventually undrafts an
existing release. Neither setting is an authorization boundary. Hand-editing
generated CI requires opting out of generation/drift checks. Future adoption must
retain owner gates and re-prove artifact-only dry runs after upgrades.

### Matrix and native qualification

| Stage | Target | Initial native host | Gate |
| --- | --- | --- | --- |
| A | `aarch64-apple-darwin` | GitHub Apple Silicon, explicit image such as `macos-14` | Fresh-host full install smoke; record tested macOS floor. |
| A | `x86_64-unknown-linux-gnu` | `ubuntu-22.04`; optional Blacksmith 22.04 after setup | Inspect and test glibc/system-library floor. |
| B | `aarch64-unknown-linux-gnu` | Native ARM Linux, e.g. Blacksmith 22.04 ARM after setup | A shipped; native run/test/install, not just cross-build. |
| C | `x86_64-pc-windows-msvc` | `windows-2022` | B shipped; PowerShell, spaces in paths, process/exit tests. |
| C | `x86_64-apple-darwin` | Explicit Intel image, e.g. `macos-15-intel` | B shipped; actual Intel execution. |

Runner examples are choices to validate during implementation, not support
promises. Recheck availability; avoid `*-latest` for release hosts. GNU is not
musl: reject Alpine/musl cleanly; do not label these archives static. Newer Linux
images can raise the glibc floor. Inspect ELF dependencies and test the claimed
floor; Node 26's own supported hosts also constrain end-to-end execution.
Cargo-dist's cross-build wrappers do not replace native qualification.

## 4. Integrity, attestations and signing

Recommended free baseline, **subject to owner acceptance**: SHA-256 for every
final archive, installer, npm tarball, skill ZIP and metadata file, plus GitHub
artifact attestations on approved publication. Hash after signing/packaging.
An inventory records filenames, version, source SHA, target and size; no digest
can describe its own containing file. Record toolchain/runner and embedded-content
hashes. Require stable metadata/archive ordering; reproducible native builds have
not been proven.

[GitHub attestations](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations)
are available for public repositories on Free. Use job-scoped `id-token: write`,
`attestations: write` and source read access, without a persistent signing key.
Consumers run `gh attestation verify <file> --repo icantcodefyi/sakuralang` and
check expected workflow/ref/commit. Provenance is not proof of code correctness
or maintainer review.

Cargo-dist's [attestation guide](https://axodotdev.github.io/cargo-dist/book/supplychain-security/attestations/github.html)
supports phase/filter selection. Default local-build attestations conflict with
**no public side effects during dry runs**: public attestations enter a
transparency log. Sakura dry runs should skip public signing/attestations as
well as releases, registries and tap writes. Attest all approved final bytes,
including globally assembled assets. A separate promotion job must record source
run/digests, not imply that compilation happened in that job.

Other free choices, not prerequisites for this draft:

- [Minisign](https://jedisct1.github.io/minisign/): owner generates/backs up a
  signing key, distributes the public key through a trusted channel and signs
  the checksum manifest. Offline verification is useful; key custody, password,
  rotation and revocation are extra responsibilities. No CA fee.
- [Cosign keyless](https://docs.sigstore.dev/cosign/signing/signing_with_blobs/):
  GitHub OIDC signs blobs with a saved verification bundle and no persistent
  private key. Constrain certificate identity **and** issuer during
  [verification](https://docs.sigstore.dev/cosign/verifying/verify/).
  Public signing exposes identity/metadata to transparency services. It overlaps
  GitHub attestations; add both only for an identified consumer requirement.
- Neither replaces Apple Developer ID/notarization or Windows Authenticode.
  Cargo-dist's [Windows signing](https://axodotdev.github.io/cargo-dist/book/supplychain-security/signing/windows.html)
  integrations include Azure in 0.33.0, but platform certificates/services need
  separate owner setup and are not presumed free. State unsigned/not-notarized
  status accurately; do not instruct users to disable OS security globally.

Installers must reject missing/empty/mismatched hashes and absent verification
tools. Shell can use `sha256sum` or macOS `shasum -a 256`; PowerShell can use
`Get-FileHash -Algorithm SHA256`. Authenticate the inventory separately when
publisher authenticity is required: an archive and hash from one server detect
corruption, not independent publisher identity. Provide versioned plain download/
verify/extract, a noninteractive prefix, bounded retries/timeouts, safe extraction
and unsupported-target errors. Test corrupt/truncated input, missing utilities,
spaces in paths, download failure, repeat installs and partial-failure cleanup.

## 5. Embedded skill and version stamping

[Plan §4](../docs/plans/agent-dx.md) separates content generation from build
provenance. Preserve that decision:

1. Set workspace version `0.1.0-alpha.1` in the release-preparation commit, update
   the lockfile with Cargo, regenerate with `agent_docs` and commit outputs before
   qualification. The current distribution manifest's `compiler_version` derives
   from the workspace and is `0.0.0` at this baseline. Archive renaming is not a
   version bump.
2. Keep canonical skill, website mirror and canonical ZIP byte-equal under the
   existing harness. Frontmatter placeholders are intentional; never put HEAD in
   committed generator outputs.
3. For every clean native build set `SAKURA_GIT_COMMIT` to the checked-out source's
   full `git rev-parse HEAD` before Cargo runs. For publication it must equal the
   peeled tag commit; reject `unknown`/mismatches. The CLI fills the installed
   skill from this value and `CARGO_PKG_VERSION`.
4. Attach the same commit's canonical skill ZIP/content manifest to the release.
   Add a **separate release provenance record** binding hashes to version/SHA;
   do not change the content manifest schema or replace the ZIP with differently
   stamped bytes under the same identity. Compare `setup --apply` output after
   the two documented frontmatter substitutions.
5. A future registry source package needs reviewed provenance inside its package
   so rebuilds without `.git` identify upstream source, not the installer's
   unrelated working directory.

The embedded skill needs no network. DX2 client trust/approval smokes remain
incomplete in plan §4; release scripts must not silently trust projects or approve
MCP servers. Human client acceptance remains separate.

## 6. Blacksmith compatibility and cost boundary

Current [runner documentation](https://docs.blacksmith.sh/blacksmith-runners/overview)
lists Linux x64/ARM64, **Apple Silicon M4 macOS**, and Windows x64 (public beta).
Relevant labels include `blacksmith-4vcpu-ubuntu-2204`,
`blacksmith-4vcpu-ubuntu-2204-arm` and `blacksmith-6vcpu-macos-15`.
The quickstart uses `blacksmith-4vcpu-ubuntu-2404`; select the build image according
to ABI qualification. No Intel macOS runner is listed, so macOS support does not
complete matrix C.

The [pricing page](https://www.blacksmith.sh/pricing) advertises **3,000 free
minutes/month** and an application-based OSS programme requiring an actively
maintained public repo, permissive license and clear community usage. Public
visibility is **not documented as unlimited free usage**. Do not assume Sakura
is accepted or that allowances are separate per OS/size; the owner must confirm
limits and any OSS award. “2x faster” is a vendor claim, not a Sakura measurement.

The owner must install/configure the [GitHub App](https://docs.blacksmith.sh/blacksmith-administration/github-app)
for this repo using the [quickstart](https://docs.blacksmith.sh/introduction/quickstart).
It is not installed yet per the owner. Keep GitHub runners operational until a
test job and cost check succeed. Then switch **Linux** jobs as requested; macOS
migration is optional and needs its own cost/host check. Do not enable paid
add-ons or use the agent-evaluation API budget for CI.

Hand-written CI selects labels through `runs-on`. Cargo-dist can instead map
`github-custom-runners`; explicitly set `runner` and matching Rust `host` for
third-party runners. Keep caching/toolchain setup correct for `compiler/`.
Test OIDC/provenance across any runner boundary. Keep publication on GitHub-hosted
runners initially: [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/)
supports GitHub-hosted Actions, not self-hosted/third-party runners such as
Blacksmith. Compilation and publication need not use the same runner provider.

## 7. Concrete implementation handoff

Each step ends at a reviewable artifact, never implicit publication.

1. **Designer + owner:** ADR 0003 records the tooling decision (Proposed until
   owner acceptance); §17.157 adopts GNU A/B/C and defers crates.io for the alpha.
   Owner acceptance, signing and npm companion names remain owner gates. Preserve
   STABILITY promises.
2. **Opus:** prepare tag-triggered builds and artifact-only dry-run dispatch.
   Reject unsupported version inputs, resolve source SHA, check tag/version,
   install pinned Rust plus WASM target and build from `compiler/` using
   `cargo build --locked --release -p sakura_cli --bin sakura --target <triple>`.
   Bound job times; pass the source stamp before each build.
3. **Sol:** one machine-readable matrix/artifact inventory, public names,
   licenses/third-party notices, Node instructions and canonical skill ZIP assets.
   Enable A only; use this inventory for every packaging consumer.
4. **Opus:** strict shell installer and manual instructions, with negative tests
   from §4. Prepare PowerShell parity for C, without advertising Windows yet.
   No updater, implicit sudo, global config writes or CPU/ABI fallback.
5. **Sol:** reviewed Homebrew formula and npm wrapper/platform tarballs. Formula
   installs `sakura`, checks hashes, declares tested Node dependency and tests a
   scaffold. npm uses `bin`, exact-version optional platform dependencies,
   `os`/`cpu` and GNU libc selection. Preserve arguments/stdio/exit codes and
   explain missing optional packages. Proposed companion pattern:
   `sakuralang-<os>-<arch>-gnu` for Linux; `sakuralang-<os>-<arch>` elsewhere.
   Owner confirms availability before publication. Test with install scripts
   disabled and a preseeded offline cache/local tarball set. Offline never means
   fetching uncached packages. See [npm fields](https://docs.npmjs.com/cli/v11/configuring-npm/package-json/).
6. **Opus + Sol:** on each fresh native A host, extract → version → scaffold →
   check → test → build → bounded MCP initialize/list/`compiler_info` → local
   `setup --apply`. Execute generated JS, prove std/runtime relocatability, backend
   presence, bundle stamps and missing-Node behavior. Run compiler gates,
   generated-doc check and installer/package tests. No external model calls or
   client trust changes.
7. **Opus:** dry-run artifacts and summary of hashes, commit, platforms, gates and
   blockers. Prove publishing/OIDC jobs cannot run in dry mode. No secrets on PRs
   or release builds through `pull_request_target`. Pin used Actions to reviewed
   full SHAs before first release.
8. **Owner + Opus:** protected, separate GitHub/attestation, Homebrew and npm
   publication gates. crates.io is deferred for the alpha (§17.157). Consume the
   exact approved run/commit artifacts; recheck digests. Require every active target. Attest final approved bytes;
   do not rebuild unnoticed during promotion.
9. **Owner:** execute [docs/release.md](../docs/release.md). Tag only the qualified
   commit, approve each channel separately, and requalify the final inventory if
   a tag workflow rebuilds. Never overwrite published bytes. B/C follow shipment
   of their predecessors and native acceptance.

## 8. Evidence boundaries

This task inspected repository source, official docs and pinned cargo-dist
templates. It did not install cargo-dist, execute release workflows, build a
release matrix, benchmark Blacksmith, reserve names or test credentials. Platform
floors, packaging architecture, account entitlement and signing remain gates.
There is no new supported release or stable API promise. Future commands in the
draft procedure are not claims of currently working distribution channels.
