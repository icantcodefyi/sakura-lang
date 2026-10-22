import { createHighlighterCore } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import { toTokenLines, type TokenLine } from "@/components/token-spans";
import { sakuraGrammar } from "@/lib/sakura/grammar";
import { sakuraDark, sakuraLight } from "@/lib/sakura/themes";

/**
 * The playground's highlighter: the site's TextMate grammar and themes on Shiki's core with its
 * JavaScript regex engine (no Oniguruma WASM). /play imports this module dynamically.
 */
const highlighter = createHighlighterCore({
  themes: [sakuraLight, sakuraDark],
  langs: [sakuraGrammar],
  engine: createJavaScriptRegexEngine(),
});

export async function loadTokenizer(): Promise<(code: string) => TokenLine[]> {
  const hl = await highlighter;
  return (code) =>
    toTokenLines(
      hl.codeToTokens(code, {
        lang: "sakura",
        themes: { light: "sakura-light", dark: "sakura-dark" },
        defaultColor: false,
      }).tokens,
    );
}
