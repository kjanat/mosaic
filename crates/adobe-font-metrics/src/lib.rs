//! Pure-Rust, zero-dependency parser for Adobe Font Metrics (AFM) files,
//! per Adobe Tech Note 5004 ([`5004.AFM_Spec`]).
//!
//! [`5004.AFM_Spec`]: https://adobe-type-tools.github.io/font-tech-notes/pdfs/5004.AFM_Spec.pdf
//!
//! # Scope
//!
//! Supports AFM **v4.x** (the format Adobe shipped with the Core 14
//! PostScript fonts). The single entry point is [`parse`], which
//! consumes a `&str` and returns a borrowed [`FontMetrics`] whose
//! `Cow<'_, str>` fields point into the source slice: zero allocations
//! for glyph names and kerning operands. Call [`FontMetrics::into_owned`]
//! to obtain an [`OwnedFontMetrics`] (`FontMetrics<'static>`) suitable
//! for caching, baking into static tables, or sending across threads.
//!
//! AFM v3.x files (e.g. older Adobe samples) are deliberately rejected
//! with [`ParseError::UnsupportedVersion`]. The reader subset here
//! would handle most v3 files, but the v4-only scope claim is honest;
//! relax it once a real v3 fixture is on hand to validate against.
//!
//! # Coverage
//!
//! - Header: `StartFontMetrics` (rejects non-4.x versions).
//! - Global keys: `FontName`, `FullName`, `FamilyName`, `Weight`,
//!   `ItalicAngle`, `IsFixedPitch`, `FontBBox`, `UnderlinePosition`,
//!   `UnderlineThickness`, `CapHeight`, `XHeight`, `Ascender`,
//!   `Descender`, `EncodingScheme`.
//! - Per-character records: `C`, `CH`, `WX`, `W0X`, `W`/`W0` (X taken),
//!   `N`, `B`. `WY`, `L`, and other tokens are ignored within a record.
//! - Kerning: `KPX`, `KPY`, `KP` (KPY rows store `adjust = 0.0`; only
//!   the X axis is exposed in the public type today). `StartKernPairs1`
//!   blocks (direction-1 kerning) are accepted and dropped.
//! - `StartComposites`/`CC` blocks are accepted and discarded per the
//!   user-facing scope of the v0.1 surface.
//! - `StartTrackKern`/`TrackKern`/`EndTrackKern` (track kerning) are
//!   not modelled and pass through silently.
//! - `StartDirection 1` blocks are skipped; direction-0 and
//!   direction-2 blocks are accepted (their inner keys read as if at
//!   the top level, matching the layout of real Core 14 AFMs).
//! - Unknown keywords at the top level are silently ignored.
//!
//! # Errors
//!
//! [`ParseError`] carries a 1-based `line` number on every variant
//! that originates inside the source. The parser never panics on
//! ill-formed input; every malformed record is converted into a
//! [`ParseError::InvalidNumber`] or [`ParseError::MalformedRecord`].

#![doc(
    html_logo_url = "https://mosaiclang.dev/assets/A4.svg",
    html_favicon_url = "https://mosaiclang.dev/assets/A4.svg"
)]
#![deny(missing_docs)]

use std::borrow::Cow;
use std::error::Error;
use std::fmt;

/// A glyph or font bounding box, in 1/1000 em.
///
/// `f32` rather than `i16` so AFMs that emit fractional values for
/// `FontBBox` or character `B` records (rare but legal) round-trip
/// without precision loss.
///
/// # Examples
///
/// ```
/// use adobe_font_metrics::BBox;
///
/// let bbox = BBox {
///     llx: -20.0,
///     lly: -200.0,
///     urx: 1000.0,
///     ury: 900.0,
/// };
///
/// assert_eq!(bbox.urx, 1000.0);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BBox {
    /// Lower-left x coordinate.
    pub llx: f32,
    /// Lower-left y coordinate.
    pub lly: f32,
    /// Upper-right x coordinate.
    pub urx: f32,
    /// Upper-right y coordinate.
    pub ury: f32,
}

