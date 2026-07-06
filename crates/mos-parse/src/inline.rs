use crate::parser::Parser;
use crate::support::{double_slash_comment, find_byte, scan_label_chars};
use mos_core::{Suggestion, codes};

use crate::{Inline, InlineKind};

#[derive(Clone, Copy, Debug, Default)]
enum InlineStyle {
    #[default]
    Plain,
    Emphasis,
    Strong,
    BoldItalic,
}

impl InlineStyle {
    const fn with(self, delimiter: Delimiter) -> Self {
        match delimiter {
            Delimiter::Strong => self.with_strong(),
            Delimiter::Emphasis => self.with_emphasis(),
        }
    }

    const fn with_strong(self) -> Self {
        match self {
            Self::Plain => Self::Strong,
            Self::Emphasis | Self::Strong | Self::BoldItalic => Self::BoldItalic,
        }
    }

    const fn with_emphasis(self) -> Self {
        match self {
            Self::Plain => Self::Emphasis,
            Self::Strong | Self::Emphasis | Self::BoldItalic => Self::BoldItalic,
        }
    }

    const fn kind(self) -> InlineKind {
        match self {
            Self::Plain => InlineKind::Text,
            Self::Emphasis => InlineKind::Emphasis,
            Self::Strong => InlineKind::Strong,
            Self::BoldItalic => InlineKind::BoldItalic,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Delimiter {
    Emphasis,
    Strong,
}

impl Delimiter {
    const fn width(self) -> usize {
        match self {
            Self::Emphasis => 1,
            Self::Strong => 2,
        }
    }

    const fn closing_text(self) -> &'static str {
        match self {
            Self::Emphasis => "*",
            Self::Strong => "**",
        }
    }
}

struct ParsedSegment {
    inlines: Vec<Inline>,
    next: usize,
    closed: Option<ClosedDelimiter>,
}

struct ClosedDelimiter {
    end: usize,
}

struct InlineSegmentParser<'parser, 'slice, 'src> {
    parser: &'parser mut Parser<'src>,
    slice: &'slice str,
    bytes: &'slice [u8],
    base: usize,
    out: Vec<Inline>,
    pending: String,
    pending_source_start: Option<usize>,
    i: usize,
    text_start: usize,
    style: InlineStyle,
    close: Option<Delimiter>,
}

impl<'parser, 'slice, 'src> InlineSegmentParser<'parser, 'slice, 'src> {
    const fn new(
        parser: &'parser mut Parser<'src>,
        slice: &'slice str,
        base: usize,
        from: usize,
        style: InlineStyle,
        close: Option<Delimiter>,
    ) -> Self {
        Self {
            parser,
            slice,
            bytes: slice.as_bytes(),
            base,
            out: Vec::new(),
            pending: String::new(),
            pending_source_start: None,
            i: from,
            text_start: from,
            style,
            close,
        }
    }

    fn parse(mut self) -> ParsedSegment {
        while self.i < self.bytes.len() {
            match self.bytes[self.i] {
                b'\\' => self.handle_backslash(),
                b'*' => {
                    if let Some(segment) = self.handle_star() {
                        return segment;
                    }
                }
                b'`' => self.handle_code(),
                b'@' => self.handle_reference(),
                b'/' if self.at_inline_block_comment() => self.handle_inline_block_comment(),
                b'/' if self.at_inline_comment() => self.handle_inline_comment(),
                b'[' if self.i + 1 < self.bytes.len() && self.bytes[self.i + 1] == b'@' => {
                    self.handle_citation();
                }
                _ => self.i += 1,
            }
        }
        self.flush(self.bytes.len());
        ParsedSegment {
            inlines: self.out,
            next: self.bytes.len(),
            closed: None,
        }
    }

    fn flush(&mut self, to: usize) {
        self.parser.flush_styled_text_with_pending(
            &mut self.out,
            self.slice,
            self.base,
            self.text_start,
            to,
            self.style,
            &mut self.pending,
            &mut self.pending_source_start,
        );
    }

    fn push_pending_escape(&mut self, text: char, width: usize) {
        if self.pending_source_start.is_none() {
            self.pending_source_start = Some(self.text_start);
        }
        self.pending.push_str(&self.slice[self.text_start..self.i]);
        self.pending.push(text);
        self.i += width;
        self.text_start = self.i;
    }

