// Ambient shim: `node-gyp-build` ships no types. This is the minimum
// surface our index.js touches; the default export accepts a package
// root and returns the resolved native binding.
declare module 'node-gyp-build' {
	const build: (root: string) => unknown;
	export default build;
}
