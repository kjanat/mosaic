//! Layout engine for Mosaic.
//!
//! MVP 0 implements the smallest end-to-end slice that gets ink on a
//! page: greedy line-breaking against fixed A4 metrics, walking a
//! lowered [`Document`] into a [`PageGraph`]. Real shaping
//! (`HarfBuzz`/`rustybuzz`), Knuth-Plass, hyphenation, and font
//! embedding are deferred per the manifest's MVP roadmap (§30,
//! §22.1, §22.2). Boundary-state reuse for incremental builds
//! (§22.3, §33) is also out of scope here.

mod metrics;

pub use metrics::{ALL_FONTS, Font, ascent, descent, glyph_width, text_width};

use mosaic_core::{
    AttrValue, Diagnostic, DiagnosticCode, Document, Node, NodeKind, Severity, SourceSpan,
};

/// A4 page width in PDF points (1pt = 1/72 inch).
pub const A4_WIDTH_PT: f32 = 595.276;
/// A4 page height in PDF points.
pub const A4_HEIGHT_PT: f32 = 841.890;
/// Page margin in points (24mm × 72/25.4).
pub const MARGIN_PT: f32 = 68.031;

/// Body font size (manifest §22.1 default; MVP 0 hard-codes it).
const BODY_SIZE_PT: f32 = 11.0;
/// Body leading multiplier (line height = size × leading).
const BODY_LEADING: f32 = 1.35;

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

/// Absolute typographic length, in points. Wraps `f32` so callers
/// can't accidentally mix points with millimetres. f32 matches the
/// PDF backend's coordinate type with plenty of precision for any
/// realistic page geometry.
#[derive(Copy, Clone, Debug, Default, PartialEq, PartialOrd)]
pub struct Pt(pub f32);

/// A single horizontal run of text on a page. The MVP 0 emitter
/// produces one run per word; coalescing same-font neighbours is an
/// MVP 2 optimisation.
#[derive(Clone, Debug)]
pub struct TextRun {
    /// X coordinate of the run's left edge, measured from the page's
    /// left edge in points.
    pub x_pt: f32,
    /// Y coordinate of the run's baseline, measured from the page's
    /// **top** edge in points. The PDF backend flips to bottom-origin
    /// once when emitting.
    pub baseline_from_top_pt: f32,
    /// Font size in points.
    pub size_pt: f32,
    /// Font face for this run.
    pub font: Font,
    /// Text content. Already filtered to printable ASCII by the
    /// engine — non-ASCII has been substituted with `?`.
    pub text: String,
}

/// One laid-out page.
#[derive(Clone, Debug)]
pub struct Page {
    pub number: u32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub runs: Vec<TextRun>,
}

/// The paginated output graph (manifest §6 stage 7).
#[derive(Clone, Debug, Default)]
pub struct PageGraph {
    pub pages: Vec<Page>,
}

/// Result of laying out a [`Document`]: a [`PageGraph`] plus any
/// warnings the engine emitted (e.g. non-ASCII substitutions). Mirrors
/// `mosaic_eval::LowerResult` so the CLI can render diagnostics
/// uniformly.
#[derive(Debug)]
pub struct LayoutResult {
    pub graph: PageGraph,
    pub diagnostics: Vec<Diagnostic>,
}

/// The driver for MVP 0 layout.
#[derive(Debug, Default)]
pub struct LayoutEngine;

