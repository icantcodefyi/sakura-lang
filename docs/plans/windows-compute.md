# Windows compute brief: the soundness campaign and every other heavy job

**For a Claude session on the owner's Windows machine.** Design §17.306 (2026-10-21): the owner
runs all compute-heavy work that is not tied to the reference Mac on this machine. The main job is
the fuzz campaign that gates `v1.0.0` — ≥ 24 CPU-hours per target, zero `soundness` findings,
recorded in the ledger in `compiler/fuzz/README.md`. This brief is self-contained: do not ask
questions; where a choice exists it is made below. Read `AGENTS.md` once after the checkout.

**Stays on the reference Mac (host id `d88970189598`, Apple M4 Pro), never run here:** the blessed
instruction-count baselines of `docs/release.md` §2 — one counted run per baseline release
(beta.3, 1.0.0), about an hour each, blessed by the owner. A count from another host is another
host's baseline and compares to nothing. Nothing else is tied to the Mac.

**Never print secrets.** Do not run `env`, `printenv`, `set`, `echo $GH_TOKEN` or `set -x` around
commands that take a token; do not paste tokens, keys or `~/.config/gh` contents into issues,
commits, the ledger or the chat. `gh auth login` stores its token itself. Issues and ledger rows
carry commit hashes, run numbers, inputs and outputs only.

## 0. Platform: WSL2 Ubuntu, not MSVC

cargo-fuzz links LLVM's libFuzzer and wants a sanitizer runtime; Rust's `-Zsanitizer=address` is
not available on `x86_64-pc-windows-msvc`, and the `exec` target spawns Node with POSIX temp
directories and the native sanitizer lanes need Valgrind and `x86_64-unknown-linux-gnu`. All of it
runs unchanged on WSL2 Ubuntu, which is what GitHub's `ubuntu-latest` runners are. Keep the
repository on the Linux filesystem (`~/sakuralang`), not under `/mnt/c` — the 9P bridge makes
cargo builds several times slower.

### 0.1 Install WSL2 (PowerShell as Administrator, once)

```powershell
wsl --install -d Ubuntu-24.04
wsl --set-default Ubuntu-24.04
```

Reboot if asked, open "Ubuntu 24.04", create the Unix user. Then give WSL2 the machine: write
`%UserProfile%\.wslconfig` (PowerShell: `notepad "$env:UserProfile\.wslconfig"`) with

```ini
[wsl2]
processors=<all logical cores>   # Get-CimInstance Win32_ComputerSystem | select NumberOfLogicalProcessors
memory=<total RAM minus 8GB>     # e.g. memory=56GB on a 64 GB machine
swap=0
```

then `wsl --shutdown` and reopen Ubuntu. `nproc` and `free -g` inside Ubuntu must show those numbers;
the fuzz workers below are sized from them.

### 0.2 Packages (inside Ubuntu)

```sh
sudo apt-get update
sudo apt-get install --yes build-essential clang lld pkg-config git curl python3 valgrind libclang-dev gh unzip
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
. "$HOME/.cargo/env"; echo '. "$HOME/.cargo/env"' >> ~/.bashrc
rustup toolchain install nightly --profile minimal
rustup target add wasm32-unknown-unknown --toolchain nightly
cargo +nightly install cargo-fuzz --version 0.13.2 --locked
# Node 26 (the CI pin) for the exec target, the dogfood rounds and the agent-DX harness.
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
. "$HOME/.nvm/nvm.sh"; nvm install 26; nvm alias default 26
node --version    # v26.x
gh auth login     # GitHub.com, HTTPS, login with a web browser; say yes to configuring git
```

The pinned stable toolchain (`compiler/rust-toolchain.toml`, 1.99.1) installs itself on the first
`cargo` call in the checkout. `NATIVE_NIGHTLY` for §2 is installed there.

### 0.3 Checkout

```sh
gh repo clone icantcodefyi/sakuralang ~/sakuralang
cd ~/sakuralang
git config core.hooksPath .githooks
git config user.name "<owner's GitHub name>"; git config user.email "<owner's public email>"
git fetch origin main && git checkout -B main origin/main
git merge-base --is-ancestor 611dae4f HEAD && echo "ok: at or after beta.2's preparation commit"
git rev-parse --short=12 HEAD   # the Commit column of every ledger row from this checkout
```

