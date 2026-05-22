use mos_core::{AttrValue, Diagnostic, DiagnosticCode, Document, Node, NodeKind, Severity};
use mos_fonts::FontFamily;

use crate::{PageStyle, TextStyle};

/// Walk root children in source order and fold each `#set page(...)`
/// and `#set text(...)` into a [`PageStyle`] / [`TextStyle`]. Later
/// directives win (last-write-wins). `#set document(...)` is consumed
/// by the lowerer for PDF metadata and ignored here.
pub(crate) fn resolve_styles(document: &Document) -> (PageStyle, TextStyle, Vec<Diagnostic>) {
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
    if let Some(AttrValue::Str(name)) = node.attributes.get("set.arg.font") {
        next.family = FontFamily::resolve(name, Some(node.span.clone()), diagnostics);
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
pub(crate) fn pt_to_f32(v: f64) -> f32 {
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

    use crate::{A4_WIDTH_PT, MARGIN_PT};

    use super::{paper_size_pt, resolve_styles};

    fn alloc_set_block(doc: &mut Document, target: &str, args: &[(&str, AttrValue)]) -> NodeId {
        let mut attrs = AttrMap::new();
        attrs.insert("set".to_owned(), AttrValue::Str(target.to_owned()));
        for (key, value) in args {
            attrs.insert(format!("set.arg.{key}"), value.clone());
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
        alloc_set_block(
            &mut doc,
            "page",
            &[("margin", AttrValue::Length(50.0 * 72.0 / 25.4))],
        );

        let (page, _, diagnostics) = resolve_styles(&doc);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let expected = 50.0_f32 * 72.0 / 25.4;
        assert!((page.margin_pt - expected).abs() < 0.05);
    }

    #[test]
    fn set_page_paper_a5_changes_page_dimensions() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(
            &mut doc,
            "page",
            &[("paper", AttrValue::Str("A5".to_owned()))],
        );

        let (page, _, diagnostics) = resolve_styles(&doc);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
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

        let (_, text, diagnostics) = resolve_styles(&doc);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!((text.size_pt - 20.0).abs() < 0.01);
    }

    #[test]
    fn negative_margin_is_rejected_with_e025() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(&mut doc, "page", &[("margin", AttrValue::Length(-10.0))]);

        let (page, _, diagnostics) = resolve_styles(&doc);

        assert!(diagnostics.iter().any(|d| d.code.0 == "E025"));
        assert!((page.margin_pt - MARGIN_PT).abs() < 0.5);
    }

    #[test]
    fn oversized_margin_is_rejected_with_e025() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(&mut doc, "page", &[("margin", AttrValue::Length(400.0))]);

        let (_, _, diagnostics) = resolve_styles(&doc);

        assert!(diagnostics.iter().any(|d| d.code.0 == "E025"));
    }

    #[test]
    fn paper_shrink_revalidates_carried_margin() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(
            &mut doc,
            "page",
            &[
                ("paper", AttrValue::Str("A0".to_owned())),
                ("margin", AttrValue::Length(300.0)),
            ],
        );
        alloc_set_block(
            &mut doc,
            "page",
            &[("paper", AttrValue::Str("A5".to_owned()))],
        );

        let (page, _, diagnostics) = resolve_styles(&doc);

        assert!(
            diagnostics.iter().any(|d| d.code.0 == "E025"),
            "expected E025 from paper shrink, got {diagnostics:?}"
        );
        assert!(
            (page.width_pt - 2383.94).abs() < 1.0,
            "w = {}",
            page.width_pt
        );
    }

    #[test]
    fn earlier_valid_size_survives_later_rejection() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(50.0))]);
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(1000.0))]);

        let (_, text, diagnostics) = resolve_styles(&doc);

        assert!(diagnostics.iter().any(|d| d.code.0 == "E025"));
        assert!((text.size_pt - 50.0).abs() < 0.01);
    }

    #[test]
    fn page_change_that_invalidates_carried_text_size_is_rejected() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(100.0))]);
        alloc_set_block(
            &mut doc,
            "page",
            &[("paper", AttrValue::Str("A8".to_owned()))],
        );

        let (page, text, diagnostics) = resolve_styles(&doc);

        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.0 == "E025" && d.message.contains("page change")),
            "expected E025 about page change, got {diagnostics:?}"
        );
        assert!((page.width_pt - A4_WIDTH_PT).abs() < 0.5);
        assert!((text.size_pt - 100.0).abs() < 0.01);
    }

    #[test]
    fn oversized_text_size_is_rejected_with_e025() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(1000.0))]);

        let (_, text, diagnostics) = resolve_styles(&doc);

        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.0 == "E025" && d.message.contains("vertical space")),
            "expected E025 about vertical space, got {diagnostics:?}"
        );
        assert_eq!(text.size_pt, crate::types::BODY_SIZE_PT);
    }

    #[test]
    fn rejected_text_size_says_previous_value_retained() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(14.0))]);
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(-1.0))]);

        let (_, _, diagnostics) = resolve_styles(&doc);

        let msg = diagnostics
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

        let (_, _, diagnostics) = resolve_styles(&doc);

        assert!(diagnostics.iter().any(|d| d.code.0 == "E025"));
    }

    #[test]
    fn unknown_paper_emits_e023() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(
            &mut doc,
            "page",
            &[("paper", AttrValue::Str("Foolscap".to_owned()))],
        );

        let (page, _, diagnostics) = resolve_styles(&doc);

        assert!(diagnostics.iter().any(|d| d.code.0 == "E023"));
        assert!((page.width_pt - A4_WIDTH_PT).abs() < 0.5);
    }

    #[test]
    fn last_set_wins() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(8.0))]);
        alloc_set_block(&mut doc, "text", &[("size", AttrValue::Length(20.0))]);

        let (_, text, diagnostics) = resolve_styles(&doc);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!((text.size_pt - 20.0).abs() < 0.01);
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
}
