// External scanner for tree-sitter-mosaic.
//
// Emits tokens that pure regex tokenisation cannot express cleanly:
//
//   BLANK_LINE        : two or more line terminators (optionally separated by
//                       horizontal whitespace). A single line_end is left to
//                       the internal `_line_end` rule so paragraphs can keep
//                       using soft breaks. The `valid_symbols` bitmap means
//                       BLANK_LINE is only requested at block-boundary
//                       positions, but emitting only at >=2 line_ends keeps
//                       us safe under any future grammar tweak.
//
//   SOFT_BREAK        : one line terminator that joins prose lines inside a
//                       paragraph or inline delimiter. It is only emitted when
//                       the next non-space character cannot start a block.
//
//   LINE_END          : one line terminator that ends a block-level construct.
//
//   NESTED_LIST_BREAK : one line terminator before a marker indented far enough
//                       to start a child list under the current item.
//
//   LIST_ITEM_BREAK   : one line terminator before a marker that starts the
//                       next item in the current list.
//
//   LIST_TAIL_BREAK   : one line terminator before a non-marker line that
//                       continues an ancestor item after a child list.
//
//   *_LIST_MARKER     : list markers, external so continuation indentation can
//                       be compared against the item text column.
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

#define MAX_LIST_DEPTH 64

enum TokenType {
    BLANK_LINE,
    SOFT_BREAK,
    LINE_END,
    NESTED_LIST_BREAK,
    LIST_ITEM_BREAK,
    LIST_TAIL_BREAK,
    UNORDERED_LIST_MARKER,
    ORDERED_LIST_MARKER,
    RAW_BODY_OPEN,
    RAW_BODY_CONTENT,
    RAW_BODY_CLOSE,
    ERROR_SENTINEL,
};

typedef struct {
    uint32_t raw_eq_count;
    uint32_t list_content_columns[MAX_LIST_DEPTH];
    uint32_t list_marker_columns[MAX_LIST_DEPTH];
    uint8_t list_depth;
    bool in_raw_body;
} Scanner;

typedef enum {
    LINE_START_EMPTY,
    LINE_START_CONTENT,
    LINE_START_BLOCK,
    LINE_START_LIST_MARKER,
} LineStartKind;

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

static void skip(TSLexer *lexer) {
    lexer->advance(lexer, true);
}