impl LayoutEngine {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Lay out `document` into a [`PageGraph`]. Never returns an
    /// error in MVP 0 — invalid blocks are skipped and surfaced as
    /// diagnostics on `LayoutResult` instead.
    pub fn layout(&mut self, document: &Document) -> LayoutResult {
        let mut state = LayoutState::new();
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
                // `#set` blocks are stashed as `Raw` children of the
                // root by the lowerer; recognise them via the `set`
                // attribute and skip silently.
                NodeKind::Raw if node.attributes.contains_key("set") => {}
                _ => {
                    // Unknown top-level kinds (Figure, Table, etc.)
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
}

impl LayoutState {
    fn new() -> Self {
        Self {
            pages: Vec::new(),
            current_page: blank_page(1),
            cursor_y: MARGIN_PT,
            page_has_content: false,
            diagnostics: Vec::new(),
        }
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
            graph: PageGraph { pages: self.pages },
            diagnostics: self.diagnostics,
        }
    }

    fn layout_heading(&mut self, document: &Document, section: &Node) {
        let level = read_level(section).unwrap_or(1).clamp(1, 3) as usize;
        let size = HEADING_SIZES_PT[level - 1];
        let space_before = HEADING_SPACE_BEFORE_PT[level - 1];
        let space_after = HEADING_SPACE_AFTER_PT[level - 1];

        if self.page_has_content {
            self.cursor_y += space_before;
        }
        let words = self.collect_words(document, section, Font::HelveticaBold, size);
        self.flow_words(&words, BODY_LEADING);
        self.cursor_y += space_after;
    }

    fn layout_paragraph(&mut self, document: &Document, paragraph: &Node) {
        let words = self.collect_words(document, paragraph, Font::Helvetica, BODY_SIZE_PT);
        self.flow_words(&words, BODY_LEADING);
        self.cursor_y += PARA_SPACE_AFTER_PT;
    }

    /// Walk `parent`'s inline children and produce a flat list of
    /// [`Word`]s. Inline newlines collapse to spaces; non-ASCII chars
    /// are replaced with `?` and a `W040` warning is emitted once per
    /// inline that contained any.
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
                NodeKind::Strong => Font::HelveticaBold,
                NodeKind::Emphasis => Font::HelveticaOblique,
                NodeKind::Raw => Font::Courier,
                _ => default_font,
            };
            let raw = match child.attributes.get("text") {
                Some(AttrValue::Str(s)) => s.as_str(),
                _ => continue,
            };
            let cleaned = sanitize_text(raw, &child.span, &mut self.diagnostics);
            for piece in cleaned.split_ascii_whitespace() {
                if piece.is_empty() {
                    continue;
                }
                out.push(Word {
                    text: piece.to_owned(),
                    font,
                    size_pt: size,
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
        let line_width = A4_WIDTH_PT - 2.0 * MARGIN_PT;
        let mut line: Vec<Word> = Vec::new();
        let mut line_width_used = 0.0_f32;
        let mut max_size_on_line = 0.0_f32;

        for word in words {
            let w_width = text_width(word.font, word.size_pt, &word.text);
            // Hard wrap: a single word wider than the column gets
            // chopped into character-sized pieces, each on its own
            // line. This is the contract MVP 0 documents.
            if w_width > line_width {
                if !line.is_empty() {
                    self.flush_line(&line, leading, max_size_on_line);
                    line.clear();
                    line_width_used = 0.0;
                    max_size_on_line = 0.0;
                }
                self.flush_oversize_word(word, leading);
                continue;
            }
            let space_w = if line.is_empty() {
                0.0
            } else {
                text_width(word.font, word.size_pt, " ")
            };
            if line_width_used + space_w + w_width > line_width && !line.is_empty() {
                self.flush_line(&line, leading, max_size_on_line);
                line.clear();
                line_width_used = 0.0;
                max_size_on_line = 0.0;
            }
            let space_w = if line.is_empty() {
                0.0
            } else {
                text_width(word.font, word.size_pt, " ")
            };
            line_width_used += space_w + w_width;
            if word.size_pt > max_size_on_line {
                max_size_on_line = word.size_pt;
            }
            line.push(word.clone());
        }
        if !line.is_empty() {
            self.flush_line(&line, leading, max_size_on_line);
        }
    }

    /// Emit one line worth of words at `cursor_y`, advancing past it.
    fn flush_line(&mut self, line: &[Word], leading: f32, max_size: f32) {
        // Reserve space at the top of the page: if this is the first
        // line on the page, advance the cursor by the ascent so the
        // baseline sits below the top margin.
        if !self.page_has_content {
            self.cursor_y = MARGIN_PT + max_size; // approximate ascent
        }
        // Page break if the baseline would fall below the bottom
        // margin. Compare against descent-aware bottom: descent is
        // small and absorbed by the margin in this MVP, so the simple
        // check is `cursor_y > height - margin`.
        if self.cursor_y > A4_HEIGHT_PT - MARGIN_PT {
            self.start_new_page();
            self.cursor_y = MARGIN_PT + max_size;
        }

        let mut x = MARGIN_PT;
        for (i, word) in line.iter().enumerate() {
            if i > 0 {
                x += text_width(word.font, word.size_pt, " ");
            }
            self.current_page.runs.push(TextRun {
                x_pt: x,
                baseline_from_top_pt: self.cursor_y,
                size_pt: word.size_pt,
                font: word.font,
                text: word.text.clone(),
            });
            x += text_width(word.font, word.size_pt, &word.text);
        }
        self.page_has_content = true;
        self.cursor_y += max_size * leading;
    }

    /// Emit a word that's wider than the column by chopping it on
    /// character boundaries. Each chunk goes on its own line.
    fn flush_oversize_word(&mut self, word: &Word, leading: f32) {
        let line_width = A4_WIDTH_PT - 2.0 * MARGIN_PT;
        let mut buf = String::new();
        let mut buf_width = 0.0_f32;
        for ch in word.text.chars() {
            let w = glyph_width(word.font, word.size_pt, ch);
            if buf_width + w > line_width && !buf.is_empty() {
                let chunk = std::mem::take(&mut buf);
                self.flush_line(
                    &[Word {
                        text: chunk,
                        font: word.font,
                        size_pt: word.size_pt,
                    }],
                    leading,
                    word.size_pt,
                );
                buf_width = 0.0;
            }
            buf.push(ch);
            buf_width += w;
        }
        if !buf.is_empty() {
            self.flush_line(
                &[Word {
                    text: buf,
                    font: word.font,
                    size_pt: word.size_pt,
                }],
                leading,
                word.size_pt,
            );
        }
    }

    fn start_new_page(&mut self) {
        let next_number = self.current_page.number + 1;
        let finished = std::mem::replace(&mut self.current_page, blank_page(next_number));
        self.pages.push(finished);
        self.cursor_y = MARGIN_PT;
        self.page_has_content = false;
    }
}

#[derive(Clone, Debug)]
struct Word {
    text: String,
    font: Font,
    size_pt: f32,
}

fn blank_page(number: u32) -> Page {
    Page {
        number,
        width_pt: A4_WIDTH_PT,
        height_pt: A4_HEIGHT_PT,
        runs: Vec::new(),
    }
}

fn read_level(section: &Node) -> Option<u8> {
    match section.attributes.get("level") {
        Some(AttrValue::Int(n)) if *n >= 1 => u8::try_from((*n).clamp(1, 255)).ok(),
        _ => None,
    }
}

/// Replace any non-ASCII character with `?` and emit a `W040`
/// warning if at least one substitution happened. Also normalises
/// CR/LF (already done by the parser, but defensive — newlines and
/// tabs collapse to spaces so word-splitting is uniform).
fn sanitize_text(raw: &str, span: &SourceSpan, diagnostics: &mut Vec<Diagnostic>) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut substituted = false;
    for ch in raw.chars() {
        let code = ch as u32;
        if ch == '\n' || ch == '\r' || ch == '\t' {
            out.push(' ');
        } else if (0x20..=0x7E).contains(&code) {
            out.push(ch);
        } else {
            out.push('?');
            substituted = true;
        }
    }
    if substituted {
        diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            code: DiagnosticCode("W040"),
            message: "non-ASCII text replaced with `?` — full Unicode lands in MVP 2".to_owned(),
            span: Some(span.clone()),
            notes: Vec::new(),
            suggestions: Vec::new(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use std::path::PathBuf;

    use mosaic_core::{
        AttrMap, AttrValue, ContentHash, Document, Node, NodeId, NodeKind, SourceSpan, StyleId,
    };

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

    #[test]
    fn heading_then_paragraph_emits_runs_in_order() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_section(&mut doc, 1, "Hello");
        make_paragraph(&mut doc, "body");
        let result = LayoutEngine::new().layout(&doc);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.graph.pages.len(), 1);
        let runs = &result.graph.pages[0].runs;
        assert!(runs.len() >= 2, "expected at least 2 runs, got {runs:?}");
        // Heading first, body below it.
        assert!(matches!(runs[0].font, Font::HelveticaBold));
        assert_eq!(runs[0].text, "Hello");
        let body_run = runs.iter().find(|r| r.text == "body").expect("body run");
        assert!(matches!(body_run.font, Font::Helvetica));
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
        let para = make_paragraph(&mut doc, "before");
        alloc_inline(&mut doc, para, NodeKind::Emphasis, "italic");
        alloc_inline(&mut doc, para, NodeKind::Text, "after");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        let italic = runs
            .iter()
            .find(|r| r.text == "italic")
            .expect("italic run");
        assert!(matches!(italic.font, Font::HelveticaOblique));
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
    fn non_ascii_substitutes_and_warns() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_paragraph(&mut doc, "café");
        let result = LayoutEngine::new().layout(&doc);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code.0, "W040");
        let runs = &result.graph.pages[0].runs;
        assert!(runs.iter().any(|r| r.text == "caf?"));
    }

    #[test]
    fn empty_document_emits_one_blank_page() {
        let doc = Document::new(PathBuf::from("test.mos"));
        let result = LayoutEngine::new().layout(&doc);
        assert_eq!(result.graph.pages.len(), 1);
        assert!(result.graph.pages[0].runs.is_empty());
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
