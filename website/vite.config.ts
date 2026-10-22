import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig, loadEnv } from 'vite';
import { thirdPartyLicenses } from './scripts/third-party-licenses.js';

export default defineConfig(({ command, mode }) => ({
	plugins: [
		tailwindcss(),
		sveltekit({
			preprocess: vitePreprocess(),
			adapter: adapter({
				pages: 'build',
				assets: 'build',
				// Unknown paths: the server serves 404.html, a client-rendered shell that shows the themed error page.
				fallback: '404.html',
				strict: true
			}),
			// SvelteKit 3 dropped the built-in `$lib` alias in favour of `#lib` subpath imports; keep `$lib` until
			// the imports under src/ move over.
			alias: { $lib: 'src/lib' },
			// Absolute asset paths, so the bootstrap scripts extracted by scripts/externalise-inline.js
			// resolve the same way from any page.
			paths: { relative: false },
			prerender: {
				handleHttpError: 'fail',
				handleMissingId: 'fail'
			}
		}),
		thirdPartyLicenses()
	],
	define: {
		// A compile-time constant, so production builds drop the mock API entirely. It is only ever enabled for
		// the dev server (`pnpm dev:mock`), never for `vite build`, whatever the environment says.
		__MOCK_API__: JSON.stringify(command === 'serve' && loadEnv(mode, '.', 'VITE_').VITE_MOCK_API === '1')
	}
}));
