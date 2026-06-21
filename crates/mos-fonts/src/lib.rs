//! Font discovery, shaping, and metrics (manifest §22.1).
//!
//! Two font-emission paths live behind one [`Font`] enum:
//!
//! - [`Font::Base14`]; the 14 standard PDF base fonts. No glyph data
//!   ships; the PDF reader supplies outlines. Advance widths come from
//!   bundled Adobe AFMs, addressed through [`pdf_base14_metrics`].
//!   `WinAnsi` natives go out as their canonical byte; the small set
//!   of extended Latin glyphs each face carries (Latin Extended-A
//!   beyond `WinAnsi`, the math operators, `fi`/`fl` ligatures) goes
//!   out through a per-document `/Differences` remap that
//!   `mos-pdf` plans. Characters outside both tiers: Cyrillic,
//!   CJK, emoji: silently substitute to `?` in both the width and
//!   emit paths (no warning, no panic; callers that want non-Latin
//!   should pick the embedded family).
//! - [`Font::Embedded`]: a bundled Noto Sans cut shaped with
//!   `rustybuzz` (`HarfBuzz` Rust port). The PDF backend embeds a
//!   subset of the actual `TrueType` outlines as a Type 0 CID font
//!   with a `/ToUnicode` `CMap`, so the output is a real
//!   Unicode-aware document: copy/paste round-trips through Cyrillic,
//!   Greek, accented Latin, and anything else Noto Sans covers.
//!
//! Six cuts ship in this crate's `data/` directory: four Noto Sans
//! style cuts (Regular, Bold, Italic, `BoldItalic`) for proportional
//! body text, one Noto Sans Mono Regular cut for `` `raw` `` runs, and
//! one Noto Sans Math cut for per-glyph fallback (see `SOURCES.md`
//! under the crate root). Style selection happens through [`FontFamily`],
//! which the layout engine receives from the eval lowerer.

#![doc(
    html_logo_url = "https://mosaiclang.dev/assets/A4.svg",
    html_favicon_url = "https://mosaiclang.dev/assets/A4.svg"
)]
#![deny(missing_docs)]

mod embedded;
mod family;
mod font;
mod metrics;
mod normalize;
#[doc(hidden)]
pub mod resources;
mod shape;

pub use embedded::{EmbeddedFont, ShapedGlyph, shape, subset};
pub use family::FontFamily;
pub use font::{EmbeddedFontId, Font};
pub use metrics::{advance_units_to_pt, ascent, descent, glyph_width, text_width};
pub use normalize::nfc_text;
pub use pdf_base14_metrics::{Base14Font, extended_glyph_name, winansi_byte};
pub use shape::{ShapedRun, WordSubRun, shape_text, shape_with_fallback};
