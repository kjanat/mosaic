use crate::parser::Parser;
use crate::support::{find_byte, find_emphasis_close, find_subslice, scan_label_chars};
use crate::{Inline, InlineKind};

impl Parser<'_> {
    /// Tokenize `slice` (whose first byte sits at `base` in `self.src`)
    /// into inline runs. Inline parsing is non-nesting in MVP 0; the
    /// inner contents of `*...*`, `**...**`, and `` `...` `` are plain text.
    pub(crate) fn parse_inlines(&mut self, slice: &str, base: usize) -> Vec<Inline> {
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
            if c == b'@' {
                let id_end = scan_label_chars(bytes, i + 1);
                if id_end > i + 1 {
                    self.flush_text(&mut out, slice, base, text_start, i);
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
}
