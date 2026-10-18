"""Offline tests for runner.py: scripted episodes, transcript replay, caps, approval refusals
and live mode against a local mock of the Messages API. No real provider is contacted.

Run: SAKURA=/path/to/sakura python3 -m unittest evals/agent-dx/harness/test_runner.py
"""
import http.server
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest

HERE = Path(__file__).resolve().parent
RUNNER = HERE / 'runner.py'
TASKS = HERE.parent / 'tasks'
SAKURA = os.environ.get('SAKURA', '')
TASK = '03-path-label'
GOLD = (TASKS / TASK / 'gold/src/main.sak').read_text()
HOLLOW = (TASKS / TASK / 'hollow/src/main.sak').read_text()
PRICES = 'claude-test=4/20'


def tool(name, uid, **args):
    return {'type': 'tool_use', 'id': uid, 'name': name, 'input': args}


def reply(*content, inp=1000, out=100, stop='tool_use', elapsed=0.0):
    return {'id': 'msg', 'type': 'message', 'role': 'assistant', 'content': list(content),
            'stop_reason': stop, 'elapsed_s': elapsed,
            'usage': {'input_tokens': inp, 'output_tokens': out}}


def done():
    return reply({'type': 'text', 'text': 'Done.'}, stop='end_turn')


def write(uid, text):
    return tool('write_file', uid, path='src/main.sak', content=text)


def check(uid, sub='check'):
    return tool('sakura', uid, args=[sub])


REPAIR = [reply(check('t1')), reply(write('t2', HOLLOW), check('t3')),
          reply(write('t4', GOLD), check('t5', 'test')), done()]


class Base(unittest.TestCase):
    def setUp(self):
        self.assertTrue(SAKURA, 'set SAKURA to the built sakura binary')
        self.tmp = Path(tempfile.mkdtemp())

    def run_cli(self, *args, env=None):
        result = subprocess.run([sys.executable, str(RUNNER), *args], capture_output=True,
                                text=True, env={**os.environ, **(env or {})}, timeout=600)
        return result

    def scripted(self, responses, task=TASK, condition='baseline', cap=None):
        script = self.tmp / 'script.json'
        script.write_text(json.dumps({'responses': responses}))
        out = self.tmp / 'out'
        arm = ('--input-caps', str(cap)) if cap else ()
        result = self.run_cli('run', '--sakura', SAKURA, '--backend', 'scripted', '--script',
                              str(script), '--tasks', task, '--conditions', condition,
                              '--model', PRICES, '--out', str(out), '--sandbox', 'none', *arm)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        summary = json.loads((out / 'summary.json').read_text())
        name = f'{task}.{condition}.claude-test.in{cap or 40000}.jsonl'
        return summary['episodes'][0], out / name


