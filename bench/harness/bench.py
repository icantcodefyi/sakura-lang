#!/usr/bin/env python3
"""The `bench` harness (docs/plans/performance-r1.md §3, design §17.110). Python 3.11+ standard
library only; the JS panel is timed in-process by harness/runner.mjs. Run it as `bench/bench`."""

import argparse
import datetime
import fnmatch
import hashlib
import json
import math
import os
import platform
import random
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading
import time
import tomllib
from pathlib import Path

BENCH = Path(__file__).resolve().parent.parent
ROOT = BENCH.parent
PROGRAMS = BENCH / "programs"
TARGET = BENCH / "target"
RUNNER = BENCH / "harness" / "runner.mjs"
# cargo resolves a relative CARGO_TARGET_DIR against compiler/, where build_cli runs it.
SAKURA = ROOT / "compiler" / os.environ.get("CARGO_TARGET_DIR", "target") / "release" / "sakura"
SCHEMA = "sakura-bench-v1"
SMALL = [0, 1, 2, 3]
SEED = 20261018
PROCESS_TIMEOUT = 600

EXIT_OK, EXIT_ORACLE, EXIT_BLOCKED, EXIT_TOOLCHAIN = 0, 1, 2, 3


class Fail(Exception):
    """A build or oracle failure for one program; it is recorded and that program is not timed."""


class NoWasi(Fail):
    """The program needs what a WASI command does not have: it has no Wasmtime cell."""


def log(*a):
    print(*a, file=sys.stderr, flush=True)


def sh(cmd, cwd=None, timeout=PROCESS_TIMEOUT):
    try:
        r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        raise Fail(f"`{' '.join(map(str, cmd))}` timed out after {timeout} s") from None
    if r.returncode != 0:
        raise Fail(f"`{' '.join(map(str, cmd))}` exited {r.returncode}:\n{(r.stdout + r.stderr)[-3000:]}")
    return r.stdout


def run(cmd, env=None):
    """subprocess.run capturing text, with a timeout that is a Fail."""
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=PROCESS_TIMEOUT, env=env)
    except subprocess.TimeoutExpired:
        raise Fail(f"`{' '.join(map(str, cmd))}` timed out after {PROCESS_TIMEOUT} s") from None


def version(cmd):
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.TimeoutExpired):
        return None
    m = re.search(r"\d+\.\d+(\.\d+)?", r.stdout + r.stderr)
    return m.group(0) if m else None


# ---------------------------------------------------------------------------------------------
# Programs and variants

def programs(pattern="*"):
    return [p for p in sorted(d.name for d in PROGRAMS.iterdir() if (d / "meta.toml").exists())
            if fnmatch.fnmatch(p, pattern)]


def meta(p):
    return tomllib.loads((PROGRAMS / p / "meta.toml").read_text())


def oracle_inputs(m):
    return SMALL + list(m["sizes"]) + list(m.get("check_sizes", []))


def build_cli():
    sh(["cargo", "build", "--release", "--locked", "-p", "sakura_cli"], cwd=ROOT / "compiler", timeout=1800)


def build_sakura(p):
    """Compile programs/<p>/sakura/main.sak; return an entry module for runner.mjs."""
    out = TARGET / "sakura" / p
    sh([str(SAKURA), "build", "--out-dir", str(out), str(PROGRAMS / p / "sakura" / "main.sak")])
    # ponytail: rewrites the emitted __main__.js (std imports + registerOps, then runMain) into
    # an importable entry; replace with a `sakura build` library mode if the CLI grows one.
    lines = (out / "__main__.js").read_text().splitlines()
    kept = [l for l in lines if not l.startswith("runMain(") and "$entry" not in l]
    kept = [l.replace("import { registerOps, runMain }", "import { registerOps }") for l in kept]
    kept += ['export * as program from "./main/main.js";', 'export { runTask } from "./sakura_runtime.js";']
    entry = out / "bench_entry.mjs"
    entry.write_text("\n".join(kept) + "\n")
    return entry


def build_wasm(p):
    """programs/<p> with `--target wasm`; runner.mjs loads the module into the Node host beside it."""
    out = TARGET / "wasm" / p
    sh([str(SAKURA), "build", "--target", "wasm", "--out-dir", str(out), str(PROGRAMS / p / "sakura" / "main.sak")])
    return out / "main.wasm"


WASI_BUILT = {}
FOREVER = 10 ** 15


def wasmtime(p, entry, inputs, forever=False):
    """`wasmtime run` of programs/<p> built as a WASI command whose `main` prints `entry(n)` for
    each input, or for the one input until killed. The WASI embedding has no `Cli.args`, so each
    input list is its own build: the program with `main` (its last item) replaced, built once
    per run of the harness."""
    key = (p, entry, tuple(inputs), forever)
    if key not in WASI_BUILT:
        source = (PROGRAMS / p / "sakura" / "main.sak").read_text()
        # ponytail: `main` is every program's last item, so an item after it would be dropped;
        # parse the items if a program ever puts one there.
        at = source.index("\npub fn main(") + 1
        head, signature = source[:at], source[at:source.index("\n", at)]  # keeps main's effect row
        if forever:
            body = (f'  let n = Option.expect(String.parse_int("{inputs[0]}"), "size")\n'
                    f'  for _ in 0..{FOREVER} {{ Console.print("{{{entry}(n)}}") }}')
        else:
            args = ", ".join(f'"{n}"' for n in inputs)
            body = (f'  for arg in [{args}] '
                    f'{{ Console.print("{{{entry}(Option.expect(String.parse_int(arg), "size"))}}") }}')
        d = TARGET / "wasi" / p / "-".join([entry, *map(str, inputs), *(["loop"] if forever else [])])
        d.mkdir(parents=True, exist_ok=True)
        (d / "main.sak").write_text(f"{head}{signature}\n{body}\n}}\n")
        sh([str(SAKURA), "build", "--target", "wasm", "--wasi", "--out-dir", str(d / "out"), str(d / "main.sak")])
        WASI_BUILT[key] = d / "out" / "main.wasm"
    return ["wasmtime", "run", str(WASI_BUILT[key])]


# ponytail: reads wasmtime's backtrace text and the backend's D090 message; a structured outcome
# from the WASI command would replace both.
def no_wasi(error):
    """Why a WASI command could not run the program, from its failure: a host operation the
    embedding traps (`unreachable` in `wasm::host::<name>`: clocks, files, network), or a
    builtin only the Node host implements, in JS (`D090`: `Json`). None for any other failure."""
    m = re.search(r"\b0:\s+0x[0-9a-f]+ - .*::wasm::host::(\w+)", error)
    if m and "wasm `unreachable`" in error:
        return f"the host operation `{m[1]}` is not pure; the WASI embedding traps it"
    m = re.search(r"defect D090: builtin (\w+) is not supported by the WASM backend", error)
    return f"the builtin `{m[1]}` exists only in the Node host's JS" if m else None


def build_rust(p, lane):
    profile = "checked" if lane == "matched" else "release"
    d = TARGET / "rust" / p
    sh(["cargo", "build", "--locked", "--quiet", "--profile", profile, "--target-dir", str(d),
        "--manifest-path", str(PROGRAMS / p / "rust" / "Cargo.toml")], timeout=1800)
    return d / profile / "main"


def build_go(p, _lane):
    out = TARGET / "go" / p / "main"
    sh(["go", "build", "-o", str(out), "."], cwd=PROGRAMS / p / "go")
    return out


