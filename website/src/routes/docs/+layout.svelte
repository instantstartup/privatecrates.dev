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
	@reference '../../app.css';

	.docs {
		display: grid;
		gap: var(--space-9);
		padding-top: var(--space-10);
	}
	@media (width >= theme(--breakpoint-7)) {
		.docs {
			grid-template-columns: var(--container-sidebar) minmax(0, 1fr);
			gap: var(--space-gutter);
		}
		.side {
			position: sticky;
			top: var(--space-7);
			align-self: start;
			/* As tall as the screen less the gap kept above and below it. */
			max-height: calc(var(--size-screen) - 2 * var(--space-7));
			overflow-y: auto;
		}
		.bar {
			display: none;
		}
	}
	.rail {
		margin-top: var(--space-8);
	}
	@media not (width >= theme(--breakpoint-7)) {
		.rail {
			display: none;
		}
	}
	.side-title {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-title);
		margin-bottom: var(--space-link-list-heading);
	}
	.side ul {
		display: grid;
		border-left: var(--border-outline) solid var(--divider);
	}
	@media not (width >= theme(--breakpoint-7)) {
		.side ul {
			grid-template-columns: repeat(auto-fill, minmax(var(--container-tile), 1fr));
			border-left: 0;
			border-top: var(--border-outline) solid var(--divider);
			padding-top: var(--space-2);
		}
	}
	.side a {
		display: block;
		padding: var(--space-nav-y) var(--space-nav-x);
		/* The item's bar lies over the list's rule. */
		margin-left: calc(-1 * var(--border-outline));
		border-left: var(--border-strong) solid transparent;
		color: var(--ink);
		text-decoration: none;
	}
	.side a:hover {
		background: color-mix(in srgb, var(--ink) 8%, transparent);
		border-left-color: var(--ink);
		text-decoration: underline;
	}
	.side a[aria-current='page'] {
		border-left-color: var(--action);
		font-weight: 700;
		background: var(--surface-raised);
	}
	.content {
		min-width: 0;
	}
	.content :global(h1) {
		font-size: var(--text-headline);
	}
	.pager {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--space-5);
		margin-top: var(--space-break) !important;
		padding-top: var(--space-7);
		border-top: var(--border-outline) solid var(--divider);
	}
	.pager a {
		display: flex;
		flex-direction: column;
		padding: var(--space-4) var(--space-5);
		border: var(--border-outline) solid var(--divider);
		border-radius: var(--radius-card);
		font-weight: 700;
		text-decoration: none;
	}
	.pager a:hover {
		border-color: var(--ink);
	}
	.pager span {
		font-weight: 400;
		font-size: var(--text-meta);
		color: var(--ink-muted);
	}
	.next {
		grid-column: 2;
		text-align: right;
	}
</style>
