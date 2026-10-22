#!/usr/bin/env python3
"""TechEmpower-style web tests (bench/web/README.md): JSON serialization and plaintext against
Sakura's std `Serve` and reference servers. Python 3.11+ standard library only.

Builds each server, starts it on a free 127.0.0.1 port, checks both endpoints against the
TechEmpower requirements (a failing server is recorded and not timed), then drives each endpoint
with the pinned load tool (`oha`) for a warmup and a measured duration at fixed connection counts.
Writes `sakura-web-v1` JSON with `--json` and prints a Markdown table on stdout.

Exit: 0 ok; 1 a build, start or check failed (that server is not timed); 3 a toolchain or the load
tool is missing (its servers are skipped and listed in `meta.missing`)."""

import argparse
import datetime
import http.client
import json
import os
import random
import re
import resource
import shutil
import signal
import socket
import subprocess
import sys
import time
import tomllib
from email.utils import parsedate_to_datetime
from pathlib import Path

WEB = Path(__file__).resolve().parent
BENCH = WEB.parent
ROOT = BENCH.parent
TARGET = BENCH / "target" / "web"
SCHEMA = "sakura-web-v1"
sys.path.insert(0, str(BENCH / "harness"))
import bench  # noqa: E402  the suite's host id, git and version helpers

EXIT_OK, EXIT_FAILED, EXIT_TOOLCHAIN = 0, 1, 3
READY_TIMEOUT_S = 60  # `sakura` and `cargo` builds happen before the start, not inside it
BODY = {"json": b'{"message":"Hello, World!"}', "plaintext": b"Hello, World!"}
# TechEmpower's verifications.py content-type patterns and wrk Accept headers.
CONTENT_TYPE = {"json": re.compile(r"^application/json(; ?charset=(UTF|utf)-8)?$"),
                "plaintext": re.compile(r"^text/plain(; ?charset=(UTF|utf)-8)?$")}
ACCEPT = {"json": "application/json,text/html;q=0.9,application/xhtml+xml;q=0.9,application/xml;q=0.8,*/*;q=0.7",
          "plaintext": "text/plain,text/html;q=0.9,application/xhtml+xml;q=0.9,application/xml;q=0.8,*/*;q=0.7"}
# Subsets of TechEmpower's levels (JSON 16..512; plaintext 256..16,384, pipelined 16 deep).
CONNECTIONS = {"json": [16, 256], "plaintext": [256, 1024]}


def log(*a):
    print(*a, file=sys.stderr, flush=True)


class Fail(Exception):
    """A server could not be built, started or checked: recorded, and that server is not timed."""


# ---------------------------------------------------------------------------------------------
# Servers

def sakura_cli(arg):
    """`--sakura`, else $SAKURA, else the workspace release build."""
    for c in (arg, os.environ.get("SAKURA"), bench.SAKURA):
        if c and Path(c).is_file():
            return Path(c).resolve()
    return None


def grant(port):
    """The `SAKURA_PERMISSIONS` table `sakura run` passes for a manifest whose `net` is this port
    (bench/web/sakura/sakura.toml names 8080 for a run by hand)."""
    return {"SAKURA_PERMISSIONS": json.dumps({"fs": {"read": [], "write": []}, "net": [f"127.0.0.1:{port}"],
                                              "env": [], "process": []})}


def build_sakura_js(cli):
    out = TARGET / "sakura" / "js"
    bench.sh([str(cli), "build", "--out-dir", str(out), str(WEB / "sakura")])
    return out / "__main__.js"


def build_sakura_native(cli):
    out = TARGET / "sakura" / "native"
    bench.sh([str(cli), "build", "--target", "native", "--out-dir", str(out), str(WEB / "sakura")])
    return out / "main"


def build_go():
    out = TARGET / "go" / "main"
    bench.sh(["go", "build", "-o", str(out), "."], cwd=WEB / "go")
    return out


def build_rust():
    target = TARGET / "rust"
    bench.sh(["cargo", "build", "--release", "--locked", "--manifest-path", str(WEB / "rust" / "Cargo.toml"),
              "--target-dir", str(target)], timeout=1800)
    return target / "release" / "main"


