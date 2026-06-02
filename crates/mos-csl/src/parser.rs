//! Parse a CSL 1.0.2 style document into the typed [`Style`] AST.
//!
//! A read-only [`roxmltree`] DOM walk, dispatching on element local names (so
//! it tolerates the CSL namespace or its absence). It models the structure and
//! common attributes; unmodelled attributes are ignored, unknown rendering
//! elements are a [`CslParseError`], and in-style `<locale>` blocks are skipped
//! (locale parsing is a later slice).

use std::collections::BTreeMap;

use roxmltree::{Document, Node};

use crate::error::{CslParseError, CslParseErrorKind};
use crate::style::{
    Bibliography, BibliographyOptions, Branch, Choose, Citation, CitationOptions, Common,
    Conditions, DateElement, DatePart, Element, EtAl, Group, Info, Label, Layout, Match,
    NameElement, NameOptions, Names, Number, SortKey, SortKeyOptions, SortTarget, Style,
    StyleClass, StyleOptions, Text, TextSource,
};

/// Parse `input` as a CSL 1.0.2 style.
///
/// # Errors
///
/// Returns a [`CslParseError`] when `input` is not well-formed XML, the root is
/// not `<style>`, the required `version`/`class` attributes are missing or
/// invalid, a `<macro>` lacks a `name`, a `<citation>`/`<bibliography>` lacks
/// its `<layout>`, a `<text>` selects no source, or an unsupported rendering
/// element is encountered.
///
/// # Examples
///
/// ```
/// use mos_csl::{parse_style, StyleClass};
///
/// let style = parse_style(
///     r#"<style version="1.0" class="in-text">
///          <info><title>Demo</title></info>
///          <citation><layout><text variable="title"/></layout></citation>
///        </style>"#,
/// )
/// .expect("valid CSL");
/// assert_eq!(style.class, StyleClass::InText);
/// assert!(style.citation.is_some());
/// ```
pub fn parse_style(input: &str) -> Result<Style, CslParseError> {
    let document = Document::parse(input).map_err(|error| {
        let offset = text_pos_to_byte_offset(input, error.pos()).unwrap_or(0);
        CslParseError::new(CslParseErrorKind::MalformedXml(error.to_string()), offset)
    })?;
    let root = document.root_element();
    if root.tag_name().name() != "style" {
        let name = root.tag_name().name().to_owned();
        return Err(err_at(root, CslParseErrorKind::UnexpectedRoot(name)));
    }

    let version = root
        .attribute("version")
        .ok_or_else(|| err_at(root, CslParseErrorKind::MissingVersion))?
        .to_owned();
    let class = match root.attribute("class") {
        Some("in-text") => StyleClass::InText,
        Some("note") => StyleClass::Note,
        Some(other) => {
            return Err(err_at(
                root,
                CslParseErrorKind::UnknownClass(other.to_owned()),
            ));
        }
        None => return Err(err_at(root, CslParseErrorKind::MissingClass)),
    };
    let default_locale = attr(root, "default-locale");

    let mut info = Info::default();
    let mut citation = None;
    let mut bibliography = None;
    let mut macros = BTreeMap::new();

    for child in child_elements(root) {
        match child.tag_name().name() {
            "info" => info = parse_info(child),
            "citation" => citation = Some(parse_citation(child)?),
            "bibliography" => bibliography = Some(parse_bibliography(child)?),
            "macro" => {
                let name = child
                    .attribute("name")
                    .ok_or_else(|| err_at(child, CslParseErrorKind::MissingMacroName))?;
                macros.insert(name.to_owned(), parse_elements(child)?);
            }
            // In-style locale data is not parsed in this slice.
            "locale" => {}
            other => {
                return Err(err_at(
                    child,
                    CslParseErrorKind::UnsupportedElement(other.to_owned()),
                ));
            }
        }
    }

    Ok(Style {
        class,
        version,
        default_locale,
        options: parse_style_options(root),
        info,
        citation,
        bibliography,
        macros,
    })
}

fn parse_info(node: Node<'_, '_>) -> Info {
    let mut info = Info::default();
    for child in child_elements(node) {
        match child.tag_name().name() {
            "id" => info.id = child.text().map(str::to_owned),
            "title" => info.title = child.text().map(str::to_owned),
            // Other <info> children (author, link, updated, …) are ignored.
            _ => {}
        }
    }
    info
}

