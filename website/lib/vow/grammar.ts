import type { LanguageRegistration } from "shiki";

/**
 * TextMate grammar for Vow. Shared by the docs (rehype-code) and the
 * landing page (codeToTokens), so ```vow blocks highlight identically.
 */
export const vowGrammar: LanguageRegistration = {
  name: "vow",
  displayName: "Vow",
  scopeName: "source.vow",
  aliases: ["vw"],
  fileTypes: ["vow"],
  patterns: [
    { include: "#comments" },
    { include: "#effect-row" },
    { include: "#declarations" },
    { include: "#strings" },
    { include: "#keywords" },
    { include: "#holes" },
    { include: "#literals" },
    { include: "#attributes" },
    { include: "#calls" },
    { include: "#types" },
    { include: "#operators" },
    { include: "#punctuation" },
  ],
  repository: {
    comments: {
      patterns: [
        {
          name: "comment.line.documentation.vow",
          match: "///.*$",
        },
        {
          name: "comment.line.double-slash.vow",
          match: "//.*$",
        },
        {
          name: "comment.block.vow",
          begin: "/\\*",
          end: "\\*/",
        },
      ],
    },
    // `! {Db, Clock, Fail<SignupError>}` — the effect row of a signature.
    "effect-row": {
      begin: "(!)\\s*(\\{)",
      beginCaptures: {
        1: { name: "keyword.operator.effect.vow" },
        2: { name: "punctuation.definition.effect-row.begin.vow" },
      },
      end: "\\}",
      endCaptures: {
        0: { name: "punctuation.definition.effect-row.end.vow" },
      },
      patterns: [
        {
          match: "\\b(Fail)\\s*(<)",
          captures: {
            1: { name: "support.type.effect.fail.vow" },
            2: { name: "punctuation.definition.generic.vow" },
          },
        },
        {
          name: "support.type.effect.vow",
          match: "\\b[A-Z][A-Za-z0-9_]*\\b",
        },
        { include: "#punctuation" },
      ],
    },
    declarations: {
      patterns: [
        {
          match: "\\b(fn)\\s+([a-z_][A-Za-z0-9_]*)",
          captures: {
            1: { name: "storage.type.function.vow" },
            2: { name: "entity.name.function.vow" },
          },
        },
        {
          match: "\\b(type|effect)\\s+([A-Z][A-Za-z0-9_]*)",
          captures: {
            1: { name: "storage.type.vow" },
            2: { name: "entity.name.type.vow" },
          },
        },
        {
          match: "\\b(let)\\s+([a-z_][A-Za-z0-9_]*)",
          captures: {
            1: { name: "storage.type.let.vow" },
            2: { name: "variable.other.declaration.vow" },
          },
        },
        {
          match: "\\b(test|property)\\s+(?=\")",
          captures: {
            1: { name: "keyword.other.test.vow" },
          },
        },
      ],
    },
    strings: {
      patterns: [
        {
          // Tagged literals: sql"…", html"…" are parsed and checked.
          begin: "\\b([a-z][a-z0-9_]*)(\")",
          beginCaptures: {
            1: { name: "entity.name.function.tagged.vow" },
            2: { name: "punctuation.definition.string.begin.vow" },
          },
          end: "\"",
          endCaptures: {
            0: { name: "punctuation.definition.string.end.vow" },
          },
          name: "string.quoted.tagged.vow",
          patterns: [{ include: "#string-escape" }, { include: "#interpolation" }],
        },
        {
          begin: "\"",
          beginCaptures: {
            0: { name: "punctuation.definition.string.begin.vow" },
          },
          end: "\"",
          endCaptures: {
            0: { name: "punctuation.definition.string.end.vow" },
          },
          name: "string.quoted.double.vow",
          patterns: [{ include: "#string-escape" }, { include: "#interpolation" }],
        },
      ],
    },
    "string-escape": {
      name: "constant.character.escape.vow",
      match: "\\\\(?:[\\\\\"nrt{}0]|u\\{[0-9A-Fa-f]{1,6}\\})",
    },
    interpolation: {
      begin: "\\{",
      beginCaptures: {
        0: { name: "punctuation.section.interpolation.begin.vow" },
      },
      end: "\\}",
      endCaptures: {
        0: { name: "punctuation.section.interpolation.end.vow" },
      },
      contentName: "meta.interpolation.vow",
      patterns: [{ include: "$self" }],
    },
    keywords: {
      patterns: [
        {
          name: "keyword.control.vow",
          match: "\\b(if|else|match|return|fail|for|in|resume)\\b",
        },
        {
          name: "keyword.other.effect.vow",
          match: "\\b(with|handle|effect)\\b",
        },
        {
          name: "keyword.other.contract.vow",
          match: "\\b(requires|ensures|where|expect)\\b",
        },
        {
          name: "keyword.other.vow",
          match: "\\b(fn|let|type|test|property|import|pub|as)\\b",
        },
        {
          name: "keyword.operator.word.vow",
          match: "\\b(and|or|not)\\b",
        },
        {
          name: "variable.language.vow",
          match: "\\b(it|result|self)\\b",
        },
      ],
    },
    holes: {
      // Typed holes: the compiler reports the expected type at `todo()`.
      name: "keyword.other.hole.vow",
      match: "\\btodo\\b",
    },
    literals: {
      patterns: [
        {
          name: "constant.language.boolean.vow",
          match: "\\b(true|false)\\b",
        },
        {
          name: "constant.numeric.hex.vow",
          match: "\\b0x[0-9A-Fa-f_]+\\b",
        },
        {
          name: "constant.numeric.vow",
          match: "\\b[0-9][0-9_]*(?:\\.[0-9][0-9_]*)?(?:[eE][+-]?[0-9]+)?\\b",
        },
      ],
    },
    attributes: {
      match: "(@)([a-z_][A-Za-z0-9_]*)",
      captures: {
        1: { name: "punctuation.definition.attribute.vow" },
        2: { name: "entity.other.attribute-name.vow" },
      },
    },
    calls: {
      patterns: [
        {
          // Module.op( — namespaced effect/stdlib calls
          match: "\\b([A-Z][A-Za-z0-9_]*)(\\.)([a-z_][A-Za-z0-9_]*)(?=\\s*(?:<[^>]*>)?\\()",
          captures: {
            1: { name: "entity.name.type.module.vow" },
            2: { name: "punctuation.accessor.vow" },
            3: { name: "entity.name.function.call.vow" },
          },
        },
        {
          match: "(?<=\\.)([a-z_][A-Za-z0-9_]*)(?=\\s*(?:<[^>]*>)?\\()",
          captures: {
            1: { name: "entity.name.function.method.vow" },
          },
        },
        {
          match: "\\b([a-z_][A-Za-z0-9_]*)(?=\\s*(?:<[^>]*>)?\\()",
          captures: {
            1: { name: "entity.name.function.call.vow" },
          },
        },
      ],
    },
    types: {
      name: "entity.name.type.vow",
      match: "\\b[A-Z][A-Za-z0-9_]*\\b",
    },
    operators: {
      patterns: [
        {
          name: "keyword.operator.arrow.vow",
          match: "->|=>",
        },
        {
          name: "keyword.operator.range.vow",
          match: "\\.\\.=?",
        },
        {
          name: "keyword.operator.pipe.vow",
          match: "\\|>",
        },
        {
          name: "keyword.operator.comparison.vow",
          match: "==|!=|<=|>=",
        },
        {
          name: "keyword.operator.vow",
          match: "[+\\-*/%=<>|!?]",
        },
      ],
    },
    punctuation: {
      patterns: [
        { name: "punctuation.separator.vow", match: "[,:;]" },
        { name: "punctuation.accessor.vow", match: "\\." },
        { name: "punctuation.brackets.vow", match: "[()\\[\\]{}]" },
      ],
    },
  },
};
