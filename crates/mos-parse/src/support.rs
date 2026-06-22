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
