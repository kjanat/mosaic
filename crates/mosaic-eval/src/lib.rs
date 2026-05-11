//! Expression and scripting evaluator (manifest §4, §25).
//!
//! The "evaluator" is really a *lowerer + resolver*: it walks a
//! [`SyntaxTree`] from `mosaic-parse` and builds the typed semantic
//! [`Document`] graph from `mosaic-core` (manifest §6 stage 2), then
//! runs the [`resolve`] pass to assign section numbers and rewrite
//! `@label` cross-references (§6 stage 3, MVP 1).

mod image;
mod resolve;
mod set_schema;

use std::collections::BTreeMap;
use std::sync::Arc;

use mosaic_core::{
    AttrMap, AttrValue, Diagnostic, DiagnosticCode, Document, Node, NodeId, NodeKind, Severity,
    SourceSpan, StyleId,
};
use mosaic_parse::{Inline, InlineKind, Item, SetArg, SetValue, SyntaxTree};

pub use resolve::resolve;

/// Document-level metadata harvested from `#set document(...)` directives.
/// The PDF backend writes `title` and `author` to the Info dictionary;
/// `language` is captured for the catalog `/Lang` entry that the next
/// PDF-metadata slice will wire up.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
}

/// Result of lowering a [`SyntaxTree`] into a [`Document`].
#[derive(Debug)]
pub struct LowerResult {
    pub document: Document,
    pub diagnostics: Vec<Diagnostic>,
    pub metadata: DocumentMetadata,
}

