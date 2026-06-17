use mos_core::{DiagnosticAnnotation, Suggestion, codes};

use crate::Item;
use crate::parser::Parser;
use crate::support::{locate_label, strip_leading_label, strip_trailing_label};

impl Parser<'_> {
    pub(crate) fn parse_heading(&mut self) {
        let (line_start, content_end, line_end) = self.current_line_bounds();
        let bytes = self.src.as_bytes();
        let mut level: u8 = 0;
        let mut i = line_start;
        while i < content_end && bytes[i] == b'=' {
            level = level.saturating_add(1);
            i += 1;
        }
        if i >= content_end || !bytes[i].is_ascii_whitespace() {
            self.parse_paragraph();
            return;
        }
        while i < content_end && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        let (text_end, parsed_label) = strip_trailing_label(self.src, i, content_end);
        if parsed_label.is_none() {
            self.flag_misplaced_heading_label(i, content_end);
        }
        let label_span = parsed_label
            .as_ref()
            .map(|label| self.span(label.start, label.end));
        let label = parsed_label.map(|label| label.text);
        let content = &self.src[i..text_end];
        let inlines = self.parse_inlines(content, i);
        let span = self.span(line_start, content_end);
        self.items.push(Item::Heading {
            level,
            inlines,
            label,
            label_span,
            span,
        });
        self.pos = line_end;
    }

    /// Emit `MOS0048` when a heading carries a `<label>` token that is not the
    /// trailing element, so [`strip_trailing_label`] left it unrecognised and
    /// it would otherwise be swallowed into the heading text. The attached
    /// suggestion moves the label to the end of the line, where it registers as
    /// a real declaration.
    fn flag_misplaced_heading_label(&mut self, start: usize, content_end: usize) {
        let Some((label, after)) = locate_label(self.src, start, content_end) else {
            return;
        };
        // Only a label with real content after it is misplaced; trailing
        // whitespace alone is harmless and not the author's mistake.
        let trailing = self.src[after..content_end].trim();
        if trailing.is_empty() {
            return;
        }
        let Some(label_open) = label.start.checked_sub(1).filter(|open| *open >= start) else {
            return;
        };
        let before = self.src[start..label_open].trim();
        let mut fixed = String::new();
        for part in [before, trailing] {
            if part.is_empty() {
                continue;
            }
            if !fixed.is_empty() {
                fixed.push(' ');
            }
            fixed.push_str(part);
        }
        if !fixed.is_empty() {
            fixed.push(' ');
        }
        fixed.push('<');
        fixed.push_str(&label.text);
        fixed.push('>');
        let message = format!(
            "heading label `<{}>` must be the last element on the line",
            label.text
        );
        let diagnostic = self
            .warn(&codes::MOS0048, &message, start, content_end)
            .with_suggestion(Suggestion::new(self.span(start, content_end), fixed))
            .with_suggestion(Suggestion::new(self.span(label_open, label.start), "\\<"))
            .with_annotation(DiagnosticAnnotation::Hint(format!(
                "if `<{label}>` is literal text (e.g. an HTML tag), escape the `<` as `\\<{label}>`",
                label = label.text
            )));
        self.diagnostics.push(diagnostic);
    }

    pub(crate) fn parse_paragraph(&mut self) {
        let bytes = self.src.as_bytes();
        let para_start = self.pos;
        let mut para_end = self.pos;
        let mut text_start: Option<usize> = None;
        loop {
            if self.pos >= bytes.len() || self.at_blank_line() {
                break;
            }
            if self.starts_with("=") && self.heading_level_of_current_line().is_some() {
                break;
            }
            if self.at_directive_keyword().is_some() || self.at_list_marker() {
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
            let (body_start, parsed_label) = strip_leading_label(self.src, start, para_end);
            let label_span = parsed_label
                .as_ref()
                .map(|label| self.span(label.start, label.end));
            let label = parsed_label.map(|label| label.text);
            let slice = &self.src[body_start..para_end];
            let mut inlines = self.parse_inlines(slice, body_start);
            for inline in &mut inlines {
                if inline.text.contains("\r\n") {
                    inline.text = inline.text.replace("\r\n", "\n");
                }
            }
            let span = self.span(para_start, para_end);
            self.items.push(Item::Paragraph {
                inlines,
                label,
                label_span,
                span,
            });
        }
    }

    /// Returns `Some(level)` if the current line is a well-formed
    /// heading of `=`+ followed by ASCII whitespace.
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
}
