<script lang="ts">
	import '../app.css';
	import displayLatin from '@fontsource-variable/big-shoulders/files/big-shoulders-latin-opsz-normal.woff2?url';
	import bodyLatin from '@fontsource-variable/atkinson-hyperlegible-next/files/atkinson-hyperlegible-next-latin-wght-normal.woff2?url';
	import Footer from '$lib/components/Footer.svelte';
	import Header from '$lib/components/Header.svelte';
	import PreviewNotice from '$lib/components/PreviewNotice.svelte';

	let { children } = $props();
</script>

<svelte:head>
	<link rel="preload" href={displayLatin} as="font" type="font/woff2" crossorigin="anonymous" />
	<link rel="preload" href={bodyLatin} as="font" type="font/woff2" crossorigin="anonymous" />
</svelte:head>

<a class="skip" href="#main">Skip to content</a>
<PreviewNotice />
<Header />
<main id="main" tabindex="-1">
	{@render children()}
</main>
<Footer />

<style>
	.skip {
		position: absolute;
		left: var(--space-5);
		top: calc(-1 * var(--space-12));
		z-index: 10;
		padding: var(--space-control-y) var(--space-5);
		background: var(--action);
		color: var(--action-ink);
		font-weight: 700;
		border: var(--border-outline) solid var(--line);
		border-radius: var(--radius-control);
	}
	.skip:focus {
		top: var(--space-4);
	}
	main:focus {
		outline: none;
	}
</style>