def build_ocaml(p, _lane):
    d = TARGET / "ocaml" / p
    d.mkdir(parents=True, exist_ok=True)
    # ocamlopt writes intermediates next to its sources; keep shared modules in scratch too.
    for name in ("runner.ml", "kernels.ml"):
        shutil.copy(PROGRAMS / "_shared" / "ocaml" / name, d / name)
    shutil.copy(PROGRAMS / p / "ocaml" / "main.ml", d / "main.ml")
    flags = ["-O3"] if ocaml_flambda() else []
    sh(["ocamlopt", *flags, "-I", "+unix", "unix.cmxa", "runner.ml", "kernels.ml",
        "main.ml", "-o", "main"], cwd=d)
    return d / "main"


def ocaml_flambda():
    r = subprocess.run(["ocamlopt", "-config-var", "flambda"], capture_output=True, text=True)
    return r.stdout.strip() == "true"


def build_koka(p, _lane, reuse=True):
    d = TARGET / ("koka" if reuse else "koka-noreuse") / p
    d.mkdir(parents=True, exist_ok=True)
    # Compile from the entry directory: Koka treats underscores in module paths as '/'.
    sh(["koka", "-O2", "--target=c", *([] if reuse else ["--fno-reuse"]), "--stack=67108864", f"--builddir={d / 'build'}",
        f"--include={PROGRAMS / '_shared' / 'koka'}", "-o", str(d / "main"), "main.kk"],
       cwd=PROGRAMS / p / "koka")
    return d / "main"


# Native variants (plan §2.2): source marker, toolchain, builder. The `matched` lane builds Rust
# with overflow checks, `idiomatic` without; Koka is built with and without reuse (§17.109's two
# Koka lanes); the other languages have one build.
NATIVE = {
    "rust": ("rust/Cargo.toml", "cargo", build_rust, ["rustc", "--version"]),
    "go": ("go/main.go", "go", build_go, ["go", "version"]),
    "ocaml": ("ocaml/main.ml", "ocamlopt", build_ocaml, ["ocamlopt", "-version"]),
    "koka": ("koka/main.kk", "koka", build_koka, ["koka", "--version"]),
    "koka-noreuse": ("koka/main.kk", "koka", lambda p, lane: build_koka(p, lane, reuse=False), ["koka", "--version"]),
}


JS_ENGINES, WASM_ENGINES = ("node", "bun"), ("node", "wasmtime")


def variants(suite, engines, p):
    """(variant, backend, engine) cells for program p in this suite. The WASM lane's control is
    hand JS on Node for both engines: Wasmtime runs no JS."""
    out = []
    if suite in ("js", "all"):
        out += [(v, "js", e) for e in engines if e in JS_ENGINES for v in ("sakura", "js")]
    if suite == "wasm":
        out += [("sakura", "wasm", e) for e in engines if e in WASM_ENGINES] + [("js", "js", "node")]
    if suite in ("native", "all"):
        out += [(v, "native", "native") for v, (src, *_rest) in NATIVE.items() if (PROGRAMS / p / src).exists()]
    return out


def cell(r):
    """A result row's identity. Rows recorded before the WASM lane have no `backend`."""
    backend = r.get("backend") or ("native" if r["engine"] == "native" else "js")
    return r["program"], r["variant"], backend, r["engine"], r["size"]


# ---------------------------------------------------------------------------------------------
# Running one fresh process

def spawn(cmd, timeout=PROCESS_TIMEOUT):
    """Run cmd; return (stdout, peak RSS in bytes). Raise Fail on a non-zero exit."""
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        proc = subprocess.Popen(cmd, stdout=out, stderr=err)
        deadline = time.monotonic() + timeout
        while True:
            pid, status, ru = os.wait4(proc.pid, os.WNOHANG)
            if pid:
                break
            if time.monotonic() > deadline:
                proc.kill()
                os.wait4(proc.pid, 0)
                raise Fail(f"`{' '.join(map(str, cmd))}` timed out after {timeout} s")
            time.sleep(0.002)
        out.seek(0)
        err.seek(0)
        code = proc.returncode = os.waitstatus_to_exitcode(status)  # reaped by wait4, not Popen
        if code != 0:
            raise Fail(f"`{' '.join(map(str, cmd))}` exited {code}: {err.read().decode(errors='replace')[-2000:]}")
        rss = ru.ru_maxrss * (1 if sys.platform == "darwin" else 1024)  # bytes on macOS, KiB on Linux
        return out.read().decode(), rss


def command(variant, engine, module_or_bin, mode, *args):
    if engine == "native":
        return [str(module_or_bin), mode, *map(str, args)]
    return [engine, str(RUNNER), mode, str(module_or_bin), *map(str, args)]


def json_lines(text):
    return [json.loads(l) for l in text.splitlines() if l.strip()]


def values(variant, engine, artefact, inputs, entry="run"):
    """{"<n>": checksum} of entry(n) for each input, from one process."""
    if engine == "wasmtime":
        r = run(wasmtime(artefact, entry, inputs))
        if r.returncode != 0:
            if reason := no_wasi(r.stderr):  # the whole stderr: frame 0 heads a long backtrace
                raise NoWasi(f"{artefact}: {reason}")
            raise Fail(f"`{' '.join(r.args)}` exited {r.returncode}: {r.stderr[-2000:]}")
        return dict(zip(map(str, inputs), r.stdout.splitlines()))
    fn = [] if entry == "run" else ["--fn", entry]
    return json_lines(spawn(command(variant, engine, artefact, "values", *fn, *inputs))[0])[-1]["values"]


def defect(variant, engine, artefact, k):
    """The defect code negative(k) ends with, or None."""
    if engine == "wasmtime":  # `_start` exits 70 after `defect Dnnn: message` on stderr
        r = run(wasmtime(artefact, "negative", [k]))
        m = re.match(r"defect (D\d{3}):", r.stderr)
        return m[1] if r.returncode == 70 and m else None
    return json_lines(spawn(command(variant, engine, artefact, "negative", k))[0])[-1]["defects"][str(k)]


def oracle(p, m, variant, engine, artefact):
    """Raise Fail unless the artefact reproduces every checksum (and defect) in meta.toml."""
    inputs = oracle_inputs(m)

    def check(entry, want):
        got = values(variant, engine, artefact, inputs, entry)
        return [f"{entry}({n}) = {got.get(str(n))}, expected {want.get(str(n))}"
                for n in inputs if str(got.get(str(n))) != str(want.get(str(n)))]

    bad = check("run", m.get("checksums", {}))
    if variant == "sakura":
        for case in m.get("negative", []):
            code = defect(variant, engine, artefact, case["arg"])
            if code != case["defect"]:
                bad.append(f"negative({case['arg']}) gave {code}, expected {case['defect']}")
    for case in cases(m, engine):
        bad += check(f"run_{case}", m["checksums"].get(case, {}))
    if bad:
        raise Fail(f"{p} {variant}/{engine}: oracle failed: " + "; ".join(bad))


def cases(m, engine):
    """meta.toml `cases`: entries `run_<case>` timed as their own cells (`map_set:int`, §17.152
    item 3). The JS panel only; native ports implement `run`."""
    return [] if engine == "native" else list(m.get("cases", []))


def label(p, case):
    return p if case is None else f"{p}:{case}"


def measure(variant, engine, artefact, n, stable, case=None):
    """One fresh process: nine samples (ms per call), the checksum, peak RSS."""
    if engine == "wasmtime":
        return measure_lines(wasmtime(artefact, f"run_{case}" if case else "run", [n], forever=True), stable)
    out, rss = spawn(command(variant, engine, artefact, "measure", n, *(["--stable"] if stable else []),
                             *(["--fn", f"run_{case}"] if case else [])))
    lines = json_lines(out)
    if engine == "native":
        samples = [l["ms"] for l in lines]
        checksum = lines[-1]["checksum"] if lines else None
    else:
        samples, checksum = lines[-1]["samples_ms"], lines[-1]["checksum"]
    return samples, str(checksum), rss


