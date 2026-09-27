import type { TocItem } from '$lib/components/Toc.svelte';

// Sections of this page, for the docs layout's "On this page" list.
export function load(): { toc: TocItem[] } {
	return {
		toc: [
			['install', 'Install and sign in'],
			['editors', 'Editors and background builds'],
			['not-found', 'A crate is “not found”'],
			['troubleshooting', 'Other problems'],
			['agent', 'With a coding agent']
		]
	};
}
