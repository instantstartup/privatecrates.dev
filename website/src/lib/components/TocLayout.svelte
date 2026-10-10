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

<style lang="postcss">
	@reference '../../app.css';

	.toc-layout {
		display: grid;
		gap: var(--space-9);
	}
	@media (width >= theme(--breakpoint-7)) {
		.toc-layout {
			grid-template-columns: var(--container-sidebar) minmax(0, 1fr);
			gap: var(--space-gutter);
		}
		.side {
			position: sticky;
			top: var(--space-7);
			align-self: start;
			padding-top: var(--space-3);
		}
		.bar {
			display: none;
		}
		/* The article's first heading sits level with the rail. */
		.content > :global(.bar + *) {
			margin-top: 0;
		}
	}
	@media not (width >= theme(--breakpoint-7)) {
		.side {
			display: none;
		}
	}
	.content {
		min-width: 0;
	}
</style>
