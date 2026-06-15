//! Diagnostic code registry — the single source of truth for every
//! diagnostic the compiler can emit.
//!
//! Identity and severity are deliberately *separate axes*:
//!
//! - A [`DiagnosticCode`] answers "which rule fired?" It is an opaque,
//!   namespaced, severity-free identifier rendered as `MOS0010`. The
//!   number has no semantic meaning — it does not encode severity,
//!   owner crate, category, or lint group. Numbers are globally unique
//!   and stable; new codes get the next free integer regardless of
//!   what they describe.
//! - A [`DiagnosticDef`] pairs that code with its slug, *default*
//!   severity, category, owning crate, and a one-line summary. The
//!   catalog groups by [`DiagnosticCategory`], not by numeric range —
//!   so a rule that moves phases (parser → eval, fonts → text shaping)
//!   keeps its stable ID and just updates its `category`.
//!
//! Both `DiagnosticCode` and `DiagnosticDef` have crate-private fields
//! and crate-private constructors, so the only place a code or def can
//! be minted is the `define_codes!` invocation below. Outside crates
//! reference the `pub static` defs (`&codes::MOS0010`) and can neither
//! forge new ones nor disagree with a code's registered severity.

use crate::Severity;

/// Stable, severity-free diagnostic identifier (manifest §16).
///
/// Rendered as a namespace followed by a zero-padded four-digit number,
/// e.g. `MOS0010`. Equality and hashing use the `(namespace, number)`
/// pair, so the display width can grow past four digits without breaking
/// tooling that keyed off the structured value.
///
/// # Examples
///
/// ```
/// use mos_core::codes;
///
/// assert_eq!(codes::MOS0010.code().to_string(), "MOS0010");
/// assert_eq!(codes::MOS0010.code().number(), 10);
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct DiagnosticCode {
    namespace: &'static str,
    number: u32,
}

impl DiagnosticCode {
    /// The namespace segment (always `"MOS"` for compiler-native codes).
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::codes;
    ///
    /// assert_eq!(codes::MOS0033.code().namespace(), "MOS");
    /// ```
    #[must_use]
    pub const fn namespace(self) -> &'static str {
        self.namespace
    }

    /// The numeric portion, without zero-padding.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::codes;
    ///
    /// assert_eq!(codes::MOS0033.code().number(), 33);
    /// ```
    #[must_use]
    pub const fn number(self) -> u32 {
        self.number
    }

    pub(crate) const fn new(namespace: &'static str, number: u32) -> Self {
        Self { namespace, number }
    }
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{:04}", self.namespace, self.number)
    }
}

/// What *kind* of thing a diagnostic describes.
///
/// Category is metadata, never identity. The catalog groups by this so a
/// rule can change phase (parser → evaluator, fonts → text shaping)
/// without breaking its stable [`DiagnosticCode`].
///
/// # Examples
///
/// ```
/// use mos_core::{DiagnosticCategory, codes};
///
/// assert_eq!(codes::MOS0033.category(), DiagnosticCategory::Semantic);
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum DiagnosticCategory {
    /// Surface syntax: tokenisation, directive shape, inline grammar.
    Syntax,
    /// Semantic lowering: name resolution, schema validation, references.
    Semantic,
    /// Page geometry, paper sizes, style application.
    Layout,
    /// Text shaping, glyph coverage, font selection.
    Text,
    /// PDF backend emission and packaging.
    Pdf,
    /// Filesystem and asset I/O (read failure, decode failure, …).
    Io,
    /// Compiler-internal invariants (should never reach end users).
    Internal,
}

impl std::fmt::Display for DiagnosticCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Syntax => "Syntax",
            Self::Semantic => "Semantic",
            Self::Layout => "Layout",
            Self::Text => "Text",
            Self::Pdf => "Pdf",
            Self::Io => "Io",
            Self::Internal => "Internal",
        };
        f.write_str(s)
    }
}

/// Registry entry: one code, its slug, default severity, category, owner, summary.
///
/// Constructed only by `define_codes!`. Fields are read through the
/// accessors; there is no public constructor and no public field, so an
/// outside crate cannot forge a def that reuses an existing code with a
/// different slug or severity.
///
/// # Examples
///
/// ```
/// use mos_core::{DiagnosticCategory, Severity, codes};
///
/// assert_eq!(codes::MOS0018.default_severity(), Severity::Notice);
/// assert_eq!(codes::MOS0018.category(), DiagnosticCategory::Text);
/// assert_eq!(codes::MOS0018.owner(), "mos-fonts");
/// ```
#[derive(Debug)]
pub struct DiagnosticDef {
    code: DiagnosticCode,
    slug: &'static str,
    default_severity: Severity,
    category: DiagnosticCategory,
    owner: &'static str,
    summary: &'static str,
}

