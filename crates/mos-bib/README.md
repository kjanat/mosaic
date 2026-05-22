# mos-bib

Placeholder bibliography and citation types for Mosaic.

This crate exists to reserve the bibliography domain boundary from `manifest.md` §12. It is not a
bibliography engine yet. Current shipped Mosaic support is `mos check` and `mos build` to PDF;
bibliography remains aspirational/stubbed.

## Current API

`mos-bib` currently exposes two simple public types:

- `Bibliography`: an empty, private-field collection placeholder.
- `Citation`: a document-body citation reference with a public `key: String`.

```rust
use mos_bib::{Bibliography, Citation};

let bibliography = Bibliography::default();
let citation = Citation {
    key: "knuth1984".to_owned(),
};

assert_eq!(citation.key, "knuth1984");
assert_eq!(format!("{bibliography:?}"), "Bibliography { _private: () }");
```

## Boundary

- Depends only on `mos-core` today.
- Should stay close to core model types until real integration needs more.
- Should own bibliography/citation data modeling when implemented.
- Should not parse `.mos` syntax, lower documents, lay out pages, or emit backend output.

## Known Non-Goals Today

- No BibTeX, BibLaTeX, or CSL parsing.
- No citation resolution, ordering, clustering, formatting, or rendering.
- No `#bibliography()` or `@citation` language support.
- No integration with `mos check`, `mos build`, layout, PDF, HTML, or LSP.
- No claim of manifest §12 completion. Manifest loud; code quiet. Booga trust code.
