//! Hand-rolled recursive-descent parser for the minimal BibTeX subset.
//!
//! The grammar is intentionally tiny:
//!
//! ```text
//! bibtex := ws* (entry ws*)*
//! entry  := '@' type '{' key (',' fields)? '}'
//! fields := field (',' field)* ','?
//! field  := name '=' value
//! value  := '{' .. '}' | '"' .. '"' | bare
//! ```
//!
//! Entry types and field names are lowercased; citation keys are kept
//! verbatim. Brace values balance nested `{}` by naive counting, so
//! `{The {LaTeX} Companion}` is captured whole, but their contents are
//! stored as raw text — no `TeX` decoding, no `@string` / `@preamble` macro
//! expansion, no `#` concatenation, no name parsing.

use std::collections::BTreeMap;

use crate::error::{BibParseError, BibParseErrorKind};
use crate::record::{BibEntry, Bibliography};

/// Parse `input` as a minimal BibTeX database.
///
/// Returns a [`Bibliography`] whose entries are keyed by citation key. On a
/// duplicate citation key the last entry in source order wins. Parsing stops
/// at the first malformed entry and returns a [`BibParseError`] pinpointing
/// the byte offset; well-formed input never panics.
///
/// # Errors
///
/// Returns a [`BibParseError`] when the input is not a sequence of
/// well-formed `@type{key, field = value, ...}` entries separated by
/// whitespace — for example a missing `@`, entry type, `{`, citation key, or
/// `=`, or an unterminated brace/quote value.
///
/// # Examples
///
/// ```
/// use mos_bib::parse_bibtex;
///
/// # fn main() -> Result<(), mos_bib::BibParseError> {
/// let bib = parse_bibtex("@article{rivest1978, author = {Ron Rivest}, year = 1978}")?;
/// assert_eq!(bib.entries["rivest1978"].fields["year"], "1978");
/// # Ok(())
/// # }
/// ```
pub fn parse_bibtex(input: &str) -> Result<Bibliography, BibParseError> {
    let mut parser = Parser::new(input);
    let mut entries = BTreeMap::new();
    parser.skip_whitespace();
    while !parser.at_end() {
        let entry = parser.parse_entry()?;
        entries.insert(entry.key.clone(), entry);
        parser.skip_whitespace();
    }
    Ok(Bibliography { entries })
}

