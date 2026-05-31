//! Lower `#image` and `#figure` parser directives into semantic nodes.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use mos_core::{
    AttrMap, AttrValue, Diagnostic, Document, Node, NodeId, NodeKind, SourceSpan, StyleId, codes,
};
use mos_parse::{SetArg, SetValue};

use crate::{image, set::coerce_positive_length};

/// Lower a top-level `#image(...)` directive into a single
/// [`NodeKind::Image`] node hanging off the document root. The decoded
/// pixel buffer and pixel dimensions are stashed in attributes so the
/// layout engine and PDF backend don't have to re-open the source file.
pub(super) fn lower_image_directive(
    document: &mut Document,
    root: NodeId,
    args: &[SetArg],
    span: &SourceSpan,
    source_file: &Path,
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
pub(super) fn lower_figure_directive(
    document: &mut Document,
    root: NodeId,
    args: &[SetArg],
    span: &SourceSpan,
    source_file: &Path,
    em_pt: f64,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Pluck the image-specifying args (`image:` path, optional
    // `width`/`height`/`alt`) into a synthetic SetArg list so the
    // existing builder can reuse them. A leading positional string
    // (the `SetArg::Positional` arm below) is also accepted as the
    // image path -- `#figure("x.png")` is the captionless short form,
    // equivalent to `#figure(image: "x.png")`.
    let mut image_args: Vec<SetArg> = Vec::new();
    let mut caption: Option<(String, SourceSpan)> = None;
    let mut figure_label: Option<String> = None;
    for arg in args {
        match arg {
            // A leading positional string is the same shorthand
            // `#image(...)` accepts -- `#figure("scan.png")` is the
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
                        Diagnostic::simple(&codes::MOS0020, None,
                            "`#figure(caption: ...)` expects a string",
                        )
                        .with_span(value_span.clone()),
                    ),
                },
                "label" => match value {
                    SetValue::Str(s) => figure_label = Some(s.clone()),
                    _ => diagnostics.push(
                        Diagnostic::simple(&codes::MOS0020, None,
                            "`#figure(label: ...)` expects a string",
                        )
                        .with_span(value_span.clone()),
                    ),
                },
                _ => diagnostics.push(
                    Diagnostic::simple(&codes::MOS0015, None,
                        format!(
                            "unknown argument `{key}` for `#figure` (valid: image, caption, alt, width, height, label)"
                        ),
                    )
                    .with_span(key_span.clone()),
                ),
            },
        }
    }

    // Build the image attributes before allocating the Figure node.
    // Failed image load should not leave a phantom caption-only figure.
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
/// missing or the bytes can't be decoded -- the caller drops the node
/// in that case rather than emitting a half-built image.
fn build_image_attributes(
    args: &[SetArg],
    span: &SourceSpan,
    source_file: &Path,
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
            // Positional first arg -- the path literal.
            SetArg::Positional { value, value_span } => match value {
                SetValue::Str(s) => src_path = Some((s.clone(), value_span.clone())),
                _ => diagnostics.push(
                    Diagnostic::simple(&codes::MOS0020, None,
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
                        Diagnostic::simple(&codes::MOS0020, None,
                            "`#image(...)` expects a string path",
                        )
                        .with_span(value_span.clone()),
                    ),
                },
                "alt" => match value {
                    SetValue::Str(s) => alt = Some(s.clone()),
                    _ => diagnostics.push(
                        Diagnostic::simple(&codes::MOS0020, None,
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
                        Diagnostic::simple(&codes::MOS0020, None,
                            "`#image(label: ...)` expects a string",
                        )
                        .with_span(value_span.clone()),
                    ),
                },
                _ => diagnostics.push(
                    Diagnostic::simple(&codes::MOS0015, None,
                        format!(
                            "unknown argument `{key}` for `#image` (valid: src/path, alt, width, height, label)"
                        ),
                    )
                    .with_span(key_span.clone()),
                ),
            },
        }
    }
    let Some((path, _path_span)) = src_path else {
        diagnostics.push(
            Diagnostic::simple(
                &codes::MOS0037,
                None,
                "`#image(...)` requires a path (e.g. `#image(\"scan.png\")`)",
            )
            .with_span(span.clone()),
        );
        return None;
    };
    // A bare empty / whitespace-only path string is the same user
    // mistake as omitting the path entirely -- they wrote `#image("")`
    // and meant to fill in a filename.
    if path.trim().is_empty() {
        diagnostics.push(
            Diagnostic::simple(
                &codes::MOS0037,
                None,
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
