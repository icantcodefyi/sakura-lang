"""Harness tests (plan §4.1 WS-B): `python3 -m unittest discover bench/harness`."""

import contextlib
import fnmatch
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
                  patch.object(bench, "variants", return_value=[("sakura", "js", "node")]),
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
                  patch.object(bench, "variants", return_value=[("sakura", "js", "node")]),
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

    def test_a_wasm_run_is_blessed_beside_the_js_baseline_not_over_it(self):
        run = loaded(wasm_doc(30.0, 10.0), load=2.9)
        run["meta"]["flags"] = {"suite": "wasm"}
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "run.json"
            path.write_text(json.dumps(run))
            with patch.object(bench, "BENCH", Path(tmp)), patch.object(bench, "ROOT", Path(tmp)), \
                 contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(bench.main(["bless", str(path)]), bench.EXIT_OK)
            self.assertEqual([p.relative_to(tmp).as_posix() for p in Path(tmp).rglob("test.json")],
                             ["baselines/wasm/test.json"])


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
        self.assertIn(("map_set:int", "js", "node", 1000), bench.ratios({"results": rows}))

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


IMPURE = """// Sleeps through the root Clock, which the WASI embedding does not implement.
import std/cli.{Cli}

pub fn run(n: Int) -> Int ! {Clock} {
  Clock.sleep(Duration.millis(1))
  n
}

pub fn main() -> Unit ! {Cli, Clock, Console} {
  for arg in Cli.args() { Console.print("{run(Option.expect(String.parse_int(arg), "size"))}") }
}
"""


def wasm_doc(sakura_ms, js_ms, engine="node", n=100):
    """A WASM-lane document: Sakura on the WASM backend over hand JS on Node."""
    d = doc({("p", "sakura", engine, n): sakura_ms, ("p", "js", "node", n): js_ms})
    for r in d["results"]:
        r["backend"] = "wasm" if r["variant"] == "sakura" else "js"
    return d


