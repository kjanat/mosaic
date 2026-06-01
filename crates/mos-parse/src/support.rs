//! Internal parser support helpers.

/// One marker line captured during list collection. Not user-facing --
/// the public AST uses [`crate::ListItem`] after nesting is resolved.
#[derive(Debug, Clone, Copy)]
pub(super) struct RawListLine {
    /// Byte count of ASCII spaces before the marker.
    pub(super) indent: usize,
    /// `true` for `\d+\. `, `false` for `- `.
    pub(super) ordered: bool,
    /// Byte offset (into `Parser::src`) of the first content byte
    /// after the marker and its trailing whitespace.
    pub(super) content_start: usize,
    /// Byte offset of the line's content end (excluding any `\r\n` or
    /// `\n` terminator).
    pub(super) content_end: usize,
    /// Byte offset of the start of the line (the first leading-space
    /// byte). Used for the item's `SourceSpan`.
    pub(super) line_start: usize,
}

/// If the line that starts at `pos` opens with a list marker, return
/// `Some((indent, ordered, content_start))`. `indent` counts the
/// leading ASCII spaces before the marker; `ordered` is `true` for
/// `\d+\. ` and `false` for `- `; `content_start` is the byte offset
/// of the first byte after the marker plus its trailing whitespace
/// run. Tabs are not recognised as either indent or post-marker
/// whitespace in MVP 0.
pub(super) fn list_marker_at(bytes: &[u8], pos: usize) -> Option<(usize, bool, usize)> {
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
pub(super) fn skip_set_ws(bytes: &[u8], from: usize, end: usize) -> usize {
    let mut i = from;
    while i < end && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    i
}

/// Advance to the next `,` or end-of-body, used for error recovery
/// inside directive argument parsing.
pub(super) fn skip_to_comma(bytes: &[u8], from: usize, end: usize) -> usize {
    let mut i = from;
    while i < end && bytes[i] != b',' {
        i += 1;
    }
    i
}

/// Return the byte offset of the next character boundary at or after
/// `from + 1`. Used to step over a single Unicode scalar value when
/// accumulating string literal contents.
pub(super) fn next_char_boundary(src: &str, from: usize) -> usize {
    let mut i = from + 1;
    while i < src.len() && !src.is_char_boundary(i) {
        i += 1;
    }
    i
}

pub(super) fn find_byte(haystack: &[u8], needle: u8, from: usize) -> Option<usize> {
    haystack[from..]
        .iter()
        .position(|&b| b == needle)
        .map(|p| p + from)
}

/// Returns the byte offset just past the longest label-identifier run
/// that starts at `from` in `bytes`. Empty (caller should detect via
/// `id_end == from`) if the first byte is not a valid identifier char.
///
/// The accepted alphabet matches manifest §3.3 examples:
/// `[A-Za-z0-9_:.-]`. Critically `:` is included so `fig:wells` and
/// `eq:bayes` round-trip.
pub(super) fn scan_label_chars(bytes: &[u8], from: usize) -> usize {
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

pub(super) fn normalize_raw_text(text: &str) -> String {
    let text = text
        .strip_prefix("\r\n")
        .or_else(|| text.strip_prefix('\n'))
        .or_else(|| text.strip_prefix('\r'))
        .unwrap_or(text);
    text.replace("\r\n", "\n").replace('\r', "\n")
}

pub(super) struct ParsedLabel {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// If the substring `src[start..end]` begins with optional ASCII
/// whitespace followed by `<label>`, return `(label_body_start, Some(id))`
/// where `label_body_start` is the offset just past the closing `>`
/// (with any trailing whitespace also consumed). Otherwise return
/// `(start, None)`.
///
/// Only a single leading label is recognised; further `<...>` runs in
/// the body are left intact for downstream stages.
pub(super) fn strip_leading_label(
    src: &str,
    start: usize,
    end: usize,
) -> (usize, Option<ParsedLabel>) {
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

/// If the substring `src[start..end]` ends with `<label>` (after any
/// trailing ASCII whitespace), return `(text_end, Some(id))` where
/// `text_end` is the offset of the first byte to *exclude* from the
/// preceding text -- trailing whitespace before the label is also
/// trimmed. Otherwise return `(end, None)`.
pub(super) fn strip_trailing_label(
    src: &str,
    start: usize,
    end: usize,
) -> (usize, Option<ParsedLabel>) {
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
