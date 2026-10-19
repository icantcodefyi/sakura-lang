import argparse
import contextlib
import io
import json
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
                    release.accept(argparse.Namespace(sakura="/installed path/sakura", commit="0" * 40))

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
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            archives = tmp / "archives"
            archives.mkdir()
            version = release.version()
            binaries = {}
            for target in release.TARGETS:
                binary = b"signed-binary-" + target["target"].encode()
                binaries[target["target"]] = binary
                top = f"sakuralang-{version}-{target['target']}"
                members = [(release.binary_name(target["target"]), binary, 0o755), (release.NOTICES, b"notices", 0o644)]
                members += [(name, (release.ROOT / name).read_bytes(), 0o644) for name in release.LICENSES]
                writer = release.write_zip if target["os"] == "win32" else release.write_tar
                writer(archives / release.archive_name(version, target["target"]), top, members, 0)
            args = argparse.Namespace(commit="0" * 40, archives=archives, out=tmp / "out", macos_signed=True)
            with contextlib.redirect_stdout(io.StringIO()):
                release.assemble(args)
            output = tmp / "out"
            for target in release.TARGETS:
                archive_name = release.archive_name(version, target["target"])
                self.assertEqual((output / archive_name).read_bytes(), (archives / archive_name).read_bytes())
                if target["os"] == "darwin" or target["target"] == "x86_64-unknown-linux-gnu":
                    self.assertIn(release.sha256(output / archive_name), (output / "sakuralang.rb").read_text())
                with tarfile.open(output / f"{target['npm']}-{version}.tgz") as package:
                    self.assertEqual(package.extractfile("package/bin/" + release.binary_name(target["target"])).read(), binaries[target["target"]])
            record = json.loads((output / "release.json").read_text())
            self.assertEqual(record["macos_signing"], "developer-id-notarized")
            self.assertIn("notarized by Apple", (output / "release-notes.md").read_text())


if __name__ == "__main__":
    unittest.main()
