<script lang="ts">
	import type { Snippet } from 'svelte';
	import Toc, { type TocItem } from './Toc.svelte';

	// A long document with an "On this page" rail beside it on wide screens, and a pinned bar above it on phones.
	// The trust centre, the security policy and the DPA share it.
	interface Props {
		toc: TocItem[];
		children: Snippet;
	}

	let { toc, children }: Props = $props();
</script>

<div class="toc-layout">
	<aside class="side">
		<Toc items={toc} variant="rail" />
	</aside>
	<article class="prose content">
		<div class="bar"><Toc items={toc} variant="bar" /></div>
		{@render children()}
	</article>
</div>

<style>
	.toc-layout {
		display: grid;
		gap: 2rem;
	}
	@media (min-width: 56rem) {
		.toc-layout {
			grid-template-columns: 14rem minmax(0, 1fr);
			gap: 3.5rem;
		}
		.side {
			position: sticky;
			top: 1.5rem;
			align-self: start;
			padding-top: 0.5rem;
		}
		.bar {
			display: none;
		}
		/* The article's first heading sits level with the rail. */
		.content > :global(.bar + *) {
			margin-top: 0;
		}
	}
	@media (max-width: 55.99rem) {
		.side {
			display: none;
		}
	}
	.content {
		min-width: 0;
	}
</style>
