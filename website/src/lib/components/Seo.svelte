<script lang="ts">
	import { SITE_NAME, SITE_URL } from '$lib/site';

	interface Props {
		title: string;
		description: string;
		/** Canonical path, e.g. "/pricing". */
		path: string;
		noindex?: boolean;
	}

	let { title, description, path, noindex = false }: Props = $props();

	const fullTitle = $derived(path === '/' ? title : `${title} · ${SITE_NAME}`);
	const url = $derived(SITE_URL + (path === '/' ? '/' : path));
</script>

<svelte:head>
	<title>{fullTitle}</title>
	<meta name="description" content={description} />
	<link rel="canonical" href={url} />
	{#if noindex}
		<meta name="robots" content="noindex" />
	{/if}
	<meta property="og:site_name" content={SITE_NAME} />
	<meta property="og:type" content="website" />
	<meta property="og:title" content={fullTitle} />
	<meta property="og:description" content={description} />
	<meta property="og:url" content={url} />
	<meta property="og:locale" content="en_GB" />
	<meta name="twitter:card" content="summary" />
</svelte:head>