def measure_lines(cmd, stable, timeout=PROCESS_TIMEOUT):
    """runner.mjs's measure protocol over a process that prints one checksum per call until it is
    killed: warm up for 250 ms (2 s with --stable) from its first line, calibrate a power-of-two
    batch to ≥ 30 ms, then nine samples (ms per call) from when the lines arrive. Process start,
    compilation and instantiation come before the first line, outside every sample."""
    with tempfile.TemporaryFile() as err:
        proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=err)
        watchdog = threading.Timer(timeout, proc.kill)
        watchdog.start()
        try:
            def lines(count):
                for _ in range(count):
                    line = proc.stdout.readline()
                    if not line:
                        err.seek(0)
                        raise Fail(f"`{' '.join(cmd)}` stopped printing: {err.read().decode(errors='replace')[-2000:]}")
                return line

            last = lines(1)
            start = time.perf_counter()
            while time.perf_counter() - start < (2 if stable else 0.25):
                last = lines(1)
            count = 1
            while True:
                t = time.perf_counter()
                last = lines(count)
                if time.perf_counter() - t >= 0.03 or count >= 1 << 20:
                    break
                count *= 2
            samples = []
            for _ in range(9):
                t = time.perf_counter()
                last = lines(count)
                samples.append((time.perf_counter() - t) * 1000 / count)
        finally:
            watchdog.cancel()
            proc.kill()
            _pid, _status, ru = os.wait4(proc.pid, 0)
            proc.returncode = -9  # reaped by wait4, not Popen
            proc.stdout.close()
    return samples, last.decode().strip(), ru.ru_maxrss * (1 if sys.platform == "darwin" else 1024)


def wasm_stats(module, args):
    """§17.118: the module's `stats()` counters after one `main` with these arguments, appended by
    the Node host's own CLI under `SAKURA_WASM_STATS`. They are deterministic: one process."""
    with tempfile.TemporaryDirectory() as d:
        path = Path(d) / "stats.jsonl"
        cmd = ["node", str(module.parent / "sakura_wasm_host.js"), str(module), *map(str, args)]
        r = run(cmd, env={**os.environ, "SAKURA_WASM_STATS": str(path)})
        if r.returncode != 0 or not path.exists():
            raise Fail(f"`{' '.join(cmd)}` exited {r.returncode}: {r.stderr[-2000:]}")
        return json_lines(path.read_text())[-1]


# ---------------------------------------------------------------------------------------------
# Host and toolchains

def host():
    system, machine = platform.system(), platform.machine()
    if sys.platform == "darwin":
        cpu = subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"], capture_output=True, text=True).stdout.strip()
        mem = int(subprocess.run(["sysctl", "-n", "hw.memsize"], capture_output=True, text=True).stdout.strip() or 0)
    else:
        info = Path("/proc/cpuinfo").read_text() if Path("/proc/cpuinfo").exists() else ""
        cpu = (re.search(r"model name\s*:\s*(.+)", info) or re.search(r"(.*)", machine)).group(1).strip()
        meminfo = Path("/proc/meminfo").read_text() if Path("/proc/meminfo").exists() else ""
        mem = int((re.search(r"MemTotal:\s*(\d+)", meminfo) or re.search("(0)", "0")).group(1)) * 1024
    os_name = f"{system} {machine}"
    hid = hashlib.sha256(f"{cpu}|{os_name}|{mem}".encode()).hexdigest()[:12]
    return {"id": hid, "cpu": cpu, "os": f"{system} {platform.release()} {machine}", "mem_bytes": mem}


# The quiet window (§17.153): every snapshot's one-minute load is at most a quarter of the logical
# CPUs and no other build, benchmark or agent executable is among the busiest; CI is never quiet.
LOAD_INTERVAL_S = 15
BUSY_CPU_PERCENT = 10.0
# ponytail: a name list; a busy process under another name is missed, the owner's pause covers it.
BUSY_EXECUTABLE = re.compile(r"(cargo|rustc|rust-analyzer|cc1.*|clang.*|ld|ld64.*|go|ocaml.*|koka|node|bun|"
                             r"deno|python[0-9.]*|claude|codex|hyperfine|sakura|wasmtime|tsc|esbuild)")


def load_snapshot():
    """One-minute load and the twelve busiest executables outside this harness's process tree."""
    ps = subprocess.run(["ps", "-axo", "pid=,ppid=,pcpu=,comm="], capture_output=True, text=True).stdout
    procs = [(int(f[0]), int(f[1]), float(f[2]), Path(f[3]).name)
             for f in (line.split(None, 3) for line in ps.splitlines()) if len(f) == 4]
    ours = {os.getpid()}
    while grown := {pid for pid, ppid, _c, _n in procs if ppid in ours} - ours:
        ours |= grown
    top = sorted((p for p in procs if p[0] not in ours), key=lambda p: -p[2])[:12]
    return {"utc": datetime.datetime.now(datetime.UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "load_1m": os.getloadavg()[0],
            "busiest": [{"executable": n, "cpu_percent": c} for _p, _pp, c, n in top]}


def quiet_verdict(snapshots, cpus, ci):
    """`meta.host.load`: the quiet verdict over a run's snapshots and the reasons it is not quiet."""
    threshold = cpus / 4
    peak = max(s["load_1m"] for s in snapshots)
    busy = sorted({b["executable"] for s in snapshots for b in s["busiest"]
                   if b["cpu_percent"] >= BUSY_CPU_PERCENT and BUSY_EXECUTABLE.fullmatch(b["executable"].lower())})
    reasons = ((["CI runner (trend evidence, never a bless)"] if ci else [])
               + ([f"one-minute load {peak:.2f} > {threshold:g}"] if peak > threshold else [])
               + ([f"busy: {', '.join(busy)}"] if busy else []))
    return {"quiet": not reasons, "ci": ci, "threshold": threshold, "logical_cpus": cpus,
            "interval_s": LOAD_INTERVAL_S, "max_load_1m": peak, "reasons": reasons, "snapshots": snapshots}


def sample_load(stop, snapshots):
    while not stop.wait(LOAD_INTERVAL_S):
        snapshots.append(load_snapshot())


def host_load(doc):
    """The maximum one-minute load of a loaded non-CI run, else None (quiet, CI or legacy)."""
    h = doc["meta"].get("host", {})
    load = h.get("load") or {}
    return load.get("max_load_1m") if h.get("quiet") is False and not load.get("ci") else None


ENGINE_VERSIONS = {"node": ["node", "--version"], "bun": ["bun", "--version"], "rustc": ["rustc", "--version"],
                   "go": ["go", "version"], "ocaml": ["ocamlopt", "-version"], "koka": ["koka", "--version"],
                   "hyperfine": ["hyperfine", "--version"], "wasmtime": ["wasmtime", "--version"]}


def engines_meta():
    return {k: version(cmd) for k, cmd in ENGINE_VERSIONS.items() if shutil.which(cmd[0])}


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True).stdout.strip()


