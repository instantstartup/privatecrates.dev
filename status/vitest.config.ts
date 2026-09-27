import { join } from 'node:path';
import { defineConfig } from 'vitest/config';

// Plain Node: the logic under test is pure, and D1 is exercised through node:sqlite (test/helpers.ts).
export default defineConfig({
	resolve: {
		// A Workers runtime module, imported by src/index.ts; see the stub.
		alias: { 'cloudflare:email': join(import.meta.dirname, 'test/stubs/cloudflare-email.ts') },
	},
	test: {
		include: ['test/**/*.test.ts'],
		environment: 'node',
	},
});