# name → (lane, runtime, tools it needs, build(ctx) → artefact, command(artefact, port), env(port)).
# `single`: one thread serving requests (Sakura's runtime is single-threaded per process);
# `multi`: Go's and tokio's defaults, one worker per CPU, recorded beside it.
SERVERS = {
    "sakura-node": ("single", "node", ["sakura", "node"], lambda c: c["sakura_js"](),
                    lambda a, p: ["node", str(a), str(p)], grant),
    "sakura-bun": ("single", "bun", ["sakura", "bun"], lambda c: c["sakura_js"](),
                   lambda a, p: ["bun", str(a), str(p)], grant),
    "sakura-native": ("single", "native", ["sakura", "cc", "sakura-native-lib"], lambda c: c["sakura_native"](),
                      lambda a, p: [str(a), str(p)], grant),
    "bun": ("single", "bun", ["bun"], lambda c: WEB / "bun" / "server.mjs",
            lambda a, p: ["bun", str(a), str(p)], lambda p: {}),
    "node": ("single", "node", ["node"], lambda c: WEB / "node" / "server.mjs",
             lambda a, p: ["node", str(a), str(p)], lambda p: {}),
    "go": ("single", "go", ["go"], lambda c: c["go"](),
           lambda a, p: [str(a), str(p)], lambda p: {"GOMAXPROCS": "1"}),
    "rust": ("single", "rust", ["cargo"], lambda c: c["rust"](),
             lambda a, p: [str(a), str(p), "single"], lambda p: {}),
    "go-mt": ("multi", "go", ["go"], lambda c: c["go"](),
              lambda a, p: [str(a), str(p)], lambda p: {}),
    "rust-mt": ("multi", "rust", ["cargo"], lambda c: c["rust"](),
                lambda a, p: [str(a), str(p), "multi"], lambda p: {}),
}


def free_port(attempts=200):
    """A free 4-digit port (TechEmpower's general requirement 8), tried at random from 2000-9999."""
    for port in random.sample(range(2000, 10000), attempts):
        with socket.socket() as s:
            try:
                s.bind(("127.0.0.1", port))
            except OSError:
                continue
            return port
    raise Fail(f"no free 4-digit port on 127.0.0.1 after {attempts} tries")


class Server:
    """A started server in its own session, so stopping it stops every process it spawned."""

    def __init__(self, name, cmd, env, logfile):
        self.name, self.cmd = name, cmd
        self.logfile = logfile
        env = {k: v for k, v in os.environ.items() if k != "GOMAXPROCS"} | env
        with open(logfile, "wb") as out:
            self.proc = subprocess.Popen(cmd, env=env, stdin=subprocess.DEVNULL, stdout=out, stderr=out,
                                         start_new_session=True)

    def stop(self):
        if self.proc.poll() is None:
            os.killpg(self.proc.pid, signal.SIGTERM)
            try:
                self.proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(self.proc.pid, signal.SIGKILL)
                self.proc.wait()

    def log_tail(self):
        return self.logfile.read_text(errors="replace")[-2000:]


