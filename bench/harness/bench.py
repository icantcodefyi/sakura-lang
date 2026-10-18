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
import time
import tomllib
from pathlib import Path

BENCH = Path(__file__).resolve().parent.parent
ROOT = BENCH.parent
PROGRAMS = BENCH / "programs"
TARGET = BENCH / "target"
RUNNER = BENCH / "harness" / "runner.mjs"
SAKURA = ROOT / "compiler" / "target" / "release" / "sakura"
SCHEMA = "sakura-bench-v1"
SMALL = [0, 1, 2, 3]
SEED = 20261018
PROCESS_TIMEOUT = 600

EXIT_OK, EXIT_ORACLE, EXIT_BLOCKED, EXIT_TOOLCHAIN = 0, 1, 2, 3


class Fail(Exception):
    """A build or oracle failure for one program; it is recorded and that program is not timed."""


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
    """Compile programs/<p>/sakura/main.sakura; return an entry module for runner.mjs."""
    out = TARGET / "sakura" / p
    sh([str(SAKURA), "build", "--out-dir", str(out), str(PROGRAMS / p / "sakura" / "main.sakura")])
    # ponytail: rewrites the emitted __main__.js (std imports + registerOps, then runMain) into
    # an importable entry; replace with a `sakura build` library mode if the CLI grows one.
    lines = (out / "__main__.js").read_text().splitlines()
    kept = [l for l in lines if not l.startswith("runMain(") and "$entry" not in l]
    kept = [l.replace("import { registerOps, runMain }", "import { registerOps }") for l in kept]
    kept += ['export * as program from "./main/main.js";', 'export { runTask } from "./sakura_runtime.js";']
    entry = out / "bench_entry.mjs"
    entry.write_text("\n".join(kept) + "\n")
    return entry


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
    shutil.copy(PROGRAMS / p / "ocaml" / "main.ml", d / "main.ml")  # ocamlopt writes next to its source
    sh(["ocamlopt", "-O3", "-o", "main", "main.ml"] if ocaml_flambda() else ["ocamlopt", "-o", "main", "main.ml"], cwd=d)
    return d / "main"


def ocaml_flambda():
    r = subprocess.run(["ocamlopt", "-config-var", "flambda"], capture_output=True, text=True)
    return r.stdout.strip() == "true"


def build_koka(p, _lane):
    d = TARGET / "koka" / p
    sh(["koka", "-O2", f"--outputdir={d / 'build'}", "-o", str(d / "main"), str(PROGRAMS / p / "koka" / "main.kk")])
    return d / "main"


# Native variants (plan §2.2): source marker, toolchain, builder. The `matched` lane builds Rust
# with overflow checks, `idiomatic` without; the other languages have one build.
NATIVE = {
    "rust": ("rust/Cargo.toml", "cargo", build_rust, ["rustc", "--version"]),
    "go": ("go/main.go", "go", build_go, ["go", "version"]),
    "ocaml": ("ocaml/main.ml", "ocamlopt", build_ocaml, ["ocamlopt", "-version"]),
    "koka": ("koka/main.kk", "koka", build_koka, ["koka", "--version"]),
}


def variants(suite, engines, p):
    """(variant, engine) pairs that exist for program p in this suite."""
    out = []
    if suite in ("js", "all"):
        out += [(v, e) for e in engines for v in ("sakura", "js")]
    if suite in ("native", "all"):
        out += [(v, "native") for v, (src, *_rest) in NATIVE.items() if (PROGRAMS / p / src).exists()]
    return out


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


def oracle(p, m, variant, engine, artefact):
    """Raise Fail unless the artefact reproduces every checksum (and defect) in meta.toml."""
    want = m.get("checksums", {})
    got = json_lines(spawn(command(variant, engine, artefact, "values", *oracle_inputs(m)))[0])[-1]["values"]
    bad = [f"run({n}) = {got.get(str(n))}, expected {want.get(str(n))}"
           for n in oracle_inputs(m) if str(got.get(str(n))) != str(want.get(str(n)))]
    if variant == "sakura":
        for case in m.get("negative", []):
            out = spawn(command(variant, engine, artefact, "negative", case["arg"]))[0]
            code = json_lines(out)[-1]["defects"][str(case["arg"])]
            if code != case["defect"]:
                bad.append(f"negative({case['arg']}) gave {code}, expected {case['defect']}")
    if bad:
        raise Fail(f"{p} {variant}/{engine}: oracle failed: " + "; ".join(bad))


