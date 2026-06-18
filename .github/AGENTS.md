# GITHUB WORKFLOWS KNOWLEDGE BASE

## OVERVIEW

`.github/` owns CI, rustdoc deploy, binary releases, and crates.io automation. Sharp rocks here:
branch, path ignores, release tags.

## WHERE TO LOOK

| Task              | Location                         | Notes                                 |
| ----------------- | -------------------------------- | ------------------------------------- |
| CI                | `workflows/ci.yml`               | fmt, clippy, build, test, doc, MSRV.  |
| Rustdoc Pages     | `workflows/docs.yml`             | Calls local Pages composite action.   |
| Pages action      | `actions/pages/action.yml`       | Builds docs index/assets for Pages.   |
| GitHub release    | `workflows/release.yml`          | Release notes + binary workflow call. |
| Binary release    | `workflows/release-binaries.yml` | Builds `mos-lsp` release assets.      |
| crates.io publish | `workflows/crates-io.yml`        | Runs on `v*` tags/manual dispatch.    |

## CI RULES

- Branch is `master`, not `main`.
- CI ignores markdown/config-only paths listed in `paths-ignore`.
- `RUSTFLAGS=-D warnings` and `RUSTDOCFLAGS=-D warnings` make warnings fatal.
- `fmt` currently has `continue-on-error: true`; clippy/build/test/doc are hard gates.
- MSRV job uses Rust `1.96` and runs `cargo bwa`.

## DOCS DEPLOY

- Docs workflow deploys rustdoc to GitHub Pages.
- It delegates site assembly to `.github/actions/pages`.
- The action builds `cargo dw`, writes `target/doc/index.html`, and copies `design/A4.svg`.

## RELEASE RULES

- Release tags are `v<version>` and must match the `mos` package version.
- Binary release workflow publishes `mos-lsp` assets; do not assume it ships `mos` too.
- Workflow checks crates.io before publish and publishes workspace crates in dependency order.
- Trusted publishing is attempted first; `CARGO_REGISTRY_TOKEN` is fallback.
- Keep the tab-sensitive heredoc in `crates-io.yml` intact. Workflow even warns you. Nice omen.

## ANTI-PATTERNS

- Do not rename CI branch filters to `main`.
- Do not assume md-only changes run drift tests in CI.
- Do not loosen warning gates to hide real issues.
