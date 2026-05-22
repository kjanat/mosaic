//! Layout engine for Mosaic.
//!
//! MVP 0 implements the smallest end-to-end slice that gets ink on a
//! page: greedy line-breaking against fixed A4 metrics, walking a
//! lowered [`Document`] into a [`PageGraph`]. Real shaping
//! (`HarfBuzz`/`rustybuzz`), Knuth-Plass, hyphenation, and font
//! embedding are deferred per the manifest's MVP roadmap (§30,
//! §22.1, §22.2). Boundary-state reuse for incremental builds
//! (§22.3, §33) is also out of scope here.

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

use mos_fonts::nfc_text;
pub use mos_fonts::{
    Base14Font, EmbeddedFontId, Font, FontFamily, ShapedGlyph, WordSubRun, ascent, descent,
    glyph_width, shape_with_fallback, text_width,
};
pub use style::paper_size_pt;
pub use types::{
    A4_HEIGHT_PT, A4_WIDTH_PT, ImageHandle, ImagePlacement, LayoutResult, MARGIN_PT, Page,
    PageGraph, PageStyle, TextRun, TextStyle,
};

use mos_core::{AttrValue, Diagnostic, Document, Node, NodeKind};
use style::resolve_styles;
use support::{blank_page, expand_tabs, read_level, read_str_attr};
use types::BODY_LEADING;
use word::{Word, word_clusters};

mod image;
mod list;
mod style;
mod support;
mod types;
mod word;

/// Heading sizes by level (1-indexed). Anything beyond level 3 falls
/// back to body size — counters and section numbering land in MVP 1.
const HEADING_SIZES_PT: [f32; 3] = [20.0, 16.0, 13.0];
/// Space above each heading level (skipped for the first block on a
/// page).
const HEADING_SPACE_BEFORE_PT: [f32; 3] = [16.0, 12.0, 10.0];
/// Space below each heading level.
const HEADING_SPACE_AFTER_PT: [f32; 3] = [10.0, 8.0, 6.0];
/// Vertical gap between consecutive paragraphs.
const PARA_SPACE_AFTER_PT: f32 = 4.0;
/// Horizontal gutter reserved for the list marker (`•` for unordered,
/// `1.` for ordered) on each nesting level. Doubles as the per-level
/// indent step: nested items shift right by this many points before
/// their own gutter is added. Sized to comfortably hold a one- or
/// two-digit ordered marker at the default body size; lists with three-
/// digit numbering will overflow the gutter visually until per-list
/// gutter tuning lands.
const LIST_MARKER_GUTTER_PT: f32 = 18.0;
/// Number of columns represented by one tab in raw code/pre blocks.
const RAW_BLOCK_TAB_WIDTH: usize = 4;

/// The driver for MVP 0 layout.
///
/// # Examples
///
/// ```
/// use mos_layout::LayoutEngine;
///
/// let engine = LayoutEngine::new();
///
/// assert_eq!(format!("{engine:?}"), "LayoutEngine");
/// ```
#[derive(Debug, Default)]
pub struct LayoutEngine;

impl LayoutEngine {
    /// Construct a layout engine.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_layout::LayoutEngine;
    ///
    /// let engine = LayoutEngine::new();
    ///
    /// assert_eq!(format!("{engine:?}"), "LayoutEngine");
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Lay out `document` into a [`PageGraph`]. Never returns an
    /// error in MVP 0 — invalid blocks are skipped and surfaced as
    /// diagnostics on `LayoutResult` instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::Document;
    /// use mos_layout::LayoutEngine;
    ///
    /// let doc = Document::new(PathBuf::from("main.mos"));
    /// let result = LayoutEngine::new().layout(&doc);
    ///
    /// assert_eq!(result.graph.pages.len(), 1);
    /// ```
    pub fn layout(&mut self, document: &Document) -> LayoutResult {
        let (page_style, text_style, mut diagnostics) = resolve_styles(document);
        let mut state = LayoutState::new(page_style, text_style);
        state.diagnostics.append(&mut diagnostics);
        let Some(root) = document.get(document.root) else {
            return state.finish();
        };
        for child_id in &root.children {
            let Some(node) = document.get(*child_id) else {
                continue;
            };
            match node.kind {
                NodeKind::Section => state.layout_heading(document, node),
                NodeKind::Paragraph => state.layout_paragraph(document, node),
                NodeKind::Image => state.layout_image(*child_id, node),
                NodeKind::Figure => state.layout_figure(document, node),
                NodeKind::List => state.layout_list(document, node),
                NodeKind::Raw if node.attributes.contains_key("raw.kind") => {
                    state.layout_raw_block(node);
                }
                // `#set` blocks are stashed as `Raw` children of the
                // root; folded into styles by `resolve_styles` above.
                NodeKind::Raw if node.attributes.contains_key("set") => {}
                _ => {
                    // Unknown top-level kinds (Table, Equation, etc.)
                    // arrive in MVP 1+; ignore so MVP 0 doesn't panic
                    // on forward-compatible input.
                }
            }
        }
        state.finish()
    }
}

