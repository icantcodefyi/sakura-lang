import type { LanguageRegistration } from "shiki";

/**
 * TextMate grammar for Vow. Shared by the docs (rehype-code) and the
 * landing page (codeToTokens), so ```vow blocks highlight identically.
 * Tracks docs/design.md §2 (lexical structure) and §3 (keywords, operators).
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
        { name: "comment.line.documentation.vow", match: "///.*$" },
        { name: "comment.line.double-slash.vow", match: "//.*$" },
        { include: "#block-comment" },
      ],
    },
    // Block comments nest (§2.1).
    "block-comment": {
      name: "comment.block.vow",
      begin: "/\\*",
      end: "\\*/",
      patterns: [{ include: "#block-comment" }],
    },
    // `! {Db, Clock, Fail<SignupError> | e}`: the effect row of a signature or function type.
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
        { include: "#fail-label" },
        {
          name: "support.type.effect.vow",
          match: "\\b[A-Z][A-Za-z0-9_]*\\b",
        },
        { name: "variable.parameter.row.vow", match: "\\b[a-z_][A-Za-z0-9_]*\\b" },
        { name: "keyword.operator.vow", match: "\\|" },
        { include: "#punctuation" },
      ],
    },
    // `-> B ! e`: a row that is just an effect-row variable.
    "effect-row-var": {
      match: "(?<=\\s)(!)\\s+([a-z_][A-Za-z0-9_]*)\\b(?=\\s*(?:[,){]|$|requires\\b|ensures\\b))",
      captures: {
        1: { name: "keyword.operator.effect.vow" },
        2: { name: "variable.parameter.row.vow" },
      },
    },
    // `Fail<EmailTaken | Banned>`: the effect is highlighted, its error tags are types.
    "fail-label": {
      begin: "\\b(Fail)\\s*(<)",
      beginCaptures: {
        1: { name: "support.type.effect.fail.vow" },
        2: { name: "punctuation.definition.generic.vow" },
      },
      end: ">",
      endCaptures: { 0: { name: "punctuation.definition.generic.vow" } },
      patterns: [
        { name: "entity.name.type.vow", match: "\\b[A-Z][A-Za-z0-9_]*\\b" },
        { name: "keyword.operator.vow", match: "\\|" },
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
          match: "\\b(type|effect|error)\\s+([A-Z][A-Za-z0-9_]*)",
          captures: {
            1: { name: "storage.type.vow" },
            2: { name: "entity.name.type.vow" },
          },
        },
        {
          match: "\\b(handler)\\s+([A-Z][A-Za-z0-9_]*)",
          captures: {
            1: { name: "keyword.other.effect.vow" },
            2: { name: "support.type.effect.vow" },
          },
        },
        {
          match: "\\b(let|var)\\s+([a-z_][A-Za-z0-9_]*)",
          captures: {
            1: { name: "storage.type.let.vow" },
            2: { name: "variable.other.declaration.vow" },
          },
        },
        {
          match: '\\b(test|property)\\s+(?=")',
          captures: {
            1: { name: "keyword.other.test.vow" },
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
            1: { name: "entity.name.function.tagged.vow" },
            2: { name: "punctuation.definition.string.begin.vow" },
          },
          end: '"',
          endCaptures: {
            0: { name: "punctuation.definition.string.end.vow" },
          },
          name: "string.quoted.tagged.vow",
          patterns: [{ include: "#string-escape" }, { include: "#interpolation" }],
        },
        {
          begin: '"',
          beginCaptures: {
            0: { name: "punctuation.definition.string.begin.vow" },
          },
          end: '"',
          endCaptures: {
            0: { name: "punctuation.definition.string.end.vow" },
          },
          name: "string.quoted.double.vow",
          patterns: [{ include: "#string-escape" }, { include: "#interpolation" }],
        },
      ],
    },
    // §2.3: \\ \" \n \r \t \0 \{ \} and \u{XXXXXX}.
    "string-escape": {
      name: "constant.character.escape.vow",
      match: '\\\\(?:[\\\\"nrt0{}]|u\\{[0-9A-Fa-f]{1,6}\\})',
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
      patterns: [{ include: "#nested-braces" }, { include: "$self" }],
    },
    // Braces inside an interpolation nest, so `"{ f({a: 1}) }"` closes at the right `}`.
    "nested-braces": {
      begin: "\\{",
      beginCaptures: { 0: { name: "punctuation.brackets.vow" } },
      end: "\\}",
      endCaptures: { 0: { name: "punctuation.brackets.vow" } },
      patterns: [{ include: "#nested-braces" }, { include: "$self" }],
    },
    // §2.1 keywords, grouped by role. Reserved words (`V0007`) are marked invalid.
    // A keyword directly after `.` or `/` is a member or module segment (`Task.spawn`,
    // `std/test`), and a reserved word directly after `fn` is an operation name.
    keywords: {
      patterns: [
        {
          name: "keyword.control.vow",
          match: "(?<![./])\\b(if|else|match|for|in|while|return|break|continue|fail|try|catch)\\b",
        },
        {
          name: "keyword.other.effect.vow",
          match: "(?<![./])\\b(with|handler|effect)\\b",
        },
        {
          name: "keyword.other.contract.vow",
          match: "(?<![./])\\b(requires|ensures|where)\\b",
        },
        {
          name: "keyword.other.vow",
          match: "(?<![./])\\b(fn|let|var|type|error|test|property|import|pub|opaque|as)\\b",
        },
        {
          name: "invalid.illegal.reserved.vow",
          match:
            "(?<![./])\\b(verify|proof|ghost|control|resume|region|spawn|actor|interface|impl)\\b(?!\\s*\\()",
        },
        {
          // Bound by `where` (it) and `ensures` (result).
          name: "variable.language.vow",
          match: "\\b(it|result)\\b",
        },
      ],
    },
    // Typed holes (§2.1): `?name`, no space. A `?` after an operand is the raise operator.
    holes: {
      name: "keyword.other.hole.vow",
      match: "(?<![A-Za-z0-9_)\\]}?])\\?[a-z_][A-Za-z0-9_]*",
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
          name: "constant.numeric.binary.vow",
          match: "\\b0b[01_]+\\b",
        },
        {
          name: "constant.numeric.float.vow",
          match: "\\b[0-9][0-9_]*\\.[0-9][0-9_]*(?:[eE][+-]?[0-9]+)?\\b",
        },
        {
          name: "constant.numeric.integer.vow",
          match: "\\b[0-9][0-9_]*\\b",
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
          // Type.f( / Effect.op(: home-module functions and effect operations (§4.9)
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
    // §2.1 punctuation and operators. There is no `|>`, no `..=` and no word operators.
    operators: {
      patterns: [
        { name: "keyword.operator.arrow.vow", match: "->|=>" },
        { name: "keyword.operator.range.vow", match: "\\.\\." },
        { name: "keyword.operator.logical.vow", match: "&&|\\|\\|" },
        { name: "keyword.operator.comparison.vow", match: "==|!=|<=|>=" },
        { name: "keyword.operator.vow", match: "[+\\-*/%=<>|!?]" },
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
