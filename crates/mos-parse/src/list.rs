use crate::parser::Parser;
use crate::support::list_marker_at;
use crate::{Inline, Item, ListItem, ListItemBlock};
use mos_core::SourceSpan;

enum RawListItemBlock {
    Paragraph {
        start: usize,
        end: usize,
    },
    List {
        ordered: bool,
        items: Vec<ListItem>,
        span: SourceSpan,
    },
}

impl Parser<'_> {
    /// Consume one list block starting at the current marker line and push it
    /// onto `self.items`.
    pub(crate) fn parse_list(&mut self) {
        let Some((indent, ordered, _)) = list_marker_at(self.src.as_bytes(), self.pos) else {
            return;
        };
        let item = self.parse_list_at(indent, ordered);
        self.items.push(item);
    }

    fn parse_list_at(&mut self, base_indent: usize, base_ordered: bool) -> Item {
        let list_start = self.pos;
        let mut items: Vec<ListItem> = Vec::new();
        let mut list_end = list_start;

        while self.pos < self.src.len() {
            if self.at_blank_line() {
                break;
            }
            let Some((indent, ordered, _)) = list_marker_at(self.src.as_bytes(), self.pos) else {
                break;
            };
            if indent != base_indent || ordered != base_ordered {
                break;
            }

            let item = self.parse_list_item(base_indent);
            list_end = list_end.max(item.span.end());
            items.push(item);
        }

        Item::List {
            ordered: base_ordered,
            items,
            span: self.span(list_start, list_end),
        }
    }

    fn parse_list_item(&mut self, base_indent: usize) -> ListItem {
        let Some((_, _, content_start)) = list_marker_at(self.src.as_bytes(), self.pos) else {
            let span = self.span(self.pos, self.pos);
            return ListItem {
                inlines: Vec::new(),
                blocks: Vec::new(),
                span,
            };
        };

        let (line_start, content_end, line_end) = self.current_line_bounds();
        let text_column = content_start.saturating_sub(line_start);
        let item_start = line_start;
        let mut item_end = content_end;
        let mut raw_blocks = vec![RawListItemBlock::Paragraph {
            start: content_start,
            end: content_end,
        }];
        self.pos = line_end;

        while self.pos < self.src.len() {
            if self.at_blank_line() {
                break;
            }

            if let Some((indent, ordered, _)) = list_marker_at(self.src.as_bytes(), self.pos) {
                if indent > base_indent {
                    let nested = self.parse_list_at(indent, ordered);
                    if let Item::List {
                        ordered,
                        items,
                        span,
                    } = nested
                    {
                        item_end = item_end.max(span.end());
                        raw_blocks.push(RawListItemBlock::List {
                            ordered,
                            items,
                            span,
                        });
                    }
                    continue;
                }
                break;
            }

            let (line_start, content_end, line_end) = self.current_line_bounds();
            let leading_spaces = self.leading_spaces_on_line(line_start, content_end);
            if leading_spaces < text_column {
                break;
            }

            let continuation_start = line_start + text_column;
            match raw_blocks.last_mut() {
                Some(RawListItemBlock::Paragraph { end, .. }) => {
                    *end = content_end;
                }
                Some(RawListItemBlock::List { .. }) | None => {
                    raw_blocks.push(RawListItemBlock::Paragraph {
                        start: continuation_start,
                        end: content_end,
                    });
                }
            }
            item_end = item_end.max(content_end);
            self.pos = line_end;
        }

        self.finish_list_item(raw_blocks, item_start, item_end)
    }

    fn finish_list_item(
        &mut self,
        raw_blocks: Vec<RawListItemBlock>,
        item_start: usize,
        item_end: usize,
    ) -> ListItem {
        let mut inlines: Vec<Inline> = Vec::new();
        let mut blocks: Vec<ListItemBlock> = Vec::new();
        let mut saw_first_paragraph = false;

        for raw in raw_blocks {
            match raw {
                RawListItemBlock::Paragraph { start, end } => {
                    let paragraph = self.parse_list_paragraph(start, end);
                    if !saw_first_paragraph {
                        inlines.clone_from(&paragraph);
                        saw_first_paragraph = true;
                    }
                    blocks.push(ListItemBlock::Paragraph {
                        inlines: paragraph,
                        span: self.span(start, end),
                    });
                }
                RawListItemBlock::List {
                    ordered,
                    items,
                    span,
                } => {
                    blocks.push(ListItemBlock::List {
                        ordered,
                        items,
                        span,
                    });
                }
            }
        }

        ListItem {
            inlines,
            blocks,
            span: self.span(item_start, item_end),
        }
    }

    fn parse_list_paragraph(&mut self, start: usize, end: usize) -> Vec<Inline> {
        let slice = &self.src[start..end];
        let mut inlines = self.parse_inlines(slice, start);
        for inline in &mut inlines {
            if inline.text.contains("\r\n") {
                inline.text = inline.text.replace("\r\n", "\n");
            }
        }
        inlines
    }

    fn leading_spaces_on_line(&self, line_start: usize, content_end: usize) -> usize {
        let bytes = self.src.as_bytes();
        let mut i = line_start;
        while i < content_end && bytes[i] == b' ' {
            i += 1;
        }
        i - line_start
    }
}
