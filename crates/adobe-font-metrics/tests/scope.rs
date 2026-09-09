//! AFM API coverage beyond the Core-14 horizontal-metric consumer.

use std::borrow::Cow;

use adobe_font_metrics::{
    CharacterCode, Direction, KerningOperands, MetricsSets, ParseError, RecordContext, Vector,
    parse, parse_bytes,
};

const EXTENDED: &str = include_str!("fixtures/Extended.afm");

fn afm(body: &str) -> String {
    format!(
        "StartFontMetrics 4.1\nFontName Scope\nFontBBox -20 -200 1000 900\n{body}EndFontMetrics\n"
    )
}

#[test]
fn retains_global_metadata_and_authored_absence() {
    let font = parse(EXTENDED).expect("extended fixture");
    assert_eq!(font.afm_version, "4.1");
    assert_eq!(font.font_name, "Extended");
    assert_eq!(font.full_name.as_deref(), Some("Extended Metrics"));
    assert_eq!(font.family_name.as_deref(), Some("Extended Family"));
    assert_eq!(font.weight.as_deref(), Some("Medium"));
    assert_eq!(font.version.as_deref(), Some("2.7"));
    assert_eq!(font.notice.as_deref(), Some("Synthetic font metadata"));
    assert_eq!(font.encoding_scheme.as_deref(), Some("FontSpecific"));
    assert_eq!(font.character_set.as_deref(), Some("ExampleSet"));
    assert_eq!(font.metrics_sets, Some(MetricsSets::Both));
    assert_eq!(font.mapping_scheme, Some(3));
    assert_eq!(font.esc_char, Some(27));
    assert_eq!(font.characters, Some(3));
    assert_eq!(font.is_base_font, Some(false));
    assert_eq!(font.is_cid_font, Some(false));
    assert_eq!(font.is_fixed_v, Some(true));
    assert_eq!(font.cap_height, Some(700.0));
    assert_eq!(font.x_height, Some(450.0));
    assert_eq!(font.ascender, Some(720.0));
    assert_eq!(font.descender, Some(-180.0));
    assert_eq!(font.std_hw, Some(50.0));
    assert_eq!(font.std_vw, Some(60.0));
    assert_eq!(font.font_bbox.llx, -20.5);
    assert!(matches!(font.notice, Some(Cow::Borrowed(_))));

    let source = afm("");
    let minimal = parse(&source).expect("minimal fixture");
    assert!(minimal.full_name.is_none());
    assert!(minimal.cap_height.is_none());
    assert!(minimal.metrics_sets.is_none());
    assert!(minimal.is_base_font.is_none());
    assert!(!minimal.fixed_v());
}

#[test]
fn shared_metrics_and_both_direction_defaults_are_available() {
    let font = parse(EXTENDED).expect("extended fixture");
    let glyph = &font.character_metrics[1];
    assert_eq!(glyph.advances, [None, None]);
    for (direction, advance, position) in [
        (Direction::Zero, Vector { x: 600.0, y: 0.0 }, -100.0),
        (Direction::One, Vector { x: 0.0, y: -1000.0 }, -40.0),
    ] {
        assert_eq!(font.advance(glyph, direction), Some(advance));
        let metrics = font.direction(direction);
        assert_eq!(metrics.underline_position, Some(position));
        assert_eq!(metrics.underline_thickness, Some(40.0));
        assert_eq!(metrics.italic_angle, Some(-12.0));
        assert!(metrics.fixed_pitch());
    }
    assert_eq!(
        font.vertical_origin(glyph),
        Some(Vector { x: 300.0, y: 800.0 })
    );
    assert!(font.fixed_v());
}

#[test]
fn global_width_infers_fixed_pitch_without_overwriting_authored_values() {
    let source = afm(
        "CharWidth 600 0\nStartCharMetrics 2\nC 65 ; N A ;\nC 66 ; WX 0 ; N B ;\nEndCharMetrics\n",
    );
    let font = parse(&source).expect("global width");
    assert_eq!(font.direction(Direction::Zero).is_fixed_pitch, None);
    assert!(font.direction(Direction::Zero).fixed_pitch());
    assert_eq!(
        font.advance(&font.character_metrics[0], Direction::Zero),
        Some(Vector { x: 600.0, y: 0.0 })
    );
    assert_eq!(
        font.advance(&font.character_metrics[1], Direction::Zero),
        Some(Vector { x: 0.0, y: 0.0 })
    );
    assert_eq!(
        font.advance(&font.character_metrics[0], Direction::One),
        None
    );
}

