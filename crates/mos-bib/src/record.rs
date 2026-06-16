//! The parsed bibliography data model: [`Bibliography`], [`BibEntry`], and
//! the document-body [`Citation`] reference.

use std::collections::BTreeMap;

/// A parsed bibliography: every [`BibEntry`] keyed by its citation key.
///
/// Entries live in a [`BTreeMap`], so iterating them yields a deterministic,
/// sorted-by-citation-key order that is easy to assert on in tests. Build
/// one from BibTeX source with [`parse_bibtex`](crate::parse_bibtex). On a
/// duplicate citation key the last entry in source order wins.
///
/// # Examples
///
/// ```
/// use mos_bib::Bibliography;
///
/// let empty = Bibliography::default();
/// assert!(empty.entries.is_empty());
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bibliography {
    /// Parsed entries keyed by citation key, in sorted key order.
    pub entries: BTreeMap<String, BibEntry>,
}

/// A single parsed BibTeX entry: one `@type{...}` record.
///
/// The entry type and field names are normalized to lowercase, because
/// BibTeX treats them case-insensitively; the citation [`key`](Self::key) is
/// preserved verbatim, because keys *are* case-sensitive. Fields live in a
/// [`BTreeMap`], so [`fields`](Self::fields) iterates in sorted, stable
/// order. Values are stored as raw text exactly as written between the
/// delimiters; no `TeX` decoding or name parsing.
///
/// # Examples
///
/// ```
/// use mos_bib::parse_bibtex;
///
/// # fn main() -> Result<(), mos_bib::BibParseError> {
/// let bib = parse_bibtex("@article{knuth1984, title = {Literate Programming}}")?;
/// let entry = &bib.entries["knuth1984"];
/// assert_eq!(entry.entry_type, "article");
/// assert_eq!(entry.key, "knuth1984");
/// assert_eq!(entry.fields["title"], "Literate Programming");
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BibEntry {
    /// The entry type without the leading `@`, lowercased (e.g. `article`).
    pub entry_type: String,
    /// The citation key, preserved verbatim (e.g. `knuth1984`).
    pub key: String,
    /// Field name (lowercased) to raw value text, in sorted name order.
    pub fields: BTreeMap<String, String>,
}

/// A citation reference within the document body: a single key that
/// resolves into a [`Bibliography`] entry at render time.
///
/// # Examples
///
/// ```
/// use mos_bib::Citation;
///
/// let citation = Citation {
///     key: "knuth1984".to_owned(),
/// };
///
/// assert_eq!(citation.key, "knuth1984");
/// ```
#[derive(Clone, Debug)]
pub struct Citation {
    pub key: String,
}
