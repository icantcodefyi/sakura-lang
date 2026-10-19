#!/usr/bin/env python3
"""`sakura check` latency per fresh process, and the per-phase split of `Analysis::analyze`.

    compile_latency.py time --runs 40 NAME=/path/to/sakura ...
        Interleaves the binaries (A, B, A, B, …) on the `sakura new cli` hello-world and the
        `bench compile` 10k-line corpus; prints median, p95 and range per binary and case.
    compile_latency.py instrument compiler/crates/sakura_analysis/src/lib.rs
        Adds per-phase timing of `analyze` and of reading std's analysis cache (`std_cache.rs`:
        read, sha256, decode, and the whole load as `std_load`), printed to stderr when
        SAKURA_PHASES is set, and a
        `latency_loop` test that analyses the hello-world under fresh URIs, so every analysis
        after the first reuses std, and a `check_loop` test that type-checks std's HIR
        SAKURA_CHECK_N times, for a sampling profiler. For a throwaway build only; never commit
        the result.
    compile_latency.py phases --runs 40 /path/to/instrumented/sakura
        Median milliseconds per phase over fresh processes, per case.
    SAKURA_PHASES=1 <test binary built with the instrumented crate> 2>&1 | compile_latency.py phases -
        The same over every analysis of one process, such as a test suite.

Phases are grouped by snapshot size and by how many files the reused analysis had (0: none).
"""
import argparse
import math
import os
import re
import statistics
import subprocess
import sys
import tempfile
import time
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "bench" / "harness"))


def cases(sakura):
    import bench

    hello = Path(tempfile.gettempdir()) / "sakura-compile-latency" / "hello"
    if not hello.exists():
        hello.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run([sakura, "new", "cli", "hello"], cwd=hello.parent, check=True, capture_output=True)
    return {"check_hello": hello, "check_cold_10k": bench.corpus(10_000)}


def run(sakura, project, env=None):
    t = time.perf_counter()
    p = subprocess.run([sakura, "check", str(project)], capture_output=True, text=True, env=env)
    ms = (time.perf_counter() - t) * 1000
    if p.returncode != 0:
        raise SystemExit(f"{sakura} check {project} failed:\n{p.stdout}{p.stderr}")
    return ms, p.stderr


def p95(xs):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, math.ceil(0.95 * len(xs)) - 1)]


def cmd_time(a):
    bins = dict(b.split("=", 1) for b in a.binaries)
    for case, project in cases(next(iter(bins.values()))).items():
        samples = defaultdict(list)
        for sakura in bins.values():
            run(sakura, project)  # warm the file cache
        for _ in range(a.runs):
            for name, sakura in bins.items():
                samples[name].append(run(sakura, project)[0])
        for name, xs in samples.items():
            print(f"{case:15} {name:10} median {statistics.median(xs):6.1f} ms  p95 {p95(xs):6.1f}  "
                  f"range {min(xs):.1f}–{max(xs):.1f}  n={len(xs)}")


def print_phases(title, lines):
    phases = defaultdict(list)
    for line in lines:
        # A test harness may print its own text on the same line.
        for files, reused, name, ms in re.findall(r"phase (\d+) (\d+) (\w+) ([\d.]+)", line):
            phases[(int(files), int(reused), name)].append(float(ms))
    print(f"{title}: median ms (snapshot files, files of the reused analysis, phase, analyses)")
    for (files, reused, name), xs in phases.items():
        print(f"  {files:4} {reused:4}  {name:20} {statistics.median(xs):7.2f}  n={len(xs)}")


def cmd_phases(a):
    if a.sakura == "-":
        return print_phases("stdin", sys.stdin)
    env = {**os.environ, "SAKURA_PHASES": "1"}
    for case, project in cases(a.sakura).items():
        lines = [line for _ in range(a.runs) for line in run(a.sakura, project, env)[1].splitlines()]
        print_phases(f"{case}, {a.runs} processes", lines)


START = r"""
    let t0 = std::time::Instant::now();
    let (mut last, mut laps) = (t0, Vec::new());
    let mut lap = |name: &'static str| {
        let now = std::time::Instant::now();
        laps.push((name, (now - last).as_secs_f64() * 1e3));
        last = now;
    };
"""

END = r"""
    laps.push(("total", t0.elapsed().as_secs_f64() * 1e3));
    if std::env::var_os("SAKURA_PHASES").is_some() {
        for (name, ms) in laps {
            eprintln!("phase {} {reused_files} {name} {ms:.3}", snapshot.files.len());
        }
    }
"""


