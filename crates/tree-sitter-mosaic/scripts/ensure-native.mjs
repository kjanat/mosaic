#!/usr/bin/env node
/**
 * This script ensures that the native bindings for tree-sitter are built
 * and available.
 * It first runs the build process using node-gyp-build, and then checks
 * if the tree-sitter module is present in node_modules.
 *
 * If it is, it attempts to require it, and if that fails
 * (indicating that the native bindings are missing),
 * it triggers a rebuild of the tree-sitter module.
 */
import { execSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';

execSync('node-gyp-build', { stdio: 'inherit' });

if (existsSync('node_modules/tree-sitter')) {
	const require = createRequire(import.meta.url);
	try {
		require('tree-sitter');
	} catch (err) {
		console.warn(
			`tree-sitter native binding unavailable (${err?.message ?? err}); rebuilding...`,
		);
		execSync('npm rebuild tree-sitter', { stdio: 'inherit' });
		// Re-verify: if the rebuild didn't fix it, surface the failure
		// instead of letting downstream `import 'tree-sitter'` crash.
		require('tree-sitter');
	}
}