fn parse_citation(node: Node<'_, '_>) -> Result<Citation, CslParseError> {
    let (layout, sort) = parse_layout_and_sort(node)?;
    Ok(Citation {
        layout,
        sort,
        options: parse_citation_options(node),
    })
}

fn parse_bibliography(node: Node<'_, '_>) -> Result<Bibliography, CslParseError> {
    let (layout, sort) = parse_layout_and_sort(node)?;
    Ok(Bibliography {
        layout,
        sort,
        options: parse_bibliography_options(node),
    })
}

fn parse_layout_and_sort(node: Node<'_, '_>) -> Result<(Layout, Vec<SortKey>), CslParseError> {
    let mut layout = None;
    let mut sort = Vec::new();
    for child in child_elements(node) {
        match child.tag_name().name() {
            "layout" => layout = Some(parse_layout(child)?),
            "sort" => sort = parse_sort(child),
            other => {
                return Err(err_at(
                    child,
                    CslParseErrorKind::UnsupportedElement(other.to_owned()),
                ));
            }
        }
    }
    let layout = layout.ok_or_else(|| err_at(node, CslParseErrorKind::MissingLayout))?;
    Ok((layout, sort))
}

fn parse_layout(node: Node<'_, '_>) -> Result<Layout, CslParseError> {
    Ok(Layout {
        elements: parse_elements(node)?,
        common: parse_common(node),
    })
}

fn parse_sort(node: Node<'_, '_>) -> Vec<SortKey> {
    let mut keys = Vec::new();
    for child in child_elements(node) {
        if child.tag_name().name() == "key" {
            let target = child.attribute("macro").map_or_else(
                || SortTarget::Variable(attr(child, "variable").unwrap_or_default()),
                |name| SortTarget::Macro(name.to_owned()),
            );
            keys.push(SortKey {
                target,
                descending: child.attribute("sort") == Some("descending"),
                options: parse_sort_key_options(child),
            });
        }
    }
    keys
}

fn parse_elements(node: Node<'_, '_>) -> Result<Vec<Element>, CslParseError> {
    let mut elements = Vec::new();
    for child in child_elements(node) {
        elements.push(parse_element(child)?);
    }
    Ok(elements)
}

fn parse_element(node: Node<'_, '_>) -> Result<Element, CslParseError> {
    let element = match node.tag_name().name() {
        "text" => Element::Text(parse_text(node)?),
        "number" => Element::Number(parse_number(node)),
        "date" => Element::Date(parse_date(node)),
        "names" => Element::Names(Box::new(parse_names(node)?)),
        "label" => Element::Label(parse_label(node)),
        "group" => Element::Group(parse_group(node)?),
        "choose" => Element::Choose(parse_choose(node)?),
        other => {
            return Err(err_at(
                node,
                CslParseErrorKind::UnsupportedElement(other.to_owned()),
            ));
        }
    };
    Ok(element)
}

fn parse_text(node: Node<'_, '_>) -> Result<Text, CslParseError> {
    let source = if let Some(variable) = node.attribute("variable") {
        TextSource::Variable {
            name: variable.to_owned(),
            form: attr(node, "form"),
        }
    } else if let Some(name) = node.attribute("macro") {
        TextSource::Macro(name.to_owned())
    } else if let Some(term) = node.attribute("term") {
        TextSource::Term {
            name: term.to_owned(),
            form: attr(node, "form"),
            plural: bool_attr(node, "plural"),
        }
    } else if let Some(value) = node.attribute("value") {
        TextSource::Value(value.to_owned())
    } else {
        return Err(err_at(node, CslParseErrorKind::TextWithoutSource));
    };
    Ok(Text {
        source,
        quotes: bool_attr(node, "quotes"),
        strip_periods: bool_attr(node, "strip-periods"),
        common: parse_common(node),
    })
}

fn parse_number(node: Node<'_, '_>) -> Number {
    Number {
        variable: attr(node, "variable").unwrap_or_default(),
        form: attr(node, "form"),
        common: parse_common(node),
    }
}

fn parse_date(node: Node<'_, '_>) -> DateElement {
    let mut parts = Vec::new();
    for child in child_elements(node) {
        if child.tag_name().name() == "date-part" {
            parts.push(DatePart {
                name: attr(child, "name").unwrap_or_default(),
                form: attr(child, "form"),
                range_delimiter: attr(child, "range-delimiter"),
                common: parse_common(child),
            });
        }
    }
    DateElement {
        variable: attr(node, "variable").unwrap_or_default(),
        form: attr(node, "form"),
        date_parts: attr(node, "date-parts"),
        parts,
        common: parse_common(node),
    }
}