impl LowerResult {
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

#[derive(Default, Debug)]
pub struct Evaluator;

impl Evaluator {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Lower `tree` into a semantic [`Document`].
    pub fn evaluate(&self, tree: &SyntaxTree) -> LowerResult {
        let mut document = Document::new(tree.file.clone());
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let mut metadata = DocumentMetadata::default();
        // Tracks the most-recently-set body text size in pt so `em`
        // literals on later directives resolve against the right unit.
        // Defaults to 11pt to match `mosaic-layout`'s `BODY_SIZE_PT`.
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
                Item::Set { name, args, span } => match name.as_str() {
                    "image" => {
                        lower_image_directive(
                            &mut document,
                            root,
                            args,
                            span,
                            &tree.file,
                            &mut diagnostics,
                        );
                    }
                    "figure" => {
                        lower_figure_directive(
                            &mut document,
                            root,
                            args,
                            span,
                            &tree.file,
                            &mut diagnostics,
                        );
                    }
                    _ => lower_set_directive(
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

/// Lower a `#set name(...)` directive into a `Raw` node carrying the
/// resolved attribute payload. The split exists so the dispatch in
/// [`Evaluator::evaluate`] only has to thread state through three
/// directive-shaped helpers instead of one large match arm.
#[allow(clippy::too_many_arguments)]
fn lower_set_directive(
    document: &mut Document,
    root: NodeId,
    name: &str,
    args: &[SetArg],
    span: &SourceSpan,
    metadata: &mut DocumentMetadata,
    current_text_size_pt: &mut f64,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut attributes: AttrMap = BTreeMap::new();
    attributes.insert("set".to_owned(), AttrValue::Str(name.to_owned()));
    let Some(target) = set_schema::lookup_target(name) else {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode("E020"),
                format!(
                    "unknown `#set` target `{name}` (expected `page`, `text`, `document`, or `image`)"
                ),
            )
            .with_span(span.clone()),
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
        return;
    };
    for arg in args {
        if arg.key.is_empty() {
            // The parser refuses positional args for `#set` already,
            // so reaching this arm means a future caller forgot the
            // `allow_positional=false` flag. Diagnose loudly.
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E015"),
                    format!("`#set {name}` does not accept positional arguments"),
                )
                .with_span(arg.value_span.clone()),
            );
            continue;
        }
        lower_set_arg(
            target,
            arg,
            &mut attributes,
            metadata,
            current_text_size_pt,
            diagnostics,
        );
    }
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

/// Lower a top-level `#image(...)` directive into a single
/// [`NodeKind::Image`] node hanging off the document root. The decoded
/// pixel buffer and pixel dimensions are stashed in attributes so the
/// layout engine and PDF backend don't have to re-open the source file.
fn lower_image_directive(
    document: &mut Document,
    root: NodeId,
    args: &[SetArg],
    span: &SourceSpan,
    source_file: &std::path::Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some((attributes, _label)) = build_image_attributes(args, span, source_file, diagnostics)
    else {
        return;
    };
    document.alloc_child(
        root,
        Node {
            id: NodeId::default(),
            kind: NodeKind::Image,
            span: span.clone(),
            content_hash: Default::default(),
            style_id: StyleId::default(),
            children: Vec::new(),
            attributes,
        },
    );
}

/// Lower a `#figure(image: ..., caption: ...)` directive into a
/// [`NodeKind::Figure`] node with two children: an Image node (built
/// the same way `#image(...)` would build it) and a caption paragraph.
/// The caption is rendered beneath the image by the layout engine.
fn lower_figure_directive(
    document: &mut Document,
    root: NodeId,
    args: &[SetArg],
    span: &SourceSpan,
    source_file: &std::path::Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Pluck the image-specifying args (`image:` path, optional
    // `width`/`height`/`alt`) into a synthetic SetArg list so the
    // existing builder can reuse them. Positional args (a leading
    // string) are not supported on `#figure` today — callers spell
    // `#figure(image: "x.png", caption: "...")`.
    let mut image_args: Vec<SetArg> = Vec::new();
    let mut caption: Option<(String, SourceSpan)> = None;
    let mut figure_label: Option<String> = None;
    for arg in args {
        match arg.key.as_str() {
            "image" => {
                // Rewrite the key to the synthetic positional slot
                // [`build_image_attributes`] expects.
                image_args.push(SetArg {
                    key: String::new(),
                    value: arg.value.clone(),
                    key_span: arg.key_span.clone(),
                    value_span: arg.value_span.clone(),
                });
            }
            "width" | "height" | "alt" => {
                image_args.push(arg.clone());
            }
            "caption" => match &arg.value {
                SetValue::Str(s) => {
                    caption = Some((s.clone(), arg.value_span.clone()));
                }
                _ => diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E022"),
                        "`#figure(caption: ...)` expects a string",
                    )
                    .with_span(arg.value_span.clone()),
                ),
            },
            "label" => match &arg.value {
                SetValue::Str(s) => figure_label = Some(s.clone()),
                _ => diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E022"),
                        "`#figure(label: ...)` expects a string",
                    )
                    .with_span(arg.value_span.clone()),
                ),
            },
            "" => diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E015"),
                    "`#figure(...)` does not accept positional arguments; use `image: \"path\"`",
                )
                .with_span(arg.value_span.clone()),
            ),
            _ => diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E021"),
                    format!(
                        "unknown argument `{}` for `#figure` (valid: image, caption, alt, width, height, label)",
                        arg.key
                    ),
                )
                .with_span(arg.key_span.clone()),
            ),
        }
    }

    let mut figure_attrs: AttrMap = BTreeMap::new();
    if let Some(label) = figure_label {
        figure_attrs.insert("label".to_owned(), AttrValue::Str(label));
    }
    let figure_id = document.alloc_child(
        root,
        Node {
            id: NodeId::default(),
            kind: NodeKind::Figure,
            span: span.clone(),
            content_hash: Default::default(),
            style_id: StyleId::default(),
            children: Vec::new(),
            attributes: figure_attrs,
        },
    );

    if let Some((attrs, _)) = build_image_attributes(&image_args, span, source_file, diagnostics) {
        document.alloc_child(
            figure_id,
            Node {
                id: NodeId::default(),
                kind: NodeKind::Image,
                span: span.clone(),
                content_hash: Default::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: attrs,
            },
        );
    }
    if let Some((text, caption_span)) = caption {
        let caption_id = document.alloc_child(
            figure_id,
            Node {
                id: NodeId::default(),
                kind: NodeKind::Paragraph,
                span: caption_span.clone(),
                content_hash: Default::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: {
                    let mut a = AttrMap::new();
                    // Tag the caption so the layout engine can give it
                    // distinct styling later. For now it renders as a
                    // plain paragraph beneath the image.
                    a.insert("role".to_owned(), AttrValue::Str("caption".to_owned()));
                    a
                },
            },
        );
        let mut child_attrs = AttrMap::new();
        child_attrs.insert("text".to_owned(), AttrValue::Str(text));
        document.alloc_child(
            caption_id,
            Node {
                id: NodeId::default(),
                kind: NodeKind::Text,
                span: caption_span,
                content_hash: Default::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: child_attrs,
            },
        );
    }
}

