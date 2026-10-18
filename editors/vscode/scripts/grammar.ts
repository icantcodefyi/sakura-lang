// Writes syntaxes/sakura.tmLanguage.json from the website's highlighting grammar, the one
// source for both. `--check` fails instead of writing when the committed file is stale.

import { readFileSync, writeFileSync } from "node:fs";
import { sakuraGrammar } from "../../../website/lib/sakura/grammar";

const out = new URL("../syntaxes/sakura.tmLanguage.json", import.meta.url);
// `displayName` and `aliases` are Shiki's; a TextMate grammar's `name` is its display name.
const { displayName, aliases: _, name: __, ...rest } = sakuraGrammar;
const json = `${JSON.stringify({ name: displayName, ...rest }, null, 2)}\n`;

if (process.argv.includes("--check")) {
  if (readFileSync(out, "utf8") !== json) {
    console.error("syntaxes/sakura.tmLanguage.json is stale: run `bun run grammar`.");
    process.exit(1);
  }
} else {
  writeFileSync(out, json);
}
