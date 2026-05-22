//! Expression and scripting evaluator (manifest §4, §25).
//!
//! The "evaluator" is really a *lowerer + resolver*: it walks a
//! [`SyntaxTree`] from `mosaic-parse` and builds the typed semantic
//! [`Document`] graph from `mosaic-core` (manifest §6 stage 2), then
//! runs the [`resolve`] pass to assign section numbers and rewrite
//! `@label` cross-references (§6 stage 3, MVP 1).

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

mod image;
mod resolve;
mod set_schema;

use std::collections::BTreeMap;
use std::sync::Arc;

use mosaic_core::{
    AttrMap, AttrValue, Diagnostic, DiagnosticCode, Document, Node, NodeId, NodeKind, Severity,
    SourceSpan, StyleId,
};
use mosaic_parse::{
    DirectiveKind, Inline, InlineKind, Item, ListItem, RawBlockKind, SetArg, SetValue, SyntaxTree,
};

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
        // The parser refuses positional args for `#set` already, so
        // reaching the Positional arm here would mean a future caller
        // forgot the `allow_positional=false` flag. Diagnose loudly.
        if matches!(arg, SetArg::Positional { .. }) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E015"),
                    format!("`#set {name}` does not accept positional arguments"),
                )
                .with_span(arg.value_span().clone()),
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
    em_pt: f64,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some((attributes, _label)) =
        build_image_attributes(args, span, source_file, em_pt, diagnostics)
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
    em_pt: f64,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Pluck the image-specifying args (`image:` path, optional
    // `width`/`height`/`alt`) into a synthetic SetArg list so the
    // existing builder can reuse them. A leading positional string
    // (the `SetArg::Positional` arm below) is also accepted as the
    // image path — `#figure("x.png")` is the captionless short form,
    // equivalent to `#figure(image: "x.png")`.
    let mut image_args: Vec<SetArg> = Vec::new();
    let mut caption: Option<(String, SourceSpan)> = None;
    let mut figure_label: Option<String> = None;
    for arg in args {
        match arg {
            // A leading positional string is the same shorthand
            // `#image(...)` accepts — `#figure("scan.png")` is the
            // captioned-image short form, equivalent to
            // `#figure(image: "scan.png")`.
            SetArg::Positional { .. } => image_args.push(arg.clone()),
            SetArg::Named {
                key,
                value,
                key_span,
                value_span,
            } => match key.as_str() {
                "image" => {
                    // Rewrite the named `image:` arg as the positional
                    // slot `build_image_attributes` expects.
                    image_args.push(SetArg::Positional {
                        value: value.clone(),
                        value_span: value_span.clone(),
                    });
                }
                "width" | "height" | "alt" => {
                    image_args.push(arg.clone());
                }
                "caption" => match value {
                    SetValue::Str(s) => {
                        caption = Some((s.clone(), value_span.clone()));
                    }
                    _ => diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode("E022"),
                            "`#figure(caption: ...)` expects a string",
                        )
                        .with_span(value_span.clone()),
                    ),
                },
                "label" => match value {
                    SetValue::Str(s) => figure_label = Some(s.clone()),
                    _ => diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode("E022"),
                            "`#figure(label: ...)` expects a string",
                        )
                        .with_span(value_span.clone()),
                    ),
                },
                _ => diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E021"),
                        format!(
                            "unknown argument `{key}` for `#figure` (valid: image, caption, alt, width, height, label)"
                        ),
                    )
                    .with_span(key_span.clone()),
                ),
            },
        }
    }

    // Build the image attributes *before* allocating the Figure node.
    // If the image can't be loaded (E050/E051/E052), we'd otherwise
    // leave a stray Figure on the document root — a caption-only
    // figure is not a meaningful artifact and would still render the
    // caption next to whatever the user thought they were captioning.
    // The caller already emitted the relevant diagnostic; dropping
    // the figure means the document keeps its other content intact
    // without a phantom block.
    let Some((image_attrs, _label)) =
        build_image_attributes(&image_args, span, source_file, em_pt, diagnostics)
    else {
        return;
    };

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
    document.alloc_child(
        figure_id,
        Node {
            id: NodeId::default(),
            kind: NodeKind::Image,
            span: span.clone(),
            content_hash: Default::default(),
            style_id: StyleId::default(),
            children: Vec::new(),
            attributes: image_attrs,
        },
    );
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
    em_pt: f64,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(AttrMap, Option<String>)> {
    let mut src_path: Option<(String, SourceSpan)> = None;
    let mut alt: Option<String> = None;
    let mut declared_width: Option<f64> = None;
    let mut declared_height: Option<f64> = None;
    let mut label: Option<String> = None;
    for arg in args {
        match arg {
            // Positional first arg — the path literal.
            SetArg::Positional { value, value_span } => match value {
                SetValue::Str(s) => src_path = Some((s.clone(), value_span.clone())),
                _ => diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E022"),
                        "`#image(...)` expects a string path",
                    )
                    .with_span(value_span.clone()),
                ),
            },
            SetArg::Named {
                key,
                value,
                key_span,
                value_span,
            } => match key.as_str() {
                "src" | "path" => match value {
                    SetValue::Str(s) => src_path = Some((s.clone(), value_span.clone())),
                    _ => diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode("E022"),
                            "`#image(...)` expects a string path",
                        )
                        .with_span(value_span.clone()),
                    ),
                },
                "alt" => match value {
                    SetValue::Str(s) => alt = Some(s.clone()),
                    _ => diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode("E022"),
                            "`#image(alt: ...)` expects a string",
                        )
                        .with_span(value_span.clone()),
                    ),
                },
                "width" => {
                    if let Some(v) =
                        coerce_positive_length(value, em_pt, "width", value_span, diagnostics)
                    {
                        declared_width = Some(v);
                    }
                }
                "height" => {
                    if let Some(v) =
                        coerce_positive_length(value, em_pt, "height", value_span, diagnostics)
                    {
                        declared_height = Some(v);
                    }
                }
                "label" => match value {
                    SetValue::Str(s) => label = Some(s.clone()),
                    _ => diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode("E022"),
                            "`#image(label: ...)` expects a string",
                        )
                        .with_span(value_span.clone()),
                    ),
                },
                _ => diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E021"),
                        format!(
                            "unknown argument `{key}` for `#image` (valid: src, alt, width, height, label)"
                        ),
                    )
                    .with_span(key_span.clone()),
                ),
            },
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
    // A bare empty / whitespace-only path string is the same user
    // mistake as omitting the path entirely — they wrote `#image("")`
    // and meant to fill in a filename. Surface it as E050 so the
    // diagnostic points at the missing path, not at an `E051` ("cannot
    // read empty path") from the I/O layer.
    if path.trim().is_empty() {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode("E050"),
                "`#image(...)` requires a non-empty path (e.g. `#image(\"scan.png\")`)",
            )
            .with_span(span.clone()),
        );
        return None;
    }
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
    // `#set` only carries `key: value` args — the caller already
    // filters positionals via the `matches!(arg, SetArg::Positional)`
    // gate in `lower_set_directive`. Destructuring here makes the
    // assumption explicit; a debug-assert flags any future caller
    // that misses the filter.
    let SetArg::Named {
        key,
        value: raw_value,
        key_span,
        value_span,
    } = arg
    else {
        debug_assert!(false, "lower_set_arg received Positional arg");
        return;
    };
    let Some(slot) = target.slot(key) else {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode("E021"),
                format!(
                    "unknown argument `{key}` for `#set {}` (valid: {})",
                    target.name(),
                    target.keys().join(", ")
                ),
            )
            .with_span(key_span.clone()),
        );
        return;
    };
    let Some(value) = coerce_value(slot, raw_value, *current_text_size_pt) else {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode("E022"),
                format!(
                    "`#set {} ({key}: …)` expects {}, got {}",
                    target.name(),
                    slot.expected(),
                    describe_value(raw_value),
                ),
            )
            .with_span(value_span.clone()),
        );
        return;
    };
    if let Some(msg) = sanity_floor_warning(target, key, &value) {
        diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            code: DiagnosticCode("W024"),
            message: msg,
            span: Some(value_span.clone()),
            notes: Vec::new(),
            suggestions: Vec::new(),
        });
    }
    // Side effects: track text.size for em resolution; capture
    // document metadata.
    if matches!(target, set_schema::Target::Text)
        && key == "size"
        && let AttrValue::Length(pt) = &value
    {
        *current_text_size_pt = *pt;
    }
    if matches!(target, set_schema::Target::Document)
        && let AttrValue::Str(s) = &value
    {
        match key.as_str() {
            "title" => metadata.title = Some(s.clone()),
            "author" => metadata.author = Some(s.clone()),
            "language" => metadata.language = Some(s.clone()),
            _ => {}
        }
    }
    attributes.insert(format!("set.arg.{key}"), value);
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

