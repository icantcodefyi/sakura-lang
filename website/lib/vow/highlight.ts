import "server-only";
import { createHighlighter, type BundledLanguage, type Highlighter } from "shiki";
import { vowGrammar } from "./grammar";
import { vowDark, vowLight } from "./themes";

import type { TokenLine } from "@/components/token-spans";

let highlighter: Promise<Highlighter> | undefined;

function getHighlighter() {
  highlighter ??= createHighlighter({
    themes: [vowLight, vowDark],
    langs: [vowGrammar, "json", "jsonc", "toml", "bash", "diff", "md"],
  });
  return highlighter;
}

/** Tokenize code into lines of dual-theme tokens (rendered via CSS vars). */
export async function tokenize(code: string, lang = "vow"): Promise<TokenLine[]> {
  "use cache";
  const hl = await getHighlighter();
  const { tokens } = hl.codeToTokens(code, {
    lang: lang as BundledLanguage,
    themes: { light: "vow-light", dark: "vow-dark" },
    defaultColor: false,
  });
  return tokens.map((line) =>
    line.map((t) => ({
      text: t.content,
      light: t.htmlStyle?.["--shiki-light"] ?? "inherit",
      dark: t.htmlStyle?.["--shiki-dark"] ?? "inherit",
      italic: t.htmlStyle?.["--shiki-light-font-style"] === "italic" || undefined,
    })),
  );
}
