use std::borrow::Cow;

/// A point or displacement in AFM units (1/1000 of the font scale).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vector {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
}

/// A font or glyph bounding box, with ordinary `f32` precision limits.
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

/// AFM writing direction, independent of the sign or axis of an advance vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Writing direction 0 (normally horizontal).
    Zero,
    /// Writing direction 1 (normally vertical).
    One,
}

impl Direction {
    /// Index into the two-element direction and advance arrays.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Zero => 0,
            Self::One => 1,
        }
    }
}

/// `MetricsSets` or `StartDirection` selector; 2 applies to both directions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MetricsSets {
    /// Direction 0; the default when `MetricsSets` is absent.
    #[default]
    Zero,
    /// Direction 1.
    One,
    /// Both directions.
    Both,
}

/// Authored metrics for one writing direction. Shared blocks populate both entries.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DirectionMetrics {
    /// Underline displacement in AFM units.
    pub underline_position: Option<f32>,
    /// Underline stroke thickness in AFM units.
    pub underline_thickness: Option<f32>,
    /// Counter-clockwise angle from vertical, in degrees.
    pub italic_angle: Option<f32>,
    /// Global character advance, also used when a character omits its advance.
    pub char_width: Option<Vector>,
    /// Authored fixed-pitch flag, preserving absence for default resolution.
    pub is_fixed_pitch: Option<bool>,
}

impl DirectionMetrics {
    /// Effective fixed-pitch flag; `CharWidth` implies true when the flag is absent.
    #[must_use]
    pub fn fixed_pitch(&self) -> bool {
        self.is_fixed_pitch
            .unwrap_or_else(|| self.char_width.is_some())
    }
}

/// An encoded character number; hexadecimal digits are retained without angle brackets.
/// These values are encoding-specific and are not Unicode scalar values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CharacterCode<'a> {
    /// Decimal `C` value; -1 denotes an unencoded character.
    Decimal(i32),
    /// Hexadecimal `CH` digits, preserving leading zeroes and arbitrary code length.
    Hex(Cow<'a, str>),
}

impl CharacterCode<'_> {
    /// Return an unsigned code when it fits in `u32`; unencoded/overlarge codes return None.
    #[must_use]
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Self::Decimal(code) => u32::try_from(*code).ok(),
            Self::Hex(digits) => u32::from_str_radix(digits, 16).ok(),
        }
    }

    /// Detach the code from its source text.
    #[must_use]
    pub fn into_owned(self) -> CharacterCode<'static> {
        match self {
            Self::Decimal(code) => CharacterCode::Decimal(code),
            Self::Hex(digits) => CharacterCode::Hex(Cow::Owned(digits.into_owned())),
        }
    }
}

/// One `L successor ligature` rule. Every rule on a character is retained in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ligature<'a> {
    /// Name of the following glyph.
    pub successor: Cow<'a, str>,
    /// Name of the resulting ligature glyph.
    pub ligature: Cow<'a, str>,
}

impl Ligature<'_> {
    /// Detach both glyph names from their source.
    #[must_use]
    pub fn into_owned(self) -> Ligature<'static> {
        Ligature {
            successor: Cow::Owned(self.successor.into_owned()),
            ligature: Cow::Owned(self.ligature.into_owned()),
        }
    }
}

/// One character record; missing advances stay absent until resolved against the font.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterMetric<'a> {
    /// Required `C` or `CH` encoding value.
    pub code: CharacterCode<'a>,
    /// Optional glyph name (`N`), without encoding interpretation.
    pub name: Option<Cow<'a, str>>,
    /// Authored advance vectors for directions 0 and 1, respectively.
    pub advances: [Option<Vector>; 2],
    /// Optional glyph bounding box (`B`).
    pub bbox: Option<BBox>,
    /// Per-glyph vertical origin displacement (`VV`).
    pub v_vector: Option<Vector>,
    /// All authored ligature rules.
    pub ligatures: Cow<'a, [Ligature<'a>]>,
}

