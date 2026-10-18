"""Harness tests (plan §4.1 WS-B): `python3 -m unittest discover bench/harness`."""

import json
import shutil
import tempfile
import unittest
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

    def test_rss_and_checksum_changes_fail_immediately(self):
        out = bench.compare(self.base, [doc(pair(20.0, 10.0), rss=1300)])
        self.assertTrue(any("RSS" in i for i in out["immediate"]))
        head = doc(pair(20.0, 10.0))
        head["results"][0]["checksum"] = "2"
        out = bench.compare(self.base, [head])
        self.assertTrue(any("checksum changed" in i for i in out["immediate"]))
        self.assertEqual(out["verdict"], "blocked")


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


if __name__ == "__main__":
    unittest.main()
