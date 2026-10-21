#!/usr/bin/env python3
"""Agent DX episode runner (design §17.146, plan DX5): scripted, replay and live backends, and
`judge` for episodes run outside it (design §17.301 (3c)).

Live mode calls the Anthropic Messages API and spends the owner's money; it refuses to start
without --budget-usd, --approved-by and pinned model prices, and keeps a committed spend ledger.
Scripted, replay and judge make no network calls and run in CI without secrets.
"""
import argparse
import datetime
try:
    import fcntl
except ImportError:  # Windows
    fcntl = None
    import msvcrt
import hashlib
import ipaddress
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parent))
import offline  # noqa: E402

HARNESS_VERSION = '2'
SUITE = offline.SUITE
OWNER = 'icantcodefyi'
OWNER_TOTAL_USD = 10.0  # owner decision 2026-10-18: $10 total across every live run
PRICE_CEILING = (4.0, 20.0)  # USD per million input/output tokens, README model A envelope
MAX_TURNS = 40
MAX_INPUT_CAP = 60_000  # the §17.146 envelope; §17.165 pairs a 60,000 arm against 40,000
TOOL_OUTPUT_CHARS = 8000
CONDITIONS = ('baseline', 'skill')
ANTHROPIC_VERSION = '2023-06-01'
SAKURA_SUBCOMMANDS = {'check', 'test', 'fmt', 'fix', 'explain', 'prompt', 'doc', 'audit',
                      'verify', 'help', '--help', '-h', '--version', '-V'}
SUBMITTING = {'check', 'test'}
RETRIEVING = {'explain', 'prompt', 'doc', 'help', '--help', '-h'}
CODE = re.compile(r'\b[A-Z]\d{4}\b')

SYSTEM = """You are a coding agent working in a small Sakura Lang project. The project directory \
is the only filesystem you can see. Use the tools to list, read and write project files and to \
run the `sakura` command-line tool (`sakura --help`, `sakura prompt`, `sakura explain <code>`, \
`sakura check`, `sakura test`). When the task is finished, reply with a one-line summary and no \
tool calls."""


class Refused(Exception):
    """A precondition for spending failed; nothing was sent."""


class Stop(Exception):
    """End the episode with a termination reason."""

    def __init__(self, reason):
        super().__init__(reason)
        self.reason = reason


class Halt(Exception):
    """Unknown billing state: stop the whole run, keeping every reservation."""


# ---------------------------------------------------------------- backends

class Recorded:
    """Responses from a script or a transcript, in order. No network."""

    def __init__(self, responses, counts=None):
        self.responses = list(responses)
        self.counts = counts if counts is not None else [
            r['usage']['input_tokens'] for r in self.responses]
        self.sent = self.counted = 0

    def count(self, body):
        if self.counted >= len(self.counts):
            raise Stop('script_exhausted')
        self.counted += 1
        return self.counts[self.counted - 1]

    def send(self, body):
        if self.sent >= len(self.responses):
            raise Stop('script_exhausted')
        self.sent += 1
        response = self.responses[self.sent - 1]
        return response, float(response.get('elapsed_s', 0.0))


class Live:
    """Anthropic Messages API over urllib. No automatic retries."""

    def __init__(self, base, key):
        self.base = base.rstrip('/')
        self.key = key

    def _post(self, path, body):
        request = urllib.request.Request(
            self.base + path, data=json.dumps(body).encode(), method='POST',
            headers={'x-api-key': self.key, 'anthropic-version': ANTHROPIC_VERSION,
                     'content-type': 'application/json'})
        start = time.monotonic()
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                return json.loads(response.read()), time.monotonic() - start
        except (urllib.error.URLError, OSError, ValueError) as error:
            raise Halt(f'{path}: {error}') from None

    def count(self, body):
        counted, _ = self._post('/v1/messages/count_tokens',
                                {k: v for k, v in body.items() if k != 'max_tokens'})
        return int(counted['input_tokens'])

    def send(self, body):
        return self._post('/v1/messages', body)


# ---------------------------------------------------------------- spend ledger

class Ledger:
    """Committed spend record. Each request is reserved before sending and settled after."""

    def __init__(self, path, run_id):
        self.path = Path(path)
        if not self.path.is_file():
            raise Refused(f'ledger {self.path} is missing; it is committed and never recreated')
        self.run_id = run_id
        # One writer per ledger. POSIX locks the directory (the ledger itself is replaced on
        # write); Windows cannot open a directory, so it locks a sibling `.lock` file.
        if fcntl:
            self.lock = os.open(self.path.parent, os.O_RDONLY)
            fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        else:
            self.lock = os.open(self.path.with_name(self.path.name + '.lock'),
                                os.O_RDWR | os.O_CREAT)
            msvcrt.locking(self.lock, msvcrt.LK_NBLCK, 1)
        self.data = json.loads(self.path.read_text())
        offline.require(self.data.get('schema_version') == 1, 'unknown ledger schema')

    def spent(self):
        return round(sum(e['cost_usd'] if e['status'] == 'settled' else e['reserved_usd']
                         for e in self.data['entries']), 6)

    def _write(self):
        temporary = self.path.with_suffix('.tmp')
        temporary.write_text(json.dumps(self.data, indent=1) + '\n')
        os.replace(temporary, self.path)

    def reserve(self, episode, model, turn, usd):
        entry = {'run_id': self.run_id, 'episode': episode, 'model': model, 'turn': turn,
                 'status': 'reserved', 'reserved_usd': round(usd, 6), 'cost_usd': None,
                 'input_tokens': None, 'output_tokens': None,
                 'at': datetime.datetime.now(datetime.UTC).isoformat(timespec='seconds')}
        self.data['entries'].append(entry)
        self._write()
        return entry

    def settle(self, entry, usage, usd):
        entry.update(status='settled', cost_usd=round(usd, 6),
                     input_tokens=usage['input_tokens'], output_tokens=usage['output_tokens'])
        self._write()

    def unknown(self, entry, reason):
        entry.update(status='unknown', note=reason[:200])
        self._write()