/// Walk a directive's argument list and produce the attribute map for
/// an [`NodeKind::Image`] node, including the decoded pixel buffer.
/// Returns `None` (and emits diagnostics) if the path argument is
/// missing or the bytes can't be decoded — the caller drops the node
/// in that case rather than emitting a half-built image.
fn build_image_attributes(
    args: &[SetArg],
    span: &SourceSpan,
    source_file: &std::path::Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(AttrMap, Option<String>)> {
    let mut src_path: Option<(String, SourceSpan)> = None;
    let mut alt: Option<String> = None;
    let mut declared_width: Option<f64> = None;
    let mut declared_height: Option<f64> = None;
    let mut label: Option<String> = None;
    for arg in args {
        match arg.key.as_str() {
            // Positional first arg or explicit `src:` key.
            "" | "src" | "path" => match &arg.value {
                SetValue::Str(s) => src_path = Some((s.clone(), arg.value_span.clone())),
                _ => diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E022"),
                        "`#image(...)` expects a string path",
                    )
                    .with_span(arg.value_span.clone()),
                ),
            },
            "alt" => match &arg.value {
                SetValue::Str(s) => alt = Some(s.clone()),
                _ => diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E022"),
                        "`#image(alt: ...)` expects a string",
                    )
                    .with_span(arg.value_span.clone()),
                ),
            },
            "width" => match &arg.value {
                SetValue::Length(v, unit) => {
                    declared_width = Some(length_to_pt(*v, *unit, 11.0));
                }
                SetValue::Float(v) => declared_width = Some(*v),
                SetValue::Int(v) => declared_width = Some(int_to_f64(*v)),
                _ => diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E022"),
                        "`#image(width: ...)` expects a length",
                    )
                    .with_span(arg.value_span.clone()),
                ),
            },
            "height" => match &arg.value {
                SetValue::Length(v, unit) => {
                    declared_height = Some(length_to_pt(*v, *unit, 11.0));
                }
                SetValue::Float(v) => declared_height = Some(*v),
                SetValue::Int(v) => declared_height = Some(int_to_f64(*v)),
                _ => diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E022"),
                        "`#image(height: ...)` expects a length",
                    )
                    .with_span(arg.value_span.clone()),
                ),
            },
            "label" => match &arg.value {
                SetValue::Str(s) => label = Some(s.clone()),
                _ => diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E022"),
                        "`#image(label: ...)` expects a string",
                    )
                    .with_span(arg.value_span.clone()),
                ),
            },
            _ => diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E021"),
                    format!(
                        "unknown argument `{}` for `#image` (valid: src, alt, width, height, label)",
                        arg.key
                    ),
                )
                .with_span(arg.key_span.clone()),
            ),
        }
    }
    let Some((path, _path_span)) = src_path else {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode("E050"),
                "`#image(...)` requires a path (e.g. `#image(\"scan.png\")`)",
            )
            .with_span(span.clone()),
        );
        return None;
    };
    let (resolved, decoded) = match image::load(&path, source_file, span) {
        Ok(v) => v,
        Err(diag) => {
            diagnostics.push(*diag);
            return None;
        }
    };

    let mut attrs: AttrMap = BTreeMap::new();
    attrs.insert("src".to_owned(), AttrValue::Str(path));
    attrs.insert(
        "resolved_path".to_owned(),
        AttrValue::Str(resolved.to_string_lossy().into_owned()),
    );
    if let Some(a) = alt {
        attrs.insert("alt".to_owned(), AttrValue::Str(a));
    }
    if let Some(w) = declared_width {
        attrs.insert("width".to_owned(), AttrValue::Length(w));
    }
    if let Some(h) = declared_height {
        attrs.insert("height".to_owned(), AttrValue::Length(h));
    }
    if let Some(l) = &label {
        attrs.insert("label".to_owned(), AttrValue::Str(l.clone()));
    }
    attrs.insert(
        "pixel_width".to_owned(),
        AttrValue::Int(i64::from(decoded.width)),
    );
    attrs.insert(
        "pixel_height".to_owned(),
        AttrValue::Int(i64::from(decoded.height)),
    );
    attrs.insert(
        "color_space".to_owned(),
        AttrValue::Str("DeviceRGB".to_owned()),
    );
    attrs.insert("bits_per_component".to_owned(), AttrValue::Int(8));
    attrs.insert(
        "pixels".to_owned(),
        AttrValue::Bytes(Arc::from(decoded.rgb8)),
    );
    Some((attrs, label))
}

