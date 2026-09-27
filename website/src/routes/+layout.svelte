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
		left: 1rem;
		top: -4rem;
		z-index: 10;
		padding: 0.6rem 1rem;
		background: var(--signal);
		color: var(--signal-ink);
		font-weight: 700;
		border: 2px solid var(--line);
		border-radius: 6px;
	}
	.skip:focus {
		top: 0.75rem;
	}
	main:focus {
		outline: none;
	}
</style>
