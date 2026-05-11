//! Layout engine for Mosaic.
//!
//! MVP 0 implements the smallest end-to-end slice that gets ink on a
//! page: greedy line-breaking against fixed A4 metrics, walking a
//! lowered [`Document`] into a [`PageGraph`]. Real shaping
//! (`HarfBuzz`/`rustybuzz`), Knuth-Plass, hyphenation, and font
//! embedding are deferred per the manifest's MVP roadmap (§30,
//! §22.1, §22.2). Boundary-state reuse for incremental builds
//! (§22.3, §33) is also out of scope here.

pub use mosaic_fonts::{Base14Font, Font, ascent, descent, glyph_width, text_width};

use mosaic_core::{
    AttrValue, Diagnostic, DiagnosticCode, Document, Node, NodeKind, Severity, SourceSpan,
};

/// A4 page width in PDF points (1pt = 1/72 inch). Kept as a public
/// constant so external callers can still read the default; the layout
/// engine now consults `PageStyle` instead of these directly.
pub const A4_WIDTH_PT: f32 = 595.276;
/// A4 page height in PDF points.
pub const A4_HEIGHT_PT: f32 = 841.890;
/// Default page margin in points (24mm × 72/25.4).
pub const MARGIN_PT: f32 = 68.031;

/// Default body font size (manifest §22.1).
const BODY_SIZE_PT: f32 = 11.0;
/// Default body leading multiplier (line height = size × leading).
const BODY_LEADING: f32 = 1.35;

/// Page geometry resolved from `#set page(...)`. `width_pt`/`height_pt`
/// describe the full media box; `margin_pt` is symmetric on all four
/// sides for MVP 1.5 (per-side margins are deferred).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageStyle {
    pub width_pt: f32,
    pub height_pt: f32,
    pub margin_pt: f32,
}

impl Default for PageStyle {
    fn default() -> Self {
        Self {
            width_pt: A4_WIDTH_PT,
            height_pt: A4_HEIGHT_PT,
            margin_pt: MARGIN_PT,
        }
    }
}

/// Body text style resolved from `#set text(...)`. `leading` applies
/// to body paragraphs only; headings keep their own multiplier so a
/// `#set text(leading: 2.0)` doesn't balloon section titles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size_pt: f32,
    pub leading: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            size_pt: BODY_SIZE_PT,
            leading: BODY_LEADING,
        }
    }
}

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
    /// Text content. Already filtered to PDF `WinAnsiEncoding`-
    /// representable characters by the engine — non-`WinAnsi` has
    /// been substituted with `?`.
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
/// warnings the engine emitted (e.g. non-`WinAnsi` substitutions). Mirrors
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
                // `#set` blocks are stashed as `Raw` children of the
                // root; folded into styles by `resolve_styles` above.
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

/// Walk root children in source order and fold each `#set page(...)`
/// and `#set text(...)` into a [`PageStyle`] / [`TextStyle`]. Later
/// directives win (last-write-wins). `#set document(...)` is consumed
/// by the lowerer for PDF metadata and ignored here.
fn resolve_styles(document: &Document) -> (PageStyle, TextStyle, Vec<Diagnostic>) {
    let mut page = PageStyle::default();
    let mut text = TextStyle::default();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let Some(root) = document.get(document.root) else {
        return (page, text, diagnostics);
    };
    for child_id in &root.children {
        let Some(node) = document.get(*child_id) else {
            continue;
        };
        if node.kind != NodeKind::Raw {
            continue;
        }
        let Some(AttrValue::Str(target)) = node.attributes.get("set") else {
            continue;
        };
        match target.as_str() {
            "page" => apply_page_set(node, &mut page, &text, &mut diagnostics),
            "text" => apply_text_set(node, &mut text, &page, &mut diagnostics),
            _ => {}
        }
    }
    (page, text, diagnostics)
}

