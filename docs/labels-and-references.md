# Labels and references

This page documents the label and `@`-reference behavior that Mosaic ships **today**. Section,
figure, and label references are resolved by a layout-free semantic pass that runs before layout and
PDF emission. A `@page(label)` reference instead resolves to a printed **page number**, which
`mos build` computes after layout through a bounded resolve↔layout fixpoint (see
[Page references](#page-references-pagelabel)); `mos check` validates the label but, because it does
not lay out, leaves the page number unresolved. Where this page and `manifest.md` disagree, this
page (and the compiler) win.

A label is an identifier attached to a block; an `@`-reference points back at it and is rewritten to
the target's section number, a figure's `Figure N` text, or the bare label text.

## Declaring a label

There are two ways to attach a label, depending on the block.

### Angle-bracket labels: headings, paragraphs, raw blocks

Write the label in angle brackets, `<id>`:

- **Heading**: at the end of the heading line:

  ```mos
  = Introduction <intro>
  ```

- **Paragraph**: at the start of the paragraph (optionally after leading spaces):

  ```mos
  <note> A short aside about scope.
  ```

- **Raw block** (`#code[[…]]` / `#pre[[…]]`): after the closing `]]`:

  ```mos
  #code[[
  let labelled = true;
  ]] <ex:code>
  ```

There is no trailing-`<id>` form on [call-style directives](#call-style-directives) like
`#image(...)` and `#figure(...)`; those use a [`label:` argument](#label-arguments) instead.

### Call-style directives

`#image(...)` and `#figure(...)` are directive calls with structured arguments.

#### `label:` arguments

`#image` and `#figure` take the label as a `label:` string argument, alongside their other
arguments:

```mos
#image("demo.png", label: "img:logo")

#figure(image: "demo.png", caption: "A demo figure.", label: "fig:demo")
```

A figure or image node is only created if its image actually loads. If the image path or decode
validation fails, no image/figure node is produced, so its label is absent; a later `@that-label`
can then also report [`MOS0033`](#unknown-references-mos0033).

#### Figure numbering controls

By default every `#figure` is auto-numbered `Figure 1`, `Figure 2`, … in document order, and that
`Figure N:` label is prefixed to its caption. Two arguments adjust this per figure:

- `numbered: false` opts a figure **out** of numbering. It carries no number, its caption keeps no
  `Figure N:` prefix, and, by the counter rule, it does **not** advance the figure counter, so the
  numbered figures around it stay contiguous (`1`, `2`, `3`, …). A reference to a skipped figure has
  no number to show and renders as its bare label, like an `#image` reference.
- `supplement: "Plate"` replaces the `Figure` supplement word in both the caption (`Plate 1: …`) and
  references (`Plate 1`). It does not change numbering; the figure still counts. `supplement: ""`
  (or `supplement: none`) drops the word entirely: the figure stays numbered but its caption and
  references show the number alone (`1: …`, and `1`); the "no visible prefix" form, distinct from
  `numbered: false`, which drops the number itself.

```mos
#figure(image: "cover.png", caption: "Cover art.", numbered: false)

#figure(image: "map.png", caption: "The site.", supplement: "Plate", label: "fig:site")
```

Numbering stays deterministic from document order: there is no way to set an explicit number
(`numbered:` is a boolean, not a count). For an inherently unnumbered graphic, prefer `#image`,
which never participates in figure numbering at all; reserve `#figure` for captioned/numbered floats
and use `numbered: false` only when a captioned float should opt out of the counter.

### Label identifiers

The reference scanner reads an `@`-label as a run of characters from `A-Z a-z 0-9 _ - : .`. The
`label:` directive argument accepts any string, but an `@`-reference can only ever match that
scanner alphabet, so **use `A-Z a-z 0-9 _ - : .` for labels you want to reference**.

`:` and `.` are ordinary label characters with no special meaning. Prefixes like `fig:demo`,
`img:logo`, or future `eq:bayes` are just label text; `:` has no special meaning today.

Because `.`, `-`, and `:` are label characters, **trailing punctuation that touches a reference is
absorbed into it**. Even when the label exists, `@intro.` at the end of a sentence references the
label `intro.` (with the period), not `intro`:

```mos
= Introduction <intro>

See @intro.
```

```console
error[MOS0033]: unknown label `intro.` in `@` reference
  --> main.mos:3:5
   |
  3| See @intro.
   |     ^^^^^^^
```

Keep a space (or another non-label character) between a reference and any following punctuation, or
rephrase so that the reference is not flush against a `.`, `-`, or `:`.

### Duplicate labels (`MOS0030`)

A label may be declared once. If the same label is declared again, the **first** declaration wins
and each later one is an error, [`MOS0030`](./diagnostic-codes.md), pointing back at the first:

```console
error[MOS0030]: label `intro` is declared more than once
  --> main.mos:3:1
   |
  3| = Methods <intro>
   | ^^^^^^^^^^^^^^^^^
  note: first declaration of `intro` is here (main.mos:1:1)
```

## Referencing a label

Write `@` immediately followed by the label identifier, with no space:

```mos
See @intro and @fig:demo here.
```

### Reference text

What the reference is rewritten to depends on the target:

| Target                             | Reference renders as                            |
| ---------------------------------- | ----------------------------------------------- |
| Heading (section)                  | the section number, e.g. `1`, `1.2`             |
| `#figure` (label)                  | supplement + number, e.g. `Figure 1`, `Plate 1` |
| `#figure(numbered: false)` (label) | the bare label, e.g. `fig:site`                 |
| Paragraph, raw block, `#image`     | the bare label, e.g. `note`                     |

A numbered figure reference renders kind-aware as its supplement word followed by the figure's
document-order number (`Figure 1` by default, or the custom `supplement:` word: see
[Figure numbering controls](#figure-numbering-controls)), joined with a non-breaking space so the
label never wraps off its number. The same `{supplement} N:` label is also prefixed to the figure's
caption. Sections render as a bare number; generic targets, and figures opted out of numbering, fall
back to the bare label.

### Page references: `@page(label)`

`@page(label)` references the **printed page number** of a labelled target, as opposed to the
target's section or figure number:

```mos
See section @intro on page @page(intro).
```

Unlike a section or figure number, which the resolver computes from document order: a page number is
only known after layout, because a page reference's own width can shift where its target lands.
`mos build` therefore resolves page references through a **bounded resolve↔layout fixpoint**: it
lays the document out, feeds the resulting label→page map back into the resolver to rewrite each
`@page(...)` to its target's page number, and re-lays-out, repeating until the page numbers stop
changing. Stable documents settle in one or two rounds.

If the page numbers never stabilize: a pathological case where resolving a reference keeps shifting
its target across a page boundary; the engine stops at an iteration cap and emits
[`MOS0047`](./diagnostic-codes.md) (a warning), keeping the last computed numbers so the build still
produces output.

Only a well-formed `@page(label)`; the identifier `page` immediately followed by `(`, a label, and
`)` is a page reference; a bare `@page` is still an ordinary reference to a label named `page`. An
**undeclared** label in `@page(...)` is reported as [`MOS0033`](./diagnostic-codes.md) at check
time, exactly like a bad `@ref`, so `mos check` catches it without laying the document out.

> Note: page references are resolved by `mos build` (which lays out). `mos check` validates the
> label but does not compute page numbers, since it does not run layout.

> Compatibility note: before page references existed, `@page(intro)` parsed as a reference to a
> label `page` followed by the literal text `(intro)`. It now parses as a single page reference.
> This is a deliberate change while the language is pre-alpha.

### Unknown references (`MOS0033`)

If a reference names a label that does not exist, it is an error,
[`MOS0033`](./diagnostic-codes.md):

```console
error[MOS0033]: unknown label `missing` in `@` reference
  --> main.mos:1:5
   |
  1| See @missing for details.
   |     ^^^^^^^^
```

The resolver leaves `?label?` as fallback text in the document, but `mos check` / `mos build` still
fail on `MOS0033`; the build will not emit a PDF.

### Stray `@` (`MOS0036`)

A `@` that is **not** followed by a label character is not a reference. It is kept as literal text
and reported as a warning, [`MOS0036`](./diagnostic-codes.md); the build continues:

```console
warning[MOS0036]: stray `@` is not followed by a label identifier; treated as text
  --> main.mos:1:10
   |
  1| Reach me @ the front desk.
   |          ^
```

> Diagnostic renderings above show the current `mos check` output shape; the exact layout is not a
> stable contract.

## Not yet implemented

- **Prefix-style page references.** The only page-reference syntax is `@page(label)` (see
  [Page references](#page-references-pagelabel)). `@page:foo` is **not** a page reference: it is an
  ordinary reference to a label named `page:foo`, and no `prefix:`-based page-reference form is
  reserved; do not rely on one.
- **Kind-aware reference text for equations, tables, and theorems.** Figures resolve to kind-aware
  `Figure N` text and sections to a bare number, but there is no equation/table/theorem numbering or
  reference text yet. The remaining kind-aware rendering is future work (manifest-tracker.md →
  Semantic Model And Resolver → "Make reference text kind-aware").
- **Localized labels.** The default figure supplement is hard-coded English (`Figure`), as is the
  `:` caption separator. A figure can override the word per-call with `supplement:`, but selecting
  the default automatically from the document language (cf. LaTeX babel `\figurename`, Typst
  `text(lang: …)`) is future work.

## Related

- `[@key]` is **citation** syntax: a separate construct that does not resolve against labels. A
  malformed `[@…]` group warns as `MOS0039` and is treated as text.
- [diagnostic-codes.md](./diagnostic-codes.md) is the full catalog and the source for the exact
  severities of `MOS0030`, `MOS0033`, `MOS0036`, and `MOS0039`.
