# MOS PACKAGES KNOWLEDGE BASE

## OVERVIEW

`mos-packages` parses `mosaic.toml`. It is not a package manager or registry client yet.

## CURRENT SCOPE

Implemented:

- `ProjectManifest::load` from TOML.
- `[project]`, `[document]`, `[output]`, and `[dependencies]` schema.
- `deny_unknown_fields` for manifest structs.
- Optional `output.pdf` used by CLI directory builds.

Not implemented:

- Registry fetch, lockfiles, dependency solving, version validation, frozen builds.
- HTML/EPUB output selection despite manifest shape.

## WHERE TO LOOK

| Task       | Location             | Notes                                 |
| ---------- | -------------------- | ------------------------------------- |
| Public API | `src/lib.rs`         | Manifest structs and load errors.     |
| CLI use    | `../mos/src/main.rs` | Entry and output path interpretation. |
| Tests      | inline tests         | Small schema coverage.                |

## CONVENTIONS

- Keep schema parsing separate from CLI path policy.
- Use serde/TOML errors for bad manifests; do not panic.
- Add tests when manifest fields start affecting real behavior.
- Keep package registry dreams out until that slice is implemented.

## ANTI-PATTERNS

- Do not fetch dependencies or touch caches from this crate.
- Do not validate output paths here unless ownership is deliberately moved from CLI.
- Do not claim `document.output = ["html"]` is active because it parses.
