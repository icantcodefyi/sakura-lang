# Road to 1.0 (provisional outline)

**Provisional.** Design §17.168, re-planned by §17.225 and §17.226 on 2026-10-19 after the
owner's decision not to wait for outside programs for a release cycle. No stage carries a date;
every stage boundary is the owner's. STABILITY.md is the policy; this is the order. Weeks below
are estimates of wall time with the agents the machine runs, not promises.

| # | Stage | Tag | Exit criteria | Owner items | Agent-days |
|---|---|---|---|---|---|
| 1 | `v0.1.0-alpha.1` (§17.157; M3 closed) | **done** 2026-10-18 (GitHub prerelease, Homebrew tap) | npm publish still owed | npm token that bypasses 2FA | — |
| 2 | **M4 exit** (§17.166, plan `m3-completion-and-m4.md` §3.8) and `v0.1.0`, the scoped first non-alpha preview | `v0.1.0` | rows 26–29, WS-W1, WS-W2 landed; parity matrix green on the exit commit; §17.109 ceilings read at a quiet window, misses carried; M4-DOCS rechecked; release notes name what is and is not stable | quiet window and bless; gate signature or written delegation; tag and environment approvals | ~12 |
| 3 | **Platforms B then C** (§17.180; rows in §6 below) | ships in `v0.2.0` | each row's acceptance; B accepted on a fresh host before C's packaging starts | P4 on a Raspberry Pi; P8 on Windows 11 and an Intel Mac | ~9 |
| 4 | **M5-core**: native backend behind the experimental flag (`m5-native.md` N0b, N1a, N1b, N2, N5, §17.173 lanes) | `v0.2.0` | parity matrix `{js, wasm, native} × {ubuntu, macos}` green; `--target native` labelled experimental in STABILITY.md; N3, N4 and the §17.177 windows are M5b, after 1.0 (§17.225 (8)) | none | ~28 |
| 5 | **Stability guarantees** as RFC 0001 in STABILITY.md (§17.225 (6)): source compatibility within a major; diagnostic codes never reused; `--json` schemas additive within a major; ABI MAJOR rule; deprecation two minors and six months; MSRV = `rust-version`, CI-tested; `[dependencies]` reserved and refused with a named diagnostic (§17.225 (7)) | `v0.2.0` | each guarantee names the first 0.x that honours it; the MSRV job is in `ci.yml`; the refusal has a `tests/ui` fixture | RFC acceptance at the tag | ~3 |
| 6 | **Normative spec** in `docs/spec/` (§17.225 (9)): grammar, diagnostic contract (generated), ABI annex by reference, static and dynamic semantics as fixture-indexed prose | grammar and diagnostics at `v0.2.0`; complete at `v1.0.0-rc.1` | every normative sentence cites a fixture; the rest is marked informative | none | ~12 |
| 7 | **Evidence produced in-repo** (§17.225 (2)–(5)): (a) six dogfood programs from two model families against the installed `v0.1.0`, each with a `sakura test` suite, upgraded to `v0.2.0` from the release notes alone; (b) agent-DX final success ≥ 10/12 and pass@1 ≥ 8/12 per family in the skill condition, no task failing on both; (c) the fuzzing and differential campaign: ≥ 24 CPU-hours per target on CI, the `exec` differential target, seven consecutive clean nights, zero open `soundness` issues; (d) the envelope held at `v0.2.0` and `v1.0.0-rc.1` | `v1.0.0-rc.1` | (a)–(d) each recorded with its numbers in the rc notes | quiet window and bless at 0.2.0 and at the rc; a fresh key and written approval for the one paid Anthropic run (≤ $5.80 left); signing certificate and notarization credentials; tags | ~31 |
| 8 | **`v1.0.0`** | `v1.0.0` | a seven-day rc soak with no P1 defect, no `soundness` issue and no contract change; a contract change means rc.2 and a new soak; macOS archives signed | the tag | ~1 |

