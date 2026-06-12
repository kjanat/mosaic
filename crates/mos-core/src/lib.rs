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

pub mod codes;
mod hash;
mod sink;

pub use codes::{DiagnosticCategory, DiagnosticCode, DiagnosticDef};
pub use hash::ContentHasher;
pub use sink::{CollectingSink, DiagnosticAbort, DiagnosticResult, DiagnosticSink};

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

/// Opaque content / dependency hash.
///
/// # Examples
///
/// ```
/// use mos_core::ContentHash;
///
/// let hash = ContentHash::default();
///
/// assert_eq!(hash.0, 0);
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct ContentHash(pub u128);

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

/// A byte-range location in a source file (manifest §6 stage 1).
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
///
/// use mos_core::SourceSpan;
///
/// let span = SourceSpan::new(PathBuf::from("main.mos"), 2, 8);
///
/// assert_eq!(span.start, 2);
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    pub file: PathBuf,
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    /// Construct a span covering `start..end` in `file`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::SourceSpan;
    ///
    /// let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 9);
    ///
    /// assert_eq!(span.end, 9);
    /// ```
    #[must_use]
    pub fn new(file: PathBuf, start: usize, end: usize) -> Self {
        Self { file, start, end }
    }

    /// A zero-length placeholder span anchored at the start of `file`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::SourceSpan;
    ///
    /// let span = SourceSpan::placeholder(PathBuf::from("main.mos"));
    ///
    /// assert_eq!((span.start, span.end), (0, 0));
    /// ```
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
///
/// Three runtime severities. `Error` marks a *failing* diagnostic (the CLI
/// exits non-zero at the next phase barrier) — it does **not** mean "abort
/// the phase right now". `Notice` is informational and non-failing
/// (substitutions, auto-decisions). Sub-message kinds (`note`/`help`/
/// `hint`) live on [`DiagnosticAnnotation`], never here.
///
/// # Examples
///
/// ```
/// use mos_core::Severity;
///
/// assert_ne!(Severity::Error, Severity::Notice);
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Severity {
    /// Failing diagnostic; non-zero exit at the next phase barrier.
    Error,
    /// Surfaced, but the build continues.
    Warning,
    /// Informational only; the build continues.
    Notice,
}

/// A sub-message attached to a [`Diagnostic`].
///
/// The diagnostic's *primary* span lives on [`Diagnostic::span`]; these are
/// only secondary spans (`Related`) and textual rows. There is intentionally
/// no `Primary` variant — that would be a second home for the primary span.
///
/// # Examples
///
/// ```
/// use mos_core::DiagnosticAnnotation;
///
/// let help = DiagnosticAnnotation::Help("try `#set text(...)`".to_owned());
/// assert!(matches!(help, DiagnosticAnnotation::Help(_)));
/// ```
#[derive(Clone, Debug)]
pub enum DiagnosticAnnotation {
    /// Another source location that helps explain the primary cause
    /// (e.g. the first declaration of a duplicated label).
    Related {
        /// Where the related span points.
        span: SourceSpan,
        /// What that location contributes.
        message: String,
    },
    /// Attached explanation, rendered as `note:`.
    Note(String),
    /// Attached suggestion, rendered as `help:`.
    Help(String),
    /// Attached hint, rendered as `hint:`.
    Hint(String),
}

/// A machine-actionable fix for a [`Diagnostic`].
///
/// A `Suggestion` says "replace the bytes at this [`SourceSpan`] with this
/// text" — it is structured data a tool can apply automatically, as opposed
/// to the prose advice carried by [`DiagnosticAnnotation::Help`]. Backends
/// consume it without re-parsing: the CLI can print a fix-it diff and an LSP
/// can surface it as a code action keyed on the same span.
///
/// Two edge cases fall out of the replace-the-span model and are intentional:
///
/// - an empty `replacement` **deletes** the bytes covered by `span`;
/// - a zero-length `span` (`start == end`) **inserts** `replacement` at that
///   offset without removing anything.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
///
/// use mos_core::{SourceSpan, Suggestion};
///
/// let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
/// let fix = Suggestion::new(span, "@intro");
///
/// assert_eq!(fix.replacement, "@intro");
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Suggestion {
    /// The source range the fix replaces. A zero-length span
    /// (`start == end`) marks a pure insertion point.
    pub span: SourceSpan,
    /// The text to substitute for the bytes covered by `span`. An empty
    /// string deletes that range.
    pub replacement: String,
}

impl Suggestion {
    /// Construct a suggestion replacing `span` with `replacement`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::{SourceSpan, Suggestion};
    ///
    /// let span = SourceSpan::new(PathBuf::from("main.mos"), 0, 3);
    /// let fix = Suggestion::new(span, "set".to_owned());
    ///
    /// assert_eq!(fix.span.start, 0);
    /// ```
    #[must_use]
    pub fn new(span: SourceSpan, replacement: impl Into<String>) -> Self {
        Self {
            span,
            replacement: replacement.into(),
        }
    }
}

