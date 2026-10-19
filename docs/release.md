# Releasing Sakura Lang — draft owner procedure

**Draft, 2026-10-18.** DX6 implements the workflow
[`.github/workflows/release.yml`](../.github/workflows/release.yml), the installer
[`tools/install/install.sh`](../tools/install/install.sh), the packaging script
[`tools/release/release.py`](../tools/release/release.py) and the reviewed channel files in
[`packaging/`](../packaging/) (§4). No channel has been published. A publishing command here is
not authorization to run it.
See [release research](../research/13-release.md), [agent-DX plan](plans/agent-dx.md)
DX6 and owner answers in §4, [design](design.md) §§17.147, 17.157 and
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

Automation uses a deploy key: an Ed25519 key added to `homebrew-sakuralang` with write
access (and to no other repository), its private half stored as `HOMEBREW_TAP_DEPLOY_KEY`
in `release-homebrew`. The main repo's `GITHUB_TOKEN` does not grant tap-repo writes; a
deploy key is narrower than a personal access token and does not expire with one. To
rotate it, generate a new key, replace the deploy key and the secret, then delete the old
key (see [deploy keys](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/managing-deploy-keys)).

### VS Code Marketplace and Open VSX

The extension in [`editors/vscode/`](../editors/vscode/) (DX7) uses publisher **`sakuralang`**
on both. CI builds, tests and uploads a `.vsix` artifact; it never publishes.