**Out of 1.0:** packages (milestone P; 1.x, additive, after its own research and plan); native as a
stable target, Perceus reuse and borrow inference (M5b); the §17.177 representation windows;
crates.io (§17.157 (3), revisited at the rc); DX8's plugin, playground and Linguist (not gates).

## Estimated timeline (§17.226)

| Weeks from now | Lands | Owner needed |
|---|---|---|
| 0–1 | **A** M4 exit rows; window run | window, bless, gate signature |
| 1–2 | **B** `v0.1.0` | npm token, tag, approvals |
| 2–4 | **C** B/C packaging rows (Sol) beside **D** and **E** | Pi acceptance, then Windows 11 and Intel Mac |
| 2–5 | **D** M5-core on three Opus lines (N1a → N2; N1b; N0b → N5) | none |
| 2–6 | **E** dogfood corpus, `judge` mode and episodes, `exec` target and campaign, spec, RFC 0001 | eval key and approval (optional paid run) |
| 5 | **F** `v0.2.0` | window, bless, RFC acceptance, tag |
| 6 | **G** `v1.0.0-rc.1`: corpus upgraded, eval numbers, streak, spec complete | window, bless, signing credentials, tag |
| 7 | **H** `v1.0.0` after the soak | tag |

About 95 agent-days in total. The critical path is N1a → N2 and the three quiet windows; stage 7
runs beside stages 3–6 from the day `v0.1.0` tags. Fuzzing runs on GitHub-hosted runners only, so
the reference machine stays quiet for the windows.

## 6. Platforms B and C (design §17.180)

Numbered 6 for the references that cite `§6 Px`; it is stage 3 in the table above.
One row per deliverable. "Starts" names the event; Opus codes, Sol does packaging, CI and
workflows, Astra writes docs; the owner accepts each stage on a fresh host (ADR 0003).
The inventory is `packaging/targets.json`; every channel reads it. musl and 32-bit `armv7`
are out (§17.180 (1)).

