# Labels and references

This page documents the label and `@`-reference behavior that Mosaic ships **today**. Labels and
references are resolved by a layout-free semantic pass that runs before layout and PDF emission, so
a reference can resolve to a section number or label text, but never to a page number. Page
references are **not** implemented — see [Not yet implemented](#not-yet-implemented). Where this
page and `manifest.md` disagree, this page (and the compiler) win.

A label is an identifier attached to a block; an `@`-reference points back at it and is rewritten to
the target's section number or to the bare label text.

## Declaring a label

There are two ways to attach a label, depending on the block.

### Angle-bracket labels: headings, paragraphs, raw blocks

Write the label in angle brackets, `<id>`:

- **Heading** — at the end of the heading line:

  ```mos
  = Introduction <intro>
  ```

- **Paragraph** — at the start of the paragraph (optionally after leading spaces):

  ```mos
  <note> A short aside about scope.
  ```

- **Raw block** (`#code[[…]]` / `#pre[[…]]`) — after the closing `]]`:

  ```mos
  #code[[
  let labelled = true;
  ]] <ex:code>
  ```

There is no trailing-`<id>` form on directives — figures and images use the `label:` argument below.

### Directive labels: `#image` and `#figure`

`#image` and `#figure` take the label as a `label:` string argument, alongside their other
arguments:

```mos
#image("demo.png", label: "img:logo")

#figure(image: "demo.png", caption: "A demo figure.", label: "fig:demo")
```

A figure or image node is only created if its image actually loads. If the image path or decode
validation fails, no image/figure node is produced, so its label is absent; a later `@that-label`
can then also report [`MOS0033`](#unknown-references-mos0033).

### Label identifiers

The reference scanner reads an `@`-label as a run of characters from `A-Z a-z 0-9 _ - : .`. The
`label:` directive argument accepts any string, but an `@`-reference can only ever match that
scanner alphabet — so **use `A-Z a-z 0-9 _ - : .` for labels you want to reference**.

`:` and `.` are ordinary label characters with no special meaning. Prefixes like `fig:demo`,
`img:logo`, or future `eq:bayes` are just label text; `:` has no special meaning today.

Because `.`, `-`, and `:` are label characters, **trailing punctuation that touches a reference is
absorbed into it**. Even when the label exists, `@intro.` at the end of a sentence references the
label `intro.` (with the period), not `intro`:

```mos
= Introduction <intro>

See @intro.
```

```text
error[MOS0033]: unknown label `intro.` in `@` reference
  --> main.mos:3:5
   |
  3| See @intro.
   |     ^^^^^^^
```

Keep a space (or another non-label character) between a reference and any following punctuation, or
rephrase so the reference is not flush against a `.`, `-`, or `:`.

### Duplicate labels (`MOS0030`)

A label may be declared once. If the same label is declared again, the **first** declaration wins
and each later one is an error, [`MOS0030`](./diagnostic-codes.md), pointing back at the first:

```text
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

| Target                         | Reference renders as                |
| ------------------------------ | ----------------------------------- |
| Heading (section)              | the section number, e.g. `1`, `1.2` |
| `#figure` (label)              | the bare label, e.g. `fig:demo`     |
| Paragraph, raw block, `#image` | the bare label, e.g. `note`         |

Figure references render as the bare label for now; numbered "Figure 3"-style text is pending — see
[Not yet implemented](#not-yet-implemented).

### Unknown references (`MOS0033`)

If a reference names a label that does not exist, it is an error,
[`MOS0033`](./diagnostic-codes.md):

```text
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

```text
warning[MOS0036]: stray `@` is not followed by a label identifier; treated as text
  --> main.mos:1:10
   |
  1| Reach me @ the front desk.
   |          ^
```

> Diagnostic renderings above show the current `mos check` output shape; the exact layout is not a
> stable contract.

## Not yet implemented

- **Page references and layout-dependent references.** A reference never resolves to a page number,
  and nothing re-runs resolution after layout. This boundary is deliberate and documented in
  [page-reference-fixpoint-boundary.md](./page-reference-fixpoint-boundary.md). Note that
  `@page:foo` is **not** page-reference syntax — it is just a reference to a label named `page:foo`.
  No `prefix:`-based page-reference form is reserved; do not rely on one.
- **Figure numbering and kind-aware reference text.** Figure and image labels work today, but figure
  references render as the bare label rather than a number, and there is no equation/table/theorem
  reference text yet. Richer, kind-aware rendering is future work (manifest-tracker.md → Semantic
  Model And Resolver → "Make reference text kind-aware", "Resolve figure numbering").

## Related

- `[@key]` is **citation** syntax — a separate construct that does not resolve against labels. A
  malformed `[@…]` group warns as `MOS0039` and is treated as text.
- [diagnostic-codes.md](./diagnostic-codes.md) is the full catalog and the source for the exact
  severities of `MOS0030`, `MOS0033`, `MOS0036`, and `MOS0039`.