def wait_ready(server, port, timeout=READY_TIMEOUT_S):
    """Polls `GET /plaintext` until any HTTP answer; a server that exits first is a Fail."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if server.proc.poll() is not None:
            raise Fail(f"exited {server.proc.returncode} before answering:\n{server.log_tail()}")
        try:
            fetch(port, "/plaintext", timeout=2)
            return
        except OSError:
            time.sleep(0.1)
    raise Fail(f"no answer on 127.0.0.1:{port} within {timeout} s:\n{server.log_tail()}")


# ---------------------------------------------------------------------------------------------
# The TechEmpower checks (toolset/test_types/verifications.py, at FrameworkBenchmarks 57d92fbe)

def fetch(port, path, timeout=10):
    """(status, headers with lowercased names, body) of one keep-alive GET."""
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    try:
        conn.request("GET", path, headers={"Accept": ACCEPT.get(path.strip("/"), "*/*"), "Connection": "keep-alive"})
        r = conn.getresponse()
        headers = {}
        for k, v in r.getheaders():
            headers.setdefault(k.lower(), v)
        return r.status, headers, r.read()
    finally:
        conn.close()


def check_response(endpoint, status, headers, body):
    """TechEmpower's per-response requirements as (level, message) problems: `fail` means the
    server does not meet the test type, `warn` that it meets it with a TechEmpower warning.
    `headers` has lowercased names. Stricter than TechEmpower where noted: the body must be the
    exact bytes (TechEmpower compares case-insensitively and allows whitespace)."""
    problems = []
    if status != 200:
        problems.append(("fail", f"status {status}, not 200"))
    if endpoint == "json":
        try:
            value = json.loads(body)
        except ValueError as e:
            value = None
            problems.append(("fail", f"invalid JSON: {e}"))
        if value is not None and value != {"message": "Hello, World!"}:
            problems.append(("fail", f"JSON body {body[:80]!r} is not {{\"message\":\"Hello, World!\"}}"))
        elif value is not None and body != BODY["json"]:
            problems.append(("warn", f"JSON body {body[:80]!r} has extra whitespace"))
    elif body != BODY["plaintext"]:
        problems.append(("fail", f"body {body[:80]!r} is not b'Hello, World!'"))
    for name in ("server", "date", "content-type"):
        if name not in headers:
            problems.append(("fail", f"required header missing: {name}"))
    if "content-length" not in headers and "transfer-encoding" not in headers:
        problems.append(("fail", "neither content-length nor transfer-encoding"))
    elif "content-length" in headers and headers["content-length"].strip() != str(len(body)):
        problems.append(("fail", f"content-length {headers['content-length']} for a {len(body)}-byte body"))
    if "content-type" in headers and not CONTENT_TYPE[endpoint].match(headers["content-type"]):
        problems.append(("fail", f"content-type {headers['content-type']!r} does not match "
                                 f"{CONTENT_TYPE[endpoint].pattern}"))
    if "date" in headers and parse_date(headers["date"]) is None:
        problems.append(("warn", f"date {headers['date']!r} is not an RFC 9110 IMF-fixdate"))
    if headers.get("content-encoding", "identity") != "identity":
        problems.append(("fail", f"content-encoding {headers['content-encoding']} (compression is not permitted)"))
    if headers.get("connection", "").lower() == "close":
        problems.append(("warn", "connection: close (keep-alive is strongly encouraged)"))
    return problems


def parse_date(text):
    """An IMF-fixdate (`Thu, 22 Oct 2026 17:12:19 GMT`) as a datetime, else None."""
    if not re.fullmatch(r"[A-Z][a-z]{2}, \d{2} [A-Z][a-z]{2} \d{4} \d{2}:\d{2}:\d{2} GMT", text.strip()):
        return None
    try:
        return parsedate_to_datetime(text)
    except (TypeError, ValueError):
        return None


def check_date_changes(first, second):
    """TechEmpower fails a server whose `Date` is the same on two requests seconds apart."""
    if first is not None and first == second:
        return [("fail", f"date {first!r} did not change between requests 1.1 s apart (cached)")]
    return []


PIPELINE_DEPTH = 16  # TechEmpower's plaintext depth (toolset/test_types/plaintext/plaintext.py)


def pipelined_answers(port, depth=PIPELINE_DEPTH, timeout=5):
    """How many `200` answers `depth` pipelined `GET /plaintext`s, written at once on one
    connection, get back within `timeout` seconds."""
    request = b"GET /plaintext HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept: text/plain\r\n\r\n"
    data = b""
    with socket.create_connection(("127.0.0.1", port), timeout=timeout) as s:
        s.sendall(request * depth)
        deadline = time.monotonic() + timeout
        while data.count(b"HTTP/1.1 200") < depth and time.monotonic() < deadline:
            try:
                chunk = s.recv(65536)
            except TimeoutError:
                break
            if not chunk:
                break
            data += chunk
    return data.count(b"HTTP/1.1 200")


def check_pipelining(answers, depth=PIPELINE_DEPTH):
    """TechEmpower assumes pipelining for plaintext and asks a server without it to degrade
    gracefully; the load tool does not pipeline (README), so this is recorded, not timed."""
    if answers < depth:
        return [("warn", f"{answers} of {depth} pipelined requests answered")]
    return []


def check_server(port):
    """{endpoint: problems} for both endpoints, the Date re-read after 1.1 s (TechEmpower waits 3),
    and plaintext's pipelining."""
    out, dates = {}, {}
    for endpoint in BODY:
        status, headers, body = fetch(port, f"/{endpoint}")
        out[endpoint] = check_response(endpoint, status, headers, body)
        dates[endpoint] = headers.get("date")
    time.sleep(1.1)
    for endpoint in BODY:
        out[endpoint] += check_date_changes(dates[endpoint], fetch(port, f"/{endpoint}")[1].get("date"))
    out["plaintext"] += check_pipelining(pipelined_answers(port))
    return out


