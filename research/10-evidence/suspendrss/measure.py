#!/usr/bin/env python3
"""Five serial, interleaved processes per build/size/arm; keep every raw GC log."""
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
ARTIFACTS = ROOT / 'research/10-evidence/target/suspendrss'


def main():
    if (HERE / 'readings.json').exists() or (HERE / 'raw').exists():
        raise SystemExit('Refusing to overwrite retained evidence; use a new output directory.')
    result = {'schema': 'sakura-suspendrss-diagnostic-v1',
              'base': subprocess.check_output(['git', 'rev-parse', 'c4da994'], text=True).strip(),
              'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
              'node': subprocess.check_output(['node', '--version'], text=True).strip(),
              'platform': platform.platform(), 'logical_cpus': os.cpu_count(),
              'quiet': False, 'advisory': 'No owner-declared quiet window',
              'calls_per_process': 60, 'processes_per_cell': 5, 'runs': [], 'artifacts': {}}
    for side in ('base', 'head'):
        result['artifacts'][side] = {
            str(p.relative_to(ARTIFACTS / side)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted((ARTIFACTS / side).rglob('*')) if p.is_file()}
    logs = HERE / 'raw'
    logs.mkdir(exist_ok=True)
    for repetition in range(1, 6):
        for n in (100000, 1000000):
            for arm in ('default', 'cap8'):
                order = ('base', 'head') if repetition % 2 else ('head', 'base')
                for side in order:
                    name = f'{n}-{arm}-{repetition}-{side}'
                    path = ARTIFACTS / side / 'bench_entry.mjs'
                    command = [shutil.which('node'), '--expose-gc', '--trace-gc', '--trace-gc-verbose']
                    if arm == 'cap8':
                        command += ['--max-semi-space-size=8']
                    command += [str(HERE / 'probe.mjs'), str(path), str(n)]
                    start = time.monotonic()
                    load_before = os.getloadavg()
                    temporary = ARTIFACTS / f'{name}.log'
                    with temporary.open('wb') as output:
                        child = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT)
                        _, status, usage = os.wait4(child.pid, 0)
                        child.returncode = os.waitstatus_to_exitcode(status)
                    raw = temporary.read_bytes()
                    (logs / f'{name}.log.gz').write_bytes(gzip.compress(raw, mtime=0))
                    temporary.unlink()
                    probes = [json.loads(line[6:]) for line in raw.decode().splitlines() if line.startswith('PROBE ')]
                    record = {'name': name, 'side': side, 'n': n, 'arm': arm,
                              'repetition': repetition, 'command': command, 'exit_code': child.returncode,
                              'elapsed_seconds': time.monotonic() - start,
                              'load_before': load_before, 'load_after': os.getloadavg(),
                              'peak_rss_bytes': usage.ru_maxrss if platform.system() == 'Darwin' else usage.ru_maxrss * 1024,
                              'log': f'raw/{name}.log.gz', 'probes': probes}
                    result['runs'].append(record)
                    (HERE / 'readings.json').write_text(json.dumps(result, indent=2) + '\n')
                    assert child.returncode == 0 and len(probes) == 4, name
                    print(f'{name}: peak {record["peak_rss_bytes"]/1e6:.2f} MB, post-GC {probes[-1]["memory"]["heapUsed"]/1e6:.3f} MB', flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
