use crate::parser::Parser;
use crate::support::{RawListLine, list_marker_at};
use crate::{Item, ListItem};

impl Parser<'_> {
    /// Consume a contiguous run of list-marker lines starting at the
    /// current position and push one or more [`Item::List`] entries
    /// onto `self.items`.
    pub(crate) fn parse_list(&mut self) {
        let raw = self.collect_list_lines();
        if raw.is_empty() {
            return;
        }
        let mut i = 0;
        while i < raw.len() {
            let (item, new_i) = self.build_list_at(&raw, i);
            self.items.push(item);
            i = new_i;
        }
    }

    fn collect_list_lines(&mut self) -> Vec<RawListLine> {
        let bytes = self.src.as_bytes();
        let mut out: Vec<RawListLine> = Vec::new();
        while self.pos < bytes.len() {
            if self.at_blank_line() {
                break;
            }
            let Some((indent, ordered, content_start)) = list_marker_at(bytes, self.pos) else {
                break;
            };
            let (line_start, content_end, line_end) = self.current_line_bounds();
            out.push(RawListLine {
                indent,
                ordered,
                content_start,
                content_end,
                line_start,
            });
            self.pos = line_end;
        }
        out
    }

    /// Build one list from the run starting at `raw[start]`.
    fn build_list_at(&mut self, raw: &[RawListLine], start: usize) -> (Item, usize) {
        let base_indent = raw[start].indent;
        let base_ordered = raw[start].ordered;
        let mut items: Vec<ListItem> = Vec::new();
        let mut last_end = raw[start].content_end;
        let mut i = start;
        while i < raw.len() {
            let cur = &raw[i];
            if cur.indent != base_indent || cur.ordered != base_ordered {
                break;
            }
            let slice = &self.src[cur.content_start..cur.content_end];
            let inlines = self.parse_inlines(slice, cur.content_start);
            let item_span = self.span(cur.line_start, cur.content_end);
            let mut item = ListItem {
                inlines,
                children: Vec::new(),
                span: item_span,
            };
            last_end = last_end.max(cur.content_end);
            i += 1;
            while i < raw.len() && raw[i].indent > base_indent {
                let (nested, new_i) = self.build_list_at(raw, i);
                if let Item::List { span, .. } = &nested {
                    last_end = last_end.max(span.end);
                }
                item.children.push(nested);
                i = new_i;
            }
            items.push(item);
        }
        let span = self.span(raw[start].line_start, last_end);
        (
            Item::List {
                ordered: base_ordered,
                items,
                span,
            },
            i,
        )
    }
}
