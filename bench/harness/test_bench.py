"""Harness tests (plan §4.1 WS-B): `python3 -m unittest discover bench/harness`."""

import contextlib
import io
import json
import shutil
import tempfile
import textwrap
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from pathlib import Path

import bench


def doc(rows, rss=1000):
    """A sakura-bench-v1 document from {(program, variant, engine, size): median_ms}."""
    return {"schema": bench.SCHEMA, "meta": {"failures": []},
            "results": [{"program": p, "variant": v, "engine": e, "lane": "matched", "ints": "js-safe", "size": n,
                         "samples_ms": [ms], "median_ms": ms, "process_medians_ms": [ms], "rss_bytes": rss,
                         "checksum_ok": True, "checksum": "1"} for (p, v, e, n), ms in rows.items()]}


def pair(sakura_ms, js_ms, program="p", n=100):
    return {(program, "sakura", "node", n): sakura_ms, (program, "js", "node", n): js_ms}


class Compare(unittest.TestCase):
    base = doc(pair(20.0, 10.0))  # ratio 2.0

    def test_twelve_percent_blocks(self):
        out = bench.compare(self.base, [doc(pair(22.4, 10.0))])  # ratio 2.24, +12%, +2.4 ms
        self.assertEqual(out["verdict"], "blocked")

    def test_six_percent_flags(self):
        out = bench.compare(self.base, [doc(pair(21.2, 10.0))])  # +6%
        self.assertEqual(out["verdict"], "flagged")

    def test_small_absolute_regression_only_flags(self):
        base = doc(pair(0.2, 0.1))
        out = bench.compare(base, [doc(pair(0.224, 0.1))])  # +12% but 0.024 ms per workload
        self.assertEqual(out["verdict"], "flagged")

    def test_block_needs_the_confirmation_run(self):
        out = bench.compare(self.base, [doc(pair(22.4, 10.0)), doc(pair(20.2, 10.0))])
        self.assertEqual(out["verdict"], "flagged")
        out = bench.compare(self.base, [doc(pair(22.4, 10.0)), doc(pair(22.6, 10.0))])
        self.assertEqual(out["verdict"], "blocked")

    def test_ratio_cancels_a_uniformly_slower_machine(self):
        out = bench.compare(self.base, [doc(pair(40.0, 20.0))])
        self.assertEqual(out["verdict"], "ok")

    def test_complexity_change_fails_immediately(self):
        sizes = [1000, 2000, 4000, 8000]
        lin = {k: v for n in sizes for k, v in pair(n * 0.001, n * 0.0005, n=n).items()}
        quad = {k: v for n in sizes for k, v in pair(n * n * 1e-7, n * 0.0005, n=n).items()}
        out = bench.compare(doc(lin), [doc(quad)])
        self.assertEqual(out["verdict"], "blocked")
        self.assertTrue(any("complexity" in i for i in out["immediate"]))

    def test_checksum_changes_fail_immediately(self):
        head = doc(pair(20.0, 10.0))
        head["results"][0]["checksum"] = "2"
        out = bench.compare(self.base, [head])
        self.assertTrue(any("checksum changed" in i for i in out["immediate"]))
        self.assertEqual(out["verdict"], "blocked")


def rss_doc(peaks, engine="node"):
    result = doc({("p", "sakura", engine, 100): 20.0, ("p", "js", engine, 100): 10.0}, rss=max(peaks))
    result["meta"]["flags"] = {"processes": len(peaks)}
    for row in result["results"]:
        row["process_rss_bytes"] = list(peaks)
    return result


