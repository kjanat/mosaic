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
//!     engine_version,               // CARGO_PKG_VERSION — bumping it invalidates
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
//! # Interim hasher
//!
//! `H` here is FNV-1a over 128 bits — fully specified, portable, and
//! deterministic (unlike the randomly-seeded `SipHash` in [`std`]'s default
//! hasher, which §4 rules out). It is an **interim** boundary hasher: the
//! design note prefers BLAKE3-truncated-to-128, and the §9.4 source/asset
//! hashing slice may swap the construction. That swap is invisible to callers —
//! the signature stays `&[u8] -> ContentHash` — but it *does* change hash
//! values, which is exactly what the stamped `engine_version` exists to absorb.
//! FNV is not collision-hardened; nothing in this pre-cache slice yet relies on
//! adversarial collision resistance.

use mos_core::ContentHash;

/// Engine version stamped into every bibliography content hash (design note §5
/// rule 2). Bumping the workspace version invalidates every bibliography hash,
/// which is the intended escape hatch when the boundary or hasher changes.
const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Domain separator: keeps this boundary's hashes from colliding with any other
/// `H(...)` boundary that happens to feed identical bytes. The trailing `/v1`
/// versions the *framing*, independently of `engine_version`.
const DOMAIN_TAG: &[u8] = b"mos-bib/bibliography-source/v1";

/// FNV-1a 128-bit offset basis (FNV spec).
const FNV_OFFSET_BASIS: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;

/// FNV-1a 128-bit prime (FNV spec): `2^88 + 2^8 + 0x3b`.
const FNV_PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

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
    let mut state = FNV_OFFSET_BASIS;
    // Field order mirrors the §4.1 `SourceHash` shape: engine_version, then the
    // kind/domain tag, then the raw bytes. Kept in lock-step with the H(...)
    // sketch in this module's docs and in docs/incremental-dependencies.md.
    fold_field(&mut state, ENGINE_VERSION.as_bytes());
    fold_field(&mut state, DOMAIN_TAG);
    fold_field(&mut state, bytes);
    ContentHash(state)
}

/// Fold one length-prefixed field into the running FNV-1a state.
///
/// The `u64` length prefix makes the field boundaries unambiguous: without it,
/// a future multi-field boundary could let bytes migrate across a field split
/// and still hash equal. The width is fixed at `u64` (not `usize`) so the hash
/// is identical on 32- and 64-bit targets.
fn fold_field(state: &mut u128, field: &[u8]) {
    let len = u64::try_from(field.len()).unwrap_or(u64::MAX);
    fold_bytes(state, &len.to_le_bytes());
    fold_bytes(state, field);
}

/// Fold raw bytes into the running FNV-1a state (XOR-then-multiply per byte).
fn fold_bytes(state: &mut u128, bytes: &[u8]) {
    for &byte in bytes {
        *state ^= u128::from(byte);
        *state = state.wrapping_mul(FNV_PRIME);
    }
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

    #[test]
    fn length_framing_separates_fields() {
        // Two field sequences with identical concatenated bytes but a different
        // split must not collide: the u64 length prefix is what keeps a future
        // multi-field boundary unambiguous. Without the prefix, `"a" ++ "bc"`
        // and `"ab" ++ "c"` would fold to the same state.
        let mut split_a = super::FNV_OFFSET_BASIS;
        super::fold_field(&mut split_a, b"a");
        super::fold_field(&mut split_a, b"bc");

        let mut split_b = super::FNV_OFFSET_BASIS;
        super::fold_field(&mut split_b, b"ab");
        super::fold_field(&mut split_b, b"c");

        assert_ne!(split_a, split_b);
    }
}