def new_result(flags):
    return {"schema": SCHEMA,
            "meta": {"commit": git("rev-parse", "HEAD"), "dirty": bool(git("status", "--porcelain", "--", "compiler", "bench")),
                     "host": host(), "engines": engines_meta(), "flags": flags, "seed": flags["seed"],
                     "started": datetime.datetime.now(datetime.UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
                     "missing": [], "failures": [], "absent": []},
            "results": []}


# ---------------------------------------------------------------------------------------------
# bench run

def cmd_run(a):
    engines = [a.engine] if a.engine else list(WASM_ENGINES if a.suite == "wasm" else JS_ENGINES)
    sizes_flag = a.sizes
    flags = {"suite": a.suite, "engines": engines, "lane": a.lane, "sizes": sizes_flag, "stable": a.stable,
             "filter": a.filter, "processes": a.processes, "seed": a.seed}
    res = new_result(flags)
    missing = res["meta"]["missing"]
    if a.suite in ("js", "all", "wasm"):
        missing += [e for e in engines if not shutil.which(e)]
    if a.suite in ("native", "all"):
        missing += [tool for tool in dict.fromkeys(t for _s, t, *_r in NATIVE.values()) if not shutil.which(tool)]
    for tool in missing:
        log(f"[missing] {tool}: its variants are skipped")
    status = EXIT_TOOLCHAIN if missing else EXIT_OK

    build_cli()
    jobs = []  # (program, case, variant, backend, engine, artefact, size)
    stats = {}  # (program, case, size) → the WASM module's counters, from the Node host (§17.118)
    for p in programs(a.filter):
        m = meta(p)
        sizes = m["sizes"][:2] if sizes_flag == "quick" else m["sizes"]
        built, verified, failed = {}, [], False
        for variant, backend, engine in variants(a.suite, engines, p):
            tool = engine if engine != "native" else NATIVE[variant][1]
            if tool in missing:
                continue
            try:
                if engine == "wasmtime":  # wasmtime() builds a WASI command per input list
                    art = p
                else:
                    if (variant, backend) not in built:  # one build per variant, shared by the engines
                        built[(variant, backend)] = (build_sakura(p) if (variant, backend) == ("sakura", "js") else
                                                     build_wasm(p) if backend == "wasm" else
                                                     PROGRAMS / p / "js" / "main.mjs" if variant == "js" else
                                                     NATIVE[variant][2](p, a.lane))
                    art = built[(variant, backend)]
                log(f"[oracle] {p} {variant}/{backend}/{engine}")
                oracle(p, m, variant, engine, art)
                if engine == "wasmtime":  # build every timed driver now, not between timed processes
                    for c in [None, *cases(m, engine)]:
                        for n in sizes:
                            wasmtime(p, f"run_{c}" if c else "run", [n], forever=True)
                if backend == "wasm" and (p, None, sizes[0]) not in stats:
                    if ("sakura", "wasm") not in built:
                        built[("sakura", "wasm")] = build_wasm(p)
                    for c in [None, *cases(m, engine)]:
                        for n in sizes:
                            stats[(p, c, n)] = wasm_stats(built[("sakura", "wasm")], [n] if c is None else [c, n])
                verified.append((variant, backend, engine, art))
            except NoWasi as e:  # §17.191 (2): the Wasmtime cell of an impure program is absent
                log(f"[absent] {e}")
                res["meta"]["absent"].append({"program": p, "variant": variant, "backend": backend,
                                              "engine": engine, "reason": str(e)})
            except Fail as e:
                log(f"[FAIL] {e}")
                res["meta"]["failures"].append({"program": p, "variant": variant, "backend": backend,
                                                "engine": engine, "error": str(e)})
                failed = True
        if failed:  # plan §3.2: nothing is timed for a program whose oracle failed
            status = EXIT_ORACLE
            continue
        jobs += [(p, c, v, b, e, art, n) for v, b, e, art in verified for c in [None, *cases(m, e)] for n in sizes]

    order = [(j, i) for j in jobs for i in range(a.processes)]
    random.Random(a.seed).shuffle(order)
    collected = {}
    snapshots, stop = [load_snapshot()], threading.Event()  # timing only: the build's load is ours
    sampler = threading.Thread(target=sample_load, args=(stop, snapshots), daemon=True)
    sampler.start()
    for k, (job, _i) in enumerate(order, 1):
        p, case, variant, backend, engine, art, n = job
        log(f"[{k}/{len(order)}] {label(p, case)} {variant}/{backend}/{engine} n={n}")
        try:
            samples, checksum, rss = measure(variant, engine, art, n, a.stable, case)
        except Fail as e:
            samples, checksum, rss = [], f"error: {e}", 0
        collected.setdefault(job, []).append((samples, checksum, rss))
    stop.set()
    sampler.join()
    snapshots.append(load_snapshot())
    load = quiet_verdict(snapshots, os.cpu_count() or 1, os.environ.get("GITHUB_ACTIONS") == "true")
    res["meta"].setdefault("host", {}).update(quiet=load["quiet"], load=load)
    log(f"[load] {'quiet' if load['quiet'] else 'not quiet: ' + '; '.join(load['reasons'])}")

    for (p, case, variant, backend, engine, _art, n), runs in collected.items():
        m = meta(p)
        expected = str((m["checksums"][case] if case else m["checksums"])[str(n)])
        medians = [statistics.median(s) for s, _c, _r in runs if s]
        res["results"].append({
            "program": label(p, case), "variant": variant, "backend": backend, "engine": engine,
            "lane": a.lane, "ints": m["ints"], "size": n,
            "samples_ms": [x for s, _c, _r in runs for x in s],
            "median_ms": statistics.median(medians) if medians else None,
            "process_medians_ms": medians,
            "rss_bytes": max(r for _s, _c, r in runs),
            "process_rss_bytes": [r for _s, _c, r in runs],
            "checksum_ok": all(c == expected for _s, c, _r in runs),
            "checksum": next((c for _s, c, _r in runs if c != expected), expected),
            **({"stats": stats.get((p, case, n))} if backend == "wasm" else {})})
        if not res["results"][-1]["checksum_ok"]:
            status = EXIT_ORACLE

    Path(a.json).parent.mkdir(parents=True, exist_ok=True)
    Path(a.json).write_text(json.dumps(res, indent=1) + "\n")
    print(summary(res))
    log(f"wrote {a.json}")
    return status


def control(backend, engine):
    """The hand-JS engine a Sakura cell is divided by: its own, or Node for the WASM lane."""
    return engine if backend == "js" else "node"


def ratios(res):
    """{(program, backend, engine, size): (sakura_ms, js_ms)} for every pair present."""
    by = {cell(r): r for r in res["results"]}
    out = {}
    for (p, v, b, e, n), r in by.items():
        js = by.get((p, "js", "js", control(b, e), n))
        if v == "sakura" and js and r["median_ms"] and js["median_ms"]:
            out[(p, b, e, n)] = (r["median_ms"], js["median_ms"])
    return out


def summary(res):
    """Markdown table of medians and Sakura/hand-JS ratios (and native medians if present)."""
    rows = {}
    for r in res["results"]:
        rows.setdefault((r["program"], r["size"]), {})[cell(r)[1:4]] = r
    cols = sorted({k for v in rows.values() for k in v},
                  key=lambda k: (k[2] == "native", k[1] != "js", k[2], k[0] != "sakura", k[0]))
    pairs = sorted({(b, e) for v, b, e in cols if v == "sakura"}, key=lambda k: (k[0] != "js", k))
    name = lambda b, e: e if b == "js" else f"{b}/{e}"
    head = (["Program", "n"] + [f"{v}/{name(b, e)} ms" for v, b, e in cols]
            + [f"ratio {name(b, e)}" for b, e in pairs])
    lines = ["| " + " | ".join(head) + " |", "|---|---:|" + "---:|" * (len(head) - 2)]
    for (p, n), cells in sorted(rows.items()):
        def ms(c):
            r = cells.get(c)
            if not r:
                return "—"
            if not r["checksum_ok"]:
                return "✗ checksum"
            return f"{r['median_ms']:.4g}"
        rat = []
        for b, e in pairs:
            s, j = cells.get(("sakura", b, e)), cells.get(("js", "js", control(b, e)))
            rat.append(f"{s['median_ms'] / j['median_ms']:.2f}×" if s and j and s["median_ms"] and j["median_ms"] else "—")
        lines.append("| " + " | ".join([p, str(n)] + [ms(c) for c in cols] + rat) + " |")
    return "\n".join(lines)


# ---------------------------------------------------------------------------------------------
# bench compare

def slope(points):
    """Least-squares slope of log(ms) over log(n): 1 is linear, 2 quadratic."""
    pts = [(math.log(n), math.log(ms)) for n, ms in points if n > 0 and ms and ms > 0]
    if len(pts) < 2:
        return None
    mx, my = statistics.fmean(x for x, _ in pts), statistics.fmean(y for _, y in pts)
    den = sum((x - mx) ** 2 for x, _ in pts)
    return sum((x - mx) * (y - my) for x, y in pts) / den if den else None


def rss_samples(document, row):
    """Validate new-format process peaks; a legacy row has only its maximum."""
    if "process_rss_bytes" not in row:
        return None
    peaks = row["process_rss_bytes"]
    count = document["meta"].get("flags", {}).get("processes")
    if (type(count) is not int or count < 1
            or not isinstance(peaks, list) or not peaks or len(peaks) != count
            or any(type(x) not in (int, float) or not math.isfinite(x) or x <= 0 for x in peaks)
            or max(peaks) != row.get("rss_bytes")):
        raise ValueError("invalid/incomplete process RSS samples or inconsistent maximum")
    return peaks


def rss_comparison(base, b, head, r, jump):
    """Keep the maximum gate; process medians explain variation without changing policy."""
    bp, hp = rss_samples(base, b), rss_samples(head, r)
    if not b.get("rss_bytes") or not r.get("rss_bytes"):
        return None
    before, after = b["rss_bytes"], r["rss_bytes"]
    return {"statistic": "maximum", "base_bytes": before, "head_bytes": after,
            "base_median_bytes": statistics.median(bp) if bp is not None else None,
            "head_median_bytes": statistics.median(hp) if hp is not None else None,
            "base_processes": len(bp) if bp is not None else None,
            "head_processes": len(hp) if hp is not None else None,
            "status": "blocked" if after > before * (1 + jump) else "ok"}


def compare(base, heads, flag=0.05, block=0.10, abs_ms=0.1, slope_jump=0.5, rss_jump=0.25):
    """Timing policy per §17.110 and maximum-RSS policy per §17.136.
    Node RSS needs the first two heads to cross; Bun RSS is advisory. When the base or a head
    was measured on a loaded host (§17.153) the verdict is `advisory`: only a correctness
    failure (checksum, oracle, malformed samples) still blocks."""
    b_rat = ratios(base)
    h_rats = [ratios(h) for h in heads]
    head = heads[0]
    rows, verdict = [], "ok"

    def worse(v):
        nonlocal verdict
        order = ["ok", "flagged", "blocked"]
        verdict = max(verdict, v, key=order.index)

    for key in sorted(set(b_rat) & set(h_rats[0])):
        p, backend, e, n = key
        bs, bj = b_rat[key]
        base_ratio = bs / bj
        deltas, absolute = [], []
        for hr in h_rats:
            if key in hr:
                hs, hj = hr[key]
                deltas.append((hs / hj) / base_ratio - 1)
                absolute.append(hs - base_ratio * hj)  # ms per workload beyond the baseline ratio
        d = deltas[0]
        status = "ok"
        if d > flag:
            status = "flagged"
        if len(deltas) == len(heads) and all(x > block for x in deltas) and all(x >= abs_ms for x in absolute):
            status = "blocked"
        if d < -block:
            status = "improved >10%: explain in the PR"
        if status in ("flagged", "blocked"):
            worse(status)
        rows.append({"program": p, "backend": backend, "engine": e, "size": n, "base_ratio": base_ratio,
                     "head_ratio": h_rats[0][key][0] / h_rats[0][key][1], "delta": d, "status": status})

    immediate, correctness, rss_rows = [], [], []  # measured (advisory when loaded), and correctness
    confirmation_programs = {r["program"] for r in rows if r["status"] == "blocked"} if len(heads) == 1 else set()
    confirmation = heads[1] if len(heads) > 1 else None
    confirmation_by = {cell(r): r for r in confirmation["results"]} if confirmation else {}
    b_by = {cell(r): r for r in base["results"]}
    for r in head["results"]:
        key = cell(r)
        b = b_by.get(key)
        if b:
            try:
                memory = rss_comparison(base, b, head, r, rss_jump)
                cr = confirmation_by.get(key)
                confirmed = rss_comparison(base, b, confirmation, cr, rss_jump) if cr else None
            except ValueError as error:
                correctness.append(f"{key}: {error}")
                memory = None
            if memory:
                crossed = memory["status"] == "blocked"
                confirmation_crossed = confirmed is not None and confirmed["status"] == "blocked"
                memory.update({"confirmation_bytes": confirmed["head_bytes"] if confirmed else None,
                               "confirmation_median_bytes": confirmed["head_median_bytes"] if confirmed else None,
                               "confirmation_processes": confirmed["head_processes"] if confirmed else None,
                               "note": ""})
                if r["engine"] == "bun" and (crossed or confirmation_crossed):
                    memory.update(status="flagged", note="Bun peak RSS is advisory (§17.136)")
                elif r["engine"] == "node" and (crossed or confirmation_crossed):
                    if crossed and confirmation_crossed:
                        memory["note"] = "Node peak RSS crossing confirmed by the second group (§17.136)"
                    else:
                        memory["status"] = "flagged"
                        memory["note"] = ("Node peak RSS crossing needs one confirmation group (§17.136)" if confirmation is None
                                          else "Node peak RSS crossing not confirmed in the same cell (§17.136)")
                        if confirmation is None:
                            confirmation_programs.add(r["program"])
                if memory["status"] in ("flagged", "blocked"):
                    worse(memory["status"])
                rss_rows.append(dict(zip(("program", "variant", "backend", "engine", "size"), key)) | memory)
                if memory["status"] == "blocked" and r["engine"] != "node":
                    before, after = memory["base_bytes"], memory["head_bytes"]
                    immediate.append(f"{key}: peak RSS ({memory['statistic']}) {before} → {after} (+{after / before - 1:.0%})")
    for group in heads:
        for r in group["results"]:
            key = cell(r)
            if not r["checksum_ok"]:
                correctness.append(f"{key}: checksum {r['checksum']}")
            b = b_by.get(key)
            if b and b["checksum_ok"] and r["checksum_ok"] and b["checksum"] != r["checksum"]:
                correctness.append(f"{key}: checksum changed {b['checksum']} → {r['checksum']}")
        for f in group["meta"].get("failures", []):
            correctness.append(f"{f['program']} {f['variant']}/{f.get('backend', 'js')}/{f['engine']}: "
                               f"{f['error'].splitlines()[0]}")
        for f in group["meta"].get("absent", []):  # a cell the base has must not just vanish
            if any(cell(r)[1:4] == (f["variant"], f["backend"], f["engine"])
                   and r["program"].split(":")[0] == f["program"] for r in base["results"]):
                correctness.append(f"{f['program']} {f['variant']}/{f['backend']}/{f['engine']}: "
                                   f"absent, {f['reason']}")
    for series in sorted({cell(r)[:4] for r in head["results"]}):
        pts = lambda res: [(r["size"], r["median_ms"]) for r in res["results"] if cell(r)[:4] == series]
        sb, sh_ = slope(pts(base)), slope(pts(head))
        if sb is not None and sh_ is not None and sh_ - sb > slope_jump:
            immediate.append(f"{series}: complexity changed, log-log slope {sb:.2f} → {sh_:.2f}")
    loads = [x for x in map(host_load, (base, *heads)) if x is not None]
    load = max(loads) if loads else None
    if load is not None and not correctness:  # §17.153 item 3: direction only, never ok or blocked
        verdict, confirmation_programs = "advisory", set()
    elif immediate or correctness:
        worse("blocked")
    return {"verdict": verdict, "host_load": load, "rows": rows, "immediate": correctness + immediate,
            "rss": rss_rows, "confirmation_programs": sorted(confirmation_programs)}


def cmd_compare(a):
    base = json.loads(Path(a.base).read_text())
    heads = [json.loads(Path(h).read_text()) for h in a.head]
    for doc in (base, *heads):
        if doc.get("schema") != SCHEMA:
            log(f"not a {SCHEMA} file")
            return EXIT_BLOCKED
    out = compare(base, heads, a.flag, a.block, a.abs_ms)
    print("| Program | Backend | Engine | n | Base ratio | Head ratio | Δ | Status |\n|---|---|---|---:|---:|---:|---:|---|")
    for r in out["rows"]:
        print(f"| {r['program']} | {r['backend']} | {r['engine']} | {r['size']} | {r['base_ratio']:.3f} | {r['head_ratio']:.3f} "
              f"| {r['delta']:+.1%} | {r['status']} |")
    memory_changes = [r for r in out["rss"] if r["status"] in ("flagged", "blocked")
                      or (r["base_median_bytes"] and r["head_median_bytes"]
                          and r["head_median_bytes"] > r["base_median_bytes"] * 1.25)]
    for r in memory_changes:
        median = (f"{r['base_median_bytes']} → {r['head_median_bytes']} bytes"
                  if r["base_median_bytes"] is not None and r["head_median_bytes"] is not None else "unavailable (legacy result)")
        confirmation = (f"; confirmation maximum {r['confirmation_bytes']} bytes, "
                        f"process median {r['confirmation_median_bytes']} bytes "
                        f"(processes {r['confirmation_processes'] or 'unknown'})"
                        if r["confirmation_bytes"] is not None else "")
        print(f"\nRSS detail: {r['program']} {r['variant']}/{r['backend']}/{r['engine']} n={r['size']}: "
              f"maximum {r['base_bytes']} → {r['head_bytes']} bytes; "
              f"process median {median} "
              f"(processes {r['base_processes'] or 'unknown'} → {r['head_processes'] or 'unknown'}; median is diagnostic only)"
              f"{confirmation}; {r['status']} {r['note']}")
    for i in out["immediate"]:
        print(f"\nIMMEDIATE FAILURE: {i}")
    advisory = f" (host load {out['host_load']:.2f}; direction only, §17.153)" if out["verdict"] == "advisory" else ""
    print(f"\nverdict: {out['verdict']}{advisory}")
    if a.json:
        Path(a.json).write_text(json.dumps(out, indent=1) + "\n")
    return EXIT_BLOCKED if out["verdict"] == "blocked" else EXIT_OK


# ---------------------------------------------------------------------------------------------
# bench envelope (§17.213 (2))

# §17.109's M4 envelope: per family, the native comparisons and their ceilings on the ratio of
# medians Sakura-WASM / native, on the Node host and on Wasmtime.
M4_ENVELOPE = (
    (("fib", "countdown", "tail_clique", "sieve", "nbody", "spectral_norm"), (("rust", 3.0), ("go", 2.0))),
    (("binary_trees", "rbtree", "pipeline", "shapes", "map_set"),
     (("ocaml", 2.5), ("koka", 2.5), ("koka-noreuse", 2.5))),
    (("handled_*",), (("koka", 3.0),)),
    (("tokenizer", "json_validate"), (("rust", 3.0),)),
)
NOT_GATING = {"tail_clique"}  # reported beside `countdown`, not an M4 exit cell (§17.213 (4))


def refusal(wasm, native):
    """Why two files are not one window's WASM and native runs, or None."""
    for d, suites, name in ((wasm, ("wasm",), "wasm"), (native, ("native", "all"), "native")):
        if d.get("schema") != SCHEMA:
            return f"the {name} file is not a {SCHEMA} file"
        if d["meta"].get("flags", {}).get("suite") not in suites:
            return f"the {name} file is not a `--suite {name}` run"
    if native["meta"]["flags"].get("lane") != "matched":
        return f"the native file's lane is {native['meta']['flags'].get('lane')}; the envelope reads Rust matched"
    for field, get in (("meta.commit", lambda d: d["meta"].get("commit")),
                       ("meta.host.id", lambda d: d["meta"].get("host", {}).get("id"))):
        if get(wasm) != get(native):
            return f"{field} differs: {get(wasm)} (wasm) and {get(native)} (native)"
    return None


def envelope(wasm, native):
    """One row per program × size × engine × comparison with its ceiling and status, and the
    reading: `met`, `miss` (a gating cell over its ceiling) or `incomplete` (a gating cell
    unmeasured); `advisory (…)` instead when either run is not quiet, with the reading it would
    have been as `direction` (§17.153). A Wasmtime cell recorded in `meta.absent` is read as its
    Node cell (§17.213 (1))."""
    by = {cell(r): r for d in (wasm, native) for r in d["results"]}
    absent = {(f["program"], f["engine"]) for f in wasm["meta"].get("absent", [])}
    rows = []
    for patterns, lanes in M4_ENVELOPE:
        for p in [p for pattern in patterns for p in programs(pattern)]:
            sizes = sorted({r["size"] for d in (wasm, native) for r in d["results"] if r["program"] == p}) or [None]
            for n in sizes:
                for engine in WASM_ENGINES:
                    for against, ceiling in lanes:
                        s, c = by.get((p, "sakura", "wasm", engine, n)), by.get((p, against, "native", "native", n))
                        ratio = None
                        if (p, engine) in absent:
                            status = "absent"
                        elif not (s and c and s["median_ms"] and c["median_ms"]):
                            status = "missing"
                        elif not (s["checksum_ok"] and c["checksum_ok"]):
                            status = "checksum"
                        else:
                            ratio = s["median_ms"] / c["median_ms"]
                            status = "met" if ratio <= ceiling else "miss"
                        rows.append({"program": p, "size": n, "engine": engine, "against": against,
                                     "sakura_ms": s and s["median_ms"], "native_ms": c and c["median_ms"],
                                     "ratio": ratio, "ceiling": ceiling, "status": status,
                                     "gating": p not in NOT_GATING})
    gating = {r["status"] for r in rows if r["gating"]}
    reading = "miss" if "miss" in gating else "incomplete" if gating & {"missing", "checksum"} else "met"
    loud = [d["meta"].get("host", {}) for d in (wasm, native) if d["meta"].get("host", {}).get("quiet") is not True]
    loads = [h["load"]["max_load_1m"] for h in loud if h.get("load") and not h["load"].get("ci")]
    advisory = (None if not loud else f"advisory (host load {max(loads):.2f})" if loads
                else "advisory (CI runner)" if any((h.get("load") or {}).get("ci") for h in loud)
                else "advisory (no quiet verdict)")
    return {"commit": wasm["meta"].get("commit"), "host_id": wasm["meta"].get("host", {}).get("id"),
            "reading": advisory or reading, "direction": reading,
            "reasons": [x for h in loud for x in (h.get("load") or {}).get("reasons", [])], "rows": rows}


def envelope_markdown(out):
    num = lambda x, f: "—" if x is None else format(x, f)
    lines = ["| Program | n | Engine | Against | Sakura ms | Native ms | Ratio | Ceiling | Status |",
             "|---|---:|---|---|---:|---:|---:|---:|---|"]
    for r in out["rows"]:
        status = r["status"] + ("" if r["gating"] else " (not gating)")
        ratio = "—" if r["ratio"] is None else f"{r['ratio']:.2f}×"
        lines.append(f"| {r['program']} | {num(r['size'], '')} | {r['engine']} | {r['against']} "
                     f"| {num(r['sakura_ms'], '.4g')} | {num(r['native_ms'], '.4g')} | {ratio} | {r['ceiling']} | {status} |")
    counts = {}
    for r in out["rows"]:
        if r["gating"]:
            counts[r["status"]] = counts.get(r["status"], 0) + 1
    tally = ", ".join(f"{k} {v}" for k, v in sorted(counts.items()))
    direction = f", direction only: {out['direction']}" if out["reading"] != out["direction"] else ""
    lines += ["", f"M4 envelope: {out['reading']}{direction} (gating cells: {tally}; "
                  f"commit {out['commit']}, host {out['host_id']})"]
    return "\n".join(lines)


def cmd_envelope(a):
    wasm, native = (json.loads(Path(f).read_text()) for f in (a.wasm, a.native))
    if reason := refusal(wasm, native):
        log(f"refusing: {reason}")
        return EXIT_BLOCKED
    out = envelope(wasm, native)
    text = envelope_markdown(out)
    print(text)
    if a.json:
        Path(a.json).write_text(json.dumps(out, indent=1) + "\n")
    if a.md:
        Path(a.md).write_text(text + "\n")
    return EXIT_OK  # a report: the gate is the owner's reading of it (§17.213 (2))


# ---------------------------------------------------------------------------------------------
# bench bless

def cmd_bless(a):
    doc = json.loads(Path(a.result).read_text())
    if doc.get("schema") != SCHEMA:
        log(f"not a {SCHEMA} file")
        return EXIT_ORACLE
    bad = [r for r in doc["results"] if not r["checksum_ok"]] + doc["meta"].get("failures", [])
    if bad or doc["meta"].get("dirty"):
        log("refusing to bless: " + ("oracle failures" if bad else "the run was made from a dirty tree"))
        return EXIT_ORACLE
    h = doc["meta"].get("host", {})
    if h.get("quiet") is not True:  # §17.153 item 1
        reasons = (h.get("load") or {}).get("reasons") or ["no quiet verdict recorded (made before §17.153)"]
        log("refusing to bless: the run was not made in a quiet window: " + "; ".join(reasons))
        return EXIT_ORACLE
    # A WASM run has no JS-lane cells: blessed over the JS baseline it would leave the PR job's
    # compare nothing to compare.
    suite = doc["meta"].get("flags", {}).get("suite")
    dest = BENCH / "baselines" / ("wasm" if suite == "wasm" else "") / f"{doc['meta']['host']['id']}.json"
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy(a.result, dest)
    log(f"blessed {dest.relative_to(ROOT)}; review and commit it by hand")
    return EXIT_OK


# ---------------------------------------------------------------------------------------------
# bench compile (research/10 §6 budgets, p95 over 15 processes)

BUDGETS_MS = {"check_cold_10k": 1000, "build_10k": 500, "check_cold_100k": 3000, "lsp_edit_10k": 100}
# Design §17.195 (1): reported beside the budgets, never an exit, until the owner promotes one.
TARGETS_MS = {"check_hello": 10, "check_cold_10k": 45}


def hello():
    """The `sakura new cli` project, made by the CLI under test."""
    d = TARGET / "hello"
    if not (d / "src" / "main.sak").exists():
        shutil.rmtree(d, ignore_errors=True)
        sh([str(SAKURA), "new", "cli", d.name], cwd=d.parent)
    return d


def corpus(lines):
    """A generated package of ~`lines` lines: modules of plain functions, records and matches."""
    d = TARGET / "corpus" / f"{lines}"
    entry = d / "src" / "main.sak"
    if entry.exists() and "import corpus/" in entry.read_text():  # older corpora import `main/`
        return d
    (d / "src" / "gen").mkdir(parents=True, exist_ok=True)
    (d / "sakura.toml").write_text('[package]\nname = "corpus"\nversion = "0.1.0"\nedition = "2026"\n')
    per_fn, per_mod = 10, 50
    mods = max(1, lines // (per_fn * per_mod))
    for m in range(mods):
        body = [f"// Generated by `bench compile`: module {m} of {mods}.", "",
                f"pub type R{m} = {{ a: Int, b: String }}", ""]
        for f in range(per_mod):
            body += [f"pub fn f{f}(x: Int, r: R{m}) -> Int",
                     "  requires x >= 0",
                     "{",
                     f"  let y = x * {f + 1} % 1000",
                     f"  let s = match r.b {{ \"{f}\" => 1, _ => 2 }}",
                     f"  if y > {f} {{ y + s + String.len(r.b) }} else {{ r.a + s }}",
                     "}", "", "", ""]
        (d / "src" / "gen" / f"m{m}.sak").write_text("\n".join(body))
    imports = [f"import corpus/gen/m{m}" for m in range(mods)]
    calls = " + ".join(f"m{m}.f0(1, m{m}.R{m} {{ a: 1, b: \"x\" }})" for m in range(mods))
    (d / "src" / "main.sak").write_text("\n".join(imports) + f"\n\npub fn main() -> Unit ! {{Console}} {{\n"
                                           f"  Console.print(\"{{{calls}}}\")\n}}\n")
    return d


def wide_match():
    """§17.200 (8)'s 3,000-arm `Int` match and 3,000-constructor enumeration, alternating guarded rows."""
    d = TARGET / "wide_match"
    d.mkdir(parents=True, exist_ok=True)
    n = 3_000
    src = f"type Big = {' | '.join(f'C{i}' for i in range(n))}\n\nfn big(b: Big) -> Int {{\n  match b {{\n"
    src += "".join(f"    C{i} => {i},\n" for i in range(n))
    src += "  }\n}\n\nfn int(x: Int) -> Int {\n  match x {\n"
    src += "".join(f"    n if n < {-i} => {i},\n    {i} => {i},\n" for i in range(n))
    (d / "main.sak").write_text(src + "    _ => -1,\n  }\n}\n")
    return d


def p95(xs):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, math.ceil(0.95 * len(xs)) - 1)]


