<script lang="ts">
	import { page } from '$app/state';
	import Toc, { type TocItem } from '$lib/components/Toc.svelte';
	import { docsNav } from '$lib/site';

	let { children } = $props();

	const index = $derived(docsNav.findIndex((l) => l.href === page.url.pathname));
	const prev = $derived(index > 0 ? docsNav[index - 1] : null);
	const next = $derived(index >= 0 && index < docsNav.length - 1 ? docsNav[index + 1] : null);
	const toc = $derived((page.data.toc as TocItem[] | undefined) ?? []);
</script>

<div class="page docs">
	<aside class="side">
		<nav aria-label="Documentation sections">
			<p class="side-title">Documentation</p>
			<ul>
				{#each docsNav as link (link.href)}
					<li>
						<a href={link.href} aria-current={page.url.pathname === link.href ? 'page' : undefined}
							>{link.label}</a
						>
					</li>
				{/each}
			</ul>
		</nav>
		{#if toc.length}
			<div class="rail"><Toc items={toc} variant="rail" /></div>
		{/if}
	</aside>
	<article class="prose content">
		{#if toc.length}
			<div class="bar"><Toc items={toc} variant="bar" /></div>
		{/if}
		{@render children()}
		{#if prev || next}
			<nav class="pager" aria-label="Previous and next page">
				{#if prev}<a class="prev" href={prev.href}><span>Previous</span>{prev.label}</a>{/if}
				{#if next}<a class="next" href={next.href}><span>Next</span>{next.label}</a>{/if}
			</nav>
		{/if}
	</article>
</div>

<style>
	.docs {
		display: grid;
		gap: 2rem;
		padding-top: 2.5rem;
	}
	@media (min-width: 56rem) {
		.docs {
			grid-template-columns: 14rem minmax(0, 1fr);
			gap: 3.5rem;
		}
		.side {
			position: sticky;
			top: 1.5rem;
			align-self: start;
			max-height: calc(100vh - 3rem);
			overflow-y: auto;
		}
		.bar {
			display: none;
		}
	}
	.rail {
		margin-top: 1.75rem;
	}
	@media (max-width: 55.99rem) {
		.rail {
			display: none;
		}
	}
	.side-title {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-xl);
		margin-bottom: 0.4rem;
	}
	.side ul {
		display: grid;
		border-left: 2px solid var(--rule);
	}
	@media (max-width: 55.99rem) {
		.side ul {
			grid-template-columns: repeat(auto-fill, minmax(9.5rem, 1fr));
			border-left: 0;
			border-top: 2px solid var(--rule);
			padding-top: 0.25rem;
		}
	}
	.side a {
		display: block;
		padding: 0.45rem 0.8rem;
		margin-left: -2px;
		border-left: 3px solid transparent;
		color: var(--ink);
		text-decoration: none;
	}
	.side a:hover {
		background: color-mix(in srgb, var(--ink) 8%, transparent);
		border-left-color: var(--ink);
		text-decoration: underline;
	}
	.side a[aria-current='page'] {
		border-left-color: var(--signal);
		font-weight: 700;
		background: var(--deck);
	}
	.content {
		min-width: 0;
	}
	.content :global(h1) {
		font-size: var(--text-3xl);
	}
	.pager {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1rem;
		margin-top: 3.5rem !important;
		padding-top: 1.5rem;
		border-top: 2px solid var(--rule);
	}
	.pager a {
		display: flex;
		flex-direction: column;
		padding: 0.75rem 1rem;
		border: 2px solid var(--rule);
		border-radius: 8px;
		font-weight: 700;
		text-decoration: none;
	}
	.pager a:hover {
		border-color: var(--ink);
	}
	.pager span {
		font-weight: 400;
		font-size: var(--text-sm);
		color: var(--ink-soft);
	}
	.next {
		grid-column: 2;
		text-align: right;
	}
</style>
