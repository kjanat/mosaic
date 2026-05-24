// External scanner for tree-sitter-mosaic.
//
// Emits four tokens that pure regex tokenisation cannot express cleanly:
//
//   BLANK_LINE        : two or more line terminators (optionally separated by
//                       horizontal whitespace). A single line_end is left to
//                       the internal `_line_end` rule so paragraphs can keep
//                       using soft breaks. The `valid_symbols` bitmap means
//                       BLANK_LINE is only requested at block-boundary
//                       positions, but emitting only at >=2 line_ends keeps
//                       us safe under any future grammar tweak.
//
//   RAW_BODY_OPEN     : Lua-style long-bracket opener (`[=[`, `[[`, etc.).
//
//   RAW_BODY_CONTENT  : literal `#pre` / `#code` body text.
//
//   RAW_BODY_CLOSE    : matching long-bracket closer (`]=]`, `]]`, etc.).

#include "tree_sitter/parser.h"

#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

enum TokenType {
    BLANK_LINE,
    RAW_BODY_OPEN,
    RAW_BODY_CONTENT,
    RAW_BODY_CLOSE,
    ERROR_SENTINEL,
};

typedef struct {
    uint32_t raw_eq_count;
    bool in_raw_body;
} Scanner;

/**
 * Determine whether a codepoint is a horizontal whitespace character (space or tab).
 * @param code Codepoint to test.
 * @returns `true` if `code` is a space (`' '`) or tab (`'\t'`), `false` otherwise.
 */
static inline bool is_hspace(int32_t code) {
    return code == ' ' || code == '\t';
}

/**
 * Check whether a codepoint is a line terminator.
 * @param code Unicode codepoint to test.
 * @returns `true` if the codepoint is `'\n'` or `'\r'`, `false` otherwise.
 */
static inline bool is_line_end(int32_t code) {
    return code == '\n' || code == '\r';
}

/**
 * Advance the lexer by a single codepoint without marking it as skipped.
 *
 * @param lexer Pointer to the TSLexer to advance.
 */
static void advance(TSLexer *lexer) {
    lexer->advance(lexer, false);
}

/**
 * Advance the lexer by one codepoint and mark that input as skipped (not part of the current token).
 *
 * @param lexer The lexer to advance.
 */
static void skip(TSLexer *lexer) {
    lexer->advance(lexer, true);
}

/**
 * Consume a single line terminator sequence from the lexer input.
 *
 * Recognizes LF (`\n`), CR (`\r`), or CR+LF (`\r\n`) and advances the lexer past the terminator.
 *
 * @param lexer The lexer whose input position will be advanced if a terminator is found.
 * @returns `true` if a line terminator was consumed, `false` otherwise.
 */
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

/**
 * Scan for a blank line consisting of two or more line terminators (each possibly
 * preceded by horizontal whitespace) and emit the `BLANK_LINE` external token.
 *
 * The scanner skips leading horizontal space, requires at least one line terminator,
 * then requires at least one additional `(hspace* line_end)` sequence before
 * emitting `BLANK_LINE`. A single line terminator is not consumed by this scanner
 * so that the internal `_line_end` rule can still match.
 *
 * @returns `true` if a `BLANK_LINE` was matched and `lexer->result_symbol` was set,
 *          `false` otherwise.
 */
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

static bool scan_raw_body_open(Scanner *scanner, TSLexer *lexer) {
    if (lexer->lookahead != '[') {
        return false;
    }
    advance(lexer);
    uint32_t eq_count = 0;
    while (lexer->lookahead == '=') {
        eq_count++;
        advance(lexer);
    }
    if (lexer->lookahead != '[') {
        return false;
    }
    advance(lexer);
    lexer->mark_end(lexer);
    scanner->raw_eq_count = eq_count;
    scanner->in_raw_body = true;
    lexer->result_symbol = RAW_BODY_OPEN;
    return true;
}

static bool scan_raw_body_close(Scanner *scanner, TSLexer *lexer) {
    if (!scanner->in_raw_body || lexer->lookahead != ']') {
        return false;
    }
    advance(lexer);
    for (uint32_t i = 0; i < scanner->raw_eq_count; i++) {
        if (lexer->lookahead != '=') {
            return false;
        }
        advance(lexer);
    }
    if (lexer->lookahead != ']') {
        return false;
    }
    advance(lexer);
    lexer->mark_end(lexer);
    scanner->raw_eq_count = 0;
    scanner->in_raw_body = false;
    lexer->result_symbol = RAW_BODY_CLOSE;
    return true;
}