impl CharacterMetric<'_> {
    /// Detach the code, name, and every ligature from their source.
    #[must_use]
    pub fn into_owned(self) -> CharacterMetric<'static> {
        CharacterMetric {
            code: self.code.into_owned(),
            name: self.name.map(|name| Cow::Owned(name.into_owned())),
            advances: self.advances,
            bbox: self.bbox,
            v_vector: self.v_vector,
            ligatures: Cow::Owned(
                self.ligatures
                    .into_owned()
                    .into_iter()
                    .map(Ligature::into_owned)
                    .collect(),
            ),
        }
    }
}

/// Pair operands retain the distinction between glyph names and encoded hexadecimal values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KerningOperands<'a> {
    /// Named operands used by `KP`, `KPX`, and `KPY`.
    Names {
        /// First glyph name.
        left: Cow<'a, str>,
        /// Second glyph name.
        right: Cow<'a, str>,
    },
    /// `KPH` operands, stored as hexadecimal digits without brackets or encoding conversion.
    Hex {
        /// First encoded character.
        left: Cow<'a, str>,
        /// Second encoded character.
        right: Cow<'a, str>,
    },
}

impl KerningOperands<'_> {
    /// Detach both operands from their source.
    #[must_use]
    pub fn into_owned(self) -> KerningOperands<'static> {
        match self {
            Self::Names { left, right } => KerningOperands::Names {
                left: Cow::Owned(left.into_owned()),
                right: Cow::Owned(right.into_owned()),
            },
            Self::Hex { left, right } => KerningOperands::Hex {
                left: Cow::Owned(left.into_owned()),
                right: Cow::Owned(right.into_owned()),
            },
        }
    }
}

/// A complete pair adjustment in one writing direction.
#[derive(Debug, Clone, PartialEq)]
pub struct KerningPair<'a> {
    /// Named or encoded operands.
    pub operands: KerningOperands<'a>,
    /// Both displacement components. `KPX`/`KPY` supply zero for the other axis.
    pub adjustment: Vector,
    /// Direction of the enclosing pair section.
    pub direction: Direction,
}

impl KerningPair<'_> {
    /// Detach pair operands from their source.
    #[must_use]
    pub fn into_owned(self) -> KerningPair<'static> {
        KerningPair {
            operands: self.operands.into_owned(),
            adjustment: self.adjustment,
            direction: self.direction,
        }
    }
}

/// One track-kerning curve, retained as its degree and endpoint values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackKern {
    /// Relative tracking tightness.
    pub degree: i32,
    /// Minimum point size.
    pub min_point_size: f32,
    /// Adjustment at the minimum point size.
    pub min_kern: f32,
    /// Maximum point size.
    pub max_point_size: f32,
    /// Adjustment at the maximum point size.
    pub max_kern: f32,
}

/// One `PCC` component in a composite definition.
#[derive(Debug, Clone, PartialEq)]
pub struct CompositeComponent<'a> {
    /// Name of the component glyph.
    pub name: Cow<'a, str>,
    /// Component displacement in AFM units.
    pub offset: Vector,
}

impl CompositeComponent<'_> {
    /// Detach the component name from its source.
    #[must_use]
    pub fn into_owned(self) -> CompositeComponent<'static> {
        CompositeComponent {
            name: Cow::Owned(self.name.into_owned()),
            offset: self.offset,
        }
    }
}

/// A `CC` construction recipe; it does not synthesize outlines or character metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct Composite<'a> {
    /// Name of the composite glyph.
    pub name: Cow<'a, str>,
    /// Ordered component names and offsets.
    pub components: Cow<'a, [CompositeComponent<'a>]>,
}

impl Composite<'_> {
    /// Detach the name and every component from their source.
    #[must_use]
    pub fn into_owned(self) -> Composite<'static> {
        Composite {
            name: Cow::Owned(self.name.into_owned()),
            components: Cow::Owned(
                self.components
                    .into_owned()
                    .into_iter()
                    .map(CompositeComponent::into_owned)
                    .collect(),
            ),
        }
    }
}

