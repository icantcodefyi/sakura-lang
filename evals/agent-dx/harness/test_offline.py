"""Offline task scheduling and deterministic reports; no compiler or API calls."""
import contextlib
import importlib.util
import io
from pathlib import Path
import sys
import threading
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('offline', Path(__file__).with_name('offline.py'))
offline = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(offline)


class OfflineSchedulingTests(unittest.TestCase):
    def report(self, arguments, exercise):
        output = io.StringIO()
        with patch.object(sys, 'argv', ['offline.py', '--sakura', 'unused', *arguments]), \
                patch.object(offline, 'exercise', exercise), contextlib.redirect_stdout(output):
            offline.main()
        return output.getvalue()

    def test_parallel_report_matches_serial_in_task_order(self):
        tasks = sorted((offline.SUITE / 'tasks').iterdir())
        expected = ''.join(f'{task.name}\n' for task in tasks)
        serial = self.report(['--jobs', '1'], lambda task, *_: task.name)
        second_finished = threading.Event()
        completed = []

        def exercise(task, *_):
            if task == tasks[0]:
                self.assertTrue(second_finished.wait(5), 'second task never ran concurrently')
            completed.append(task.name)
            if task == tasks[1]:
                second_finished.set()
            return task.name

        parallel = self.report(['--jobs', '2'], exercise)
        self.assertEqual(completed[0], tasks[1].name)
        self.assertEqual(serial, expected)
        self.assertEqual(parallel, serial)

    def test_default_without_cpu_count(self):
        with patch.object(offline.os, 'cpu_count', return_value=None):
            report = self.report([], lambda task, *_: task.name)
        tasks = sorted((offline.SUITE / 'tasks').iterdir())
        self.assertEqual(report, ''.join(f'{task.name}\n' for task in tasks))

    def test_task_failure_names_task_without_partial_report(self):
        tasks = sorted((offline.SUITE / 'tasks').iterdir())

        def exercise(task, *_):
            if task == tasks[-1]:
                raise AssertionError('fixture rejected')
            return task.name

        output = io.StringIO()
        with patch.object(sys, 'argv', ['offline.py', '--sakura', 'unused', '--jobs', '2']), \
                patch.object(offline, 'exercise', exercise), contextlib.redirect_stdout(output):
            with self.assertRaisesRegex(AssertionError, f'{tasks[-1].name}: fixture rejected'):
                offline.main()
        self.assertEqual(output.getvalue(), '')

    def test_selected_task(self):
        name = '03-path-label'
        self.assertEqual(self.report(['--jobs', '32', '--task', name],
                                     lambda task, *_: task.name), f'{name}\n')

    def test_invalid_jobs(self):
        for value in ('0', '-1', 'invalid'):
            with self.subTest(value=value), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    self.report(['--jobs', value], lambda task, *_: task.name)
                self.assertEqual(error.exception.code, 2)


if __name__ == '__main__':
    unittest.main()
