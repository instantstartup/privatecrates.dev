import js from '@eslint/js';
import prettier from 'eslint-config-prettier';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

export default ts.config(
	{ ignores: ['build/', '.svelte-kit/', 'node_modules/'] },
	js.configs.recommended,
	...ts.configs.recommended,
	...svelte.configs.recommended,
	prettier,
	...svelte.configs.prettier,
	{
		languageOptions: { globals: { ...globals.browser, ...globals.node } },
		rules: {
			// The site has no base path and every link is a root-relative path to a prerendered page.
			'svelte/no-navigation-without-resolve': 'off',
			// Svelte trims the whitespace at a block branch's edges, entities included, so `{' '}` is how a branch keeps the
			// space that separates it from the text before it (scripts/check-paid-copy.js).
			'svelte/no-useless-mustaches': 'off'
		}
	},
	{
		files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
		languageOptions: {
			parserOptions: {
				projectService: true,
				extraFileExtensions: ['.svelte'],
				parser: ts.parser
			}
		}
	}
);
