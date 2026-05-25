# Mosaic diagnostic code catalog

Authoritative list of diagnostic codes currently emitted by the compiler. Codes are stable
identifiers (`E0XX`/`W0XX`) the CLI prints as `severity[CODE]: message` (see
`crates/mos/src/main.rs::render_diagnostic`). Editor integrations, golden tests, and downstream
tooling key off these strings; do not renumber or repurpose them.

`E` prefix is `Severity::Error` (non-zero exit from `mos check`/`mos build`). `W` prefix is
`Severity::Warning` (informational only; the build still succeeds).

Truth source is the code itself — grep `DiagnosticCode("…")` and the parser/inline `self.warn("…")`
shortcut. Update this file in the same change that adds, renames, or retires a code.

## Number ranges

Ranges are conventions, not enforced. Keep new codes inside the band for the layer that emits them
so the catalog stays scannable.

| Range       | Layer / crate                       | Notes                                              |
| ----------- | ----------------------------------- | -------------------------------------------------- |
| `E001`      | reserved / example                  | Used only in `mos-core` doctests, not emitted.     |
| `E010–E019` | `mos-parse` directive surface       | `#set` / `#image` / `#figure` shape errors.        |
| `W020–W029` | `mos-parse` inline + `mos-eval` set | Recoverable inline + semantic sanity warnings.     |
| `E020–E029` | `mos-eval` `#set` + `mos-layout`    | Semantic `#set` errors, layout-level value errors. |
| `E041–E049` | `mos-eval` resolver                 | Label / reference resolution.                      |
| `W040–W049` | `mos-pdf` encoding / fonts          | Encoding budget + font fallback warnings.          |
| `E050–E059` | `mos-eval` image lowering + I/O     | Image directive validity and on-disk loading.      |
| `W050–W059` | `mos-layout` image step             | Layout-stage image fallbacks.                      |
| `E090–E099` | `mos-pdf` backend                   | PDF emission / font subsetting / I/O.              |

## Errors

| Code   | Owner crate / module                             | Triggered by                                                                                                                                                                              |
| ------ | ------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `E010` | `mos-parse` `directive.rs`                       | `#set` not followed by an identifier (`expected an identifier after #set`).                                                                                                               |
| `E011` | `mos-parse` `directive.rs`                       | Missing `(` after `#set NAME`, `#image`, or `#figure`; raw `#…` form not using long brackets.                                                                                             |
| `E012` | `mos-parse` `directive.rs`                       | Unterminated `#NAME(...)` or `#NAME[[...]]` block.                                                                                                                                        |
| `E013` | `mos-parse` `directive.rs`                       | Unexpected trailing content after a `#NAME(...)` or raw `#NAME[[...]]` block on the same line.                                                                                            |
| `E014` | `mos-parse` `directive.rs`                       | Malformed directive argument value: bad escape, unknown unit, unterminated string, lone `-`, malformed number/length.                                                                     |
| `E015` | `mos-parse` `directive.rs`, `mos-eval` `set.rs`  | Argument list shape: missing `:`, missing `,`/`)`, positional arg where named expected, or `#set` rejecting positional.                                                                   |
| `E020` | `mos-eval` `set.rs`                              | Unknown `#set` target (`#set` allows only `page`, `text`, `document`, `image`).                                                                                                           |
| `E021` | `mos-eval` `set.rs`, `mos-eval` `image_lower.rs` | Unknown keyword argument for `#set TARGET`, `#image`, or `#figure`.                                                                                                                       |
| `E022` | `mos-eval` `set.rs`, `mos-eval` `image_lower.rs` | Argument type mismatch (e.g. length expected, string given) or non-positive length.                                                                                                       |
| `E023` | `mos-layout` `style.rs`                          | Unknown paper size in `#set page(paper: ...)`.                                                                                                                                            |
| `E025` | `mos-layout` `style.rs`                          | `#set` value is well-typed but breaks page geometry (e.g. text size larger than vertical space); previous value retained.                                                                 |
| `E041` | `mos-eval` `resolve.rs`                          | Label declared more than once. First declaration wins and is kept in the index; duplicate carries a note pointing back at the original.                                                   |
| `E042` | `mos-eval` `resolve.rs`                          | `@label` reference to a label that does not exist. Reference text stays at the lowered `?label?` placeholder so it remains visible in rendered output.                                    |
| `E050` | `mos-eval` `image_lower.rs`                      | `#image(...)` or `#figure(...)` missing a path argument (or path is an empty/whitespace string). Both surfaces share `build_image_attributes`; the rendered message is `#image`-flavored. |
| `E051` | `mos-eval` `image.rs`                            | Image file cannot be read from disk (resolver I/O failure).                                                                                                                               |
| `E052` | `mos-eval` `image.rs`                            | Image file cannot be decoded (unsupported or corrupt PNG/JPEG).                                                                                                                           |
| `E090` | `mos-pdf` `lib.rs`                               | PDF backend I/O failure (cannot create output directory or write PDF bytes).                                                                                                              |
| `E091` | `mos-pdf` `embedded.rs`                          | Font subsetting failure for an embedded face.                                                                                                                                             |
| `E092` | `mos-pdf` `content.rs`                           | Internal: missing embedded font plan for a shaped run. Indicates an upstream layout/font-plan bug, not author input.                                                                      |

## Warnings

| Code   | Owner crate / module    | Triggered by                                                                                                                   |
| ------ | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `W020` | `mos-parse` `inline.rs` | Unterminated `**strong**` run; treated as literal text.                                                                        |
| `W021` | `mos-parse` `inline.rs` | Unterminated `*emphasis*` run; treated as literal text.                                                                        |
| `W023` | `mos-parse` `inline.rs` | Stray `@` not followed by a label identifier; treated as text.                                                                 |
| `W024` | `mos-eval` `set.rs`     | `#set` value passes typing but trips a sanity floor (e.g. a margin below the renderable minimum); value is still applied.      |
| `W025` | `mos-parse` `inline.rs` | Lone trailing `\` at end of input; treated as literal text.                                                                    |
| `W041` | `mos-pdf` `encoding.rs` | Base-14 `/Differences` glyph budget exhausted; some characters could not be encoded in the 256-slot extended table for a face. |
| `W045` | `mos-fonts` `family.rs` | Unknown font family in `#set text(font: ...)`; falling back to bundled Noto Sans.                                              |
| `W050` | `mos-layout` `image.rs` | Image node reached layout without decoded pixel data; image is skipped on the page.                                            |

## Reserved / retired

- `E001`: reserved as a doctest example in `mos-core::DiagnosticCode`. Not emitted by any compiler
  stage. Do not assign to a real diagnostic without updating the doctests first.
- `W040`: retired. `crates/mos-layout/src/lib.rs` keeps a regression test asserting it is never
  emitted; do not reuse the number for a new code.

## CLI rendering

`mos check` and `mos build` render every diagnostic through
`crates/mos/src/main.rs::render_diagnostic` in the form:

```text
error[E042]: unknown label `intro` in `@` reference
  --> main.mos:3:5
   |
 3 | see @intro
   |     ^^^^^^
  note: ...
  help: ...
```

The bracketed code is always `diag.code.0` verbatim. CLI integration tests in
`crates/mos/tests/cli.rs` lock the rendered prefix (`error[E041]`, `error[E042]`, `error[E023]`,
`error[E012]`, `warning[W021]`, `E051`) against drift. Tests that exercise specific codes end-to-end
should keep using the `severity[CODE]` substring rather than re-parsing the caret block.