# ---------------------------------------------------------------- tools and isolation

def sandboxed(binary, mode, scratch):
    """A `sakura` wrapper that denies the network to generated programs."""
    if mode == 'none':
        return binary
    if sys.platform == 'darwin':
        prefix = "sandbox-exec -p '(version 1)(allow default)(deny network*)'"
    else:
        prefix = 'unshare --user --map-root-user --net --'
    wrapper = Path(scratch) / 'sakura'
    wrapper.write_text(f'#!/bin/sh\nexec {prefix} "{binary}" "$@"\n')
    wrapper.chmod(0o755)
    code, out, err = offline.run([str(wrapper), '--version'], scratch)
    if code != 0:
        raise Refused(f'network sandbox unavailable: {out}{err}')
    return str(wrapper)


def inside(project, relative):
    """Resolve a model-supplied path, refusing anything outside the project or into `.git`."""
    if not isinstance(relative, str) or not relative or relative.startswith(('/', '\\')):
        raise ValueError('path must be relative to the project')
    parts = Path(relative).parts
    if '..' in parts or '.git' in parts:
        raise ValueError('path escapes the project')
    path = (project / relative).resolve()
    if project.resolve() not in path.parents and path != project.resolve():
        raise ValueError('path escapes the project')
    return path


def tool_schemas(skill):
    tools = [
        {'name': 'list_files', 'description': 'List every file in the project.',
         'input_schema': {'type': 'object', 'properties': {}}},
        {'name': 'read_file', 'description': 'Read a project file (path relative to the project).',
         'input_schema': {'type': 'object', 'properties': {'path': {'type': 'string'}},
                          'required': ['path']}},
        {'name': 'write_file', 'description': 'Replace a project file with the given content.',
         'input_schema': {'type': 'object', 'properties': {'path': {'type': 'string'},
                                                           'content': {'type': 'string'}},
                          'required': ['path', 'content']}},
        {'name': 'sakura', 'description': 'Run the sakura CLI in the project directory, e.g. '
         '["check"], ["test"], ["explain", "V2001"], ["prompt", "--budget", "2000"]. '
         'Allowed subcommands: ' + ', '.join(sorted(SAKURA_SUBCOMMANDS - {'-h', '-V'})) + '.',
         'input_schema': {'type': 'object', 'properties': {'args': {'type': 'array',
                                                                    'items': {'type': 'string'}}},
                          'required': ['args']}},
    ]
    if skill:
        tools.append({'name': 'load_skill', 'description': 'Load a file of the `sakura` skill: '
                      + skill['description'] + ' Start with SKILL.md.',
                      'input_schema': {'type': 'object', 'properties': {
                          'path': {'type': 'string', 'enum': sorted(skill['files'])}},
                          'required': ['path']}})
    return tools


def load_skill_bundle(binary, scratch):
    """The skill this exact binary embeds, as `sakura setup` writes it."""
    project = Path(tempfile.mkdtemp(prefix='skill-', dir=scratch))
    shutil.copy(SUITE / 'tasks/01-bounded-clamp/starter/sakura.toml', project / 'sakura.toml')
    code, out, err = offline.run([binary, 'setup', '--client', 'claude', '--apply'], project)
    offline.require(code == 0, f'sakura setup failed: {out}{err}')
    root = project / '.claude/skills/sakura'
    files = {p.relative_to(root).as_posix(): p.read_text()
             for p in sorted(root.rglob('*.md')) if 'assets' not in p.relative_to(root).parts}
    description = re.search(r'^description: (.*)$', files['SKILL.md'], re.M).group(1)
    sha = offline.digest(json.dumps(files, sort_keys=True).encode())
    return {'files': files, 'description': description, 'sha256': sha}


# ---------------------------------------------------------------- acceptance

def codes_of(rows):
    return sorted({row['code'] for row in rows if row.get('code')})


def reported_codes(text):
    """Codes of the diagnostics a command reported: its `--json` diagnostic records, or the
    `error[V1002]` headers of the human rendering; never codes quoted in prose."""
    codes = set()
    for line in text.splitlines():
        try:
            record = json.loads(line)
        except ValueError:
            codes.update(re.findall(r'^(?:error|warning|note)\[([A-Z]\d{4})\]', line))
            continue
        if isinstance(record, dict) and record.get('kind') == 'diagnostic' and record.get('code'):
            codes.add(record['code'])
    return codes


