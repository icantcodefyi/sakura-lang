#!/usr/bin/env python3
"""Trusted fixture validation only; no model API, agent runner or security sandbox."""
import argparse
import collections
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SUITE = ROOT / 'evals/agent-dx'
CATEGORIES = {'syntax-data', 'diagnose-repair', 'holes-retrieval',
              'application-change', 'deterministic-effects', 'preserve-assurance'}


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def validate(value, schema, location='$'):
    """Validate the deliberately small JSON Schema subset used by task.schema.json."""
    supported = {'$schema', 'title', 'description', 'type', 'const', 'enum',
                 'properties', 'required', 'additionalProperties', 'items',
                 'minItems', 'maxItems', 'uniqueItems', 'minimum', 'maximum',
                 'minLength', 'pattern'}
    require(not set(schema) - supported, f'{location}: unsupported schema keyword')
    types = {'object': dict, 'array': list, 'string': str, 'integer': int, 'boolean': bool}
    if 'type' in schema:
        require(type(value) is types[schema['type']], f'{location}: wrong type')
    if 'const' in schema:
        require(json.dumps(value, sort_keys=True) == json.dumps(schema['const'], sort_keys=True),
                f'{location}: wrong constant')
    if 'enum' in schema:
        require(value in schema['enum'], f'{location}: unknown value {value!r}')
    if isinstance(value, dict):
        require(set(schema.get('required', [])) <= value.keys(), f'{location}: missing fields')
        properties = schema.get('properties', {})
        extra = set(value) - properties.keys()
        additional = schema.get('additionalProperties', True)
        require(additional is not False or not extra, f'{location}: unknown fields {extra}')
        for key, item in value.items():
            child = properties.get(key, additional)
            if isinstance(child, dict):
                validate(item, child, f'{location}.{key}')
    if isinstance(value, list):
        require(len(value) >= schema.get('minItems', 0), f'{location}: too few items')
        require(len(value) <= schema.get('maxItems', len(value)), f'{location}: too many items')
        if schema.get('uniqueItems'):
            require(len({json.dumps(v, sort_keys=True) for v in value}) == len(value),
                    f'{location}: duplicates')
        for i, item in enumerate(value):
            validate(item, schema.get('items', {}), f'{location}[{i}]')
    if type(value) is int:
        require(schema.get('minimum', value) <= value <= schema.get('maximum', value),
                f'{location}: outside bounds')
    if isinstance(value, str):
        require(len(value) >= schema.get('minLength', 0), f'{location}: too short')
        require(re.search(schema.get('pattern', ''), value) is not None,
                f'{location}: pattern mismatch')


def files(directory):
    result = {}
    for path in sorted(directory.rglob('*')):
        require(not path.is_symlink(), f'symlink forbidden: {path}')
        if path.is_file() and 'target' not in path.relative_to(directory).parts:
            result[path.relative_to(directory).as_posix()] = path.read_bytes()
    return result


def digest(data):
    return hashlib.sha256(data).hexdigest()


def starter_hashes(directory):
    hashes = {name: digest(data) for name, data in files(directory).items()}
    aggregate = digest(''.join(f'{name}\0{sha}\n' for name, sha in sorted(hashes.items())).encode())
    return hashes, aggregate


def run(args, cwd, timeout=30):
    env = {key: os.environ[key] for key in ('PATH', 'TMPDIR', 'SYSTEMROOT') if key in os.environ}
    with subprocess.Popen(args, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          text=True, start_new_session=True) as process:
        try:
            stdout, stderr = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.communicate()
            raise AssertionError(f'tool timeout after {timeout}s: {args}') from None
    return process.returncode, stdout, stderr


def records(result):
    code, stdout, stderr = result
    rows = [json.loads(line) for line in stdout.splitlines()]
    require(rows and rows[-1].get('kind') == 'summary', f'no summary: {code}, {stdout}, {stderr}')
    require(all(row.get('schema_version') == '1.0' for row in rows), 'unknown diagnostic schema')
    return rows


def git(directory, *args):
    result = run(['git', '-c', 'user.name=Offline eval', '-c', 'user.email=eval@invalid',
                  '-c', 'commit.gpgsign=false', *args], directory)
    require(result[0] == 0, f'git failed: {result}')
    return result[1].strip()


def baseline(task, directory):
    for name, data in files(task / 'starter').items():
        path = directory / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    git(directory, 'init', '-q', '--initial-branch=eval-baseline')
    git(directory, 'add', '.')
    git(directory, 'commit', '-qm', 'Frozen starter')
    return git(directory, 'rev-parse', 'HEAD')


