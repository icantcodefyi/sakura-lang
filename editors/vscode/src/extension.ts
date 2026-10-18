// A thin client over `sakura lsp`: one server per project (the nearest `sakura.toml`), started
// from the `sakura.path` setting or `PATH`. Everything else comes from the server.

import { dirname } from "node:path";
import * as vscode from "vscode";
import { LanguageClient, type LanguageClientOptions } from "vscode-languageclient/node";
import { findExecutable, INSTALL_URL, projectRoot, titleWithImpact } from "./lib";

const clients = new Map<string, LanguageClient>();
let output: vscode.LogOutputChannel;
let lastError: string | undefined;

/** What the integration tests read; not a public API. */
export interface SakuraApi {
  roots(): string[];
  lastError(): string | undefined;
  ready(root: string): Promise<void>;
}

export function activate(context: vscode.ExtensionContext): SakuraApi {
  output = vscode.window.createOutputChannel("Sakura", { log: true });
  const starting = new Map<string, Promise<void>>();
  const open = (doc: vscode.TextDocument) => {
    if (doc.languageId !== "sakura" || doc.uri.scheme !== "file") return;
    const root = rootOf(doc.uri);
    if (!starting.has(root)) starting.set(root, start(root));
  };
  context.subscriptions.push(
    output,
    vscode.workspace.onDidOpenTextDocument(open),
    vscode.workspace.onDidChangeConfiguration(async (e) => {
      if (!e.affectsConfiguration("sakura.path")) return;
      await stopAll();
      starting.clear();
      lastError = undefined;
      vscode.workspace.textDocuments.forEach(open);
    }),
    vscode.commands.registerCommand("sakura.showHoleContext", showHoleContext),
    vscode.commands.registerCommand("sakura.applyFix", applyFix),
  );
  vscode.workspace.textDocuments.forEach(open);
  return {
    roots: () => [...clients.keys()],
    lastError: () => lastError,
    ready: (root) => starting.get(root) ?? Promise.reject(new Error(`no server for ${root}`)),
  };
}

export function deactivate(): Promise<void> {
  return stopAll();
}

/** The project root of a file: its nearest `sakura.toml`, else its workspace folder or directory. */
function rootOf(uri: vscode.Uri): string {
  return (
    projectRoot(uri.fsPath) ??
    vscode.workspace.getWorkspaceFolder(uri)?.uri.fsPath ??
    dirname(uri.fsPath)
  );
}

async function start(root: string): Promise<void> {
  const setting = vscode.workspace.getConfiguration("sakura").get<string>("path", "");
  const base = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? root;
  const exe = findExecutable(setting, process.env, base);
  if ("error" in exe) return fail(exe.error);
  const rootUri = vscode.Uri.file(root);
  const options: LanguageClientOptions = {
    documentSelector: [
      {
        scheme: "file",
        language: "sakura",
        pattern: { baseUri: rootUri.toString(), pattern: "**/*" },
      },
    ],
    // The server analyses the files under its first workspace folder, so that is the project.
    workspaceFolder: { uri: rootUri, name: root, index: 0 },
    outputChannel: output,
    middleware: {
      async provideCodeActions(document, range, context, token, next) {
        const version = document.version;
        const actions = await next(document, range, context, token);
        return actions?.map((a) => (isCommand(a) ? a : pinned(a, document.uri, version)));
      },
    },
  };
  const client = new LanguageClient(
    "sakura",
    "Sakura",
    // No `transport`: it would append `--stdio`, and `sakura lsp` always speaks stdio.
    { command: exe.path, args: ["lsp"], options: { cwd: root } },
    options,
  );
  clients.set(root, client);
  try {
    await client.start();
  } catch (e) {
    clients.delete(root);
    return fail(`Could not start \`${exe.path} lsp\`: ${e instanceof Error ? e.message : e}`);
  }
}

function isCommand(a: vscode.Command | vscode.CodeAction): a is vscode.Command {
  return typeof (a as vscode.Command).command === "string";
}

/**
 * Shows the fix's impact class and replaces its edit with `sakura.applyFix`, which refuses to
 * apply it once the document has changed since the fix was computed (VS Code's converted edit
 * drops the version the server pins it to).
 */
function pinned(action: vscode.CodeAction, uri: vscode.Uri, version: number): vscode.CodeAction {
  action.title = titleWithImpact(action.title, (action as { data?: unknown }).data);
  if (action.edit) {
    // Plain JSON, so the arguments survive the command round trip unchanged.
    const edits: FixEdit[] = action.edit.entries().flatMap(([file, edits]) =>
      edits.map((e) => ({
        uri: file.toString(),
        range: [
          e.range.start.line,
          e.range.start.character,
          e.range.end.line,
          e.range.end.character,
        ],
        newText: e.newText,
      })),
    );
    action.command = {
      title: action.title,
      command: "sakura.applyFix",
      arguments: [uri.toString(), version, edits],
    };
    action.edit = undefined;
  }
  return action;
}

type FixEdit = { uri: string; range: [number, number, number, number]; newText: string };

async function applyFix(uri: string, version: number, edits: FixEdit[]): Promise<boolean> {
  const doc = vscode.workspace.textDocuments.find((d) => d.uri.toString() === uri);
  if (doc && doc.version !== version) {
    vscode.window.showWarningMessage(
      "Sakura: this fix is stale (the file changed after it was computed). Ask for it again.",
    );
    return false;
  }
  const edit = new vscode.WorkspaceEdit();
  for (const e of edits)
    edit.replace(vscode.Uri.parse(e.uri), new vscode.Range(...e.range), e.newText);
  return vscode.workspace.applyEdit(edit);
}

async function showHoleContext(): Promise<unknown> {
  const editor = vscode.window.activeTextEditor;
  const client = editor && clients.get(rootOf(editor.document.uri));
  if (!editor || editor.document.languageId !== "sakura" || !client) {
    vscode.window.showInformationMessage("Sakura: open a .sak file of a running project.");
    return null;
  }
  const c2p = client.code2ProtocolConverter;
  const record = await client.sendRequest("sakura/holeContext", {
    textDocument: c2p.asTextDocumentIdentifier(editor.document),
    position: c2p.asPosition(editor.selection.active),
  });
  if (record === null) {
    vscode.window.showInformationMessage("Sakura: there is no typed hole at the cursor.");
    return null;
  }
  const content = JSON.stringify(record, null, 2);
  const doc = await vscode.workspace.openTextDocument({ language: "json", content });
  await vscode.window.showTextDocument(doc, {
    viewColumn: vscode.ViewColumn.Beside,
    preview: true,
  });
  return record;
}

function fail(message: string): void {
  lastError = message;
  output.error(message);
  vscode.window
    .showErrorMessage(`Sakura: ${message}`, "Install Sakura", "Open Settings")
    .then((choice) => {
      if (choice === "Install Sakura") vscode.env.openExternal(vscode.Uri.parse(INSTALL_URL));
      if (choice === "Open Settings")
        vscode.commands.executeCommand("workbench.action.openSettings", "sakura.path");
    });
}

async function stopAll(): Promise<void> {
  const all = [...clients.values()];
  clients.clear();
  await Promise.all(all.map((c) => c.stop().catch(() => undefined)));
}
