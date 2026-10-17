#!/usr/bin/env python3
"""Run paired, serial, interleaved processes. No third-party Python dependencies."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import re
import shutil
import statistics
import subprocess
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
CASES = ['fib', 'tail_sum', 'loop_sum', 'list_map_fold', 'records', 'strings', 'effects', 'contracts', 'contracts_plain']

def run(args, cwd=ROOT, timeout=120):
    return subprocess.run([str(a) for a in args], cwd=cwd, check=True, text=True,
                          capture_output=True, timeout=timeout)

def prepare():
    env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / 'compiler/target'))
    subprocess.run(['cargo', 'build', '--release', '-p', 'sakura_cli'], cwd=ROOT / 'compiler', env=env, check=True)
    run([ROOT / 'compiler/target/release/sakura', 'build', HERE / 'micro.sakura', '--out-dir', HERE / 'target/js'])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--prepare', action='store_true')
    parser.add_argument('--repetitions', type=int, default=5)
    parser.add_argument('--output', default='results.json')
    args = parser.parse_args()
    if args.prepare:
        prepare()
    engines = {key: shutil.which(key) for key in ['node', 'bun']}
    if not all(engines.values()):
        raise SystemExit('Both node and bun must be installed')
    metadata = {
        'date_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'commit': run(['git', 'rev-parse', 'HEAD']).stdout.strip(),
        'platform': platform.platform(), 'machine': platform.machine(),
        'cpu': run(['sysctl', '-n', 'machdep.cpu.brand_string']).stdout.strip(),
        'ram_bytes': int(run(['sysctl', '-n', 'hw.memsize']).stdout),
        'engines': {k: {'path': v, 'version': run([v, '--version']).stdout.strip()} for k, v in engines.items()},
        'rustc': run(['rustc', '-Vv'], cwd=ROOT / 'compiler').stdout,
        'seed': 20261018, 'repetitions': args.repetitions,
        'method': '250ms warmup, power-of-two calibration >=30ms, 9 samples/process, alternating n..n+3, median of process medians',
        'sha256': {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in [
            HERE / 'micro.sakura', HERE / 'bench.mjs', HERE / 'reference.mjs', HERE / 'native.mjs',
            HERE / 'target/js/main/micro.js', HERE / 'target/js/sakura_runtime.js']},
    }
    rows = []
    rng = random.Random(metadata['seed'])
    out = HERE / args.output
    for repetition in range(args.repetitions):
        jobs = [(engine, case, variant) for engine in engines for case in CASES for variant in ['sakura', 'js', 'checked']]
        rng.shuffle(jobs)
        for engine, case, variant in jobs:
            command = [engines[engine], HERE / 'bench.mjs', case, variant]
            # Darwin /usr/bin/time -l reports maximum RSS in bytes; include whole VM process.
            proc = run(['/usr/bin/time', '-l', *command], timeout=60)
            row = json.loads(proc.stdout)
            rss = re.search(r'(\d+)\s+maximum resident set size', proc.stderr)
            row.update(engine_name=engine, repetition=repetition,
                       max_rss_bytes=int(rss.group(1)) if rss else None,
                       median_ms=statistics.median(row['samples_ms']))
            rows.append(row)
            out.write_text(json.dumps({'metadata': metadata, 'rows': rows}, indent=2) + '\n')
        print(f'completed repetition {repetition + 1}/{args.repetitions}', flush=True)
    # Whole CLI latency on this small corpus; file cache is warm, no persistent server.
    cli = ROOT / 'compiler/target/release/sakura'
    latency = {}
    for operation in ['check', 'build']:
        command = [cli, operation, HERE / 'micro.sakura']
        if operation == 'build': command += ['--out-dir', HERE / 'target/latency']
        run(command)
        samples = []
        for _ in range(15):
            start = time.perf_counter(); run(command); samples.append((time.perf_counter() - start) * 1000)
        latency[operation] = samples
    out.write_text(json.dumps({'metadata': metadata, 'rows': rows, 'cli_ms': latency}, indent=2) + '\n')
    print_table(rows)


def print_table(rows):
    print('| Workload | Node Sakura ms | Node / JS | Node / checked | Bun Sakura ms | Bun / JS | Bun / checked |')
    print('|---|---:|---:|---:|---:|---:|---:|')
    for case in CASES:
        cells = []
        for engine in ['node', 'bun']:
            med = {v: statistics.median(r['median_ms'] for r in rows if r['name'] == case and r['engine_name'] == engine and r['variant'] == v) for v in ['sakura','js','checked']}
            cells += [f"{med['sakura']:.3f}", f"{med['sakura']/med['js']:.2f}×", f"{med['sakura']/med['checked']:.2f}×"]
        print('| ' + ' | '.join([case, *cells]) + ' |')

if __name__ == '__main__':
    main()
