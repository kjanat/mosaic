//! Expression and scripting evaluator (manifest §4, §25).
//!
//! The "evaluator" is really a *lowerer + resolver*: it walks a
//! [`SyntaxTree`] from `mos-parse` and builds the typed semantic
//! [`Document`] graph from `mos-core` (manifest §6 stage 2), then
//! runs the [`resolve`] pass to assign section numbers and rewrite
//! `@label` cross-references (§6 stage 3, MVP 1).

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

mod image;
mod image_lower;
mod inline;
mod list;
mod resolve;
mod set;
mod set_schema;

use std::collections::BTreeMap;

use mos_core::{
    AttrMap, AttrValue, Diagnostic, Document, Node, NodeId, NodeKind, Severity, SourceSpan, StyleId,
};
use mos_parse::{DirectiveKind, Item, RawBlockKind, SyntaxTree};

pub use resolve::resolve;

use image_lower::{lower_figure_directive, lower_image_directive};
use inline::lower_inlines;
use list::lower_list;
use set::lower_set_directive;

/// Document-level metadata harvested from `#set document(...)` directives.
/// The PDF backend writes `title` and `author` to the Info dictionary;
/// `language` is captured for the catalog `/Lang` entry that the next
/// PDF-metadata slice will wire up.
///
/// # Examples
///
/// ```
/// use mos_eval::DocumentMetadata;
///
/// let metadata = DocumentMetadata {
///     title: Some("Demo".to_owned()),
///     author: None,
///     language: Some("en".to_owned()),
/// };
///
/// assert_eq!(metadata.title.as_deref(), Some("Demo"));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
}

/// Result of lowering a [`SyntaxTree`] into a [`Document`].
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// use mos_eval::{Evaluator, LowerResult};
///
/// let parsed = mos_parse::parse("= Hello\n", Path::new("main.mos"));
/// let result: LowerResult = Evaluator::new().evaluate(&parsed.tree);
///
/// assert!(!result.has_errors());
/// ```
#[derive(Debug)]
pub struct LowerResult {
    pub document: Document,
    pub diagnostics: Vec<Diagnostic>,
    pub metadata: DocumentMetadata,
}

