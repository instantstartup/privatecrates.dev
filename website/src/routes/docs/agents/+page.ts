import type { TocItem } from '$lib/components/Toc.svelte';

// Sections of this page, for the docs layout's "On this page" list.
export function load(): { toc: TocItem[] } {
	return {
		toc: [
			['who', 'What the agent does, and what needs you'],
			['prompts', 'The prompts'],
			['cli', 'The cargo privatecrates CLI'],
			['security', 'Security']
		]
	};
}