/// Mutable cursor + accumulator threaded through the layout.
struct LayoutState {
    pages: Vec<Page>,
    /// In-progress page being filled.
    current_page: Page,
    /// Y position of the next baseline, measured from page top.
    cursor_y: f32,
    /// Whether `current_page` has had any block emitted yet (controls
    /// `space_before` skipping).
    page_has_content: bool,
    diagnostics: Vec<Diagnostic>,
    page: PageStyle,
    text: TextStyle,
    /// Image dedup table: resolved path → handle. Two `#image(...)`
    /// directives that reference the same on-disk file share one
    /// [`ImageHandle`] (and therefore one `XObject` in the emitted PDF).
    image_handles: Vec<ImageHandle>,
    /// Left edge of the current text column. Equals `page.margin_pt`
    /// at the top level; list layout pushes this rightward so item
    /// text hangs into the gutter under its marker.
    current_left_pt: f32,
    /// Marker run to emit at the start of the next flushed line. Used
    /// by list items to draw `•` / `1.` in the gutter to the left of
    /// `current_left_pt` on the first line of each item. Cleared by
    /// `flush_line` once the marker is committed to a page.
    pending_marker: Option<PendingMarker>,
}

#[derive(Clone, Debug)]
struct PendingMarker {
    /// X position (page-relative, points from the page's left edge)
    /// where the marker's left edge should sit.
    x_pt: f32,
    /// Pre-shaped marker word. Width is informational only — the
    /// marker is drawn outside `current_left_pt` so it doesn't reserve
    /// space in the text column.
    word: Word,
}

impl LayoutState {
    fn new(page: PageStyle, text: TextStyle) -> Self {
        Self {
            pages: Vec::new(),
            current_page: blank_page(1, page),
            cursor_y: page.margin_pt,
            page_has_content: false,
            diagnostics: Vec::new(),
            page,
            text,
            image_handles: Vec::new(),
            current_left_pt: page.margin_pt,
            pending_marker: None,
        }
    }

    fn column_width_pt(&self) -> f32 {
        self.page.width_pt - self.page.margin_pt - self.current_left_pt
    }

    fn finish(mut self) -> LayoutResult {
        // Always emit the last page even if empty so the PDF is valid
        // (a Pages tree with `Count 0` is illegal); only skip when an
        // earlier page already accumulated content and the trailing
        // page is genuinely blank.
        if self.page_has_content || self.pages.is_empty() {
            self.pages.push(self.current_page);
        }
        LayoutResult {
            graph: PageGraph {
                pages: self.pages,
                images: self.image_handles,
            },
            diagnostics: self.diagnostics,
        }
    }

    fn layout_heading(&mut self, document: &Document, section: &Node) {
        let level = usize::from(read_level(section).unwrap_or(1).clamp(1, 3));
        let size = HEADING_SIZES_PT[level - 1];
        let space_before = HEADING_SPACE_BEFORE_PT[level - 1];
        let space_after = HEADING_SPACE_AFTER_PT[level - 1];

        if self.page_has_content {
            self.cursor_y += space_before;
        }
        let bold = self.text.family.bold;
        let mut words = self.collect_words(document, section, bold, size);
        // Resolver-assigned section number is rendered as a leading
        // word so it gets the same font/size as the title and flows
        // through the existing line-break path. The trailing `.` is
        // the conventional "1." style; `#set heading(numbering: ...)`
        // (manifest §4) overrides it once `#set` is interpreted.
        if let Some(number) = read_str_attr(section, "number") {
            let prefix = format!("{number}.");
            let subruns = shape_with_fallback(bold, self.text.family.fallbacks, size, &prefix);
            let width_pt: f32 = subruns.iter().map(|s| s.advance_pt).sum();
            words.insert(
                0,
                Word {
                    text: prefix,
                    actual_text: None,
                    font: bold,
                    size_pt: size,
                    width_pt,
                    subruns,
                },
            );
        }
        self.flow_words(&words, BODY_LEADING);
        self.cursor_y += space_after;
    }