impl LowerResult {
    /// Return whether any lowering diagnostic is an error.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let result = mos_eval::lower("= Hello\n", Path::new("main.mos"));
    ///
    /// assert!(!result.has_errors());
    /// ```
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// Lowerer from parse syntax to semantic document graph.
///
/// # Examples
///
/// ```
/// use mos_eval::Evaluator;
///
/// let evaluator = Evaluator::new();
///
/// assert_eq!(format!("{evaluator:?}"), "Evaluator");
/// ```
#[derive(Default, Debug)]
pub struct Evaluator;

impl Evaluator {
    /// Construct an evaluator.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_eval::Evaluator;
    ///
    /// let evaluator = Evaluator::new();
    ///
    /// assert_eq!(format!("{evaluator:?}"), "Evaluator");
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Lower `tree` into a semantic [`Document`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// use mos_eval::Evaluator;
    ///
    /// let parsed = mos_parse::parse("= Hello\n", Path::new("main.mos"));
    /// let result = Evaluator::new().evaluate(&parsed.tree);
    ///
    /// assert_eq!(result.document.len(), 3);
    /// ```
    pub fn evaluate(&self, tree: &SyntaxTree) -> LowerResult {
        let mut document = Document::new(tree.file.clone());
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let mut metadata = DocumentMetadata::default();
        // Tracks the most-recently-set body text size in pt so `em`
        // literals on later directives resolve against the right unit.
        // Defaults to 11pt to match `mos-layout`'s `BODY_SIZE_PT`.
        let mut current_text_size_pt: f64 = 11.0;
        let root = document.root;

        for item in &tree.items {
            match item {
                Item::Heading {
                    level,
                    inlines,
                    label,
                    span,
                } => {
                    let mut attributes: AttrMap = BTreeMap::new();
                    attributes.insert("level".to_owned(), AttrValue::Int(i64::from(*level)));
                    if let Some(id) = label {
                        attributes.insert("label".to_owned(), AttrValue::Str(id.clone()));
                    }
                    let heading = document.alloc_child(
                        root,
                        Node {
                            id: NodeId::default(),
                            kind: NodeKind::Section,
                            span: span.clone(),
                            content_hash: Default::default(),
                            style_id: StyleId::default(),
                            children: Vec::new(),
                            attributes,
                        },
                    );
                    lower_inlines(&mut document, heading, inlines);
                }
                Item::Paragraph {
                    inlines,
                    label,
                    span,
                } => {
                    let mut attributes: AttrMap = BTreeMap::new();
                    if let Some(id) = label {
                        attributes.insert("label".to_owned(), AttrValue::Str(id.clone()));
                    }
                    let para = document.alloc_child(
                        root,
                        Node {
                            id: NodeId::default(),
                            kind: NodeKind::Paragraph,
                            span: span.clone(),
                            content_hash: Default::default(),
                            style_id: StyleId::default(),
                            children: Vec::new(),
                            attributes,
                        },
                    );
                    lower_inlines(&mut document, para, inlines);
                }
                Item::List {
                    ordered,
                    items,
                    span,
                } => {
                    lower_list(&mut document, root, *ordered, items, span);
                }
                Item::RawBlock {
                    kind,
                    text,
                    label,
                    span,
                    ..
                } => {
                    lower_raw_block(&mut document, root, *kind, text, label.as_deref(), span);
                }
                Item::Set {
                    kind,
                    name,
                    args,
                    span,
                } => match kind {
                    // `DirectiveKind` (set by the parser) is the
                    // discriminator here, *not* `name` — `#set image(...)`
                    // and `#image(...)` are both parsed with `name ==
                    // "image"`, and dispatching on the string would
                    // route `#set image(width: 200pt)` into the image
                    // loader and incorrectly raise E050 "missing path".
                    DirectiveKind::Image => {
                        lower_image_directive(
                            &mut document,
                            root,
                            args,
                            span,
                            &tree.file,
                            current_text_size_pt,
                            &mut diagnostics,
                        );
                    }
                    DirectiveKind::Figure => {
                        lower_figure_directive(
                            &mut document,
                            root,
                            args,
                            span,
                            &tree.file,
                            current_text_size_pt,
                            &mut diagnostics,
                        );
                    }
                    DirectiveKind::Set => lower_set_directive(
                        &mut document,
                        root,
                        name,
                        args,
                        span,
                        &mut metadata,
                        &mut current_text_size_pt,
                        &mut diagnostics,
                    ),
                },
            }
        }

        LowerResult {
            document,
            diagnostics,
            metadata,
        }
    }
}

fn lower_raw_block(
    document: &mut Document,
    root: NodeId,
    kind: RawBlockKind,
    text: &str,
    label: Option<&str>,
    span: &SourceSpan,
) {
    let mut attributes: AttrMap = BTreeMap::new();
    attributes.insert("text".to_owned(), AttrValue::Str(text.to_owned()));
    if let Some(id) = label {
        attributes.insert("label".to_owned(), AttrValue::Str(id.to_owned()));
    }
    attributes.insert(
        "raw.kind".to_owned(),
        AttrValue::Str(
            match kind {
                RawBlockKind::Pre => "pre",
                RawBlockKind::Code => "code",
            }
            .to_owned(),
        ),
    );
    document.alloc_child(
        root,
        Node {
            id: NodeId::default(),
            kind: NodeKind::Raw,
            span: span.clone(),
            content_hash: Default::default(),
            style_id: StyleId::default(),
            children: Vec::new(),
            attributes,
        },
    );
}

/// Convenience: parse + lower + resolve in one step. Concatenates the
/// diagnostics from each stage so callers can render them uniformly.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// let result = mos_eval::lower("= Hello\n", Path::new("main.mos"));
///
/// assert!(!result.has_errors());
/// assert_eq!(result.document.len(), 3);
/// ```
pub fn lower(src: &str, file: &std::path::Path) -> LowerResult {
    let parse_result = mos_parse::parse(src, file);
    let mut diagnostics = parse_result.diagnostics;
    let mut lower = Evaluator::new().evaluate(&parse_result.tree);
    diagnostics.append(&mut lower.diagnostics);
    diagnostics.extend(resolve(&mut lower.document));
    LowerResult {
        document: lower.document,
        diagnostics,
        metadata: lower.metadata,
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        reason = "tests panic loudly on setup failure; matches crate-wide test-module convention"
    )]
    use std::path::PathBuf;

    use mos_core::NodeKind;

    use super::*;

    #[test]
    fn lowers_heading_and_paragraph() {
        let r = lower(
            "= Hello\n\nbody *italic* text\n",
            &PathBuf::from("test.mos"),
        );
        assert!(!r.has_errors());
        // Document root + Section + Paragraph + 1 Text inside Section
        // + 3 inline children of Paragraph (text/emphasis/text).
        assert_eq!(r.document.len(), 1 + 2 + 1 + 3);

        let kinds: Vec<NodeKind> = r.document.nodes().map(|n| n.kind).collect();
        assert_eq!(kinds[0], NodeKind::Document);
        assert!(kinds.contains(&NodeKind::Section));
        assert!(kinds.contains(&NodeKind::Paragraph));
        assert!(kinds.contains(&NodeKind::Emphasis));
    }

