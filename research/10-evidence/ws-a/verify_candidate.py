#!/usr/bin/env python3
"""Verify candidate coverage, pins and statistics against its recorded source commit.

This verifies recorded measurement integrity, not full semantic matching, quiet host
conditions, performance targets or human baseline approval. It never calls bless.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import statistics
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[3]


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True).strip()


def verify(path):
    raw = path.read_bytes()
    document = json.loads(raw)
    assert document['schema'] == 'sakura-bench-v1'
    meta = document['meta']
    commit = meta['commit']
    assert re.fullmatch('[0-9a-f]{40}', commit)
    subprocess.run(['git', 'merge-base', '--is-ancestor', '5b10553', commit], cwd=ROOT, check=True)
    assert not meta['dirty'] and not meta['failures'] and not meta['missing']
    flags = meta['flags']
    assert flags['suite'] == 'js' and flags['lane'] == 'matched' and flags['sizes'] == 'all'
    assert flags['processes'] == 5 and not flags['stable'] and flags['filter'] == '*'
    assert sorted(flags['engines']) == ['bun', 'node']
    pins = tomllib.loads(git('show', f'{commit}:bench/toolchains.toml'))
    assert all(meta['engines'][engine] == pins[engine] for engine in ('node', 'bun'))
    source_paths = git('ls-tree', '-r', '--name-only', commit, 'bench/programs').splitlines()
    definitions = {Path(source).parent.name: tomllib.loads(git('show', f'{commit}:{source}'))
                   for source in source_paths if source.endswith('/meta.toml')}
    expected = {(program, variant, engine, size) for program, definition in definitions.items()
                for variant in ('sakura', 'js') for engine in ('node', 'bun') for size in definition['sizes']}
    seen = set()
    for row in document['results']:
        key = (row['program'], row['variant'], row['engine'], row['size'])
        assert key not in seen
        seen.add(key)
        definition = definitions[row['program']]
        assert row['lane'] == 'matched' and row['ints'] == definition['ints']
        assert row['checksum_ok'] and row['checksum'] == definition['checksums'][str(row['size'])]
        samples = row['samples_ms']
        assert len(samples) == 45 and all(math.isfinite(x) and x > 0 for x in samples)
        medians = [statistics.median(samples[start:start + 9]) for start in range(0, 45, 9)]
        assert row['process_medians_ms'] == medians
        assert row['median_ms'] == statistics.median(medians)
        assert row['rss_bytes'] > 0
    assert seen == expected
    assert len(definitions) == 21 and len(seen) == 336
    return {'commit': commit, 'host_id': meta['host']['id'], 'programs': len(definitions),
            'cells': len(seen), 'processes': len(seen) * 5, 'samples': len(seen) * 45,
            'sha256': hashlib.sha256(raw).hexdigest()}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('result', type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.result), indent=2))
