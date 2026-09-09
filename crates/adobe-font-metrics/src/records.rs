use std::borrow::Cow;

use crate::{
    BBox, CharacterCode, CharacterMetric, Composite, CompositeComponent, Direction,
    KerningOperands, KerningPair, Ligature, ParseError, RecordContext, SourceRecord, TrackKern,
    Vector,
};

pub(crate) fn malformed(line: usize, keyword: &'static str, reason: &'static str) -> ParseError {
    ParseError::MalformedRecord {
        line,
        keyword,
        reason,
    }
}

pub(crate) fn keyword(line: &str) -> (&str, &str) {
    let line = line.trim();
    line.find(|c: char| c.is_ascii_whitespace())
        .map_or((line, ""), |i| (&line[..i], line[i..].trim()))
}

pub(crate) fn number(s: &str, field: &'static str, line: usize) -> Result<f32, ParseError> {
    let value = s.trim();
    let result = value.parse::<f32>().ok().filter(|value| value.is_finite());
    result.ok_or_else(|| ParseError::InvalidNumber {
        line,
        field,
        value: value.to_owned(),
    })
}

pub(crate) fn integer<T: std::str::FromStr>(
    s: &str,
    field: &'static str,
    line: usize,
) -> Result<T, ParseError> {
    s.trim()
        .parse()
        .map_err(|_error| ParseError::InvalidNumber {
            line,
            field,
            value: s.trim().to_owned(),
        })
}

pub(crate) fn boolean(s: &str, field: &'static str, line: usize) -> Result<bool, ParseError> {
    match s.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(malformed(line, field, "expected `true` or `false`")),
    }
}

pub(crate) fn operands<'a, const N: usize>(
    s: &'a str,
    field: &'static str,
    line: usize,
) -> Result<[&'a str; N], ParseError> {
    let mut tokens = s.split_ascii_whitespace();
    let mut values = [""; N];
    for value in &mut values {
        *value = tokens
            .next()
            .ok_or_else(|| malformed(line, field, "missing operand"))?;
    }
    if tokens.next().is_some() {
        return Err(malformed(line, field, "too many operands"));
    }
    Ok(values)
}

pub(crate) fn vector(s: &str, field: &'static str, line: usize) -> Result<Vector, ParseError> {
    let [x, y] = operands(s, field, line)?;
    Ok(Vector {
        x: number(x, field, line)?,
        y: number(y, field, line)?,
    })
}

pub(crate) fn bbox(s: &str, field: &'static str, line: usize) -> Result<BBox, ParseError> {
    let [llx, lly, urx, ury] = operands(s, field, line)?;
    Ok(BBox {
        llx: number(llx, field, line)?,
        lly: number(lly, field, line)?,
        urx: number(urx, field, line)?,
        ury: number(ury, field, line)?,
    })
}

fn hex<'a>(s: &'a str, field: &'static str, line: usize) -> Result<Cow<'a, str>, ParseError> {
    let digits = s.strip_prefix('<').and_then(|s| s.strip_suffix('>'));
    match digits {
        Some(digits) if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_hexdigit()) => {
            Ok(Cow::Borrowed(digits))
        }
        _ => Err(ParseError::InvalidNumber {
            line,
            field,
            value: s.to_owned(),
        }),
    }
}

pub(crate) fn source_record<'a>(
    records: &mut Vec<SourceRecord<'a>>,
    line: usize,
    context: RecordContext,
    key: &'a str,
    value: &'a str,
) {
    records.push(SourceRecord {
        line,
        context,
        keyword: Cow::Borrowed(key),
        value: Cow::Borrowed(value),
    });
}

