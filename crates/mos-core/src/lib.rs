//! Core types for the Mosaic typesetting engine.
//!
//! Implements the document model (manifest §5) and diagnostics surface
//! (manifest §31). Every other crate depends on this one; nothing here
//! depends on parsing, layout, or backends.

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Stable identifier for a document node.
///
/// Per manifest §5.1, IDs should ideally be derived from
/// `hash(file path + syntactic position + explicit label + local structure)`
/// rather than parse order. The MVP 0 lowerer (`mos-eval`) hands out
/// monotonic IDs through `Document::alloc`; the hash-based derivation is
/// deferred to MVP 5 when stable IDs become observable through the cache.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct NodeId(pub u64);

/// Opaque content / dependency hash.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct ContentHash(pub u128);

/// Identifier for a resolved style bundle.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct StyleId(pub u32);

/// The kinds of nodes Mosaic recognises (manifest §5.1).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum NodeKind {
    Document,
    Section,
    Paragraph,
    Text,
    Emphasis,
    Strong,
    Math,
    Equation,
    /// A captioned container — an image plus a caption paragraph, laid
    /// out together with the caption beneath. Cross-references via
    /// `@fig:foo` will target this kind once MVP 3 lands.
    Figure,
    /// A raster image (PNG / JPEG in MVP 1.5). The decoded pixel data
    /// and natural dimensions live on the node's attributes; see the
    /// `mos-eval` resolver for the exact attribute names.
    Image,
    Table,
    Citation,
    Reference,
    Theorem,
    Footnote,
    Bibliography,
    Raw,
    /// A bullet or numbered list. The `ordered` attribute distinguishes
    /// the two kinds and child nodes are [`NodeKind::ListItem`]s.
    List,
    /// One entry inside a [`NodeKind::List`]. Inline children carry the
    /// item's text; nested [`NodeKind::List`] children describe deeper
    /// levels.
    ListItem,
}

/// A semantic document node (manifest §5.1).
#[derive(Clone, Debug)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub span: SourceSpan,
    pub content_hash: ContentHash,
    pub style_id: StyleId,
    pub children: Vec<NodeId>,
    pub attributes: AttrMap,
}

/// Attribute map carried on each node. Keys are interned strings in a
/// later iteration; for now plain `String` keys are fine for the stub.
pub type AttrMap = BTreeMap<String, AttrValue>;

#[derive(Clone, Debug, PartialEq)]
pub enum AttrValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<Self>),
    /// A length already resolved to PDF points. The parser carries
    /// unit-tagged literals (`mm`, `pt`, `em`); the lowerer converts
    /// them to a single canonical scalar so layout never has to know
    /// about units.
    Length(f64),
    /// Opaque binary payload — currently used to carry decoded raster
    /// image pixels (RGB8) onto an [`NodeKind::Image`] node so the PDF
    /// backend can emit them as an Image `XObject` without re-reading the
    /// source file.
    ///
    /// Stored as `Arc<[u8]>` so a node carrying decoded pixels is cheap
    /// to clone (e.g. across cache boundaries or when the same image
    /// would otherwise be duplicated through the document graph). The
    /// layout engine still dedups by resolved path, so most documents
    /// hold one buffer per image regardless; the `Arc` is insurance
    /// against accidental copies on the eval → layout boundary.
    Bytes(Arc<[u8]>),
}

/// A byte-range location in a source file (manifest §6 stage 1).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    pub file: PathBuf,
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    /// Construct a span covering `start..end` in `file`.
    #[must_use]
    pub fn new(file: PathBuf, start: usize, end: usize) -> Self {
        Self { file, start, end }
    }

    /// A zero-length placeholder span anchored at the start of `file`.
    #[must_use]
    pub fn placeholder(file: PathBuf) -> Self {
        Self {
            file,
            start: 0,
            end: 0,
        }
    }
}

/// Diagnostic severity (manifest §31).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Severity {
    Error,
    Warning,
    Note,
    Help,
}

/// Stable diagnostic code (e.g. `E041`, `W203`, manifest §16).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct DiagnosticCode(pub &'static str);

#[derive(Clone, Debug)]
pub struct DiagnosticNote {
    pub message: String,
    pub span: Option<SourceSpan>,
}

#[derive(Clone, Debug)]
pub struct Suggestion {
    pub message: String,
    pub replacement: Option<String>,
    pub span: Option<SourceSpan>,
}

/// A user-facing diagnostic (manifest §16, §31).
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
    pub span: Option<SourceSpan>,
    pub notes: Vec<DiagnosticNote>,
    pub suggestions: Vec<Suggestion>,
}

impl Diagnostic {
    pub fn error(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            span: None,
            notes: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code.0, self.message)
    }
}

