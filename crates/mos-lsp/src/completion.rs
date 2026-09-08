//! `textDocument/completion`: citation keys after `[@`.

use mos_eval::LowerResult;
use mos_parse::scan_label_chars;
use serde_json::{Value, json};

use crate::definition::position_to_byte;
use crate::diagnostics::{LspPosition, LspRange, byte_to_position};

const REFERENCE_ITEM_KIND: u32 = 18;

/// The citation key token the cursor sits in: `[@` followed by label
/// characters, on the cursor's line, with the cursor inside or right after
/// the key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CitationPrefix {
    /// Byte offset of the key's first character (right after `[@`).
    pub start: usize,
    /// Byte offset just past the last label character of the key.
    pub end: usize,
    /// Whether a `]` already follows the key.
    pub closed: bool,
}

/// The citation key token covering `offset`, or `None` when the cursor is
/// not inside a `[@key` token on its line.
#[must_use]
pub fn citation_prefix_at(src: &str, offset: usize) -> Option<CitationPrefix> {
    let line_start = src[..offset].rfind('\n').map_or(0, |newline| newline + 1);
    let opener = src[line_start..offset].rfind("[@")? + line_start;
    let start = opener + 2;
    let end = scan_label_chars(src.as_bytes(), start);
    if end < offset {
        return None;
    }
    Some(CitationPrefix {
        start,
        end,
        closed: src[end..].starts_with(']'),
    })
}

/// One LSP `CompletionItem` per loaded bibliography record when `position`
/// sits inside a `[@key` token; empty otherwise. Each item's edit replaces
/// the whole key and appends the closing `]` when none follows it.
#[must_use]
pub fn citation_completions(lowered: &LowerResult, src: &str, position: LspPosition) -> Vec<Value> {
    let Some(prefix) = citation_prefix_at(src, position_to_byte(src, position)) else {
        return Vec::new();
    };
    let range = LspRange {
        start: byte_to_position(src, prefix.start),
        end: byte_to_position(src, prefix.end),
    };
    lowered
        .bibliography
        .entries
        .iter()
        .map(|(key, entry)| {
            let new_text = if prefix.closed {
                key.clone()
            } else {
                format!("{key}]")
            };
            let mut item = json!({
                "label": key,
                "kind": REFERENCE_ITEM_KIND,
                "detail": entry.entry_type,
                "filterText": key,
                "textEdit": { "range": range, "newText": new_text },
            });
            if let Some(title) = entry.field_text("title") {
                item["documentation"] = json!(title);
            }
            item
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        reason = "tests panic loudly on setup failure; matches crate-wide test-module convention"
    )]

    use std::path::PathBuf;

    use serde_json::{Value, json};

    use super::*;
    use crate::diagnostics::byte_to_position;

    #[test]
    fn prefix_after_an_open_citation_covers_the_typed_key() {
        let src = "Cite [@pat";
        let prefix = citation_prefix_at(src, src.len()).expect("inside a citation");
        assert_eq!(prefix.start, src.find("pat").unwrap());
        assert_eq!(prefix.end, src.len());
        assert!(!prefix.closed);
    }

    #[test]
    fn prefix_with_an_empty_key_is_still_a_citation() {
        let src = "Cite [@";
        let prefix = citation_prefix_at(src, src.len()).expect("inside a citation");
        assert_eq!(prefix.start, src.len());
        assert_eq!(prefix.end, src.len());
    }

    #[test]
    fn prefix_inside_an_existing_key_covers_the_whole_key_and_sees_its_closer() {
        let src = "See [@patashnik1988] here.\n";
        let cursor = src.find("pat").unwrap() + 3;
        let prefix = citation_prefix_at(src, cursor).expect("inside a citation");
        assert_eq!(&src[prefix.start..prefix.end], "patashnik1988");
        assert!(prefix.closed);
    }

    #[test]
    fn cursor_off_any_citation_yields_no_prefix() {
        let plain_reference = "see @pat";
        assert!(citation_prefix_at(plain_reference, plain_reference.len()).is_none());
        let after_closed = "[@a] tail";
        assert!(citation_prefix_at(after_closed, after_closed.len()).is_none());
        let broken_key = "[@a b";
        assert!(citation_prefix_at(broken_key, broken_key.len()).is_none());
        let previous_line = "[@a\nb";
        assert!(citation_prefix_at(previous_line, previous_line.len()).is_none());
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mos-lsp-completion-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn labels(items: &[Value]) -> Vec<&str> {
        items
            .iter()
            .filter_map(|item| item.get("label").and_then(Value::as_str))
            .collect()
    }

    #[test]
    fn completions_offer_every_loaded_key_and_close_the_citation() {
        let dir = unique_temp_dir("keys");
        std::fs::write(
            dir.join("refs.bib"),
            "@book{knuth1984, title={The {TeX}book}}\n@article{beta, year=1}\n",
        )
        .expect("write bib");
        let main = dir.join("main.mos");
        let src = "#bibliography(\"refs.bib\")\n\nCite [@kn";
        let lowered = mos_eval::lower(src, &main);

        let items = citation_completions(&lowered, src, byte_to_position(src, src.len()));

        assert_eq!(labels(&items), ["beta", "knuth1984"]);
        let knuth = &items[1];
        assert_eq!(knuth.get("kind"), Some(&json!(18)));
        assert_eq!(knuth.get("detail"), Some(&json!("book")));
        assert_eq!(knuth.get("documentation"), Some(&json!("The {TeX}book")));
        assert_eq!(
            knuth.pointer("/textEdit/newText"),
            Some(&json!("knuth1984]"))
        );
        assert_eq!(
            knuth.pointer("/textEdit/range/start/character"),
            Some(&json!(7))
        );
        assert_eq!(
            knuth.pointer("/textEdit/range/end/character"),
            Some(&json!(9))
        );
        assert!(items[0].get("documentation").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn completions_keep_an_existing_closer() {
        let dir = unique_temp_dir("closer");
        std::fs::write(dir.join("refs.bib"), "@book{alpha, title={A}}\n").expect("write bib");
        let main = dir.join("main.mos");
        let src = "#bibliography(\"refs.bib\")\n\nCite [@al].\n";
        let lowered = mos_eval::lower(src, &main);
        let cursor = src.find("al]").unwrap() + 2;

        let items = citation_completions(&lowered, src, byte_to_position(src, cursor));

        assert_eq!(labels(&items), ["alpha"]);
        assert_eq!(items[0].pointer("/textEdit/newText"), Some(&json!("alpha")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn completions_off_a_citation_are_empty() {
        let dir = unique_temp_dir("off");
        std::fs::write(dir.join("refs.bib"), "@book{alpha, title={A}}\n").expect("write bib");
        let main = dir.join("main.mos");
        let src = "#bibliography(\"refs.bib\")\n\nSee @al\n";
        let lowered = mos_eval::lower(src, &main);
        let cursor = src.find("@al").unwrap() + 3;

        let items = citation_completions(&lowered, src, byte_to_position(src, cursor));

        assert!(items.is_empty(), "{items:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn completions_without_loaded_records_are_empty() {
        let main = PathBuf::from("/virtual/main.mos");
        let src = "#bibliography(\"missing.bib\")\n\nCite [@";
        let lowered = mos_eval::lower(src, &main);

        let items = citation_completions(&lowered, src, byte_to_position(src, src.len()));

        assert!(items.is_empty(), "{items:?}");
    }
}
