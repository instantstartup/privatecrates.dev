import type { TocItem } from '$lib/components/Toc.svelte';

// Sections of this page, for the docs layout's "On this page" list.
export function load(): { toc: TocItem[] } {
	return {
		toc: [
			['workflow', 'Run it on every publish'],
			['checks', 'What it reports'],
			['results', 'Reading the results'],
			['options', 'Options']
		]
	};
}
