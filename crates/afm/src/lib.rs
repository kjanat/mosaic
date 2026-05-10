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
//! `Cow<'_, str>` fields point into the source slice — zero allocations
//! for glyph names and kerning operands. Call [`FontMetrics::into_owned`]
//! to obtain an [`OwnedFontMetrics`] (`FontMetrics<'static>`) suitable
//! for caching, baking into static tables, or sending across threads.
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
//!   the X axis is exposed in the public type today).
//! - `StartComposites`/`CC` blocks are accepted and discarded per the
//!   user-facing scope of the v0.1 surface.
//! - Unknown keywords at the top level are silently ignored.
//!
//! # Errors
//!
//! [`ParseError`] carries a 1-based `line` number on every variant
//! that originates inside the source. The parser never panics on
//! ill-formed input — every malformed record is converted into a
//! [`ParseError::InvalidNumber`] or [`ParseError::MalformedRecord`].

#![deny(missing_docs)]

use std::borrow::Cow;
use std::error::Error;
use std::fmt;

/// A glyph or font bounding box, in 1/1000 em.
///
/// `f32` rather than `i16` so AFMs that emit fractional values for
/// `FontBBox` or character `B` records (rare but legal) round-trip
/// without precision loss.
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
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterMetric<'a> {
    /// Encoding-table code, or `-1` if the glyph is unencoded.
    pub code: i16,
    /// PostScript glyph name (e.g. `"A"`, `"section"`).
    pub name: Cow<'a, str>,
    /// Horizontal advance in 1/1000 em.
    pub width_x: f32,
    /// Glyph bounding box if the AFM provided one.
    pub bbox: Option<BBox>,
}

/// One entry from a `StartKernPairs` block.
#[derive(Debug, Clone, PartialEq)]
pub struct KerningPair<'a> {
    /// PostScript name of the left-hand glyph.
    pub left: Cow<'a, str>,
    /// PostScript name of the right-hand glyph.
    pub right: Cow<'a, str>,
    /// Horizontal kerning adjustment in 1/1000 em. `KPY` records
    /// always store `0.0` here at v0.1 — the public type does not
    /// expose vertical kerning yet.
    pub adjust: f32,
}

/// All public AFM data extracted from a single `.afm` file.
///
/// `Cow` everywhere so a single type serves both runtime parsing
/// (`Cow::Borrowed` slices of the source) and compile-time baked
/// statics (`Cow::Borrowed` of `&'static`).
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
pub type OwnedFontMetrics = FontMetrics<'static>;

/// Errors returned by [`parse`]. Line numbers are 1-based.
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

impl<'a> CharacterMetric<'a> {
    /// Lift to `'static` by cloning any borrowed strings.
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

impl<'a> KerningPair<'a> {
    /// Lift to `'static` by cloning any borrowed strings.
    #[must_use]
    pub fn into_owned(self) -> KerningPair<'static> {
        KerningPair {
            left: Cow::Owned(self.left.into_owned()),
            right: Cow::Owned(self.right.into_owned()),
            adjust: self.adjust,
        }
    }
}

impl<'a> FontMetrics<'a> {
    /// Lift to `'static`, cloning every borrowed slice. Intended for
    /// callers who need to outlive the source `&str` (caches, baked
    /// statics, cross-thread sends).
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
#[must_use = "discarding the parsed FontMetrics also discards any parse error"]
pub fn parse(src: &str) -> Result<FontMetrics<'_>, ParseError> {
    let mut header_seen = false;
    let mut state = State::Top;
    let mut composites_depth: u32 = 0;

    let mut font_name: Cow<'_, str> = Cow::Borrowed("");
    let mut full_name: Cow<'_, str> = Cow::Borrowed("");
    let mut family_name: Cow<'_, str> = Cow::Borrowed("");
    let mut weight: Cow<'_, str> = Cow::Borrowed("");
    let mut encoding_scheme: Cow<'_, str> = Cow::Borrowed("");
    let mut italic_angle: f32 = 0.0;
    let mut is_fixed_pitch = false;
    let mut font_bbox = BBox::default();
    let mut font_bbox_seen = false;
    let mut underline_position: f32 = 0.0;
    let mut underline_thickness: f32 = 0.0;
    let mut cap_height: f32 = 0.0;
    let mut x_height: f32 = 0.0;
    let mut ascender: f32 = 0.0;
    let mut descender: f32 = 0.0;
    let mut chars: Vec<CharacterMetric<'_>> = Vec::new();
    let mut kerns: Vec<KerningPair<'_>> = Vec::new();