/// A user-facing diagnostic (manifest §16, §31).
///
/// Identity and default severity come from a `'static` [`DiagnosticDef`] in
/// [`codes`]; the instance carries the *resolved* severity (today always the
/// def's default, later a config override) so rendering never has to consult
/// the def. Fields are private — construct via [`Diagnostic::simple`] or
/// [`Diagnostic::new`].
///
/// # Examples
///
/// ```
/// use mos_core::{Diagnostic, Severity, codes};
///
/// let diagnostic = Diagnostic::simple(&codes::MOS0010, None, "boom");
///
/// assert_eq!(diagnostic.severity(), Severity::Error);
/// assert_eq!(diagnostic.def().code().to_string(), "MOS0010");
/// ```
#[derive(Clone, Debug)]
pub struct Diagnostic {
    def: &'static DiagnosticDef,
    severity: Severity,
    span: Option<SourceSpan>,
    message: String,
    annotations: Vec<DiagnosticAnnotation>,
    suggestions: Vec<Suggestion>,
}

impl Diagnostic {
    /// Full constructor: the caller supplies the resolved severity. The
    /// future config resolver uses this; nothing has to crack open the
    /// struct.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, Severity, codes};
    ///
    /// // Promote a warning-by-default code to an error.
    /// let d = Diagnostic::new(&codes::MOS0028, Severity::Error, None, "promoted");
    /// assert_eq!(d.severity(), Severity::Error);
    /// ```
    pub fn new(
        def: &'static DiagnosticDef,
        severity: Severity,
        span: Option<SourceSpan>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            def,
            severity,
            span,
            message: message.into(),
            annotations: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    /// Convenience: severity defaults to `def.default_severity()`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, Severity, codes};
    ///
    /// let d = Diagnostic::simple(&codes::MOS0018, None, "substituted Noto Sans");
    /// assert_eq!(d.severity(), Severity::Notice);
    /// ```
    pub fn simple(
        def: &'static DiagnosticDef,
        span: Option<SourceSpan>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(def, def.default_severity(), span, message)
    }

    /// Attach a sub-message annotation, builder-style.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, DiagnosticAnnotation, codes};
    ///
    /// let d = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
    ///     .with_annotation(DiagnosticAnnotation::Help("did you mean `@intro`?".to_owned()));
    /// assert_eq!(d.annotations().len(), 1);
    /// ```
    #[must_use]
    pub fn with_annotation(mut self, annotation: DiagnosticAnnotation) -> Self {
        self.annotations.push(annotation);
        self
    }

    /// Attach a machine-actionable [`Suggestion`], builder-style.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::{Diagnostic, SourceSpan, Suggestion, codes};
    ///
    /// let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
    /// let d = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
    ///     .with_suggestion(Suggestion::new(span, "@intro"));
    /// assert_eq!(d.suggestions().len(), 1);
    /// ```
    #[must_use]
    pub fn with_suggestion(mut self, suggestion: Suggestion) -> Self {
        self.suggestions.push(suggestion);
        self
    }

    /// Attach a span, builder-style.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::{Diagnostic, SourceSpan, codes};
    ///
    /// let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
    /// let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
    ///     .with_span(span.clone());
    ///
    /// assert_eq!(diagnostic.span(), Some(&span));
    /// ```
    #[must_use]
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// The registry definition behind this diagnostic.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, codes};
    ///
    /// let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label");
    ///
    /// assert_eq!(diagnostic.def().code(), codes::MOS0033.code());
    /// ```
    #[must_use]
    pub fn def(&self) -> &'static DiagnosticDef {
        self.def
    }

    /// The resolved severity carried by this instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, Severity, codes};
    ///
    /// let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label");
    ///
    /// assert_eq!(diagnostic.severity(), Severity::Error);
    /// ```
    #[must_use]
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// The primary span, if any.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, codes};
    ///
    /// let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label");
    ///
    /// assert!(diagnostic.span().is_none());
    /// ```
    #[must_use]
    pub fn span(&self) -> Option<&SourceSpan> {
        self.span.as_ref()
    }

