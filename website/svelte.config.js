import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),
	kit: {
		adapter: adapter({
			pages: 'build',
			assets: 'build',
			// Unknown paths: the server serves 404.html, a client-rendered shell that shows the themed error page.
			fallback: '404.html',
			strict: true
		}),
		// Absolute asset paths, so the bootstrap scripts extracted by scripts/externalise-inline.js
		// resolve the same way from any page.
		paths: { relative: false },
		prerender: {
			handleHttpError: 'fail',
			handleMissingId: 'fail'
		}
	}
};

export default config;
