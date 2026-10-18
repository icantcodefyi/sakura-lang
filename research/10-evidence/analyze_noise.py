#!/usr/bin/env python3
"""Describe repeated fresh-process harness runs; never treat batches as independent runs."""
import argparse
import itertools
import json
from pathlib import Path
import statistics


def ratio_rows(document):
    if document['schema'] != 'sakura-bench-v1':
        raise ValueError('unsupported result schema')
    if document['meta']['dirty'] or document['meta']['failures'] or document['meta']['missing']:
        raise ValueError('incomplete run')
    rows = {}
    for row in document['results']:
        if (not row['checksum_ok']
                or len(row['process_medians_ms']) != document['meta']['flags']['processes']
                or len(row['samples_ms']) != 9 * document['meta']['flags']['processes']):
            raise ValueError('failed or incomplete timing row')
        key = (row['program'], row['engine'], row['lane'], row['ints'], row['size'])
        variants = rows.setdefault(key, {})
        if row['variant'] in variants:
            raise ValueError('duplicate timing cell')
        variants[row['variant']] = row['median_ms']
    ratios = {k: (v['sakura'] / v['js'], v['sakura'], v['js']) for k, v in rows.items()
              if 'sakura' in v and 'js' in v}
    if not ratios:
        raise ValueError('no Sakura/hand-JS ratio cells')
    return ratios


def analyze(paths, excluded_programs=()):
    documents = [json.loads(path.read_text()) for path in paths]
    first = documents[0]['meta']
    for document in documents:
        current = document['meta']
        for field in ('commit', 'host', 'engines'):
            if current[field] != first[field]:
                raise ValueError(f'mismatched {field}; do not pool machines, pins or commits')
        for field in ('suite', 'engines', 'lane', 'sizes', 'stable', 'filter', 'processes'):
            if current['flags'][field] != first['flags'][field]:
                raise ValueError(f'mismatched flags.{field}')
    runs = [ratio_rows(d) for d in documents]
    if any(set(run) != set(runs[0]) for run in runs):
        raise ValueError('different coverage')
    rows = []
    for key in sorted(runs[0]):
        if key[0] in excluded_programs:
            continue
        values = [run[key][0] for run in runs]
        median = statistics.median(values)
        pairwise = [(b[0] / a[0] - 1, b[1] - a[0] * b[2])
                    for a, b in itertools.permutations([run[key] for run in runs], 2)]
        rows.append(dict(zip(('program', 'engine', 'lane', 'ints', 'size'), key)) | {
            'ratios': values, 'median_ratio': median, 'min_ratio': min(values), 'max_ratio': max(values),
            'width_over_median': (max(values) - min(values)) / median,
            'max_increase_over_min': max(values) / min(values) - 1,
            'ordered_pairs': len(pairwise),
            'pairs_above_5_percent': sum(delta > .05 for delta, _ in pairwise),
            'pairs_above_10_percent': sum(delta > .10 for delta, _ in pairwise),
            'pairs_above_10_percent_and_0_1_ms': sum(delta > .10 and absolute >= .1 for delta, absolute in pairwise)})
    return {'commit': first['commit'], 'host': first['host'], 'engines': first['engines'],
            'flags': first['flags'], 'files': [str(p) for p in paths], 'run_count': len(runs), 'excluded_programs': sorted(excluded_programs),
            'method': 'ratio of median-of-process-medians per run; empirical min/max across runs; ordered pairs are descriptive, not independent trials or a false-positive rate',
            'rows': rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('runs', type=Path, nargs='+')
    parser.add_argument('--json', type=Path, required=True)
    parser.add_argument('--exclude-program', action='append', default=[], help='omit an explicitly incompatible workload; keep raw evidence unchanged')
    args = parser.parse_args()
    if len(args.runs) < 2:
        parser.error('at least two separate runs are required')
    result = analyze(args.runs, args.exclude_program)
    args.json.write_text(json.dumps(result, indent=2) + '\n')
    print('| Program | Engine | n | Median ratio | Min–max | Width / median | Max increase |')
    print('|---|---|---:|---:|---:|---:|---:|')
    for row in result['rows']:
        print(f"| {row['program']} | {row['engine']} | {row['size']} | {row['median_ratio']:.3f} | "
              f"{row['min_ratio']:.3f}–{row['max_ratio']:.3f} | {row['width_over_median']:.1%} | {row['max_increase_over_min']:.1%} |")


if __name__ == '__main__':
    main()
