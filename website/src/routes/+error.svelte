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

<style lang="postcss">
	@reference '../app.css';

	.lost {
		display: grid;
		gap: var(--space-9);
		align-items: center;
		padding-block: var(--space-11);
	}
	@media (width >= theme(--breakpoint-6)) {
		.lost {
			grid-template-columns: 1fr 1fr;
		}
	}
	.art {
		max-width: var(--size-standalone);
		width: 100%;
		justify-self: center;
	}
	h1 {
		font-size: var(--text-headline);
		margin-bottom: var(--space-5);
	}
	.lede code {
		overflow-wrap: anywhere;
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-4);
		margin-top: var(--space-7);
	}
</style>