fn apply_page_set(
    node: &Node,
    page: &mut PageStyle,
    text: &TextStyle,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Stage all updates from this directive into `next` and validate
    // the *combined* result against both the new page geometry and
    // the carried-over text style. Validating field-at-a-time would
    // miss the case where only `paper` changes and either the carried
    // margin or the carried text.size becomes unworkable on the new
    // page (e.g. `paper: "A0", margin: 300pt` then `paper: "A5"`, or
    // `text(size: 50pt)` then `paper: "A8"`).
    let mut next = *page;
    if let Some(AttrValue::Str(name)) = node.attributes.get("set.arg.paper") {
        if let Some((w, h)) = paper_size_pt(name) {
            next.width_pt = w;
            next.height_pt = h;
        } else {
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                code: DiagnosticCode("E023"),
                message: format!(
                    "unknown paper size `{name}` (expected an ISO A/B size or `Letter`/`Legal`)"
                ),
                span: Some(node.span.clone()),
                notes: Vec::new(),
                suggestions: Vec::new(),
            });
        }
    }
    if let Some(AttrValue::Length(pt)) = node.attributes.get("set.arg.margin") {
        next.margin_pt = pt_to_f32(*pt);
    }
    // Reject geometrically impossible margins.
    if next.margin_pt < 0.0 || 2.0 * next.margin_pt >= next.width_pt {
        diagnostics.push(reject(
            node,
            format!(
                "page margin {:.2}pt is invalid for a {:.0}pt-wide page; previous value retained",
                next.margin_pt, next.width_pt
            ),
        ));
        return;
    }
    // Reject page changes that would make the carried text.size_pt
    // overflow the page's vertical margin gap.
    let available_pt = next.height_pt - 2.0 * next.margin_pt;
    if available_pt > 0.0 && text.size_pt > available_pt {
        diagnostics.push(reject(
            node,
            format!(
                "page change to {:.0}×{:.0}pt leaves text size {:.2}pt too large for {:.2}pt of vertical space; previous page geometry retained",
                next.width_pt, next.height_pt, text.size_pt, available_pt
            ),
        ));
        return;
    }
    *page = next;
}

fn apply_text_set(
    node: &Node,
    text: &mut TextStyle,
    page: &PageStyle,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut next = *text;
    if let Some(AttrValue::Length(pt)) = node.attributes.get("set.arg.size") {
        next.size_pt = pt_to_f32(*pt);
    }
    if let Some(AttrValue::Float(v)) = node.attributes.get("set.arg.leading") {
        next.leading = pt_to_f32(*v);
    }
    if next.size_pt <= 0.0 {
        diagnostics.push(reject(
            node,
            format!(
                "text size {:.2}pt is not positive; previous value retained",
                next.size_pt
            ),
        ));
        return;
    }
    // Leading must be strictly positive — zero or negative would
    // stack lines on top of each other or walk upward.
    if next.leading <= 0.0 {
        diagnostics.push(reject(
            node,
            format!(
                "text leading {:.2} is not positive; previous value retained",
                next.leading
            ),
        ));
        return;
    }
    // The new text.size_pt must fit in the page's vertical margin
    // gap; otherwise `flush_line` would page-break repeatedly into
    // the same off-page state. text.size_pt is a safe upper bound on
    // a line's ascent for our standard fonts (ascent < size).
    let available_pt = page.height_pt - 2.0 * page.margin_pt;
    if available_pt > 0.0 && next.size_pt > available_pt {
        diagnostics.push(reject(
            node,
            format!(
                "text size {:.2}pt does not fit in {:.2}pt of vertical space on the {:.0}×{:.0}pt page; previous value retained",
                next.size_pt, available_pt, page.width_pt, page.height_pt
            ),
        ));
        return;
    }
    *text = next;
}

/// Build an `E025` diagnostic for a `#set` argument whose value, while
/// well-typed, would produce broken page geometry. The value is *not*
/// applied; the previous (or default) value is retained.
fn reject(node: &Node, message: String) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: DiagnosticCode("E025"),
        message,
        span: Some(node.span.clone()),
        notes: Vec::new(),
        suggestions: Vec::new(),
    }
}

/// Narrow an `f64` measurement (always a small positive page-pt or
/// dimensionless leading multiplier) to `f32`. Values arriving here
/// are bounded above by the largest ISO-216 size (~4000pt), so the
/// cast cannot overflow and any lost precision sits well below a
/// typographic point.
#[allow(
    clippy::cast_possible_truncation,
    reason = "values bounded to typographic ranges; loss is sub-pt"
)]
fn pt_to_f32(v: f64) -> f32 {
    v as f32
}