class RssComparison(unittest.TestCase):
    def test_node_crossing_confirmed_blocks_even_with_only_one_peak(self):
        base = rss_doc([1000] * 5)
        head = rss_doc([1000, 1000, 1000, 1000, 3000])
        confirmation = rss_doc([1000, 1000, 1000, 1000, 1300])
        out = bench.compare(base, [head, confirmation])
        self.assertEqual(out["verdict"], "blocked")
        self.assertFalse(out["immediate"])
        self.assertEqual(out["rss"][0]["head_median_bytes"], 1000)
        self.assertEqual(out["rss"][0]["head_bytes"], 3000)
        self.assertEqual(out["rss"][0]["confirmation_bytes"], 1300)
        self.assertEqual(out["rss"][0]["confirmation_median_bytes"], 1000)
        self.assertEqual(out["rss"][0]["confirmation_processes"], 5)
        self.assertEqual(out["confirmation_programs"], [])

    def test_node_crossing_not_confirmed_passes_with_note(self):
        base, head = rss_doc([1000] * 5), rss_doc([1300] * 5)
        out = bench.compare(base, [head, base])
        self.assertEqual(out["verdict"], "flagged")
        self.assertEqual(out["rss"][0]["status"], "flagged")
        self.assertIn("not confirmed", out["rss"][0]["note"])
        self.assertEqual(out["confirmation_programs"], [])
        self.assertFalse(out["immediate"])

    def test_node_first_group_crossing_requests_one_confirmation(self):
        out = bench.compare(rss_doc([1000] * 5), [rss_doc([1300] * 5)])
        self.assertEqual(out["verdict"], "flagged")
        self.assertEqual(out["confirmation_programs"], ["p"])
        self.assertIn("needs one confirmation", out["rss"][0]["note"])

    def test_bun_crossing_warns_even_when_confirmed(self):
        base, head = rss_doc([1000] * 5, "bun"), rss_doc([1300] * 5, "bun")
        for heads in ([head], [head, head]):
            out = bench.compare(base, heads)
            self.assertEqual(out["verdict"], "flagged")
            self.assertEqual(out["rss"][0]["status"], "flagged")
            self.assertIn("advisory", out["rss"][0]["note"])
            self.assertEqual(out["confirmation_programs"], [])
            self.assertFalse(out["immediate"])

    def test_confirmation_must_cross_in_the_same_cell(self):
        base, head, confirmation = (rss_doc([1000] * 5), rss_doc([1300] * 5), rss_doc([1000] * 5))
        # A rise in the other variant cannot confirm the Sakura peak.
        head["results"][1] = base["results"][1]
        confirmation["results"][1] = rss_doc([1400] * 5)["results"][1]
        out = bench.compare(base, [head, confirmation])
        self.assertEqual(out["verdict"], "flagged")
        self.assertTrue(all(row["status"] == "flagged" for row in out["rss"]))
        confirmation["results"] = []
        self.assertEqual(bench.compare(base, [head, confirmation])["verdict"], "flagged")

    def test_only_one_confirmation_group_decides_rss(self):
        base, head = rss_doc([1000] * 5), rss_doc([1300] * 5)
        self.assertEqual(bench.compare(base, [head, head, base])["verdict"], "blocked")
        self.assertEqual(bench.compare(base, [head, base, head])["verdict"], "flagged")

    def test_median_rise_is_diagnostic_and_does_not_change_the_maximum_rule(self):
        out = bench.compare(rss_doc([1000, 1000, 1000, 1000, 3000]), [rss_doc([1300] * 5)])
        self.assertEqual(out["verdict"], "ok")
        self.assertEqual(out["rss"][0]["base_bytes"], 3000)
        self.assertEqual(out["rss"][0]["base_median_bytes"], 1000)
        self.assertEqual(out["rss"][0]["head_median_bytes"], 1300)

    def test_native_keeps_maximum_gate_without_native_variance_evidence(self):
        out = bench.compare(rss_doc([1000] * 5, "native"),
                            [rss_doc([1000, 1000, 1000, 1000, 1300], "native")])
        self.assertEqual(out["verdict"], "blocked")
        self.assertIn("maximum", out["rss"][0]["statistic"])

    def test_twenty_five_percent_boundary_is_unchanged(self):
        base = rss_doc([1000] * 5)
        self.assertEqual(bench.compare(base, [rss_doc([1250] * 5)])["verdict"], "ok")
        self.assertEqual(bench.compare(base, [rss_doc([1251] * 5), rss_doc([1251] * 5)])["verdict"], "blocked")

    def test_legacy_either_side_uses_maximum_on_both_sides(self):
        legacy = doc(pair(20.0, 10.0), rss=1000)
        new = rss_doc([1000, 1000, 1000, 1000, 3000])
        out = bench.compare(legacy, [new, new])
        self.assertEqual(out["verdict"], "blocked")
        self.assertIn("maximum", out["rss"][0]["statistic"])
        self.assertEqual(out["rss"][0]["head_bytes"], 3000)
        out = bench.compare(new, [doc(pair(20.0, 10.0), rss=4000)] * 2)
        self.assertEqual(out["verdict"], "blocked")
        self.assertEqual(out["rss"][0]["base_bytes"], 3000)

    def test_fewer_than_five_processes_preserves_maximum_gate(self):
        for count in (1, 2, 3, 4):
            out = bench.compare(rss_doc([1000] * 5), [rss_doc([1000] * (count - 1) + [1300])] * 2)
            self.assertEqual(out["verdict"], "blocked")
            self.assertIn("maximum", out["rss"][0]["statistic"])

    def test_all_samples_used_when_more_than_five_processes_requested(self):
        out = bench.compare(rss_doc([1000] * 7), [rss_doc([1000] * 3 + [1300] * 4)] * 2)
        self.assertEqual(out["verdict"], "blocked")
        self.assertEqual(out["rss"][0]["head_processes"], 7)

    def test_malformed_or_incomplete_samples_block_instead_of_falling_back(self):
        for bad in ([], [1000] * 4, [1000] * 4 + [0], [1000] * 4 + [True],
                    [1000] * 4 + [float("nan")], [1000] * 4 + [float("inf")],
                    [1000] * 4 + ["1000"], [1000] * 4 + [2000], None):
            for bad_side in ("base", "head"):
                with self.subTest(bad=bad, side=bad_side):
                    base, head = rss_doc([1000] * 5), rss_doc([1000] * 5)
                    (base if bad_side == "base" else head)["results"][0]["process_rss_bytes"] = bad
                    out = bench.compare(base, [head])
                    self.assertEqual(out["verdict"], "blocked")
                    self.assertTrue(any("invalid/incomplete process RSS" in message for message in out["immediate"]))

    def test_cli_reports_both_groups_and_exit_status(self):
        cases = [("node", 1300, 1400, bench.EXIT_BLOCKED, "confirmed"),
                 ("node", 1300, 1000, bench.EXIT_OK, "not confirmed"),
                 ("bun", 1300, 1400, bench.EXIT_OK, "advisory")]
        with tempfile.TemporaryDirectory() as tmp:
            base, head, confirmation, report = [Path(tmp) / f"{name}.json"
                                                for name in ("base", "head", "confirmation", "report")]
            for engine, first, second, expected, note in cases:
                with self.subTest(engine=engine, second=second):
                    for path, value in ((base, 1000), (head, first), (confirmation, second)):
                        path.write_text(json.dumps(rss_doc([value] * 5, engine)))
                    stdout = io.StringIO()
                    with contextlib.redirect_stdout(stdout):
                        code = bench.main(["compare", str(base), str(head), str(confirmation), "--json", str(report)])
                    self.assertEqual(code, expected)
                    row = json.loads(report.read_text())["rss"][0]
                    self.assertEqual(row["statistic"], "maximum")
                    self.assertEqual(row["head_bytes"], first)
                    self.assertEqual(row["confirmation_bytes"], second)
                    self.assertIn(f"maximum 1000 → {first}", stdout.getvalue())
                    self.assertIn(f"confirmation maximum {second}", stdout.getvalue())
                    self.assertIn("median is diagnostic only", stdout.getvalue())
                    self.assertIn(note, stdout.getvalue())

    def test_malformed_or_failed_confirmation_still_blocks(self):
        base, head = rss_doc([1000] * 5), rss_doc([1300] * 5)
        for failure in ("samples", "checksum", "changed", "oracle"):
            with self.subTest(failure=failure):
                confirmation = rss_doc([1000] * 5)
                if failure == "samples":
                    confirmation["results"][0]["process_rss_bytes"] = [1000] * 4
                elif failure == "checksum":
                    confirmation["results"][0]["checksum_ok"] = False
                elif failure == "changed":
                    confirmation["results"][0]["checksum"] = "2"
                else:
                    confirmation["meta"]["failures"] = [{"program": "p", "variant": "sakura", "engine": "node", "error": "oracle failed"}]
                out = bench.compare(base, [head, confirmation])
                self.assertEqual(out["verdict"], "blocked")
                self.assertTrue(out["immediate"])

    def test_checksum_failure_still_blocks_when_median_is_stable(self):
        base, head = rss_doc([1000] * 5), rss_doc([1000] * 5)
        head["results"][0]["checksum_ok"] = False
        out = bench.compare(base, [head])
        self.assertEqual(out["verdict"], "blocked")
        self.assertTrue(any("checksum" in message for message in out["immediate"]))


