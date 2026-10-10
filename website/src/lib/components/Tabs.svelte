<script lang="ts" module>
	export interface Tab {
		id: string;
		label: string;
	}

	/** The ids that tie a tab to its panel; the parent renders each panel with these. */
	export function tabIds(prefix: string, id: string): { tab: string; panel: string } {
		return { tab: `${prefix}-tab-${id}`, panel: `${prefix}-panel-${id}` };
	}
</script>

<script lang="ts">
	interface Props {
		tabs: Tab[];
		/** The selected tab's id. */
		selected: string;
		/** Unique on the page. */
		prefix: string;
		label: string;
	}

	let { tabs, selected = $bindable(), prefix, label }: Props = $props();

	let buttons = $state<HTMLButtonElement[]>([]);

	// Arrow keys, Home and End move between tabs and select them (automatic activation).
	function onkeydown(event: KeyboardEvent, index: number) {
		const last = tabs.length - 1;
		const to =
			event.key === 'ArrowRight'
				? index === last
					? 0
					: index + 1
				: event.key === 'ArrowLeft'
					? index === 0
						? last
						: index - 1
					: event.key === 'Home'
						? 0
						: event.key === 'End'
							? last
							: null;
		if (to === null) return;
		event.preventDefault();
		selected = tabs[to].id;
		buttons[to]?.focus();
	}
</script>

<div class="tabs" role="tablist" aria-label={label}>
	{#each tabs as tab, i (tab.id)}
		{@const ids = tabIds(prefix, tab.id)}
		<button
			bind:this={buttons[i]}
			type="button"
			role="tab"
			id={ids.tab}
			aria-controls={ids.panel}
			aria-selected={selected === tab.id}
			tabindex={selected === tab.id ? 0 : -1}
			onclick={() => (selected = tab.id)}
			onkeydown={(e) => onkeydown(e, i)}>{tab.label}</button
		>
	{/each}
</div>

<style>
	.tabs {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
		border-bottom: 2px solid var(--line);
	}
	button {
		min-height: 2.75rem;
		padding: 0.5rem 1rem;
		margin-bottom: -2px;
		border: 2px solid transparent;
		border-bottom: 0;
		border-radius: 6px 6px 0 0;
		background: transparent;
		color: var(--ink-muted);
		font: inherit;
		font-weight: 700;
		cursor: pointer;
	}
	button:hover {
		color: var(--ink);
		background: color-mix(in srgb, var(--ink) 7%, transparent);
	}
	button[aria-selected='true'] {
		color: var(--ink);
		background: var(--surface-raised);
		border-color: var(--ink);
		box-shadow: inset 0 3px 0 var(--action);
	}
</style>