/// One entry from a `StartCharMetrics` block.
///
/// # Examples
///
/// ```
/// use std::borrow::Cow;
///
/// use adobe_font_metrics::{BBox, CharacterMetric};
///
/// let metric = CharacterMetric {
///     code: 65,
///     name: Cow::Borrowed("A"),
///     width_x: 667.0,
///     bbox: Some(BBox {
///         llx: 8.0,
///         lly: 0.0,
///         urx: 660.0,
///         ury: 718.0,
///     }),
/// };
///
/// assert_eq!(metric.name, "A");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterMetric<'a> {
    /// Encoding-table code, or `-1` if the glyph is unencoded.
    /// `i32` to accommodate multi-byte `CH <hex>` codes (e.g. CJK
    /// fonts where values exceed `i16::MAX`).
    pub code: i32,
    /// PostScript glyph name (e.g. `"A"`, `"section"`).
    pub name: Cow<'a, str>,
    /// Horizontal advance in 1/1000 em.
    pub width_x: f32,
    /// Glyph bounding box if the AFM provided one.
    pub bbox: Option<BBox>,
}

/// One entry from a `StartKernPairs` block.
///
/// # Examples
///
/// ```
/// use std::borrow::Cow;
///
/// use adobe_font_metrics::KerningPair;
///
/// let pair = KerningPair {
///     left: Cow::Borrowed("A"),
///     right: Cow::Borrowed("V"),
///     adjust: -80.0,
/// };
///
/// assert_eq!(pair.adjust, -80.0);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct KerningPair<'a> {
    /// PostScript name of the left-hand glyph.
    pub left: Cow<'a, str>,
    /// PostScript name of the right-hand glyph.
    pub right: Cow<'a, str>,
    /// Horizontal kerning adjustment in 1/1000 em. `KPY` records
    /// always store `0.0` here at v0.1; the public type does not
    /// expose vertical kerning yet.
    pub adjust: f32,
}

/// All public AFM data extracted from a single `.adobe-font-metrics` file.
///
/// `Cow` everywhere so a single type serves both runtime parsing
/// (`Cow::Borrowed` slices of the source) and compile-time baked
/// statics (`Cow::Borrowed` of `&'static`).
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), adobe_font_metrics::ParseError> {
/// use adobe_font_metrics::{FontMetrics, parse};
///
/// let src = "StartFontMetrics 4.1\nFontName Demo\nFontBBox 0 0 1000 1000\nEndFontMetrics\n";
/// let metrics: FontMetrics<'_> = parse(src)?;
///
/// assert_eq!(metrics.font_name, "Demo");
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct FontMetrics<'a> {
    /// PostScript `FontName` (e.g. `"Helvetica"`).
    pub font_name: Cow<'a, str>,
    /// Human-readable `FullName` (e.g. `"Helvetica Bold Oblique"`).
    pub full_name: Cow<'a, str>,
    /// PostScript `FamilyName` (e.g. `"Helvetica"`).
    pub family_name: Cow<'a, str>,
    /// `Weight` token, free-form per the spec (`"Medium"`, `"Bold"`, etc.).
    pub weight: Cow<'a, str>,
    /// Italic angle in degrees, counter-clockwise from vertical.
    pub italic_angle: f32,
    /// `true` if every glyph has the same advance width.
    pub is_fixed_pitch: bool,
    /// Bounding box that contains every glyph in the font.
    pub font_bbox: BBox,
    /// Recommended y position of the underline, in 1/1000 em.
    pub underline_position: f32,
    /// Recommended thickness of the underline, in 1/1000 em.
    pub underline_thickness: f32,
    /// Height of an unaccented capital, in 1/1000 em.
    pub cap_height: f32,
    /// Height of a lowercase `x`, in 1/1000 em.
    pub x_height: f32,
    /// Ascender height, in 1/1000 em.
    pub ascender: f32,
    /// Descender depth (negative for descents below the baseline).
    pub descender: f32,
    /// Encoding scheme name (e.g. `"AdobeStandardEncoding"`).
    pub encoding_scheme: Cow<'a, str>,
    /// Per-glyph metrics. [`parse`] always returns this as
    /// `Cow::Owned`; `Cow::Borrowed(&'static [...])` is reserved for
    /// compile-time-baked statics in downstream crates (e.g.
    /// `pdf-base14-metrics`).
    pub character_metrics: Cow<'a, [CharacterMetric<'a>]>,
    /// Kerning pairs. [`parse`] always returns this as `Cow::Owned`;
    /// `Cow::Borrowed(&'static [...])` is reserved for
    /// compile-time-baked statics in downstream crates (e.g.
    /// `pdf-base14-metrics`).
    pub kerning_pairs: Cow<'a, [KerningPair<'a>]>,
}

