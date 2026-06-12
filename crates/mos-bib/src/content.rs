//! Content-hash boundary for bibliography inputs (manifest §7, §32; design
//! note `docs/incremental-dependencies.md` §4.1).
//!
//! A future incremental build needs to answer "did this `.bib` change?" before
//! it can decide whether cached citation data is stale.
//! [`bibliography_content_hash`] supplies the *boundary* half of that answer:
//! it pins exactly which bytes feed a bibliography source's content hash, so
//! two builds of the same file converge on the same [`ContentHash`] and any
//! edit diverges. The path-shaped *identity* half lives next door in
//! `mos_cache::DependencyId::Bibliography`, and the two are paired by
//! `mos_cache::BibliographyDependency`.
//!
//! # Hash boundary (design note §4.1)
//!
//! ```text
//! BibliographyContentHash = H(
//!     engine_version,               // stamped by ContentHasher::new()
//!     domain_tag,                   // distinguishes this boundary from other H(...)
//!     file_bytes                    // raw bytes as read, byte-for-byte, no normalization
//! )
//! ```
//!
//! The bytes are hashed **raw**: no NFC, no line-ending fold, no BOM strip. That
//! mirrors §4.1 — the parser does not normalize source today, so the content
//! hash must reflect what the parser actually consumed, or the cache would
//! "forget" cosmetic edits the parser is sensitive to. Filesystem-derived data
//! (mtime, inode, absolute path) is deliberately *not* an input.
//!
//! `H` is [`mos_core::ContentHasher`] — the shared, engine-version-stamped,
//! length-framed FNV-1a-128 boundary hasher (interim; swappable to BLAKE3 per
//! §9.4 without changing this `&[u8] -> ContentHash` signature). This boundary
//! just supplies the domain tag and the raw bytes.

use mos_core::{ContentHash, ContentHasher};

/// Domain separator: keeps this boundary's hashes from colliding with any other
/// `H(...)` boundary that happens to feed identical bytes. The trailing `/v1`
/// versions the *framing*, independently of `engine_version`.
const DOMAIN_TAG: &[u8] = b"mos-bib/bibliography-source/v1";

/// Compute the content-hash boundary for one bibliography source's raw bytes.
///
/// The result is the §4.1 source hash specialized to bibliography inputs:
/// deterministic for identical bytes, divergent for any byte change, and
/// independent of where or when the file was read. Pair it with a
/// `mos_cache::DependencyId::Bibliography` identity (typically via
/// `mos_cache::BibliographyDependency`) to model a full bibliography
/// dependency.
///
/// # Examples
///
/// Identical bytes hash equal; a one-byte edit diverges:
///
/// ```
/// use mos_bib::bibliography_content_hash;
///
/// let a = bibliography_content_hash(b"@article{k, year = 1984}");
/// let b = bibliography_content_hash(b"@article{k, year = 1984}");
/// assert_eq!(a, b);
/// assert_ne!(a, bibliography_content_hash(b"@article{k, year = 1985}"));
/// ```
///
/// Hashing is byte-for-byte, so NFC- and NFD-encoded text that *looks* the same
/// produces different hashes (the parser sees different bytes, §4.1):
///
/// ```
/// use mos_bib::bibliography_content_hash;
///
/// // "é" composed (NFC) vs. "e" + combining acute (NFD).
/// let nfc = bibliography_content_hash("@misc{r, note = {\u{00e9}}}".as_bytes());
/// let nfd = bibliography_content_hash("@misc{r, note = {e\u{0301}}}".as_bytes());
/// assert_ne!(nfc, nfd);
/// ```
#[must_use]
pub fn bibliography_content_hash(bytes: &[u8]) -> ContentHash {
    // ContentHasher::new() stamps engine_version (§5 rule 2); the domain tag and
    // raw bytes follow, both length-framed. Field order mirrors the §4.1
    // `SourceHash` shape: engine_version, kind/domain tag, raw bytes.
    let mut hasher = ContentHasher::new();
    hasher.field(DOMAIN_TAG).field(bytes);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::bibliography_content_hash;

    #[test]
    fn identical_bytes_hash_equal() {
        let source = b"@article{knuth1984, title = {Literate Programming}, year = 1984}";
        assert_eq!(
            bibliography_content_hash(source),
            bibliography_content_hash(source)
        );
    }

    #[test]
    fn one_byte_change_diverges() {
        assert_ne!(
            bibliography_content_hash(b"@article{k, year = 1984}"),
            bibliography_content_hash(b"@article{k, year = 1985}")
        );
    }

    #[test]
    fn hashing_is_byte_for_byte_not_normalized() {
        // Composed (NFC) vs. decomposed (NFD) "é": same text, different bytes,
        // therefore different hashes — the parser would see different bytes.
        assert_ne!(
            bibliography_content_hash("\u{00e9}".as_bytes()),
            bibliography_content_hash("e\u{0301}".as_bytes())
        );
    }

    #[test]
    fn empty_input_has_a_stable_distinct_hash() {
        assert_eq!(
            bibliography_content_hash(b""),
            bibliography_content_hash(b"")
        );
        assert_ne!(
            bibliography_content_hash(b""),
            bibliography_content_hash(b" ")
        );
    }
}