    fn layout_paragraph(&mut self, document: &Document, paragraph: &Node) {
        let size = self.text.size_pt;
        let leading = self.text.leading;
        let regular = self.text.family.regular;
        let words = self.collect_words(document, paragraph, regular, size);
        self.flow_words(&words, leading);
        self.cursor_y += PARA_SPACE_AFTER_PT;
    }

    fn layout_raw_block(&mut self, raw: &Node) {
        let Some(AttrValue::Str(text)) = raw.attributes.get("text") else {
            return;
        };
        let size = self.text.size_pt;
        let leading = self.text.leading;
        let font = self.text.family.monospace;
        let mut emitted = false;
        for line in text.lines() {
            if line.is_empty() {
                if !self.page_has_content {
                    self.cursor_y = self.page.margin_pt + ascent(font, size);
                    self.page_has_content = true;
                }
                self.cursor_y += size * leading;
                continue;
            }
            let expanded_line = expand_tabs(line, RAW_BLOCK_TAB_WIDTH);
            let subruns = shape_with_fallback(
                font,
                self.text.family.fallbacks,
                size,
                expanded_line.as_ref(),
            );
            let width_pt: f32 = subruns.iter().map(|s| s.advance_pt).sum();
            let actual_text = (expanded_line.as_ref() != line).then(|| line.to_owned());
            let word = Word {
                text: expanded_line.into_owned(),
                actual_text,
                font,
                size_pt: size,
                width_pt,
                subruns,
            };
            self.flow_words(&[word], leading);
            emitted = true;
        }
        if emitted {
            self.cursor_y += PARA_SPACE_AFTER_PT;
        }
    }

    /// Walk `parent`'s inline children and produce a flat list of
    /// [`Word`]s. Inline whitespace collapses to a single split point
    /// (`split_ascii_whitespace` handles `\n`/`\r`/`\t` uniformly).
    /// Each word is shaped once here; the resulting glyphs and width
    /// flow through to [`TextRun`] without re-shaping during line
    /// breaking.
    fn collect_words(
        &mut self,
        document: &Document,
        parent: &Node,
        default_font: Font,
        size: f32,
    ) -> Vec<Word> {
        let mut out: Vec<Word> = Vec::new();
        for child_id in &parent.children {
            let Some(child) = document.get(*child_id) else {
                continue;
            };
            let font = match child.kind {
                NodeKind::Strong => self.text.family.bold,
                NodeKind::Emphasis => self.text.family.italic,
                NodeKind::BoldItalic => self.text.family.bold_italic,
                NodeKind::Raw => self.text.family.monospace,
                // Nested list blocks under a `ListItem` are laid out
                // separately by `layout_list`; skip them here so they
                // don't leak into the parent item's word stream.
                NodeKind::List | NodeKind::ListItem => continue,
                _ => default_font,
            };
            let raw = match child.attributes.get("text") {
                Some(AttrValue::Str(s)) => s.as_str(),
                _ => continue,
            };
            for piece in raw.split_ascii_whitespace() {
                if piece.is_empty() {
                    continue;
                }
                let piece = nfc_text(piece);
                let piece = piece.as_ref();
                let subruns = shape_with_fallback(font, self.text.family.fallbacks, size, piece);
                let width_pt: f32 = subruns.iter().map(|s| s.advance_pt).sum();
                out.push(Word {
                    text: piece.to_owned(),
                    actual_text: None,
                    font,
                    size_pt: size,
                    width_pt,
                    subruns,
                });
            }
        }
        out
    }