def verdict(task, meta, binary, candidate, scratch):
    """Independent acceptance of one frozen snapshot, outside the agent's directory."""
    directory = Path(tempfile.mkdtemp(prefix='verify-', dir=scratch))
    commit = offline.baseline(task, directory)
    reasons = []
    try:
        offline.guard(task, candidate, meta)
        offline.materialize(directory, candidate)
    except (AssertionError, KeyError, UnicodeDecodeError) as error:
        reasons.append(f'guard: {error}')
        if 'src/main.sak' in candidate:
            (directory / 'src/main.sak').write_bytes(candidate['src/main.sak'])
    try:
        rows = offline.records(offline.run([binary, 'check', '--json'], directory))
        codes, holes = codes_of(rows), rows[-1].get('holes')
    except (AssertionError, ValueError) as error:
        codes, holes = None, None
        reasons.append(f'check: {str(error)[:300]}')
    structural = not reasons
    if structural:
        try:
            offline.checked(binary, directory, commit)
        except (AssertionError, ValueError) as error:
            structural = False
            reasons.append(f'frozen: {str(error)[:300]}')
    oracles = {}
    for name in meta['oracle']['files']:
        if not structural:
            oracles[name] = False
            continue
        try:
            oracles[name] = offline.oracle(binary, directory, task, name, candidate)
        except (AssertionError, ValueError) as error:
            oracles[name] = False
            reasons.append(f'{name}: {str(error)[:300]}')
    if structural:
        offline.materialize(directory, candidate)
        try:
            visible = offline.run([binary, 'test', '--json'], directory)
            if visible[0] != 0:
                reasons.append('visible tests failed')
        except AssertionError as error:
            reasons.append(f'visible: {error}')
    accepted = not reasons and all(oracles.values())
    return {'accepted': accepted, 'reasons': reasons, 'oracles': oracles,
            'diagnostics': codes, 'holes': holes}


# ---------------------------------------------------------------- one episode

def cost(usage, prices):
    """USD for one response. Cache writes are charged at 2x input (the 1-hour rate), the
    conservative bound; caching is never requested."""
    pin, pout = prices
    return (usage.get('input_tokens', 0) * pin
            + (usage.get('cache_creation_input_tokens') or 0) * pin * 2
            + (usage.get('cache_read_input_tokens') or 0) * pin
            + usage.get('output_tokens', 0) * pout) / 1_000_000


def budget_of(task, input_cap=None):
    """The task's budget, with the aggregate input cap of the episode's arm if one is set."""
    budget = dict(json.loads((task / 'task.json').read_text())['budget'])
    if input_cap is not None:
        budget['max_input_tokens'] = input_cap
    return budget


def episode_id(task, condition, model, budget):
    return f"{task.name}.{condition}.{model}.in{budget['max_input_tokens']}"


def lookup_id(tool, args):
    """The document a tool call retrieves (`explain V1002`, `doc List.map`, `skill:SKILL.md`),
    or None for project and checking tools."""
    if tool == 'load_skill':
        return f"skill:{args.get('path')}"
    argv = args.get('args') if tool == 'sakura' else None
    if isinstance(argv, list) and argv and argv[0] in RETRIEVING:
        return ' '.join(str(a) for a in argv)
    return None


def episode_max_usd(budget, prices):
    return (budget['max_input_tokens'] * prices[0] + budget['max_output_tokens'] * prices[1]) / 1e6