#[test]
fn scalar_and_vector_advances_preserve_their_axes() {
    for (field, index, expected) in [
        ("WX 625.5", 0, Vector { x: 625.5, y: 0.0 }),
        ("W0X 625.5", 0, Vector { x: 625.5, y: 0.0 }),
        ("WY 625.5", 0, Vector { x: 0.0, y: 625.5 }),
        ("W0Y 625.5", 0, Vector { x: 0.0, y: 625.5 }),
        ("W1X 625.5", 1, Vector { x: 625.5, y: 0.0 }),
        ("W1Y -625.5", 1, Vector { x: 0.0, y: -625.5 }),
        ("W 625.5 17", 0, Vector { x: 625.5, y: 17.0 }),
        ("W0 625.5 17", 0, Vector { x: 625.5, y: 17.0 }),
        ("W1 17 -625.5", 1, Vector { x: 17.0, y: -625.5 }),
    ] {
        let source = afm(&format!(
            "StartCharMetrics 1\nN glyph ; {field} ; C -1 ;\nEndCharMetrics\n"
        ));
        let font = parse(&source).expect("advance alias");
        let glyph = &font.character_metrics[0];
        assert_eq!(glyph.advances[index], Some(expected), "{field}");
        assert_eq!(glyph.advances[1 - index], None, "{field}");
    }
}

#[test]
fn retains_ligatures_origins_and_encoded_character_identity() {
    let font = parse(EXTENDED).expect("extended fixture");
    let glyph = &font.character_metrics[0];
    assert_eq!(glyph.name.as_deref(), Some("A"));
    assert_eq!(glyph.bbox.map(|bbox| bbox.urx), Some(599.25));
    assert_eq!(glyph.v_vector, Some(Vector { x: 300.0, y: 800.0 }));
    let ligatures: Vec<_> = glyph
        .ligatures
        .iter()
        .map(|rule| (rule.successor.as_ref(), rule.ligature.as_ref()))
        .collect();
    assert_eq!(ligatures, [("V", "AV"), ("A", "AA")]);
    assert!(matches!(glyph.ligatures[0].ligature, Cow::Borrowed(_)));
    assert_eq!(
        font.character_metrics[1].code,
        CharacterCode::Hex(Cow::Borrowed("008000"))
    );
    assert_eq!(font.character_metrics[1].code.as_u32(), Some(0x8000));
    assert_eq!(font.character_metrics[2].code, CharacterCode::Decimal(-1));
    assert_eq!(font.character_metrics[2].code.as_u32(), None);

    let source = afm("StartCharMetrics 1\nCH <0123456789abcdef> ; VV 10 20 ;\nEndCharMetrics\n");
    let font = parse(&source).expect("long hexadecimal code");
    assert_eq!(font.character_metrics[0].code.as_u32(), None);
    assert_eq!(
        font.character_metrics[0].code,
        CharacterCode::Hex(Cow::Borrowed("0123456789abcdef"))
    );
    assert_eq!(
        font.vertical_origin(&font.character_metrics[0]),
        Some(Vector { x: 10.0, y: 20.0 })
    );
}

#[test]
fn pair_and_track_kerning_preserve_complete_values() {
    let font = parse(EXTENDED).expect("extended fixture");
    assert_eq!(font.kerning_pairs.len(), 5);
    assert_eq!(
        font.kerning_pairs[0].adjustment,
        Vector { x: -80.0, y: 20.0 }
    );
    assert_eq!(
        font.kerning_pairs[1].adjustment,
        Vector { x: 0.0, y: -35.0 }
    );
    assert_eq!(
        font.kerning_pairs[2].adjustment,
        Vector { x: -10.0, y: 11.0 }
    );
    assert_eq!(
        font.kerning_pairs[2].operands,
        KerningOperands::Hex {
            left: Cow::Borrowed("0041"),
            right: Cow::Borrowed("56")
        }
    );
    assert_eq!(font.kerning_pairs[3].direction, Direction::One);
    assert_eq!(
        font.kerning_pairs[3].adjustment,
        Vector { x: 30.0, y: -100.0 }
    );
    assert_eq!(font.kerning_pairs[4].adjustment, Vector { x: 15.0, y: 0.0 });
    for pair in &font.kerning_pairs[..3] {
        assert_eq!(pair.direction, Direction::Zero);
    }
    assert_eq!(font.track_kerns.len(), 2);
    let track = font.track_kerns[0];
    assert_eq!(track.degree, -1);
    assert_eq!(
        (
            track.min_point_size,
            track.min_kern,
            track.max_point_size,
            track.max_kern
        ),
        (8.0, -0.5, 72.0, -3.0)
    );
    assert_eq!(font.track_kerns[1].degree, 1);
}