    /// Greedy line-break `words` and emit text runs onto the page,
    /// paginating as we go. `leading` is the line-height multiplier
    /// applied per line.
    fn flow_words(&mut self, words: &[Word], leading: f32) {
        if words.is_empty() {
            return;
        }
        let line_width = self.column_width_pt();
        let mut line: Vec<Word> = Vec::new();
        let mut line_width_used = 0.0_f32;

        for word in words {
            // Hard wrap: a single word wider than the column gets
            // chopped into character-sized pieces, each on its own
            // line. This is the contract MVP 0 documents.
            if word.width_pt > line_width {
                if !line.is_empty() {
                    self.flush_line(&line, leading);
                    line.clear();
                    line_width_used = 0.0;
                }
                self.flush_oversize_word(word, leading);
                continue;
            }
            let space_w = if line.is_empty() {
                0.0
            } else {
                text_width(word.font, word.size_pt, " ")
            };
            if !line.is_empty() && line_width_used + space_w + word.width_pt > line_width {
                self.flush_line(&line, leading);
                line.clear();
                // Post-flush the line is empty, so no leading space
                // is charged before the wrapped word.
                line_width_used = word.width_pt;
            } else {
                line_width_used += space_w + word.width_pt;
            }
            line.push(word.clone());
        }
        if !line.is_empty() {
            self.flush_line(&line, leading);
        }
    }

    /// Emit one line worth of words at `cursor_y`, advancing past it.
    /// Computes the line's typographic metrics from `line` itself so
    /// the caller doesn't have to track them in parallel.
    fn flush_line(&mut self, line: &[Word], leading: f32) {
        // The marker participates in the line's vertical metrics so a
        // taller marker still gets the right baseline. In practice the
        // marker uses the body face at body size, but folding it in
        // costs nothing and avoids surprises if list layout grows the
        // ability to override marker size later.
        let marker_size = self
            .pending_marker
            .as_ref()
            .map_or(0.0_f32, |m| m.word.size_pt);
        let marker_ascent = self.pending_marker.as_ref().map_or(0.0_f32, |m| {
            m.word
                .subruns
                .iter()
                .map(|sub| ascent(sub.font, m.word.size_pt))
                .fold(0.0_f32, f32::max)
        });
        let max_size = line.iter().map(|w| w.size_pt).fold(marker_size, f32::max);
        let max_ascent = line
            .iter()
            .flat_map(|w| w.subruns.iter().map(|sub| ascent(sub.font, w.size_pt)))
            .fold(marker_ascent, f32::max);

        // First line on a page: drop the baseline by the line's
        // ascent so the glyph tops sit at the top margin.
        if !self.page_has_content {
            self.cursor_y = self.page.margin_pt + max_ascent;
        }
        // Page break if the baseline would fall below the bottom
        // margin. Descent is small and absorbed by the bottom margin.
        if self.cursor_y > self.page.height_pt - self.page.margin_pt {
            self.start_new_page();
            self.cursor_y = self.page.margin_pt + max_ascent;
        }

        // Marker (`•` / `1.` …) is drawn in the gutter to the left of
        // `current_left_pt` once the baseline is locked. Consumed on
        // emit so subsequent wrapped lines of the same item don't
        // restamp the marker.
        if let Some(marker) = self.pending_marker.take() {
            let mut marker_x = marker.x_pt;
            for sub in marker.word.subruns {
                self.current_page.runs.push(TextRun {
                    x_pt: marker_x,
                    baseline_from_top_pt: self.cursor_y,
                    size_pt: marker.word.size_pt,
                    font: sub.font,
                    text: sub.text,
                    actual_text: None,
                    glyphs: sub.glyphs,
                });
                marker_x += sub.advance_pt;
            }
        }

        let mut x = self.current_left_pt;
        for (i, word) in line.iter().enumerate() {
            if i > 0 {
                x += text_width(word.font, word.size_pt, " ");
            }
            // One TextRun per sub-run — same baseline, x advances by
            // each sub-run's `advance_pt`. PDF emit's per-run `Tf`
            // switch fires naturally at the font boundary between
            // sub-runs (Latin → Math → Latin in `a≤b`-style runs).
            for sub in &word.subruns {
                self.current_page.runs.push(TextRun {
                    x_pt: x,
                    baseline_from_top_pt: self.cursor_y,
                    size_pt: word.size_pt,
                    font: sub.font,
                    text: sub.text.clone(),
                    actual_text: word.actual_text.clone(),
                    glyphs: sub.glyphs.clone(),
                });
                x += sub.advance_pt;
            }
        }
        self.page_has_content = true;
        self.cursor_y += max_size * leading;
    }