# ---------------------------------------------------------------------------------------------
# Load

def oha_command(oha, port, endpoint, connections, seconds, threads=None, output="json"):
    cmd = [str(oha), "-z", f"{seconds}s", "-c", str(connections), "-w", "--no-tui", "--http-version", "1.1",
           "--output-format", output, "-H", f"Accept: {ACCEPT[endpoint]}", "-H", "Connection: keep-alive"]
    if threads:
        cmd += ["--worker-threads", str(threads)]
    return cmd + [f"http://127.0.0.1:{port}/{endpoint}"]


def parse_oha(doc, endpoint):
    """One cell from `oha --output-format json`: requests/s counts only 200 answers with the
    expected size; every other answer and every transport error is an error."""
    s = doc["summary"]
    codes = {str(k): int(v) for k, v in (doc.get("statusCodeDistribution") or {}).items()}
    errors = {str(k): int(v) for k, v in (doc.get("errorDistribution") or {}).items()}
    ok = codes.get("200", 0)
    total_s = float(s["total"])
    pct = doc.get("latencyPercentiles") or {}
    size = s.get("sizePerRequest")
    return {"requests_per_s": ok / total_s if total_s > 0 else 0.0,
            "requests": ok,
            "errors": sum(errors.values()) + sum(n for c, n in codes.items() if c != "200"),
            "latency_ms": {k: None if pct.get(k) is None else float(pct[k]) * 1000 for k in ("p50", "p99")},
            "duration_s": total_s,
            "status_codes": codes,
            "error_kinds": errors,
            "bytes_per_response": size,
            "size_ok": size is None or int(size) == len(BODY[endpoint]),
            "oha_success_rate": s.get("successRate")}


def drive(oha, port, endpoint, connections, seconds, threads):
    r = subprocess.run(oha_command(oha, port, endpoint, connections, seconds, threads), capture_output=True,
                       text=True, timeout=seconds + 120)
    if r.returncode != 0:
        raise Fail(f"oha exited {r.returncode}: {r.stderr[-1500:]}")
    return parse_oha(json.loads(r.stdout), endpoint)


def warm(oha, port, endpoint, connections, seconds, threads):
    subprocess.run(oha_command(oha, port, endpoint, connections, seconds, threads, output="quiet"),
                   capture_output=True, timeout=seconds + 120)


# ---------------------------------------------------------------------------------------------
# Run

def tool_versions(cli):
    versions = {k: bench.version(cmd) for k, cmd in {"node": ["node", "--version"], "bun": ["bun", "--version"],
                                                     "go": ["go", "version"], "rustc": ["rustc", "--version"],
                                                     "cc": ["cc", "--version"]}.items() if shutil.which(cmd[0])}
    if cli:
        versions["sakura"] = subprocess.run([str(cli), "--version"], capture_output=True, text=True).stdout.strip()
    return versions


def pins():
    return tomllib.loads((BENCH / "toolchains.toml").read_text())


def find_oha(arg):
    for c in (arg, os.environ.get("OHA"), TARGET / "tools" / "bin" / "oha", shutil.which("oha")):
        if c and Path(c).is_file():
            return Path(c).resolve()
    return None


def raise_fd_limit():
    """Every connection is a descriptor in oha and in the server: lift the soft limit to the hard one."""
    soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
    want = hard if hard != resource.RLIM_INFINITY else 65536
    if sys.platform == "darwin":
        want = min(want, 10240)  # OPEN_MAX: setrlimit refuses more on macOS
    if want > soft:
        resource.setrlimit(resource.RLIMIT_NOFILE, (want, hard))
    return resource.getrlimit(resource.RLIMIT_NOFILE)[0]


def needs_met(needs, cli, missing):
    tools = {"sakura": lambda: cli is not None,
             "sakura-native-lib": lambda: bool(os.environ.get("SAKURA_NATIVE_LIB_DIR"))}
    lacking = [n for n in needs if not (tools[n]() if n in tools else shutil.which(n))]
    for n in lacking:
        if n not in missing:
            missing.append(n)
    return lacking


