# EXAMPLES KNOWLEDGE BASE

## OVERVIEW

`examples/` contains self-contained Mosaic projects plus committed PDF snapshots. They are demo and
regression fossils, not normal `cargo test` snapshots.

## STRUCTURE

```text
examples/<name>/
├── main.mos
├── mosaic.toml       # declares [output].pdf = "<name>.pdf"
├── <name>.pdf        # committed snapshot
└── build/main.pdf    # fallback/generated path; ignored
```

## WHERE TO LOOK

| Example      | Exercises                                                                                                                                 |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `code`       | Inline code, multiline inline code, long-bracket `#pre`/`#code`.                                                                          |
| `hello`      | Noto Sans embedding, Unicode text, bold/italic, lists, image figure.                                                                      |
| `linebreaks` | NBSP, hard line breaks, and greedy soft-hyphen breaks.                                                                                    |
| `lists`      | Ordered/unordered lists, nesting, hanging indent, marker behavior.                                                                        |
| `lsp`        | `mos-lsp` demo: heading + figure labels, `@label` / `@page(label)` refs, `[@key]` citations + `refs.bib`, `#figure(image:)` (`demo.png`). |
| `math`       | Base-14 Helvetica, `/Differences`, math-ish glyph copy/paste.                                                                             |
| `polish`     | Extended Latin diacritics through embedded Noto Sans path.                                                                                |

## CONVENTIONS

- Regenerate snapshots with `just examples` from repo root.
- `just examples` runs `runner mos build examples/*` after `just setup`; each example manifest
  declares `[output].pdf` so the CLI writes `<name>.pdf` directly.
- Commit `<name>.pdf`; do not commit generated `build/*.pdf`.
- `mosaic.toml` is partly active: CLI directory builds honor `project.entry` and `output.pdf`.
  Current CLI still does not honor `document.output = ["html"]`.
- `hello/demo.png` has a generator at `crates/mos-eval/examples/gen_demo_png.rs`.

## GOTCHAS

- Long list items should stay one source line; layout wraps visually. Continuation lines are not
  list continuations today.
- Ordered list source digits are ignored by current renderer; visible numbering starts at 1.
- Reader font substitution can affect Base-14 visual output, especially unusual symbols.
- If editing `polish`, verify prose still matches actual embedded Noto Sans behavior.

## ANTI-PATTERNS

- Do not assume examples prove HTML/EPUB support.
- Do not edit generated `build/main.pdf` directly.
- Do not add future manifest features to examples unless the code supports them.