    fn handle_backslash(&mut self) {
        if self.i + 1 < self.bytes.len() && self.bytes[self.i + 1] == b'\\' {
            self.flush(self.i);
            self.out.push(Inline {
                kind: InlineKind::HardBreak,
                text: String::new(),
                span: self.parser.span(self.base + self.i, self.base + self.i + 2),
                label_span: None,
            });
            self.i += 2;
            self.text_start = self.i;
            return;
        }
        if self.i + 1 < self.bytes.len() && self.bytes[self.i + 1] == b'-' {
            self.push_pending_escape('\u{AD}', 2);
            return;
        }
        if self.i + 1 < self.bytes.len() && self.bytes[self.i + 1] == b'<' {
            self.push_pending_escape('<', 2);
            return;
        }
        if self.i + 1 < self.bytes.len() && self.bytes[self.i + 1] == b'*' {
            self.push_pending_escape('*', 2);
            return;
        }
        if self.i + 1 >= self.bytes.len() {
            self.parser.diagnostics.push(self.parser.warn(
                &codes::MOS0038,
                "lone trailing `\\` is not a recognized escape; treated as literal text",
                self.base + self.i,
                self.base + self.i + 1,
            ));
        }
        self.i += 1;
    }

    fn handle_star(&mut self) -> Option<ParsedSegment> {
        let run_len = star_run_len(self.bytes, self.i);
        if let Some(delimiter) = self.close
            && delimiter_closes(delimiter, run_len)
        {
            self.flush(self.i);
            let width = delimiter.width();
            return Some(ParsedSegment {
                inlines: std::mem::take(&mut self.out),
                next: self.i + width,
                closed: Some(ClosedDelimiter {
                    end: self.i + width,
                }),
            });
        }

        let delimiter = if run_len >= 2 {
            Delimiter::Strong
        } else {
            Delimiter::Emphasis
        };
        let diagnostic_checkpoint = self.parser.diagnostics.len();
        let parsed = self.parser.parse_inline_segment(
            self.slice,
            self.base,
            self.i + delimiter.width(),
            self.style.with(delimiter),
            Some(delimiter),
        );

        if let Some(closed) = parsed.closed {
            self.flush(self.i);
            let mut children = parsed.inlines;
            widen_span_to_delimiters(&mut children, self.base + self.i, self.base + closed.end);
            self.out.extend(children);
            self.i = parsed.next;
            self.text_start = self.i;
        } else {
            self.parser.diagnostics.truncate(diagnostic_checkpoint);
            if self.close.is_none() {
                self.parser
                    .warn_unterminated_delimiter(self.slice, self.base, self.i, delimiter);
            }
            self.i += delimiter.width();
        }
        None
    }

    fn handle_code(&mut self) {
        if let Some(end) = find_byte(self.bytes, b'`', self.i + 1) {
            self.flush(self.i);
            self.out.push(Inline {
                kind: InlineKind::Code,
                text: self.slice[self.i + 1..end].to_owned(),
                span: self.parser.span(self.base + self.i, self.base + end + 1),
                label_span: None,
            });
            self.i = end + 1;
            self.text_start = self.i;
            return;
        }
        let mut diagnostic = self.parser.warn(
            &codes::MOS0034,
            "unterminated `` `code` `` run; treated as text",
            self.base + self.i,
            self.base + self.i + 1,
        );
        if let Some(insertion) = Parser::code_closing_insertion(self.slice, self.i, self.close) {
            let insertion = self.base + insertion;
            diagnostic = diagnostic
                .with_suggestion(Suggestion::new(self.parser.span(insertion, insertion), "`"));
        }
        self.parser.diagnostics.push(diagnostic);
        self.i += 1;
    }

    /// Whether the cursor sits at a trailing/interior `//` line comment: the
    /// slashes must be at a whitespace boundary (slice start or a preceding
    /// space/tab/newline) and be followed by a space or end-of-line. Code spans
    /// and `@refs` are already consumed atomically before the cursor gets here,
    /// so this only ever fires in the default text state.
    fn at_inline_comment(&self) -> bool {
        let boundary =
            self.i == 0 || matches!(self.bytes[self.i - 1], b' ' | b'\t' | b'\n' | b'\r');
        boundary && double_slash_comment(self.bytes, self.i, self.bytes.len())
    }

    /// Drop a `//` comment from the cursor to the end of its line: flush text up
    /// to the comment (trimming the space/tab run before `//`, but never a
    /// joining newline), then resume scanning at the next `\n` (or slice end)
    /// without consuming it, so a following paragraph line is preserved.
    fn handle_inline_comment(&mut self) {
        let mut ws = self.i;
        while ws > self.text_start && matches!(self.bytes[ws - 1], b' ' | b'\t') {
            ws -= 1;
        }
        self.flush(ws);
        let mut j = self.i;
        while j < self.bytes.len() && self.bytes[j] != b'\n' {
            j += 1;
        }
        self.i = j;
        self.text_start = j;
    }