@unittest.skipUnless(shutil.which("wasmtime"), "wasmtime unavailable")
class WasmLane(unittest.TestCase):
    """§17.191 (2): the lane's acceptance fixtures, on real builds of the bench programs."""

    @classmethod
    def setUpClass(cls):
        bench.build_cli()

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        saved = bench.PROGRAMS, bench.TARGET, dict(bench.WASI_BUILT)
        bench.TARGET = self.tmp / "target"
        bench.WASI_BUILT.clear()

        def restore():
            bench.PROGRAMS, bench.TARGET = saved[:2]
            bench.WASI_BUILT.clear()
            bench.WASI_BUILT.update(saved[2])
        self.addCleanup(restore)

    def run_suite(self, *flags, measure=None):
        output = self.tmp / "run.json"
        fake = measure or (lambda variant, engine, art, n, stable, case=None:
                           ([1.0] * 9, bench.meta("fib")["checksums"][str(n)], 100))
        with (patch.object(bench, "measure", side_effect=fake),
              patch.object(bench, "load_snapshot", return_value=snapshot(11.0, ("cargo", 90.0))),
              patch.dict("os.environ", {"GITHUB_ACTIONS": "false"})):
            code = bench.main(["run", "--suite", "wasm", "--sizes", "quick", "--processes", "1",
                               *flags, "--json", str(output)])
        return code, json.loads(output.read_text())

    def test_a_record_carries_the_counters_backend_and_the_quiet_verdict(self):
        code, out = self.run_suite("--filter", "fib")
        self.assertEqual((code, out["meta"]["failures"], out["meta"]["absent"]), (bench.EXIT_OK, [], []))
        cells = {bench.cell(r)[1:4]: r for r in out["results"] if r["size"] == 20}
        self.assertEqual(set(cells), {("sakura", "wasm", "node"), ("sakura", "wasm", "wasmtime"), ("js", "js", "node")})
        stats = cells[("sakura", "wasm", "node")]["stats"]
        for counter in ("frames", "boxes", "dups", "drops", "restarts",
                        "reuse_attempts", "reuses", "borrow_candidates", "borrows"):
            self.assertIsInstance(stats[counter], int)
        # ABI 1.3.0's set, named by the record's `abi_version` (design §17.176 (1)).
        self.assertEqual(stats["abi_version"], 0x010f00)
        self.assertEqual(stats["borrows"], 0)
        rate = stats["reuse_rate"]
        self.assertTrue(rate is None if stats["reuse_attempts"] == 0
                        else rate == stats["reuses"] / stats["reuse_attempts"])
        self.assertEqual(cells[("sakura", "wasm", "wasmtime")]["stats"], stats)
        self.assertNotIn("stats", cells[("js", "js", "node")])
        self.assertIs(out["meta"]["host"]["quiet"], False)
        self.assertIn(("fib", "wasm", "wasmtime", 20), bench.ratios(out))

    def test_the_wasmtime_cell_is_absent_for_an_impure_program(self):
        bench.PROGRAMS = self.tmp / "programs"
        program = bench.PROGRAMS / "impure"
        (program / "sakura").mkdir(parents=True)
        (program / "js").mkdir()
        (program / "sakura" / "main.sak").write_text(IMPURE)
        (program / "js" / "main.mjs").write_text("export function run(n) { return n; }\n")
        (program / "meta.toml").write_text('sizes = [5, 6]\nints = "js-safe"\n\n[checksums]\n'
                                           + "".join(f'"{n}" = "{n}"\n' for n in (0, 1, 2, 3, 5, 6)))
        code, out = self.run_suite(measure=lambda variant, engine, art, n, stable, case=None: ([1.0] * 9, str(n), 100))
        self.assertEqual((code, out["meta"]["failures"]), (bench.EXIT_OK, []))
        [absent] = out["meta"]["absent"]
        self.assertEqual((absent["program"], absent["engine"]), ("impure", "wasmtime"))
        self.assertIn("the host operation `clock_sleep_ms` is not pure", absent["reason"])
        self.assertEqual({bench.cell(r)[1:4] for r in out["results"]}, {("sakura", "wasm", "node"), ("js", "js", "node")})

    def test_a_builtin_only_the_node_host_implements_has_no_wasmtime_cell(self):
        with self.assertRaisesRegex(bench.NoWasi, "the builtin `json_decode` exists only in the Node host"):
            bench.values("sakura", "wasmtime", "json_validate", [100])

    def test_negative_cases_defect_on_both_engines(self):
        module = bench.build_wasm("contracts")
        for engine, art in (("node", module), ("wasmtime", "contracts")):
            with self.subTest(engine=engine):
                self.assertEqual([bench.defect("sakura", engine, art, k) for k in (0, 1)], ["D020", "D021"])

    @unittest.skipUnless(bench.sys.platform == "darwin", "instruction counts read macOS's /usr/bin/time -l")
    def test_wasmtime_counts_the_instructions_of_its_calls(self):
        one, many, checksum = bench.count_calls("sakura", "wasmtime", "fib", 20, 8)
        self.assertEqual(checksum, "6765")
        self.assertGreater(many["instructions"], one["instructions"])
        self.assertEqual(bench.run(bench.calls_command("sakura", "wasmtime", "fib", 20, 3)).stdout, "6765\n" * 3)

    def test_wasmtime_measures_calls_from_its_output_lines(self):
        samples, checksum, rss = bench.measure("sakura", "wasmtime", "fib", 20, False)
        self.assertEqual((len(samples), checksum), (9, "6765"))
        self.assertTrue(all(0 < ms < 100 for ms in samples))
        self.assertGreater(rss, 0)


class WasmCompare(unittest.TestCase):
    def test_a_cell_the_base_has_cannot_go_absent_silently(self):
        base, head = wasm_doc(30.0, 10.0, "wasmtime"), wasm_doc(30.0, 10.0, "node")
        head["meta"]["absent"] = [{"program": "p", "variant": "sakura", "backend": "wasm", "engine": "wasmtime",
                                   "reason": "p: the builtin `json_decode` exists only in the Node host's JS"}]
        out = bench.compare(base, [head])
        self.assertEqual(out["verdict"], "blocked")
        self.assertIn("p sakura/wasm/wasmtime: absent", out["immediate"][0])
        self.assertEqual(bench.compare(wasm_doc(30.0, 10.0, "node"), [head])["verdict"], "ok")

    def test_wasm_cells_compare_over_hand_js_on_node(self):
        for engine in ("node", "wasmtime"):
            with self.subTest(engine=engine):
                out = bench.compare(wasm_doc(30.0, 10.0, engine), [wasm_doc(33.6, 10.0, engine)])  # +12%
                [row] = out["rows"]
                self.assertEqual((row["backend"], row["engine"], row["base_ratio"]), ("wasm", engine, 3.0))
                self.assertEqual(out["verdict"], "blocked")

    def test_wasm_and_js_backends_are_separate_cells(self):
        js, wasm = doc(pair(20.0, 10.0)), wasm_doc(40.0, 10.0)
        both = {"schema": bench.SCHEMA, "meta": {"failures": []},
                "results": wasm["results"] + [r for r in js["results"] if r["variant"] == "sakura"]}
        self.assertEqual(bench.ratios(both), {("p", "wasm", "node", 100): (40.0, 10.0), ("p", "js", "node", 100): (20.0, 10.0)})
        self.assertEqual(bench.compare(js, [wasm])["rows"], [])  # a legacy JS base has no WASM cells
        self.assertIn("ratio wasm/node", bench.summary(both))


