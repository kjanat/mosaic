//! Map parsed BibTeX records ([`mos_bib::BibEntry`]) into CSL [`Item`]s.
//!
//! This is a best-effort, infallible mapping: BibTeX entry types become the
//! closest CSL [`ItemType`] (unknown → [`ItemType::Document`]), and recognised
//! BibTeX fields become CSL variables. Unrecognised fields are dropped, as CSL
//! processors do.
//!
//! Name handling is intentionally minimal: `author`/`editor` are split on
//! ` and `, and per name a `Last, First` comma form becomes family/given —
//! everything else is kept as a name [`literal`](Name::literal). Full BibTeX
//! name parsing (von/Jr particles) and `month` handling are future refinements.

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
        apply_field(&mut item, field, value);
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
fn apply_field(item: &mut Item, field: &str, value: &str) {
    // Recognised string ("standard") fields, grouped by their CSL target.
    let standard = match field {
        "title" => Some(StandardVariable::Title),
        "journal" | "booktitle" => Some(StandardVariable::ContainerTitle),
        "publisher" | "school" | "institution" => Some(StandardVariable::Publisher),
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

/// Split a BibTeX name list on ` and ` into individual CSL names.
fn parse_names(value: &str) -> Vec<Name> {
    value
        .split(" and ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(parse_one_name)
        .collect()
}

/// A `Last, First` comma form becomes family/given; anything else is a literal.
fn parse_one_name(token: &str) -> Name {
    match token.split_once(',') {
        Some((family, given)) => Name::person(family.trim(), given.trim()),
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
        assert_eq!(authors[1], Name::literal("Ada Lovelace"));
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
}
