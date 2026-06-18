//! Lower `#image` and `#figure` parser directives into semantic nodes.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use mos_core::{
    AttrMap, AttrValue, Diagnostic, Document, NodeId, NodeKind, NodeSpec, SourceSpan, codes,
};
use mos_parse::{SetArg, SetValue};

use crate::{image, insert_label_attributes, set::coerce_positive_length};

/// Lower `#image(...)` into one [`NodeKind::Image`] node.
///
/// Decoded pixels and dimensions are stored in node attributes so later stages
/// do not re-open the source file.
pub fn lower_image_directive(
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
        NodeSpec::new(NodeKind::Image, span.clone()).with_attributes(attributes),
    );
}

/// Lower `#figure(image: ..., caption: ...)` into a figure node.
///
/// The figure gets an image child and a caption paragraph child.
pub fn lower_figure_directive(
    document: &mut Document,
    root: NodeId,
    args: &[SetArg],
    span: &SourceSpan,
    source_file: &Path,
    em_pt: f64,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let figure_args = collect_figure_args(args, diagnostics);

    // Build the image attributes before allocating the Figure node.
    // Failed image load should not leave a phantom caption-only figure.
    let Some((image_attrs, _label)) = build_image_attributes(
        &figure_args.image_args,
        span,
        source_file,
        em_pt,
        diagnostics,
    ) else {
        return;
    };

    let mut figure_attrs: AttrMap = BTreeMap::new();
    if let Some((label, label_span)) = figure_args.label {
        insert_label_attributes(&mut figure_attrs, &label, Some(&label_span));
    }
    // Only the non-default opt-out is recorded; absence means "numbered".
    if figure_args.numbered == Some(false) {
        figure_attrs.insert("numbered".to_owned(), AttrValue::Bool(false));
    }
    if let Some(supp) = figure_args.supplement {
        figure_attrs.insert("supplement".to_owned(), AttrValue::Str(supp));
    }
    let figure_id = document.alloc_child(
        root,
        NodeSpec::new(NodeKind::Figure, span.clone()).with_attributes(figure_attrs),
    );
    document.alloc_child(
        figure_id,
        NodeSpec::new(NodeKind::Image, span.clone()).with_attributes(image_attrs),
    );
    if let Some(caption) = figure_args.caption {
        append_caption(document, figure_id, caption);
    }
}

struct FigureDirectiveArgs {
    image_args: Vec<SetArg>,
    caption: Option<(String, SourceSpan)>,
    label: Option<(String, SourceSpan)>,
    numbered: Option<bool>,
    supplement: Option<String>,
}

fn collect_figure_args(args: &[SetArg], diagnostics: &mut Vec<Diagnostic>) -> FigureDirectiveArgs {
    let mut collected = FigureDirectiveArgs {
        image_args: Vec::new(),
        caption: None,
        label: None,
        numbered: None,
        supplement: None,
    };
    for arg in args {
        collect_one_figure_arg(arg, &mut collected, diagnostics);
    }
    collected
}

fn collect_one_figure_arg(
    arg: &SetArg,
    collected: &mut FigureDirectiveArgs,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match arg {
        SetArg::Positional { .. } => collected.image_args.push(arg.clone()),
        SetArg::Named {
            key,
            value,
            key_span,
            value_span,
        } => match key.as_str() {
            "image" => collected.image_args.push(SetArg::Positional {
                value: value.clone(),
                value_span: value_span.clone(),
            }),
            "width" | "height" | "alt" => collected.image_args.push(arg.clone()),
            "caption" => collect_string_arg(
                value,
                value_span,
                "`#figure(caption: ...)` expects a string",
                &mut collected.caption,
                diagnostics,
            ),
            "label" => match value {
                SetValue::Str(s) => {
                    collected.label = Some((s.clone(), string_content_span(value_span)));
                }
                _ => type_error(value_span, "`#figure(label: ...)` expects a string", diagnostics),
            },
            "numbered" => collect_numbered(value, value_span, collected, diagnostics),
            "supplement" => collect_supplement(value, value_span, collected, diagnostics),
            _ => diagnostics.push(
                Diagnostic::simple(&codes::MOS0015, None,
                    format!(
                        "unknown argument `{key}` for `#figure` (valid: image, caption, alt, width, height, label, numbered, supplement)"
                    ),
                )
                .with_span(key_span.clone()),
            ),
        },
    }
}

fn collect_string_arg(
    value: &SetValue,
    value_span: &SourceSpan,
    message: &'static str,
    target: &mut Option<(String, SourceSpan)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match value {
        SetValue::Str(s) => *target = Some((s.clone(), value_span.clone())),
        _ => type_error(value_span, message, diagnostics),
    }
}

fn collect_numbered(
    value: &SetValue,
    value_span: &SourceSpan,
    collected: &mut FigureDirectiveArgs,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match value {
        SetValue::Ident(word) if word == "true" => collected.numbered = Some(true),
        SetValue::Ident(word) if word == "false" => collected.numbered = Some(false),
        _ => type_error(
            value_span,
            "`#figure(numbered: ...)` expects `true` or `false`",
            diagnostics,
        ),
    }
}