class Episode:
    def __init__(self, task, condition, model, prices, backend, binary, skill, scratch,
                 transcript, ledger=None, spend_cap=None, input_cap=None):
        self.task, self.meta = task, json.loads((task / 'task.json').read_text())
        self.budget = budget_of(task, input_cap)
        self.id = episode_id(task, condition, model, self.budget)
        self.condition, self.model, self.prices = condition, model, prices
        self.backend, self.binary, self.scratch = backend, binary, Path(scratch)
        self.skill = skill if condition == 'skill' else None
        self.ledger, self.spend_cap = ledger, spend_cap
        self.log = open(transcript, 'w')
        self.project = Path(tempfile.mkdtemp(prefix='agent-', dir=scratch))
        for name, data in offline.files(task / 'starter').items():
            (self.project / name).parent.mkdir(parents=True, exist_ok=True)
            (self.project / name).write_bytes(data)
        self.starter = offline.files(self.project)
        self.submissions, self.verdicts = [], []
        self.usage = {'input_tokens': 0, 'output_tokens': 0,
                      'cache_creation_input_tokens': 0, 'cache_read_input_tokens': 0}
        self.usd = 0.0
        self.model_s = self.tool_s = 0.0
        self.turns = self.tool_calls = 0
        self.seen, self.documents, self.unauthorized = set(), [], []
        self.reported, self.quoted = set(), set()
        self.refused_input = None
        self.skill_loaded = False

    def emit(self, event, **fields):
        self.log.write(json.dumps({'event': event, **fields}, sort_keys=True) + '\n')
        self.log.flush()

    def elapsed(self):
        return self.model_s + self.tool_s

    def input_used(self):
        return sum(v for k, v in self.usage.items() if k != 'output_tokens')

    # -- tools

    def submit(self, trigger):
        current = offline.files(self.project)
        previous = self.submissions[-1] if self.submissions else self.starter
        if current == previous:
            return
        if len(self.submissions) > self.budget['max_repair_cycles']:
            raise Stop('repair_cap')
        self.submissions.append(current)
        self.emit('submission', index=len(self.submissions), trigger=trigger,
                  files={k: v.decode(errors='replace') for k, v in current.items()})

    def call(self, name, args):
        if name == 'list_files':
            return '\n'.join(offline.files(self.project))
        if name == 'read_file':
            return inside(self.project, args.get('path')).read_text()
        if name == 'write_file':
            path, content = inside(self.project, args.get('path')), args.get('content')
            if not isinstance(content, str) or len(content) > 65536:
                raise ValueError('content must be a string of at most 64 KiB')
            relative = path.relative_to(self.project.resolve()).as_posix()
            if relative not in self.meta['editable_files']:
                self.unauthorized.append(relative)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
            return f'wrote {relative}'
        if name == 'load_skill' and self.skill:
            path = args.get('path')
            if path not in self.skill['files']:
                raise ValueError('unknown skill file')
            self.skill_loaded = True
            text = self.skill['files'][path]
            self.quoted.update(CODE.findall(text))
            return text
        if name == 'sakura':
            argv = args.get('args')
            if not (isinstance(argv, list) and argv and all(isinstance(a, str) for a in argv)
                    and argv[0] in SAKURA_SUBCOMMANDS and '--record' not in argv):
                raise ValueError('allowed: ' + ', '.join(sorted(SAKURA_SUBCOMMANDS)))
            if argv[0] in SUBMITTING:
                self.submit('sakura ' + argv[0])
            try:
                code, out, err = offline.run([self.binary, *argv], self.project,
                                             self.budget['tool_seconds'])
            except AssertionError:
                return f'timed out after {self.budget["tool_seconds"]}s'
            text = f'exit {code}\n{out}{err}'
            for path in {str(self.project.resolve()), str(self.project)}:
                text = text.replace(path, '<project>')
            self.seen.update(CODE.findall(text))
            if argv[0] in RETRIEVING:
                self.quoted.update(CODE.findall(text))
            else:
                self.reported.update(reported_codes(out + err))
            return text
        raise ValueError(f'unknown tool {name}')

    def document(self, lookup, tool, output, failed):
        """One retrieval: what was asked, whether it succeeded, what the model received, and
        the aggregate input billed so far, the request that asked for it included (§17.165 (3)).
        `approx_tokens` is bytes / 4, not a tokenizer count."""
        sent = output[:TOOL_OUTPUT_CHARS].encode()
        # `sakura doc` exits 0 with `no symbol matches` when the lookup found nothing.
        ok = not failed and (tool != 'sakura' or (output.startswith('exit 0\n')
                                                  and output[7:].strip() != 'no symbol matches'))
        return {'id': lookup, 'ok': ok, 'bytes': len(sent),
                'approx_tokens': math.ceil(len(sent) / 4),
                'truncated': len(output) > TOOL_OUTPUT_CHARS,
                'input_used': self.input_used(), 'turn': self.turns}

    # -- the loop

    def request(self, messages):
        remaining_out = self.budget['max_output_tokens'] - self.usage['output_tokens']
        if remaining_out <= 0:
            raise Stop('output_cap')
        body = {'model': self.model, 'max_tokens': remaining_out, 'system': SYSTEM,
                'tools': tool_schemas(self.skill), 'messages': messages}
        if self.skill:
            body['system'] += ('\n\nAvailable skills (load with the load_skill tool):\n- sakura: '
                               + self.skill['description'])
        used_in = self.input_used()
        counted = self.backend.count(body)
        self.emit('count', turn=self.turns, counted_input=counted)
        if used_in + counted > self.budget['max_input_tokens']:
            self.refused_input = counted
            raise Stop('input_cap')
        reserve = (counted * self.prices[0] + remaining_out * self.prices[1]) / 1e6
        entry = None
        if self.ledger:
            available = min(self.spend_cap - self.usd, OWNER_TOTAL_USD - self.ledger.spent())
            if reserve > available + 1e-9:
                raise Stop('budget')
            entry = self.ledger.reserve(self.id, self.model, self.turns, reserve)
        self.emit('request', turn=self.turns, counted_input=counted, max_tokens=remaining_out,
                  reserved_usd=round(reserve, 6), messages=len(messages))
        try:
            response, seconds = self.backend.send(body)
            usage = {k: response['usage'].get(k) or 0 for k in self.usage}
        except (Halt, KeyError, TypeError) as error:
            if entry:
                self.ledger.unknown(entry, str(error))
            raise Halt(f'{self.id}: {error}') from None
        spent = cost(usage, self.prices)
        if entry:
            self.ledger.settle(entry, usage, spent)
        for key in self.usage:
            self.usage[key] += usage[key]
        self.usd += spent
        self.model_s += seconds
        self.turns += 1
        self.emit('response', turn=self.turns, body=response, elapsed_s=round(seconds, 3),
                  cost_usd=round(spent, 6))
        return response

    def loop(self):
        messages = [{'role': 'user', 'content': (self.task / 'prompt.md').read_text()}]
        while True:
            if self.turns >= MAX_TURNS:
                raise Stop('max_turns')
            if self.elapsed() > self.budget['wall_seconds']:
                raise Stop('wall_clock')
            response = self.request(messages)
            if self.usage['input_tokens'] > self.budget['max_input_tokens']:
                raise Stop('input_cap')
            if self.elapsed() > self.budget['wall_seconds']:
                raise Stop('wall_clock')
            content = response.get('content') or []
            if response.get('stop_reason') == 'max_tokens':
                raise Stop('output_cap')
            uses = [block for block in content if block.get('type') == 'tool_use']
            if not uses:
                self.submit('final answer')
                return response.get('stop_reason') or 'end_turn'
            messages.append({'role': 'assistant', 'content': content})
            results = []
            for use in uses:
                self.tool_calls += 1
                start = time.monotonic()
                try:
                    output, failed = self.call(use['name'], use.get('input') or {}), False
                except (ValueError, OSError, UnicodeDecodeError) as error:
                    output, failed = f'error: {error}', True
                    if use['name'] == 'write_file':
                        self.unauthorized.append(str((use.get('input') or {}).get('path')))
                self.tool_s += time.monotonic() - start
                lookup = lookup_id(use['name'], use.get('input') or {})
                if lookup:
                    self.documents.append(self.document(lookup, use['name'], output, failed))
                output = output[:TOOL_OUTPUT_CHARS]
                self.emit('tool', turn=self.turns, id=use['id'], name=use['name'],
                          input=use.get('input'), output=output, is_error=failed)
                results.append({'type': 'tool_result', 'tool_use_id': use['id'],
                                'content': output, 'is_error': failed})
            messages.append({'role': 'user', 'content': results})

    def run(self):
        initial = offline.records(offline.run([self.binary, 'check', '--json'], self.project))
        self.emit('start', task=self.task.name, revision=self.meta['revision'],
                  starter_sha256=self.meta['starter_sha256'], condition=self.condition,
                  model=self.model, prices=list(self.prices), budget=self.budget,
                  max_turns=MAX_TURNS, harness=HARNESS_VERSION,
                  skill_sha256=self.skill['sha256'] if self.skill else None)
        try:
            termination = self.loop()
        except Stop as stop:
            termination = stop.reason
        for index, files in enumerate(self.submissions, 1):
            result = verdict(self.task, self.meta, self.binary, files, self.scratch)
            self.verdicts.append(result)
            self.emit('verdict', index=index, **result)
        metrics = self.metrics(codes_of(initial), initial[-1].get('holes'), termination)
        self.emit('end', metrics=metrics)
        self.log.close()
        shutil.rmtree(self.project, ignore_errors=True)
        return metrics

    def metrics(self, initial_codes, initial_holes, termination):
        accepted = [v['accepted'] for v in self.verdicts]
        green = accepted.index(True) + 1 if True in accepted else None
        final = self.verdicts[-1] if self.verdicts else None
        expected = self.meta['expected_diagnostics']
        fixed = None
        if expected:
            fixed = bool(final and final['accepted'] and final['diagnostics'] == [])
        return {
            'episode': self.id, 'task': self.task.name, 'condition': self.condition,
            'model': self.model, 'input_cap': self.budget['max_input_tokens'],
            'termination': termination,
            'submissions': len(self.submissions), 'pass_at_1': bool(accepted and accepted[0]),
            'success': bool(accepted and accepted[-1]), 'iterations_to_green': green,
            'censored': green is None, 'turns': self.turns, 'tool_calls': self.tool_calls,
            'skill_loaded': self.skill_loaded if self.skill else None,
            'documents': self.documents, 'unauthorized_edits': sorted(set(self.unauthorized)),
            'diagnostics': {'initial': initial_codes, 'initial_holes': initial_holes,
                            'seen': sorted(self.seen), 'reported': sorted(self.reported),
                            'quoted': sorted(self.quoted),
                            'final': final['diagnostics'] if final else None,
                            'fix_success': fixed},
            'tokens': dict(self.usage), 'cost_usd': round(self.usd, 6),
            'input_budget': {'cap': self.budget['max_input_tokens'], 'used': self.input_used(),
                             'remaining': self.budget['max_input_tokens'] - self.input_used(),
                             'refused_request': self.refused_input},
            'model_seconds': round(self.model_s, 3),
        }


