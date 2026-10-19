import json, os, statistics, subprocess, sys, datetime
from pathlib import Path
repo = Path(sys.argv[1]); root = Path('/tmp/wsp13b-bench')
runner = repo / 'bench/harness/runner.mjs'
arms = ['base', 'after', 'owned']
runs = []
for rnd in range(41):
    order = arms[rnd % 3:] + arms[:rnd % 3]
    for arm in order:
        before = os.getloadavg()[0]
        r = subprocess.run(['node', str(runner), 'measure', str(root / arm / 'bench_entry.mjs'), '5000'],
                           capture_output=True, text=True, timeout=300)
        assert r.returncode == 0, r.stderr
        res = json.loads(r.stdout)
        assert res['checksum'] == '139986176', res
        med = statistics.median(res['samples_ms'])
        runs.append({'round': rnd, 'arm': arm, 'median_ms': med, 'samples_ms': res['samples_ms'],
                     'load1_before': before, 'load1_after': os.getloadavg()[0],
                     'utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
        print(rnd, arm, round(med, 3), round(before, 1), flush=True)
(root / 'readings.json').write_text(json.dumps(runs, indent=1))
for arm in arms:
    ms = [r['median_ms'] for r in runs if r['arm'] == arm]
    print(arm, 'median', round(statistics.median(ms), 3), 'min', round(min(ms), 3), 'max', round(max(ms), 3))