/// Resolve a paper-size name (`"A4"`, `"B5"`, `"Letter"`, `"Legal"`) to
/// `(width_pt, height_pt)`. ISO 216 `A` and `B` sizes are computed
/// algorithmically; non-ISO sizes are explicit constants.
///
/// Formula: A0 = 841 × 1189 mm. Each subsequent size halves the long
/// edge: `A(n+1)` has width = `floor(A_n.height / 2)`, height =
/// `A_n.width`. B0 = 1000 × 1414 mm follows the same recurrence.
#[allow(
    clippy::cast_precision_loss,
    reason = "ISO 216 dimensions max out at ~4000mm, well inside f32's 23-bit mantissa"
)]
pub fn paper_size_pt(name: &str) -> Option<(f32, f32)> {
    let mm_to_pt = 72.0_f32 / 25.4_f32;
    if let Some(rest) = name.strip_prefix(['A', 'a'])
        && let Ok(n) = rest.parse::<u8>()
        && n <= 10
    {
        let (w_mm, h_mm) = iso_size(841, 1189, n);
        return Some((w_mm as f32 * mm_to_pt, h_mm as f32 * mm_to_pt));
    }
    if let Some(rest) = name.strip_prefix(['B', 'b'])
        && let Ok(n) = rest.parse::<u8>()
        && n <= 10
    {
        let (w_mm, h_mm) = iso_size(1000, 1414, n);
        return Some((w_mm as f32 * mm_to_pt, h_mm as f32 * mm_to_pt));
    }
    match name {
        "Letter" | "letter" | "US-Letter" => Some((612.0, 792.0)),
        "Legal" | "legal" | "US-Legal" => Some((612.0, 1008.0)),
        _ => None,
    }
}

fn iso_size(w0_mm: u32, h0_mm: u32, n: u8) -> (u32, u32) {
    let mut w = w0_mm;
    let mut h = h0_mm;
    for _ in 0..n {
        let new_w = h / 2;
        let new_h = w;
        w = new_w;
        h = new_h;
    }
    (w, h)
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
        }
    }

    fn column_width_pt(&self) -> f32 {
        self.page.width_pt - 2.0 * self.page.margin_pt
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
        let level = usize::from(read_level(section).unwrap_or(1).clamp(1, 3));
        let size = HEADING_SIZES_PT[level - 1];
        let space_before = HEADING_SPACE_BEFORE_PT[level - 1];
        let space_after = HEADING_SPACE_AFTER_PT[level - 1];

        if self.page_has_content {
            self.cursor_y += space_before;
        }
        let mut words =
            self.collect_words(document, section, Font(Base14Font::HelveticaBold), size);
        // Resolver-assigned section number is rendered as a leading
        // word so it gets the same font/size as the title and flows
        // through the existing line-break path. The trailing `.` is
        // the conventional "1." style; `#set heading(numbering: ...)`
        // (manifest §4) overrides it once `#set` is interpreted.
        if let Some(number) = read_str_attr(section, "number") {
            let prefix = format!("{number}.");
            let width_pt = text_width(Font(Base14Font::HelveticaBold), size, &prefix);
            words.insert(
                0,
                Word {
                    text: prefix,
                    font: Font(Base14Font::HelveticaBold),
                    size_pt: size,
                    width_pt,
                },
            );
        }
        self.flow_words(&words, BODY_LEADING);
        self.cursor_y += space_after;
    }

    fn layout_paragraph(&mut self, document: &Document, paragraph: &Node) {
        let size = self.text.size_pt;
        let leading = self.text.leading;
        let words = self.collect_words(document, paragraph, Font(Base14Font::Helvetica), size);
        self.flow_words(&words, leading);
        self.cursor_y += PARA_SPACE_AFTER_PT;
    }

    /// Walk `parent`'s inline children and produce a flat list of
    /// [`Word`]s. Inline newlines collapse to spaces; non-`WinAnsi`
    /// chars are replaced with `?` and a `W040` warning is emitted
    /// once per inline that contained any.
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
                NodeKind::Strong => Font(Base14Font::HelveticaBold),
                NodeKind::Emphasis => Font(Base14Font::HelveticaOblique),
                NodeKind::Raw => Font(Base14Font::Courier),
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
                let width_pt = text_width(font, size, piece);
                out.push(Word {
                    text: piece.to_owned(),
                    font,
                    size_pt: size,
                    width_pt,
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
        let max_size = line.iter().map(|w| w.size_pt).fold(0.0_f32, f32::max);
        let max_ascent = line
            .iter()
            .map(|w| ascent(w.font, w.size_pt))
            .fold(0.0_f32, f32::max);

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

        let mut x = self.page.margin_pt;
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
            x += word.width_pt;
        }
        self.page_has_content = true;
        self.cursor_y += max_size * leading;
    }

    /// Emit a word that's wider than the column by chopping it on
    /// character boundaries. Each chunk goes on its own line.
    fn flush_oversize_word(&mut self, word: &Word, leading: f32) {
        let line_width = self.column_width_pt();
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
                        width_pt: buf_width,
                    }],
                    leading,
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
                    width_pt: buf_width,
                }],
                leading,
            );
        }
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

