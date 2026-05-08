//! Core types for the Mosaic typesetting engine.
//!
//! Implements the document model (manifest §5) and diagnostics surface
//! (manifest §31). Every other crate depends on this one; nothing here
//! depends on parsing, layout, or backends.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Stable identifier for a document node.
///
/// Per manifest §5.1, IDs should ideally be derived from
/// `hash(file path + syntactic position + explicit label + local structure)`
/// rather than parse order. The MVP 0 lowerer (`mosaic-eval`) hands out
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
    Figure,
    Table,
    Citation,
    Reference,
    Theorem,
    Footnote,
    Bibliography,
    Raw,
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

#[derive(Clone, Debug)]
pub enum AttrValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<Self>),
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
/// `src` is treated as UTF-8; columns are counted in bytes. Both the
/// returned line and column are clamped to a minimum of 1, and offsets
/// past the end of `src` map to the final line.
#[must_use]
pub fn linecol(src: &str, byte_offset: usize) -> (usize, usize) {
    let clamped = byte_offset.min(src.len());
    let mut line = 1_usize;
    let mut last_newline: Option<usize> = None;
    for (i, b) in src.as_bytes().iter().enumerate().take(clamped) {
        if *b == b'\n' {
            line += 1;
            last_newline = Some(i);
        }
    }
    let col_start = last_newline.map_or(0, |i| i + 1);
    let column = (clamped - col_start) + 1;
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
    /// `parent` must come from this `Document`; if it does not, the new
    /// node is still allocated but ends up detached from the tree.
    /// Internal callers (the lowerer) preserve this invariant.
    pub fn alloc_child(&mut self, parent: NodeId, node: Node) -> NodeId {
        let child_id = self.alloc(node);
        if let Some(parent_node) = self.nodes.get_mut(&parent) {
            parent_node.children.push(child_id);
        } else {
            debug_assert!(false, "Document::alloc_child: unknown parent {parent:?}");
        }
        child_id
    }

    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
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
    fn linecol_handles_offsets() {
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
