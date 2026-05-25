# MOS CLI KNOWLEDGE BASE

## OVERVIEW

`mos` is CLI orchestration only. It wires packages, parse, eval, layout, and PDF; compiler behavior
belongs in owning crates.

## CURRENT SCOPE

Implemented:

- `mos check`: accepts `.mos` files or project dirs, lowers, resolves refs, prints diagnostics.
- `mos build`: emits PDF for direct files or directory projects with `project.entry` /
  `[output].pdf`.
- Multiple entries; non-`.mos` files are skipped only when many entries are supplied.
- `--open` after successful PDF build.

Parsed/stubbed:

- `init`, `watch`, `fmt`, `test`, `profile`, `clean`, `package`.
- `--frozen` and `--reproducible` flags.

## WHERE TO LOOK

| Task             | Location          | Notes                                 |
| ---------------- | ----------------- | ------------------------------------- |
| Command wiring   | `src/main.rs`     | Clap types and command dispatch.      |
| Entry resolution | `collect_entries` | Files, dirs, project manifests.       |
| Check pipeline   | `run_check`       | Parse/lower/diagnostic exit behavior. |
| Build pipeline   | `run_build`       | Layout/PDF path and `--open`.         |
| Black-box tests  | `tests/cli.rs`    | Real binary via `CARGO_BIN_EXE_mos`.  |

## CONVENTIONS

- Keep logic thin: call crate APIs, print diagnostics, map errors to process exits.
- Path safety for declared PDF output lives here today.
- User-facing failures go through `CoreError`/diagnostics, not panics.
- CLI tests use temp projects and structural PDF checks through `lopdf` when needed.

## ANTI-PATTERNS

- Do not add parser/evaluator/layout/PDF policy here.
- Do not make stub subcommands silently succeed.
- Do not expand package registry/cache semantics just because flags or manifest fields exist.
- Do not claim HTML/watch/format/package behavior until command tests prove it.
