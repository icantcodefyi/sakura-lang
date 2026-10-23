#!/usr/bin/env python3
"""The bench/http load test: build every server, seed it, ramp load, write JSON and Markdown.

    python3 bench/http/run.py --json out.json --md out.md            # everything, defaults below
    python3 bench/http/run.py --servers sakura-node,node --mixes read --repeats 1 --duration 2

Python 3.11+ standard library only. For each server and mix it starts a fresh server, seeds it
over HTTP (seed.py), checks the answers' shapes, warms up, then runs a closed-loop ramp over
`--concurrency` and an open-loop ramp at `--rate-factors` times the best closed-loop rate, each
point `--repeats` times. Every point records the server's peak RSS and the host's load average.
See README.md for the method. AI-assisted (Claude).
"""

import argparse
import datetime
import http.client
import json
import os
import platform
import resource
import shutil
import signal
import socket
import statistics
import subprocess
import sys
import threading
import time
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
TARGET = BENCH / "target" / "http"
sys.path.insert(0, str(BENCH / "harness"))
sys.path.insert(0, str(HERE))
import bench  # noqa: E402  host id, git and version helpers
import seed  # noqa: E402

SCHEMA = "sakura-http-v1"
READY_TIMEOUT_S = 60
SEED = {"users": 1000, "posts": 10000, "likes": 20000}
# A point passes when its error rate is under this (non-2xx answers and transport errors).
MAX_ERROR_RATE = 0.001
# An open-loop point keeps up when it completes at least this share of the offered rate.
KEEP_UP = 0.95


def log(*a):
    print(*a, file=sys.stderr, flush=True)


# ---------------------------------------------------------------------------------------------
# Servers: name → (runtime, command(port), env)

def grant(port):
    """The `SAKURA_PERMISSIONS` table `sakura run` passes for a manifest whose `net` is this port."""
    return {"SAKURA_PERMISSIONS": json.dumps({"fs": {"read": [], "write": []}, "net": [f"127.0.0.1:{port}"],
                                              "env": [], "process": []})}


def servers(art):
    js, wasm, native = art["sakura_js"], art["sakura_wasm"], art["sakura_native"]
    return {
        "sakura-node": ("node", lambda p: ["node", str(js), str(p)], grant),
        "sakura-bun": ("bun", lambda p: ["bun", str(js), str(p)], grant),
        "sakura-wasm": ("node", lambda p: ["node", str(wasm.parent / "sakura_wasm_host.js"), str(wasm), str(p)], grant),
        "sakura-native": ("native", lambda p: [str(native), str(p)], grant),
        "bun": ("bun", lambda p: ["bun", str(HERE / "bun" / "server.mjs"), str(p)], lambda p: {}),
        "node": ("node", lambda p: ["node", str(HERE / "node" / "server.mjs"), str(p)], lambda p: {}),
        "go": ("go", lambda p: [str(art["go"]), str(p)], lambda p: {"GOMAXPROCS": "1"}),
        "go-mt": ("go", lambda p: [str(art["go"]), str(p)], lambda p: {}),
    }


SERVER_NAMES = ["sakura-node", "sakura-bun", "sakura-wasm", "sakura-native", "bun", "node", "go", "go-mt"]
NEEDS = {"sakura-node": "sakura_js", "sakura-bun": "sakura_js", "sakura-wasm": "sakura_wasm",
         "sakura-native": "sakura_native", "go": "go", "go-mt": "go"}


