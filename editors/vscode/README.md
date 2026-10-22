# Sakura Lang for VS Code

The extension is experimental; the compiler it wraps is stable from 1.0. A thin client over the compiler's language server, `sakura lsp`: everything it shows
comes from the compiler you have installed.

- Highlighting for `.sak` files (the grammar the website uses), comments, brackets and
  auto-closing pairs.
- Diagnostics as you type; each error code links to its page,
  `https://www.sakuralang.com/docs/errors/<code>`.
- Hover, go to definition and completion.
- Formatting (`sakura fmt`'s output), on save by default.
- Quick fixes with their impact class in the title, e.g. ``Use `List` (impact: local)``. A fix
  computed before the file last changed is refused instead of applied to the wrong text.
- **Sakura: Show Hole Context** (command palette or editor context menu): the typed hole at the
  cursor, its expected type, effects, locals and candidates, as JSON beside the editor.

## Requirements

A `sakura` executable: build it from source as in the
[quickstart](https://www.sakuralang.com/docs/quickstart#install-from-source). The extension uses
the `sakura.path` setting when it is set, else the first `sakura` on `PATH`. Without one it says
so and links the install instructions; set the path and the server starts.

Each project (the nearest directory above a file holding `sakura.toml`) gets its own server, so a
workspace with several projects analyses each separately. A file outside any project uses its
workspace folder.

## Settings

| Setting | Default | Meaning |
|---|---|---|
| `sakura.path` | `""` | The `sakura` executable. Empty: `PATH`. `~/` is your home directory; a relative path is taken from the first workspace folder. |

The extension also defaults `editor.formatOnSave` to `true` for Sakura files; turn it off under
`[sakura]` in your settings.

## Development

From `editors/vscode/` with [Bun](https://bun.sh):

```sh
bun install --frozen-lockfile
bun run fmt:check && bun run lint && bun run typecheck
bun run test                  # unit tests and the grammar drift check
SAKURA_BIN=../../compiler/target/debug/sakura bun run test:vscode   # VS Code integration tests
bun run package               # a local .vsix; install with `code --install-extension`
```

`syntaxes/sakura.tmLanguage.json` is generated from `website/lib/sakura/grammar.ts` by
`bun run grammar`; `bun run test` fails when it is stale. `test:vscode` downloads VS Code
(set `VSCODE_VERSION` to pin one) and runs `test/suite.ts` in it against a real `sakura lsp`.

Publishing to the VS Code Marketplace and Open VSX (publisher `sakuralang`) is done by the
maintainer only; see `docs/release.md`.

## Licence

MIT or Apache-2.0, at your option, as the rest of the repository.