    /// Whether the cursor sits at a `/*` block-comment opener. Unlike the `//`
    /// line comment, a block comment is recognized anywhere (matching the
    /// editor grammar), not just at a whitespace boundary: verbatim contexts
    /// (code spans, directive strings, raw blocks) are consumed before the
    /// cursor reaches them, and `/*` does not occur in URLs, so no URL-safety
    /// carve-out is needed.
    fn at_inline_block_comment(&self) -> bool {
        self.i + 1 < self.bytes.len()
            && self.bytes[self.i] == b'/'
            && self.bytes[self.i + 1] == b'*'
    }

    /// Drop a `/* … */` block comment from the cursor to its closing `*/`:
    /// flush text up to the comment (trimming the space/tab run before `/*`,
    /// matching [`Self::handle_inline_comment`]), scan raw bytes across newlines
    /// for the first `*/`, then resume just past it. An unterminated `/*` is a
    /// recoverable `MOS0050` warning and consumes to the end of the slice.
    fn handle_inline_block_comment(&mut self) {
        let mut ws = self.i;
        while ws > self.text_start && matches!(self.bytes[ws - 1], b' ' | b'\t') {
            ws -= 1;
        }
        self.flush(ws);
        let mut j = self.i + 2;
        while j + 1 < self.bytes.len() {
            if self.bytes[j] == b'*' && self.bytes[j + 1] == b'/' {
                self.i = j + 2;
                self.text_start = self.i;
                return;
            }
            j += 1;
        }
        self.parser.diagnostics.push(self.parser.warn(
            &codes::MOS0050,
            "unterminated `/*` block comment; consumed to end of input",
            self.base + self.i,
            self.base + self.bytes.len(),
        ));
        self.i = self.bytes.len();
        self.text_start = self.bytes.len();
    }

    fn handle_reference(&mut self) {
        let id_end = scan_label_chars(self.bytes, self.i + 1);
        if id_end <= self.i + 1 {
            self.warn_stray_at();
            return;
        }
        if self.push_page_reference(id_end) {
            return;
        }
        self.flush(self.i);
        self.out.push(Inline {
            kind: InlineKind::Reference,
            text: self.slice[self.i + 1..id_end].to_owned(),
            span: self.parser.span(self.base + self.i, self.base + id_end),
            label_span: Some(self.parser.span(self.base + self.i + 1, self.base + id_end)),
        });
        self.i = id_end;
        self.text_start = self.i;
    }

    fn push_page_reference(&mut self, id_end: usize) -> bool {
        if &self.slice[self.i + 1..id_end] != "page"
            || id_end >= self.bytes.len()
            || self.bytes[id_end] != b'('
        {
            return false;
        }
        let label_start = id_end + 1;
        let label_end = scan_label_chars(self.bytes, label_start);
        if label_end <= label_start
            || label_end >= self.bytes.len()
            || self.bytes[label_end] != b')'
        {
            return false;
        }
        self.flush(self.i);
        self.out.push(Inline {
            kind: InlineKind::PageReference,
            text: self.slice[label_start..label_end].to_owned(),
            span: self
                .parser
                .span(self.base + self.i, self.base + label_end + 1),
            label_span: Some(
                self.parser
                    .span(self.base + label_start, self.base + label_end),
            ),
        });
        self.i = label_end + 1;
        self.text_start = self.i;
        true
    }

    fn warn_stray_at(&mut self) {
        self.parser.diagnostics.push(self.parser.warn(
            &codes::MOS0036,
            "stray `@` is not followed by a label identifier; treated as text",
            self.base + self.i,
            self.base + self.i + 1,
        ));
        self.i += 1;
    }

    fn handle_citation(&mut self) {
        let key_start = self.i + 2;
        let key_end = scan_label_chars(self.bytes, key_start);
        if key_end > key_start && key_end < self.bytes.len() && self.bytes[key_end] == b']' {
            self.flush(self.i);
            let end = key_end + 1;
            self.out.push(Inline {
                kind: InlineKind::Citation,
                text: self.slice[key_start..key_end].to_owned(),
                span: self.parser.span(self.base + self.i, self.base + end),
                label_span: None,
            });
            self.i = end;
            self.text_start = self.i;
            return;
        }
        let recovery_end =
            find_byte(self.bytes, b']', key_start).map_or(key_start, |close| close + 1);
        self.parser.diagnostics.push(self.parser.warn(
            &codes::MOS0039,
            "malformed citation `[@…]`; expected `[@key]`; treated as text",
            self.base + self.i,
            self.base + recovery_end,
        ));
        self.i = recovery_end;
    }
}

impl Parser<'_> {
    /// Tokenize `slice` (whose first byte sits at `base` in `self.src`)
    /// into inline runs. Backtick code and `@label` references are
    /// atomic; emphasis delimiters can nest into bold+italic text runs.
    pub(crate) fn parse_inlines(&mut self, slice: &str, base: usize) -> Vec<Inline> {
        self.parse_inline_segment(slice, base, 0, InlineStyle::default(), None)
            .inlines
    }

