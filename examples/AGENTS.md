# EXAMPLES KNOWLEDGE BASE

## OVERVIEW

`examples/` contains self-contained Mosaic projects plus committed PDF snapshots. They are demo and
regression fossils, not normal `cargo test` snapshots.

## STRUCTURE

```text
examples/<name>/
├── main.mos
├── mosaic.toml
├── <name>.pdf        # committed snapshot
└── build/main.pdf    # generated, ignored
```

## WHERE TO LOOK

| Example  | Exercises                                                            |
| -------- | -------------------------------------------------------------------- |
| `hello`  | Noto Sans embedding, Unicode text, bold/italic, lists, image figure. |
| `lists`  | Ordered/unordered lists, nesting, hanging indent, marker behavior.   |
| `math`   | Base-14 Helvetica, `/Differences`, math-ish glyph copy/paste.        |
| `polish` | Extended Latin diacritics through embedded Noto Sans path.           |

## CONVENTIONS

- Regenerate snapshots with `just examples` from repo root.
- `just examples` runs `cargo mos build` inside each example dir, then copies `build/main.pdf` to
  `<name>.pdf`.
- Commit `<name>.pdf`; do not commit `build/main.pdf`.
- `mosaic.toml` is mostly convention/docs today. Current CLI does not honor `output = ["html"]`.
- `hello/demo.png` has a generator at `crates/mosaic-eval/examples/gen_demo_png.rs`.

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
