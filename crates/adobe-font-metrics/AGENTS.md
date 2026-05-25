# ADOBE FONT METRICS KNOWLEDGE BASE

## OVERVIEW

`adobe-font-metrics` is a zero-dependency AFM v4.x parser used below Base-14 PDF metrics.

## WHERE TO LOOK

| Task           | Location                | Notes                                   |
| -------------- | ----------------------- | --------------------------------------- |
| Parser         | `src/lib.rs`            | Borrows input through `Cow`.            |
| Fixtures       | `tests/fixtures/*.afm`  | Vendored local AFM samples.             |
| Parser tests   | `tests/parser.rs`       | Real AFM and malformed record coverage. |
| Downstream use | `../pdf-base14-metrics` | Build-time parser consumer.             |

## CONVENTIONS

- Parse bad AFM as `ParseError`, not panic.
- Keep crate independently buildable; fixtures are intentionally local.
- `FontMetrics::into_owned()` exists for static/cache use.
- Unknown AFM records may be ignored; modeled malformed records should error.
- Cargo metadata is source truth; README may lag.

## ANTI-PATTERNS

- Do not add Mosaic, PDF writer, layout, shaping, or encoding dependencies here.
- Do not claim AFM v3 support without fixtures and tests.
- Do not silently accept malformed records already modeled by tests.
- Do not share fixtures from `pdf-base14-metrics`; independence matters.
