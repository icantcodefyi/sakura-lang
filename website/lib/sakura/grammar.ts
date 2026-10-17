import type { LanguageRegistration } from "shiki";

/**
 * TextMate grammar for Sakura Lang. Shared by the docs (rehype-code) and the
 * landing page (codeToTokens), so ```sakura blocks highlight identically.
 * Tracks docs/design.md §2 (lexical structure) and §3 (keywords, operators).
 */
export const sakuraGrammar: LanguageRegistration = {
  name: "sakura",
  displayName: "Sakura Lang",
  scopeName: "source.sakura",
  aliases: [],
  fileTypes: ["sakura"],
  patterns: [
    { include: "#comments" },
    { include: "#effect-row" },
    { include: "#effect-row-var" },
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
        { name: "comment.line.documentation.sakura", match: "///.*$" },
        { name: "comment.line.double-slash.sakura", match: "//.*$" },
        { include: "#block-comment" },
      ],
    },
    // Block comments nest (§2.1).
    "block-comment": {
      name: "comment.block.sakura",
      begin: "/\\*",
      end: "\\*/",
      patterns: [{ include: "#block-comment" }],
    },
    // `! {Db, Clock, Fail<SignupError> | e}`: the effect row of a signature or function type.
    "effect-row": {
      begin: "(!)\\s*(\\{)",
      beginCaptures: {
        1: { name: "keyword.operator.effect.sakura" },
        2: { name: "punctuation.definition.effect-row.begin.sakura" },
      },
      end: "\\}",
      endCaptures: {
        0: { name: "punctuation.definition.effect-row.end.sakura" },
      },
      patterns: [
        { include: "#fail-label" },
        {
          name: "support.type.effect.sakura",
          match: "\\b[A-Z][A-Za-z0-9_]*\\b",
        },
        { name: "variable.parameter.row.sakura", match: "\\b[a-z_][A-Za-z0-9_]*\\b" },
        { name: "keyword.operator.sakura", match: "\\|" },
        { include: "#punctuation" },
      ],
    },
    // `-> B ! e`: a row that is just an effect-row variable.
    "effect-row-var": {
      match: "(?<=\\s)(!)\\s+([a-z_][A-Za-z0-9_]*)\\b(?=\\s*(?:[,){]|$|requires\\b|ensures\\b))",
      captures: {
        1: { name: "keyword.operator.effect.sakura" },
        2: { name: "variable.parameter.row.sakura" },
      },
    },
    // `Fail<EmailTaken | Banned>`: the effect is highlighted, its error tags are types.
    "fail-label": {
      begin: "\\b(Fail)\\s*(<)",
      beginCaptures: {
        1: { name: "support.type.effect.fail.sakura" },
        2: { name: "punctuation.definition.generic.sakura" },
      },
      end: ">",
      endCaptures: { 0: { name: "punctuation.definition.generic.sakura" } },
      patterns: [
        { name: "entity.name.type.sakura", match: "\\b[A-Z][A-Za-z0-9_]*\\b" },
        { name: "keyword.operator.sakura", match: "\\|" },
        { include: "#punctuation" },
      ],
    },
    declarations: {
      patterns: [
        {
          match: "\\b(fn)\\s+([a-z_][A-Za-z0-9_]*)",
          captures: {
            1: { name: "storage.type.function.sakura" },
            2: { name: "entity.name.function.sakura" },
          },
        },
        {
          match: "\\b(type|effect|error)\\s+([A-Z][A-Za-z0-9_]*)",
          captures: {
            1: { name: "storage.type.sakura" },
            2: { name: "entity.name.type.sakura" },
          },
        },
        {
          match: "\\b(handler)\\s+([A-Z][A-Za-z0-9_]*)",
          captures: {
            1: { name: "keyword.other.effect.sakura" },
            2: { name: "support.type.effect.sakura" },
          },
        },
        {
          match: "\\b(let|var)\\s+([a-z_][A-Za-z0-9_]*)",
          captures: {
            1: { name: "storage.type.let.sakura" },
            2: { name: "variable.other.declaration.sakura" },
          },
        },
        {
          match: '\\b(test|property)\\s+(?=")',
          captures: {
            1: { name: "keyword.other.test.sakura" },
          },
        },
      ],
    },
    strings: {
      patterns: [
        {
          // Tagged templates (§4.11): sql"…", html"…". No whitespace before the quote.
          begin: '\\b([a-z_][A-Za-z0-9_]*)(")',
          beginCaptures: {
            1: { name: "entity.name.function.tagged.sakura" },
            2: { name: "punctuation.definition.string.begin.sakura" },
          },
          end: '"',
          endCaptures: {
            0: { name: "punctuation.definition.string.end.sakura" },
          },
          name: "string.quoted.tagged.sakura",
          patterns: [{ include: "#string-escape" }, { include: "#interpolation" }],
        },
        {
          begin: '"',
          beginCaptures: {
            0: { name: "punctuation.definition.string.begin.sakura" },
          },
          end: '"',
          endCaptures: {
            0: { name: "punctuation.definition.string.end.sakura" },
          },
          name: "string.quoted.double.sakura",
          patterns: [{ include: "#string-escape" }, { include: "#interpolation" }],
        },
      ],
    },
    // §2.3: \\ \" \n \r \t \0 \{ \} and \u{XXXXXX}.
    "string-escape": {
      name: "constant.character.escape.sakura",
      match: '\\\\(?:[\\\\"nrt0{}]|u\\{[0-9A-Fa-f]{1,6}\\})',
    },
    interpolation: {
      begin: "\\{",
      beginCaptures: {
        0: { name: "punctuation.section.interpolation.begin.sakura" },
      },
      end: "\\}",
      endCaptures: {
        0: { name: "punctuation.section.interpolation.end.sakura" },
      },
      contentName: "meta.interpolation.sakura",
      patterns: [{ include: "#nested-braces" }, { include: "$self" }],
    },
    // Braces inside an interpolation nest, so `"{ f({a: 1}) }"` closes at the right `}`.
    "nested-braces": {
      begin: "\\{",
      beginCaptures: { 0: { name: "punctuation.brackets.sakura" } },
      end: "\\}",
      endCaptures: { 0: { name: "punctuation.brackets.sakura" } },
      patterns: [{ include: "#nested-braces" }, { include: "$self" }],
    },
    // §2.1 keywords, grouped by role. Reserved words (`V0007`) are marked invalid.
    // A keyword directly after `.` or `/` is a member or module segment (`Task.spawn`,
    // `std/test`), and a reserved word directly after `fn` is an operation name.
    keywords: {
      patterns: [
        {
          name: "keyword.control.sakura",
          match: "(?<![./])\\b(if|else|match|for|in|while|return|break|continue|fail|try|catch)\\b",
        },
        {
          name: "keyword.other.effect.sakura",
          match: "(?<![./])\\b(with|handler|effect)\\b",
        },
        {
          name: "keyword.other.contract.sakura",
          match: "(?<![./])\\b(requires|ensures|where)\\b",
        },
        {
          name: "keyword.other.sakura",
          match: "(?<![./])\\b(fn|let|var|type|error|test|property|import|pub|opaque|as)\\b",
        },
        {
          name: "invalid.illegal.reserved.sakura",
          match:
            "(?<![./])\\b(verify|proof|ghost|control|resume|region|spawn|actor|interface|impl)\\b(?!\\s*\\()",
        },
        {
          // Bound by `where` (it) and `ensures` (result).
          name: "variable.language.sakura",
          match: "\\b(it|result)\\b",
        },
      ],
    },
    // Typed holes (§2.1): `?name`, no space. A `?` after an operand is the raise operator.
    holes: {
      name: "keyword.other.hole.sakura",
      match: "(?<![A-Za-z0-9_)\\]}?])\\?[a-z_][A-Za-z0-9_]*",
    },
    literals: {
      patterns: [
        {
          name: "constant.language.boolean.sakura",
          match: "\\b(true|false)\\b",
        },
        {
          name: "constant.numeric.hex.sakura",
          match: "\\b0x[0-9A-Fa-f_]+\\b",
        },
        {
          name: "constant.numeric.binary.sakura",
          match: "\\b0b[01_]+\\b",
        },
        {
          name: "constant.numeric.float.sakura",
          match: "\\b[0-9][0-9_]*\\.[0-9][0-9_]*(?:[eE][+-]?[0-9]+)?\\b",
        },
        {
          name: "constant.numeric.integer.sakura",
          match: "\\b[0-9][0-9_]*\\b",
        },
      ],
    },
    attributes: {
      match: "(@)([a-z_][A-Za-z0-9_]*)",
      captures: {
        1: { name: "punctuation.definition.attribute.sakura" },
        2: { name: "entity.other.attribute-name.sakura" },
      },
    },
    calls: {
      patterns: [
        {
          // Type.f( / Effect.op(: home-module functions and effect operations (§4.9)
          match: "\\b([A-Z][A-Za-z0-9_]*)(\\.)([a-z_][A-Za-z0-9_]*)(?=\\s*(?:<[^>]*>)?\\()",
          captures: {
            1: { name: "entity.name.type.module.sakura" },
            2: { name: "punctuation.accessor.sakura" },
            3: { name: "entity.name.function.call.sakura" },
          },
        },
        {
          match: "(?<=\\.)([a-z_][A-Za-z0-9_]*)(?=\\s*(?:<[^>]*>)?\\()",
          captures: {
            1: { name: "entity.name.function.method.sakura" },
          },
        },
        {
          match: "\\b([a-z_][A-Za-z0-9_]*)(?=\\s*(?:<[^>]*>)?\\()",
          captures: {
            1: { name: "entity.name.function.call.sakura" },
          },
        },
      ],
    },
    types: {
      name: "entity.name.type.sakura",
      match: "\\b[A-Z][A-Za-z0-9_]*\\b",
    },
    // §2.1 punctuation and operators. There is no `|>`, no `..=` and no word operators.
    operators: {
      patterns: [
        { name: "keyword.operator.arrow.sakura", match: "->|=>" },
        { name: "keyword.operator.range.sakura", match: "\\.\\." },
        { name: "keyword.operator.logical.sakura", match: "&&|\\|\\|" },
        { name: "keyword.operator.comparison.sakura", match: "==|!=|<=|>=" },
        { name: "keyword.operator.sakura", match: "[+\\-*/%=<>|!?]" },
      ],
    },
    punctuation: {
      patterns: [
        { name: "punctuation.separator.sakura", match: "[,:;]" },
        { name: "punctuation.accessor.sakura", match: "\\." },
        { name: "punctuation.brackets.sakura", match: "[()\\[\\]{}]" },
      ],
    },
  },
};