Hours count only at or after `611dae4f` (§17.306 (2)). Before each day's runs, `git fetch origin
main && git rebase origin/main` (or `git checkout -B main origin/main` when nothing is local) so
the hours land on current `main`; the ledger row names the commit the binary was built from, so a
run that straddles a rebase is still one row at the older commit.

## 1. The campaign (the gate)

### 1.1 Targets

From `compiler/fuzz/README.md`; the first column is the `cargo fuzz` name.

| Target | Input | Seeds beyond `fuzz/corpus/<target>` |
|---|---|---|
| `parse` | any bytes, ≤ 16 KiB | `tests/ui/*.sak`, `tests/semantics/*.sak` |
| `check` | any bytes, ≤ 8 KiB, with std | same |
| `fmt_idempotent` | sources that parse | same |
| `row_unify` | 4 bytes | none |
| `lir` | checked sources, ≤ 8 KiB | same as `parse` |
| `vc` | checked sources, ≤ 8 KiB | same as `parse` |
| `exec` | checked sources whose `main` is at most `{Console}`; differential JS/LIR/WASM/reference | `tests/semantics/*.sak` only |

`exec` is the one that needs Node on `PATH` (or `SAKURA_NODE`) and the `wasm32-unknown-unknown`
target; each worker runs Node processes with a 512 MiB heap, so it gets half the workers.

### 1.2 Sizing

```sh
cd ~/sakuralang/compiler
N=$(nproc); RAM=$(free -g | awk '/^Mem:/{print $2}')
W=$(( N < RAM*2/5 ? N : RAM*2/5 ))     # ≤ 2.5 GiB per worker (the 2 GiB rss limit plus Node)
[ "$W" -lt 1 ] && W=1
WEXEC=$(( W/2 < 1 ? 1 : W/2 ))
SECS=$(( 24*3600/W + 24*3600/W/20 ))   # 24 CPU-hours per target plus 5 % margin
SECS_EXEC=$(( 24*3600/WEXEC + 24*3600/WEXEC/20 ))
echo "workers=$W exec_workers=$WEXEC seconds_per_target=$SECS exec_seconds=$SECS_EXEC"
```

Sixteen workers give 5,670 s (1.6 h) per target and 3.2 h for `exec`: about 12.5 wall-hours for
all seven, run one target at a time. Eight workers double it. Write the numbers down; they go into
the ledger.

### 1.3 Build and seed once

```sh
cd ~/sakuralang/compiler
cargo +nightly fetch --locked --manifest-path fuzz/Cargo.toml     # a stale fuzz/Cargo.lock fails here, not an hour in
cargo +nightly fuzz build                                          # all seven targets, release, ~10 min cold
for t in parse check fmt_idempotent row_unify lir vc exec; do
  mkdir -p "target/fuzz-corpus/$t"
  cp fuzz/corpus/"$t"/* "target/fuzz-corpus/$t/"
  case "$t" in
    row_unify) ;;
    exec) cp tests/semantics/*.sak "target/fuzz-corpus/$t/" ;;
    *) cp tests/ui/*.sak tests/semantics/*.sak "target/fuzz-corpus/$t/" ;;
  esac
done
```

`target/fuzz-corpus/<target>` is the living corpus: libFuzzer adds every interesting input there,
each worker reloads it, and later runs start from it (do not delete it between runs; it is
git-ignored). Crashes go to `fuzz/artifacts/<target>/` (also git-ignored). Per-worker logs are
`fuzz-<i>.log` in the directory the run is started from.

### 1.4 Run one target

```sh
cd ~/sakuralang/compiler
t=parse; w=$W; secs=$SECS                     # for exec: t=exec; w=$WEXEC; secs=$SECS_EXEC
start=$(date -u +%FT%TZ)
cargo +nightly fuzz run "$t" "target/fuzz-corpus/$t" -- \
  -jobs="$w" -workers="$w" -max_total_time="$secs" -max_len=16384 -timeout=10 -rss_limit_mb=2048 \
  2>&1 | tee "$HOME/fuzz-$t-$(date -u +%Y%m%dT%H%M).log"
echo "started $start, ended $(date -u +%FT%TZ), exit ${PIPESTATUS[0]}"
ls fuzz/artifacts/"$t"/ 2>/dev/null | wc -l                  # 0 = clean
ls "target/fuzz-corpus/$t" | wc -l                            # the Corpus-after column
```

`-jobs=$w -workers=$w` runs `w` processes in parallel for `secs` seconds each, so the row's CPU-h
is `w × secs / 3600` (≥ 24 by construction when `secs` is from §1.2). The parent prints a line per
job as it finishes and exits non-zero if any job found an artefact. Run the seven targets in the
order of §1.1, one after another (a `for` loop over the same block is fine; `exec` last so the six
fast ones have their rows before the long one). Each run may be stopped and resumed: the corpus
persists, and a shorter run is simply a smaller row; two rows add up.

**Done when**: every target's CPU-h total in the ledger (§1.7) is ≥ 24 at or after `611dae4f`,
every finding is filed (§1.5) and, for a target that had a fix, ≥ 4 CPU-h of its rows are at or
after the fix's commit with `Findings` = `none`.

### 1.5 Triage a finding

An artefact is `fuzz/artifacts/<target>/{crash,timeout,oom,leak}-<sha1>`. For each:

```sh
cd ~/sakuralang/compiler
t=<target>; a=fuzz/artifacts/$t/<file>
cargo +nightly fuzz run "$t" "$a" -- -timeout=10 -rss_limit_mb=2048        # 1. reproduce on this commit
cargo +nightly fuzz tmin "$t" "$a" -- -timeout=10 -rss_limit_mb=2048       # 2. minimise (writes minimized-from-<sha1> beside it)
cargo +nightly fuzz fmt "$t" "fuzz/artifacts/$t/minimized-from-<sha1>"     # 3. print the input as the target's type
```

Classify:

- **Does not reproduce** on the current commit → not a finding; delete the artefact, note it in
  the row's `Findings` as `none` (say "one stale artefact" in the commit message).
- **`exec` `timeout-` or `oom-`** whose program allocates without bound inside the reference
  evaluator's step bound (a `List.range`/`String.repeat`/`List.push` loop with a huge count: read
  the printed program) → `limit-hit` in the row, no issue (`compiler/fuzz/README.md` explains why).
- **Anything else that reproduces** — a panic, a stack overflow past the nesting limit, an
  assertion in the target, an `exec` divergence ("the backends diverge (a soundness bug)"), a
  timeout or OOM in a non-`exec` target — is a **finding**. File it:

```sh
gh label create soundness --repo icantcodefyi/sakuralang --color B60205 \
  --description "A soundness finding (fuzz campaign, parity, verifier); open ones block v1.0.0" 2>/dev/null || true
gh issue create --repo icantcodefyi/sakuralang --label soundness,bug \
  --title "soundness: fuzz <target> <panic message or 'divergence'> at <commit12>" \
  --body-file /tmp/issue.md
```

`/tmp/issue.md` contains: the target; the commit (`git rev-parse HEAD`); the exact reproduce
command; the minimised input in a fenced block (`cargo fuzz fmt`'s output; for `parse`/`check`
bytes that are not UTF-8, `base64 -w0 <file>` in the block and say so); the libFuzzer summary
(the `==ERROR` or `panicked at` lines and the top ten frames, no more); for `exec`, the printed
"why" and program; and the line "Found by the §17.306 campaign on WSL2; ledger row <date>". Do not
fix it in this session: the owning lane (C or R, `compiler/fuzz/README.md`'s last paragraph)
takes the issue, adds the regression under `tests/ui` or `tests/semantics`, and lands the fix;
then this session reruns that target (§1.4) for ≥ 4 CPU-h at or after the fix. Copy every
artefact to `~/fuzz-findings/<target>/` before deleting anything.

### 1.6 Corpus backup

After each target: `tar czf ~/fuzz-corpus-backup/$t-$(date -u +%Y%m%d).tgz -C target/fuzz-corpus $t`
(mkdir it first). The corpus never goes into git; a `fuzz-corpus-<target>` artifact can be
uploaded to a hosted run only by dispatching `fuzz.yml`, which is optional.

### 1.7 Report: the ledger

After every run append one row to the table under "Campaign ledger" in
`compiler/fuzz/README.md` and update the Totals line (sum per target, one decimal). Columns:

| Date (UTC) | Commit | Target | Host | Workers | Wall s | CPU-h | Corpus after | Findings |
|---|---|---|---|---|---|---|---|---|
| 2026-10-22 | `abcdef123456` | `parse` | windows-wsl2 (`<CPU model>`, N cores) | 16 | 5670 | 25.2 | 1,842 | none |

`Host` is the string above with the CPU from `lscpu | grep 'Model name'`; `Findings` is `none`,
`limit-hit`, or `#<issue>` per finding. Commit it by pathspec and push to `main`:

```sh
cd ~/sakuralang
git add compiler/fuzz/README.md
git commit -m "docs(fuzz): ledger row, <target> <CPU-h> CPU-h at <commit12>" \
  -m "Co-Authored-By: Claude <noreply@anthropic.com>"
git fetch origin main && git rebase origin/main && git push origin HEAD:main
```

The pre-push hook builds the compiler from the pushed commit (a few minutes the first time). If
the push is refused (branch protection, no write access), open a PR instead:
`git checkout -b ledger/<date> && git push -u origin HEAD && gh pr create --fill --title "docs(fuzz): ledger rows <date>"`,
then delete the branch after merge. One commit per target or per day; never commit
`target/`, `fuzz/artifacts/`, `fuzz-*.log` or the corpus.

## 2. Long-running jobs that also run here

### 2.1 The native sanitizer lanes (at each beta's commit, ~1 h)

`.github/workflows/native-sanitizers.yml` runs Miri, ASan + LSan and Memcheck with their
detection controls; the Memcheck corpus step only runs nightly. Run all of it here at beta.3's
commit and at `v1.0.0`'s candidate (x86_64 only — `uname -m` must print `x86_64`; on an ARM
Windows machine skip ASan and Memcheck and say so in the ledger).

```sh
cd ~/sakuralang/compiler
export NATIVE_NIGHTLY=$(grep -oP 'NATIVE_NIGHTLY: \K\S+' ../.github/workflows/native-sanitizers.yml)
rustup toolchain install "$NATIVE_NIGHTLY" --profile minimal --component miri,rust-src
# Miri (~15 min)
cargo +"$NATIVE_NIGHTLY" miri test -p sakura_rt --features native --locked --test native --test heap --test native_detect 2>&1 | tee ~/miri.txt
grep -F 'test an_invalid_slot_kind_is_stopped - should panic ... ok' ~/miri.txt
# ASan + LSan (~30 min)
export RUSTFLAGS=-Zsanitizer=address CC=clang CFLAGS="-fsanitize=address -fno-omit-frame-pointer" ASAN_OPTIONS=detect_leaks=1 TARGET=x86_64-unknown-linux-gnu
cargo +"$NATIVE_NIGHTLY" test --target "$TARGET" -p sakura_rt --features asan --locked --test native_detect -- --nocapture 2>&1 | tee ~/controls.txt
grep -F 'test asan_reports_a_double_drop ... ok' ~/controls.txt && grep -F 'test asan_reports_a_stale_request ... ok' ~/controls.txt
cargo +"$NATIVE_NIGHTLY" test --target "$TARGET" -p sakura_rt --features asan --locked
cargo +"$NATIVE_NIGHTLY" test --target "$TARGET" -p sakura_host_native --features native,sakura_rt/asan --locked --lib
cargo +"$NATIVE_NIGHTLY" test --target "$TARGET" -p sakura_host_native --features native,sakura_rt/asan --locked --test corpus -- \
  --exact the_corpus_runs_natively host_root_handlers_runs_natively virtual_clock_and_seeded_random_match_js two_instances_close_in_one_process
unset RUSTFLAGS CC CFLAGS ASAN_OPTIONS
# Memcheck with the full corpus (~40 min)
export SAKURA_NATIVE_DRIVER_FEATURES=sakura_rt/valgrind SAKURA_NATIVE_RUNNER="valgrind --quiet --error-exitcode=99"
cargo test -p sakura_host_native --features native --locked --test native_detect -- --exact memcheck_catches_a_freed_object_read --nocapture 2>&1 | tee ~/control.txt
grep -F 'skipped:' ~/control.txt && echo "CONTROL SKIPPED: red" ; grep -F 'test memcheck_catches_a_freed_object_read ... ok' ~/control.txt
cargo test -p sakura_host_native --features native --locked --test corpus -- --exact the_corpus_compiles_links_and_runs_natively --nocapture
unset SAKURA_NATIVE_DRIVER_FEATURES SAKURA_NATIVE_RUNNER
```

**Done when** every command exits 0 and every `grep` prints its line. Report as one ledger row
with `Target` = `sanitizers (miri, asan, memcheck)`, `Workers` = `—`, `Wall s` = the total,
`CPU-h` = `—`, `Findings` = `none` or `#<issue>`. A sanitizer report is a finding: file it as §1.5
with the sanitizer's report in place of the libFuzzer summary. Native is experimental and off
every tag's gate (§17.245 (2)), so a native-only finding does not block 1.0.0 but is filed and
fixed the same way.

### 2.2 The `exec` differential

It is the campaign's own target (§1); its 24 CPU-hours are the "differential of stated size"
§17.225 (4b) asks for. No separate job.

### 2.3 Soak and stress loops

The repository has none beyond the fuzz targets; do not invent one. The one-off checks worth a
run at each beta's commit, each a few minutes, reported in the sanitizer row's `Findings`:

```sh
cd ~/sakuralang/compiler
cargo test -p sakura_cli --test parity --locked -- --nocapture       # the JS/WASM parity matrix on Node 26
cargo test --workspace --locked                                       # the workspace, as CI's rust job
```

## 3. Optional extras (run after §1's hours are in, or on spare cores)

### 3.1 The offline agent-DX harness (~10 min)

Fixture validation only — no model, no API key, no spend (§17.301 (3c)'s (ii)):

```sh
cd ~/sakuralang/compiler && cargo build --release -p sakura_cli --locked
cd ~/sakuralang && python3 -I evals/agent-dx/harness/offline.py --sakura compiler/target/release/sakura
```

**Done when** it exits 0 (every task's gold passes and hollow fails its oracle). A failure is an
issue labelled `bug` (not `soundness`) with the task id and the output; it means a task or the
compiler changed under the other.

### 3.2 The dogfood round against a published beta (~5 min)

```sh
. "$HOME/.nvm/nvm.sh"; npm install -g sakuralang@beta
cd ~/sakuralang && dogfood/round.sh "$(npm prefix --global)/bin/sakura"
```

**Done when** every line is `ok` and it exits 0: the six programs `check` and `test` on js and
wasm against the installed beta (`dogfood.yml` runs the same after each publish). A `FAIL` is an
issue labelled `bug` with the program, the step and the log; the per-beta upgrade round
(§17.301 (3d)) is Opus's, from Astra's `MIGRATION.md`, not this session's.

### 3.3 Hosted-runner hours (optional, same ledger)

`gh workflow run fuzz.yml --repo icantcodefyi/sakuralang -f max_total_time=21600` adds six hours
per target on GitHub's runners (one worker each, so 6 CPU-h per target per dispatch). Record each
such run as a row with `Host` = `github ubuntu-latest run <id>`, `Workers` = 1. Not required.

## 4. End of campaign: what to hand back

1. The ledger's Totals line reads ≥ 24.0 for all seven targets, every row at or after
   `611dae4f`, and every finding's issue is closed with a fix commit and a regression; a fixed
   target has ≥ 4 clean CPU-h at or after its fix.
2. `gh issue list --repo icantcodefyi/sakuralang --label soundness --state open` prints nothing.
3. The §2.1 sanitizer row for the last beta's commit is `none`.
4. A one-paragraph note in the final commit message or PR body: machine (CPU, cores, RAM, WSL2
   kernel from `uname -r`), total wall-hours, targets' CPU-h, findings by issue number, and
   anything skipped (ARM: ASan/Memcheck) with the reason.

That note and the ledger are `v1.0.0`'s soundness evidence (§17.306 (2)); the release notes cite
the ledger's final commit. The tag itself, the bless and the channel approvals are the owner's
steps on the Mac (`docs/release.md` §5).
