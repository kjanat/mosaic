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
# `tree-sitter-mosaic/queries/` sources. Zed loads highlights + injections;
# `locals.scm` and `tags.scm` are tree-sitter-only and not copied.
sync-zed-queries:
    cp crates/tree-sitter-mosaic/queries/highlights.scm crates/zed-mosaic/languages/mosaic/highlights.scm
    cp crates/tree-sitter-mosaic/queries/injections.scm crates/zed-mosaic/languages/mosaic/injections.scm

# Refresh the `tree-sitter-mosaic-root` branch. Zed's extension loader
# clones `[grammars.mosaic].repository` and expects `grammar.js` at the
# repo root, but the grammar lives in `crates/tree-sitter-mosaic/`. We
# `subtree split` that subdirectory into its own branch whose root IS the
# grammar, and Zed fetches from that branch. Run this after any change
# inside `crates/tree-sitter-mosaic/`.
refresh-zed-grammar:
    git subtree split --prefix=crates/tree-sitter-mosaic --rejoin -b tree-sitter-mosaic-root
    @echo "tree-sitter-mosaic-root updated. Push with:"
    @echo "    git push origin tree-sitter-mosaic-root"
