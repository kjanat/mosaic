//! Map parsed BibTeX records ([`mos_bib::BibEntry`]) into CSL [`Item`]s.
//!
//! This is a best-effort, infallible mapping: BibTeX entry types become the
//! closest CSL [`ItemType`] (unknown → [`ItemType::Document`]), and recognised
//! BibTeX fields become CSL variables. Unrecognised fields are dropped, as CSL
//! processors do.
//!
//! Name handling is intentionally minimal: `author`/`editor` are split on
//! whitespace-delimited `and` tokens, and per name a `Last, First` comma form
//! or `First Last` form becomes family/given. Single-token names are kept as a
//! [`literal`](Name::literal). Full BibTeX name parsing (protected institutional
//! names, von/Jr particles) and `month` handling are future refinements.

use std::collections::BTreeMap;

use mos_bib::{BibEntry, Bibliography};

use crate::item::{
    Date, DateVariable, Item, ItemType, Name, NameVariable, NumberVariable, StandardVariable,
};

/// Map one BibTeX entry to a CSL [`Item`].
#[must_use]
pub fn item_from_bib_entry(entry: &BibEntry) -> Item {
    let mut item = Item::new(entry.key.clone(), item_type_for(&entry.entry_type));
    for (field, value) in &entry.fields {
        apply_field(&mut item, &entry.entry_type, field, value);
    }
    item
}

/// Map a whole [`Bibliography`] to CSL items keyed by citation key.
#[must_use]
pub fn library_from_bibliography(bibliography: &Bibliography) -> BTreeMap<String, Item> {
    bibliography
        .entries
        .iter()
        .map(|(key, entry)| (key.clone(), item_from_bib_entry(entry)))
        .collect()
}

/// BibTeX entry type (already lowercased by `mos-bib`) → closest CSL type.
fn item_type_for(entry_type: &str) -> ItemType {
    match entry_type {
        "article" => ItemType::ArticleJournal,
        "book" | "proceedings" => ItemType::Book,
        "booklet" => ItemType::Pamphlet,
        "inbook" | "incollection" => ItemType::Chapter,
        "conference" | "inproceedings" => ItemType::PaperConference,
        "manual" | "techreport" => ItemType::Report,
        "mastersthesis" | "phdthesis" | "thesis" => ItemType::Thesis,
        "unpublished" => ItemType::Manuscript,
        "online" | "electronic" => ItemType::Webpage,
        _ => ItemType::Document,
    }
}

/// Place one recognised BibTeX field onto the item; drop unknown fields.
fn apply_field(item: &mut Item, entry_type: &str, field: &str, value: &str) {
    // Recognised string ("standard") fields, grouped by their CSL target.
    let standard = match field {
        "title" => Some(StandardVariable::Title),
        "journal" | "booktitle" => Some(StandardVariable::ContainerTitle),
        "publisher" | "school" | "institution" => Some(StandardVariable::Publisher),
        "address" if is_conference_entry(entry_type) => Some(StandardVariable::EventPlace),
        "address" => Some(StandardVariable::PublisherPlace),
        "series" => Some(StandardVariable::CollectionTitle),
        "note" => Some(StandardVariable::Note),
        "abstract" => Some(StandardVariable::Abstract),
        "keywords" => Some(StandardVariable::Keyword),
        "doi" => Some(StandardVariable::Doi),
        "url" => Some(StandardVariable::Url),
        "isbn" => Some(StandardVariable::Isbn),
        "issn" => Some(StandardVariable::Issn),
        "language" => Some(StandardVariable::Language),
        _ => None,
    };
    if let Some(variable) = standard {
        item.standard.insert(variable, value.to_owned());
        return;
    }

    // Recognised number fields.
    let number = match field {
        "volume" => Some(NumberVariable::Volume),
        "number" if is_report_entry(entry_type) => Some(NumberVariable::Number),
        "number" => Some(NumberVariable::Issue),
        "pages" => Some(NumberVariable::Page),
        "edition" => Some(NumberVariable::Edition),
        "chapter" => Some(NumberVariable::ChapterNumber),
        _ => None,
    };
    if let Some(variable) = number {
        item.number.insert(variable, value.to_owned());
        return;
    }

    // Name and date fields; anything else is dropped, as CSL processors do.
    match field {
        "author" => {
            item.name.insert(NameVariable::Author, parse_names(value));
        }
        "editor" => {
            item.name.insert(NameVariable::Editor, parse_names(value));
        }
        "year" => {
            item.date.insert(DateVariable::Issued, parse_year(value));
        }
        _ => {}
    }
}

