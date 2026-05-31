use std::path::{Path, PathBuf};

use mos_core::{Diagnostic, DiagnosticDef, SourceSpan};

use crate::support::list_marker_at;
use crate::{Item, ParseResult, SyntaxTree};

pub(crate) struct Parser<'a> {
    pub(crate) src: &'a str,
    pub(crate) file: PathBuf,
    pub(crate) pos: usize,
    pub(crate) items: Vec<Item>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(src: &'a str, file: &Path) -> Self {
        Self {
            src,
            file: file.to_path_buf(),
            pos: 0,
            items: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub(crate) fn run(mut self) -> ParseResult {
        while self.pos < self.src.len() {
            if self.at_blank_line() {
                self.skip_line();
                continue;
            }
            if let Some(kw) = self.at_directive_keyword() {
                self.parse_directive_block(kw);
            } else if self.starts_with("=") {
                self.parse_heading();
            } else if self.at_list_marker() {
                self.parse_list();
            } else {
                self.parse_paragraph();
            }
        }
        ParseResult {
            tree: SyntaxTree {
                file: self.file,
                items: self.items,
            },
            diagnostics: self.diagnostics,
        }
    }

    pub(crate) fn at_list_marker(&self) -> bool {
        list_marker_at(self.src.as_bytes(), self.pos).is_some()
    }

    pub(crate) fn span(&self, start: usize, end: usize) -> SourceSpan {
        SourceSpan::new(self.file.clone(), start, end)
    }

    pub(crate) fn starts_with(&self, prefix: &str) -> bool {
        self.src.as_bytes()[self.pos..].starts_with(prefix.as_bytes())
    }

    pub(crate) fn at_directive_keyword(&self) -> Option<&'static str> {
        const KEYWORDS: &[&str] = &["set", "image", "figure", "pre", "code"];
        if !self.starts_with("#") {
            return None;
        }
        let after_hash = self.pos + 1;
        let bytes = self.src.as_bytes();
        for kw in KEYWORDS {
            let end = after_hash + kw.len();
            if end > bytes.len() {
                continue;
            }
            if &bytes[after_hash..end] != kw.as_bytes() {
                continue;
            }
            let boundary = bytes.get(end).is_none_or(|&b| {
                b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == b'(' || b == b'['
            });
            if boundary {
                return Some(kw);
            }
        }
        None
    }

    pub(crate) fn at_blank_line(&self) -> bool {
        let bytes = self.src.as_bytes();
        let mut i = self.pos;
        while i < bytes.len() && bytes[i] != b'\n' {
            if !bytes[i].is_ascii_whitespace() {
                return false;
            }
            i += 1;
        }
        true
    }

    pub(crate) fn skip_line(&mut self) {
        let bytes = self.src.as_bytes();
        while self.pos < bytes.len() && bytes[self.pos] != b'\n' {
            self.pos += 1;
        }
        if self.pos < bytes.len() {
            self.pos += 1;
        }
    }

    pub(crate) fn current_line_bounds(&self) -> (usize, usize, usize) {
        self.line_bounds_from(self.pos)
    }

    pub(crate) fn line_bounds_from(&self, start: usize) -> (usize, usize, usize) {
        let bytes = self.src.as_bytes();
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'\n' {
            end += 1;
        }
        let line_end = if end < bytes.len() { end + 1 } else { end };
        let mut content_end = end;
        if content_end > start && bytes[content_end - 1] == b'\r' {
            content_end -= 1;
        }
        (start, content_end, line_end)
    }

    pub(crate) fn warn(
        &self,
        def: &'static DiagnosticDef,
        message: &str,
        start: usize,
        end: usize,
    ) -> Diagnostic {
        Diagnostic::simple(def, Some(self.span(start, end)), message)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use mos_core::{CollectingSink, Severity, codes};

    use crate::*;

    fn parse_str(src: &str) -> ParseResult {
        let mut sink = CollectingSink::new();
        let tree = parse(src, &PathBuf::from("test.mos"), &mut sink)
            .expect("parse never structurally aborts");
        ParseResult {
            tree,
            diagnostics: sink.into_diagnostics(),
        }
    }

    #[test]
    fn empty_source() {
        let r = parse_str("");
        assert!(r.tree.items.is_empty());
        assert!(!r.has_errors());
    }

    #[test]
    fn single_heading() {
        let r = parse_str("= Hello\n");
        assert!(!r.has_errors());
        assert_eq!(r.tree.items.len(), 1);
        let (level, inlines, _) = r.tree.items[0].as_heading().unwrap();
        assert_eq!(level, 1);
        assert_eq!(inlines.len(), 1);
        assert_eq!(inlines[0].text, "Hello");
        assert_eq!(inlines[0].kind, InlineKind::Text);
    }

    #[test]
    fn heading_levels() {
        let src = "= One\n== Two\n=== Three\n";
        let r = parse_str(src);
        assert!(!r.has_errors());
        let levels: Vec<u8> = r
            .tree
            .items
            .iter()
            .filter_map(|i| i.as_heading().map(|(l, _, _)| l))
            .collect();
        assert_eq!(levels, vec![1, 2, 3]);
    }

    #[test]
    fn paragraph_collects_lines() {
        let src = "first line\nsecond line\n\nnext para\n";
        let r = parse_str(src);
        assert!(!r.has_errors());
        assert_eq!(r.tree.items.len(), 2);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        assert_eq!(inlines.len(), 1);
        assert_eq!(inlines[0].text, "first line\nsecond line");
    }

    #[test]
    fn inline_emphasis_strong_code() {
        let src = "a *b* c **d** e `f` g\n";
        let r = parse_str(src);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        let kinds: Vec<InlineKind> = inlines.iter().map(|i| i.kind).collect();
        assert_eq!(
            kinds,
            vec![
                InlineKind::Text,
                InlineKind::Emphasis,
                InlineKind::Text,
                InlineKind::Strong,
                InlineKind::Text,
                InlineKind::Code,
                InlineKind::Text,
            ]
        );
        let texts: Vec<&str> = inlines.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, vec!["a ", "b", " c ", "d", " e ", "f", " g"]);
    }

    #[test]
    fn nested_bold_italic_triple_delimiter() {
        let r = parse_str("***x***\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        assert_eq!(inlines.len(), 1, "got {inlines:?}");
        assert_eq!(inlines[0].kind, InlineKind::BoldItalic);
        assert_eq!(inlines[0].text, "x");
    }

    #[test]
    fn nested_emphasis_inside_strong() {
        let r = parse_str("**a *b* c**\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        let kinds: Vec<InlineKind> = inlines.iter().map(|i| i.kind).collect();
        assert_eq!(
            kinds,
            vec![
                InlineKind::Strong,
                InlineKind::BoldItalic,
                InlineKind::Strong,
            ],
            "got {inlines:?}",
        );
        let texts: Vec<&str> = inlines.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, vec!["a ", "b", " c"]);
    }

    #[test]
    fn nested_strong_inside_emphasis() {
        let r = parse_str("*a **b** c*\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        let kinds: Vec<InlineKind> = inlines.iter().map(|i| i.kind).collect();
        assert_eq!(
            kinds,
            vec![
                InlineKind::Emphasis,
                InlineKind::BoldItalic,
                InlineKind::Emphasis,
            ],
            "got {inlines:?}",
        );
        let texts: Vec<&str> = inlines.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, vec!["a ", "b", " c"]);
    }

    #[test]
    fn ambiguous_inner_star_stays_strong_text() {
        let r = parse_str("**a*b**\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        assert_eq!(inlines.len(), 1, "got {inlines:?}");
        assert_eq!(inlines[0].kind, InlineKind::Strong);
        assert_eq!(inlines[0].text, "a*b");
    }

    #[test]
    fn code_spans_do_not_parse_nested_emphasis() {
        let r = parse_str("`***x***`\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        assert_eq!(inlines.len(), 1, "got {inlines:?}");
        assert_eq!(inlines[0].kind, InlineKind::Code);
        assert_eq!(inlines[0].text, "***x***");
    }

    #[test]
    fn unterminated_emphasis_warns() {
        let r = parse_str("hi *there\n");
        assert!(!r.has_errors());
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0021.code()
                    && d.severity() == Severity::Warning)
        );
    }

    #[test]
    fn unterminated_strong_warns() {
        let r = parse_str("hi **there\n");
        assert!(!r.has_errors());
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0020.code()
                    && d.severity() == Severity::Warning)
        );
    }

    #[test]
    fn set_block_simple() {
        let r = parse_str("#set page(paper: \"A4\")\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (name, args, _) = r.tree.items[0].as_set().unwrap();
        assert_eq!(name, "page");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].key(), Some("paper"));
        assert_eq!(args[0].value(), &SetValue::Str("A4".to_owned()));
    }

    #[test]
    fn set_block_multiline() {
        let src = "#set document(\n  title: \"x\",\n  author: \"y\",\n)\n\n= After\n";
        let r = parse_str(src);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 2);
        let (name, args, _) = r.tree.items[0].as_set().unwrap();
        assert_eq!(name, "document");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0].key(), Some("title"));
        assert_eq!(args[0].value(), &SetValue::Str("x".to_owned()));
        assert_eq!(args[1].key(), Some("author"));
        assert_eq!(args[1].value(), &SetValue::Str("y".to_owned()));
        assert_eq!(r.tree.items[1].as_heading().unwrap().0, 1);
    }

    #[test]
    fn set_value_length_units() {
        let src = "#set page(margin: 24mm)\n#set text(size: 11pt, leading: 1.35, scale: 2em)\n";
        let r = parse_str(src);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (_, page_args, _) = r.tree.items[0].as_set().unwrap();
        assert_eq!(
            page_args[0].value(),
            &SetValue::Length(24.0, LengthUnit::Mm)
        );
        let (_, text_args, _) = r.tree.items[1].as_set().unwrap();
        assert_eq!(
            text_args[0].value(),
            &SetValue::Length(11.0, LengthUnit::Pt)
        );
        assert_eq!(text_args[1].value(), &SetValue::Float(1.35));
        assert_eq!(text_args[2].value(), &SetValue::Length(2.0, LengthUnit::Em));
    }

    #[test]
    fn set_value_int_and_ident() {
        let r = parse_str("#set foo(count: 42, alignment: bottom-center)\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (_, args, _) = r.tree.items[0].as_set().unwrap();
        assert_eq!(args[0].value(), &SetValue::Int(42));
        assert_eq!(
            args[1].value(),
            &SetValue::Ident("bottom-center".to_owned())
        );
    }

    #[test]
    fn set_value_trailing_comma_ok() {
        let r = parse_str("#set page(paper: \"A4\",)\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (_, args, _) = r.tree.items[0].as_set().unwrap();
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn set_string_escape_sequences() {
        let r = parse_str("#set foo(s: \"a\\\"b\\nc\\\\d\")\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (_, args, _) = r.tree.items[0].as_set().unwrap();
        assert_eq!(args[0].value(), &SetValue::Str("a\"b\nc\\d".to_owned()));
    }

    #[test]
    fn set_unknown_escape_with_multibyte_does_not_panic() {
        // Regression: the byte after `\` may be the leading byte of a
        // multibyte UTF-8 scalar (here `é` = 0xC3 0xA9). Advancing by 2
        // would leave the cursor mid-codepoint and the next slice
        // would panic. The parser must walk to a char boundary and
        // emit MOS0014 instead.
        let r = parse_str("#set foo(s: \"\\é\")\n");
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0014.code()),
            "expected MOS0014, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn set_unknown_unit_emits_e014() {
        let r = parse_str("#set page(margin: 24xx)\n");
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0014.code()),
            "expected MOS0014, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn set_lone_minus_emits_e014() {
        // `-` not followed by a digit is a malformed number literal.
        let r = parse_str("#set foo(x: -)\n");
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0014.code()),
            "expected MOS0014, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn set_without_identifier_emits_e010() {
        // `#set` followed only by whitespace before the newline has no
        // target identifier. The parser must diagnose with MOS0010 and
        // skip the line rather than treat the next line as the body.
        let r = parse_str("#set\nbody\n");
        let e010: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.def().code() == codes::MOS0010.code())
            .collect();
        assert_eq!(
            e010.len(),
            1,
            "expected exactly one MOS0010, got {:?}",
            r.diagnostics
        );
        assert!(
            e010[0].message().contains("#set"),
            "MOS0010 message should mention `#set`, got {:?}",
            e010[0].message()
        );
        // Recovery: the next line must parse as its own paragraph,
        // not get eaten as the directive body.
        assert!(
            r.tree.items.iter().any(|i| {
                i.as_paragraph()
                    .is_some_and(|(inlines, _)| inlines.iter().any(|x| x.text.contains("body")))
            }),
            "expected a recovered `body` paragraph, got items {:?}",
            r.tree.items
        );
    }

    #[test]
    fn set_missing_colon_emits_e015() {
        let r = parse_str("#set page(paper \"A4\")\n");
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0015.code()),
            "expected MOS0015, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn set_positional_arg_emits_e015() {
        let r = parse_str("#set page(\"A4\")\n");
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0015.code())
        );
    }

    #[test]
    fn unterminated_set_block_errors() {
        let r = parse_str("#set page(\n  paper: \"A4\",\n");
        assert!(r.has_errors());
    }

    #[test]
    fn trailing_content_after_set_block_diagnoses_and_recovers() {
        // Prior behaviour swallowed everything between `)` and the
        // next `\n`. The parser now emits MOS0013 and leaves the
        // trailing bytes in place so they parse as a paragraph.
        let r = parse_str("#set page(paper: \"A4\") leftover\n");
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0013.code()),
            "expected MOS0013 diagnostic, got {:?}",
            r.diagnostics
        );
        assert!(r.tree.items.iter().any(|i| i.as_set().is_some()));
        assert!(r.tree.items.iter().any(|i| {
            i.as_paragraph()
                .is_some_and(|(inlines, _)| inlines.iter().any(|x| x.text.contains("leftover")))
        }));
    }

    #[test]
    fn set_block_followed_by_horizontal_whitespace_then_newline_is_ok() {
        let r = parse_str("#set page(paper: \"A4\")  \t\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 1);
    }

    #[test]
    fn set_with_string_containing_paren() {
        let r = parse_str("#set foo(label: \"closes ) inside\")\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 1);
    }

    #[test]
    fn equals_without_space_is_paragraph() {
        let r = parse_str("=notaheading\n");
        assert!(!r.has_errors());
        assert!(r.tree.items[0].as_paragraph().is_some());
    }

    #[test]
    fn heading_span_is_within_source() {
        let src = "= Title\n";
        let r = parse_str(src);
        let (_, _, span) = r.tree.items[0].as_heading().unwrap();
        assert_eq!(&src[span.start..span.end], "= Title");
    }

    #[test]
    fn crlf_line_endings_handled() {
        let r = parse_str("= Title\r\nbody\r\n");
        assert!(!r.has_errors());
        assert_eq!(r.tree.items.len(), 2);
    }

    #[test]
    fn set_prefix_without_token_boundary_stays_paragraph() {
        // `#setting` is not the `#set` keyword. The parser must not
        // route it to the set-block path and emit a spurious error.
        let r = parse_str("#setting up\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert!(r.tree.items[0].as_paragraph().is_some());
    }

    #[test]
    fn set_prefix_followed_by_paren_is_set_block() {
        // No whitespace, but `(` is also a valid token boundary.
        let r = parse_str("#set(name: \"x\")\n");
        // Either the parser recognises this as a set block with no
        // identifier (MOS0010) or it parses as paragraph; what matters
        // is that it does NOT panic and returns a structured result.
        // We document the current behaviour here: `#set` is treated
        // as the keyword and `name` is parsed as the body identifier
        // — see `at_set_keyword`. This guards against regression.
        assert_eq!(r.tree.items.len() + r.diagnostics.len(), 1);
    }

    #[test]
    fn paragraph_inline_spans_align_with_crlf_source() {
        // Regression for the CRLF byte-offset bug: when the paragraph
        // contains `\r\n` between its lines, inline spans on the
        // second line must still index into the original source.
        let src = "first\r\n*x*\r\n";
        let r = parse_str(src);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        // The emphasis run is the byte sequence `*x*` on the second
        // line. Its span must point at exactly those three bytes in
        // the original source, regardless of the CR before it.
        let emph = inlines
            .iter()
            .find(|i| i.kind == InlineKind::Emphasis)
            .expect("emphasis inline");
        assert_eq!(&src[emph.span.start..emph.span.end], "*x*");
        assert_eq!(emph.text, "x");
    }

    #[test]
    fn heading_with_trailing_label_attaches() {
        let r = parse_str("= Methods <sec:methods>\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let item = &r.tree.items[0];
        let (_, inlines, _) = item.as_heading().unwrap();
        assert_eq!(item.label(), Some("sec:methods"));
        assert_eq!(inlines.len(), 1);
        assert_eq!(inlines[0].text, "Methods");
    }

    #[test]
    fn paragraph_with_leading_label_attaches() {
        let r = parse_str("<intro> body text\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let item = &r.tree.items[0];
        let (inlines, _) = item.as_paragraph().unwrap();
        assert_eq!(item.label(), Some("intro"));
        assert_eq!(inlines[0].text, "body text");
    }

    #[test]
    fn at_label_produces_reference_inline() {
        let r = parse_str("see @sec:methods now\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        let kinds: Vec<InlineKind> = inlines.iter().map(|i| i.kind).collect();
        assert_eq!(
            kinds,
            vec![InlineKind::Text, InlineKind::Reference, InlineKind::Text]
        );
        let r_inline = inlines
            .iter()
            .find(|i| i.kind == InlineKind::Reference)
            .unwrap();
        assert_eq!(r_inline.text, "sec:methods");
    }

    #[test]
    fn stray_at_warns_and_stays_text() {
        let r = parse_str("an @ symbol\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0023.code())
        );
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        assert!(!inlines.iter().any(|i| i.kind == InlineKind::Reference));
    }

    #[test]
    fn heading_without_label_keeps_full_text() {
        let r = parse_str("= Just a title\n");
        let item = &r.tree.items[0];
        let (_, inlines, _) = item.as_heading().unwrap();
        assert_eq!(item.label(), None);
        assert_eq!(inlines[0].text, "Just a title");
    }

    #[test]
    fn paragraph_with_angle_text_not_label() {
        // `<` inside paragraph body that isn't a leading label-only
        // token must be left as text.
        let r = parse_str("a < b > c\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let item = &r.tree.items[0];
        assert_eq!(item.label(), None);
        let (inlines, _) = item.as_paragraph().unwrap();
        assert_eq!(inlines[0].text, "a < b > c");
    }

    #[test]
    fn paragraph_inline_text_is_crlf_normalized() {
        // The raw slice contains `\r\n` between paragraph lines, but
        // the Inline.text payload should be `\n`-only so the same
        // source lowers identically on Windows and Unix.
        let src = "alpha\r\nbeta\r\n";
        let r = parse_str(src);
        assert!(!r.has_errors());
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        assert!(
            inlines.iter().all(|i| !i.text.contains('\r')),
            "inline text should be CRLF-normalized: {:?}",
            inlines.iter().map(|i| &i.text).collect::<Vec<_>>()
        );
        // The first text run still spans the raw bytes including the
        // CR — only the *payload* is normalized.
        let text = inlines.iter().find(|i| i.kind == InlineKind::Text).unwrap();
        assert_eq!(text.text, "alpha\nbeta");
        assert_eq!(&src[text.span.start..text.span.end], "alpha\r\nbeta");
    }

    #[test]
    fn image_directive_with_positional_path() {
        let r = parse_str("#image(\"scan.png\")\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (name, args, _) = r.tree.items[0].as_set().unwrap();
        assert_eq!(name, "image");
        assert_eq!(args.len(), 1);
        // Positional arg: `.key()` returns `None`, and the variant
        // pattern-matches as `SetArg::Positional`. Both forms are
        // exercised so test sites that prefer pattern-matching and
        // sites that prefer accessor methods both see the contract.
        assert!(matches!(args[0], SetArg::Positional { .. }));
        assert_eq!(args[0].key(), None);
        assert_eq!(args[0].value(), &SetValue::Str("scan.png".to_owned()));
    }

    #[test]
    fn image_directive_with_positional_and_keyed_args() {
        let r = parse_str("#image(\"scan.png\", alt: \"a CTPA scan\", width: 200pt)\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (name, args, _) = r.tree.items[0].as_set().unwrap();
        assert_eq!(name, "image");
        assert_eq!(args.len(), 3);
        assert_eq!(args[0].key(), None);
        assert_eq!(args[1].key(), Some("alt"));
        assert_eq!(args[2].key(), Some("width"));
        assert_eq!(args[2].value(), &SetValue::Length(200.0, LengthUnit::Pt));
    }

    #[test]
    fn figure_directive_with_keyed_args() {
        let r = parse_str("#figure(image: \"scan.png\", caption: \"A scan.\")\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (name, args, _) = r.tree.items[0].as_set().unwrap();
        assert_eq!(name, "figure");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0].key(), Some("image"));
        assert_eq!(args[0].value(), &SetValue::Str("scan.png".to_owned()));
        assert_eq!(args[1].key(), Some("caption"));
    }

    #[test]
    fn figure_directive_positional_path() {
        // Pins the `#figure("…")` spelling the directive grammar
        // advertises — the eval layer treats the first positional arg
        // the same way `#image(...)` does, so the parser-level shape
        // must match `#image`'s.
        let r = parse_str("#figure(\"scan.png\")\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (name, args, _) = r.tree.items[0].as_set().unwrap();
        assert_eq!(name, "figure");
        assert_eq!(args.len(), 1);
        assert!(matches!(args[0], SetArg::Positional { .. }));
        assert_eq!(args[0].value(), &SetValue::Str("scan.png".to_owned()));
    }

    #[test]
    fn raw_blocks_preserve_body_text() {
        let r = parse_str("#code[[fn main() {\n    println(\"hi\");\n}]]\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 1);
        let raw = r.tree.items[0].as_raw_block();
        assert!(
            raw.is_some(),
            "expected raw block, got {:?}",
            r.tree.items[0]
        );
        if let Some(raw) = raw {
            assert_eq!(raw.kind, RawBlockKind::Code);
            assert!(raw.args.is_empty());
            assert_eq!(raw.label, None);
            assert_eq!(raw.text, "fn main() {\n    println(\"hi\");\n}");
        }
    }

    #[test]
    fn raw_blocks_preserve_zero_equals_inner_brackets() {
        let r = parse_str("#code[[let x = vec![1, 2, 3];]]\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let raw = r.tree.items[0].as_raw_block();
        assert!(
            raw.is_some(),
            "expected raw block, got {:?}",
            r.tree.items[0]
        );
        if let Some(raw) = raw {
            assert_eq!(raw.kind, RawBlockKind::Code);
            assert_eq!(raw.text, "let x = vec![1, 2, 3];");
        }
    }

    #[test]
    fn raw_blocks_preserve_delimiter_like_text() {
        let r = parse_str("#pre[=[open \\] close ] and ]] close]=]\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let raw = r.tree.items[0].as_raw_block();
        assert!(
            raw.is_some(),
            "expected raw block, got {:?}",
            r.tree.items[0]
        );
        if let Some(raw) = raw {
            assert_eq!(raw.kind, RawBlockKind::Pre);
            assert!(raw.args.is_empty());
            assert_eq!(raw.label, None);
            assert_eq!(raw.text, "open \\] close ] and ]] close");
        }
    }

    #[test]
    fn raw_blocks_preserve_arguments_and_label() {
        let r = parse_str("#code(lang: \"rust\")[[fn main() {}]] <ex:code>\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 1);

        let raw = r.tree.items[0].as_raw_block();
        assert!(
            raw.is_some(),
            "expected raw block, got {:?}",
            r.tree.items[0]
        );
        if let Some(raw) = raw {
            assert_eq!(raw.kind, RawBlockKind::Code);
            assert_eq!(raw.args.len(), 1);
            assert_eq!(raw.args[0].key(), Some("lang"));
            assert_eq!(raw.args[0].value(), &SetValue::Str("rust".to_owned()));
            assert_eq!(raw.text, "fn main() {}");
            assert_eq!(raw.label, Some("ex:code"));
        }
        assert_eq!(r.tree.items[0].label(), Some("ex:code"));
    }

    #[test]
    fn raw_blocks_trim_leading_delimiter_newline_and_normalize_line_endings() {
        let r = parse_str("#code[[\r\n\tprintln!(\"hi\");\r\n]]\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let raw = r.tree.items[0].as_raw_block();
        assert!(
            raw.is_some(),
            "expected raw block, got {:?}",
            r.tree.items[0]
        );
        if let Some(raw) = raw {
            assert_eq!(raw.text, "\tprintln!(\"hi\");\n");
        }
    }

    #[test]
    fn bracket_raw_blocks_are_rejected() {
        let r = parse_str("#code[fn main() {}]\n");
        assert!(r.has_errors(), "{:?}", r.diagnostics);
        assert!(r.tree.items.is_empty(), "{:?}", r.tree.items);
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.message().contains("long brackets")),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn directive_prefix_without_token_boundary_stays_paragraph() {
        // `#imagery` and `#figures` are not directive keywords. They
        // must not be routed to the directive path.
        let r = parse_str("#imagery here\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert!(r.tree.items[0].as_paragraph().is_some());
    }

    #[test]
    fn unterminated_image_directive_errors_with_e012() {
        let r = parse_str("#image(\n  alt: \"x\"\n");
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0012.code() && d.message().contains("#image")),
            "expected MOS0012 mentioning #image, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn directive_terminates_paragraph() {
        // A paragraph in progress must stop at the next directive so
        // the directive parses cleanly instead of being slurped into
        // the paragraph body.
        for (src, expected_kind, expected_name) in [
            (
                "body line\n#set document(title: \"x\")\nmore\n",
                DirectiveKind::Set,
                "document",
            ),
            (
                "body line\n#image(\"x.png\")\nmore\n",
                DirectiveKind::Image,
                "image",
            ),
            (
                "body line\n#figure(\"x.png\")\nmore\n",
                DirectiveKind::Figure,
                "figure",
            ),
        ] {
            let r = parse_str(src);
            assert!(!r.has_errors(), "{:?}", r.diagnostics);
            // Expect: paragraph, directive, paragraph.
            assert_eq!(r.tree.items.len(), 3);
            assert!(r.tree.items[0].as_paragraph().is_some());
            assert_eq!(r.tree.items[1].directive_kind(), Some(expected_kind));
            let (name, _, _) = r.tree.items[1].as_set().unwrap();
            assert_eq!(name, expected_name);
            assert!(r.tree.items[2].as_paragraph().is_some());
        }
    }

    #[test]
    fn unordered_list_simple() {
        let r = parse_str("- a\n- b\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 1);
        let (ordered, items, _) = r.tree.items[0].as_list().unwrap();
        assert!(!ordered);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].inlines[0].text, "a");
        assert_eq!(items[1].inlines[0].text, "b");
        assert!(items[0].children.is_empty());
        assert!(items[1].children.is_empty());
    }

    #[test]
    fn ordered_list_simple() {
        let r = parse_str("1. first\n2. second\n3. third\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 1);
        let (ordered, items, _) = r.tree.items[0].as_list().unwrap();
        assert!(ordered);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].inlines[0].text, "first");
        assert_eq!(items[1].inlines[0].text, "second");
        assert_eq!(items[2].inlines[0].text, "third");
    }

    #[test]
    fn list_items_carry_inline_emphasis() {
        let r = parse_str("- plain\n- *italic* text\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (_, items, _) = r.tree.items[0].as_list().unwrap();
        let kinds: Vec<InlineKind> = items[1].inlines.iter().map(|i| i.kind).collect();
        assert_eq!(
            kinds,
            vec![InlineKind::Emphasis, InlineKind::Text],
            "got {:?}",
            items[1].inlines
        );
    }

    #[test]
    fn nested_list_two_deep() {
        let src = "- outer 1\n  - inner a\n  - inner b\n- outer 2\n";
        let r = parse_str(src);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 1);
        let (_, items, _) = r.tree.items[0].as_list().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].inlines[0].text, "outer 1");
        assert_eq!(items[1].inlines[0].text, "outer 2");
        assert_eq!(items[0].children.len(), 1);
        assert!(items[1].children.is_empty());
        let (nested_ordered, nested_items, _) = items[0].children[0].as_list().unwrap();
        assert!(!nested_ordered);
        assert_eq!(nested_items.len(), 2);
        assert_eq!(nested_items[0].inlines[0].text, "inner a");
        assert_eq!(nested_items[1].inlines[0].text, "inner b");
    }

    #[test]
    fn mixed_prose_and_list() {
        let src = "Intro paragraph.\n\n- one\n- two\n\nClosing paragraph.\n";
        let r = parse_str(src);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 3);
        assert!(r.tree.items[0].as_paragraph().is_some());
        let (_, list_items, _) = r.tree.items[1].as_list().unwrap();
        assert_eq!(list_items.len(), 2);
        assert!(r.tree.items[2].as_paragraph().is_some());
    }

    #[test]
    fn list_marker_breaks_running_paragraph() {
        // No blank line between paragraph and list — the marker still
        // opens a fresh block.
        let r = parse_str("paragraph line\n- item\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 2);
        assert!(r.tree.items[0].as_paragraph().is_some());
        let (_, items, _) = r.tree.items[1].as_list().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].inlines[0].text, "item");
    }

    #[test]
    fn ordered_renumbers_from_one_regardless_of_source_digits() {
        // The parser preserves the literal digits the user typed in
        // each item's text, but ordered_renumbering is the lowerer's /
        // layout's job. At parse time, the only thing we report is
        // that the items are ordered; the numbering source is the
        // item index, not the literal `5.` typed in source.
        let r = parse_str("5. five\n7. seven\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (ordered, items, _) = r.tree.items[0].as_list().unwrap();
        assert!(ordered);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].inlines[0].text, "five");
        assert_eq!(items[1].inlines[0].text, "seven");
    }

    #[test]
    fn ordered_to_unordered_at_same_indent_splits_lists() {
        let r = parse_str("1. one\n2. two\n- three\n- four\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 2);
        let (a_ordered, a_items, _) = r.tree.items[0].as_list().unwrap();
        assert!(a_ordered);
        assert_eq!(a_items.len(), 2);
        let (b_ordered, b_items, _) = r.tree.items[1].as_list().unwrap();
        assert!(!b_ordered);
        assert_eq!(b_items.len(), 2);
    }

    #[test]
    fn dash_without_space_is_paragraph() {
        // A bare `-foo` line is a paragraph, not a list — the marker
        // requires trailing whitespace.
        let r = parse_str("-foo\n");
        assert!(!r.has_errors());
        assert!(r.tree.items[0].as_paragraph().is_some());
    }

    #[test]
    fn number_dot_without_space_is_paragraph() {
        // `1.foo` without trailing whitespace is not an ordered list
        // marker. (Even `1.` alone with no content is not — keeps the
        // parser conservative around inline numerals like `1.5`.)
        let r = parse_str("1.foo\n");
        assert!(!r.has_errors());
        assert!(r.tree.items[0].as_paragraph().is_some());
    }

    #[test]
    fn list_terminated_by_blank_line() {
        let src = "- a\n- b\n\n- c\n";
        let r = parse_str(src);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        // Two separate lists, split by the blank line.
        assert_eq!(r.tree.items.len(), 2);
        let (_, a, _) = r.tree.items[0].as_list().unwrap();
        let (_, c, _) = r.tree.items[1].as_list().unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn list_item_span_covers_its_line() {
        let src = "- hello\n";
        let r = parse_str(src);
        let (_, items, _) = r.tree.items[0].as_list().unwrap();
        let span = &items[0].span;
        assert_eq!(&src[span.start..span.end], "- hello");
    }

    #[test]
    fn nested_list_span_includes_children() {
        let src = "- a\n  - b\n";
        let r = parse_str(src);
        let (_, _, span) = r.tree.items[0].as_list().unwrap();
        // Outer list's span should reach to the end of the nested item.
        assert!(span.end > src.find('b').unwrap());
    }

    // ---------- line-break controls (issue #26) ----------

    #[test]
    fn nbsp_is_preserved_inside_a_single_text_inline() {
        // U+00A0 NBSP is not ASCII whitespace; it must round-trip
        // through the parser as one logical word, distinct from a
        // regular space which would split the paragraph's text differently
        // downstream. Pinned so a future UAX #14-aware splitter can't
        // accidentally normalize NBSP to a regular space.
        let r = parse_str("Mr.\u{A0}Smith\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        assert_eq!(inlines.len(), 1, "got {inlines:?}");
        assert_eq!(inlines[0].kind, InlineKind::Text);
        assert!(
            inlines[0].text.contains('\u{A0}'),
            "expected NBSP in text payload, got {:?}",
            inlines[0].text
        );
        assert_eq!(inlines[0].text, "Mr.\u{A0}Smith");
    }

    #[test]
    fn hard_break_double_backslash() {
        let r = parse_str("foo\\\\bar\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        let kinds: Vec<InlineKind> = inlines.iter().map(|i| i.kind).collect();
        assert_eq!(
            kinds,
            vec![InlineKind::Text, InlineKind::HardBreak, InlineKind::Text],
            "got {inlines:?}"
        );
        assert_eq!(inlines[0].text, "foo");
        assert!(inlines[1].text.is_empty());
        assert_eq!(inlines[2].text, "bar");
    }

    #[test]
    fn hard_break_double_in_a_row() {
        let r = parse_str("a\\\\\\\\b\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        let kinds: Vec<InlineKind> = inlines.iter().map(|i| i.kind).collect();
        assert_eq!(
            kinds,
            vec![
                InlineKind::Text,
                InlineKind::HardBreak,
                InlineKind::HardBreak,
                InlineKind::Text,
            ],
            "got {inlines:?}"
        );
    }

    #[test]
    fn hard_break_at_start_of_paragraph() {
        let r = parse_str("\\\\foo\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        let kinds: Vec<InlineKind> = inlines.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, vec![InlineKind::HardBreak, InlineKind::Text]);
        assert_eq!(inlines[1].text, "foo");
    }

    #[test]
    fn hard_break_then_strong() {
        // `\\` flushes any pending text and slots cleanly between
        // adjacent inline styles. Regression check that the delimiter
        // scanner sees the right `text_start` after the hard break.
        let r = parse_str("a\\\\**b**\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        let kinds: Vec<InlineKind> = inlines.iter().map(|i| i.kind).collect();
        assert_eq!(
            kinds,
            vec![InlineKind::Text, InlineKind::HardBreak, InlineKind::Strong],
            "got {inlines:?}"
        );
        assert_eq!(inlines[2].text, "b");
    }

    #[test]
    fn lone_trailing_backslash_warns_with_w025() {
        let r = parse_str("foo\\\n");
        assert!(!r.has_errors());
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0024.code()
                    && d.severity() == Severity::Warning),
            "expected MOS0024 warning, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn backslash_before_non_escape_byte_is_silent_literal() {
        // `\` followed by a byte we don't recognise as an escape
        // (`*`, `@`, `x`, drive-letter path, etc.) must not emit a
        // diagnostic -- the prior contract was "backslash is literal",
        // and only the trailing-`\` case crosses the bar where a
        // warning helps the author. Three samples cover the cases that
        // previously fired spurious W025s.
        for src in [
            "foo \\* bar\n",
            "see C:\\Temp\\file\n",
            "stray \\x literal\n",
        ] {
            let r = parse_str(src);
            assert!(!r.has_errors(), "src {src:?}: {:?}", r.diagnostics);
            assert!(
                !r.diagnostics
                    .iter()
                    .any(|d| d.def().code() == codes::MOS0024.code()),
                "src {src:?} produced unexpected MOS0024: {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn soft_hyphen_shorthand_expands_to_u00ad() {
        // `\-` is a soft-hyphen shorthand: it expands inline to a
        // literal U+00AD inside the current text run. No new IR
        // variant -- SHY is just a codepoint that downstream shaping
        // strips before rendering.
        let r = parse_str("a\\-b\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        assert_eq!(inlines.len(), 1, "got {inlines:?}");
        assert_eq!(inlines[0].kind, InlineKind::Text);
        assert_eq!(inlines[0].text, "a\u{AD}b");
    }

    #[test]
    fn soft_hyphen_span_covers_the_consumed_source_bytes() {
        // Regression: the `\-` shorthand was advancing `text_start`
        // past the consumed bytes without recording where the run
        // originally started, so the emitted Inline carried a
        // zero-width span pointing at the post-`\-` bytes only.
        let src = "a\\-b\n";
        let r = parse_str(src);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        assert_eq!(inlines.len(), 1, "got {inlines:?}");
        assert_eq!(inlines[0].text, "a\u{AD}b");
        // The span should cover all four source bytes of `a\-b`,
        // not just the trailing `b`. Newline is not part of the
        // paragraph inline run.
        assert_eq!(
            inlines[0].span.end - inlines[0].span.start,
            4,
            "expected span over `a\\-b` (4 bytes), got {:?}",
            inlines[0].span
        );
    }

    #[test]
    fn soft_hyphen_shorthand_repeats_in_one_run() {
        // Multiple `\-` shorthands inside one text run accumulate into
        // a single Text inline -- the pending buffer flushes only at
        // run boundaries, not at every escape.
        let r = parse_str("su\\-per\\-cali\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        assert_eq!(inlines.len(), 1, "got {inlines:?}");
        assert_eq!(inlines[0].kind, InlineKind::Text);
        assert_eq!(inlines[0].text, "su\u{AD}per\u{AD}cali");
    }

    #[test]
    fn literal_nbsp_codepoint_round_trips_through_emphasis() {
        // Belt-and-suspenders: NBSP inside an emphasis run also survives
        // unchanged (no whitespace-style normalization at the inline
        // boundary).
        let r = parse_str("*Mr.\u{A0}Smith*\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let (inlines, _) = r.tree.items[0].as_paragraph().unwrap();
        assert_eq!(inlines.len(), 1, "got {inlines:?}");
        assert_eq!(inlines[0].kind, InlineKind::Emphasis);
        assert_eq!(inlines[0].text, "Mr.\u{A0}Smith");
    }
}
