//! The typed CSL style AST produced by [`parse_style`](crate::parse_style).
//!
//! This models the structure of a CSL 1.0.2 style — the `<style>` root, its
//! `<info>`, `<citation>`, `<bibliography>`, and `<macro>` children, and the
//! rendering elements ([`Element`]) with their common attributes. It is a
//! faithful *structural* model, not a processor: there is no evaluation of a
//! style against data here. Rendering-critical options are retained on typed
//! option structs so a future processor can interpret them. Attributes outside
//! the modelled set are ignored; unknown rendering elements are a parse error.

use std::collections::BTreeMap;

/// A parsed CSL style (the `<style>` root element).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Style {
    /// Whether citations are in-text or notes (`class` attribute).
    pub class: StyleClass,
    /// The declared CSL version (`version` attribute, e.g. `"1.0"`).
    pub version: String,
    /// An optional overriding style locale (`default-locale`).
    pub default_locale: Option<String>,
    /// Global style options retained for a future processor.
    pub options: StyleOptions,
    /// Style metadata from `<info>`.
    pub info: Info,
    /// The `<citation>` element, if present.
    pub citation: Option<Citation>,
    /// The `<bibliography>` element, if present.
    pub bibliography: Option<Bibliography>,
    /// `<macro>` definitions keyed by name, in sorted order.
    pub macros: BTreeMap<String, Vec<Element>>,
}

/// Global `<style>` rendering options retained but not evaluated.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleOptions {
    pub page_range_format: Option<String>,
    pub demote_non_dropping_particle: Option<String>,
    pub initialize_with_hyphen: Option<String>,
}

/// The `class` of a CSL style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleClass {
    /// In-text citations (`class="in-text"`).
    InText,
    /// Note (foot/endnote) citations (`class="note"`).
    Note,
}

/// Style metadata (`<info>`). Only the commonly-needed fields are modelled.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Info {
    /// `<id>` — the stable style identifier.
    pub id: Option<String>,
    /// `<title>` — the human-facing style name.
    pub title: Option<String>,
}

/// The `<citation>` element: how cites are formatted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Citation {
    /// The required `<layout>`.
    pub layout: Layout,
    /// `<sort>` keys, in priority order (empty when absent).
    pub sort: Vec<SortKey>,
    /// Citation-specific rendering options retained for a future processor.
    pub options: CitationOptions,
}

/// `<citation>` options retained but not evaluated.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CitationOptions {
    pub et_al_min: Option<String>,
    pub et_al_use_first: Option<String>,
    pub et_al_subsequent_min: Option<String>,
    pub et_al_subsequent_use_first: Option<String>,
    pub collapse: Option<String>,
    pub cite_group_delimiter: Option<String>,
    pub disambiguate_add_names: Option<String>,
    pub disambiguate_add_givenname: Option<String>,
    pub disambiguate_add_year_suffix: Option<String>,
    pub givenname_disambiguation_rule: Option<String>,
    pub near_note_distance: Option<String>,
}

/// The `<bibliography>` element: how bibliography entries are formatted.
///
/// This is the CSL *style* element, distinct from a bibliographic database
/// (`mos_bib::Bibliography`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bibliography {
    /// The required `<layout>`.
    pub layout: Layout,
    /// `<sort>` keys, in priority order (empty when absent).
    pub sort: Vec<SortKey>,
    /// Bibliography-specific rendering options retained for a future processor.
    pub options: BibliographyOptions,
}

/// `<bibliography>` options retained but not evaluated.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BibliographyOptions {
    pub et_al_min: Option<String>,
    pub et_al_use_first: Option<String>,
    pub et_al_subsequent_min: Option<String>,
    pub et_al_subsequent_use_first: Option<String>,
    pub hanging_indent: Option<String>,
    pub second_field_align: Option<String>,
    pub line_spacing: Option<String>,
    pub entry_spacing: Option<String>,
    pub subsequent_author_substitute: Option<String>,
    pub subsequent_author_substitute_rule: Option<String>,
}

/// A `<layout>` — an ordered list of rendering elements plus common attributes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub elements: Vec<Element>,
    pub common: Common,
}

/// One `<key>` in a `<sort>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortKey {
    pub target: SortTarget,
    /// `true` for `sort="descending"` (default is ascending).
    pub descending: bool,
    /// Name-rendering sort-key options retained for a future processor.
    pub options: SortKeyOptions,
}

/// `<key>` options retained but not evaluated.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SortKeyOptions {
    pub names_min: Option<String>,
    pub names_use_first: Option<String>,
    pub names_use_last: Option<String>,
}

/// What a [`SortKey`] sorts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SortTarget {
    Variable(String),
    Macro(String),
}