/// Convenience alias for fully-owned metrics (`'static`).
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), adobe_font_metrics::ParseError> {
/// use adobe_font_metrics::{OwnedFontMetrics, parse};
///
/// let src = "StartFontMetrics 4.1\nFontName Demo\nFontBBox 0 0 1000 1000\nEndFontMetrics\n";
/// let metrics: OwnedFontMetrics = parse(src)?.into_owned();
///
/// assert_eq!(metrics.font_name, "Demo");
/// # Ok(())
/// # }
/// ```
pub type OwnedFontMetrics = FontMetrics<'static>;

/// Errors returned by [`parse`]. Line numbers are 1-based.
///
/// # Examples
///
/// ```
/// use adobe_font_metrics::{ParseError, parse};
///
/// let err = parse("").err();
///
/// assert!(matches!(err, Some(ParseError::MissingHeader { line: 1 })));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// First non-blank, non-comment line was not `StartFontMetrics`.
    MissingHeader {
        /// 1-based source line number where the missing header was expected.
        line: usize,
    },
    /// `StartFontMetrics` declared a version outside the 4.x family.
    UnsupportedVersion {
        /// 1-based source line number of the offending `StartFontMetrics`.
        line: usize,
        /// Version literal that was rejected (e.g. `"5.0"`).
        version: String,
    },
    /// A field that the parser requires (currently `FontName` and
    /// `FontBBox`) never appeared.
    MissingRequiredField {
        /// Name of the missing field.
        field: &'static str,
    },
    /// A token that should have parsed as a number didn't.
    InvalidNumber {
        /// 1-based source line number where parsing failed.
        line: usize,
        /// Logical field whose value couldn't be parsed (e.g. `"FontBBox"`).
        field: &'static str,
        /// The raw token that failed to parse.
        value: String,
    },
    /// A record was structurally malformed (wrong arity, unrecognised
    /// boolean, etc.).
    MalformedRecord {
        /// 1-based source line number where the record appeared.
        line: usize,
        /// AFM keyword that introduced the record.
        keyword: &'static str,
        /// Human-readable description of how the record was malformed.
        reason: &'static str,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHeader { line } => {
                write!(f, "line {line}: expected StartFontMetrics header")
            }
            Self::UnsupportedVersion { line, version } => {
                write!(
                    f,
                    "line {line}: unsupported AFM version {version:?} (need 4.x)"
                )
            }
            Self::MissingRequiredField { field } => {
                write!(f, "missing required field {field}")
            }
            Self::InvalidNumber { line, field, value } => {
                write!(f, "line {line}: invalid number {value:?} for {field}")
            }
            Self::MalformedRecord {
                line,
                keyword,
                reason,
            } => {
                write!(f, "line {line}: malformed {keyword} record: {reason}")
            }
        }
    }
}

impl Error for ParseError {}

// ---------------------------------------------------------------- impls

impl CharacterMetric<'_> {
    /// Lift to `'static` by cloning any borrowed strings.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::borrow::Cow;
    ///
    /// use adobe_font_metrics::CharacterMetric;
    ///
    /// let metric = CharacterMetric {
    ///     code: 65,
    ///     name: Cow::Borrowed("A"),
    ///     width_x: 667.0,
    ///     bbox: None,
    /// };
    /// let owned = metric.into_owned();
    ///
    /// assert_eq!(owned.name, "A");
    /// ```
    #[must_use]
    pub fn into_owned(self) -> CharacterMetric<'static> {
        CharacterMetric {
            code: self.code,
            name: Cow::Owned(self.name.into_owned()),
            width_x: self.width_x,
            bbox: self.bbox,
        }
    }
}

impl KerningPair<'_> {
    /// Lift to `'static` by cloning any borrowed strings.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::borrow::Cow;
    ///
    /// use adobe_font_metrics::KerningPair;
    ///
    /// let pair = KerningPair {
    ///     left: Cow::Borrowed("A"),
    ///     right: Cow::Borrowed("V"),
    ///     adjust: -80.0,
    /// };
    /// let owned = pair.into_owned();
    ///
    /// assert_eq!(owned.right, "V");
    /// ```
    #[must_use]
    pub fn into_owned(self) -> KerningPair<'static> {
        KerningPair {
            left: Cow::Owned(self.left.into_owned()),
            right: Cow::Owned(self.right.into_owned()),
            adjust: self.adjust,
        }
    }
}

