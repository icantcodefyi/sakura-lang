"""Check retained measurements and ensure the shipped gate still equals the maximum rule."""
import itertools
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[1] / 'harness'))
import bench
from analyze import analyze

paths = [HERE / name for name in ('suite-1.json', 'followup-2.json', 'followup-3.json')]
docs = [json.loads(p.read_text()) for p in paths]
assert analyze(paths) == json.loads((HERE / 'summary.json').read_text())
assert len(docs[0]['results']) == 336
assert all(len(d['results']) == 36 for d in docs[1:])
assert all(r['checksum_ok'] for d in docs for r in d['results'])
counts = {engine: {'ordered_pairs': 0, 'maximum_blocks': 0, 'median_crossings': 0} for engine in ('node', 'bun')}
for base, head in itertools.permutations(docs, 2):
    result = bench.compare(base, [head])
    legacy = json.loads(json.dumps([base, head]))
    for doc in legacy:
        for row in doc['results']:
            del row['process_rss_bytes']
    old = bench.compare(legacy[0], [legacy[1]])
    assert result['verdict'] == old['verdict']
    assert [(r['program'], r['variant'], r['engine'], r['size'], r['status']) for r in result['rss']] == [
        (r['program'], r['variant'], r['engine'], r['size'], r['status']) for r in old['rss']]
    for row in result['rss']:
        c = counts[row['engine']]
        c['ordered_pairs'] += 1
        c['maximum_blocks'] += row['status'] == 'blocked'
        c['median_crossings'] += row['head_median_bytes'] > row['base_median_bytes'] * 1.25
assert counts == {'node': {'ordered_pairs': 138, 'maximum_blocks': 2, 'median_crossings': 2},
                  'bun': {'ordered_pairs': 78, 'maximum_blocks': 3, 'median_crossings': 4}}
record = {'processes': sum(len(r['process_rss_bytes']) for d in docs for r in d['results']),
          'timing_batches': sum(len(r['samples_ms']) for d in docs for r in d['results']),
          'all_checksums_pass': True, 'summary_reproduced': True,
          'new_and_legacy_maximum_gate_verdicts_equal_on_all_six_ordered_run_pairs': True,
          'comparison_counts': counts}
(HERE / 'validation.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record, indent=2))