class CiConfirmation(unittest.TestCase):
    def test_ci_reuses_one_five_process_rerun_for_rss_and_timing(self):
        workflow = (bench.ROOT / ".github/workflows/bench.yml").read_text()
        script = textwrap.dedent(workflow[workflow.index("          import copy, json, os, pathlib, subprocess"):
                                          workflow.index("          PY", workflow.index("          import copy, json, os, pathlib, subprocess"))])
        cases = [("node", 1300, 1400, 20.0, 2, 2),
                 ("node", 1300, 1000, 20.0, 0, 2),
                 ("bun", 1300, 1400, 20.0, 0, 1),
                 ("node", 1300, 1400, 22.4, 2, 2),
                 ("bun", 1300, 1400, 22.4, 2, 2)]
        for engine, first, second, sakura_ms, expected_code, expected_runs in cases:
            with self.subTest(engine=engine, second=second, sakura_ms=sakura_ms), tempfile.TemporaryDirectory() as tmp:
                base = rss_doc([1000] * 5, engine)
                groups = [rss_doc([first] * 5, engine), rss_doc([second] * 5, engine)]
                for group in (base, *groups):
                    group["meta"].update(host={"id": "test", "os": "Darwin", "cpu": "test"}, started="2026-10-18")
                for group in groups:
                    group["results"][0]["median_ms"] = sakura_ms
                root = Path(tmp)
                (root / "bench-results").mkdir()
                baseline = root / "baseline.json"
                baseline.write_text(json.dumps(base))
                runs = []

                def command(args, check=False):
                    self.assertEqual(args[0], "bench")
                    if args[1] == "run":
                        self.assertEqual(args[args.index("--processes") + 1], "5")
                        if runs:
                            self.assertEqual(args[args.index("--filter") + 1], "p")
                        path = Path(args[args.index("--json") + 1])
                        path.write_text(json.dumps(groups[len(runs)]))
                        runs.append(path)
                        return SimpleNamespace(returncode=0)
                    return SimpleNamespace(returncode=bench.main(args[1:]))

                stdout = io.StringIO()
                with (contextlib.chdir(root), contextlib.redirect_stdout(stdout),
                      patch.dict("os.environ", {"BENCH_ENGINE": engine}),
                      patch.object(Path, "glob", return_value=[baseline]),
                      patch.object(bench.subprocess, "run", side_effect=command)):
                    with self.assertRaises(SystemExit) as stopped:
                        exec(compile(script, "bench.yml:perf", "exec"), {})
                self.assertEqual(stopped.exception.code, expected_code)
                self.assertEqual(len(runs), expected_runs)
                self.assertIn("::warning::p", stdout.getvalue())
                if expected_runs == 2:
                    report = json.loads((root / "bench-results" / f"confirmed-{engine}-0.json").read_text())
                    self.assertEqual(report["rss"][0]["head_bytes"], first)
                    self.assertEqual(report["rss"][0]["confirmation_bytes"], second)


