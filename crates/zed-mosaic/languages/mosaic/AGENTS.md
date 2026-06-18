# ZED MOSAIC LANGUAGE BUNDLE KNOWLEDGE BASE

## OVERVIEW

This directory is Zed's copied Mosaic language bundle: config, tasks, and query files. Canonical
queries live in `../../../tree-sitter-mosaic/queries`.

## WHERE TO LOOK

| Task            | Location      | Notes                                      |
| --------------- | ------------- | ------------------------------------------ |
| Zed language    | `config.toml` | Language name, extensions, grammar wiring. |
| Zed tasks       | `tasks.json`  | Editor tasks for Mosaic commands.          |
| Highlight/query | `*.scm`       | Copied from Tree-sitter query sources.     |

## CONVENTIONS

- Edit canonical query files in `crates/tree-sitter-mosaic/queries` first.
- Regenerate copies with `just sync-zed-queries` from repo root.
- `locals.scm` and `tags.scm` are intentionally not copied by the sync script.

## ANTI-PATTERNS

- Do not patch copied `*.scm` files here without updating canonical Tree-sitter queries.
- Do not assume workspace Cargo commands verify this extension; use the Zed crate guide.
- Do not edit `../grammars/mosaic` as source; it can be an ignored local clone/build artifact.
