import type { TocItem } from '$lib/components/Toc.svelte';

// Sections of this page, for the docs layout's "On this page" list.
export function load(): { toc: TocItem[] } {
	return {
		toc: [
			['trusted', 'Trusted publishing from GitHub Actions'],
			['first-publish', 'A crate’s first publish'],
			['owners', 'The owners file'],
			['laptop', 'Publishing from a developer’s machine'],
			['yank', 'Yanking'],
			['clashes', 'Names that exist on crates.io']
		]
	};
}