    /// The primary message.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, codes};
    ///
    /// let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label");
    ///
    /// assert_eq!(diagnostic.message(), "unknown label");
    /// ```
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The attached sub-message annotations, in attach order.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, DiagnosticAnnotation, codes};
    ///
    /// let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
    ///     .with_annotation(DiagnosticAnnotation::Help("declare `<intro>` first".to_owned()));
    ///
    /// assert_eq!(diagnostic.annotations().len(), 1);
    /// ```
    #[must_use]
    pub fn annotations(&self) -> &[DiagnosticAnnotation] {
        &self.annotations
    }

    /// The attached machine-actionable suggestions, in attach order.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::{Diagnostic, SourceSpan, Suggestion, codes};
    ///
    /// let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
    /// let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
    ///     .with_suggestion(Suggestion::new(span, "@intro"));
    ///
    /// assert_eq!(diagnostic.suggestions().len(), 1);
    /// ```
    #[must_use]
    pub fn suggestions(&self) -> &[Suggestion] {
        &self.suggestions
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.def.code(), self.message)
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
///
/// # Examples
///
/// ```
/// use mos_core::linecol;
///
/// assert_eq!(linecol("a\nb", 2), (2, 1));
/// ```
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
///
/// # Examples
///
/// ```
/// use mos_core::CoreError;
///
/// let err = CoreError::Unimplemented("cache");
///
/// assert_eq!(err.to_string(), "not yet implemented: cache");
/// ```
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

    #[test]
    fn suggestion_new_sets_span_and_replacement() {
        let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
        let suggestion = Suggestion::new(span.clone(), "@intro");
        assert_eq!(suggestion.span, span);
        assert_eq!(suggestion.replacement, "@intro");
    }

    #[test]
    fn diagnostic_has_no_suggestions_by_default() {
        let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label");
        assert!(diagnostic.suggestions().is_empty());
    }

    #[test]
    fn with_suggestion_accumulates_in_order() {
        let first = Suggestion::new(SourceSpan::new(PathBuf::from("main.mos"), 4, 10), "@intro");
        let second = Suggestion::new(
            SourceSpan::new(PathBuf::from("other.mos"), 12, 15),
            "@summary",
        );
        let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
            .with_suggestion(first)
            .with_suggestion(second);

        let suggestions = diagnostic.suggestions();
        assert_eq!(suggestions.len(), 2);

        assert_eq!(suggestions[0].span.file, PathBuf::from("main.mos"));
        assert_eq!(suggestions[0].span.start, 4);
        assert_eq!(suggestions[0].span.end, 10);
        assert_eq!(suggestions[0].replacement, "@intro");

        assert_eq!(suggestions[1].span.file, PathBuf::from("other.mos"));
        assert_eq!(suggestions[1].span.start, 12);
        assert_eq!(suggestions[1].span.end, 15);
        assert_eq!(suggestions[1].replacement, "@summary");
    }

    #[test]
    fn suggestion_new_accepts_str_and_owned_string() {
        let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
        let from_str = Suggestion::new(span.clone(), "@intro");
        let from_string = Suggestion::new(span, String::from("@intro"));
        assert_eq!(from_str, from_string);
    }

    #[test]
    fn suggestion_clone_and_equality() {
        let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
        let suggestion = Suggestion::new(span.clone(), "@intro");

        // A clone equals its original.
        assert_eq!(suggestion.clone(), suggestion);
        // Built independently from the same parts => equal.
        assert_eq!(Suggestion::new(span.clone(), "@intro"), suggestion);
        // Differing replacement text => unequal.
        assert_ne!(Suggestion::new(span, "@outro"), suggestion);
        // Differing span => unequal.
        let wider = SourceSpan::new(PathBuf::from("main.mos"), 4, 11);
        assert_ne!(Suggestion::new(wider, "@intro"), suggestion);
    }

    #[test]
    fn suggestion_empty_replacement_encodes_deletion() {
        let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
        let deletion = Suggestion::new(span, "");
        assert!(deletion.replacement.is_empty());
        // A deletion still covers a real, non-empty range.
        assert!(deletion.span.start < deletion.span.end);
    }

    #[test]
    fn suggestion_zero_length_span_encodes_insertion() {
        let point = SourceSpan::new(PathBuf::from("main.mos"), 7, 7);
        let insertion = Suggestion::new(point, "@intro");
        assert_eq!(insertion.span.start, insertion.span.end);
        assert_eq!(insertion.replacement, "@intro");
    }

    #[test]
    fn suggestions_and_annotations_are_independent_channels() {
        let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);

        // A suggestion does not leak into the annotation channel.
        let with_fix = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
            .with_suggestion(Suggestion::new(span.clone(), "@intro"));
        assert_eq!(with_fix.suggestions().len(), 1);
        assert!(with_fix.annotations().is_empty());

        // Prose help does not leak into the suggestion channel.
        let with_help = Diagnostic::simple(&codes::MOS0033, None, "unknown label").with_annotation(
            DiagnosticAnnotation::Help("did you mean `@intro`?".to_owned()),
        );
        assert_eq!(with_help.annotations().len(), 1);
        assert!(with_help.suggestions().is_empty());

        // Both channels populate independently and keep their own payloads.
        let with_both = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
            .with_annotation(DiagnosticAnnotation::Help(
                "did you mean `@intro`?".to_owned(),
            ))
            .with_suggestion(Suggestion::new(span, "@intro"));
        assert_eq!(with_both.suggestions().len(), 1);
        assert_eq!(with_both.annotations().len(), 1);
        assert_eq!(with_both.suggestions()[0].replacement, "@intro");

        // The existing Help annotation is carried through unchanged.
        let help_text = match &with_both.annotations()[0] {
            DiagnosticAnnotation::Help(text) => Some(text.as_str()),
            _ => None,
        };
        assert_eq!(help_text, Some("did you mean `@intro`?"));
    }
}
