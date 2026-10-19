#!/usr/bin/env python3
"""Release packaging for Sakura Lang (design §17.147, research/13 §7).

    release.py prepare [--previous-tag TAG] [--date YYYY-MM-DD]
    release.py version                       print the workspace version (rejects 0.0.0)
    release.py archive  --target T --binary B --out DIR
    release.py smoke    --archive A --commit C
    release.py accept   --sakura INSTALLED --commit C
    release.py assemble --commit C --archives DIR --out DIR

`packaging/targets.json` is the one target inventory: the workflow matrix, the archives,
the npm platform packages and the Homebrew formula all come from it. Standard library only;
run with `python3 -I`. Nothing here publishes anything.
"""

import argparse
import calendar
import datetime
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
import time
import threading
import zipfile
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


def semver(text):
    match = re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?", text)
    if not match:
        fail(f"invalid release version: {text!r}")
    major, minor, patch = map(int, match.group(1, 2, 3))
    pre = match.group(4)
    identifiers = []
    if pre:
        for part in pre.split("."):
            if part.isdigit() and len(part) > 1 and part.startswith("0"):
                fail(f"invalid numeric prerelease identifier: {text!r}")
            identifiers.append((0, int(part)) if part.isdigit() else (1, part))
    return major, minor, patch, pre is None, tuple(identifiers)


def release_date(value):
    if type(value) is datetime.date:
        return value
    if isinstance(value, str):
        try:
            return datetime.date.fromisoformat(value)
        except ValueError:
            pass
    fail(f"expected an ISO calendar date, got {value!r}")


def read_toml(text):
    try:
        import tomllib
    except ModuleNotFoundError:
        fail("prepare requires Python 3.11+ for TOML parsing")
    try:
        return tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        fail(f"invalid release TOML: {error}")


def deprecations(text):
    data = read_toml(text)
    if set(data) != {"deprecation"} or not isinstance(data["deprecation"], list):
        fail("deprecations.toml must contain deprecation = [] or [[deprecation]] entries")
    entries, codes = {}, set()
    for raw in data["deprecation"]:
        required = {"feature", "code", "since", "earliest_removal"}
        allowed = required | {"replacement", "reason", "removed"}
        if not isinstance(raw, dict) or not required <= raw.keys() or raw.keys() - allowed:
            fail("deprecation entry has missing or unknown fields")
        entry = dict(raw)
        for field in ("feature", "code"):
            if not isinstance(entry[field], str) or not entry[field].strip():
                fail(f"deprecation {field} must be a nonempty string")
        feature, code = entry["feature"], entry["code"]
        if feature in entries or code in codes or not re.fullmatch(r"W[0-9]{4}", code):
            fail(f"duplicate feature/code or invalid warning code: {feature!r}, {code!r}")
        if ("replacement" in entry) == ("reason" in entry):
            fail(f"{feature}: provide exactly one replacement or reason")
        explanation = entry.get("replacement", entry.get("reason"))
        if not isinstance(explanation, str) or not explanation.strip():
            fail(f"{feature}: replacement or reason must be nonempty")
        for field in ("since", "earliest_removal"):
            boundary = entry[field]
            if not isinstance(boundary, dict) or set(boundary) != {"version", "date"}:
                fail(f"{feature}: {field} needs exactly version and date")
            if not isinstance(boundary["version"], str):
                fail(f"{feature}: {field}.version must be a string")
            parsed = semver(boundary["version"])
            if not parsed[3]:
                fail(f"{feature}: deprecation boundaries must be final releases")
            entry[field] = {"version": boundary["version"], "date": release_date(boundary["date"])}
        since, earliest = entry["since"], entry["earliest_removal"]
        first = semver(since["version"])
        minimum_version = (first[0], first[1] + 2, 0, True, ())
        month = since["date"].month - 1 + 6
        year = since["date"].year + month // 12
        month = month % 12 + 1
        try:
            minimum_date = datetime.date(year, month, min(since["date"].day, calendar.monthrange(year, month)[1]))
        except ValueError:
            fail(f"{feature}: since.date cannot represent a six-month deprecation period")
        if semver(earliest["version"]) < minimum_version or earliest["date"] < minimum_date:
            fail(f"{feature}: earliest_removal must allow two minors and six calendar months")
        if "removed" in entry:
            if not isinstance(entry["removed"], str):
                fail(f"{feature}: removed must be a version")
            removed = semver(entry["removed"])
            if not removed[3] or removed < semver(earliest["version"]) or removed[0] <= first[0]:
                fail(f"{feature}: removed precedes earliest_removal or is outside a major bump")
        entries[feature] = entry
        codes.add(code)
    return entries


