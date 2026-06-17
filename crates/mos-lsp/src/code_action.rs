//! `textDocument/codeAction` quick fixes backed by compiler suggestions.
//!
//! The server never invents editor-side fixes. It lowers the open document,
//! reads [`mos_core::Suggestion`] values attached to compiler diagnostics, and
//! projects each safe same-file suggestion into one LSP `quickfix` action.

use std::path::Path;

use mos_core::{Diagnostic, SourceSpan, Suggestion};
use serde_json::{Value, json};

use crate::definition::position_to_byte;
use crate::diagnostics::{LspRange, span_to_range};

pub(crate) fn code_actions_for_range(
    file: &Path,
    src: &str,
    uri: &str,
    lowered: &mos_eval::LowerResult,
    request_range: LspRange,
) -> Vec<Value> {
    let request = ByteRange::from_lsp(src, request_range);
    lowered
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic_has_actions_in_range(file, diagnostic, request))
        .flat_map(|diagnostic| {
            diagnostic
                .suggestions()
                .iter()
                .filter(|suggestion| {
                    suggestion.span.file == file && request.intersects(&suggestion.span)
                })
                .map(move |suggestion| action_for_suggestion(src, uri, diagnostic, suggestion))
        })
        .collect()
}

fn diagnostic_has_actions_in_range(
    file: &Path,
    diagnostic: &Diagnostic,
    request: ByteRange,
) -> bool {
    if !diagnostic
        .suggestions()
        .iter()
        .any(|suggestion| suggestion.span.file == file)
    {
        return false;
    }
    if diagnostic
        .span()
        .is_some_and(|span| span.file == file && request.intersects(span))
    {
        return true;
    }
    diagnostic
        .suggestions()
        .iter()
        .any(|suggestion| suggestion.span.file == file && request.intersects(&suggestion.span))
}

fn action_for_suggestion(
    src: &str,
    uri: &str,
    diagnostic: &Diagnostic,
    suggestion: &Suggestion,
) -> Value {
    let title = action_title(src, diagnostic, suggestion);
    let edit = json!({
        "changes": {
            uri: [{
                "range": span_to_range(src, &suggestion.span),
                "newText": suggestion.replacement,
            }],
        },
    });
    json!({
        "title": title,
        "kind": "quickfix",
        "isPreferred": false,
        "edit": edit,
    })
}

fn action_title(src: &str, diagnostic: &Diagnostic, suggestion: &Suggestion) -> String {
    let code = diagnostic.def().code();
    match span_text(src, &suggestion.span) {
        Some("") => format!("{code}: insert `{}`", suggestion.replacement),
        Some(text) if suggestion.replacement.is_empty() => format!("{code}: delete `{text}`"),
        Some(text) => format!("{code}: replace `{text}` with `{}`", suggestion.replacement),
        None => format!("{code}: apply suggestion"),
    }
}

fn span_text<'src>(src: &'src str, span: &SourceSpan) -> Option<&'src str> {
    if span.start() > span.end()
        || span.end() > src.len()
        || !src.is_char_boundary(span.start())
        || !src.is_char_boundary(span.end())
    {
        return None;
    }
    Some(&src[span.start()..span.end()])
}

#[derive(Copy, Clone)]
struct ByteRange {
    start: usize,
    end: usize,
}

impl ByteRange {
    fn from_lsp(src: &str, range: LspRange) -> Self {
        let start = position_to_byte(src, range.start);
        let end = position_to_byte(src, range.end);
        if start <= end {
            Self { start, end }
        } else {
            Self {
                start: end,
                end: start,
            }
        }
    }

