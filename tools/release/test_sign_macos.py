import base64
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("sign-macos.sh")
IDENTITY = "Developer ID Application: Aniruddh Dubge (P62B65A837)"
STUB = '''#!/usr/bin/env python3
import json, os, pathlib, sys
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["CALLS"], "a") as out:
    out.write(json.dumps([name, args]) + "\\n")
if name == "openssl":
    print("derived-keychain-password")
elif name == "security":
    if args[0] == "create-keychain":
        pathlib.Path(args[-1]).touch()
    elif args[0] == "delete-keychain":
        pathlib.Path(args[-1]).unlink()
    elif args[0] == "list-keychains":
        state = pathlib.Path(os.environ["SEARCH"])
        if "-s" in args:
            state.write_text("".join('    "' + item + '"\\n' for item in args[args.index("-s") + 1:]))
        else:
            print(state.read_text(), end="")
    elif args[0] == "find-identity":
        identity = os.environ["TEST_IDENTITY"]
        print('1) 1234567890 "' + identity + '"')
    else:
        print("PRIVATE-DIAGNOSTIC")
elif name == "codesign":
    if "--sign" in args:
        first = pathlib.Path(os.environ["SEARCH"]).read_text().splitlines()[0].strip()
        assert first == '"' + args[args.index("--keychain") + 1] + '"', first
    if os.environ.get("FAIL_SIGN") and "--sign" in args:
        print(os.environ.get("SIGN_DIAGNOSTIC", ""), file=sys.stderr)
        print("PRIVATE-DIAGNOSTIC: errSecInternalComponent", file=sys.stderr)
        sys.exit(1)
    if os.environ.get("FAIL_VERIFY") and "--verify" in args:
        sys.exit(1)
    if "--sign" in args:
        binary = pathlib.Path(args[-1])
        binary.write_bytes(binary.read_bytes() + b"SIGNED")
elif name == "ditto":
    pathlib.Path(args[-1]).write_bytes(b"zip")
elif name == "xcrun":
    if os.environ.get("FAIL_NOTARY"):
        print("PRIVATE-DIAGNOSTIC", file=sys.stderr)
        sys.exit(1)
    key = pathlib.Path(args[args.index("--key") + 1])
    assert key.read_text() == os.environ["APPLE_API_KEY_P8"] + "\\n"
    assert key.stat().st_mode & 0o777 == 0o600
    assert str(key).startswith(os.environ["RUNNER_TEMP"])
    print("PRIVATE-DIAGNOSTIC", file=sys.stderr)
    print(json.dumps({"status": os.environ.get("NOTARY_STATUS", "Accepted")}))
'''


