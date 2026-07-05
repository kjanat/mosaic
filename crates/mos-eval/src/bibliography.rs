//! Lower `#bibliography(...)` directives into a semantic node.
//!
//! This is the *source boundary* only (manifest §4, MVP 4): the directive
//! declares one bibliography database path, which is resolved relative to
//! the current `.mos` source file and stashed on a
//! [`NodeKind::Bibliography`] node. After lowering, citation keys are checked
//! against the parsed BibTeX records from those declared sources, resolved
//! citations get numeric `[N]` markers, and the first bibliography node
//! receives one rendered entry child per cited key in first-use order
//! (numeric slice only; CSL styles are a later slice).
//!
//! Diagnostics:
//!
//! - `MOS0040`: `#bibliography(...)` called without a (non-empty) path.
//! - `MOS0042`: `#bibliography(...)` declared more than one path; first wins.
//! - `MOS0020`: the path argument is present but not a string.
//! - `MOS0015`: an unknown keyword argument was supplied.
//! - `MOS0041`: the resolved path does not point to a file on disk
//!   (a warning; the node is still emitted with its resolved path).
//! - `MOS0045`: a citation key does not exist in a complete parsed bibliography set.
//! - `MOS0046`: a citation key appears in more than one declared bibliography source.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use mos_bib::Bibliography;
use mos_core::{
    AttrMap, AttrValue, Diagnostic, Document, NodeId, NodeKind, NodeSpec, SourceSpan, Suggestion,
    codes,
};
use mos_parse::{SetArg, SetValue};

use crate::suggest;

/// Named keys accepted by [`bibliography_path`]; the MOS0015 nearest-match
/// candidate set. Keep in sync with the match arms.
const BIBLIOGRAPHY_KEYS: &[&str] = &["src", "path"];

/// Lower `#bibliography("refs.bib")` into a bibliography node.
///
/// The literal path is recorded under `src`; the path resolved against the
/// source file's directory is recorded under `resolved_path`.
pub fn lower_bibliography_directive(
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
    let resolved = match mos_core::resolve_source_path(&path, source_file) {
        Ok(resolved) => resolved,
        Err(err) => {
            diagnostics.push(
                Diagnostic::simple(
                    &codes::MOS0049,
                    None,
                    format!("cannot use bibliography path `{path}`: {err}"),
                )
                .with_span(span.clone()),
            );
            return;
        }
    };
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
        NodeSpec::new(NodeKind::Bibliography, span.clone()).with_attributes(attributes),
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
                _ => diagnostics.push(suggest::unknown_key_diagnostic(
                    format!("unknown argument `{key}` for `#bibliography` (valid: src/path)"),
                    key,
                    key_span,
                    BIBLIOGRAPHY_KEYS,
                )),
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

/// Resolve citation keys against declared bibliography sources.
///
/// Known citations are rewritten to dense numeric labels assigned by first-use
/// order. Unknown keys emit `MOS0045` and keep `[?key?]` placeholders.
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
pub fn resolve_citations(
    document: &mut Document,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeSet<String> {
    let bibliography = load_bibliography(document, diagnostics);
    let citation_ids: Vec<NodeId> = document
        .nodes()
        .filter(|node| node.kind == NodeKind::Citation)
        .map(|node| node.id)
        .collect();
    // The entry list attaches to the *first* Bibliography node in document
    // order: one bibliography section is the typical document shape, and
    // partitioning entries across several declarations has no defined
    // semantics in this numeric slice. Later declarations still contribute
    // records; their nodes just stay childless.
    let bibliography_node = document
        .nodes()
        .find(|node| node.kind == NodeKind::Bibliography)
        .map(|node| (node.id, node.span.clone()));

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
            let number = *numbers.entry(key.clone()).or_insert(next_number);
            if let Some(node) = document.get_mut(citation_id) {
                node.attributes
                    .insert("resolved".to_owned(), AttrValue::Bool(true));
                node.attributes
                    .insert("text".to_owned(), AttrValue::Str(format!("[{number}]")));
                if let Some(origin) = bibliography.origins.get(&key) {
                    node.attributes.insert(
                        "target_path".to_owned(),
                        AttrValue::Str(origin.path.to_string_lossy().into_owned()),
                    );
                    if let (Ok(start), Ok(end)) = (
                        i64::try_from(origin.key_span.start()),
                        i64::try_from(origin.key_span.end()),
                    ) {
                        node.attributes
                            .insert("target_span.start".to_owned(), AttrValue::Int(start));
                        node.attributes
                            .insert("target_span.end".to_owned(), AttrValue::Int(end));
                    }
                }
            }
            continue;
        }
        if !bibliography.complete {
            continue;
        }
        let mut diagnostic = Diagnostic::simple(
            &codes::MOS0045,
            Some(node.span.clone()),
            format!("unknown citation key `{key}` in bibliography records"),
        )
        .with_annotation(mos_core::DiagnosticAnnotation::Hint(
            "declare the key in a `#bibliography(...)` BibTeX source".to_owned(),
        ));
        if let Some(candidate) = nearest_citation_key(&key, &bibliography.records.entries)
            && let Some(span) = citation_key_span(node, &key)
        {
            diagnostic = diagnostic.with_suggestion(Suggestion::new(span, candidate));
        }
        diagnostics.push(diagnostic);
    }

    if let Some((bib_id, bib_span)) = bibliography_node {
        append_bibliography_entries(document, bib_id, &bib_span, &bibliography, &numbers);
    }

    bibliography.records.entries.keys().cloned().collect()
}