    for (idx, raw) in src.lines().enumerate() {
        let lineno = idx + 1;
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let (kw, rest) = split_keyword(line);

        // Skip composite blocks wholesale.
        if composites_depth > 0 {
            if kw == "EndComposites" {
                composites_depth -= 1;
            }
            continue;
        }

        if !header_seen {
            if kw == "Comment" {
                continue;
            }
            if kw != "StartFontMetrics" {
                return Err(ParseError::MissingHeader { line: lineno });
            }
            let version = rest.trim();
            // Strict `4.<digits>` — reject `4.`, `4.x`, `4.bad`, etc.
            let is_v4 = version.split_once('.').is_some_and(|(major, minor)| {
                major == "4" && !minor.is_empty() && minor.bytes().all(|b| b.is_ascii_digit())
            });
            if !is_v4 {
                return Err(ParseError::UnsupportedVersion {
                    line: lineno,
                    version: version.to_owned(),
                });
            }
            header_seen = true;
            continue;
        }

        match kw {
            "EndFontMetrics" => break,
            "StartComposites" => composites_depth = 1,

            "FontName" => font_name = Cow::Borrowed(rest.trim()),
            "FullName" => full_name = Cow::Borrowed(rest.trim()),
            "FamilyName" => family_name = Cow::Borrowed(rest.trim()),
            "Weight" => weight = Cow::Borrowed(rest.trim()),
            "EncodingScheme" => encoding_scheme = Cow::Borrowed(rest.trim()),

            "ItalicAngle" => italic_angle = parse_f32(rest, "ItalicAngle", lineno)?,
            "IsFixedPitch" => is_fixed_pitch = parse_bool(rest, lineno)?,
            "UnderlinePosition" => {
                underline_position = parse_f32(rest, "UnderlinePosition", lineno)?;
            }
            "UnderlineThickness" => {
                underline_thickness = parse_f32(rest, "UnderlineThickness", lineno)?;
            }
            "CapHeight" => cap_height = parse_f32(rest, "CapHeight", lineno)?,
            "XHeight" => x_height = parse_f32(rest, "XHeight", lineno)?,
            "Ascender" => ascender = parse_f32(rest, "Ascender", lineno)?,
            "Descender" => descender = parse_f32(rest, "Descender", lineno)?,
            "FontBBox" => {
                font_bbox = parse_bbox(rest, "FontBBox", lineno)?;
                font_bbox_seen = true;
            }

            "StartCharMetrics" => {
                if let Ok(n) = rest.trim().parse::<usize>() {
                    chars.reserve(n);
                }
                state = State::CharMetrics;
            }
            "EndCharMetrics" | "EndKernPairs" | "EndKernData" => state = State::Top,

            "StartKernData" => state = State::KernPairs,
            "StartKernPairs" | "StartKernPairs0" => {
                if let Ok(n) = rest.trim().parse::<usize>() {
                    kerns.reserve(n);
                }
                state = State::KernPairs;
            }

            "C" | "CH" if state == State::CharMetrics => {
                chars.push(parse_char_metric_line(line, lineno)?);
            }

            "KPX" | "KPY" | "KP" if state == State::KernPairs => {
                if let Some(pair) = parse_kern_record(kw, rest, lineno)? {
                    kerns.push(pair);
                }
            }
            "KPH" if state == State::KernPairs => {
                // Hex-encoded kern pairs — accepted and discarded; the
                // public type doesn't model decoded byte-coded names.
            }

            _ => {} // unknown / out-of-context: silently skip
        }
    }

    if !header_seen {
        return Err(ParseError::MissingHeader { line: 1 });
    }
    if font_name.is_empty() {
        return Err(ParseError::MissingRequiredField { field: "FontName" });
    }
    if !font_bbox_seen {
        return Err(ParseError::MissingRequiredField { field: "FontBBox" });
    }

    Ok(FontMetrics {
        font_name,
        full_name,
        family_name,
        weight,
        italic_angle,
        is_fixed_pitch,
        font_bbox,
        underline_position,
        underline_thickness,
        cap_height,
        x_height,
        ascender,
        descender,
        encoding_scheme,
        character_metrics: Cow::Owned(chars),
        kerning_pairs: Cow::Owned(kerns),
    })
}

// ---------------------------------------------------------------- helpers

fn split_keyword(line: &str) -> (&str, &str) {
    match line.find(|c: char| c.is_ascii_whitespace()) {
        Some(i) => (&line[..i], &line[i..]),
        None => (line, ""),
    }
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

fn parse_i16(s: &str, field: &'static str, lineno: usize) -> Result<i16, ParseError> {
    let trimmed = s.trim();
    trimmed
        .parse::<i16>()
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
    let mut code: i16 = -1;
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
            "C" => code = parse_i16(rest, "C", lineno)?,
            "CH" => {
                let hex = rest.trim_start_matches('<').trim_end_matches('>').trim();
                code = i16::from_str_radix(hex, 16).map_err(|_e| ParseError::InvalidNumber {
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
            _ => {} // WY, L, VV, etc. — silently ignored
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
    let mut toks = rest.split_ascii_whitespace();
    let left = toks.next().ok_or(ParseError::MalformedRecord {
        line: lineno,
        keyword: "KP*",
        reason: "missing left glyph name",
    })?;
    let right = toks.next().ok_or(ParseError::MalformedRecord {
        line: lineno,
        keyword: "KP*",
        reason: "missing right glyph name",
    })?;
    let first_num = toks.next().ok_or(ParseError::MalformedRecord {
        line: lineno,
        keyword: "KP*",
        reason: "missing kern adjustment",
    })?;
    let adjust = match kw {
        "KPX" => parse_f32(first_num, "KPX", lineno)?,
        "KPY" => {
            // Validate the operand even though we discard it: a y-only
            // kern still has to be a well-formed number.
            let _ = parse_f32(first_num, "KPY", lineno)?;
            0.0
        }
        "KP" => {
            // `KP left right xadj yadj` — both operands required.
            let x = parse_f32(first_num, "KP", lineno)?;
            let y = toks.next().ok_or(ParseError::MalformedRecord {
                line: lineno,
                keyword: "KP",
                reason: "missing y kern adjustment",
            })?;
            let _ = parse_f32(y, "KP", lineno)?;
            x
        }
        _ => return Ok(None),
    };
    Ok(Some(KerningPair {
        left: Cow::Borrowed(left),
        right: Cow::Borrowed(right),
        adjust,
    }))
}
