use std::sync::Arc;

use mos_core::{AttrValue, Diagnostic, DiagnosticCode, Document, Node, NodeId, NodeKind, Severity};
use mos_fonts::{ascent, text_width};

use crate::support::{read_int_attr, read_length_attr};
use crate::word::{ShyBreak, Word, WordItem, try_shy_break, word_clusters};
use crate::{ImageHandle, ImagePlacement, LayoutState, PARA_SPACE_AFTER_PT};

impl LayoutState {
    /// Lay out a top-level `Image` node as a block. The image is
    /// horizontally centred within the column and capped at the column
    /// width; declared `width`/`height` override natural 72-DPI size.
    pub(super) fn layout_image(&mut self, node_id: NodeId, image: &Node) {
        let Some((width_pt, height_pt)) = self.intrinsic_image_size(image) else {
            return;
        };
        let Some(handle) = self.intern_image(image) else {
            self.diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                code: DiagnosticCode("W050"),
                message: format!("image node {node_id:?} missing decoded pixel data; skipping"),
                span: Some(image.span.clone()),
                notes: Vec::new(),
                suggestions: Vec::new(),
            });
            return;
        };

        let column_w = self.column_width_pt();
        let render_w = width_pt.min(column_w);
        let aspect = if width_pt > 0.0 {
            height_pt / width_pt
        } else {
            1.0
        };
        let render_h = if render_w < width_pt {
            render_w * aspect
        } else {
            height_pt
        };
        let available_y = self.page.height_pt - self.page.margin_pt;
        if self.cursor_y + render_h > available_y && self.page_has_content {
            self.start_new_page();
        }

        let x = self.page.margin_pt + (column_w - render_w) * 0.5;
        self.current_page.images.push(ImagePlacement {
            handle,
            x_pt: x,
            top_from_top_pt: self.cursor_y,
            width_pt: render_w,
            height_pt: render_h,
        });
        self.page_has_content = true;

        // `cursor_y` is the next text baseline. Add body ascent so
        // following text starts after image bottom + paragraph gap.
        let body_ascent = ascent(self.text.family.regular, self.text.size_pt);
        self.cursor_y += render_h + PARA_SPACE_AFTER_PT + body_ascent;
    }

    /// Lay out a `Figure` block and keep image + caption together when
    /// the remaining space allows.
    pub(super) fn layout_figure(&mut self, document: &Document, figure: &Node) {
        let column_w = self.column_width_pt();
        let body_ascent = ascent(self.text.family.regular, self.text.size_pt);
        let mut total_h = 0.0_f32;
        let mut block_count = 0_u32;
        for child_id in &figure.children {
            let Some(child) = document.get(*child_id) else {
                continue;
            };
            let block_h = match child.kind {
                NodeKind::Image => self.intrinsic_image_size(child).map_or(0.0, |(w, h)| {
                    let render_w = w.min(column_w);
                    let render_h = if w > 0.0 && render_w < w {
                        render_w * (h / w)
                    } else {
                        h
                    };
                    render_h + body_ascent
                }),
                NodeKind::Paragraph => self.measure_paragraph_height(document, child),
                _ => continue,
            };
            total_h += block_h;
            block_count += 1;
        }
        #[allow(
            clippy::cast_precision_loss,
            reason = "a figure with > 2^23 children is not a real document"
        )]
        if block_count > 0 {
            total_h += PARA_SPACE_AFTER_PT * block_count as f32;
        }
        let available_y = self.page.height_pt - self.page.margin_pt;
        if self.cursor_y + total_h > available_y && self.page_has_content {
            self.start_new_page();
        }
        for child_id in &figure.children {
            let Some(child) = document.get(*child_id) else {
                continue;
            };
            match child.kind {
                NodeKind::Image => self.layout_image(*child_id, child),
                NodeKind::Paragraph => self.layout_paragraph(document, child),
                _ => {}
            }
        }
    }

    fn measure_paragraph_height(&mut self, document: &Document, paragraph: &Node) -> f32 {
        let size = self.text.size_pt;
        let leading = self.text.leading;
        let regular = self.text.family.regular;
        let items = self.collect_words(document, paragraph, regular, size);
        if items.is_empty() {
            return 0.0;
        }
        let line_width = self.column_width_pt();
        let mut lines = 0_u32;
        let mut line_has_words = false;
        let mut line_width_used = 0.0_f32;
        let mut paragraph_emitted_line = false;
        let mut last_was_hardbreak_flush = false;
        let mut pending: Option<Word> = None;
        let mut item_idx = 0;

        loop {
            let word = if let Some(word) = pending.take() {
                word
            } else if item_idx < items.len() {
                let item = &items[item_idx];
                item_idx += 1;
                match item {
                    WordItem::Word(word) => word.clone(),
                    WordItem::HardBreak => {
                        if line_has_words {
                            lines += 1;
                            line_has_words = false;
                            line_width_used = 0.0;
                            paragraph_emitted_line = true;
                            last_was_hardbreak_flush = true;
                        } else if last_was_hardbreak_flush {
                            lines += 1;
                        } else if paragraph_emitted_line {
                            last_was_hardbreak_flush = true;
                        }
                        continue;
                    }
                }
            } else {
                break;
            };

            let space_w = if line_has_words {
                text_width(word.font, word.size_pt, " ")
            } else {
                0.0
            };

            if line_width_used + space_w + word.width_pt <= line_width {
                line_has_words = true;
                line_width_used += space_w + word.width_pt;
                continue;
            }

            if line_has_words
                && let Some(ShyBreak { suffix, .. }) = try_shy_break(
                    &word,
                    line_width - line_width_used - space_w,
                    self.text.family.fallbacks,
                )
            {
                lines += 1;
                line_has_words = false;
                line_width_used = 0.0;
                paragraph_emitted_line = true;
                last_was_hardbreak_flush = false;
                pending = Some(suffix);
                continue;
            }

            if line_has_words {
                lines += 1;
                line_has_words = false;
                line_width_used = 0.0;
                paragraph_emitted_line = true;
                last_was_hardbreak_flush = false;
            }

            if word.width_pt > line_width {
                if let Some(ShyBreak { suffix, .. }) =
                    try_shy_break(&word, line_width, self.text.family.fallbacks)
                {
                    lines += 1;
                    paragraph_emitted_line = true;
                    last_was_hardbreak_flush = false;
                    pending = Some(suffix);
                    continue;
                }
                lines += oversize_chunk_count(&word, line_width);
                paragraph_emitted_line = true;
                last_was_hardbreak_flush = false;
                continue;
            }

            line_has_words = true;
            line_width_used = word.width_pt;
        }
        if line_has_words {
            lines += 1;
        }
        #[allow(
            clippy::cast_precision_loss,
            reason = "line counts in any sane document fit well inside the f32 mantissa"
        )]
        let h = lines as f32 * size * leading;
        h
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "pixel dimensions clamp well below the f32 mantissa cap"
    )]
    fn intrinsic_image_size(&self, image: &Node) -> Option<(f32, f32)> {
        let pw = read_int_attr(image, "pixel_width")?;
        let ph = read_int_attr(image, "pixel_height")?;
        if pw <= 0 || ph <= 0 {
            return None;
        }
        let natural_w = pw as f32;
        let natural_h = ph as f32;
        let declared_w = read_length_attr(image, "width");
        let declared_h = read_length_attr(image, "height");
        let aspect = natural_h / natural_w;
        let (w, h) = match (declared_w, declared_h) {
            (Some(w), Some(h)) => {
                let scale = (w / natural_w).min(h / natural_h);
                (natural_w * scale, natural_h * scale)
            }
            (Some(w), None) => (w, w * aspect),
            (None, Some(h)) => (h / aspect, h),
            (None, None) => (natural_w, natural_h),
        };
        Some((w, h))
    }

    fn intern_image(&mut self, image: &Node) -> Option<ImageHandle> {
        let resolved_path = match image.attributes.get("resolved_path") {
            Some(AttrValue::Str(s)) => s.clone(),
            _ => match image.attributes.get("src") {
                Some(AttrValue::Str(s)) => s.clone(),
                _ => return None,
            },
        };
        if let Some(existing) = self
            .image_handles
            .iter()
            .find(|h| h.resolved_path == resolved_path)
        {
            return Some(existing.clone());
        }
        let pw = read_int_attr(image, "pixel_width")?;
        let ph = read_int_attr(image, "pixel_height")?;
        let pixels: Arc<[u8]> = match image.attributes.get("pixels") {
            Some(AttrValue::Bytes(b)) => Arc::clone(b),
            _ => return None,
        };
        let id = u32::try_from(self.image_handles.len()).unwrap_or(u32::MAX);
        let handle = ImageHandle {
            id,
            resolved_path,
            pixel_width: u32::try_from(pw).ok()?,
            pixel_height: u32::try_from(ph).ok()?,
            rgb8: pixels,
        };
        self.image_handles.push(handle.clone());
        Some(handle)
    }
}