/// Append one rendered entry per *cited* key as children of the first
/// [`NodeKind::Bibliography`] node, ordered by first-use citation number.
///
/// Each entry is a [`NodeKind::Paragraph`] carrying `entry_number` /
/// `entry_key` attributes and one [`NodeKind::Text`] child with the
/// [`format_entry`] text, so layout can render the list with `[N]` markers
/// without re-reading BibTeX sources. Uncited records add nothing, and
/// unresolved keys never made it into `numbers`, so the numbered entries
/// always match the citation markers in the prose.
fn append_bibliography_entries(
    document: &mut Document,
    bib_id: NodeId,
    bib_span: &SourceSpan,
    bibliography: &LoadedBibliography,
    numbers: &BTreeMap<String, usize>,
) {
    let mut ordered: Vec<(usize, &str)> = numbers
        .iter()
        .map(|(key, number)| (*number, key.as_str()))
        .collect();
    ordered.sort_unstable();

    for (number, key) in ordered {
        let Some(entry) = bibliography.records.entries.get(key) else {
            continue;
        };
        let mut entry_attrs: AttrMap = BTreeMap::new();
        entry_attrs.insert(
            "entry_number".to_owned(),
            AttrValue::Int(i64::try_from(number).unwrap_or(i64::MAX)),
        );
        entry_attrs.insert("entry_key".to_owned(), AttrValue::Str(key.to_owned()));
        let paragraph = document.alloc_child(
            bib_id,
            NodeSpec::new(NodeKind::Paragraph, bib_span.clone()).with_attributes(entry_attrs),
        );
        let mut text_attrs: AttrMap = BTreeMap::new();
        text_attrs.insert("text".to_owned(), AttrValue::Str(format_entry(key, entry)));
        document.alloc_child(
            paragraph,
            NodeSpec::new(NodeKind::Text, bib_span.clone()).with_attributes(text_attrs),
        );
    }
}

/// Render one BibTeX entry as plain text for the bibliography list.
///
/// Not a CSL processor: a fixed `Author. Title. Venue, volume, pp. pages.
/// Publisher. Year.` order where every missing field simply drops out.
/// Brace protection is stripped and whitespace collapsed via
/// [`clean_field`]; a part that already ends in `.` (initials like
/// `Knuth, D. E.`) is not double-terminated. An entry with no usable fields
/// falls back to its citation key so a numbered slot never renders empty.
fn format_entry(key: &str, entry: &mos_bib::BibEntry) -> String {
    let field = |name: &str| {
        entry
            .fields
            .get(name)
            .map(|raw| clean_field(raw))
            .filter(|value| !value.is_empty())
    };

    let mut parts: Vec<String> = Vec::new();
    parts.extend(field("author"));
    parts.extend(field("title"));

    // Venue clause: journal (or booktitle), then volume and pages appended
    // as `, {volume}` / `, pp. {pages}` so they stay attached to the venue
    // they qualify. Without a venue they still surface on their own.
    let mut venue = field("journal").or_else(|| field("booktitle"));
    if let Some(volume) = field("volume") {
        venue = Some(match venue {
            Some(venue) => format!("{venue}, {volume}"),
            None => volume,
        });
    }
    if let Some(pages) = field("pages") {
        venue = Some(match venue {
            Some(venue) => format!("{venue}, pp. {pages}"),
            None => format!("pp. {pages}"),
        });
    }
    parts.extend(venue);
    parts.extend(field("publisher"));
    parts.extend(field("year"));

    if parts.is_empty() {
        return key.to_owned();
    }
    let mut out = String::new();
    for part in &parts {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(part);
        if !part.ends_with('.') {
            out.push('.');
        }
    }
    out
}

/// Strip BibTeX brace protection and collapse runs of whitespace, so field
/// values like `{The {TeX}book}` render as `The TeXbook`.
fn clean_field(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut pending_space = false;
    for ch in raw.chars() {
        if matches!(ch, '{' | '}') {
            continue;
        }
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(ch);
    }
    out
}

