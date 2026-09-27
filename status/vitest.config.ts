import { defineConfig } from 'vitest/config';

// Plain Node: the logic under test is pure, and D1 is exercised through node:sqlite (test/d1.ts).
export default defineConfig({
	test: {
		include: ['test/**/*.test.ts'],
		environment: 'node',
	},
});