def git_text(*arguments):
    result = subprocess.run(["git", *arguments], cwd=ROOT, text=True, capture_output=True)
    if result.returncode:
        fail(f"git {' '.join(arguments)} failed; fetch full release history and tags")
    return result.stdout


def tagged_deprecations(tag):
    exists = git_text("ls-tree", "--name-only", tag, "compiler/deprecations.toml").strip()
    return deprecations(git_text("show", f"{tag}:compiler/deprecations.toml")) if exists else {}


def prepare(args):
    candidate = version()
    current = semver(candidate)
    today = release_date(args.date) if args.date else datetime.datetime.now(datetime.timezone.utc).date()
    if git_text("rev-parse", "--is-shallow-repository").strip() == "true":
        fail("prepare needs full history; fetch --unshallow and release tags")
    tags = []
    for tag in git_text("tag", "--merged", "HEAD").splitlines():
        if tag.startswith("v") and SEMVER.fullmatch(tag[1:]):
            parsed = semver(tag[1:])
            if parsed < current:
                tags.append((parsed, tag))
    tags.sort()
    previous = tags[-1][1] if tags else None
    if args.previous_tag is not None and args.previous_tag != previous:
        fail(f"--previous-tag must select the latest preceding ancestor release: {previous!r}")
    entries = deprecations((COMPILER / "deprecations.toml").read_text())
    if previous is None:
        if current[2] != 0 or any("removed" in entry for entry in entries.values()):
            fail("no previous release tag; cannot validate a patch or removal without release history")
        old_entries = {}
    else:
        manifest = read_toml(git_text("show", f"{previous}:compiler/Cargo.toml"))
        prior = semver(previous[1:])
        if current[:2] == prior[:2]:
            msrv = read_toml((COMPILER / "Cargo.toml").read_text())["workspace"]["package"]["rust-version"]
            if msrv != manifest["workspace"]["package"]["rust-version"]:
                fail(f"patch release MSRV differs from {previous}; rust-version must stay unchanged")
        old_entries = tagged_deprecations(previous)
    for feature, old in old_entries.items():
        entry = entries.get(feature)
        if entry is None:
            fail(f"{feature}: retain historical deprecation metadata")
        for field in ("code", "since", "earliest_removal"):
            if entry[field] != old[field]:
                fail(f"{feature}: historical {field} must not change")
        if "removed" in old and entry.get("removed") != old["removed"]:
            fail(f"{feature}: historical removal must not change")
    for feature, entry in entries.items():
        if "removed" not in entry or "removed" in old_entries.get(feature, {}):
            continue
        if entry["removed"] != candidate or previous is None or current[0] <= semver(previous[1:])[0]:
            fail(f"{feature}: new removal must name this candidate and occur in a major bump")
        if today < entry["earliest_removal"]["date"]:
            fail(f"{feature}: removal date precedes earliest_removal")
        first = semver(entry["since"]["version"])
        minors = set()
        for parsed, tag in tags:
            if parsed[3] and first <= parsed < current:
                shipped = tagged_deprecations(tag).get(feature, {})
                if (shipped.get("code") == entry["code"] and shipped.get("since") == entry["since"]
                        and "removed" not in shipped):
                    minors.add(parsed[:2])
        if len(minors) < 2:
            fail(f"{feature}: removal needs two shipped final minor releases; prereleases do not count")
    print(f"candidate {candidate}: deprecation and MSRV checks passed (previous {previous or 'none'})")


def target_info(target):
    for t in TARGETS:
        if t["target"] == target:
            return t
    fail(f"{target} is not in packaging/targets.json")


def binary_name(target):
    return "sakura.exe" if target_info(target)["os"] == "win32" else "sakura"


def archive_name(ver, target):
    suffix = ".zip" if target_info(target)["os"] == "win32" else ".tar.gz"
    return f"sakuralang-{ver}-{target}{suffix}"