class Runner(Base):
    def test_repair_episode_and_replay(self):
        metrics, transcript = self.scripted(REPAIR)
        self.assertEqual(metrics['termination'], 'end_turn')
        self.assertEqual(metrics['submissions'], 2)
        self.assertFalse(metrics['pass_at_1'])
        self.assertTrue(metrics['success'])
        self.assertEqual(metrics['iterations_to_green'], 2)
        self.assertEqual(metrics['diagnostics']['initial'], ['V1002', 'V2001'])
        self.assertLessEqual({'V1002', 'V2001'}, set(metrics['diagnostics']['seen']))
        self.assertTrue(metrics['diagnostics']['fix_success'])
        self.assertEqual(metrics['tokens']['input_tokens'], 4000)
        self.assertAlmostEqual(metrics['cost_usd'], (4000 * 4 + 400 * 20) / 1e6)
        self.assertNotIn('oracle', ''.join(
            json.loads(line).get('output', '') for line in transcript.read_text().splitlines()))
        result = self.run_cli('replay', '--sakura', SAKURA, '--transcript', str(transcript))
        self.assertEqual(result.returncode, 0, result.stderr)
        tampered = self.tmp / 'tampered.jsonl'
        tampered.write_text(transcript.read_text().replace('"iterations_to_green": 2',
                                                           '"iterations_to_green": 1'))
        self.assertEqual(self.run_cli('replay', '--sakura', SAKURA, '--transcript',
                                      str(tampered)).returncode, 1)

    def test_committed_canned_transcript_replays(self):
        result = self.run_cli('replay', '--sakura', SAKURA, '--transcript',
                              str(HERE / 'fixtures/03-path-label.repair.jsonl'))
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_caps_and_termination(self):
        cases = [
            ([reply(write('a', GOLD), out=4000, stop='max_tokens')], 'output_cap'),
            ([reply(check('a'), inp=39000), reply(check('b'), inp=2000)], 'input_cap'),
            ([reply(check('a'), inp=39000), reply(check('b'), inp=2000)], 'script_exhausted',
             60000),
            ([reply(check('a'), elapsed=601)], 'wall_clock'),
            ([reply(write(f'w{i}', HOLLOW + '\n' * i), check(f'c{i}')) for i in range(7)],
             'repair_cap'),
            ([reply(check('a'))], 'script_exhausted'),
        ]
        for responses, reason, *cap in cases:
            with self.subTest(reason, cap=cap):
                metrics, transcript = self.scripted(responses, cap=cap[0] if cap else None)
                self.assertEqual(metrics['termination'], reason)
                self.assertEqual(metrics['input_cap'], cap[0] if cap else 40000)
                if reason == 'input_cap':
                    self.assertEqual(metrics['input_budget'], {
                        'cap': 40000, 'used': 39000, 'remaining': 1000, 'refused_request': 2000})
                if cap:
                    self.assertEqual(metrics['input_budget']['remaining'], 60000 - 39000 - 2000)
                    self.assertIsNone(metrics['input_budget']['refused_request'])
                self.assertFalse(metrics['success'])
                if reason == 'repair_cap':
                    self.assertEqual(metrics['submissions'], 5)
                result = self.run_cli('replay', '--sakura', SAKURA, '--transcript',
                                      str(transcript))
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_unauthorized_and_escaping_edits(self):
        metrics, _ = self.scripted([
            reply(tool('write_file', 'a', path='../escape.txt', content='x'),
                  tool('write_file', 'b', path='sakura.toml', content='[package]\n'),
                  tool('read_file', 'c', path='/etc/hosts')),
            reply(write('d', GOLD), check('e')), done()])
        self.assertEqual(metrics['unauthorized_edits'], ['../escape.txt', 'sakura.toml'])
        self.assertFalse(metrics['success'])
        self.assertFalse((TASKS.parent / 'escape.txt').exists())

    def test_retrieval_telemetry(self):
        metrics, transcript = self.scripted([
            reply(tool('load_skill', 'a', path='SKILL.md'), inp=1500),
            reply(tool('load_skill', 'b', path='missing.md'),
                  tool('sakura', 'c', args=['explain', 'V3001']), inp=2500),
            reply(tool('sakura', 'd', args=['doc', 'len']),
                  tool('sakura', 'e', args=['doc', 'no_such_symbol']), check('f')),
            done()], condition='skill')
        documents = metrics['documents']
        self.assertEqual([(d['id'], d['ok'], d['input_used'], d['turn']) for d in documents], [
            ('skill:SKILL.md', True, 1500, 1), ('skill:missing.md', False, 4000, 2),
            ('explain V3001', True, 4000, 2), ('doc len', True, 5000, 3),
            ('doc no_such_symbol', False, 5000, 3)])
        tools = {e['id']: e['output'] for e in map(json.loads, transcript.read_text().splitlines())
                 if e['event'] == 'tool'}
        for document, uid in zip(documents, 'abcde'):
            self.assertEqual(document['bytes'], len(tools[uid].encode()))
            self.assertEqual(document['approx_tokens'], -(-document['bytes'] // 4))
        # The entry point must fit below the harness's 8,000-character tool-output cut.
        self.assertEqual([d['truncated'] for d in documents], [False, False, False, False, False])
        diagnostics = metrics['diagnostics']
        self.assertEqual(diagnostics['reported'], ['V1002', 'V2001'])
        self.assertIn('V3001', diagnostics['quoted'])
        self.assertNotIn('V3001', diagnostics['reported'])
        self.assertEqual(metrics['input_budget'], {'cap': 40000, 'used': 6000,
                                                   'remaining': 34000, 'refused_request': None})
        result = self.run_cli('replay', '--sakura', SAKURA, '--transcript', str(transcript))
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_replay_refuses_other_harness_versions(self):
        old = self.tmp / 'old.jsonl'
        old.write_text((HERE / 'fixtures/03-path-label.repair.jsonl').read_text()
                       .replace('"harness": "2"', '"harness": "1"'))
        result = self.run_cli('replay', '--sakura', SAKURA, '--transcript', str(old))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('recorded by harness 1', result.stderr)

    def test_skill_condition_records_activation(self):
        metrics, _ = self.scripted([reply(tool('load_skill', 'a', path='SKILL.md')),
                                    reply(write('b', GOLD), check('c')), done()],
                                   condition='skill')
        self.assertTrue(metrics['skill_loaded'])
        self.assertEqual([d['id'] for d in metrics['documents']], ['skill:SKILL.md'])
        self.assertTrue(metrics['pass_at_1'])
        metrics, _ = self.scripted([reply(tool('load_skill', 'a', path='SKILL.md')), done()])
        self.assertIsNone(metrics['skill_loaded'])
        self.assertEqual(metrics['submissions'], 0)


class Mock(http.server.BaseHTTPRequestHandler):
    responses, seen, fail = [], [], False

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['content-length'])))
        Mock.seen.append((self.path, {k.lower(): v for k, v in self.headers.items()}, body))
        if Mock.fail and self.path == '/v1/messages':
            self.send_response(500)
            self.end_headers()
            return
        if self.path == '/v1/messages/count_tokens':
            payload = {'input_tokens': Mock.responses[0]['usage']['input_tokens']}
        else:
            payload = Mock.responses.pop(0)
        data = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header('content-length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *args):
        pass


class LiveMode(Base):
    def setUp(self):
        super().setUp()
        self.server = http.server.HTTPServer(('127.0.0.1', 0), Mock)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.base = f'http://127.0.0.1:{self.server.server_port}'
        self.ledger = self.tmp / 'ledger.json'
        self.ledger.write_text(json.dumps({'schema_version': 1, 'owner_cap_usd': 10.0,
                                           'entries': []}))
        Mock.responses, Mock.seen, Mock.fail = [], [], False

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()

    def live(self, *extra, key='sk-test-not-real'):
        return self.run_cli('run', '--sakura', SAKURA, '--backend', 'live', '--tasks', TASK,
                            '--conditions', 'baseline', '--model', PRICES, '--ledger',
                            str(self.ledger), '--api-base', self.base, '--sandbox', 'none',
                            '--out', str(self.tmp / 'live'), '--run-id', 'mock', *extra,
                            env={'ANTHROPIC_API_KEY': key})

    def test_refusals(self):
        approved = ('--budget-usd', '1', '--approved-by', 'icantcodefyi')
        for extra, why in [((), 'require --budget-usd'),
                           (('--budget-usd', '1'), 'require --budget-usd'),
                           (('--budget-usd', '1', '--approved-by', 'someone'), 'only the owner'),
                           (('--budget-usd', '11', '--approved-by', 'icantcodefyi'), 'must be in')]:
            result = self.live(*extra)
            self.assertEqual(result.returncode, 2)
            self.assertIn(why, result.stderr)
        self.assertIn('ANTHROPIC_API_KEY', self.live(*approved, key='').stderr)
        result = self.run_cli('run', '--sakura', SAKURA, '--backend', 'live', '--tasks', TASK,
                              '--model', 'claude-test=5/25', *approved, '--ledger',
                              str(self.ledger), '--api-base', self.base, '--sandbox', 'none')
        self.assertIn('prices must be', result.stderr)
        result = self.run_cli('run', '--sakura', SAKURA, '--backend', 'live', '--tasks', TASK,
                              '--model', PRICES, *approved, '--ledger', str(self.ledger),
                              '--sandbox', 'none')
        self.assertIn('requires --sandbox auto', result.stderr)
        self.assertIn('is missing', self.live(*approved, '--ledger', str(self.tmp / 'no')).stderr)
        self.assertEqual(Mock.seen, [])

    def test_spend_cap_refuses_episode(self):
        # $9.80 already spent; one task episode may cost up to $0.24 at $4/$20.
        self.ledger.write_text(json.dumps({'schema_version': 1, 'owner_cap_usd': 10.0, 'entries': [
            {'run_id': 'old', 'episode': 'x', 'model': 'm', 'turn': 0, 'status': 'settled',
             'reserved_usd': 9.8, 'cost_usd': 9.8, 'input_tokens': 0, 'output_tokens': 0}]}))
        result = self.live('--budget-usd', '10', '--approved-by', 'icantcodefyi')
        self.assertEqual(result.returncode, 2)
        self.assertIn('stopping', result.stdout)
        self.assertEqual(Mock.seen, [])
        result = self.live('--budget-usd', '0.1', '--approved-by', 'icantcodefyi')
        self.assertIn('stopping', result.stdout)

    def test_live_episode_against_mock(self):
        Mock.responses = [dict(r) for r in REPAIR]
        result = self.live('--budget-usd', '1', '--approved-by', 'icantcodefyi')
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        sent = [s for s in Mock.seen if s[0] == '/v1/messages']
        self.assertEqual(len(sent), 4)
        for _, headers, body in sent:
            self.assertEqual(headers['x-api-key'], 'sk-test-not-real')
            self.assertEqual(headers['anthropic-version'], '2023-06-01')
            self.assertEqual(body['model'], 'claude-test')
            self.assertLessEqual(body['max_tokens'], 4000)
            self.assertNotIn('cache_control', json.dumps(body))
        self.assertEqual(sent[1][2]['max_tokens'], 3900)
        entries = json.loads(self.ledger.read_text())['entries']
        self.assertEqual([e['status'] for e in entries], ['settled'] * 4)
        self.assertAlmostEqual(sum(e['cost_usd'] for e in entries), (4000 * 4 + 400 * 20) / 1e6)
        for path in (self.tmp / 'live').iterdir():
            self.assertNotIn('sk-test-not-real', path.read_text())
        metrics = json.loads((self.tmp / 'live/summary.json').read_text())['episodes'][0]
        self.assertTrue(metrics['success'])

    def test_paired_input_arms_plan(self):
        approved = ('--budget-usd', '1', '--approved-by', 'icantcodefyi', '--plan-only')
        result = self.live(*approved, '--input-caps', '40000,60000')
        self.assertEqual(result.returncode, 0, result.stderr)
        plan = json.loads(result.stdout.splitlines()[0])
        self.assertEqual(sorted(plan['order']), [f'{TASK}.baseline.claude-test.in40000',
                                                 f'{TASK}.baseline.claude-test.in60000'])
        self.assertAlmostEqual(plan['maximum_usd'],
                               ((40000 + 60000) * 4 + 2 * 4000 * 20) / 1e6)
        for bad in ('70000', '40000,40000', '0'):
            result = self.live(*approved, '--input-caps', bad)
            self.assertNotEqual(result.returncode, 0)
        self.assertEqual(Mock.seen, [])

    def test_episode_cap(self):
        entry = {'run_id': 'old', 'model': 'm', 'turn': 0, 'status': 'settled',
                 'reserved_usd': 0.0, 'cost_usd': 0.0, 'input_tokens': 0, 'output_tokens': 0}
        self.ledger.write_text(json.dumps({'schema_version': 1, 'owner_cap_usd': 10.0, 'entries': [
            dict(entry, episode=str(i)) for i in range(48)]}))
        result = self.live('--budget-usd', '1', '--approved-by', 'icantcodefyi')
        self.assertIn('48-episode cap', result.stderr)
        self.assertEqual(Mock.seen, [])

    def test_failed_request_keeps_reservation(self):
        Mock.responses, Mock.fail = [dict(r) for r in REPAIR], True
        result = self.live('--budget-usd', '1', '--approved-by', 'icantcodefyi')
        self.assertEqual(result.returncode, 3, result.stdout)
        entries = json.loads(self.ledger.read_text())['entries']
        self.assertEqual([e['status'] for e in entries], ['unknown'])
        self.assertAlmostEqual(entries[0]['reserved_usd'], (1000 * 4 + 4000 * 20) / 1e6)


if __name__ == '__main__':
    unittest.main()