impl FontMetrics<'_> {
    /// Lift to `'static`, cloning every borrowed slice. Intended for
    /// callers who need to outlive the source `&str` (caches, baked
    /// statics, cross-thread sends).
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> Result<(), adobe_font_metrics::ParseError> {
    /// use adobe_font_metrics::{OwnedFontMetrics, parse};
    ///
    /// let src = "StartFontMetrics 4.1\nFontName Demo\nFontBBox 0 0 1000 1000\nEndFontMetrics\n";
    /// let owned: OwnedFontMetrics = parse(src)?.into_owned();
    ///
    /// assert_eq!(owned.font_bbox.urx, 1000.0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn into_owned(self) -> OwnedFontMetrics {
        let chars: Vec<CharacterMetric<'static>> = self
            .character_metrics
            .into_owned()
            .into_iter()
            .map(CharacterMetric::into_owned)
            .collect();
        let kerns: Vec<KerningPair<'static>> = self
            .kerning_pairs
            .into_owned()
            .into_iter()
            .map(KerningPair::into_owned)
            .collect();
        FontMetrics {
            font_name: Cow::Owned(self.font_name.into_owned()),
            full_name: Cow::Owned(self.full_name.into_owned()),
            family_name: Cow::Owned(self.family_name.into_owned()),
            weight: Cow::Owned(self.weight.into_owned()),
            italic_angle: self.italic_angle,
            is_fixed_pitch: self.is_fixed_pitch,
            font_bbox: self.font_bbox,
            underline_position: self.underline_position,
            underline_thickness: self.underline_thickness,
            cap_height: self.cap_height,
            x_height: self.x_height,
            ascender: self.ascender,
            descender: self.descender,
            encoding_scheme: Cow::Owned(self.encoding_scheme.into_owned()),
            character_metrics: Cow::Owned(chars),
            kerning_pairs: Cow::Owned(kerns),
        }
    }
}

