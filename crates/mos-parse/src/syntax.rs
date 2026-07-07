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
    /// lists always renumber from 1 in MVP: explicit `start: N` is
    /// deferred.
    List {
        ordered: bool,
        items: Vec<ListItem>,
        span: SourceSpan,
    },
    /// A `/** … */` documentation comment. Unlike `//` line and `/*` block
    /// comments (both dropped), a doc comment is preserved so the lowerer can
    /// attach its `text` as a `doc` attribute on the semantic node it
    /// precedes (a heading, or a labelled block), which the LSP surfaces on
    /// hover. `text` is the cleaned inner body, fences removed.
    DocComment { text: String, span: SourceSpan },
}

/// One entry inside an [`Item::List`].
///
/// `blocks` preserves source order between item paragraphs and nested lists;
/// nested lists live only there. `inlines` mirrors the first paragraph for
/// older consumers.
#[derive(Debug, Clone)]
pub struct ListItem {
    pub inlines: Vec<Inline>,
    pub blocks: Vec<ListItemBlock>,
    pub span: SourceSpan,
}

/// Ordered content inside a [`ListItem`].
#[derive(Debug, Clone)]
pub enum ListItemBlock {
    Paragraph {
        inlines: Vec<Inline>,
        span: SourceSpan,
    },
    List {
        ordered: bool,
        items: Vec<ListItem>,
        span: SourceSpan,
    },
}

/// Tag for the directive shapes [`Item::Set`] can represent.
///
/// Distinguishes `#set <target>(...)` from standalone `#image(...)`,
/// `#figure(...)`, and `#bibliography(...)` calls so the lowerer does not
/// infer semantics from [`Item::Set::name`].
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DirectiveKind {
    /// `#set <name>(...)`: sets defaults on a style target.
    Set,
    /// `#image("path", ...)`: raster image directive.
    Image,
    /// `#figure(image: ..., caption: ...)`: captioned image container.
    Figure,
    /// `#bibliography("refs.bib")`: declares a bibliography source
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

/// One argument inside a directive body: either a `key: value`
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
    pub const fn value(&self) -> &SetValue {
        match self {
            Self::Named { value, .. } | Self::Positional { value, .. } => value,
        }
    }

    /// The span covering the argument's value literal.
    #[must_use]
    pub const fn value_span(&self) -> &SourceSpan {
        match self {
            Self::Named { value_span, .. } | Self::Positional { value_span, .. } => value_span,
        }
    }

    /// The key identifier for [`Self::Named`]; `None` for
    /// [`Self::Positional`].
    #[must_use]
    pub const fn key(&self) -> Option<&str> {
        match self {
            Self::Named { key, .. } => Some(key.as_str()),
            Self::Positional { .. } => None,
        }
    }

    /// The span covering the key identifier, for [`Self::Named`].
    /// `None` for [`Self::Positional`].
    #[must_use]
    pub const fn key_span(&self) -> Option<&SourceSpan> {
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
    /// For [`InlineKind::Reference`] / [`InlineKind::PageReference`], the
    /// source span of the label *identifier* alone; the `intro` in `@intro`
    /// or `@page(intro)`, excluding the `@` sigil and the `@page(`…`)`
    /// wrapper. The lowerer stamps it as the node's `label_span` so editor
    /// features (rename) read the identifier range directly instead of
    /// re-deriving it from [`Self::span`] geometry. `None` for every other
    /// inline kind.
    pub label_span: Option<SourceSpan>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InlineKind {
    Text,
    Emphasis,
    Strong,
    BoldItalic,
    Code,
    /// `@label`: a cross-reference to a labelled block. The
    /// [`Inline::text`] payload is the bare label name (no leading
    /// `@`); the resolver rewrites it to the target's resolved text.
    Reference,
    /// `@page(label)`: a reference to the printed *page number* of a
    /// labelled target. The [`Inline::text`] payload is the bare label name
    /// (the `page(` wrapper and `)` stripped). Distinct from
    /// [`Reference`](Self::Reference), which resolves to the target's section
    /// or figure number; a page reference resolves to where the target lands,
    /// which is only known after layout. Resolution runs through the
    /// resolve↔layout fixpoint (issue #72); this slice parses and models the
    /// reference but leaves it unresolved (placeholder text).
    PageReference,
    /// `[@key]`: a citation to a bibliography entry. The
    /// [`Inline::text`] payload is the bare citation key (no leading
    /// `[@` or trailing `]`); bibliography loading and rendering are
    /// future work tracked under MVP 4. The key alphabet matches the
    /// label alphabet (`[A-Za-z0-9_:.-]`); a single key per
    /// `[@…]` group is the only form recognised in this slice: list
    /// forms like `[@a; @b]` and prefix/suffix bodies are deferred.
    Citation,
    /// `\\`: a forced line break inside a paragraph. The line
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
    /// accessor name is retained for back-compat; every existing
    /// caller pre-dates the `#image`/`#figure` directives and only
    /// looks at name/args/span.
    #[must_use]
    pub const fn as_set(&self) -> Option<(&str, &[SetArg], &SourceSpan)> {
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
    pub const fn directive_kind(&self) -> Option<DirectiveKind> {
        if let Self::Set { kind, .. } = self {
            Some(*kind)
        } else {
            None
        }
    }

    /// Borrow the list payload if `self` is [`Item::List`]. The
    /// returned tuple is `(ordered, items, span)`.
    #[must_use]
    pub const fn as_list(&self) -> Option<(bool, &[ListItem], &SourceSpan)> {
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

    /// Borrow the `/** … */` doc-comment payload if `self` is
    /// [`Item::DocComment`]. The returned tuple is `(cleaned text, span)`.
    #[must_use]
    pub fn as_doc_comment(&self) -> Option<(&str, &SourceSpan)> {
        if let Self::DocComment { text, span } = self {
            Some((text.as_str(), span))
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
            Self::Set { .. } | Self::List { .. } | Self::DocComment { .. } => None,
        }
    }

    /// Borrow the source span covering only the label token text, if any.
    /// The delimiters (`<`, `>`, or directive string quotes) are excluded so a
    /// structured suggestion can replace just the label bytes.
    #[must_use]
    pub const fn label_span(&self) -> Option<&SourceSpan> {
        match self {
            Self::Heading { label_span, .. }
            | Self::Paragraph { label_span, .. }
            | Self::RawBlock { label_span, .. } => label_span.as_ref(),
            Self::Set { .. } | Self::List { .. } | Self::DocComment { .. } => None,
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