#[test]
fn unnumbered_pair_sections_are_direction_zero() {
    for section in ["StartKernPairs", "StartKernPairs0"] {
        let source = afm(&format!("{section} 1\nKPX A V -80\nEndKernPairs\n"));
        let font = parse(&source).expect("standalone pairs");
        assert_eq!(font.kerning_pairs[0].direction, Direction::Zero);
        assert_eq!(
            font.kerning_pairs[0].adjustment,
            Vector { x: -80.0, y: 0.0 }
        );
    }
}

#[test]
fn composites_keep_ordered_components_and_fractional_offsets() {
    let font = parse(EXTENDED).expect("extended fixture");
    assert_eq!(font.composites.len(), 1);
    let composite = &font.composites[0];
    assert_eq!(composite.name, "Aacute");
    assert_eq!(composite.components.len(), 2);
    assert_eq!(composite.components[0].name, "A");
    assert_eq!(composite.components[0].offset, Vector { x: 0.0, y: 0.0 });
    assert_eq!(composite.components[1].name, "acute");
    assert_eq!(
        composite.components[1].offset,
        Vector {
            x: 160.5,
            y: 170.25
        }
    );
    assert_eq!(font.character_metrics[2].name.as_deref(), Some("Aacute"));
}

#[test]
fn comments_and_extensions_retain_source_context() {
    let font = parse(EXTENDED).expect("extended fixture");
    let records: Vec<_> = font
        .source_records
        .iter()
        .map(|r| (r.keyword.as_ref(), r.context))
        .collect();
    assert_eq!(
        records,
        [
            ("Comment", RecordContext::Font),
            ("BlendAxisTypes", RecordContext::Font),
            ("customFontData", RecordContext::Font),
            (
                "customDirectionData",
                RecordContext::Direction(MetricsSets::One)
            ),
            ("customGlyphData", RecordContext::Character(0)),
        ]
    );
    assert_eq!(font.source_records[0].line, 2);
    assert_eq!(font.source_records[1].value, "[/Weight /Width]");
    for record in font.source_records.iter() {
        assert!(matches!(record.value, Cow::Borrowed(_)));
    }
}

#[test]
fn owned_conversion_detaches_every_nested_record() {
    let bytes = EXTENDED.as_bytes().to_vec();
    let font = parse_bytes(&bytes).expect("byte input");
    assert!(matches!(font.font_name, Cow::Borrowed(_)));
    let owned = font.into_owned();
    drop(bytes);
    let expected = parse(EXTENDED).expect("static fixture").into_owned();
    assert_eq!(owned, expected);
    assert!(matches!(
        owned.character_metrics[0].ligatures[0].successor,
        Cow::Owned(_)
    ));
    assert!(matches!(
        owned.character_metrics[1].code,
        CharacterCode::Hex(Cow::Owned(_))
    ));
    assert!(matches!(
        owned.kerning_pairs[2].operands,
        KerningOperands::Hex {
            left: Cow::Owned(_),
            right: Cow::Owned(_)
        }
    ));
    assert!(matches!(
        owned.composites[0].components[1].name,
        Cow::Owned(_)
    ));
    assert!(matches!(owned.source_records[1].value, Cow::Owned(_)));
}

#[test]
fn byte_api_handles_crlf_and_reports_invalid_byte_locations() {
    let crlf = EXTENDED.replace('\n', "\r\n");
    assert_eq!(
        parse_bytes(crlf.as_bytes()).expect("CRLF").into_owned(),
        parse(EXTENDED).expect("LF").into_owned()
    );
    let prefix = b"StartFontMetrics 4.1\n";
    for value in [0, 0x0b, 0x7f, 0x80, 0xff] {
        let mut bytes = prefix.to_vec();
        bytes.push(value);
        assert!(
            matches!(parse_bytes(&bytes), Err(ParseError::InvalidByte { offset, line: 2, value: actual }) if offset == prefix.len() && actual == value)
        );
    }
    assert!(matches!(
        parse("StartFontMetrics 4.1\nNotice Caf\u{e9}"),
        Err(ParseError::InvalidByte { line: 2, .. })
    ));
}

#[test]
fn all_proper_truncations_of_extended_fixture_fail() {
    let source = EXTENDED.trim_end();
    for (offset, _) in source.match_indices('\n') {
        assert!(
            parse(&source[..offset]).is_err(),
            "accepted truncation at byte {offset}"
        );
    }
    assert!(parse(source).is_ok());
}

