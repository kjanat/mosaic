//! Black-box tests for the CSL style parser, via the public API.
//!
//! Tests return `()` and use `expect`/`expect_err` (the workspace enables
//! `clippy::panic_in_result_fn`, so `Result`-returning tests with `assert!`
//! would themselves be clippy errors).

use mos_csl::{CslParseErrorKind, Element, Match, SortTarget, StyleClass, TextSource, parse_style};

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
fn malformed_xml_reports_real_offset() {
    let input = "<style version=\"1.0\" class=\"in-text\">\n  <\n</style>";
    let err = parse_style(input).expect_err("bad element start");
    assert!(matches!(err.kind(), CslParseErrorKind::MalformedXml(_)));
    assert_ne!(err.offset(), 0);
    assert_eq!(err.line_col(input).0, 2);
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
fn parses_rendering_variants_sort_and_conditions() {
    let style = parse_style(
        r#"<style version="1.0" class="note" page-range-format="expanded" demote-non-dropping-particle="sort-only" initialize-with-hyphen="false">
          <locale><terms><term name="page">p.</term></terms></locale>
          <macro name="term-macro">
            <text term="editor" form="short" plural="true"/>
          </macro>
          <citation et-al-min="3" et-al-use-first="1" et-al-subsequent-min="4" et-al-subsequent-use-first="2" collapse="year" cite-group-delimiter=", " disambiguate-add-names="true" disambiguate-add-givenname="true" disambiguate-add-year-suffix="true" givenname-disambiguation-rule="primary-name" near-note-distance="5">
            <sort>
              <key macro="term-macro" sort="descending" names-min="3" names-use-first="1" names-use-last="true"/>
              <key variable="issued"/>
            </sort>
            <layout prefix="[" suffix="]" delimiter=", " font-style="italic" font-variant="small-caps" font-weight="bold" text-decoration="underline" vertical-align="sup" text-case="capitalize-first" display="block">
              <number variable="volume" form="roman" prefix="v"/>
              <date variable="issued" form="text" date-parts="year-month-day">
                <date-part name="month" form="short" range-delimiter="/" suffix=" "/>
                <date-part name="year"/>
              </date>
              <names variable="author editor" delimiter=", ">
                <name form="short" and="symbol" et-al-min="3" et-al-use-first="1" et-al-subsequent-min="4" et-al-subsequent-use-first="2" et-al-use-last="true" delimiter-precedes-et-al="always" delimiter-precedes-last="contextual" initialize="true" initialize-with=". " name-as-sort-order="first" sort-separator=", "/>
                <et-al term="et-al"/>
                <label form="short" plural="contextual" strip-periods="true"/>
                <substitute><text value="Anonymous"/></substitute>
              </names>
              <label variable="page" form="short" plural="always" strip-periods="true"/>
              <choose>
                <if match="any" type="book article" variable="title issued" is-numeric="volume" is-uncertain-date="issued" locator="page" position="first subsequent" disambiguate="true">
                  <text variable="title" form="short"/>
                </if>
                <else-if match="none" variable="DOI"><text macro="term-macro"/></else-if>
                <else><text value="fallback" quotes="true" strip-periods="true"/></else>
              </choose>
            </layout>
          </citation>
          <bibliography et-al-min="5" et-al-use-first="2" et-al-subsequent-min="6" et-al-subsequent-use-first="3" hanging-indent="true" second-field-align="flush" line-spacing="2" entry-spacing="1" subsequent-author-substitute="---" subsequent-author-substitute-rule="partial-each">
            <sort><key variable="title" sort="descending"/></sort>
            <layout><text variable="title"/></layout>
          </bibliography>
        </style>"#,
    )
    .expect("style should parse");

    assert_eq!(style.class, StyleClass::Note);
    assert_eq!(style.options.page_range_format.as_deref(), Some("expanded"));
    assert_eq!(
        style.options.demote_non_dropping_particle.as_deref(),
        Some("sort-only")
    );
    assert_eq!(
        style.options.initialize_with_hyphen.as_deref(),
        Some("false")
    );
    assert_eq!(style.macros.len(), 1, "locale children should be skipped");
    let macro_elements = style.macros.get("term-macro").expect("macro present");
    assert!(
        matches!(
            &macro_elements[0],
            Element::Text(text)
                if text.source == (TextSource::Term {
                    name: "editor".to_owned(),
                    form: Some("short".to_owned()),
                    plural: true,
                })
        ),
        "macro should preserve term source"
    );

    let citation = style.citation.expect("citation present");
    assert_eq!(citation.options.et_al_min.as_deref(), Some("3"));
    assert_eq!(citation.options.et_al_use_first.as_deref(), Some("1"));
    assert_eq!(citation.options.et_al_subsequent_min.as_deref(), Some("4"));
    assert_eq!(
        citation.options.et_al_subsequent_use_first.as_deref(),
        Some("2")
    );
    assert_eq!(citation.options.collapse.as_deref(), Some("year"));
    assert_eq!(citation.options.cite_group_delimiter.as_deref(), Some(", "));
    assert_eq!(
        citation.options.disambiguate_add_names.as_deref(),
        Some("true")
    );
    assert_eq!(
        citation.options.disambiguate_add_givenname.as_deref(),
        Some("true")
    );
    assert_eq!(
        citation.options.disambiguate_add_year_suffix.as_deref(),
        Some("true")
    );
    assert_eq!(
        citation.options.givenname_disambiguation_rule.as_deref(),
        Some("primary-name")
    );
    assert_eq!(citation.options.near_note_distance.as_deref(), Some("5"));
    assert_eq!(citation.sort.len(), 2);
    assert_eq!(
        citation.sort[0].target,
        SortTarget::Macro("term-macro".to_owned())
    );
    assert!(citation.sort[0].descending);
    assert_eq!(citation.sort[0].options.names_min.as_deref(), Some("3"));
    assert_eq!(
        citation.sort[0].options.names_use_first.as_deref(),
        Some("1")
    );
    assert_eq!(
        citation.sort[0].options.names_use_last.as_deref(),
        Some("true")
    );
    assert_eq!(
        citation.sort[1].target,
        SortTarget::Variable("issued".to_owned())
    );
    assert!(!citation.sort[1].descending);

    let common = &citation.layout.common;
    assert_eq!(common.prefix.as_deref(), Some("["));
    assert_eq!(common.suffix.as_deref(), Some("]"));
    assert_eq!(common.delimiter.as_deref(), Some(", "));
    assert_eq!(common.font_style.as_deref(), Some("italic"));
    assert_eq!(common.font_variant.as_deref(), Some("small-caps"));
    assert_eq!(common.font_weight.as_deref(), Some("bold"));
    assert_eq!(common.text_decoration.as_deref(), Some("underline"));
    assert_eq!(common.vertical_align.as_deref(), Some("sup"));
    assert_eq!(common.text_case.as_deref(), Some("capitalize-first"));
    assert_eq!(common.display.as_deref(), Some("block"));

    let number = match &citation.layout.elements[0] {
        Element::Number(number) => Some(number),
        _ => None,
    }
    .expect("number element");
    assert_eq!(number.variable, "volume");
    assert_eq!(number.form.as_deref(), Some("roman"));
    assert_eq!(number.common.prefix.as_deref(), Some("v"));

    let date = match &citation.layout.elements[1] {
        Element::Date(date) => Some(date),
        _ => None,
    }
    .expect("date element");
    assert_eq!(date.variable, "issued");
    assert_eq!(date.form.as_deref(), Some("text"));
    assert_eq!(date.date_parts.as_deref(), Some("year-month-day"));
    assert_eq!(date.parts.len(), 2);
    assert_eq!(date.parts[0].name, "month");
    assert_eq!(date.parts[0].form.as_deref(), Some("short"));
    assert_eq!(date.parts[0].range_delimiter.as_deref(), Some("/"));
    assert_eq!(date.parts[0].common.suffix.as_deref(), Some(" "));
    assert_eq!(date.parts[1].name, "year");

    let names = match &citation.layout.elements[2] {
        Element::Names(names) => Some(names),
        _ => None,
    }
    .expect("names element");
    assert_eq!(
        names.variables,
        vec!["author".to_owned(), "editor".to_owned()]
    );
    let name = names.name.as_ref().expect("name child");
    assert_eq!(name.form.as_deref(), Some("short"));
    assert_eq!(name.and.as_deref(), Some("symbol"));
    assert_eq!(name.options.et_al_min.as_deref(), Some("3"));
    assert_eq!(name.options.et_al_use_first.as_deref(), Some("1"));
    assert_eq!(name.options.et_al_subsequent_min.as_deref(), Some("4"));
    assert_eq!(
        name.options.et_al_subsequent_use_first.as_deref(),
        Some("2")
    );
    assert_eq!(name.options.et_al_use_last.as_deref(), Some("true"));
    assert_eq!(
        name.options.delimiter_precedes_et_al.as_deref(),
        Some("always")
    );
    assert_eq!(
        name.options.delimiter_precedes_last.as_deref(),
        Some("contextual")
    );
    assert_eq!(name.options.initialize.as_deref(), Some("true"));
    assert_eq!(name.options.initialize_with.as_deref(), Some(". "));
    assert_eq!(name.options.name_as_sort_order.as_deref(), Some("first"));
    assert_eq!(name.options.sort_separator.as_deref(), Some(", "));
    assert_eq!(
        names.et_al.as_ref().and_then(|et_al| et_al.term.as_deref()),
        Some("et-al")
    );
    let label = names.label.as_ref().expect("label child");
    assert_eq!(label.form.as_deref(), Some("short"));
    assert_eq!(label.plural.as_deref(), Some("contextual"));
    assert_eq!(label.strip_periods.as_deref(), Some("true"));
    assert!(
        matches!(
            &names.substitute[0],
            Element::Text(text) if text.source == TextSource::Value("Anonymous".to_owned())
        ),
        "substitute should hold fallback text"
    );

    let label = match &citation.layout.elements[3] {
        Element::Label(label) => Some(label),
        _ => None,
    }
    .expect("label element");
    assert_eq!(label.variable.as_deref(), Some("page"));
    assert_eq!(label.form.as_deref(), Some("short"));
    assert_eq!(label.plural.as_deref(), Some("always"));
    assert_eq!(label.strip_periods.as_deref(), Some("true"));

    let choose = match &citation.layout.elements[4] {
        Element::Choose(choose) => Some(choose),
        _ => None,
    }
    .expect("choose element");
    assert_eq!(choose.branches.len(), 2);
    assert_eq!(choose.branches[0].conditions.match_mode, Match::Any);
    assert_eq!(
        choose.branches[0].conditions.kind,
        vec!["book".to_owned(), "article".to_owned()]
    );
    assert_eq!(
        choose.branches[0].conditions.variable,
        vec!["title".to_owned(), "issued".to_owned()]
    );
    assert_eq!(
        choose.branches[0].conditions.is_numeric,
        vec!["volume".to_owned()]
    );
    assert_eq!(
        choose.branches[0].conditions.is_uncertain_date,
        vec!["issued".to_owned()]
    );
    assert_eq!(
        choose.branches[0].conditions.locator,
        vec!["page".to_owned()]
    );
    assert_eq!(
        choose.branches[0].conditions.position,
        vec!["first".to_owned(), "subsequent".to_owned()]
    );
    assert!(choose.branches[0].conditions.disambiguate);
    assert_eq!(choose.branches[1].conditions.match_mode, Match::None);
    assert_eq!(
        choose.branches[1].conditions.variable,
        vec!["DOI".to_owned()]
    );
    assert!(
        matches!(
            &choose.otherwise[0],
            Element::Text(text)
                if text.source == TextSource::Value("fallback".to_owned())
                    && text.quotes
                    && text.strip_periods
        ),
        "else branch should preserve literal text options"
    );

    let bibliography = style.bibliography.expect("bibliography present");
    assert_eq!(bibliography.options.et_al_min.as_deref(), Some("5"));
    assert_eq!(bibliography.options.et_al_use_first.as_deref(), Some("2"));
    assert_eq!(
        bibliography.options.et_al_subsequent_min.as_deref(),
        Some("6")
    );
    assert_eq!(
        bibliography.options.et_al_subsequent_use_first.as_deref(),
        Some("3")
    );
    assert_eq!(bibliography.options.hanging_indent.as_deref(), Some("true"));
    assert_eq!(
        bibliography.options.second_field_align.as_deref(),
        Some("flush")
    );
    assert_eq!(bibliography.options.line_spacing.as_deref(), Some("2"));
    assert_eq!(bibliography.options.entry_spacing.as_deref(), Some("1"));
    assert_eq!(
        bibliography.options.subsequent_author_substitute.as_deref(),
        Some("---")
    );
    assert_eq!(
        bibliography
            .options
            .subsequent_author_substitute_rule
            .as_deref(),
        Some("partial-each")
    );
    assert_eq!(bibliography.sort.len(), 1);
    assert_eq!(
        bibliography.sort[0].target,
        SortTarget::Variable("title".to_owned())
    );
    assert!(bibliography.sort[0].descending);
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
fn bibliography_requires_a_layout() {
    let err = parse_style(r#"<style version="1.0" class="in-text"><bibliography/></style>"#)
        .expect_err("bibliography without a layout");
    assert_eq!(err.kind(), &CslParseErrorKind::MissingLayout);
}

#[test]
fn macro_requires_name() {
    let err = parse_style(r#"<style version="1.0" class="in-text"><macro/></style>"#)
        .expect_err("macro without name");
    assert_eq!(err.kind(), &CslParseErrorKind::MissingMacroName);
}

#[test]
fn rejects_unsupported_children_in_containers() {
    let citation_child = parse_style(
        r#"<style version="1.0" class="in-text"><citation><layout/><foo/></citation></style>"#,
    )
    .expect_err("unsupported citation child");
    assert!(matches!(
        citation_child.kind(),
        CslParseErrorKind::UnsupportedElement(name) if name == "foo"
    ));

    let names_child = parse_style(
        r#"<style version="1.0" class="in-text"><citation><layout><names variable="author"><foo/></names></layout></citation></style>"#,
    )
    .expect_err("unsupported names child");
    assert!(matches!(
        names_child.kind(),
        CslParseErrorKind::UnsupportedElement(name) if name == "foo"
    ));

    let choose_child = parse_style(
        r#"<style version="1.0" class="in-text"><citation><layout><choose><foo/></choose></layout></citation></style>"#,
    )
    .expect_err("unsupported choose child");
    assert!(matches!(
        choose_child.kind(),
        CslParseErrorKind::UnsupportedElement(name) if name == "foo"
    ));
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
