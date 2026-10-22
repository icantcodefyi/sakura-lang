import "server-only";
import { createHighlighter, type BundledLanguage, type Highlighter } from "shiki";
import { sakuraGrammar } from "./grammar";
import { sakuraDark, sakuraLight } from "./themes";

import { toTokenLines, type TokenLine } from "@/components/token-spans";

let highlighter: Promise<Highlighter> | undefined;

function getHighlighter() {
  highlighter ??= createHighlighter({
    themes: [sakuraLight, sakuraDark],
    langs: [sakuraGrammar, "json", "jsonc", "toml", "bash", "diff", "md"],
  });
  return highlighter;
}

/** Tokenize code into lines of dual-theme tokens (rendered via CSS vars). */
export async function tokenize(code: string, lang = "sakura"): Promise<TokenLine[]> {
  "use cache";
  const hl = await getHighlighter();
  const { tokens } = hl.codeToTokens(code, {
    lang: lang as BundledLanguage,
    themes: { light: "sakura-light", dark: "sakura-dark" },
    defaultColor: false,
  });
  return toTokenLines(tokens);
}