impl DiagnosticDef {
    /// The stable identifier.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::codes;
    ///
    /// assert_eq!(codes::MOS0033.code().to_string(), "MOS0033");
    /// ```
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        self.code
    }

    /// The machine-readable kebab-case handle (e.g. `"label-duplicate"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::codes;
    ///
    /// assert_eq!(codes::MOS0033.slug(), "label-missing");
    /// ```
    #[must_use]
    pub const fn slug(&self) -> &'static str {
        self.slug
    }

    /// The severity this code carries unless overridden by future config.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Severity, codes};
    ///
    /// assert_eq!(codes::MOS0033.default_severity(), Severity::Error);
    /// ```
    #[must_use]
    pub const fn default_severity(&self) -> Severity {
        self.default_severity
    }

    /// What kind of thing this code describes. Used by the catalog to
    /// group rules; never folded into identity.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{DiagnosticCategory, codes};
    ///
    /// assert_eq!(codes::MOS0033.category(), DiagnosticCategory::Semantic);
    /// ```
    #[must_use]
    pub const fn category(&self) -> DiagnosticCategory {
        self.category
    }

    /// The crate that owns the emit site(s).
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::codes;
    ///
    /// assert_eq!(codes::MOS0033.owner(), "mos-eval");
    /// ```
    #[must_use]
    pub const fn owner(&self) -> &'static str {
        self.owner
    }

    /// One-line human summary, mirrored verbatim into the catalog.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::codes;
    ///
    /// assert!(codes::MOS0033.summary().contains("@reference"));
    /// ```
    #[must_use]
    pub const fn summary(&self) -> &'static str {
        self.summary
    }

    pub(crate) const fn new(
        code: DiagnosticCode,
        slug: &'static str,
        default_severity: Severity,
        category: DiagnosticCategory,
        owner: &'static str,
        summary: &'static str,
    ) -> Self {
        Self {
            code,
            slug,
            default_severity,
            category,
            owner,
            summary,
        }
    }
}

