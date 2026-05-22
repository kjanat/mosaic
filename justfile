# https://just.systems

alias f := fmt
alias format := fmt

# List all available recipes.
default:
    @just --list

# Build every example and refresh its committed `<name>.pdf` snapshot
# next to `main.mos`. GitHub previews these inline; the regen target
# keeps them in sync with the current `mos-fonts` / `mos-pdf` state.
examples:
    #!/usr/bin/env bash
    set -euo pipefail
    shopt -s nullglob
    files=(examples/*/main.mos)
    if [ ${#files[@]} -eq 0 ]; then
    	echo "no examples found under examples/*/main.mos"
    	exit 0
    fi
    for ex in "${files[@]}"; do
    	dir="$(dirname "$ex")"
    	name="$(basename "$dir")"
    	(cd "$dir" && cargo mos build)
    	cp "$dir/build/main.pdf" "$dir/$name.pdf"
    	echo "regenerated $dir/$name.pdf"
    done

fmt:
    dprint fmt

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
