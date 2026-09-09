//! The lowered semantic document graph (manifest §5, §6 stage 2).
//!
//! [`Document`] owns every [`Node`] and hands them out through their stable
//! [`NodeId`]. Each node carries a [`NodeKind`], a [`SourceSpan`], a
//! [`ContentHash`], a [`StyleId`], and an [`AttrMap`] of [`AttrValue`]s.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::{ContentHash, ContentHasher, SourceSpan};

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
    /// A captioned container: an image plus a caption paragraph, laid
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
    /// One entry inside a [`NodeKind::List`]. Paragraph children carry item
    /// text; nested [`NodeKind::List`] children describe deeper levels.
    ListItem,
    /// `\\`: a forced line break inside a paragraph. Carries no
    /// attributes; layout consumes it as a `WordItem::HardBreak`
    /// sentinel in the inline word stream. A blank-line paragraph
    /// break is **not** the same node: it ends the paragraph and
    /// triggers paragraph-spacing leading, whereas `HardBreak` keeps
    /// the same paragraph and applies normal inter-line leading.
    HardBreak,
}

/// A semantic document node (manifest §5.1).
///
/// Nodes are allocated only by [`Document::alloc`] / [`Document::alloc_child`]
/// from a [`NodeSpec`]: the arena assigns the [`NodeId`] and owns the
/// `content_hash` and `style_id` fields. Those two fields are `pub(crate)`,
/// which makes the struct literal unconstructible outside this crate, so no
/// caller can fabricate a node with a fake id or a hand-set hash.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
///
/// use mos_core::{Document, NodeKind, NodeSpec, SourceSpan};
///
/// let file = PathBuf::from("main.mos");
/// let mut doc = Document::new(file.clone());
/// let id = doc.alloc(NodeSpec::new(NodeKind::Paragraph, SourceSpan::placeholder(file)));
///
/// assert_eq!(doc.get(id).map(|node| &node.kind), Some(&NodeKind::Paragraph));
/// ```
#[derive(Clone, Debug)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub span: SourceSpan,
    pub children: Vec<NodeId>,
    pub attributes: AttrMap,
    /// Authored subtree hash, computed by `Document::update_content_hashes`.
    /// Default until that pass runs. `pub(crate)` to seal external construction.
    pub(crate) content_hash: ContentHash,
    /// Resolved style slot placeholder; set by the arena, always default
    /// until styling lands. `pub(crate)` to seal external construction.
    pub(crate) style_id: StyleId,
}

impl Node {
    /// The authored subtree hash from the last
    /// [`Document::update_content_hashes`] pass, or default if never computed.
    ///
    /// This is a snapshot, not a live hash of public attributes. `mos-eval`
    /// computes it before resolution; later numbering and reference rewrites
    /// leave it intact. It is not a complete layout or artifact cache key.
    #[must_use]
    pub const fn content_hash(&self) -> ContentHash {
        self.content_hash
    }

    /// The node's resolved style slot: a placeholder, default until styling
    /// lands. Read-only: the arena owns this field.
    #[must_use]
    pub const fn style_id(&self) -> StyleId {
        self.style_id
    }
}

/// Blueprint for allocating a node in the document arena.
///
/// Carries only caller-chosen fields: `kind`, `span`, and `attributes`.
/// The arena supplies the `id`, empty `children`, and identity/style
/// placeholders.
#[derive(Clone, Debug)]
pub struct NodeSpec {
    pub kind: NodeKind,
    pub span: SourceSpan,
    pub attributes: AttrMap,
}

impl NodeSpec {
    /// A spec for a node of `kind` spanning `span`, with no attributes.
    #[must_use]
    pub const fn new(kind: NodeKind, span: SourceSpan) -> Self {
        Self {
            kind,
            span,
            attributes: AttrMap::new(),
        }
    }

    /// Attach `attributes` to this spec.
    #[must_use]
    pub fn with_attributes(mut self, attributes: AttrMap) -> Self {
        self.attributes = attributes;
        self
    }
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
    /// Opaque binary payload; currently used to carry decoded raster
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