fn citation_key_span(node: &mos_core::Node, key: &str) -> Option<SourceSpan> {
    let start = node.span.start().checked_add(2)?;
    let end = start.checked_add(key.len())?;
    (end < node.span.end()).then(|| SourceSpan::new(node.span.file.clone(), start, end))
}

fn is_citation_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b'.'))
}

/// The single loaded citation key that is a conservative near-miss for
/// `unknown`, if any. Candidate filtering stays here ([`is_citation_key`]);
/// the selection rule (length floor, `len / 3` bound, ties suggest nothing)
/// lives in [`crate::suggest::nearest_match`].
fn nearest_citation_key(
    unknown: &str,
    records: &BTreeMap<String, mos_bib::BibEntry>,
) -> Option<String> {
    suggest::nearest_match(
        unknown,
        records
            .keys()
            .filter(|key| is_citation_key(key))
            .map(String::as_str),
    )
    .map(str::to_owned)
}

struct LoadedBibliography {
    records: Bibliography,
    origins: BTreeMap<String, BibliographyOrigin>,
    complete: bool,
}

struct BibliographyOrigin {
    path: PathBuf,
    key_span: SourceSpan,
}

fn load_bibliography(document: &Document, diagnostics: &mut Vec<Diagnostic>) -> LoadedBibliography {
    let mut merged = Bibliography::default();
    let mut origins: BTreeMap<String, BibliographyOrigin> = BTreeMap::new();
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
                    let key_span =
                        SourceSpan::new(path_buf.clone(), entry.key_span.start, entry.key_span.end);
                    if let Some(first) = origins.get(&key) {
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
                                span: first.key_span.clone(),
                                message: format!(
                                    "first bibliography source for `{key}` was `{}`",
                                    mos_core::display_path(&first.path)
                                ),
                            })
                            .with_annotation(mos_core::DiagnosticAnnotation::Hint(
                                "keep citation keys unique across all declared bibliography sources"
                                    .to_owned(),
                            )),
                        );
                    } else {
                        origins.insert(
                            key.clone(),
                            BibliographyOrigin {
                                path: path_buf.clone(),
                                key_span,
                            },
                        );
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
        origins,
        complete,
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "tests panic loudly on setup failure; matches crate-wide test-module convention"
    )]

    use std::collections::BTreeMap;

    use mos_bib::BibEntry;

    use super::{clean_field, format_entry};

    fn entry(fields: &[(&str, &str)]) -> BibEntry {
        BibEntry {
            entry_type: "book".to_owned(),
            key: "key".to_owned(),
            key_span: 0..3,
            fields: fields
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect::<BTreeMap<String, String>>(),
        }
    }

    #[test]
    fn full_entry_renders_all_clauses_in_order() {
        let entry = entry(&[
            ("author", "Knuth, Donald E."),
            ("title", "Literate Programming"),
            ("journal", "The Computer Journal"),
            ("volume", "27"),
            ("pages", "97--111"),
            ("year", "1984"),
        ]);
        assert_eq!(
            format_entry("knuth1984", &entry),
            "Knuth, Donald E. Literate Programming. The Computer Journal, 27, pp. 97--111. 1984."
        );
    }

    #[test]
    fn part_ending_in_period_is_not_double_terminated() {
        let entry = entry(&[("author", "Knuth, D. E."), ("title", "The TeXbook")]);
        assert_eq!(
            format_entry("knuth", &entry),
            "Knuth, D. E. The TeXbook.",
            "initials already end the author clause"
        );
    }

    #[test]
    fn title_only_entry_renders_just_the_title() {
        let entry = entry(&[("title", "Alone")]);
        assert_eq!(format_entry("solo", &entry), "Alone.");
    }

    #[test]
    fn entry_without_useful_fields_falls_back_to_its_key() {
        let entry = entry(&[("isbn", "978-0")]);
        assert_eq!(format_entry("fallback2001", &entry), "fallback2001");
    }

    #[test]
    fn booktitle_and_bare_pages_still_surface() {
        let entry = entry(&[("booktitle", "Proc. of Mosaic"), ("pages", "1--7")]);
        assert_eq!(
            format_entry("conf", &entry),
            "Proc. of Mosaic, pp. 1--7.",
            "booktitle stands in for journal; pages attach to the venue"
        );
    }

    #[test]
    fn clean_field_strips_braces_and_collapses_whitespace() {
        assert_eq!(clean_field("{The {TeX}book}"), "The TeXbook");
        assert_eq!(clean_field("  spaced\n\tout  "), "spaced out");
        assert_eq!(clean_field("{}"), "");
    }
}
