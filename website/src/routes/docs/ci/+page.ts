import type { TocItem } from '$lib/components/Toc.svelte';

// Sections of this page, for the docs layout's "On this page" list.
export function load(): { toc: TocItem[] } {
	return {
		toc: [
			['workflow', 'The workflow'],
			['how', 'What happens'],
			['elsewhere', 'Docker builds and CI outside GitHub Actions']
		]
	};
}
