#!/usr/bin/env python3
"""Release packaging for Sakura Lang (design §17.147, research/13 §7).

    release.py version                       print the workspace version (rejects 0.0.0)
    release.py archive  --target T --binary B --out DIR
    release.py smoke    --archive A --commit C
    release.py assemble --commit C --archives DIR --out DIR

`packaging/targets.json` is the one target inventory: the workflow matrix, the archives,
the npm platform packages and the Homebrew formula all come from it. Standard library only;
run with `python3 -I`. Nothing here publishes anything.
"""

import argparse
import gzip
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
COMPILER = ROOT / "compiler"
TARGETS = json.loads((ROOT / "packaging/targets.json").read_text())
LICENSES = ["LICENSE-MIT", "LICENSE-APACHE"]
NOTICES = "THIRD-PARTY-NOTICES.md"
NODE_MAJOR = 26
SEMVER = re.compile(r"^\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$")
REPO_URL = "https://github.com/icantcodefyi/sakuralang"


def fail(msg):
    sys.exit(f"release.py: {msg}")


def version():
    text = (COMPILER / "Cargo.toml").read_text()
    section = text.split("[workspace.package]", 1)[1].split("\n[", 1)[0]
    m = re.search(r'^version = "([^"]+)"$', section, re.M)
    if not m or not SEMVER.match(m.group(1)) or m.group(1) == "0.0.0":
        fail(f"the workspace version must be a release version, not {m and m.group(1)!r}")
    return m.group(1)


def target_info(target):
    for t in TARGETS:
        if t["target"] == target:
            return t
    fail(f"{target} is not in packaging/targets.json")


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def source_epoch():
    out = subprocess.run(["git", "-C", ROOT, "log", "-1", "--format=%ct"], capture_output=True, text=True)
    return int(out.stdout.strip()) if out.returncode == 0 and out.stdout.strip() else 0


# --- archive ------------------------------------------------------------------------------


def notices(target):
    """The licence texts of every crate linked into `sakura` for `target`."""
    meta = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", target],
            cwd=COMPILER,
            check=True,
            capture_output=True,
        ).stdout
    )
    pkgs = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = next(p["id"] for p in meta["packages"] if p["name"] == "sakura_cli" and p["source"] is None)
    seen, todo = set(), [root]
    while todo:
        node = nodes[todo.pop()]
        for dep in node["deps"]:
            if dep["pkg"] not in seen and any(k["kind"] is None for k in dep["dep_kinds"]):
                seen.add(dep["pkg"])
                todo.append(dep["pkg"])
    out = [
        "# Third-party notices\n",
        f"`sakura` for `{target}` links the following crates from crates.io. Sakura Lang's own",
        "code is under MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE).\n",
    ]
    for pkg in sorted((pkgs[i] for i in seen if pkgs[i]["source"]), key=lambda p: (p["name"], p["version"])):
        crate_dir = Path(pkg["manifest_path"]).parent
        files = sorted(
            f for f in crate_dir.iterdir()
            if f.is_file() and re.match(r"(LICEN[CS]E|COPYING|NOTICE|UNLICENSE)", f.name, re.I)
        )
        out.append(f"## {pkg['name']} {pkg['version']}\n")
        out.append(f"License: {pkg.get('license') or 'see below'}  ")
        out.append(f"Source: {pkg.get('repository') or 'https://crates.io/crates/' + pkg['name']}\n")
        if not files:
            out.append("(The published crate ships no licence file; its licence is the SPDX expression above.)\n")
        for f in files:
            out.append(f"### {f.name}\n\n```text\n{f.read_text(errors='replace').strip()}\n```\n")
    return "\n".join(out) + "\n"


def readme(ver, target):
    return f"""# Sakura Lang {ver} ({target})

Pre-alpha. `sakura` is self-contained: the standard library, the JS and WASM runtimes,
`docs/llms.txt` and the agent skill are embedded, so it needs no checkout.

Install: put `sakura` on your PATH (for example `install -m 755 sakura ~/.local/bin/`), or use
`install.sh` from the release, which verifies `SHA256SUMS` first. `sakura --version` prints
the version and the source commit.

Node {NODE_MAJOR} is required to run programs and tests (`sakura run`, `sakura test`, and MCP
tools that execute code); set `SAKURA_NODE` to choose the executable. `check`, `build`, `doc`,
`fmt`, `explain` and `mcp serve` need no Node. `sakura verify` needs the pinned Z3, which is
not bundled. The binary is not signed or notarized.

Start with `sakura new cli demo`, then `sakura check`, `sakura test`, `sakura build`;
`sakura setup --apply` installs the agent skill and MCP entries in a project.
Source: {REPO_URL}
"""