    fn parse_inline_segment(
        &mut self,
        slice: &str,
        base: usize,
        from: usize,
        style: InlineStyle,
        close: Option<Delimiter>,
    ) -> ParsedSegment {
        InlineSegmentParser::new(self, slice, base, from, style, close).parse()
    }

    /// Flush `slice[from..to]` (possibly prefixed by buffered `pending`
    /// text from earlier escape expansions like `\-` → U+00AD) into a
    /// single styled-text inline. The span covers the full source range
    /// from the earliest byte that fed `pending` (or `from` when pending
    /// is empty) through `to`, so emitted inlines whose text includes
    /// expanded escapes still carry a span covering the original source
    /// bytes: including the consumed `\-` markers.
    #[allow(
        clippy::too_many_arguments,
        reason = "transitional: extends the existing `flush_styled_text` (7-arg) with a buffered-text channel and a pending-source-start tracker for escape expansion. Bundling the slice/base/style triple into a context struct would churn every call site in `parse_inline_segment` for no net clarity."
    )]
    fn flush_styled_text_with_pending(
        &self,
        out: &mut Vec<Inline>,
        slice: &str,
        base: usize,
        from: usize,
        to: usize,
        style: InlineStyle,
        pending: &mut String,
        pending_source_start: &mut Option<usize>,
    ) {
        if pending.is_empty() {
            // Defensive: pending_source_start should always be paired
            // with a non-empty pending. Clear it anyway so a future
            // escape that splices into `pending` starts from a fresh
            // state.
            *pending_source_start = None;
            self.flush_styled_text(out, slice, base, from, to, style);
            return;
        }
        let mut text = std::mem::take(pending);
        if from < to {
            text.push_str(&slice[from..to]);
        }
        let span_from = pending_source_start.take().unwrap_or(from);
        out.push(Inline {
            kind: style.kind(),
            text,
            span: self.span(base + span_from, base + to),
            label_span: None,
        });
    }

    fn flush_styled_text(
        &self,
        out: &mut Vec<Inline>,
        slice: &str,
        base: usize,
        from: usize,
        to: usize,
        style: InlineStyle,
    ) {
        if from < to {
            out.push(Inline {
                kind: style.kind(),
                text: slice[from..to].to_owned(),
                span: self.span(base + from, base + to),
                label_span: None,
            });
        }
    }

    fn warn_unterminated_delimiter(
        &mut self,
        slice: &str,
        base: usize,
        i: usize,
        delimiter: Delimiter,
    ) {
        let (def, message) = match delimiter {
            Delimiter::Strong => (
                &codes::MOS0028,
                "unterminated `**strong**` run; treated as text",
            ),
            Delimiter::Emphasis => (
                &codes::MOS0031,
                "unterminated `*emphasis*` run; treated as text",
            ),
        };
        let mut diagnostic = self.warn(def, message, base + i, base + i + delimiter.width());
        if let Some(suggestion) = self.closing_delimiter_suggestion(slice, base, i, delimiter) {
            diagnostic = diagnostic.with_suggestion(suggestion);
        }
        self.diagnostics.push(diagnostic);
    }

    fn closing_delimiter_suggestion(
        &self,
        slice: &str,
        base: usize,
        i: usize,
        delimiter: Delimiter,
    ) -> Option<Suggestion> {
        let after_opener = i + delimiter.width();
        if slice.as_bytes()[after_opener..].contains(&b'*') {
            return None;
        }
        let insertion = base + slice.len();
        Some(Suggestion::new(
            self.span(insertion, insertion),
            delimiter.closing_text(),
        ))
    }

    fn code_closing_insertion(slice: &str, i: usize, close: Option<Delimiter>) -> Option<usize> {
        let bytes = slice.as_bytes();
        let mut cursor = i + 1;
        while cursor < bytes.len() {
            if bytes[cursor] == b'*' {
                let run_len = star_run_len(bytes, cursor);
                if close.is_some_and(|delimiter| delimiter_closes(delimiter, run_len)) {
                    return Some(cursor);
                }
                return None;
            }
            cursor += 1;
        }
        Some(bytes.len())
    }
}

fn star_run_len(bytes: &[u8], from: usize) -> usize {
    let mut end = from;
    while end < bytes.len() && bytes[end] == b'*' {
        end += 1;
    }
    end - from
}

const fn delimiter_closes(delimiter: Delimiter, run_len: usize) -> bool {
    match delimiter {
        Delimiter::Strong => run_len >= 2,
        Delimiter::Emphasis => run_len % 2 == 1,
    }
}

fn widen_span_to_delimiters(inlines: &mut [Inline], start: usize, end: usize) {
    if let Some(first) = inlines.first_mut() {
        first.span.set_start(start);
    }
    if let Some(last) = inlines.last_mut() {
        last.span.set_end(end);
    }
}
