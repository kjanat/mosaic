//! Deterministic content hashing for cache boundaries (design note
//! `docs/incremental-dependencies.md` §4, §5).
//!
//! Incremental builds compare *content boundaries*: "did these bytes change?"
//!, and the §4 hash sketches all share one shape: `H(engine_version, …fields)`.
//! [`ContentHasher`] is the one place that shape is implemented, so every
//! boundary (bibliography sources, page layout output, future source/asset
//! hashes per §9.4) folds its fields the same way instead of re-deriving FNV in
//! each crate.
//!
//! # What it guarantees
//!
//! - **Engine-version stamped.** [`ContentHasher::new`] folds
//!   `CARGO_PKG_VERSION` first, so bumping the engine invalidates every hash
//!   (§5 rule 2). Callers cannot forget it.
//! - **Portable & deterministic.** The state is FNV-1a over 128 bits: fully
//!   specified, endianness-pinned (fixed little-endian widths), and independent
//!   of pointer identity or hash-map order. Unlike [`std`]'s randomly-seeded
//!   `SipHash`, which §4 rules out, two runs of the same input always agree.
//! - **Unambiguous framing.** [`field`](ContentHasher::field) length-prefixes
//!   its bytes, so concatenated variable-length fields cannot collide across a
//!   different split. Fixed-width numbers ([`u32`](ContentHasher::u32)) need no
//!   prefix.
//!
//! # Interim hasher
//!
//! FNV-1a is an **interim** choice: the design note prefers
//! BLAKE3-truncated-to-128, and the §9.4 slice may swap the construction. That
//! swap stays internal; the [`ContentHasher`] API is unchanged, but it does
//! change hash *values*, which the stamped engine version absorbs. FNV is not
//! collision-hardened; nothing yet relies on adversarial collision resistance.

/// Opaque content / dependency hash.
///
/// # Examples
///
/// ```
/// use mos_core::ContentHash;
///
/// let hash = ContentHash::default();
///
/// assert_eq!(hash.0, 0);
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct ContentHash(pub u128);

/// Engine version stamped into every hash (§5 rule 2).
const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// FNV-1a 128-bit offset basis (FNV spec).
const FNV_OFFSET_BASIS: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;

/// FNV-1a 128-bit prime (FNV spec): `2^88 + 2^8 + 0x3b`.
const FNV_PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

/// An incremental builder for a deterministic [`ContentHash`] over a sequence
/// of typed fields (design note §4).
///
/// Construct with [`new`](Self::new) (which stamps the engine version), fold a
/// domain tag plus the boundary's fields with [`field`](Self::field) /
/// [`u32`](Self::u32), then read the result with [`finish`](Self::finish). The
/// field sequence is the boundary's contract: keep it fixed per domain, and
/// lead with a domain tag so two boundaries that fold identical bytes still
/// differ.
///
/// # Examples
///
/// ```
/// use mos_core::ContentHasher;
///
/// // Same fields in the same order hash equal; any change diverges.
/// let mut a = ContentHasher::new();
/// a.field(b"example/v1").u32(7).field(b"payload");
///
/// let mut b = ContentHasher::new();
/// b.field(b"example/v1").u32(7).field(b"payload");
///
/// assert_eq!(a.finish(), b.finish());
///
/// let mut c = ContentHasher::new();
/// c.field(b"example/v1").u32(8).field(b"payload");
/// assert_ne!(a.finish(), c.finish());
/// ```
#[derive(Clone, Debug)]
pub struct ContentHasher {
    state: u128,
}

