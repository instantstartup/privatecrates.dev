<script lang="ts">
	import { PREVIEW, TERMS_PATH } from '$lib/site';
</script>

<!-- Prerendered into every page at a fixed height, so it never pushes the page down after load. -->
{#if PREVIEW}
	<aside class="preview-notice" aria-label="Preview notice">
		<p class="page line">
			<strong class="tag">Preview</strong>
			<span>Free, as is, with no warranty: use at your own risk.</span>
			<a href={TERMS_PATH}>Read the terms</a>
		</p>
	</aside>
{/if}

<style>
	.preview-notice {
		background: var(--notice);
		color: var(--notice-ink);
		font-size: var(--text-sm);
	}
	.line {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.15rem 0.6rem;
		min-height: 2.25rem;
		padding-block: 0.3rem;
		line-height: 1.3;
	}
	.tag {
		padding: 0.05rem 0.45rem;
		border-radius: 4px;
		background: var(--primary);
		color: var(--primary-ink);
		font-weight: 700;
	}
	a {
		display: inline-block;
		padding-block: 0.25rem;
		color: inherit;
		font-weight: 700;
	}
	a:hover {
		color: inherit;
	}
	/* The band is dark in both themes: the focus ring follows the text. */
	a:focus-visible {
		outline-color: currentColor;
	}
</style>