# ---------------------------------------------------------------- reporting

def wilson(successes, n, z=1.96):
    if n == 0:
        return None
    p = successes / n
    centre = (p + z * z / (2 * n)) / (1 + z * z / n)
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / (1 + z * z / n)
    return [round(centre - half, 4), round(centre + half, 4)]


def summarize(episodes):
    cells = {}
    for m in episodes:
        cells.setdefault(f"{m['model']}/{m['condition']}/in{m['input_cap']}", []).append(m)
    summary = {}
    for key, rows in sorted(cells.items()):
        n = len(rows)
        passed = sum(r['pass_at_1'] for r in rows)
        success = sum(r['success'] for r in rows)
        greens = sorted(r['iterations_to_green'] for r in rows if r['iterations_to_green'])
        spent = round(sum(r['cost_usd'] for r in rows), 6)
        summary[key] = {
            'episodes': n, 'pass_at_1': [passed, n, wilson(passed, n)],
            'success': [success, n, wilson(success, n)],
            'iterations_to_green': {'median': greens[len(greens) // 2] if greens else None,
                                    'p90': greens[min(len(greens) - 1, int(0.9 * len(greens)))]
                                    if greens else None},
            'cost_usd': spent, 'cost_per_accepted': round(spent / success, 6) if success else None,
            'skill_not_loaded': sum(r['skill_loaded'] is False for r in rows),
            'input_cap_stops': sum(r['termination'] == 'input_cap' for r in rows),
            'failed': [r['episode'] for r in rows if not r['success']],
        }
    return summary


# ---------------------------------------------------------------- commands

def parse_model(spec):
    match = re.fullmatch(r'([A-Za-z0-9._:-]+)=([0-9.]+)/([0-9.]+)', spec)
    if not match:
        raise argparse.ArgumentTypeError('expected MODEL_ID=INPUT_USD/OUTPUT_USD per million')
    return match.group(1), (float(match.group(2)), float(match.group(3)))


def loopback(url):
    host = urllib.parse.urlparse(url).hostname or ''
    try:
        return ipaddress.ip_address(host).is_loopback
    except ValueError:
        return host == 'localhost'


def environment(binary):
    def text(args):
        try:
            return subprocess.run(args, capture_output=True, text=True, timeout=30).stdout.strip()
        except OSError:
            return None
    return {'sakura': text([binary, '--version']), 'node': text(['node', '--version']),
            'python': platform.python_version(), 'platform': platform.platform(),
            'repo_commit': text(['git', '-C', str(offline.ROOT), 'rev-parse', 'HEAD']),
            'harness': HARNESS_VERSION,
            'date': datetime.datetime.now(datetime.UTC).isoformat(timespec='seconds')}


def command_run(args):
    binary = str(Path(args.sakura).resolve())
    tasks = sorted(p for p in (SUITE / 'tasks').iterdir() if p.is_dir())
    if args.tasks != 'all':
        wanted = args.tasks.split(',')
        offline.require(set(wanted) <= {p.name for p in tasks}, f'unknown task in {wanted}')
        tasks = [p for p in tasks if p.name in wanted]
    conditions = args.conditions.split(',')
    offline.require(set(conditions) <= set(CONDITIONS), f'conditions: {CONDITIONS}')
    models = args.model or []
    offline.require(models, 'pin at least one --model MODEL_ID=IN/OUT')
    offline.require(len({m for m, _ in models}) == len(models), 'duplicate --model')
    caps = [None]
    if args.input_caps:
        caps = [int(c) for c in args.input_caps.split(',')]
        offline.require(len(set(caps)) == len(caps) and all(0 < c <= MAX_INPUT_CAP for c in caps),
                        f'--input-caps: distinct values in (0, {MAX_INPUT_CAP}]')
    # Every arm runs every cell, shuffled together, so the arms are paired in one design.
    plan = [(t, c, m, p, k) for t in tasks for c in conditions for m, p in models for k in caps]
    random.Random(args.seed).shuffle(plan)
    budgets = [budget_of(t, k) for t, _, _, _, k in plan]
    estimate = sum((b['estimated_input_tokens'] * p[0] + b['estimated_output_tokens'] * p[1]) / 1e6
                   for b, (_, _, _, p, _) in zip(budgets, plan))
    maximum = sum(episode_max_usd(b, p) for b, (_, _, _, p, _) in zip(budgets, plan))
    run_id = args.run_id or datetime.datetime.now(datetime.UTC).strftime('%Y%m%dT%H%M%SZ')
    ledger = None
    if args.backend == 'live':
        if args.budget_usd is None or not args.approved_by:
            raise Refused('live runs require --budget-usd and --approved-by')
        if args.approved_by != OWNER:
            raise Refused(f'only the owner ({OWNER}) can approve spending')
        if not 0 < args.budget_usd <= OWNER_TOTAL_USD:
            raise Refused(f'--budget-usd must be in (0, {OWNER_TOTAL_USD}]')
        for model, (pin, pout) in models:
            if pin > PRICE_CEILING[0] or pout > PRICE_CEILING[1] or pin <= 0 or pout <= 0:
                raise Refused(f'{model}: prices must be positive and at most {PRICE_CEILING}')
        if args.sandbox == 'none' and not loopback(args.api_base):
            raise Refused('a real provider requires --sandbox auto')
        key = os.environ.get('ANTHROPIC_API_KEY')
        if not key and not args.plan_only:
            raise Refused('ANTHROPIC_API_KEY is not set')
        # Per approved run (§17.182 (4)); across runs the dollar ledger is the only rail.
        if not 0 < len(plan) <= args.max_episodes:
            raise Refused(f'{len(plan)} episodes planned; --max-episodes allows '
                          f'{args.max_episodes} for this run')
        ledger = Ledger(args.ledger, run_id)
    elif args.backend == 'scripted':
        offline.require(args.script and len(plan) == 1, 'scripted runs need --script and one episode')
    print(json.dumps({'run_id': run_id, 'episodes': len(plan), 'estimated_usd': round(estimate, 4),
                      'maximum_usd': round(maximum, 4),
                      'ledger_spent_usd': ledger.spent() if ledger else None,
                      'order': [episode_id(t, c, m, b) for (t, c, m, _, _), b in zip(plan, budgets)]}),
          flush=True)
    if args.plan_only:
        return 0
    out = Path(args.out or offline.ROOT / 'target/agent-dx/runs' / run_id)
    out.mkdir(parents=True, exist_ok=True)
    scratch_root = offline.ROOT / 'target/agent-dx'
    scratch_root.mkdir(parents=True, exist_ok=True)
    results, status = [], 0
    with tempfile.TemporaryDirectory(prefix='run-', dir=scratch_root) as scratch:
        wrapped = sandboxed(binary, args.sandbox, scratch)
        skill = load_skill_bundle(binary, scratch)
        header = {'run_id': run_id, 'backend': args.backend, 'approved_by': args.approved_by,
                  'budget_usd': args.budget_usd, 'seed': args.seed, 'sandbox': args.sandbox,
                  'models': {m: list(p) for m, p in models}, 'conditions': conditions,
                  'input_caps': caps,
                  'skill_sha256': skill['sha256'], 'github_run': os.environ.get('GITHUB_RUN_ID'),
                  'environment': environment(binary)}
        (out / 'run.json').write_text(json.dumps(header, indent=1) + '\n')
        run_spent = 0.0
        for (task, condition, model, prices, cap), budget in zip(plan, budgets):
            if ledger:
                available = min(args.budget_usd - run_spent, OWNER_TOTAL_USD - ledger.spent())
                need = episode_max_usd(budget, prices)
                if need > available + 1e-9:
                    print(f'stopping: next episode may cost ${need:.4f}, ${available:.4f} left',
                          flush=True)
                    status = 2
                    break
                backend = Live(args.api_base, os.environ['ANTHROPIC_API_KEY'])
            else:
                script = json.loads(Path(args.script).read_text())
                backend = Recorded(script['responses'])
            episode = Episode(task, condition, model, prices, backend, wrapped, skill, scratch,
                              out / f'{episode_id(task, condition, model, budget)}.jsonl', ledger,
                              (args.budget_usd or 0) - run_spent, cap)
            try:
                metrics = episode.run()
            except Halt as halt:
                print(f'halted with unknown billing state: {halt}', file=sys.stderr, flush=True)
                status = 3
                break
            run_spent += metrics['cost_usd']
            results.append(metrics)
            print(json.dumps(metrics, sort_keys=True), flush=True)
    report = {'run': header, 'summary': summarize(results), 'episodes': results}
    (out / 'summary.json').write_text(json.dumps(report, indent=1, sort_keys=True) + '\n')
    print(f'wrote {out}', flush=True)
    return status


VOLATILE = ('model_seconds',)


def command_replay(args):
    """Rerun tools, oracles and accounting from a transcript; the recorded model responses
    stand in for the provider."""
    binary = str(Path(args.sakura).resolve())
    events = [json.loads(line) for line in Path(args.transcript).read_text().splitlines()]
    start = next(e for e in events if e['event'] == 'start')
    offline.require(start['harness'] == HARNESS_VERSION,
                    f"transcript was recorded by harness {start['harness']}, not {HARNESS_VERSION}")
    recorded = next(e for e in events if e['event'] == 'end')['metrics']
    responses = [dict(e['body'], elapsed_s=e['elapsed_s'])
                 for e in events if e['event'] == 'response']
    counts = [e['counted_input'] for e in events if e['event'] == 'count']
    task = SUITE / 'tasks' / start['task']
    meta = json.loads((task / 'task.json').read_text())
    offline.require(meta['revision'] == start['revision']
                    and meta['starter_sha256'] == start['starter_sha256'],
                    'transcript was recorded against a different task revision')
    scratch_root = offline.ROOT / 'target/agent-dx'
    scratch_root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='replay-', dir=scratch_root) as scratch:
        skill = load_skill_bundle(binary, scratch)
        out = Path(args.out or scratch) / ('replay-' + Path(args.transcript).name)
        episode = Episode(task, start['condition'], start['model'], tuple(start['prices']),
                          Recorded(responses, counts), binary, skill, scratch, out,
                          input_cap=start['budget']['max_input_tokens'])
        replayed = episode.run()
    print(json.dumps(replayed, sort_keys=True), flush=True)
    diff = {k: (recorded.get(k), replayed.get(k)) for k in set(recorded) | set(replayed)
            if k not in VOLATILE and recorded.get(k) != replayed.get(k)}
    if diff:
        print(f'replay differs from the recording: {json.dumps(diff, sort_keys=True)}',
              file=sys.stderr)
        return 1
    return 0