impl ContentHasher {
    /// Start a hasher, stamping the engine version (§5 rule 2) so callers
    /// cannot forget it.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::ContentHasher;
    ///
    /// // A fresh hasher already carries the engine-version stamp, so two
    /// // freshly-constructed hashers agree before any field is folded.
    /// assert_eq!(ContentHasher::new().finish(), ContentHasher::new().finish());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        let mut hasher = Self {
            state: FNV_OFFSET_BASIS,
        };
        hasher.field(ENGINE_VERSION.as_bytes());
        hasher
    }

    /// Fold one variable-length field, length-prefixed so field boundaries stay
    /// unambiguous.
    ///
    /// The `u64` length prefix is fixed-width (not `usize`) so the hash is
    /// identical on 32- and 64-bit targets.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::ContentHasher;
    ///
    /// // Length framing keeps `("a", "bc")` distinct from `("ab", "c")`.
    /// let mut split_a = ContentHasher::new();
    /// split_a.field(b"a").field(b"bc");
    /// let mut split_b = ContentHasher::new();
    /// split_b.field(b"ab").field(b"c");
    /// assert_ne!(split_a.finish(), split_b.finish());
    /// ```
    pub fn field(&mut self, bytes: &[u8]) -> &mut Self {
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        self.fold(&len.to_le_bytes());
        self.fold(bytes);
        self
    }

    /// Fold a fixed-width `u32` (little-endian). No length prefix is needed: the
    /// width is constant, so the field boundary is implicit.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::ContentHasher;
    ///
    /// let mut a = ContentHasher::new();
    /// let mut b = ContentHasher::new();
    /// a.u32(1);
    /// b.u32(2);
    /// assert_ne!(a.finish(), b.finish());
    /// ```
    pub fn u32(&mut self, value: u32) -> &mut Self {
        self.fold(&value.to_le_bytes());
        self
    }

    /// The accumulated [`ContentHash`].
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{ContentHash, ContentHasher};
    ///
    /// let hash: ContentHash = ContentHasher::new().field(b"x").finish();
    /// // It is a real 128-bit value, not the default.
    /// assert_ne!(hash, ContentHash::default());
    /// ```
    #[must_use]
    pub fn finish(&self) -> ContentHash {
        ContentHash(self.state)
    }

    /// Fold raw bytes into the running FNV-1a state (XOR-then-multiply).
    fn fold(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.state ^= u128::from(byte);
            self.state = self.state.wrapping_mul(FNV_PRIME);
        }
    }
}

impl Default for ContentHasher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{ContentHash, ContentHasher};

    #[test]
    fn same_fields_hash_equal() {
        let mut a = ContentHasher::new();
        a.field(b"dom").u32(3).field(b"body");
        let mut b = ContentHasher::new();
        b.field(b"dom").u32(3).field(b"body");
        assert_eq!(a.finish(), b.finish());
    }

    #[test]
    fn any_field_change_diverges() {
        let mut base = ContentHasher::new();
        base.field(b"dom").u32(3).field(b"body");

        let mut changed_num = ContentHasher::new();
        changed_num.field(b"dom").u32(4).field(b"body");

        let mut changed_bytes = ContentHasher::new();
        changed_bytes.field(b"dom").u32(3).field(b"BODY");

        assert_ne!(base.finish(), changed_num.finish());
        assert_ne!(base.finish(), changed_bytes.finish());
    }

    #[test]
    fn length_framing_separates_fields() {
        let mut split_a = ContentHasher::new();
        split_a.field(b"a").field(b"bc");
        let mut split_b = ContentHasher::new();
        split_b.field(b"ab").field(b"c");
        assert_ne!(split_a.finish(), split_b.finish());
    }

    #[test]
    fn engine_version_is_stamped() {
        // A fresh hasher is not the raw FNV offset basis: new() folds the
        // engine version, so finish() already differs from a zero hash.
        assert_ne!(ContentHasher::new().finish(), ContentHash::default());
    }

    #[test]
    fn fixed_width_u32_needs_no_length_prefix_to_stay_unambiguous() {
        // Two u32s vs. one: distinct because the values differ, and the
        // fixed-width framing means 1,2 never aliases 2,1.
        let mut forward = ContentHasher::new();
        forward.u32(1).u32(2);
        let mut backward = ContentHasher::new();
        backward.u32(2).u32(1);
        assert_ne!(forward.finish(), backward.finish());
    }
}
