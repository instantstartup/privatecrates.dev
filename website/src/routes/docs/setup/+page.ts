import type { TocItem } from '$lib/components/Toc.svelte';

// Sections of this page, for the docs layout's "On this page" list.
export function load(): { toc: TocItem[] } {
	return {
		toc: [
			['organisation', 'For the organisation'],
			['developers', 'For each developer'],
			['other-tokens', 'Using gh or a personal access token instead'],
			['sso', 'SAML single sign-on']
		]
	};
}
