#!/usr/bin/env python3
"""Recompute summaries from retained readings and unmodified verbose GC logs."""
import collections
import gzip
import json
from pathlib import Path
import re
import statistics

HERE = Path(__file__).resolve().parent


def main():
    readings = json.loads((HERE / 'readings.json').read_text())
    assert len(readings['runs']) == 40
    groups = collections.defaultdict(list)
    for run in readings['runs']:
        raw = gzip.decompress((HERE / run['log']).read_bytes()).decode()
        assert run['exit_code'] == 0
        assert [json.loads(x[6:]) for x in raw.splitlines() if x.startswith('PROBE ')] == run['probes']
        assert run['probes'][1]['calls'] == 60
        assert run['probes'][1]['checksum'] == {100000: '300001', 1000000: '2999999'}[run['n']]
        natural, forced = raw.split('PROBE {"phase":"before-forced-gc"', 1)
        spaces = {}
        for label in ('New space', 'Old space', 'Code space', 'Large object space', 'Trusted space', 'New large object space'):
            events = [tuple(map(int, m)) for m in re.findall(
                re.escape(label) + r',\s+used:\s+(\d+) KB,.*committed:\s+(\d+) KB', natural)]
            assert events, (run['name'], label)
            spaces[label] = {'max_used_KiB': max(x[0] for x in events),
                             'max_committed_KiB': max(x[1] for x in events),
                             'committed_levels_KiB': sorted({x[1] for x in events})}
        assert forced.count('Mark-Compact') == 2, run['name']
        record = {'name': run['name'], 'peak_rss_bytes': run['peak_rss_bytes'],
                  'post_gc_heap_used_bytes': run['probes'][-1]['memory']['heapUsed'],
                  'post_gc_external_bytes': run['probes'][-1]['memory']['external'],
                  'post_gc_array_buffers_bytes': run['probes'][-1]['memory']['arrayBuffers'],
                  'natural_mark_compact_count': natural.count('Mark-Compact'),
                  'natural_scavenge_count': natural.count('Scavenge'), 'spaces': spaces}
        groups[(run['n'], run['arm'], run['side'])].append(record)
    cells = []
    for (n, arm, side), runs in sorted(groups.items()):
        assert len(runs) == 5
        assert len({r['name'] for r in runs}) == 5
        cell = {'n': n, 'arm': arm, 'side': side, 'runs': runs}
        for key in ('peak_rss_bytes', 'post_gc_heap_used_bytes'):
            values = [r[key] for r in runs]
            cell[key] = {'min': min(values), 'max': max(values), 'median': statistics.median(values)}
        cells.append(cell)
    load = [json.loads(x) for x in (HERE / 'load.jsonl').read_text().splitlines()]
    assert load[-1]['exit_code'] == 0 and load[-1]['phase'] == 'after'
    summary = {'base': readings['base'], 'head': readings['head'], 'node': readings['node'],
               'load_1_min': min(x['load_average_1_5_15'][0] for x in load),
               'load_1_max': max(x['load_average_1_5_15'][0] for x in load),
               'load_snapshots': len(load), 'cells': cells}
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