def timed(cmd, runs=15):
    sh(cmd)  # warm-up: the OS file cache and std's analysis cache (design §17.195 (3))
    out = []
    for _ in range(runs):
        t = time.perf_counter()
        sh(cmd)
        out.append((time.perf_counter() - t) * 1000)
    return out


def lsp_edit_ms(root, runs=15):
    """Warm `sakura lsp`: time from a didChange to its publishDiagnostics. The server publishes
    nothing for a clean file it has not reported before, so edits alternate between a private
    body with a type error and the fixed body; both publish."""
    path = root / "src" / "gen" / "m0.sak"
    uri, text = path.as_uri(), path.read_text()
    broken = text.replace("x * 1 % 1000", 'x * "1" % 1000', 1)
    proc = subprocess.Popen([str(SAKURA), "lsp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, cwd=root)

    def send(msg):
        data = json.dumps({"jsonrpc": "2.0", **msg}).encode()
        proc.stdin.write(b"Content-Length: %d\r\n\r\n" % len(data) + data)
        proc.stdin.flush()

    def recv():
        length = 0
        while (line := proc.stdout.readline()).strip():
            if line.lower().startswith(b"content-length:"):
                length = int(line.split(b":")[1])
        if not line:
            raise Fail("sakura lsp closed its output")
        return json.loads(proc.stdout.read(length))

    def published():
        while (m := recv()).get("method") != "textDocument/publishDiagnostics" or m["params"]["uri"] != uri:
            pass
        return m["params"]["diagnostics"]

    try:
        send({"id": 1, "method": "initialize",
              "params": {"processId": os.getpid(), "rootUri": root.as_uri(), "capabilities": {}}})
        while recv().get("id") != 1:
            pass
        send({"method": "initialized", "params": {}})
        send({"method": "textDocument/didOpen",
              "params": {"textDocument": {"uri": uri, "languageId": "sakura", "version": 1, "text": text}}})
        out = []
        for v in range(2, runs + 2):
            t = time.perf_counter()
            send({"method": "textDocument/didChange",
                  "params": {"textDocument": {"uri": uri, "version": v},
                             "contentChanges": [{"text": broken if v % 2 == 0 else text}]}})
            diags = published()
            out.append((time.perf_counter() - t) * 1000)
            if bool(diags) != (v % 2 == 0):
                raise Fail(f"sakura lsp: unexpected diagnostics after edit {v}: {diags[:1]}")
        send({"id": 2, "method": "shutdown"})
        send({"method": "exit"})
        return out
    finally:
        proc.kill()
        proc.wait()


def cmd_compile(a):
    build_cli()
    # The bench's own cache of std's analysis, so a run does not depend on the user's.
    os.environ.setdefault("SAKURA_CACHE_DIR", str(TARGET / "cache"))
    res = new_result({"command": "compile", "seed": SEED,
                      "std_cache": "off" if os.environ.get("SAKURA_NO_CACHE", "0") not in ("", "0") else "on"})
    TARGET.mkdir(parents=True, exist_ok=True)
    small, large, hi, wide = corpus(10_000), corpus(100_000), hello(), wide_match()
    lines = lambda d: sum(len(f.read_text().splitlines()) for f in d.rglob("*.sak"))
    # `test_hello` has no budget or target: it shows the Node spawn's share (§17.195 (1)).
    cases = {"check_hello": (hi, lambda: timed([str(SAKURA), "check", str(hi)])),
             "test_hello": (hi, lambda: timed([str(SAKURA), "test", str(hi)])),
             "check_cold_10k": (small, lambda: timed([str(SAKURA), "check", str(small)])),
             "build_10k": (small, lambda: timed([str(SAKURA), "build", "--out-dir", str(TARGET / "corpus-out"), str(small)])),
             "check_cold_100k": (large, lambda: timed([str(SAKURA), "check", str(large)])),
             "lsp_edit_10k": (small, lambda: lsp_edit_ms(small)),
             "check_wide_match": (wide, lambda: timed([str(SAKURA), "check", str(wide)]))}
    status = EXIT_OK
    print("| Case | Lines | p95 ms | Median ms | Budget ms | Target ms | |\n|---|---:|---:|---:|---:|---:|---|",
          flush=True)
    for name, (d, run) in cases.items():
        budget, target = BUDGETS_MS.get(name), TARGETS_MS.get(name)
        cols = f"{budget or ''} | {target or ''}"
        try:
            ms = run()
        except Fail as e:
            res["meta"]["failures"].append({"program": name, "variant": "sakura", "engine": "cli", "error": str(e)})
            print(f"| {name} | {lines(d)} | ✗ | | {cols} | {str(e).splitlines()[0]} |")
            status = EXIT_BLOCKED
            continue
        over = budget is not None and p95(ms) > budget
        missed = target is not None and p95(ms) > target
        status = EXIT_BLOCKED if over else status
        res["results"].append({"program": name, "variant": "sakura", "engine": "cli", "lane": "compile", "ints": None,
                               "size": lines(d), "samples_ms": ms, "median_ms": statistics.median(ms),
                               "p95_ms": p95(ms), "budget_ms": budget, "target_ms": target,
                               "process_medians_ms": ms, "rss_bytes": None, "checksum_ok": True, "checksum": None})
        verdict = "over budget" if over else "target missed" if missed else "ok"
        print(f"| {name} | {lines(d)} | {p95(ms):.1f} | {statistics.median(ms):.1f} | {cols} | {verdict} |")
    Path(a.json).parent.mkdir(parents=True, exist_ok=True)
    Path(a.json).write_text(json.dumps(res, indent=1) + "\n")
    return status


# ---------------------------------------------------------------------------------------------
# bench oracle (deviation 4: records and checks meta.toml [checksums])

def cmd_oracle(a):
    build_cli()
    status = EXIT_OK
    for p in programs(a.filter):
        m = meta(p)
        try:
            entry = build_sakura(p)
            js = PROGRAMS / p / "js" / "main.mjs"
            if a.write:
                tables = {}
                for case in [None, *cases(m, "node")]:
                    fn = ["--fn", f"run_{case}"] if case else []
                    values = lambda art: json_lines(spawn(command("x", "node", art, "values", *fn,
                                                                  *oracle_inputs(m)))[0])[-1]["values"]
                    sk, hand = values(entry), values(js)
                    if sk != hand:
                        raise Fail(f"{p}: Sakura {label('run', case)} {sk} != JS {hand}")
                    tables[case] = sk
                sk = tables.pop(None)
                write_checksums(p, sk, tables)
                log(f"{p}: wrote {len(sk)} checksums")
            else:
                oracle(p, m, "sakura", "node", entry)
                oracle(p, m, "js", "node", js)
                log(f"{p}: ok")
        except Fail as e:
            log(e)
            status = EXIT_ORACLE
    return status


def write_checksums(p, values, case_values=None):
    path = PROGRAMS / p / "meta.toml"
    head = path.read_text().split("[checksums]")[0].rstrip() + "\n\n"
    table = lambda vs: "".join(f'"{n}" = "{v}"\n' for n, v in vs.items())
    body = "[checksums]  # run(n), recorded by `bench oracle --write`\n" + table(values)
    for case, vs in (case_values or {}).items():
        body += f"\n[checksums.{case}]  # run_{case}(n)\n" + table(vs)
    path.write_text(head + body)


# ---------------------------------------------------------------------------------------------

def main(argv=None):
    ap = argparse.ArgumentParser(prog="bench", description=__doc__)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run", help="build, verify and time the suite")
    r.add_argument("--suite", choices=["js", "wasm", "native", "all"], default="js")
    r.add_argument("--engine", choices=["node", "bun", "wasmtime"])
    r.add_argument("--lane", choices=["matched", "idiomatic"], default="matched")
    r.add_argument("--filter", default="*", help="glob over program names")
    r.add_argument("--sizes", choices=["all", "quick"], default="all")
    r.add_argument("--stable", action="store_true", help="2 s in-process warmup instead of 250 ms")
    r.add_argument("--processes", type=int, default=5, help="fresh processes per combination")
    r.add_argument("--seed", type=int, default=SEED)
    r.add_argument("--json", required=True)
    c = sub.add_parser("compare", help="compare Sakura/hand-JS ratios of head against base (§17.110)")
    c.add_argument("base")
    c.add_argument("head", nargs="+", help="head run; a second file is the confirmation rerun")
    c.add_argument("--flag", type=float, default=0.05)
    c.add_argument("--block", type=float, default=0.10)
    c.add_argument("--abs-ms", type=float, default=0.1)
    c.add_argument("--json")
    v = sub.add_parser("envelope", help="the §17.109 M4 envelope from one window's wasm and native runs")
    v.add_argument("wasm", help="a `run --suite wasm` result")
    v.add_argument("native", help="a `run --suite native --lane matched` result from the same commit and host")
    v.add_argument("--json")
    v.add_argument("--md")
    b = sub.add_parser("bless", help="copy a reviewed run into baselines/<host-id>.json")
    b.add_argument("result")
    k = sub.add_parser("compile", help="check/build/LSP latency against the research/10 §6 budgets")
    k.add_argument("--json", required=True)
    o = sub.add_parser("oracle", help="check Sakura and hand JS against meta.toml [checksums]")
    o.add_argument("--filter", default="*")
    o.add_argument("--write", action="store_true", help="record [checksums]; Sakura must equal JS")
    a = ap.parse_args(argv)
    if a.cmd in ("run", "compile", "oracle") and not shutil.which("node"):
        log("node is required")
        return EXIT_TOOLCHAIN
    try:
        return {"run": cmd_run, "compare": cmd_compare, "bless": cmd_bless, "compile": cmd_compile,
                "envelope": cmd_envelope, "oracle": cmd_oracle}[a.cmd](a)
    except Fail as e:  # a build of the CLI itself failed
        log(e)
        return EXIT_ORACLE


if __name__ == "__main__":
    sys.exit(main())
