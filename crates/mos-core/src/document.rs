//! The lowered semantic document graph (manifest §5, §6 stage 2).
//!
//! [`Document`] owns every [`Node`] and hands them out through their stable
//! [`NodeId`]. Each node carries a [`NodeKind`], a [`SourceSpan`], a
//! [`ContentHash`], a [`StyleId`], and an [`AttrMap`] of [`AttrValue`]s.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::{ContentHash, SourceSpan};

/// Stable identifier for a document node.
///
/// Per manifest §5.1, IDs should ideally be derived from
/// `hash(file path + syntactic position + explicit label + local structure)`
/// rather than parse order. The MVP 0 lowerer (`mos-eval`) hands out
/// monotonic IDs through `Document::alloc`; the hash-based derivation is
/// deferred to MVP 5 when stable IDs become observable through the cache.
///
/// # Examples
///
/// ```
/// use mos_core::NodeId;
///
/// let root = NodeId(0);
///
/// assert_eq!(root.0, 0);
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct NodeId(pub u64);

/// Identifier for a resolved style bundle.
///
/// # Examples
///
/// ```
/// use mos_core::StyleId;
///
/// let style = StyleId::default();
///
/// assert_eq!(style.0, 0);
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct StyleId(pub u32);

/// The kinds of nodes Mosaic recognises (manifest §5.1).
///
/// # Examples
///
/// ```
/// use mos_core::NodeKind;
///
/// let kind = NodeKind::Paragraph;
///
/// assert_eq!(kind, NodeKind::Paragraph);
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum NodeKind {
    Document,
    Section,
    Paragraph,
    Text,
    Emphasis,
    Strong,
    BoldItalic,
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
    /// A `@page(label)` reference to the printed page number of a labelled
    /// target. Distinct from [`Reference`](Self::Reference) (which resolves to
    /// a section/figure number): a page reference resolves to where the target
    /// lands, which is only known after layout, via the resolve↔layout fixpoint
    /// (issue #72). Carries a `label` attribute and placeholder `text`; layout
    /// renders the `text` attribute like any inline run.
    PageReference,
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
    /// `\\` — a forced line break inside a paragraph. Carries no
    /// attributes; layout consumes it as a `WordItem::HardBreak`
    /// sentinel in the inline word stream. A blank-line paragraph
    /// break is **not** the same node — it ends the paragraph and
    /// triggers paragraph-spacing leading, whereas `HardBreak` keeps
    /// the same paragraph and applies normal inter-line leading.
    HardBreak,
}

/// A semantic document node (manifest §5.1).
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
///
/// use mos_core::{AttrMap, ContentHash, Node, NodeId, NodeKind, SourceSpan, StyleId};
///
/// let file = PathBuf::from("main.mos");
/// let node = Node {
///     id: NodeId(1),
///     kind: NodeKind::Paragraph,
///     span: SourceSpan::placeholder(file),
///     content_hash: ContentHash::default(),
///     style_id: StyleId::default(),
///     children: Vec::new(),
///     attributes: AttrMap::new(),
/// };
///
/// assert_eq!(node.kind, NodeKind::Paragraph);
/// ```
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

/// Attribute value carried on a semantic [`Node`].
///
/// # Examples
///
/// ```
/// use mos_core::AttrValue;
///
/// let value = AttrValue::Str("intro".to_owned());
///
/// assert_eq!(value, AttrValue::Str("intro".to_owned()));
/// ```
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

/// The lowered semantic document graph (manifest §5, §6 stage 2).
///
/// Owns every [`Node`] and exposes them through their stable [`NodeId`].
/// MVP 0 stores nodes in insertion order; the manifest §5.1 hash-derived
/// IDs land alongside the cache work in MVP 5.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
///
/// use mos_core::{Document, NodeId};
///
/// let doc = Document::new(PathBuf::from("main.mos"));
///
/// assert_eq!(doc.root, NodeId(0));
/// ```
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
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::Document;
    ///
    /// let doc = Document::new(PathBuf::from("main.mos"));
    ///
    /// assert_eq!(doc.len(), 1);
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::{AttrMap, ContentHash, Document, Node, NodeId, NodeKind, SourceSpan, StyleId};
    ///
    /// let file = PathBuf::from("main.mos");
    /// let mut doc = Document::new(file.clone());
    /// let id = doc.alloc(Node {
    ///     id: NodeId::default(),
    ///     kind: NodeKind::Paragraph,
    ///     span: SourceSpan::placeholder(file),
    ///     content_hash: ContentHash::default(),
    ///     style_id: StyleId::default(),
    ///     children: Vec::new(),
    ///     attributes: AttrMap::new(),
    /// });
    ///
    /// assert_eq!(id, NodeId(1));
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::{AttrMap, ContentHash, Document, Node, NodeId, NodeKind, SourceSpan, StyleId};
    ///
    /// let file = PathBuf::from("main.mos");
    /// let mut doc = Document::new(file.clone());
    /// let child = doc.alloc_child(doc.root, Node {
    ///     id: NodeId::default(),
    ///     kind: NodeKind::Paragraph,
    ///     span: SourceSpan::placeholder(file),
    ///     content_hash: ContentHash::default(),
    ///     style_id: StyleId::default(),
    ///     children: Vec::new(),
    ///     attributes: AttrMap::new(),
    /// });
    ///
    /// assert_eq!(doc.get(doc.root).map(|node| node.children.as_slice()), Some(&[child][..]));
    /// ```
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

    /// Get a node by id.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::{Document, NodeKind};
    ///
    /// let doc = Document::new(PathBuf::from("main.mos"));
    ///
    /// assert_eq!(doc.get(doc.root).map(|node| node.kind), Some(NodeKind::Document));
    /// ```
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Mutable accessor for a single node. Used by the resolver
    /// (manifest §6 stage 3) to back-patch attributes like `number`
    /// onto sections and `text` onto `@label` references.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::{AttrValue, Document};
    ///
    /// let mut doc = Document::new(PathBuf::from("main.mos"));
    /// if let Some(root) = doc.get_mut(doc.root) {
    ///     root.attributes.insert("title".to_owned(), AttrValue::Str("Demo".to_owned()));
    /// }
    ///
    /// assert!(doc.get(doc.root).is_some_and(|node| node.attributes.contains_key("title")));
    /// ```
    #[must_use]
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    /// Iterate over every node in the arena in insertion order.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::{Document, NodeKind};
    ///
    /// let doc = Document::new(PathBuf::from("main.mos"));
    /// let kinds: Vec<NodeKind> = doc.nodes().map(|node| node.kind).collect();
    ///
    /// assert_eq!(kinds, vec![NodeKind::Document]);
    /// ```
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    /// Total number of nodes including the document root.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::Document;
    ///
    /// let doc = Document::new(PathBuf::from("main.mos"));
    ///
    /// assert_eq!(doc.len(), 1);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Return whether the document has no semantic content beyond the root.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::Document;
    ///
    /// let doc = Document::new(PathBuf::from("main.mos"));
    ///
    /// assert!(doc.is_empty());
    /// ```
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