SKILL_PATH = re.compile(r'skills/sakura/SKILL\.md')
SKILL_LISTED = re.compile(r'^- sakura: ', re.M)


def nodes(value):
    """Every dict in a parsed JSON value, depth first."""
    if isinstance(value, dict):
        yield value
        value = list(value.values())
    if isinstance(value, list):
        for item in value:
            yield from nodes(item)


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from strings(item)


def family_of(model):
    if model.startswith('claude'):
        return 'claude'
    if re.match(r'(gpt|codex|o\d)', model):
        return 'openai'
    return None


def read_client_transcript(path):
    """Family, model, condition and skill activation from a Claude Code or Codex JSONL
    transcript (design §17.301 (3c) (iii)). A tool call is a Claude `tool_use` block or a Codex
    `function_call`/`custom_tool_call`/`local_shell_call` item. The skill counts as loaded when
    a tool call invokes the `sakura` skill or names its `SKILL.md`; the condition is `skill`
    when the transcript lists the skill or names that file anywhere, else `baseline`."""
    events = []
    for number, line in enumerate(Path(path).read_text().splitlines(), 1):
        if line.strip():
            try:
                events.append(json.loads(line))
            except ValueError:
                raise Refused(f'{path}:{number}: not a JSON line') from None
    models = sorted({node['model'] for node in nodes(events)
                     if isinstance(node.get('model'), str) and not node['model'].startswith('<')})
    families = {family_of(m) for m in models}
    if len(families) != 1 or None in families:
        raise Refused(f'{path}: expected one known model family, found models {models}')
    loaded = False
    for node in nodes(events):
        kind = node.get('type')
        if kind == 'tool_use':
            call = node.get('input') or {}
            if node.get('name') == 'Skill' and isinstance(call, dict) and call.get('skill') == 'sakura':
                loaded = True
        elif kind in ('function_call', 'custom_tool_call', 'local_shell_call'):
            call = [node.get('arguments'), node.get('input'), node.get('action')]
        else:
            continue
        loaded = loaded or any(SKILL_PATH.search(s) for s in strings(call))
    available = loaded or any(SKILL_PATH.search(s) or SKILL_LISTED.search(s)
                              for s in strings(events))
    return {'family': families.pop(), 'models': models,
            'condition': 'skill' if available else 'baseline', 'skill_loaded': loaded}