    fn intersects(self, span: &SourceSpan) -> bool {
        if span.start() == span.end() {
            if self.start == self.end {
                return self.start == span.start();
            }
            return self.start <= span.start() && span.start() < self.end;
        }
        if self.start == self.end {
            return span.start() <= self.start && self.start < span.end();
        }
        span.start() < self.end && self.start < span.end()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use mos_core::{Diagnostic, Document, SourceSpan, Suggestion, codes};

    use super::*;
    use crate::diagnostics::byte_to_position;

    fn range_for(src: &str, needle: &str) -> LspRange {
        let start = src.find(needle).expect("needle in source");
        LspRange {
            start: byte_to_position(src, start),
            end: byte_to_position(src, start + needle.len()),
        }
    }

    #[test]
    fn projects_each_compiler_suggestion_to_a_text_edit() {
        let file = PathBuf::from("/virtual/main.mos");
        let src = "= Intro <intro>\n\nSee @intrp.\n";
        let lowered = mos_eval::lower(src, &file);

        let actions = code_actions_for_range(
            &file,
            src,
            "file:///virtual/main.mos",
            &lowered,
            range_for(src, "@intrp"),
        );

        assert_eq!(actions.len(), 1, "one MOS0033 fix: {actions:?}");
        assert_eq!(
            actions[0].pointer("/kind").and_then(Value::as_str),
            Some("quickfix")
        );
        assert_eq!(
            actions[0]
                .pointer("/edit/changes")
                .and_then(|changes| changes.get("file:///virtual/main.mos"))
                .and_then(Value::as_array)
                .and_then(|edits| edits.first())
                .and_then(|edit| edit.get("newText"))
                .and_then(Value::as_str),
            Some("@intro")
        );
        assert!(
            actions[0]
                .get("title")
                .and_then(Value::as_str)
                .is_some_and(|title| title.contains(&codes::MOS0033.code().to_string()))
        );
    }

    #[test]
    fn filters_each_suggestion_by_request_range() {
        let file = PathBuf::from("/virtual/main.mos");
        let src = "alpha beta";
        let diagnostic = Diagnostic::simple(
            &codes::MOS0033,
            Some(SourceSpan::new(file.clone(), 0, src.len())),
            "synthetic diagnostic with disjoint fixes",
        )
        .with_suggestion(Suggestion::new(
            SourceSpan::new(file.clone(), 0, 5),
            "ALPHA",
        ))
        .with_suggestion(Suggestion::new(
            SourceSpan::new(file.clone(), 6, 10),
            "BETA",
        ));
        let lowered = mos_eval::LowerResult {
            document: Document::new(file.clone()),
            diagnostics: vec![diagnostic],
            metadata: mos_eval::DocumentMetadata::default(),
            reads_external_resources: false,
        };

        let actions = code_actions_for_range(
            &file,
            src,
            "file:///virtual/main.mos",
            &lowered,
            range_for(src, "alpha"),
        );

        assert_eq!(actions.len(), 1, "only the first fix overlaps: {actions:?}");
        assert_eq!(
            actions[0].get("title").and_then(Value::as_str),
            Some("MOS0033: replace `alpha` with `ALPHA`")
        );
    }

    #[test]
    fn cursor_range_uses_half_open_span_end() {
        let file = PathBuf::from("/virtual/main.mos");
        let src = "alpha";
        let diagnostic = Diagnostic::simple(
            &codes::MOS0033,
            Some(SourceSpan::new(file.clone(), 0, src.len())),
            "synthetic diagnostic at span end",
        )
        .with_suggestion(Suggestion::new(
            SourceSpan::new(file.clone(), 0, src.len()),
            "ALPHA",
        ));
        let lowered = mos_eval::LowerResult {
            document: Document::new(file.clone()),
            diagnostics: vec![diagnostic],
            metadata: mos_eval::DocumentMetadata::default(),
            reads_external_resources: false,
        };
        let request_range = LspRange {
            start: byte_to_position(src, src.len()),
            end: byte_to_position(src, src.len()),
        };

        let actions = code_actions_for_range(
            &file,
            src,
            "file:///virtual/main.mos",
            &lowered,
            request_range,
        );

        assert!(actions.is_empty(), "cursor just past span must not match");
    }

    #[test]
    fn cursor_range_matches_zero_length_insertion_suggestion() {
        let file = PathBuf::from("/virtual/main.mos");
        let src = "alpha";
        let diagnostic = Diagnostic::simple(
            &codes::MOS0034,
            Some(SourceSpan::new(file.clone(), 0, 1)),
            "synthetic diagnostic with insertion fix",
        )
        .with_suggestion(Suggestion::new(
            SourceSpan::new(file.clone(), src.len(), src.len()),
            "!",
        ));
        let lowered = mos_eval::LowerResult {
            document: Document::new(file.clone()),
            diagnostics: vec![diagnostic],
            metadata: mos_eval::DocumentMetadata::default(),
            reads_external_resources: false,
        };
        let request_range = LspRange {
            start: byte_to_position(src, src.len()),
            end: byte_to_position(src, src.len()),
        };

        let actions = code_actions_for_range(
            &file,
            src,
            "file:///virtual/main.mos",
            &lowered,
            request_range,
        );

        assert_eq!(actions.len(), 1, "insertion action: {actions:?}");
        assert_eq!(
            actions[0].get("title").and_then(Value::as_str),
            Some("MOS0034: insert `!`")
        );
    }
}