pub(crate) fn character<'a>(
    s: &'a str,
    line: usize,
    index: usize,
    records: &mut Vec<SourceRecord<'a>>,
) -> Result<CharacterMetric<'a>, ParseError> {
    let mut code = None;
    let mut name = None;
    let mut advances = [None; 2];
    let mut bounds = None;
    let mut v_vector = None;
    let mut ligatures = Vec::new();
    for segment in s.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (key, value) = keyword(segment);
        match key {
            "C" => {
                let value: i32 = integer(value, "C", line)?;
                if value < -1 {
                    return Err(malformed(
                        line,
                        "C",
                        "character code must be -1 or nonnegative",
                    ));
                }
                if code.replace(CharacterCode::Decimal(value)).is_some() {
                    return Err(malformed(line, "C", "duplicate character code"));
                }
            }
            "CH" => {
                let value = CharacterCode::Hex(hex(value, "CH", line)?);
                if code.replace(value).is_some() {
                    return Err(malformed(line, "CH", "duplicate character code"));
                }
            }
            "N" => {
                let [value] = operands(value, "N", line)?;
                name = Some(Cow::Borrowed(value));
            }
            "WX" | "W0X" => {
                advances[0] = Some(Vector {
                    x: number(value, "WX", line)?,
                    y: 0.0,
                });
            }
            "WY" | "W0Y" => {
                advances[0] = Some(Vector {
                    x: 0.0,
                    y: number(value, "WY", line)?,
                });
            }
            "W1X" => {
                advances[1] = Some(Vector {
                    x: number(value, "W1X", line)?,
                    y: 0.0,
                });
            }
            "W1Y" => {
                advances[1] = Some(Vector {
                    x: 0.0,
                    y: number(value, "W1Y", line)?,
                });
            }
            "W" | "W0" => advances[0] = Some(vector(value, "W", line)?),
            "W1" => advances[1] = Some(vector(value, "W1", line)?),
            "B" => bounds = Some(bbox(value, "B", line)?),
            "VV" => v_vector = Some(vector(value, "VV", line)?),
            "L" => {
                let [successor, ligature] = operands(value, "L", line)?;
                ligatures.push(Ligature {
                    successor: Cow::Borrowed(successor),
                    ligature: Cow::Borrowed(ligature),
                });
            }
            _ if known_record(key) => {
                return Err(malformed(
                    line,
                    "section",
                    "modeled record in a character record",
                ));
            }
            _ => source_record(records, line, RecordContext::Character(index), key, value),
        }
    }
    Ok(CharacterMetric {
        code: code.ok_or_else(|| malformed(line, "C", "character record requires C or CH"))?,
        name,
        advances,
        bbox: bounds,
        v_vector,
        ligatures: Cow::Owned(ligatures),
    })
}

pub(crate) fn pair<'a>(
    key: &str,
    value: &'a str,
    line: usize,
    direction: Direction,
) -> Result<KerningPair<'a>, ParseError> {
    let (operands, adjustment) = match key {
        "KP" => {
            let [left, right, x, y] = operands(value, "KP", line)?;
            (
                KerningOperands::Names {
                    left: Cow::Borrowed(left),
                    right: Cow::Borrowed(right),
                },
                Vector {
                    x: number(x, "KP", line)?,
                    y: number(y, "KP", line)?,
                },
            )
        }
        "KPH" => {
            let [left, right, x, y] = operands(value, "KPH", line)?;
            (
                KerningOperands::Hex {
                    left: hex(left, "KPH", line)?,
                    right: hex(right, "KPH", line)?,
                },
                Vector {
                    x: number(x, "KPH", line)?,
                    y: number(y, "KPH", line)?,
                },
            )
        }
        "KPX" => {
            let [left, right, x] = operands(value, "KPX", line)?;
            (
                KerningOperands::Names {
                    left: Cow::Borrowed(left),
                    right: Cow::Borrowed(right),
                },
                Vector {
                    x: number(x, "KPX", line)?,
                    y: 0.0,
                },
            )
        }
        "KPY" => {
            let [left, right, y] = operands(value, "KPY", line)?;
            (
                KerningOperands::Names {
                    left: Cow::Borrowed(left),
                    right: Cow::Borrowed(right),
                },
                Vector {
                    x: 0.0,
                    y: number(y, "KPY", line)?,
                },
            )
        }
        _ => return Err(malformed(line, "StartKernPairs", "expected a pair record")),
    };
    Ok(KerningPair {
        operands,
        adjustment,
        direction,
    })
}