#[derive(Clone, Debug)]
struct Word {
    text: String,
    font: Font,
    size_pt: f32,
    /// Pre-computed advance width — populated when the word is
    /// constructed in `collect_words` so the line-breaker doesn't
    /// re-measure on every comparison.
    width_pt: f32,
}

fn blank_page(number: u32, style: PageStyle) -> Page {
    Page {
        number,
        width_pt: style.width_pt,
        height_pt: style.height_pt,
        runs: Vec::new(),
    }
}

fn read_level(section: &Node) -> Option<u8> {
    match section.attributes.get("level") {
        Some(AttrValue::Int(n)) if *n >= 1 => u8::try_from((*n).clamp(1, 255)).ok(),
        _ => None,
    }
}

fn read_str_attr<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    match node.attributes.get(key) {
        Some(AttrValue::Str(s)) => Some(s.as_str()),
        _ => None,
    }
}

/// Replace any character without a PDF `WinAnsiEncoding` slot with
/// `?` and emit a `W040` warning if at least one substitution
/// happened. Also normalises CR/LF/tab to space so word-splitting is
/// uniform. `WinAnsi` covers ASCII, Latin-1, and the Windows-specific
/// `0x80..=0x9F` band (Euro, smart quotes, bullet, …); anything else
/// — Cyrillic, CJK, etc. — needs font embedding (issue #9).
fn sanitize_text(raw: &str, span: &SourceSpan, diagnostics: &mut Vec<Diagnostic>) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut substituted = false;
    for ch in raw.chars() {
        if ch == '\n' || ch == '\r' || ch == '\t' {
            out.push(' ');
        } else if mosaic_fonts::winansi_byte(ch).is_some() {
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
            message: "character not representable in WinAnsi (PDF base-font encoding) \
                      replaced with `?` — embedded fonts land in MVP 2"
                .to_owned(),
            span: Some(span.clone()),
            notes: Vec::new(),
            suggestions: Vec::new(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "tests panic loudly on setup failure; matches crate-wide test-module convention"
    )]
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
        assert!(matches!(runs[0].font, Font(Base14Font::HelveticaBold)));
        assert_eq!(runs[0].text, "Hello");
        let body_run = runs.iter().find(|r| r.text == "body").expect("body run");
        assert!(matches!(body_run.font, Font(Base14Font::Helvetica)));
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
        assert!(matches!(italic.font, Font(Base14Font::HelveticaOblique)));
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
    fn non_winansi_substitutes_and_warns() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        // Cyrillic "Привет" has no WinAnsi slot — every char substitutes.
        make_paragraph(&mut doc, "Привет");
        let result = LayoutEngine::new().layout(&doc);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code.0, "W040");
        assert!(
            result.diagnostics[0].message.contains("WinAnsi"),
            "W040 message should reference WinAnsi, got {:?}",
            result.diagnostics[0].message
        );
        let runs = &result.graph.pages[0].runs;
        assert!(runs.iter().any(|r| r.text == "??????"));
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
        let para = make_paragraph(&mut doc, "before");
        alloc_inline(&mut doc, para, NodeKind::Raw, "code");
        alloc_inline(&mut doc, para, NodeKind::Text, "after");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        let code_run = runs.iter().find(|r| r.text == "code").expect("code run");
        assert!(matches!(code_run.font, Font(Base14Font::Courier)));
        // Adjacent runs stay in the default Helvetica face so the
        // engine isn't accidentally promoting everything to Courier.
        assert!(matches!(
            runs.iter().find(|r| r.text == "before").unwrap().font,
            Font(Base14Font::Helvetica)
        ));
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
                if run.text == "After" && matches!(run.font, Font(Base14Font::HelveticaBold)) {
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
        assert!(matches!(runs[0].font, Font(Base14Font::HelveticaBold)));
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
        assert!(matches!(reference.font, Font(Base14Font::Helvetica)));
    }

    fn alloc_set_block(doc: &mut Document, target: &str, args: &[(&str, AttrValue)]) -> NodeId {
        let mut attrs = AttrMap::new();
        attrs.insert("set".to_owned(), AttrValue::Str(target.to_owned()));
        for (k, v) in args {
            attrs.insert(format!("set.arg.{k}"), v.clone());
        }
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
    fn set_page_margin_shifts_runs_inward() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        // 50mm × 72/25.4 ≈ 141.732 pt
        alloc_set_block(
            &mut doc,
            "page",
            &[("margin", AttrValue::Length(50.0 * 72.0 / 25.4))],
        );
        make_paragraph(&mut doc, "hi");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        assert!(!runs.is_empty());
        let expected = 50.0_f32 * 72.0 / 25.4;
        assert!(
            (runs[0].x_pt - expected).abs() < 0.05,
            "x = {}, expected ~{expected}",
            runs[0].x_pt
        );
    }

    #[test]
    fn set_page_paper_a5_changes_page_dimensions() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(
            &mut doc,
            "page",
            &[("paper", AttrValue::Str("A5".to_owned()))],
        );
        make_paragraph(&mut doc, "hi");
        let result = LayoutEngine::new().layout(&doc);
        let page = &result.graph.pages[0];
        // A5 = 148 × 210 mm → 419.5 × 595.3 pt.
        let expected_w = 148.0_f32 * 72.0 / 25.4;
        let expected_h = 210.0_f32 * 72.0 / 25.4;
        assert!(
            (page.width_pt - expected_w).abs() < 1.0,
            "w = {}",
            page.width_pt
        );
        assert!(
            (page.height_pt - expected_h).abs() < 1.0,
            "h = {}",
            page.height_pt
        );
    }

    #[test]
    fn set_text_size_changes_run_size() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(20.0))]);
        make_paragraph(&mut doc, "hi");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        assert!((runs[0].size_pt - 20.0).abs() < 0.01);
    }

    #[test]
    fn negative_margin_is_rejected_with_e025() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(&mut doc, "page", &[("margin", AttrValue::Length(-10.0))]);
        make_paragraph(&mut doc, "hi");
        let result = LayoutEngine::new().layout(&doc);
        assert!(result.diagnostics.iter().any(|d| d.code.0 == "E025"));
        // Default A4 margin retained.
        let runs = &result.graph.pages[0].runs;
        assert!(
            (runs[0].x_pt - MARGIN_PT).abs() < 0.5,
            "x = {}, expected default {MARGIN_PT}",
            runs[0].x_pt
        );
    }

    #[test]
    fn oversized_margin_is_rejected_with_e025() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        // 400pt × 2 = 800pt, wider than A4's 595pt width.
        alloc_set_block(&mut doc, "page", &[("margin", AttrValue::Length(400.0))]);
        make_paragraph(&mut doc, "hi");
        let result = LayoutEngine::new().layout(&doc);
        assert!(result.diagnostics.iter().any(|d| d.code.0 == "E025"));
    }

    #[test]
    fn paper_shrink_revalidates_carried_margin() {
        // Regression: a margin set on a large paper must be re-checked
        // when a later directive shrinks the paper. Prior implementation
        // only validated when the same node carried both fields, so a
        // paper-only override could leave an oversized margin in place.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(
            &mut doc,
            "page",
            &[
                ("paper", AttrValue::Str("A0".to_owned())),
                ("margin", AttrValue::Length(300.0)),
            ],
        );
        // Then shrink to A5 (419pt wide) — 300pt margin is now > width/2.
        alloc_set_block(
            &mut doc,
            "page",
            &[("paper", AttrValue::Str("A5".to_owned()))],
        );
        make_paragraph(&mut doc, "hi");
        let result = LayoutEngine::new().layout(&doc);
        assert!(
            result.diagnostics.iter().any(|d| d.code.0 == "E025"),
            "expected E025 from paper shrink, got {:?}",
            result.diagnostics
        );
        // Page must remain a valid (A0 + 300pt margin) configuration —
        // the rejected A5 update reverts to the prior staged value.
        let page = &result.graph.pages[0];
        assert!(
            (page.width_pt - 2383.94).abs() < 1.0,
            "w = {}",
            page.width_pt
        );
    }

    #[test]
    fn earlier_valid_size_survives_later_rejection() {
        // CodeRabbit scenario: 50pt is valid against A4, then a 180pt
        // size is too big for A4's vertical gap (~706pt actually fits
        // — pick something that genuinely breaks). The rejected
        // directive must roll back, leaving 50pt intact for layout.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(50.0))]);
        // 1000pt > A4's ~706pt vertical gap — rejected.
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(1000.0))]);
        make_paragraph(&mut doc, "hi");
        let result = LayoutEngine::new().layout(&doc);
        assert!(result.diagnostics.iter().any(|d| d.code.0 == "E025"));
        let runs = &result.graph.pages[0].runs;
        assert!(
            (runs[0].size_pt - 50.0).abs() < 0.01,
            "expected 50pt preserved, got {}",
            runs[0].size_pt
        );
    }

    #[test]
    fn page_change_that_invalidates_carried_text_size_is_rejected() {
        // 100pt is valid against A4 (~706pt gap). Switching to A8
        // (~74pt gap) would make 100pt unfit; reject the page change
        // so both the prior page and the prior text size survive.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(100.0))]);
        alloc_set_block(
            &mut doc,
            "page",
            &[("paper", AttrValue::Str("A8".to_owned()))],
        );
        make_paragraph(&mut doc, "hi");
        let result = LayoutEngine::new().layout(&doc);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.code.0 == "E025" && d.message.contains("page change")),
            "expected E025 about page change, got {:?}",
            result.diagnostics
        );
        let page = &result.graph.pages[0];
        let runs = &page.runs;
        // A4 retained (page change rejected), and text.size = 100 kept.
        assert!((page.width_pt - A4_WIDTH_PT).abs() < 0.5);
        assert!((runs[0].size_pt - 100.0).abs() < 0.01);
    }

    #[test]
    fn oversized_text_size_is_rejected_with_e025() {
        // Regression: a `text.size` larger than the page's vertical
        // margin gap would make every flush_line hit the page-break
        // branch and re-emit at the same off-page baseline. Reject
        // up front, fall back to default size.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        // A4 vertical gap = 841.89 - 2*68.031 ≈ 705.83pt; ask for 1000pt.
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(1000.0))]);
        make_paragraph(&mut doc, "hi");
        let result = LayoutEngine::new().layout(&doc);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.code.0 == "E025" && d.message.contains("vertical space")),
            "expected E025 about vertical space, got {:?}",
            result.diagnostics
        );
        // Default body size retained → run renders at 11pt.
        let runs = &result.graph.pages[0].runs;
        assert!((runs[0].size_pt - BODY_SIZE_PT).abs() < 0.01);
    }

    #[test]
    fn rejected_text_size_says_previous_value_retained() {
        // Wording check: after a valid #set text(size:) sets a custom
        // value, a subsequent invalid one must not claim the default
        // was retained — the previous custom value is.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(14.0))]);
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(-1.0))]);
        make_paragraph(&mut doc, "hi");
        let result = LayoutEngine::new().layout(&doc);
        let msg = result
            .diagnostics
            .iter()
            .find(|d| d.code.0 == "E025")
            .expect("E025 emitted")
            .message
            .as_str();
        assert!(
            msg.contains("previous value retained"),
            "message does not say `previous value retained`: {msg}"
        );
    }

    #[test]
    fn nonpositive_leading_is_rejected_with_e025() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(&mut doc, "text", &[("leading", AttrValue::Float(0.0))]);
        make_paragraph(&mut doc, "hi");
        let result = LayoutEngine::new().layout(&doc);
        assert!(result.diagnostics.iter().any(|d| d.code.0 == "E025"));
    }

    #[test]
    fn unknown_paper_emits_e023() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(
            &mut doc,
            "page",
            &[("paper", AttrValue::Str("Foolscap".to_owned()))],
        );
        make_paragraph(&mut doc, "hi");
        let result = LayoutEngine::new().layout(&doc);
        assert!(result.diagnostics.iter().any(|d| d.code.0 == "E023"));
        // Default A4 retained.
        let page = &result.graph.pages[0];
        assert!((page.width_pt - A4_WIDTH_PT).abs() < 0.5);
    }

    #[test]
    fn last_set_wins() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(8.0))]);
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(20.0))]);
        make_paragraph(&mut doc, "hi");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        assert!((runs[0].size_pt - 20.0).abs() < 0.01);
    }

    #[test]
    fn paper_size_pt_resolves_iso_a_and_letter() {
        let (w, h) = paper_size_pt("A4").unwrap();
        assert!((w - 595.276).abs() < 1.0);
        assert!((h - 841.89).abs() < 1.0);
        let (w, h) = paper_size_pt("A5").unwrap();
        assert!((w - 419.527).abs() < 1.0);
        assert!((h - 595.276).abs() < 1.0);
        let (w, h) = paper_size_pt("Letter").unwrap();
        assert_eq!((w, h), (612.0, 792.0));
        assert!(paper_size_pt("Foolscap").is_none());
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