def guard(task, candidate, metadata):
    original = files(task / 'starter')
    require(set(candidate) == set(original), 'candidate file inventory changed')
    for name, content in original.items():
        if name not in metadata['editable_files']:
            require(candidate[name] == content, f'immutable file changed: {name}')
    source = candidate['src/main.sak'].decode()
    require(not re.search(r'@trust\b|@external\b|\.host\s*\(', source),
            'trust, externals and host handlers forbidden in this pilot')


def materialize(directory, candidate):
    for name, data in candidate.items():
        (directory / name).write_bytes(data)


def checked(binary, directory, commit):
    result = run([binary, 'check', '--json', '--frozen-specs', 'all', '--since', commit], directory)
    rows = records(result)
    require(result[0] == 0 and rows[-1].get('holes') == 0,
            f'candidate check/frozen/holes rejected: {result}')
    require(not any(row.get('code') for row in rows), f'candidate diagnostics: {rows}')


def oracle(binary, directory, task, name, candidate):
    source = candidate['src/main.sak'] + b'\n' + (task / name).read_bytes()
    (directory / 'src/main.sak').write_bytes(source)
    check = run([binary, 'check', '--json'], directory)
    require(check[0] == 0 and records(check)[-1].get('holes') == 0,
            f'oracle must compile before behavioral acceptance: {check}')
    result = run([binary, 'test', '--json', '--filter', 'oracle/'], directory)
    rows = records(result)
    tests = [row for row in rows if row['kind'] == 'test']
    require(len(tests) == 1 and rows[-1]['total'] >= 2
            and rows[-1]['skipped'] == rows[-1]['total'] - 1
            and rows[-1]['passed'] + rows[-1]['failed'] == 1,
            f'each oracle must execute exactly one test: {rows}')
    require(tests[0]['name'] == f'oracle/{Path(name).stem}', 'unexpected oracle test')
    require(tests[0]['status'] in ('passed', 'failed', 'defect'), f'unexpected test status: {rows}')
    if tests[0]['status'] == 'defect':
        require(task.name == '11-successor-contract' and tests[0]['defect']['code'] == 'D021',
                f'unexpected oracle defect: {rows}')
    passed = tests[0]['status'] == 'passed'
    require(result[0] == (0 if passed else 3), f'inconsistent test exit: {result}')
    return passed


def rejected(operation, reason):
    try:
        operation()
    except AssertionError as error:
        require(reason in str(error), f'wrong rejection (wanted {reason}): {error}')
        return
    raise AssertionError(f'negative control was accepted: {reason}')


def validate_task(task, metadata, schema):
    validate(metadata, schema, task.name)
    require(metadata['id'] == task.name, 'directory/id mismatch')
    expected_languages = ['typescript', 'python'] if metadata['comparison']['portable'] else []
    require(metadata['comparison']['languages'] == expected_languages,
            'comparison languages disagree with eligibility')
    hashes, aggregate = starter_hashes(task / 'starter')
    require(metadata['starter_files'] == hashes, f'{task.name}: starter file hash drift')
    require(metadata['starter_sha256'] == aggregate, f'{task.name}: starter tree hash drift')
    require(set(hashes) == {'sakura.toml', 'src/main.sak'}, 'unexpected starter files')
    require(0 < (task / 'prompt.md').stat().st_size <= 2400, 'prompt exceeds compact pilot size')
    require(len((task / 'starter/src/main.sak').read_text().splitlines()) <= 80,
            'starter exceeds compact pilot size')
    require(set(metadata['oracle']['files']) == {'oracle/' + p for p in files(task / 'oracle')},
            'missing or unlisted oracle')
    for name in metadata['oracle']['files']:
        text = (task / name).read_text()
        require(text.count('test "') == 1 and f'test "oracle/{Path(name).stem}"' in text,
                'oracle must contain one named test')
    for variant in ('gold', 'hollow'):
        guard(task, files(task / variant), metadata)


