//! Internal parser support helpers.

/// Return a list marker at `pos`, if present.
///
/// The tuple is `(indent, ordered, content_start)`. Indentation counts ASCII
/// spaces only; tabs are tolerated only as post-marker whitespace.
#[must_use]
pub fn list_marker_at(bytes: &[u8], pos: usize) -> Option<(usize, bool, usize)> {
    let mut i = pos;
    let mut indent = 0_usize;
    while i < bytes.len() && bytes[i] == b' ' {
        indent += 1;
        i += 1;
    }
    if i >= bytes.len() || bytes[i] == b'\n' || bytes[i] == b'\r' {
        return None;
    }
    if bytes[i] == b'-' {
        let after = i + 1;
        if after >= bytes.len() {
            return None;
        }
        if bytes[after] != b' ' && bytes[after] != b'\t' {
            return None;
        }
        let mut j = after;
        while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
        }
        return Some((indent, false, j));
    }
    if bytes[i].is_ascii_digit() {
        let mut j = i;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'.' {
            return None;
        }
        let after = j + 1;
        if after >= bytes.len() {
            return None;
        }
        if bytes[after] != b' ' && bytes[after] != b'\t' {
            return None;
        }
        let mut k = after;
        while k < bytes.len() && (bytes[k] == b' ' || bytes[k] == b'\t') {
            k += 1;
        }
        return Some((indent, true, k));
    }
    None
}

/// Whether `bytes[i..]` begins a `//` line comment: two slashes followed by a
/// space or a buffer/line boundary (`end`, `\n`, or `\r`).
///
/// The trailing-space (or end-of-line) requirement is what keeps URLs safe:
/// `https://x` has a non-space after the slashes, so it never matches. `end`
/// bounds the buffer; for a single line pass the line's `content_end`, for a
/// multi-line inline slice pass the slice length.
#[must_use]
pub fn double_slash_comment(bytes: &[u8], i: usize, end: usize) -> bool {
    i + 1 < end
        && bytes[i] == b'/'
        && bytes[i + 1] == b'/'
        && (i + 2 >= end || matches!(bytes[i + 2], b' ' | b'\n' | b'\r'))
}

/// Whether the line `[line_start, content_end)` is a whole-line `//` comment:
/// after skipping leading spaces/tabs, the first content is a URL-safe `//`
/// run. Used at block level so a comment line pushes no item.
#[must_use]
pub fn line_comment_at(bytes: &[u8], line_start: usize, content_end: usize) -> bool {
    let mut p = line_start;
    while p < content_end && (bytes[p] == b' ' || bytes[p] == b'\t') {
        p += 1;
    }
    double_slash_comment(bytes, p, content_end)
}

/// Byte offset of a block-comment opener `/*` on the line
/// `[line_start, content_end)` after skipping leading spaces/tabs, or `None`
/// when the first non-blank content is not `/*`.
#[must_use]
pub fn block_comment_at(bytes: &[u8], line_start: usize, content_end: usize) -> Option<usize> {
    let mut p = line_start;
    while p < content_end && (bytes[p] == b' ' || bytes[p] == b'\t') {
        p += 1;
    }
    if p + 1 < content_end && bytes[p] == b'/' && bytes[p + 1] == b'*' {
        Some(p)
    } else {
        None
    }
}

/// Whether the `/*` opener at `open` is a `/**` documentation-comment opener.
///
/// `open` points at the `/` and `open + 1` at the first `*`. A doc opener has a
/// second `*` that is not immediately followed by `/` — so `/** … */` and
/// `/*** … */` are doc comments, but the empty block comment `/**/` is not.
/// Doc comments are preserved (their text feeds hover); plain `/*` and `//`
/// comments are dropped.
#[must_use]
pub fn is_doc_comment_open(bytes: &[u8], open: usize) -> bool {
    open + 2 < bytes.len()
        && bytes[open + 2] == b'*'
        && !(open + 3 < bytes.len() && bytes[open + 3] == b'/')
}

