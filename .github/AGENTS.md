# GITHUB WORKFLOWS KNOWLEDGE BASE

## OVERVIEW

`.github/` owns CI, rustdoc deploy, and crates.io release automation. Sharp rocks here: branch, path
ignores, release tags.

## WHERE TO LOOK

| Task              | Location                  | Notes                                      |
| ----------------- | ------------------------- | ------------------------------------------ |
| CI                | `workflows/ci.yml`        | fmt, clippy, build, test, doc, MSRV.       |
| Rustdoc Pages     | `workflows/docs.yml`      | Builds `cargo dw`, copies `design/A4.svg`. |
| crates.io publish | `workflows/crates-io.yml` | Runs on `v*` tags/manual dispatch.         |

## CI RULES

- Branch is `master`, not `main`.
- CI ignores markdown/config-only paths listed in `paths-ignore`.
- `RUSTFLAGS=-D warnings` and `RUSTDOCFLAGS=-D warnings` make warnings fatal.
- `fmt` currently has `continue-on-error: true`; clippy/build/test/doc are hard gates.
- MSRV job uses Rust `1.95` and runs `cargo bwa`.

## DOCS DEPLOY

- Docs workflow deploys rustdoc to GitHub Pages.
- It copies `design/A4.svg` into `target/doc/assets` for logo/favicon use.
- Inline generated `target/doc/index.html` and CSS live in the workflow, not repo files.

## RELEASE RULES

- Release tags are `v<version>` and must match the `mos` package version.
- Workflow checks crates.io before publish and publishes workspace crates in dependency order.
- Trusted publishing is attempted first; `CARGO_REGISTRY_TOKEN` is fallback.
- Keep the tab-sensitive heredoc in `crates-io.yml` intact. Workflow even warns you. Nice omen.

## ANTI-PATTERNS

- Do not rename CI branch filters to `main`.
- Do not assume md-only changes run drift tests in CI.
- Do not loosen warning gates to hide real issues.