/// Convert one parser-level `SetArg` into an attribute on the Raw node
/// representing this `#set` directive. Emits semantic diagnostics
/// (`E021` unknown key, `E022` type mismatch, `W024` sanity floor) and
/// updates `metadata` / `current_text_size_pt` as a side effect.
fn lower_set_arg(
    target: set_schema::Target,
    arg: &SetArg,
    attributes: &mut AttrMap,
    metadata: &mut DocumentMetadata,
    current_text_size_pt: &mut f64,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(slot) = target.slot(&arg.key) else {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode("E021"),
                format!(
                    "unknown argument `{}` for `#set {}` (valid: {})",
                    arg.key,
                    target.name(),
                    target.keys().join(", ")
                ),
            )
            .with_span(arg.key_span.clone()),
        );
        return;
    };
    let Some(value) = coerce_value(slot, &arg.value, *current_text_size_pt) else {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode("E022"),
                format!(
                    "`#set {} ({}: …)` expects {}, got {}",
                    target.name(),
                    arg.key,
                    slot.expected(),
                    describe_value(&arg.value),
                ),
            )
            .with_span(arg.value_span.clone()),
        );
        return;
    };
    if let Some(msg) = sanity_floor_warning(target, &arg.key, &value) {
        diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            code: DiagnosticCode("W024"),
            message: msg,
            span: Some(arg.value_span.clone()),
            notes: Vec::new(),
            suggestions: Vec::new(),
        });
    }
    // Side effects: track text.size for em resolution; capture
    // document metadata.
    if matches!(target, set_schema::Target::Text)
        && arg.key == "size"
        && let AttrValue::Length(pt) = &value
    {
        *current_text_size_pt = *pt;
    }
    if matches!(target, set_schema::Target::Document)
        && let AttrValue::Str(s) = &value
    {
        match arg.key.as_str() {
            "title" => metadata.title = Some(s.clone()),
            "author" => metadata.author = Some(s.clone()),
            "language" => metadata.language = Some(s.clone()),
            _ => {}
        }
    }
    attributes.insert(format!("set.arg.{}", arg.key), value);
}

/// Coerce a parser literal to the type required by the target slot.
/// Length values are resolved to PDF points using `em_pt` for `em`
/// literals.
fn coerce_value(slot: set_schema::SlotType, value: &SetValue, em_pt: f64) -> Option<AttrValue> {
    use set_schema::SlotType;
    match (slot, value) {
        // Bare identifiers are accepted in string slots so authors can
        // write `numbering: bottom-center` without quotes — folded with
        // the quoted-string arm because they produce the same value.
        (SlotType::Str, SetValue::Str(s) | SetValue::Ident(s)) => Some(AttrValue::Str(s.clone())),
        (SlotType::Length, SetValue::Length(v, unit)) => {
            Some(AttrValue::Length(length_to_pt(*v, *unit, em_pt)))
        }
        // A bare number in a length slot defaults to pt for ergonomic
        // input; this mirrors how Typst treats `1pt` and `1` in length
        // contexts. (Float-without-unit is more common than int.)
        (SlotType::Length, SetValue::Float(v)) => Some(AttrValue::Length(*v)),
        (SlotType::Length, SetValue::Int(v)) => Some(AttrValue::Length(int_to_f64(*v))),
        (SlotType::Float, SetValue::Float(v)) => Some(AttrValue::Float(*v)),
        (SlotType::Float, SetValue::Int(v)) => Some(AttrValue::Float(int_to_f64(*v))),
        _ => None,
    }
}