def exercise(task, metadata, binary, scratch):
    directory = scratch / task.name
    directory.mkdir()
    commit = baseline(task, directory)
    initial = run([binary, 'check', '--json'], directory)
    rows = records(initial)
    codes = sorted({row['code'] for row in rows if 'code' in row})
    require(codes == sorted(metadata['expected_diagnostics']), f'initial codes: {codes}')
    require(initial[0] == (1 if codes else 0), f'initial check exit: {initial}')
    require(rows[-1].get('holes') == metadata['expected_holes'], f'initial holes: {rows[-1]}')
    results = {}
    for variant in ('gold', 'hollow'):
        candidate = files(task / variant)
        materialize(directory, candidate)
        checked(binary, directory, commit)
        if variant == 'gold':
            visible = run([binary, 'test', '--json'], directory)
            require(visible[0] == 0 and records(visible)[-1]['passed'] >= 1,
                    f'gold visible tests failed: {visible}')
        results[variant] = [oracle(binary, directory, task, name, candidate)
                            for name in metadata['oracle']['files']]
        require(all(p == (variant == 'gold') for p in results[variant]),
                f'{variant} oracle outcomes: {results[variant]}')
    gold = files(task / 'gold')
    altered = dict(gold)
    altered['sakura.toml'] += b'net = ["*"]\n'
    rejected(lambda: guard(task, altered, metadata), 'immutable file')
    altered = dict(gold)
    altered['src/main.sak'] += b'\nfn bypass() -> Int { Untrusted.trust(Untrusted.of(1), @trust("bypass")) }\n'
    rejected(lambda: guard(task, altered, metadata), 'trust, externals')
    # These are valid candidates as far as the path/permission guard is concerned;
    # the compiler must enforce frozen tests/contracts and unfinished holes.
    for label, transform, reason in (
        ('removed-test', lambda s: s[:s.index('test "visible example"')], 'V8020'),
        ('unfinished-hole', lambda s: s + '\nfn unfinished() -> Int { ?body }\n', 'holes rejected'),
    ):
        altered = dict(gold)
        altered['src/main.sak'] = transform(gold['src/main.sak'].decode()).encode()
        guard(task, altered, metadata)
        materialize(directory, altered)
        rejected(lambda: checked(binary, directory, commit), reason)
    if task.name == '11-successor-contract':
        altered = dict(gold)
        altered['src/main.sak'] = gold['src/main.sak'].replace(b'result == x + 1', b'true')
        materialize(directory, altered)
        rejected(lambda: checked(binary, directory, commit), 'V8020')
    if task.name == '04-local-env-handler':
        altered = dict(gold)
        altered['src/main.sak'] = gold['src/main.sak'].replace(
            b'configured(read: fn() -> String ! {Env}) -> String {', b'configured(read: fn() -> String ! {Env}) -> String ! {Env} {')
        materialize(directory, altered)
        rejected(lambda: checked(binary, directory, commit), 'V8020')
    return (f'{task.name}: schema + starter diagnostics + gold {len(results["gold"])}/'
            f'{len(results["gold"])} pass; hollow 0/{len(results["hollow"])} pass; controls rejected')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sakura', required=True, help='locally built CLI; no API calls')
    parser.add_argument('--task', help='validate one task while authoring; CI omits this')
    parser.add_argument('--jobs', type=int, default=min(os.cpu_count() or 1, 32),
                        help='parallel tasks (default: CPU count capped at 32; 1 for serial)')
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error('--jobs must be a positive integer')
    binary = str(Path(args.sakura).resolve())
    schema = json.loads((SUITE / 'task.schema.json').read_text())
    tasks = sorted((SUITE / 'tasks').iterdir())
    require(len(tasks) == 12 and all(p.is_dir() and not p.is_symlink() for p in tasks), 'pilot must have exactly 12 tasks')
    metadata = {p.name: json.loads((p / 'task.json').read_text()) for p in tasks}
    require(collections.Counter(m['category'] for m in metadata.values()) ==
            collections.Counter({c: 2 for c in CATEGORIES}), 'need two tasks per category')
    require(sum(m['comparison']['portable'] for m in metadata.values()) == 6,
            'portable subset must contain six tasks')
    for task in tasks:
        validate_task(task, metadata[task.name], schema)
    # A malformed manifest must not silently become a skipped task.
    bad = dict(metadata[tasks[0].name], unexpected=True)
    rejected(lambda: validate(bad, schema), 'unknown fields')
    bad = dict(metadata[tasks[0].name], oracle={'files': ['../outside', 'oracle/valid.sak'], 'command': []})
    rejected(lambda: validate(bad, schema), 'pattern mismatch')
    if args.task:
        require(args.task in metadata, 'unknown selected task')
        tasks = [p for p in tasks if p.name == args.task]
    temporary = ROOT / 'target/agent-dx'
    temporary.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='offline-', dir=temporary) as directory:
        def execute(task):
            try:
                return exercise(task, metadata[task.name], binary, Path(directory))
            except AssertionError as error:
                raise AssertionError(f'{task.name}: {error}') from error

        with concurrent.futures.ThreadPoolExecutor(max_workers=min(args.jobs, len(tasks))) as pool:
            reports = list(pool.map(execute, tasks))
        for report in reports:
            print(report, flush=True)


if __name__ == '__main__':
    main()
