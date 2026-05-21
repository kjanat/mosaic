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
//   LINEBREAK_ESCAPE  : `hspace* '\' hspace* line_end` in paragraph context
//                       (CommonMark hard break). The internal `escaped_char`
//                       handles every other `\X` form.
//
//   RAW_BODY_CONTENT  : one chunk of `#pre[...]` / `#code[...]` body text.
//                       Stops before raw escapes and the closing `]` so the
//                       grammar can expose both separately.
//
//   RAW_BODY_ESCAPE   : `\\` or `\]` inside a raw body. `\]` means "literal
//                       `]`, not the raw-body delimiter"; the Rust parser
//                       decodes it before lowering.
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
    RAW_BODY_ESCAPE,
    ERROR_SENTINEL,
};

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

/**
 * Scan for a CommonMark hard line break: optional horizontal whitespace, a
 * backslash, optional horizontal whitespace, then a line terminator.
 *
 * @param lexer Lexer positioned at the start of the potential sequence.
 * @returns `true` if the scanner consumed a linebreak-escape sequence and set
 *          `lexer->result_symbol = LINEBREAK_ESCAPE`, `false` otherwise.
 */
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

/**
 * Scan a raw-body escape inside `#pre[...]` or `#code[...]`.
 *
 * @param lexer The Tree-sitter lexer to read from and advance.
 * @returns `true` if a raw-body escape was consumed and emitted.
 */
static bool scan_raw_body_escape(TSLexer *lexer) {
    if (lexer->lookahead != '\\') {
        return false;
    }
    advance(lexer);
    if (lexer->lookahead != ']' && lexer->lookahead != '\\') {
        return false;
    }
    advance(lexer);
    lexer->mark_end(lexer);
    lexer->result_symbol = RAW_BODY_ESCAPE;
    return true;
}

/**
 * Scan a single raw-body content chunk inside `#pre[...]` or `#code[...]`, consuming input until the next raw escape or unescaped `]`.
 *
 * The scanner consumes characters as content and stops before raw escape sequences (`\\` and `\]`) or the closing `]` (which is left for the grammar). Plain `\X` for other `X` remains content. On success it sets `lexer->result_symbol` to `RAW_BODY_CONTENT` and marks the token end.
 *
 * @param lexer The Tree-sitter lexer to read from and advance.
 * @returns `true` if at least one character was consumed and a `RAW_BODY_CONTENT` token was produced, `false` otherwise.
 */
static bool scan_raw_body_content(TSLexer *lexer) {
    bool consumed = false;
    while (lexer->lookahead != 0) {
        if (lexer->lookahead == ']') {
            // Closing bracket; leave it for the grammar's literal `]`.
            break;
        }
        if (lexer->lookahead == '\\') {
            advance(lexer);
            if (lexer->lookahead == ']' || lexer->lookahead == '\\') {
                break;
            }
            // Plain `\X` for any other X is also fine inside a raw body;
            // the backslash is already consumed.
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
 * scanner to invoke according to `valid_symbols` and a fixed priority:
 * `RAW_BODY_ESCAPE` first, then `RAW_BODY_CONTENT`, then `LINEBREAK_ESCAPE`,
 * then `BLANK_LINE`.
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
    (void)payload;

    // During error recovery tree-sitter sets every external symbol in
    // `valid_symbols`. Bail out so we don't gobble unrelated input.
    if (valid_symbols[ERROR_SENTINEL]) {
        return false;
    }

    if (valid_symbols[RAW_BODY_ESCAPE]) {
        if (scan_raw_body_escape(lexer)) {
            return true;
        }
    }

    // Raw body has priority over paragraph-level escapes: when the parser asks
    // for raw content, we must produce exactly one chunk and not interpret
    // anything else.
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

/**
 * Create a new external scanner instance.
 *
 * @returns NULL because the scanner maintains no persistent state.
 */
void *tree_sitter_mosaic_external_scanner_create(void) {
    return NULL;
}

/**
 * Destroy the external scanner payload.
 *
 * No-op: this scanner maintains no persistent state, so the `payload` is ignored.
 *
 * @param payload Pointer previously returned by create; may be NULL and is not used.
 */
void tree_sitter_mosaic_external_scanner_destroy(void *payload) {
    (void)payload;
}

/**
 * Reset the external scanner's persistent state.
 *
 * This scanner maintains no persistent state; the provided `payload` is ignored.
 * @param payload Unused pointer to scanner state (may be NULL).
 */
void tree_sitter_mosaic_external_scanner_reset(void *payload) {
    (void)payload;
}

/**
 * Serialize the external scanner's persistent state into the provided buffer.
 *
 * The scanner maintains no persistent state; both `payload` and `buffer` are ignored.
 *
 * @param payload Pointer to scanner state (ignored).
 * @param buffer Destination buffer for serialized state (ignored).
 * @returns The number of bytes written into `buffer`. Always `0` (no state serialized).
 */
unsigned tree_sitter_mosaic_external_scanner_serialize(
    void *payload,
    char *buffer) {
    (void)payload;
    (void)buffer;
    return 0;
}

/**
 * Restore the external scanner's state from a previously serialized buffer.
 *
 * This scanner does not keep persistent state; the function ignores all arguments
 * and performs no action.
 *
 * @param payload Unused scanner payload pointer.
 * @param buffer Unused pointer to serialized data.
 * @param length Unused length of the serialized data.
 */
void tree_sitter_mosaic_external_scanner_deserialize(
    void *payload,
    const char *buffer,
    unsigned length) {
    (void)payload;
    (void)buffer;
    (void)length;
}
