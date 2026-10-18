// Runs inside VS Code (see test/run.ts) against a real `sakura lsp`.

import { strict as assert } from "node:assert";
import { join } from "node:path";
import * as vscode from "vscode";
import type { SakuraApi } from "../src/extension";

const ws = process.env.SAKURA_TEST_WS!;
const a = join(ws, "a");
const uri = (path: string) => vscode.Uri.file(join(ws, path));

async function waitFor<T>(
  what: string,
  f: () => T | undefined | false | Promise<T | undefined | false>,
  ms = 20_000,
) {
  const end = Date.now() + ms;
  for (;;) {
    const v = await f();
    if (v) return v;
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`);
    await new Promise((r) => setTimeout(r, 50));
  }
}

const diagnostics = (path: string) =>
  waitFor(`diagnostics on ${path}`, () => {
    const d = vscode.languages.getDiagnostics(uri(path));
    return d.length > 0 && d;
  });

async function open(path: string) {
  const doc = await vscode.workspace.openTextDocument(uri(path));
  return vscode.window.showTextDocument(doc);
}

const tests: [string, (api: SakuraApi) => Promise<void>][] = [
  [
    "stdio start: diagnostics with their error-page link",
    async (api) => {
      await open("a/src/main.sak");
      await waitFor("a server", () => api.roots().length > 0);
      await api.ready(a);
      const [d] = await diagnostics("a/src/main.sak");
      assert.equal(d.source, "sakura");
      const code = d.code as { value: string; target: vscode.Uri };
      assert.match(code.value, /^[A-Z]\d{4}$/);
      assert.equal(
        code.target.toString(true),
        `https://www.sakuralang.com/docs/errors/${code.value}`,
      );
    },
  ],
  [
    "root selection: the nearest sakura.toml, not the workspace folder",
    async (api) => {
      assert.deepEqual(api.roots(), [a]);
      // a/src/bad.sak was never opened; the server found it under its root.
      await diagnostics("a/src/bad.sak");
      // b is another project: its files are not analysed by a's server.
      assert.equal(vscode.languages.getDiagnostics(uri("b/src/bad.sak")).length, 0);
    },
  ],
  [
    "UTF-16 positions after edits: hole context at the cursor",
    async () => {
      const editor = await open("a/src/holes.sak");
      await editor.edit((e) => e.insert(new vscode.Position(0, 0), "// é𝄞\n"));
      const line = editor.document.lineAt(1).text;
      const at = line.indexOf("{ ?rest");
      await editor.edit((e) => e.insert(new vscode.Position(1, at + 2), 'String.len("é𝄞") + '));
      const col = editor.document.lineAt(1).text.indexOf("?rest");
      editor.selection = new vscode.Selection(1, col + 2, 1, col + 2);
      const record = await vscode.commands.executeCommand("sakura.showHoleContext");
      assert.ok(record, "a hole record at the cursor");
      assert.match(JSON.stringify(record), /"rest"/);
      // One column before the hole there is none.
      await vscode.window.showTextDocument(editor.document);
      editor.selection = new vscode.Selection(1, col - 3, 1, col - 3);
      assert.equal(await vscode.commands.executeCommand("sakura.showHoleContext"), null);
    },
  ],
  [
    "quick fixes show their impact class and refuse to apply once stale",
    async () => {
      const editor = await open("a/src/fix.sak");
      const doc = editor.document;
      const fixDiagnostic = (line: number) =>
        waitFor(`quick-fix diagnostic on line ${line + 1}`, () =>
          vscode.languages
            .getDiagnostics(doc.uri)
            .find((d) => d.source === "sakura" && d.range.start.line === line),
        );
      await fixDiagnostic(0);
      // Only this extension's fixes (VS Code's built-in chat offers its own "Fix").
      const fix = () =>
        waitFor("a Sakura quick fix", async () => {
          const actions = await vscode.commands.executeCommand<vscode.CodeAction[]>(
            "vscode.executeCodeActionProvider",
            doc.uri,
            new vscode.Range(0, 0, doc.lineCount, 0),
          );
          return actions.find((a) => a.command?.command === "sakura.applyFix");
        });
      const run = (fix: vscode.CodeAction) =>
        vscode.commands.executeCommand(fix.command!.command, ...(fix.command!.arguments ?? []));
      const stale = await fix();
      assert.equal(stale.title, "Use `List` (impact: local)");
      await editor.edit((e) => e.insert(new vscode.Position(0, 0), "\n"));
      const before = doc.getText();
      assert.equal(await run(stale), false);
      assert.equal(doc.getText(), before);
      // The edit resolves before the LSP publishes its new analysis. Old diagnostics
      // still exist, so wait for the diagnostic to move with the inserted newline.
      await fixDiagnostic(1);
      const fresh = await fix();
      assert.equal(await run(fresh), true);
      assert.equal(doc.getText(), "\nfn count() -> Int { List.len([1, 2]) }\n");
    },
  ],
  [
    "format on save",
    async () => {
      const editor = await open("a/src/fmt.sak");
      await editor.edit((e) => e.insert(new vscode.Position(1, 0), "\n"));
      await editor.document.save();
      assert.ok(editor.document.getText().startsWith("fn double(x: Int) -> Int"));
    },
  ],
  [
    "missing executable: a clear error, then recovery",
    async (api) => {
      const config = vscode.workspace.getConfiguration("sakura");
      await config.update("path", "/nonexistent/sakura", vscode.ConfigurationTarget.Workspace);
      const error = await waitFor("the error", () => api.lastError());
      assert.match(error, /\/nonexistent\/sakura, which is not an executable file/);
      assert.deepEqual(api.roots(), []);
      await config.update("path", process.env.SAKURA_BIN, vscode.ConfigurationTarget.Workspace);
      await waitFor("a restarted server", () => api.roots().length === 1);
      assert.equal(api.lastError(), undefined);
    },
  ],
];

export async function run(): Promise<void> {
  const ext = vscode.extensions.getExtension<SakuraApi>("sakuralang.sakura")!;
  const api = await ext.activate();
  let failed = 0;
  for (const [name, test] of tests) {
    try {
      await Promise.race([
        test(api),
        new Promise((_, reject) => setTimeout(() => reject(new Error("timed out")), 60_000)),
      ]);
      console.log(`ok   ${name}`);
    } catch (e) {
      failed++;
      console.log(`FAIL ${name}\n${e instanceof Error ? e.stack : e}`);
    }
  }
  if (failed > 0) throw new Error(`${failed} of ${tests.length} tests failed`);
}
