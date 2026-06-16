//! Core types for the Mosaic typesetting engine.
//!
//! Implements the document model (manifest §5) and diagnostics surface
//! (manifest §31). Every other crate depends on this one; nothing here
//! depends on parsing, layout, or backends.
//!
//! The implementation is split into focused modules and re-exported flat at
//! the crate root: consumers use `mos_core::Diagnostic`,
//! `mos_core::SourceSpan`, etc., never the module paths. Internally:
//!
//! - `document`; the lowered semantic node graph ([`Document`], [`Node`],
//!   [`NodeSpec`])
//! - `span`: source byte-ranges ([`SourceSpan`]) and [`linecol`]
//! - `diagnostics`: [`Diagnostic`], [`DiagnosticAnnotation`], [`Severity`],
//!   [`Suggestion`]
//! - [`codes`]; the `MOS####` diagnostic-code registry
//! - `sink`: diagnostic emission plumbing ([`DiagnosticSink`])
//! - `error`; the crate-level [`CoreError`] and `Result` alias
//! - `hash`: deterministic content hashing ([`ContentHash`], [`ContentHasher`])
//! - `path`: portable path helpers ([`display_path`], [`resolve_relative`])

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

pub mod codes;
mod diagnostics;
mod document;
mod error;
mod hash;
mod path;
mod sink;
mod span;

pub use codes::{DiagnosticCategory, DiagnosticCode, DiagnosticDef};
pub use diagnostics::{Diagnostic, DiagnosticAnnotation, Severity, Suggestion};
pub use document::{AttrMap, AttrValue, Document, Node, NodeId, NodeKind, NodeSpec, StyleId};
pub use error::{CoreError, Result};
pub use hash::{ContentHash, ContentHasher};
pub use path::{display_path, resolve_relative, resolve_source_path};
pub use sink::{CollectingSink, DiagnosticAbort, DiagnosticResult, DiagnosticSink};
pub use span::{SourceSpan, linecol};