/// Common rendering attributes shared across most CSL elements (affixes,
/// formatting, and `delimiter`). Unmodelled attributes are ignored by the
/// parser.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Common {
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub delimiter: Option<String>,
    pub font_style: Option<String>,
    pub font_variant: Option<String>,
    pub font_weight: Option<String>,
    pub text_decoration: Option<String>,
    pub vertical_align: Option<String>,
    pub text_case: Option<String>,
    pub display: Option<String>,
}

/// A CSL rendering element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Element {
    Text(Text),
    Number(Number),
    Date(DateElement),
    /// Boxed: `<names>` is by far the largest element (it nests `<name>`,
    /// `<et-al>`, `<label>`, and a substitute list), so it is boxed to keep
    /// [`Element`] small.
    Names(Box<Names>),
    Label(Label),
    Group(Group),
    Choose(Choose),
}

/// `<text>` — renders a variable, macro, term, or literal value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    pub source: TextSource,
    pub quotes: bool,
    pub strip_periods: bool,
    pub common: Common,
}

/// What a [`Text`] element renders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextSource {
    /// `variable="..."` (with optional `form`).
    Variable { name: String, form: Option<String> },
    /// `macro="..."`.
    Macro(String),
    /// `term="..."` (with optional `form` and `plural`).
    Term {
        name: String,
        form: Option<String>,
        plural: bool,
    },
    /// `value="..."` — a literal string.
    Value(String),
}

/// `<number>` — renders a number variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Number {
    pub variable: String,
    pub form: Option<String>,
    pub common: Common,
}

/// `<date>` — renders a date variable, localized or with explicit parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateElement {
    pub variable: String,
    /// `form="numeric"|"text"` for a localized date, or `None`.
    pub form: Option<String>,
    /// `date-parts="year-month-day"|"year-month"|"year"`.
    pub date_parts: Option<String>,
    pub parts: Vec<DatePart>,
    pub common: Common,
}

/// A `<date-part>` child of `<date>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatePart {
    /// `name="day"|"month"|"year"`.
    pub name: String,
    pub form: Option<String>,
    pub range_delimiter: Option<String>,
    pub common: Common,
}

/// `<names>` — renders one or more name variables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Names {
    /// The `variable` attribute, split on whitespace.
    pub variables: Vec<String>,
    pub name: Option<NameElement>,
    pub et_al: Option<EtAl>,
    pub label: Option<Label>,
    /// `<substitute>` fallback elements.
    pub substitute: Vec<Element>,
    pub common: Common,
}

/// The `<name>` child of `<names>` (a subset of its many options).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NameElement {
    pub form: Option<String>,
    /// `and="text"|"symbol"`.
    pub and: Option<String>,
    /// Name-rendering options retained for a future processor.
    pub options: NameOptions,
    pub common: Common,
}

/// `<name>` options retained but not evaluated.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NameOptions {
    pub et_al_min: Option<String>,
    pub et_al_use_first: Option<String>,
    pub et_al_subsequent_min: Option<String>,
    pub et_al_subsequent_use_first: Option<String>,
    pub et_al_use_last: Option<String>,
    pub delimiter_precedes_et_al: Option<String>,
    pub delimiter_precedes_last: Option<String>,
    pub initialize: Option<String>,
    pub initialize_with: Option<String>,
    pub name_as_sort_order: Option<String>,
    pub sort_separator: Option<String>,
}

/// The `<et-al>` child of `<names>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EtAl {
    /// `term="et-al"|"and others"`.
    pub term: Option<String>,
    pub common: Common,
}

/// `<label>` — renders a term matching a variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    /// The `variable` attribute. `None` when nested in `<names>` (it inherits
    /// the parent's variables).
    pub variable: Option<String>,
    pub form: Option<String>,
    pub plural: Option<String>,
    pub strip_periods: Option<String>,
    pub common: Common,
}

/// `<group>` — a delimited, conditionally-suppressed run of elements.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Group {
    pub children: Vec<Element>,
    pub common: Common,
}

/// `<choose>` — conditional rendering.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choose {
    /// `<if>` followed by any `<else-if>` branches, in order.
    pub branches: Vec<Branch>,
    /// `<else>` elements (empty when absent).
    pub otherwise: Vec<Element>,
}

/// One `<if>` / `<else-if>` branch of a [`Choose`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub conditions: Conditions,
    pub children: Vec<Element>,
}

/// The conditions on an `<if>` / `<else-if>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conditions {
    pub match_mode: Match,
    pub kind: Vec<String>,
    pub variable: Vec<String>,
    pub is_numeric: Vec<String>,
    pub is_uncertain_date: Vec<String>,
    pub locator: Vec<String>,
    pub position: Vec<String>,
    pub disambiguate: bool,
}

/// The `match` attribute on a condition set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Match {
    /// All conditions must hold (`match="all"`, the default).
    #[default]
    All,
    /// Any condition may hold (`match="any"`).
    Any,
    /// No condition may hold (`match="none"`).
    None,
}
