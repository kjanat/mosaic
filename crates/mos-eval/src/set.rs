//! Lower `#set` directives and coerce directive values.

use std::collections::BTreeMap;

use mos_core::{
    AttrMap, AttrValue, Diagnostic, Document, NodeId, NodeKind, NodeSpec, SourceSpan, codes,
};
use mos_parse::{SetArg, SetValue};

use crate::{DocumentMetadata, set_schema};

/// Lower a `#set name(...)` directive into a `Raw` node carrying the
/// resolved attribute payload. The split exists so the dispatch in
/// `Evaluator::evaluate` only has to thread state through three
/// directive-shaped helpers instead of one large match arm.
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_set_directive(
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
            Diagnostic::simple(&codes::MOS0011, None,
                format!(
                    "unknown `#set` target `{name}` (expected `page`, `text`, `document`, or `image`)"
                ),
            )
            .with_span(span.clone()),
        );
        document.alloc_child(root, set_node(span, attributes));
        return;
    };
    for arg in args {
        // The parser refuses positional args for `#set` already, so
        // reaching the Positional arm here would mean a future caller
        // forgot the `allow_positional=false` flag. Diagnose loudly.
        if matches!(arg, SetArg::Positional { .. }) {
            diagnostics.push(
                Diagnostic::simple(
                    &codes::MOS0024,
                    None,
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
    document.alloc_child(root, set_node(span, attributes));
}

fn set_node(span: &SourceSpan, attributes: AttrMap) -> NodeSpec {
    NodeSpec::new(NodeKind::Raw, span.clone()).with_attributes(attributes)
}

/// Convert one parser-level `SetArg` into an attribute on the Raw node
/// representing this `#set` directive. Emits semantic diagnostics
/// (`MOS0015` unknown key, `MOS0020` type mismatch, `MOS0027` sanity floor) and
/// updates `metadata` / `current_text_size_pt` as a side effect.
fn lower_set_arg(
    target: set_schema::Target,
    arg: &SetArg,
    attributes: &mut AttrMap,
    metadata: &mut DocumentMetadata,
    current_text_size_pt: &mut f64,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // `#set` only carries `key: value` args. Caller filters positionals;
    // debug assert flags future callers that miss that gate.
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
            Diagnostic::simple(
                &codes::MOS0015,
                None,
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
            Diagnostic::simple(
                &codes::MOS0020,
                None,
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
        diagnostics
            .push(Diagnostic::simple(&codes::MOS0027, None, msg).with_span(value_span.clone()));
    }
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
        // write `numbering: bottom-center` without quotes.
        (SlotType::Str, SetValue::Str(s) | SetValue::Ident(s)) => Some(AttrValue::Str(s.clone())),
        (SlotType::Length, SetValue::Length(v, unit)) => {
            Some(AttrValue::Length(length_to_pt(*v, *unit, em_pt)))
        }
        // Bare numbers in length slots default to pt for ergonomic input.
        (SlotType::Length, SetValue::Float(v)) => Some(AttrValue::Length(*v)),
        (SlotType::Length, SetValue::Int(v)) => Some(AttrValue::Length(int_to_f64(*v))),
        (SlotType::Float, SetValue::Float(v)) => Some(AttrValue::Float(*v)),
        (SlotType::Float, SetValue::Int(v)) => Some(AttrValue::Float(int_to_f64(*v))),
        _ => None,
    }
}

/// Coerce a `#image(width|height: ...)` argument to a strictly positive
/// length in points. Bare numerics resolve as pt for ergonomics.
pub(super) fn coerce_positive_length(
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
                Diagnostic::simple(
                    &codes::MOS0020,
                    None,
                    format!("`#image({key}: ...)` expects a length"),
                )
                .with_span(value_span.clone()),
            );
            return None;
        }
    };
    if pt <= 0.0 {
        diagnostics.push(
            Diagnostic::simple(
                &codes::MOS0020,
                None,
                format!("`#image({key}: ...)` expects a positive length"),
            )
            .with_span(value_span.clone()),
        );
        return None;
    }
    Some(pt)
}

/// `#set` literals only accept i64 values that fit comfortably in
/// f64's mantissa; cap at ±2^53 so the cast is exact.
#[allow(
    clippy::cast_precision_loss,
    reason = "values clamped to f64-exact range above"
)]
fn int_to_f64(v: i64) -> f64 {
    v.clamp(-(1_i64 << 53), 1_i64 << 53) as f64
}

fn length_to_pt(value: f64, unit: mos_parse::LengthUnit, em_pt: f64) -> f64 {
    match unit {
        mos_parse::LengthUnit::Pt => value,
        mos_parse::LengthUnit::Mm => value * 72.0 / 25.4,
        mos_parse::LengthUnit::Em => value * em_pt,
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
            // 3mm ~= 8.5pt, below most printer hardware margins.
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

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        reason = "tests panic loudly on setup failure; matches crate-wide test-module convention"
    )]

    use std::path::PathBuf;

    use mos_core::{AttrValue, Node, NodeKind, codes};

    use crate::lower;

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
                assert!((pt - 68.031).abs() < 0.01, "margin pt = {pt}");
            }
            other => panic!("expected Length for margin, got {other:?}"),
        }
    }

    #[test]
    fn unknown_set_target_emits_mos0011() {
        let r = lower("#set widget(x: 1)\n", &PathBuf::from("test.mos"));
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0011.code())
        );
    }

    #[test]
    fn unknown_set_arg_emits_mos0015() {
        let r = lower("#set page(weirdkey: 1)\n", &PathBuf::from("test.mos"));
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0015.code()),
            "got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn type_mismatch_emits_mos0020() {
        let r = lower("#set page(margin: \"wide\")\n", &PathBuf::from("test.mos"));
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0020.code()),
            "got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn sanity_floor_emits_mos0027() {
        let r = lower("#set page(margin: 0.5mm)\n", &PathBuf::from("test.mos"));
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0027.code()),
            "got {:?}",
            r.diagnostics
        );
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
    fn set_image_width_is_accepted_without_error() {
        let r = lower("#set image(width: 200pt)\n", &PathBuf::from("test.mos"));
        assert!(
            r.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            r.diagnostics
        );
        assert!(!r.document.nodes().any(|n| n.kind == NodeKind::Image));
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
}
