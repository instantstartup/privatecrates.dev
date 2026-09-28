import type { TocItem } from '$lib/components/Toc.svelte';

// Sections of this page, for the docs layout's "On this page" list.
export function load(): { toc: TocItem[] } {
	return {
		toc: [
			['which', 'Which way your platform deploys'],
			['actions', 'Deploying from GitHub Actions'],
			['platform-builds', 'Platforms that build from Git'],
			['token', 'The token'],
			['render', 'Render'],
			['railway', 'Railway'],
			['others', 'Other platforms'],
			['rotate', 'Rotating and revoking']
		]
	};
}
