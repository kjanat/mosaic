//! Diagnostic code registry — the single source of truth for every
//! diagnostic the compiler can emit.
//!
//! Identity and severity are deliberately *separate axes*:
//!
//! - A [`DiagnosticCode`] answers "which rule fired?" It is a stable,
//!   namespaced, severity-free identifier rendered as `MOS0110`.
//! - A [`DiagnosticDef`] pairs that code with its machine slug, its
//!   *default* severity, the owning crate, and a one-line summary.
//!
//! Both types have crate-private fields and crate-private constructors,
//! so the only place a code or def can be minted is the `define_codes!`
//! invocation below. Outside crates reference the `pub static` defs
//! (`&codes::MOS0010`) and can neither forge new ones nor disagree with a
//! code's registered severity.
//!
//! Numbers are organised by *category* (100-block), never by severity:
//!
//! | Range       | Category               |
//! |-------------|------------------------|
//! | `0000–0099` | syntax (parse)         |
//! | `0100–0199` | semantic (lower/eval)  |
//! | `0200–0299` | layout                 |
//! | `0300–0399` | text / fonts / shaping |
//! | `0400–0499` | PDF emission           |
//! | `0500–0599` | project / CLI / IO     |
//! | `0600–9999` | reserved               |

use crate::Severity;

/// Stable, severity-free diagnostic identifier (manifest §16).
///
/// Rendered as a namespace followed by a zero-padded four-digit number,
/// e.g. `MOS0110`. Equality and hashing use the `(namespace, number)`
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
    #[must_use]
    pub const fn namespace(self) -> &'static str {
        self.namespace
    }

    /// The numeric portion, without zero-padding.
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

/// Registry entry: one code, its slug, default severity, owner, summary.
///
/// Constructed only by `define_codes!`. Fields are read through the
/// accessors; there is no public constructor and no public field, so an
/// outside crate cannot forge a def that reuses an existing code with a
/// different slug or severity.
///
/// # Examples
///
/// ```
/// use mos_core::{Severity, codes};
///
/// assert_eq!(codes::MOS0300.default_severity(), Severity::Notice);
/// assert_eq!(codes::MOS0300.owner(), "mos-fonts");
/// ```
#[derive(Debug)]
pub struct DiagnosticDef {
    code: DiagnosticCode,
    slug: &'static str,
    default_severity: Severity,
    owner: &'static str,
    summary: &'static str,
}