def counted_doc(sakura, js, quiet=False, suite="js"):
    """An `--instructions` run (§17.264) from instructions per call, measured on a loaded host."""
    d = loaded(doc(pair(50.0, 10.0)), load=10.4 if not quiet else 1.0)
    d["meta"]["flags"] = {"suite": suite, "metric": "instructions"}
    for r, v in zip(d["results"], (sakura, js)):
        r["instructions"] = v
    return d


class Instructions(unittest.TestCase):
    """§17.264: instructions retired per call are the primary metric; no quiet host is needed."""

    def test_a_cell_is_the_median_of_its_pairs_over_the_median_one_call_process(self):
        c = lambda i, s=1.0: {"instructions": i, "cycles": i // 2, "wall_s": s, "rss_bytes": 7}
        runs = [(c(100), c(1100), "1"), (c(102), c(1102), "1"), (c(98), c(1120), "1")]
        row = bench.counted_row(runs, 10)
        self.assertEqual(row["process_instructions"], [100.0, 100.2, 102.0])
        self.assertEqual((row["instructions"], row["calls"]), (100.2, 10))
        self.assertAlmostEqual(row["instruction_spread"], 2.0 / 100.2)
        self.assertEqual(row["cycles"], 50.1)
        self.assertEqual(bench.counted_row([(None, None, "error: x")], 4)["instructions"], None)

    def test_calibration_doubles_to_the_budget_without_leaping_on_noise(self):
        startup, per_call = 1_300_000_000, 1_000_000
        seen = []

        def counted(cmd):
            self.assertEqual(cmd[:2], ["node", "--predictable"])  # V8's helper threads off
            calls = cmd.count("20")
            seen.append(calls)
            noise = 5_000_000 if calls == 1 else 0  # one call is inside start-up noise
            return "", {"instructions": startup + calls * per_call - noise}

        with patch.object(bench, "counted", counted):
            calls = bench.calls_for("js", "node", "m.mjs", 20)
        self.assertEqual(calls, 8192)  # 2^13 calls of 1 M ≥ 2^32 beyond one call
        self.assertEqual(seen[:3], [1, 2, 17])  # the probe after the noisy one grows 16×, not to the cap

    def test_bless_takes_a_loaded_instruction_run(self):
        code, written, _ = Bless.bless(self, counted_doc(5e6, 1e6))
        self.assertEqual((code, written), (bench.EXIT_OK, True))
        timed = loaded(doc(pair(20.0, 10.0)), load=10.4)
        self.assertEqual(Bless.bless(self, timed)[:2], (bench.EXIT_ORACLE, False))

    def test_compare_reads_instructions_and_not_the_load(self):
        out = bench.compare(counted_doc(5e6, 1e6), [counted_doc(5.6e6, 1e6)])  # +12% instructions
        self.assertEqual((out["verdict"], out["metric"], out["host_load"]), ("blocked", "instructions", None))
        self.assertEqual(bench.compare(counted_doc(5e6, 1e6), [counted_doc(5e6, 1e6)])["verdict"], "ok")
        # A timed head against a counted base (the CI job) stays on milliseconds, advisory under load.
        self.assertEqual(bench.compare(counted_doc(5e6, 1e6), [doc(pair(50.0, 10.0))])["metric"], "median_ms")

    def test_a_counted_bun_row_never_blocks_and_flags_past_the_block_threshold(self):
        """§17.265 (5): Bun is advisory in a counted compare; Node blocks as before."""
        def bun(sakura, spread=0.01):
            d = counted_doc(sakura, 1e6)
            for r in d["results"]:
                r["engine"], r["instruction_spread"] = "bun", spread
            return d
        for sakura, status in ((5.4e6, "ok"), (5.6e6, "flagged"), (9e6, "flagged"), (4e6, "improved >10%: explain in the PR")):
            with self.subTest(sakura=sakura):
                out = bench.compare(bun(5e6, 0.004), [bun(sakura, 0.021)])
                self.assertEqual(out["rows"][0]["status"], status)
                self.assertEqual(out["verdict"], "flagged" if status == "flagged" else "ok")
                self.assertEqual(out["rows"][0]["note"], "Bun advisory (§17.265 (5)); spread 0.40%, 0.40% → 2.10%, 2.10%")
        self.assertEqual(bench.compare(counted_doc(5e6, 1e6), [counted_doc(9e6, 1e6)])["verdict"], "blocked")  # Node
        self.assertEqual(bench.compare(counted_doc(5e6, 1e6), [counted_doc(5.4e6, 1e6)])["verdict"], "flagged")
        timed = lambda ms: {**doc({k[:2] + ("bun",) + k[3:]: v for k, v in pair(ms, 10.0).items()}), "meta": {"failures": []}}
        self.assertEqual(bench.compare(timed(20.0), [timed(22.4)])["verdict"], "blocked")  # a timed Bun row is unchanged

    def test_a_blocked_node_cell_is_recounted_against_the_baseline_artefact(self):
        """§17.272 (2)(b): head over an A/A recount at one location; `location` when the recount
        is off the blessed number."""
        def run(blessed, head, again, now):
            base, h = counted_doc(blessed, 1e6), counted_doc(head, 1e6)
            base["meta"]["commit"] = "abc"
            for r in h["results"]:
                r["calls"] = 16
            out = bench.compare(base, [h])
            self.assertEqual(out["verdict"], "blocked")
            seen = []

            def count(variant, engine, artefact, n, calls, case=None):
                seen.append((artefact, calls))
                v = {"base.mjs": again, "head.mjs": now}[artefact]
                c = lambda i: {"instructions": i, "cycles": i, "wall_s": 1.0, "rss_bytes": 7}
                return c(1e6), c(1e6 + v * calls), "1"

            bench.recount(base, h, out, artefacts=lambda sha, p: ("base.mjs", "head.mjs"), count=count)
            self.assertEqual(seen[:4], [("base.mjs", 16), ("head.mjs", 16), ("head.mjs", 16), ("base.mjs", 16)])
            return out
        out = run(5e6, 6.5e6, 6.4e6, 6.5e6)  # the blessed cell is the outlier: the recount reads with the head
        self.assertEqual((out["verdict"], out["rows"][0]["status"]), ("ok", "ok"))
        self.assertEqual(out["rows"][0]["recount"]["location"], True)
        self.assertIn("location", out["rows"][0]["note"])
        out = run(5e6, 6.5e6, 5.1e6, 6.5e6)  # the recount agrees with the blessed cell: still blocked
        self.assertEqual((out["verdict"], out["rows"][0]["recount"]["location"]), ("blocked", False))

    def test_summary_prints_instructions_and_the_spread(self):
        d = counted_doc(5e6, 1e6)
        d["results"][0]["instruction_spread"] = 0.004
        self.assertIn("| p | 100 | 5e+06 | 1e+06 | 5.00× | 0.40% |", bench.summary(d))
        self.assertIn("| p | 100 | 50 | 10 | 5.00× |", bench.summary(d, "median_ms"))


ENVELOPE_PROGRAMS = ["fib", "tail_clique", "map_set", "handled_d1", "json_validate"]


def window_doc(suite, rows, commit="c0ffee", host="h0", quiet=True, lane="matched"):
    """A quiet-window document of `suite` from {(program, variant, backend, engine, size): median_ms}."""
    d = doc({(p, v, e, n): ms for (p, v, _b, e, n), ms in rows.items()})
    for r, (_p, _v, b, _e, _n) in zip(d["results"], rows):
        r["backend"] = b
    verdict = bench.quiet_verdict([snapshot(1.0 if quiet else 10.4)], 12, False)
    d["meta"].update(commit=commit, flags={"suite": suite, "lane": lane}, absent=[],
                     host={"id": host, "quiet": verdict["quiet"], "load": verdict})
    return d


def envelope_docs(ratio=2.0, n=100, **meta):
    """A `--suite wasm` and a `--suite native` file in which every Sakura cell is `ratio` × every native one."""
    wasm = window_doc("wasm", {(p, "sakura", "wasm", e, n): 10.0 * ratio
                               for p in ENVELOPE_PROGRAMS for e in bench.WASM_ENGINES}, **meta)
    native = window_doc("native", {(p, v, "native", "native", n): 10.0
                                   for p in ENVELOPE_PROGRAMS for v in bench.NATIVE}, **meta)
    return wasm, native


@patch.object(bench, "programs", lambda pattern="*": fnmatch.filter(ENVELOPE_PROGRAMS, pattern))
class Envelope(unittest.TestCase):
    """§17.213 (2): the M4 envelope of §17.109 from one window's WASM and native files."""

    def statuses(self, out):
        return {(r["program"], r["engine"], r["against"]): r["status"] for r in out["rows"]}

    def test_every_cell_inside_its_ceiling_meets_the_envelope(self):
        out = bench.envelope(*envelope_docs(ratio=2.0))
        self.assertEqual((out["reading"], out["direction"]), ("met", "met"))
        self.assertEqual(set(self.statuses(out).values()), {"met"})
        lanes = {(r["program"], r["against"], r["ceiling"]) for r in out["rows"]}
        self.assertEqual(lanes, {("fib", "rust", 3.0), ("fib", "go", 2.0),
                                 ("tail_clique", "rust", 3.0), ("tail_clique", "go", 2.0),
                                 ("map_set", "ocaml", 2.5), ("map_set", "koka", 2.5), ("map_set", "koka-noreuse", 2.5),
                                 ("handled_d1", "koka", 3.0), ("json_validate", "rust", 3.0)})
        self.assertEqual(len(out["rows"]), 2 * len(lanes))  # Node host and Wasmtime

    def test_instruction_runs_read_the_ceilings_on_time_with_the_count_lane_beside(self):
        """§17.265 (2): the ceilings keep their unit, read from `median_ms` (direction only on a
        loaded host); the count ratio is printed beside them and judges nothing."""
        wasm, native = envelope_docs(ratio=2.2, quiet=False)  # 2.2× in time, loaded host
        for d, value in ((wasm, 2.6e6), (native, 1e6)):
            d["meta"]["flags"]["metric"] = "instructions"
            for r in d["results"]:
                r["instructions"] = value
        out = bench.envelope(wasm, native)
        self.assertEqual((out["metric"], out["reading"], out["direction"]),
                         ("instructions", "advisory (host load 10.40)", "miss"))  # 2.2× in time over Go's 2.0
        self.assertEqual(self.statuses(out)[("fib", "node", "rust")], "met")  # 2.6× in counts would not matter
        row = next(r for r in out["rows"] if (r["program"], r["engine"], r["against"]) == ("fib", "node", "go"))
        self.assertEqual((row["ratio"], row["count_ratio"]), (2.2, 2.6))
        text = bench.envelope_markdown(out)
        self.assertIn("| Time ratio | Ceiling | Status | Sakura instr | Native instr | Count ratio |", text)
        self.assertIn("| fib | 100 | node | go | 22 | 10 | 2.20× | 2.0 | miss | 2.6e+06 | 1e+06 | 2.60× |", text)
        self.assertIn("M4 envelope, wall-time lane on median_ms: advisory (host load 10.40), direction only: miss", text)
        self.assertIn("Count lane: Sakura/native instructions per call, a ratio of counts and not the ceilings' unit", text)
        self.assertNotIn("Count lane", bench.envelope_markdown(bench.envelope(*envelope_docs())))

    def test_a_cell_over_its_ceiling_misses_and_the_report_still_exits_zero(self):
        out = bench.envelope(*envelope_docs(ratio=2.2))  # over Go's 2.0, inside the others
        self.assertEqual(out["reading"], "miss")
        self.assertEqual(self.statuses(out)[("fib", "node", "go")], "miss")
        self.assertEqual(self.statuses(out)[("fib", "node", "rust")], "met")
        code, stdout, _ = self.cli(*envelope_docs(ratio=2.2))
        self.assertEqual(code, bench.EXIT_OK)
        self.assertIn("| fib | 100 | wasmtime | go | 22 | 10 | 2.20× | 2.0 | miss |", stdout)
        self.assertIn("M4 envelope, wall-time lane on median_ms: miss", stdout)

    def test_tail_clique_is_reported_with_countdown_but_does_not_gate(self):
        wasm, native = envelope_docs(ratio=2.0)
        for r in wasm["results"]:
            if r["program"] == "tail_clique":
                r["median_ms"] = 40.0
        out = bench.envelope(wasm, native)
        self.assertEqual(out["reading"], "met")
        self.assertEqual(self.statuses(out)[("tail_clique", "node", "go")], "miss")
        self.assertFalse(next(r for r in out["rows"] if r["program"] == "tail_clique")["gating"])

    def test_a_missing_cell_makes_the_reading_incomplete_not_met(self):
        wasm, native = envelope_docs(ratio=2.0)
        native["results"] = [r for r in native["results"] if r["variant"] != "koka-noreuse"]
        wasm["results"] = [r for r in wasm["results"] if (r["program"], r["engine"]) != ("fib", "wasmtime")]
        next(r for r in wasm["results"] if r["program"] == "handled_d1")["median_ms"] = None
        out = bench.envelope(wasm, native)
        self.assertEqual(self.statuses(out)[("handled_d1", "node", "koka")], "missing")
        self.assertEqual(out["reading"], "incomplete")
        self.assertEqual(self.statuses(out)[("map_set", "node", "koka-noreuse")], "missing")
        self.assertEqual(self.statuses(out)[("fib", "wasmtime", "rust")], "missing")
        self.assertEqual(self.statuses(out)[("fib", "node", "rust")], "met")

    def test_a_failed_checksum_is_not_a_measurement(self):
        wasm, native = envelope_docs(ratio=2.0)
        wasm["results"][0]["checksum_ok"] = False
        out = bench.envelope(wasm, native)
        self.assertEqual(out["reading"], "incomplete")
        self.assertEqual(self.statuses(out)[("fib", "node", "rust")], "checksum")

    def test_an_absent_wasmtime_cell_is_its_node_cell_neither_a_miss_nor_missing(self):
        wasm, native = envelope_docs(ratio=2.0)
        wasm["results"] = [r for r in wasm["results"] if (r["program"], r["engine"]) != ("json_validate", "wasmtime")]
        wasm["meta"]["absent"] = [{"program": "json_validate", "variant": "sakura", "backend": "wasm",
                                   "engine": "wasmtime", "reason": "the builtin `json_decode` exists only in the Node host's JS"}]
        out = bench.envelope(wasm, native)
        self.assertEqual(out["reading"], "met")
        self.assertEqual(self.statuses(out)[("json_validate", "wasmtime", "rust")], "absent")
        self.assertEqual(self.statuses(out)[("json_validate", "node", "rust")], "met")

    def test_a_loaded_run_is_advisory_with_its_load(self):
        wasm, native = envelope_docs(ratio=2.0)
        native["meta"]["host"] = window_doc("native", {}, quiet=False)["meta"]["host"]
        out = bench.envelope(wasm, native)
        self.assertEqual((out["reading"], out["direction"]), ("advisory (host load 10.40)", "met"))
        _code, stdout, _ = self.cli(wasm, native)
        self.assertIn("M4 envelope, wall-time lane on median_ms: advisory (host load 10.40), direction only: met", stdout)
        native["meta"]["host"] = loaded(window_doc("native", {}), load=1.2, ci=True)["meta"]["host"]
        self.assertEqual(bench.envelope(wasm, native)["reading"], "advisory (CI runner)")

    def test_files_from_different_commits_hosts_suites_or_lanes_are_refused(self):
        wasm, native = envelope_docs()
        cases = {"meta.commit differs": (wasm, envelope_docs(commit="beef")[1]),
                 "meta.host.id differs": (wasm, envelope_docs(host="h1")[1]),
                 "is not a `--suite wasm` run": (native, native),
                 "is not a `--suite native` run": (wasm, wasm),
                 "lane is idiomatic": (wasm, envelope_docs(lane="idiomatic")[1])}
        for reason, files in cases.items():
            with self.subTest(reason=reason):
                code, stdout, stderr = self.cli(*files)
                self.assertEqual((code, stdout), (bench.EXIT_BLOCKED, ""))
                self.assertIn(reason, stderr)

    def test_cli_writes_the_json_and_markdown_reports(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_json, out_md = Path(tmp) / "envelope.json", Path(tmp) / "envelope.md"
            code, stdout, _ = self.cli(*envelope_docs(ratio=2.0), "--json", str(out_json), "--md", str(out_md))
            self.assertEqual(code, bench.EXIT_OK)
            report = json.loads(out_json.read_text())
            self.assertEqual((report["reading"], report["commit"], report["host_id"]), ("met", "c0ffee", "h0"))
            self.assertEqual(out_md.read_text(), stdout)

    def test_the_native_panel_times_both_koka_lanes(self):
        self.assertEqual([v for v, _b, _e in bench.variants("native", [], "rbtree")],
                         ["rust", "go", "ocaml", "koka", "koka-noreuse"])
        with patch.object(bench, "sh") as sh, tempfile.TemporaryDirectory() as tmp, \
             patch.object(bench, "TARGET", Path(tmp)):
            bench.NATIVE["koka-noreuse"][2]("rbtree", "matched")
        self.assertIn("--fno-reuse", sh.call_args.args[0])

    def cli(self, wasm, native, *flags):
        with tempfile.TemporaryDirectory() as tmp:
            paths = [Path(tmp) / "wasm.json", Path(tmp) / "native.json"]
            for path, document in zip(paths, (wasm, native)):
                path.write_text(json.dumps(document))
            stdout, stderr = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                code = bench.main(["envelope", *map(str, paths), *flags])
        return code, stdout.getvalue(), stderr.getvalue()


def peaks(document, one_mb, many_mb, calls):
    """Give every row of a counted document five pairs with these one-call and many-call peaks."""
    for r in document["results"]:
        r.update(calls=calls, rss_bytes=many_mb << 20,
                 process_counters=[{"one": {"rss_bytes": one_mb << 20}, "many": {"rss_bytes": many_mb << 20}}] * 5)
    return document


class CountedRss(unittest.TestCase):
    """§17.283 (3): a counted compare's RSS gate reads the one-call peak, not the many-call one."""

    def test_a_many_call_crossing_at_doubled_calls_reads_ok(self):
        out = bench.compare(peaks(counted_doc(5e6, 1e6), 100, 200, 8), [peaks(counted_doc(5e6, 1e6), 100, 300, 16)])
        self.assertEqual((out["verdict"], out["confirmation_programs"]), ("ok", []))
        row = out["rss"][0]
        self.assertEqual((row["statistic"], row["status"], row["base_bytes"], row["head_bytes"]),
                         ("one-call maximum", "ok", 100 << 20, 100 << 20))
        self.assertEqual((row["many_call_base_bytes"], row["many_call_head_bytes"], row["base_calls"], row["head_calls"]),
                         (200 << 20, 300 << 20, 8, 16))

    def test_a_one_call_crossing_takes_the_confirmation_group(self):
        base = peaks(counted_doc(5e6, 1e6), 100, 200, 8)
        out = bench.compare(base, [peaks(counted_doc(5e6, 1e6), 130, 200, 8)])
        self.assertEqual((out["verdict"], out["confirmation_programs"]), ("flagged", ["p"]))
        self.assertEqual(out["rss"][0]["head_bytes"], 130 << 20)
        confirmed = bench.compare(base, [peaks(counted_doc(5e6, 1e6), 130, 200, 8),
                                         peaks(counted_doc(5e6, 1e6), 131, 150, 8)])
        self.assertEqual((confirmed["verdict"], confirmed["rss"][0]["confirmation_bytes"]), ("blocked", 131 << 20))

    def test_a_timed_compare_still_reads_the_process_peak(self):
        out = bench.compare(doc(pair(20.0, 10.0), rss=100), [doc(pair(20.0, 10.0), rss=200)])
        self.assertEqual((out["rss"][0]["statistic"], out["verdict"]), ("maximum", "flagged"))


class NodeFlags(unittest.TestCase):
    """§17.282: one young generation on every measured Node process; files under other flags are refused."""
    fixed = ["--predictable", "--min-semi-space-size=16", "--max-semi-space-size=16"]

    def test_every_measured_node_process_runs_under_the_flags(self):
        self.assertEqual(bench.NODE_FLAGS, self.fixed)
        timed = bench.command("sakura", "node", "m.mjs", "measure", 20)
        counted = bench.calls_command("sakura", "node", "m.wasm", 20, 2)
        for cmd in (timed, counted):
            self.assertEqual(cmd[:4], ["node", *self.fixed])
        self.assertEqual(bench.command("js", "bun", "m.mjs", "measure", 20)[1], str(bench.RUNNER))
        self.assertNotIn("--predictable", bench.calls_command("sakura", "wasmtime", "fib", 20, 2))

    def test_a_file_without_the_list_ran_under_predictable_alone(self):
        self.assertEqual(bench.node_flags(doc(pair(20.0, 10.0))), ["--predictable"])

    def test_compare_refuses_two_flag_lists_and_prints_both(self):
        head = doc(pair(20.0, 10.0))
        head["meta"]["flags"] = {"node": self.fixed}
        out = bench.compare(doc(pair(20.0, 10.0)), [head])
        self.assertEqual((out["verdict"], out["rows"]), ("flags", []))
        self.assertIn("base [--predictable]; head1 [" + " ".join(self.fixed) + "]", out["flags"])
        with tempfile.TemporaryDirectory() as tmp:
            paths = [Path(tmp) / "base.json", Path(tmp) / "head.json"]
            for path, document in zip(paths, (doc(pair(20.0, 10.0)), head)):
                path.write_text(json.dumps({**document, "schema": bench.SCHEMA}))
            stdout = io.StringIO()
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(io.StringIO()):
                code = bench.main(["compare", *map(str, paths)])
        self.assertEqual(code, bench.EXIT_BLOCKED)
        self.assertIn("verdict: flags (meta.flags.node differs", stdout.getvalue())
        same = doc(pair(20.0, 10.0))
        same["meta"]["flags"] = {"node": self.fixed}
        self.assertEqual(bench.compare(same, [head])["verdict"], "ok")

    def test_envelope_refuses_two_flag_lists(self):
        wasm, native = envelope_docs()
        wasm["meta"]["flags"]["node"] = self.fixed
        self.assertIn("meta.flags.node differs", bench.refusal(wasm, native))
        native["meta"]["flags"]["node"] = self.fixed
        self.assertIsNone(bench.refusal(wasm, native))

    def test_bless_refuses_to_overwrite_a_baseline_under_other_flags(self):
        run = counted_doc(5e6, 1e6)
        run["meta"]["flags"]["node"] = self.fixed
        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp) / "baselines" / "test.json"
            dest.parent.mkdir()
            old = counted_doc(5e6, 1e6)
            old["meta"]["commit"] = "320c99802fd6"
            dest.write_text(json.dumps(old))
            path = Path(tmp) / "run.json"
            path.write_text(json.dumps(run))
            stderr = io.StringIO()
            with patch.object(bench, "BENCH", Path(tmp)), patch.object(bench, "ROOT", Path(tmp)), \
                 contextlib.redirect_stderr(stderr):
                self.assertEqual(bench.main(["bless", str(path)]), bench.EXIT_ORACLE)
                self.assertIn("baselines/archive/320c9980/test.json first", stderr.getvalue())
                self.assertEqual(json.loads(dest.read_text())["meta"]["commit"], "320c99802fd6")
                dest.unlink()  # the owner archived it
                self.assertEqual(bench.main(["bless", str(path)]), bench.EXIT_OK)


class EnvelopeTable(unittest.TestCase):
    def test_every_envelope_program_is_a_program_of_the_suite(self):
        for patterns, _lanes in bench.M4_ENVELOPE:
            for pattern in patterns:
                self.assertTrue(bench.programs(pattern), pattern)


class Workflow(unittest.TestCase):
    def test_noise_runs_weekly_with_five_repeats_and_nightly_keeps_its_own_schedule(self):
        workflow = (bench.ROOT / ".github/workflows/bench.yml").read_text()
        self.assertIn('- cron: "43 3 * * 1"', workflow)
        self.assertIn("if: github.event.schedule == '43 3 * * 1' || (github.event_name == 'workflow_dispatch' "
                      "&& inputs.mode == 'noise')", workflow)
        self.assertIn("if: github.event.schedule == '17 2 * * *' ||", workflow)
        self.assertEqual(workflow.count("NOISE_REPEATS: ${{ inputs.repeats || 5 }}"), 2)
        self.assertIn("name: bench-noise-${{ matrix.os }}-${{ matrix.engine }}", workflow)

    def test_the_wasm_nightly_installs_the_pinned_wasmtime_and_runs_the_lane(self):
        workflow = (bench.ROOT / ".github/workflows/bench.yml").read_text()
        job = workflow[workflow.index("  nightly-wasm:"):workflow.index("  noise:")]
        self.assertIn("if: github.event.schedule == '17 2 * * *' ||", job)
        self.assertIn("WASMTIME_VERSION: ${{ steps.pins.outputs.wasmtime }}", job)
        self.assertIn("rustup target add wasm32-unknown-unknown", job)
        self.assertIn("bench run --suite wasm --lane matched --sizes all --stable", job)
        pins = (bench.BENCH / "toolchains.toml").read_text()
        self.assertIn('wasmtime = "', pins)


class TypeScriptSuite(unittest.TestCase):
    def test_the_lockfile_installs_the_pinned_toolchain(self):
        import tomllib
        pins = tomllib.loads((bench.BENCH / "toolchains.toml").read_text())
        lock = json.loads((bench.TS_TOOLS / "package-lock.json").read_text())["packages"]
        for name, pin in (("typescript", "typescript"), ("@types/node", "types_node"), ("esbuild", "esbuild")):
            self.assertEqual(lock[f"node_modules/{name}"]["version"], pins[pin], name)
            self.assertEqual(lock[""]["devDependencies"][name], pins[pin], name)

    def test_the_typescript_corpus_mirrors_the_sakura_one(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(bench, "TS_TREE", Path(tmp)):
            entry = bench.ts_corpus(1_000)
            mods = sorted(p.name for p in (entry.parent / "gen").iterdir())
            self.assertEqual(mods, ["m0.ts", "m1.ts"])  # 1,000 lines at 10 per function, 50 per module
            m0 = (entry.parent / "gen" / "m0.ts").read_text()
            self.assertEqual(m0.count("export function f"), 50)
            self.assertIn('if (!(x >= 0)) throw new RangeError("requires x >= 0");', m0)
            self.assertIn('import * as m1 from "./gen/m1.ts";', entry.read_text())

    def bless(self, flags):
        run = loaded(doc(pair(20.0, 10.0)), load=9.0)
        run["meta"]["flags"] = {**flags, "metric": "instructions"}
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "run.json"
            path.write_text(json.dumps(run))
            with patch.object(bench, "BENCH", Path(tmp)), patch.object(bench, "ROOT", Path(tmp)), \
                 contextlib.redirect_stderr(io.StringIO()):
                code = bench.main(["bless", str(path)])
            return code, [p.relative_to(tmp).as_posix() for p in Path(tmp).rglob("test.json")]

    def test_a_typescript_run_is_blessed_beside_the_js_baseline(self):
        self.assertEqual(self.bless({"suite": "typescript", "command": "typescript"}),
                         (bench.EXIT_OK, ["baselines/typescript/test.json"]))

    def test_a_compile_run_is_never_blessed_over_the_js_baseline(self):
        self.assertEqual(self.bless({"command": "compile"}), (bench.EXIT_ORACLE, []))


if __name__ == "__main__":
    unittest.main()