    #[test]
    fn root_owns_top_level_items() {
        let r = lower("= A\n\n= B\n\npara\n", &PathBuf::from("test.mos"));
        let root = r.document.get(r.document.root).unwrap();
        assert_eq!(root.children.len(), 3);
    }

    /// Hand-craft a tiny PNG in a temp dir so the eval tests don't
    /// depend on `examples/` paths or the workspace layout.
    /// `::image::` (rather than `image::`) routes through the extern
    /// `image` crate; the bare `image` identifier inside the eval
    /// crate resolves to the local `mod image` we declared up top.
    fn write_tiny_png(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mos-eval-image-{}-{}",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let mut buf = ::image::RgbaImage::new(3, 2);
        for x in 0_u32..3 {
            for y in 0_u32..2 {
                let r = u8::try_from(x * 80).unwrap_or(0);
                let g = u8::try_from(y * 120).unwrap_or(0);
                buf.put_pixel(x, y, ::image::Rgba([r, g, 200, 255]));
            }
        }
        buf.save(&path).unwrap();
        path
    }

    #[test]
    fn image_directive_attaches_decoded_pixels() {
        let png_path = write_tiny_png("tiny.png");
        let source = png_path.parent().unwrap().join("main.mos");
        std::fs::write(&source, "#image(\"tiny.png\")\n").unwrap();
        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let image_node = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Image)
            .expect("Image node");
        assert_eq!(
            image_node.attributes.get("src"),
            Some(&AttrValue::Str("tiny.png".to_owned()))
        );
        assert_eq!(
            image_node.attributes.get("pixel_width"),
            Some(&AttrValue::Int(3))
        );
        assert_eq!(
            image_node.attributes.get("pixel_height"),
            Some(&AttrValue::Int(2))
        );
        match image_node.attributes.get("pixels") {
            Some(AttrValue::Bytes(b)) => assert_eq!(b.len(), 3 * 3 * 2),
            other => panic!("expected pixel bytes, got {other:?}"),
        }
        std::fs::remove_dir_all(png_path.parent().unwrap()).ok();
    }

