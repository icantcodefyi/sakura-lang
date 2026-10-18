"""Five more fresh processes per preselected cell, using the pilot's exact built modules.

Selection: all cells whose pilot max/min exceeds 1.25, plus every shapes cell.
Selection is frozen from the pilot; never repeat until a gate passes. Run twice with
seeds 20261019 and 20261020. No compilation or flags affecting GC/timing are changed.
"""
import argparse
import hashlib
import json
import random
import statistics
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'harness'))
import bench


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('pilot', type=Path)
    ap.add_argument('--seed', type=int, required=True)
    ap.add_argument('--json', type=Path, required=True)
    args = ap.parse_args()
    pilot = json.loads(args.pilot.read_text())
    rows = [r for r in pilot['results'] if r['program'] == 'shapes'
            or max(r['process_rss_bytes']) > min(r['process_rss_bytes']) * 1.25]
    flags = pilot['meta']['flags'] | {'filter': 'pilot-volatile-plus-shapes', 'seed': args.seed}
    result = bench.new_result(flags)
    if result['meta']['commit'] != pilot['meta']['commit']:
        raise ValueError('source commit changed')
    saved = json.loads((args.pilot.parent / 'artifacts.json').read_text())
    for name, digest in saved.items():
        if hashlib.sha256((bench.ROOT / name).read_bytes()).hexdigest() != digest:
            raise ValueError(f'artefact changed: {name}')
    jobs = [(i, repetition) for i in range(len(rows)) for repetition in range(flags['processes'])]
    random.Random(args.seed).shuffle(jobs)
    measured = [[] for _ in rows]
    for k, (i, _) in enumerate(jobs, 1):
        row = rows[i]
        p, v, e, n = [row[f] for f in ('program', 'variant', 'engine', 'size')]
        art = bench.TARGET / 'sakura' / p / 'bench_entry.mjs' if v == 'sakura' else bench.PROGRAMS / p / 'js' / 'main.mjs'
        bench.log(f'[{k}/{len(jobs)}] {p} {v}/{e} n={n}')
        samples, checksum, rss = bench.measure(v, e, art, n, flags['stable'])
        measured[i].append((samples, checksum, rss))
    for row, runs in zip(rows, measured):
        medians = [statistics.median(s) for s, _, _ in runs]
        result['results'].append({f: row[f] for f in ('program', 'variant', 'engine', 'lane', 'ints', 'size')} | {
            'samples_ms': [x for s, _, _ in runs for x in s], 'process_medians_ms': medians,
            'median_ms': statistics.median(medians), 'rss_bytes': max(r for _, _, r in runs),
            'process_rss_bytes': [r for _, _, r in runs],
            'checksum': next((c for _, c, _ in runs if c != row['checksum']), row['checksum']),
            'checksum_ok': all(c == row['checksum'] for _, c, _ in runs)})
    args.json.write_text(json.dumps(result, indent=1) + '\n')
    return int(any(not r['checksum_ok'] for r in result['results']))


if __name__ == '__main__':
    raise SystemExit(main())
