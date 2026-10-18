# Releasing Sakura Lang — draft owner procedure

**Draft, 2026-10-18.** DX6 implements the workflow
[`.github/workflows/release.yml`](../.github/workflows/release.yml), the installer
[`tools/install/install.sh`](../tools/install/install.sh), the packaging script
[`tools/release/release.py`](../tools/release/release.py) and the reviewed channel files in
[`packaging/`](../packaging/) (§3). No channel has been published. A publishing command here is
not authorization to run it.
See [release research](../research/13-release.md), [agent-DX plan](plans/agent-dx.md)
DX6 and owner answers in §3, [design](design.md) §§17.147, 17.157 and
[STABILITY](../STABILITY.md).

Owner: **@icantcodefyi**. First tag: `v0.1.0-alpha.1`. Public repository:
`icantcodefyi/sakuralang`. Package/formula name: `sakuralang`; executable: `sakura`.
Alpha channels: GitHub archives with the installer, Homebrew tap and npm.
crates.io is deferred (§17.157). Publish matrix A only: `aarch64-apple-darwin` and
`x86_64-unknown-linux-gnu`. Linux uses GNU libc; Alpine/musl is unsupported.
B adds Linux ARM64 after A ships; C adds Windows x64 and Intel macOS after B
ships. These are decisions, not proof of registry ownership or supported hosts.

## 1. One-time owner setup

The owner performs account, credential, environment and App setup. Preparation
and dry runs need no publication credentials.

### GitHub and approvals

In repository **Settings → Environments**, create proposed publication
environments `release-github`, `release-homebrew` and `release-npm`.
Require **@icantcodefyi** as reviewer, disable administrator bypass and allow
release tags matching `v*`. Only then set the repository **variable**
`RELEASE_ENVIRONMENTS_PROTECTED` to `true` (Settings → Secrets and variables → Actions →
Variables): a tag-triggered run fails without it, because GitHub silently creates an
unprotected environment when a job names one that does not exist. Workflow checks
must additionally validate the exact version/approved SHA. Every publishing job must reference its environment:
creating environments alone protects nothing. Scope secrets to those environments.

For this owner-only repository, leave **Prevent self-review off** if the owner
also triggers the run; otherwise the sole publisher cannot approve it. An explicit
approval click is still required. Independent approval needs a second eligible
reviewer before enabling that option. GitHub documents these public-repository
controls in [environment setup](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments)
and [protection rules](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments).

Protect release tags against changes/deletion and limit their creation. Before
first release, pin every workflow action to a reviewed full commit SHA, as
[CONTRIBUTING](../CONTRIBUTING.md) requires. GitHub creates `GITHUB_TOKEN` for the
job; the owner need not generate it. Only the approved release job needs
`contents: write`. Attestations additionally need `id-token: write` and
`attestations: write`. Build/PR jobs must not inherit these publishing permissions.

### crates.io — deferred (§17.157)

crates.io is deferred for `v0.1.0-alpha.1`; no account/token setup or publication
is required for this channel. The current CLI has unpublished dependencies and
embedded inputs outside crate roots. Revisit for the first beta, or earlier on a
`cargo install sakuralang` request, through a new source-packaging ADR. Do not
publish internal crates, a placeholder or a binary-downloading crate. See
[research §2](../research/13-release.md).

### npm

Use/create the owner's [npm account](https://www.npmjs.com/), enable account 2FA,
and check ownership/availability of `sakuralang` and approved platform companion
names. First publication requires an owner interactive login/2FA or approved
token. Future package settings are not a substitute for establishing ownership.

