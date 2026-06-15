#!/usr/bin/env node
/**
 * Build the native addons this grammar needs and place them where bun's
 * loader expects them.
 *
 * Two addons are involved, and each `bindings/node/index.js` loads them
 * from `prebuilds/<platform>-<arch>/<name>.node` when running under bun —
 * whereas node-gyp emits to `build/Release/<target>.node`, and bun does
 * not run a dependency's own install script, so the `tree-sitter` runtime
 * addon is never compiled by `bun install` either. This script bridges
 * both gaps for a fresh `bun install`:
 *
 *   1. this grammar's addon            -> prebuilds/<p-a>/tree-sitter-mosaic.node
 *   2. the `tree-sitter` runtime addon -> prebuilds/<p-a>/tree-sitter.node
 *
 * Under plain Node both index.js files resolve through `node-gyp-build`
 * (which reads `build/Release`), so this is bun-shaped but harmless there.
 */
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const platformDir = `${process.platform}-${process.arch}`;

// node-gyp ships bundled inside npm, which lives next to the running node
// binary (`<prefix>/bin/node` -> `<prefix>/lib/node_modules/npm`). Deriving
// it from `process.execPath` avoids depending on `node-gyp` being on PATH.
const bundledNodeGyp = join(
	dirname(dirname(process.execPath)),
	'lib',
	'node_modules',
	'npm',
	'node_modules',
	'node-gyp',
	'bin',
	'node-gyp.js',
);

// Locate a `node-gyp.js` to run with the current node binary, in order of
// preference: the copy bundled with npm (present on most Unix node installs),
// then the `node-gyp` dev dependency resolved from this package. The bundled
// path uses npm's Unix layout (`<prefix>/lib/node_modules/npm`), which does
// not exist on Windows, so the dev-dependency fallback is what makes a fresh
// `bun install` succeed there.
function resolveNodeGyp() {
	if (existsSync(bundledNodeGyp)) {
		return bundledNodeGyp;
	}
	try {
		return require.resolve('node-gyp/bin/node-gyp.js');
	} catch {
		return null;
	}
}

function runNodeGyp(cwd) {
	const nodeGyp = resolveNodeGyp();
	if (nodeGyp) {
		execFileSync(process.execPath, [nodeGyp, 'rebuild'], { cwd, stdio: 'inherit' });
	} else {
		// Last resort: a PATH-resolved node-gyp.
		execFileSync('node-gyp', ['rebuild'], { cwd, stdio: 'inherit' });
	}
}

/**
 * Ensure `pkgDir`'s gyp addon is compiled and copied to the
 * `prebuilds/<platform>-<arch>/<prebuildName>.node` path bun loads from.
 * Idempotent: skips the compile when `build/Release/<builtName>` already
 * exists, and always refreshes the prebuild copy.
 */
function ensureAddon(pkgDir, builtName, prebuildName) {
	const built = join(pkgDir, 'build', 'Release', builtName);
	if (!existsSync(built)) {
		runNodeGyp(pkgDir);
	}
	if (!existsSync(built)) {
		throw new Error(`node-gyp did not produce ${built}`);
	}
	const prebuild = join(pkgDir, 'prebuilds', platformDir, `${prebuildName}.node`);
	mkdirSync(dirname(prebuild), { recursive: true });
	copyFileSync(built, prebuild);
}

// 1. This grammar. The script lives in `<pkg>/scripts/`, so the package
//    root is one directory up.
const grammarRoot = fileURLToPath(new URL('..', import.meta.url));
ensureAddon(grammarRoot, 'tree_sitter_mosaic_binding.node', 'tree-sitter-mosaic');

// 2. The `tree-sitter` runtime addon — a dependency bun won't build itself.
const treeSitterRoot = dirname(require.resolve('tree-sitter/package.json'));
ensureAddon(treeSitterRoot, 'tree_sitter_runtime_binding.node', 'tree-sitter');
