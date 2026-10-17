#!/usr/bin/env python3
"""Recompute the report's table and preserve process-level spread (not a CI)."""
import json
import statistics as st
from pathlib import Path
from measure import CASES, HERE

def summary():
    data = json.loads((HERE / 'results.json').read_text())
    rows = data['rows']
    assert len(rows) == 270
    assert all(len(r['samples_ms']) == 9 and r['count'] >= 4 for r in rows)
    index = {}
    for engine in ['node', 'bun']:
        for case in CASES:
            for variant in ['sakura', 'js', 'checked']:
                group = [r for r in rows if (r['engine_name'], r['name'], r['variant']) == (engine, case, variant)]
                assert len(group) == 5 and sorted(r['repetition'] for r in group) == list(range(5))
                ms = [st.median(r['samples_ms']) for r in group]
                index[engine, case, variant] = {
                    'median_ms': st.median(ms), 'min_process_ms': min(ms), 'max_process_ms': max(ms),
                    'process_medians_ms': ms,
                    'median_max_rss_mib': st.median(r['max_rss_bytes'] for r in group) / 2**20,
                }
    table = ['| Workload | Node Sakura ms | Node / JS | Node / checked | Bun Sakura ms | Bun / JS | Bun / checked |',
             '|---|---:|---:|---:|---:|---:|---:|']
    for case in CASES:
        cells = []
        for engine in ['node', 'bun']:
            v = {k: index[engine, case, k]['median_ms'] for k in ['sakura', 'js', 'checked']}
            cells += [f"{v['sakura']:.3f}", f"{v['sakura']/v['js']:.2f}×", f"{v['sakura']/v['checked']:.2f}×"]
        table.append('| ' + ' | '.join([case, *cells]) + ' |')
    table = '\n'.join(table)
    lines = ['# Process-level summary', '', 'Generated from `results.json` by `python3 research/10-evidence/summarize.py`.', '', table, '',
             'The range below is min–max of five process medians, **not a confidence interval**.',
             'RSS is median of five whole-process peaks, including warmup and common imported harness modules.', '',
             '| Engine | Workload | Variant | Median ms | Process range ms | Peak RSS MiB |', '|---|---|---|---:|---:|---:|']
    for (engine, case, variant), v in index.items():
        lines.append(f"| {engine} | {case} | {variant} | {v['median_ms']:.6f} | {v['min_process_ms']:.6f}–{v['max_process_ms']:.6f} | {v['median_max_rss_mib']:.1f} |")
    lines += ['', '## Whole CLI latency', '', '15 new processes per operation after one warmup; warm filesystem cache; no persistent analysis service.', '']
    for operation, samples in data['cli_ms'].items():
        lines.append(f'- `{operation}` median {st.median(samples):.2f} ms; min–max {min(samples):.2f}–{max(samples):.2f} ms.')
    (HERE / 'summary.md').write_text('\n'.join(lines) + '\n')
    print(table)
    for engine in ['node','bun']:
        c=index[engine,'contracts','sakura']['median_ms'];p=index[engine,'contracts_plain','sakura']['median_ms']
        print(f'{engine}: contracts/plain {c/p:.2f}x')
    return table

if __name__ == '__main__':
    summary()
