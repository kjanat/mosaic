# ZED MOSAIC KNOWLEDGE BASE

## OVERVIEW

`zed-mosaic` is a Zed extension for Mosaic files. It lives under `crates/` but is excluded from the
workspace on purpose.

## STRUCTURE

```text
zed-mosaic/
├── extension.toml              # Zed extension manifest, pinned grammar, `mos-lsp` language server
├── src/lib.rs                  # extension registration + `mos-lsp` language server spawn
├── languages/mosaic/*.scm      # copied Tree-sitter queries for Zed
├── languages/mosaic/config.toml
└── languages/mosaic/tasks.json # editor tasks around `mos build`
```

## WHERE TO LOOK

| Task             | Location                        | Notes                                       |
| ---------------- | ------------------------------- | ------------------------------------------- |
| Extension config | `extension.toml`                | Grammar pin + `[language_servers.mos-lsp]`. |
| Rust entry       | `src/lib.rs`                    | Registration + `mos-lsp` spawn/discovery.   |
| Language server  | `../mos-lsp/`                   | Source of truth; extension only spawns it.  |
| Query copies     | `languages/mosaic/*.scm`        | Generated from Tree-sitter query source.    |
| Canonical query  | `../tree-sitter-mosaic/queries` | Source of truth.                            |
| Sync command     | root `justfile`                 | `just sync-zed-queries`.                    |

## CONVENTIONS

- Run Cargo commands from this directory; root workspace excludes this crate.
- After canonical query edits, run `just sync-zed-queries` from repo root.
- Sync skips `locals.scm` and `tags.scm`; Zed does not load them under those names.
- Generated/local artifacts stay untracked: `*.wasm`, `grammars/`, `target/`, `Cargo.lock`.
- Keep grammar source in `tree-sitter-mosaic`, not here.
- `mos-lsp` binary discovery in `src/lib.rs`: settings `binary.path` → `mos-lsp` on `PATH`. Keep
  this order; surface a clear error when nothing resolves.
- Verify the crate (it is workspace-excluded) with:
  `cargo check --manifest-path crates/zed-mosaic/Cargo.toml --target wasm32-wasip2`. CI runs the
  same check. Install the server for live testing with `cargo mosils`.

## ANTI-PATTERNS

- Do not add compiler or package behavior here.
- Do not edit copied query files without updating canonical Tree-sitter queries.
- Do not assume this crate is checked by `cargo check --workspace`.
- Do not make Zed tasks smarter than real CLI behavior.
