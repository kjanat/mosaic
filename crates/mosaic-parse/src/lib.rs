//! Parser for the Mosaic source language (`.mos`).
//!
//! See manifest §3 (language design) and §6 stages 1–2 (parse + lower).
//! Currently covers:
//!
//! - `= Heading` / `== Subheading` / `=== Subsubheading`,
//! - paragraphs (newline-joined non-empty line groups),
//! - inline `*emphasis*`, `**strong**`, and `` `inline code` ``,
//! - `#set name(...)` blocks, recorded with span and name but interpreted
//!   later by the evaluator,
//! - `#image(...)` and `#figure(...)` directives, sharing the same
//!   `key: value` body grammar as `#set` plus an optional leading
//!   positional string literal (`#image("path.png")`),
//! - raw `#pre[...]` and `#code[...]` blocks,
//! - `<label>` attached to the preceding block (trailing on a heading or
//!   leading on a paragraph), and `@label` cross-references as inline
//!   [`InlineKind::Reference`] runs (manifest §3.3 and the MVP 1
//!   resolver).
//!
//! Anything outside that subset is preserved as text and a recoverable
//! diagnostic is emitted; the parser never panics on user input
//! (manifest §31).

use std::path::{Path, PathBuf};

use mosaic_core::{Diagnostic, DiagnosticCode, Severity, SourceSpan};

/// Concrete syntax tree for a single `.mos` source file.
#[derive(Debug, Clone)]
pub struct SyntaxTree {
    pub file: PathBuf,
    pub items: Vec<Item>,
}

/// Top-level construct in a `.mos` file.
#[derive(Debug, Clone)]
pub enum Item {
    /// `= Title`, `== Subtitle`, `=== Subsubtitle`. A trailing
    /// `<label>` token after the title attaches to this heading.
    Heading {
        level: u8,
        inlines: Vec<Inline>,
        label: Option<String>,
        span: SourceSpan,
    },
    /// One or more consecutive non-blank lines that are not a heading
    /// and not a `#set` block. A leading `<label>` token (possibly
    /// preceded by ASCII whitespace) attaches to this paragraph.
    Paragraph {
        inlines: Vec<Inline>,
        label: Option<String>,
        span: SourceSpan,
    },
    /// `#set name(...)`, `#image(...)`, `#figure(...)`. The body is
    /// lexed into typed `(key, value)` args; semantic validation
    /// (known target/key, type coercion, sanity floors) happens in
    /// the lowerer. `kind` distinguishes the `#set`-style configuration
    /// directive from standalone calls like `#image` and `#figure`,
    /// which the lowerer dispatches to dedicated paths.
    Set {
        kind: DirectiveKind,
        name: String,
        args: Vec<SetArg>,
        span: SourceSpan,
    },
    /// Raw preformatted text or code block. Both forms preserve their
    /// bracket body as text; the kind leaves room for later styling or
    /// language-aware code rendering.
    RawBlock {
        kind: RawBlockKind,
        text: String,
        span: SourceSpan,
    },
    /// A bullet (`- `) or numbered (`\d+\. `) list. Sibling items at
    /// the same indent are grouped under one list; deeper indents
    /// become nested lists hanging off the most recent item. Numbered
    /// lists always renumber from 1 in MVP — explicit `start: N` is
    /// deferred.
    List {
        ordered: bool,
        items: Vec<ListItem>,
        span: SourceSpan,
    },
}

/// One entry inside an [`Item::List`]. `inlines` is the item's own
/// text (markers stripped, parsed with the same inline tokenizer as
/// paragraphs); `children` carries nested blocks, currently restricted
/// to further [`Item::List`]s per the MVP scope.
#[derive(Debug, Clone)]
pub struct ListItem {
    pub inlines: Vec<Inline>,
    pub children: Vec<Item>,
    pub span: SourceSpan,
}

/// Tag for the three directive shapes [`Item::Set`] can represent —
/// the `#set <target>(...)` configuration directive vs the standalone
/// `#image(...)` and `#figure(...)` calls. The lowerer dispatches on
/// this rather than the [`Item::Set::name`] string so `#set image(...)`
/// can never collide with `#image(...)`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DirectiveKind {
    /// `#set <name>(...)` — sets defaults on a style target.
    Set,
    /// `#image("path", ...)` — raster image directive.
    Image,
    /// `#figure(image: ..., caption: ...)` — captioned image container.
    Figure,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RawBlockKind {
    Pre,
    Code,
}

/// One argument inside a directive body — either a `key: value`
/// pair (the only form `#set` accepts) or a positional value (a
/// leading string literal allowed on `#image(...)` / `#figure(...)`).
///
/// This used to be a struct with an empty-string `key` standing in
/// for "positional," but that sentinel was a brittle public contract:
/// any consumer that forgot the special-case would silently treat a
/// positional path as a named arg called `""`. The enum form makes
/// the two shapes explicit so the compiler can enforce exhaustive
/// matches.
#[derive(Debug, Clone)]
pub enum SetArg {
    /// A `key: value` argument. `key_span` covers the identifier
    /// before the colon; `value_span` covers the literal.
    Named {
        key: String,
        value: SetValue,
        key_span: SourceSpan,
        value_span: SourceSpan,
    },
    /// A leading positional value. The parser currently only accepts
    /// string literals here (used for `#image("path.png")`); other
    /// literal kinds in a positional slot would surface as a parse
    /// error rather than land in this variant.
    Positional {
        value: SetValue,
        value_span: SourceSpan,
    },
}

impl SetArg {
    /// Borrow the value carried by this argument, regardless of shape.
    #[must_use]
    pub fn value(&self) -> &SetValue {
        match self {
            Self::Named { value, .. } | Self::Positional { value, .. } => value,
        }
    }

    /// The span covering the argument's value literal.
    #[must_use]
    pub fn value_span(&self) -> &SourceSpan {
        match self {
            Self::Named { value_span, .. } | Self::Positional { value_span, .. } => value_span,
        }
    }

    /// The key identifier for [`Self::Named`]; `None` for
    /// [`Self::Positional`].
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        match self {
            Self::Named { key, .. } => Some(key.as_str()),
            Self::Positional { .. } => None,
        }
    }

    /// The span covering the key identifier, for [`Self::Named`].
    /// `None` for [`Self::Positional`].
    #[must_use]
    pub fn key_span(&self) -> Option<&SourceSpan> {
        match self {
            Self::Named { key_span, .. } => Some(key_span),
            Self::Positional { .. } => None,
        }
    }
}

