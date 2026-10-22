<script lang="ts">
	import { onMount } from 'svelte';

	type Theme = 'light' | 'dark';
	let theme = $state<Theme | null>(null);

	onMount(() => {
		const saved = document.documentElement.getAttribute('data-theme');
		theme =
			saved === 'light' || saved === 'dark'
				? saved
				: matchMedia('(prefers-color-scheme: dark)').matches
					? 'dark'
					: 'light';
	});

	function toggle() {
		theme = theme === 'dark' ? 'light' : 'dark';
		document.documentElement.setAttribute('data-theme', theme);
		try {
			localStorage.setItem('pc-theme', theme);
		} catch {
			// Storage unavailable: the choice lasts for this page only.
		}
	}

	const next = $derived(theme === 'dark' ? 'light' : 'dark');
</script>

<!-- Rendered disabled until mounted, so the server-rendered button has the same size and never shifts. -->
<button
	type="button"
	class="toggle"
	onclick={toggle}
	disabled={theme === null}
	aria-label="Dark theme"
	aria-pressed={theme === 'dark'}
	title={theme ? `Switch to ${next} theme` : 'Switch theme'}
>
	<svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
		{#if theme === 'dark'}
			<!-- Sun: switching to light -->
			<circle cx="12" cy="12" r="4.5" />
			<path
				d="M12 2.5v2.5M12 19v2.5M2.5 12H5M19 12h2.5M5.3 5.3l1.8 1.8M16.9 16.9l1.8 1.8M5.3 18.7l1.8-1.8M16.9 7.1l1.8-1.8"
			/>
		{:else}
			<!-- Moon: switching to dark -->
			<path d="M20 14.5A8.5 8.5 0 0 1 9.5 4 8.5 8.5 0 1 0 20 14.5Z" />
		{/if}
	</svg>
</button>

<style>
	.toggle {
		display: inline-grid;
		place-items: center;
		width: var(--size-control);
		height: var(--size-control);
		border-radius: var(--radius-control);
		color: var(--ink);
		cursor: pointer;
	}
	.toggle:hover {
		background: color-mix(in srgb, var(--ink) 8%, transparent);
	}
	.toggle:disabled {
		cursor: default;
	}
	svg {
		width: var(--size-icon);
		height: var(--size-icon);
		fill: none;
		stroke: currentColor;
		stroke-width: 2;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
</style>