fn is_conference_entry(entry_type: &str) -> bool {
    matches!(entry_type, "conference" | "inproceedings")
}

fn is_report_entry(entry_type: &str) -> bool {
    matches!(entry_type, "manual" | "techreport")
}

/// Split a BibTeX name list on whitespace-delimited `and` tokens.
fn parse_names(value: &str) -> Vec<Name> {
    let mut names = Vec::new();
    let mut token_start = 0;
    let mut search_start = 0;

    while let Some(relative_start) = value[search_start..].find("and") {
        let and_start = search_start + relative_start;
        let and_end = and_start + "and".len();
        if is_name_separator(value, and_start, and_end) {
            push_name(&mut names, &value[token_start..and_start]);
            token_start = and_end;
        }
        search_start = and_end;
    }

    push_name(&mut names, &value[token_start..]);
    names
}

fn is_name_separator(value: &str, start: usize, end: usize) -> bool {
    let before = value[..start].chars().next_back();
    let after = value[end..].chars().next();
    before.is_some_and(char::is_whitespace) && after.is_some_and(char::is_whitespace)
}

fn push_name(names: &mut Vec<Name>, token: &str) {
    let trimmed = token.trim();
    if !trimmed.is_empty() {
        names.push(parse_one_name(trimmed));
    }
}

/// `Last, First` and `First Last` forms become family/given; single-token names
/// stay literal because `mos-bib` does not preserve institutional bracing yet.
fn parse_one_name(token: &str) -> Name {
    match token.split_once(',') {
        Some((family, given)) => Name::person(family.trim(), given.trim()),
        None => parse_name_without_comma(token),
    }
}

fn parse_name_without_comma(token: &str) -> Name {
    match token.rsplit_once(char::is_whitespace) {
        Some((given, family)) => Name::person(family.trim(), given.trim()),
        None => Name::literal(token),
    }
}

