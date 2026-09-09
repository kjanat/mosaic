#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test assertions")]

use std::path::Path;

use mos_core::{AttrValue, NodeKind};
use serde_json::{Value, json};

use super::code_block_hints;
use crate::diagnostics::{LspPosition, LspRange, byte_to_position};

fn whole_document() -> LspRange {
    LspRange {
        start: LspPosition {
            line: 0,
            character: 0,
        },
        end: LspPosition {
            line: u32::MAX,
            character: u32::MAX,
        },
    }
}

fn hints(src: &str) -> Vec<Value> {
    let lowered = mos_eval::lower(src, Path::new("main.mos"));
    code_block_hints(&lowered.document, src, whole_document())
}

#[test]
fn generated_name_is_readable_and_display_only() {
    let src = "#code(lang: \"rust\")[[\nfn main() {}\n]]";
    let lowered = mos_eval::lower(src, Path::new("main.mos"));
    assert!(!lowered.has_errors());
    let hints = code_block_hints(&lowered.document, src, whole_document());
    assert_eq!(hints.len(), 1);
    let hint = &hints[0];
    let label = hint["label"].as_str().unwrap();
    assert!(label.starts_with("rust: fn main() {} ["), "{label}");
    assert_eq!(hint["position"], json!({ "line": 2, "character": 2 }));
    assert_eq!(hint["paddingLeft"], true);
    assert!(hint["tooltip"].as_str().unwrap().contains("<label>"));
    for field in ["textEdits", "command", "data", "kind"] {
        assert!(hint.get(field).is_none(), "unexpected {field}");
    }
    assert!(
        lowered
            .document
            .nodes()
            .all(|node| !node.attributes.contains_key("label"))
    );
}

#[test]
fn manual_labels_pre_inline_code_and_comments_do_not_get_hints() {
    let src = "#code(lang: \"rust\")[[\nfn main() {}\n]] <ex:main>\n\n@ex:main\n\n#pre[[raw]]\n\n\u{60}#code[[inline]]\u{60}\n\n// #code[[line comment]]\n/*\n#code[[block comment]]\n*/\n";
    let lowered = mos_eval::lower(src, Path::new("main.mos"));
    assert!(!lowered.has_errors(), "{:?}", lowered.diagnostics);
    assert!(code_block_hints(&lowered.document, src, whole_document()).is_empty());
    let raw = lowered
        .document
        .nodes()
        .find(|node| node.kind == NodeKind::Raw && node.attributes.contains_key("label"))
        .unwrap();
    assert_eq!(raw.attributes["label"], AttrValue::Str("ex:main".into()));
}

#[test]
fn incomplete_raw_blocks_have_no_closing_hint() {
    for src in [
        "#code",
        "#code(lang: \"rust\")[[\nfn main() {}",
        "#code[=[wrong close]]",
    ] {
        assert!(hints(src).is_empty(), "{src}");
    }
}

#[test]
fn language_and_empty_body_fallbacks_are_nonempty() {
    for src in [
        "#code[[]]",
        "#code(lang: 42)[[]]",
        "#code(lang: \"  \")[[]]",
    ] {
        let items = hints(src);
        assert_eq!(items.len(), 1, "{src}");
        assert!(
            items[0]["label"]
                .as_str()
                .unwrap()
                .starts_with("code: empty block [")
        );
    }
    assert!(
        hints("#code(lang: python)[[print(1)]]")[0]["label"]
            .as_str()
            .unwrap()
            .starts_with("python: print(1) [")
    );
}

#[test]
fn names_survive_relocation_and_unrelated_insertions() {
    let src = "#code(lang: \"rust\")[[\nfn main() {}\n]]";
    let original = hints(src)[0]["label"].clone();
    let moved = format!("= New heading\n\n#code(lang: python)[[print(1)]]\n\n{src}");
    assert_eq!(hints(&moved)[1]["label"], original);
    let relocated = mos_eval::lower(src, Path::new("elsewhere/main.mos"));
    assert_eq!(
        code_block_hints(&relocated.document, src, whole_document())[0]["label"],
        original
    );
    assert_eq!(hints(&src.replace('\n', "\r\n"))[0]["label"], original);
    assert_ne!(hints(&src.replace("rust", "python"))[0]["label"], original);
    assert_ne!(
        hints(&src.replace("fn main() {}", "fn main() {}\n// changed body"))[0]["label"],
        original
    );
}

#[test]
fn duplicates_are_disambiguated_before_viewport_filtering() {
    let src = "#code[[same]]\n#code[[same]]\n";
    let lowered = mos_eval::lower(src, Path::new("main.mos"));
    let all = code_block_hints(&lowered.document, src, whole_document());
    assert_eq!(all.len(), 2);
    assert_eq!(
        all[1]["label"],
        format!("{} #2", all[0]["label"].as_str().unwrap())
    );
    let visible = code_block_hints(
        &lowered.document,
        src,
        LspRange {
            start: LspPosition {
                line: 1,
                character: 0,
            },
            end: LspPosition {
                line: 1,
                character: 20,
            },
        },
    );
    assert_eq!(visible, vec![all[1].clone()]);
}

#[test]
fn closing_position_handles_long_delimiters_crlf_and_utf16() {
    let src = "= 😀\r\n\r\n#code(lang: \"rust\")[==[😀 ]] [=[nested]=]]==]   \r\n";
    let items = hints(src);
    assert_eq!(items.len(), 1);
    let close_end = src.find("]==]").unwrap() + 4;
    let line_start = src.find("#code").unwrap();
    assert_eq!(
        items[0]["position"],
        json!({
            "line": 2,
            "character": src[line_start..close_end].encode_utf16().count(),
        })
    );
}

#[test]
fn viewport_is_based_on_closing_position_including_eof_boundary() {
    let src = "#code[[\nbody\n]]";
    let lowered = mos_eval::lower(src, Path::new("main.mos"));
    let close = byte_to_position(src, src.len());
    let at_close = LspRange {
        start: close,
        end: close,
    };
    assert_eq!(code_block_hints(&lowered.document, src, at_close).len(), 1);
    let body_only = LspRange {
        start: LspPosition {
            line: 0,
            character: 0,
        },
        end: LspPosition {
            line: 1,
            character: 4,
        },
    };
    assert!(code_block_hints(&lowered.document, src, body_only).is_empty());
    assert!(
        code_block_hints(
            &lowered.document,
            src,
            LspRange {
                start: close,
                end: body_only.start
            }
        )
        .is_empty()
    );
}

#[test]
fn previews_are_bounded_unicode_text_without_control_characters() {
    let src = format!(
        "#code(lang: \"a very long language name\")[[\n\t{}\n]]",
        "😀".repeat(60)
    );
    let items = hints(&src);
    let label = items[0]["label"].as_str().unwrap();
    assert!(label.starts_with("a very long lang…: "));
    assert!(label.contains(&format!("{}… [", "😀".repeat(40))));
    assert!(!label.chars().any(char::is_control));
}
