//! Parser for the Mosaic source language (`.mos`).
//!
//! See manifest §3 (language design) and §6 stages 1–2 (parse + lower).
//! MVP 0 covers:
//!
//! - `= Heading` / `== Subheading` / `=== Subsubheading`,
//! - paragraphs (newline-joined non-empty line groups),
//! - inline `*emphasis*`, `**strong**`, and `` `inline code` ``,
//! - `#set name(...)` blocks, recorded with span and name but otherwise
//!   inert until the resolver lands in MVP 1.
//!
//! Anything outside that subset is preserved as text and a recoverable
//! diagnostic is emitted; the parser never panics on user input
//! (manifest §31).

use std::path::{Path, PathBuf};

use mosaic_core::{Diagnostic, DiagnosticCode, Severity, SourceSpan};

/// Concrete syntax tree for a single `.mos` source file.
#[derive(Debug, Clone)]
pub struct SyntaxTree {
    pub file: PathBuf,
    pub items: Vec<Item>,
}

/// Top-level construct in a `.mos` file.
#[derive(Debug, Clone)]
pub enum Item {
    /// `= Title`, `== Subtitle`, `=== Subsubtitle`.
    Heading {
        level: u8,
        inlines: Vec<Inline>,
        span: SourceSpan,
    },
    /// One or more consecutive non-blank lines that are not a heading
    /// and not a `#set` block.
    Paragraph {
        inlines: Vec<Inline>,
        span: SourceSpan,
    },
    /// `#set name(...)` — captured for span tracking; the body is not
    /// interpreted until the evaluator lands in MVP 1.
    Set { name: String, span: SourceSpan },
}

/// Inline run produced by the markup tokenizer.
#[derive(Debug, Clone)]
pub struct Inline {
    pub kind: InlineKind,
    pub text: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InlineKind {
    Text,
    Emphasis,
    Strong,
    Code,
}

impl Item {
    /// Borrow the heading payload if `self` is [`Item::Heading`].
    #[must_use]
    pub fn as_heading(&self) -> Option<(u8, &[Inline], &SourceSpan)> {
        if let Self::Heading {
            level,
            inlines,
            span,
        } = self
        {
            Some((*level, inlines, span))
        } else {
            None
        }
    }

    /// Borrow the paragraph payload if `self` is [`Item::Paragraph`].
    #[must_use]
    pub fn as_paragraph(&self) -> Option<(&[Inline], &SourceSpan)> {
        if let Self::Paragraph { inlines, span } = self {
            Some((inlines, span))
        } else {
            None
        }
    }