- **VS Code Marketplace:** sign in at the [publisher management page](https://marketplace.visualstudio.com/manage),
  create publisher ID `sakuralang`, and create an Azure DevOps personal access token with the
  **Marketplace (Manage)** scope, all accessible organizations and a short expiry
  ([publishing guide](https://code.visualstudio.com/api/working-with-extensions/publishing-extension)).
- **Open VSX:** create an Eclipse account, sign the publisher agreement, create an access token
  at [open-vsx.org](https://open-vsx.org), then the namespace with
  `npx ovsx create-namespace sakuralang -p <token>`
  ([publishing guide](https://github.com/eclipse/openvsx/wiki/Publishing-Extensions)).

Keep both tokens out of the repository; no workflow holds them.

### macOS signing and notarization

New tag releases require Developer ID signing and Apple notarization before assembly
computes `SHA256SUMS`, generates the Homebrew formula or packs npm companions. The
protected `sign-darwin` job runs on public GitHub-hosted `macos-14` in `release-github`.
It signs only with **Developer ID Application: Aniruddh Dubge (P62B65A837)**, scoped to
its temporary keychain. It never selects another identity from the runner.

The environment already holds `MACOS_CERT_P12_BASE64`, `MACOS_CERT_PASSWORD`,
`APPLE_API_KEY_P8`, `APPLE_API_KEY_ID` and `APPLE_API_ISSUER_ID`. Only the signing step
receives them. It writes the certificate and API key under `$RUNNER_TEMP`, masks
credential values and the generated keychain password, and removes the temporary
keychain and files on success or failure. An `always()` job step repeats cleanup
after cancellation. Credential-bearing tool diagnostics are never uploaded or printed.

Each Darwin `sakura` is signed with `codesign --force --options runtime --timestamp`,
submitted in a temporary ZIP with `xcrun notarytool submit --wait`, and required to
receive `Accepted`. Strict signature verification checks team `P62B65A837`; online
verification uses `codesign -R notarized --check-notarization`. Per
[Apple's product verification guidance](https://developer.apple.com/forums/thread/130560),
`spctl -a -t install` applies to installer packages. Sakura ships a bare Mach-O,
so that installer assessment is not applicable. A bare executable cannot carry a
stapled ticket; Gatekeeper may need network access to check Apple's ticket.

The signed binary replaces the unsigned Darwin archive before assembly. Homebrew
references that archive's checksum; npm receives its exact binary bytes. Native
install smokes verify both installed copies again. Existing immutable releases are
not retroactively signed. Linux binaries remain unsigned; GitHub attestations and
SHA-256 verification remain separate checks.

To test credentials without a release or tag, dispatch on `main`:

```sh
gh workflow run release.yml --repo icantcodefyi/sakuralang --ref main -f sign_test=true
```

This runs only `sign-darwin`, builds fresh Darwin binaries from that commit, signs,
notarizes, verifies, archives and smokes them, then uploads `sign-test-darwin`.
It uses the same protected environment and may wait for the owner's approval.
Agents must report pending approval, never approve their own signing test.
No publication job can run on any dispatch. An ordinary dispatch leaves `sign_test`
false, needs no Apple secrets or protected-environment approval, and explicitly
records that signing and notarization were skipped.

See [Apple's notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)
and [GitHub's temporary-keychain guidance](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications).

### Runner providers

Blacksmith is optional. Owner installs its [GitHub App](https://docs.blacksmith.sh/blacksmith-administration/github-app)
for the repo and confirms allowance/OSS acceptance and spend limits before a
runner switch. [Pricing](https://www.blacksmith.sh/pricing) advertises 3,000 free
minutes/month plus an OSS application, not automatic unlimited public-repo CI.
Start with Linux; retain GitHub-hosted publication. Current
[runner docs](https://docs.blacksmith.sh/blacksmith-runners/overview) include
Apple Silicon macOS, but no Intel macOS offering. No billing or paid add-ons are
authorized by the owner's proposal.

## 2. Quiet window and bless

Every baseline release has one quiet window (design §17.153, §17.228 (2), §17.238 (4)): the
M4 exit for `v0.1.0`, then `v0.2.0` and `v1.0.0-rc.1` (§17.225 (5)). It measures **the commit
the gate signs**, never `main`'s head, in a worktree of its own, so the blessed baseline's
`meta.commit` is that commit and later docs commits move nothing. For M4 the commit is
`dee11301`. The owner runs every command below in one terminal on the reference machine
(host id `d88970189598`, 12 CPUs, Apple M4 Pro); nothing else runs on the machine meanwhile.
Astra reads the files afterwards and writes the report; agents never bless.

**Before the window**

1. Pause every agent (Claude Code, Codex, T3 threads) and close builds and browsers.
   `uptime` must show a one-minute load under 3.0 before each run starts.
2. A clean worktree at the commit (replace `dee11301` for a later window):
   ```sh
   cd <your sakuralang checkout>
   git fetch origin
   git worktree add ../sakura-window dee11301 --detach
   cd ../sakura-window
   mkdir -p bench/target
   ```
3. Toolchains at the pins in [`bench/toolchains.toml`](../bench/toolchains.toml); each line must
   print the pinned version, or its cells come out `missing` and the envelope `incomplete`:
   ```sh
   node --version; bun --version; rustc --version; go version
   ocamlopt -version; koka --version; wasmtime --version; hyperfine --version
   ```
4. Build once, then let the load settle under 3.0 (`uptime`); the harness samples only the
   timing phase:
   ```sh
   (cd compiler && cargo build --release --locked -p sakura_cli)
   uptime
   ```

**The runs** (in this order; each ends with `[load] quiet` or `[load] not quiet: <reasons>`;
on `not quiet` keep the file, stop, and resume at that run later; completed quiet runs stand.
Resume on the same commit in the same worktree, the same night or the next (design §17.252
(3)); never rerun a cell inside a run)

```sh
python3 research/10-evidence/record_load.py --log bench/target/quiet-load-js.jsonl -- \
  bench/bench run --suite js --stable --json bench/target/quiet-js.json
python3 research/10-evidence/record_load.py --log bench/target/quiet-load-wasm.jsonl -- \
  bench/bench run --suite wasm --stable --json bench/target/quiet-wasm.json
python3 research/10-evidence/record_load.py --log bench/target/quiet-load-native.jsonl -- \
  bench/bench run --suite native --lane matched --stable --json bench/target/quiet-native.json
bench/bench compile --json bench/target/quiet-compile.json
bench/bench envelope bench/target/quiet-wasm.json bench/target/quiet-native.json \
  --json bench/target/envelope.json --md bench/target/envelope.md
```

`run` prints a table of medians and Sakura/hand-JS ratios as it goes. `envelope` exits 0
whatever it reads; its reading line (`met`, `miss`, `incomplete`, or `advisory` when a run
was not quiet) is the gate report's envelope section, read against design §17.109's ceilings.
`compile` reports p95 against the research/10 §6 budgets and §17.195 (1)'s targets.

**Bless** (the owner only; `bless` refuses a run with a failed oracle, from a dirty
`compiler/` or `bench/` tree, or whose `meta.host.quiet` is not `true`):

```sh
bench/bench bless bench/target/quiet-js.json      # -> bench/baselines/<host-id>.json
bench/bench bless bench/target/quiet-wasm.json    # -> bench/baselines/wasm/<host-id>.json
```

The native and compile runs are evidence, not baselines, and are not blessed.

**Evidence files**, committed on `main` by hand (copy them from the worktree into your main
checkout; `D` is the date, `C` the short commit, `H` the host id):

- `bench/baselines/H.json` and `bench/baselines/wasm/H.json`: the two blessed runs.
- `bench/results/D-C-js.json`, `D-C-wasm.json`, `D-C-native.json`, `D-C-compile.json`,
  `D-C-envelope.json`, `D-C-envelope.md`, and the three `D-C-load-*.jsonl` recordings.
- `bench/deltas.md`: the envelope's Markdown and each program's reading against its
  §17.109 ceiling, every miss with its named decision (Astra writes it; you read it).
- The checklist line for the window with your name, the date and `meta.commit`.

```sh
cd <your sakuralang checkout>
git add bench/baselines bench/results bench/deltas.md docs/plans/m4-exit-checklist.md
git commit -m "bench: quiet window at dee11301, blessed JS and WASM baselines"
git push origin HEAD:main
git worktree remove ../sakura-window
```

A code change on a benchmarked path after the window reopens the question for the owner;
anything else does not (§17.228 (2)).

## 3. Prepare the candidate

1. Resolve [designer requests](../compiler/REQUESTS.md): tooling/packaging
   architecture, channel scope, signing and companion npm names. Record explicit
   owner acceptance of any deferred channel.
2. Set workspace version `0.1.0-alpha.1`; update lockfile and generated content in
   the preparation commit. Update CHANGELOG for user-visible changes. The owner
   reviews STABILITY at the tag; do not invent alpha API guarantees.
3. Run `python3 -I tools/release/release.py prepare` from the repository root.
   Fetch full history and release tags first. The gate compares the candidate with
   the highest preceding ancestor release tag. `--previous-tag vX.Y.Z` pins that
   selection, and `--date YYYY-MM-DD` sets the planned UTC release date.
   Patch releases retain the previous tag's exact manifest MSRV. Retain every
   `compiler/deprecations.toml` entry, including removals. Each entry records
   `feature`, its `deprecated_*` warning `code`, `since = { version, date }`,
   exactly one `replacement` or `reason`, and `earliest_removal = { version, date }`.
   Use quoted versions and ISO calendar dates. Removal adds `removed = "X.Y.Z"`
   only at a major bump after both the written version/date thresholds and two
   final minor releases with the warning. Prereleases do not count. A first
   candidate with no earlier tag may bootstrap only at patch zero without removals.
4. Run `cargo run -p agent_docs --locked -- --check` from `compiler/`. Canonical
   skill, website mirror/ZIP and manifest must agree. Keep frontmatter placeholders;
   separate release provenance binds the canonical ZIP hash to version/commit.
5. Inspect archive content, embedded runtime/std, notices and channel manifests.
   Native binaries need no checkout. JS execution needs **Node 26**; the current
   WASM host does too. State the prerequisite; shell installation must not
   invisibly install Node. Homebrew declares its tested Node dependency; npm
   packaging states the tested Node range.
6. Commit preparation and record the full candidate SHA. All native builds set
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

## 4. Dry run — no tag or publication

Run **Actions → Release → Run workflow** on the candidate ref, or
`gh workflow run release.yml --repo icantcodefyi/sakuralang --ref main`. Leave `sign_test` false for the ordinary secret-free dry run. It builds every target in
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

The npm packages ship no lifecycle scripts, so users install with a plain
`npm install --global sakuralang`. `release.py assemble` enforces this: `npm_pack`
fails if a staged `package.json` declares `preinstall`, `install`, `postinstall` or
`prepare`, and still packs with `--ignore-scripts`. The release and published-install
workflows keep `--ignore-scripts` as a guard.

Inspect npm's `npm pack --dry-run` output and actual tarballs. Install with scripts
disabled/network denied using a preseeded cache or local wrapper/platform
tarballs. An empty cache cannot prove offline install. Confirm no hidden binary
download. Test the Homebrew formula against staged artifacts before a tap push.

### Windows packaging checks

The Windows lane builds `x86_64-pc-windows-msvc` with `+crt-static` and archives
`sakura.exe` in a ZIP. The dry run checks the ZIP, installs it with `install.ps1`,
and installs the npm wrapper and companion from local tarballs. CI runs the installer
fixture suite with both PowerShell 5.1 and PowerShell 7. The installer verifies the
matching `SHA256SUMS` entry before extraction. Run the downloaded script with
`powershell -ExecutionPolicy Bypass -File .\install.ps1 -Version <version>`. It uses
`%LOCALAPPDATA%\Programs\sakuralang\bin` by default, and does not edit PATH.

After publication, the Windows `verify-published.yml` row installs from the release URL
and npm. Acceptance includes a fresh project's `fmt --check` after a Git checkout with
`core.autocrlf=true`, a stdio LSP initialize round trip, and the shared MCP/setup checks.
Proof-key storage remains unavailable on Windows; its Unix-only tests stay gated.

## 5. Owner approval and tag

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

## 6. Publish each approved channel

The tag run of `release.yml` rebuilds and re-smoke-tests, then pauses at three jobs:
approving `release-github` attests every file and creates the release;
`release-homebrew` commits the filled formula with `HOMEBREW_TAP_DEPLOY_KEY`; `release-npm`
publishes the platform packages, then the wrapper (trusted publishing, else `NPM_TOKEN`).
The latter two wait for the first. The flags follow the version string (design §17.249): a
plain `X.Y.Z` such as `0.1.0` is created `--latest=true` and publishes to npm `latest`; only a
version with a prerelease identifier (`-alpha.N`, `-rc.N`) is created
`--prerelease --latest=false` and publishes under that identifier's dist-tag (`alpha`, `rc`).
Native builds are not proven byte-identical, so review the tag run's own `SHA256SUMS`
and summary before approving, not only the dry run's.

1. **GitHub first.** Approve `release-github` for the tagged run/inventory. Create
   release `v<version>`, marked latest or prerelease by the version string as above.
   Publish tested bytes, final hashes, notices, skill assets, provenance and approved
   signatures/attestations. Notes call the version a pre-1.0 preview with no stability promise yet
   (RFC 0001, STABILITY.md) and state tested platforms/floors, Node 26,
   solver scope and signing/notarization status. Verify downloaded assets. Run
   `gh attestation verify <downloaded-file> --repo icantcodefyi/sakuralang` and
   inspect expected source/workflow identity.
2. **Homebrew.** Once asset URLs work, review formula version/targets/hashes/Node
   dependency. Explicitly accept the release as the tap's current formula, then
   approve `release-homebrew` or push it personally. Exercise installation and
   formula tests on supported hosts. A mutable formula name is not version
   pinning; immutable archive downloads remain the pinned route.
3. **npm.** Approve `release-npm`. Publish platform packages first, verify their
   availability/digests, then `sakuralang` with exact matching optional dependency
   versions and the version's dist-tag (`latest` for `X.Y.Z`, else `alpha`/`rc`). Owner
   handles initial ownership/2FA. Test `npm install -g sakuralang@<version>` and the
   binary on clean hosts; upload success does not prove platform selection works.

4. **VS Code extension.** After the GitHub release (its users need a `sakura`), in
   `editors/vscode/`: the Marketplace rejects SemVer pre-release suffixes, so set
   `package.json`'s version to a plain `0.1.0` (later alphas `0.1.1`, `0.1.2`, …), then
   `bun install --frozen-lockfile`, `bun run test`, `bun run build`, and
   `bunx vsce package --pre-release --no-dependencies --skip-license` after copying the two
   licence files in (as `bun run package` does). Install the `.vsix` with
   `code --install-extension` and open a project before publishing that same file:
   `bunx vsce publish --pre-release --packagePath sakura-0.1.0.vsix` and
   `bunx ovsx publish sakura-0.1.0.vsix --pre-release -p <token>`. Commit the version change.

A later channel failure does not authorize mutating earlier assets. Record
partial rollout and seek approval for remaining publication. Resume identical
reviewed artifacts only where the channel allows; changed published bytes need a
new version, with owner-approved deprecation/yank guidance if necessary.

## 7. Close-out

Record tag/SHA, run IDs, hashes, URLs, tap commit, package versions, verification
and credential expiry/rotation reminders without secrets. Retain qualification
evidence/provenance. State unsigned artifacts, deferred channels and incomplete
client acceptance honestly. After every channel has published, dispatch
`verify-published.yml` with the version and record its run URL: it installs from the public
channels on fresh GitHub-hosted runners and runs the acceptance list (design §17.227); a red
row is a release defect fixed forward in a new version. Only after A is published and verified
begin B; repeat this procedure for later matrix expansions.

The tag push starts `compat.yml`, which runs the previous release's `tests/semantics`, `tests/ui`
and `examples` under the tagged compiler (design §17.232 (e)); record its run URL. If the
release ships a WASM ABI version that `compiler/runtime/rt/tests/fixtures/abi/` lacks, capture
its module with the published binary as that directory's README says (§17.232 (c)).

This draft was researched and written with GPT-6-Astra. It claims no existing
release, credential, account, App installation or native release test.

Homebrew 6 requires explicit trust for third-party formulae. Before the published-install
smoke uses the documented tap/install command, it runs `brew tap icantcodefyi/sakuralang`
and `brew trust --formula icantcodefyi/sakuralang/sakuralang` on the fresh runner.
See [Homebrew tap trust](https://docs.brew.sh/Tap-Trust).

### Intel Mac packaging and runner retirement

Intel macOS builds, suites, JS/WASM parity and installer/npm/Homebrew smoke run on
GitHub's public `macos-15-intel` runner. The Darwin signing job signs and notarizes
both inventory binaries before assembly computes hashes and stages npm and Homebrew.
Dry runs skip signing. The Intel formula smoke mirrors the assembled archive through a
local `file://` URL with the production checksum, then runs `brew test`.

When GitHub retires `macos-15-intel`, move the Intel packaging and acceptance row to
`macos-15` under Rosetta. Check `arch -x86_64 /usr/bin/true`; if it fails, run
`softwareupdate --install-rosetta --agree-to-license`. Smoke the archive only, using
`install.sh --target x86_64-apple-darwin` to override native CPU selection deliberately.
Record the last green Intel-runner date and mark Intel npm and Homebrew as
"untested since <date>" in the install docs; Rosetta alone does not qualify an x64
Node or Homebrew installation (design §17.227 (2)). No paid runner is required.
