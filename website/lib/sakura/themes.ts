import type { ThemeRegistration } from "shiki";

/**
 * Two Shiki themes built around one idea: effects are the loudest thing in
 * the code. Effect rows and handlers carry the brand sakura, failures are
 * coral, contracts are mint, types are sky. Everything else is ink and stays
 * quiet. Every colour is at least 4.5:1 on its background (design/BRAND.md).
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
      {
        scope: ["comment", "punctuation.definition.comment"],
        settings: { foreground: p.comment, fontStyle: "italic" },
      },
      { scope: ["comment.line.documentation"], settings: { foreground: p.comment, fontStyle: "" } },
      {
        scope: ["punctuation", "meta.brace", "punctuation.separator", "punctuation.accessor"],
        settings: { foreground: p.punct },
      },
      {
        scope: [
          "keyword",
          "storage",
          "storage.type",
          "storage.modifier",
          "keyword.operator.word",
          "keyword.control",
        ],
        settings: { foreground: p.keyword },
      },
      { scope: ["keyword.operator"], settings: { foreground: p.punct } },
      {
        scope: ["keyword.operator.arrow", "keyword.operator.range", "keyword.operator.pipe"],
        settings: { foreground: p.keyword },
      },
      {
        scope: [
          "keyword.other.effect",
          "keyword.operator.effect",
          "support.type.effect",
          "punctuation.definition.effect-row",
        ],
        settings: { foreground: p.effect },
      },
      { scope: ["support.type.effect.fail"], settings: { foreground: p.fail } },
      {
        scope: ["keyword.other.contract", "keyword.other.test"],
        settings: { foreground: p.contract },
      },
      {
        scope: ["keyword.other.hole"],
        settings: { foreground: p.fail, fontStyle: "italic underline" },
      },
      { scope: ["variable.language"], settings: { foreground: p.contract, fontStyle: "italic" } },
      {
        scope: [
          "entity.name.function",
          "meta.definition.function entity.name.function",
          "support.function",
        ],
        settings: { foreground: p.fnDecl },
      },
      {
        scope: [
          "entity.name.function.call",
          "entity.name.function.method",
          "meta.function-call",
          "entity.name.function.tagged",
        ],
        settings: { foreground: p.call },
      },
      { scope: ["entity.name.type.module"], settings: { foreground: p.module } },
      {
        scope: [
          "entity.name.type",
          "entity.name.class",
          "support.type",
          "support.class",
          "storage.type.primitive",
          "entity.other.inherited-class",
        ],
        settings: { foreground: p.type },
      },
      {
        scope: ["string", "string.quoted", "markup.inline.raw"],
        settings: { foreground: p.string },
      },
      {
        scope: ["constant.character.escape", "punctuation.section.interpolation"],
        settings: { foreground: p.number },
      },
      { scope: ["meta.interpolation"], settings: { foreground: p.fg } },
      {
        scope: ["constant", "constant.numeric", "constant.language"],
        settings: { foreground: p.number },
      },
      {
        scope: ["entity.other.attribute-name", "punctuation.definition.attribute"],
        settings: { foreground: p.effect },
      },
      {
        scope: ["variable", "variable.other", "variable.parameter"],
        settings: { foreground: p.fg },
      },
      { scope: ["variable.other.declaration"], settings: { foreground: p.fnDecl } },
      {
        scope: ["support.type.property-name", "meta.object-literal.key", "entity.name.tag"],
        settings: { foreground: p.call },
      },
      {
        scope: ["markup.heading", "entity.name.section"],
        settings: { foreground: p.fnDecl, fontStyle: "bold" },
      },
      { scope: ["markup.bold"], settings: { fontStyle: "bold" } },
      { scope: ["markup.italic"], settings: { fontStyle: "italic" } },
      { scope: ["markup.inserted"], settings: { foreground: p.contract } },
      { scope: ["markup.deleted", "invalid"], settings: { foreground: p.fail } },
    ],
  };
}

export const sakuraDark = makeTheme("sakura-dark", "dark", {
  bg: "#161616",
  fg: "#e6e2e4",
  punct: "#9a9397",
  comment: "#938d91",
  keyword: "#b9b0ba",
  fnDecl: "#fafafa",
  call: "#d9d3d7",
  module: "#c8bfc9",
  type: "#9fc3e7",
  string: "#e3a6d4",
  number: "#e3a6d4",
  effect: "#f386a1",
  fail: "#ff8a78",
  contract: "#7fd3b8",
  selection: "#f386a133",
  lineHighlight: "#ffffff0a",
});

export const sakuraLight = makeTheme("sakura-light", "light", {
  bg: "#f8f6f6",
  fg: "#1e1e1e",
  punct: "#655f63",
  comment: "#68636a",
  keyword: "#544c58",
  fnDecl: "#141414",
  call: "#363034",
  module: "#4a4250",
  type: "#2a5b8c",
  string: "#8a3f7c",
  number: "#8a3f7c",
  effect: "#b4245f",
  fail: "#b8321f",
  contract: "#0b6e5a",
  selection: "#f386a140",
  lineHighlight: "#0000000a",
});