/// `#set` literals only accept i64 values that fit comfortably in
/// f64's mantissa; we cap the range at ±2^53 so the cast is exact for
/// every value an author could plausibly type.
#[allow(
    clippy::cast_precision_loss,
    reason = "values clamped to f64-exact range above"
)]
fn int_to_f64(v: i64) -> f64 {
    v.clamp(-(1_i64 << 53), 1_i64 << 53) as f64
}

fn length_to_pt(value: f64, unit: mosaic_parse::LengthUnit, em_pt: f64) -> f64 {
    match unit {
        mosaic_parse::LengthUnit::Pt => value,
        mosaic_parse::LengthUnit::Mm => value * 72.0 / 25.4,
        mosaic_parse::LengthUnit::Em => value * em_pt,
    }
}

fn describe_value(v: &SetValue) -> &'static str {
    match v {
        SetValue::Str(_) => "a string",
        SetValue::Int(_) => "an integer",
        SetValue::Float(_) => "a number",
        SetValue::Length(_, _) => "a length",
        SetValue::Ident(_) => "an identifier",
    }
}

fn sanity_floor_warning(
    target: set_schema::Target,
    key: &str,
    value: &AttrValue,
) -> Option<String> {
    use set_schema::Target;
    match (target, key) {
        (Target::Page, "margin") => {
            // 3mm ≈ 8.5pt — below most printer hardware margins.
            if let AttrValue::Length(pt) = value
                && *pt < 8.5
            {
                return Some(format!(
                    "page margin {pt:.2}pt is below the 3mm sanity floor"
                ));
            }
        }
        (Target::Text, "size") => {
            if let AttrValue::Length(pt) = value
                && *pt < 4.0
            {
                return Some(format!("text size {pt:.2}pt is below the 4pt sanity floor"));
            }
        }
        (Target::Text, "leading") => {
            if let AttrValue::Float(v) = value
                && *v < 0.8
            {
                return Some(format!("text leading {v:.2} is below the 0.8 sanity floor"));
            }
        }
        _ => {}
    }
    None
}

fn lower_inlines(doc: &mut Document, parent: NodeId, inlines: &[Inline]) {
    for inline in inlines {
        let kind = match inline.kind {
            InlineKind::Text => NodeKind::Text,
            InlineKind::Emphasis => NodeKind::Emphasis,
            InlineKind::Strong => NodeKind::Strong,
            InlineKind::Code => NodeKind::Raw,
            InlineKind::Reference => NodeKind::Reference,
        };
        let mut attributes: AttrMap = BTreeMap::new();
        match inline.kind {
            InlineKind::Reference => {
                // Pre-resolve placeholder text: the resolver overwrites
                // `text` with the target's resolved string. Layout reads
                // the same `text` attribute either way, so an unresolved
                // reference still renders something visible — which makes
                // E042 diagnostics easier to spot in the output PDF.
                attributes.insert("label".to_owned(), AttrValue::Str(inline.text.clone()));
                attributes.insert(
                    "text".to_owned(),
                    AttrValue::Str(format!("?{}?", inline.text)),
                );
            }
            _ => {
                attributes.insert("text".to_owned(), AttrValue::Str(inline.text.clone()));
            }
        }
        doc.alloc_child(
            parent,
            Node {
                id: NodeId::default(),
                kind,
                span: inline.span.clone(),
                content_hash: Default::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes,
            },
        );
    }
}