class RssCollection(unittest.TestCase):
    def test_run_retains_every_process_peak_including_outliers(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "run.json"
            peaks = [100, 110, 105, 1000, 115]
            with (patch.object(bench, "build_cli"), patch.object(bench, "build_sakura", return_value="unused"),
                  patch.object(bench, "oracle"), patch.object(bench, "programs", return_value=["p"]),
                  patch.object(bench, "variants", return_value=[("sakura", "node")]),
                  patch.object(bench, "meta", return_value={"sizes": [1], "ints": "js-safe", "checksums": {"1": "1"}}),
                  patch.object(bench, "measure", side_effect=[([1.0] * 9, "1", r) for r in peaks]),
                  patch.object(bench, "new_result", return_value={"meta": {"missing": [], "failures": []}, "results": []})):
                code = bench.main(["run", "--engine", "node", "--json", str(output)])
            row = json.loads(output.read_text())["results"][0]
            self.assertEqual(code, bench.EXIT_OK)
            self.assertEqual(row["process_rss_bytes"], peaks)
            self.assertEqual(row["rss_bytes"], 1000)
            self.assertEqual(len(row["samples_ms"]), 45)


FAKE_NATIVE = """#!/usr/bin/env python3
import json, sys
fib = lambda n: n if n < 2 else fib(n - 1) + fib(n - 2)
mode, args = sys.argv[1], sys.argv[2:]
if mode == "values":
    print(json.dumps({"values": {a: str(fib(int(a))) for a in args}}))
else:
    for _ in range(9):
        print(json.dumps({"n": int(args[0]), "ms": 0.5, "checksum": str(fib(int(args[0])))}))
"""


class NativeProtocol(unittest.TestCase):
    def test_values_and_measure_lines_are_parsed(self):
        tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp)
        exe = tmp / "main"
        exe.write_text(FAKE_NATIVE)
        exe.chmod(0o755)
        m = bench.meta("fib")
        bench.oracle("fib", m, "rust", "native", exe)  # raises on a mismatch
        samples, checksum, rss = bench.measure("rust", "native", exe, 20, False)
        self.assertEqual((len(samples), checksum), (9, "6765"))
        self.assertGreater(rss, 0)
        m["checksums"]["20"] = "1"
        with self.assertRaises(bench.Fail):
            bench.oracle("fib", m, "rust", "native", exe)


