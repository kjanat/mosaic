# CARGO CONFIG KNOWLEDGE BASE

## OVERVIEW

`.cargo/` holds project Cargo aliases and nightly rustdoc config. Small cave, sharp tools.

## WHERE TO LOOK

| Task           | Location       | Notes                                           |
| -------------- | -------------- | ----------------------------------------------- |
| Stable aliases | `config.toml`  | Build/check/test/lint/run/install shortcuts.    |
| Nightly docs   | `nightly.toml` | Extra rustdoc flags used by `just doc-nightly`. |

## ALIASES

- `cargo bw`: build workspace.
- `cargo bwa`: build workspace all targets.
- `cargo cw`: check workspace all targets.
- `cargo tw`: test workspace.
- `cargo lint`: clippy workspace all targets/features with warnings fatal.
- `cargo mos -- ...`: run release `mos` CLI.
- `cargo mosls`: run release `mos-lsp`.
- `cargo mosi` / `cargo mosils`: install local binaries with `--force`.

## CONVENTIONS

- Keep aliases non-interactive and CI-friendly.
- If CI uses an alias, update `.github/AGENTS.md` and workflow docs when changing it.
- `dwn` is intended through `rustup run nightly -- cargo dwn` or `just doc-nightly`.

## ANTI-PATTERNS

- Do not hide warning gates behind aliases; CI depends on fatal warnings.
- Do not make aliases mutate source or VCS state.
- Do not assume `crates/zed-mosaic` is covered by workspace aliases.
