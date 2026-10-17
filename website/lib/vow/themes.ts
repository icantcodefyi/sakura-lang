import type { ThemeRegistration } from "shiki";

/**
 * Two Shiki themes built around one idea: effects are the loudest thing in
 * the code. Effect rows and handlers carry the brand saffron, failures are
 * coral, contracts are mint. Everything else stays quiet.
 */
type Palette = {
  bg: string;
  fg: string;
  punct: string;
  comment: string;
  keyword: string;
  fnDecl: string;
  call: string;
  module: string;
  type: string;
  string: string;
  number: string;
  effect: string;
  fail: string;
  contract: string;
  selection: string;
  lineHighlight: string;
};

function makeTheme(name: string, type: "light" | "dark", p: Palette): ThemeRegistration {
  return {
    name,
    type,
    colors: {
      "editor.background": p.bg,
      "editor.foreground": p.fg,
      "editor.selectionBackground": p.selection,
      "editor.lineHighlightBackground": p.lineHighlight,
    },
    fg: p.fg,
    bg: p.bg,
    tokenColors: [
      { scope: ["comment", "punctuation.definition.comment"], settings: { foreground: p.comment, fontStyle: "italic" } },
      { scope: ["comment.line.documentation"], settings: { foreground: p.comment, fontStyle: "" } },
      { scope: ["punctuation", "meta.brace", "punctuation.separator", "punctuation.accessor"], settings: { foreground: p.punct } },
      {
        scope: ["keyword", "storage", "storage.type", "storage.modifier", "keyword.operator.word", "keyword.control"],
        settings: { foreground: p.keyword },
      },
      { scope: ["keyword.operator"], settings: { foreground: p.punct } },
      { scope: ["keyword.operator.arrow", "keyword.operator.range", "keyword.operator.pipe"], settings: { foreground: p.keyword } },
      {
        scope: ["keyword.other.effect", "keyword.operator.effect", "support.type.effect", "punctuation.definition.effect-row"],
        settings: { foreground: p.effect },
      },
      { scope: ["support.type.effect.fail"], settings: { foreground: p.fail } },
      { scope: ["keyword.other.contract", "keyword.other.test"], settings: { foreground: p.contract } },
      { scope: ["keyword.other.hole"], settings: { foreground: p.fail, fontStyle: "italic underline" } },
      { scope: ["variable.language"], settings: { foreground: p.contract, fontStyle: "italic" } },
      {
        scope: ["entity.name.function", "meta.definition.function entity.name.function", "support.function"],
        settings: { foreground: p.fnDecl },
      },
      {
        scope: ["entity.name.function.call", "entity.name.function.method", "meta.function-call", "entity.name.function.tagged"],
        settings: { foreground: p.call },
      },
      { scope: ["entity.name.type.module"], settings: { foreground: p.module } },
      {
        scope: ["entity.name.type", "entity.name.class", "support.type", "support.class", "storage.type.primitive", "entity.other.inherited-class"],
        settings: { foreground: p.type },
      },
      { scope: ["string", "string.quoted", "markup.inline.raw"], settings: { foreground: p.string } },
      { scope: ["constant.character.escape", "punctuation.section.interpolation"], settings: { foreground: p.number } },
      { scope: ["meta.interpolation"], settings: { foreground: p.fg } },
      { scope: ["constant", "constant.numeric", "constant.language"], settings: { foreground: p.number } },
      { scope: ["entity.other.attribute-name", "punctuation.definition.attribute"], settings: { foreground: p.effect } },
      { scope: ["variable", "variable.other", "variable.parameter"], settings: { foreground: p.fg } },
      { scope: ["variable.other.declaration"], settings: { foreground: p.fnDecl } },
      { scope: ["support.type.property-name", "meta.object-literal.key", "entity.name.tag"], settings: { foreground: p.call } },
      { scope: ["markup.heading", "entity.name.section"], settings: { foreground: p.fnDecl, fontStyle: "bold" } },
      { scope: ["markup.bold"], settings: { fontStyle: "bold" } },
      { scope: ["markup.italic"], settings: { fontStyle: "italic" } },
      { scope: ["markup.inserted"], settings: { foreground: p.contract } },
      { scope: ["markup.deleted", "invalid"], settings: { foreground: p.fail } },
    ],
  };
}

export const vowDark = makeTheme("vow-dark", "dark", {
  bg: "#131518",
  fg: "#dde1e7",
  punct: "#8a919c",
  comment: "#737a86",
  keyword: "#9aa8bc",
  fnDecl: "#f5f6f8",
  call: "#cdd5e1",
  module: "#b3bfd1",
  type: "#e6d2ab",
  string: "#a9c799",
  number: "#eba27c",
  effect: "#f5b942",
  fail: "#ff8573",
  contract: "#7dd4c0",
  selection: "#f5b94233",
  lineHighlight: "#ffffff0a",
});

export const vowLight = makeTheme("vow-light", "light", {
  bg: "#f7f8fa",
  fg: "#23272e",
  punct: "#636a75",
  comment: "#6f7580",
  keyword: "#46556e",
  fnDecl: "#0f1216",
  call: "#2b3749",
  module: "#3a4860",
  type: "#74541a",
  string: "#3c6a2c",
  number: "#a94c22",
  effect: "#8f5a00",
  fail: "#b8321f",
  contract: "#0b6e5a",
  selection: "#d9a03a40",
  lineHighlight: "#0000000a",
});
