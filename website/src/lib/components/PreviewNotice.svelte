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
		font-size: var(--text-meta);
	}
	.line {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1) var(--space-3);
		min-height: var(--size-control-compact);
		padding-block: var(--space-2);
		line-height: 1.3;
	}
	.tag {
		padding: var(--space-1) var(--space-3);
		border-radius: var(--radius-inline);
		background: var(--action);
		color: var(--action-ink);
		font-weight: 700;
	}
	a {
		display: inline-block;
		padding-block: var(--space-2);
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
