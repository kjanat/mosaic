# https://just.systems
set unstable

alias f := fmt
alias format := fmt

# https://npm.im/runner-run
runner := "node_modules" / ".bin" / "runner"
runner-package := "--package=runner-run"

bootstrap-runner := if path_exists(runner) == "true" { quote(runner) } else if which("runner") != "" { "runner" } else if which("bunx") != "" { "bunx " + runner-package + " runner" } else { "npx -y " + runner-package + " runner" }

# List all available recipes.
default:
    just --list

setup:
    @{{ bootstrap-runner }} install

# Build every example project and refresh committed PDF snapshots.
examples: setup
    {{ runner }} mos build examples/*

fmt: setup
    {{ runner }} fmt

# Build docs with nightly-only rustdoc config.
doc-nightly: setup
    rustup run nightly -- {{ runner }} dwn

# Sync the Zed extension's query files from the canonical
# `tree-sitter-mosaic/queries/` sources. Zed does not load Tree-sitter
# `locals.scm` or `tags.scm` under those filenames.
sync-zed-queries:
    #!/usr/bin/env bash
    set -euo pipefail
    src=crates/tree-sitter-mosaic/queries
    dst=crates/zed-mosaic/languages/mosaic
    for path in "$src"/*.scm; do
      query="$(basename "$path")"
      case "$query" in
        locals.scm|tags.scm) continue ;;
      esac
      cp "$path" "$dst/$query"
    done
