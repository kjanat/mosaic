use crate::parser::Parser;
use crate::support::{find_byte, scan_label_chars};
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
    fn with(self, delimiter: Delimiter) -> Self {
        match delimiter {
            Delimiter::Strong => self.with_strong(),
            Delimiter::Emphasis => self.with_emphasis(),
        }
    }

    fn with_strong(self) -> Self {
        match self {
            Self::Plain => Self::Strong,
            Self::Emphasis | Self::Strong | Self::BoldItalic => Self::BoldItalic,
        }
    }

    fn with_emphasis(self) -> Self {
        match self {
            Self::Plain => Self::Emphasis,
            Self::Strong | Self::Emphasis | Self::BoldItalic => Self::BoldItalic,
        }
    }

    fn kind(self) -> InlineKind {
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
    fn width(self) -> usize {
        match self {
            Self::Emphasis => 1,
            Self::Strong => 2,
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
        let bytes = slice.as_bytes();
        let mut out: Vec<Inline> = Vec::new();
        let mut i = from;
        let mut text_start = from;
        while i < bytes.len() {
            let c = bytes[i];
            if c == b'*' {
                let run_len = star_run_len(bytes, i);
                if let Some(delimiter) = close
                    && delimiter_closes(delimiter, run_len)
                {
                    self.flush_styled_text(&mut out, slice, base, text_start, i, style);
                    let width = delimiter.width();
                    return ParsedSegment {
                        inlines: out,
                        next: i + width,
                        closed: Some(ClosedDelimiter { end: i + width }),
                    };
                }

                let delimiter = if run_len >= 2 {
                    Delimiter::Strong
                } else {
                    Delimiter::Emphasis
                };
                let diagnostic_checkpoint = self.diagnostics.len();
                let parsed = self.parse_inline_segment(
                    slice,
                    base,
                    i + delimiter.width(),
                    style.with(delimiter),
                    Some(delimiter),
                );

                if let Some(closed) = parsed.closed {
                    self.flush_styled_text(&mut out, slice, base, text_start, i, style);
                    let mut children = parsed.inlines;
                    widen_span_to_delimiters(&mut children, base + i, base + closed.end);
                    out.extend(children);
                    i = parsed.next;
                    text_start = i;
                    continue;
                }

                self.diagnostics.truncate(diagnostic_checkpoint);
                if close.is_none() {
                    self.warn_unterminated_delimiter(base, i, delimiter);
                }
                i += delimiter.width();
                continue;
            }
            if c == b'`' {
                if let Some(end) = find_byte(bytes, b'`', i + 1) {
                    self.flush_styled_text(&mut out, slice, base, text_start, i, style);
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
            if c == b'@' {
                let id_end = scan_label_chars(bytes, i + 1);
                if id_end > i + 1 {
                    self.flush_styled_text(&mut out, slice, base, text_start, i, style);
                    out.push(Inline {
                        kind: InlineKind::Reference,
                        text: slice[i + 1..id_end].to_owned(),
                        span: self.span(base + i, base + id_end),
                    });
                    i = id_end;
                    text_start = i;
                    continue;
                }
                self.diagnostics.push(self.warn(
                    "W023",
                    "stray `@` is not followed by a label identifier; treated as text",
                    base + i,
                    base + i + 1,
                ));
                i += 1;
                continue;
            }
            i += 1;
        }
        self.flush_styled_text(&mut out, slice, base, text_start, bytes.len(), style);
        ParsedSegment {
            inlines: out,
            next: bytes.len(),
            closed: None,
        }
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
            });
        }
    }

    fn warn_unterminated_delimiter(&mut self, base: usize, i: usize, delimiter: Delimiter) {
        match delimiter {
            Delimiter::Strong => self.diagnostics.push(self.warn(
                "W020",
                "unterminated `**strong**` run; treated as text",
                base + i,
                base + i + 2,
            )),
            Delimiter::Emphasis => self.diagnostics.push(self.warn(
                "W021",
                "unterminated `*emphasis*` run; treated as text",
                base + i,
                base + i + 1,
            )),
        }
    }
}

fn star_run_len(bytes: &[u8], from: usize) -> usize {
    let mut end = from;
    while end < bytes.len() && bytes[end] == b'*' {
        end += 1;
    }
    end - from
}

fn delimiter_closes(delimiter: Delimiter, run_len: usize) -> bool {
    match delimiter {
        Delimiter::Strong => run_len >= 2,
        Delimiter::Emphasis => run_len % 2 == 1,
    }
}

fn widen_span_to_delimiters(inlines: &mut [Inline], start: usize, end: usize) {
    if let Some(first) = inlines.first_mut() {
        first.span.start = start;
    }
    if let Some(last) = inlines.last_mut() {
        last.span.end = end;
    }
}