def table(results, absent, failures):
    rows = ["| server | lane | endpoint | connections | requests/s | p50 ms | p99 ms | errors |",
            "|---|---|---|---:|---:|---:|---:|---:|"]
    fmt = lambda v, d: "–" if v is None else f"{v:,.{d}f}"
    for r in results:
        rows.append(f"| {r['server']} | {r['lane']} | {r['endpoint']} | {r['connections']} | "
                    f"{fmt(r['requests_per_s'], 0)} | {fmt(r['latency_ms']['p50'], 3)} | "
                    f"{fmt(r['latency_ms']['p99'], 3)} | {r['errors']} |")
    for a in absent:
        rows.append(f"| {a['server']} | {a.get('lane', '')} | – | – | absent: {a['reason'].splitlines()[0][:120]} | | | |")
    for f in failures:
        rows.append(f"| {f['server']} | {f.get('lane', '')} | {f.get('endpoint', '–')} | – | failed: "
                    f"{'; '.join(m for _l, m in f['problems'])[:160]} | | | |")
    return "\n".join(rows)


def parse_levels(text):
    levels = [int(x) for x in text.split(",") if x.strip()]
    if not levels or any(n <= 0 for n in levels):
        raise argparse.ArgumentTypeError(f"connection counts are positive integers, not {text!r}")
    return levels


