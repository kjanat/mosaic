# PDF BASE14 METRICS KNOWLEDGE BASE

## OVERVIEW

`pdf-base14-metrics` bakes vendored AFM data for PDF Base-14 fonts. It sits between
`adobe-font-metrics` and `mos-fonts`.

## STRUCTURE

```text
pdf-base14-metrics/
├── data/afm/*.afm       # metric source truth
├── data/agl/glyphlist.txt # test oracle only, not published
├── build.rs            # generates $OUT_DIR/baked.rs
├── src/winansi_table.rs # shared by build script and runtime include
└── tests/              # baked metrics and AGL/WinAnsi oracle tests
```

## WHERE TO LOOK

| Task             | Location                  | Notes                                |
| ---------------- | ------------------------- | ------------------------------------ |
| Public metrics   | `src/lib.rs`              | Font tables and lookup helpers.      |
| Build generation | `build.rs`                | AFM parse and static Rust emission.  |
| WinAnsi bands    | `src/winansi_table.rs`    | No inner docs; included by build.rs. |
| Runtime char map | `src/winansi_char_map.rs` | Runtime-only helpers.                |
| AGL subset       | `src/agl_subset.rs`       | Curated glyph-name mapping.          |

## CONVENTIONS

- Edit AFM data, WinAnsi tables, or build script; never generated `$OUT_DIR/baked.rs`.
- `data/agl` is test oracle only and excluded from published crate for license boundary.
- WinAnsi here means PDF WinAnsi, not raw CP1252.
- Symbol and ZapfDingbats are not Latin WinAnsi fonts.
- Build output must stay deterministic and sorted where runtime lookup expects it.

## ANTI-PATTERNS

- Do not make build read `data/agl`.
- Do not ship AGL test oracle in the published crate.
- Do not binary-search raw AFM char metrics; they are not sorted.
- Do not add shaping, font discovery, layout, or PDF emission policy here.