impl DiagnosticDef {
    /// The stable identifier.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        self.code
    }

    /// The machine-readable kebab-case handle (e.g. `"label-duplicate"`).
    #[must_use]
    pub const fn slug(&self) -> &'static str {
        self.slug
    }

    /// The severity this code carries unless overridden by future config.
    #[must_use]
    pub const fn default_severity(&self) -> Severity {
        self.default_severity
    }

    /// The crate that owns the emit site(s).
    #[must_use]
    pub const fn owner(&self) -> &'static str {
        self.owner
    }

    /// One-line human summary, mirrored verbatim into the catalog.
    #[must_use]
    pub const fn summary(&self) -> &'static str {
        self.summary
    }

    pub(crate) const fn new(
        code: DiagnosticCode,
        slug: &'static str,
        default_severity: Severity,
        owner: &'static str,
        summary: &'static str,
    ) -> Self {
        Self {
            code,
            slug,
            default_severity,
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
            $name:ident = $num:literal, $sev:ident, $slug:literal, $owner:literal, $summary:literal;
        )*
    ) => {
        $(
            $(#[$meta])*
            pub static $name: DiagnosticDef = DiagnosticDef::new(
                DiagnosticCode::new("MOS", $num),
                $slug,
                Severity::$sev,
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

define_codes! {
    // ── reserved / documentation ──────────────────────────────────────
    /// Reserved documentation/example code. Never emitted by any stage;
    /// used only in doctests where a concrete code is needed.
    MOS0000 = 0, Error, "example", "mos-core",
        "reserved documentation example; never emitted";

    // ── syntax (mos-parse), 0000–0099 ─────────────────────────────────
    /// `#set` not followed by an identifier.
    MOS0010 = 10, Error, "set-missing-identifier", "mos-parse",
        "syntax: #set not followed by an identifier";
    /// Missing `(` after `#set NAME`, `#image`, or `#figure`.
    MOS0011 = 11, Error, "directive-missing-paren", "mos-parse",
        "syntax: directive missing opening parenthesis";
    /// Unterminated `#NAME(...)` or `#NAME[[...]]` block.
    MOS0012 = 12, Error, "directive-unterminated", "mos-parse",
        "syntax: unterminated directive block";
    /// Unexpected trailing content after a directive on the same line.
    MOS0013 = 13, Error, "directive-trailing-content", "mos-parse",
        "syntax: unexpected trailing content after directive";
    /// Malformed directive argument value (bad escape, unknown unit,
    /// unterminated string, lone `-`, malformed number/length).
    MOS0014 = 14, Error, "directive-malformed-arg", "mos-parse",
        "syntax: malformed directive argument value";
    /// Argument-list shape error (missing `:`, missing `,`/`)`,
    /// positional where named expected).
    MOS0015 = 15, Error, "arglist-shape", "mos-parse",
        "syntax: malformed argument list";
    /// Unterminated `**strong**` run; treated as literal text.
    MOS0020 = 20, Warning, "unterminated-strong", "mos-parse",
        "syntax: unterminated **strong** run; treated as text";
    /// Unterminated `*emphasis*` run; treated as literal text.
    MOS0021 = 21, Warning, "unterminated-emphasis", "mos-parse",
        "syntax: unterminated *emphasis* run; treated as text";
    /// Unterminated `` `code` `` run; treated as literal text.
    MOS0022 = 22, Warning, "unterminated-code", "mos-parse",
        "syntax: unterminated `code` run; treated as text";
    /// Stray `@` not followed by a label identifier; treated as text.
    MOS0023 = 23, Warning, "stray-at-sign", "mos-parse",
        "syntax: stray @ not followed by a label; treated as text";
    /// Lone trailing `\` at end of input; treated as literal text.
    MOS0024 = 24, Warning, "lone-trailing-backslash", "mos-parse",
        "syntax: lone trailing backslash at end of input; treated as text";

    // ── semantic (mos-eval), 0100–0199 ────────────────────────────────
    /// Unknown `#set` target (only `page`, `text`, `document`, `image`).
    MOS0100 = 100, Error, "set-unknown-target", "mos-eval",
        "semantic: unknown #set target";
    /// Unknown keyword argument for `#set TARGET`, `#image`, or `#figure`.
    MOS0101 = 101, Error, "unknown-kwarg", "mos-eval",
        "semantic: unknown keyword argument";
    /// Argument type mismatch or non-positive length.
    MOS0102 = 102, Error, "arg-type-mismatch", "mos-eval",
        "semantic: argument type mismatch or non-positive length";
    /// `#set` rejecting a positional argument where named is required.
    MOS0103 = 103, Error, "set-positional-rejected", "mos-eval",
        "semantic: #set rejects positional argument";
    /// `#set` value passes typing but trips a sanity floor; still applied.
    MOS0120 = 120, Warning, "set-sanity-floor", "mos-eval",
        "semantic: #set value trips a sanity floor; value still applied";
    /// Label declared more than once; first declaration wins.
    MOS0140 = 140, Error, "label-duplicate", "mos-eval",
        "semantic: label declared more than once";
    /// `@label` reference to a label that does not exist.
    MOS0141 = 141, Error, "label-missing", "mos-eval",
        "semantic: @reference to a label that does not exist";
    /// `#image(...)`/`#figure(...)` missing a path argument.
    MOS0160 = 160, Error, "image-missing-path", "mos-eval",
        "semantic: #image/#figure missing a path argument";
    /// Image file cannot be read from disk.
    MOS0161 = 161, Error, "image-read-failed", "mos-eval",
        "semantic: image file cannot be read from disk";
    /// Image file cannot be decoded (unsupported or corrupt).
    MOS0162 = 162, Error, "image-decode-failed", "mos-eval",
        "semantic: image file cannot be decoded";

    // ── layout (mos-layout), 0200–0299 ────────────────────────────────
    /// Unknown paper size in `#set page(paper: ...)`.
    MOS0200 = 200, Error, "paper-size-unknown", "mos-layout",
        "layout: unknown paper size";
    /// Well-typed `#set` value breaks page geometry; previous value kept.
    MOS0201 = 201, Error, "geometry-breaks-page", "mos-layout",
        "layout: value breaks page geometry; previous value retained";
    /// Image reached layout without decoded pixels; skipped on the page.
    MOS0220 = 220, Warning, "image-skipped-no-pixels", "mos-layout",
        "layout: image reached layout without decoded pixels; skipped";

    // ── text / fonts / shaping, 0300–0399 ─────────────────────────────
    /// Unknown font family; falling back to bundled Noto Sans.
    MOS0300 = 300, Notice, "font-family-substituted", "mos-fonts",
        "font: substituted bundled Noto Sans for unknown family";
    /// Base-14 `/Differences` glyph budget exhausted for a face.
    MOS0310 = 310, Warning, "glyph-budget-exhausted", "mos-pdf",
        "font: Base-14 /Differences glyph budget exhausted";

    // ── PDF emission (mos-pdf), 0400–0499 ─────────────────────────────
    /// PDF backend I/O failure (cannot create dir or write bytes).
    MOS0400 = 400, Error, "pdf-io-failed", "mos-pdf",
        "pdf: backend I/O failure";
    /// Font subsetting failure for an embedded face.
    MOS0401 = 401, Error, "font-subset-failed", "mos-pdf",
        "pdf: font subsetting failure for an embedded face";
    /// Internal: missing embedded font plan for a shaped run.
    MOS0402 = 402, Error, "internal-missing-font-plan", "mos-pdf",
        "pdf: internal: missing embedded font plan for a shaped run";
}