// ---------------------------------------------------------------- parser

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum State {
    Top,
    CharMetrics,
    KernPairs,
    /// Inside a `StartKernPairs1` block: direction-1 kerning is not
    /// modelled by the public type, so records are dropped instead
    /// of being conflated into the direction-0 vector.
    SkipKernPairs,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum HeaderState {
    Pending,
    Seen,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum DirectionState {
    Reading,
    Skipping,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum FinishState {
    Reading,
    Done,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum Presence {
    Missing,
    Present,
}

struct ParseAccumulator<'a> {
    header: HeaderState,
    state: State,
    composites_depth: u32,
    direction: DirectionState,
    finish: FinishState,
    font_name: Cow<'a, str>,
    full_name: Cow<'a, str>,
    family_name: Cow<'a, str>,
    weight: Cow<'a, str>,
    encoding_scheme: Cow<'a, str>,
    italic_angle: f32,
    is_fixed_pitch: bool,
    font_bbox: BBox,
    font_bbox_presence: Presence,
    underline_position: f32,
    underline_thickness: f32,
    cap_height: f32,
    x_height: f32,
    ascender: f32,
    descender: f32,
    chars: Vec<CharacterMetric<'a>>,
    kerns: Vec<KerningPair<'a>>,
}

impl<'a> ParseAccumulator<'a> {
    fn new() -> Self {
        Self {
            header: HeaderState::Pending,
            state: State::Top,
            composites_depth: 0,
            direction: DirectionState::Reading,
            finish: FinishState::Reading,
            font_name: Cow::Borrowed(""),
            full_name: Cow::Borrowed(""),
            family_name: Cow::Borrowed(""),
            weight: Cow::Borrowed(""),
            encoding_scheme: Cow::Borrowed(""),
            italic_angle: 0.0,
            is_fixed_pitch: false,
            font_bbox: BBox::default(),
            font_bbox_presence: Presence::Missing,
            underline_position: 0.0,
            underline_thickness: 0.0,
            cap_height: 0.0,
            x_height: 0.0,
            ascender: 0.0,
            descender: 0.0,
            chars: Vec::new(),
            kerns: Vec::new(),
        }
    }

    fn parse_line(&mut self, raw: &'a str, lineno: usize) -> Result<(), ParseError> {
        let line = raw.trim();
        if line.is_empty() || self.is_done() {
            return Ok(());
        }
        let (kw, rest) = split_keyword(line);

        if self.skip_block_line(kw) || self.parse_header_line(kw, rest, lineno)? {
            return Ok(());
        }

        self.parse_body_line(line, kw, rest, lineno)
    }

    fn finish(self) -> Result<FontMetrics<'a>, ParseError> {
        if self.header == HeaderState::Pending {
            return Err(ParseError::MissingHeader { line: 1 });
        }
        if self.font_name.is_empty() {
            return Err(ParseError::MissingRequiredField { field: "FontName" });
        }
        if self.font_bbox_presence == Presence::Missing {
            return Err(ParseError::MissingRequiredField { field: "FontBBox" });
        }

        Ok(FontMetrics {
            font_name: self.font_name,
            full_name: self.full_name,
            family_name: self.family_name,
            weight: self.weight,
            italic_angle: self.italic_angle,
            is_fixed_pitch: self.is_fixed_pitch,
            font_bbox: self.font_bbox,
            underline_position: self.underline_position,
            underline_thickness: self.underline_thickness,
            cap_height: self.cap_height,
            x_height: self.x_height,
            ascender: self.ascender,
            descender: self.descender,
            encoding_scheme: self.encoding_scheme,
            character_metrics: Cow::Owned(self.chars),
            kerning_pairs: Cow::Owned(self.kerns),
        })
    }

    fn skip_block_line(&mut self, kw: &str) -> bool {
        if self.composites_depth > 0 {
            if kw == "EndComposites" {
                self.composites_depth -= 1;
            }
            return true;
        }
        if self.direction == DirectionState::Skipping {
            if kw == "EndDirection" {
                self.direction = DirectionState::Reading;
            }
            return true;
        }
        false
    }

    fn parse_header_line(
        &mut self,
        kw: &str,
        rest: &str,
        lineno: usize,
    ) -> Result<bool, ParseError> {
        if self.header == HeaderState::Seen {
            return Ok(false);
        }
        if kw == "Comment" {
            return Ok(true);
        }
        if kw != "StartFontMetrics" {
            return Err(ParseError::MissingHeader { line: lineno });
        }
        let version = rest.trim();
        let is_v4 = version.split_once('.').is_some_and(|(major, minor)| {
            major == "4" && !minor.is_empty() && minor.bytes().all(|b| b.is_ascii_digit())
        });
        if !is_v4 {
            return Err(ParseError::UnsupportedVersion {
                line: lineno,
                version: version.to_owned(),
            });
        }
        self.header = HeaderState::Seen;
        Ok(true)
    }

    fn is_done(&self) -> bool {
        self.finish == FinishState::Done
    }

    fn parse_body_line(
        &mut self,
        line: &'a str,
        kw: &str,
        rest: &'a str,
        lineno: usize,
    ) -> Result<(), ParseError> {
        match kw {
            "EndFontMetrics" => self.finish = FinishState::Done,
            "StartComposites" => self.composites_depth = 1,
            "FontName" => self.font_name = Cow::Borrowed(rest.trim()),
            "FullName" => self.full_name = Cow::Borrowed(rest.trim()),
            "FamilyName" => self.family_name = Cow::Borrowed(rest.trim()),
            "Weight" => self.weight = Cow::Borrowed(rest.trim()),
            "EncodingScheme" => self.encoding_scheme = Cow::Borrowed(rest.trim()),
            "ItalicAngle" => self.italic_angle = parse_f32(rest, "ItalicAngle", lineno)?,
            "IsFixedPitch" => self.is_fixed_pitch = parse_bool(rest, lineno)?,
            "UnderlinePosition" => {
                self.underline_position = parse_f32(rest, "UnderlinePosition", lineno)?;
            }
            "UnderlineThickness" => {
                self.underline_thickness = parse_f32(rest, "UnderlineThickness", lineno)?;
            }
            "CapHeight" => self.cap_height = parse_f32(rest, "CapHeight", lineno)?,
            "XHeight" => self.x_height = parse_f32(rest, "XHeight", lineno)?,
            "Ascender" => self.ascender = parse_f32(rest, "Ascender", lineno)?,
            "Descender" => self.descender = parse_f32(rest, "Descender", lineno)?,
            "FontBBox" => {
                self.font_bbox = parse_bbox(rest, "FontBBox", lineno)?;
                self.font_bbox_presence = Presence::Present;
            }
            "StartCharMetrics" => self.start_char_metrics(rest),
            "EndCharMetrics" | "EndKernPairs" | "EndKernData" => self.state = State::Top,
            "StartKernData" => self.state = State::KernPairs,
            "StartKernPairs" | "StartKernPairs0" => self.start_kern_pairs(rest),
            "StartKernPairs1" => self.state = State::SkipKernPairs,
            "StartDirection" => self.start_direction(rest),
            "C" | "CH" if self.state == State::CharMetrics => {
                self.chars.push(parse_char_metric_line(line, lineno)?);
            }
            "KPX" | "KPY" | "KP" if self.state == State::KernPairs => {
                if let Some(pair) = parse_kern_record(kw, rest, lineno)? {
                    self.kerns.push(pair);
                }
            }
            "KPH" if self.state == State::KernPairs => {}
            _ => {}
        }
        Ok(())
    }

    fn start_char_metrics(&mut self, rest: &str) {
        if let Ok(n) = rest.trim().parse::<usize>() {
            self.chars.reserve(n);
        }
        self.state = State::CharMetrics;
    }

    fn start_kern_pairs(&mut self, rest: &str) {
        if let Ok(n) = rest.trim().parse::<usize>() {
            self.kerns.reserve(n);
        }
        self.state = State::KernPairs;
    }

    fn start_direction(&mut self, rest: &str) {
        if rest.trim().parse::<u8>().unwrap_or(0) == 1 {
            self.direction = DirectionState::Skipping;
        }
    }
}

/// Parse an AFM file into a borrowed [`FontMetrics`].
///
/// The returned struct borrows from `src`. Use
/// [`FontMetrics::into_owned`] to detach.
///
/// # Errors
///
/// Returns [`ParseError`] if the header is missing, the version is
/// outside the 4.x range, a required field never appears, or any
/// record is structurally malformed.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), adobe_font_metrics::ParseError> {
/// use adobe_font_metrics::parse;
///
/// let src = "StartFontMetrics 4.1\nFontName Demo\nFontBBox 0 0 1000 1000\nEndFontMetrics\n";
/// let metrics = parse(src)?;
///
/// assert_eq!(metrics.font_name, "Demo");
/// # Ok(())
/// # }
/// ```
#[must_use = "discarding the parsed FontMetrics also discards any parse error"]
pub fn parse(src: &str) -> Result<FontMetrics<'_>, ParseError> {
    let mut accumulator = ParseAccumulator::new();

    for (idx, raw) in src.lines().enumerate() {
        accumulator.parse_line(raw, idx + 1)?;
        if accumulator.is_done() {
            break;
        }
    }

    accumulator.finish()
}

// ---------------------------------------------------------------- helpers

fn split_keyword(line: &str) -> (&str, &str) {
    line.find(|c: char| c.is_ascii_whitespace())
        .map_or((line, ""), |i| (&line[..i], &line[i..]))
}

fn parse_f32(s: &str, field: &'static str, lineno: usize) -> Result<f32, ParseError> {
    let trimmed = s.trim();
    trimmed
        .parse::<f32>()
        .map_err(|_e| ParseError::InvalidNumber {
            line: lineno,
            field,
            value: trimmed.to_owned(),
        })
}

fn parse_i32(s: &str, field: &'static str, lineno: usize) -> Result<i32, ParseError> {
    let trimmed = s.trim();
    trimmed
        .parse::<i32>()
        .map_err(|_e| ParseError::InvalidNumber {
            line: lineno,
            field,
            value: trimmed.to_owned(),
        })
}

fn parse_bool(s: &str, lineno: usize) -> Result<bool, ParseError> {
    match s.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ParseError::MalformedRecord {
            line: lineno,
            keyword: "IsFixedPitch",
            reason: "expected `true` or `false`",
        }),
    }
}

