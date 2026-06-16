//! Black-box tests for the `mos-bib` minimal BibTeX record parser, driven
//! entirely through the public API.
//!
//! Tests return `()` and use `expect`/`expect_err` (allowed in tests by
//! `clippy.toml`): the workspace also enables `clippy::panic_in_result_fn`, so a
//! `Result`-returning test with `assert!` would itself be a clippy error.

use mos_bib::{BibEntry, BibParseErrorKind, parse_bibtex};

/// Field names in their (sorted) iteration order.
fn field_names(entry: &BibEntry) -> Vec<&str> {
    entry.fields.keys().map(String::as_str).collect()
}

#[test]
fn parses_minimal_article() {
    let bib = parse_bibtex(
        "@article{knuth1984, title = {Literate Programming}, author = {Donald Knuth}, year = {1984}}",
    )
    .expect("input should parse");
    assert_eq!(bib.entries.len(), 1);
    let entry = bib.entries.get("knuth1984").expect("entry present");
    assert_eq!(entry.entry_type, "article");
    assert_eq!(entry.key, "knuth1984");
    assert_eq!(
        entry.fields.get("title").map(String::as_str),
        Some("Literate Programming")
    );
    assert_eq!(
        entry.fields.get("author").map(String::as_str),
        Some("Donald Knuth")
    );
    assert_eq!(entry.fields.get("year").map(String::as_str), Some("1984"));
}

