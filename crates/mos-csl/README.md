# mos-csl

Citation Style Language (CSL) support for Mosaic.

This crate provides the **data foundations** for CSL 1.0.2 (`manifest.md` §12): it is **not** a CSL
processor. The scope is deliberately bounded to a typed data model, a BibTeX mapping, and a style
parser; evaluating a style against data to render citations is a separate, later concern.

> [!WARNING]
> While this crate is in the `0.0.x` line, Mosaic treats it as pre-alpha. Breaking changes are
> acceptable between patch releases. If you depend on this crate, pin an exact version such as
> `=0.0.2`, or accept the risk of API breakage.

## API

- **Item data model**: `Item` (`id` + `ItemType` + variable maps), the `ItemType` enum and the
  `StandardVariable` / `NumberVariable` / `DateVariable` / `NameVariable` vocabularies (spec
  Appendices III–IV, including deprecated standard variable `event`), plus `Name` and
  `Date`/`DateParts`. Each vocabulary has `as_str` / `from_csl`.
- **BibTeX → CSL mapping**: `item_from_bib_entry(&BibEntry) -> Item` and
  `library_from_bibliography(&Bibliography) -> BTreeMap<String, Item>` (infallible, best-effort):
  common entry types/fields, `Last, First` and `First Last` names, numeric years, report numbers,
  and conference event places.
- **CSL style parser**: `parse_style(&str) -> Result<Style, CslParseError>` producing the typed
  `Style` AST (`<style>`, `<info>`, `<citation>`, `<bibliography>`, `<macro>`, and the rendering
  elements). `CslParseError` / `CslParseErrorKind` carry a byte offset and bridge to a `mos-core`
  `Diagnostic` (`MOS0044`).

```rust
use mos_csl::{parse_style, StyleClass};

let style = parse_style(
    r#"<style version="1.0" class="in-text">
         <info><title>Demo</title></info>
         <citation><layout><text variable="title"/></layout></citation>
       </style>"#,
)
.expect("valid CSL");
assert_eq!(style.class, StyleClass::InText);
```

## What the parser handles

- The `<style>` tree: `class`/`version`/`default-locale`, `<info>` (`id`/`title`, dependent-style
  links, categories, authors/contributors, `updated`, `issn`), retained raw in-style `<locale>`
  blocks, `<macro>` definitions, and `<citation>`/`<bibliography>` with their `<layout>` and
  `<sort>`.
- Rendering elements `text`, `number`, `date` (+ `date-part`), `names` (+ `name`/`et-al`/`label`/
  `substitute`), `label`, `group`, and `choose` (+ `if`/`else-if`/`else`), with common attributes
  (affixes, formatting, `delimiter`, `text-case`, …), `<name-part>` formatting, and
  retained-but-not-evaluated style, citation, bibliography, name, sort-key, date-part, and label
  rendering options.
- Leniency decisions: dispatch is on element local names, and the `<style>` root must be in the CSL
  namespace or none (a foreign namespace is rejected); `version` must be `1.0` or a `1.0.x` release;
  `<text>` must select exactly one of `variable`/`macro`/`term`/`value`; `<choose>` must use `<if>`,
  then `<else-if>`, then optional `<else>` order.
- Useful, panic-free errors: malformed XML, wrong root, missing/unsupported `version`, missing or
  unknown `class`, a `<macro>` without a `name`, a `<citation>`/`<bibliography>` without a
  `<layout>`, a `<text>` with no or multiple sources, invalid `<choose>` order, or an unsupported
  rendering element. Unmodelled attributes are ignored.

## Boundary

- Depends only on `mos-core`, `mos-bib`, and `roxmltree` (XML parsing).
- Must not depend on `mos-parse` or `mos-eval`.
- Owns CSL data/style modelling; should not lower `.mos`, lay out pages, or emit backends.

## Out of scope (separate features)

These are distinct capabilities, not unfinished parts of this crate:

- The CSL **processor**: evaluating retained style options against items to produce formatted
  citations or bibliographies: formatting, sorting, disambiguation, cite grouping/collapsing, name
  ordering, ordinals, term/date rendering.
- Locale files (`locales-xx-XX.xml`) and locale fallback; in-style `<locale>` blocks are retained as
  raw XML, not interpreted.
- Full BibTeX name parsing (protected institutional names, von/Jr particles) in the mapping.
- Reading `.csl`/`.bib` files from disk, and any `mos check` / `mos build` / layout / PDF / LSP
  wiring.

This crate does not claim manifest §12 is complete. Where the manifest and the code disagree, trust
the code.