fn parse_bbox(s: &str, field: &'static str, lineno: usize) -> Result<BBox, ParseError> {
    let mut toks = s.split_ascii_whitespace();
    let llx = next_f32(&mut toks, field, lineno)?;
    let lly = next_f32(&mut toks, field, lineno)?;
    let urx = next_f32(&mut toks, field, lineno)?;
    let ury = next_f32(&mut toks, field, lineno)?;
    if toks.next().is_some() {
        return Err(ParseError::MalformedRecord {
            line: lineno,
            keyword: field,
            reason: "too many numbers",
        });
    }
    Ok(BBox { llx, lly, urx, ury })
}

fn next_f32(
    toks: &mut std::str::SplitAsciiWhitespace<'_>,
    field: &'static str,
    lineno: usize,
) -> Result<f32, ParseError> {
    let t = toks.next().ok_or(ParseError::MalformedRecord {
        line: lineno,
        keyword: field,
        reason: "expected number",
    })?;
    parse_f32(t, field, lineno)
}

fn parse_char_metric_line(line: &str, lineno: usize) -> Result<CharacterMetric<'_>, ParseError> {
    let mut code: i32 = -1;
    let mut name: &str = "";
    let mut width_x: f32 = 0.0;
    let mut bbox: Option<BBox> = None;

    for seg in line.split(';') {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        let (tok, rest) = split_keyword(seg);
        let rest = rest.trim();
        match tok {
            "C" => code = parse_i32(rest, "C", lineno)?,
            "CH" => {
                let hex = rest.trim_start_matches('<').trim_end_matches('>').trim();
                code = i32::from_str_radix(hex, 16).map_err(|_e| ParseError::InvalidNumber {
                    line: lineno,
                    field: "CH",
                    value: rest.to_owned(),
                })?;
            }
            "WX" | "W0X" => width_x = parse_f32(rest, "WX", lineno)?,
            "W" | "W0" => {
                let x =
                    rest.split_ascii_whitespace()
                        .next()
                        .ok_or(ParseError::MalformedRecord {
                            line: lineno,
                            keyword: "W",
                            reason: "missing x advance",
                        })?;
                width_x = parse_f32(x, "W", lineno)?;
            }
            "N" => name = rest,
            "B" => bbox = Some(parse_bbox(rest, "B", lineno)?),
            _ => {} // WY, L, VV, etc.: silently ignored
        }
    }

    Ok(CharacterMetric {
        code,
        name: Cow::Borrowed(name),
        width_x,
        bbox,
    })
}