/// Coerce a `#image(width|height: ...)` argument to a strictly positive
/// length in points. Bare numerics resolve as pt for ergonomics
/// (mirrors `coerce_value`). Non-numeric values, zero, and negative
/// values all produce `E022` so layout never sees a zero/negative
/// image box.
fn coerce_positive_length(
    value: &SetValue,
    em_pt: f64,
    key: &str,
    value_span: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<f64> {
    let pt = match value {
        SetValue::Length(v, unit) => length_to_pt(*v, *unit, em_pt),
        SetValue::Float(v) => *v,
        SetValue::Int(v) => int_to_f64(*v),
        _ => {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E022"),
                    format!("`#image({key}: ...)` expects a length"),
                )
                .with_span(value_span.clone()),
            );
            return None;
        }
    };
    if pt <= 0.0 {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode("E022"),
                format!("`#image({key}: ...)` expects a positive length"),
            )
            .with_span(value_span.clone()),
        );
        return None;
    }
    Some(pt)
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

/// Allocate a [`NodeKind::List`] under `parent` and recursively lower
/// its [`ListItem`]s into [`NodeKind::ListItem`] children. The
/// `ordered` flag is preserved as a `Bool` attribute so layout can pick
/// the right marker style without re-walking the tree.
fn lower_list(
    doc: &mut Document,
    parent: NodeId,
    ordered: bool,
    items: &[ListItem],
    span: &SourceSpan,
) {
    let mut attributes: AttrMap = BTreeMap::new();
    attributes.insert("ordered".to_owned(), AttrValue::Bool(ordered));
    let list_id = doc.alloc_child(
        parent,
        Node {
            id: NodeId::default(),
            kind: NodeKind::List,
            span: span.clone(),
            content_hash: Default::default(),
            style_id: StyleId::default(),
            children: Vec::new(),
            attributes,
        },
    );
    for item in items {
        lower_list_item(doc, list_id, item);
    }
}

