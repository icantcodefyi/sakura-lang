# ADR 0003: Hand-written release workflow and installers over cargo-dist

- Date: 2026-10-18
- Status: Proposed
- Decision owner: @icantcodefyi
- Design entry: [design §17.157](../design.md); evidence: [research/13](../../research/13-release.md)

## Context

DX6 ([design §17.147](../design.md), [plan](../plans/agent-dx.md)) prepares the first release,
`v0.1.0-alpha.1`, for the owner's matrix A (`aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`),
then B (`aarch64-unknown-linux-gnu`) and C (`x86_64-pc-windows-msvc`, `x86_64-apple-darwin`).
§17.147 requires: a pinned version, checksum verification that fails rather than skips, platform
detection with a clean failure on unsupported targets, a non-interactive prefix, a plain
download-and-extract path, npm packages that carry the binary and install offline, artifact-only
dry runs, and the owner's explicit approval at every publishing channel.

research/13 inspected cargo-dist 0.33.0's pinned templates. Its shell installer succeeds without
verification when `sha256sum` is missing and accepts an absent checksum; its PowerShell installer
compares no checksum; its npm wrapper downloads archives at install time; its stock publishing has
no per-channel owner gate and no prerelease dist-tag. Each is fixable only by owning the generated
files, which forfeits the generator.

## Decision

Release tooling is hand-written and small, with one source of truth:

- `packaging/targets.json` is the only inventory of targets (triple, runner, npm fields).
  `.github/workflows/release.yml`, `tools/release/release.py` and the Homebrew and npm files read
  it; adding a matrix stage is one entry there plus its channel stanzas.
- `.github/workflows/release.yml`: `workflow_dispatch` is always an artifact-only dry run; a `v*`
  tag equal to the workspace version builds, smokes and assembles, then owner-gated environments
  (`release-github`, `release-homebrew`, `release-npm`) publish, each separately approved; a tag
  run fails unless the repository variable `RELEASE_ENVIRONMENTS_PROTECTED` is `true`. Actions are
  pinned to full SHAs; publishing jobs run on GitHub-hosted runners.
- `tools/release/release.py`: deterministic archives, native smoke on the extracted archive,
  assembly with `SHA256SUMS` and a separate provenance record `release.json`.
- `tools/install/install.sh` (POSIX sh; PowerShell at stage C): pinned version, strict SHA-256
  with `sha256sum` or `shasum` and failure without either, safe extraction, no sudo, no profile
  edits, musl and unknown CPUs refused.
- Channels at the alpha: GitHub release archives with the installer, the Homebrew tap
  `icantcodefyi/homebrew-sakuralang` (formula `sakuralang`, binary `sakura`), npm `sakuralang`
  with exact-version platform packages. crates.io is deferred (§17.157 (3)).

## Consequences

- The project owns OS detection, archive safety, download errors, hashing and their tests
  (`tools/install/test.sh`); no release ships without those tests green.
- Platform logic is written once per stage, by hand; a stage is added only after the previous one
  has shipped and been accepted on a fresh host.
- Dry runs produce no public side effect (no release, no registry write, no attestation).
- Integrity is SHA-256 over every asset plus GitHub artifact attestations on approved
  publication; signing keys and notarization are the owner's later choice.

## Alternatives

- **cargo-dist 0.33.0 as built**: rejected for the verification gaps and the npm model above;
  reconsidered when a release verifies unconditionally in both installers and its npm packaging
  carries binaries.
- **cargo-dist for archives, custom installers and publishing on top**: two integration layers
  for five eventual targets; rejected for the alpha.
- **Publishing from CI on merge, a mutable `latest`, Homebrew core**: rejected in §17.147.

## Links

- Implementation and deviations: [plan `agent-dx.md` §4 DX6](../plans/agent-dx.md)
- Owner procedure: [docs/release.md](../release.md)
