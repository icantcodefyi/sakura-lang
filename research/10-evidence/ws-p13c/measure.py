import datetime
import hashlib
import json
import os
import platform
import statistics
import subprocess
import sys
import tomllib
from pathlib import Path

repo = Path.cwd()
root = Path(sys.argv[1]).resolve()
runner = repo / 'bench/harness/runner.mjs'
variants = {arm: root / arm / 'bench_entry.mjs' for arm in ['base', 'owned', 'flat']}
variants['js'] = repo / 'bench/programs/deltablue/js/main.mjs'
checks = tomllib.loads((repo / 'bench/programs/deltablue/meta.toml').read_text())['checksums']
data = {'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        'system': platform.platform(), 'cpus': os.cpu_count(), 'quiet': False,
        'versions': {e: subprocess.check_output([e, '--version'], text=True).strip() for e in ['node', 'bun']},
        'sha256': {arm: {str(p.relative_to(root / arm)): hashlib.sha256(p.read_bytes()).hexdigest()
                         for p in sorted((root / arm).rglob('*')) if p.is_file()}
                   for arm in ['base', 'owned', 'flat']},
        'oracles': [], 'runs': [], 'failures': []}

def command(args):
    result = subprocess.run([str(a) for a in args], capture_output=True, text=True, timeout=180)
    assert result.returncode == 0, (args, result.returncode, result.stderr)
    return json.loads(result.stdout)

try:
    for engine in ['node', 'bun']:
        for arm, path in variants.items():
            result = command([engine, runner, 'values', path, *checks.keys()])
            assert result['values'] == checks, (engine, arm, result)
            data['oracles'].append({'engine': engine, 'arm': arm, **result})
            print('oracle', engine, arm, 'PASS', flush=True)
    for rnd in range(8):
        names = list(variants)
        names = names[rnd % 4:] + names[:rnd % 4]
        for arm in names:
            before = os.getloadavg()
            result = command(['node', runner, 'measure', variants[arm], 5000])
            assert result['checksum'] == checks['5000'], (arm, result)
            data['runs'].append({'round': rnd, 'arm': arm, 'engine': 'node',
                                 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                                 'load_before': before, 'load_after': os.getloadavg(), **result})
            (root / 'readings.json').write_text(json.dumps(data, indent=2) + '\n')
            print(rnd, arm, statistics.median(result['samples_ms']), flush=True)
except BaseException as error:
    data['failures'].append(repr(error))
    raise
finally:
    (root / 'readings.json').write_text(json.dumps(data, indent=2) + '\n')