def read_archive(path):
    if Path(path).suffix == ".zip":
        with zipfile.ZipFile(path) as archive:
            return {n.rstrip("/"): archive.read(n) for n in archive.namelist()}
    with tarfile.open(path) as archive:
        return {m.name: archive.extractfile(m).read() if m.isfile() else b"" for m in archive.getmembers()}


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


def readme(ver, target, macos_signed=False):
    signing = (
        "This macOS binary is Developer ID signed and notarized. The bare Mach-O cannot\n"
        "carry a stapled ticket; Gatekeeper may need network access to check notarization."
        if macos_signed else "This binary has no Developer ID signature or notarization."
    )
    installation = (
        "Install: put `sakura.exe` in a directory on PATH, or download `install.ps1` from\n"
        "the release and run `powershell -ExecutionPolicy Bypass -File .\\install.ps1 -Version " + ver + "`. The installer verifies\n"
        "`SHA256SUMS` before extracting and uses `%LOCALAPPDATA%\\Programs\\sakuralang\\bin`.\n"
        "Add that directory to PATH yourself."
        if target_info(target)["os"] == "win32" else
        "Install: put `sakura` on your PATH (for example `install -m 755 sakura ~/.local/bin/`),\n"
        "or use `install.sh` from the release, which verifies `SHA256SUMS` first."
    )
    return f"""# Sakura Lang {ver} ({target})

Pre-alpha. `sakura` is self-contained: the standard library, the JS and WASM runtimes,
`docs/llms.txt` and the agent skill are embedded, so it needs no checkout.

{installation} `sakura --version` prints the version and the source commit.

Node {NODE_MAJOR} is required to run programs and tests (`sakura run`, `sakura test`, and MCP
tools that execute code); set `SAKURA_NODE` to choose the executable. `check`, `build`, `doc`,
`fmt`, `explain` and `mcp serve` need no Node. `sakura verify` needs the pinned Z3, which is
not bundled. {signing}

Start with `sakura new cli demo`, then `sakura check`, `sakura test`, `sakura build`;
`sakura setup --apply` installs the agent skill and MCP entries in a project.
Source: {REPO_URL}
"""