def measure(variant, engine, artefact, n, stable):
    """One fresh process: nine samples (ms per call), the checksum, peak RSS."""
    out, rss = spawn(command(variant, engine, artefact, "measure", n, *(["--stable"] if stable else [])))
    lines = json_lines(out)
    if engine == "native":
        samples = [l["ms"] for l in lines]
        checksum = lines[-1]["checksum"] if lines else None
    else:
        samples, checksum = lines[-1]["samples_ms"], lines[-1]["checksum"]
    return samples, str(checksum), rss


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


ENGINE_VERSIONS = {"node": ["node", "--version"], "bun": ["bun", "--version"], "rustc": ["rustc", "--version"],
                   "go": ["go", "version"], "ocaml": ["ocamlopt", "-version"], "koka": ["koka", "--version"],
                   "hyperfine": ["hyperfine", "--version"]}


def engines_meta():
    return {k: version(cmd) for k, cmd in ENGINE_VERSIONS.items() if shutil.which(cmd[0])}


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True).stdout.strip()


def new_result(flags):
    return {"schema": SCHEMA,
            "meta": {"commit": git("rev-parse", "HEAD"), "dirty": bool(git("status", "--porcelain", "--", "compiler", "bench")),
                     "host": host(), "engines": engines_meta(), "flags": flags, "seed": flags["seed"],
                     "started": datetime.datetime.now(datetime.UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
                     "missing": [], "failures": []},
            "results": []}


# ---------------------------------------------------------------------------------------------
# bench run

def cmd_run(a):
    engines = [a.engine] if a.engine else ["node", "bun"]
    sizes_flag = a.sizes
    flags = {"suite": a.suite, "engines": engines, "lane": a.lane, "sizes": sizes_flag, "stable": a.stable,
             "filter": a.filter, "processes": a.processes, "seed": a.seed}
    res = new_result(flags)
    missing = res["meta"]["missing"]
    if a.suite in ("js", "all"):
        missing += [e for e in engines if not shutil.which(e)]
    if a.suite in ("native", "all"):
        missing += [tool for _v, (_s, tool, *_r) in NATIVE.items() if not shutil.which(tool)]
    for tool in missing:
        log(f"[missing] {tool}: its variants are skipped")
    status = EXIT_TOOLCHAIN if missing else EXIT_OK

    build_cli()
    jobs = []  # (program, variant, engine, artefact, size)
    for p in programs(a.filter):
        m = meta(p)
        sizes = m["sizes"][:2] if sizes_flag == "quick" else m["sizes"]
        built, verified, failed = {}, [], False
        for variant, engine in variants(a.suite, engines, p):
            tool = engine if engine != "native" else NATIVE[variant][1]
            if tool in missing:
                continue
            try:
                if variant not in built:  # one build per variant, shared by the engines
                    built[variant] = (build_sakura(p) if variant == "sakura" else
                                      PROGRAMS / p / "js" / "main.mjs" if variant == "js" else
                                      NATIVE[variant][2](p, a.lane))
                log(f"[oracle] {p} {variant}/{engine}")
                oracle(p, m, variant, engine, built[variant])
                verified.append((variant, engine))
            except Fail as e:
                log(f"[FAIL] {e}")
                res["meta"]["failures"].append({"program": p, "variant": variant, "engine": engine, "error": str(e)})
                failed = True
        if failed:  # plan §3.2: nothing is timed for a program whose oracle failed
            status = EXIT_ORACLE
            continue
        jobs += [(p, v, e, built[v], n) for v, e in verified for n in sizes]

    order = [(j, i) for j in jobs for i in range(a.processes)]
    random.Random(a.seed).shuffle(order)
    collected = {}
    for k, (job, _i) in enumerate(order, 1):
        p, variant, engine, art, n = job
        log(f"[{k}/{len(order)}] {p} {variant}/{engine} n={n}")
        try:
            samples, checksum, rss = measure(variant, engine, art, n, a.stable)
        except Fail as e:
            samples, checksum, rss = [], f"error: {e}", 0
        collected.setdefault(job, []).append((samples, checksum, rss))

    for (p, variant, engine, _art, n), runs in collected.items():
        m = meta(p)
        expected = str(m["checksums"][str(n)])
        medians = [statistics.median(s) for s, _c, _r in runs if s]
        res["results"].append({
            "program": p, "variant": variant, "engine": engine, "lane": a.lane, "ints": m["ints"], "size": n,
            "samples_ms": [x for s, _c, _r in runs for x in s],
            "median_ms": statistics.median(medians) if medians else None,
            "process_medians_ms": medians,
            "rss_bytes": max(r for _s, _c, r in runs),
            "process_rss_bytes": [r for _s, _c, r in runs],
            "checksum_ok": all(c == expected for _s, c, _r in runs),
            "checksum": next((c for _s, c, _r in runs if c != expected), expected)})
        if not res["results"][-1]["checksum_ok"]:
            status = EXIT_ORACLE

    Path(a.json).parent.mkdir(parents=True, exist_ok=True)
    Path(a.json).write_text(json.dumps(res, indent=1) + "\n")
    print(summary(res))
    log(f"wrote {a.json}")
    return status


def ratios(res):
    """{(program, engine, size): (sakura_ms, js_ms)} for every pair present."""
    by = {(r["program"], r["variant"], r["engine"], r["size"]): r for r in res["results"]}
    out = {}
    for (p, v, e, n), r in by.items():
        js = by.get((p, "js", e, n))
        if v == "sakura" and js and r["median_ms"] and js["median_ms"]:
            out[(p, e, n)] = (r["median_ms"], js["median_ms"])
    return out


def summary(res):
    """Markdown table of medians and Sakura/hand-JS ratios (and native medians if present)."""
    rows = {}
    for r in res["results"]:
        rows.setdefault((r["program"], r["size"]), {})[(r["variant"], r["engine"])] = r
    cols = sorted({k for v in rows.values() for k in v}, key=lambda k: (k[1] == "native", k[1], k[0] != "sakura", k[0]))
    engines = sorted({e for _v, e in cols if e != "native"})
    head = ["Program", "n"] + [f"{v}/{e} ms" for v, e in cols] + [f"ratio {e}" for e in engines]
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
        for e in engines:
            s, j = cells.get(("sakura", e)), cells.get(("js", e))
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
    Node RSS needs the first two heads to cross; Bun RSS is advisory."""
    b_rat = ratios(base)
    h_rats = [ratios(h) for h in heads]
    head = heads[0]
    rows, verdict = [], "ok"

    def worse(v):
        nonlocal verdict
        order = ["ok", "flagged", "blocked"]
        verdict = max(verdict, v, key=order.index)

    for key in sorted(set(b_rat) & set(h_rats[0])):
        p, e, n = key
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
        rows.append({"program": p, "engine": e, "size": n, "base_ratio": base_ratio,
                     "head_ratio": h_rats[0][key][0] / h_rats[0][key][1], "delta": d, "status": status})

    immediate, rss_rows = [], []
    confirmation_programs = {r["program"] for r in rows if r["status"] == "blocked"} if len(heads) == 1 else set()
    confirmation = heads[1] if len(heads) > 1 else None
    confirmation_by = {(r["program"], r["variant"], r["engine"], r["size"]): r
                       for r in confirmation["results"]} if confirmation else {}
    b_by = {(r["program"], r["variant"], r["engine"], r["size"]): r for r in base["results"]}
    for r in head["results"]:
        key = (r["program"], r["variant"], r["engine"], r["size"])
        b = b_by.get(key)
        if b:
            try:
                memory = rss_comparison(base, b, head, r, rss_jump)
                cr = confirmation_by.get(key)
                confirmed = rss_comparison(base, b, confirmation, cr, rss_jump) if cr else None
            except ValueError as error:
                immediate.append(f"{key}: {error}")
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
                rss_rows.append({field: r[field] for field in ("program", "variant", "engine", "size")} | memory)
                if memory["status"] == "blocked" and r["engine"] != "node":
                    before, after = memory["base_bytes"], memory["head_bytes"]
                    immediate.append(f"{key}: peak RSS ({memory['statistic']}) {before} → {after} (+{after / before - 1:.0%})")
    for group in heads:
        for r in group["results"]:
            key = (r["program"], r["variant"], r["engine"], r["size"])
            if not r["checksum_ok"]:
                immediate.append(f"{key}: checksum {r['checksum']}")
            b = b_by.get(key)
            if b and b["checksum_ok"] and r["checksum_ok"] and b["checksum"] != r["checksum"]:
                immediate.append(f"{key}: checksum changed {b['checksum']} → {r['checksum']}")
        for f in group["meta"].get("failures", []):
            immediate.append(f"{f['program']} {f['variant']}/{f['engine']}: {f['error'].splitlines()[0]}")
    for p, v, e in sorted({(r["program"], r["variant"], r["engine"]) for r in head["results"]}):
        pts = lambda res: [(r["size"], r["median_ms"]) for r in res["results"]
                           if (r["program"], r["variant"], r["engine"]) == (p, v, e)]
        sb, sh_ = slope(pts(base)), slope(pts(head))
        if sb is not None and sh_ is not None and sh_ - sb > slope_jump:
            immediate.append(f"({p!r}, {v!r}, {e!r}): complexity changed, log-log slope {sb:.2f} → {sh_:.2f}")
    if immediate:
        worse("blocked")
    return {"verdict": verdict, "rows": rows, "immediate": immediate, "rss": rss_rows,
            "confirmation_programs": sorted(confirmation_programs)}


def cmd_compare(a):
    base = json.loads(Path(a.base).read_text())
    heads = [json.loads(Path(h).read_text()) for h in a.head]
    for doc in (base, *heads):
        if doc.get("schema") != SCHEMA:
            log(f"not a {SCHEMA} file")
            return EXIT_BLOCKED
    out = compare(base, heads, a.flag, a.block, a.abs_ms)
    print("| Program | Engine | n | Base ratio | Head ratio | Δ | Status |\n|---|---|---:|---:|---:|---:|---|")
    for r in out["rows"]:
        print(f"| {r['program']} | {r['engine']} | {r['size']} | {r['base_ratio']:.3f} | {r['head_ratio']:.3f} "
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
        print(f"\nRSS detail: {r['program']} {r['variant']}/{r['engine']} n={r['size']}: "
              f"maximum {r['base_bytes']} → {r['head_bytes']} bytes; "
              f"process median {median} "
              f"(processes {r['base_processes'] or 'unknown'} → {r['head_processes'] or 'unknown'}; median is diagnostic only)"
              f"{confirmation}; {r['status']} {r['note']}")
    for i in out["immediate"]:
        print(f"\nIMMEDIATE FAILURE: {i}")
    print(f"\nverdict: {out['verdict']}")
    if a.json:
        Path(a.json).write_text(json.dumps(out, indent=1) + "\n")
    return EXIT_BLOCKED if out["verdict"] == "blocked" else EXIT_OK


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
    dest = BENCH / "baselines" / f"{doc['meta']['host']['id']}.json"
    dest.parent.mkdir(exist_ok=True)
    shutil.copy(a.result, dest)
    log(f"blessed {dest.relative_to(ROOT)}; review and commit it by hand")
    return EXIT_OK


# ---------------------------------------------------------------------------------------------
# bench compile (research/10 §6 budgets, p95 over 15 processes)

BUDGETS_MS = {"check_cold_10k": 1000, "build_10k": 500, "check_cold_100k": 3000, "lsp_edit_10k": 100}


def corpus(lines):
    """A generated package of ~`lines` lines: modules of plain functions, records and matches."""
    d = TARGET / "corpus" / f"{lines}"
    entry = d / "src" / "main.sakura"
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
        (d / "src" / "gen" / f"m{m}.sakura").write_text("\n".join(body))
    imports = [f"import corpus/gen/m{m}" for m in range(mods)]
    calls = " + ".join(f"m{m}.f0(1, m{m}.R{m} {{ a: 1, b: \"x\" }})" for m in range(mods))
    (d / "src" / "main.sakura").write_text("\n".join(imports) + f"\n\npub fn main() -> Unit ! {{Console}} {{\n"
                                           f"  Console.print(\"{{{calls}}}\")\n}}\n")
    return d


def p95(xs):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, math.ceil(0.95 * len(xs)) - 1)]


def timed(cmd, runs=15):
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
    path = root / "src" / "gen" / "m0.sakura"
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
    res = new_result({"command": "compile", "seed": SEED})
    small, large = corpus(10_000), corpus(100_000)
    lines = lambda d: sum(len(f.read_text().splitlines()) for f in d.rglob("*.sakura"))
    cases = {"check_cold_10k": (small, lambda: timed([str(SAKURA), "check", str(small)])),
             "build_10k": (small, lambda: timed([str(SAKURA), "build", "--out-dir", str(TARGET / "corpus-out"), str(small)])),
             "check_cold_100k": (large, lambda: timed([str(SAKURA), "check", str(large)])),
             "lsp_edit_10k": (small, lambda: lsp_edit_ms(small))}
    status = EXIT_OK
    print("| Case | Lines | p95 ms | Median ms | Budget ms | |\n|---|---:|---:|---:|---:|---|", flush=True)
    for name, (d, run) in cases.items():
        try:
            ms = run()
        except Fail as e:
            res["meta"]["failures"].append({"program": name, "variant": "sakura", "engine": "cli", "error": str(e)})
            print(f"| {name} | {lines(d)} | ✗ | | {BUDGETS_MS[name]} | {str(e).splitlines()[0]} |")
            status = EXIT_BLOCKED
            continue
        over = p95(ms) > BUDGETS_MS[name]
        status = EXIT_BLOCKED if over else status
        res["results"].append({"program": name, "variant": "sakura", "engine": "cli", "lane": "compile", "ints": None,
                               "size": lines(d), "samples_ms": ms, "median_ms": statistics.median(ms),
                               "p95_ms": p95(ms), "budget_ms": BUDGETS_MS[name], "process_medians_ms": ms,
                               "rss_bytes": None, "checksum_ok": True, "checksum": None})
        print(f"| {name} | {lines(d)} | {p95(ms):.1f} | {statistics.median(ms):.1f} | {BUDGETS_MS[name]} "
              f"| {'over budget' if over else 'ok'} |")
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
                values = lambda art: json_lines(spawn(command("x", "node", art, "values", *oracle_inputs(m)))[0])[-1]["values"]
                sk, hand = values(entry), values(js)
                if sk != hand:
                    raise Fail(f"{p}: Sakura {sk} != JS {hand}")
                write_checksums(p, sk)
                log(f"{p}: wrote {len(sk)} checksums")
            else:
                oracle(p, m, "sakura", "node", entry)
                oracle(p, m, "js", "node", js)
                log(f"{p}: ok")
        except Fail as e:
            log(e)
            status = EXIT_ORACLE
    return status


def write_checksums(p, values):
    path = PROGRAMS / p / "meta.toml"
    head = path.read_text().split("[checksums]")[0].rstrip() + "\n\n"
    body = "".join(f'"{n}" = "{v}"\n' for n, v in values.items())
    path.write_text(head + "[checksums]  # run(n), recorded by `bench oracle --write`\n" + body)


# ---------------------------------------------------------------------------------------------

def main(argv=None):
    ap = argparse.ArgumentParser(prog="bench", description=__doc__)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run", help="build, verify and time the suite")
    r.add_argument("--suite", choices=["js", "native", "all"], default="js")
    r.add_argument("--engine", choices=["node", "bun"])
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
                "oracle": cmd_oracle}[a.cmd](a)
    except Fail as e:  # a build of the CLI itself failed
        log(e)
        return EXIT_ORACLE


if __name__ == "__main__":
    sys.exit(main())
