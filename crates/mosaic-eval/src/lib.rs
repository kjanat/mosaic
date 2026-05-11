//! Expression and scripting evaluator (manifest §4, §25).
//!
//! The "evaluator" is really a *lowerer + resolver*: it walks a
//! [`SyntaxTree`] from `mosaic-parse` and builds the typed semantic
//! [`Document`] graph from `mosaic-core` (manifest §6 stage 2), then
//! runs the [`resolve`] pass to assign section numbers and rewrite
//! `@label` cross-references (§6 stage 3, MVP 1).

mod resolve;
mod set_schema;

use std::collections::BTreeMap;

use mosaic_core::{
    AttrMap, AttrValue, Diagnostic, DiagnosticCode, Document, Node, NodeId, NodeKind, Severity,
    StyleId,
};
use mosaic_parse::{Inline, InlineKind, Item, ListItem, SetArg, SetValue, SyntaxTree};

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
                Item::Set { name, args, span } => {
                    let mut attributes: AttrMap = BTreeMap::new();
                    attributes.insert("set".to_owned(), AttrValue::Str(name.clone()));
                    let Some(target) = set_schema::lookup_target(name) else {
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticCode("E020"),
                                format!(
                                    "unknown `#set` target `{name}` (expected `page`, `text`, or `document`)"
                                ),
                            )
                            .with_span(span.clone()),
                        );
                        // Still emit the Raw node so downstream stages
                        // can see the directive existed (useful for
                        // diagnostic spans pointing at it later).
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
                        continue;
                    };
                    for arg in args {
                        lower_set_arg(
                            target,
                            arg,
                            &mut attributes,
                            &mut metadata,
                            &mut current_text_size_pt,
                            &mut diagnostics,
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
            }
        }

        LowerResult {
            document,
            diagnostics,
            metadata,
        }
    }
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

/// Allocate a [`NodeKind::List`] under `parent` and recursively lower
/// its [`ListItem`]s into [`NodeKind::ListItem`] children. The
/// `ordered` flag is preserved as a `Bool` attribute so layout can pick
/// the right marker style without re-walking the tree.
fn lower_list(
    doc: &mut Document,
    parent: NodeId,
    ordered: bool,
    items: &[ListItem],
    span: &mosaic_core::SourceSpan,
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