fn oversize_chunk_count(word: &Word, line_width: f32) -> u32 {
    let mut chunks = 0_u32;
    let mut chunk_has_content = false;
    let mut chunk_width = 0.0_f32;
    for cluster in word_clusters(word) {
        if chunk_width + cluster.advance_pt > line_width && chunk_has_content {
            chunks += 1;
            chunk_width = 0.0;
        }
        chunk_width += cluster.advance_pt;
        chunk_has_content = true;
    }
    if chunk_has_content {
        chunks += 1;
    }
    chunks
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "tests panic loudly on setup failure; matches crate-wide test-module convention"
    )]

    use std::path::PathBuf;
    use std::sync::Arc;

    use mos_core::{AttrMap, ContentHash, SourceSpan, StyleId};

    use mos_fonts::FontFamily;

    use crate::types::{BODY_LEADING, BODY_SIZE_PT};
    use crate::{A4_HEIGHT_PT, A4_WIDTH_PT, LayoutEngine, MARGIN_PT, PageStyle, TextStyle};

    use super::*;

    fn alloc_inline(doc: &mut Document, parent: NodeId, kind: NodeKind, text: &str) {
        let mut attrs = AttrMap::new();
        attrs.insert("text".to_owned(), AttrValue::Str(text.to_owned()));
        doc.alloc_child(
            parent,
            Node {
                id: NodeId::default(),
                kind,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: attrs,
            },
        );
    }

    fn pin_helvetica(doc: &mut Document) {
        let mut attrs = AttrMap::new();
        attrs.insert("set".to_owned(), AttrValue::Str("text".to_owned()));
        attrs.insert(
            "set.arg.font".to_owned(),
            AttrValue::Str("Helvetica".to_owned()),
        );
        doc.alloc_child(
            doc.root,
            Node {
                id: NodeId::default(),
                kind: NodeKind::Raw,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: attrs,
            },
        );
    }

    fn helvetica_state_with_column_width(column_width_pt: f32) -> LayoutState {
        LayoutState::new(
            PageStyle {
                width_pt: A4_WIDTH_PT,
                height_pt: A4_HEIGHT_PT,
                margin_pt: (A4_WIDTH_PT - column_width_pt) * 0.5,
            },
            TextStyle {
                size_pt: BODY_SIZE_PT,
                leading: BODY_LEADING,
                family: FontFamily::helvetica(),
            },
        )
    }

    fn make_paragraph(doc: &mut Document, text: &str) -> NodeId {
        let id = doc.alloc_child(
            doc.root,
            Node {
                id: NodeId::default(),
                kind: NodeKind::Paragraph,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: AttrMap::new(),
            },
        );
        alloc_inline(doc, id, NodeKind::Text, text);
        id
    }

    fn make_image(
        doc: &mut Document,
        path: &str,
        pixel_w: u32,
        pixel_h: u32,
        declared_width_pt: Option<f64>,
        declared_height_pt: Option<f64>,
    ) -> NodeId {
        let pixels: Arc<[u8]> = Arc::from(vec![0; (pixel_w * pixel_h * 3) as usize]);
        let mut attrs = AttrMap::new();
        attrs.insert("src".to_owned(), AttrValue::Str(path.to_owned()));
        attrs.insert(
            "resolved_path".to_owned(),
            AttrValue::Str(format!("/tmp/{path}")),
        );
        attrs.insert("pixel_width".to_owned(), AttrValue::Int(i64::from(pixel_w)));
        attrs.insert(
            "pixel_height".to_owned(),
            AttrValue::Int(i64::from(pixel_h)),
        );
        attrs.insert("pixels".to_owned(), AttrValue::Bytes(pixels));
        if let Some(w) = declared_width_pt {
            attrs.insert("width".to_owned(), AttrValue::Length(w));
        }
        if let Some(h) = declared_height_pt {
            attrs.insert("height".to_owned(), AttrValue::Length(h));
        }
        doc.alloc_child(
            doc.root,
            Node {
                id: NodeId::default(),
                kind: NodeKind::Image,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: attrs,
            },
        )
    }

    #[test]
    fn image_block_natural_size_at_72dpi() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_image(&mut doc, "x.png", 100, 60, None, None);

        let result = LayoutEngine::new().layout(&doc);

        let img = &result.graph.pages[0].images[0];
        assert!((img.width_pt - 100.0).abs() < 0.5);
        assert!((img.height_pt - 60.0).abs() < 0.5);
    }

    #[test]
    fn image_block_declared_width_preserves_aspect_ratio() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_image(&mut doc, "x.png", 200, 100, Some(80.0), None);

        let result = LayoutEngine::new().layout(&doc);

        let img = &result.graph.pages[0].images[0];
        assert!((img.width_pt - 80.0).abs() < 0.5);
        assert!((img.height_pt - 40.0).abs() < 0.5);
    }

    #[test]
    fn image_block_both_dims_fits_inside_box_preserving_aspect() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_image(&mut doc, "x.png", 200, 100, Some(80.0), Some(80.0));

        let result = LayoutEngine::new().layout(&doc);

        let img = &result.graph.pages[0].images[0];
        assert!((img.width_pt - 80.0).abs() < 0.5, "w = {}", img.width_pt);
        assert!((img.height_pt - 40.0).abs() < 0.5, "h = {}", img.height_pt);
    }

    #[test]
    fn image_block_both_dims_taller_box_fits_by_width() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_image(&mut doc, "x.png", 200, 100, Some(40.0), Some(80.0));

        let result = LayoutEngine::new().layout(&doc);

        let img = &result.graph.pages[0].images[0];
        assert!((img.width_pt - 40.0).abs() < 0.5, "w = {}", img.width_pt);
        assert!((img.height_pt - 20.0).abs() < 0.5, "h = {}", img.height_pt);
    }

    #[test]
    fn image_block_clamped_to_column_width() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_image(&mut doc, "x.png", 4000, 2000, None, None);

        let result = LayoutEngine::new().layout(&doc);

        let img = &result.graph.pages[0].images[0];
        let col = A4_WIDTH_PT - 2.0 * MARGIN_PT;
        assert!(img.width_pt <= col + 0.5);
        let aspect = 4000.0_f32 / 2000.0;
        assert!((img.height_pt - img.width_pt / aspect).abs() < 0.5);
    }

    #[test]
    fn oversized_image_after_paragraph_does_not_force_extra_page() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_paragraph(&mut doc, "lead paragraph");
        make_image(&mut doc, "big.png", 1000, 1000, None, None);

        let result = LayoutEngine::new().layout(&doc);

        assert_eq!(result.graph.pages.len(), 1);
        assert_eq!(result.graph.pages[0].images.len(), 1);
        assert!(!result.graph.pages[0].runs.is_empty());
    }

    #[test]
    fn image_dedup_emits_one_handle_per_resolved_path() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_image(&mut doc, "same.png", 50, 50, None, None);
        make_image(&mut doc, "same.png", 50, 50, None, None);

        let result = LayoutEngine::new().layout(&doc);

        assert_eq!(result.graph.images.len(), 1);
        let placements: Vec<_> = result
            .graph
            .pages
            .iter()
            .flat_map(|p| p.images.iter())
            .collect();
        assert_eq!(placements.len(), 2);
        assert_eq!(placements[0].handle.id, placements[1].handle.id);
    }

    #[test]
    fn figure_lays_out_image_then_caption() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        make_figure_with_image_and_caption(&mut doc, 80, 50, "Caption text.");

        let result = LayoutEngine::new().layout(&doc);

        let page = &result.graph.pages[0];
        assert_eq!(page.images.len(), 1);
        let caption_run = page
            .runs
            .iter()
            .find(|r| r.text == "Caption" || r.text == "text.")
            .expect("caption run not found");
        assert!(caption_run.baseline_from_top_pt > page.images[0].top_from_top_pt);
    }

    #[test]
    fn paragraph_height_measurement_counts_shy_breaks_like_flow() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let para = make_paragraph(&mut doc, "x super\u{AD}cali");
        let line_width = text_width(
            mos_fonts::Font::Base14(mos_fonts::Base14Font::Helvetica),
            BODY_SIZE_PT,
            "x super-",
        ) + 1.0;
        let mut state = helvetica_state_with_column_width(line_width);

        let height = state.measure_paragraph_height(&doc, doc.get(para).expect("paragraph"));
        let expected = 2.0 * BODY_SIZE_PT * BODY_LEADING;

        assert!(
            (height - expected).abs() < 0.01,
            "expected two measured lines ({expected:.3}pt), got {height:.3}pt"
        );
    }

    fn make_figure_with_image_and_caption(
        doc: &mut Document,
        pixel_w: u32,
        pixel_h: u32,
        caption: &str,
    ) -> NodeId {
        let fig = doc.alloc_child(
            doc.root,
            Node {
                id: NodeId::default(),
                kind: NodeKind::Figure,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: AttrMap::new(),
            },
        );
        let mut img_attrs = AttrMap::new();
        img_attrs.insert("src".to_owned(), AttrValue::Str("fig.png".to_owned()));
        img_attrs.insert(
            "resolved_path".to_owned(),
            AttrValue::Str(format!("/tmp/figkt-{pixel_w}x{pixel_h}.png")),
        );
        img_attrs.insert("pixel_width".to_owned(), AttrValue::Int(i64::from(pixel_w)));
        img_attrs.insert(
            "pixel_height".to_owned(),
            AttrValue::Int(i64::from(pixel_h)),
        );
        img_attrs.insert(
            "pixels".to_owned(),
            AttrValue::Bytes(Arc::from(vec![0_u8; (pixel_w * pixel_h * 3) as usize])),
        );
        doc.alloc_child(
            fig,
            Node {
                id: NodeId::default(),
                kind: NodeKind::Image,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: img_attrs,
            },
        );
        let cap = doc.alloc_child(
            fig,
            Node {
                id: NodeId::default(),
                kind: NodeKind::Paragraph,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: AttrMap::new(),
            },
        );
        alloc_inline(doc, cap, NodeKind::Text, caption);
        fig
    }

    #[test]
    fn figure_image_and_caption_stay_on_the_same_page() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let mut filler = String::new();
        for i in 0..540 {
            filler.push_str(&format!("word{i} "));
        }
        make_paragraph(&mut doc, filler.trim());
        make_figure_with_image_and_caption(&mut doc, 400, 300, "Tight caption.");

        let result = LayoutEngine::new().layout(&doc);

        let mut figure_page: Option<u32> = None;
        let mut caption_page: Option<u32> = None;
        for page in &result.graph.pages {
            if !page.images.is_empty() && figure_page.is_none() {
                figure_page = Some(page.number);
            }
            if page
                .runs
                .iter()
                .any(|r| r.text == "Tight" || r.text == "caption.")
                && caption_page.is_none()
            {
                caption_page = Some(page.number);
            }
        }
        assert_eq!(figure_page, caption_page);
    }
}
