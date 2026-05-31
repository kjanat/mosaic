// @ts-check
/// <reference path="./node-gyp-build.d.ts" />
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

/** @typedef {import('tree-sitter').BaseNode} BaseNode */
/** @typedef {import('tree-sitter').ChildNode} ChildNode */
/**
 * One entry in `node-types.json`. Either a polymorphic node with `subtypes`,
 * or a structural node with `fields` and a single `children` descriptor.
 *
 * Note: this overrides `tree-sitter`'s own `NodeInfo`, which types `children`
 * as `ChildNode[]`. The on-disk schema emits a single `ChildNode` object.
 *
 * @see {@link https://tree-sitter.github.io/tree-sitter/using-parsers/6-static-node-types Static Node Types}
 *
 * @typedef {import('tree-sitter').NodeInfo} NodeInfo
 */

/** @typedef {'HIGHLIGHTS_QUERY' | 'INJECTIONS_QUERY' | 'LOCALS_QUERY' | 'TAGS_QUERY'} QueryKey */

/**
 * The tree-sitter language binding for this grammar. Extends `Parser.Language`
 * with the lazy-loaded query strings.
 *
 * @see {@link https://tree-sitter.github.io/node-tree-sitter/interfaces/Parser.Language.html Parser.Language}
 *
 * @example
 * import Parser from 'tree-sitter';
 * import Mosaic from 'tree-sitter-mosaic';
 *
 * const parser = new Parser();
 * parser.setLanguage(Mosaic);
 *
 * @typedef {import('tree-sitter').Language & {
 *   HIGHLIGHTS_QUERY?: string;
 *   INJECTIONS_QUERY?: string;
 *   LOCALS_QUERY?: string;
 *   TAGS_QUERY?: string;
 * }} Binding
 */

const root = fileURLToPath(new URL('../..', import.meta.url));

const bindingModule = process.versions.bun
	// Bun rejects `import` of a Node-API `.node`; load it through
	// `require` (as the `tree-sitter` runtime itself does) from the
	// `prebuilds/<platform>-<arch>/` path bun expects.
	? createRequire(import.meta.url)(
		`${root}/prebuilds/${process.platform}-${process.arch}/tree-sitter-mosaic.node`,
	)
	: (await import('node-gyp-build')).default(root);

const binding = /** @type {Binding} */ (bindingModule?.default ?? bindingModule);

try {
	const nodeTypes = await import(`${root}/src/node-types.json`, { with: { type: 'json' } });
	binding.nodeTypeInfo = nodeTypes.default;
} catch {}

/** @type {Array<[QueryKey, string]>} */
const queries = [
	['HIGHLIGHTS_QUERY', `${root}/queries/highlights.scm`],
	['INJECTIONS_QUERY', `${root}/queries/injections.scm`],
	['LOCALS_QUERY', `${root}/queries/locals.scm`],
	['TAGS_QUERY', `${root}/queries/tags.scm`],
];

for (const [prop, path] of queries) {
	Object.defineProperty(binding, prop, {
		configurable: true,
		enumerable: true,
		get() {
			delete binding[prop];
			try {
				binding[prop] = readFileSync(path, 'utf8');
			} catch (err) {
				const message = err instanceof Error ? err.message : String(err);
				console.error(`Failed to load ${prop} from ${path}: ${message}`);
				throw err;
			}
			return binding[prop];
		},
	});
}

export default binding;
