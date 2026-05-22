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

pub use mos_fonts::{
    Base14Font, EmbeddedFontId, Font, FontFamily, ShapedGlyph, WordSubRun, ascent, descent,
    glyph_width, shape_with_fallback, text_width,
};
pub use style::paper_size_pt;
pub use types::{
    A4_HEIGHT_PT, A4_WIDTH_PT, ImageHandle, ImagePlacement, LayoutResult, MARGIN_PT, Page,
    PageGraph, PageStyle, TextRun, TextStyle,
};

use std::borrow::Cow;
use std::sync::Arc;

use mos_core::{AttrValue, Diagnostic, DiagnosticCode, Document, Node, NodeId, NodeKind, Severity};
use style::{pt_to_f32, resolve_styles};
use types::BODY_LEADING;

mod style;
mod types;

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

    /// Lay out a top-level `Image` node as a block. The image is
    /// horizontally centred within the column and capped at the column
    /// width — author-declared `width`/`height` attrs override the
    /// natural pixel-at-72-DPI size proportionally.
    fn layout_image(&mut self, node_id: NodeId, image: &Node) {
        let Some((width_pt, height_pt)) = self.intrinsic_image_size(image) else {
            return;
        };
        let Some(handle) = self.intern_image(image) else {
            self.diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                code: DiagnosticCode("W050"),
                message: format!(
                    "image node {:?} missing decoded pixel data; skipping",
                    node_id
                ),
                span: Some(image.span.clone()),
                notes: Vec::new(),
                suggestions: Vec::new(),
            });
            return;
        };
        // Clamp width to column first so the page-break check below
        // tests the *rendered* height, not the intrinsic one. Otherwise
        // a 1000×1000 pixel image (1000pt natural at 72 DPI) would
        // trigger a page break thinking it needs 1000pt, even though
        // it ultimately renders at ~459pt × ~459pt after clamping.
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
        // Reserve vertical space; page-break if the image wouldn't fit
        // in the remaining gap. Images that exceed the full vertical
        // gap render on their own page (clipped at the bottom margin
        // is acceptable for MVP; smarter scaling lands with §10 floats).
        let available_y = self.page.height_pt - self.page.margin_pt;
        if self.cursor_y + render_h > available_y && self.page_has_content {
            self.start_new_page();
        }
        // Centre horizontally within the column.
        let x = self.page.margin_pt + (column_w - render_w) * 0.5;
        self.current_page.images.push(ImagePlacement {
            handle,
            x_pt: x,
            top_from_top_pt: self.cursor_y,
            width_pt: render_w,
            height_pt: render_h,
        });
        self.page_has_content = true;
        // Advance the cursor past the image. `flush_line` interprets
        // `cursor_y` as the *baseline* of the next text line (not its
        // top), so the next paragraph after the image would otherwise
        // place its baseline at `image_bottom + PARA_SPACE_AFTER_PT`
        // and its glyph tops at `image_bottom + PARA_SPACE_AFTER_PT -
        // ascent`, eating into the image. Adding one body-text ascent
        // here shifts the baseline far enough below `image_bottom`
        // that the caption's glyph tops land at exactly
        // `image_bottom + PARA_SPACE_AFTER_PT`.
        let body_ascent = ascent(self.text.family.regular, self.text.size_pt);
        self.cursor_y += render_h + PARA_SPACE_AFTER_PT + body_ascent;
    }

    /// Lay out a `Figure` block: image children render as blocks, the
    /// caption paragraph renders beneath them, and the whole figure
    /// is kept on one page when the remaining space allows. If the
    /// combined height of every child plus inter-block spacing
    /// wouldn't fit on the current page, the figure begins a fresh
    /// page so the image and its caption stay together.
    fn layout_figure(&mut self, document: &Document, figure: &Node) {
        // Pre-flight the combined height. Image heights come from
        // `intrinsic_image_size` (with the same column clamping
        // `layout_image` applies), caption heights from a dry-run
        // line-breaker. Inter-block spacing matches what each
        // `layout_*` call adds.
        let column_w = self.column_width_pt();
        let body_ascent = ascent(self.text.family.regular, self.text.size_pt);
        let mut total_h = 0.0_f32;
        let mut block_count = 0_u32;
        for child_id in &figure.children {
            let Some(child) = document.get(*child_id) else {
                continue;
            };
            let block_h = match child.kind {
                // Images advance the cursor by `render_h + body_ascent`
                // (plus the per-block PARA_SPACE_AFTER_PT added below).
                // See `layout_image` for why the ascent is there.
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
        // Each child's `layout_*` adds `PARA_SPACE_AFTER_PT` after it.
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

    /// Measure the rendered height of `paragraph` without flushing
    /// anything to the current page. Mirrors `flow_words`' greedy
    /// line-breaking exactly so the figure pre-flight matches what
    /// the real layout produces — any divergence would leak figures
    /// onto the wrong page even after the page-break check fired.
    fn measure_paragraph_height(&mut self, document: &Document, paragraph: &Node) -> f32 {
        let size = self.text.size_pt;
        let leading = self.text.leading;
        let regular = self.text.family.regular;
        let words = self.collect_words(document, paragraph, regular, size);
        if words.is_empty() {
            return 0.0;
        }
        let line_width = self.column_width_pt();
        let mut lines: u32 = 1;
        let mut line_width_used = 0.0_f32;
        for word in &words {
            if word.width_pt > line_width {
                // Oversize words wrap at character boundaries; the
                // line-break path emits ceil(advance / line_width)
                // chunks, each on its own line. Round up via integer
                // math without `as_f32` round-tripping.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let chunks = (word.width_pt / line_width).ceil().max(1.0) as u32;
                if line_width_used > 0.0 {
                    lines += 1;
                }
                lines += chunks.saturating_sub(1);
                line_width_used = 0.0;
                continue;
            }
            let space_w = if line_width_used > 0.0 {
                text_width(word.font, word.size_pt, " ")
            } else {
                0.0
            };
            if line_width_used > 0.0 && line_width_used + space_w + word.width_pt > line_width {
                lines += 1;
                line_width_used = word.width_pt;
            } else {
                line_width_used += space_w + word.width_pt;
            }
        }
        #[allow(
            clippy::cast_precision_loss,
            reason = "line counts in any sane document fit well inside the f32 mantissa"
        )]
        let h = (lines as f32) * size * leading;
        h
    }

    /// Resolve the rendered (`width_pt`, `height_pt`) for an Image node,
    /// folding in author-declared dimensions and the 72-DPI natural
    /// fallback. Returns `None` when the node has no pixel dimensions
    /// recorded (which only happens if the lowerer failed to decode
    /// the file, in which case it should have dropped the node).
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
        let natural_w = pw as f32; // 1 pt per pixel ≈ 72 DPI
        let natural_h = ph as f32;
        let declared_w = read_length_attr(image, "width");
        let declared_h = read_length_attr(image, "height");
        let aspect = natural_h / natural_w;
        // When both dimensions are declared, fit the image inside the
        // requested box without distorting — scale uniformly by the
        // tighter of the two ratios. This is the `object-fit: contain`
        // convention rather than LaTeX's "stretch to exact box."
        // Users that want non-uniform scaling can pre-process the
        // bitmap; preserving aspect at the typesetter is the more
        // forgiving default when a quick `width: 200pt, height:
        // 200pt` would otherwise silently squash a 2:1 figure.
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

    /// Look up the image in the dedup table, or intern a new handle.
    /// Returns `None` if the node is missing the decoded pixel buffer
    /// (broken lowerer invariant — diagnose at the call site).
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
            // `Arc<[u8]>::clone()` only bumps a refcount; the slice
            // itself stays shared with the source node.
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

    /// Lay out a [`NodeKind::List`] and its [`NodeKind::ListItem`]
    /// children with hanging indent. Each item gets a marker (`•` for
    /// unordered, `1.` `2.` … for ordered) in the gutter to the left
    /// of its text. Nested [`NodeKind::List`] children under each item
    /// recurse with one more level of indent.
    fn layout_list(&mut self, document: &Document, list_node: &Node) {
        let ordered = matches!(
            list_node.attributes.get("ordered"),
            Some(AttrValue::Bool(true))
        );
        let regular = self.text.family.regular;
        let size = self.text.size_pt;
        let leading = self.text.leading;
        let saved_left = self.current_left_pt;

        // Size the gutter against the widest marker this list will
        // emit. A fixed gutter would crash long markers (`100.`,
        // `1000.`, …) into the text column. We can't shortcut to the
        // last index even for ordered lists: with proportional
        // numerals an earlier marker like `88.` may shape wider than
        // `100.`, so shape every marker and take the max.
        // `LIST_MARKER_GUTTER_PT` is the floor so small lists still
        // get visual breathing room and stay aligned with neighbouring
        // unordered lists at the same depth.
        let widest_marker_pt = if ordered {
            list_node
                .children
                .iter()
                .filter_map(|id| document.get(*id))
                .filter(|n| n.kind == NodeKind::ListItem)
                .enumerate()
                .map(|(idx, _)| {
                    shape_with_fallback(
                        regular,
                        self.text.family.fallbacks,
                        size,
                        &format!("{}.", idx + 1),
                    )
                    .iter()
                    .map(|s| s.advance_pt)
                    .sum()
                })
                .fold(0.0_f32, f32::max)
        } else {
            shape_with_fallback(regular, self.text.family.fallbacks, size, "\u{2022}")
                .iter()
                .map(|s| s.advance_pt)
                .sum()
        };
        let marker_gap_pt = text_width(regular, size, " ");
        let gutter = (widest_marker_pt + marker_gap_pt).max(LIST_MARKER_GUTTER_PT);
        let item_left = saved_left + gutter;

        // Track the rendered index separately from the source position
        // so non-`ListItem` children (forward-compat: comments, future
        // block kinds) don't create gaps in the numbering.
        let mut item_idx = 0_usize;
        for item_id in &list_node.children {
            let Some(item) = document.get(*item_id) else {
                continue;
            };
            if item.kind != NodeKind::ListItem {
                continue;
            }
            item_idx += 1;

            let marker_text = if ordered {
                format!("{item_idx}.")
            } else {
                "\u{2022}".to_owned()
            };
            let subruns =
                shape_with_fallback(regular, self.text.family.fallbacks, size, &marker_text);
            let width_pt: f32 = subruns.iter().map(|s| s.advance_pt).sum();
            let marker_word = Word {
                text: marker_text,
                actual_text: None,
                font: regular,
                size_pt: size,
                width_pt,
                subruns,
            };
            // Right-align the marker against the gutter, leaving
            // `marker_gap_pt` of space between marker and text. This
            // is the standard typographic convention for numbered
            // lists (dots line up vertically across items of differing
            // widths) and harmless for bullets (all share one advance).
            let marker_x = item_left - marker_gap_pt - marker_word.width_pt;

            self.current_left_pt = item_left;
            self.pending_marker = Some(PendingMarker {
                x_pt: marker_x,
                word: marker_word,
            });

            let words = self.collect_words(document, item, regular, size);
            // An item with no inline words — either truly empty (`- \n`)
            // or one whose entire body is a nested list — would
            // otherwise never reach `flush_line`, silently dropping
            // its marker. Force a marker-only line so the marker still
            // renders before any nested children.
            if words.is_empty() {
                self.flush_line(&[], leading);
            } else {
                self.flow_words(&words, leading);
            }

            for child_id in &item.children {
                let Some(child) = document.get(*child_id) else {
                    continue;
                };
                if child.kind == NodeKind::List {
                    self.layout_list(document, child);
                }
            }
        }

        self.current_left_pt = saved_left;
        // Only add trailing space at the top level of a list block.
        // Nested lists inside a list item don't need their own gap
        // because the surrounding item's leading already separates
        // them from the next sibling item.
        if (saved_left - self.page.margin_pt).abs() < f32::EPSILON {
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

#[derive(Clone, Debug)]
struct Word {
    text: String,
    actual_text: Option<String>,
    /// Primary face — the style-resolved choice from the active
    /// `FontFamily` (regular/bold/italic/monospace). Used for line
    /// metrics (ascent/descent), inter-word spacing, and
    /// character-wise hyphenation width estimates. Per-glyph fallback
    /// faces (e.g. Noto Sans Math for `≤`) live inside [`Word::subruns`].
    font: Font,
    size_pt: f32,
    /// Pre-computed advance width — populated when the word is
    /// constructed in `collect_words` (sum of `subruns[i].advance_pt`)
    /// so the line-breaker doesn't re-measure on every comparison.
    width_pt: f32,
    /// Per-glyph-fallback sub-runs produced by `shape_with_fallback`.
    /// One sub-run per contiguous source span that shares a face;
    /// each carries its own font + text slice + glyph stream with
    /// cluster offsets rebased to its local text. `flush_line` emits
    /// one [`TextRun`] per sub-run, advancing the x cursor by
    /// `subrun.advance_pt` between them. For Base14 primary faces
    /// the result is always a single sub-run with empty `glyphs`
    /// (no fallback target — Base14 emit path uses `WinAnsi`-byte
    /// strings instead).
    subruns: Vec<WordSubRun>,
}

fn word_clusters(word: &Word) -> Vec<WordSubRun> {
    let mut clusters = Vec::new();
    for sub in &word.subruns {
        if sub.glyphs.is_empty() {
            for ch in sub.text.chars() {
                let mut text = String::new();
                text.push(ch);
                clusters.push(WordSubRun {
                    font: sub.font,
                    advance_pt: text_width(sub.font, word.size_pt, &text),
                    text,
                    glyphs: Vec::new(),
                });
            }
            continue;
        }

        let mut i = 0;
        while i < sub.glyphs.len() {
            let cluster = sub.glyphs[i].cluster;
            let mut j = i + 1;
            while j < sub.glyphs.len() && sub.glyphs[j].cluster == cluster {
                j += 1;
            }
            let start = usize::try_from(cluster).unwrap_or(usize::MAX);
            let end = if j < sub.glyphs.len() {
                usize::try_from(sub.glyphs[j].cluster).unwrap_or(usize::MAX)
            } else {
                sub.text.len()
            };
            debug_assert!(start <= end && end <= sub.text.len());
            let Some(text) = sub.text.get(start..end) else {
                i = j;
                continue;
            };
            let shift = u32::try_from(start).unwrap_or(u32::MAX);
            let glyphs: Vec<_> = sub.glyphs[i..j]
                .iter()
                .map(|g| ShapedGlyph {
                    cluster: g.cluster.saturating_sub(shift),
                    ..*g
                })
                .collect();
            clusters.push(WordSubRun {
                font: sub.font,
                text: text.to_owned(),
                advance_pt: glyphs_advance_pt(sub.font, word.size_pt, &glyphs),
                glyphs,
            });
            i = j;
        }
    }
    clusters
}

fn glyphs_advance_pt(font: Font, size_pt: f32, glyphs: &[ShapedGlyph]) -> f32 {
    let upem = match font {
        Font::Embedded(id) => f32::from(id.data().units_per_em),
        Font::Base14(_) => 1000.0,
    };
    // Sign-preserving conversion lives in mos_fonts to keep the
    // two crates from drifting on hmtx semantics.
    glyphs
        .iter()
        .map(|g| mos_fonts::advance_units_to_pt(g.advance_units, size_pt, upem))
        .sum()
}

fn blank_page(number: u32, style: PageStyle) -> Page {
    Page {
        number,
        width_pt: style.width_pt,
        height_pt: style.height_pt,
        runs: Vec::new(),
        images: Vec::new(),
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

fn read_int_attr(node: &Node, key: &str) -> Option<i64> {
    match node.attributes.get(key) {
        Some(AttrValue::Int(n)) => Some(*n),
        _ => None,
    }
}

fn read_length_attr(node: &Node, key: &str) -> Option<f32> {
    match node.attributes.get(key) {
        Some(AttrValue::Length(pt)) => Some(pt_to_f32(*pt)),
        _ => None,
    }
}

fn expand_tabs(line: &str, tab_width: usize) -> Cow<'_, str> {
    if !line.contains('\t') {
        return Cow::Borrowed(line);
    }

    let tab_width = tab_width.max(1);
    let mut out = String::with_capacity(line.len());
    let mut col = 0_usize;
    for ch in line.chars() {
        if ch == '\t' {
            let spaces = tab_width - (col % tab_width);
            out.extend(std::iter::repeat_n(' ', spaces));
            col += spaces;
        } else {
            out.push(ch);
            col += 1;
        }
    }
    Cow::Owned(out)
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

    /// Allocate an Image node with the lowerer-shaped attributes the
    /// layout engine expects: `src/resolved_path/pixel_{w,h}/pixels/`.
    /// Used by every image-block test below.
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

    fn alloc_list(doc: &mut Document, parent: NodeId, ordered: bool) -> NodeId {
        let mut attrs = AttrMap::new();
        attrs.insert("ordered".to_owned(), AttrValue::Bool(ordered));
        doc.alloc_child(
            parent,
            Node {
                id: NodeId::default(),
                kind: NodeKind::List,
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
        let page = &result.graph.pages[0];
        assert_eq!(page.images.len(), 1);
        let img = &page.images[0];
        // 100 px → 100 pt at 72 DPI; 60 px → 60 pt.
        assert!((img.width_pt - 100.0).abs() < 0.5);
        assert!((img.height_pt - 60.0).abs() < 0.5);
    }

    #[test]
    fn image_block_declared_width_preserves_aspect_ratio() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_image(&mut doc, "x.png", 200, 100, Some(80.0), None);
        let result = LayoutEngine::new().layout(&doc);
        let img = &result.graph.pages[0].images[0];
        // 200:100 = 2:1, so width 80pt → height 40pt.
        assert!((img.width_pt - 80.0).abs() < 0.5);
        assert!((img.height_pt - 40.0).abs() < 0.5);
    }

    #[test]
    fn image_block_both_dims_fits_inside_box_preserving_aspect() {
        // 2:1 source with `width: 80pt, height: 80pt`: the result must
        // be 80×40 (fit inside the box, scale by the tighter ratio),
        // *not* 80×80 (which would squash the bitmap). Guards the
        // CodeRabbit fix that switched to `object-fit: contain`
        // semantics for both-dims-declared.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_image(&mut doc, "x.png", 200, 100, Some(80.0), Some(80.0));
        let result = LayoutEngine::new().layout(&doc);
        let img = &result.graph.pages[0].images[0];
        assert!((img.width_pt - 80.0).abs() < 0.5, "w = {}", img.width_pt);
        assert!((img.height_pt - 40.0).abs() < 0.5, "h = {}", img.height_pt);
    }

    #[test]
    fn image_block_both_dims_taller_box_fits_by_width() {
        // Symmetric case: 2:1 source with `width: 40pt, height: 80pt`.
        // The width is the tighter constraint (ratio 40/200 = 0.2 vs
        // 80/100 = 0.8), so the scale picks 0.2 → 40×20.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_image(&mut doc, "x.png", 200, 100, Some(40.0), Some(80.0));
        let result = LayoutEngine::new().layout(&doc);
        let img = &result.graph.pages[0].images[0];
        assert!((img.width_pt - 40.0).abs() < 0.5, "w = {}", img.width_pt);
        assert!((img.height_pt - 20.0).abs() < 0.5, "h = {}", img.height_pt);
    }

    #[test]
    fn image_block_clamped_to_column_width() {
        // An image wider than the column should shrink (proportionally)
        // to fit. A4 has ~459 pt of column width with the default 24mm
        // margins.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_image(&mut doc, "x.png", 4000, 2000, None, None);
        let result = LayoutEngine::new().layout(&doc);
        let img = &result.graph.pages[0].images[0];
        let col = A4_WIDTH_PT - 2.0 * MARGIN_PT;
        assert!(img.width_pt <= col + 0.5);
        // Aspect ratio preserved.
        let aspect = 4000.0_f32 / 2000.0;
        let expected_h = img.width_pt / aspect;
        assert!((img.height_pt - expected_h).abs() < 0.5);
    }

    #[test]
    fn oversized_image_after_paragraph_does_not_force_extra_page() {
        // Regression guard: the page-break check used to fire on the
        // unclamped intrinsic height. A 1000×1000-pixel image
        // (1000pt natural at 72 DPI, well over A4's ~706pt vertical
        // gap) would spuriously land on its own page even though the
        // column-width clamp scales it down to ~459pt — which fits
        // alongside a short preceding paragraph. Now the check tests
        // the *rendered* height, so both blocks share page 1.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        make_paragraph(&mut doc, "lead paragraph");
        make_image(&mut doc, "big.png", 1000, 1000, None, None);
        let result = LayoutEngine::new().layout(&doc);
        assert_eq!(
            result.graph.pages.len(),
            1,
            "expected a single page, got {}",
            result.graph.pages.len()
        );
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
        // Both placements reference the same handle id.
        let placements: Vec<&ImagePlacement> = result
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
        // A Figure with an Image and a caption Paragraph as children
        // should produce one image placement and at least one text run
        // (the caption) on the same page, with the caption beneath the
        // image.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
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
        // Inline-allocate an image as a child of the figure.
        let mut img_attrs = AttrMap::new();
        img_attrs.insert("src".to_owned(), AttrValue::Str("fig.png".to_owned()));
        img_attrs.insert(
            "resolved_path".to_owned(),
            AttrValue::Str("/tmp/fig.png".to_owned()),
        );
        img_attrs.insert("pixel_width".to_owned(), AttrValue::Int(80));
        img_attrs.insert("pixel_height".to_owned(), AttrValue::Int(50));
        img_attrs.insert(
            "pixels".to_owned(),
            AttrValue::Bytes(Arc::from(vec![0_u8; 80 * 50 * 3])),
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
        alloc_inline(&mut doc, cap, NodeKind::Text, "Caption text.");
        let result = LayoutEngine::new().layout(&doc);
        let page = &result.graph.pages[0];
        assert_eq!(page.images.len(), 1);
        let caption_run = page
            .runs
            .iter()
            .find(|r| r.text == "Caption" || r.text == "text.")
            .expect("caption run not found");
        // Caption baseline sits below the image top edge.
        assert!(caption_run.baseline_from_top_pt > page.images[0].top_from_top_pt);
    }

    /// Allocate a Figure node whose only children are one Image and a
    /// short caption paragraph. Used by the keep-together test below.
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
        // Place a long lead paragraph so the cursor lands near the
        // bottom of page 1, then drop a figure whose image+caption
        // together exceed the remaining vertical gap. The figure
        // must move *as a unit* to page 2 — splitting the image
        // from its caption would be the regression this test guards.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        // ~80 lines of body text gets us close to the bottom of A4
        // (706pt gap ÷ ~15pt line ≈ 47 lines fit; 80 spills onto a
        // second page with the cursor near the top there). To force
        // a near-bottom state, use a paragraph long enough to spill
        // to page 2 *and* leave only ~100pt left there.
        let mut filler = String::new();
        for i in 0..540 {
            filler.push_str(&format!("word{i} "));
        }
        make_paragraph(&mut doc, filler.trim());
        // A 400×300pt figure won't fit in 100pt of remaining gap.
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
        let figure_page = figure_page.expect("figure image not emitted");
        let caption_page = caption_page.expect("caption run not emitted");
        assert_eq!(
            figure_page, caption_page,
            "figure and caption ended up on different pages ({} vs {})",
            figure_page, caption_page
        );
    }
    fn alloc_list_item(doc: &mut Document, parent: NodeId, text: &str) -> NodeId {
        let id = doc.alloc_child(
            parent,
            Node {
                id: NodeId::default(),
                kind: NodeKind::ListItem,
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
    fn unordered_list_emits_bullet_markers() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, false);
        alloc_list_item(&mut doc, list, "alpha");
        alloc_list_item(&mut doc, list, "beta");
        let result = LayoutEngine::new().layout(&doc);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let runs = &result.graph.pages[0].runs;
        let bullets: Vec<&TextRun> = runs.iter().filter(|r| r.text == "\u{2022}").collect();
        assert_eq!(bullets.len(), 2, "expected 2 bullets, got runs {runs:?}");
        // Markers sit inside the gutter: right-aligned against the
        // text column, never overflowing into either the left margin
        // or the text column itself.
        for bullet in &bullets {
            let bullet_w = text_width(bullet.font, bullet.size_pt, &bullet.text);
            assert!(
                bullet.x_pt >= MARGIN_PT - 0.5,
                "bullet x = {} should sit at or right of the page margin",
                bullet.x_pt
            );
            assert!(
                bullet.x_pt + bullet_w <= MARGIN_PT + LIST_MARKER_GUTTER_PT + 0.5,
                "bullet right edge {} should sit within gutter ending at {}",
                bullet.x_pt + bullet_w,
                MARGIN_PT + LIST_MARKER_GUTTER_PT
            );
        }
        let alpha = runs.iter().find(|r| r.text == "alpha").expect("alpha run");
        assert!(
            alpha.x_pt > bullets[0].x_pt,
            "item text must be indented past marker: alpha.x = {}, bullet.x = {}",
            alpha.x_pt,
            bullets[0].x_pt
        );
    }

    #[test]
    fn ordered_list_numbers_from_one() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, true);
        alloc_list_item(&mut doc, list, "first");
        alloc_list_item(&mut doc, list, "second");
        alloc_list_item(&mut doc, list, "third");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        let markers: Vec<&str> = runs
            .iter()
            .filter(|r| {
                r.text.ends_with('.') && r.text.chars().next().is_some_and(|c| c.is_ascii_digit())
            })
            .map(|r| r.text.as_str())
            .collect();
        assert_eq!(markers, vec!["1.", "2.", "3."]);
    }

    #[test]
    fn list_item_text_indented_past_marker_gutter() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, false);
        alloc_list_item(&mut doc, list, "hello");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        let hello = runs.iter().find(|r| r.text == "hello").expect("hello run");
        // Item text starts one gutter-width inside the page margin.
        let expected = MARGIN_PT + LIST_MARKER_GUTTER_PT;
        assert!(
            (hello.x_pt - expected).abs() < 0.5,
            "text x = {}, expected {expected}",
            hello.x_pt
        );
    }

    #[test]
    fn nested_list_indents_one_more_level() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let outer = alloc_list(&mut doc, root, false);
        let outer_item = alloc_list_item(&mut doc, outer, "outer");
        let inner = alloc_list(&mut doc, outer_item, false);
        alloc_list_item(&mut doc, inner, "inner");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        let bullets: Vec<&TextRun> = runs.iter().filter(|r| r.text == "\u{2022}").collect();
        assert_eq!(bullets.len(), 2);
        // Outer marker sits inside the outer gutter (page margin to
        // one gutter-width inward); inner marker sits inside the
        // inner gutter (one gutter inset from the outer text column).
        let (outer_b, inner_b) = (bullets[0], bullets[1]);
        let outer_w = text_width(outer_b.font, outer_b.size_pt, &outer_b.text);
        assert!(outer_b.x_pt >= MARGIN_PT - 0.5);
        assert!(outer_b.x_pt + outer_w <= MARGIN_PT + LIST_MARKER_GUTTER_PT + 0.5);
        let inner_w = text_width(inner_b.font, inner_b.size_pt, &inner_b.text);
        assert!(inner_b.x_pt >= MARGIN_PT + LIST_MARKER_GUTTER_PT - 0.5);
        assert!(
            inner_b.x_pt + inner_w <= MARGIN_PT + 2.0 * LIST_MARKER_GUTTER_PT + 0.5,
            "inner marker right edge {} should sit within inner gutter",
            inner_b.x_pt + inner_w
        );
        // Inner text is at MARGIN_PT + 2 * gutter.
        let inner = runs.iter().find(|r| r.text == "inner").unwrap();
        let expected = MARGIN_PT + 2.0 * LIST_MARKER_GUTTER_PT;
        assert!(
            (inner.x_pt - expected).abs() < 0.5,
            "inner text x = {}, expected {expected}",
            inner.x_pt
        );
    }

    #[test]
    fn long_ordered_list_widens_gutter_so_markers_dont_overlap_text() {
        // 100 items → widest marker is `100.` (~3 digits + dot), much
        // wider than the 18pt fixed gutter at 11pt body. The list
        // should widen its gutter so the marker right edge never
        // crosses the text left edge for any item.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, true);
        for i in 0..100 {
            alloc_list_item(&mut doc, list, &format!("item{i}"));
        }
        let result = LayoutEngine::new().layout(&doc);
        let all_runs: Vec<&TextRun> = result
            .graph
            .pages
            .iter()
            .flat_map(|p| p.runs.iter())
            .collect();
        let marker_100 = all_runs
            .iter()
            .find(|r| r.text == "100.")
            .expect("`100.` marker emitted");
        let text_99 = all_runs
            .iter()
            .find(|r| r.text == "item99")
            .expect("`item99` text emitted");
        let marker_right =
            marker_100.x_pt + text_width(marker_100.font, marker_100.size_pt, &marker_100.text);
        assert!(
            marker_right <= text_99.x_pt + 0.01,
            "marker `100.` ends at {marker_right} but text starts at {} — they overlap",
            text_99.x_pt
        );
    }

    #[test]
    fn marker_only_item_still_emits_marker() {
        // An item with no inline children must still render its
        // marker. Without the marker-only flush path the bullet would
        // be silently dropped — a regression caught by CodeRabbit.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, false);
        // First item is empty (no inline children); second is normal.
        doc.alloc_child(
            list,
            Node {
                id: NodeId::default(),
                kind: NodeKind::ListItem,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: AttrMap::new(),
            },
        );
        alloc_list_item(&mut doc, list, "second");
        let result = LayoutEngine::new().layout(&doc);
        let bullets: Vec<&TextRun> = result.graph.pages[0]
            .runs
            .iter()
            .filter(|r| r.text == "\u{2022}")
            .collect();
        assert_eq!(bullets.len(), 2, "expected one bullet per item");
        // Empty-item bullet sits above the second-item bullet.
        assert!(bullets[0].baseline_from_top_pt < bullets[1].baseline_from_top_pt);
    }

    #[test]
    fn item_with_only_nested_child_keeps_its_marker() {
        // An outer item whose entire body is a nested list (no inline
        // text of its own) must still render its own marker so the
        // hierarchy is unambiguous in the output.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let outer = alloc_list(&mut doc, root, false);
        let outer_item = doc.alloc_child(
            outer,
            Node {
                id: NodeId::default(),
                kind: NodeKind::ListItem,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: AttrMap::new(),
            },
        );
        let inner = alloc_list(&mut doc, outer_item, false);
        alloc_list_item(&mut doc, inner, "deep");
        let result = LayoutEngine::new().layout(&doc);
        let bullets: Vec<&TextRun> = result.graph.pages[0]
            .runs
            .iter()
            .filter(|r| r.text == "\u{2022}")
            .collect();
        assert_eq!(
            bullets.len(),
            2,
            "expected outer + inner bullets, got {bullets:?}"
        );
        // Inner bullet is deeper (further right) and lower (further down)
        // than the outer bullet.
        assert!(bullets[1].x_pt > bullets[0].x_pt);
        assert!(bullets[1].baseline_from_top_pt > bullets[0].baseline_from_top_pt);
    }

    #[test]
    fn list_marker_baseline_matches_first_line_of_item() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, false);
        alloc_list_item(&mut doc, list, "one");
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        let bullet = runs.iter().find(|r| r.text == "\u{2022}").unwrap();
        let one = runs.iter().find(|r| r.text == "one").unwrap();
        assert!(
            (bullet.baseline_from_top_pt - one.baseline_from_top_pt).abs() < 1e-3,
            "bullet baseline {} vs text baseline {}",
            bullet.baseline_from_top_pt,
            one.baseline_from_top_pt
        );
    }

    #[test]
    fn list_text_wraps_within_indented_column() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, false);
        // Force wrap by stuffing many words into one item.
        let long: String = (0..40).map(|i| format!("word{i} ")).collect();
        alloc_list_item(&mut doc, list, long.trim());
        let result = LayoutEngine::new().layout(&doc);
        let runs = &result.graph.pages[0].runs;
        let text_left = MARGIN_PT + LIST_MARKER_GUTTER_PT;
        let text_right = A4_WIDTH_PT - MARGIN_PT;
        // Every non-marker run sits inside the indented text column.
        for run in runs.iter().filter(|r| r.text != "\u{2022}") {
            assert!(
                run.x_pt >= text_left - 0.5,
                "run `{}` at x={} is left of indented column {}",
                run.text,
                run.x_pt,
                text_left
            );
            let end = run.x_pt + text_width(run.font, run.size_pt, &run.text);
            assert!(
                end <= text_right + 1e-3,
                "run `{}` ends at {} past right edge {}",
                run.text,
                end,
                text_right
            );
        }
    }
}