    #[test]
    fn image_directive_records_explicit_dimensions() {
        let png_path = write_tiny_png("dims.png");
        let source = png_path.parent().unwrap().join("main.mos");
        std::fs::write(
            &source,
            "#image(\"dims.png\", width: 100pt, height: 60pt, alt: \"a tiny image\")\n",
        )
        .unwrap();
        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let image_node = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Image)
            .expect("Image node");
        assert_eq!(
            image_node.attributes.get("width"),
            Some(&AttrValue::Length(100.0))
        );
        assert_eq!(
            image_node.attributes.get("height"),
            Some(&AttrValue::Length(60.0))
        );
        assert_eq!(
            image_node.attributes.get("alt"),
            Some(&AttrValue::Str("a tiny image".to_owned()))
        );
        std::fs::remove_dir_all(png_path.parent().unwrap()).ok();
    }

    #[test]
    fn image_em_width_resolves_against_current_text_size() {
        // Regression: `#image(width: 2em)` after `#set text(size: 20pt)`
        // must yield 40pt, not 22pt (which is what the old hardcoded
        // 11pt em base produced). The lowerer now threads the tracked
        // body text size through to `build_image_attributes`.
        let png_path = write_tiny_png("em.png");
        let dir = png_path.parent().unwrap();
        let source = dir.join("main.mos");
        std::fs::write(
            &source,
            "#set text(size: 20pt)\n#image(\"em.png\", width: 2em)\n",
        )
        .unwrap();
        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let image_node = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Image)
            .expect("Image node");
        match image_node.attributes.get("width") {
            Some(AttrValue::Length(pt)) => assert!(
                (pt - 40.0).abs() < 0.01,
                "width = {pt}pt, expected 40pt (2em at 20pt)"
            ),
            other => panic!("expected width Length, got {other:?}"),
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_image_path_emits_e050() {
        let r = lower("#image()\n", &PathBuf::from("/tmp/no-such.mos"));
        assert!(
            r.diagnostics.iter().any(|d| d.code.0 == "E050"),
            "expected E050, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn unreadable_image_emits_e051() {
        let r = lower(
            "#image(\"does-not-exist.png\")\n",
            &PathBuf::from("/tmp/no-such-dir/main.mos"),
        );
        assert!(
            r.diagnostics.iter().any(|d| d.code.0 == "E051"),
            "expected E051, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn empty_image_path_emits_e050_not_io_error() {
        // `#image("")` is a missing-path mistake, not an I/O failure.
        // The diagnostic surface treats it the same as omitting the
        // path entirely so the user sees a clear "needs a path"
        // message instead of `E051`/`E052` noise.
        let r = lower("#image(\"\")\n", &PathBuf::from("/tmp/whatever/main.mos"));
        assert!(
            r.diagnostics.iter().any(|d| d.code.0 == "E050"),
            "expected E050, got {:?}",
            r.diagnostics
        );
        // No E051/E052 should leak through.
        assert!(
            !r.diagnostics
                .iter()
                .any(|d| matches!(d.code.0, "E051" | "E052")),
            "unexpected I/O diagnostic: {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn non_positive_image_width_emits_e022() {
        // `width: 0pt` and `width: -10pt` would otherwise produce a
        // zero/negative image box that sails into layout and PDF
        // emit. Reject at lower time with E022.
        for src in [
            "#image(\"x.png\", width: 0pt)\n",
            "#image(\"x.png\", width: -10pt)\n",
            "#image(\"x.png\", width: 0)\n",
            "#image(\"x.png\", width: -1)\n",
        ] {
            let r = lower(src, &PathBuf::from("/tmp/whatever/main.mos"));
            assert!(
                r.diagnostics.iter().any(|d| d.code.0 == "E022"),
                "expected E022 for `{src}`, got {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn non_positive_image_height_emits_e022() {
        for src in [
            "#image(\"x.png\", height: 0pt)\n",
            "#image(\"x.png\", height: -1mm)\n",
        ] {
            let r = lower(src, &PathBuf::from("/tmp/whatever/main.mos"));
            assert!(
                r.diagnostics.iter().any(|d| d.code.0 == "E022"),
                "expected E022 for `{src}`, got {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn undecodable_image_emits_e052() {
        let dir = std::env::temp_dir().join(format!(
            "mos-eval-bad-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("bad.png");
        std::fs::write(&png, b"not really a PNG").unwrap();
        let source = dir.join("main.mos");
        std::fs::write(&source, "#image(\"bad.png\")\n").unwrap();
        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(
            r.diagnostics.iter().any(|d| d.code.0 == "E052"),
            "expected E052, got {:?}",
            r.diagnostics
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn figure_directive_creates_figure_with_image_and_caption() {
        let png_path = write_tiny_png("fig.png");
        let source = png_path.parent().unwrap().join("main.mos");
        std::fs::write(
            &source,
            "#figure(image: \"fig.png\", caption: \"A tiny picture.\")\n",
        )
        .unwrap();
        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let figure = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Figure)
            .expect("Figure node");
        assert_eq!(figure.children.len(), 2);
        let img = r.document.get(figure.children[0]).unwrap();
        assert_eq!(img.kind, NodeKind::Image);
        let caption = r.document.get(figure.children[1]).unwrap();
        assert_eq!(caption.kind, NodeKind::Paragraph);
        assert_eq!(
            caption.attributes.get("role"),
            Some(&AttrValue::Str("caption".to_owned()))
        );
        let caption_text = r.document.get(caption.children[0]).unwrap();
        assert_eq!(
            caption_text.attributes.get("text"),
            Some(&AttrValue::Str("A tiny picture.".to_owned()))
        );
        std::fs::remove_dir_all(png_path.parent().unwrap()).ok();
    }

    #[test]
    fn figure_with_missing_image_does_not_leak_empty_node() {
        // If `#figure(image: "broken.png", caption: "...")` fails to
        // load the image, the caller still emits E051; the lowerer
        // must NOT leave a Figure (or its caption paragraph) hanging
        // on the document root. A caption-only figure renders next
        // to whatever the user thought they were captioning, which
        // is worse than no output for the failed block.
        let r = lower(
            "#figure(image: \"does-not-exist.png\", caption: \"missing\")\n",
            &PathBuf::from("/tmp/no-such-dir/main.mos"),
        );
        assert!(r.diagnostics.iter().any(|d| d.code.0 == "E051"));
        assert!(
            !r.document.nodes().any(|n| n.kind == NodeKind::Figure),
            "Figure node leaked after image load failure",
        );
    }

    #[test]
    fn figure_directive_accepts_positional_path() {
        // `#figure("path.png")` is the captionless short form. The
        // parser accepts it; the lowerer used to reject it with E015,
        // which broke the spelling end-to-end.
        let png_path = write_tiny_png("fig_pos.png");
        let source = png_path.parent().unwrap().join("main.mos");
        std::fs::write(&source, "#figure(\"fig_pos.png\")\n").unwrap();
        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let figure = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Figure)
            .expect("Figure node");
        // One child: just the image (no caption was supplied).
        assert_eq!(figure.children.len(), 1);
        let img = r.document.get(figure.children[0]).unwrap();
        assert_eq!(img.kind, NodeKind::Image);
        assert_eq!(
            img.attributes.get("src"),
            Some(&AttrValue::Str("fig_pos.png".to_owned()))
        );
        std::fs::remove_dir_all(png_path.parent().unwrap()).ok();
    }
}
