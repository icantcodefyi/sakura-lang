# Native harness build integration validation

2026-10-18. AI implementation, diagnosis and verification by GPT-6.1-Sol;
the human contributor owns review. Worktree: `sakura-worktrees/nativeports`,
branch `wt/nativeports`, starting commit `d900ee3`.

## Root cause and fix

The [WS-S10 record](wss10-validation.md) retained 16 native integration
failures while the independent verifier passed. Re-running every OCaml/Koka
harness builder at the starting commit reproduces **16/16 build failures**:
five OCaml `Unbound module Runner`, eleven Koka `could not find module: kernels`.
[Original builder failures](nativeports-before.json).

The installed compilers are the pinned OCaml **5.5.1** and Koka **3.2.9**.
No opam switch, package install, toolchain version change or environment override
is required. The harness had not adopted the shared-module build configuration
already used by `bench/programs/_shared/python/verify.py`:

- OCaml compiled only `main.ml`. Copy and compile `runner.ml` and `kernels.ml`
  before `main.ml`, and link the bundled `unix.cmxa` with `-I +unix`.
  The shared runner calls `Unix.gettimeofday`. A probe adding only the shared
  sources fails with `No implementation provided ... Unix referenced from Runner`;
  adding the Unix library succeeds. Existing conditional flambda `-O3` is preserved
  (this machine reports flambda `false`). All intermediates remain in scratch.
- Koka had no shared include path. Adding only `--include` to the old command
  exposes a second failure: the root-relative module
  `bench/programs/binary_trees/koka/main` is sought as
  `bench/programs/binary/trees/koka/main`. Invoke `main.kk` from the port's `koka/`
  directory with the shared include path. Match the verifier's `--target=c`,
  `--stack=67108864`, and `--builddir`, isolated per program. Default reuse stays
  enabled; no-reuse remains a separate verifier invocation.

[Controlled build probes](nativeports-probes.json) retain commands, working
directories, exit codes and compiler output. The actual root causes are missing
shared build inputs/linking and Koka module-path resolution; the C target, stack
and build-directory flags align with the passing verifier, without claiming that
each was independently a cause of the original build failures.

No port algorithms, shared port sources, checksum metadata, toolchain pins or
`bench/baselines/` files changed. This is a harness integration repair, so no
benchmark source-definition boundary is introduced. The historical WS-S10
failure evidence is retained unchanged.

## Regression coverage

Two real-compiler integration tests build `binary_trees` in isolated target
directories, check every oracle input, and consume nine real sample records with
the expected checksum. The Koka case exercises a program name with an underscore.
Both tests fail with the original builders and pass with the repaired builders:
[before](nativeports-regression-before.log), [after](nativeports-regression-after.log).
The tests explicitly skip when the corresponding compiler is absent from `PATH`.

## Full native run

From the worktree root:

```sh
bench/bench run --suite native --lane matched --sizes quick --processes 1 --json bench/target/nativeports-validation/native-after.json
python3 -m unittest discover -s bench/harness
git diff --check
git diff --exit-code -- bench/programs bench/toolchains.toml bench/baselines
```

- Full native harness: **exit 0**, **58 builds/oracles pass** (21 Rust, 21 Go,
  5 OCaml, 11 Koka), **466 oracle values** (169 Rust, 169 Go, 40 OCaml, 88 Koka).
  All **116 timing cells** pass checksums, with nine finite nonnegative samples
  each, exact expected program/variant/size coverage, no missing tools and no
  failures. The repaired ports contribute **32 cells** (10 OCaml, 22 Koka).
  [Raw JSON](nativeports-native-quick.json), [run log](nativeports-native-quick.log).
- Harness suite: **45 tests, OK**, exit 0, including both real-compiler regressions;
  no skips on this machine. [Test log](nativeports-tests.log). Its intentional
  wrong-checksum rejection test prints `[FAIL]` for the injected bad fib metadata;
  the suite itself passes.
- `git diff --check` and the unchanged-port/pin/baseline check: **exit 0**.
  Local documentation links and retained JSON were checked.

[Source identities](nativeports-source-identities.json) pin the tested harness,
tests, independent verifier and toolchain file. The run's `meta.commit` is the
starting commit `d900ee3`; `meta.dirty: true` identifies the uncommitted harness
repair tested here. The result includes the actual tool versions and host data.

**Advisory timings only:** shared loaded host, `meta.host.quiet: false`, one
process per quick timing cell, no performance-ceiling claim or baseline adoption.
No compiler/runtime/std changes were made; no full compiler-workspace, website
or no-reuse-panel verification is claimed for this repair. Git rebase, post-rebase
harness tests and push outcomes are reported with the final commit SHA by the
implementation agent.