For token-based CI, **profile menu → Access Tokens → Generate New Token** creates
a granular token. Select package write permission, expiry and the narrowest
package set; existing-package selection may not cover initial creation. Store
`NPM_TOKEN` in `release-npm`, passed as `NODE_AUTH_TOKEN` only during publication.
Unattended token publishing needs owner-approved **Bypass 2FA** and compatible
package policy. Keep account 2FA enabled; an OTP is not a reusable CI secret.
Revoke/narrow bootstrap tokens. Follow [current token docs](https://docs.npmjs.com/creating-and-viewing-access-tokens/),
not old classic automation-token advice.

Prefer [trusted publishing](https://docs.npmjs.com/trusted-publishers/) once each
package exists: package **Settings → Trusted Publisher → GitHub Actions**, user
`icantcodefyi`, repository `sakuralang`, exact reviewed workflow filename and
environment `release-npm`. Configure companion packages too. Use a supported npm
CLI and **GitHub-hosted** publish job with `id-token: write`; npm currently excludes
self-hosted/third-party runners such as Blacksmith. This removes a long-lived
publication token, not the owner approval gate.

### Homebrew

Create public repository **`icantcodefyi/homebrew-sakuralang`** with a README.
The reviewed `Formula/sakuralang.rb` will install `sakura` from immutable archives.
The user command will be `brew install icantcodefyi/sakuralang/sakuralang`.
This is a custom tap, not Homebrew core; see [tap maintenance](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap).

For automation create a fine-grained PAT through GitHub **Settings → Developer
settings → Personal access tokens → Fine-grained tokens**. Choose owner
`icantcodefyi`, only repository `homebrew-sakuralang`, **Contents: read and write**
and an expiry. Store `HOMEBREW_TAP_TOKEN` in `release-homebrew`. The main repo's
`GITHUB_TOKEN` does not grant tap-repo writes. Manual owner Git credentials or a
reviewed GitHub App scoped to the tap are alternatives; a broad classic `repo`
token is unnecessary. See [GitHub token setup](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens).

### Signing and Blacksmith

Owner signing choice remains open. Recommended first policy: SHA-256 and GitHub
artifact attestations, with explicit unsigned/not-notarized platform status.
Attestations need permissions, not a purchased certificate or persistent signing
key. Optional minisign requires owner key generation/backup and public-key
distribution; cosign keyless uses OIDC/public transparency records. See
[research §4](../research/13-release.md). Neither implies Apple notarization or
Windows Authenticode.

Blacksmith is optional. Owner installs its [GitHub App](https://docs.blacksmith.sh/blacksmith-administration/github-app)
for the repo and confirms allowance/OSS acceptance and spend limits before a
runner switch. [Pricing](https://www.blacksmith.sh/pricing) advertises 3,000 free
minutes/month plus an OSS application, not automatic unlimited public-repo CI.
Start with Linux; retain GitHub-hosted publication. Current
[runner docs](https://docs.blacksmith.sh/blacksmith-runners/overview) include
Apple Silicon macOS, but no Intel macOS offering. No billing or paid add-ons are
authorized by the owner's proposal.

## 2. Prepare the candidate

1. Resolve [designer requests](../compiler/REQUESTS.md): tooling/packaging
   architecture, channel scope, signing and companion npm names. Record explicit
   owner acceptance of any deferred channel.
2. Set workspace version `0.1.0-alpha.1`; update lockfile and generated content in
   the preparation commit. Update CHANGELOG for user-visible changes. The owner
   reviews STABILITY at the tag; do not invent alpha API guarantees.
3. Run `cargo run -p agent_docs --locked -- --check` from `compiler/`. Canonical
   skill, website mirror/ZIP and manifest must agree. Keep frontmatter placeholders;
   separate release provenance binds the canonical ZIP hash to version/commit.
4. Inspect archive content, embedded runtime/std, notices and channel manifests.
   Native binaries need no checkout. JS execution needs **Node 26**; the current
   WASM host does too. State the prerequisite; shell installation must not
   invisibly install Node. Homebrew declares its tested Node dependency; npm
   packaging states the tested Node range.
5. Commit preparation and record the full candidate SHA. All native builds set
   `SAKURA_GIT_COMMIT` to it. Version, provenance and installed skill must agree.
   Reject `0.0.0`, `unknown`, missing/mismatched SHA or filename-only version bumps.

Require the current compiler gates from `compiler/`:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -p agent_docs --locked -- --check
```

Workflow changes additionally require YAML/actionlint and meaningful installer/
package tests. If preparation changes website sources or generated website assets,
run from `website/`: `bun install --frozen-lockfile`, `bun run fmt:check`,
`bun run lint`, `bun run typecheck`, `bun run build`. Record exact outcomes and
limitations. Source tests alone do not qualify released binaries.

## 3. Dry run — no tag or publication

Run **Actions → Release → Run workflow** on the candidate ref, or
`gh workflow run release.yml --repo icantcodefyi/sakuralang --ref master`. The dispatch
has no inputs: it is always a dry run. It builds every target in
[`packaging/targets.json`](../packaging/targets.json) with `SAKURA_GIT_COMMIT` set to
the commit, smoke-tests each extracted archive on its native runner (`release.py smoke`:
version and commit, `new`/`check`/`test`/`build`, generated JS on Node 26, a WASM run,
a bounded MCP session with `compiler_info`, `setup --apply` twice against the canonical
skill, no writes to `HOME`, the missing-Node error), runs the installer tests, assembles the
set and installs it from the assembled directory with `install.sh` and offline npm.
The publish jobs are skipped: they require a tag push. Download the results with
`gh run download <run-id> --repo icantcodefyi/sakuralang -n release-<version>`
(`archive-<target>` holds each target's archive alone). The run summary lists the
SHA-256 inventory and each target's host floor (glibc or macOS minimum).

Download from that **specific run ID and SHA**. Review the full active matrix's
archives, notices/Node requirements, shell/manual installer, canonical skill ZIP/
manifest, release provenance, SHA-256 inventory, formula and npm tarballs.
C later adds Windows ZIP/PowerShell.
Do not publish only the successful subset of a failing matrix.

On each fresh native host:

1. Verify hashes before extraction; inspect archive paths/contents. Test manual
   extraction and explicit script prefix, including a path with spaces.
2. Run `sakura --version`, then `sakura new cli demo`. In `demo` run `sakura check`,
   `sakura test` and `sakura build`. Execute generated JS using Node 26.
3. Use a bounded stdio MCP smoke: initialize, list tools, call `compiler_info`,
   inspect version/features, close stdin and confirm clean exit. A waiting
   server process alone is not a smoke result.
4. Run `sakura setup --client all --apply` in the scratch project; verify local
   files/version/commit, repeat for idempotence and confirm no global writes.
   Do not trust/approve actual clients automatically; plan §4 records remaining
   human client acceptance.
5. Without Node, check/build/doc and MCP startup should work; executing programs
   should give the documented actionable error. Test WASM backend presence if
   advertised, not merely successful compilation with an empty fallback.
6. Reject corrupted archives, absent hashes/verifiers, unsupported CPU/libc and
   failed downloads before installation. Test repeat installs and cleanup.
   Save logs, OS/ABI floor and tool versions.

Without the script, a user verifies and installs by hand (macOS shown; on Linux use
`sha256sum -c`):

```sh
v=0.1.0-alpha.1 t=aarch64-apple-darwin
base=https://github.com/icantcodefyi/sakuralang/releases/download/v$v
curl -fLO "$base/SHA256SUMS" && curl -fLO "$base/sakuralang-$v-$t.tar.gz"
grep " sakuralang-$v-$t.tar.gz\$" SHA256SUMS | shasum -a 256 -c
tar -tzf "sakuralang-$v-$t.tar.gz"   # inspect before extracting
tar -xzf "sakuralang-$v-$t.tar.gz" && install -m 755 "sakuralang-$v-$t/sakura" ~/.local/bin/
```

The scripted path is the same steps: download `install.sh` from the release, read it, run
`sh install.sh --prefix ~/.local`. `sh tools/install/test.sh` covers its refusals.

Inspect npm's `npm pack --dry-run` output and actual tarballs. Install with scripts
disabled/network denied using a preseeded cache or local wrapper/platform
tarballs. An empty cache cannot prove offline install. Confirm no hidden binary
download. Test the Homebrew formula against staged artifacts before a tap push.

## 4. Owner approval and tag

Present candidate SHA, version, run ID, final digest inventory, platforms/floors,
checks, channel artifacts, signing and limitations. Record explicit approval for
the tag and channels. Changed source/binary/packaging bytes invalidate the
corresponding approval.

**Only afterward**, the owner creates/pushes an annotated tag on that commit.
This is a future command template, not an instruction for the research agent:

```sh
git tag -a v0.1.0-alpha.1 <approved-full-commit-sha> -m 'Sakura Lang 0.1.0-alpha.1'
git push origin refs/tags/v0.1.0-alpha.1
```

Check each step separately. Never move/force-push a published tag. It must peel to
the approved SHA, not a newer master commit. Tag-triggered preparation must still
pause at channel gates. If it rebuilds, compare/qualify final hashes again before
publication; same-source builds are not assumed byte-identical.

## 5. Publish each approved channel

The tag run of `release.yml` rebuilds and re-smoke-tests, then pauses at three jobs:
approving `release-github` attests every file and creates the prerelease
(`--prerelease --latest=false`); `release-homebrew` commits the filled formula with
`HOMEBREW_TAP_TOKEN`; `release-npm` publishes the platform packages, then the wrapper, with
`--tag alpha` (trusted publishing, else `NPM_TOKEN`). The latter two wait for the first.
Native builds are not proven byte-identical, so review the tag run's own `SHA256SUMS`
and summary before approving, not only the dry run's.

1. **GitHub first.** Approve `release-github` for the tagged run/inventory. Create
   prerelease `v0.1.0-alpha.1`, explicitly not stable/latest. Publish tested bytes,
   final hashes, notices, skill assets, provenance and approved signatures/
   attestations. Notes state alpha limits, tested platforms/floors, Node 26,
   solver scope and signing/notarization status. Verify downloaded assets. Run
   `gh attestation verify <downloaded-file> --repo icantcodefyi/sakuralang` and
   inspect expected source/workflow identity.
2. **Homebrew.** Once asset URLs work, review formula version/targets/hashes/Node
   dependency. Explicitly accept the alpha as the tap's current formula, then
   approve `release-homebrew` or push it personally. Exercise installation and
   formula tests on supported hosts. A mutable formula name is not version
   pinning; immutable archive downloads remain the pinned route.
3. **npm.** Approve `release-npm`. Publish platform packages first, verify their
   availability/digests, then `sakuralang` with exact matching optional dependency
   versions and **`--tag alpha`**, not `latest`. Owner handles initial ownership/
   2FA. Test `npm install -g sakuralang@0.1.0-alpha.1` and the binary on clean
   hosts; upload success does not prove platform selection works.

A later channel failure does not authorize mutating earlier assets. Record
partial rollout and seek approval for remaining publication. Resume identical
reviewed artifacts only where the channel allows; changed published bytes need a
new version, with owner-approved deprecation/yank guidance if necessary.

## 6. Close-out

Record tag/SHA, run IDs, hashes, URLs, tap commit, package versions, verification
and credential expiry/rotation reminders without secrets. Retain qualification
evidence/provenance. State unsigned artifacts, deferred channels and incomplete
client acceptance honestly. Only after A is published and verified begin B;
repeat this procedure for later matrix expansions.

This draft was researched and written with GPT-6-Astra. It claims no existing
release, credential, account, App installation or native release test.
