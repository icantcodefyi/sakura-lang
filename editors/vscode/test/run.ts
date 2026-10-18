// Starts VS Code (downloaded by @vscode/test-electron) with this extension on a scratch
// workspace of two projects and runs test/suite.ts in it. Needs `SAKURA_BIN`, a built `sakura`.

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { runTests } from "@vscode/test-electron";

const bin = process.env.SAKURA_BIN;
if (!bin) {
  console.error("SAKURA_BIN must name a built `sakura` (e.g. compiler/target/debug/sakura).");
  process.exit(2);
}
const tmp = mkdtempSync(join(tmpdir(), "sakura-vscode-it-"));
const ws = join(tmp, "ws");
const manifest = (name: string) =>
  `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2026"\n\n[permissions]\n`;
const files: Record<string, string> = {
  ".vscode/settings.json": JSON.stringify({ "sakura.path": resolve(bin) }),
  "a/sakura.toml": manifest("a"),
  "a/src/main.sakura": "fn double(x: Int) -> Int { x * true }\n",
  "a/src/bad.sakura": 'fn bad() -> Int { "no" }\n',
  "a/src/holes.sakura": "fn label(n: Int) -> Int { ?rest }\n",
  "a/src/fix.sakura": "fn count() -> Int { list.len([1, 2]) }\n",
  "a/src/fmt.sakura": "fn double(x: Int)->Int{x*2}\n",
  "b/sakura.toml": manifest("b"),
  "b/src/bad.sakura": 'fn bad() -> Int { "no" }\n',
};
for (const [path, text] of Object.entries(files)) {
  mkdirSync(join(ws, path, ".."), { recursive: true });
  writeFileSync(join(ws, path), text);
}

const root = resolve(import.meta.dir, "..");
// Set when this runs under an Electron host (an editor's terminal); VS Code would start as Node.
delete process.env.ELECTRON_RUN_AS_NODE;
try {
  await runTests({
    version: process.env.VSCODE_VERSION ?? "stable",
    extensionDevelopmentPath: root,
    extensionTestsPath: join(root, "dist/test/suite.js"),
    launchArgs: [ws, "--disable-extensions", "--user-data-dir", join(tmp, "user")],
    extensionTestsEnv: { SAKURA_TEST_WS: ws, SAKURA_BIN: resolve(bin) },
  });
} catch (e) {
  console.error(e);
  process.exitCode = 1;
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
