//! Lower `#bibliography(...)` directives into a semantic node.
//!
//! This is the *source boundary* only (manifest §4, MVP 4): the directive
//! declares one bibliography database path, which is resolved relative to
//! the current `.mos` source file and stashed on a
//! [`NodeKind::Bibliography`] node so a later BibTeX-parsing slice can read
//! it. Parsing `.bib` contents, resolving citation keys, and rendering the
//! bibliography are explicitly out of scope here.
//!
//! Diagnostics:
//!
//! - `MOS0040`: `#bibliography(...)` called without a (non-empty) path.
//! - `MOS0020`: the path argument is present but not a string.
//! - `MOS0015`: an unknown keyword argument was supplied.
//! - `MOS0041`: the resolved path does not point to a file on disk
//!   (a warning — the node is still emitted with its resolved path).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mos_core::{
    AttrMap, AttrValue, Diagnostic, Document, Node, NodeId, NodeKind, SourceSpan, StyleId, codes,
};
use mos_parse::{SetArg, SetValue};

/// Lower a top-level `#bibliography("refs.bib")` directive into a single
/// [`NodeKind::Bibliography`] node hanging off the document root. The
/// literal path is recorded under `src`; the path resolved against the
/// source file's directory is recorded under `resolved_path` so the
/// later BibTeX reader can open the database without re-deriving the
/// location.
pub(super) fn lower_bibliography_directive(
    document: &mut Document,
    root: NodeId,
    args: &[SetArg],
    span: &SourceSpan,
    source_file: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(path) = bibliography_path(args, span, diagnostics) else {
        return;
    };
    let resolved = resolve_path(&path, source_file);
    // The directive only *declares* the source in this slice, so a missing
    // file is a non-fatal warning rather than the hard error `#image(...)`
    // raises: the node is still emitted with its resolved path, and the
    // BibTeX-reading slice surfaces a read/parse error when it opens the
    // database for real.
    if !resolved.is_file() {
        diagnostics.push(
            Diagnostic::simple(
                &codes::MOS0041,
                None,
                format!(
                    "declared bibliography source `{}` was not found",
                    resolved.display()
                ),
            )
            .with_span(span.clone()),
        );
    }
    let mut attributes: AttrMap = BTreeMap::new();
    attributes.insert("src".to_owned(), AttrValue::Str(path));
    attributes.insert(
        "resolved_path".to_owned(),
        AttrValue::Str(resolved.to_string_lossy().into_owned()),
    );
    document.alloc_child(
        root,
        Node {
            id: NodeId::default(),
            kind: NodeKind::Bibliography,
            span: span.clone(),
            content_hash: Default::default(),
            style_id: StyleId::default(),
            children: Vec::new(),
            attributes,
        },
    );
}

/// Pull the single source path out of the directive arguments. A leading
/// positional string (`#bibliography("refs.bib")`) or the named
/// `path:`/`src:` forms are accepted, mirroring `#image(...)`. Returns
/// `None` (after emitting a diagnostic) when the path is missing, empty,
/// or not a string.
fn bibliography_path(
    args: &[SetArg],
    span: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    let mut path: Option<String> = None;
    for arg in args {
        match arg {
            // Positional first arg -- the path literal, same shorthand
            // `#image("path.png")` accepts.
            SetArg::Positional { value, value_span } => match value {
                SetValue::Str(s) => path = Some(s.clone()),
                _ => diagnostics.push(
                    Diagnostic::simple(
                        &codes::MOS0020,
                        None,
                        "`#bibliography(...)` expects a string path",
                    )
                    .with_span(value_span.clone()),
                ),
            },
            SetArg::Named {
                key,
                value,
                key_span,
                value_span,
            } => match key.as_str() {
                "src" | "path" => match value {
                    SetValue::Str(s) => path = Some(s.clone()),
                    _ => diagnostics.push(
                        Diagnostic::simple(
                            &codes::MOS0020,
                            None,
                            "`#bibliography(...)` expects a string path",
                        )
                        .with_span(value_span.clone()),
                    ),
                },
                _ => diagnostics.push(
                    Diagnostic::simple(
                        &codes::MOS0015,
                        None,
                        format!("unknown argument `{key}` for `#bibliography` (valid: src/path)"),
                    )
                    .with_span(key_span.clone()),
                ),
            },
        }
    }
    let Some(path) = path else {
        diagnostics.push(
            Diagnostic::simple(
                &codes::MOS0040,
                None,
                "`#bibliography(...)` requires a path (e.g. `#bibliography(\"refs.bib\")`)",
            )
            .with_span(span.clone()),
        );
        return None;
    };
    // A bare empty / whitespace-only path is the same mistake as omitting
    // it -- they wrote `#bibliography("")` and meant to fill in a filename.
    if path.trim().is_empty() {
        diagnostics.push(
            Diagnostic::simple(
                &codes::MOS0040,
                None,
                "`#bibliography(...)` requires a non-empty path (e.g. `#bibliography(\"refs.bib\")`)",
            )
            .with_span(span.clone()),
        );
        return None;
    }
    Some(path)
}

/// Resolve `src_path` (as written in the source) relative to the `.mos`
/// file currently being lowered. Mirrors the resolver in [`crate::image`];
/// absolute paths pass through untouched, and a parentless source file
/// (e.g. a bare `main.mos`) leaves the path relative to the cwd.
fn resolve_path(src_path: &str, source_file: &Path) -> PathBuf {
    let candidate = PathBuf::from(src_path);
    if candidate.is_absolute() {
        return candidate;
    }
    if let Some(parent) = source_file.parent()
        && !parent.as_os_str().is_empty()
    {
        return parent.join(candidate);
    }
    candidate
}