/// Parse a BibTeX `year` into an `issued` [`Date`]; a non-numeric year is kept
/// as a literal.
fn parse_year(value: &str) -> Date {
    match value.trim().parse::<i32>() {
        Ok(year) => Date::year(year),
        Err(_) => Date::literal(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(entry_type: &str, key: &str, fields: &[(&str, &str)]) -> BibEntry {
        BibEntry {
            entry_type: entry_type.to_owned(),
            key: key.to_owned(),
            fields: fields
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        }
    }

    #[test]
    fn maps_article_type_and_core_fields() {
        let bib_entry = entry(
            "article",
            "knuth1984",
            &[
                ("title", "Literate Programming"),
                ("year", "1984"),
                ("journal", "The Computer Journal"),
            ],
        );
        let item = item_from_bib_entry(&bib_entry);
        assert_eq!(item.id, "knuth1984");
        assert_eq!(item.item_type, ItemType::ArticleJournal);
        assert_eq!(
            item.standard
                .get(&StandardVariable::Title)
                .map(String::as_str),
            Some("Literate Programming")
        );
        assert_eq!(
            item.standard
                .get(&StandardVariable::ContainerTitle)
                .map(String::as_str),
            Some("The Computer Journal")
        );
        assert_eq!(
            item.date.get(&DateVariable::Issued),
            Some(&Date::year(1984))
        );
    }

    #[test]
    fn splits_authors_on_and_and_comma() {
        let bib_entry = entry(
            "book",
            "k",
            &[("author", "Knuth, Donald E. and Ada Lovelace")],
        );
        let item = item_from_bib_entry(&bib_entry);
        let authors = item
            .name
            .get(&NameVariable::Author)
            .expect("authors present");
        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0], Name::person("Knuth", "Donald E."));
        assert_eq!(authors[1], Name::person("Lovelace", "Ada"));
    }

    #[test]
    fn splits_names_on_whitespace_delimited_and_tokens() {
        let bib_entry = entry(
            "book",
            "k",
            &[("author", "Knuth, Donald E.  and\n  Ada Lovelace")],
        );
        let item = item_from_bib_entry(&bib_entry);
        let authors = item
            .name
            .get(&NameVariable::Author)
            .expect("authors present");
        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0], Name::person("Knuth", "Donald E."));
        assert_eq!(authors[1], Name::person("Lovelace", "Ada"));
    }

    #[test]
    fn unknown_type_is_document_and_unknown_fields_drop() {
        let bib_entry = entry("flibble", "k", &[("title", "T"), ("nonsense", "x")]);
        let item = item_from_bib_entry(&bib_entry);
        assert_eq!(item.item_type, ItemType::Document);
        assert!(item.standard.contains_key(&StandardVariable::Title));
        assert_eq!(item.standard.len(), 1, "unknown field should be dropped");
    }

    #[test]
    fn non_numeric_year_becomes_a_literal_date() {
        let bib_entry = entry("misc", "k", &[("year", "in press")]);
        let item = item_from_bib_entry(&bib_entry);
        assert_eq!(
            item.date.get(&DateVariable::Issued),
            Some(&Date::literal("in press"))
        );
    }

    #[test]
    fn maps_bibtex_entry_type_groups() {
        let cases = [
            ("article", ItemType::ArticleJournal),
            ("book", ItemType::Book),
            ("proceedings", ItemType::Book),
            ("booklet", ItemType::Pamphlet),
            ("inbook", ItemType::Chapter),
            ("incollection", ItemType::Chapter),
            ("conference", ItemType::PaperConference),
            ("inproceedings", ItemType::PaperConference),
            ("manual", ItemType::Report),
            ("techreport", ItemType::Report),
            ("mastersthesis", ItemType::Thesis),
            ("phdthesis", ItemType::Thesis),
            ("thesis", ItemType::Thesis),
            ("unpublished", ItemType::Manuscript),
            ("online", ItemType::Webpage),
            ("electronic", ItemType::Webpage),
            ("misc", ItemType::Document),
        ];

        for (entry_type, expected) in cases {
            let item = item_from_bib_entry(&entry(entry_type, "k", &[]));
            assert_eq!(item.item_type, expected, "entry type: {entry_type}");
        }
    }

    #[test]
    fn maps_standard_and_number_field_groups() {
        let bib_entry = entry(
            "book",
            "k",
            &[
                ("title", "Title"),
                ("booktitle", "Container"),
                ("publisher", "Publisher"),
                ("school", "School"),
                ("institution", "Institution"),
                ("address", "Place"),
                ("series", "Series"),
                ("note", "Note"),
                ("abstract", "Abstract"),
                ("keywords", "Keywords"),
                ("doi", "10.0/demo"),
                ("url", "https://example.invalid"),
                ("isbn", "ISBN"),
                ("issn", "ISSN"),
                ("language", "en"),
                ("volume", "2"),
                ("number", "4"),
                ("pages", "10-20"),
                ("edition", "3"),
                ("chapter", "7"),
            ],
        );
        let item = item_from_bib_entry(&bib_entry);

        assert_eq!(
            item.standard
                .get(&StandardVariable::Title)
                .map(String::as_str),
            Some("Title")
        );
        assert_eq!(
            item.standard
                .get(&StandardVariable::ContainerTitle)
                .map(String::as_str),
            Some("Container")
        );
        assert_eq!(
            item.standard
                .get(&StandardVariable::Publisher)
                .map(String::as_str),
            Some("School")
        );
        assert_eq!(
            item.standard
                .get(&StandardVariable::PublisherPlace)
                .map(String::as_str),
            Some("Place")
        );
        assert_eq!(
            item.standard
                .get(&StandardVariable::CollectionTitle)
                .map(String::as_str),
            Some("Series")
        );
        assert_eq!(
            item.standard
                .get(&StandardVariable::Note)
                .map(String::as_str),
            Some("Note")
        );
        assert_eq!(
            item.standard
                .get(&StandardVariable::Abstract)
                .map(String::as_str),
            Some("Abstract")
        );
        assert_eq!(
            item.standard
                .get(&StandardVariable::Keyword)
                .map(String::as_str),
            Some("Keywords")
        );
        assert_eq!(
            item.standard
                .get(&StandardVariable::Doi)
                .map(String::as_str),
            Some("10.0/demo")
        );
        assert_eq!(
            item.standard
                .get(&StandardVariable::Url)
                .map(String::as_str),
            Some("https://example.invalid")
        );
        assert_eq!(
            item.standard
                .get(&StandardVariable::Isbn)
                .map(String::as_str),
            Some("ISBN")
        );
        assert_eq!(
            item.standard
                .get(&StandardVariable::Issn)
                .map(String::as_str),
            Some("ISSN")
        );
        assert_eq!(
            item.standard
                .get(&StandardVariable::Language)
                .map(String::as_str),
            Some("en")
        );

        assert_eq!(
            item.number.get(&NumberVariable::Volume).map(String::as_str),
            Some("2")
        );
        assert_eq!(
            item.number.get(&NumberVariable::Issue).map(String::as_str),
            Some("4")
        );
        assert_eq!(
            item.number.get(&NumberVariable::Page).map(String::as_str),
            Some("10-20")
        );
        assert_eq!(
            item.number
                .get(&NumberVariable::Edition)
                .map(String::as_str),
            Some("3")
        );
        assert_eq!(
            item.number
                .get(&NumberVariable::ChapterNumber)
                .map(String::as_str),
            Some("7")
        );
    }

    #[test]
    fn maps_report_number_to_number_not_issue() {
        let item = item_from_bib_entry(&entry("techreport", "k", &[("number", "TR-7")]));
        assert_eq!(
            item.number.get(&NumberVariable::Number).map(String::as_str),
            Some("TR-7")
        );
        assert!(!item.number.contains_key(&NumberVariable::Issue));
    }

    #[test]
    fn maps_conference_address_to_event_place() {
        let item = item_from_bib_entry(&entry("inproceedings", "k", &[("address", "Paris")]));
        assert_eq!(
            item.standard
                .get(&StandardVariable::EventPlace)
                .map(String::as_str),
            Some("Paris")
        );
        assert!(
            !item
                .standard
                .contains_key(&StandardVariable::PublisherPlace)
        );
    }

    #[test]
    fn maps_editors_and_skips_empty_name_tokens() {
        let bib_entry = entry(
            "book",
            "k",
            &[
                ("author", " Ada Lovelace and  and Turing, Alan "),
                ("editor", "Knuth, Donald"),
            ],
        );
        let item = item_from_bib_entry(&bib_entry);
        assert_eq!(
            item.name.get(&NameVariable::Author),
            Some(&vec![
                Name::person("Lovelace", "Ada"),
                Name::person("Turing", "Alan")
            ])
        );
        assert_eq!(
            item.name.get(&NameVariable::Editor),
            Some(&vec![Name::person("Knuth", "Donald")])
        );
    }

    #[test]
    fn maps_whole_bibliography_by_key() {
        let bibliography = Bibliography {
            entries: [
                ("a".to_owned(), entry("article", "a", &[("title", "First")])),
                ("b".to_owned(), entry("book", "b", &[("title", "Second")])),
            ]
            .into_iter()
            .collect(),
        };

        let library = library_from_bibliography(&bibliography);
        assert_eq!(library.len(), 2);
        assert_eq!(
            library.get("a").map(|item| item.item_type),
            Some(ItemType::ArticleJournal)
        );
        assert_eq!(
            library.get("b").map(|item| item.item_type),
            Some(ItemType::Book)
        );
    }

    #[test]
    fn keeps_single_token_names_literal() {
        let item = item_from_bib_entry(&entry("book", "k", &[("author", "Plato")]));
        assert_eq!(
            item.name.get(&NameVariable::Author),
            Some(&vec![Name::literal("Plato")])
        );
    }
}