/// Convenience: parse + lower + resolve in one step. Concatenates the
/// diagnostics from each stage so callers can render them uniformly.
pub fn lower(src: &str, file: &std::path::Path) -> LowerResult {
    let parse_result = mosaic_parse::parse(src, file);
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

    use mosaic_core::{Node, NodeKind};

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
    fn lowers_set_block_as_raw() {
        let r = lower(
            "#set page(paper: \"A4\", margin: 24mm)\n\n= After\n",
            &PathBuf::from("test.mos"),
        );
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let kinds: Vec<NodeKind> = r.document.nodes().map(|n| n.kind).collect();
        assert!(kinds.contains(&NodeKind::Raw));
        assert!(kinds.contains(&NodeKind::Section));
        // Find the Raw set node and check its attributes contain typed
        // values: paper as Str, margin resolved to pt as Length.
        let raw = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Raw && n.attributes.contains_key("set"))
            .expect("set Raw node");
        assert_eq!(
            raw.attributes.get("set.arg.paper"),
            Some(&AttrValue::Str("A4".to_owned()))
        );
        match raw.attributes.get("set.arg.margin") {
            Some(AttrValue::Length(pt)) => {
                // 24mm × 72 / 25.4 ≈ 68.031pt
                assert!((pt - 68.031).abs() < 0.01, "margin pt = {pt}");
            }
            other => panic!("expected Length for margin, got {other:?}"),
        }
    }

    #[test]
    fn unknown_set_target_emits_e020() {
        let r = lower("#set widget(x: 1)\n", &PathBuf::from("test.mos"));
        assert!(r.diagnostics.iter().any(|d| d.code.0 == "E020"));
    }

    #[test]
    fn unknown_set_arg_emits_e021() {
        let r = lower("#set page(weirdkey: 1)\n", &PathBuf::from("test.mos"));
        assert!(
            r.diagnostics.iter().any(|d| d.code.0 == "E021"),
            "got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn type_mismatch_emits_e022() {
        let r = lower("#set page(margin: \"wide\")\n", &PathBuf::from("test.mos"));
        assert!(
            r.diagnostics.iter().any(|d| d.code.0 == "E022"),
            "got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn sanity_floor_emits_w024() {
        let r = lower("#set page(margin: 0.5mm)\n", &PathBuf::from("test.mos"));
        assert!(
            r.diagnostics.iter().any(|d| d.code.0 == "W024"),
            "got {:?}",
            r.diagnostics
        );
        // Value should still apply despite the warning.
        assert!(!r.has_errors());
    }

    #[test]
    fn em_resolves_against_current_text_size() {
        let r = lower(
            "#set text(size: 12pt)\n#set text(leading: 1.0)\n#set page(margin: 2em)\n",
            &PathBuf::from("test.mos"),
        );
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let raws: Vec<&Node> = r
            .document
            .nodes()
            .filter(|n| n.kind == NodeKind::Raw && n.attributes.contains_key("set"))
            .collect();
        let page = raws
            .iter()
            .find(|n| n.attributes.get("set") == Some(&AttrValue::Str("page".to_owned())))
            .unwrap();
        match page.attributes.get("set.arg.margin") {
            Some(AttrValue::Length(pt)) => assert!((pt - 24.0).abs() < 0.01, "got {pt}"),
            other => panic!("expected Length, got {other:?}"),
        }
    }

    #[test]
    fn document_metadata_captured() {
        let r = lower(
            "#set document(title: \"T\", author: \"A\", language: \"en\")\n",
            &PathBuf::from("test.mos"),
        );
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.metadata.title.as_deref(), Some("T"));
        assert_eq!(r.metadata.author.as_deref(), Some("A"));
        assert_eq!(r.metadata.language.as_deref(), Some("en"));
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
            "mosaic-eval-image-{}-{}",
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
    fn undecodable_image_emits_e052() {
        let dir = std::env::temp_dir().join(format!(
            "mosaic-eval-bad-{}",
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
    fn set_image_width_is_accepted_without_error() {
        // Schema acceptance — even though MVP 1.5 doesn't apply
        // `#set image(width: ...)` to bare images yet, the directive
        // should not emit E020 (unknown target) or E021 (unknown key).
        let r = lower("#set image(width: 200pt)\n", &PathBuf::from("test.mos"));
        assert!(
            !r.diagnostics
                .iter()
                .any(|d| d.code.0 == "E020" || d.code.0 == "E021"),
            "unexpected diagnostics: {:?}",
            r.diagnostics
        );
    }
}