def command_judge(args):
    """Verdict on submissions produced outside the runner (the subscription episodes):
    `<episodes>/<task-id>/submission/` holds the project's files as the agent left them and
    `<episodes>/<task-id>/transcript.jsonl` the client's transcript. No provider call."""
    binary = str(Path(args.sakura).resolve())
    root = Path(args.episodes)
    tasks = {p.name: p for p in (SUITE / 'tasks').iterdir() if p.is_dir()}
    found = sorted(p for p in root.iterdir() if p.is_dir())
    unknown = [p.name for p in found if p.name not in tasks]
    if unknown or not found:
        raise Refused(f'{root}: expected task directories, found {unknown or "none"}')
    scratch_root = offline.ROOT / 'target/agent-dx'
    scratch_root.mkdir(parents=True, exist_ok=True)
    episodes = []
    with tempfile.TemporaryDirectory(prefix='judge-', dir=scratch_root) as scratch:
        for directory in found:
            task = tasks[directory.name]
            meta = json.loads((task / 'task.json').read_text())
            client = read_client_transcript(directory / 'transcript.jsonl')
            submission = directory / 'submission'
            if not submission.is_dir():
                raise Refused(f'{submission} is missing')
            result = verdict(task, meta, binary, offline.files(submission), scratch)
            episode = {'task': task.name, 'revision': meta['revision'], **client, **result}
            episodes.append(episode)
            print(json.dumps(episode, sort_keys=True), flush=True)
    cells = {}
    for e in episodes:
        cells.setdefault(f"{e['family']}/{e['condition']}", []).append(e)
    summary = {key: {'episodes': len(rows),
                     'accepted': [sum(r['accepted'] for r in rows), len(rows),
                                  wilson(sum(r['accepted'] for r in rows), len(rows))],
                     'skill_not_loaded': sum(r['condition'] == 'skill' and not r['skill_loaded']
                                             for r in rows),
                     'failed': [r['task'] for r in rows if not r['accepted']]}
               for key, rows in sorted(cells.items())}
    report = {'kind': 'subscription episodes, judged offline', 'harness': HARNESS_VERSION,
              'environment': environment(binary), 'summary': summary, 'episodes': episodes}
    out = Path(args.out or root / 'judged.json')
    out.write_text(json.dumps(report, indent=1, sort_keys=True) + '\n')
    print(f'wrote {out}', flush=True)
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    run = commands.add_parser('run', help='run episodes with a scripted or live backend')
    run.add_argument('--sakura', required=True)
    run.add_argument('--backend', choices=('scripted', 'live'), required=True)
    run.add_argument('--script', help='scripted backend: JSON {"responses": [...]}')
    run.add_argument('--tasks', default='all', help='comma-separated task ids, or all')
    run.add_argument('--conditions', default='baseline,skill')
    run.add_argument('--model', action='append', type=parse_model,
                     help='MODEL_ID=INPUT_USD/OUTPUT_USD per million tokens (repeat per model)')
    run.add_argument('--budget-usd', type=float)
    run.add_argument('--approved-by')
    run.add_argument('--seed', type=int, default=0)
    run.add_argument('--max-episodes', type=int, default=48,
                     help='live episodes this approved run may plan (default 48)')
    run.add_argument('--input-caps', help='comma-separated aggregate input caps, one arm each '
                     '(e.g. 40000,60000, §17.165); default: each task\'s own cap')
    run.add_argument('--run-id')
    run.add_argument('--out', help='transcripts and summary (default: target/agent-dx/runs/<id>)')
    run.add_argument('--ledger', default=str(SUITE / 'reports/ledger.json'))
    run.add_argument('--api-base', default='https://api.anthropic.com')
    run.add_argument('--sandbox', choices=('auto', 'none'), default='auto')
    run.add_argument('--plan-only', action='store_true', help='print the plan and cost bound only')
    replay = commands.add_parser('replay', help='rerun oracles and accounting from a transcript')
    replay.add_argument('--sakura', required=True)
    replay.add_argument('--transcript', required=True)
    replay.add_argument('--out')
    judge = commands.add_parser('judge', help='verdict on submissions made outside the runner')
    judge.add_argument('--sakura', required=True)
    judge.add_argument('--episodes', required=True,
                       help='directory of <task-id>/{submission/,transcript.jsonl}')
    judge.add_argument('--out', help='report (default: <episodes>/judged.json)')
    args = parser.parse_args(argv)
    commands = {'run': command_run, 'replay': command_replay, 'judge': command_judge}
    try:
        return commands[args.command](args)
    except Refused as refused:
        print(f'refused: {refused}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
