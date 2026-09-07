//! The parsed bibliography data model: [`Bibliography`], [`BibEntry`], and
//! the document-body [`Citation`] reference.

use std::collections::BTreeMap;
use std::ops::Range;

/// A parsed bibliography: every [`BibEntry`] keyed by its citation key.
///
/// Entries live in a [`BTreeMap`], so iterating them yields a deterministic,
/// sorted-by-citation-key order that is easy to assert on in tests. Build
/// one from BibTeX source with [`parse_bibtex`](crate::parse_bibtex), which
/// rejects a duplicate citation key with
/// [`BibParseErrorKind::DuplicateKey`](crate::BibParseErrorKind::DuplicateKey)
/// at the duplicate key's offset. Last-value-wins applies only to a repeated
/// field name inside one entry.
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
/// order. Values are stored as raw text exactly as written, outer `{}` or
/// `""` delimiters included; bare values have none. A field name repeated
/// within one entry keeps its last value. No `TeX` decoding or name parsing.
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
/// assert_eq!(entry.fields["title"], "{Literate Programming}");
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BibEntry {
    /// The entry type without the leading `@`, lowercased (e.g. `article`).
    pub entry_type: String,
    /// The citation key, preserved verbatim (e.g. `knuth1984`).
    pub key: String,
    /// Byte range of [`key`](Self::key) inside the parsed BibTeX source.
    pub key_span: Range<usize>,
    /// Field name (lowercased) to raw value text including its outer
    /// delimiters, in sorted name order.
    pub fields: BTreeMap<String, String>,
}

impl BibEntry {
    /// A field's value with its outer `{}` / `""` delimiters removed; see
    /// [`unwrap_value`]. Inner text is still raw (no `TeX` decoding).
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_bib::parse_bibtex;
    ///
    /// # fn main() -> Result<(), mos_bib::BibParseError> {
    /// let bib = parse_bibtex(r#"@book{k, title = {The {TeX}book}, year = "1984"}"#)?;
    /// let entry = &bib.entries["k"];
    /// assert_eq!(entry.field_text("title"), Some("The {TeX}book"));
    /// assert_eq!(entry.field_text("year"), Some("1984"));
    /// assert_eq!(entry.field_text("pages"), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn field_text(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(|raw| unwrap_value(raw))
    }
}

/// Strip one matching outer `{...}` or `"..."` pair from a raw field value.
///
/// The pair must enclose the whole value: a `{` balances against the final
/// `}` only, and a `"` is not closed by an interior unescaped quote outside a
/// braced group, mirroring [`parse_bibtex`](crate::parse_bibtex). Bare values
/// and anything else are returned unchanged.
///
/// # Examples
///
/// ```
/// use mos_bib::unwrap_value;
///
/// assert_eq!(unwrap_value("{Literate Programming}"), "Literate Programming");
/// assert_eq!(unwrap_value(r#""Quoted""#), "Quoted");
/// assert_eq!(unwrap_value("1984"), "1984");
/// assert_eq!(unwrap_value("{a} {b}"), "{a} {b}");
/// ```
#[must_use]
pub fn unwrap_value(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    let enclosed = match (bytes.first(), bytes.last()) {
        (Some(b'{'), Some(b'}')) => brace_pair_encloses(bytes),
        (Some(b'"'), Some(b'"')) => quote_pair_encloses(bytes),
        _ => false,
    };
    if enclosed {
        &raw[1..raw.len() - 1]
    } else {
        raw
    }
}

fn brace_pair_encloses(bytes: &[u8]) -> bool {
    let mut depth = 0_usize;
    for (i, &b) in bytes.iter().enumerate() {
        if depth == 0 && i > 0 {
            return false;
        }
        match b {
            b'{' => depth += 1,
            b'}' => match depth.checked_sub(1) {
                Some(next) => depth = next,
                None => return false,
            },
            _ => {}
        }
    }
    depth == 0 && bytes.len() >= 2
}

fn quote_pair_encloses(bytes: &[u8]) -> bool {
    let mut depth = 0_usize;
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'{' => depth += 1,
            b'}' if depth > 0 => depth -= 1,
            b'"' if depth == 0 => return i == bytes.len() - 1,
            _ => {}
        }
        i += 1;
    }
    false
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Citation {
    /// The citation key as written in `[@key]`, matched verbatim against
    /// [`BibEntry::key`].
    pub key: String,
}