fn parse_names(node: Node<'_, '_>) -> Result<Names, CslParseError> {
    let variables = attr(node, "variable")
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let mut name = None;
    let mut et_al = None;
    let mut label = None;
    let mut substitute = Vec::new();
    for child in child_elements(node) {
        match child.tag_name().name() {
            "name" => {
                name = Some(NameElement {
                    form: attr(child, "form"),
                    and: attr(child, "and"),
                    options: parse_name_options(child),
                    common: parse_common(child),
                });
            }
            "et-al" => {
                et_al = Some(EtAl {
                    term: attr(child, "term"),
                    common: parse_common(child),
                });
            }
            "label" => label = Some(parse_label(child)),
            "substitute" => substitute = parse_elements(child)?,
            other => {
                return Err(err_at(
                    child,
                    CslParseErrorKind::UnsupportedElement(other.to_owned()),
                ));
            }
        }
    }
    Ok(Names {
        variables,
        name,
        et_al,
        label,
        substitute,
        common: parse_common(node),
    })
}

fn parse_label(node: Node<'_, '_>) -> Label {
    Label {
        variable: attr(node, "variable"),
        form: attr(node, "form"),
        plural: attr(node, "plural"),
        strip_periods: attr(node, "strip-periods"),
        common: parse_common(node),
    }
}

fn parse_group(node: Node<'_, '_>) -> Result<Group, CslParseError> {
    Ok(Group {
        children: parse_elements(node)?,
        common: parse_common(node),
    })
}

fn parse_choose(node: Node<'_, '_>) -> Result<Choose, CslParseError> {
    let mut branches = Vec::new();
    let mut otherwise = Vec::new();
    for child in child_elements(node) {
        match child.tag_name().name() {
            "if" | "else-if" => branches.push(Branch {
                conditions: parse_conditions(child),
                children: parse_elements(child)?,
            }),
            "else" => otherwise = parse_elements(child)?,
            other => {
                return Err(err_at(
                    child,
                    CslParseErrorKind::UnsupportedElement(other.to_owned()),
                ));
            }
        }
    }
    Ok(Choose {
        branches,
        otherwise,
    })
}

fn parse_conditions(node: Node<'_, '_>) -> Conditions {
    let match_mode = match node.attribute("match") {
        Some("any") => Match::Any,
        Some("none") => Match::None,
        _ => Match::All,
    };
    Conditions {
        match_mode,
        kind: tokens(node, "type"),
        variable: tokens(node, "variable"),
        is_numeric: tokens(node, "is-numeric"),
        is_uncertain_date: tokens(node, "is-uncertain-date"),
        locator: tokens(node, "locator"),
        position: tokens(node, "position"),
        disambiguate: bool_attr(node, "disambiguate"),
    }
}

fn parse_common(node: Node<'_, '_>) -> Common {
    Common {
        prefix: attr(node, "prefix"),
        suffix: attr(node, "suffix"),
        delimiter: attr(node, "delimiter"),
        font_style: attr(node, "font-style"),
        font_variant: attr(node, "font-variant"),
        font_weight: attr(node, "font-weight"),
        text_decoration: attr(node, "text-decoration"),
        vertical_align: attr(node, "vertical-align"),
        text_case: attr(node, "text-case"),
        display: attr(node, "display"),
    }
}

fn parse_style_options(node: Node<'_, '_>) -> StyleOptions {
    StyleOptions {
        page_range_format: attr(node, "page-range-format"),
        demote_non_dropping_particle: attr(node, "demote-non-dropping-particle"),
        initialize_with_hyphen: attr(node, "initialize-with-hyphen"),
    }
}

fn parse_citation_options(node: Node<'_, '_>) -> CitationOptions {
    CitationOptions {
        et_al_min: attr(node, "et-al-min"),
        et_al_use_first: attr(node, "et-al-use-first"),
        et_al_subsequent_min: attr(node, "et-al-subsequent-min"),
        et_al_subsequent_use_first: attr(node, "et-al-subsequent-use-first"),
        collapse: attr(node, "collapse"),
        cite_group_delimiter: attr(node, "cite-group-delimiter"),
        disambiguate_add_names: attr(node, "disambiguate-add-names"),
        disambiguate_add_givenname: attr(node, "disambiguate-add-givenname"),
        disambiguate_add_year_suffix: attr(node, "disambiguate-add-year-suffix"),
        givenname_disambiguation_rule: attr(node, "givenname-disambiguation-rule"),
        near_note_distance: attr(node, "near-note-distance"),
    }
}

