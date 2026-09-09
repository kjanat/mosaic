use std::borrow::Cow;

use crate::records::{
    self, bbox, boolean, integer, keyword, malformed, number, source_record, vector,
};
use crate::{Direction, DirectionMetrics, FontMetrics, MetricsSets, ParseError, RecordContext};

#[derive(Debug, Clone, Copy)]
struct Count {
    expected: usize,
    seen: usize,
    keyword: &'static str,
}

impl Count {
    fn new(
        value: &str,
        keyword: &'static str,
        line: usize,
        input_len: usize,
    ) -> Result<Self, ParseError> {
        let expected = integer(value, keyword, line)?;
        // A record consumes at least one input byte. Never allocate from an untrusted count.
        if expected > input_len {
            return Err(malformed(
                line,
                keyword,
                "declared count exceeds input size",
            ));
        }
        Ok(Self {
            expected,
            seen: 0,
            keyword,
        })
    }
    fn add(&mut self, line: usize) -> Result<(), ParseError> {
        self.seen += 1;
        if self.seen > self.expected {
            return Err(malformed(line, self.keyword, "more records than declared"));
        }
        Ok(())
    }
    fn close(self, line: usize) -> Result<(), ParseError> {
        if self.seen != self.expected {
            return Err(malformed(
                line,
                self.keyword,
                "record count does not match declaration",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum Section {
    Font,
    Direction(MetricsSets),
    Characters(Count),
    Pairs(Direction, Count),
    Tracks(Count),
    Composites(Count),
}

struct Reader<'a> {
    font: FontMetrics<'a>,
    section: Section,
    header_seen: bool,
    bbox_seen: bool,
    kern_data: bool,
    finished: bool,
    input_len: usize,
    direction_lines: [usize; 2],
    vector_line: usize,
}

impl<'a> Reader<'a> {
    fn new(input_len: usize) -> Self {
        Self {
            font: FontMetrics::default(),
            section: Section::Font,
            header_seen: false,
            bbox_seen: false,
            kern_data: false,
            finished: false,
            input_len,
            direction_lines: [1; 2],
            vector_line: 1,
        }
    }

    fn context(&self) -> RecordContext {
        match self.section {
            Section::Font if self.kern_data => RecordContext::KernData,
            Section::Font => RecordContext::Font,
            Section::Direction(direction) => RecordContext::Direction(direction),
            Section::Characters(_) => RecordContext::CharacterMetrics,
            Section::Pairs(direction, _) => RecordContext::KernPairs(direction),
            Section::Tracks(_) => RecordContext::TrackKern,
            Section::Composites(_) => RecordContext::Composites,
        }
    }

    fn retain(&mut self, line: usize, key: &'a str, value: &'a str) {
        let context = self.context();
        source_record(self.font.source_records.to_mut(), line, context, key, value);
    }

    fn line(&mut self, raw: &'a str, line: usize) -> Result<(), ParseError> {
        let text = raw.trim();
        if text.is_empty() {
            return Ok(());
        }
        if self.finished {
            return Err(malformed(
                line,
                "EndFontMetrics",
                "unexpected trailing data",
            ));
        }
        let (key, value) = keyword(text);
        if key == "Comment" {
            self.retain(line, key, value);
            return Ok(());
        }
        if !self.header_seen {
            if key != "StartFontMetrics" {
                return Err(ParseError::MissingHeader { line });
            }
            if !value.split_once('.').is_some_and(|(major, minor)| {
                major == "4" && !minor.is_empty() && minor.bytes().all(|b| b.is_ascii_digit())
            }) {
                return Err(ParseError::UnsupportedVersion {
                    line,
                    version: value.to_owned(),
                });
            }
            self.header_seen = true;
            self.font.afm_version = Cow::Borrowed(value);
            return Ok(());
        }
        if self.control(key, value, line)? {
            return Ok(());
        }
        match self.section {
            Section::Font if !self.kern_data => {
                if !self.global(key, value, line)?
                    && !self.direction_metric(key, value, line, MetricsSets::Zero)?
                {
                    self.extra_or_misplaced(key, value, line)?;
                }
            }
            Section::Direction(selector) => {
                if !self.direction_metric(key, value, line, selector)? {
                    self.extra_or_misplaced(key, value, line)?;
                }
            }
            Section::Characters(_) => {
                if character_line(text) {
                    let character = records::character(
                        text,
                        line,
                        self.font.character_metrics.len(),
                        self.font.source_records.to_mut(),
                    )?;
                    self.font.character_metrics.to_mut().push(character);
                    if let Section::Characters(count) = &mut self.section {
                        count.add(line)?;
                    }
                } else {
                    self.extra_or_misplaced(key, value, line)?;
                }
            }
            Section::Pairs(direction, _) if matches!(key, "KP" | "KPX" | "KPY" | "KPH") => {
                self.font
                    .kerning_pairs
                    .to_mut()
                    .push(records::pair(key, value, line, direction)?);
                if let Section::Pairs(_, count) = &mut self.section {
                    count.add(line)?;
                }
            }
            Section::Tracks(_) if key == "TrackKern" => {
                self.font
                    .track_kerns
                    .to_mut()
                    .push(records::track(value, line)?);
                if let Section::Tracks(count) = &mut self.section {
                    count.add(line)?;
                }
            }
            Section::Composites(_) if key == "CC" => {
                let composite = records::composite(
                    text,
                    line,
                    self.font.composites.len(),
                    self.font.source_records.to_mut(),
                )?;
                self.font.composites.to_mut().push(composite);
                if let Section::Composites(count) = &mut self.section {
                    count.add(line)?;
                }
            }
            _ => self.extra_or_misplaced(key, value, line)?,
        }
        Ok(())
    }

    fn extra_or_misplaced(
        &mut self,
        key: &'a str,
        value: &'a str,
        line: usize,
    ) -> Result<(), ParseError> {
        if records::known_record(key) {
            return Err(malformed(
                line,
                "section",
                "modeled record in an incompatible section",
            ));
        }
        self.retain(line, key, value);
        Ok(())
    }

    fn require_font_section(
        &self,
        key: &'static str,
        line: usize,
        allow_kern: bool,
    ) -> Result<(), ParseError> {
        if !matches!(self.section, Section::Font) || (!allow_kern && self.kern_data) {
            return Err(malformed(line, key, "incompatible or unclosed section"));
        }
        Ok(())
    }

    fn control(&mut self, key: &str, value: &str, line: usize) -> Result<bool, ParseError> {
        match key {
            "StartFontMetrics" => {
                return Err(malformed(line, "StartFontMetrics", "duplicate header"));
            }
            "EndFontMetrics" => {
                self.require_font_section("EndFontMetrics", line, false)?;
                records::operands::<0>(value, "EndFontMetrics", line)?;
                self.finished = true;
            }
            "StartDirection" => {
                self.require_font_section("StartDirection", line, false)?;
                self.section = Section::Direction(selector(value, "StartDirection", line)?);
            }
            "EndDirection" => {
                if !matches!(self.section, Section::Direction(_)) {
                    return Err(malformed(line, "EndDirection", "no open direction section"));
                }
                records::operands::<0>(value, "EndDirection", line)?;
                self.section = Section::Font;
            }
            "StartCharMetrics" => {
                self.require_font_section("StartCharMetrics", line, false)?;
                self.section = Section::Characters(Count::new(
                    value,
                    "StartCharMetrics",
                    line,
                    self.input_len,
                )?);
            }
            "EndCharMetrics" => {
                let Section::Characters(count) = self.section else {
                    return Err(malformed(
                        line,
                        "EndCharMetrics",
                        "no open character section",
                    ));
                };
                records::operands::<0>(value, "EndCharMetrics", line)?;
                count.close(line)?;
                self.section = Section::Font;
            }
            "StartKernData" => {
                self.require_font_section("StartKernData", line, false)?;
                records::operands::<0>(value, "StartKernData", line)?;
                self.kern_data = true;
            }
            "EndKernData" => {
                self.require_font_section("EndKernData", line, true)?;
                if !self.kern_data {
                    return Err(malformed(line, "EndKernData", "no open kerning container"));
                }
                records::operands::<0>(value, "EndKernData", line)?;
                self.kern_data = false;
            }
            "StartKernPairs" | "StartKernPairs0" | "StartKernPairs1" => {
                self.require_font_section("StartKernPairs", line, true)?;
                let (direction, field) = match key {
                    "StartKernPairs0" => (Direction::Zero, "StartKernPairs0"),
                    "StartKernPairs1" => (Direction::One, "StartKernPairs1"),
                    _ => (Direction::Zero, "StartKernPairs"),
                };
                self.section =
                    Section::Pairs(direction, Count::new(value, field, line, self.input_len)?);
            }
            "EndKernPairs" => {
                let Section::Pairs(_, count) = self.section else {
                    return Err(malformed(line, "EndKernPairs", "no open pair section"));
                };
                records::operands::<0>(value, "EndKernPairs", line)?;
                count.close(line)?;
                self.section = Section::Font;
            }
            "StartTrackKern" => {
                self.require_font_section("StartTrackKern", line, true)?;
                self.section =
                    Section::Tracks(Count::new(value, "StartTrackKern", line, self.input_len)?);
            }
            "EndTrackKern" => {
                let Section::Tracks(count) = self.section else {
                    return Err(malformed(line, "EndTrackKern", "no open track section"));
                };
                records::operands::<0>(value, "EndTrackKern", line)?;
                count.close(line)?;
                self.section = Section::Font;
            }
            "StartComposites" => {
                self.require_font_section("StartComposites", line, false)?;
                self.section = Section::Composites(Count::new(
                    value,
                    "StartComposites",
                    line,
                    self.input_len,
                )?);
            }
            "EndComposites" => {
                let Section::Composites(count) = self.section else {
                    return Err(malformed(
                        line,
                        "EndComposites",
                        "no open composite section",
                    ));
                };
                records::operands::<0>(value, "EndComposites", line)?;
                count.close(line)?;
                self.section = Section::Font;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn direction_metric(
        &mut self,
        key: &str,
        value: &str,
        line: usize,
        selector: MetricsSets,
    ) -> Result<bool, ParseError> {
        let mut changes = DirectionMetrics::default();
        match key {
            "UnderlinePosition" => {
                changes.underline_position = Some(number(value, "UnderlinePosition", line)?);
            }
            "UnderlineThickness" => {
                changes.underline_thickness = Some(number(value, "UnderlineThickness", line)?);
            }
            "ItalicAngle" => changes.italic_angle = Some(number(value, "ItalicAngle", line)?),
            "CharWidth" => changes.char_width = Some(vector(value, "CharWidth", line)?),
            "IsFixedPitch" => changes.is_fixed_pitch = Some(boolean(value, "IsFixedPitch", line)?),
            _ => return Ok(false),
        }
        for direction in [Direction::Zero, Direction::One] {
            if matches!(
                (selector, direction),
                (MetricsSets::Zero, Direction::One) | (MetricsSets::One, Direction::Zero)
            ) {
                continue;
            }
            let metrics = &mut self.font.directions[direction.index()];
            metrics.underline_position = changes.underline_position.or(metrics.underline_position);
            metrics.underline_thickness =
                changes.underline_thickness.or(metrics.underline_thickness);
            metrics.italic_angle = changes.italic_angle.or(metrics.italic_angle);
            metrics.char_width = changes.char_width.or(metrics.char_width);
            metrics.is_fixed_pitch = changes.is_fixed_pitch.or(metrics.is_fixed_pitch);
            self.direction_lines[direction.index()] = line;
        }
        Ok(true)
    }

    fn global(&mut self, key: &'a str, value: &'a str, line: usize) -> Result<bool, ParseError> {
        match key {
            "FontName" => self.font.font_name = Cow::Borrowed(value),
            "FontBBox" => {
                self.font.font_bbox = bbox(value, "FontBBox", line)?;
                self.bbox_seen = true;
            }
            "MetricsSets" => self.font.metrics_sets = Some(selector(value, "MetricsSets", line)?),
            "VVector" => {
                self.font.v_vector = Some(vector(value, "VVector", line)?);
                self.vector_line = line;
            }
            "FullName" => self.font.full_name = Some(Cow::Borrowed(value)),
            "FamilyName" => self.font.family_name = Some(Cow::Borrowed(value)),
            "Weight" => self.font.weight = Some(Cow::Borrowed(value)),
            "Version" => self.font.version = Some(Cow::Borrowed(value)),
            "Notice" => self.font.notice = Some(Cow::Borrowed(value)),
            "EncodingScheme" => self.font.encoding_scheme = Some(Cow::Borrowed(value)),
            "CharacterSet" => self.font.character_set = Some(Cow::Borrowed(value)),
            "MappingScheme" => {
                self.font.mapping_scheme = Some(integer(value, "MappingScheme", line)?);
            }
            "EscChar" => self.font.esc_char = Some(integer(value, "EscChar", line)?),
            "Characters" => self.font.characters = Some(integer(value, "Characters", line)?),
            "IsBaseFont" => self.font.is_base_font = Some(boolean(value, "IsBaseFont", line)?),
            "IsCIDFont" => self.font.is_cid_font = Some(boolean(value, "IsCIDFont", line)?),
            "IsFixedV" => self.font.is_fixed_v = Some(boolean(value, "IsFixedV", line)?),
            "CapHeight" => self.font.cap_height = Some(number(value, "CapHeight", line)?),
            "XHeight" => self.font.x_height = Some(number(value, "XHeight", line)?),
            "Ascender" => self.font.ascender = Some(number(value, "Ascender", line)?),
            "Descender" => self.font.descender = Some(number(value, "Descender", line)?),
            "StdHW" => self.font.std_hw = Some(number(value, "StdHW", line)?),
            "StdVW" => self.font.std_vw = Some(number(value, "StdVW", line)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn finish(self, line: usize) -> Result<FontMetrics<'a>, ParseError> {
        if !self.header_seen {
            return Err(ParseError::MissingHeader { line: 1 });
        }
        if self.font.font_name.is_empty() {
            return Err(ParseError::MissingRequiredField { field: "FontName" });
        }
        if !self.bbox_seen {
            return Err(ParseError::MissingRequiredField { field: "FontBBox" });
        }
        if !self.finished {
            return Err(malformed(
                line,
                "EndFontMetrics",
                "missing closing font marker",
            ));
        }
        if self.font.mapping_scheme == Some(3) && self.font.esc_char.is_none() {
            return Err(ParseError::MissingRequiredField { field: "EscChar" });
        }
        if self.font.is_fixed_v == Some(false) && self.font.v_vector.is_some() {
            return Err(malformed(
                self.vector_line,
                "VVector",
                "global vector conflicts with IsFixedV false",
            ));
        }
        for direction in [Direction::Zero, Direction::One] {
            let metrics = self.font.direction(direction);
            if metrics.char_width.is_some() && metrics.is_fixed_pitch == Some(false) {
                return Err(malformed(
                    self.direction_lines[direction.index()],
                    "CharWidth",
                    "global width conflicts with IsFixedPitch false",
                ));
            }
        }
        Ok(self.font)
    }
}

fn selector(value: &str, field: &'static str, line: usize) -> Result<MetricsSets, ParseError> {
    match integer::<u8>(value, field, line)? {
        0 => Ok(MetricsSets::Zero),
        1 => Ok(MetricsSets::One),
        2 => Ok(MetricsSets::Both),
        _ => Err(malformed(
            line,
            field,
            "expected direction selector 0, 1, or 2",
        )),
    }
}

fn character_line(text: &str) -> bool {
    text.split(';').any(|part| {
        matches!(
            keyword(part).0,
            "C" | "CH"
                | "N"
                | "B"
                | "WX"
                | "WY"
                | "W0X"
                | "W0Y"
                | "W1X"
                | "W1Y"
                | "W"
                | "W0"
                | "W1"
                | "VV"
                | "L"
        )
    })
}

fn validate_text(source: &[u8]) -> Result<(), ParseError> {
    let mut line = 1;
    for (offset, &value) in source.iter().enumerate() {
        if !matches!(value, b'\t' | b'\n' | b'\r' | 0x1b | 0x20..=0x7e) {
            return Err(ParseError::InvalidByte {
                offset,
                line,
                value,
            });
        }
        if value == b'\n' {
            line += 1;
        }
    }
    Ok(())
}

fn read(source: &str) -> Result<FontMetrics<'_>, ParseError> {
    let mut reader = Reader::new(source.len());
    let mut line = 0;
    for raw in source.lines() {
        line += 1;
        reader.line(raw, line)?;
    }
    reader.finish(line + 1)
}

/// Parse AFM v4.x text into borrowed font metrics.
///
/// Accepts LF/CRLF ASCII text and preserves comments and uninterpreted records.
/// Optional fields retain absence. See the crate documentation for coverage.
///
/// # Errors
/// Returns a source-located [`ParseError`] for invalid text, unsupported versions,
/// missing required fields, malformed modeled records, or invalid section boundaries/counts.
#[must_use = "parsing may return an error"]
pub fn parse(source: &str) -> Result<FontMetrics<'_>, ParseError> {
    validate_text(source.as_bytes())?;
    read(source)
}

/// Parse AFM file bytes without a caller-side decoding step or lossy conversion.
/// Names and text borrow the byte slice through a validated ASCII view.
///
/// # Errors
/// Returns the same errors as [`parse`]. Invalid bytes report their zero-based
/// byte offset and one-based source line through [`ParseError::InvalidByte`].
#[must_use = "parsing may return an error"]
pub fn parse_bytes(source: &[u8]) -> Result<FontMetrics<'_>, ParseError> {
    validate_text(source)?;
    let text = std::str::from_utf8(source).map_err(|error| ParseError::InvalidByte {
        offset: error.valid_up_to(),
        line: 1,
        value: source[error.valid_up_to()],
    })?;
    read(text)
}