/// Location of a comment or unrecognized record within the parsed AFM structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordContext {
    /// Global font information (including comments before the header).
    Font,
    /// An explicit writing-direction section.
    Direction(MetricsSets),
    /// Character-metric section outside an individual record.
    CharacterMetrics,
    /// A character record, indexed in `FontMetrics::character_metrics`.
    Character(usize),
    /// A kerning container outside its subsections.
    KernData,
    /// Pair section for the given direction.
    KernPairs(Direction),
    /// Track-kerning section.
    TrackKern,
    /// Composite section outside an individual record.
    Composites,
    /// A composite record, indexed in `FontMetrics::composites`.
    Composite(usize),
}

/// Comment or uninterpreted extension data, ordered by appearance in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRecord<'a> {
    /// One-based source line.
    pub line: usize,
    /// Enclosing section or record.
    pub context: RecordContext,
    /// `Comment` or the unrecognized keyword.
    pub keyword: Cow<'a, str>,
    /// Trimmed operand text; no interpretation or round-trip formatting guarantee.
    pub value: Cow<'a, str>,
}

impl SourceRecord<'_> {
    /// Detach the keyword and operand text from their source.
    #[must_use]
    pub fn into_owned(self) -> SourceRecord<'static> {
        SourceRecord {
            line: self.line,
            context: self.context,
            keyword: Cow::Owned(self.keyword.into_owned()),
            value: Cow::Owned(self.value.into_owned()),
        }
    }
}

/// Parsed AFM metric data. Optional authored fields retain absence; accessors resolve defaults.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FontMetrics<'a> {
    /// AFM format version from the header.
    pub afm_version: Cow<'a, str>,
    /// Required font program name.
    pub font_name: Cow<'a, str>,
    /// Required font bounding box.
    pub font_bbox: BBox,
    /// Declared writing directions; absence implies direction 0.
    pub metrics_sets: Option<MetricsSets>,
    /// Full display name.
    pub full_name: Option<Cow<'a, str>>,
    /// Typeface family.
    pub family_name: Option<Cow<'a, str>>,
    /// Weight description.
    pub weight: Option<Cow<'a, str>>,
    /// Font program version, distinct from the AFM format version.
    pub version: Option<Cow<'a, str>>,
    /// Font notice or copyright text.
    pub notice: Option<Cow<'a, str>>,
    /// Default encoding description.
    pub encoding_scheme: Option<Cow<'a, str>>,
    /// Glyph complement description.
    pub character_set: Option<Cow<'a, str>>,
    /// Encoding mapping selector.
    pub mapping_scheme: Option<u32>,
    /// Escape byte for an escape-mapped font.
    pub esc_char: Option<u8>,
    /// Declared total glyph count, independent of the character-metric section count.
    pub characters: Option<u32>,
    /// Authored base-font flag; absence implies true.
    pub is_base_font: Option<bool>,
    /// Authored CID-keyed font flag.
    pub is_cid_font: Option<bool>,
    /// Global displacement between writing origins.
    pub v_vector: Option<Vector>,
    /// Authored fixed-origin-vector flag.
    pub is_fixed_v: Option<bool>,
    /// Capital height.
    pub cap_height: Option<f32>,
    /// Lowercase x height.
    pub x_height: Option<f32>,
    /// Ascender height.
    pub ascender: Option<f32>,
    /// Descender depth, typically negative.
    pub descender: Option<f32>,
    /// Dominant horizontal stem width.
    pub std_hw: Option<f32>,
    /// Dominant vertical stem width.
    pub std_vw: Option<f32>,
    /// Metrics for directions 0 and 1; shared blocks populate both.
    pub directions: [DirectionMetrics; 2],
    /// All character records in source order.
    pub character_metrics: Cow<'a, [CharacterMetric<'a>]>,
    /// All named/hexadecimal pairs in source order, with direction identity.
    pub kerning_pairs: Cow<'a, [KerningPair<'a>]>,
    /// Track-kerning records in source order.
    pub track_kerns: Cow<'a, [TrackKern]>,
    /// Composite construction recipes in source order.
    pub composites: Cow<'a, [Composite<'a>]>,
    /// Comments and uninterpreted records, including unmodeled multiple-master arrays.
    pub source_records: Cow<'a, [SourceRecord<'a>]>,
}

