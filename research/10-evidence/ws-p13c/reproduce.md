# Reproduce WS-P13c

Use Node 26.11.0, Bun 1.4.3, and compiler revision
`9fe3050f`. Keep this evidence directory available in the pinned checkout.
Run from the repository root. The original scratch directory is
`/tmp/astra-wsp13c`.

```sh
export CARGO_TARGET_DIR=/tmp/sakura-p13c-target
export P13C_WORK=/tmp/astra-wsp13c
(cd compiler && cargo build --release --locked -p sakura_cli)
"$CARGO_TARGET_DIR/release/sakura" build --out-dir "$P13C_WORK/base" research/10-evidence/ws-p13c/main.sak
"$CARGO_TARGET_DIR/release/sakura" build --dump owned research/10-evidence/ws-p13c/main.sak > "$P13C_WORK/owned.txt"
python3 research/10-evidence/ws-p13c/prepare.py "$P13C_WORK"
python3 research/10-evidence/record_load.py --log "$P13C_WORK/load.jsonl" -- python3 research/10-evidence/ws-p13c/measure.py "$P13C_WORK"
```

[prepare.py](prepare.py) creates the benchmark entry wrapper used by WS-P13a.
It changes only four Planner mutation sites for the owned arm. The two
`List.set` calls use std's existing `set$o` wrapper, which preserves the
precondition and calls `list_set_owned`. The two allocation sites use
`list_push_owned`. The flat arm changes only the Planner's two lists and their
accesses. It copies the emitted `set$o` precondition into its flat setter.
It keeps the original defect code, message, caller site, and contract site.
Runtime and std modules are byte-identical across arms.

[measure.py](measure.py) checks every oracle input on Node and Bun before timing.
It measures Node n=5000 only, per the task's narrower scope than §17.217's full
matrix. Eight rounds rotate `base`, `owned`, `flat`, and the unchanged hand-JS
control. Each arm occupies each position twice. Processes run serially.

The unchanged `bench/harness/runner.mjs` warms up for 250 ms, calibrates a
power-of-two batch to at least 30 ms, and records nine samples per process.
Each sample is elapsed milliseconds divided by calls in that batch. The final
checksum must equal the checked-in oracle. A failure aborts collection and is
retained in `readings.json`. No process is dropped.

Aggregate the nine samples into a process median, then report the median,
minimum, and maximum of eight process medians. Divide the base median by
257.8 ms for §17.217 (3a). For (3b), compute `1 - owned / base`.
For the representation window, compute `1 - flat / owned`.

Profile each arm separately after measurement. Profile timings never enter
the table.

```sh
for arm in base owned flat; do
  node --cpu-prof --cpu-prof-dir="$P13C_WORK" --cpu-prof-name="$arm.cpuprofile" bench/harness/runner.mjs measure "$P13C_WORK/$arm/bench_entry.mjs" 5000 --stable > "$P13C_WORK/$arm-profile.json"
done
```

Use WS-P13a's [profile aggregation script](../ws-p13a/reproduce.md#regenerate-the-js-profile-summary)
with `root` set to `$P13C_WORK`. It weights leaf samples by `timeDeltas`,
including startup and GC. The raw profiles are compressed without changing
contents. The flat helper assumes the workload's dense, never-removed IDs.
Forced mutation is an experiment, not a proof that retained Planner versions
remain unchanged. Ownership soundness and `audit_owned` remain WS-P13b work.

The recorded source-map hash depends on the original absolute checkout path.
Runtime and generated JavaScript hashes can be compared across checkout paths;
expect the `.js.map` hash to differ. The nine-file rebuild check in the note
used the original checkout path.

To archive a new run, copy `readings.json`, `load.jsonl`, `owned.patch`,
`flat.patch`, `owned.txt`, and the regenerated `js-profile-summary.json` from
`$P13C_WORK` into a separate evidence directory. Compress the raw profiles with
`gzip -c "$P13C_WORK/base.cpuprofile" > "$ARCHIVE/base.cpuprofile.gz"`, and repeat
for `owned` and `flat`. Keep this run's evidence unchanged when reproducing it.

Check the negative read/write behavior on both engines with
`node research/10-evidence/ws-p13c/check-defects.mjs "$P13C_WORK"` and the same
command with `bun`.