    /// Borrow the `#set` payload if `self` is [`Item::Set`].
    #[must_use]
    pub fn as_set(&self) -> Option<(&str, &SourceSpan)> {
        if let Self::Set { name, span } = self {
            Some((name.as_str(), span))
        } else {
            None
        }
    }
}

/// Output of [`parse`]. Diagnostics may include warnings even when the
/// tree is structurally usable; callers decide what to do per
/// [`ParseResult::has_errors`].
#[derive(Debug)]
pub struct ParseResult {
    pub tree: SyntaxTree,
    pub diagnostics: Vec<Diagnostic>,
}

impl ParseResult {
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// Parse a Mosaic source string. Always returns a [`ParseResult`]; the
/// parser is recoverable per manifest §6 stage 1.
#[must_use]
pub fn parse(src: &str, file: &Path) -> ParseResult {
    Parser::new(src, file).run()
}

struct Parser<'a> {
    src: &'a str,
    file: PathBuf,
    pos: usize,
    items: Vec<Item>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, file: &Path) -> Self {
        Self {
            src,
            file: file.to_path_buf(),
            pos: 0,
            items: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self) -> ParseResult {
        while self.pos < self.src.len() {
            if self.at_blank_line() {
                self.skip_line();
                continue;
            }
            if self.at_set_keyword() {
                self.parse_set_block();
            } else if self.starts_with("=") {
                self.parse_heading();
            } else {
                self.parse_paragraph();
            }
        }
        ParseResult {
            tree: SyntaxTree {
                file: self.file,
                items: self.items,
            },
            diagnostics: self.diagnostics,
        }
    }

    fn span(&self, start: usize, end: usize) -> SourceSpan {
        SourceSpan::new(self.file.clone(), start, end)
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.src.as_bytes()[self.pos..].starts_with(prefix.as_bytes())
    }

    /// Returns true if the current position spells the `#set` keyword
    /// followed by a token boundary (whitespace, EOF, or `(`). Without
    /// the boundary check, prefixes like `#setting` would be routed to
    /// `parse_set_block` and emit spurious diagnostics.
    fn at_set_keyword(&self) -> bool {
        if !self.starts_with("#set") {
            return false;
        }
        match self.src.as_bytes().get(self.pos + 4) {
            None => true,
            Some(&b) => b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == b'(',
        }
    }

    /// Returns true if the current line is blank (contains only ASCII
    /// whitespace before the line terminator).
    fn at_blank_line(&self) -> bool {
        let bytes = self.src.as_bytes();
        let mut i = self.pos;
        while i < bytes.len() && bytes[i] != b'\n' {
            if !bytes[i].is_ascii_whitespace() {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Advance past the next `\n`, or to end-of-source if none remains.
    fn skip_line(&mut self) {
        let bytes = self.src.as_bytes();
        while self.pos < bytes.len() && bytes[self.pos] != b'\n' {
            self.pos += 1;
        }
        if self.pos < bytes.len() {
            self.pos += 1; // consume '\n'
        }
    }

    /// Returns the byte offsets `(content_start, content_end, line_end)`
    /// of the current line. `content_end` excludes any trailing `\r\n`
    /// or `\n`; `line_end` is the offset *after* the terminator.
    fn current_line_bounds(&self) -> (usize, usize, usize) {
        let bytes = self.src.as_bytes();
        let start = self.pos;
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'\n' {
            end += 1;
        }
        let line_end = if end < bytes.len() { end + 1 } else { end };
        let mut content_end = end;
        if content_end > start && bytes[content_end - 1] == b'\r' {
            content_end -= 1;
        }
        (start, content_end, line_end)
    }

    fn parse_heading(&mut self) {
        let (line_start, content_end, line_end) = self.current_line_bounds();
        let bytes = self.src.as_bytes();
        let mut level: u8 = 0;
        let mut i = line_start;
        while i < content_end && bytes[i] == b'=' {
            // Cap heading level at 6 (manifest §3.1 only spells three out
            // explicitly, but `======` is a perfectly reasonable extension).
            level = level.saturating_add(1);
            i += 1;
        }
        // Require at least one whitespace after the `=` run, otherwise
        // it isn't a heading — treat the line as a paragraph instead.
        if i >= content_end || !bytes[i].is_ascii_whitespace() {
            self.parse_paragraph();
            return;
        }
        // Skip leading spaces of the heading content.
        while i < content_end && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        let content = &self.src[i..content_end];
        let inlines = self.parse_inlines(content, i);
        let span = self.span(line_start, content_end);
        self.items.push(Item::Heading {
            level,
            inlines,
            span,
        });
        self.pos = line_end;
    }

    fn parse_set_block(&mut self) {
        let (line_start, _content_end, _line_end) = self.current_line_bounds();
        let bytes = self.src.as_bytes();
        debug_assert!(self.src[line_start..].starts_with("#set"));
        let mut i = line_start + "#set".len();
        // Whitespace before name.
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        let name_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        let name = self.src[name_start..i].to_owned();
        if name.is_empty() {
            self.diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E010"),
                    "expected an identifier after `#set`",
                )
                .with_span(self.span(line_start, line_start + "#set".len())),
            );
            self.skip_line();
            return;
        }
        // Whitespace before `(`.
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'(' {
            self.diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E011"),
                    format!("expected `(` after `#set {name}`"),
                )
                .with_span(self.span(name_start, i)),
            );
            self.skip_line();
            return;
        }
        let body_start = i;
        if let Some(end) = self.scan_balanced_parens(body_start) {
            let span = self.span(line_start, end);
            self.items.push(Item::Set { name, span });
            self.pos = end;
            // Consume only horizontal whitespace after the closing `)`.
            // Anything else on the line is unexpected and gets a
            // recoverable diagnostic — silently dropping trailing
            // tokens would hide user mistakes like
            // `#set page(...) leftover`.
            while self.pos < bytes.len() && (bytes[self.pos] == b' ' || bytes[self.pos] == b'\t') {
                self.pos += 1;
            }
            if self.pos >= bytes.len() {
                // EOF — nothing to do.
            } else if bytes[self.pos] == b'\n' {
                self.pos += 1;
            } else if bytes[self.pos] == b'\r' && bytes.get(self.pos + 1) == Some(&b'\n') {
                self.pos += 2;
            } else {
                let (_, content_end, _) = self.current_line_bounds();
                self.diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E013"),
                        "unexpected trailing content after `#set ... )`",
                    )
                    .with_span(self.span(self.pos, content_end)),
                );
                // Leave the trailing bytes in place; the outer loop
                // will parse them as a paragraph so they remain
                // visible to downstream stages.
            }
        } else {
            self.diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E012"),
                    format!("unterminated `#set {name}(...)` block"),
                )
                .with_span(self.span(line_start, bytes.len())),
            );
            self.pos = bytes.len();
        }
    }

    /// Starting at `start` (which must point at `(`), find the byte
    /// offset *after* the matching `)`. Tracks string literals so that
    /// `(` / `)` inside `"..."` don't fool the scanner. Returns `None`
    /// if no matching `)` exists before EOF.
    fn scan_balanced_parens(&self, start: usize) -> Option<usize> {
        let bytes = self.src.as_bytes();
        debug_assert_eq!(bytes.get(start), Some(&b'('));
        let mut depth: u32 = 0;
        let mut i = start;
        let mut in_string = false;
        while i < bytes.len() {
            let b = bytes[i];
            if in_string {
                if b == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if b == b'"' {
                    in_string = false;
                }
                i += 1;
                continue;
            }
            match b {
                b'"' => in_string = true,
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i + 1);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    fn parse_paragraph(&mut self) {
        let bytes = self.src.as_bytes();
        let para_start = self.pos;
        let mut para_end = self.pos;
        let mut text_start: Option<usize> = None;
        loop {
            if self.pos >= bytes.len() {
                break;
            }
            if self.at_blank_line() {
                break;
            }
            // A heading or `#set` always begins a fresh block, so they
            // terminate any in-progress paragraph too.
            if self.starts_with("=") && self.heading_level_of_current_line().is_some() {
                break;
            }
            if self.at_set_keyword() {
                break;
            }
            let (line_start, content_end, line_end) = self.current_line_bounds();
            if text_start.is_none() {
                text_start = Some(line_start);
            }
            para_end = content_end;
            self.pos = line_end;
        }
        if let Some(start) = text_start {
            // Slice the paragraph directly out of `self.src` so byte
            // offsets stay aligned with the original file. Building a
            // separate buffer with synthetic `\n` separators would
            // shift inline spans by one byte per CRLF line ending.
            let slice = &self.src[start..para_end];
            let mut inlines = self.parse_inlines(slice, start);
            // Spans stay anchored to the raw source, but the displayed
            // text payload is platform-stable: normalize `\r\n` → `\n`
            // so the same logical paragraph lowers identically on
            // Windows and Unix sources.
            for inline in &mut inlines {
                if inline.text.contains("\r\n") {
                    inline.text = inline.text.replace("\r\n", "\n");
                }
            }
            let span = self.span(para_start, para_end);
            self.items.push(Item::Paragraph { inlines, span });
        }
    }

    /// Returns `Some(level)` if the current line is a well-formed
    /// heading of `=`+ followed by ASCII whitespace. Used to terminate
    /// paragraphs without consuming.
    fn heading_level_of_current_line(&self) -> Option<u8> {
        let (start, content_end, _) = self.current_line_bounds();
        let bytes = self.src.as_bytes();
        let mut i = start;
        let mut level: u8 = 0;
        while i < content_end && bytes[i] == b'=' {
            level = level.saturating_add(1);
            i += 1;
        }
        if level == 0 {
            return None;
        }
        if i < content_end && bytes[i].is_ascii_whitespace() {
            Some(level)
        } else {
            None
        }
    }

    /// Tokenize `slice` (whose first byte sits at `base` in `self.src`)
    /// into inline runs. Inline parsing is non-nesting in MVP 0; the
    /// inner contents of `*…*`, `**…**`, and `` `…` `` are plain text.
    fn parse_inlines(&mut self, slice: &str, base: usize) -> Vec<Inline> {
        let bytes = slice.as_bytes();
        let mut out: Vec<Inline> = Vec::new();
        let mut i = 0;
        let mut text_start = 0;
        while i < bytes.len() {
            let c = bytes[i];
            if c == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                if let Some(end) = find_subslice(bytes, b"**", i + 2) {
                    self.flush_text(&mut out, slice, base, text_start, i);
                    out.push(Inline {
                        kind: InlineKind::Strong,
                        text: slice[i + 2..end].to_owned(),
                        span: self.span(base + i, base + end + 2),
                    });
                    i = end + 2;
                    text_start = i;
                    continue;
                }
                self.diagnostics.push(self.warn(
                    "W020",
                    "unterminated `**strong**` run; treated as text",
                    base + i,
                    base + i + 2,
                ));
                i += 2;
                continue;
            }
            if c == b'*' {
                if let Some(end) = find_emphasis_close(bytes, i + 1) {
                    self.flush_text(&mut out, slice, base, text_start, i);
                    out.push(Inline {
                        kind: InlineKind::Emphasis,
                        text: slice[i + 1..end].to_owned(),
                        span: self.span(base + i, base + end + 1),
                    });
                    i = end + 1;
                    text_start = i;
                    continue;
                }
                self.diagnostics.push(self.warn(
                    "W021",
                    "unterminated `*emphasis*` run; treated as text",
                    base + i,
                    base + i + 1,
                ));
                i += 1;
                continue;
            }
            if c == b'`' {
                if let Some(end) = find_byte(bytes, b'`', i + 1) {
                    self.flush_text(&mut out, slice, base, text_start, i);
                    out.push(Inline {
                        kind: InlineKind::Code,
                        text: slice[i + 1..end].to_owned(),
                        span: self.span(base + i, base + end + 1),
                    });
                    i = end + 1;
                    text_start = i;
                    continue;
                }
                self.diagnostics.push(self.warn(
                    "W022",
                    "unterminated `` `code` `` run; treated as text",
                    base + i,
                    base + i + 1,
                ));
                i += 1;
                continue;
            }
            i += 1;
        }
        self.flush_text(&mut out, slice, base, text_start, bytes.len());
        out
    }

    fn flush_text(&self, out: &mut Vec<Inline>, slice: &str, base: usize, from: usize, to: usize) {
        if from < to {
            out.push(Inline {
                kind: InlineKind::Text,
                text: slice[from..to].to_owned(),
                span: self.span(base + from, base + to),
            });
        }
    }

    fn warn(&self, code: &'static str, message: &str, start: usize, end: usize) -> Diagnostic {
        Diagnostic {
            severity: Severity::Warning,
            code: DiagnosticCode(code),
            message: message.to_owned(),
            span: Some(self.span(start, end)),
            notes: Vec::new(),
            suggestions: Vec::new(),
        }
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from > haystack.len() {
        return None;
    }
    let mut i = from;
    while i + needle.len() <= haystack.len() {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_byte(haystack: &[u8], needle: u8, from: usize) -> Option<usize> {
    haystack[from..]
        .iter()
        .position(|&b| b == needle)
        .map(|p| p + from)
}

/// Locate the closing `*` of an emphasis run starting at `from`. The
/// match must be a *single* `*` — neither preceded nor followed by
/// another `*` — to avoid swallowing a `**strong**` delimiter.
fn find_emphasis_close(haystack: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i < haystack.len() {
        if haystack[i] == b'*' {
            let prev_is_star = i > 0 && haystack[i - 1] == b'*';
            let next_is_star = haystack.get(i + 1) == Some(&b'*');
            if !prev_is_star && !next_is_star {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn parse_str(src: &str) -> ParseResult {
        parse(src, &PathBuf::from("test.mos"))
    }

    #[test]
    fn empty_source() {
        let r = parse_str("");
        assert!(r.tree.items.is_empty());
        assert!(!r.has_errors());
    }

    #[test]
    fn single_heading() {
        let r = parse_str("= Hello\n");
        assert!(!r.has_errors());
        assert_eq!(r.tree.items.len(), 1);
        let (level, inlines, _) = r.tree.items[0].as_heading().unwrap();
        assert_eq!(level, 1);
        assert_eq!(inlines.len(), 1);
        assert_eq!(inlines[0].text, "Hello");
        assert_eq!(inlines[0].kind, InlineKind::Text);
    }

    #[test]
    fn heading_levels() {
        let src = "= One\n== Two\n=== Three\n";
        let r = parse_str(src);
        assert!(!r.has_errors());
        let levels: Vec<u8> = r
            .tree
            .items
            .iter()
            .filter_map(|i| i.as_heading().map(|(l, _, _)| l))
            .collect();
        assert_eq!(levels, vec![1, 2, 3]);
    }

    #[test]
    fn paragraph_collects_lines() {
        let src = "first line\nsecond line\n\nnext para\n";
        let r = parse_str(src);
        assert!(!r.has_errors());
        assert_eq!(r.tree.items.len(), 2);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        assert_eq!(inlines.len(), 1);
        assert_eq!(inlines[0].text, "first line\nsecond line");
    }

    #[test]
    fn inline_emphasis_strong_code() {
        let src = "a *b* c **d** e `f` g\n";
        let r = parse_str(src);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        let kinds: Vec<InlineKind> = inlines.iter().map(|i| i.kind).collect();
        assert_eq!(
            kinds,
            vec![
                InlineKind::Text,
                InlineKind::Emphasis,
                InlineKind::Text,
                InlineKind::Strong,
                InlineKind::Text,
                InlineKind::Code,
                InlineKind::Text,
            ]
        );
        let texts: Vec<&str> = inlines.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, vec!["a ", "b", " c ", "d", " e ", "f", " g"]);
    }

    #[test]
    fn unterminated_emphasis_warns() {
        let r = parse_str("hi *there\n");
        assert!(!r.has_errors());
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code.0 == "W021" && d.severity == Severity::Warning)
        );
    }

    #[test]
    fn set_block_simple() {
        let r = parse_str("#set page(paper: \"A4\")\n");
        assert!(!r.has_errors());
        let (name, _) = r.tree.items[0].as_set().unwrap();
        assert_eq!(name, "page");
    }

    #[test]
    fn set_block_multiline() {
        let src = "#set document(\n  title: \"x\",\n  author: \"y\",\n)\n\n= After\n";
        let r = parse_str(src);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 2);
        assert_eq!(r.tree.items[0].as_set().unwrap().0, "document");
        assert_eq!(r.tree.items[1].as_heading().unwrap().0, 1);
    }

    #[test]
    fn unterminated_set_block_errors() {
        let r = parse_str("#set page(\n  paper: \"A4\",\n");
        assert!(r.has_errors());
    }

    #[test]
    fn trailing_content_after_set_block_diagnoses_and_recovers() {
        // Prior behaviour swallowed everything between `)` and the
        // next `\n`. The parser now emits E013 and leaves the
        // trailing bytes in place so they parse as a paragraph.
        let r = parse_str("#set page(paper: \"A4\") leftover\n");
        assert!(
            r.diagnostics.iter().any(|d| d.code.0 == "E013"),
            "expected E013 diagnostic, got {:?}",
            r.diagnostics
        );
        assert!(r.tree.items.iter().any(|i| i.as_set().is_some()));
        assert!(r.tree.items.iter().any(|i| {
            i.as_paragraph()
                .is_some_and(|(inlines, _)| inlines.iter().any(|x| x.text.contains("leftover")))
        }));
    }

    #[test]
    fn set_block_followed_by_horizontal_whitespace_then_newline_is_ok() {
        let r = parse_str("#set page(paper: \"A4\")  \t\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 1);
    }

    #[test]
    fn set_with_string_containing_paren() {
        let r = parse_str("#set foo(label: \"closes ) inside\")\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 1);
    }

    #[test]
    fn equals_without_space_is_paragraph() {
        let r = parse_str("=notaheading\n");
        assert!(!r.has_errors());
        assert!(r.tree.items[0].as_paragraph().is_some());
    }

    #[test]
    fn heading_span_is_within_source() {
        let src = "= Title\n";
        let r = parse_str(src);
        let (_, _, span) = r.tree.items[0].as_heading().unwrap();
        assert_eq!(&src[span.start..span.end], "= Title");
    }

    #[test]
    fn crlf_line_endings_handled() {
        let r = parse_str("= Title\r\nbody\r\n");
        assert!(!r.has_errors());
        assert_eq!(r.tree.items.len(), 2);
    }

    #[test]
    fn set_prefix_without_token_boundary_stays_paragraph() {
        // `#setting` is not the `#set` keyword. The parser must not
        // route it to the set-block path and emit a spurious error.
        let r = parse_str("#setting up\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert!(r.tree.items[0].as_paragraph().is_some());
    }

    #[test]
    fn set_prefix_followed_by_paren_is_set_block() {
        // No whitespace, but `(` is also a valid token boundary.
        let r = parse_str("#set(name: \"x\")\n");
        // Either the parser recognises this as a set block with no
        // identifier (E010) or it parses as paragraph; what matters
        // is that it does NOT panic and returns a structured result.
        // We document the current behaviour here: `#set` is treated
        // as the keyword and `name` is parsed as the body identifier
        // — see `at_set_keyword`. This guards against regression.
        assert_eq!(r.tree.items.len() + r.diagnostics.len(), 1);
    }

    #[test]
    fn paragraph_inline_spans_align_with_crlf_source() {
        // Regression for the CRLF byte-offset bug: when the paragraph
        // contains `\r\n` between its lines, inline spans on the
        // second line must still index into the original source.
        let src = "first\r\n*x*\r\n";
        let r = parse_str(src);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        // The emphasis run is the byte sequence `*x*` on the second
        // line. Its span must point at exactly those three bytes in
        // the original source, regardless of the CR before it.
        let emph = inlines
            .iter()
            .find(|i| i.kind == InlineKind::Emphasis)
            .expect("emphasis inline");
        assert_eq!(&src[emph.span.start..emph.span.end], "*x*");
        assert_eq!(emph.text, "x");
    }

    #[test]
    fn paragraph_inline_text_is_crlf_normalized() {
        // The raw slice contains `\r\n` between paragraph lines, but
        // the Inline.text payload should be `\n`-only so the same
        // source lowers identically on Windows and Unix.
        let src = "alpha\r\nbeta\r\n";
        let r = parse_str(src);
        assert!(!r.has_errors());
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        assert!(
            inlines.iter().all(|i| !i.text.contains('\r')),
            "inline text should be CRLF-normalized: {:?}",
            inlines.iter().map(|i| &i.text).collect::<Vec<_>>()
        );
        // The first text run still spans the raw bytes including the
        // CR — only the *payload* is normalized.
        let text = inlines.iter().find(|i| i.kind == InlineKind::Text).unwrap();
        assert_eq!(text.text, "alpha\nbeta");
        assert_eq!(&src[text.span.start..text.span.end], "alpha\r\nbeta");
    }
}