/// Fully owned AFM data that can outlive the original input.
pub type OwnedFontMetrics = FontMetrics<'static>;

impl FontMetrics<'_> {
    /// Metrics for one writing direction.
    #[must_use]
    pub const fn direction(&self, direction: Direction) -> &DirectionMetrics {
        &self.directions[direction.index()]
    }

    /// Resolve a character advance against the direction's global `CharWidth`.
    /// Returns None if neither was specified; an explicit zero remains Some.
    #[must_use]
    pub fn advance(&self, character: &CharacterMetric<'_>, direction: Direction) -> Option<Vector> {
        character.advances[direction.index()].or_else(|| self.direction(direction).char_width)
    }

    /// Resolve a character's vertical origin vector against the global `VVector`.
    #[must_use]
    pub fn vertical_origin(&self, character: &CharacterMetric<'_>) -> Option<Vector> {
        character.v_vector.or(self.v_vector)
    }

    /// Effective `IsFixedV`, inferred from global `VVector` when absent.
    #[must_use]
    pub fn fixed_v(&self) -> bool {
        self.is_fixed_v.unwrap_or_else(|| self.v_vector.is_some())
    }

    /// Detach all borrowed strings and nested records from the source.
    #[must_use]
    pub fn into_owned(self) -> OwnedFontMetrics {
        FontMetrics {
            afm_version: Cow::Owned(self.afm_version.into_owned()),
            font_name: Cow::Owned(self.font_name.into_owned()),
            font_bbox: self.font_bbox,
            metrics_sets: self.metrics_sets,
            full_name: self.full_name.map(|value| Cow::Owned(value.into_owned())),
            family_name: self.family_name.map(|value| Cow::Owned(value.into_owned())),
            weight: self.weight.map(|value| Cow::Owned(value.into_owned())),
            version: self.version.map(|value| Cow::Owned(value.into_owned())),
            notice: self.notice.map(|value| Cow::Owned(value.into_owned())),
            encoding_scheme: self
                .encoding_scheme
                .map(|value| Cow::Owned(value.into_owned())),
            character_set: self
                .character_set
                .map(|value| Cow::Owned(value.into_owned())),
            mapping_scheme: self.mapping_scheme,
            esc_char: self.esc_char,
            characters: self.characters,
            is_base_font: self.is_base_font,
            is_cid_font: self.is_cid_font,
            v_vector: self.v_vector,
            is_fixed_v: self.is_fixed_v,
            cap_height: self.cap_height,
            x_height: self.x_height,
            ascender: self.ascender,
            descender: self.descender,
            std_hw: self.std_hw,
            std_vw: self.std_vw,
            directions: self.directions,
            character_metrics: Cow::Owned(
                self.character_metrics
                    .into_owned()
                    .into_iter()
                    .map(CharacterMetric::into_owned)
                    .collect(),
            ),
            kerning_pairs: Cow::Owned(
                self.kerning_pairs
                    .into_owned()
                    .into_iter()
                    .map(KerningPair::into_owned)
                    .collect(),
            ),
            track_kerns: Cow::Owned(self.track_kerns.into_owned()),
            composites: Cow::Owned(
                self.composites
                    .into_owned()
                    .into_iter()
                    .map(Composite::into_owned)
                    .collect(),
            ),
            source_records: Cow::Owned(
                self.source_records
                    .into_owned()
                    .into_iter()
                    .map(SourceRecord::into_owned)
                    .collect(),
            ),
        }
    }
}