static void skip_hspace(TSLexer *lexer) {
    while (is_hspace(lexer->lookahead)) {
        skip(lexer);
    }
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

static bool starts_heading(TSLexer *lexer) {
    uint32_t marker_count = 0;
    while (lexer->lookahead == '=') {
        marker_count++;
        advance(lexer);
    }
    return marker_count >= 1 && marker_count <= 6 && is_hspace(lexer->lookahead);
}

static bool starts_unordered_list(TSLexer *lexer) {
    if (lexer->lookahead != '-') {
        return false;
    }
    advance(lexer);
    return is_hspace(lexer->lookahead);
}

static bool starts_ordered_list(TSLexer *lexer) {
    if (lexer->lookahead < '0' || lexer->lookahead > '9') {
        return false;
    }
    do {
        advance(lexer);
    } while (lexer->lookahead >= '0' && lexer->lookahead <= '9');
    if (lexer->lookahead != '.') {
        return false;
    }
    advance(lexer);
    return is_hspace(lexer->lookahead);
}

static bool starts_hash_block(TSLexer *lexer) {
    if (lexer->lookahead != '#') {
        return false;
    }
    advance(lexer);
    return lexer->lookahead != '!';
}

static bool is_label_start(int32_t code) {
    return (code >= 'A' && code <= 'Z') || (code >= 'a' && code <= 'z') || code == '_';
}

static bool is_label_continue(int32_t code) {
    return is_label_start(code) || (code >= '0' && code <= '9') || code == '-';
}

static bool starts_label(TSLexer *lexer) {
    if (lexer->lookahead != '<') {
        return false;
    }
    advance(lexer);

    if (!is_label_start(lexer->lookahead)) {
        return false;
    }

    for (;;) {
        do {
            advance(lexer);
        } while (is_label_continue(lexer->lookahead));

        if (lexer->lookahead != ':') {
            break;
        }

        advance(lexer);
        if (!is_label_start(lexer->lookahead)) {
            return false;
        }
    }

    return lexer->lookahead == '>';
}

static bool starts_content_body_close(TSLexer *lexer) {
    return lexer->lookahead == ']';
}

static LineStartKind classify_line_start(TSLexer *lexer) {
    if (lexer->eof(lexer) || is_line_end(lexer->lookahead)) {
        return LINE_START_EMPTY;
    }

    switch (lexer->lookahead) {
        case '-':
            return starts_unordered_list(lexer) ? LINE_START_LIST_MARKER : LINE_START_CONTENT;
        case '#':
            return starts_hash_block(lexer) ? LINE_START_BLOCK : LINE_START_CONTENT;
        case '=':
            return starts_heading(lexer) ? LINE_START_BLOCK : LINE_START_CONTENT;
        case '<':
            return starts_label(lexer) ? LINE_START_BLOCK : LINE_START_CONTENT;
        case ']':
            return starts_content_body_close(lexer) ? LINE_START_BLOCK : LINE_START_CONTENT;
        default:
            return starts_ordered_list(lexer) ? LINE_START_LIST_MARKER : LINE_START_CONTENT;
    }
}

static void reset_list_state(Scanner *scanner) {
    scanner->list_depth = 0;
}

static bool has_list_context(const Scanner *scanner) {
    return scanner->list_depth > 0;
}

static uint32_t current_list_content_column(const Scanner *scanner) {
    return scanner->list_content_columns[scanner->list_depth - 1];
}

static uint32_t current_list_marker_column(const Scanner *scanner) {
    return scanner->list_marker_columns[scanner->list_depth - 1];
}

static void push_list_columns(Scanner *scanner, uint32_t marker_column, uint32_t content_column) {
    if (scanner->list_depth >= MAX_LIST_DEPTH) {
        scanner->list_marker_columns[MAX_LIST_DEPTH - 1] = marker_column;
        scanner->list_content_columns[MAX_LIST_DEPTH - 1] = content_column;
        return;
    }

    scanner->list_marker_columns[scanner->list_depth] = marker_column;
    scanner->list_content_columns[scanner->list_depth] = content_column;
    scanner->list_depth++;
}

static void pop_list_to_marker_indent(Scanner *scanner, uint32_t marker_column) {
    while (scanner->list_depth > 0 && marker_column < current_list_marker_column(scanner)) {
        scanner->list_depth--;
    }
}

static void pop_list_to_content_column(Scanner *scanner, uint32_t content_column) {
    while (scanner->list_depth > 0 && content_column < current_list_content_column(scanner)) {
        scanner->list_depth--;
    }
}

static void record_list_marker(Scanner *scanner, uint32_t marker_column, uint32_t content_column) {
    pop_list_to_marker_indent(scanner, marker_column);
    push_list_columns(scanner, marker_column, content_column);
}

static bool scan_unordered_list_marker(Scanner *scanner, TSLexer *lexer) {
    skip_hspace(lexer);

    if (lexer->lookahead != '-') {
        return false;
    }

    uint32_t marker_column = lexer->get_column(lexer);
    advance(lexer);
    if (!is_hspace(lexer->lookahead)) {
        return false;
    }

    do {
        advance(lexer);
    } while (is_hspace(lexer->lookahead));

    lexer->mark_end(lexer);
    record_list_marker(scanner, marker_column, lexer->get_column(lexer));
    lexer->result_symbol = UNORDERED_LIST_MARKER;
    return true;
}

static bool scan_ordered_list_marker(Scanner *scanner, TSLexer *lexer) {
    skip_hspace(lexer);

    if (lexer->lookahead < '0' || lexer->lookahead > '9') {
        return false;
    }

    uint32_t marker_column = lexer->get_column(lexer);
    do {
        advance(lexer);
    } while (lexer->lookahead >= '0' && lexer->lookahead <= '9');

    if (lexer->lookahead != '.') {
        return false;
    }
    advance(lexer);
    if (!is_hspace(lexer->lookahead)) {
        return false;
    }

    do {
        advance(lexer);
    } while (is_hspace(lexer->lookahead));

    lexer->mark_end(lexer);
    record_list_marker(scanner, marker_column, lexer->get_column(lexer));
    lexer->result_symbol = ORDERED_LIST_MARKER;
    return true;
}

static bool scan_line_ending(Scanner *scanner, TSLexer *lexer, const bool *valid_symbols) {
    if (!is_line_end(lexer->lookahead)) {
        return false;
    }

    consume_line_end(lexer);
    lexer->mark_end(lexer);

    uint32_t space_indent = 0;
    while (is_hspace(lexer->lookahead)) {
        if (lexer->lookahead == ' ') {
            space_indent++;
        }
        advance(lexer);
    }

    bool has_extra_line = false;
    if (valid_symbols[BLANK_LINE]) {
        while (is_line_end(lexer->lookahead)) {
            consume_line_end(lexer);
            lexer->mark_end(lexer);
            has_extra_line = true;

            while (is_hspace(lexer->lookahead)) {
                advance(lexer);
            }
        }
    }

    if (has_extra_line) {
        reset_list_state(scanner);
        lexer->result_symbol = BLANK_LINE;
        return true;
    }

    uint32_t next_column = lexer->get_column(lexer);
    lexer->mark_end(lexer);
    LineStartKind line_start = classify_line_start(lexer);
    bool had_list_context = has_list_context(scanner);

    if (line_start == LINE_START_LIST_MARKER && had_list_context) {
        if (valid_symbols[NESTED_LIST_BREAK] && next_column > current_list_marker_column(scanner)) {
            lexer->result_symbol = NESTED_LIST_BREAK;
            return true;
        }

        if (valid_symbols[LIST_ITEM_BREAK] && next_column == current_list_marker_column(scanner)) {
            scanner->list_depth--;
            lexer->result_symbol = LIST_ITEM_BREAK;
            return true;
        }

        pop_list_to_marker_indent(scanner, next_column);
    }

    bool const can_continue = line_start == LINE_START_CONTENT;

    if (valid_symbols[SOFT_BREAK] && can_continue) {
        if (had_list_context) {
            if (space_indent >= current_list_content_column(scanner)) {
                lexer->result_symbol = SOFT_BREAK;
                return true;
            }
        } else {
            lexer->result_symbol = SOFT_BREAK;
            return true;
        }
    }

    if (line_start == LINE_START_CONTENT) {
        pop_list_to_content_column(scanner, space_indent);
        if (valid_symbols[LIST_TAIL_BREAK]
            && has_list_context(scanner)
            && space_indent >= current_list_content_column(scanner)) {
            lexer->result_symbol = LIST_TAIL_BREAK;
            return true;
        }
    }

    if (line_start == LINE_START_BLOCK || line_start == LINE_START_EMPTY) {
        reset_list_state(scanner);
    }

    if (valid_symbols[SOFT_BREAK] && can_continue && !had_list_context) {
        lexer->result_symbol = SOFT_BREAK;
        return true;
    }

    if (valid_symbols[LINE_END]) {
        lexer->result_symbol = LINE_END;
        return true;
    }

    if (valid_symbols[SOFT_BREAK] && !had_list_context) {
        lexer->result_symbol = SOFT_BREAK;
        return true;
    }

    return false;
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

    if (valid_symbols[UNORDERED_LIST_MARKER]) {
        if (scan_unordered_list_marker(scanner, lexer)) {
            return true;
        }
    }

    if (valid_symbols[ORDERED_LIST_MARKER]) {
        if (scan_ordered_list_marker(scanner, lexer)) {
            return true;
        }
    }

    if (valid_symbols[BLANK_LINE]
        || valid_symbols[SOFT_BREAK]
        || valid_symbols[LINE_END]
        || valid_symbols[NESTED_LIST_BREAK]
        || valid_symbols[LIST_ITEM_BREAK]
        || valid_symbols[LIST_TAIL_BREAK]) {
        if (scan_line_ending(scanner, lexer, valid_symbols)) {
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
    scanner->list_depth = 0;
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
    buffer[5] = (char)scanner->list_depth;

    unsigned offset = 6;
    for (uint8_t i = 0; i < scanner->list_depth; i++) {
        uint32_t marker_column = scanner->list_marker_columns[i];
        uint32_t content_column = scanner->list_content_columns[i];
        buffer[offset] = (char)(marker_column & 0xff);
        buffer[offset + 1] = (char)((marker_column >> 8) & 0xff);
        buffer[offset + 2] = (char)((marker_column >> 16) & 0xff);
        buffer[offset + 3] = (char)((marker_column >> 24) & 0xff);
        buffer[offset + 4] = (char)(content_column & 0xff);
        buffer[offset + 5] = (char)((content_column >> 8) & 0xff);
        buffer[offset + 6] = (char)((content_column >> 16) & 0xff);
        buffer[offset + 7] = (char)((content_column >> 24) & 0xff);
        offset += 8;
    }

    return offset;
}

void tree_sitter_mosaic_external_scanner_deserialize(
    void *payload,
    const char *buffer,
    unsigned length) {
    Scanner *scanner = (Scanner *)payload;
    scanner->raw_eq_count = 0;
    scanner->list_depth = 0;
    scanner->in_raw_body = false;
    if (length >= 5) {
        scanner->in_raw_body = buffer[0] != 0;
        scanner->raw_eq_count = (uint32_t)(unsigned char)buffer[1]
            | ((uint32_t)(unsigned char)buffer[2] << 8)
            | ((uint32_t)(unsigned char)buffer[3] << 16)
            | ((uint32_t)(unsigned char)buffer[4] << 24);
    }
    if (length >= 6) {
        uint8_t depth = (uint8_t)buffer[5];
        if (depth > MAX_LIST_DEPTH) {
            depth = MAX_LIST_DEPTH;
        }
        unsigned required_length = 6 + (unsigned)depth * 8;
        if (length >= required_length) {
            scanner->list_depth = depth;
            unsigned offset = 6;
            for (uint8_t i = 0; i < depth; i++) {
                scanner->list_marker_columns[i] = (uint32_t)(unsigned char)buffer[offset]
                    | ((uint32_t)(unsigned char)buffer[offset + 1] << 8)
                    | ((uint32_t)(unsigned char)buffer[offset + 2] << 16)
                    | ((uint32_t)(unsigned char)buffer[offset + 3] << 24);
                scanner->list_content_columns[i] = (uint32_t)(unsigned char)buffer[offset + 4]
                    | ((uint32_t)(unsigned char)buffer[offset + 5] << 8)
                    | ((uint32_t)(unsigned char)buffer[offset + 6] << 16)
                    | ((uint32_t)(unsigned char)buffer[offset + 7] << 24);
                offset += 8;
            }
        }
    }
}
