import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig, loadEnv } from 'vite';

export default defineConfig(({ command, mode }) => ({
	plugins: [tailwindcss(), sveltekit()],
	define: {
		// A compile-time constant, so production builds drop the mock API entirely. It is only ever enabled for
		// the dev server (`pnpm dev:mock`), never for `vite build`, whatever the environment says.
		__MOCK_API__: JSON.stringify(command === 'serve' && loadEnv(mode, '.', 'VITE_').VITE_MOCK_API === '1')
	}
}));
