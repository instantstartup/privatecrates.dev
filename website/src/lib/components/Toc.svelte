<script lang="ts" module>
	/** [heading id, heading text] for one section of a page. */
	export type TocItem = [string, string];
</script>

<script lang="ts">
	interface Props {
		items: TocItem[];
		/** "rail": a plain list for the sticky sidebar. "bar": a collapsible bar pinned to the top on phones. */
		variant: 'rail' | 'bar';
	}

	let { items, variant }: Props = $props();
	let open = $state(false);
</script>

{#if variant === 'rail'}
	<nav class="rail" aria-label="On this page">
		<p class="title">On this page</p>
		<ul>
			{#each items as [id, label] (id)}
				<li><a href="#{id}">{label}</a></li>
			{/each}
		</ul>
	</nav>
{:else}
	<nav class="bar" aria-label="On this page">
		<details bind:open>
			<summary>
				On this page
				<svg viewBox="0 0 20 20" aria-hidden="true" focusable="false"><path d="M5 8l5 5 5-5" /></svg>
			</summary>
			<ul>
				{#each items as [id, label] (id)}
					<li><a href="#{id}" onclick={() => (open = false)}>{label}</a></li>
				{/each}
			</ul>
		</details>
	</nav>
{/if}

<style>
	.title {
		font-weight: 700;
		margin-bottom: 0.25rem;
	}
	.rail ul {
		display: grid;
		border-left: 3px solid var(--action);
	}
	.rail a {
		display: block;
		padding: 0.3rem 0.8rem;
		color: var(--ink-muted);
		font-size: var(--text-sm);
		text-decoration: none;
	}
	.rail a:hover {
		color: var(--ink);
		text-decoration: underline;
	}

	.bar {
		position: sticky;
		top: 0;
		z-index: 5;
		margin-inline: -1rem;
		padding-inline: 1rem;
		background: var(--surface);
		border-bottom: 2px solid var(--divider);
	}
	summary {
		display: flex;
		align-items: center;
		justify-content: space-between;
		list-style: none;
		min-height: 2.75rem;
		font-weight: 700;
		cursor: pointer;
	}
	summary::-webkit-details-marker {
		display: none;
	}
	summary svg {
		width: 1.25rem;
		height: 1.25rem;
		fill: none;
		stroke: currentColor;
		stroke-width: 2.25;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	details[open] summary svg {
		transform: rotate(180deg);
	}
	.bar ul {
		padding-bottom: 0.5rem;
	}
	.bar a {
		display: block;
		padding-block: 0.45rem;
	}
</style>