fn parse_kern_record<'a>(
    kw: &str,
    rest: &'a str,
    lineno: usize,
) -> Result<Option<KerningPair<'a>>, ParseError> {
    // Resolve the canonical keyword up front so error messages and the
    // arity check below carry the actual record name, not `"KP*"`.
    let keyword = match kw {
        "KPX" => "KPX",
        "KPY" => "KPY",
        "KP" => "KP",
        _ => return Ok(None),
    };
    let mut toks = rest.split_ascii_whitespace();
    let left = toks.next().ok_or(ParseError::MalformedRecord {
        line: lineno,
        keyword,
        reason: "missing left glyph name",
    })?;
    let right = toks.next().ok_or(ParseError::MalformedRecord {
        line: lineno,
        keyword,
        reason: "missing right glyph name",
    })?;
    let first_num = toks.next().ok_or(ParseError::MalformedRecord {
        line: lineno,
        keyword,
        reason: "missing kern adjustment",
    })?;
    let adjust = match keyword {
        "KPX" => parse_f32(first_num, "KPX", lineno)?,
        "KPY" => {
            // Validate the operand even though we discard it: a y-only
            // kern still has to be a well-formed number.
            let _ = parse_f32(first_num, "KPY", lineno)?;
            0.0
        }
        "KP" => {
            // `KP left right xadj yadj`: both operands required.
            let x = parse_f32(first_num, "KP", lineno)?;
            let y = toks.next().ok_or(ParseError::MalformedRecord {
                line: lineno,
                keyword,
                reason: "missing y kern adjustment",
            })?;
            let _ = parse_f32(y, "KP", lineno)?;
            x
        }
        // Unreachable: `keyword` was set from the same set of literals.
        _ => return Ok(None),
    };
    if toks.next().is_some() {
        return Err(ParseError::MalformedRecord {
            line: lineno,
            keyword,
            reason: "too many operands",
        });
    }
    Ok(Some(KerningPair {
        left: Cow::Borrowed(left),
        right: Cow::Borrowed(right),
        adjust,
    }))
}