/// Literal values recognised inside a `#set` body. Full expression
/// evaluation (`#let`, function calls, `if`) is deferred to MVP 5; this
/// covers what the manifest examples actually use.
#[derive(Debug, Clone, PartialEq)]
pub enum SetValue {
    Str(String),
    Int(i64),
    Float(f64),
    Length(f64, LengthUnit),
    Ident(String),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LengthUnit {
    Mm,
    Pt,
    Em,
}

/// Inline run produced by the markup tokenizer.
#[derive(Debug, Clone)]
pub struct Inline {
    pub kind: InlineKind,
    pub text: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InlineKind {
    Text,
    Emphasis,
    Strong,
    Code,
    /// `@label` — a cross-reference to a labelled block. The
    /// [`Inline::text`] payload is the bare label name (no leading
    /// `@`); the resolver rewrites it to the target's resolved text.
    Reference,
}

impl Item {
    /// Borrow the heading payload if `self` is [`Item::Heading`].
    #[must_use]
    pub fn as_heading(&self) -> Option<(u8, &[Inline], &SourceSpan)> {
        if let Self::Heading {
            level,
            inlines,
            span,
            ..
        } = self
        {
            Some((*level, inlines, span))
        } else {
            None
        }
    }

    /// Borrow the paragraph payload if `self` is [`Item::Paragraph`].
    #[must_use]
    pub fn as_paragraph(&self) -> Option<(&[Inline], &SourceSpan)> {
        if let Self::Paragraph { inlines, span, .. } = self {
            Some((inlines, span))
        } else {
            None
        }
    }

    /// Borrow the directive payload if `self` is [`Item::Set`].
    ///
    /// The returned tuple is `(name, args, span)`; the caller can also
    /// reach [`DirectiveKind`] via [`Self::directive_kind`]. The
    /// accessor name is retained for back-compat — every existing
    /// caller pre-dates the `#image`/`#figure` directives and only
    /// looks at name/args/span.
    #[must_use]
    pub fn as_set(&self) -> Option<(&str, &[SetArg], &SourceSpan)> {
        if let Self::Set {
            name, args, span, ..
        } = self
        {
            Some((name.as_str(), args.as_slice(), span))
        } else {
            None
        }
    }

    /// Borrow the [`DirectiveKind`] tag if `self` is [`Item::Set`].
    #[must_use]
    pub fn directive_kind(&self) -> Option<DirectiveKind> {
        if let Self::Set { kind, .. } = self {
            Some(*kind)
        } else {
            None
        }
    }

    /// Borrow the list payload if `self` is [`Item::List`]. The
    /// returned tuple is `(ordered, items, span)`.
    #[must_use]
    pub fn as_list(&self) -> Option<(bool, &[ListItem], &SourceSpan)> {
        if let Self::List {
            ordered,
            items,
            span,
        } = self
        {
            Some((*ordered, items.as_slice(), span))
        } else {
            None
        }
    }

    /// Borrow the explicit `<label>` attached to this block, if any.
    /// Returns `None` for [`Item::Set`], [`Item::RawBlock`], and [`Item::List`] (label
    /// syntax is not yet defined on those blocks).
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Heading { label, .. } | Self::Paragraph { label, .. } => label.as_deref(),
            Self::Set { .. } | Self::RawBlock { .. } | Self::List { .. } => None,
        }
    }
}

/// Output of [`parse`]. Diagnostics may include warnings even when the
/// tree is structurally usable; callers decide what to do per
/// [`ParseResult::has_errors`].
#[derive(Debug)]
pub struct ParseResult {
    pub tree: SyntaxTree,
    pub diagnostics: Vec<Diagnostic>,
}

