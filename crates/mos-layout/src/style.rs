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