fn lower_list_item(doc: &mut Document, parent: NodeId, item: &ListItem) {
    let item_id = doc.alloc_child(
        parent,
        Node {
            id: NodeId::default(),
            kind: NodeKind::ListItem,
            span: item.span.clone(),
            content_hash: Default::default(),
            style_id: StyleId::default(),
            children: Vec::new(),
            attributes: AttrMap::new(),
        },
    );
    lower_inlines(doc, item_id, &item.inlines);
    for child in &item.children {
        if let Item::List {
            ordered,
            items,
            span,
        } = child
        {
            lower_list(doc, item_id, *ordered, items, span);
        }
    }
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

    #[test]
    fn set_image_width_is_accepted_without_error() {
        // `#set image(width: 200pt)` must lower as a style-config Raw
        // node, *not* as an image directive (which would raise E050
        // for the missing path). The parser tags the two shapes with
        // distinct `DirectiveKind`s — this test guards the routing.
        let r = lower("#set image(width: 200pt)\n", &PathBuf::from("test.mos"));
        assert!(
            r.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            r.diagnostics
        );
        // No Image node — `#set image(...)` is style configuration,
        // not a placement.
        assert!(!r.document.nodes().any(|n| n.kind == NodeKind::Image));
        // The Raw set node carries the image target + width slot.
        let raw = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::Raw && n.attributes.contains_key("set"))
            .expect("set Raw node");
        assert_eq!(
            raw.attributes.get("set"),
            Some(&AttrValue::Str("image".to_owned()))
        );
        assert_eq!(
            raw.attributes.get("set.arg.width"),
            Some(&AttrValue::Length(200.0))
        );
    }

    #[test]
    fn lowers_unordered_list() {
        let r = lower("- one\n- two\n", &PathBuf::from("test.mos"));
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let list = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::List)
            .expect("list node");
        assert_eq!(
            list.attributes.get("ordered"),
            Some(&AttrValue::Bool(false))
        );
        assert_eq!(list.children.len(), 2);
        let items: Vec<&Node> = list
            .children
            .iter()
            .filter_map(|id| r.document.get(*id))
            .collect();
        assert!(items.iter().all(|n| n.kind == NodeKind::ListItem));
        // Each item has at least one Text child carrying its content.
        for (item, expected) in items.iter().zip(["one", "two"]) {
            let text_child = item
                .children
                .iter()
                .filter_map(|id| r.document.get(*id))
                .find(|n| n.kind == NodeKind::Text)
                .expect("text child");
            assert_eq!(
                text_child.attributes.get("text"),
                Some(&AttrValue::Str(expected.to_owned()))
            );
        }
    }

    #[test]
    fn lowers_ordered_list_flag() {
        let r = lower("1. a\n2. b\n", &PathBuf::from("test.mos"));
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let list = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::List)
            .expect("list node");
        assert_eq!(list.attributes.get("ordered"), Some(&AttrValue::Bool(true)));
    }

    #[test]
    fn lowers_nested_list_as_listitem_child() {
        let r = lower("- outer\n  - inner\n", &PathBuf::from("test.mos"));
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let outer = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::List)
            .expect("outer list");
        let outer_item = r.document.get(outer.children[0]).unwrap();
        // The nested list lives as a child of the outer ListItem.
        let nested = outer_item
            .children
            .iter()
            .filter_map(|id| r.document.get(*id))
            .find(|n| n.kind == NodeKind::List)
            .expect("nested list");
        assert_eq!(nested.children.len(), 1);
        let nested_item = r.document.get(nested.children[0]).unwrap();
        assert_eq!(nested_item.kind, NodeKind::ListItem);
    }
}