impl ParseResult {
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// Parse a Mosaic source string. Always returns a [`ParseResult`]; the
/// parser is recoverable per manifest §6 stage 1.
#[must_use]
pub fn parse(src: &str, file: &Path) -> ParseResult {
    Parser::new(src, file).run()
}

struct Parser<'a> {
    src: &'a str,
    file: PathBuf,
    pos: usize,
    items: Vec<Item>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, file: &Path) -> Self {
        Self {
            src,
            file: file.to_path_buf(),
            pos: 0,
            items: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self) -> ParseResult {
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

    fn at_list_marker(&self) -> bool {
        list_marker_at(self.src.as_bytes(), self.pos).is_some()
    }

    fn span(&self, start: usize, end: usize) -> SourceSpan {
        SourceSpan::new(self.file.clone(), start, end)
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.src.as_bytes()[self.pos..].starts_with(prefix.as_bytes())
    }

    /// Returns the matched keyword (`"set"`, `"image"`, `"figure"`,
    /// `"pre"`, `"code"`) if
    /// the current position spells one of the recognised directive
    /// keywords followed by a token boundary (whitespace, EOF, or `(`).
    /// The boundary check guards against prefixes like `#setting` or
    /// `#imagery` being misrouted into the directive path.
    fn at_directive_keyword(&self) -> Option<&'static str> {
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

    /// Returns true if the current line is blank (contains only ASCII
    /// whitespace before the line terminator).
    fn at_blank_line(&self) -> bool {
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

    /// Advance past the next `\n`, or to end-of-source if none remains.
    fn skip_line(&mut self) {
        let bytes = self.src.as_bytes();
        while self.pos < bytes.len() && bytes[self.pos] != b'\n' {
            self.pos += 1;
        }
        if self.pos < bytes.len() {
            self.pos += 1; // consume '\n'
        }
    }

    /// Returns the byte offsets `(content_start, content_end, line_end)`
    /// of the current line. `content_end` excludes any trailing `\r\n`
    /// or `\n`; `line_end` is the offset *after* the terminator.
    fn current_line_bounds(&self) -> (usize, usize, usize) {
        let bytes = self.src.as_bytes();
        let start = self.pos;
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

    fn parse_heading(&mut self) {
        let (line_start, content_end, line_end) = self.current_line_bounds();
        let bytes = self.src.as_bytes();
        let mut level: u8 = 0;
        let mut i = line_start;
        while i < content_end && bytes[i] == b'=' {
            // Cap heading level at 6 (manifest §3.1 only spells three out
            // explicitly, but `======` is a perfectly reasonable extension).
            level = level.saturating_add(1);
            i += 1;
        }
        // Require at least one whitespace after the `=` run, otherwise
        // it isn't a heading — treat the line as a paragraph instead.
        if i >= content_end || !bytes[i].is_ascii_whitespace() {
            self.parse_paragraph();
            return;
        }
        // Skip leading spaces of the heading content.
        while i < content_end && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        // Strip a trailing `<label>` (with optional leading whitespace
        // before it) off the heading content. The label attaches to
        // *this* heading; only the text before it goes through the
        // inline tokenizer.
        let (text_end, label) = strip_trailing_label(self.src, i, content_end);
        let content = &self.src[i..text_end];
        let inlines = self.parse_inlines(content, i);
        let span = self.span(line_start, content_end);
        self.items.push(Item::Heading {
            level,
            inlines,
            label,
            span,
        });
        self.pos = line_end;
    }

    /// Parse a `#<kw>(...)` directive block where `kw` is one of the
    /// keywords matched by [`Self::at_directive_keyword`].
    ///
    /// `#set` is the only directive that carries a separate inner
    /// identifier (`#set page(...)`); for `#image` and `#figure` the
    /// directive keyword itself is the [`Item::Set::name`] payload.
    /// `#pre[...]` and `#code[...]` carry raw bracket bodies.
    fn parse_directive_block(&mut self, kw: &'static str) {
        if kw == "set" {
            self.parse_set_block();
        } else if kw == "pre" || kw == "code" {
            self.parse_raw_block(kw);
        } else {
            self.parse_call_block(kw);
        }
    }

    fn parse_raw_block(&mut self, kw: &'static str) {
        let (line_start, _content_end, _line_end) = self.current_line_bounds();
        let bytes = self.src.as_bytes();
        debug_assert!(self.src[line_start + 1..].starts_with(kw));
        let mut i = line_start + 1 + kw.len();
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'[' {
            self.diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E011"),
                    format!("expected `[` after `#{kw}`"),
                )
                .with_span(self.span(line_start, i)),
            );
            self.skip_line();
            return;
        }
        if let Some(end) = self.scan_raw_brackets(i) {
            let inner_start = i + 1;
            let inner_end = end - 1;
            let text = self.src[inner_start..inner_end].to_owned();
            let kind = if kw == "code" {
                RawBlockKind::Code
            } else {
                RawBlockKind::Pre
            };
            self.items.push(Item::RawBlock {
                kind,
                text,
                span: self.span(line_start, end),
            });
            self.pos = end;
            while self.pos < bytes.len() && (bytes[self.pos] == b' ' || bytes[self.pos] == b'\t') {
                self.pos += 1;
            }
            if self.pos >= bytes.len() {
                // EOF — nothing to do.
            } else if bytes[self.pos] == b'\n' {
                self.pos += 1;
            } else if bytes[self.pos] == b'\r' && bytes.get(self.pos + 1) == Some(&b'\n') {
                self.pos += 2;
            } else {
                let (_, content_end, _) = self.current_line_bounds();
                self.diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E013"),
                        format!("unexpected trailing content after `#{kw}[...]`"),
                    )
                    .with_span(self.span(self.pos, content_end)),
                );
            }
        } else {
            self.diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E012"),
                    format!("unterminated `#{kw}[...]` block"),
                )
                .with_span(self.span(line_start, bytes.len())),
            );
            self.pos = bytes.len();
        }
    }

    fn parse_set_block(&mut self) {
        let (line_start, _content_end, _line_end) = self.current_line_bounds();
        let bytes = self.src.as_bytes();
        debug_assert!(self.src[line_start..].starts_with("#set"));
        let mut i = line_start + "#set".len();
        // Whitespace before name.
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        let name_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        let name = self.src[name_start..i].to_owned();
        if name.is_empty() {
            self.diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E010"),
                    "expected an identifier after `#set`",
                )
                .with_span(self.span(line_start, line_start + "#set".len())),
            );
            self.skip_line();
            return;
        }
        // Whitespace before `(`.
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'(' {
            self.diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E011"),
                    format!("expected `(` after `#set {name}`"),
                )
                .with_span(self.span(name_start, i)),
            );
            self.skip_line();
            return;
        }
        self.finish_directive_block(line_start, i, DirectiveKind::Set, name, "set", false);
    }

    /// Parse a directive whose keyword stands on its own — `#image(...)`,
    /// `#figure(...)` — so there is no inner identifier to consume.
    fn parse_call_block(&mut self, kw: &'static str) {
        let (line_start, _content_end, _line_end) = self.current_line_bounds();
        let bytes = self.src.as_bytes();
        debug_assert!(self.src[line_start + 1..].starts_with(kw));
        let mut i = line_start + 1 + kw.len();
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'(' {
            self.diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E011"),
                    format!("expected `(` after `#{kw}`"),
                )
                .with_span(self.span(line_start, i)),
            );
            self.skip_line();
            return;
        }
        let kind = match kw {
            "image" => DirectiveKind::Image,
            "figure" => DirectiveKind::Figure,
            // `at_directive_keyword` returns only the strings handled
            // above. Reaching this arm would indicate a parser-internal
            // mismatch between the keyword recogniser and the dispatch
            // table — fall back to `Set` rather than panicking in
            // release so user input can never crash the parser, but
            // a `debug_assert!` fires in tests.
            other => {
                debug_assert!(false, "parse_call_block: unexpected keyword `{other}`");
                DirectiveKind::Set
            }
        };
        self.finish_directive_block(line_start, i, kind, kw.to_owned(), kw, true);
    }

    /// Shared tail of [`Self::parse_set_block`] and
    /// [`Self::parse_call_block`]: consume the balanced parenthesised
    /// body starting at `paren_pos`, push the resulting [`Item::Set`],
    /// and report any trailing-content diagnostic. `display_kw` is the
    /// keyword spelling used in user-facing messages (`set`, `image`,
    /// `figure`); for `#set` it's "set" and the directive's own inner
    /// identifier already appears in `name`. `allow_positional` controls
    /// whether a leading positional string literal may appear; `#set`
    /// strictly requires `key: value` pairs (so a stray positional is
    /// an E015), but `#image("path.png")` is a common ergonomic spelling.
    fn finish_directive_block(
        &mut self,
        line_start: usize,
        paren_pos: usize,
        kind: DirectiveKind,
        name: String,
        display_kw: &str,
        allow_positional: bool,
    ) {
        let bytes = self.src.as_bytes();
        let body_start = paren_pos;
        if let Some(end) = self.scan_balanced_parens(body_start) {
            let inner_start = body_start + 1;
            let inner_end = end - 1;
            let args = self.parse_set_body(inner_start, inner_end, allow_positional);
            let span = self.span(line_start, end);
            self.items.push(Item::Set {
                kind,
                name,
                args,
                span,
            });
            self.pos = end;
            while self.pos < bytes.len() && (bytes[self.pos] == b' ' || bytes[self.pos] == b'\t') {
                self.pos += 1;
            }
            if self.pos >= bytes.len() {
                // EOF — nothing to do.
            } else if bytes[self.pos] == b'\n' {
                self.pos += 1;
            } else if bytes[self.pos] == b'\r' && bytes.get(self.pos + 1) == Some(&b'\n') {
                self.pos += 2;
            } else {
                let (_, content_end, _) = self.current_line_bounds();
                self.diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E013"),
                        format!("unexpected trailing content after `#{display_kw} ... )`"),
                    )
                    .with_span(self.span(self.pos, content_end)),
                );
            }
        } else {
            self.diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E012"),
                    format!("unterminated `#{display_kw}(...)` block"),
                )
                .with_span(self.span(line_start, bytes.len())),
            );
            self.pos = bytes.len();
        }
    }

    /// Starting at `start` (which must point at `(`), find the byte
    /// offset *after* the matching `)`. Tracks string literals so that
    /// `(` / `)` inside `"..."` don't fool the scanner. Returns `None`
    /// if no matching `)` exists before EOF.
    fn scan_balanced_parens(&self, start: usize) -> Option<usize> {
        let bytes = self.src.as_bytes();
        debug_assert_eq!(bytes.get(start), Some(&b'('));
        let mut depth: u32 = 0;
        let mut i = start;
        let mut in_string = false;
        while i < bytes.len() {
            let b = bytes[i];
            if in_string {
                if b == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if b == b'"' {
                    in_string = false;
                }
                i += 1;
                continue;
            }
            match b {
                b'"' => in_string = true,
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i + 1);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    fn scan_raw_brackets(&self, start: usize) -> Option<usize> {
        let bytes = self.src.as_bytes();
        debug_assert_eq!(bytes.get(start), Some(&b'['));
        let mut i = start + 1;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' if i + 1 < bytes.len() => i += 2,
                b']' => return Some(i + 1),
                _ => i += 1,
            }
        }
        None
    }

    /// Lex `src[start..end]` (the contents of `#<kw>( ... )`) into
    /// a list of arguments. Most arguments are `key: value` pairs;
    /// directive callers (`#image`, `#figure`) may also pass a leading
    /// positional value, which is stored as a [`SetArg`] with an empty
    /// `key` field. Recoverable: emits `E014`/`E015` for malformed
    /// literals or structural problems and continues scanning so a
    /// single typo doesn't drop the whole block.
    fn parse_set_body(&mut self, start: usize, end: usize, allow_positional: bool) -> Vec<SetArg> {
        let bytes = self.src.as_bytes();
        let mut args: Vec<SetArg> = Vec::new();
        let mut i = start;
        let mut first = true;
        loop {
            i = skip_set_ws(bytes, i, end);
            if i >= end {
                break;
            }
            // Positional argument: only allowed as the very first
            // entry, and only when the value is a string literal. This
            // is enough to spell `#image("path.png")` ergonomically
            // without inventing a whole positional-argument grammar;
            // the lowerer rejects unexpected positional args per
            // directive.
            if allow_positional && first && bytes[i] == b'"' {
                let value_start = i;
                let parsed = self.parse_set_value(&mut i, end);
                let value_span = self.span(value_start, i);
                if let Some(value) = parsed {
                    args.push(SetArg::Positional { value, value_span });
                }
                first = false;
                i = skip_set_ws(bytes, i, end);
                if i < end {
                    if bytes[i] == b',' {
                        i += 1;
                    } else {
                        self.diagnostics.push(
                            Diagnostic::error(
                                DiagnosticCode("E015"),
                                "expected `,` or `)` between directive arguments",
                            )
                            .with_span(self.span(i, (i + 1).min(end))),
                        );
                        i = skip_to_comma(bytes, i, end);
                        if i < end && bytes[i] == b',' {
                            i += 1;
                        }
                    }
                }
                continue;
            }
            first = false;
            // Key.
            let key_start = i;
            while i < end && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'-')) {
                i += 1;
            }
            if i == key_start {
                self.diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E015"),
                        "expected `key: value` in directive arguments",
                    )
                    .with_span(self.span(i, (i + 1).min(end))),
                );
                // Skip to next comma to recover.
                i = skip_to_comma(bytes, i, end);
                if i < end && bytes[i] == b',' {
                    i += 1;
                }
                continue;
            }
            let key = self.src[key_start..i].to_owned();
            let key_span = self.span(key_start, i);
            i = skip_set_ws(bytes, i, end);
            if i >= end || bytes[i] != b':' {
                self.diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E015"),
                        format!("expected `:` after `{key}` in directive arguments"),
                    )
                    .with_span(key_span.clone()),
                );
                i = skip_to_comma(bytes, i, end);
                if i < end && bytes[i] == b',' {
                    i += 1;
                }
                continue;
            }
            i += 1; // consume ':'
            i = skip_set_ws(bytes, i, end);
            // Value.
            let value_start = i;
            let parsed = self.parse_set_value(&mut i, end);
            let value_span = self.span(value_start, i);
            if let Some(value) = parsed {
                args.push(SetArg::Named {
                    key,
                    value,
                    key_span,
                    value_span,
                });
            }
            // Trailing whitespace then optional comma.
            i = skip_set_ws(bytes, i, end);
            if i < end {
                if bytes[i] == b',' {
                    i += 1;
                } else {
                    self.diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode("E015"),
                            "expected `,` or `)` between directive arguments",
                        )
                        .with_span(self.span(i, (i + 1).min(end))),
                    );
                    i = skip_to_comma(bytes, i, end);
                    if i < end && bytes[i] == b',' {
                        i += 1;
                    }
                }
            }
        }
        args
    }

    /// Parse a single literal value starting at `*i`, advancing `*i`
    /// past the consumed bytes. Returns `None` and emits `E014` if the
    /// literal is malformed; `*i` is still advanced past the broken
    /// token so the surrounding loop can resume.
    fn parse_set_value(&mut self, i: &mut usize, end: usize) -> Option<SetValue> {
        let bytes = self.src.as_bytes();
        if *i >= end {
            self.diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode("E014"),
                    "expected a value in directive arguments",
                )
                .with_span(self.span(*i, *i)),
            );
            return None;
        }
        let b = bytes[*i];
        // String literal.
        if b == b'"' {
            let start = *i;
            *i += 1;
            let mut out = String::new();
            while *i < end {
                let c = bytes[*i];
                if c == b'\\' && *i + 1 < end {
                    let esc = bytes[*i + 1];
                    match esc {
                        b'\\' => {
                            out.push('\\');
                            *i += 2;
                        }
                        b'"' => {
                            out.push('"');
                            *i += 2;
                        }
                        b'n' => {
                            out.push('\n');
                            *i += 2;
                        }
                        b't' => {
                            out.push('\t');
                            *i += 2;
                        }
                        b'r' => {
                            out.push('\r');
                            *i += 2;
                        }
                        _ => {
                            // Unknown escape: the byte after `\` may
                            // start a multibyte UTF-8 scalar, so we
                            // can't blindly advance by 2 — that would
                            // strand `*i` mid-codepoint and panic on
                            // the next slice. Walk to the next char
                            // boundary, push the source slice as-is,
                            // and report the offending range.
                            let esc_start = *i + 1;
                            let esc_end = next_char_boundary(self.src, esc_start);
                            self.diagnostics.push(
                                Diagnostic::error(
                                    DiagnosticCode("E014"),
                                    format!(
                                        "unknown escape sequence `\\{}` in string",
                                        &self.src[esc_start..esc_end]
                                    ),
                                )
                                .with_span(self.span(*i, esc_end)),
                            );
                            out.push_str(&self.src[esc_start..esc_end]);
                            *i = esc_end;
                        }
                    }
                    continue;
                }
                if c == b'"' {
                    *i += 1;
                    return Some(SetValue::Str(out));
                }
                // Push raw byte; non-ASCII multi-byte UTF-8 sequences
                // are passed through by accumulating the raw bytes via
                // `str` slicing on each char boundary.
                let ch_start = *i;
                let ch_end = next_char_boundary(self.src, ch_start);
                out.push_str(&self.src[ch_start..ch_end]);
                *i = ch_end;
            }
            self.diagnostics.push(
                Diagnostic::error(DiagnosticCode("E014"), "unterminated string literal")
                    .with_span(self.span(start, end)),
            );
            return None;
        }
        // Number / length literal.
        if b == b'-' || b.is_ascii_digit() {
            let num_start = *i;
            if b == b'-' {
                *i += 1;
            }
            let int_start = *i;
            while *i < end && bytes[*i].is_ascii_digit() {
                *i += 1;
            }
            let mut is_float = false;
            if *i < end && bytes[*i] == b'.' && *i + 1 < end && bytes[*i + 1].is_ascii_digit() {
                is_float = true;
                *i += 1;
                while *i < end && bytes[*i].is_ascii_digit() {
                    *i += 1;
                }
            }
            if *i == int_start {
                self.diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E014"),
                        "expected a number after `-` in directive value",
                    )
                    .with_span(self.span(num_start, *i)),
                );
                return None;
            }
            let num_end = *i;
            // Optional unit suffix.
            let unit_start = *i;
            while *i < end && bytes[*i].is_ascii_alphabetic() {
                *i += 1;
            }
            let unit = &self.src[unit_start..*i];
            if unit.is_empty() {
                let text = &self.src[num_start..num_end];
                if is_float {
                    return text.parse::<f64>().ok().map(SetValue::Float).or_else(|| {
                        self.diagnostics.push(
                            Diagnostic::error(
                                DiagnosticCode("E014"),
                                format!("malformed number `{text}`"),
                            )
                            .with_span(self.span(num_start, num_end)),
                        );
                        None
                    });
                }
                return text.parse::<i64>().ok().map(SetValue::Int).or_else(|| {
                    self.diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode("E014"),
                            format!("malformed integer `{text}`"),
                        )
                        .with_span(self.span(num_start, num_end)),
                    );
                    None
                });
            }
            let length_unit = match unit {
                "mm" => LengthUnit::Mm,
                "pt" => LengthUnit::Pt,
                "em" => LengthUnit::Em,
                _ => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode("E014"),
                            format!("unknown length unit `{unit}` (expected mm, pt, or em)"),
                        )
                        .with_span(self.span(unit_start, *i)),
                    );
                    return None;
                }
            };
            let value = self.src[num_start..num_end].parse::<f64>().ok();
            return value.map(|v| SetValue::Length(v, length_unit)).or_else(|| {
                self.diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode("E014"),
                        format!("malformed length value `{}`", &self.src[num_start..num_end]),
                    )
                    .with_span(self.span(num_start, num_end)),
                );
                None
            });
        }
        // Bare identifier. Allows hyphens for things like `bottom-center`.
        if b.is_ascii_alphabetic() {
            let id_start = *i;
            while *i < end
                && (bytes[*i].is_ascii_alphanumeric() || matches!(bytes[*i], b'_' | b'-'))
            {
                *i += 1;
            }
            return Some(SetValue::Ident(self.src[id_start..*i].to_owned()));
        }
        // Anything else.
        self.diagnostics.push(
            Diagnostic::error(
                DiagnosticCode("E014"),
                format!("unexpected character `{}` in directive value", b as char),
            )
            .with_span(self.span(*i, *i + 1)),
        );
        *i += 1;
        None
    }

    fn parse_paragraph(&mut self) {
        let bytes = self.src.as_bytes();
        let para_start = self.pos;
        let mut para_end = self.pos;
        let mut text_start: Option<usize> = None;
        loop {
            if self.pos >= bytes.len() {
                break;
            }
            if self.at_blank_line() {
                break;
            }
            // A heading, directive, or list marker always begins a
            // fresh block, so they terminate any in-progress paragraph
            // too.
            if self.starts_with("=") && self.heading_level_of_current_line().is_some() {
                break;
            }
            if self.at_directive_keyword().is_some() {
                break;
            }
            if self.at_list_marker() {
                break;
            }
            let (line_start, content_end, line_end) = self.current_line_bounds();
            if text_start.is_none() {
                text_start = Some(line_start);
            }
            para_end = content_end;
            self.pos = line_end;
        }
        if let Some(start) = text_start {
            // Slice the paragraph directly out of `self.src` so byte
            // offsets stay aligned with the original file. Building a
            // separate buffer with synthetic `\n` separators would
            // shift inline spans by one byte per CRLF line ending.
            //
            // A leading `<label>` (after optional whitespace) attaches
            // to the paragraph rather than rendering as text — peel it
            // off before tokenizing inlines.
            let (body_start, label) = strip_leading_label(self.src, start, para_end);
            let slice = &self.src[body_start..para_end];
            let mut inlines = self.parse_inlines(slice, body_start);
            // Spans stay anchored to the raw source, but the displayed
            // text payload is platform-stable: normalize `\r\n` → `\n`
            // so the same logical paragraph lowers identically on
            // Windows and Unix sources.
            for inline in &mut inlines {
                if inline.text.contains("\r\n") {
                    inline.text = inline.text.replace("\r\n", "\n");
                }
            }
            let span = self.span(para_start, para_end);
            self.items.push(Item::Paragraph {
                inlines,
                label,
                span,
            });
        }
    }

    /// Consume a contiguous run of list-marker lines starting at the
    /// current position and push one or more [`Item::List`] entries
    /// onto `self.items`. Sibling lines at the same indent and same
    /// marker kind share a list; a switch from ordered to unordered
    /// (or vice versa) at the same indent splits into two adjacent
    /// lists. Deeper-indented marker lines become nested lists on the
    /// most recent item.
    fn parse_list(&mut self) {
        let raw = self.collect_list_lines();
        if raw.is_empty() {
            return;
        }
        let mut i = 0;
        while i < raw.len() {
            let (item, new_i) = self.build_list_at(&raw, i);
            self.items.push(item);
            i = new_i;
        }
    }

    fn collect_list_lines(&mut self) -> Vec<RawListLine> {
        let bytes = self.src.as_bytes();
        let mut out: Vec<RawListLine> = Vec::new();
        while self.pos < bytes.len() {
            if self.at_blank_line() {
                break;
            }
            let Some((indent, ordered, content_start)) = list_marker_at(bytes, self.pos) else {
                break;
            };
            let (line_start, content_end, line_end) = self.current_line_bounds();
            out.push(RawListLine {
                indent,
                ordered,
                content_start,
                content_end,
                line_start,
            });
            self.pos = line_end;
        }
        out
    }

    /// Build one list from the run starting at `raw[start]`. Returns
    /// the assembled [`Item::List`] together with the index in `raw`
    /// where the caller should resume — either the first sibling at a
    /// smaller indent or a same-indent run of the opposite marker
    /// kind.
    fn build_list_at(&mut self, raw: &[RawListLine], start: usize) -> (Item, usize) {
        let base_indent = raw[start].indent;
        let base_ordered = raw[start].ordered;
        let mut items: Vec<ListItem> = Vec::new();
        let mut last_end = raw[start].content_end;
        let mut i = start;
        while i < raw.len() {
            let cur = &raw[i];
            // A shallower indent belongs to an outer scope; a deeper
            // indent here means we entered a nested block that the
            // previous iteration's recursion should have already
            // swallowed — guard anyway so a malformed input can't
            // wedge the loop.
            if cur.indent != base_indent {
                break;
            }
            if cur.ordered != base_ordered {
                break;
            }
            let slice = &self.src[cur.content_start..cur.content_end];
            let inlines = self.parse_inlines(slice, cur.content_start);
            let item_span = self.span(cur.line_start, cur.content_end);
            let mut item = ListItem {
                inlines,
                children: Vec::new(),
                span: item_span,
            };
            last_end = last_end.max(cur.content_end);
            i += 1;
            while i < raw.len() && raw[i].indent > base_indent {
                let (nested, new_i) = self.build_list_at(raw, i);
                if let Item::List { span, .. } = &nested {
                    last_end = last_end.max(span.end);
                }
                item.children.push(nested);
                i = new_i;
            }
            items.push(item);
        }
        let span = self.span(raw[start].line_start, last_end);
        (
            Item::List {
                ordered: base_ordered,
                items,
                span,
            },
            i,
        )
    }

    /// Returns `Some(level)` if the current line is a well-formed
    /// heading of `=`+ followed by ASCII whitespace. Used to terminate
    /// paragraphs without consuming.
    fn heading_level_of_current_line(&self) -> Option<u8> {
        let (start, content_end, _) = self.current_line_bounds();
        let bytes = self.src.as_bytes();
        let mut i = start;
        let mut level: u8 = 0;
        while i < content_end && bytes[i] == b'=' {
            level = level.saturating_add(1);
            i += 1;
        }
        if level == 0 {
            return None;
        }
        if i < content_end && bytes[i].is_ascii_whitespace() {
            Some(level)
        } else {
            None
        }
    }

    /// Tokenize `slice` (whose first byte sits at `base` in `self.src`)
    /// into inline runs. Inline parsing is non-nesting in MVP 0; the
    /// inner contents of `*…*`, `**…**`, and `` `…` `` are plain text.
    fn parse_inlines(&mut self, slice: &str, base: usize) -> Vec<Inline> {
        let bytes = slice.as_bytes();
        let mut out: Vec<Inline> = Vec::new();
        let mut i = 0;
        let mut text_start = 0;
        while i < bytes.len() {
            let c = bytes[i];
            if c == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                if let Some(end) = find_subslice(bytes, b"**", i + 2) {
                    self.flush_text(&mut out, slice, base, text_start, i);
                    out.push(Inline {
                        kind: InlineKind::Strong,
                        text: slice[i + 2..end].to_owned(),
                        span: self.span(base + i, base + end + 2),
                    });
                    i = end + 2;
                    text_start = i;
                    continue;
                }
                self.diagnostics.push(self.warn(
                    "W020",
                    "unterminated `**strong**` run; treated as text",
                    base + i,
                    base + i + 2,
                ));
                i += 2;
                continue;
            }
            if c == b'*' {
                if let Some(end) = find_emphasis_close(bytes, i + 1) {
                    self.flush_text(&mut out, slice, base, text_start, i);
                    out.push(Inline {
                        kind: InlineKind::Emphasis,
                        text: slice[i + 1..end].to_owned(),
                        span: self.span(base + i, base + end + 1),
                    });
                    i = end + 1;
                    text_start = i;
                    continue;
                }
                self.diagnostics.push(self.warn(
                    "W021",
                    "unterminated `*emphasis*` run; treated as text",
                    base + i,
                    base + i + 1,
                ));
                i += 1;
                continue;
            }
            if c == b'`' {
                if let Some(end) = find_byte(bytes, b'`', i + 1) {
                    self.flush_text(&mut out, slice, base, text_start, i);
                    out.push(Inline {
                        kind: InlineKind::Code,
                        text: slice[i + 1..end].to_owned(),
                        span: self.span(base + i, base + end + 1),
                    });
                    i = end + 1;
                    text_start = i;
                    continue;
                }
                self.diagnostics.push(self.warn(
                    "W022",
                    "unterminated `` `code` `` run; treated as text",
                    base + i,
                    base + i + 1,
                ));
                i += 1;
                continue;
            }
            if c == b'@' {
                let id_end = scan_label_chars(bytes, i + 1);
                if id_end > i + 1 {
                    self.flush_text(&mut out, slice, base, text_start, i);
                    out.push(Inline {
                        kind: InlineKind::Reference,
                        text: slice[i + 1..id_end].to_owned(),
                        span: self.span(base + i, base + id_end),
                    });
                    i = id_end;
                    text_start = i;
                    continue;
                }
                self.diagnostics.push(self.warn(
                    "W023",
                    "stray `@` is not followed by a label identifier; treated as text",
                    base + i,
                    base + i + 1,
                ));
                i += 1;
                continue;
            }
            i += 1;
        }
        self.flush_text(&mut out, slice, base, text_start, bytes.len());
        out
    }

    fn flush_text(&self, out: &mut Vec<Inline>, slice: &str, base: usize, from: usize, to: usize) {
        if from < to {
            out.push(Inline {
                kind: InlineKind::Text,
                text: slice[from..to].to_owned(),
                span: self.span(base + from, base + to),
            });
        }
    }

    fn warn(&self, code: &'static str, message: &str, start: usize, end: usize) -> Diagnostic {
        Diagnostic {
            severity: Severity::Warning,
            code: DiagnosticCode(code),
            message: message.to_owned(),
            span: Some(self.span(start, end)),
            notes: Vec::new(),
            suggestions: Vec::new(),
        }
    }
}