#[test]
fn rejects_malformed_newly_modeled_records() {
    for body in [
        "CharWidth 600\n",
        "CharWidth 600 0 extra\n",
        "StdHW nope\n",
        "IsCIDFont maybe\n",
        "MetricsSets 3\n",
        "EscChar 256\n",
        "Ascender NaN\n",
        "FontBBox 0 0 inf 900\n",
        "StartCharMetrics 1\nC 65 ; W 500 ;\nEndCharMetrics\n",
        "StartCharMetrics 1\nC 65 ; W1 500 nope ;\nEndCharMetrics\n",
        "StartCharMetrics 1\nC 65 ; VV 1 2 3 ;\nEndCharMetrics\n",
        "StartCharMetrics 1\nC 65 ; L i ;\nEndCharMetrics\n",
        "StartCharMetrics 1\nCH 41 ;\nEndCharMetrics\n",
        "StartCharMetrics 1\nCH <41 ;\nEndCharMetrics\n",
        "StartCharMetrics 1\nC -2 ;\nEndCharMetrics\n",
        "StartCharMetrics 1\nC 1 ; CH <01> ;\nEndCharMetrics\n",
        "StartKernPairs1 1\nKPY A V nope\nEndKernPairs\n",
        "StartKernPairs 1\nKPH <41> <GG> 1 2\nEndKernPairs\n",
        "StartTrackKern 1\nTrackKern 1 72 0 8 0\nEndTrackKern\n",
        "StartTrackKern 1\nTrackKern 1 8 0 72\nEndTrackKern\n",
        "StartComposites 1\nCC Test 2 ; PCC A 1 2 ;\nEndComposites\n",
        "StartComposites 1\nCC Test 1 ; PCC A nope 2 ;\nEndComposites\n",
    ] {
        assert!(parse(&afm(body)).is_err(), "accepted {body:?}");
    }
}

#[test]
fn rejects_count_mismatches_and_incompatible_sections() {
    for body in [
        "C 65 ;\n",
        "KPX A V 10\n",
        "CC Aacute 0 ;\n",
        "StartCharMetrics 1\nC 65 ; KPX A V 10 ;\nEndCharMetrics\n",
        "StartCharMetrics 1\nC 65 ; EndFontMetrics ;\nEndCharMetrics\n",
        "StartComposites 1\nCC Aacute 0 ; WX 600 ;\nEndComposites\n",
        "StartCharMetrics 0\nC 65 ;\nEndCharMetrics\n",
        "StartCharMetrics 2\nC 65 ;\nEndCharMetrics\n",
        "StartKernPairs1 2\nKP A V 1 2\nEndKernPairs\n",
        "StartTrackKern 1\nEndTrackKern\n",
        "StartComposites 1\nEndComposites\n",
        "StartDirection 0\nStartDirection 1\nEndDirection\n",
        "EndDirection\n",
        "EndKernPairs\n",
        "StartCharMetrics 0\nFontName WrongSection\nEndCharMetrics\n",
        "StartKernData\nKPX A V 10\nEndKernData\n",
        "StartKernData\nStartComposites 0\nEndComposites\nEndKernData\n",
        "StartKernPairs 0\nStartTrackKern 0\nEndTrackKern\nEndKernPairs\n",
    ] {
        assert!(parse(&afm(body)).is_err(), "accepted {body:?}");
    }
    let trailing = format!("{}FontName trailing\n", afm(""));
    assert!(parse(&trailing).is_err());
}

#[test]
fn rejects_inconsistent_global_defaults() {
    for body in [
        "MappingScheme 3\n",
        "CharWidth 600 0\nIsFixedPitch false\n",
        "IsFixedPitch false\nCharWidth 600 0\n",
        "VVector 0 100\nIsFixedV false\n",
    ] {
        assert!(parse(&afm(body)).is_err(), "accepted {body:?}");
    }
}

#[test]
fn errors_in_new_fields_retain_the_record_line() {
    let error = parse(&afm(
        "StartCharMetrics 1\nC 65 ; W1 0 invalid ;\nEndCharMetrics\n",
    ))
    .expect_err("invalid W1");
    assert!(matches!(
        error,
        ParseError::InvalidNumber {
            line: 5,
            field: "W1",
            ..
        }
    ));
    let error = parse(&afm(
        "StartComposites 1\nCC Aacute 1 ; PCC acute 1 nope ;\nEndComposites\n",
    ))
    .expect_err("invalid PCC");
    assert!(matches!(
        error,
        ParseError::InvalidNumber {
            line: 5,
            field: "PCC",
            ..
        }
    ));
}

#[test]
fn afm_v3_and_related_container_formats_are_rejected() {
    assert!(matches!(
        parse("StartFontMetrics 3.0\n"),
        Err(ParseError::UnsupportedVersion { line: 1, .. })
    ));
    for header in ["StartCompFontMetrics 4.1", "StartMasterFontMetrics 4.1"] {
        assert!(matches!(
            parse(header),
            Err(ParseError::MissingHeader { line: 1 })
        ));
    }
}
