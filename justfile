# https://just.systems

alias f := fmt
alias format := fmt

# List all available recipes.
default:
    @just --list

# Build every example project and refresh committed PDF snapshots.
examples:
    cargo mos build examples/*

fmt:
    dprint fmt

# Build docs with nightly-only rustdoc config.
dwn:
    rustup run nightly -- cargo dwn

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