def build(cli, names):
    """Build what `names` need. Returns the artefacts and {artefact: error} for failed builds."""
    art, failed = {k: None for k in ("sakura_js", "sakura_wasm", "sakura_native", "go")}, {}
    proj = str(HERE / "sakura")

    def step(key, cmd, out, cwd=None):
        try:
            bench.sh(cmd, cwd=cwd, timeout=1800)
            art[key] = out
        except bench.Fail as e:
            failed[key] = str(e)

    if {"sakura-node", "sakura-bun"} & set(names):
        step("sakura_js", [cli, "build", "--out-dir", str(TARGET / "sakura-js"), proj],
             TARGET / "sakura-js" / "__main__.js")
    if "sakura-wasm" in names:
        step("sakura_wasm", [cli, "build", "--target", "wasm", "--out-dir", str(TARGET / "sakura-wasm"), proj],
             TARGET / "sakura-wasm" / "main.wasm")
    if "sakura-native" in names:
        step("sakura_native", [cli, "build", "--target", "native", "--out-dir", str(TARGET / "sakura-native"), proj],
             TARGET / "sakura-native" / "main")
    if {"go", "go-mt"} & set(names):
        step("go", ["go", "build", "-o", str(TARGET / "go" / "server"), "."], TARGET / "go" / "server", cwd=HERE / "go")
    step("loadgen", ["go", "build", "-o", str(TARGET / "loadgen"), "."], TARGET / "loadgen", cwd=HERE / "loadgen")
    return art, failed


# ---------------------------------------------------------------------------------------------
# Processes

def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def pinned(cmd, cpus):
    return ["taskset", "-c", cpus, *cmd] if cpus else cmd


def rss_kib(pid):
    status = Path(f"/proc/{pid}/status")
    if status.exists():
        try:
            for line in status.read_text().splitlines():
                if line.startswith("VmRSS:"):
                    return int(line.split()[1])
        except OSError:
            pass
        return None
    out = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
    return int(out) if out.isdigit() else None