/// Define the entire diagnostic registry.
///
/// Each line expands to a `pub static DiagnosticDef` plus an entry in
/// [`ALL`]. The macro is the *only* mint site for codes and defs, and it
/// generates the invariant tests (unique numbers, unique slugs, the
/// static's name matches its rendered code, `MOS` + four digits).
macro_rules! define_codes {
    (
        $(
            $(#[$meta:meta])*
            $name:ident = $num:literal, $sev:ident, $cat:ident, $slug:literal, $owner:literal, $summary:literal;
        )*
    ) => {
        $(
            $(#[$meta])*
            pub static $name: DiagnosticDef = DiagnosticDef::new(
                DiagnosticCode::new("MOS", $num),
                $slug,
                Severity::$sev,
                DiagnosticCategory::$cat,
                $owner,
                $summary,
            );
        )*

        /// Every registered diagnostic definition, in declaration order.
        ///
        /// The catalog drift test (`crates/mos/tests/catalog.rs`) walks
        /// this slice; keep it as the single machine-readable source.
        pub static ALL: &[&DiagnosticDef] = &[ $( &$name ),* ];

        #[cfg(test)]
        mod generated_tests {
            use super::*;

            #[test]
            fn numbers_are_globally_unique() {
                let mut seen = std::collections::BTreeSet::new();
                for def in ALL {
                    let key = (def.code().namespace(), def.code().number());
                    assert!(
                        seen.insert(key),
                        "duplicate diagnostic number: {}",
                        def.code()
                    );
                }
            }

            #[test]
            fn slugs_are_unique_and_kebab_case() {
                let mut seen = std::collections::BTreeSet::new();
                for def in ALL {
                    assert!(seen.insert(def.slug()), "duplicate slug: {}", def.slug());
                    assert!(
                        !def.slug().is_empty()
                            && def.slug().bytes().all(|b| {
                                b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'
                            }),
                        "slug {:?} must be non-empty kebab-case",
                        def.slug()
                    );
                }
            }

            #[test]
            fn rendered_code_is_namespace_plus_four_digits() {
                for def in ALL {
                    let rendered = def.code().to_string();
                    assert!(rendered.starts_with("MOS"), "code {rendered} must start with MOS");
                    assert_eq!(rendered.len(), 7, "code {rendered} must be MOS + 4 digits");
                    assert!(
                        rendered[3..].bytes().all(|b| b.is_ascii_digit()),
                        "code {rendered} tail must be all digits"
                    );
                }
            }

            #[test]
            fn static_name_matches_rendered_code() {
                $(
                    assert_eq!(
                        stringify!($name),
                        $name.code().to_string(),
                        "the static's name must equal its rendered code"
                    );
                )*
            }
        }
    };
}

// Numbers are opaque. They do not encode category, severity, owner, or
// phase. Current assignments intentionally interleave categories to avoid
// accidental range semantics. Declaration order groups by category here
// for source-reading convenience only — the catalog (and any consumer)
// groups by `category()`, not by numeric range.
define_codes! {
    // ── syntax (mos-parse) ────────────────────────────────────────────
    /// `#set` not followed by an identifier.
    MOS0010 = 10, Error, Syntax, "set-missing-identifier", "mos-parse",
        "syntax: #set not followed by an identifier";
    /// Missing `(` after `#set NAME`, `#image`, or `#figure`.
    MOS0013 = 13, Error, Syntax, "directive-missing-paren", "mos-parse",
        "syntax: directive missing opening parenthesis";
    /// Unterminated `#NAME(...)` or `#NAME[[...]]` block.
    MOS0016 = 16, Error, Syntax, "directive-unterminated", "mos-parse",
        "syntax: unterminated directive block";
    /// Unexpected trailing content after a directive on the same line.
    MOS0019 = 19, Error, Syntax, "directive-trailing-content", "mos-parse",
        "syntax: unexpected trailing content after directive";
    /// Malformed directive argument value (bad escape, unknown unit,
    /// unterminated string, lone `-`, malformed number/length).
    MOS0022 = 22, Error, Syntax, "directive-malformed-arg", "mos-parse",
        "syntax: malformed directive argument value";
    /// Argument-list shape error (missing `:`, missing `,`/`)`,
    /// positional where named expected).
    MOS0025 = 25, Error, Syntax, "arglist-shape", "mos-parse",
        "syntax: malformed argument list";
    /// Unterminated `**strong**` run; treated as literal text.
    MOS0028 = 28, Warning, Syntax, "unterminated-strong", "mos-parse",
        "syntax: unterminated **strong** run; treated as text";
    /// Unterminated `*emphasis*` run; treated as literal text.
    MOS0031 = 31, Warning, Syntax, "unterminated-emphasis", "mos-parse",
        "syntax: unterminated *emphasis* run; treated as text";
    /// Unterminated `` `code` `` run; treated as literal text.
    MOS0034 = 34, Warning, Syntax, "unterminated-code", "mos-parse",
        "syntax: unterminated `code` run; treated as text";
    /// Stray `@` not followed by a label identifier; treated as text.
    MOS0036 = 36, Warning, Syntax, "stray-at-sign", "mos-parse",
        "syntax: stray @ not followed by a label; treated as text";
    /// Lone trailing `\` at end of input; treated as literal text.
    MOS0038 = 38, Warning, Syntax, "lone-trailing-backslash", "mos-parse",
        "syntax: lone trailing backslash at end of input; treated as text";
    /// Malformed citation group; treated as literal text.
    MOS0039 = 39, Warning, Syntax, "malformed-citation", "mos-parse",
        "syntax: malformed citation group; treated as text";
    /// A heading `<label>` is not the last element on the line, so it is not
    /// recognised as a label declaration and is treated as literal text;
    /// references to it would then fail to resolve.
    MOS0048 = 48, Warning, Syntax, "heading-label-not-trailing", "mos-parse",
        "syntax: heading label is not the last element on the line; treated as text";
    /// BibTeX database could not be parsed (`mos-bib`).
    MOS0043 = 43, Error, Syntax, "bibtex-parse-failed", "mos-bib",
        "syntax: BibTeX database could not be parsed";
    /// CSL style could not be parsed (`mos-csl`).
    MOS0044 = 44, Error, Syntax, "csl-parse-failed", "mos-csl",
        "syntax: CSL style could not be parsed";

    // ── semantic (mos-eval) ───────────────────────────────────────────
    /// Unknown `#set` target (only `page`, `text`, `document`, `image`).
    MOS0011 = 11, Error, Semantic, "set-unknown-target", "mos-eval",
        "semantic: unknown #set target";
    /// Unknown keyword argument for `#set TARGET`, `#image`, or `#figure`.
    MOS0015 = 15, Error, Semantic, "unknown-kwarg", "mos-eval",
        "semantic: unknown keyword argument";
    /// Argument type mismatch or non-positive length.
    MOS0020 = 20, Error, Semantic, "arg-type-mismatch", "mos-eval",
        "semantic: argument type mismatch or non-positive length";
    /// `#set` rejecting a positional argument where named is required.
    MOS0024 = 24, Error, Semantic, "set-positional-rejected", "mos-eval",
        "semantic: #set rejects positional argument";
    /// `#set` value passes typing but trips a sanity floor; still applied.
    MOS0027 = 27, Warning, Semantic, "set-sanity-floor", "mos-eval",
        "semantic: #set value trips a sanity floor; value still applied";
    /// Label declared more than once; first declaration wins.
    MOS0030 = 30, Error, Semantic, "label-duplicate", "mos-eval",
        "semantic: label declared more than once";
    /// `@label` reference to a label that does not exist.
    MOS0033 = 33, Error, Semantic, "label-missing", "mos-eval",
        "semantic: @reference to a label that does not exist";
    /// `#image(...)`/`#figure(...)` missing a path argument.
    MOS0037 = 37, Error, Semantic, "image-missing-path", "mos-eval",
        "semantic: #image/#figure missing a path argument";
    /// `#bibliography(...)` missing a path argument.
    MOS0040 = 40, Error, Semantic, "bibliography-missing-path", "mos-eval",
        "semantic: #bibliography missing a path argument";
    /// `#bibliography(...)` path declared more than once; first wins.
    MOS0042 = 42, Error, Semantic, "bibliography-duplicate-path", "mos-eval",
        "semantic: #bibliography path argument declared more than once";
    /// `[@key]` citation to a bibliography record that does not exist.
    MOS0045 = 45, Error, Semantic, "citation-missing", "mos-eval",
        "semantic: citation key does not exist in bibliography records";
    /// Citation key appears in more than one declared bibliography source.
    MOS0046 = 46, Error, Semantic, "bibliography-duplicate-key", "mos-eval",
        "semantic: citation key appears in more than one bibliography source";

    // ── filesystem / asset I/O ────────────────────────────────────────
    /// Image file cannot be read from disk.
    MOS0012 = 12, Error, Io, "image-read-failed", "mos-eval",
        "io: image file cannot be read from disk";
    /// Image file cannot be decoded (unsupported or corrupt).
    MOS0029 = 29, Error, Io, "image-decode-failed", "mos-eval",
        "io: image file cannot be decoded";
    /// Declared `#bibliography(...)` source file is not on disk.
    MOS0041 = 41, Warning, Io, "bibliography-source-missing", "mos-eval",
        "io: declared bibliography source file not found";

    // ── layout (mos-layout) ───────────────────────────────────────────
    /// Unknown paper size in `#set page(paper: ...)`.
    MOS0017 = 17, Error, Layout, "paper-size-unknown", "mos-layout",
        "layout: unknown paper size";
    /// Well-typed `#set` value breaks page geometry; previous value kept.
    MOS0023 = 23, Error, Layout, "geometry-breaks-page", "mos-layout",
        "layout: value breaks page geometry; previous value retained";
    /// Image reached layout without decoded pixels; skipped on the page.
    MOS0035 = 35, Warning, Layout, "image-skipped-no-pixels", "mos-layout",
        "layout: image reached layout without decoded pixels; skipped";
    /// `@page(...)` references did not converge to stable page numbers within
    /// the iteration cap; the last computed numbers are used.
    MOS0047 = 47, Warning, Layout, "page-fixpoint-nonconvergence", "mos-eval",
        "layout: page references did not converge; last computed page numbers used";

    // ── text / fonts / shaping ────────────────────────────────────────
    /// Unknown font family; falling back to bundled Noto Sans.
    MOS0018 = 18, Notice, Text, "font-family-substituted", "mos-fonts",
        "text: substituted bundled Noto Sans for unknown font family";
    /// Base-14 `/Differences` glyph budget exhausted for a face.
    MOS0032 = 32, Warning, Text, "glyph-budget-exhausted", "mos-pdf",
        "text: Base-14 /Differences glyph budget exhausted";

    // ── PDF emission (mos-pdf) ────────────────────────────────────────
    /// PDF backend I/O failure (cannot create dir or write bytes).
    MOS0014 = 14, Error, Pdf, "pdf-io-failed", "mos-pdf",
        "pdf: backend I/O failure";
    /// Font subsetting failure for an embedded face.
    MOS0026 = 26, Error, Pdf, "font-subset-failed", "mos-pdf",
        "pdf: font subsetting failure for an embedded face";

    // ── compiler-internal invariants ──────────────────────────────────
    /// Internal: missing embedded font plan for a shaped run.
    MOS0021 = 21, Error, Internal, "internal-missing-font-plan", "mos-pdf",
        "internal: missing embedded font plan for a shaped run";
}