class NativeBuilds(unittest.TestCase):
    def check_port(self, variant, builder):
        # Real shared modules, linking and module resolution; no mocked compiler command.
        bench.TARGET.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="native_build_", dir=bench.TARGET) as tmp:
            with patch.object(bench, "TARGET", Path(tmp)):
                exe = builder("binary_trees", "matched")
                bench.oracle("binary_trees", bench.meta("binary_trees"), variant, "native", exe)
                samples, checksum, _rss = bench.measure(variant, "native", exe, 8, False)
                self.assertEqual(len(samples), 9)
                self.assertEqual(checksum, bench.meta("binary_trees")["checksums"]["8"])
                self.assertTrue(all(0 <= ms < float("inf") for ms in samples))

    @unittest.skipUnless(shutil.which("ocamlopt"), "OCaml toolchain unavailable")
    def test_ocaml_shared_port_builds_and_runs(self):
        self.check_port("ocaml", bench.build_ocaml)

    @unittest.skipUnless(shutil.which("koka"), "Koka toolchain unavailable")
    def test_koka_shared_port_with_underscore_builds_and_runs(self):
        self.check_port("koka", bench.build_koka)


class Oracle(unittest.TestCase):
    def test_wrong_checksum_is_refused_and_not_timed(self):
        tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp)
        src = bench.PROGRAMS / "fib"
        bad = tmp / "programs" / "fib"
        shutil.copytree(src, bad, ignore=shutil.ignore_patterns("rust", "go", "ocaml", "koka"))
        text = (bad / "meta.toml").read_text().replace('"20" = "6765"', '"20" = "6766"')
        (bad / "meta.toml").write_text(text)
        saved = bench.PROGRAMS, bench.TARGET
        bench.PROGRAMS, bench.TARGET = tmp / "programs", tmp / "target"
        try:
            code = bench.main(["run", "--engine", "node", "--sizes", "quick", "--processes", "1",
                               "--json", str(tmp / "out.json")])
        finally:
            bench.PROGRAMS, bench.TARGET = saved
        out = json.loads((tmp / "out.json").read_text())
        self.assertEqual(code, bench.EXIT_ORACLE)
        self.assertEqual(out["results"], [])
        self.assertTrue(any("oracle failed" in f["error"] for f in out["meta"]["failures"]))


