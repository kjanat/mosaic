//! Lower `#bibliography(...)` directives into a semantic node.
//!
//! This is the *source boundary* only (manifest §4, MVP 4): the directive
//! declares one bibliography database path, which is resolved relative to
//! the current `.mos` source file and stashed on a
//! [`NodeKind::Bibliography`] node. After lowering, citation keys are checked
//! against the parsed BibTeX records from those declared sources. Rendering
//! citation markers and bibliography entries is still a later slice.
//!
//! Diagnostics:
//!
//! - `MOS0040`: `#bibliography(...)` called without a (non-empty) path.
//! - `MOS0042`: `#bibliography(...)` declared more than one path; first wins.
//! - `MOS0020`: the path argument is present but not a string.
//! - `MOS0015`: an unknown keyword argument was supplied.
//! - `MOS0041`: the resolved path does not point to a file on disk
//!   (a warning — the node is still emitted with its resolved path).
//! - `MOS0045`: a citation key does not exist in a complete parsed bibliography set.
//! - `MOS0046`: a citation key appears in more than one declared bibliography source.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use mos_bib::Bibliography;
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
    let resolved = mos_core::resolve_source_path(&path, source_file);
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
                    mos_core::display_path(&resolved)
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
    let mut invalid_path_arg = false;
    for arg in args {
        match arg {
            // Positional first arg -- the path literal, same shorthand
            // `#image("path.png")` accepts.
            SetArg::Positional { value, value_span } => {
                if let SetValue::Str(s) = value {
                    if path.is_some() {
                        diagnostics.push(
                            Diagnostic::simple(
                                &codes::MOS0042,
                                None,
                                "duplicate path argument for `#bibliography`",
                            )
                            .with_span(value_span.clone()),
                        );
                    } else {
                        path = Some(s.clone());
                    }
                } else {
                    invalid_path_arg = true;
                    diagnostics.push(
                        Diagnostic::simple(
                            &codes::MOS0020,
                            None,
                            "`#bibliography(...)` expects a string path",
                        )
                        .with_span(value_span.clone()),
                    );
                }
            }
            SetArg::Named {
                key,
                value,
                key_span,
                value_span,
            } => match key.as_str() {
                "src" | "path" => {
                    if let SetValue::Str(s) = value {
                        if path.is_some() {
                            diagnostics.push(
                                Diagnostic::simple(
                                    &codes::MOS0042,
                                    None,
                                    "duplicate path argument for `#bibliography`",
                                )
                                .with_span(value_span.clone()),
                            );
                        } else {
                            path = Some(s.clone());
                        }
                    } else {
                        invalid_path_arg = true;
                        diagnostics.push(
                            Diagnostic::simple(
                                &codes::MOS0020,
                                None,
                                "`#bibliography(...)` expects a string path",
                            )
                            .with_span(value_span.clone()),
                        );
                    }
                }
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
        if invalid_path_arg {
            return None;
        }
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

/// Load every declared bibliography source, mark citation nodes whose keys
/// exist in any parsed record set, and rewrite their visible text to a
/// numeric label assigned by first-use order. Unknown citation keys emit
/// `MOS0045` once per citation node and keep their `[?key?]` placeholder.
///
/// Numbering is dense over *known* citations: a key consumes a number only
/// when it resolves, and repeated uses of the same key reuse its first
/// number. Unresolved keys never burn a slot, so `[1]`, `[2]`, ... always
/// index real bibliography records. This is the numeric-placeholder slice
/// (issue #67), not full CSL: no author-year styles, sorted output, or
/// citation clusters.
/// Resolve `[@key]` citations and return the set of keys declared by the
/// loaded bibliography sources. The reference resolver consumes that set to
/// tell an `@key` label reference that *misses* the label index but *matches*
/// a bibliography key apart -- a near-certain "meant a citation" mistake --
/// from a plain unknown label (see [`crate::resolve::resolve`]).
pub(super) fn resolve_citations(
    document: &mut Document,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeSet<String> {
    let bibliography = load_bibliography(document, diagnostics);
    let citation_ids: Vec<NodeId> = document
        .nodes()
        .filter(|node| node.kind == NodeKind::Citation)
        .map(|node| node.id)
        .collect();

    // `nodes()` walks the `BTreeMap<NodeId, Node>` in `NodeId` order, which is
    // the lowerer's allocation order -- i.e. document order. Collecting the ids
    // above preserves that order, so the first new key encountered here is the
    // document's first-cited key.
    let mut numbers: BTreeMap<String, usize> = BTreeMap::new();

    for citation_id in citation_ids {
        let Some(node) = document.get(citation_id) else {
            continue;
        };
        let Some(AttrValue::Str(key)) = node.attributes.get("key").cloned() else {
            continue;
        };
        if bibliography.records.entries.contains_key(&key) {
            let next_number = numbers.len() + 1;
            let number = *numbers.entry(key).or_insert(next_number);
            if let Some(node) = document.get_mut(citation_id) {
                node.attributes
                    .insert("resolved".to_owned(), AttrValue::Bool(true));
                node.attributes
                    .insert("text".to_owned(), AttrValue::Str(format!("[{number}]")));
            }
            continue;
        }
        if !bibliography.complete {
            continue;
        }
        diagnostics.push(
            Diagnostic::simple(
                &codes::MOS0045,
                Some(node.span.clone()),
                format!("unknown citation key `{key}` in bibliography records"),
            )
            .with_annotation(mos_core::DiagnosticAnnotation::Hint(
                "declare the key in a `#bibliography(...)` BibTeX source".to_owned(),
            )),
        );
    }

    bibliography.records.entries.into_keys().collect()
}

struct LoadedBibliography {
    records: Bibliography,
    complete: bool,
}

fn load_bibliography(document: &Document, diagnostics: &mut Vec<Diagnostic>) -> LoadedBibliography {
    let mut merged = Bibliography::default();
    let mut origins: BTreeMap<String, (PathBuf, SourceSpan)> = BTreeMap::new();
    let mut complete = true;
    for node in document
        .nodes()
        .filter(|node| node.kind == NodeKind::Bibliography)
    {
        let Some(AttrValue::Str(path)) = node.attributes.get("resolved_path") else {
            complete = false;
            continue;
        };
        let path_buf = PathBuf::from(path);
        if !path_buf.is_file() {
            complete = false;
            continue;
        }
        let source = match fs::read_to_string(&path_buf) {
            Ok(source) => source,
            Err(err) => {
                complete = false;
                diagnostics.push(Diagnostic::simple(
                    &codes::MOS0041,
                    Some(node.span.clone()),
                    format!(
                        "declared bibliography source `{}` could not be read: {err}",
                        mos_core::display_path(&path_buf)
                    ),
                ));
                continue;
            }
        };
        match mos_bib::parse_bibtex(&source) {
            Ok(parsed) => {
                for (key, entry) in parsed.entries {
                    if let Some((first_path, first_span)) = origins.get(&key) {
                        diagnostics.push(
                            Diagnostic::simple(
                                &codes::MOS0046,
                                Some(node.span.clone()),
                                format!(
                                    "duplicate citation key `{key}` in bibliography source `{}`",
                                    mos_core::display_path(&path_buf)
                                ),
                            )
                            .with_annotation(mos_core::DiagnosticAnnotation::Related {
                                span: first_span.clone(),
                                message: format!(
                                    "first bibliography source for `{key}` was `{}`",
                                    mos_core::display_path(first_path)
                                ),
                            })
                            .with_annotation(mos_core::DiagnosticAnnotation::Hint(
                                "keep citation keys unique across all declared bibliography sources"
                                    .to_owned(),
                            )),
                        );
                    } else {
                        origins.insert(key.clone(), (path_buf.clone(), node.span.clone()));
                        merged.entries.insert(key, entry);
                    }
                }
            }
            Err(err) => {
                complete = false;
                diagnostics.push(err.to_diagnostic(path_buf));
            }
        }
    }
    LoadedBibliography {
        records: merged,
        complete,
    }
}
