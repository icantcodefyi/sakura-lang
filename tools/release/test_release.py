import argparse
import contextlib
import io
import json
import shutil
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import release


class ReleasePackaging(unittest.TestCase):
    def test_accept_rejects_wrong_published_version_or_commit(self):
        for output in ("sakura 9.9.9 (" + "0" * 40 + ")\n",
                       "sakura " + release.version() + " (" + "1" * 40 + ")\n"):
            with self.subTest(output=output), patch.object(release, "run", return_value=output):
                with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                    release.accept(argparse.Namespace(sakura="/installed path/sakura", commit="0" * 40, native=False))

    def test_npm_pack_refuses_install_lifecycle_scripts(self):
        for hook in release.INSTALL_HOOKS:
            with self.subTest(hook=hook), tempfile.TemporaryDirectory() as tmp:
                Path(tmp, "package.json").write_text(json.dumps({"name": "sakuralang", "scripts": {hook: "node x.js"}}))
                with patch.object(release.subprocess, "run") as npm, contextlib.redirect_stderr(io.StringIO()), \
                        self.assertRaises(SystemExit):
                    release.npm_pack(tmp, tmp)
                npm.assert_not_called()

    def test_windows_readme_uses_the_powershell_installer(self):
        readme = release.readme("1.2.3", "x86_64-pc-windows-msvc")
        self.assertIn("sakura.exe", readme)
        self.assertIn("powershell -ExecutionPolicy Bypass -File .\\install.ps1 -Version 1.2.3", readme)
        self.assertNotIn("install -m", readme)

    def test_archive_carries_the_native_driver_only_for_native_targets(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            binary, driver = tmp / "sakura", tmp / "driver"
            binary.write_bytes(b"binary")
            driver.mkdir()
            for name in release.NATIVE_FILES:
                (driver / name).write_bytes(name.encode())
            with patch.object(release, "notices", return_value="notices"), \
                    contextlib.redirect_stdout(io.StringIO()):
                for target in release.TARGETS:
                    name = target["target"]
                    args = argparse.Namespace(target=name, binary=binary, out=tmp / "out", macos_signed=False,
                                              native_dir=driver if target.get("native") else None)
                    release.archive(args)
                    entries = release.read_archive(tmp / "out" / release.archive_name(release.version(), name))
                    top = f"sakuralang-{release.version()}-{name}"
                    for member in release.native_members(name):
                        self.assertEqual(entries[f"{top}/{member}"], Path(member).name.encode())
                    self.assertEqual(any("lib/sakura/native" in e for e in entries), bool(target.get("native")))
                    with self.subTest(target=name), contextlib.redirect_stderr(io.StringIO()), \
                            self.assertRaises(SystemExit):
                        release.archive(argparse.Namespace(**{**vars(args), "native_dir": None if args.native_dir else driver}))
        self.assertEqual({t["target"] for t in release.TARGETS if t.get("native")},
                         {"aarch64-apple-darwin", "x86_64-unknown-linux-gnu"})

    def test_dist_tag_follows_the_version_string(self):
        for ver, tag, formula in (("0.1.0", "latest", "sakuralang"), ("1.2.3", "latest", "sakuralang"),
                                  ("0.1.0-alpha.1", "alpha", "sakuralang-alpha"),
                                  ("1.0.0-beta.1", "beta", "sakuralang-beta"), ("1.0.0-rc.2", "rc", "sakuralang-rc")):
            with self.subTest(ver=ver):
                self.assertEqual(release.dist_tag(ver), tag)
                self.assertEqual(release.formula_name(ver), formula)
        for ver in ("1.0.0-gamma.1", "0.1.0-1", "0.1.0-dev"):
            with self.subTest(ver=ver), contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                release.dist_tag(ver)

    def test_notes_call_every_version_a_pre_1_0_preview(self):
        for ver in ("0.1.0", "0.1.0-alpha.1"):
            notes = release.release_notes(ver, "0" * 40)
            self.assertIn(f"Sakura Lang {ver}, a pre-1.0 preview release", notes)
            self.assertIn("they begin at\nv1.0.0-beta.1; STABILITY.md lists what each contract covers today.", notes)
            self.assertNotIn("pre-alpha", notes)

    def test_beta_notes_and_readme_say_it_may_have_bugs_and_change(self):
        notes = release.release_notes("1.0.0-beta.1", "0" * 40)
        self.assertIn("Sakura Lang 1.0.0-beta.1, a beta of 1.0", notes)
        self.assertIn("It may have bugs and may\nchange before 1.0.0, and it is not for production.", notes)
        self.assertIn("listed in\nCHANGELOG.md with a migration", notes)
        self.assertNotIn("pre-1.0 preview", notes)
        readme = release.readme("1.0.0-beta.1", "aarch64-apple-darwin")
        self.assertIn("may have bugs and may change before 1.0.0", readme)
        self.assertNotIn("pre-1.0 preview", readme)

    def test_notes_distinguish_unsigned_dry_run_and_notarized_release(self):
        unsigned = release.release_notes("1.2.3", "0" * 40)
        signed = release.release_notes("1.2.3", "0" * 40, True)
        self.assertIn("signing and notarization were skipped", unsigned)
        self.assertNotIn("notarized by Apple", unsigned)
        self.assertIn("notarized by Apple", signed)
        self.assertIn("P62B65A837", signed)
        self.assertIn("stapled ticket", signed)
        for target in release.TARGETS:
            self.assertIn(target["target"], signed)

    def test_assembly_preserves_signed_bytes_for_homebrew_and_npm(self):
        self.assemble_and_check(release.version())

    def test_beta_assembly_names_its_formula_and_packages(self):
        # A dry run of a -beta.N release on a copy of the packaging inputs, bumped as the
        # preparation commit bumps them.
        version = "1.0.0-beta.1"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for path in ("website/public/skills", "tools/install", "packaging"):
                shutil.copytree(release.ROOT / path, root / path)
            for name in release.LICENSES:
                shutil.copy(release.ROOT / name, root / name)
            wrapper = root / "packaging/npm/sakuralang/package.json"
            package = json.loads(wrapper.read_text())
            package["version"] = version
            package["optionalDependencies"] = {t["npm"]: version for t in release.TARGETS}
            wrapper.write_text(json.dumps(package))
            manifest = root / "website/public/skills/manifest.json"
            manifest.write_text(json.dumps({**json.loads(manifest.read_text()), "compiler_version": version}))
            with patch.object(release, "ROOT", root), patch.object(release, "version", return_value=version):
                self.assemble_and_check(version)

    def assemble_and_check(self, version):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            archives = tmp / "archives"
            archives.mkdir()
            binaries = {}
            for target in release.TARGETS:
                binary = b"signed-binary-" + target["target"].encode()
                binaries[target["target"]] = binary
                top = f"sakuralang-{version}-{target['target']}"
                members = [(release.binary_name(target["target"]), binary, 0o755), (release.NOTICES, b"notices", 0o644)]
                members += [(name, (release.ROOT / name).read_bytes(), 0o644) for name in release.LICENSES]
                members += [(name, name.encode(), 0o644) for name in release.native_members(target["target"])]
                writer = release.write_zip if target["os"] == "win32" else release.write_tar
                writer(archives / release.archive_name(version, target["target"]), top, members, 0)
            args = argparse.Namespace(commit="0" * 40, archives=archives, out=tmp / "out", macos_signed=True)
            with contextlib.redirect_stdout(io.StringIO()):
                release.assemble(args)
            output = tmp / "out"
            name = release.formula_name(version)
            formula = (output / f"{name}.rb").read_text()
            self.assertIn(f"class {name.title().replace('-', '')} < Formula\n", formula)
            self.assertEqual('conflicts_with "sakuralang"' in formula, name != "sakuralang")
            for target in release.TARGETS:
                archive_name = release.archive_name(version, target["target"])
                self.assertEqual((output / archive_name).read_bytes(), (archives / archive_name).read_bytes())
                if target["os"] == "darwin" or target["target"] == "x86_64-unknown-linux-gnu":
                    self.assertIn(release.sha256(output / archive_name), formula)
                with tarfile.open(output / f"{target['npm']}-{version}.tgz") as package:
                    self.assertEqual(package.extractfile("package/bin/" + release.binary_name(target["target"])).read(), binaries[target["target"]])
                    self.assertIn("npm i -g sakuralang", package.extractfile("package/README.md").read().decode())
                    for name in release.native_members(target["target"]):
                        self.assertEqual(package.extractfile("package/" + name).read(), name.encode())
            with tarfile.open(output / f"sakuralang-{version}.tgz") as wrapper:
                self.assertIn("# Sakura Lang", wrapper.extractfile("package/README.md").read().decode())
            record = json.loads((output / "release.json").read_text())
            self.assertEqual(record["macos_signing"], "developer-id-notarized")
            self.assertIn("notarized by Apple", (output / "release-notes.md").read_text())


if __name__ == "__main__":
    unittest.main()
