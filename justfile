# https://just.systems

alias f := fmt
alias format := fmt

# List all available recipes.
default:
    @just --list

# Build every example and refresh its committed `<name>.pdf` snapshot
# next to `main.mos`. GitHub previews these inline; the regen target
# keeps them in sync with the current `mosaic-fonts` / `mosaic-pdf` state.
examples:
    #!/usr/bin/env bash
    set -euo pipefail
    for ex in examples/*/main.mos; do
    	dir="$(dirname "$ex")"
    	name="$(basename "$dir")"
    	(cd "$dir" && cargo mos build)
    	cp "$dir/build/main.pdf" "$dir/$name.pdf"
    	echo "regenerated $dir/$name.pdf"
    done

fmt:
    dprint fmt