#[test]
fn accepts_quoted_values() {
    let bib =
        parse_bibtex(r#"@article{lovelace, title = "A Quoted Title", author = "Ada Lovelace"}"#)
            .expect("input should parse");
    let entry = bib.entries.get("lovelace").expect("entry present");
    assert_eq!(
        entry.fields.get("title").map(String::as_str),
        Some("A Quoted Title")
    );
    assert_eq!(
        entry.fields.get("author").map(String::as_str),
        Some("Ada Lovelace")
    );
}

#[test]
fn quoted_values_keep_tex_accents_verbatim() {
    let bib =
        parse_bibtex(r#"@article{k, title = "Schr{\"o}dinger"}"#).expect("input should parse");
    let entry = bib.entries.get("k").expect("entry present");
    assert_eq!(
        entry.fields.get("title").map(String::as_str),
        Some(r#"Schr{\"o}dinger"#)
    );
}

#[test]
fn quoted_values_keep_escaped_quotes_verbatim() {
    let bib = parse_bibtex(r#"@article{k, title = "He said \"hi\""}"#).expect("input should parse");
    let entry = bib.entries.get("k").expect("entry present");
    assert_eq!(
        entry.fields.get("title").map(String::as_str),
        Some(r#"He said \"hi\""#)
    );
}

#[test]
fn accepts_mixed_quote_and_brace_values() {
    let bib = parse_bibtex(r#"@article{k, title = {Braced}, author = "Quoted"}"#)
        .expect("input should parse");
    let entry = bib.entries.get("k").expect("entry present");
    assert_eq!(
        entry.fields.get("title").map(String::as_str),
        Some("Braced")
    );
    assert_eq!(
        entry.fields.get("author").map(String::as_str),
        Some("Quoted")
    );
}

#[test]
fn accepts_bare_numeric_value() {
    // BibTeX lets numbers stand alone (e.g. `year = 1984`).
    let bib = parse_bibtex("@article{k, year = 1984}").expect("input should parse");
    let entry = bib.entries.get("k").expect("entry present");
    assert_eq!(entry.fields.get("year").map(String::as_str), Some("1984"));
}

#[test]
fn parses_multiple_entries() {
    let bib = parse_bibtex("@article{a, title = {First}}\n@book{b, title = {Second}}\n")
        .expect("input should parse");
    assert_eq!(bib.entries.len(), 2);
    let first = bib.entries.get("a").expect("entry a present");
    let second = bib.entries.get("b").expect("entry b present");
    assert_eq!(first.entry_type, "article");
    assert_eq!(second.entry_type, "book");
}

#[test]
fn field_order_is_deterministic() {
    // Fields are supplied out of alphabetical order; the BTreeMap sorts them,
    // so iteration order is stable regardless of source order.
    let bib = parse_bibtex("@article{k, year = {1984}, author = {K}, title = {T}}")
        .expect("input should parse");
    let entry = bib.entries.get("k").expect("entry present");
    assert_eq!(field_names(entry), ["author", "title", "year"]);
}

#[test]
fn normalizes_type_and_field_case_but_preserves_key() {
    // BibTeX entry types and tag names are case-insensitive; keys are not.
    let bib =
        parse_bibtex("@ARTICLE{Knuth1984, Title = {T}, AUTHOR = {A}}").expect("input should parse");
    let entry = bib
        .entries
        .get("Knuth1984")
        .expect("key preserved verbatim");
    assert_eq!(entry.entry_type, "article");
    assert_eq!(field_names(entry), ["author", "title"]);
}

#[test]
fn accepts_trailing_comma() {
    let bib = parse_bibtex("@article{k, title = {T}, year = {1984},}").expect("input should parse");
    let entry = bib.entries.get("k").expect("entry present");
    assert_eq!(entry.fields.len(), 2);
}

#[test]
fn rejects_trailing_comma_without_fields() {
    // A comma after the key must be followed by at least one field, so
    // `@type{key,}` is malformed (distinct from the field-less `@type{key}`).
    let err = parse_bibtex("@article{k,}").expect_err("comma without a field should be rejected");
    assert_eq!(err.kind(), BibParseErrorKind::ExpectedFieldName);
}

#[test]
fn accepts_entry_without_fields() {
    let bib = parse_bibtex("@misc{k}").expect("input should parse");
    let entry = bib.entries.get("k").expect("entry present");
    assert_eq!(entry.entry_type, "misc");
    assert!(entry.fields.is_empty());
}

#[test]
fn balances_nested_braces_in_values() {
    let bib =
        parse_bibtex("@article{k, title = {The {LaTeX} Companion}}").expect("input should parse");
    let entry = bib.entries.get("k").expect("entry present");
    assert_eq!(
        entry.fields.get("title").map(String::as_str),
        Some("The {LaTeX} Companion")
    );
}

#[test]
fn ignores_surrounding_whitespace() {
    let bib =
        parse_bibtex("\n\n  @article{ k ,\n    title = {T} ,\n  }\n").expect("input should parse");
    let entry = bib.entries.get("k").expect("entry present");
    assert_eq!(entry.fields.get("title").map(String::as_str), Some("T"));
}

#[test]
fn captures_unicode_and_latex_in_values_verbatim() {
    // No TeX decoding: backslash commands and accents survive untouched, and
    // multi-byte UTF-8 in values is preserved.
    let bib = parse_bibtex(r#"@article{k, title = {Caf\'{e} \LaTeX}, note = {naïve}}"#)
        .expect("input should parse");
    let entry = bib.entries.get("k").expect("entry present");
    assert_eq!(
        entry.fields.get("title").map(String::as_str),
        Some(r"Caf\'{e} \LaTeX")
    );
    assert_eq!(entry.fields.get("note").map(String::as_str), Some("naïve"));
}

#[test]
fn empty_input_yields_empty_bibliography() {
    assert!(parse_bibtex("").expect("empty parses").entries.is_empty());
    assert!(
        parse_bibtex("   \n\t  ")
            .expect("whitespace parses")
            .entries
            .is_empty()
    );
}

#[test]
fn duplicate_key_is_rejected() {
    let err = parse_bibtex("@article{k, title = {First}}@article{k, title = {Second}}")
        .expect_err("duplicate key should be rejected");
    assert_eq!(err.kind(), BibParseErrorKind::DuplicateKey);
}

#[test]
fn malformed_input_reports_useful_error_kind() {
    let cases = [
        ("article{k}", BibParseErrorKind::ExpectedAt),
        ("@{k}", BibParseErrorKind::ExpectedEntryType),
        ("@article k}", BibParseErrorKind::ExpectedOpenBrace),
        ("@article{, title = {T}}", BibParseErrorKind::ExpectedKey),
        ("@article{k, title {T}}", BibParseErrorKind::ExpectedEquals),
        ("@article{k, = {T}}", BibParseErrorKind::ExpectedFieldName),
        ("@article{k, title = @}", BibParseErrorKind::ExpectedValue),
        (
            "@article{k, title = {T} year = {1984}}",
            BibParseErrorKind::ExpectedCommaOrCloseBrace,
        ),
    ];
    for (input, expected) in cases {
        let err = parse_bibtex(input).expect_err("malformed input should be rejected");
        assert_eq!(err.kind(), expected, "input: {input:?}");
    }
}

#[test]
fn unterminated_brace_value_is_reported() {
    let err = parse_bibtex("@article{k, title = {oops").expect_err("unterminated value");
    assert_eq!(err.kind(), BibParseErrorKind::UnterminatedValue);
}

#[test]
fn unterminated_quote_value_is_reported() {
    let err = parse_bibtex(r#"@article{k, title = "oops}"#).expect_err("unterminated value");
    assert_eq!(err.kind(), BibParseErrorKind::UnterminatedValue);
}

#[test]
fn unterminated_entry_is_reported() {
    // The value is closed, but the entry's `}` never arrives.
    let err = parse_bibtex("@article{k, title = {ok}").expect_err("unterminated entry");
    assert_eq!(err.kind(), BibParseErrorKind::UnterminatedEntry);
}

#[test]
fn string_concatenation_is_rejected_not_panicked() {
    // `#` concatenation is an `@string` feature and out of scope; it must
    // produce an error, never a panic.
    let err = parse_bibtex(r#"@article{k, publisher = "nob" # "ody"}"#)
        .expect_err("concatenation unsupported");
    assert_eq!(err.kind(), BibParseErrorKind::ExpectedCommaOrCloseBrace);
}

#[test]
fn error_carries_offset_message_and_line_col() {
    let src = "   article{k}";
    let err = parse_bibtex(src).expect_err("missing '@'");
    assert_eq!(err.kind(), BibParseErrorKind::ExpectedAt);
    assert_eq!(err.offset(), 3);
    let shown = err.to_string();
    assert!(shown.contains("byte 3"), "got: {shown}");
    assert!(shown.contains('@'), "got: {shown}");
    assert_eq!(err.line_col(src), (1, 4));
}

#[test]
fn error_bridges_to_a_core_diagnostic() {
    // The local error converts into the standard `mos-core` diagnostic
    // (`MOS0043`) carrying the byte offset as a span; no parallel pipeline.
    let err = parse_bibtex("nope").expect_err("malformed input should be rejected");
    let diagnostic = err.to_diagnostic("refs.bib");
    assert_eq!(diagnostic.def().code().to_string(), "MOS0043");
    let span = diagnostic.span().expect("diagnostic should carry a span");
    assert_eq!(span.start(), err.offset());
}

#[test]
fn arbitrary_input_never_panics() {
    // Each of these is malformed or partial; the contract is only that parsing
    // returns (Ok or Err) rather than panicking.
    let inputs = [
        "@",
        "@@@@",
        "@article",
        "@article{",
        "@article{k",
        "@article{k,",
        "@article{k, t",
        "@article{k, t =",
        "@article{k, t = ",
        "@article{k, t = {}}",
        "{}}}",
        "====",
        "\"\"\"",
        "@a{b,c={d}",
        "@a{b,c=\"d",
        "%ignored?",
        "@ {x}",
        "@article{ {nested} }",
        "café not bibtex",
        "@article{k, title = {caf\u{e9}}}",
    ];
    for input in inputs {
        // Discard the result; a panic here would fail the test.
        let _ = parse_bibtex(input);
    }
}
