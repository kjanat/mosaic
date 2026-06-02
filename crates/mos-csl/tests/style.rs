//! Black-box tests for the CSL style parser, via the public API.
//!
//! Tests return `()` and use `expect`/`expect_err` (the workspace enables
//! `clippy::panic_in_result_fn`, so `Result`-returning tests with `assert!`
//! would themselves be clippy errors).

use mos_csl::{CslParseErrorKind, Element, StyleClass, TextSource, parse_style};

const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text" default-locale="en-US">
  <info>
    <id>http://example.org/styles/demo</id>
    <title>Demo Style</title>
  </info>
  <macro name="author">
    <names variable="author">
      <name and="text" delimiter=", "/>
      <et-al term="and others"/>
    </names>
  </macro>
  <citation>
    <sort>
      <key variable="citation-number"/>
    </sort>
    <layout prefix="(" suffix=")" delimiter="; ">
      <text macro="author"/>
      <text variable="title" font-style="italic"/>
    </layout>
  </citation>
  <bibliography>
    <layout>
      <group delimiter=". ">
        <text macro="author"/>
        <date variable="issued" form="numeric"/>
      </group>
    </layout>
  </bibliography>
</style>"#;

#[test]
fn parses_a_full_style() {
    let style = parse_style(SAMPLE).expect("sample should parse");
    assert_eq!(style.class, StyleClass::InText);
    assert_eq!(style.version, "1.0");
    assert_eq!(style.default_locale.as_deref(), Some("en-US"));
    assert_eq!(style.info.title.as_deref(), Some("Demo Style"));
    assert_eq!(
        style.info.id.as_deref(),
        Some("http://example.org/styles/demo")
    );
    assert!(style.macros.contains_key("author"));

    let citation = style.citation.expect("citation present");
    assert_eq!(citation.sort.len(), 1);
    assert_eq!(citation.layout.common.prefix.as_deref(), Some("("));
    assert_eq!(citation.layout.common.delimiter.as_deref(), Some("; "));
    assert_eq!(citation.layout.elements.len(), 2);
    assert!(
        matches!(
            &citation.layout.elements[0],
            Element::Text(text) if text.source == TextSource::Macro("author".to_owned())
        ),
        "first layout element should be <text macro=\"author\">"
    );
    assert!(style.bibliography.is_some());
}

#[test]
fn rejects_malformed_xml() {
    let err = parse_style(r#"<style version="1.0" class="in-text">"#).expect_err("unclosed root");
    assert!(matches!(err.kind(), CslParseErrorKind::MalformedXml(_)));
}

#[test]
fn rejects_a_wrong_root_element() {
    let err = parse_style("<not-a-style/>").expect_err("wrong root");
    assert!(matches!(err.kind(), CslParseErrorKind::UnexpectedRoot(_)));
}

#[test]
fn requires_version_and_class() {
    let no_version = parse_style(r#"<style class="in-text"/>"#).expect_err("missing version");
    assert_eq!(no_version.kind(), &CslParseErrorKind::MissingVersion);

    let no_class = parse_style(r#"<style version="1.0"/>"#).expect_err("missing class");
    assert_eq!(no_class.kind(), &CslParseErrorKind::MissingClass);

    let bad_class =
        parse_style(r#"<style version="1.0" class="weird"/>"#).expect_err("unknown class");
    assert!(matches!(
        bad_class.kind(),
        CslParseErrorKind::UnknownClass(_)
    ));
}

#[test]
fn rejects_an_unsupported_element() {
    let err = parse_style(
        r#"<style version="1.0" class="in-text"><citation><layout><frobnicate/></layout></citation></style>"#,
    )
    .expect_err("unsupported element");
    assert!(matches!(
        err.kind(),
        CslParseErrorKind::UnsupportedElement(_)
    ));
}

#[test]
fn text_requires_a_source() {
    let err = parse_style(
        r#"<style version="1.0" class="in-text"><citation><layout><text/></layout></citation></style>"#,
    )
    .expect_err("text without a source");
    assert_eq!(err.kind(), &CslParseErrorKind::TextWithoutSource);
}

#[test]
fn citation_requires_a_layout() {
    let err = parse_style(r#"<style version="1.0" class="in-text"><citation/></style>"#)
        .expect_err("citation without a layout");
    assert_eq!(err.kind(), &CslParseErrorKind::MissingLayout);
}

#[test]
fn arbitrary_input_never_panics() {
    let inputs = [
        "",
        "<",
        "<style>",
        "<style/>",
        "not xml at all",
        r#"<style version="1.0" class="note"/>"#,
        r#"<style version="1.0" class="in-text"><macro/></style>"#,
    ];
    for input in inputs {
        // Discard the result; a panic here would fail the test.
        let _ = parse_style(input);
    }
}