static bool scan_raw_body_content(Scanner *scanner, TSLexer *lexer) {
    if (!scanner->in_raw_body) {
        return false;
    }
    bool consumed = false;
    while (lexer->lookahead != 0) {
        if (lexer->lookahead == ']') {
            advance(lexer);
            uint32_t eq_seen = 0;
            while (eq_seen < scanner->raw_eq_count && lexer->lookahead == '=') {
                eq_seen++;
                advance(lexer);
            }
            if (eq_seen == scanner->raw_eq_count && lexer->lookahead == ']') {
                break;
            }
            consumed = true;
            lexer->mark_end(lexer);
            continue;
        }
        advance(lexer);
        consumed = true;
        lexer->mark_end(lexer);
    }
    if (!consumed) {
        return false;
    }
    lexer->result_symbol = RAW_BODY_CONTENT;
    return true;
}

/**
 * Choose and run the appropriate external token scanner based on parser context.
 *
 * When Tree-sitter requests an external token, this function selects which
 * scanner to invoke according to `valid_symbols` and a fixed priority.
 * If `valid_symbols[ERROR_SENTINEL]` is set (error-recovery mode), the
 * function returns immediately without consuming input.
 *
 * @param payload Ignored.
 * @param lexer The Tree-sitter lexer used to inspect and advance input.
 * @param valid_symbols Array indicating which external symbols the parser will accept.
 * @returns `true` if a scanner produced a token (and `lexer->result_symbol` was set), `false` otherwise.
 */
bool tree_sitter_mosaic_external_scanner_scan(
    void *payload,
    TSLexer *lexer,
    const bool *valid_symbols) {
    Scanner *scanner = (Scanner *)payload;

    // During error recovery tree-sitter sets every external symbol in
    // `valid_symbols`. Bail out so we don't gobble unrelated input.
    if (valid_symbols[ERROR_SENTINEL]) {
        return false;
    }

    if (valid_symbols[RAW_BODY_OPEN]) {
        if (scan_raw_body_open(scanner, lexer)) {
            return true;
        }
    }

    if (valid_symbols[RAW_BODY_CLOSE]) {
        if (scan_raw_body_close(scanner, lexer)) {
            return true;
        }
    }

    if (valid_symbols[RAW_BODY_CONTENT]) {
        if (scan_raw_body_content(scanner, lexer)) {
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
    return calloc(1, sizeof(Scanner));
}

void tree_sitter_mosaic_external_scanner_destroy(void *payload) {
    free(payload);
}

void tree_sitter_mosaic_external_scanner_reset(void *payload) {
    Scanner *scanner = (Scanner *)payload;
    scanner->raw_eq_count = 0;
    scanner->in_raw_body = false;
}

unsigned tree_sitter_mosaic_external_scanner_serialize(
    void *payload,
    char *buffer) {
    Scanner *scanner = (Scanner *)payload;
    buffer[0] = scanner->in_raw_body ? 1 : 0;
    buffer[1] = (char)(scanner->raw_eq_count & 0xff);
    buffer[2] = (char)((scanner->raw_eq_count >> 8) & 0xff);
    buffer[3] = (char)((scanner->raw_eq_count >> 16) & 0xff);
    buffer[4] = (char)((scanner->raw_eq_count >> 24) & 0xff);
    return 5;
}

void tree_sitter_mosaic_external_scanner_deserialize(
    void *payload,
    const char *buffer,
    unsigned length) {
    Scanner *scanner = (Scanner *)payload;
    scanner->raw_eq_count = 0;
    scanner->in_raw_body = false;
    if (length >= 5) {
        scanner->in_raw_body = buffer[0] != 0;
        scanner->raw_eq_count = (uint32_t)(unsigned char)buffer[1]
            | ((uint32_t)(unsigned char)buffer[2] << 8)
            | ((uint32_t)(unsigned char)buffer[3] << 16)
            | ((uint32_t)(unsigned char)buffer[4] << 24);
    }
}