    /// Allocate a node from `spec` in the arena and return its assigned
    /// [`NodeId`]. The arena fills in the id, an empty `children` list, and
    /// the default `content_hash`/`style_id` placeholders.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::{Document, NodeId, NodeKind, NodeSpec, SourceSpan};
    ///
    /// let file = PathBuf::from("main.mos");
    /// let mut doc = Document::new(file.clone());
    /// let id = doc.alloc(NodeSpec::new(NodeKind::Paragraph, SourceSpan::placeholder(file)));
    ///
    /// assert_eq!(id, NodeId(1));
    /// ```
    pub fn alloc(&mut self, spec: NodeSpec) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        self.nodes.insert(id, Self::node_from_spec(id, spec));
        id
    }

    /// Build the arena-owned [`Node`] for `id` from a caller's [`NodeSpec`],
    /// supplying the fields the caller does not control.
    fn node_from_spec(id: NodeId, spec: NodeSpec) -> Node {
        Node {
            id,
            kind: spec.kind,
            span: spec.span,
            children: Vec::new(),
            attributes: spec.attributes,
            content_hash: ContentHash::default(),
            style_id: StyleId::default(),
        }
    }

    /// Allocate a node from `spec` as a child of `parent` and return its
    /// [`NodeId`].
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
    /// use mos_core::{Document, NodeKind, NodeSpec, SourceSpan};
    ///
    /// let file = PathBuf::from("main.mos");
    /// let mut doc = Document::new(file.clone());
    /// let child = doc.alloc_child(doc.root, NodeSpec::new(NodeKind::Paragraph, SourceSpan::placeholder(file)));
    ///
    /// assert_eq!(doc.get(doc.root).map(|node| node.children.as_slice()), Some(&[child][..]));
    /// ```
    pub fn alloc_child(&mut self, parent: NodeId, spec: NodeSpec) -> NodeId {
        assert!(
            self.nodes.contains_key(&parent),
            "Document::alloc_child: unknown parent {parent:?}"
        );
        let child_id = self.alloc(spec);
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

    /// Compute each node's content hash from its own semantic inputs and its
    /// ordered child hashes. The caller supplies the hash of the node's kind,
    /// authored attributes, and external inputs; this crate owns graph traversal
    /// and subtree framing. IDs, spans, and previous hashes are not folded here.
    ///
    /// Hashes are snapshots: after changing authored inputs, the caller must
    /// run this pass again with the appropriate projection. Resolvers may keep
    /// the original hashes while adding derived attributes.
    ///
    /// # Panics
    ///
    /// Panics if children contain a cycle or refer to an unallocated node.
    /// These are document-construction bugs, not source-document errors.
    pub fn update_content_hashes(&mut self, mut own_content: impl FnMut(&Node) -> ContentHash) {
        let mut hashes = BTreeMap::<NodeId, ContentHash>::new();
        let mut active = BTreeSet::new();
        // Iterative postorder also handles shared children, detached nodes,
        // and children allocated before their parent without recursion.
        for &id in self.nodes.keys() {
            if hashes.contains_key(&id) {
                continue;
            }
            let mut pending = vec![(id, false)];
            while let Some((id, exiting)) = pending.pop() {
                if hashes.contains_key(&id) {
                    continue;
                }
                let node = &self.nodes[&id];
                if exiting {
                    let mut hasher = ContentHasher::new();
                    hasher
                        .field(b"mos-core/semantic-subtree/v1")
                        .field(&own_content(node).0.to_le_bytes());
                    for child in &node.children {
                        hasher.field(&hashes[child].0.to_le_bytes());
                    }
                    hashes.insert(id, hasher.finish());
                    active.remove(&id);
                } else {
                    assert!(
                        active.insert(id),
                        "Document::update_content_hashes: cycle at {id:?}"
                    );
                    pending.push((id, true));
                    pending.extend(node.children.iter().rev().map(|&child| (child, false)));
                }
            }
        }
        for (id, node) in &mut self.nodes {
            node.content_hash = hashes[id];
        }
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
    fn content_hashes_follow_child_order_and_ignore_allocation_order() {
        fn graph(reverse_allocation: bool) -> (Document, NodeId, NodeId) {
            let mut document = Document::new(PathBuf::from("test.mos"));
            let spec = || {
                NodeSpec::new(
                    NodeKind::Text,
                    SourceSpan::placeholder(document.file.clone()),
                )
            };
            let a_spec = spec();
            let b_spec = spec();
            let (a, b) = if reverse_allocation {
                let b = document.alloc(b_spec);
                (document.alloc(a_spec), b)
            } else {
                (document.alloc(a_spec), document.alloc(b_spec))
            };
            document.get_mut(document.root).unwrap().children = vec![a, b, a];
            (document, a, b)
        }
        fn hash(document: &mut Document, a: NodeId, b: NodeId) {
            document.update_content_hashes(|node| {
                ContentHasher::new()
                    .field(if node.id == a {
                        b"a"
                    } else if node.id == b {
                        b"b"
                    } else {
                        b"root"
                    })
                    .finish()
            });
        }
        let (mut first, a, b) = graph(false);
        let (mut second, other_a, other_b) = graph(true);
        hash(&mut first, a, b);
        hash(&mut second, other_a, other_b);
        let original = first.get(first.root).unwrap().content_hash();
        assert_ne!(original, ContentHash::default());
        assert_eq!(original, second.get(second.root).unwrap().content_hash());
        let child_hash = first.get(a).unwrap().content_hash();
        first.get_mut(first.root).unwrap().children.swap(0, 1);
        hash(&mut first, a, b);
        assert_ne!(original, first.get(first.root).unwrap().content_hash());
        assert_eq!(child_hash, first.get(a).unwrap().content_hash());
    }

    #[test]
    fn content_hashes_support_older_children_and_deep_graphs() {
        let mut document = Document::new(PathBuf::from("test.mos"));
        let mut child = document.alloc(NodeSpec::new(
            NodeKind::Text,
            SourceSpan::placeholder(document.file.clone()),
        ));
        for _ in 0..4_000 {
            let parent = document.alloc(NodeSpec::new(
                NodeKind::Paragraph,
                SourceSpan::placeholder(document.file.clone()),
            ));
            document.get_mut(parent).unwrap().children.push(child);
            child = parent;
        }
        document
            .get_mut(document.root)
            .unwrap()
            .children
            .push(child);
        let mut visits = BTreeSet::new();
        document.update_content_hashes(|node| {
            assert!(visits.insert(node.id), "each node hashed only once");
            ContentHash(1)
        });
        assert_eq!(visits.len(), document.len());
        assert!(
            document
                .nodes()
                .all(|node| node.content_hash() != ContentHash::default())
        );
    }

    #[test]
    #[should_panic(expected = "cycle")]
    fn content_hashes_reject_cycles() {
        let mut document = Document::new(PathBuf::from("test.mos"));
        let root = document.root;
        document.get_mut(root).unwrap().children.push(root);
        document.update_content_hashes(|_| ContentHash(1));
    }

    #[test]
    #[should_panic]
    fn content_hashes_reject_missing_children() {
        let mut document = Document::new(PathBuf::from("test.mos"));
        document
            .get_mut(document.root)
            .unwrap()
            .children
            .push(NodeId(99));
        document.update_content_hashes(|_| ContentHash(1));
    }

    #[test]
    #[should_panic(expected = "unknown parent")]
    fn alloc_child_panics_on_unknown_parent() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        // `NodeId(9999)` was never allocated by `doc`; the call must
        // abort instead of leaking a detached node.
        doc.alloc_child(
            NodeId(9999),
            NodeSpec::new(
                NodeKind::Text,
                SourceSpan::placeholder(PathBuf::from("test.mos")),
            ),
        );
    }

    #[test]
    fn document_alloc_and_traverse() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let para = doc.alloc_child(
            doc.root,
            NodeSpec::new(
                NodeKind::Paragraph,
                SourceSpan::placeholder(PathBuf::from("test.mos")),
            ),
        );
        doc.alloc_child(
            para,
            NodeSpec::new(
                NodeKind::Text,
                SourceSpan::placeholder(PathBuf::from("test.mos")),
            ),
        );
        assert_eq!(doc.len(), 3);
        assert_eq!(doc.get(doc.root).unwrap().children.len(), 1);
        assert_eq!(doc.get(para).unwrap().children.len(), 1);
    }
}