def snapshot(load, *busy):
    return {"utc": "2026-10-18T00:00:00Z", "load_1m": load,
            "busiest": [{"executable": name, "cpu_percent": cpu} for name, cpu in busy]}


def loaded(document, load=10.4, ci=False):
    """Mark a document as measured on a host with this one-minute load (§17.153)."""
    verdict = bench.quiet_verdict([snapshot(load)], 12, ci)
    document["meta"]["host"] = {"id": "test", "quiet": verdict["quiet"], "load": verdict}
    return document


class QuietWindow(unittest.TestCase):
    def test_a_run_over_the_load_threshold_is_not_quiet(self):
        verdict = bench.quiet_verdict([snapshot(1.0), snapshot(3.2), snapshot(2.0)], 12, False)
        self.assertFalse(verdict["quiet"])
        self.assertEqual((verdict["threshold"], verdict["max_load_1m"]), (3.0, 3.2))
        self.assertIn("one-minute load 3.20 > 3", verdict["reasons"][0])
        self.assertTrue(bench.quiet_verdict([snapshot(3.0), snapshot(0.5)], 12, False)["quiet"])

    def test_a_busy_build_benchmark_or_agent_process_is_not_quiet(self):
        for name in ("rustc", "cargo", "node", "bun", "claude", "Python", "clang-21", "ld64.lld"):
            with self.subTest(name=name):
                verdict = bench.quiet_verdict([snapshot(1.0, (name, 85.0))], 12, False)
                self.assertFalse(verdict["quiet"])
                self.assertIn(f"busy: {name}", verdict["reasons"])
        # Idle (paused) agents and unrelated busy programs do not count.
        self.assertTrue(bench.quiet_verdict([snapshot(1.0, ("node", 0.3), ("WindowServer", 40.0),
                                                      ("Google Chrome Helper", 30.0))], 12, False)["quiet"])

    def test_ci_is_never_quiet(self):
        verdict = bench.quiet_verdict([snapshot(0.1)], 4, True)
        self.assertFalse(verdict["quiet"])
        self.assertIn("CI runner", verdict["reasons"][0])

    def test_snapshot_reads_the_host(self):
        s = bench.load_snapshot()
        self.assertGreaterEqual(s["load_1m"], 0)
        self.assertLessEqual(len(s["busiest"]), 12)

    def test_run_records_the_verdict_in_meta_host(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "run.json"
            with (patch.object(bench, "build_cli"), patch.object(bench, "build_sakura", return_value="unused"),
                  patch.object(bench, "oracle"), patch.object(bench, "programs", return_value=["p"]),
                  patch.object(bench, "variants", return_value=[("sakura", "node")]),
                  patch.object(bench, "meta", return_value={"sizes": [1], "ints": "js-safe", "checksums": {"1": "1"}}),
                  patch.object(bench, "measure", return_value=([1.0] * 9, "1", 100)),
                  patch.object(bench, "load_snapshot", side_effect=[snapshot(1.0), snapshot(4.5, ("rustc", 90.0))]),
                  patch.dict("os.environ", {"GITHUB_ACTIONS": "false"}),
                  patch.object(bench.os, "cpu_count", return_value=12)):
                bench.main(["run", "--engine", "node", "--processes", "1", "--json", str(output)])
            host = json.loads(output.read_text())["meta"]["host"]
            self.assertIs(host["quiet"], False)
            self.assertEqual(host["load"]["max_load_1m"], 4.5)
            self.assertEqual(len(host["load"]["snapshots"]), 2)
            self.assertIn("busy: rustc", host["load"]["reasons"])


class Bless(unittest.TestCase):
    def bless(self, document):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "run.json"
            path.write_text(json.dumps(document))
            stderr = io.StringIO()
            with patch.object(bench, "BENCH", Path(tmp)), patch.object(bench, "ROOT", Path(tmp)), \
                 contextlib.redirect_stderr(stderr):
                code = bench.main(["bless", str(path)])
            return code, (Path(tmp) / "baselines" / "test.json").exists(), stderr.getvalue()

    def test_bless_refuses_a_run_that_is_not_quiet(self):
        code, written, message = self.bless(loaded(doc(pair(20.0, 10.0)), load=3.5))
        self.assertEqual((code, written), (bench.EXIT_ORACLE, False))
        self.assertIn("not made in a quiet window: one-minute load 3.50 > 3", message)
        code, written, message = self.bless(loaded(doc(pair(20.0, 10.0)), load=0.5, ci=True))
        self.assertEqual((code, written), (bench.EXIT_ORACLE, False))
        self.assertIn("CI runner", message)

    def test_bless_refuses_a_run_without_a_verdict(self):
        legacy = doc(pair(20.0, 10.0))
        legacy["meta"]["host"] = {"id": "test"}
        code, written, message = self.bless(legacy)
        self.assertEqual((code, written), (bench.EXIT_ORACLE, False))
        self.assertIn("no quiet verdict", message)

    def test_bless_copies_a_quiet_run(self):
        self.assertEqual(self.bless(loaded(doc(pair(20.0, 10.0)), load=2.9))[:2], (bench.EXIT_OK, True))


class AdvisoryCompare(unittest.TestCase):
    base = doc(pair(20.0, 10.0))

    def test_loaded_base_or_head_is_advisory_never_blocked_or_ok(self):
        regressed = doc(pair(22.4, 10.0))  # +12%: blocked on a quiet host
        for base, head in ((loaded(doc(pair(20.0, 10.0))), regressed), (self.base, loaded(doc(pair(22.4, 10.0))))):
            out = bench.compare(base, [head])
            self.assertEqual((out["verdict"], out["host_load"]), ("advisory", 10.4))
            self.assertEqual(out["rows"][0]["status"], "blocked")  # the row keeps its direction
        self.assertEqual(bench.compare(self.base, [loaded(doc(pair(20.0, 10.0)))])["verdict"], "advisory")

    def test_loaded_rss_and_complexity_are_advisory_and_need_no_confirmation(self):
        out = bench.compare(rss_doc([1000] * 5), [loaded(rss_doc([1300] * 5))])
        self.assertEqual(out["verdict"], "advisory")
        self.assertEqual(out["confirmation_programs"], [])
        out = bench.compare(rss_doc([1000] * 5, "native"), [loaded(rss_doc([1300] * 5, "native"))])
        self.assertEqual(out["verdict"], "advisory")

    def test_correctness_still_blocks_on_a_loaded_host(self):
        head = loaded(doc(pair(20.0, 10.0)))
        head["results"][0]["checksum"] = "2"
        self.assertEqual(bench.compare(self.base, [head])["verdict"], "blocked")

    def test_quiet_legacy_and_ci_runs_keep_the_gate(self):
        for head in (loaded(doc(pair(22.4, 10.0)), load=2.0), doc(pair(22.4, 10.0)),
                     loaded(doc(pair(22.4, 10.0)), load=10.4, ci=True)):
            self.assertEqual(bench.compare(self.base, [head])["verdict"], "blocked")

    def test_cli_prints_advisory_with_the_load_and_exits_zero(self):
        with tempfile.TemporaryDirectory() as tmp:
            base, head = Path(tmp) / "base.json", Path(tmp) / "head.json"
            base.write_text(json.dumps(self.base))
            head.write_text(json.dumps(loaded(doc(pair(22.4, 10.0)), load=10.4)))
            stdout = io.StringIO()
            with contextlib.redirect_stdout(stdout):
                code = bench.main(["compare", str(base), str(head)])
        self.assertEqual(code, bench.EXIT_OK)
        self.assertIn("verdict: advisory (host load 10.40", stdout.getvalue())


class KeyKindCases(unittest.TestCase):
    def test_map_set_reports_int_string_and_tuple_keys_as_separate_cells(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "run.json"
            m = bench.meta("map_set") | {"sizes": [1000]}
            calls = []

            def measure(variant, engine, artefact, n, stable, case=None):
                calls.append((variant, case))
                table = m["checksums"][case] if case else m["checksums"]
                return [1.0] * 9, table[str(n)], 100

            with (patch.object(bench, "build_cli"), patch.object(bench, "build_sakura", return_value="unused"),
                  patch.object(bench, "oracle"), patch.object(bench, "programs", return_value=["map_set"]),
                  patch.object(bench, "meta", return_value=m), patch.object(bench, "measure", side_effect=measure)):
                code = bench.main(["run", "--engine", "node", "--processes", "1", "--json", str(output)])
            rows = json.loads(output.read_text())["results"]
        cells = {(r["program"], r["variant"]) for r in rows}
        for program in ("map_set", "map_set:int", "map_set:string", "map_set:tuple"):
            self.assertIn((program, "sakura"), cells)
            self.assertIn((program, "js"), cells)
        self.assertTrue(all(r["checksum_ok"] for r in rows))
        self.assertEqual(set(calls), {(v, c) for v in ("sakura", "js") for c in (None, *m["cases"])})
        self.assertEqual(bench.cases(m, "native"), [])  # native ports implement run only
        self.assertEqual(code, bench.EXIT_OK)
        self.assertIn(("map_set:int", "node", 1000), bench.ratios({"results": rows}))

    def test_case_checksums_are_part_of_the_oracle(self):
        m = bench.meta("map_set")
        self.assertEqual(m["cases"], ["int", "string", "tuple"])
        for n in bench.oracle_inputs(m):  # the cases partition run(n)
            self.assertEqual(sum(int(m["checksums"][c][str(n)]) for c in m["cases"]), int(m["checksums"][str(n)]))
        js = bench.PROGRAMS / "map_set" / "js" / "main.mjs"
        bench.oracle("map_set", m, "js", "node", js)  # runner.mjs --fn run_<case>
        m["checksums"]["tuple"]["1000"] = "1"
        with self.assertRaisesRegex(bench.Fail, r"run_tuple\(1000\) = 5657, expected 1"):
            bench.oracle("map_set", m, "js", "node", js)


class Workflow(unittest.TestCase):
    def test_noise_runs_weekly_with_five_repeats_and_nightly_keeps_its_own_schedule(self):
        workflow = (bench.ROOT / ".github/workflows/bench.yml").read_text()
        self.assertIn('- cron: "43 3 * * 1"', workflow)
        self.assertIn("if: github.event.schedule == '43 3 * * 1' || (github.event_name == 'workflow_dispatch' "
                      "&& inputs.mode == 'noise')", workflow)
        self.assertIn("if: github.event.schedule == '17 2 * * *' ||", workflow)
        self.assertEqual(workflow.count("NOISE_REPEATS: ${{ inputs.repeats || 5 }}"), 2)
        self.assertIn("name: bench-noise-${{ matrix.os }}-${{ matrix.engine }}", workflow)


if __name__ == "__main__":
    unittest.main()
