//! Citation Style Language (CSL) support for Mosaic (manifest §12).
//!
//! This crate provides the **data foundations** for CSL 1.0.2; not a CSL
//! processor. It ships:
//!
//! - a typed **item data model** ([`Item`] with the [`ItemType`] and variable
//!   vocabularies) mirroring the CSL-JSON shape a style formats;
//! - an infallible **BibTeX → CSL mapping** ([`item_from_bib_entry`],
//!   [`library_from_bibliography`]) from `mos-bib` records; and
//! - a **CSL XML style parser** ([`parse_style`]) producing a typed [`Style`]
//!   AST (`<style>` / `<info>` / `<citation>` / `<bibliography>` / `<macro>`
//!   and the rendering elements).
//!
//! Out of scope: the CSL **processor** itself: evaluating a style against
//! items to render citations or bibliographies (formatting, sorting,
//! disambiguation, name ordering, locales). This crate has no
//! `mos-eval` / layout / PDF wiring.

#![doc(
    html_logo_url = "https://mosaiclang.dev/assets/A4.svg",
    html_favicon_url = "https://mosaiclang.dev/assets/A4.svg"
)]

mod error;
mod from_bibtex;
mod item;
mod parser;
mod style;

pub use error::{CslParseError, CslParseErrorKind};
pub use from_bibtex::{item_from_bib_entry, library_from_bibliography};
pub use item::{
    Date, DateParts, DateVariable, Item, ItemType, Name, NameVariable, NumberVariable,
    StandardVariable,
};
pub use parser::parse_style;
pub use style::{
    Bibliography, Branch, Choose, Citation, Common, Conditions, DateElement, DatePart, Element,
    EtAl, Group, Info, Label, Layout, Match, NameElement, Names, Number, SortKey, SortTarget,
    Style, StyleClass, Text, TextSource,
};