def main(argv=None):
    ap = argparse.ArgumentParser(prog="web.py", description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--json", help="write the sakura-web-v1 result here")
    ap.add_argument("--servers", default=",".join(SERVERS), help="comma-separated subset of: " + ", ".join(SERVERS))
    ap.add_argument("--duration", type=int, default=15, help="measured seconds per cell (TechEmpower: 15)")
    ap.add_argument("--warmup", type=int, default=5, help="seconds of load per endpoint before its cells")
    ap.add_argument("--json-connections", type=parse_levels, default=CONNECTIONS["json"])
    ap.add_argument("--plaintext-connections", type=parse_levels, default=CONNECTIONS["plaintext"])
    ap.add_argument("--tool-threads", type=int, help="oha --worker-threads (default: oha's, one per physical core)")
    ap.add_argument("--sakura", help="the Sakura CLI (default: $SAKURA, compiler/target/release/sakura)")
    ap.add_argument("--oha", help="the load tool (default: $OHA, bench/target/web/tools/bin/oha, PATH)")
    ap.add_argument("--build-cli", action="store_true",
                    help="build the CLI with `--target native` and the native driver first, as `bench run --suite "
                         "native` does (CI; rebuilds compiler/target/release/sakura)")
    ap.add_argument("--check-only", action="store_true", help="build, start and check every server; time nothing")
    a = ap.parse_args(argv)
    names = [n.strip() for n in a.servers.split(",") if n.strip()]
    unknown = [n for n in names if n not in SERVERS]
    if unknown:
        ap.error(f"unknown server(s): {', '.join(unknown)}")
    levels = {"json": a.json_connections, "plaintext": a.plaintext_connections}

    TARGET.mkdir(parents=True, exist_ok=True)
    (TARGET / "logs").mkdir(exist_ok=True)
    if a.build_cli:
        if shutil.which("cc"):
            os.environ["SAKURA_NATIVE_LIB_DIR"] = str(bench.build_native_cli())
        else:
            bench.build_cli()
        a.sakura = a.sakura or str(bench.SAKURA)
    elif not os.environ.get("SAKURA_NATIVE_LIB_DIR"):
        lib = BENCH / "target" / "native-driver" / "release"
        if (lib / "libsakura_native_driver.a").exists():
            os.environ["SAKURA_NATIVE_LIB_DIR"] = str(lib)
    cli, oha = sakura_cli(a.sakura), find_oha(a.oha)
    fds = raise_fd_limit()
    pinned = pins().get("oha")
    oha_version = bench.version([str(oha), "--version"]) if oha else None
    missing = [] if oha or a.check_only else ["oha"]
    meta = {"commit": bench.git("rev-parse", "HEAD"),
            "dirty": bool(bench.git("status", "--porcelain", "--", "compiler", "bench")),
            "host": bench.host(), "ci": bool(os.environ.get("CI")),
            "started": datetime.datetime.now(datetime.UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "tool": {"name": "oha", "version": oha_version, "pinned": pinned, "path": str(oha) if oha else None,
                     "worker_threads": a.tool_threads, "pipelining": 1, "http": "1.1", "keep_alive": True},
            "runtimes": tool_versions(cli),
            "flags": {"duration_s": a.duration, "warmup_s": a.warmup, "connections": levels, "servers": names,
                      "check_only": a.check_only, "fd_limit": fds},
            "load_1m": {"start": os.getloadavg()[0]},
            "missing": missing, "absent": [], "failures": [], "checks": {}}
    if oha and pinned and oha_version != pinned:
        log(f"[note] oha {oha_version} is not the pinned {pinned}: recorded in meta.tool")
    for tool in missing:
        log(f"[missing] {tool}: nothing is timed")
    results, built = [], {}
    builders = {"sakura_js": lambda: build_sakura_js(cli), "sakura_native": lambda: build_sakura_native(cli),
                "go": build_go, "rust": build_rust}
    ctx = {k: (lambda k=k: built[k] if k in built else built.setdefault(k, builders[k]())) for k in builders}
    for name in names:
        lane, runtime, needs, build, command, env = SERVERS[name]
        lacking = needs_met(needs, cli, meta["missing"])
        if lacking:
            log(f"[missing] {name}: needs {', '.join(lacking)}")
            continue
        try:
            log(f"[build] {name}")
            artefact = build(ctx)
        except bench.Fail as e:
            meta["absent" if name == "sakura-native" else "failures"].append(
                {"server": name, "lane": lane, "reason": str(e), "problems": [("fail", f"build: {str(e)[:500]}")]})
            log(f"[{'absent' if name == 'sakura-native' else 'failed'}] {name}: build failed\n{e}")
            continue
        server = None
        try:
            port = free_port()
            server = Server(name, command(artefact, port), env(port), TARGET / "logs" / f"{name}.log")
            wait_ready(server, port)
            checks = check_server(port)
            meta["checks"][name] = checks
            for endpoint, problems in checks.items():
                for level, msg in problems:
                    log(f"[{level}] {name} /{endpoint}: {msg}")
            bad = {e: p for e, p in checks.items() if any(level == "fail" for level, _m in p)}
            for endpoint, problems in bad.items():
                meta["failures"].append({"server": name, "lane": lane, "endpoint": endpoint, "problems": problems})
            if oha is None or a.check_only:
                continue
            for endpoint in BODY:
                if endpoint in bad:
                    continue
                warnings = [m for level, m in checks[endpoint] if level == "warn"]
                log(f"[warmup] {name} /{endpoint} {a.warmup} s at {max(levels[endpoint])} connections")
                warm(oha, port, endpoint, max(levels[endpoint]), a.warmup, a.tool_threads)
                for c in levels[endpoint]:
                    time.sleep(1)
                    log(f"[load] {name} /{endpoint} {a.duration} s at {c} connections")
                    cell = drive(oha, port, endpoint, c, a.duration, a.tool_threads)
                    if server.proc.poll() is not None:
                        raise Fail(f"exited {server.proc.returncode} under load:\n{server.log_tail()}")
                    results.append({"server": name, "lane": lane, "runtime": runtime, "endpoint": endpoint,
                                    "connections": c, "warnings": warnings, **cell})
        except (Fail, OSError, subprocess.TimeoutExpired, json.JSONDecodeError) as e:
            meta["failures"].append({"server": name, "lane": lane, "problems": [("fail", str(e)[:2000])]})
            log(f"[failed] {name}: {e}")
        finally:
            if server is not None:
                server.stop()
    meta["load_1m"]["end"] = os.getloadavg()[0]
    doc = {"schema": SCHEMA, "meta": meta, "results": results}
    if a.json:
        Path(a.json).write_text(json.dumps(doc, indent=1) + "\n")
    print(table(results, meta["absent"], meta["failures"]))
    if meta["missing"]:
        print(f"\nMissing (skipped): {', '.join(meta['missing'])}")
    return EXIT_FAILED if meta["failures"] else EXIT_TOOLCHAIN if meta["missing"] else EXIT_OK


if __name__ == "__main__":
    sys.exit(main())
