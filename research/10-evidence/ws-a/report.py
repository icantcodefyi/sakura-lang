#!/usr/bin/env python3
"""Regenerate the WS-A report from its retained measurement artifacts."""
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
CANDIDATE_RECORD = HERE / 'candidate-record.json'
CANDIDATE = ROOT / json.loads(CANDIDATE_RECORD.read_text())['path'] if CANDIDATE_RECORD.exists() else None


def read(path):
    return json.loads(path.read_text())


def percent(value):
    return f'{value * 100:.1f}%'


def table(summary):
    lines = ['| Program | Engine | n | Median ratio | Min–max ratio | Width / median | Max/min − 1 |',
             '|---|---|---:|---:|---:|---:|---:|']
    for row in summary['rows']:
        lines.append(f"| {row['program']} | {row['engine']} | {row['size']} | {row['median_ratio']:.3f}× | "
                     f"{row['min_ratio']:.3f}–{row['max_ratio']:.3f}× | {percent(row['width_over_median'])} | "
                     f"{percent(row['max_increase_over_min'])} |")
    return '\n'.join(lines)


def main():
    local = read(HERE / 'local-noise-summary.json')
    ci = {name: read(HERE / f'ci/{name}-summary.json')
          for name in ('ubuntu-node', 'macos-node', 'macos-bun')}
    lines = ['# WS-A: performance round 1 measurement evidence', '',
             '**Recommendation: retain the 5% warning and confirmed 10% block, with the 0.1 ms floor. Do not tighten.**', '',
             'Five unchanged-commit repetitions already cross the warning margin on both CI operating systems. '
             'The reference-host series was heavily contended and shows much larger variation. '
             'This is evidence against tightening, not an estimate of a universal noise bound or a reason to loosen correctness gates.', '',
             '## Scope and provenance', '',
             '- Local noise: `eaf58bbe625640cd4cdc2f79405bbb4e0f493cf4`, Apple M4 Pro, 24 GiB, 12 logical CPUs; '
             'Node 26.11.0 and Bun 1.4.3. Five complete invocations of `bench/bench run --suite js --lane matched '
             '--sizes quick --filter "[fps]*"`, five processes per cell, nine samples per process, '
             'seeds 20261018–20261022. The five selected programs are fib, pipeline, shapes, sieve and spectral_norm. '
             'All 1,000 fresh timed processes passed checksums (9,000 retained batches). '
             'The four sieve ratio cells per run are excluded from analysis after the algorithm changed at '
             '`5b10553`: 800 processes / 7,200 samples remain in the analyzed subset.',
             '- CI noise: [run 39086512905](https://github.com/icantcodefyi/sakura-lang/actions/runs/39086512905), '
             '`e45f69620c44b586fa95e6faa9baa873f72a1074`, `mode=noise`, `repeats=5`. '
             'The `[fp]*` subset is fib and pipeline, two sizes, five fresh processes per variant per size, '
             '250 ms warmup; 15 complete harness invocations across three jobs, 600 timed processes and 5,400 batches. '
             'All three measurement jobs succeeded, with no missing variants or failed checksums. '
             'That success means data collection passed; the noise job does not enforce `compare` policy.',
             '- Each CI job retained one allocated runner for its five repetitions. This does **not** measure '
             'between-allocation, day-to-day, thermal, stable-warmup or full-suite noise. The macOS Node and Bun '
             'jobs received different hardware; they are not pooled or used to rank engine stability.',
             '- Compiler/runtime/std implementation and research timer/control sources at the local noise revision '
             'are identical to first-green harness commit `f6d04c0b16c7b35721c0c324615d5be92357554f`; '
             'the only excluded std change is the documentation file `compiler/std/INDEX.md`. '
             'See [provenance.json](provenance.json). The requested research rerun was subsequently executed '
             'at the exact `f6d04c0` checkout in this same worktree, then `wt/wsa` was restored. '
             'The earlier source-equivalent `e45f696` rerun is also retained as supplementary evidence.', '',
             'The earlier [full-nightly dispatch](https://github.com/icantcodefyi/sakura-lang/actions/runs/39085419775) '
             'only exercised macOS and the expensive native panel. It was cancelled after the dedicated '
             'noise jobs succeeded to avoid unnecessary runner use; it is not counted as passed or used '
             'in the noise analysis. [Recorded status](ci-dispatch.json).', '',
             '**Sieve compatibility boundary:** `5b10553` replaced full-list clearing passes with indexed '
             'clearing from p² using `List.set`, in Sakura and every control/port. All earlier sieve '
             'ratios—including WS-B’s 2026-10-18 reference files and the raw local noise series—measure '
             'the old algorithm. They are retained only as historical data and excluded from noise '
             'summaries and all baseline comparisons. The local comparison inputs in '
             '`local-comparison-inputs/` explicitly record this selection. A candidate must be measured '
             'after that change; matching prime-count checksums alone does not make the algorithms '
             'comparable. [Source-blob evidence](sieve-definition-change.json) confirms the source changed '
             'while the oracle metadata stayed identical.', '',
             '## Definition of spread', '',
             'For each program, engine and input size, one observation is the ratio of the median of five '
             'Sakura process medians to the median of five hand-JS process medians. The tables describe five '
             'such observations. Width / median is `(max − min) / median`; max/min − 1 is the largest possible '
             'increase between two observations in this sample. These are empirical ranges, not confidence '
             'intervals. We retain all runs and do not treat batches or overlapping ordered pairs as independent '
             'trials. [analyze_noise.py](../analyze_noise.py) rejects dirty, incomplete or mixed-provenance runs.', '',
             '## Reference machine: shared-host / noisy', '',
             'The one-minute load average ranged from **4.76 to 62.10 on 12 logical CPUs**. '
             'Other agents were building/timing; Miri was active. WS-A also ran its required Clippy and workspace '
             'tests during this series, contributing substantial contention. We deliberately retain and label '
             'those observations; this is **not an idle-machine noise floor**. The data cannot separate scheduling '
             'effects from JIT, GC, temperature or power-management effects. '
             '[Load summary](local-load-summary.json), [raw run 1](noise-1.json), '
             '[run 2](noise-2.json), [run 3](noise-3.json), [run 4](noise-4.json), [run 5](noise-5.json). '
             'Each run has a same-prefix `-load.jsonl` and `.log` file.', '', table(local), '',
             '## CI runners', '',
             'Raw downloaded artifacts are retained under `ci/<runner-engine>/`; each includes '
             '`noise-1.json` through `noise-5.json`, command output and 15-second load snapshots '
             '(plus before/after snapshots). See [dispatch/job metadata](ci-noise-dispatch.json).', '']
    for name, summary in ci.items():
        snapshots = [json.loads(line) for p in sorted((HERE / 'ci' / name).glob('*-load.jsonl'))
                     for line in p.read_text().splitlines()]
        host = summary['host']
        lines += [f'### {name}', '',
                  f"{host['cpu']}; {host['os']}; {host['mem_bytes'] / 2**30:.2f} GiB; "
                  f"{snapshots[0]['logical_cpus']} logical CPUs; host `{host['id']}`. "
                  f"One-minute load {min(r['load_average_1_5_15'][0] for r in snapshots):.2f}–"
                  f"{max(r['load_average_1_5_15'][0] for r in snapshots):.2f}. "
                  f'[Summary JSON](ci/{name}-summary.json).', '', table(summary), '']
    lines += ['**Rust metadata caveat:** CI results report ambient root-directory `rustc` 1.99.0. '
              'The actual release CLI build runs inside `compiler/`, whose toolchain file selects 1.99.1; '
              '[job log excerpts](ci/rust-toolchain-log.txt) confirm that override. Node/Bun timing pins match '
              '26.11.0 / 1.4.3. Do not interpret the ambient Rust field as the compiler build version. '
              'No native-panel timing claim is made here.', '',
              '## Applying the existing gate to unchanged source', '',
              'For local runs we first excluded sieve using the provenance-labelled copies in '
              '`local-comparison-inputs/`. We then ran `bench/bench compare noise-1.json noise-2.json noise-3.json` and, for CI, '
              '`bench/bench compare noise-1.json noise-4.json noise-5.json`, retaining the JSON verdicts. '
              'The second head is the same-size confirmation. These few overlapping comparisons are '
              'descriptive examples, not a false-positive-rate estimate.', '',
              '| Series / head+confirmation | Verdict | Timing blocked cells | Immediate failures |',
              '|---|---|---:|---:|']
    comparisons = [('local / 2+3', HERE / 'local-confirmation-123.json')]
    comparisons += [(f'{name} / {suffix[1]}+{suffix[2]}', HERE / f'ci/{name}-confirmation-{suffix}.json')
                    for name in ci for suffix in ('123', '145')]
    for name, path in comparisons:
        doc = read(path)
        lines.append(f"| {name} | {doc['verdict']} | {sum(r['status'] == 'blocked' for r in doc['rows'])} | {len(doc['immediate'])} |")
    lines += ['', 'Local 2+3 has five confirmed timing blocks and two immediate `fib` complexity flags. '
              'macOS/Node 2+3 also fails immediately on the hand-JS `fib` slope (10.21 → 11.33), '
              'despite no source change; its timing rows are only warnings. `fib` is exponential already, '
              'and a two-size log-log slope is particularly sensitive to timing variation. '
              'The current slope heuristic can therefore flag an unchanged algorithm. Tightening the '
              '5%/10% timing constants would not repair that independent gate.', '',
              'Keep both thresholds and the absolute floor. Before any future tightening, collect more '
              'independent runner allocations and quiet local repetitions, extend beyond these quick subset '
              'sizes/families, and test stable warmup separately. Investigate the slope-screen behaviour '
              'with the harness owner; this work does not weaken or change it. Even a confirmed timing '
              'block can reflect shared contention, as the retained local example shows.', '',
              '## Research/10 rerun and compile latency', '',
              '[Exact first-green research rerun](research-first-green-exact.json), [recomputed summary](research-first-green-exact-summary.md), '
              '[load snapshots](research-first-green-exact-load.jsonl), [generated module](research-first-green-exact-generated.js.txt). Five repetitions × two engines × nine workloads '
              '× three controls = 270 processes and 2,430 timing samples; all validation checks passed. '
              'The original `results.json`, `summary.md`, `stability.json` and generated-code evidence '
              'were preserved. This rerun uses the ordinary-JS and checked research controls; do not '
              'compare these ratios directly with the expanded matched suite or call differences from '
              'the historical loaded run optimisation deltas. The earlier source-equivalent run is retained '
              'in `research-rerun.json`; the exact-commit run above is the primary requested evidence.', '',
              'Historical source-equivalent `e45f696` [compile results](compile.json), '
              '[load snapshots](compile-load.jsonl):', '',
              '| Case | p95 ms | Budget ms | Verdict |', '|---|---:|---:|---|']
    for row in read(HERE / 'compile.json')['results']:
        lines.append(f"| {row['program']} | {row['p95_ms']:.1f} | {row['budget_ms']} | {'pass' if row['p95_ms'] <= row['budget_ms'] else 'over budget'} |")
    if (HERE / 'compile-current.json').exists():
        current_compile = read(HERE / 'compile-current.json')
        lines += ['', f"Current `{current_compile['meta']['commit']}` [compile results](compile-current.json), "
                  '[load snapshots](compile-current-load.jsonl):', '',
                  '| Case | p95 ms | Budget ms | Verdict |', '|---|---:|---:|---|']
        for row in current_compile['results']:
            lines.append(f"| {row['program']} | {row['p95_ms']:.1f} | {row['budget_ms']} | {'pass' if row['p95_ms'] <= row['budget_ms'] else 'over budget'} |")
        lines += ['', '**Current compile command exited 2:** the LSP first-edit p95 is 109.2 ms against '
                  'the 100 ms budget (median about 1.1 ms). The other three cases pass. The full sample '
                  'is retained and the budget is unchanged; this observation is not attributed to a '
                  'compiler regression without a controlled repeat.']
    lines += ['', 'These are the harness-defined warm-cache fresh-process CLI and warm-server LSP '
              'measurements, with 15 observations per case. The nearest-rank p95 of 15 is the largest '
              'observed sample; the LSP p95 is sensitive to its first edit. '
              'See [methodology](../../../bench/README.md#methodology) for timer boundaries.', '',
              '## Candidate first baseline', '',
              'The completed [pre-change full run](historical-pre-sieve.json) at `e45f696` retains all '
              '336 passing result cells and [load snapshots](historical-pre-sieve-load.jsonl). It was '
              'superseded by the sieve definition change while timing was in progress and is retained '
              'as historical evidence only; its sieve rows are never used for comparison or adoption.', '']
    if CANDIDATE is not None and CANDIDATE.exists():
        candidate = read(CANDIDATE)
        assert not candidate['meta']['dirty'] and not candidate['meta']['failures'] and not candidate['meta']['missing']
        assert len(candidate['results']) == 336 and all(r['checksum_ok'] for r in candidate['results'])
        indexed = {(r['program'], r['engine'], r['size'], r['variant']): r for r in candidate['results']}
        ratios = []
        for (p, e, n, v), row in indexed.items():
            if v == 'sakura':
                ratios.append((row['median_ms'] / indexed[p, e, n, 'js']['median_ms'], p, e, n, row['median_ms'], indexed[p, e, n, 'js']['median_ms']))
        largest = {(p, e): max(n for _r, q, f, n, _s, _j in ratios if (q, f) == (p, e)) for _r, p, e, _n, _s, _j in ratios}
        top = sorted((row for row in ratios if row[3] == largest[row[1], row[2]]), reverse=True)[:8]
        lines += [f'The [candidate JSON](../../../{CANDIDATE.relative_to(ROOT)}) '
                  f"is a single clean-tree full-suite invocation at `{candidate['meta']['commit']}`, after `5b10553`, with all 21 programs, four "
                  'sizes, two engines, both Sakura and hand-JS variants: 336 cells, 1,680 processes and '
                  '15,120 samples. All checksums passed. It remains **unblessed**. '
                  '[Human adoption instructions](../../../bench/baselines/candidate/README.md), '
                  '[load log](candidate-current-load.jsonl), [run output](candidate-current.log).', '',
                  'Largest Sakura/hand-JS ratios, restricting each program/engine to its largest input '
                  'so one program does not occupy all rows:', '',
                  '| Program | Engine | n | Sakura ms | Hand JS ms | Ratio |', '|---|---|---:|---:|---:|---:|']
        for ratio, program, engine, size, sakura, js in top:
            lines.append(f'| {program} | {engine} | {size} | {sakura:.4f} | {js:.4f} | {ratio:.2f}× |')
        loads = [json.loads(line) for line in (HERE / 'candidate-current-load.jsonl').read_text().splitlines()]
        lines += ['', f"Candidate one-minute load ranged {min(r['load_average_1_5_15'][0] for r in loads):.2f}–"
                  f"{max(r['load_average_1_5_15'][0] for r in loads):.2f}. This is a shared-host candidate, "
                  'not a quiet confirmation; the noise subset is not a repeated full-suite baseline. '
                  'No bottleneck or optimisation cause is inferred from a ratio alone.', '',
                  '**DeltaBlue fairness limitation:** the JS control asserts each destination after scale '
                  'and offset updates (projection tests 3/4); Sakura currently sums them without those '
                  'assertion loops. Aggregate oracles agree, but validation work is not identical. '
                  'WS-S already recorded this in `compiler/REQUESTS.md`. Treat the DeltaBlue ratios '
                  'as current-suite observations, not a fully matched-work competitiveness claim; '
                  'resolve or review the discrepancy before baseline adoption. The declared matched lane '
                  'also needs arithmetic-check review: hand-JS `fib` uses raw Number arithmetic over '
                  'the js-safe corpus. Oracle agreement alone does not certify overflow-check placement '
                  'or full §17.109 semantics matching.']
    else:
        lines += ['The current-algorithm candidate is pending; the pre-5b10553 full run is historical evidence, not an adoptable baseline.']
    lines += ['', '## Reproduce and validate', '', '```sh',
              'python3 research/10-evidence/analyze_noise.py research/10-evidence/ws-a/noise-[1-5].json --exclude-program sieve --json research/10-evidence/target/local-noise-summary.json',
              'python3 research/10-evidence/summarize.py --input research/10-evidence/ws-a/research-first-green-exact.json --output research/10-evidence/target/research-first-green-summary.md',
              'python3 research/10-evidence/ws-a/report.py', '```', '',
              'Use `record_load.py --log <new-log> -- <command>` for each fresh measurement. '
              'The logs retain exact invocations; use a new output directory rather than overwriting '
              'these records. [Validation record](validation.md) lists actual checks and limitations. '
              'The [delta ledger](../../../bench/deltas.md) makes no optimisation claim from this work.', '',
              'AI assistance: GPT-6-Astra authored the WS-A measurements, analysis, documentation and '
              'noise-dispatch workflow. The human contributor owns review; only the human can bless a baseline.']
    (HERE / 'README.md').write_text('\n'.join(lines) + '\n')


if __name__ == '__main__':
    main()
