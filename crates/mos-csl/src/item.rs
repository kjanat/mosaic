//! The CSL item data model; the typed input data a CSL style formats.
//!
//! This mirrors the "CSL-JSON" shape from the CSL 1.0.2 specification
//! (Appendix III item types, Appendix IV variables) as typed Rust. An [`Item`]
//! is an `id` plus an [`ItemType`] and four category-keyed maps of variables
//! (string, number, date, name). Everything is stored in
//! [`BTreeMap`](std::collections::BTreeMap)s keyed by ordered enums, so
//! iteration is deterministic.

use std::collections::BTreeMap;
use std::fmt;

/// Generate a closed CSL vocabulary enum with `as_str` / `from_csl` / Display.
///
/// Each variant maps to its exact CSL string form (e.g. `"article-journal"`,
/// `"DOI"`). `from_csl` is the inverse and returns `None` for unknown strings.
macro_rules! csl_vocab {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident { $($(#[$variant_meta:meta])* $variant:ident => $text:literal),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        $vis enum $name {
            $($(#[$variant_meta])* $variant),+
        }

        impl $name {
            /// The CSL string form of this value.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)+
                }
            }

            /// Parse a CSL string form; returns `None` if unrecognised.
            #[must_use]
            pub fn from_csl(text: &str) -> Option<Self> {
                match text {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

csl_vocab! {
    /// CSL item types (specification Appendix III).
    ///
    /// [`ItemType::Document`] is the catch-all used when a source type has no
    /// closer match.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_csl::ItemType;
    ///
    /// assert_eq!(ItemType::ArticleJournal.as_str(), "article-journal");
    /// assert_eq!(ItemType::from_csl("legal_case"), Some(ItemType::LegalCase));
    /// assert_eq!(ItemType::from_csl("not-a-type"), None);
    /// ```
    pub enum ItemType {
        Article => "article",
        ArticleJournal => "article-journal",
        ArticleMagazine => "article-magazine",
        ArticleNewspaper => "article-newspaper",
        Bill => "bill",
        Book => "book",
        Broadcast => "broadcast",
        Chapter => "chapter",
        Classic => "classic",
        Collection => "collection",
        Dataset => "dataset",
        Document => "document",
        Entry => "entry",
        EntryDictionary => "entry-dictionary",
        EntryEncyclopedia => "entry-encyclopedia",
        Event => "event",
        Figure => "figure",
        Graphic => "graphic",
        Hearing => "hearing",
        Interview => "interview",
        LegalCase => "legal_case",
        Legislation => "legislation",
        Manuscript => "manuscript",
        Map => "map",
        MotionPicture => "motion_picture",
        MusicalScore => "musical_score",
        Pamphlet => "pamphlet",
        PaperConference => "paper-conference",
        Patent => "patent",
        Performance => "performance",
        Periodical => "periodical",
        PersonalCommunication => "personal_communication",
        Post => "post",
        PostWeblog => "post-weblog",
        Regulation => "regulation",
        Report => "report",
        Review => "review",
        ReviewBook => "review-book",
        Software => "software",
        Song => "song",
        Speech => "speech",
        Standard => "standard",
        Thesis => "thesis",
        Treaty => "treaty",
        Webpage => "webpage",
    }
}

csl_vocab! {
    /// CSL string ("standard") variables (specification Appendix IV).
    pub enum StandardVariable {
        Abstract => "abstract",
        Annote => "annote",
        Archive => "archive",
        ArchiveCollection => "archive_collection",
        ArchiveLocation => "archive_location",
        ArchivePlace => "archive-place",
        Authority => "authority",
        CallNumber => "call-number",
        CitationKey => "citation-key",
        CitationLabel => "citation-label",
        CollectionTitle => "collection-title",
        ContainerTitle => "container-title",
        ContainerTitleShort => "container-title-short",
        Dimensions => "dimensions",
        Division => "division",
        Doi => "DOI",
        /// Deprecated CSL standard variable retained for spec coverage.
        Event => "event",
        EventTitle => "event-title",
        EventPlace => "event-place",
        Genre => "genre",
        Isbn => "ISBN",
        Issn => "ISSN",
        Jurisdiction => "jurisdiction",
        Keyword => "keyword",
        Language => "language",
        License => "license",
        Medium => "medium",
        Note => "note",
        OriginalPublisher => "original-publisher",
        OriginalPublisherPlace => "original-publisher-place",
        OriginalTitle => "original-title",
        PartTitle => "part-title",
        Pmcid => "PMCID",
        Pmid => "PMID",
        Publisher => "publisher",
        PublisherPlace => "publisher-place",
        References => "references",
        ReviewedGenre => "reviewed-genre",
        ReviewedTitle => "reviewed-title",
        Scale => "scale",
        Source => "source",
        Status => "status",
        Title => "title",
        TitleShort => "title-short",
        Url => "URL",
        VolumeTitle => "volume-title",
        YearSuffix => "year-suffix",
    }
}

csl_vocab! {
    /// CSL number variables (specification Appendix IV).
    ///
    /// Stored as strings because CSL numbers may carry affixes (`2E`) and
    /// ranges (`5-7`); extraction is the processor's concern, not this model's.
    pub enum NumberVariable {
        ChapterNumber => "chapter-number",
        CitationNumber => "citation-number",
        CollectionNumber => "collection-number",
        Edition => "edition",
        FirstReferenceNoteNumber => "first-reference-note-number",
        Issue => "issue",
        Locator => "locator",
        Number => "number",
        NumberOfPages => "number-of-pages",
        NumberOfVolumes => "number-of-volumes",
        Page => "page",
        PageFirst => "page-first",
        PartNumber => "part-number",
        PrintingNumber => "printing-number",
        Section => "section",
        SupplementNumber => "supplement-number",
        Version => "version",
        Volume => "volume",
    }
}

csl_vocab! {
    /// CSL date variables (specification Appendix IV).
    pub enum DateVariable {
        Accessed => "accessed",
        AvailableDate => "available-date",
        EventDate => "event-date",
        Issued => "issued",
        OriginalDate => "original-date",
        Submitted => "submitted",
    }
}

csl_vocab! {
    /// CSL name variables (specification Appendix IV).
    pub enum NameVariable {
        Author => "author",
        Chair => "chair",
        CollectionEditor => "collection-editor",
        Compiler => "compiler",
        Composer => "composer",
        ContainerAuthor => "container-author",
        Contributor => "contributor",
        Curator => "curator",
        Director => "director",
        Editor => "editor",
        EditorialDirector => "editorial-director",
        EditorTranslator => "editor-translator",
        ExecutiveProducer => "executive-producer",
        Guest => "guest",
        Host => "host",
        Illustrator => "illustrator",
        Interviewer => "interviewer",
        Narrator => "narrator",
        Organizer => "organizer",
        OriginalAuthor => "original-author",
        Performer => "performer",
        Producer => "producer",
        Recipient => "recipient",
        ReviewedAuthor => "reviewed-author",
        ScriptWriter => "script-writer",
        SeriesCreator => "series-creator",
        Translator => "translator",
    }
}

/// A personal or institutional name (specification "Name" name-parts).
///
/// Personal names use the part fields; an institution, or any name kept whole,
/// goes in [`literal`](Self::literal).
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name {
    pub family: Option<String>,
    pub given: Option<String>,
    pub suffix: Option<String>,
    pub dropping_particle: Option<String>,
    pub non_dropping_particle: Option<String>,
    pub literal: Option<String>,
}

impl Name {
    /// A whole-name literal (an institution, or a name not split into parts).
    #[must_use]
    pub fn literal(text: impl Into<String>) -> Self {
        Self {
            literal: Some(text.into()),
            ..Self::default()
        }
    }

    /// A personal name split into `family` and `given` parts.
    #[must_use]
    pub fn person(family: impl Into<String>, given: impl Into<String>) -> Self {
        Self {
            family: Some(family.into()),
            given: Some(given.into()),
            ..Self::default()
        }
    }
}

/// One date in a CSL [`Date`]; any subset of the parts may be present.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DateParts {
    pub year: Option<i32>,
    pub month: Option<u8>,
    pub day: Option<u8>,
    pub season: Option<u8>,
}

/// A CSL date-variable value: a single date, a range (`start`..`end`), or a
/// free-form [`literal`](Self::literal). `circa` marks an approximate date.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub start: DateParts,
    pub end: Option<DateParts>,
    pub circa: bool,
    pub literal: Option<String>,
}

impl Date {
    /// A date carrying only a year.
    #[must_use]
    pub fn year(year: i32) -> Self {
        Self {
            start: DateParts {
                year: Some(year),
                ..DateParts::default()
            },
            ..Self::default()
        }
    }

    /// A free-form literal date (when a source can't be split into parts).
    #[must_use]
    pub fn literal(text: impl Into<String>) -> Self {
        Self {
            literal: Some(text.into()),
            ..Self::default()
        }
    }
}

/// A single bibliographic item; the unit a CSL style formats.
///
/// Variables are split by category into deterministic [`BTreeMap`]s. Build one
/// from a parsed BibTeX record with
/// [`item_from_bib_entry`](crate::item_from_bib_entry).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub item_type: ItemType,
    pub standard: BTreeMap<StandardVariable, String>,
    pub number: BTreeMap<NumberVariable, String>,
    pub date: BTreeMap<DateVariable, Date>,
    pub name: BTreeMap<NameVariable, Vec<Name>>,
}

impl Default for Item {
    fn default() -> Self {
        // `ItemType` deliberately has no `Default`; `Document` is the CSL
        // catch-all, so an item's default type is `Document`.
        Self {
            id: String::new(),
            item_type: ItemType::Document,
            standard: BTreeMap::new(),
            number: BTreeMap::new(),
            date: BTreeMap::new(),
            name: BTreeMap::new(),
        }
    }
}

impl Item {
    /// An empty item of `item_type` identified by `id`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_csl::{Item, ItemType};
    ///
    /// let item = Item::new("knuth1984", ItemType::ArticleJournal);
    /// assert_eq!(item.id, "knuth1984");
    /// assert!(item.standard.is_empty());
    /// ```
    #[must_use]
    pub fn new(id: impl Into<String>, item_type: ItemType) -> Self {
        Self {
            id: id.into(),
            item_type,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocab_helpers_round_trip_known_values_and_reject_unknowns() {
        assert_eq!(ItemType::ArticleJournal.as_str(), "article-journal");
        assert_eq!(ItemType::from_csl("book"), Some(ItemType::Book));
        assert_eq!(ItemType::from_csl("unknown"), None);
        assert_eq!(ItemType::Webpage.to_string(), "webpage");

        assert_eq!(StandardVariable::Doi.as_str(), "DOI");
        assert_eq!(StandardVariable::Event.as_str(), "event");
        assert_eq!(
            StandardVariable::from_csl("container-title"),
            Some(StandardVariable::ContainerTitle)
        );
        assert_eq!(
            StandardVariable::from_csl("event"),
            Some(StandardVariable::Event)
        );
        assert_eq!(StandardVariable::from_csl("doi"), None);
        assert_eq!(StandardVariable::Url.to_string(), "URL");

        assert_eq!(NumberVariable::from_csl("page"), Some(NumberVariable::Page));
        assert_eq!(NumberVariable::from_csl("pages"), None);
        assert_eq!(NumberVariable::Volume.to_string(), "volume");

        assert_eq!(DateVariable::from_csl("issued"), Some(DateVariable::Issued));
        assert_eq!(DateVariable::from_csl("published"), None);
        assert_eq!(DateVariable::Accessed.to_string(), "accessed");

        assert_eq!(NameVariable::from_csl("author"), Some(NameVariable::Author));
        assert_eq!(NameVariable::from_csl("authors"), None);
        assert_eq!(NameVariable::Translator.to_string(), "translator");
    }

    #[test]
    fn constructors_create_precise_item_values() {
        let literal = Name::literal("Mosaic Team");
        assert_eq!(literal.literal.as_deref(), Some("Mosaic Team"));
        assert_eq!(literal.family, None);

        let person = Name::person("Lovelace", "Ada");
        assert_eq!(person.family.as_deref(), Some("Lovelace"));
        assert_eq!(person.given.as_deref(), Some("Ada"));

        let year = Date::year(1843);
        assert_eq!(year.start.year, Some(1843));
        assert_eq!(year.literal, None);

        let literal_date = Date::literal("forthcoming");
        assert_eq!(literal_date.literal.as_deref(), Some("forthcoming"));
        assert_eq!(literal_date.start.year, None);

        let default_item = Item::default();
        assert_eq!(default_item.item_type, ItemType::Document);
        assert!(default_item.standard.is_empty());

        let article = Item::new("ada1843", ItemType::ArticleJournal);
        assert_eq!(article.id, "ada1843");
        assert_eq!(article.item_type, ItemType::ArticleJournal);
    }
}
