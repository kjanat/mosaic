//! Public horizontal-metric boundaries audited against Adobe Tech Note 5004.
//! Inputs here are synthetic; the independently vendored real fonts live in fixtures/.

use std::borrow::Cow;

use adobe_font_metrics::{BBox, CharacterMetric, KerningPair, ParseError, parse};

fn afm(body: &str) -> String {
    format!(
        "StartFontMetrics 4.1\nFontName Scope\nFontBBox -20 -200 1000 900\n{body}EndFontMetrics\n"
    )
}

#[test]
fn horizontal_width_aliases_discard_other_axes_and_ligatures() {
    for width in ["WX 625.5", "W0X 625.5", "W 625.5 17", "W0 625.5 17"] {
        let src = afm(&format!(
            "StartCharMetrics 1\n\
             C 65 ; {width} ; W1X 123 ; W1Y -900 ; W1 123 -900 ; VV 12 34 ; L V AV ;\n\
             EndCharMetrics\n"
        ));
        let metrics = parse(&src).expect("synthetic AFM should parse");
        assert_eq!(
            metrics.character_metrics.as_ref(),
            &[CharacterMetric {
                code: 65,
                name: Cow::Borrowed(""),
                width_x: 625.5,
                bbox: None,
            }],
            "{width}"
        );
    }
}

#[test]
fn vertical_only_widths_leave_horizontal_advance_zero() {
    for width in ["WY 500", "W0Y 500", "W1X 700", "W1Y -1000", "W1 700 -1000"] {
        let src = afm(&format!(
            "StartCharMetrics 1\nC -1 ; {width} ;\nEndCharMetrics\n"
        ));
        let metrics = parse(&src).expect("synthetic AFM should parse");
        assert_eq!(
            metrics.character_metrics.as_ref(),
            &[CharacterMetric {
                code: -1,
                name: Cow::Borrowed(""),
                width_x: 0.0,
                bbox: None,
            }],
            "{width}"
        );
    }
}

#[test]
fn global_char_width_does_not_supply_defaults() {
    let src = afm("CharWidth 600 0\nStartCharMetrics 1\nC 65 ; N A ;\nEndCharMetrics\n");
    let metrics = parse(&src).expect("synthetic AFM should parse");
    assert!(!metrics.is_fixed_pitch);
    assert_eq!(metrics.full_name, "");
    assert_eq!(
        metrics.character_metrics.as_ref(),
        &[CharacterMetric {
            code: 65,
            name: Cow::Borrowed("A"),
            width_x: 0.0,
            bbox: None,
        }]
    );
}

#[test]
fn direction_zero_and_shared_metrics_use_the_flat_fields() {
    for (sets, direction) in [(0, 0), (2, 2)] {
        let src = afm(&format!(
            "MetricsSets {sets}\n\
             VVector 0 500\n\
             StartDirection {direction}\n\
             UnderlinePosition -100\n\
             UnderlineThickness 50\n\
             ItalicAngle -12\n\
             IsFixedPitch true\n\
             EndDirection\n"
        ));
        let equivalent = afm(
            "UnderlinePosition -100\nUnderlineThickness 50\nItalicAngle -12\nIsFixedPitch true\n",
        );
        assert_eq!(
            parse(&src).expect("synthetic AFM should parse"),
            parse(&equivalent).expect("equivalent metrics should parse"),
            "direction {direction}"
        );
    }
}

#[test]
fn named_kerning_retains_only_horizontal_adjustments() {
    for section in ["StartKernPairs", "StartKernPairs0"] {
        let src = afm(&format!(
            "StartKernData\n\
             {section} 4\n\
             KPX A V -80\n\
             KP T o -40 15\n\
             KPY W a -30\n\
             KPH <41> <56> -99 10\n\
             EndKernPairs\n\
             EndKernData\n"
        ));
        let metrics = parse(&src).expect("synthetic AFM should parse");
        let expected =
            [("A", "V", -80.0), ("T", "o", -40.0), ("W", "a", 0.0)].map(|(left, right, adjust)| {
                KerningPair {
                    left: Cow::Borrowed(left),
                    right: Cow::Borrowed(right),
                    adjust,
                }
            });
        assert_eq!(metrics.kerning_pairs.as_ref(), &expected, "{section}");
        for pair in metrics.kerning_pairs.iter() {
            assert!(matches!(pair.left, Cow::Borrowed(_)));
            assert!(matches!(pair.right, Cow::Borrowed(_)));
        }
    }
}

#[test]
fn discarded_metadata_tracks_and_composites_leave_metrics_unchanged() {
    let src = afm("Version 1.0\n\
         Notice Synthetic audit input\n\
         Comment Discard this comment\n\
         CharacterSet Synthetic\n\
         Characters 0\n\
         IsBaseFont true\n\
         StdHW 50\n\
         StdVW 60\n\
         customExtension data\n\
         StartKernData\n\
         StartTrackKern 1\n\
         TrackKern -1 8 -0.5 72 -3\n\
         EndTrackKern\n\
         EndKernData\n\
         StartComposites 1\n\
         CC Aacute 2 ; PCC A 0 0 ; PCC acute 160 170 ;\n\
         EndComposites\n");
    assert_eq!(
        parse(&src).expect("synthetic AFM should parse"),
        parse(&afm("")).expect("minimal AFM should parse")
    );
}

#[test]
fn fractional_character_box_and_borrowed_name_survive_owned_conversion() {
    let src = afm(
        "StartCharMetrics 1\nCH <8000> ; WX 500 ; N sample ; B -1.5 0 499.25 700.5 ;\nEndCharMetrics\n",
    );
    let metrics = parse(&src).expect("synthetic AFM should parse");
    assert_eq!(
        metrics.character_metrics.as_ref(),
        &[CharacterMetric {
            code: 0x8000,
            name: Cow::Borrowed("sample"),
            width_x: 500.0,
            bbox: Some(BBox {
                llx: -1.5,
                lly: 0.0,
                urx: 499.25,
                ury: 700.5,
            }),
        }]
    );
    assert!(matches!(
        metrics.character_metrics[0].name,
        Cow::Borrowed(_)
    ));
    let owned = metrics.into_owned();
    drop(src);
    assert_eq!(owned.character_metrics[0].name, "sample");
}

#[test]
fn afm_v3_and_related_container_formats_are_rejected() {
    let v3 = "StartFontMetrics 3.0\nFontName Scope\nFontBBox 0 0 1000 1000\nEndFontMetrics\n";
    assert!(matches!(
        parse(v3),
        Err(ParseError::UnsupportedVersion { line: 1, version }) if version == "3.0"
    ));
    for header in ["StartCompFontMetrics 4.1", "StartMasterFontMetrics 4.1"] {
        assert!(matches!(
            parse(header),
            Err(ParseError::MissingHeader { line: 1 })
        ));
    }
}
