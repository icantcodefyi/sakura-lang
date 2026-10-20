# Candidate baselines — NOT BLESSED

## The §17.282 flag-commit candidate (2026-10-20, `3bf2343f`)

`2026-10-20-d88970189598-3bf2343f-js.json` and `…-wasm.json` are counted (`--instructions`)
runs of the `js` and `wasm` suites at the flag commit `3bf2343fa8c1199a163b87e92886c41ae34025ca`,
host `d88970189598`, Node 26.11.0, Bun 1.4.3, from a clean detached worktree of that commit
(`meta.dirty: false`; `meta.paths` names it), all sizes, five pairs a cell. `meta.flags.node`
is `--predictable --min-semi-space-size=16 --max-semi-space-size=16`. Every oracle and
checksum is green; `meta.failures` and `meta.missing` are empty; the wasm file's one
`meta.absent` is the impure program's Wasmtime cell (§17.191 (2)). Tables:
[js](../../results/2026-10-20-3bf2343f-js.md), [wasm](../../results/2026-10-20-3bf2343f-wasm.md)
(the js table was regenerated from the JSON with `bench.summary`, its run output having been
deleted from `/tmp`; the same regeneration reproduces the wasm run's output byte for byte).
The host was loaded by other agents' builds and counts (load 6–10); counts are read regardless.

Spread (release.md §2): Node's JS cells a median 0.35% (15 of 198 over 1%), its WASM lane
0.29% (13 of 198), Wasmtime 0.16% (1 of 95), Bun 1.56% (121 of 198), against 0.14%, 0.30%,
0.11% and 1.35% at `320c9980`. The Node cells over 1% are almost all allocation-heavy
programs at their largest sizes (`shapes`, `map_set*`, `rbtree`, `deltablue`, up to 2.47% on
`shapes` n = 100,000), where a 16 MB young generation scavenges more often than the default
and a pair's count follows its collections; the rest are sub-0.2 M-instruction cells at the
call cap (`countdown`, `handled_d4`, `sieve` n = 1,000). Wasmtime's one is `countdown`
n = 1,000 (29.8%): one pair at 113 k instructions per call, the other four 87.3–87.5 k, a
65,536-call cell at the cap; the median is the four.

Bless (the owner, release.md §2): archive the `320c9980` files, then bless these two.

## The first reference candidate (2026-10-18, `a843e60`)

Only the human owner, @icantcodefyi, may adopt this candidate. GPT-6-Astra prepared
measurement evidence and has **not** run `bench/bench bless`. CI never blesses results.
Files in this directory are outside the workflow's `bench/baselines/*.json` glob.

The candidate is the complete JS matched-lane run at `a843e609d95eed9272b3f1e82f4bec8b813d5538`
on Apple M4 Pro host `d88970189598`, Node 26.11.0 and Bun 1.4.3. All four input sizes,
21 programs (15 families with handler/contract companions), five fresh processes per
variant/engine/size, default 250 ms warmup and all raw samples are retained. Its JSON
records the measured commit; later documentation commits do not change that provenance.

This is a **shared-host/noisy candidate**, not an assertion that the host was idle or the
run is suitable for automatic adoption. Read the [WS-A report](../../../research/10-evidence/ws-a/README.md),
[load log](../../../research/10-evidence/ws-a/candidate-current-load.jsonl) and
[run output](../../../research/10-evidence/ws-a/candidate-current.log) before deciding.
The noise series is a selected quick suite, not a repeat of every candidate cell.
An idle full-suite confirmation remains advisable before blessing. The current
`bench compile` result also has an LSP p95 of 109.2 ms over its 100 ms budget; the other
three compile cases pass. That miss is retained in the report and has not been waived.

**Known matched-work gap:** at the measured commit, DeltaBlue's JS control performs
per-destination assertions after scale and offset changes (projection tests 3 and 4),
while the Sakura program sums those destinations without the two assertion loops.
This is also reported by WS-S in `compiler/REQUESTS.md`. Matching aggregate checksums
does not prove matching validation work. This candidate records the current suite;
it is not a competitiveness acceptance result. Resolve or explicitly review this gap
before adoption; a source fix needs a new measurement, not edited metadata.

WS-B's earlier [20-program run](../../results/2026-10-18-d88970189598.json) at `f6d04c0`
and its [DeltaBlue supplement](../../results/2026-10-18-d88970189598-deltablue.json) at
`8ce80d7` are retained separately. The supplement has `meta.dirty: true`. This candidate
does not combine their rows, remove their dirty flags, or pretend they were one commit.
No load snapshots were supplied with those historical files. Their sieve rows also
predate `5b10553`, which changed the algorithm to indexed clearing from p² using `List.set`;
those rows are excluded from noise and baseline comparisons. The current candidate must
include that commit and uses one fresh full-suite invocation, not a mixture of versions.

The lane flag is not a full semantic audit. The JS `fib` control uses raw `Number`
arithmetic over the js-safe corpus; review overflow-check placement against §17.109
before claiming complete matched-semantics coverage.

## Human adoption procedure

1. Inspect the complete candidate JSON and the report: `meta.dirty` must be false,
   `meta.failures` and `meta.missing` empty, every `checksum_ok` true, all expected cells
   present, and pins/host/source appropriate for the intended baseline. Review retained
   samples, load and any surprising ratios; a successful exit alone is insufficient.
   The recorded matrix, pins, checksums and medians can be checked without adopting it:

   ```sh
   python3 research/10-evidence/ws-a/verify_candidate.py bench/baselines/candidate/2026-10-18-d88970189598-a843e60.json
   ```
2. Prefer a fresh full-suite confirmation during a quiet window. Keep it as another raw
   artifact and use `bench/bench compare` with identical pins, flags and sizes. Investigate
   timing, RSS and complexity alerts; do not raise thresholds or remove samples to make it pass.
3. If satisfied, **the human owner** runs this from the repository root:

   ```sh
   bench/bench bless bench/baselines/candidate/2026-10-18-d88970189598-a843e60.json
   git add -- bench/baselines/d88970189598.json
   git diff --cached -- bench/baselines/d88970189598.json
   git commit -m "chore(bench): bless first reference baseline" -m "Co-Authored-By: GPT-6-Astra <noreply@openai.com>" -- bench/baselines/d88970189598.json
   ```

   `bless` copies the reviewed run to `bench/baselines/d88970189598.json`. Preserve any
   existing baseline under a dated archive before replacing it, especially when pins
   change. If choosing a newer confirmed run, pass that reviewed JSON instead; do not
   edit this candidate's metadata to make it look newer or cleaner.
4. Push through the repository's normal validation/review process. This agent has not
   performed these adoption steps. When no root baseline exists, the CI regression job
   reports that absence; it never adopts files from this candidate directory.

AI assistance: GPT-6-Astra authored the methodology, analysis and candidate documentation;
the human contributor owns review and adoption.