def write_tar(out, top, members, mtime):
    """A deterministic .tar.gz: sorted entries, fixed owner, `mtime`, normalized modes."""
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.PAX_FORMAT) as tar:
        dirinfo = tarfile.TarInfo(top)
        dirinfo.type, dirinfo.mode, dirinfo.mtime = tarfile.DIRTYPE, 0o755, mtime
        tar.addfile(dirinfo)
        for name, data, mode in sorted(members):
            info = tarfile.TarInfo(f"{top}/{name}")
            info.size, info.mode, info.mtime = len(data), mode, mtime
            tar.addfile(info, io.BytesIO(data))
    with open(out, "wb") as f, gzip.GzipFile(filename="", mode="wb", fileobj=f, mtime=mtime) as gz:
        gz.write(raw.getvalue())


def archive(args):
    ver = version()
    target_info(args.target)
    top = f"sakuralang-{ver}-{args.target}"
    members = [("sakura", Path(args.binary).read_bytes(), 0o755)]
    members += [(name, (ROOT / name).read_bytes(), 0o644) for name in LICENSES]
    members.append((NOTICES, notices(args.target).encode(), 0o644))
    members.append(("README.md", readme(ver, args.target).encode(), 0o644))
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    path = out / f"{top}.tar.gz"
    write_tar(path, top, members, source_epoch())
    print(path)


# --- smoke --------------------------------------------------------------------------------


def run(cmd, cwd, env, want=0, stdin=None):
    p = subprocess.run(cmd, cwd=cwd, env=env, input=stdin, capture_output=True, text=True, timeout=300)
    if p.returncode != want:
        fail(f"{cmd} exited {p.returncode}, wanted {want}\nstdout:\n{p.stdout}\nstderr:\n{p.stderr}")
    return p.stdout