fn collect_supplement(
    value: &SetValue,
    value_span: &SourceSpan,
    collected: &mut FigureDirectiveArgs,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match value {
        SetValue::Str(s) => collected.supplement = Some(s.clone()),
        SetValue::Ident(word) if word == "none" => collected.supplement = Some(String::new()),
        _ => type_error(
            value_span,
            "`#figure(supplement: ...)` expects a string or `none`",
            diagnostics,
        ),
    }
}

fn append_caption(document: &mut Document, figure_id: NodeId, caption: (String, SourceSpan)) {
    let (text, caption_span) = caption;
    let caption_id = document.alloc_child(
        figure_id,
        NodeSpec::new(NodeKind::Paragraph, caption_span.clone()).with_attributes({
            let mut attrs = AttrMap::new();
            attrs.insert("role".to_owned(), AttrValue::Str("caption".to_owned()));
            attrs
        }),
    );
    let mut child_attrs = AttrMap::new();
    child_attrs.insert("text".to_owned(), AttrValue::Str(text));
    document.alloc_child(
        caption_id,
        NodeSpec::new(NodeKind::Text, caption_span).with_attributes(child_attrs),
    );
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
    let image_args = collect_image_args(args, em_pt, diagnostics);
    let Some((path, _path_span)) = image_args.src_path else {
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
    if let Some(a) = image_args.alt {
        attrs.insert("alt".to_owned(), AttrValue::Str(a));
    }
    if let Some(w) = image_args.declared_width {
        attrs.insert("width".to_owned(), AttrValue::Length(w));
    }
    if let Some(h) = image_args.declared_height {
        attrs.insert("height".to_owned(), AttrValue::Length(h));
    }
    if let Some((label_text, label_span)) = &image_args.label {
        insert_label_attributes(&mut attrs, label_text, Some(label_span));
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
    Some((attrs, image_args.label.map(|(text, _)| text)))
}

struct ImageDirectiveArgs {
    src_path: Option<(String, SourceSpan)>,
    alt: Option<String>,
    declared_width: Option<f64>,
    declared_height: Option<f64>,
    label: Option<(String, SourceSpan)>,
}

fn collect_image_args(
    args: &[SetArg],
    em_pt: f64,
    diagnostics: &mut Vec<Diagnostic>,
) -> ImageDirectiveArgs {
    let mut collected = ImageDirectiveArgs {
        src_path: None,
        alt: None,
        declared_width: None,
        declared_height: None,
        label: None,
    };
    for arg in args {
        collect_one_image_arg(arg, em_pt, &mut collected, diagnostics);
    }
    collected
}

fn collect_one_image_arg(
    arg: &SetArg,
    em_pt: f64,
    collected: &mut ImageDirectiveArgs,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match arg {
        SetArg::Positional { value, value_span } => collect_image_path(
            value,
            value_span,
            &mut collected.src_path,
            diagnostics,
        ),
        SetArg::Named {
            key,
            value,
            key_span,
            value_span,
        } => match key.as_str() {
            "src" | "path" => collect_image_path(
                value,
                value_span,
                &mut collected.src_path,
                diagnostics,
            ),
            "alt" => match value {
                SetValue::Str(s) => collected.alt = Some(s.clone()),
                _ => type_error(value_span, "`#image(alt: ...)` expects a string", diagnostics),
            },
            "width" => {
                if let Some(width) = coerce_positive_length(
                    value,
                    em_pt,
                    "width",
                    value_span,
                    diagnostics,
                ) {
                    collected.declared_width = Some(width);
                }
            }
            "height" => {
                if let Some(height) = coerce_positive_length(
                    value,
                    em_pt,
                    "height",
                    value_span,
                    diagnostics,
                ) {
                    collected.declared_height = Some(height);
                }
            }
            "label" => match value {
                SetValue::Str(s) => {
                    collected.label = Some((s.clone(), string_content_span(value_span)));
                }
                _ => type_error(value_span, "`#image(label: ...)` expects a string", diagnostics),
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

fn collect_image_path(
    value: &SetValue,
    value_span: &SourceSpan,
    target: &mut Option<(String, SourceSpan)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match value {
        SetValue::Str(s) => *target = Some((s.clone(), value_span.clone())),
        _ => type_error(
            value_span,
            "`#image(...)` expects a string path",
            diagnostics,
        ),
    }
}

fn type_error(value_span: &SourceSpan, message: &'static str, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics
        .push(Diagnostic::simple(&codes::MOS0020, None, message).with_span(value_span.clone()));
}

fn string_content_span(value_span: &SourceSpan) -> SourceSpan {
    if value_span.end() > value_span.start().saturating_add(1) {
        SourceSpan::new(
            value_span.file.clone(),
            value_span.start() + 1,
            value_span.end() - 1,
        )
    } else {
        value_span.clone()
    }
}