/// Clean the inner text of a `/** … */` doc comment for display.
///
/// Each line is left-trimmed and, when it opens with a ` * ` continuation
/// marker (or a bare `*`), that marker is stripped — the jsdoc/rustdoc house
/// style. A leading `*` that is *not* followed by a space is left intact so
/// `*emphasis*` content survives. Blank framing lines are dropped.
#[must_use]
pub fn clean_doc_text(raw: &str) -> String {
    raw.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            let body =
                trimmed
                    .strip_prefix("* ")
                    .unwrap_or(if trimmed == "*" { "" } else { trimmed });
            body.trim_end()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

/// Byte offset at which content should end when `src[start..end]` carries a
/// trailing `//` line comment, or `None` when it does not.
///
/// The comment marker must sit at a whitespace boundary (start of range, or a
/// preceding space/tab/newline) and be followed by a space or the range end.
/// The returned offset excludes the whitespace run immediately before `//`, so
/// `Title // note` trims to `Title` (no dangling space). A `//` inside a
/// `/* … */` block comment is skipped, so `Title /* // note */` is not mistaken
/// for a line comment that would truncate the block comment mid-way.
#[must_use]
pub fn trailing_line_comment_start(bytes: &[u8], start: usize, end: usize) -> Option<usize> {
    let mut j = start;
    while j < end {
        if j + 1 < end && bytes[j] == b'/' && bytes[j + 1] == b'*' {
            let mut k = j + 2;
            while k + 1 < end && !(bytes[k] == b'*' && bytes[k + 1] == b'/') {
                k += 1;
            }
            if k + 1 >= end {
                return None;
            }
            j = k + 2;
            continue;
        }
        let boundary = j == start || matches!(bytes[j - 1], b' ' | b'\t' | b'\n' | b'\r');
        if boundary && double_slash_comment(bytes, j, end) {
            let mut cut = j;
            while cut > start && (bytes[cut - 1] == b' ' || bytes[cut - 1] == b'\t') {
                cut -= 1;
            }
            return Some(cut);
        }
        j += 1;
    }
    None
}

/// Byte offset at which content should end when `src[start..end]` carries a
/// trailing `/* … */` block comment (optionally followed by spaces/tabs), or
/// `None` when it does not. Mirrors [`trailing_line_comment_start`] for the
/// `/* */` form so a heading like `= Title <lbl> /* note */` still attaches its
/// label instead of hiding it behind the comment.
///
/// Only a *closed* trailing comment is stripped; the returned offset excludes
/// the whitespace run immediately before `/*`. Comments are paired
/// left-to-right (each `/*` with its nearest `*/`), and only a comment whose
/// closer lands exactly at the range end counts as trailing — so a stray `*/`
/// cannot bind to an already-closed earlier opener and silently eat the prose
/// between them (`= H /* a */ text */`). An unterminated `/*` (no matching `*/`
/// at the line end) is left in place for the inline scanner to diagnose
/// (`MOS0050`).
#[must_use]
pub fn trailing_block_comment_start(bytes: &[u8], start: usize, end: usize) -> Option<usize> {
    let mut e = end;
    while e > start && matches!(bytes[e - 1], b' ' | b'\t') {
        e -= 1;
    }
    // Need at least `/**/` (four bytes) and a trailing `*/` closer.
    if e < start + 4 || bytes[e - 1] != b'/' || bytes[e - 2] != b'*' {
        return None;
    }
    let mut k = start;
    let mut trailing_open = None;
    while k + 1 < e {
        if bytes[k] == b'/' && bytes[k + 1] == b'*' {
            let mut close = k + 2;
            while close + 1 < e && !(bytes[close] == b'*' && bytes[close + 1] == b'/') {
                close += 1;
            }
            if close + 1 >= e {
                return None;
            }
            if close + 2 == e {
                trailing_open = Some(k);
            }
            k = close + 2;
        } else {
            k += 1;
        }
    }
    let mut cut = trailing_open?;
    while cut > start && matches!(bytes[cut - 1], b' ' | b'\t') {
        cut -= 1;
    }
    Some(cut)
}

/// Skip ASCII whitespace (space, tab, CR, LF) inside a `#set` body.
#[must_use]
pub fn skip_set_ws(bytes: &[u8], from: usize, end: usize) -> usize {
    let mut i = from;
    while i < end && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    i
}

/// Advance to the next `,` or end-of-body, used for error recovery
/// inside directive argument parsing.
#[must_use]
pub fn skip_to_comma(bytes: &[u8], from: usize, end: usize) -> usize {
    let mut i = from;
    while i < end && bytes[i] != b',' {
        i += 1;
    }
    i
}

/// Return the byte offset of the next character boundary at or after
/// `from + 1`. Used to step over a single Unicode scalar value when
/// accumulating string literal contents.
#[must_use]
pub const fn next_char_boundary(src: &str, from: usize) -> usize {
    let mut i = from + 1;
    while i < src.len() && !src.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Find `needle` in `haystack` starting at `from`.
#[must_use]
pub fn find_byte(haystack: &[u8], needle: u8, from: usize) -> Option<usize> {
    haystack[from..]
        .iter()
        .position(|&b| b == needle)
        .map(|p| p + from)
}

/// Return the byte offset just past a label-identifier run.
///
/// The accepted alphabet matches manifest §3.3 examples:
/// `[A-Za-z0-9_:.-]`. Critically `:` is included so `fig:wells` and
/// `eq:bayes` round-trip.
#[must_use]
pub fn scan_label_chars(bytes: &[u8], from: usize) -> usize {
    let mut i = from;
    while i < bytes.len() {
        let b = bytes[i];
        let is_id = b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b'.');
        if !is_id {
            break;
        }
        i += 1;
    }
    i
}

/// Normalize raw block text line endings and leading delimiter newline.
#[must_use]
pub fn normalize_raw_text(text: &str) -> String {
    let text = text
        .strip_prefix("\r\n")
        .or_else(|| text.strip_prefix('\n'))
        .or_else(|| text.strip_prefix('\r'))
        .unwrap_or(text);
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Parsed `<label>` text and source range.
#[derive(Debug)]
pub struct ParsedLabel {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// Strip one leading `<label>` from `src[start..end]`, if present.
///
/// Only a single leading label is recognised; further `<...>` runs in
/// the body are left intact for downstream stages.
#[must_use]
pub fn strip_leading_label(src: &str, start: usize, end: usize) -> (usize, Option<ParsedLabel>) {
    let bytes = src.as_bytes();
    let mut i = start;
    while i < end && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= end || bytes[i] != b'<' {
        return (start, None);
    }
    let id_start = i + 1;
    let id_end = scan_label_chars(bytes, id_start);
    if id_end == id_start || id_end >= end || bytes[id_end] != b'>' {
        return (start, None);
    }
    let label = ParsedLabel {
        text: src[id_start..id_end].to_owned(),
        start: id_start,
        end: id_end,
    };
    let mut after = id_end + 1;
    while after < end && (bytes[after] == b' ' || bytes[after] == b'\t' || bytes[after] == b'\n') {
        after += 1;
    }
    (after, Some(label))
}

/// Strip one trailing `<label>` from `src[start..end]`, if present.
///
/// The returned text end excludes whitespace before the label.
#[must_use]
pub fn strip_trailing_label(src: &str, start: usize, end: usize) -> (usize, Option<ParsedLabel>) {
    let bytes = src.as_bytes();
    if end <= start || bytes[end - 1] != b'>' {
        return (end, None);
    }
    let close = end - 1;
    // Walk back over identifier chars to find the matching `<`.
    let mut i = close;
    while i > start {
        let b = bytes[i - 1];
        let is_id = b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b'.');
        if !is_id {
            break;
        }
        i -= 1;
    }
    if i == close || i == start || bytes[i - 1] != b'<' {
        return (end, None);
    }
    // A `\<` opens literal angle-bracket text (e.g. an HTML tag), not a label.
    if is_escaped(bytes, start, i - 1) {
        return (end, None);
    }
    let label = ParsedLabel {
        text: src[i..close].to_owned(),
        start: i,
        end: close,
    };
    let mut text_end = i - 1;
    while text_end > start && (bytes[text_end - 1] == b' ' || bytes[text_end - 1] == b'\t') {
        text_end -= 1;
    }
    (text_end, Some(label))
}

/// Locate the first `<label>` token in `src[start..end]`.
///
/// Unlike [`strip_trailing_label`] this finds a label anywhere in the range, so
/// the heading parser can flag non-trailing labels with `MOS0048`.
#[must_use]
pub fn locate_label(src: &str, start: usize, end: usize) -> Option<(ParsedLabel, usize)> {
    let bytes = src.as_bytes();
    let mut open = start;
    while open < end {
        if bytes[open] == b'<' {
            let mut i = open + 1;
            while i < end {
                let b = bytes[i];
                if b == b'>' {
                    break;
                }
                if !(b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b'.')) {
                    break;
                }
                i += 1;
            }
            // A real label has at least one identifier byte and a closing `>`,
            // and its `<` must not be escaped (`\<` opens literal angle-bracket
            // text such as an HTML tag, not label syntax).
            if i < end && bytes[i] == b'>' && i > open + 1 && !is_escaped(bytes, start, open) {
                let label = ParsedLabel {
                    text: src[open + 1..i].to_owned(),
                    start: open + 1,
                    end: i,
                };
                return Some((label, i + 1));
            }
        }
        open += 1;
    }
    None
}

/// Whether the byte at `pos` is escaped by an odd-length run of `\` immediately
/// before it (bounded below by `start`). Backslashes pair up (`\\` is a hard
/// break, not an escape), so only an odd count escapes the following byte. The
/// label scanners use this so a `\<` opens literal `<` text instead of a label.
fn is_escaped(bytes: &[u8], start: usize, pos: usize) -> bool {
    let mut count = 0;
    let mut j = pos;
    while j > start && bytes[j - 1] == b'\\' {
        count += 1;
        j -= 1;
    }
    count % 2 == 1
}