/// One marker line captured during list collection. Not user-facing —
/// the public AST uses [`ListItem`] after nesting is resolved.
#[derive(Debug, Clone, Copy)]
struct RawListLine {
    /// Byte count of ASCII spaces before the marker.
    indent: usize,
    /// `true` for `\d+\. `, `false` for `- `.
    ordered: bool,
    /// Byte offset (into `Parser::src`) of the first content byte
    /// after the marker and its trailing whitespace.
    content_start: usize,
    /// Byte offset of the line's content end (excluding any `\r\n` or
    /// `\n` terminator).
    content_end: usize,
    /// Byte offset of the start of the line (the first leading-space
    /// byte). Used for the item's `SourceSpan`.
    line_start: usize,
}

/// If the line that starts at `pos` opens with a list marker, return
/// `Some((indent, ordered, content_start))`. `indent` counts the
/// leading ASCII spaces before the marker; `ordered` is `true` for
/// `\d+\. ` and `false` for `- `; `content_start` is the byte offset
/// of the first byte after the marker plus its trailing whitespace
/// run. Tabs are not recognised as either indent or post-marker
/// whitespace in MVP 0.
fn list_marker_at(bytes: &[u8], pos: usize) -> Option<(usize, bool, usize)> {
    let mut i = pos;
    let mut indent = 0_usize;
    while i < bytes.len() && bytes[i] == b' ' {
        indent += 1;
        i += 1;
    }
    if i >= bytes.len() || bytes[i] == b'\n' || bytes[i] == b'\r' {
        return None;
    }
    if bytes[i] == b'-' {
        let after = i + 1;
        if after >= bytes.len() {
            return None;
        }
        if bytes[after] != b' ' && bytes[after] != b'\t' {
            return None;
        }
        let mut j = after;
        while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
        }
        return Some((indent, false, j));
    }
    if bytes[i].is_ascii_digit() {
        let mut j = i;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'.' {
            return None;
        }
        let after = j + 1;
        if after >= bytes.len() {
            return None;
        }
        if bytes[after] != b' ' && bytes[after] != b'\t' {
            return None;
        }
        let mut k = after;
        while k < bytes.len() && (bytes[k] == b' ' || bytes[k] == b'\t') {
            k += 1;
        }
        return Some((indent, true, k));
    }
    None
}