    /// Emit a word that's wider than the column by chopping it on
    /// already-shaped cluster boundaries. The word was shaped when it
    /// was collected, so this avoids re-running rustybuzz for every
    /// growing prefix of a degenerate long word.
    fn flush_oversize_word(&mut self, word: &Word, leading: f32) {
        let line_width = self.column_width_pt();
        let mut chunk_text = String::with_capacity(word.text.len());
        let mut chunk_width = 0.0_f32;
        let mut chunk_subruns = Vec::new();
        for cluster in word_clusters(word) {
            if chunk_width + cluster.advance_pt > line_width && !chunk_subruns.is_empty() {
                self.flush_oversize_chunk(
                    std::mem::take(&mut chunk_text),
                    chunk_width,
                    std::mem::take(&mut chunk_subruns),
                    word,
                    leading,
                );
                chunk_width = 0.0;
            }
            chunk_text.push_str(&cluster.text);
            chunk_width += cluster.advance_pt;
            chunk_subruns.push(cluster);
        }
        if !chunk_subruns.is_empty() {
            self.flush_oversize_chunk(chunk_text, chunk_width, chunk_subruns, word, leading);
        }
    }

    fn flush_oversize_chunk(
        &mut self,
        text: String,
        width_pt: f32,
        subruns: Vec<WordSubRun>,
        source: &Word,
        leading: f32,
    ) {
        self.flush_line(
            &[Word {
                text,
                actual_text: None,
                font: source.font,
                size_pt: source.size_pt,
                width_pt,
                subruns,
            }],
            leading,
        );
    }

