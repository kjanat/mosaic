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

mod bibliography;
mod image;
mod image_lower;
mod inline;
mod list;
mod pageref;
mod resolve;
mod set;
mod set_schema;

use std::collections::BTreeMap;

use mos_core::{
    AttrMap, AttrValue, CollectingSink, Diagnostic, Document, Node, NodeId, NodeKind, Severity,
    SourceSpan, StyleId,
};
use mos_parse::{DirectiveKind, Item, RawBlockKind, SyntaxTree};

pub use pageref::{PageFixpointOutcome, resolve_page_reference_fixpoint, resolve_page_references};
pub use resolve::resolve;

use bibliography::{lower_bibliography_directive, resolve_citations};
use image_lower::{lower_figure_directive, lower_image_directive};
use inline::lower_inlines;
use list::lower_list;
use set::lower_set_directive;

const LABEL_SPAN_START_ATTR: &str = "label_span.start";
const LABEL_SPAN_END_ATTR: &str = "label_span.end";

fn insert_label_attributes(attributes: &mut AttrMap, label: &str, label_span: Option<&SourceSpan>) {
    attributes.insert("label".to_owned(), AttrValue::Str(label.to_owned()));
    let Some(span) = label_span else {
        return;
    };
    let (Ok(start), Ok(end)) = (i64::try_from(span.start), i64::try_from(span.end)) else {
        // AttrValue::Int is i64 while SourceSpan offsets are usize. If a future
        // source can exceed that range, omit the fix-it span instead of storing
        // a lossy edit location; the resolver will skip the unsafe suggestion.
        return;
    };
    attributes.insert(LABEL_SPAN_START_ATTR.to_owned(), AttrValue::Int(start));
    attributes.insert(LABEL_SPAN_END_ATTR.to_owned(), AttrValue::Int(end));
}

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
/// use mos_core::CollectingSink;
/// use mos_eval::{Evaluator, LowerResult};
///
/// let mut sink = CollectingSink::new();
/// let parse_result = mos_parse::parse("= Hello\n", Path::new("main.mos"), &mut sink);
/// assert!(
///     parse_result.is_ok(),
///     "parse structurally aborted: {parse_result:?}"
/// );
/// if let Ok(tree) = parse_result {
///     let result: LowerResult = Evaluator::new().evaluate(&tree);
///
///     assert!(!result.has_errors());
/// }
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
            .any(|d| d.severity() == Severity::Error)
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
    /// use mos_core::CollectingSink;
    /// use mos_eval::Evaluator;
    ///
    /// let mut sink = CollectingSink::new();
    /// let parse_result = mos_parse::parse("= Hello\n", Path::new("main.mos"), &mut sink);
    /// assert!(
    ///     parse_result.is_ok(),
    ///     "parse structurally aborted: {parse_result:?}"
    /// );
    /// if let Ok(tree) = parse_result {
    ///     let result = Evaluator::new().evaluate(&tree);
    ///
    ///     assert_eq!(result.document.len(), 3);
    /// }
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
                    label_span,
                    span,
                } => {
                    let mut attributes: AttrMap = BTreeMap::new();
                    attributes.insert("level".to_owned(), AttrValue::Int(i64::from(*level)));
                    if let Some(id) = label {
                        insert_label_attributes(&mut attributes, id, label_span.as_ref());
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
                    label_span,
                    span,
                } => {
                    let mut attributes: AttrMap = BTreeMap::new();
                    if let Some(id) = label {
                        insert_label_attributes(&mut attributes, id, label_span.as_ref());
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
                    label_span,
                    span,
                    ..
                } => {
                    lower_raw_block(
                        &mut document,
                        root,
                        *kind,
                        text,
                        label.as_deref(),
                        label_span.as_ref(),
                        span,
                    );
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
                    // loader and incorrectly raise MOS0037 "missing path".
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
                    DirectiveKind::Bibliography => {
                        lower_bibliography_directive(
                            &mut document,
                            root,
                            args,
                            span,
                            &tree.file,
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
    label_span: Option<&SourceSpan>,
    span: &SourceSpan,
) {
    let mut attributes: AttrMap = BTreeMap::new();
    attributes.insert("text".to_owned(), AttrValue::Str(text.to_owned()));
    if let Some(id) = label {
        insert_label_attributes(&mut attributes, id, label_span);
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
    let mut sink = CollectingSink::new();
    let tree = match mos_parse::parse(src, file, &mut sink) {
        Ok(tree) => tree,
        // `CollectingSink` never asks the parser to abort; this arm is
        // unreachable in practice but keeps the pipeline total.
        Err(mos_core::DiagnosticAbort) => {
            return LowerResult {
                document: Document::new(file.to_path_buf()),
                diagnostics: sink.into_diagnostics(),
                metadata: DocumentMetadata::default(),
            };
        }
    };
    let mut diagnostics = sink.into_diagnostics();
    let mut lowered = lower_tree(&tree);
    diagnostics.append(&mut lowered.diagnostics);
    LowerResult {
        document: lowered.document,
        diagnostics,
        metadata: lowered.metadata,
    }
}

/// Lower an already-parsed [`SyntaxTree`]: evaluate it, then run the
/// §6 stage-3 resolver. The CLI calls this *after* `mos_parse::parse`
/// so a phase barrier can sit between parsing and lowering; [`lower`]
/// is the parse-and-lower convenience used by tests and embedders.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// let mut sink = mos_core::CollectingSink::new();
/// let tree = mos_parse::parse(
///     "= Intro <intro>\n\nSee @intro\n",
///     Path::new("main.mos"),
///     &mut sink,
/// )?;
/// let lowered = mos_eval::lower_tree(&tree);
///
/// assert!(!lowered.has_errors());
/// # Ok::<(), mos_core::DiagnosticAbort>(())
/// ```
#[must_use]
pub fn lower_tree(tree: &SyntaxTree) -> LowerResult {
    let mut lowered = Evaluator::new().evaluate(tree);
    let mut diagnostics = std::mem::take(&mut lowered.diagnostics);
    resolve_citations(&mut lowered.document, &mut diagnostics);
    diagnostics.extend(resolve(&mut lowered.document));
    LowerResult {
        document: lowered.document,
        diagnostics,
        metadata: lowered.metadata,
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

    use mos_core::{NodeKind, codes};

    use super::*;

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn label_attributes_omit_unrepresentable_span_bounds() {
        let too_large = usize::try_from(i64::MAX).unwrap().saturating_add(1);
        let span = SourceSpan::new(
            PathBuf::from("test.mos"),
            too_large,
            too_large.saturating_add(1),
        );
        let mut attributes = AttrMap::new();

        insert_label_attributes(&mut attributes, "huge", Some(&span));

        assert_eq!(
            attributes.get("label"),
            Some(&AttrValue::Str("huge".to_owned()))
        );
        assert!(!attributes.contains_key(LABEL_SPAN_START_ATTR));
        assert!(!attributes.contains_key(LABEL_SPAN_END_ATTR));
    }

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
    fn lowers_nested_bold_italic_inline() {
        let r = lower("***both***\n", &PathBuf::from("test.mos"));
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert!(
            r.document.nodes().any(|n| n.kind == NodeKind::BoldItalic),
            "expected bold-italic node in {:?}",
            r.document.nodes().map(|n| n.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn root_owns_top_level_items() {
        let r = lower("= A\n\n= B\n\npara\n", &PathBuf::from("test.mos"));
        let root = r.document.get(r.document.root).unwrap();
        assert_eq!(root.children.len(), 3);
    }

    #[test]
    fn hard_break_lowers_to_hardbreak_node_without_text_attr() {
        let r = lower("foo\\\\bar\n", &PathBuf::from("test.mos"));
        assert!(!r.has_errors(), "{:?}", r.diagnostics);

        // The paragraph is the second top-level node (after the
        // document root). Find its children.
        let root = r.document.get(r.document.root).unwrap();
        let paragraph_id = *root.children.first().unwrap();
        let paragraph = r.document.get(paragraph_id).unwrap();
        let inline_kinds: Vec<NodeKind> = paragraph
            .children
            .iter()
            .filter_map(|id| r.document.get(*id).map(|n| n.kind))
            .collect();
        assert_eq!(
            inline_kinds,
            vec![NodeKind::Text, NodeKind::HardBreak, NodeKind::Text],
            "got {inline_kinds:?}"
        );

        // The HardBreak node must have no `text` attribute -- layout
        // dispatch matches on kind, not on text presence.
        let hardbreak_id = paragraph.children[1];
        let hardbreak = r.document.get(hardbreak_id).unwrap();
        assert!(
            hardbreak.attributes.is_empty(),
            "expected empty attribute map on HardBreak, got {:?}",
            hardbreak.attributes
        );
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
    fn missing_image_path_emits_mos0037() {
        let r = lower("#image()\n", &PathBuf::from("/tmp/no-such.mos"));
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0037.code()),
            "expected MOS0037, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn unreadable_image_emits_mos0012() {
        let r = lower(
            "#image(\"does-not-exist.png\")\n",
            &PathBuf::from("/tmp/no-such-dir/main.mos"),
        );
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0012.code()),
            "expected MOS0012, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn empty_image_path_emits_mos0037_not_io_error() {
        // `#image("")` is a missing-path mistake, not an I/O failure.
        // The diagnostic surface treats it the same as omitting the
        // path entirely so the user sees a clear "needs a path"
        // message instead of `MOS0012`/`MOS0029` noise.
        let r = lower("#image(\"\")\n", &PathBuf::from("/tmp/whatever/main.mos"));
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0037.code()),
            "expected MOS0037, got {:?}",
            r.diagnostics
        );
        // No MOS0012/MOS0029 should leak through.
        assert!(
            !r.diagnostics.iter().any(|d| {
                d.def().code() == codes::MOS0012.code() || d.def().code() == codes::MOS0029.code()
            }),
            "unexpected I/O diagnostic: {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn non_positive_image_width_emits_mos0020() {
        // `width: 0pt` and `width: -10pt` would otherwise produce a
        // zero/negative image box that sails into layout and PDF
        // emit. Reject at lower time with MOS0020.
        for src in [
            "#image(\"x.png\", width: 0pt)\n",
            "#image(\"x.png\", width: -10pt)\n",
            "#image(\"x.png\", width: 0)\n",
            "#image(\"x.png\", width: -1)\n",
        ] {
            let r = lower(src, &PathBuf::from("/tmp/whatever/main.mos"));
            assert!(
                r.diagnostics
                    .iter()
                    .any(|d| d.def().code() == codes::MOS0020.code()),
                "expected MOS0020 for `{src}`, got {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn non_positive_image_height_emits_mos0020() {
        for src in [
            "#image(\"x.png\", height: 0pt)\n",
            "#image(\"x.png\", height: -1mm)\n",
        ] {
            let r = lower(src, &PathBuf::from("/tmp/whatever/main.mos"));
            assert!(
                r.diagnostics
                    .iter()
                    .any(|d| d.def().code() == codes::MOS0020.code()),
                "expected MOS0020 for `{src}`, got {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn undecodable_image_emits_mos0029() {
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
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0029.code()),
            "expected MOS0029, got {:?}",
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
        // `lower` runs the resolver, which numbers the figure and stamps
        // the non-breaking `Figure 1: ` supplement label onto the caption.
        let caption_text = r.document.get(caption.children[0]).unwrap();
        assert_eq!(
            caption_text.attributes.get("text"),
            Some(&AttrValue::Str(
                "Figure\u{00A0}1: A tiny picture.".to_owned()
            ))
        );
        std::fs::remove_dir_all(png_path.parent().unwrap()).ok();
    }

    #[test]
    fn figure_with_missing_image_does_not_leak_empty_node() {
        // If `#figure(image: "broken.png", caption: "...")` fails to
        // load the image, the caller still emits MOS0012; the lowerer
        // must NOT leave a Figure (or its caption paragraph) hanging
        // on the document root. A caption-only figure renders next
        // to whatever the user thought they were captioning, which
        // is worse than no output for the failed block.
        let r = lower(
            "#figure(image: \"does-not-exist.png\", caption: \"missing\")\n",
            &PathBuf::from("/tmp/no-such-dir/main.mos"),
        );
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0012.code())
        );
        assert!(
            !r.document.nodes().any(|n| n.kind == NodeKind::Figure),
            "Figure node leaked after image load failure",
        );
    }

    #[test]
    fn figure_label_reference_resolves_to_figure_number() {
        // End-to-end: a real `#figure(label: ...)` lowers with its label
        // on the Figure node, the resolver numbers the figure, and an
        // `@label` reference rewrites to kind-aware `Figure 1` text. Note
        // the space before `here.` — a `.` flush against the reference
        // would be absorbed into the label (`fig:plot.`) and miss.
        let png_path = write_tiny_png("ref-fig.png");
        let dir = png_path.parent().unwrap();
        let source = dir.join("main.mos");
        std::fs::write(
            &source,
            "#figure(image: \"ref-fig.png\", caption: \"A plot.\", label: \"fig:plot\")\n\nSee @fig:plot here.\n",
        )
        .unwrap();
        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);

        let figure = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Figure)
            .expect("Figure node");
        assert_eq!(
            figure.attributes.get("number"),
            Some(&AttrValue::Str("1".to_owned())),
            "the lowered figure is numbered in document order"
        );
        assert_eq!(
            figure.attributes.get("label"),
            Some(&AttrValue::Str("fig:plot".to_owned())),
            "the `label:` argument lands on the Figure node"
        );

        // The caption text is stamped with the visible, non-breaking label.
        let caption_text = figure
            .children
            .iter()
            .filter_map(|c| r.document.get(*c))
            .find(|c| {
                matches!(c.attributes.get("role"), Some(AttrValue::Str(role)) if role == "caption")
            })
            .and_then(|caption| caption.children.first())
            .and_then(|text_id| r.document.get(*text_id))
            .and_then(|text| text.attributes.get("text"));
        assert_eq!(
            caption_text,
            Some(&AttrValue::Str("Figure\u{00A0}1: A plot.".to_owned())),
            "the caption is prefixed with the `Figure N: ` label"
        );

        let reference = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Reference)
            .expect("Reference node");
        assert_eq!(
            reference.attributes.get("text"),
            Some(&AttrValue::Str("Figure\u{00A0}1".to_owned())),
            "the `@fig:plot` reference resolves to kind-aware figure text"
        );

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn page_reference_lowers_to_inert_page_reference_node() {
        // `@page(label)` reaches the semantic model as a distinct
        // `NodeKind::PageReference` carrying the bare label and a `?label?`
        // placeholder (the unresolved-reference pattern). This slice models the
        // node but does not resolve it — page resolution is the resolve↔layout
        // fixpoint (issue #72) — so it must NOT be folded into the cross-
        // reference machinery and the placeholder must survive lowering. The
        // label is declared so the lower-time validation stays quiet here.
        let r = lower(
            "= Intro <intro>\n\nSee @page(intro) here.\n",
            &PathBuf::from("test.mos"),
        );
        assert!(!r.has_errors(), "{:?}", r.diagnostics);

        let page_ref = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::PageReference)
            .expect("PageReference node");
        assert_eq!(
            page_ref.attributes.get("label"),
            Some(&AttrValue::Str("intro".to_owned())),
        );
        assert_eq!(
            page_ref.attributes.get("text"),
            Some(&AttrValue::Str("?intro?".to_owned())),
            "unresolved page references keep a visible placeholder",
        );
        // A page reference is its own kind, not an `@label` cross-reference.
        assert!(!r.document.nodes().any(|n| n.kind == NodeKind::Reference));
    }

    #[test]
    fn undeclared_page_reference_label_emits_mos0033() {
        // An undeclared label in `@page(...)` is a lower-time error, exactly
        // like a bad `@ref` — `mos check` reports it without laying out.
        let r = lower("See @page(missing) here.\n", &PathBuf::from("test.mos"));
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0033.code()),
            "{:?}",
            r.diagnostics
        );
        // The placeholder survives so the page reference stays visible.
        let page_ref = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::PageReference)
            .expect("PageReference node");
        assert_eq!(
            page_ref.attributes.get("text"),
            Some(&AttrValue::Str("?missing?".to_owned())),
        );
    }

    #[test]
    fn page_reference_to_a_declared_label_is_not_a_duplicate_declaration() {
        // A page reference *consumes* a label; it must not be mistaken for a
        // second declaration of `intro`, which would wrongly emit MOS0030.
        let r = lower(
            "= Intro <intro>\n\nSee @page(intro) here.\n",
            &PathBuf::from("test.mos"),
        );
        assert!(
            !r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0030.code()),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn citation_lowers_to_citation_node_with_key_and_span() {
        // `[@key]` must reach the semantic model as `NodeKind::Citation`
        // with the bare key in the `key` attribute and a span that
        // covers the full `[@key]` source extent. The placeholder
        // `text` attribute mirrors the unresolved-reference pattern
        // so layout still renders something visible before citation
        // display rendering exists.
        let src = "see [@smith2024] here\n";
        let r = lower(src, &PathBuf::from("test.mos"));
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0045.code()),
            "expected MOS0045 because no bibliography records are declared, got {:?}",
            r.diagnostics
        );
        let citation = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Citation)
            .expect("citation node");
        assert_eq!(
            citation.attributes.get("key"),
            Some(&AttrValue::Str("smith2024".to_owned())),
        );
        assert_eq!(
            citation.attributes.get("text"),
            Some(&AttrValue::Str("[?smith2024?]".to_owned())),
        );
        let span_text = &src[citation.span.start..citation.span.end];
        assert_eq!(span_text, "[@smith2024]");
    }

    #[test]
    fn malformed_citation_does_not_create_citation_node() {
        // `[@]` with an empty key must surface as a parse warning
        // (MOS0039) and produce zero `NodeKind::Citation` nodes — the
        // semantic model only carries citations that parsed cleanly.
        let r = lower("look [@] here\n", &PathBuf::from("test.mos"));
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0039.code()),
            "expected MOS0039, got {:?}",
            r.diagnostics,
        );
        assert!(
            !r.document.nodes().any(|n| n.kind == NodeKind::Citation),
            "no Citation nodes expected, got {:?}",
            r.document.nodes().map(|n| n.kind).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn unterminated_citation_does_not_leak_into_reference_resolver() {
        // Regression: an unterminated `[@key` used to advance just past
        // `[`, leaving `@key` to be re-tokenized by the `@`-reference
        // branch. The resolver then surfaced a bogus `MOS0033 unknown
        // label` on what was a citation typo, not a label typo.
        // Recovery in the parser now consumes the malformed citation
        // extent end-to-end so no phantom `Reference` reaches the
        // resolver.
        let r = lower(
            "see [@smith2024 missing close\n",
            &PathBuf::from("test.mos"),
        );
        assert!(
            !r.has_errors(),
            "no errors expected, got {:?}",
            r.diagnostics,
        );
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0039.code()),
            "expected MOS0039, got {:?}",
            r.diagnostics,
        );
        assert!(
            !r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0033.code()),
            "malformed citation must not surface as unknown-label MOS0033: {:?}",
            r.diagnostics,
        );
        assert!(!r.document.nodes().any(|n| n.kind == NodeKind::Citation));
        assert!(!r.document.nodes().any(|n| n.kind == NodeKind::Reference));
    }

    #[test]
    fn deferred_multi_key_citation_does_not_leak_into_reference_resolver() {
        // `[@a; @b]` is the pandoc multi-key form and is deferred to
        // a later bibliography slice. Until then it must round-trip
        // as a single `MOS0039` warning with zero `Citation`/`Reference`
        // nodes and zero `MOS0033` follow-on errors from the resolver.
        let r = lower(
            "compare [@smith2024; @jones2025] now\n",
            &PathBuf::from("test.mos"),
        );
        assert!(
            !r.has_errors(),
            "no errors expected, got {:?}",
            r.diagnostics,
        );
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0039.code())
        );
        assert!(
            !r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0033.code()),
            "multi-key citation must not surface as unknown-label MOS0033: {:?}",
            r.diagnostics,
        );
        assert!(!r.document.nodes().any(|n| n.kind == NodeKind::Citation));
        assert!(!r.document.nodes().any(|n| n.kind == NodeKind::Reference));
    }

    #[test]
    fn figure_directive_accepts_positional_path() {
        // `#figure("path.png")` is the captionless short form. The
        // parser accepts it; the lowerer used to reject it with MOS0024,
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

    /// Create a unique temp dir for a bibliography test. Salted with the
    /// caller's `name` plus a high-resolution timestamp so parallel tests
    /// don't collide, mirroring `write_tiny_png`.
    fn unique_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mos-eval-bib-{}-{}",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn bibliography_directive_preserves_resolved_path() {
        // A declared `#bibliography("refs.bib")` lowers to a Bibliography
        // node that preserves both the literal `src` and the path resolved
        // against the source file's directory, so the later BibTeX reader
        // can open the database. With the file present there is no warning.
        let dir = unique_temp_dir("preserve");
        let bib = dir.join("refs.bib");
        std::fs::write(&bib, "@book{a, title={A}}\n").unwrap();
        let source = dir.join("main.mos");
        std::fs::write(&source, "#bibliography(\"refs.bib\")\n").unwrap();
        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let node = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Bibliography)
            .expect("Bibliography node");
        assert_eq!(
            node.attributes.get("src"),
            Some(&AttrValue::Str("refs.bib".to_owned()))
        );
        assert_eq!(
            node.attributes.get("resolved_path"),
            Some(&AttrValue::Str(bib.to_string_lossy().into_owned()))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bibliography_named_path_resolves_against_source_dir() {
        // The named `path:` form resolves a subdirectory-relative path the
        // same way, exercising project-relative resolution explicitly.
        let dir = unique_temp_dir("named");
        let sub = dir.join("sources");
        std::fs::create_dir_all(&sub).unwrap();
        let bib = sub.join("refs.bib");
        std::fs::write(&bib, "@book{a, title={A}}\n").unwrap();
        let source = dir.join("main.mos");
        std::fs::write(&source, "#bibliography(path: \"sources/refs.bib\")\n").unwrap();
        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let node = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Bibliography)
            .expect("Bibliography node");
        assert_eq!(
            node.attributes.get("resolved_path"),
            Some(&AttrValue::Str(bib.to_string_lossy().into_owned()))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bibliography_src_alias_resolves_against_source_dir() {
        // The `src:` alias is accepted for parity with image source naming,
        // and preserves the literal source path for the later BibTeX reader.
        let dir = unique_temp_dir("src-alias");
        let sub = dir.join("sources");
        std::fs::create_dir_all(&sub).unwrap();
        let bib = sub.join("refs.bib");
        std::fs::write(&bib, "@book{a, title={A}}\n").unwrap();
        let source = dir.join("main.mos");
        std::fs::write(&source, "#bibliography(src: \"sources/refs.bib\")\n").unwrap();
        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let node = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Bibliography)
            .expect("Bibliography node");
        assert_eq!(
            node.attributes.get("src"),
            Some(&AttrValue::Str("sources/refs.bib".to_owned()))
        );
        assert_eq!(
            node.attributes.get("resolved_path"),
            Some(&AttrValue::Str(bib.to_string_lossy().into_owned()))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn known_citation_key_resolves_against_bibliography_records() {
        // A citation key declared in the parsed BibTeX source is marked
        // resolved and its visible text is rewritten to its first-use
        // numeric label `[1]` (issue #67).
        let dir = unique_temp_dir("citation-known");
        let bib = dir.join("refs.bib");
        std::fs::write(&bib, "@article{smith2024, title={Known}}\n").unwrap();
        let source = dir.join("main.mos");
        let source_text =
            "#bibliography(\"refs.bib\")\n\n= Intro <intro>\n\nsee [@smith2024] and @intro\n";
        std::fs::write(&source, source_text).unwrap();

        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);

        let citation = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Citation)
            .expect("Citation node");
        assert_eq!(
            citation.attributes.get("resolved"),
            Some(&AttrValue::Bool(true)),
            "known key should be marked resolved for later rendering"
        );
        assert_eq!(
            citation.attributes.get("text"),
            Some(&AttrValue::Str("[1]".to_owned())),
            "a resolved citation renders its first-use numeric label"
        );

        let reference = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Reference)
            .expect("Reference node");
        assert_eq!(
            reference.attributes.get("text"),
            Some(&AttrValue::Str("1".to_owned())),
            "label references still resolve while citations are checked"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn repeated_known_citation_key_reuses_its_first_number() {
        // Two citations to the same resolved key render the same numeric
        // label -- a key is numbered once, on first use.
        let dir = unique_temp_dir("citation-repeat");
        let bib = dir.join("refs.bib");
        std::fs::write(&bib, "@article{smith2024, title={Known}}\n").unwrap();
        let source = dir.join("main.mos");
        let source_text =
            "#bibliography(\"refs.bib\")\n\nsee [@smith2024] and again [@smith2024]\n";
        std::fs::write(&source, source_text).unwrap();

        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);

        let labels: Vec<Option<AttrValue>> = r
            .document
            .nodes()
            .filter(|n| n.kind == NodeKind::Citation)
            .map(|n| n.attributes.get("text").cloned())
            .collect();
        assert_eq!(
            labels,
            vec![
                Some(AttrValue::Str("[1]".to_owned())),
                Some(AttrValue::Str("[1]".to_owned())),
            ],
            "repeated key reuses its first-use number"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn distinct_known_citation_keys_number_by_first_use_order() {
        // Distinct resolved keys are numbered by the order they are first
        // cited, independent of their order in the BibTeX source, and a
        // later repeat of an earlier key keeps that key's number.
        let dir = unique_temp_dir("citation-order");
        let bib = dir.join("refs.bib");
        // `alpha` precedes `beta` in the database file...
        std::fs::write(
            &bib,
            "@article{alpha, title={A}}\n@article{beta, title={B}}\n",
        )
        .unwrap();
        let source = dir.join("main.mos");
        // ...but `beta` is *cited* first, so beta -> [1] and alpha -> [2].
        let source_text = "#bibliography(\"refs.bib\")\n\nsee [@beta] then [@alpha] and [@beta]\n";
        std::fs::write(&source, source_text).unwrap();

        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);

        let labels: Vec<Option<AttrValue>> = r
            .document
            .nodes()
            .filter(|n| n.kind == NodeKind::Citation)
            .map(|n| n.attributes.get("text").cloned())
            .collect();
        assert_eq!(
            labels,
            vec![
                Some(AttrValue::Str("[1]".to_owned())),
                Some(AttrValue::Str("[2]".to_owned())),
                Some(AttrValue::Str("[1]".to_owned())),
            ],
            "numbering follows first citation, not bibliography source order"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_citation_key_emits_mos0045_with_source_span() {
        let dir = unique_temp_dir("citation-unknown");
        let bib = dir.join("refs.bib");
        std::fs::write(&bib, "@article{known, title={Known}}\n").unwrap();
        let source = dir.join("main.mos");
        let source_text = "#bibliography(\"refs.bib\")\n\nsee [@missing]\n";
        std::fs::write(&source, source_text).unwrap();

        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        let missing: Vec<&Diagnostic> = r
            .diagnostics
            .iter()
            .filter(|d| d.def().code() == codes::MOS0045.code())
            .collect();
        assert_eq!(
            missing.len(),
            1,
            "expected one MOS0045, got {:?}",
            r.diagnostics
        );
        let diagnostic = missing[0];
        assert!(
            diagnostic.message().contains("`missing`"),
            "diagnostic should name missing citation key, got {:?}",
            diagnostic.message()
        );
        assert_eq!(
            diagnostic
                .span()
                .map(|span| &source_text[span.start..span.end]),
            Some("[@missing]"),
            "MOS0045 should point at the citation token"
        );

        let citation = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Citation)
            .expect("Citation node");
        assert_eq!(
            citation.attributes.get("text"),
            Some(&AttrValue::Str("[?missing?]".to_owned())),
            "unknown citations keep visible placeholder text"
        );
        assert_eq!(citation.attributes.get("resolved"), None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn multiple_unknown_citations_emit_deterministic_mos0045_diagnostics() {
        let dir = unique_temp_dir("citation-multiple-unknown");
        let bib = dir.join("refs.bib");
        std::fs::write(&bib, "@article{known, title={Known}}\n").unwrap();
        let source = dir.join("main.mos");
        let source_text = "#bibliography(\"refs.bib\")\n\nsee [@alpha] and [@beta]\n";
        std::fs::write(&source, source_text).unwrap();

        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        let spans: Vec<&str> = r
            .diagnostics
            .iter()
            .filter(|d| d.def().code() == codes::MOS0045.code())
            .filter_map(|d| d.span().map(|span| &source_text[span.start..span.end]))
            .collect();
        assert_eq!(
            spans,
            vec!["[@alpha]", "[@beta]"],
            "unknown citation diagnostics should follow document order"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn incomplete_bibliography_sources_do_not_emit_false_missing_citations() {
        let dir = unique_temp_dir("citation-incomplete-bibliography");
        let bib = dir.join("refs.bib");
        std::fs::write(&bib, "@article{known, title={Known}}\n").unwrap();
        let source = dir.join("main.mos");
        let source_text = "#bibliography(\"refs.bib\")\n#bibliography(\"missing.bib\")\n\nsee [@known] and [@maybe-in-missing]\n";
        std::fs::write(&source, source_text).unwrap();

        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0041.code()),
            "expected missing bibliography source warning, got {:?}",
            r.diagnostics
        );
        assert!(
            !r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0045.code()),
            "incomplete bibliography set must not produce false MOS0045 diagnostics"
        );

        let known = r
            .document
            .nodes()
            .filter(|n| n.kind == NodeKind::Citation)
            .find(|n| n.attributes.get("key") == Some(&AttrValue::Str("known".to_owned())))
            .expect("known citation node");
        assert_eq!(
            known.attributes.get("resolved"),
            Some(&AttrValue::Bool(true))
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn duplicate_citation_keys_across_bibliography_sources_emit_mos0046() {
        let dir = unique_temp_dir("citation-duplicate-key");
        let first = dir.join("first.bib");
        let second = dir.join("second.bib");
        std::fs::write(&first, "@article{dup, title={First}}\n").unwrap();
        std::fs::write(&second, "@book{dup, title={Second}}\n").unwrap();
        let source = dir.join("main.mos");
        let source_text =
            "#bibliography(\"first.bib\")\n#bibliography(\"second.bib\")\n\nsee [@dup]\n";
        std::fs::write(&source, source_text).unwrap();

        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        let duplicates: Vec<&Diagnostic> = r
            .diagnostics
            .iter()
            .filter(|d| d.def().code() == codes::MOS0046.code())
            .collect();
        assert_eq!(
            duplicates.len(),
            1,
            "expected one MOS0046, got {:?}",
            r.diagnostics
        );
        let diagnostic = duplicates[0];
        assert!(
            diagnostic.message().contains("`dup`"),
            "diagnostic should name duplicate citation key, got {:?}",
            diagnostic.message()
        );
        assert_eq!(
            diagnostic
                .span()
                .map(|span| &source_text[span.start..span.end]),
            Some("#bibliography(\"second.bib\")"),
            "duplicate should point at the later bibliography source"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_bibliography_path_emits_mos0040() {
        // `#bibliography()` with no path is the same authoring mistake as
        // `#image()`: a hard error, and no node leaks into the document.
        let r = lower("#bibliography()\n", &PathBuf::from("/tmp/no-such.mos"));
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0040.code()),
            "expected MOS0040, got {:?}",
            r.diagnostics
        );
        assert!(!r.document.nodes().any(|n| n.kind == NodeKind::Bibliography));
    }

    #[test]
    fn empty_bibliography_path_emits_mos0040() {
        // `#bibliography("")` is a missing-path mistake, not an I/O failure;
        // it surfaces as MOS0040 and never reaches the filesystem check.
        let r = lower(
            "#bibliography(\"\")\n",
            &PathBuf::from("/tmp/whatever/main.mos"),
        );
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0040.code()),
            "expected MOS0040, got {:?}",
            r.diagnostics
        );
        assert!(
            !r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0041.code()),
            "empty path must not trip the filesystem warning: {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn non_string_bibliography_path_emits_type_mismatch_only() {
        // A path-shaped arg with the wrong type is not "missing"; report
        // the type mismatch once and do not also emit missing-path/I/O noise.
        let r = lower(
            "#bibliography(src: 12pt)\n",
            &PathBuf::from("/tmp/whatever/main.mos"),
        );
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0020.code()),
            "expected MOS0020, got {:?}",
            r.diagnostics
        );
        assert!(
            !r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0040.code()),
            "non-string path must not also emit MOS0040: {:?}",
            r.diagnostics
        );
        assert!(
            !r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0041.code()),
            "non-string path must not reach filesystem warning: {:?}",
            r.diagnostics
        );
        assert!(!r.document.nodes().any(|n| n.kind == NodeKind::Bibliography));
    }

    #[test]
    fn duplicate_bibliography_path_keeps_first_path() {
        // Duplicate path declarations are an authoring error, but the first
        // source still wins so later accidental args cannot silently redirect
        // the bibliography boundary.
        let dir = unique_temp_dir("duplicate-path");
        let first = dir.join("first.bib");
        let second = dir.join("second.bib");
        std::fs::write(&first, "@book{first}\n").unwrap();
        std::fs::write(&second, "@book{second}\n").unwrap();
        let source = dir.join("main.mos");
        let source_text = "#bibliography(\"first.bib\", path: \"second.bib\")\n";
        std::fs::write(&source, source_text).unwrap();
        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        let duplicate_path_diagnostics: Vec<&Diagnostic> = r
            .diagnostics
            .iter()
            .filter(|d| d.def().code() == codes::MOS0042.code())
            .collect();
        assert_eq!(
            duplicate_path_diagnostics.len(),
            1,
            "expected one MOS0042, got {:?}",
            r.diagnostics
        );
        let duplicate = duplicate_path_diagnostics[0];
        assert_eq!(
            duplicate
                .span()
                .map(|span| &source_text[span.start..span.end]),
            Some("\"second.bib\""),
            "duplicate path diagnostic should point at the later path value"
        );
        let node = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Bibliography)
            .expect("Bibliography node");
        assert_eq!(
            node.attributes.get("src"),
            Some(&AttrValue::Str("first.bib".to_owned()))
        );
        assert_eq!(
            node.attributes.get("resolved_path"),
            Some(&AttrValue::Str(first.to_string_lossy().into_owned()))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_bibliography_source_warns_mos0041_but_keeps_node() {
        // A declared-but-absent database is a non-fatal warning: the build
        // still succeeds and the node is emitted with its resolved path so
        // the later BibTeX slice can act on it.
        let dir = unique_temp_dir("absent");
        let source = dir.join("main.mos");
        std::fs::write(&source, "#bibliography(\"nope.bib\")\n").unwrap();
        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(
            !r.has_errors(),
            "a missing source is a warning, not an error: {:?}",
            r.diagnostics
        );
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0041.code()),
            "expected MOS0041, got {:?}",
            r.diagnostics
        );
        let node = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Bibliography)
            .expect("Bibliography node still emitted on a missing source");
        assert_eq!(
            node.attributes.get("resolved_path"),
            Some(&AttrValue::Str(
                dir.join("nope.bib").to_string_lossy().into_owned()
            ))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_bibliography_arg_emits_mos0015() {
        // Arguments beyond the path (e.g. a future `style:`) are rejected
        // now so the directive's surface stays narrow until later slices
        // grow it deliberately.
        let dir = unique_temp_dir("unknownarg");
        std::fs::write(dir.join("refs.bib"), "@book{a}\n").unwrap();
        let source = dir.join("main.mos");
        std::fs::write(&source, "#bibliography(\"refs.bib\", style: \"ieee\")\n").unwrap();
        let r = lower(&std::fs::read_to_string(&source).unwrap(), &source);
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0015.code()),
            "expected MOS0015, got {:?}",
            r.diagnostics
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
