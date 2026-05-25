# ZED MOSAIC KNOWLEDGE BASE

## OVERVIEW

`zed-mosaic` is a Zed extension for Mosaic files. It lives under `crates/` but is excluded from the
workspace on purpose.

## STRUCTURE

```text
zed-mosaic/
├── extension.toml              # Zed extension manifest and pinned grammar
├── src/lib.rs                  # tiny extension registration
├── languages/mosaic/*.scm      # copied Tree-sitter queries for Zed
├── languages/mosaic/config.toml
└── languages/mosaic/tasks.json # editor tasks around `mos build`
```

## WHERE TO LOOK

| Task             | Location                        | Notes                                    |
| ---------------- | ------------------------------- | ---------------------------------------- |
| Extension config | `extension.toml`                | Grammar repo/rev/path pin.               |
| Rust entry       | `src/lib.rs`                    | `register_extension!` only.              |
| Query copies     | `languages/mosaic/*.scm`        | Generated from Tree-sitter query source. |
| Canonical query  | `../tree-sitter-mosaic/queries` | Source of truth.                         |
| Sync command     | root `justfile`                 | `just sync-zed-queries`.                 |

## CONVENTIONS

- Run Cargo commands from this directory; root workspace excludes this crate.
- After canonical query edits, run `just sync-zed-queries` from repo root.
- Sync skips `locals.scm` and `tags.scm`; Zed does not load them under those names.
- Generated/local artifacts stay untracked: `*.wasm`, `grammars/`, `target/`, `Cargo.lock`.
- Keep grammar source in `tree-sitter-mosaic`, not here.

## ANTI-PATTERNS

- Do not add compiler or package behavior here.
- Do not edit copied query files without updating canonical Tree-sitter queries.
- Do not assume this crate is checked by `cargo check --workspace`.
- Do not make Zed tasks smarter than real CLI behavior.
