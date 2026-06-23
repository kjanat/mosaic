//! This crate provides Mosaic language support for the [tree-sitter] parsing library.
//!
//! Typically, you will use the [`LANGUAGE`] constant to add this language to a
//! tree-sitter [`Parser`], and then use the parser to parse some code:
//!
//! ```
//! let code = "= Hello\n\nA short paragraph.\n";
//! let mut parser = tree_sitter::Parser::new();
//! let language = tree_sitter_mosaic::LANGUAGE;
//! parser
//!     .set_language(&language.into())
//!     .expect("Error loading Mosaic parser");
//! let tree = parser.parse(code, None).unwrap();
//! assert!(!tree.root_node().has_error());
//! ```
//!
//! [`Parser`]: https://docs.rs/tree-sitter/0.26.8/tree_sitter/struct.Parser.html
//! [tree-sitter]: https://tree-sitter.github.io/

#![doc(
    html_logo_url = "https://mosaiclang.dev/assets/A4.svg",
    html_favicon_url = "https://mosaiclang.dev/assets/A4.svg"
)]

use tree_sitter_language::LanguageFn;

// SAFETY: `tree_sitter_mosaic` is the C entry point emitted by
// `tree-sitter generate` into `src/parser.c` and linked by `build.rs`.
// Its signature matches what `tree-sitter` expects from a grammar.
#[allow(
    unsafe_code,
    reason = "FFI declaration of the generated C parser entry point"
)]
unsafe extern "C" {
    fn tree_sitter_mosaic() -> *const ();
}

/// The tree-sitter [`LanguageFn`] for this grammar.
#[allow(
    unsafe_code,
    reason = "Wrap C parser entry point in tree_sitter_language::LanguageFn"
)]
// SAFETY: `tree_sitter_mosaic` returns a pointer to a `TSLanguage` whose
// shape matches the ABI declared by the linked `tree_sitter_language`
// crate; wrapping it in `LanguageFn::from_raw` is the documented way to
// expose a generated grammar to consumers.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_mosaic) };

/// The content of the [`node-types.json`] file for this grammar.
///
/// [`node-types.json`]: https://tree-sitter.github.io/tree-sitter/using-parsers/6-static-node-types
pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

#[cfg(with_highlights_query)]
/// The syntax highlighting query for this grammar.
pub const HIGHLIGHTS_QUERY: &str = include_str!("../../queries/highlights.scm");

#[cfg(with_injections_query)]
/// The language injection query for this grammar.
pub const INJECTIONS_QUERY: &str = include_str!("../../queries/injections.scm");

#[cfg(with_locals_query)]
/// The local variable query for this grammar.
pub const LOCALS_QUERY: &str = include_str!("../../queries/locals.scm");

#[cfg(with_tags_query)]
/// The symbol tagging query for this grammar.
pub const TAGS_QUERY: &str = include_str!("../../queries/tags.scm");

#[cfg(test)]
mod tests {
    fn parse_sexp(source: &str) -> Option<String> {
        let mut parser = tree_sitter::Parser::new();
        let language = super::LANGUAGE.into();
        let language_result = parser.set_language(&language);
        assert!(
            language_result.is_ok(),
            "Error loading Mosaic parser: {:?}",
            language_result.err()
        );

        let tree = parser.parse(source, None);
        assert!(tree.is_some(), "parser returned no tree");
        let tree = tree?;
        let root = tree.root_node();
        assert!(
            !root.has_error(),
            "unexpected parse error: {}",
            root.to_sexp()
        );
        Some(root.to_sexp())
    }

    #[test]
    fn test_can_load_grammar() {
        let mut parser = tree_sitter::Parser::new();
        let language = super::LANGUAGE.into();
        let language_result = parser.set_language(&language);

        assert!(
            language_result.is_ok(),
            "Error loading Mosaic parser: {:?}",
            language_result.err()
        );
    }

    #[test]
    fn parses_byte_zero_shebang_with_crlf() {
        let sexp = parse_sexp("#!/usr/bin/env -S mos build --open\r\n= Hello\n");

        assert_eq!(
            sexp.as_deref(),
            Some(
                "(source_file (shebang) (section (section1 (heading marker: (heading_marker) content: (inline_sequence (text))))))"
            )
        );
    }
}