def release_notes(ver, commit, macos_signed=False):
    platforms = "\n".join(f"- `{t['target']}` (npm `{t['npm']}`)" for t in TARGETS)
    signing = (
        "macOS binaries are signed with Developer ID Application: Aniruddh Dubge\n"
        "(P62B65A837) and notarized by Apple. Bare Mach-O executables cannot carry a\n"
        "stapled ticket; Gatekeeper may need network access. Linux and Windows binaries are unsigned."
        if macos_signed else
        "Dry run: macOS signing and notarization were skipped; these artifacts have no\n"
        "Developer ID signature or notarization."
    )
    return f"""Sakura Lang {ver}, a pre-alpha release from commit {commit}. Nothing beyond the
versioned diagnostic schema and the fixture formats is stable (STABILITY.md).

Packaging targets (called supported for this version only after the published-install
smoke passes on every applicable channel):

{platforms}

{signing} Verify artifacts with `SHA256SUMS` and
`gh attestation verify <file> --repo icantcodefyi/sakuralang`.

Node {NODE_MAJOR} is required for `sakura run`, `sakura test` and MCP tools that execute
programs; `check`, `build`, `doc` and `mcp serve` need no Node. `sakura verify`
needs the pinned Z3, which is not bundled.

Install: download `install.sh`, read it, then `sh install.sh --prefix ~/.local`; or
`npm install -g sakuralang@{ver}`, or verify and extract an archive by hand
(docs/release.md).
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


def write_zip(out, top, members, mtime):
    date = time.gmtime(max(mtime, 315532800))[:6]
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data, mode in sorted(members):
            info = zipfile.ZipInfo(f"{top}/{name}", date_time=date)
            info.create_system = 3
            info.external_attr = (0o100000 | mode) << 16
            archive.writestr(info, data, compress_type=zipfile.ZIP_DEFLATED)


def archive(args):
    ver = version()
    target_info(args.target)
    if args.macos_signed and not args.target.endswith("-apple-darwin"):
        fail("--macos-signed requires a Darwin target")
    top = f"sakuralang-{ver}-{args.target}"
    members = [(binary_name(args.target), Path(args.binary).read_bytes(), 0o755)]
    members += [(name, (ROOT / name).read_bytes(), 0o644) for name in LICENSES]
    members.append((NOTICES, notices(args.target).encode(), 0o644))
    members.append(("README.md", readme(ver, args.target, args.macos_signed).encode(), 0o644))
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    path = out / archive_name(ver, args.target)
    writer = write_zip if path.suffix == ".zip" else write_tar
    writer(path, top, members, source_epoch())
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


def lsp_smoke(sakura, cwd, env):
    process = subprocess.Popen([sakura, "lsp"], cwd=cwd, env=env,
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    timer = threading.Timer(30, process.kill)
    timer.start()
    try:
        def request(message):
            data = json.dumps(message).encode()
            process.stdin.write(f"Content-Length: {len(data)}\r\n\r\n".encode() + data)
            process.stdin.flush()
            while True:
                length = None
                while True:
                    line = process.stdout.readline()
                    if not line:
                        fail("LSP closed stdout before replying")
                    if line in (b"\r\n", b"\n"):
                        break
                    if line.lower().startswith(b"content-length:"):
                        length = int(line.split(b":", 1)[1])
                if length is None or length > 4 * 1024 * 1024:
                    fail("invalid LSP Content-Length")
                reply = json.loads(process.stdout.read(length))
                if reply.get("id") == message["id"]:
                    return reply
        reply = request({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                         "params": {"capabilities": {}, "rootUri": cwd.as_uri()}})
        if reply.get("result", {}).get("serverInfo", {}).get("name") != "sakura":
            fail(f"unexpected LSP initialize reply: {reply}")
        data = json.dumps({"jsonrpc": "2.0", "method": "initialized", "params": {}}).encode()
        process.stdin.write(f"Content-Length: {len(data)}\r\n\r\n".encode() + data)
        process.stdin.flush()
        request({"jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": None})
        data = json.dumps({"jsonrpc": "2.0", "method": "exit"}).encode()
        process.stdin.write(f"Content-Length: {len(data)}\r\n\r\n".encode() + data)
        process.stdin.flush()
        process.stdin.close()
        if process.wait(timeout=30) != 0:
            fail("LSP did not exit cleanly")
    finally:
        timer.cancel()
        process.kill()


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
        windows = archive_path.suffix == ".zip"
        top = archive_path.name.removesuffix(".zip" if windows else ".tar.gz")
        binary = "sakura.exe" if windows else "sakura"
        entries = read_archive(archive_path)
        want = {f"{top}/{n}" for n in [binary, "README.md", NOTICES] + LICENSES}
        if set(entries) - {top} != want:
            fail(f"archive layout {sorted(entries)}, wanted {sorted(want)}")
        unpacked = tmp / "unpacked" / top
        unpacked.mkdir(parents=True)
        for name in want:
            (unpacked / Path(name).name).write_bytes(entries[name])
        sakura = str(unpacked / binary)
        Path(sakura).chmod(0o755)
        accept_installed(sakura, ver, commit, tmp)
    print(f"smoke ok: {archive_path.name}")


def accept(args):
    sakura = os.path.abspath(args.sakura)
    with tempfile.TemporaryDirectory(prefix="sakura accept ") as tmp:
        accept_installed(sakura, version(), args.commit, Path(tmp))
    print(f"accept ok: {sakura}")


def accept_installed(sakura, ver, commit, tmp):
    home = tmp / "home"
    home.mkdir()
    # A clean environment: no checkout, an empty HOME, Node from PATH.
    env = {"PATH": os.environ["PATH"], "HOME": str(home), "LANG": "C.UTF-8",
           "SAKURA_CACHE_DIR": str(tmp / "cache")}
    if os.name == "nt":
        env.update({k: os.environ[k] for k in ("SystemRoot", "PATHEXT", "COMSPEC", "WINDIR") if k in os.environ})
        env["USERPROFILE"] = str(home)
        temporary = tmp / "temp"
        temporary.mkdir()
        env.update(TMP=str(temporary), TEMP=str(temporary))
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
    if run([sakura, "run", "--", "Ann"], demo, env) != "hello, Ann\n":
        fail("the default run printed the wrong greeting")
    mcp_smoke(sakura, demo, env, ver, commit)
    if os.name == "nt":
        run(["git", "init"], demo, env)
        run(["git", "config", "core.autocrlf", "true"], demo, env)
        run(["git", "add", "."], demo, env)
        run(["git", "-c", "user.name=Release smoke", "-c", "user.email=smoke@example.invalid",
             "commit", "-m", "Fresh project"], demo, env)
        for source in demo.rglob("*.sak"):
            source.unlink()
        run(["git", "checkout", "HEAD", "--", "."], demo, env)
        run([sakura, "fmt", "--check"], demo, env)
        lsp_smoke(sakura, demo, env)
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


# --- assemble -----------------------------------------------------------------------------


def render(text, ver, sums):
    text = text.replace("@VERSION@", ver)
    for t in TARGETS:
        text = text.replace(f"@SHA256_{t['target']}@", sums[archive_name(ver, t["target"])])
    left = re.search(r"@(VERSION|SHA256_[\w-]+)@", text)
    if left:
        fail(f"unfilled placeholder {left.group(0)}: a target lacks an archive")
    return text


# npm runs these on the user's machine; the packages promise to ship none (docs/release.md).
INSTALL_HOOKS = ("preinstall", "install", "postinstall", "prepare")


def npm_pack(stage, out):
    hooks = sorted(set(json.loads((Path(stage) / "package.json").read_text()).get("scripts", {})) & set(INSTALL_HOOKS))
    if hooks:
        fail(f"{stage}/package.json declares install lifecycle scripts {hooks}: the npm packages must not run code on install")
    p = subprocess.run(["npm", "pack", "--ignore-scripts", "--json", "--pack-destination", str(Path(out).resolve())], cwd=stage,
                       check=True, capture_output=True, text=True)
    return json.loads(p.stdout)[0]["filename"]


def assemble(args):
    ver, commit = version(), args.commit
    src, out = Path(args.archives), Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    sums = {}
    for t in TARGETS:
        name = archive_name(ver, t["target"])
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

    windows_installer = (ROOT / "tools/install/install.ps1").read_text()
    (out / "install.ps1").write_text(windows_installer.replace('$DefaultVersion = ""', f'$DefaultVersion = "{ver}"', 1))

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
            binary = binary_name(t["target"])
            entries = read_archive(out / archive_name(ver, t["target"]))
            for name in [binary, NOTICES] + LICENSES:
                dest = stage / (f"bin/{binary}" if name == binary else name)
                dest.write_bytes(entries[f"{top}/{name}"])
            (stage / f"bin/{binary}").chmod(0o755)
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
                "files": [f"bin/{binary}", NOTICES] + LICENSES,
                "preferUnplugged": True,
            }
            (stage / "package.json").write_text(json.dumps(pkg, indent=2) + "\n")
            npm_files.append(npm_pack(stage, out))

    (out / "release-notes.md").write_text(release_notes(ver, commit, args.macos_signed))

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
        "macos_signing": "developer-id-notarized" if args.macos_signed else "skipped-dry-run",
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
    p = sub.add_parser("prepare")
    p.add_argument("--previous-tag")
    p.add_argument("--date", help="candidate release date in UTC (YYYY-MM-DD)")
    a = sub.add_parser("archive")
    a.add_argument("--target", required=True)
    a.add_argument("--binary", required=True)
    a.add_argument("--out", required=True)
    a.add_argument("--macos-signed", action="store_true")
    s = sub.add_parser("smoke")
    s.add_argument("--archive", required=True)
    s.add_argument("--commit", required=True)
    a = sub.add_parser("accept")
    a.add_argument("--sakura", required=True)
    a.add_argument("--commit", required=True)
    m = sub.add_parser("assemble")
    m.add_argument("--commit", required=True)
    m.add_argument("--archives", required=True)
    m.add_argument("--out", required=True)
    m.add_argument("--macos-signed", action="store_true")
    args = ap.parse_args()
    if getattr(args, "commit", None) is not None and not re.fullmatch(r"[0-9a-f]{40}", args.commit):
        fail(f"--commit must be a full 40-character SHA, not {args.commit!r}")
    if args.cmd == "version":
        print(version())
    else:
        {"archive": archive, "smoke": smoke, "accept": accept, "assemble": assemble, "prepare": prepare}[args.cmd](args)


if __name__ == "__main__":
    main()
