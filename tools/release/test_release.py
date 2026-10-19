import argparse
import contextlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

import release


class ReleasePackaging(unittest.TestCase):
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
                members = [("sakura", binary, 0o755), (release.NOTICES, b"notices", 0o644)]
                members += [(name, (release.ROOT / name).read_bytes(), 0o644) for name in release.LICENSES]
                release.write_tar(archives / f"{top}.tar.gz", top, members, 0)
            args = argparse.Namespace(commit="0" * 40, archives=archives, out=tmp / "out", macos_signed=True)
            with contextlib.redirect_stdout(io.StringIO()):
                release.assemble(args)
            output = tmp / "out"
            for target in release.TARGETS:
                if target["os"] != "darwin":
                    continue
                archive_name = f"sakuralang-{version}-{target['target']}.tar.gz"
                self.assertEqual((output / archive_name).read_bytes(), (archives / archive_name).read_bytes())
                self.assertIn(release.sha256(output / archive_name), (output / "sakuralang.rb").read_text())
                with tarfile.open(output / f"{target['npm']}-{version}.tgz") as package:
                    self.assertEqual(package.extractfile("package/bin/sakura").read(), binaries[target["target"]])
            record = json.loads((output / "release.json").read_text())
            self.assertEqual(record["macos_signing"], "developer-id-notarized")
            self.assertIn("notarized by Apple", (output / "release-notes.md").read_text())


if __name__ == "__main__":
    unittest.main()