/// Convert a byte offset into a 1-based `(line, column)` pair.
///
/// `src` is treated as UTF-8; columns are counted in *Unicode scalar
/// values* (i.e. `char`s), not bytes, so a span pointing at the byte
/// after `µ` reports column 2 rather than 3. Both the returned line
/// and column are at least 1, and offsets past the end of `src` are
/// clamped to the end. Offsets that fall in the middle of a UTF-8
/// code-point round down to the start of that code-point.
#[must_use]
pub fn linecol(src: &str, byte_offset: usize) -> (usize, usize) {
    let mut clamped = byte_offset.min(src.len());
    while clamped > 0 && !src.is_char_boundary(clamped) {
        clamped -= 1;
    }
    let mut line = 1_usize;
    let mut line_start = 0_usize;
    for (i, b) in src.as_bytes().iter().enumerate().take(clamped) {
        if *b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    let column = src[line_start..clamped].chars().count() + 1;
    (line, column)
}

impl std::error::Error for Diagnostic {}

/// Convenience top-level error type for crates that want a single
/// `Result` alias without inventing their own.
#[derive(thiserror::Error, Debug)]
pub enum CoreError {
    #[error("not yet implemented: {0}")]
    Unimplemented(&'static str),

    #[error(transparent)]
    Diagnostic(Box<Diagnostic>),
}

pub type Result<T> = std::result::Result<T, CoreError>;

/// The lowered semantic document graph (manifest §5, §6 stage 2).
///
/// Owns every [`Node`] and exposes them through their stable [`NodeId`].
/// MVP 0 stores nodes in insertion order; the manifest §5.1 hash-derived
/// IDs land alongside the cache work in MVP 5.
#[derive(Debug)]
pub struct Document {
    pub root: NodeId,
    pub file: PathBuf,
    nodes: BTreeMap<NodeId, Node>,
    next_id: u64,
}

impl Document {
    /// Create an empty document rooted at `file`. Allocates the
    /// `Document` root node (`NodeId(0)`) eagerly so callers can append
    /// children to it immediately.
    #[must_use]
    pub fn new(file: PathBuf) -> Self {
        let root_id = NodeId(0);
        let root_node = Node {
            id: root_id,
            kind: NodeKind::Document,
            span: SourceSpan::placeholder(file.clone()),
            content_hash: ContentHash::default(),
            style_id: StyleId::default(),
            children: Vec::new(),
            attributes: AttrMap::new(),
        };
        let mut nodes = BTreeMap::new();
        nodes.insert(root_id, root_node);
        Self {
            root: root_id,
            file,
            nodes,
            next_id: 1,
        }
    }

    /// Allocate `node` in the arena and return its assigned [`NodeId`].
    /// The `id` field on the input is overwritten with the fresh ID.
    pub fn alloc(&mut self, mut node: Node) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        node.id = id;
        self.nodes.insert(id, node);
        id
    }

    /// Allocate `node` as a child of `parent` and return its [`NodeId`].
    ///
    /// # Panics
    ///
    /// Panics if `parent` is not a node already allocated by this
    /// `Document`. Silently producing detached nodes would hide lowerer
    /// bugs in release builds, so this is intentionally a release-time
    /// assertion rather than a `debug_assert!`.
    pub fn alloc_child(&mut self, parent: NodeId, node: Node) -> NodeId {
        assert!(
            self.nodes.contains_key(&parent),
            "Document::alloc_child: unknown parent {parent:?}"
        );
        let child_id = self.alloc(node);
        // Safe to index: we just verified the key exists, and `alloc`
        // doesn't remove existing entries.
        if let Some(parent_node) = self.nodes.get_mut(&parent) {
            parent_node.children.push(child_id);
        }
        child_id
    }

    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Mutable accessor for a single node. Used by the resolver
    /// (manifest §6 stage 3) to back-patch attributes like `number`
    /// onto sections and `text` onto `@label` references.
    #[must_use]
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    /// Iterate over every node in the arena in insertion order.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    /// Total number of nodes including the document root.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        // The root always exists, so `Document` is never truly empty;
        // expose the conventional method anyway for clippy compliance.
        self.len() <= 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linecol_handles_ascii_offsets() {
        let src = "ab\ncd\nef";
        assert_eq!(linecol(src, 0), (1, 1));
        assert_eq!(linecol(src, 1), (1, 2));
        assert_eq!(linecol(src, 2), (1, 3));
        assert_eq!(linecol(src, 3), (2, 1));
        assert_eq!(linecol(src, 6), (3, 1));
        assert_eq!(linecol(src, 7), (3, 2));
        // Past the end clamps.
        assert_eq!(linecol(src, 9999), (3, 3));
    }

    #[test]
    fn linecol_counts_chars_not_bytes() {
        // `µ` is 2 bytes in UTF-8, `字` is 3 bytes. The column for the
        // byte after them should still be 2, not 3 / 4.
        let src = "µx\n字y\n";
        assert_eq!(linecol(src, 0), (1, 1));
        assert_eq!(linecol(src, 2), (1, 2)); // after `µ`
        assert_eq!(linecol(src, 3), (1, 3)); // after `µx`
        assert_eq!(linecol(src, 4), (2, 1)); // start of line 2
        assert_eq!(linecol(src, 7), (2, 2)); // after `字`
    }

    #[test]
    fn linecol_offsets_inside_codepoints_round_down() {
        // Pointing at the second byte of `µ` should still report
        // column 1 of line 1, not panic.
        let src = "µ";
        assert_eq!(linecol(src, 1), (1, 1));
    }

    #[test]
    #[should_panic(expected = "unknown parent")]
    fn alloc_child_panics_on_unknown_parent() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        // `NodeId(9999)` was never allocated by `doc`; the call must
        // abort instead of leaking a detached node.
        doc.alloc_child(
            NodeId(9999),
            Node {
                id: NodeId::default(),
                kind: NodeKind::Text,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: AttrMap::new(),
            },
        );
    }

    #[test]
    fn document_alloc_and_traverse() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let para = doc.alloc_child(
            doc.root,
            Node {
                id: NodeId::default(),
                kind: NodeKind::Paragraph,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: AttrMap::new(),
            },
        );
        doc.alloc_child(
            para,
            Node {
                id: NodeId::default(),
                kind: NodeKind::Text,
                span: SourceSpan::placeholder(PathBuf::from("test.mos")),
                content_hash: ContentHash::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes: AttrMap::new(),
            },
        );
        assert_eq!(doc.len(), 3);
        assert_eq!(doc.get(doc.root).unwrap().children.len(), 1);
        assert_eq!(doc.get(para).unwrap().children.len(), 1);
    }
}
