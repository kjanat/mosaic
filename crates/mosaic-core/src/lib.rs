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
/// rather than parse order. This type is opaque so the derivation can
/// change without touching call sites.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
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
    pub id:           NodeId,
    pub kind:         NodeKind,
    pub span:         SourceSpan,
    pub content_hash: ContentHash,
    pub style_id:     StyleId,
    pub children:     Vec<NodeId>,
    pub attributes:   AttrMap,
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
    pub file:  PathBuf,
    pub start: usize,
    pub end:   usize,
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
    pub span:    Option<SourceSpan>,
}

#[derive(Clone, Debug)]
pub struct Suggestion {
    pub message:     String,
    pub replacement: Option<String>,
    pub span:        Option<SourceSpan>,
}

/// A user-facing diagnostic (manifest §16, §31).
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity:    Severity,
    pub code:        DiagnosticCode,
    pub message:     String,
    pub span:        Option<SourceSpan>,
    pub notes:       Vec<DiagnosticNote>,
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
}

/// Convenience top-level error type for crates that want a single
/// `Result` alias without inventing their own.
#[derive(thiserror::Error, Debug)]
pub enum CoreError {
    #[error("not yet implemented: {0}")]
    Unimplemented(&'static str),

    #[error("{0}")]
    Diagnostic(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