fn parse_bibliography_options(node: Node<'_, '_>) -> BibliographyOptions {
    BibliographyOptions {
        et_al_min: attr(node, "et-al-min"),
        et_al_use_first: attr(node, "et-al-use-first"),
        et_al_subsequent_min: attr(node, "et-al-subsequent-min"),
        et_al_subsequent_use_first: attr(node, "et-al-subsequent-use-first"),
        hanging_indent: attr(node, "hanging-indent"),
        second_field_align: attr(node, "second-field-align"),
        line_spacing: attr(node, "line-spacing"),
        entry_spacing: attr(node, "entry-spacing"),
        subsequent_author_substitute: attr(node, "subsequent-author-substitute"),
        subsequent_author_substitute_rule: attr(node, "subsequent-author-substitute-rule"),
    }
}

fn parse_sort_key_options(node: Node<'_, '_>) -> SortKeyOptions {
    SortKeyOptions {
        names_min: attr(node, "names-min"),
        names_use_first: attr(node, "names-use-first"),
        names_use_last: attr(node, "names-use-last"),
    }
}

fn parse_name_options(node: Node<'_, '_>) -> NameOptions {
    NameOptions {
        et_al_min: attr(node, "et-al-min"),
        et_al_use_first: attr(node, "et-al-use-first"),
        et_al_subsequent_min: attr(node, "et-al-subsequent-min"),
        et_al_subsequent_use_first: attr(node, "et-al-subsequent-use-first"),
        et_al_use_last: attr(node, "et-al-use-last"),
        delimiter_precedes_et_al: attr(node, "delimiter-precedes-et-al"),
        delimiter_precedes_last: attr(node, "delimiter-precedes-last"),
        initialize: attr(node, "initialize"),
        initialize_with: attr(node, "initialize-with"),
        name_as_sort_order: attr(node, "name-as-sort-order"),
        sort_separator: attr(node, "sort-separator"),
    }
}

/// Element-only children of `node` (skips text, comments, and whitespace).
fn child_elements<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(Node::is_element)
}

/// An attribute as an owned `String`, if present.
fn attr(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.attribute(name).map(str::to_owned)
}

/// A boolean attribute: `true` only when the value is exactly `"true"`.
fn bool_attr(node: Node<'_, '_>, name: &str) -> bool {
    node.attribute(name) == Some("true")
}

/// A whitespace-separated attribute split into owned tokens.
fn tokens(node: Node<'_, '_>, name: &str) -> Vec<String> {
    node.attribute(name)
        .map(|value| value.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default()
}

fn text_pos_to_byte_offset(input: &str, position: roxmltree::TextPos) -> Option<usize> {
    let row = usize::try_from(position.row).ok()?;
    let col = usize::try_from(position.col).ok()?;
    if row == 0 || col == 0 {
        return None;
    }

    let (line_start, line) = line_at(input, row)?;
    let col_offset = column_to_byte_offset(line, col)?;
    Some(line_start + col_offset)
}

fn line_at(input: &str, row: usize) -> Option<(usize, &str)> {
    let mut line_start = 0;
    for (line_index, line) in input.split_inclusive('\n').enumerate() {
        if line_index + 1 == row {
            let line_without_newline = match line.strip_suffix('\n') {
                Some(stripped) => stripped,
                None => line,
            };
            return Some((line_start, line_without_newline));
        }
        line_start += line.len();
    }

    if row == 1 && input.is_empty() {
        return Some((0, ""));
    }
    None
}

fn column_to_byte_offset(line: &str, col: usize) -> Option<usize> {
    let target_chars = col.checked_sub(1)?;
    let mut chars_seen = 0;
    for (byte_offset, _) in line.char_indices() {
        if chars_seen == target_chars {
            return Some(byte_offset);
        }
        chars_seen += 1;
    }

    if chars_seen == target_chars {
        Some(line.len())
    } else {
        None
    }
}

/// Build an error anchored at a node's start byte offset.
fn err_at(node: Node<'_, '_>, kind: CslParseErrorKind) -> CslParseError {
    CslParseError::new(kind, node.range().start)
}
