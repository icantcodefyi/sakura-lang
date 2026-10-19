// External scanner for Sakura Lang: the §2.4 newline rule, comments (nesting, `///` doc
// comments, a line comment ending before the `\r` of `\r\n`) and string text.
//
// A line break is a terminator only where the grammar accepts one (rule 1, approximated by the
// parse state) and when the next significant token cannot continue the statement (rule 2). The
// terminator is zero-width, placed at the next token; when comments come first it follows them.

#include "tree_sitter/alloc.h"
#include "tree_sitter/parser.h"

#include <stdbool.h>
#include <stdint.h>

enum TokenType {
  NEWLINE,
  STRING_CONTENT,
  COMMENT,
  DOC_COMMENT,
  ERROR_SENTINEL,
};

typedef struct {
  // A comment that followed a line break decided the break was a terminator; emit it next.
  bool newline_next;
} Scanner;

static void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

static void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

static bool is_space(int32_t c) { return c == ' ' || c == '\t' || c == '\r' || c == '\n'; }

static bool is_ident(int32_t c) {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_';
}

// After `//`: the rest of the line, excluding the `\r` of a `\r\n` (§2).
static void line_comment_rest(TSLexer *lexer, bool mark) {
  while (!lexer->eof(lexer) && lexer->lookahead != '\n') {
    bool cr = lexer->lookahead == '\r';
    advance(lexer);
    if (mark && !(cr && lexer->lookahead == '\n')) lexer->mark_end(lexer);
  }
}

// After `/*`: up to the matching `*/`, nesting; an unterminated comment runs to the end.
static void block_comment_rest(TSLexer *lexer) {
  unsigned depth = 1;
  while (depth > 0 && !lexer->eof(lexer)) {
    int32_t c = lexer->lookahead;
    advance(lexer);
    if (c == '/' && lexer->lookahead == '*') {
      advance(lexer);
      depth++;
    } else if (c == '*' && lexer->lookahead == '/') {
      advance(lexer);
      depth--;
    }
  }
}

// §2.4 rule 2: whether the next significant token continues the statement.
static bool continues(TSLexer *lexer) {
  for (;;) {
    while (is_space(lexer->lookahead)) advance(lexer);
    if (lexer->lookahead != '/') break;
    advance(lexer);
    if (lexer->lookahead == '/') {
      line_comment_rest(lexer, false);
    } else if (lexer->lookahead == '*') {
      advance(lexer);
      block_comment_rest(lexer);
    } else {
      return true;  // `/`
    }
  }
  if (lexer->eof(lexer)) return false;
  int32_t c = lexer->lookahead;
  switch (c) {
    case '.': case ')': case ']': case '}': case ',': case '=': case '|':
    case '+': case '*': case '%': case '<': case '>':
      return true;
    case '-':
      advance(lexer);
      return lexer->lookahead == '>';
    case '!':
      advance(lexer);
      return lexer->lookahead == '=';
    case '&':
      advance(lexer);
      return lexer->lookahead == '&';
    default:
      break;
  }
  char word[6];
  unsigned len = 0;
  while (is_ident(lexer->lookahead)) {
    if (len == sizeof word) return false;
    word[len++] = (char)lexer->lookahead;
    advance(lexer);
  }
  return (len == 4 && word[0] == 'e' && word[1] == 'l' && word[2] == 's' && word[3] == 'e') ||
         (len == 5 && word[0] == 'c' && word[1] == 'a' && word[2] == 't' && word[3] == 'c' &&
          word[4] == 'h');
}

static bool scan_string_content(TSLexer *lexer) {
  bool any = false;
  while (!lexer->eof(lexer) && lexer->lookahead != '"' && lexer->lookahead != '{' &&
         lexer->lookahead != '\\') {
    advance(lexer);
    any = true;
  }
  lexer->mark_end(lexer);
  lexer->result_symbol = STRING_CONTENT;
  return any;
}

void *tree_sitter_sakura_external_scanner_create(void) {
  return ts_calloc(1, sizeof(Scanner));
}

void tree_sitter_sakura_external_scanner_destroy(void *payload) { ts_free(payload); }

unsigned tree_sitter_sakura_external_scanner_serialize(void *payload, char *buffer) {
  buffer[0] = (char)((Scanner *)payload)->newline_next;
  return 1;
}

void tree_sitter_sakura_external_scanner_deserialize(void *payload, const char *buffer,
                                                     unsigned length) {
  ((Scanner *)payload)->newline_next = length > 0 && buffer[0];
}

bool tree_sitter_sakura_external_scanner_scan(void *payload, TSLexer *lexer,
                                              const bool *valid_symbols) {
  Scanner *s = payload;
  bool recovering = valid_symbols[ERROR_SENTINEL];

  if (valid_symbols[STRING_CONTENT] && !recovering) return scan_string_content(lexer);

  if (s->newline_next) {
    s->newline_next = false;
    if (valid_symbols[NEWLINE] && !recovering) {
      lexer->mark_end(lexer);
      lexer->result_symbol = NEWLINE;
      return true;
    }
  }

  bool saw_newline = false;
  while (is_space(lexer->lookahead)) {
    if (lexer->lookahead == '\n') saw_newline = true;
    skip(lexer);
  }
  bool want_newline = saw_newline && valid_symbols[NEWLINE] && !recovering;

  if (lexer->lookahead == '/') {
    advance(lexer);
    enum TokenType kind = COMMENT;
    if (lexer->lookahead == '/') {
      advance(lexer);
      if (lexer->lookahead == '/') {
        advance(lexer);
        if (lexer->lookahead != '/') kind = DOC_COMMENT;
      }
      lexer->mark_end(lexer);
      line_comment_rest(lexer, true);
    } else if (lexer->lookahead == '*') {
      advance(lexer);
      block_comment_rest(lexer);
      lexer->mark_end(lexer);
    } else {
      return false;  // `/` continues the statement; the grammar lexes it.
    }
    if (want_newline) s->newline_next = !continues(lexer);
    lexer->result_symbol = kind;
    return true;
  }

  if (want_newline) {
    lexer->mark_end(lexer);
    if (!continues(lexer)) {
      lexer->result_symbol = NEWLINE;
      return true;
    }
  }
  return false;
}