/// A byte cursor over the BibTeX source. All structural delimiters
/// (`@ { } " , =`) and whitespace are ASCII, so scanning byte-by-byte never
/// splits a multi-byte UTF-8 sequence and every recorded offset lands on a
/// `char` boundary.
struct Parser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) {
        self.pos += 1;
    }

    fn skip_whitespace(&mut self) {
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn error_here(&self, kind: BibParseErrorKind) -> BibParseError {
        BibParseError::new(kind, self.pos)
    }

    fn error_at(&self, offset: usize, kind: BibParseErrorKind) -> BibParseError {
        BibParseError::new(kind, offset)
    }

    /// Consume `byte` if it is next; otherwise fail with `kind`.
    fn expect_byte(&mut self, byte: u8, kind: BibParseErrorKind) -> Result<(), BibParseError> {
        if self.peek() == Some(byte) {
            self.bump();
            Ok(())
        } else {
            Err(self.error_here(kind))
        }
    }

    /// Consume a run of identifier bytes, returning the lowercased text.
    /// Returns `None` (consuming nothing) when no identifier byte is next.
    fn take_identifier(&mut self) -> Option<String> {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if is_identifier_byte(b) {
                self.bump();
            } else {
                break;
            }
        }
        if self.pos == start {
            None
        } else {
            Some(self.src[start..self.pos].to_ascii_lowercase())
        }
    }

    fn parse_entry(&mut self) -> Result<BibEntry, BibParseError> {
        self.expect_byte(b'@', BibParseErrorKind::ExpectedAt)?;
        self.skip_whitespace();
        let entry_type = self
            .take_identifier()
            .ok_or_else(|| self.error_here(BibParseErrorKind::ExpectedEntryType))?;
        self.skip_whitespace();
        self.expect_byte(b'{', BibParseErrorKind::ExpectedOpenBrace)?;
        self.skip_whitespace();
        let key = self.parse_key()?;
        self.skip_whitespace();
        let mut fields = BTreeMap::new();
        match self.peek() {
            Some(b'}') => self.bump(),
            Some(b',') => {
                self.bump();
                self.parse_fields(&mut fields)?;
            }
            Some(_) => return Err(self.error_here(BibParseErrorKind::ExpectedCommaOrCloseBrace)),
            None => return Err(self.error_here(BibParseErrorKind::UnterminatedEntry)),
        }
        Ok(BibEntry {
            entry_type,
            key,
            fields,
        })
    }

    /// A citation key runs verbatim until a structural delimiter or
    /// whitespace. It must be non-empty.
    fn parse_key(&mut self) -> Result<String, BibParseError> {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if is_key_byte(b) {
                self.bump();
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(self.error_here(BibParseErrorKind::ExpectedKey));
        }
        Ok(self.src[start..self.pos].to_owned())
    }

    /// Parse the comma-separated field list up to and including the closing
    /// `}`. A trailing comma before `}` is accepted.
    fn parse_fields(&mut self, fields: &mut BTreeMap<String, String>) -> Result<(), BibParseError> {
        loop {
            self.skip_whitespace();
            match self.peek() {
                // Closing brace (also the trailing-comma / empty-list case).
                Some(b'}') => {
                    self.bump();
                    return Ok(());
                }
                None => return Err(self.error_here(BibParseErrorKind::UnterminatedEntry)),
                _ => {}
            }
            let name = self
                .take_identifier()
                .ok_or_else(|| self.error_here(BibParseErrorKind::ExpectedFieldName))?;
            self.skip_whitespace();
            self.expect_byte(b'=', BibParseErrorKind::ExpectedEquals)?;
            self.skip_whitespace();
            let value = self.parse_value()?;
            // Last field wins on a repeated (post-lowercasing) field name.
            fields.insert(name, value);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.bump(),
                Some(b'}') => {
                    self.bump();
                    return Ok(());
                }
                None => return Err(self.error_here(BibParseErrorKind::UnterminatedEntry)),
                Some(_) => {
                    return Err(self.error_here(BibParseErrorKind::ExpectedCommaOrCloseBrace));
                }
            }
        }
    }

    fn parse_value(&mut self) -> Result<String, BibParseError> {
        match self.peek() {
            Some(b'{') => self.parse_braced(),
            Some(b'"') => self.parse_quoted(),
            Some(b) if is_bare_value_byte(b) => Ok(self.take_bare_value()),
            _ => Err(self.error_here(BibParseErrorKind::ExpectedValue)),
        }
    }

    /// Capture a `{...}` value, balancing nested braces by naive counting.
    /// The inner text is returned verbatim, braces and all.
    fn parse_braced(&mut self) -> Result<String, BibParseError> {
        let open_offset = self.pos;
        self.bump(); // consume '{'
        let content_start = self.pos;
        let mut depth = 1_usize;
        while let Some(b) = self.peek() {
            match b {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let value = self.src[content_start..self.pos].to_owned();
                        self.bump(); // consume closing '}'
                        return Ok(value);
                    }
                }
                _ => {}
            }
            self.bump();
        }
        Err(self.error_at(open_offset, BibParseErrorKind::UnterminatedValue))
    }

    /// Capture a `"..."` value, reading to the next `"`. Braces inside are
    /// not tracked; this is the documented minimal limitation.
    fn parse_quoted(&mut self) -> Result<String, BibParseError> {
        let open_offset = self.pos;
        self.bump(); // consume opening '"'
        let content_start = self.pos;
        while let Some(b) = self.peek() {
            if b == b'"' {
                let value = self.src[content_start..self.pos].to_owned();
                self.bump(); // consume closing '"'
                return Ok(value);
            }
            self.bump();
        }
        Err(self.error_at(open_offset, BibParseErrorKind::UnterminatedValue))
    }

    /// Capture an unquoted value (e.g. `1984`) as a single token. The caller
    /// has already confirmed the first byte is a bare-value byte, so the
    /// result is non-empty. `@string` macros are not resolved.
    fn take_bare_value(&mut self) -> String {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if is_bare_value_byte(b) {
                self.bump();
            } else {
                break;
            }
        }
        self.src[start..self.pos].to_owned()
    }
}

/// Bytes allowed in an entry type or field name.
fn is_identifier_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+' | b'.' | b':' | b'/')
}

/// Bytes allowed in a citation key: anything but a structural delimiter or
/// whitespace.
fn is_key_byte(b: u8) -> bool {
    !b.is_ascii_whitespace() && !matches!(b, b',' | b'{' | b'}' | b'"' | b'=' | b'@')
}

/// Bytes allowed in a bare (unquoted, unbraced) value.
fn is_bare_value_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+' | b'.' | b':' | b'/')
}
