//! Bibliography records for Mosaic (manifest §12).
//!
//! [`parse_bibtex`] reads a BibTeX string into typed [`BibEntry`] records,
//! keyed by citation key inside a [`Bibliography`]. The grammar is a
//! deliberately small, well-defined BibTeX subset — entry type, citation
//! key, and `{braced}` / `"quoted"` / bare string fields — chosen to give a
//! later citation resolver a stable, ordered record model to build on.
//!
//! Within that subset the parser is complete: it accepts any
//! `@type{key, field = value, ...}` entry, lowercases the (case-insensitive)
//! entry type and field names while keeping citation keys verbatim, balances
//! nested braces, and reports malformed input as a [`BibParseError`] with a
//! byte offset instead of panicking. Entries and fields live in
//! [`BTreeMap`](std::collections::BTreeMap)s, so iteration is deterministic
//! and sorted.
//!
//! Bibliography features beyond record parsing are separate concerns and
//! live elsewhere when they land: CSL / `BibLaTeX` styling, `@string` /
//! `@preamble` / `@comment` and `#` concatenation, `TeX` decoding, name
//! parsing, reading `.bib` files from disk, citation-key resolution, and
//! citation or bibliography rendering. This crate does none of those and has
//! no `mos-eval` / layout / PDF wiring.
//!
//! # Examples
//!
//! ```
//! use mos_bib::parse_bibtex;
//!
//! # fn main() -> Result<(), mos_bib::BibParseError> {
//! let bib = parse_bibtex("@article{knuth1984, title = {Literate Programming}, year = 1984}")?;
//! let entry = &bib.entries["knuth1984"];
//! assert_eq!(entry.entry_type, "article");
//! assert_eq!(entry.fields["title"], "Literate Programming");
//! assert_eq!(entry.fields["year"], "1984");
//! # Ok(())
//! # }
//! ```

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

mod content;
mod error;
mod parser;
mod record;

pub use content::bibliography_content_hash;
pub use error::{BibParseError, BibParseErrorKind};
pub use parser::parse_bibtex;
pub use record::{BibEntry, Bibliography, Citation};
