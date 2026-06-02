//! Citation Style Language (CSL) support for Mosaic (manifest §12).
//!
//! This crate provides the **data foundations** for CSL 1.0.2 — not a CSL
//! processor. It ships:
//!
//! - a typed **item data model** ([`Item`] with the [`ItemType`] and variable
//!   vocabularies) mirroring the CSL-JSON shape a style formats; and
//! - an infallible **BibTeX → CSL mapping** ([`item_from_bib_entry`],
//!   [`library_from_bibliography`]) from `mos-bib` records.
//!
//! Out of scope: the CSL **processor** itself — evaluating a style against
//! items to render citations or bibliographies (formatting, sorting,
//! disambiguation, name ordering, locales). This crate has no
//! `mos-eval` / layout / PDF wiring.

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

mod from_bibtex;
mod item;

pub use from_bibtex::{item_from_bib_entry, library_from_bibliography};
pub use item::{
    Date, DateParts, DateVariable, Item, ItemType, Name, NameVariable, NumberVariable,
    StandardVariable,
};
