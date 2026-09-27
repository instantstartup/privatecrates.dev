// See https://svelte.dev/docs/kit/types#app.d.ts
declare global {
	/** True only in `pnpm dev:mock` (VITE_MOCK_API=1): the account API is served by an in-browser mock. */
	const __MOCK_API__: boolean;

	namespace App {
		// interface Error {}
		// interface Locals {}
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}
}

export {};
