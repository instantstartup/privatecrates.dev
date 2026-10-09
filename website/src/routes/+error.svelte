<script lang="ts">
	import { page } from '$app/state';
	import Seo from '$lib/components/Seo.svelte';
	import LostCrate from '$lib/illustrations/LostCrate.svelte';

	const notFound = $derived(page.status === 404);
</script>

<Seo
	title={notFound ? 'Page not found' : 'Something went wrong'}
	description="This page is not on the manifest."
	path={page.url.pathname}
	noindex
/>

<div class="page lost">
	<div class="art">
		<LostCrate />
	</div>
	<div class="text">
		{#if notFound}
			<h1>This crate has drifted off the manifest</h1>
			<p class="lede">
				Nothing is published at <code>{page.url.pathname}</code>. It may have moved, or the link has a typo.
			</p>
		{:else}
			<h1>Something went wrong</h1>
			<p class="lede">{page.error?.message ?? 'The page could not be shown.'} Reload to try again.</p>
		{/if}
		<div class="actions">
			<a class="btn btn-primary" href="/">Back to the harbour</a>
			<a class="btn btn-quiet" href="/docs">Browse the docs</a>
		</div>
	</div>
</div>

<style>
	.lost {
		display: grid;
		gap: 2rem;
		align-items: center;
		padding-block: 3rem;
	}
	@media (min-width: 52rem) {
		.lost {
			grid-template-columns: 1fr 1fr;
		}
	}
	.art {
		max-width: 28rem;
		width: 100%;
		justify-self: center;
	}
	h1 {
		font-size: var(--text-headline);
		margin-bottom: 1rem;
	}
	.lede code {
		overflow-wrap: anywhere;
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		margin-top: 1.5rem;
	}
</style>
