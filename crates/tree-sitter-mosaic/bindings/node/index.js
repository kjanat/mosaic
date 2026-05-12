// @ts-check
/// <reference path="./node-gyp-build.d.ts" />
import { readFileSync } from 'node:fs';
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
 * @typedef {(BaseNode & { subtypes: BaseNode[] })
 *   | (BaseNode & { fields: Record<string, ChildNode>, children: ChildNode })} NodeInfo
 */

/** @typedef {'HIGHLIGHTS_QUERY' | 'INJECTIONS_QUERY' | 'LOCALS_QUERY' | 'TAGS_QUERY'} QueryKey */

/**
 * The tree-sitter language binding for this grammar.
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
 * @typedef {Object} Binding
 * @property {unknown} language - The inner language object.
 * @property {NodeInfo[]} [nodeTypeInfo] - The content of `node-types.json` for this grammar, if bundled.
 * @property {string} [HIGHLIGHTS_QUERY] - The syntax highlighting query for this grammar.
 * @property {string} [INJECTIONS_QUERY] - The language injection query for this grammar.
 * @property {string} [LOCALS_QUERY] - The local variable query for this grammar.
 * @property {string} [TAGS_QUERY] - The symbol tagging query for this grammar.
 */

const root = fileURLToPath(new URL('../..', import.meta.url));

const bindingModule = typeof process.versions.bun === 'string'
	// Support `bun build --compile` by being statically analyzable enough to find the .node file at build-time
	? await import(`${root}/prebuilds/${process.platform}-${process.arch}/tree-sitter-mosaic.node`)
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
			} catch {}
			return binding[prop];
		},
	});
}

export default binding;