LOOP = r"""
#[test]
fn latency_loop() {
    let main = std::env::var("SAKURA_LOOP_MAIN").expect("SAKURA_LOOP_MAIN: a hello-world main.sak");
    let text = std::fs::read_to_string(main).unwrap();
    let n: usize = std::env::var("SAKURA_LOOP_N").map_or(300, |n| n.parse().unwrap());
    for i in 0..n {
        let uri = format!("file:///t{i}/src/main.sak");
        let a = sakura_analysis::Analysis::analyze(&sakura_analysis::Snapshot::with_std([(uri, text.clone())]));
        assert!(!a.check.has_errors(), "{:?}", a.check.diagnostics);
    }
}

#[test]
fn check_loop() {
    let n: u32 = std::env::var("SAKURA_CHECK_N").map_or(0, |n| n.parse().unwrap());
    let std = sakura_analysis::Snapshot::new(sakura_analysis::std_sources());
    let a = sakura_analysis::Analysis::analyze(&std);
    let t = std::time::Instant::now();
    for _ in 0..n {
        let (_, diags) = sakura_types::check_program(std::hint::black_box(&a.program));
        assert!(diags.is_empty(), "{diags:?}");
    }
    if n > 0 {
        eprintln!("check of std: {:.3} ms", t.elapsed().as_secs_f64() * 1e3 / f64::from(n));
    }
}
"""


def cmd_instrument(a):
    p = Path(a.lib)
    (p.parent.parent / "tests" / "latency_loop.rs").write_text(LOOP.lstrip("\n"))
    s = p.read_text()

    def after(anchor, code):
        nonlocal s
        assert s.count(anchor) == 1, anchor
        s = s.replace(anchor, anchor + code)

    def before(anchor, code):
        nonlocal s
        assert s.count(anchor) == 1, anchor
        s = s.replace(anchor, code + anchor)

    lap = lambda name: f'    lap("{name}");\n'
    after("    ANALYSES.fetch_add(1, Ordering::Relaxed);\n", START.lstrip("\n"))
    before("    let mut prevs = prevs.into_iter();\n", lap("parse"))
    after("    let evaluated = evaluated_functions(&calls);\n", lap("evaluated_functions"))
    before("    let changed: Vec<bool>", lap("interface_hash"))
    before("    let changed: Vec<bool>", "    let reused_files = prev.map_or(0, |p| p.snapshot.files.len());\n")
    after("    LOWERS.fetch_add(lowered, Ordering::Relaxed);\n", lap("lower"))
    before("    let holes = typed\n", lap("check"))
    before("    let mut analyzed = Analyzed {\n", lap("symbols_index"))
    after("    analyzed.check.diagnostics = diagnostics;\n", lap("rest") + END.lstrip("\n"))
    before("        std_cache::load(&snapshot).unwrap_or_else(|| {\n",
           "        let t = std::time::Instant::now();\n"
           "        let loaded = std_cache::load(&snapshot);\n"
           "        if std::env::var_os(\"SAKURA_PHASES\").is_some() {\n"
           "            eprintln!(\"phase 0 0 std_load {:.3}\", t.elapsed().as_secs_f64() * 1e3);\n"
           "        }\n")
    s = s.replace("        std_cache::load(&snapshot).unwrap_or_else(|| {\n", "        loaded.unwrap_or_else(|| {\n", 1)
    p.write_text(s)
    c = p.parent / "std_cache.rs"
    s = c.read_text()
    show = ('    if std::env::var_os("SAKURA_PHASES").is_some() {{\n'
            '        eprintln!("phase 0 0 {} {{:.3}}", t.elapsed().as_secs_f64() * 1e3);\n'
            '    }}\n    let t = std::time::Instant::now();\n')
    after("fn load_from(path: &Path, snapshot: &Snapshot) -> Option<Analyzed> {\n",
          "    let t = std::time::Instant::now();\n")
    before("    let (magic, rest) = bytes.split_at_checked(MAGIC.len())?;\n", show.format("cache_read"))
    before("    decode(payload, snapshot)\n}", show.format("cache_sha256"))
    after("    decode(payload, snapshot)\n}", "")
    s = s.replace("    decode(payload, snapshot)\n}",
                  "    let a = decode(payload, snapshot);\n" + show.format("cache_decode") + "    a\n}", 1)
    c.write_text(s)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    t = sub.add_parser("time")
    t.add_argument("--runs", type=int, default=40)
    t.add_argument("binaries", nargs="+", metavar="NAME=PATH")
    p = sub.add_parser("phases")
    p.add_argument("--runs", type=int, default=40)
    p.add_argument("sakura", help="an instrumented `sakura`, or - for phase lines on stdin")
    i = sub.add_parser("instrument")
    i.add_argument("lib")
    a = ap.parse_args()
    {"time": cmd_time, "phases": cmd_phases, "instrument": cmd_instrument}[a.cmd](a)


if __name__ == "__main__":
    main()
