import type { TocItem } from '$lib/components/Toc.svelte';

// Sections of this page, for the docs layout's "On this page" list.
export function load(): { toc: TocItem[] } {
	return {
		toc: [
			['boundary', 'The trust boundary'],
			['threats', 'Threats and outcomes'],
			['permissions', 'Permissions are GitHub’s'],
			['controls', 'Controls'],
			['integrity', 'Integrity and provenance'],
			['source', 'Source code']
		]
	};
}