| # | Row | Owner | Starts | Acceptance |
|---|---|---|---|---|
| P0 | **Done** (2026-10-18; `2329019`; deviation: the drive letter is upper-cased, so a client's `file:///c%3A/…` canonicalizes to the snapshot's URI; Windows cases are unit tests on every host; owed at P5: the cross-OS `--json` comparison on Windows CI) — **URI model**: `disk::file_uri`/`uri_path` RFC 8089 on every OS (`file:///C:/dir/src/main.sak`); module name from the path relative to `src/`; every string test on a URI (`/src/`, `/sakura.toml`, cwd strip, MCP prefix filter, git-base) through one helper | Opus | now | Unix snapshots unchanged; a Windows-path unit test for each former string test; `sakura check --json` `file` fields equal for the same project on every OS |
| P1 | **Done** (2026-10-18; `f0fdaf7`; owed: the named CODEOWNERS review of the §2 lexical rule; §17.196 closed the `*.snap` question: `*.snap -text` replaces `text eol=lf` in `sakura new`/`init` and the runners' hint (Opus), the repository gains the same line (Sol); §2's comment sentence is restated for block comments with one fmt test; `/C:/…` in human output for a file outside the cwd is a P0 follow-up (`display_files` through `disk::uri_path`, Windows-flag unit test), not P5's) — **Line endings**: `\r\n` in a string literal is `\n`, comments exclude `\r` (§2); `fmt` writes LF and names CRLF as the cause; `sakura new` writes `.gitattributes` (`*.sak`, `*.snap` `eol=lf`); snapshot mismatch names line endings when only `\r` differs | Opus | now | CRLF semantics fixture (checked in `-text`) equal to its LF twin in stdout and trace under both ABIs and on WASM; CRLF `fmt` case; CODEOWNERS review (lexical rule) |
| P2 | **Done** (2026-10-18; `2224295`, `5fdfe9c`; deviation: Unix grants keep the real path, since Node's loader realpaths the entry and macOS `$TMPDIR` is a symlink; `std::path::absolute` on Windows; ratified in §17.186, which owes one symlinked-project test; the `windows-2025` acceptance waits for P5) — **Processes**: Node lookup honours `PATHEXT`; child status outside `0..=255` exits `EXIT_DEFECT` with the raw code on stderr; grants via `std::path::absolute`; solver lookup with `EXE_SUFFIX`; a deadline-killed solver reported as killed on Windows | Opus | now (solver items after V4b) | unit tests per item; `sakura test` with a snapshot write passes on `windows-2025` once P5 exists |
| P3 | **B inventory and CI**: the `aarch64-unknown-linux-gnu` entry (runner `ubuntu-22.04-arm`, npm `sakuralang-linux-arm64-gnu`); `ci.yml` rust and parity jobs on `ubuntu-24.04-arm`; the `Linux-ARM64` solver case from the pinned z3/cvc5 assets (built in a cached step when the pinned version has none); `release.yml` steps under `shell: bash`; release notes and `tools/install/test.sh` read `targets.json` | Sol | alpha.1 accepted on a fresh host (npm published) | CI green on the ARM lane; a `workflow_dispatch` dry run produces and smokes the ARM archive and npm package; the formula's Linux ARM64 refusal names the installer and npm |
| P4 | **B acceptance** | owner | P3 shipped in a release | on a Raspberry Pi 4/5 with 64-bit Raspberry Pi OS: `install.sh`, `npm i -g sakuralang`, `sakura new cli` then `check`, `test`, `build`, `run`, `run --target wasm`, the MCP smoke |
| P5 | **C-Windows packaging**: `targets.json` entry (`windows-2025`, `sakuralang-win32-x64-msvc`); `release.py` `.zip` with `sakura.exe`, smoke on Windows (`SystemRoot`, `PATHEXT` in the env), `+crt-static`; `tools/install/install.ps1` (pinned version, `Get-FileHash` against `SHA256SUMS`, fail without a sum, `Expand-Archive` to `%LOCALAPPDATA%\Programs\sakuralang\bin`, no PATH edits); launcher resolves `bin/sakura.exe`, no signal re-raise on `win32`; `ci.yml` on `windows-2025` with the `Windows-X64` solver case; unix-only tests stay `#[cfg(unix)]`, the `m3.rs` symlink test gated, `agent_docs` separator normalisation; host-floor step Windows case; VS Code client prefers `sakura.exe` and never spawns `sakura.cmd` without a shell | Sol (CI test gating with Opus) | B accepted | CI green on Windows; dry run produces and smokes the `.zip` and npm package; `install.ps1` tested against a stub release as `test.sh` is |
| P6 | **C-Intel packaging**: `targets.json` entry (`macos-15-intel`, `sakuralang-darwin-x64`); formula `on_intel` block, `depends_on arch` line dropped on macOS; `ci.yml` parity on `macos-15-intel` with the `macOS-X64` solver case; fallback when the Intel runner retires: cross-build on arm64 and smoke under Rosetta | Sol | B accepted | CI green; dry run produces and smokes the Intel archive, npm package and formula |
| P7 | **Docs and floors**: install page per OS (Pi: Node 26 from NodeSource or nvm, the installer does not install Node); STABILITY.md floors (glibc ≥ 2.35 both CPUs, macOS ≥ 12.0 both CPUs, Windows 10 x64+) and limitations (no proof cache on Windows; `bench/` reference host only) | Astra drafts, Sol lands | with P3 and P5/P6 | the page matches `targets.json`; the floor sentence appears only after that stage's smoke test exists (§17.175 (5)) |
| P8 | **C acceptance** | owner | P5 and P6 shipped | Windows 11 x64: `install.ps1`, npm, the P4 list, `fmt --check` clean under `core.autocrlf=true`, the VS Code LSP, a Claude or Codex MCP config; Intel Mac: `install.sh`, `brew install`, npm, the P4 list |

Native codegen on B and C is M5b's business (`m5-native.md` §5 rows B and C), after 1.0 (§17.225 (8)).
