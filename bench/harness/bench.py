#!/usr/bin/env python3
"""The `bench` harness (docs/plans/performance-r1.md §3). Python 3.11+ standard library only;
the JS panel is timed in-process by harness/runner.mjs. Run through the `bench/bench` shim."""

import argparse
import fnmatch
import json
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

BENCH = Path(__file__).resolve().parent.parent
ROOT = BENCH.parent
PROGRAMS = BENCH / "programs"
TARGET = BENCH / "target"
RUNNER = BENCH / "harness" / "runner.mjs"
SAKURA = ROOT / "compiler" / "target" / "release" / "sakura"
SMALL = [0, 1, 2, 3]


class Fail(Exception):
    """A build or oracle failure for one program; the harness reports it and moves on."""


def sh(cmd, cwd=None, timeout=600):
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)
    if r.returncode != 0:
        raise Fail(f"`{' '.join(map(str, cmd))}` exited {r.returncode}:\n{(r.stdout + r.stderr)[-3000:]}")
    return r.stdout


def programs(pattern="*"):
    return [p for p in sorted(d.name for d in PROGRAMS.iterdir() if (d / "meta.toml").exists())
            if fnmatch.fnmatch(p, pattern)]


def meta(p):
    return tomllib.loads((PROGRAMS / p / "meta.toml").read_text())


def build_cli():
    sh(["cargo", "build", "--release", "--locked", "-p", "sakura_cli"], cwd=ROOT / "compiler", timeout=1800)


def build_sakura(p):
    """Compile programs/<p>/sakura/main.sakura and return an entry module for runner.mjs."""
    out = TARGET / "sakura" / p
    sh([str(SAKURA), "build", "--out-dir", str(out), str(PROGRAMS / p / "sakura" / "main.sakura")])
    # ponytail: rewrites the emitted __main__.js (std imports + registerOps, then runMain) into
    # an importable entry; replace with a `sakura build --library` flag if the CLI grows one.
    lines = (out / "__main__.js").read_text().splitlines()
    kept = [l for l in lines if not l.startswith("runMain(") and "$entry" not in l]
    kept = [l.replace("import { registerOps, runMain }", "import { registerOps }") for l in kept]
    kept += ['export * as program from "./main/main.js";', 'export { runTask } from "./sakura_runtime.js";']
    entry = out / "bench_entry.mjs"
    entry.write_text("\n".join(kept) + "\n")
    return entry


def js_module(p):
    return PROGRAMS / p / "js" / "main.mjs"


def node(engine, *args, timeout=600):
    out = sh([engine, str(RUNNER), *map(str, args)], timeout=timeout)
    return json.loads(out.strip().splitlines()[-1])


def oracle_inputs(m):
    return SMALL + list(m["sizes"]) + list(m.get("check_sizes", []))


def oracle(p, module, engine, m):
    """Raise Fail unless the module reproduces every checksum (and defect) in meta.toml."""
    want = m.get("checksums", {})
    got = node(engine, "values", module, *oracle_inputs(m))["values"]
    bad = [f"run({n}) = {got.get(str(n))}, expected {want.get(str(n))}"
           for n in oracle_inputs(m) if str(got.get(str(n))) != str(want.get(str(n)))]
    for case in m.get("negative", []):
        if module.name != "bench_entry.mjs":
            break  # the hand-JS controls have no negative entry point
        code = node(engine, "negative", module, case["arg"])["defects"][str(case["arg"])]
        if code != case["defect"]:
            bad.append(f"negative({case['arg']}) gave {code}, expected {case['defect']}")
    if bad:
        raise Fail(f"{p} on {engine}: oracle failed: " + "; ".join(bad))


def cmd_oracle(a):
    """Check (or with --write, record) checksums: Sakura and hand JS must agree on Node."""
    build_cli()
    status = 0
    for p in programs(a.filter):
        m = meta(p)
        try:
            entry = build_sakura(p)
            if a.write:
                sk = node("node", "values", entry, *oracle_inputs(m))["values"]
                js = node("node", "values", js_module(p), *oracle_inputs(m))["values"]
                if sk != js:
                    raise Fail(f"{p}: Sakura {sk} != JS {js}")
                write_checksums(p, sk)
                print(f"{p}: wrote {len(sk)} checksums")
            else:
                for module in (entry, js_module(p)):
                    oracle(p, module, "node", m)
                print(f"{p}: ok")
        except Fail as e:
            print(e, file=sys.stderr)
            status = 1
    return status


def write_checksums(p, values):
    path = PROGRAMS / p / "meta.toml"
    text = path.read_text()
    head = text.split("[checksums]")[0].rstrip() + "\n\n"
    body = "".join(f'"{n}" = "{v}"\n' for n, v in values.items())
    path.write_text(head + "[checksums]  # run(n), recorded by `bench oracle --write`\n" + body)


def main():
    ap = argparse.ArgumentParser(prog="bench", description=__doc__)
    sub = ap.add_subparsers(dest="cmd", required=True)
    o = sub.add_parser("oracle", help="verify Sakura and hand JS against meta.toml checksums")
    o.add_argument("--filter", default="*")
    o.add_argument("--write", action="store_true", help="record checksums (Sakura must equal JS)")
    a = ap.parse_args()
    if not shutil.which("node"):
        print("node is required", file=sys.stderr)
        return 3
    return {"oracle": cmd_oracle}[a.cmd](a)


if __name__ == "__main__":
    sys.exit(main())