/// Skip ASCII whitespace (space, tab, CR, LF) inside a `#set` body.
fn skip_set_ws(bytes: &[u8], from: usize, end: usize) -> usize {
    let mut i = from;
    while i < end && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    i
}

/// Advance to the next `,` or end-of-body, used for error recovery
/// inside directive argument parsing.
fn skip_to_comma(bytes: &[u8], from: usize, end: usize) -> usize {
    let mut i = from;
    while i < end && bytes[i] != b',' {
        i += 1;
    }
    i
}

/// Return the byte offset of the next character boundary at or after
/// `from + 1`. Used to step over a single Unicode scalar value when
/// accumulating string literal contents.
fn next_char_boundary(src: &str, from: usize) -> usize {
    let mut i = from + 1;
    while i < src.len() && !src.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn find_subslice(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from > haystack.len() {
        return None;
    }
    let mut i = from;
    while i + needle.len() <= haystack.len() {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_byte(haystack: &[u8], needle: u8, from: usize) -> Option<usize> {
    haystack[from..]
        .iter()
        .position(|&b| b == needle)
        .map(|p| p + from)
}

/// Returns the byte offset just past the longest label-identifier run
/// that starts at `from` in `bytes`. Empty (caller should detect via
/// `id_end == from`) if the first byte is not a valid identifier char.
///
/// The accepted alphabet matches manifest §3.3 examples:
/// `[A-Za-z0-9_:.-]`. Critically `:` is included so `fig:wells` and
/// `eq:bayes` round-trip.
fn scan_label_chars(bytes: &[u8], from: usize) -> usize {
    let mut i = from;
    while i < bytes.len() {
        let b = bytes[i];
        let is_id = b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b'.');
        if !is_id {
            break;
        }
        i += 1;
    }
    i
}

/// If the substring `src[start..end]` begins with optional ASCII
/// whitespace followed by `<label>`, return `(label_body_start, Some(id))`
/// where `label_body_start` is the offset just past the closing `>`
/// (with any trailing whitespace also consumed). Otherwise return
/// `(start, None)`.
///
/// Only a single leading label is recognised; further `<...>` runs in
/// the body are left intact for downstream stages.
fn strip_leading_label(src: &str, start: usize, end: usize) -> (usize, Option<String>) {
    let bytes = src.as_bytes();
    let mut i = start;
    while i < end && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= end || bytes[i] != b'<' {
        return (start, None);
    }
    let id_start = i + 1;
    let id_end = scan_label_chars(bytes, id_start);
    if id_end == id_start || id_end >= end || bytes[id_end] != b'>' {
        return (start, None);
    }
    let label = src[id_start..id_end].to_owned();
    let mut after = id_end + 1;
    while after < end && (bytes[after] == b' ' || bytes[after] == b'\t' || bytes[after] == b'\n') {
        after += 1;
    }
    (after, Some(label))
}

/// If the substring `src[start..end]` ends with `<label>` (after any
/// trailing ASCII whitespace), return `(text_end, Some(id))` where
/// `text_end` is the offset of the first byte to *exclude* from the
/// preceding text — trailing whitespace before the label is also
/// trimmed. Otherwise return `(end, None)`.
fn strip_trailing_label(src: &str, start: usize, end: usize) -> (usize, Option<String>) {
    let bytes = src.as_bytes();
    if end <= start || bytes[end - 1] != b'>' {
        return (end, None);
    }
    let close = end - 1;
    // Walk back over identifier chars to find the matching `<`.
    let mut i = close;
    while i > start {
        let b = bytes[i - 1];
        let is_id = b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b'.');
        if !is_id {
            break;
        }
        i -= 1;
    }
    if i == close || i == start || bytes[i - 1] != b'<' {
        return (end, None);
    }
    let label = src[i..close].to_owned();
    let mut text_end = i - 1;
    while text_end > start && (bytes[text_end - 1] == b' ' || bytes[text_end - 1] == b'\t') {
        text_end -= 1;
    }
    (text_end, Some(label))
}

/// Locate the closing `*` of an emphasis run starting at `from`. The
/// match must be a *single* `*` — neither preceded nor followed by
/// another `*` — to avoid swallowing a `**strong**` delimiter.
fn find_emphasis_close(haystack: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i < haystack.len() {
        if haystack[i] == b'*' {
            let prev_is_star = i > 0 && haystack[i - 1] == b'*';
            let next_is_star = haystack.get(i + 1) == Some(&b'*');
            if !prev_is_star && !next_is_star {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn parse_str(src: &str) -> ParseResult {
        parse(src, &PathBuf::from("test.mos"))
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
    fn unterminated_emphasis_warns() {
        let r = parse_str("hi *there\n");
        assert!(!r.has_errors());
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code.0 == "W021" && d.severity == Severity::Warning)
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
        // emit E014 instead.
        let r = parse_str("#set foo(s: \"\\é\")\n");
        assert!(
            r.diagnostics.iter().any(|d| d.code.0 == "E014"),
            "expected E014, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn set_unknown_unit_emits_e014() {
        let r = parse_str("#set page(margin: 24xx)\n");
        assert!(
            r.diagnostics.iter().any(|d| d.code.0 == "E014"),
            "expected E014, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn set_lone_minus_emits_e014() {
        // `-` not followed by a digit is a malformed number literal.
        let r = parse_str("#set foo(x: -)\n");
        assert!(
            r.diagnostics.iter().any(|d| d.code.0 == "E014"),
            "expected E014, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn set_missing_colon_emits_e015() {
        let r = parse_str("#set page(paper \"A4\")\n");
        assert!(
            r.diagnostics.iter().any(|d| d.code.0 == "E015"),
            "expected E015, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn set_positional_arg_emits_e015() {
        let r = parse_str("#set page(\"A4\")\n");
        assert!(r.diagnostics.iter().any(|d| d.code.0 == "E015"));
    }

    #[test]
    fn unterminated_set_block_errors() {
        let r = parse_str("#set page(\n  paper: \"A4\",\n");
        assert!(r.has_errors());
    }

    #[test]
    fn trailing_content_after_set_block_diagnoses_and_recovers() {
        // Prior behaviour swallowed everything between `)` and the
        // next `\n`. The parser now emits E013 and leaves the
        // trailing bytes in place so they parse as a paragraph.
        let r = parse_str("#set page(paper: \"A4\") leftover\n");
        assert!(
            r.diagnostics.iter().any(|d| d.code.0 == "E013"),
            "expected E013 diagnostic, got {:?}",
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
        // identifier (E010) or it parses as paragraph; what matters
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
        assert!(r.diagnostics.iter().any(|d| d.code.0 == "W023"));
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
        let r = parse_str("#code[fn main() {\n    println(\"hi\");\n}]\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.tree.items.len(), 1);
        let Item::RawBlock { kind, text, .. } = &r.tree.items[0] else {
            panic!("expected raw block, got {:?}", r.tree.items[0]);
        };
        assert_eq!(*kind, RawBlockKind::Code);
        assert_eq!(text, "fn main() {\n    println(\"hi\");\n}");
    }

    #[test]
    fn raw_blocks_allow_escaped_closing_bracket() {
        let r = parse_str("#pre[open \\] close]\n");
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let Item::RawBlock { kind, text, .. } = &r.tree.items[0] else {
            panic!("expected raw block, got {:?}", r.tree.items[0]);
        };
        assert_eq!(*kind, RawBlockKind::Pre);
        assert_eq!(text, "open \\] close");
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
                .any(|d| d.code.0 == "E012" && d.message.contains("#image")),
            "expected E012 mentioning #image, got {:?}",
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
}
