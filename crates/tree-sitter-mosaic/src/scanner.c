// External scanner for tree-sitter-mosaic.
//
// Emits three tokens that pure regex tokenisation cannot express cleanly:
//
//   BLANK_LINE        : two or more line terminators (optionally separated by
//                       horizontal whitespace). A single line_end is left to
//                       the internal `_line_end` rule so paragraphs can keep
//                       using soft breaks. The `valid_symbols` bitmap means
//                       BLANK_LINE is only requested at block-boundary
//                       positions, but emitting only at >=2 line_ends keeps
//                       us safe under any future grammar tweak.
//
//   LINEBREAK_ESCAPE  : `hspace* '\' hspace* line_end` in paragraph context
//                       (CommonMark hard break). The internal `escaped_char`
//                       handles every other `\X` form.
//
//   RAW_BODY_CONTENT  : one chunk of `#pre[...]` / `#code[...]` body text.
//                       Honours `\\` and `\]` as `raw_escape`. Stops before
//                       the closing `]` so the grammar matches it as a
//                       literal.
//
// No persistent state; serialize length is 0.

#include "tree_sitter/parser.h"

#include <stddef.h>
#include <stdint.h>
#include <wctype.h>

enum TokenType {
    BLANK_LINE,
    LINEBREAK_ESCAPE,
    RAW_BODY_CONTENT,
    ERROR_SENTINEL,
};

static inline bool is_hspace(int32_t c) {
    return c == ' ' || c == '\t';
}

static inline bool is_line_end(int32_t c) {
    return c == '\n' || c == '\r';
}

static void advance(TSLexer *lexer) {
    lexer->advance(lexer, false);
}

static void skip(TSLexer *lexer) {
    lexer->advance(lexer, true);
}

// Consume one line terminator: \n, \r, or \r\n. Returns true if consumed.
static bool consume_line_end(TSLexer *lexer) {
    if (lexer->lookahead == '\n') {
        advance(lexer);
        return true;
    }
    if (lexer->lookahead == '\r') {
        advance(lexer);
        if (lexer->lookahead == '\n') {
            advance(lexer);
        }
        return true;
    }
    return false;
}

static bool scan_blank_line(TSLexer *lexer) {
    // Skip leading hspace; do not consume the first line_end yet.
    while (is_hspace(lexer->lookahead)) {
        skip(lexer);
    }
    if (!is_line_end(lexer->lookahead)) {
        return false;
    }
    // Consume the first line_end.
    consume_line_end(lexer);
    // Try to match >=1 additional (hspace* line_end) sequences. Only emit
    // BLANK_LINE if we found at least one extra line terminator, otherwise a
    // single newline must stay available as the internal `_line_end` token
    // (paragraph soft break).
    int extra = 0;
    for (;;) {
        // Mark a checkpoint after each accepted line_end so we don't roll
        // back over an already-consumed blank tail.
        lexer->mark_end(lexer);
        while (is_hspace(lexer->lookahead)) {
            advance(lexer);
        }
        if (!is_line_end(lexer->lookahead)) {
            break;
        }
        consume_line_end(lexer);
        extra++;
    }
    if (extra == 0) {
        return false;
    }
    lexer->result_symbol = BLANK_LINE;
    return true;
}

static bool scan_linebreak_escape(TSLexer *lexer) {
    while (is_hspace(lexer->lookahead)) {
        advance(lexer);
    }
    if (lexer->lookahead != '\\') {
        return false;
    }
    advance(lexer);
    while (is_hspace(lexer->lookahead)) {
        advance(lexer);
    }
    if (!is_line_end(lexer->lookahead)) {
        return false;
    }
    consume_line_end(lexer);
    lexer->mark_end(lexer);
    lexer->result_symbol = LINEBREAK_ESCAPE;
    return true;
}

static bool scan_raw_body_content(TSLexer *lexer) {
    bool consumed = false;
    while (lexer->lookahead != 0) {
        if (lexer->lookahead == ']') {
            // Closing bracket; leave it for the grammar's literal `]`.
            break;
        }
        if (lexer->lookahead == '\\') {
            // Look at the char after the backslash without committing.
            advance(lexer);
            if (lexer->lookahead == ']' || lexer->lookahead == '\\') {
                advance(lexer);
            }
            // Plain `\X` for any other X is also fine inside a raw body;
            // the backslash is already consumed.
            consumed = true;
            continue;
        }
        advance(lexer);
        consumed = true;
    }
    if (!consumed) {
        return false;
    }
    lexer->mark_end(lexer);
    lexer->result_symbol = RAW_BODY_CONTENT;
    return true;
}

bool tree_sitter_mosaic_external_scanner_scan(
    void *payload,
    TSLexer *lexer,
    const bool *valid_symbols) {
    (void)payload;

    // During error recovery tree-sitter sets every external symbol in
    // `valid_symbols`. Bail out so we don't gobble unrelated input.
    if (valid_symbols[ERROR_SENTINEL]) {
        return false;
    }

    // Raw body has top priority: when the parser asks for raw content, we
    // must produce exactly one chunk and not interpret anything else.
    if (valid_symbols[RAW_BODY_CONTENT]) {
        if (scan_raw_body_content(lexer)) {
            return true;
        }
    }

    if (valid_symbols[LINEBREAK_ESCAPE]) {
        // Speculative scan; mark_end is only set on success so we don't
        // strand the parser if this fails.
        if (scan_linebreak_escape(lexer)) {
            return true;
        }
    }

    if (valid_symbols[BLANK_LINE]) {
        if (scan_blank_line(lexer)) {
            return true;
        }
    }

    return false;
}

void *tree_sitter_mosaic_external_scanner_create(void) {
    return NULL;
}

void tree_sitter_mosaic_external_scanner_destroy(void *payload) {
    (void)payload;
}

void tree_sitter_mosaic_external_scanner_reset(void *payload) {
    (void)payload;
}

unsigned tree_sitter_mosaic_external_scanner_serialize(
    void *payload,
    char *buffer) {
    (void)payload;
    (void)buffer;
    return 0;
}

void tree_sitter_mosaic_external_scanner_deserialize(
    void *payload,
    const char *buffer,
    unsigned length) {
    (void)payload;
    (void)buffer;
    (void)length;
}
