#!/usr/bin/env python3
"""Build and check WS-S ports against WS-B metadata. This does not publish timings."""
import argparse
import fnmatch
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[4]
PROGRAMS = ROOT / 'bench/programs'
TARGET = ROOT / 'compiler' / os.environ.get('CARGO_TARGET_DIR', 'target') / 'ports'


def execute(command, **kwargs):
    result = subprocess.run(list(map(str, command)), capture_output=True, text=True,
                            timeout=600, **kwargs)
    if result.returncode:
        raise RuntimeError(f'{command}: exit {result.returncode}\n{result.stdout[-2000:]}\n{result.stderr[-2000:]}')
    return result.stdout


def build(program, language, lane, reuse):
    output = TARGET / 'bin' / f'{language}-{lane}-{reuse}-{program.name}'
    output.parent.mkdir(parents=True, exist_ok=True)
    if language == 'rust':
        env = dict(os.environ)
        env['CARGO_TARGET_DIR'] = str(TARGET / f'rust-{lane}')
        env['CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS'] = 'true' if lane == 'matched' else 'false'
        env['RUSTFLAGS'] = '-C overflow-checks=' + ('on' if lane == 'matched' else 'off')
        execute(['cargo', 'build', '--release', '--locked', '--offline', '--manifest-path',
                 program / 'rust/Cargo.toml'], env=env)
        shutil.copy2(Path(env['CARGO_TARGET_DIR']) / 'release/main', output)
    elif language == 'go':
        execute(['go', 'build', '-o', output, '.'], cwd=program / 'go')
    elif language == 'ocaml':
        work = TARGET / 'ocaml-build' / program.name
        work.mkdir(parents=True, exist_ok=True)
        for path in (PROGRAMS / '_shared/ocaml').glob('*.ml'):
            shutil.copy2(path, work / path.name)
        shutil.copy2(program / 'ocaml/main.ml', work / 'main.ml')
        execute(['ocamlopt', '-I', '+unix', 'unix.cmxa', 'runner.ml', 'kernels.ml',
                 'main.ml', '-o', output], cwd=work)
    elif language == 'koka':
        flags = ['--fno-reuse'] if reuse == 'no-reuse' else []
        # Compile from the entry directory: Koka interprets '_' in module paths as '/'.
        execute(['koka', '-v0', '-O2', '--target=c', '--stack=67108864', *flags,
                 f'--builddir={TARGET / ("koka-" + reuse)}',
                 f'--include={PROGRAMS / "_shared/koka"}', '-o', output, 'main.kk'],
                cwd=program / 'koka')
    elif language == 'python':
        return ['python3', program / 'python/main.py']
    elif language == 'typescript':
        return ['bun', program / 'typescript/main.ts']
    return [output]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--language', required=True,
                        choices=['rust', 'go', 'python', 'typescript', 'ocaml', 'koka'])
    parser.add_argument('--lane', choices=['matched', 'idiomatic'], default='matched')
    parser.add_argument('--reuse', choices=['reuse', 'no-reuse'], default='reuse')
    parser.add_argument('--filter', default='*')
    options = parser.parse_args()
    allocation = {'binary_trees', 'pipeline', 'rbtree', 'map_set', 'shapes'}
    entries = sorted(p for p in PROGRAMS.iterdir()
                     if (p / 'meta.toml').exists() and fnmatch.fnmatch(p.name, options.filter))
    checked = 0
    missing = []
    for program in entries:
        if options.language == 'ocaml' and program.name not in allocation:
            continue
        if options.language == 'koka' and program.name not in allocation and not program.name.startswith('handled_'):
            continue
        suffix = {'rust': 'rs', 'go': 'go', 'python': 'py', 'typescript': 'ts',
                  'ocaml': 'ml', 'koka': 'kk'}[options.language]
        if not (program / options.language / f'main.{suffix}').exists():
            port = f'{program.name}/{options.language}'
            missing.append(port)
            print(f'skip: missing port {port}', flush=True)
            continue
        metadata = tomllib.loads((program / 'meta.toml').read_text())
        command = build(program, options.language, options.lane, options.reuse)
        expected = {size: checksum for size, checksum in metadata['checksums'].items()
                    if size not in metadata.get('cases', [])}
        values = json.loads(execute([*command, 'values', *expected]))['values']
        if values != expected:
            raise RuntimeError(f'{program.name}: {values} != {expected}')
        # One real sample invocation per entry validates the printer and consumes its checksum.
        size = metadata['sizes'][0]
        samples = [json.loads(line) for line in execute([*command, 'measure', size]).splitlines()]
        if len(samples) != 9 or any(s['n'] != size or s['checksum'] != expected[str(size)] or
                                    not isinstance(s['ms'], (int, float)) or not 0 <= s['ms'] < float('inf')
                                    for s in samples):
            raise RuntimeError(f'{program.name}: invalid samples {samples}')
        invalid = subprocess.run(list(map(str, [*command, 'values', -1])), capture_output=True, text=True, timeout=30)
        if invalid.returncode == 0 or invalid.stdout:
            raise RuntimeError(f'{program.name}: invalid size accepted or wrote stdout')
        checked += 1
        print(f'{options.language}/{options.lane}/{options.reuse} {program.name}: {len(values)} oracles, 9 samples, bounds OK', flush=True)
    if missing:
        raise SystemExit('missing ports: ' + ', '.join(missing))
    if checked == 0:
        raise RuntimeError('no ports selected')


if __name__ == '__main__':
    main()