def mcp_smoke(sakura, cwd, env, ver, commit):
    """Bounded stdio MCP session: initialize, list tools, `compiler_info`, EOF, clean exit."""
    msgs = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "release-smoke", "version": "0"}}},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
        {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "compiler_info", "arguments": {}}},
    ]
    p = subprocess.Popen([sakura, "mcp", "serve"], cwd=cwd, env=env, text=True,
                         stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    replies = {}
    try:
        # One request at a time, so EOF never races an unanswered request.
        for msg in msgs:
            p.stdin.write(json.dumps(msg) + "\n")
            p.stdin.flush()
            if "id" in msg:
                while msg["id"] not in replies:
                    line = p.stdout.readline()
                    if not line:
                        fail(f"mcp serve closed stdout early: {p.stderr.read()}")
                    reply = json.loads(line)
                    replies[reply.get("id")] = reply
        p.stdin.close()
        code = p.wait(timeout=30)
    finally:
        p.kill()
    if code != 0:
        fail(f"mcp serve exited {code} after EOF")
    tools = [t["name"] for t in replies[2]["result"]["tools"]]
    info = replies[3]["result"]["structuredContent"]
    if "compiler_info" not in tools or info["version"] != ver or info["commit"] != commit:
        fail(f"unexpected MCP replies: tools={tools} info={info}")
    if "wasm" not in info["features"]["backends"]:
        fail(f"the WASM backend is missing: {info['features']}")


def skill_matches_source(installed, ver, commit):
    """`setup --apply`'s skill is the canonical skill after the two frontmatter fills."""
    source = ROOT / "skills/sakura"
    want = {p.relative_to(source).as_posix(): p.read_bytes() for p in source.rglob("*") if p.is_file()}
    want["SKILL.md"] = (
        want["SKILL.md"].decode()
        .replace("@SAKURA_COMPILER_VERSION@", ver)
        .replace("@SAKURA_COMPILER_GIT@", commit)
        .encode()
    )
    got = {p.relative_to(installed).as_posix(): p.read_bytes() for p in installed.rglob("*") if p.is_file()}
    if got != want:
        diff = sorted(k for k in set(got) | set(want) if got.get(k) != want.get(k))
        fail(f"installed skill differs from skills/sakura in {diff}")


def smoke(args):
    ver, commit = version(), args.commit
    archive_path = Path(args.archive).resolve()
    with tempfile.TemporaryDirectory(prefix="sakura smoke ") as tmp:
        tmp = Path(tmp)
        with tarfile.open(archive_path) as tar:
            names = sorted(tar.getnames())
            top = archive_path.name.removesuffix(".tar.gz")
            want = sorted([top] + [f"{top}/{n}" for n in ["sakura", "README.md", NOTICES] + LICENSES])
            if names != want:
                fail(f"archive layout {names}, wanted {want}")
            tar.extractall(tmp / "unpacked", filter="data")
        sakura = str(tmp / "unpacked" / top / "sakura")
        home = tmp / "home"
        home.mkdir()
        # A clean environment: no checkout, an empty HOME, Node from PATH.
        env = {"PATH": os.environ["PATH"], "HOME": str(home), "LANG": "C.UTF-8"}
        out = run([sakura, "--version"], tmp, env)
        if out != f"sakura {ver} ({commit})\n":
            fail(f"--version printed {out!r}")
        run([sakura, "new", "cli", "demo"], tmp, env)
        demo = tmp / "demo"
        run([sakura, "check"], demo, env)
        run([sakura, "test"], demo, env)
        run([sakura, "build"], demo, env)
        if run(["node", "target/sakura/js/__main__.js", "Ann"], demo, env) != "hello, Ann\n":
            fail("the built JS printed the wrong greeting")
        if run([sakura, "run", "--target", "wasm", "--", "Bob"], demo, env) != "hello, Bob\n":
            fail("the WASM build printed the wrong greeting")
        mcp_smoke(sakura, demo, env, ver, commit)
        run([sakura, "setup", "--client", "all", "--apply"], demo, env)
        again = run([sakura, "setup", "--client", "all", "--apply"], demo, env)
        if any(line.split()[0] not in ("same", "keep") for line in again.splitlines()[1:]):
            fail(f"a second setup --apply was not idempotent:\n{again}")
        for client in (".claude", ".agents", ".cursor"):
            skill_matches_source(demo / client / "skills/sakura", ver, commit)
        if any(home.iterdir()):
            fail(f"the smoke wrote into HOME: {sorted(p.name for p in home.iterdir())}")
        # Without Node: checking works, running tests fails with an actionable error.
        no_node = dict(env, SAKURA_NODE=str(tmp / "no-such-node"))
        run([sakura, "check"], demo, no_node)
        p = subprocess.run([sakura, "test"], cwd=demo, env=no_node, capture_output=True, text=True, timeout=60)
        if p.returncode == 0 or "cannot start node" not in p.stderr:
            fail(f"a missing Node gave exit {p.returncode}: {p.stderr}")
    print(f"smoke ok: {archive_path.name}")


# --- assemble -----------------------------------------------------------------------------


def render(text, ver, sums):
    text = text.replace("@VERSION@", ver)
    for t in TARGETS:
        text = text.replace(f"@SHA256_{t['target']}@", sums[f"sakuralang-{ver}-{t['target']}.tar.gz"])
    left = re.search(r"@(VERSION|SHA256_[\w-]+)@", text)
    if left:
        fail(f"unfilled placeholder {left.group(0)}: a target lacks an archive")
    return text


def npm_pack(stage, out):
    p = subprocess.run(["npm", "pack", "--json", "--pack-destination", str(out)], cwd=stage,
                       check=True, capture_output=True, text=True)
    return json.loads(p.stdout)[0]["filename"]


def assemble(args):
    ver, commit = version(), args.commit
    src, out = Path(args.archives), Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    sums = {}
    for t in TARGETS:
        name = f"sakuralang-{ver}-{t['target']}.tar.gz"
        if not (src / name).is_file():
            fail(f"missing {name}: every target in packaging/targets.json must build")
        shutil.copy(src / name, out / name)
        sums[name] = sha256(out / name)

    # The canonical skill ZIP and its content manifest, byte for byte (plan §4, DX1).
    skills = ROOT / "website/public/skills"
    manifest = json.loads((skills / "manifest.json").read_text())
    if manifest["compiler_version"] != ver or manifest["sha256"] != sha256(skills / "sakura-skill.zip"):
        fail("website/public/skills is stale: run `cargo run -p agent_docs` and commit")
    shutil.copy(skills / "sakura-skill.zip", out / "sakura-skill.zip")
    shutil.copy(skills / "manifest.json", out / "sakura-skill-manifest.json")

    # The installer, defaulting to this release's version.
    installer = (ROOT / "tools/install/install.sh").read_text()
    if 'default_version=""' not in installer:
        fail("install.sh lost its default_version line")
    (out / "install.sh").write_text(installer.replace('default_version=""', f'default_version="{ver}"', 1))

    formula = render((ROOT / "packaging/homebrew/sakuralang.rb").read_text(), ver, sums)
    (out / "sakuralang.rb").write_text(formula)

    # npm: the reviewed wrapper as committed, and one platform package per target.
    wrapper = json.loads((ROOT / "packaging/npm/sakuralang/package.json").read_text())
    want_deps = {t["npm"]: ver for t in TARGETS}
    if wrapper["version"] != ver or wrapper.get("optionalDependencies") != want_deps:
        fail(f"packaging/npm/sakuralang/package.json must have version {ver} and "
             f"optionalDependencies {want_deps}")
    with tempfile.TemporaryDirectory() as tmp:
        stage = Path(tmp) / "sakuralang"
        shutil.copytree(ROOT / "packaging/npm/sakuralang", stage)
        for name in LICENSES:
            shutil.copy(ROOT / name, stage / name)
        npm_files = [npm_pack(stage, out)]
        for t in TARGETS:
            stage = Path(tmp) / t["npm"]
            (stage / "bin").mkdir(parents=True)
            top = f"sakuralang-{ver}-{t['target']}"
            with tarfile.open(out / f"{top}.tar.gz") as tar:
                for name in ["sakura", NOTICES] + LICENSES:
                    data = tar.extractfile(f"{top}/{name}").read()
                    dest = stage / ("bin/sakura" if name == "sakura" else name)
                    dest.write_bytes(data)
            (stage / "bin/sakura").chmod(0o755)
            pkg = {
                "name": t["npm"],
                "version": ver,
                "description": f"The `sakura` binary for {t['target']}; install `sakuralang` instead.",
                "license": "MIT OR Apache-2.0",
                "homepage": "https://www.sakuralang.com",
                "repository": {"type": "git", "url": f"git+{REPO_URL}.git"},
                "os": [t["os"]],
                "cpu": [t["cpu"]],
                **({"libc": [t["libc"]]} if "libc" in t else {}),
                "files": ["bin/sakura", NOTICES] + LICENSES,
                "preferUnplugged": True,
            }
            (stage / "package.json").write_text(json.dumps(pkg, indent=2) + "\n")
            npm_files.append(npm_pack(stage, out))

    files = sorted(p.name for p in out.iterdir() if p.name not in ("SHA256SUMS", "release.json"))
    record = {
        "name": "sakuralang",
        "version": ver,
        "tag": f"v{ver}",
        "commit": commit,
        "repository": REPO_URL,
        "run": os.environ.get("GITHUB_RUN_ID"),
        "targets": [t["target"] for t in TARGETS],
        "node": NODE_MAJOR,
        "skill_sha256": manifest["sha256"],
        "npm": npm_files,
        "files": [{"name": n, "sha256": sha256(out / n), "size": (out / n).stat().st_size} for n in files],
    }
    (out / "release.json").write_text(json.dumps(record, indent=2) + "\n")
    lines = [f"{sha256(out / n)}  {n}\n" for n in sorted(files + ["release.json"])]
    (out / "SHA256SUMS").write_text("".join(lines))
    print("".join(lines), end="")


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("version")
    a = sub.add_parser("archive")
    a.add_argument("--target", required=True)
    a.add_argument("--binary", required=True)
    a.add_argument("--out", required=True)
    s = sub.add_parser("smoke")
    s.add_argument("--archive", required=True)
    s.add_argument("--commit", required=True)
    m = sub.add_parser("assemble")
    m.add_argument("--commit", required=True)
    m.add_argument("--archives", required=True)
    m.add_argument("--out", required=True)
    args = ap.parse_args()
    if getattr(args, "commit", None) is not None and not re.fullmatch(r"[0-9a-f]{40}", args.commit):
        fail(f"--commit must be a full 40-character SHA, not {args.commit!r}")
    if args.cmd == "version":
        print(version())
    else:
        {"archive": archive, "smoke": smoke, "assemble": assemble}[args.cmd](args)


if __name__ == "__main__":
    main()