pub(crate) fn track(value: &str, line: usize) -> Result<TrackKern, ParseError> {
    let [degree, min_point_size, min_kern, max_point_size, max_kern] =
        operands(value, "TrackKern", line)?;
    let track = TrackKern {
        degree: integer(degree, "TrackKern", line)?,
        min_point_size: number(min_point_size, "TrackKern", line)?,
        min_kern: number(min_kern, "TrackKern", line)?,
        max_point_size: number(max_point_size, "TrackKern", line)?,
        max_kern: number(max_kern, "TrackKern", line)?,
    };
    if track.min_point_size > track.max_point_size {
        return Err(malformed(
            line,
            "TrackKern",
            "minimum point size exceeds maximum",
        ));
    }
    Ok(track)
}

pub(crate) fn composite<'a>(
    s: &'a str,
    line: usize,
    index: usize,
    records: &mut Vec<SourceRecord<'a>>,
) -> Result<Composite<'a>, ParseError> {
    let mut segments = s.split(';').map(str::trim).filter(|s| !s.is_empty());
    let (key, value) = keyword(
        segments
            .next()
            .ok_or_else(|| malformed(line, "CC", "missing composite record"))?,
    );
    if key != "CC" {
        return Err(malformed(line, "CC", "expected composite header"));
    }
    let [name, count] = operands(value, "CC", line)?;
    let count: usize = integer(count, "CC", line)?;
    let mut components = Vec::new();
    for segment in segments {
        let (key, value) = keyword(segment);
        if key == "PCC" {
            let [name, x, y] = operands(value, "PCC", line)?;
            components.push(CompositeComponent {
                name: Cow::Borrowed(name),
                offset: Vector {
                    x: number(x, "PCC", line)?,
                    y: number(y, "PCC", line)?,
                },
            });
        } else if known_record(key) {
            return Err(malformed(
                line,
                "section",
                "modeled record in a composite record",
            ));
        } else {
            source_record(records, line, RecordContext::Composite(index), key, value);
        }
    }
    if components.len() != count {
        return Err(malformed(
            line,
            "CC",
            "component count does not match declaration",
        ));
    }
    Ok(Composite {
        name: Cow::Borrowed(name),
        components: Cow::Owned(components),
    })
}

pub(crate) fn known_record(key: &str) -> bool {
    matches!(
        key,
        "StartFontMetrics"
            | "EndFontMetrics"
            | "StartDirection"
            | "EndDirection"
            | "StartCharMetrics"
            | "EndCharMetrics"
            | "StartKernData"
            | "EndKernData"
            | "StartKernPairs"
            | "StartKernPairs0"
            | "StartKernPairs1"
            | "EndKernPairs"
            | "StartTrackKern"
            | "EndTrackKern"
            | "StartComposites"
            | "EndComposites"
            | "FontName"
            | "FontBBox"
            | "MetricsSets"
            | "FullName"
            | "FamilyName"
            | "Weight"
            | "Version"
            | "Notice"
            | "EncodingScheme"
            | "CharacterSet"
            | "MappingScheme"
            | "EscChar"
            | "Characters"
            | "IsBaseFont"
            | "IsCIDFont"
            | "VVector"
            | "IsFixedV"
            | "CapHeight"
            | "XHeight"
            | "Ascender"
            | "Descender"
            | "StdHW"
            | "StdVW"
            | "UnderlinePosition"
            | "UnderlineThickness"
            | "ItalicAngle"
            | "CharWidth"
            | "IsFixedPitch"
            | "C"
            | "CH"
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
            | "KP"
            | "KPX"
            | "KPY"
            | "KPH"
            | "TrackKern"
            | "CC"
            | "PCC"
    )
}