class Server:
    """One server process: started, sampled for RSS every 250 ms, and always stopped."""

    live = set()

    def __init__(self, cmd, env, log_path):
        self.log = open(log_path, "wb")
        self.proc = subprocess.Popen(cmd, env={**os.environ, **env}, stdout=self.log, stderr=subprocess.STDOUT,
                                     start_new_session=True)
        Server.live.add(self)
        self.peak = 0
        self._stop = threading.Event()
        threading.Thread(target=self._sample, daemon=True).start()

    def _sample(self):
        while not self._stop.wait(0.25):
            self.peak = max(self.peak, rss_kib(self.proc.pid) or 0)

    def take_peak(self):
        """The peak RSS since the last call and the current RSS, in MiB."""
        kib = rss_kib(self.proc.pid) or 0
        peak, self.peak = max(self.peak, kib), kib
        return round(peak / 1024, 1), round(kib / 1024, 1)

    def alive(self):
        return self.proc.poll() is None

    def stop(self):
        self._stop.set()
        if self.alive():
            self.proc.send_signal(signal.SIGTERM)
            try:
                self.proc.wait(5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
        self.log.close()
        Server.live.discard(self)


def stop_all(*_):
    for s in list(Server.live):
        s.stop()
    sys.exit(130)


def wait_ready(base, srv):
    deadline = time.monotonic() + READY_TIMEOUT_S
    while time.monotonic() < deadline:
        if not srv.alive():
            return False
        try:
            with urllib.request.urlopen(base + "/health", timeout=1) as r:
                if r.status == 200:
                    return True
        except OSError:
            time.sleep(0.1)
    return False


def check_shapes(base):
    """Problems with a freshly seeded server's answers (every server must give the same JSON)."""
    problems = []

    def get(path):
        with urllib.request.urlopen(base + path, timeout=5) as r:
            return r.status, r.headers.get("content-type", ""), json.loads(r.read())

    keys = ["id", "user_id", "body", "likes"]
    status, ctype, feed = get("/feed?limit=20")
    if status != 200 or not ctype.startswith("application/json"):
        problems.append(f"/feed: {status} {ctype}")
    posts = feed.get("posts", [])
    if [p.get("id") for p in posts] != list(range(SEED["posts"], SEED["posts"] - 20, -1)):
        problems.append("/feed?limit=20: not the 20 latest posts")
    if any(list(p) != keys for p in posts):
        problems.append("/feed: post keys differ")
    _s, _c, post = get("/posts/1")
    if list(post) != keys or post["id"] != 1 or post["body"] != "seed post 1":
        problems.append(f"/posts/1: {post}")
    return problems


# ---------------------------------------------------------------------------------------------
# Load

def loadgen(art, base, mix, cpus, *, c=None, rate=None, conns=None, duration, timeout):
    cmd = [str(art["loadgen"]), "-url", base, "-mix", mix, "-d", f"{duration}s", "-timeout", f"{timeout}s",
           "-users", str(SEED["users"]), "-posts", str(SEED["posts"])]
    cmd += ["-c", str(c)] if c else ["-rate", str(rate), "-conns", str(conns)]
    r = subprocess.run(pinned(cmd, cpus), capture_output=True, text=True, timeout=duration + timeout + 120)
    if r.returncode != 0:
        raise bench.Fail(f"loadgen exited {r.returncode}: {r.stderr[-2000:]}")
    return json.loads(r.stdout)


def oha(base, cpus, c, duration):
    """`oha` on /health at `c` connections: a cross-check of loadgen's closed loop at the same point."""
    exe = shutil.which("oha")
    if not exe:
        return None
    cmd = [exe, "--no-tui", "-z", f"{duration}s", "-c", str(c), "--output-format", "json", base + "/health"]
    r = subprocess.run(pinned(cmd, cpus), capture_output=True, text=True, timeout=duration + 120)
    if r.returncode != 0:
        return {"error": r.stderr[-500:]}
    d = json.loads(r.stdout)
    lat = d.get("latencyPercentiles") or {}
    return {"connections": c, "duration_s": duration, "rps": d["summary"]["requestsPerSec"],
            "status_codes": {str(k): v for k, v in (d.get("statusCodeDistribution") or {}).items()},
            "errors": sum((d.get("errorDistribution") or {}).values()),
            "latency_ms": {"p50": (lat.get("p50") or 0) * 1000, "p99": (lat.get("p99") or 0) * 1000}}


def merge(dicts):
    out = {}
    for d in dicts:
        for k, v in d.items():
            out[k] = out.get(k, 0) + v
    return out


def point(art, srv, base, mix, a, *, c=None, rate=None):
    """One point, `--repeats` runs, each with the server's peak RSS, the load average and liveness."""
    runs = []
    for _ in range(a.repeats):
        load_before = os.getloadavg()[0]
        srv.take_peak()
        r = loadgen(art, base, mix, a.load_cpus, c=c, rate=rate, conns=a.open_conns, duration=a.duration,
                    timeout=a.timeout)
        r["rss_peak_mib"], r["rss_end_mib"] = srv.take_peak()
        r["load_1m"] = [round(load_before, 2), round(os.getloadavg()[0], 2)]
        r["server_alive"] = srv.alive()
        runs.append(r)
        if not r["server_alive"]:
            break
    med = lambda f: statistics.median(f(r) for r in runs)
    return {"mode": "closed" if c else "rate", "connections": c or a.open_conns, "rate": rate,
            "rps": med(lambda r: r["rps"]), "rps_min": min(r["rps"] for r in runs),
            "rps_max": max(r["rps"] for r in runs), "error_rate": med(lambda r: r["error_rate"]),
            "p50_ms": med(lambda r: r["latency_ms"]["p50"]), "p99_ms": med(lambda r: r["latency_ms"]["p99"]),
            "p99_min_ms": min(r["latency_ms"]["p99"] for r in runs),
            "p99_max_ms": max(r["latency_ms"]["p99"] for r in runs),
            "rss_peak_mib": max(r["rss_peak_mib"] for r in runs),
            "load_1m_max": max(max(r["load_1m"]) for r in runs),
            "status_codes": merge(r["status_codes"] for r in runs),
            "error_kinds": merge(r["error_kinds"] for r in runs),
            "server_alive": runs[-1]["server_alive"], "runs": runs}


def passes(p):
    return p["error_rate"] < MAX_ERROR_RATE and p["server_alive"]


def keeps_up(p):
    return passes(p) and p["rps"] >= KEEP_UP * p["rate"]


def run_cell(name, mix, art, spec, a, logs):
    """One server under one mix: a fresh process, seeded and checked, warmed up, then both ramps."""
    _runtime, command, env = spec
    port = free_port()
    base = f"http://127.0.0.1:{port}"
    log_path = logs / f"{name}-{mix}.log"
    srv = Server(pinned(command(port), a.server_cpus), env(port), log_path)
    cell = {"server": name, "mix": mix, "closed": [], "rate": [], "problems": []}
    try:
        if not wait_ready(base, srv):
            cell["problems"].append(f"not ready within {READY_TIMEOUT_S} s (exit {srv.proc.poll()})")
            return cell
        t0 = time.monotonic()
        seed.seed(base, **SEED)
        cell["seed_s"] = round(time.monotonic() - t0, 2)
        cell["problems"] += check_shapes(base)
        cell["rss_seeded_mib"] = srv.take_peak()[1]
        loadgen(art, base, mix, a.load_cpus, c=32, duration=a.warmup, timeout=a.timeout)
        for c in a.concurrency:
            p = point(art, srv, base, mix, a, c=c)
            cell["closed"].append(p)
            log(f"  {name} {mix} c={c}: {p['rps']:.0f} rps ({p['rps_min']:.0f}-{p['rps_max']:.0f}), "
                f"err {p['error_rate']:.4%}, p50 {p['p50_ms']:.2f} ms, p99 {p['p99_ms']:.2f} ms, "
                f"RSS {p['rss_peak_mib']} MiB, load {p['load_1m_max']}, kinds {p['error_kinds']}")
            if not p["server_alive"]:
                break
        best = max((p["rps"] for p in cell["closed"] if passes(p)), default=0)
        cell["best_closed_rps"] = best
        if mix == "health" and srv.alive():
            cell["oha"] = oha(base, a.load_cpus, 128, a.duration)
            loadgen_128 = next((p for p in cell["closed"] if p["connections"] == 128), None)
            if cell["oha"] and loadgen_128:
                log(f"  {name} oha c=128: {cell['oha'].get('rps', 0):.0f} rps; loadgen: {loadgen_128['rps']:.0f}")
        for f in a.rate_factors if best and srv.alive() else []:
            rate = max(1, round(best * f))
            p = point(art, srv, base, mix, a, rate=rate)
            p["factor"] = f
            cell["rate"].append(p)
            log(f"  {name} {mix} rate={rate} ({f}x): {p['rps']:.0f} rps, err {p['error_rate']:.4%}, "
                f"p50 {p['p50_ms']:.2f} ms, p99 {p['p99_ms']:.2f} ms, RSS {p['rss_peak_mib']} MiB, "
                f"kinds {p['error_kinds']}")
            if not p["server_alive"]:
                break
        cell.update(summarize(cell))
    except (bench.Fail, seed.SeedError, OSError, ValueError, http.client.HTTPException) as e:
        cell["problems"].append(f"{type(e).__name__}: {str(e)[:500]}")
    finally:
        cell["exit_code"] = srv.proc.poll()
        srv.stop()
        tail = log_path.read_bytes()[-2000:].decode(errors="replace")
        if tail.strip():
            cell["log_tail"] = tail
    return cell


def summarize(cell):
    """The knee (highest offered rate kept up with) and what the first failing point looked like."""
    up = [p for p in cell["rate"] if keeps_up(p)]
    knee = max(up, key=lambda p: p["rate"], default=None)
    first_bad = next((p for p in cell["rate"] if not keeps_up(p)), None)
    first_bad_closed = next((p for p in cell["closed"] if not passes(p)), None)
    return {"knee_rps": knee["rate"] if knee else None, "knee_p99_ms": knee["p99_ms"] if knee else None,
            "first_failing_rate": describe(first_bad) if first_bad else None,
            "first_failing_closed": describe(first_bad_closed) if first_bad_closed else None,
            "crashed": any(not p["server_alive"] for p in cell["closed"] + cell["rate"])}


def describe(p):
    return {"connections": p["connections"], "rate": p["rate"], "rps": p["rps"], "error_rate": p["error_rate"],
            "p99_ms": p["p99_ms"], "non_2xx": {k: v for k, v in p["status_codes"].items() if not k.startswith("2")},
            "error_kinds": p["error_kinds"], "server_alive": p["server_alive"], "rss_peak_mib": p["rss_peak_mib"]}


# ---------------------------------------------------------------------------------------------
# Report

def markdown(doc):
    m, f = doc["meta"], doc["meta"]["flags"]
    lines = [f"Host: {m['host']['cpu']}, {m['host']['os']}, {m['host']['mem_bytes'] / 2**30:.0f} GiB, "
             f"{m['logical_cpus']} logical CPUs; CI: {m['ci']}; commit {m['commit'][:10]}; pinning: server "
             f"{f['server_cpus'] or 'unpinned'}, load {f['load_cpus'] or 'unpinned'}.",
             f"Runtimes: {', '.join(f'{k} {v}' for k, v in m['runtimes'].items())}.",
             f"Each point: {f['repeats']} runs of {f['duration_s']} s. Cells: RPS median (min–max), error rate, "
             "p50/p99 ms, peak RSS MiB, highest 1-minute load average seen.", ""]
    for mix in f["mixes"]:
        cells = [c for c in doc["results"] if c["mix"] == mix]
        lines += [f"### Mix `{mix}`: closed loop", "",
                  "| Server | " + " | ".join(f"c={c}" for c in f["concurrency"]) + " |",
                  "|---|" + "---|" * len(f["concurrency"])]
        for cell in cells:
            row = [fmt_point(next((p for p in cell["closed"] if p["connections"] == c), None)) for c in f["concurrency"]]
            lines.append(f"| {cell['server']} | " + " | ".join(row) + " |")
        lines += ["", f"### Mix `{mix}`: open loop (1.0× = best closed-loop RPS), knee, failure", "",
                  "| Server | best closed RPS | knee RPS (p99 ms) | "
                  + " | ".join(f"{x}×" for x in f["rate_factors"]) + " | first point past the knee |",
                  "|---|---|---|" + "---|" * len(f["rate_factors"]) + "---|"]
        for cell in cells:
            row = []
            for x in f["rate_factors"]:
                p = next((p for p in cell["rate"] if p.get("factor") == x), None)
                row.append(f"{p['rate']}/s → {fmt_point(p)}" if p else "—")
            knee = f"{cell['knee_rps']} ({cell['knee_p99_ms']:.1f})" if cell.get("knee_rps") else "—"
            lines.append(f"| {cell['server']} | {cell.get('best_closed_rps', 0):.0f} | {knee} | "
                         + " | ".join(row) + f" | {failure_text(cell)} |")
        lines.append("")
    oha_rows = [c for c in doc["results"] if c.get("oha") and "rps" in c["oha"]]
    if oha_rows:
        lines += ["### Load generator cross-check: `/health` at 128 connections", "",
                  "| Server | loadgen RPS (median) | oha RPS (one run) |", "|---|---|---|"]
        for c in oha_rows:
            lg = next((p for p in c["closed"] if p["connections"] == 128), None)
            lines.append(f"| {c['server']} | {lg['rps']:.0f} | {c['oha']['rps']:.0f} |" if lg else "")
        lines.append("")
    return "\n".join(lines)


def summary(doc):
    """One compact table per mix: the numbers bench/http/README.md quotes."""
    lines = []
    for mix in doc["meta"]["flags"]["mixes"]:
        lines += [f"Mix `{mix}`", "",
                  "| Server | Max sustained RPS (connections) | p50 / p99 ms there | Knee RPS (p99 ms) "
                  "| Past the knee: offered → done, p99, errors | Peak RSS MiB | Max load |",
                  "|---|---|---|---|---|---|---|"]
        for cell in (c for c in doc["results"] if c["mix"] == mix):
            if cell.get("problems") and not cell["closed"]:
                lines.append(f"| {cell['server']} | {'; '.join(cell['problems'])} | | | | | |")
                continue
            ok = [p for p in cell["closed"] if passes(p)]
            best = max(ok, key=lambda p: p["rps"], default=None)
            points = cell["closed"] + cell["rate"]
            knee = f"{cell['knee_rps']:,} ({cell['knee_p99_ms']:.1f})" if cell.get("knee_rps") else "none"
            fb = cell.get("first_failing_rate")
            if fb:
                kinds = dict(fb["non_2xx"], **fb["error_kinds"])
                past = (f"{fb['rate']:,} → {fb['rps']:,.0f}/s, {fb['p99_ms']:,.0f} ms, "
                        + (", ".join(f"{k} ×{v:,}" for k, v in sorted(kinds.items())) or "none")
                        + ("" if fb["server_alive"] else ", **server died**"))
            else:
                past = "kept up at every rate"
            lines.append(
                f"| {cell['server']} | {best['rps']:,.0f} ({best['rps_min']:,.0f}–{best['rps_max']:,.0f}) at {best['connections']} "
                f"| {best['p50_ms']:.1f} / {best['p99_ms']:.1f} | {knee} | {past} "
                f"| {max(p['rss_peak_mib'] for p in points):.0f} | {max(p['load_1m_max'] for p in points):.1f} |"
                if best else f"| {cell['server']} | none under {MAX_ERROR_RATE:.1%} errors | | {knee} | {past} | | |")
        lines.append("")
    return "\n".join(lines)


def fmt_point(p):
    if not p:
        return "—"
    alive = "" if p["server_alive"] else " **died**"
    return (f"{p['rps']:.0f} ({p['rps_min']:.0f}–{p['rps_max']:.0f}), {p['error_rate']:.2%}, "
            f"{p['p50_ms']:.1f}/{p['p99_ms']:.1f}, {p['rss_peak_mib']:.0f}, L{p['load_1m_max']:.1f}{alive}")


def failure_text(cell):
    if cell.get("problems"):
        return "; ".join(cell["problems"])
    fb = cell.get("first_failing_rate")
    if not fb:
        return "kept up at every rate"
    what = []
    if fb["non_2xx"]:
        what.append("status " + ", ".join(f"{k}×{v}" for k, v in sorted(fb["non_2xx"].items())))
    if fb["error_kinds"]:
        what.append(", ".join(f"{k}×{v}" for k, v in sorted(fb["error_kinds"].items())))
    state = "server kept running" if fb["server_alive"] else "**server died**"
    return (f"{fb['rate']}/s: {fb['rps']:.0f}/s done, p99 {fb['p99_ms']:.0f} ms, "
            f"{'; '.join(what) or 'slow, no errors'}; {state}")


# ---------------------------------------------------------------------------------------------

def default_pinning():
    """On Linux with taskset and ≥ 4 CPUs: the server on the first half, the load on the second."""
    if sys.platform != "linux" or not shutil.which("taskset"):
        return "", ""
    cpus = sorted(os.sched_getaffinity(0))
    if len(cpus) < 4:
        return "", ""
    half = len(cpus) // 2
    return ",".join(map(str, cpus[:half])), ",".join(map(str, cpus[half:]))


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--servers", default=",".join(SERVER_NAMES))
    ap.add_argument("--mixes", default="read,write,health")
    ap.add_argument("--concurrency", default="1,8,32,128,512,1024")
    ap.add_argument("--rate-factors", default="0.5,0.75,0.9,1.0,1.1,1.25,1.5,2.0")
    ap.add_argument("--open-conns", type=int, default=256, help="open loop: maximum connections")
    ap.add_argument("--duration", type=int, default=5, help="seconds per run")
    ap.add_argument("--warmup", type=int, default=5, help="seconds of warm-up per server and mix")
    ap.add_argument("--repeats", type=int, default=3)
    ap.add_argument("--timeout", type=int, default=5, help="per-request timeout, seconds")
    ap.add_argument("--server-cpus", default=None, help="taskset CPU list for the server (Linux; default first half)")
    ap.add_argument("--load-cpus", default=None, help="taskset CPU list for the load generator (Linux; second half)")
    ap.add_argument("--sakura", default=os.environ.get("SAKURA") or shutil.which("sakura"))
    ap.add_argument("--json")
    ap.add_argument("--md")
    ap.add_argument("--report", metavar="JSON", help="print the summary tables of a result file and exit")
    a = ap.parse_args()
    if a.report:
        print(summary(json.loads(Path(a.report).read_text())))
        return
    a.concurrency = [int(x) for x in a.concurrency.split(",")]
    a.rate_factors = [float(x) for x in a.rate_factors.split(",")]
    names = [n for n in a.servers.split(",") if n]
    mixes = [m for m in a.mixes.split(",") if m]
    if unknown := set(names) - set(SERVER_NAMES):
        sys.exit(f"unknown servers: {', '.join(sorted(unknown))}")
    dsc, dlc = default_pinning()
    a.server_cpus = dsc if a.server_cpus is None else a.server_cpus
    a.load_cpus = dlc if a.load_cpus is None else a.load_cpus

    soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
    want = 65536 if hard == resource.RLIM_INFINITY else min(hard, 65536)
    if soft != resource.RLIM_INFINITY and soft < want:
        resource.setrlimit(resource.RLIMIT_NOFILE, (want, hard))
    signal.signal(signal.SIGTERM, stop_all)
    signal.signal(signal.SIGINT, stop_all)

    logs = TARGET / "logs"
    logs.mkdir(parents=True, exist_ok=True)
    if not a.sakura:
        names = [n for n in names if not n.startswith("sakura")]
        log("no sakura CLI: Sakura servers skipped")
    art, failed = build(a.sakura, names)
    if "loadgen" in failed:
        sys.exit(failed["loadgen"])
    specs = servers(art)

    runtimes = {"sakura": subprocess.run([a.sakura, "--version"], capture_output=True, text=True).stdout.strip()
                if a.sakura else None,
                "node": bench.version(["node", "--version"]), "bun": bench.version(["bun", "--version"]),
                "go": bench.version(["go", "version"]),
                "oha": bench.version(["oha", "--version"]) if shutil.which("oha") else None}
    meta = {"commit": bench.git("rev-parse", "HEAD"),
            "dirty": bool(bench.git("status", "--porcelain", "--", "bench/http")),
            "host": bench.host(), "logical_cpus": os.cpu_count(), "ci": bool(os.environ.get("CI")),
            "platform": platform.platform(),
            "started": datetime.datetime.now(datetime.UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "runtimes": {k: v for k, v in runtimes.items() if v},
            "flags": {"servers": names, "mixes": mixes, "concurrency": a.concurrency, "rate_factors": a.rate_factors,
                      "open_conns": a.open_conns, "duration_s": a.duration, "warmup_s": a.warmup,
                      "repeats": a.repeats, "timeout_s": a.timeout, "server_cpus": a.server_cpus,
                      "load_cpus": a.load_cpus, "seed": SEED, "max_error_rate": MAX_ERROR_RATE, "keep_up": KEEP_UP,
                      "fd_limit": resource.getrlimit(resource.RLIMIT_NOFILE)[0]},
            "build_failures": failed}
    if sys.platform == "linux" and shutil.which("lscpu"):
        meta["lscpu"] = subprocess.run(["lscpu", "-e"], capture_output=True, text=True).stdout
    doc = {"schema": SCHEMA, "meta": meta, "results": []}

    def save():
        if a.json:
            Path(a.json).write_text(json.dumps(doc, indent=1) + "\n")
        if a.md:
            Path(a.md).write_text(markdown(doc) + "\n")

    for mix in mixes:
        for name in names:
            need = NEEDS.get(name)
            if need and not art.get(need):
                doc["results"].append({"server": name, "mix": mix, "closed": [], "rate": [],
                                       "problems": [f"build failed: {failed.get(need, 'not built')[:300]}"]})
                continue
            log(f"[{name} / {mix}] load {os.getloadavg()[0]:.2f}")
            doc["results"].append(run_cell(name, mix, art, specs[name], a, logs))
            save()
    meta["finished"] = datetime.datetime.now(datetime.UTC).strftime("%Y-%m-%dT%H:%M:%SZ")
    save()
    print(markdown(doc))


if __name__ == "__main__":
    main()