class Signing(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="signing test ")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        tools = self.root / "tools"
        tools.mkdir()
        for name in ("security", "codesign", "openssl", "ditto", "xcrun"):
            command = tools / name
            command.write_text(STUB)
            command.chmod(0o755)
        self.binary = self.root / "sakura"
        self.binary.write_bytes(b"MACHO")
        self.search = '    "/Users/runner/Library/Keychains/login.keychain-db"\n    "/Library/Keychains/My Team.keychain"\n'
        (self.root / "search").write_text(self.search)
        self.env = dict(os.environ, PATH=f"{tools}:{os.environ['PATH']}",
                        RUNNER_TEMP=str(self.root), CALLS=str(self.root / "calls"), SEARCH=str(self.root / "search"),
                        GITHUB_STEP_SUMMARY=str(self.root / "summary"),
                        MACOS_CERT_P12_BASE64=base64.b64encode(b"certificate").decode(),
                        MACOS_CERT_PASSWORD="certificate-password", APPLE_API_KEY_P8="PRIVATE-KEY\nPRIVATE-BODY",
                        APPLE_API_KEY_ID="KEYID", APPLE_API_ISSUER_ID="ISSUERID", TEST_IDENTITY=IDENTITY)

    def run_signing(self, **changes):
        result = subprocess.run(["bash", str(SCRIPT), str(self.binary)],
                                env=dict(self.env, **changes), capture_output=True, text=True, timeout=20)
        self.assertFalse((self.root / "sakura-signing").exists(), "temporary credentials survived")
        self.assertEqual((self.root / "search").read_text(), self.search, "search list not restored")
        public = "\n".join(line for line in result.stdout.splitlines() if not line.startswith("::add-mask::")) + result.stderr
        for value in ("PRIVATE-DIAGNOSTIC", "derived-keychain-password", "certificate-password", "PRIVATE-KEY", "PRIVATE-BODY", "KEYID", "ISSUERID"):
            self.assertNotIn(value, public)
        return result

    def calls(self):
        return [json.loads(line) for line in (self.root / "calls").read_text().splitlines()]

    def test_signs_with_only_the_pinned_identity_and_cleans_up(self):
        result = self.run_signing()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.binary.read_bytes(), b"MACHOSIGNED")
        calls = self.calls()
        sign = next(args for name, args in calls if name == "codesign" and "--sign" in args)
        self.assertEqual(sign[sign.index("--sign") + 1], IDENTITY)
        self.assertEqual(sign[:5], ["--force", "--options", "runtime", "--timestamp", "--keychain"])
        self.assertTrue(sign[sign.index("--keychain") + 1].startswith(str(self.root)))
        notary = next(args for name, args in calls if name == "xcrun")
        self.assertEqual(notary[:2], ["notarytool", "submit"])
        self.assertIn("--wait", notary)
        self.assertTrue(any(name == "codesign" and "--check-notarization" in args for name, args in calls))
        imports = [args[1] for name, args in calls if name == "security" and args[0] == "import"]
        self.assertTrue(imports[1].endswith("DeveloperIDG2CA.cer"))
        self.assertEqual(calls[-1][0:1], ["security"])
        self.assertEqual(calls[-1][1][0], "delete-keychain")
        self.assertIn("stapled ticket", (self.root / "summary").read_text())
        self.assertIn("::add-mask::derived-keychain-password", result.stdout)
        self.assertIn("::add-mask::PRIVATE-BODY", result.stdout)

    def test_refuses_any_other_identity_before_signing(self):
        result = self.run_signing(TEST_IDENTITY="Developer ID Application: L8 CONES (WRONGTEAM)")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(name in ("codesign", "xcrun") for name, _ in self.calls()))

    def test_signing_failure_removes_credentials(self):
        result = self.run_signing(FAIL_SIGN="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("::error::Reason: errSecInternalComponent", result.stdout)
        self.assertFalse(any(name == "xcrun" for name, _ in self.calls()))

    def test_signing_failure_does_not_echo_unknown_error_identifiers(self):
        result = self.run_signing(FAIL_SIGN="1", SIGN_DIAGNOSTIC="errSecUnlistedPrivateMaterial")
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("UnlistedPrivateMaterial", result.stdout + result.stderr)

    def test_signing_failure_reports_fixed_known_reasons(self):
        for reason in ("unable to build chain", "no identity found", "ambiguous",
                       "timestamp service is not available", "User interaction is not allowed",
                       "The specified item could not be found in the keychain", "CSSMERR_TP_CERT_EXPIRED"):
            with self.subTest(reason=reason):
                result = self.run_signing(FAIL_SIGN="1", SIGN_DIAGNOSTIC=reason + " PRIVATE-KEY")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("::error::Reason: " + reason, result.stdout)

    def test_notarization_transport_failure_removes_credentials(self):
        self.assertNotEqual(self.run_signing(FAIL_NOTARY="1").returncode, 0)

    def test_rejected_notarization_removes_credentials(self):
        self.assertNotEqual(self.run_signing(NOTARY_STATUS="Invalid").returncode, 0)

    def test_verification_failure_is_fatal(self):
        self.assertNotEqual(self.run_signing(FAIL_VERIFY="1").returncode, 0)

    def test_missing_secret_fails_before_import(self):
        self.assertNotEqual(self.run_signing(APPLE_API_KEY_P8="").returncode, 0)
        self.assertFalse((self.root / "calls").exists())


if __name__ == "__main__":
    unittest.main()
