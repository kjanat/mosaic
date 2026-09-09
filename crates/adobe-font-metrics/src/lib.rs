//! Borrowing, zero-dependency Adobe Font Metrics (AFM) v4.x parser.
//!
//! [`parse`] and [`parse_bytes`] retain global metadata, both writing directions,
//! character advance vectors and ligatures, named/hexadecimal pair kerning,
//! track kerning, and composite definitions. [`FontMetrics::advance`] resolves
//! character advances against direction-specific `CharWidth` defaults.
//!
//! Optional fields preserve absence. Comments and unrecognized records remain
//! available through [`FontMetrics::source_records`]. Names and record text borrow
//! the input; [`FontMetrics::into_owned`] detaches the entire model for storage.
//! Numeric metrics use `f32`; source formatting is not preserved for serialization.
//!
//! # Example
//!
//! ```
//! use adobe_font_metrics::{Direction, Vector, parse};
//! # fn main() -> Result<(), adobe_font_metrics::ParseError> {
//! let source = "StartFontMetrics 4.1\nFontName Demo\nFontBBox 0 0 600 700\n\
//!               CharWidth 600 0\nStartCharMetrics 1\nC 65 ; N A ;\n\
//!               EndCharMetrics\nEndFontMetrics\n";
//! let font = parse(source)?;
//! assert_eq!(font.advance(&font.character_metrics[0], Direction::Zero),
//!            Some(Vector { x: 600.0, y: 0.0 }));
//! assert!(font.direction(Direction::Zero).fixed_pitch());
//! # Ok(())
//! # }
//! ```
//!
//! # Validation and limits
//!
//! The reader checks modeled operands, finite numbers, section boundaries,
//! declared section counts, and the closing font marker. Standalone pair/track
//! sections are accepted for compatibility with existing callers. It does not
//! resolve glyph references, interpret encodings, or certify all cross-field AFM
//! constraints. Unknown records are preserved without interpretation.
//!
//! AFM v3, ACFM/AMFM containers, multiple-master array interpretation, shaping,
//! and serialization remain outside the implemented API. See the [coverage audit]
//! and [Adobe Tech Note 5004] for the precise boundary.
//!
//! [coverage audit]: https://github.com/kjanat/mosaic/blob/master/docs/afm-parser-scope.md
//! [Adobe Tech Note 5004]: https://adobe-type-tools.github.io/font-tech-notes/pdfs/5004.AFM_Spec.pdf

#![doc(
    html_logo_url = "https://mosaiclang.dev/assets/A4.svg",
    html_favicon_url = "https://mosaiclang.dev/assets/A4.svg"
)]
#![deny(missing_docs)]

mod error;
mod model;
mod parser;
mod records;

pub use error::ParseError;
pub use model::*;
pub use parser::{parse, parse_bytes};
