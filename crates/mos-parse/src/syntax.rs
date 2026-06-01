use std::path::PathBuf;

use mos_core::{Diagnostic, Severity, SourceSpan};

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
        label_span: Option<SourceSpan>,
        span: SourceSpan,
    },
    /// One or more consecutive non-blank lines that are not a heading
    /// and not a `#set` block. A leading `<label>` token (possibly
    /// preceded by ASCII whitespace) attaches to this paragraph.
    Paragraph {
        inlines: Vec<Inline>,
        label: Option<String>,
        label_span: Option<SourceSpan>,
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
    /// long-bracket body as text; the kind leaves room for later styling
    /// or language-aware code rendering.
    RawBlock {
        kind: RawBlockKind,
        args: Vec<SetArg>,
        text: String,
        label: Option<String>,
        label_span: Option<SourceSpan>,
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

/// Tag for the directive shapes [`Item::Set`] can represent — the
/// `#set <target>(...)` configuration directive vs the standalone
/// `#image(...)`, `#figure(...)`, and `#bibliography(...)` calls. The
/// lowerer dispatches on this rather than the [`Item::Set::name`] string
/// so `#set image(...)` can never collide with `#image(...)`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DirectiveKind {
    /// `#set <name>(...)` — sets defaults on a style target.
    Set,
    /// `#image("path", ...)` — raster image directive.
    Image,
    /// `#figure(image: ..., caption: ...)` — captioned image container.
    Figure,
    /// `#bibliography("refs.bib")` — declares a bibliography source
    /// database. The lowerer records the (source-relative) path so a
    /// later BibTeX-parsing slice can read it; citation resolution and
    /// rendering are not part of this directive.
    Bibliography,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RawBlockKind {
    Pre,
    Code,
}

/// Borrowed view of an [`Item::RawBlock`] payload.
#[derive(Debug, Clone, Copy)]
pub struct RawBlockView<'a> {
    pub kind: RawBlockKind,
    pub args: &'a [SetArg],
    pub text: &'a str,
    pub label: Option<&'a str>,
    pub label_span: Option<&'a SourceSpan>,
    pub span: &'a SourceSpan,
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
    BoldItalic,
    Code,
    /// `@label` — a cross-reference to a labelled block. The
    /// [`Inline::text`] payload is the bare label name (no leading
    /// `@`); the resolver rewrites it to the target's resolved text.
    Reference,
    /// `[@key]` — a citation to a bibliography entry. The
    /// [`Inline::text`] payload is the bare citation key (no leading
    /// `[@` or trailing `]`); bibliography loading and rendering are
    /// future work tracked under MVP 4. The key alphabet matches the
    /// label alphabet (`[A-Za-z0-9_:.-]`); a single key per
    /// `[@…]` group is the only form recognised in this slice — list
    /// forms like `[@a; @b]` and prefix/suffix bodies are deferred.
    Citation,
    /// `\\` — a forced line break inside a paragraph. The line
    /// breaks here without the extra leading a blank-line paragraph
    /// break would give. Carries no text payload. The shorthand for
    /// a soft hyphen `\-` lowers to a literal U+00AD inside a
    /// surrounding [`InlineKind::Text`] run, not to a separate variant.
    HardBreak,
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

    /// Borrow the raw block payload if `self` is [`Item::RawBlock`].
    #[must_use]
    pub fn as_raw_block(&self) -> Option<RawBlockView<'_>> {
        if let Self::RawBlock {
            kind,
            args,
            text,
            label,
            label_span,
            span,
        } = self
        {
            Some(RawBlockView {
                kind: *kind,
                args: args.as_slice(),
                text: text.as_str(),
                label: label.as_deref(),
                label_span: label_span.as_ref(),
                span,
            })
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
    /// Returns `None` for [`Item::Set`] and [`Item::List`] (label
    /// syntax is not yet defined on those blocks).
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Heading { label, .. }
            | Self::Paragraph { label, .. }
            | Self::RawBlock { label, .. } => label.as_deref(),
            Self::Set { .. } | Self::List { .. } => None,
        }
    }

    /// Borrow the source span covering only the label token text, if any.
    /// The delimiters (`<`, `>`, or directive string quotes) are excluded so a
    /// structured suggestion can replace just the label bytes.
    #[must_use]
    pub fn label_span(&self) -> Option<&SourceSpan> {
        match self {
            Self::Heading { label_span, .. }
            | Self::Paragraph { label_span, .. }
            | Self::RawBlock { label_span, .. } => label_span.as_ref(),
            Self::Set { .. } | Self::List { .. } => None,
        }
    }
}

/// Output of [`crate::parse`]. Diagnostics may include warnings even
/// when the tree is structurally usable; callers decide what to do per
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
            .any(|d| d.severity() == Severity::Error)
    }
}
