"""Reproduce the RSS distribution report from the retained, untrimmed process peaks."""
import argparse
import itertools
import json
import statistics
from pathlib import Path

FIELDS = ('program', 'variant', 'engine', 'lane', 'ints', 'size')


def spread(values):
    lo, hi, med = min(values), max(values), statistics.median(values)
    return {'min': lo, 'median': med, 'max': hi,
            'width_over_median': (hi - lo) / med,
            'max_over_min_minus_one': hi / lo - 1}


def analyze(paths):
    docs = [json.loads(p.read_text()) for p in paths]
    first = docs[0]['meta']
    groups = {}
    for path, doc in zip(paths, docs):
        for field in ('commit', 'host', 'engines'):
            if doc['meta'][field] != first[field]:
                raise ValueError(f'mismatched {field}')
        if doc['meta']['missing'] or doc['meta']['failures']:
            raise ValueError(f'incomplete run: {path}')
        for field in ('suite', 'engines', 'lane', 'stable', 'processes'):
            if doc['meta']['flags'][field] != first['flags'][field]:
                raise ValueError(f'mismatched flag: {field}')
        seen = set()
        for row in doc['results']:
            key = tuple(row[f] for f in FIELDS)
            peaks = row['process_rss_bytes']
            if key in seen or not row['checksum_ok'] or len(peaks) != first['flags']['processes']:
                raise ValueError(f'failed/incomplete/duplicate cell: {key}')
            if len(row['samples_ms']) != 9 * len(peaks) or any(x <= 0 for x in peaks):
                raise ValueError(f'incomplete samples: {key}')
            seen.add(key)
            groups.setdefault(key, []).append({'file': path.name, 'peaks': peaks})
    cells = []
    for key, runs in sorted(groups.items()):
        maxima = [max(r['peaks']) for r in runs]
        medians = [statistics.median(r['peaks']) for r in runs]
        cells.append(dict(zip(FIELDS, key)) | {
            'runs': runs, 'individual_peaks': spread([v for r in runs for v in r['peaks']]),
            'run_maxima': spread(maxima), 'run_medians': spread(medians),
            'max_pairs_over_25pct': sum(b > a * 1.25 for a, b in itertools.permutations(maxima, 2)),
            'median_pairs_over_25pct': sum(b > a * 1.25 for a, b in itertools.permutations(medians, 2)),
            'ordered_pairs': len(runs) * (len(runs) - 1)})
    return {'commit': first['commit'], 'host': first['host'], 'engines': first['engines'],
            'files': [p.name for p in paths],
            'note': 'Empirical ranges; ordered pairs reuse observations and are not independent trials or false-alarm estimates. Follow-up coverage may be a selected subset.',
            'cells': cells}


if __name__ == '__main__':
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('files', nargs='+', type=Path)
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    args.output.write_text(json.dumps(analyze(args.files), indent=2) + '\n')