    fn start_new_page(&mut self) {
        let next_number = self.current_page.number + 1;
        let finished =
            std::mem::replace(&mut self.current_page, blank_page(next_number, self.page));
        self.pages.push(finished);
        self.cursor_y = self.page.margin_pt;
        self.page_has_content = false;
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "tests panic loudly on setup failure; matches crate-wide test-module convention"
    )]
    use std::path::PathBuf;

    use mos_core::{
        AttrMap, AttrValue, ContentHash, Document, Node, NodeId, NodeKind, SourceSpan, StyleId,
    };

    use crate::types::BODY_SIZE_PT;

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

    /// Tests that assert Base14 font variants on `TextRun` need to opt
    /// out of the default Noto Sans family. Prepend a `#set
    /// text(font: "Helvetica")` block so the family resolves to
    /// Base14 Helvetica.
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

    fn make_section(doc: &mut Document, level: i64, text: &str) -> NodeId {
        let mut attrs = AttrMap::new();
        attrs.insert("level".to_owned(), AttrValue::Int(level));
        let id = doc.alloc_child(
            doc.root,
            Node {
                id: NodeId::default(),
                kind: NodeKind::Section,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: attrs,
            },
        );
        alloc_inline(doc, id, NodeKind::Text, text);
        id
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

    fn make_raw_block(doc: &mut Document, text: &str) -> NodeId {
        let mut attrs = AttrMap::new();
        attrs.insert("raw.kind".to_owned(), AttrValue::Str("code".to_owned()));
        attrs.insert("text".to_owned(), AttrValue::Str(text.to_owned()));
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
        )
    }

    #[test]
    fn heading_then_paragraph_emits_runs_in_order() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        make_section(&mut doc, 1, "Hello");
        make_paragraph(&mut doc, "body");
        let result = LayoutEngine::new().layout(&doc);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.graph.pages.len(), 1);
        let runs = &result.graph.pages[0].runs;
        assert!(runs.len() >= 2, "expected at least 2 runs, got {runs:?}");
        // Heading first, body below it.
        assert!(matches!(
            runs[0].font,
            Font::Base14(Base14Font::HelveticaBold)
        ));
        assert_eq!(runs[0].text, "Hello");
        let body_run = runs.iter().find(|r| r.text == "body").expect("body run");
        assert!(matches!(body_run.font, Font::Base14(Base14Font::Helvetica)));
        assert!(body_run.baseline_from_top_pt > runs[0].baseline_from_top_pt);
    }

    #[test]
    fn long_paragraph_paginates() {
        // Build a paragraph long enough to spill a second page at
        // body size + leading 1.35.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        // ~150 lines of text at 11pt × 1.35 leading ≈ 2227 pt of
        // copy. A4 minus margins is roughly 706 pt of vertical
        // space, so we expect ≥ 3 pages.
        let mut text = String::new();
        for i in 0..1500 {
            text.push_str(&format!("word{i} "));
        }
        make_paragraph(&mut doc, text.trim());
        let result = LayoutEngine::new().layout(&doc);
        assert!(
            result.graph.pages.len() >= 2,
            "expected pagination, got {} page(s)",
            result.graph.pages.len()
        );
    }

    #[test]
    fn emphasis_run_uses_oblique() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let para = make_paragraph(&mut doc, "before");
        alloc_inline(&mut doc, para, NodeKind::Emphasis, "italic");
        alloc_inline(&mut doc, para, NodeKind::Text, "after");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        let italic = runs
            .iter()
            .find(|r| r.text == "italic")
            .expect("italic run");
        assert!(matches!(
            italic.font,
            Font::Base14(Base14Font::HelveticaOblique)
        ));
    }

    #[test]
    fn bold_italic_run_uses_bold_oblique() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let para = make_paragraph(&mut doc, "before");
        alloc_inline(&mut doc, para, NodeKind::BoldItalic, "both");
        alloc_inline(&mut doc, para, NodeKind::Text, "after");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        let both = runs
            .iter()
            .find(|r| r.text == "both")
            .expect("bold-italic run");
        assert!(matches!(
            both.font,
            Font::Base14(Base14Font::HelveticaBoldOblique)
        ));
    }

    #[test]
    fn runs_stay_within_horizontal_margins() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_paragraph(
            &mut doc,
            "the quick brown fox jumps over the lazy dog the quick brown fox",
        );
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        assert!(!runs.is_empty());
        let right = A4_WIDTH_PT - MARGIN_PT;
        for run in runs {
            assert!(run.x_pt >= MARGIN_PT - 1e-6, "x={}", run.x_pt);
            let end = run.x_pt + text_width(run.font, run.size_pt, &run.text);
            assert!(end <= right + 1e-3, "end={end} right={right}");
        }
    }

    #[test]
    fn cyrillic_flows_through_embedded_default_without_substitution() {
        // The default text family is bundled Noto Sans, which covers
        // Cyrillic. The run carries the original UTF-8 text verbatim
        // and a non-empty shaped glyph stream; no W040 (the diagnostic
        // is retired) and no `?` substitution.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_paragraph(&mut doc, "Привет");
        let result = LayoutEngine::new().layout(&doc);
        assert!(
            result.diagnostics.is_empty(),
            "expected no diagnostics, got {:?}",
            result.diagnostics
        );
        let runs = &result.graph.pages[0].runs;
        let cyr = runs.iter().find(|r| r.text == "Привет").expect("cyr run");
        assert!(matches!(cyr.font, Font::Embedded(_)));
        assert!(!cyr.glyphs.is_empty(), "expected shaped glyphs");
    }

    #[test]
    fn decomposed_text_is_normalized_before_shaping() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_paragraph(&mut doc, "S\u{0326}");

        let result = LayoutEngine::new().layout(&doc);

        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let run = result.graph.pages[0]
            .runs
            .iter()
            .find(|r| r.text == "\u{0218}")
            .expect("normalized run");
        assert!(matches!(run.font, Font::Embedded(_)));
        assert!(!run.glyphs.is_empty(), "expected shaped glyphs");
    }

    #[test]
    fn extended_latin_passes_through_without_warning() {
        // Polish + Czech: every char is either a WinAnsi native
        // (`ó`, `r`, `i`, …) or an extended glyph reachable via
        // `extended_glyph_name` (`ł`, `Ł`, `ě`, `ř`; `ž` is WinAnsi at 0x9E).
        // No substitution, no W040.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_paragraph(&mut doc, "Łódź — Příliš");
        let result = LayoutEngine::new().layout(&doc);
        assert!(
            result.diagnostics.is_empty(),
            "expected no diagnostics, got {:?}",
            result.diagnostics
        );
        let text: String = result.graph.pages[0]
            .runs
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("Łódź"), "got {text}");
        assert!(text.contains("Příliš"), "got {text}");
    }

    #[test]
    fn cjk_and_emoji_flow_through_without_diagnostics() {
        // W040 is retired. CJK and emoji are not covered by bundled
        // Noto Sans Regular either, but the layout engine no longer
        // filters them — they pass through to the shaped glyph stream
        // (rustybuzz emits `.notdef` glyphs for missing coverage,
        // which the PDF backend embeds harmlessly).
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_paragraph(&mut doc, "日本語 🦀");
        let result = LayoutEngine::new().layout(&doc);
        assert!(
            !result.diagnostics.iter().any(|d| d.code.0 == "W040"),
            "W040 should be retired, got {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn winansi_chars_pass_through_without_warning() {
        // café / §1 / Straße all live in WinAnsi (Latin-1 + section
        // sign + germandbls). No substitution, no W040.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_paragraph(&mut doc, "café §1 Straße");
        let result = LayoutEngine::new().layout(&doc);
        assert!(
            result.diagnostics.is_empty(),
            "expected no diagnostics, got {:?}",
            result.diagnostics
        );
        let text: String = result.graph.pages[0]
            .runs
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("café"), "got {text}");
        assert!(text.contains("Straße"), "got {text}");
    }

    #[test]
    fn empty_document_emits_one_blank_page() {
        let doc = Document::new(PathBuf::from("test.mos"));
        let result = LayoutEngine::new().layout(&doc);
        assert_eq!(result.graph.pages.len(), 1);
        assert!(result.graph.pages[0].runs.is_empty());
    }

    #[test]
    fn raw_inline_uses_courier() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let para = make_paragraph(&mut doc, "before");
        alloc_inline(&mut doc, para, NodeKind::Raw, "code");
        alloc_inline(&mut doc, para, NodeKind::Text, "after");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        let code_run = runs.iter().find(|r| r.text == "code").expect("code run");
        assert!(matches!(code_run.font, Font::Base14(Base14Font::Courier)));
        // Adjacent runs stay in the default Helvetica face so the
        // engine isn't accidentally promoting everything to Courier.
        assert!(matches!(
            runs.iter().find(|r| r.text == "before").unwrap().font,
            Font::Base14(Base14Font::Helvetica)
        ));
    }

    #[test]
    fn raw_block_tabs_render_as_spaces() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_raw_block(&mut doc, "\tprintln(\"hello\");");

        let result = LayoutEngine::new().layout(&doc);

        let rendered = result.graph.pages[0]
            .runs
            .iter()
            .map(|run| run.text.as_str())
            .collect::<String>();
        assert!(
            !rendered.contains('\t'),
            "raw block tabs should be expanded before shaping: {rendered:?}"
        );
        assert!(
            rendered.contains("    println"),
            "expected four-space tab expansion, got {rendered:?}"
        );
        assert!(
            result.graph.pages[0]
                .runs
                .iter()
                .any(|run| run.actual_text.as_deref() == Some("\tprintln(\"hello\");")),
            "raw block tabs should retain their original text for extraction"
        );
    }

    #[test]
    fn raw_block_leading_blank_line_preserves_spacing() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_raw_block(&mut doc, "\ncode");

        let result = LayoutEngine::new().layout(&doc);

        let first_run = result.graph.pages[0]
            .runs
            .first()
            .expect("raw block should emit text after the leading blank");
        let expected_baseline = MARGIN_PT
            + ascent(FontFamily::noto_sans().monospace, BODY_SIZE_PT)
            + BODY_SIZE_PT * BODY_LEADING;
        assert!(
            (first_run.baseline_from_top_pt - expected_baseline).abs() < 0.01,
            "baseline {}, expected {expected_baseline}",
            first_run.baseline_from_top_pt
        );
    }

    #[test]
    fn heading_levels_pick_distinct_sizes() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_section(&mut doc, 1, "H1");
        make_section(&mut doc, 2, "H2");
        make_section(&mut doc, 3, "H3");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        let h1 = runs.iter().find(|r| r.text == "H1").expect("H1 run");
        let h2 = runs.iter().find(|r| r.text == "H2").expect("H2 run");
        let h3 = runs.iter().find(|r| r.text == "H3").expect("H3 run");
        assert_eq!(h1.size_pt, HEADING_SIZES_PT[0]);
        assert_eq!(h2.size_pt, HEADING_SIZES_PT[1]);
        assert_eq!(h3.size_pt, HEADING_SIZES_PT[2]);
        // Each level is strictly smaller than the one above it.
        assert!(h1.size_pt > h2.size_pt);
        assert!(h2.size_pt > h3.size_pt);
        // Vertical order matches source order.
        assert!(h1.baseline_from_top_pt < h2.baseline_from_top_pt);
        assert!(h2.baseline_from_top_pt < h3.baseline_from_top_pt);
    }

    #[test]
    fn heading_after_long_paragraph_paginates_correctly() {
        // A paragraph long enough to span multiple pages, followed
        // by a heading. The heading must appear *after* every
        // paragraph word in document order, and the first paragraph
        // word and the heading must end up on different pages.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let mut text = String::new();
        for i in 0..1500 {
            text.push_str(&format!("word{i} "));
        }
        make_paragraph(&mut doc, text.trim());
        make_section(&mut doc, 1, "After");
        let result = LayoutEngine::new().layout(&doc);
        assert!(
            result.graph.pages.len() >= 2,
            "expected pagination, got {} page(s)",
            result.graph.pages.len()
        );
        // Locate the heading and the very first paragraph word.
        let mut heading_page: Option<u32> = None;
        let mut first_word_page: Option<u32> = None;
        for page in &result.graph.pages {
            for run in &page.runs {
                if run.text == "After"
                    && matches!(run.font, Font::Base14(Base14Font::HelveticaBold))
                {
                    heading_page = Some(page.number);
                }
                if run.text == "word0" && first_word_page.is_none() {
                    first_word_page = Some(page.number);
                }
            }
        }
        let heading_page = heading_page.expect("heading run not emitted");
        let first_word_page = first_word_page.expect("first paragraph word not emitted");
        assert!(
            heading_page > first_word_page,
            "heading on page {heading_page}, first paragraph word on page {first_word_page}"
        );
    }

    #[test]
    fn heading_with_number_attribute_emits_prefix_run() {
        // Resolver writes `number = "2.1"` onto a section node; layout
        // must emit a leading bold run with that number plus a trailing
        // dot, ahead of the heading text.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let mut attrs = AttrMap::new();
        attrs.insert("level".to_owned(), AttrValue::Int(2));
        attrs.insert("number".to_owned(), AttrValue::Str("2.1".to_owned()));
        let section = doc.alloc_child(
            doc.root,
            Node {
                id: NodeId::default(),
                kind: NodeKind::Section,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: attrs,
            },
        );
        alloc_inline(&mut doc, section, NodeKind::Text, "Background");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        assert!(matches!(
            runs[0].font,
            Font::Base14(Base14Font::HelveticaBold)
        ));
        assert_eq!(runs[0].text, "2.1.");
        assert!(runs.iter().any(|r| r.text == "Background"));
        // The number's baseline matches the title's baseline because
        // they live on the same line.
        let title = runs.iter().find(|r| r.text == "Background").unwrap();
        assert!((runs[0].baseline_from_top_pt - title.baseline_from_top_pt).abs() < 1e-3);
    }

    #[test]
    fn reference_node_renders_resolved_text() {
        // A `Reference` node with a `text` attribute (set by the
        // resolver) flows through `collect_words` like any other inline
        // — no separate code path. The font defaults to the body face.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let para = make_paragraph(&mut doc, "see");
        let mut attrs = AttrMap::new();
        attrs.insert("label".to_owned(), AttrValue::Str("intro".to_owned()));
        attrs.insert("text".to_owned(), AttrValue::Str("1.2".to_owned()));
        doc.alloc_child(
            para,
            Node {
                id: NodeId::default(),
                kind: NodeKind::Reference,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: attrs,
            },
        );
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        let reference = runs.iter().find(|r| r.text == "1.2").expect("ref run");
        assert!(matches!(
            reference.font,
            Font::Base14(Base14Font::Helvetica)
        ));
    }

    #[test]
    fn set_blocks_are_skipped() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let mut attrs = AttrMap::new();
        attrs.insert("set".to_owned(), AttrValue::Str("page".to_owned()));
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
        make_paragraph(&mut doc, "body");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "body");
    }
}
