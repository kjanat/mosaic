use crate::parser::Parser;
use crate::support::{find_byte, scan_label_chars};
use mos_core::{Suggestion, codes};

use crate::{Inline, InlineKind};

#[derive(Clone, Copy, Debug, Default)]
enum InlineStyle {
    #[default]
    Plain,
    Emphasis,
    Strong,
    BoldItalic,
}

impl InlineStyle {
    fn with(self, delimiter: Delimiter) -> Self {
        match delimiter {
            Delimiter::Strong => self.with_strong(),
            Delimiter::Emphasis => self.with_emphasis(),
        }
    }

    fn with_strong(self) -> Self {
        match self {
            Self::Plain => Self::Strong,
            Self::Emphasis | Self::Strong | Self::BoldItalic => Self::BoldItalic,
        }
    }

    fn with_emphasis(self) -> Self {
        match self {
            Self::Plain => Self::Emphasis,
            Self::Strong | Self::Emphasis | Self::BoldItalic => Self::BoldItalic,
        }
    }

    fn kind(self) -> InlineKind {
        match self {
            Self::Plain => InlineKind::Text,
            Self::Emphasis => InlineKind::Emphasis,
            Self::Strong => InlineKind::Strong,
            Self::BoldItalic => InlineKind::BoldItalic,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Delimiter {
    Emphasis,
    Strong,
}

impl Delimiter {
    fn width(self) -> usize {
        match self {
            Self::Emphasis => 1,
            Self::Strong => 2,
        }
    }

    fn closing_text(self) -> &'static str {
        match self {
            Self::Emphasis => "*",
            Self::Strong => "**",
        }
    }
}

struct ParsedSegment {
    inlines: Vec<Inline>,
    next: usize,
    closed: Option<ClosedDelimiter>,
}

struct ClosedDelimiter {
    end: usize,
}

impl Parser<'_> {
    /// Tokenize `slice` (whose first byte sits at `base` in `self.src`)
    /// into inline runs. Backtick code and `@label` references are
    /// atomic; emphasis delimiters can nest into bold+italic text runs.
    pub(crate) fn parse_inlines(&mut self, slice: &str, base: usize) -> Vec<Inline> {
        self.parse_inline_segment(slice, base, 0, InlineStyle::default(), None)
            .inlines
    }

    fn parse_inline_segment(
        &mut self,
        slice: &str,
        base: usize,
        from: usize,
        style: InlineStyle,
        close: Option<Delimiter>,
    ) -> ParsedSegment {
        let bytes = slice.as_bytes();
        let mut out: Vec<Inline> = Vec::new();
        // `pending` accumulates characters that belong to the current
        // styled text run but aren't a verbatim slice of `slice`: at
        // the moment, only the soft-hyphen shorthand `\-` (which
        // contributes a U+00AD codepoint without `\` or `-` ever
        // appearing in the run). When `pending` is non-empty, the run
        // can't be captured by a single `slice[start..end]` so we
        // switch to a `String`-buffered flush path. `pending_source_start`
        // remembers the first source byte that fed `pending` so the
        // emitted Inline's span still covers the full source extent
        // (including the consumed `\-` bytes).
        let mut pending: String = String::new();
        let mut pending_source_start: Option<usize> = None;
        let mut i = from;
        let mut text_start = from;
        while i < bytes.len() {
            let c = bytes[i];
            if c == b'\\' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                    self.flush_styled_text_with_pending(
                        &mut out,
                        slice,
                        base,
                        text_start,
                        i,
                        style,
                        &mut pending,
                        &mut pending_source_start,
                    );
                    out.push(Inline {
                        kind: InlineKind::HardBreak,
                        text: String::new(),
                        span: self.span(base + i, base + i + 2),
                        label_span: None,
                    });
                    i += 2;
                    text_start = i;
                    continue;
                }
                if i + 1 < bytes.len() && bytes[i + 1] == b'-' {
                    // `\-` -> literal U+00AD soft hyphen. Splice the
                    // preceding slice text into `pending`, append the
                    // SHY codepoint, skip both source bytes. Remember
                    // the earliest source byte covered by `pending` so
                    // the eventual flush spans the original `\-` bytes
                    // instead of collapsing to a zero-width range.
                    if pending_source_start.is_none() {
                        pending_source_start = Some(text_start);
                    }
                    pending.push_str(&slice[text_start..i]);
                    pending.push('\u{AD}');
                    i += 2;
                    text_start = i;
                    continue;
                }
                if i + 1 < bytes.len() && bytes[i + 1] == b'<' {
                    // `\<` -> literal `<`. Lets authors write angle-bracket text
                    // (e.g. an HTML tag like `<head>`) in prose and headings
                    // without `<...>` being read as label syntax. The heading
                    // label scanners skip an escaped `<` to match.
                    if pending_source_start.is_none() {
                        pending_source_start = Some(text_start);
                    }
                    pending.push_str(&slice[text_start..i]);
                    pending.push('<');
                    i += 2;
                    text_start = i;
                    continue;
                }
                // Backslash followed by anything other than `\`, `-`, or `<`
                // is left to fall through as a literal `\` byte (the
                // slice path picks it up at the next flush). A lone
                // trailing `\` at end-of-input gets a warning so the
                // author notices a likely-incomplete escape; a `\`
                // followed by some other character is kept silent
                // because the previous behaviour was "backslash is
                // literal", and emitting a diagnostic for every
                // `C:\Temp` / `\*foo*` / etc. would be noisy.
                if i + 1 >= bytes.len() {
                    self.diagnostics.push(self.warn(
                        &codes::MOS0038,
                        "lone trailing `\\` is not a recognized escape; treated as literal text",
                        base + i,
                        base + i + 1,
                    ));
                }
                i += 1;
                continue;
            }
            if c == b'*' {
                let run_len = star_run_len(bytes, i);
                if let Some(delimiter) = close
                    && delimiter_closes(delimiter, run_len)
                {
                    self.flush_styled_text_with_pending(
                        &mut out,
                        slice,
                        base,
                        text_start,
                        i,
                        style,
                        &mut pending,
                        &mut pending_source_start,
                    );
                    let width = delimiter.width();
                    return ParsedSegment {
                        inlines: out,
                        next: i + width,
                        closed: Some(ClosedDelimiter { end: i + width }),
                    };
                }

                let delimiter = if run_len >= 2 {
                    Delimiter::Strong
                } else {
                    Delimiter::Emphasis
                };
                let diagnostic_checkpoint = self.diagnostics.len();
                let parsed = self.parse_inline_segment(
                    slice,
                    base,
                    i + delimiter.width(),
                    style.with(delimiter),
                    Some(delimiter),
                );

                if let Some(closed) = parsed.closed {
                    self.flush_styled_text_with_pending(
                        &mut out,
                        slice,
                        base,
                        text_start,
                        i,
                        style,
                        &mut pending,
                        &mut pending_source_start,
                    );
                    let mut children = parsed.inlines;
                    widen_span_to_delimiters(&mut children, base + i, base + closed.end);
                    out.extend(children);
                    i = parsed.next;
                    text_start = i;
                    continue;
                }

                self.diagnostics.truncate(diagnostic_checkpoint);
                if close.is_none() {
                    self.warn_unterminated_delimiter(slice, base, i, delimiter);
                }
                i += delimiter.width();
                continue;
            }
            if c == b'`' {
                if let Some(end) = find_byte(bytes, b'`', i + 1) {
                    self.flush_styled_text_with_pending(
                        &mut out,
                        slice,
                        base,
                        text_start,
                        i,
                        style,
                        &mut pending,
                        &mut pending_source_start,
                    );
                    out.push(Inline {
                        kind: InlineKind::Code,
                        text: slice[i + 1..end].to_owned(),
                        span: self.span(base + i, base + end + 1),
                        label_span: None,
                    });
                    i = end + 1;
                    text_start = i;
                    continue;
                }
                let mut diagnostic = self.warn(
                    &codes::MOS0034,
                    "unterminated `` `code` `` run; treated as text",
                    base + i,
                    base + i + 1,
                );
                if let Some(insertion) = Self::code_closing_insertion(slice, i, close) {
                    let insertion = base + insertion;
                    diagnostic = diagnostic
                        .with_suggestion(Suggestion::new(self.span(insertion, insertion), "`"));
                }
                self.diagnostics.push(diagnostic);
                i += 1;
                continue;
            }
            if c == b'@' {
                let id_end = scan_label_chars(bytes, i + 1);
                if id_end > i + 1 {
                    // `@page(label)`: a page reference. Only a *well-formed*
                    // `@page(` + label + `)` takes this branch; anything else
                    // (`@page` alone, an unterminated `@page(`, `@pages`) falls
                    // through to the ordinary `@label` reference path below.
                    if &slice[i + 1..id_end] == "page"
                        && id_end < bytes.len()
                        && bytes[id_end] == b'('
                    {
                        let label_start = id_end + 1;
                        let label_end = scan_label_chars(bytes, label_start);
                        if label_end > label_start
                            && label_end < bytes.len()
                            && bytes[label_end] == b')'
                        {
                            self.flush_styled_text_with_pending(
                                &mut out,
                                slice,
                                base,
                                text_start,
                                i,
                                style,
                                &mut pending,
                                &mut pending_source_start,
                            );
                            out.push(Inline {
                                kind: InlineKind::PageReference,
                                text: slice[label_start..label_end].to_owned(),
                                span: self.span(base + i, base + label_end + 1),
                                // The label identifier between `@page(` and `)`.
                                label_span: Some(self.span(base + label_start, base + label_end)),
                            });
                            i = label_end + 1;
                            text_start = i;
                            continue;
                        }
                    }
                    self.flush_styled_text_with_pending(
                        &mut out,
                        slice,
                        base,
                        text_start,
                        i,
                        style,
                        &mut pending,
                        &mut pending_source_start,
                    );
                    out.push(Inline {
                        kind: InlineKind::Reference,
                        text: slice[i + 1..id_end].to_owned(),
                        span: self.span(base + i, base + id_end),
                        // The label identifier after the `@` sigil.
                        label_span: Some(self.span(base + i + 1, base + id_end)),
                    });
                    i = id_end;
                    text_start = i;
                    continue;
                }
                self.diagnostics.push(self.warn(
                    &codes::MOS0036,
                    "stray `@` is not followed by a label identifier; treated as text",
                    base + i,
                    base + i + 1,
                ));
                i += 1;
                continue;
            }
            if c == b'[' && i + 1 < bytes.len() && bytes[i + 1] == b'@' {
                // `[@key]`: citation. Only enter the citation branch
                // once we have seen `[@`, so a bare `[` keeps its
                // current literal-text behaviour and never warns.
                let key_start = i + 2;
                let key_end = scan_label_chars(bytes, key_start);
                if key_end > key_start && key_end < bytes.len() && bytes[key_end] == b']' {
                    self.flush_styled_text_with_pending(
                        &mut out,
                        slice,
                        base,
                        text_start,
                        i,
                        style,
                        &mut pending,
                        &mut pending_source_start,
                    );
                    let end = key_end + 1;
                    out.push(Inline {
                        kind: InlineKind::Citation,
                        text: slice[key_start..key_end].to_owned(),
                        span: self.span(base + i, base + end),
                        label_span: None,
                    });
                    i = end;
                    text_start = i;
                    continue;
                }
                // Either the key was empty, the `]` was missing, or
                // the body uses a not-yet-supported form (`[@a; @b]`,
                // prefix/suffix). Warn once and *consume* the
                // citation-candidate extent so the trailing `@key`
                // bytes don't fall back through to the `@`-reference
                // branch; that would surface a bogus MOS0033 in the
                // resolver for what was syntactically a malformed
                // citation, not an unknown label.
                //
                // Recovery extent:
                // * if a `]` exists later in this inline slice,
                //   consume up to and including it (covers
                //   `[@a; @b]`, `[@see @key, p. 33]`, `[@]`);
                // * otherwise skip past `[@` only (covers truly
                //   unterminated `[@key…` at end of paragraph) and
                //   let the bare key chars settle as literal text.
                let recovery_end = if let Some(close) = find_byte(bytes, b']', key_start) {
                    close + 1
                } else {
                    key_start
                };
                self.diagnostics.push(self.warn(
                    &codes::MOS0039,
                    "malformed citation `[@…]`; expected `[@key]`; treated as text",
                    base + i,
                    base + recovery_end,
                ));
                i = recovery_end;
                continue;
            }
            i += 1;
        }
        self.flush_styled_text_with_pending(
            &mut out,
            slice,
            base,
            text_start,
            bytes.len(),
            style,
            &mut pending,
            &mut pending_source_start,
        );
        ParsedSegment {
            inlines: out,
            next: bytes.len(),
            closed: None,
        }
    }

    /// Flush `slice[from..to]` (possibly prefixed by buffered `pending`
    /// text from earlier escape expansions like `\-` → U+00AD) into a
    /// single styled-text inline. The span covers the full source range
    /// from the earliest byte that fed `pending` (or `from` when pending
    /// is empty) through `to`, so emitted inlines whose text includes
    /// expanded escapes still carry a span covering the original source
    /// bytes: including the consumed `\-` markers.
    #[allow(
        clippy::too_many_arguments,
        reason = "transitional: extends the existing `flush_styled_text` (7-arg) with a buffered-text channel and a pending-source-start tracker for escape expansion. Bundling the slice/base/style triple into a context struct would churn every call site in `parse_inline_segment` for no net clarity."
    )]
    fn flush_styled_text_with_pending(
        &self,
        out: &mut Vec<Inline>,
        slice: &str,
        base: usize,
        from: usize,
        to: usize,
        style: InlineStyle,
        pending: &mut String,
        pending_source_start: &mut Option<usize>,
    ) {
        if pending.is_empty() {
            // Defensive: pending_source_start should always be paired
            // with a non-empty pending. Clear it anyway so a future
            // escape that splices into `pending` starts from a fresh
            // state.
            *pending_source_start = None;
            self.flush_styled_text(out, slice, base, from, to, style);
            return;
        }
        let mut text = std::mem::take(pending);
        if from < to {
            text.push_str(&slice[from..to]);
        }
        let span_from = pending_source_start.take().unwrap_or(from);
        out.push(Inline {
            kind: style.kind(),
            text,
            span: self.span(base + span_from, base + to),
            label_span: None,
        });
    }

    fn flush_styled_text(
        &self,
        out: &mut Vec<Inline>,
        slice: &str,
        base: usize,
        from: usize,
        to: usize,
        style: InlineStyle,
    ) {
        if from < to {
            out.push(Inline {
                kind: style.kind(),
                text: slice[from..to].to_owned(),
                span: self.span(base + from, base + to),
                label_span: None,
            });
        }
    }

    fn warn_unterminated_delimiter(
        &mut self,
        slice: &str,
        base: usize,
        i: usize,
        delimiter: Delimiter,
    ) {
        let (def, message) = match delimiter {
            Delimiter::Strong => (
                &codes::MOS0028,
                "unterminated `**strong**` run; treated as text",
            ),
            Delimiter::Emphasis => (
                &codes::MOS0031,
                "unterminated `*emphasis*` run; treated as text",
            ),
        };
        let mut diagnostic = self.warn(def, message, base + i, base + i + delimiter.width());
        if let Some(suggestion) = self.closing_delimiter_suggestion(slice, base, i, delimiter) {
            diagnostic = diagnostic.with_suggestion(suggestion);
        }
        self.diagnostics.push(diagnostic);
    }

    fn closing_delimiter_suggestion(
        &self,
        slice: &str,
        base: usize,
        i: usize,
        delimiter: Delimiter,
    ) -> Option<Suggestion> {
        let after_opener = i + delimiter.width();
        if slice.as_bytes()[after_opener..].contains(&b'*') {
            return None;
        }
        let insertion = base + slice.len();
        Some(Suggestion::new(
            self.span(insertion, insertion),
            delimiter.closing_text(),
        ))
    }

    fn code_closing_insertion(slice: &str, i: usize, close: Option<Delimiter>) -> Option<usize> {
        let bytes = slice.as_bytes();
        let mut cursor = i + 1;
        while cursor < bytes.len() {
            if bytes[cursor] == b'*' {
                let run_len = star_run_len(bytes, cursor);
                if close.is_some_and(|delimiter| delimiter_closes(delimiter, run_len)) {
                    return Some(cursor);
                }
                return None;
            }
            cursor += 1;
        }
        Some(bytes.len())
    }
}

fn star_run_len(bytes: &[u8], from: usize) -> usize {
    let mut end = from;
    while end < bytes.len() && bytes[end] == b'*' {
        end += 1;
    }
    end - from
}

fn delimiter_closes(delimiter: Delimiter, run_len: usize) -> bool {
    match delimiter {
        Delimiter::Strong => run_len >= 2,
        Delimiter::Emphasis => run_len % 2 == 1,
    }
}

fn widen_span_to_delimiters(inlines: &mut [Inline], start: usize, end: usize) {
    if let Some(first) = inlines.first_mut() {
        first.span.set_start(start);
    }
    if let Some(last) = inlines.last_mut() {
        last.span.set_end(end);
    }
}
