<script lang="ts">
	import type { Terms } from '$lib/api';

	// The admin's acceptance of the terms on behalf of the organisation (docs/preview.md §2). Used where a registry is
	// created (SlugForm) and where an existing one still needs it (TermsBanner); the parent owns the button.
	interface Props {
		org: string;
		terms: Terms;
		checked: boolean;
		/** The id of the text explaining why the button waits, which the checkbox also points to. */
		describedby?: string;
	}

	let { org, terms, checked = $bindable(false), describedby }: Props = $props();

	const id = $derived(`accept-terms-${org}`);
	const section = (anchor: string) => `${terms.url}#${anchor}`;
</script>

<!-- The terms open in a new tab, so reading them never loses what is typed here. -->
{#snippet newTab(href: string, text: string)}
	<a {href} target="_blank" rel="noopener"
		>{text}<span class="new-tab" aria-hidden="true">↗</span><span class="visually-hidden">
			(opens in a new tab)</span
		></a
	>
{/snippet}

<div class="terms-check">
	<input
		{id}
		type="checkbox"
		name="accept_terms"
		value={terms.version}
		required
		bind:checked
		aria-describedby="{id}-summary{describedby ? ` ${describedby}` : ''}"
	/>
	<label for={id}>
		I have read and accept the {@render newTab(terms.url, 'preview terms')} on behalf of
		<strong>{org}</strong>
	</label>
	<p class="summary" id="{id}-summary">
		In short: free during the preview, provided {@render newTab(
			section('as-is'),
			'as is and at your own risk'
		)}, and your crates {@render newTab(section('your-data'), 'stay in your own repository')}. Version
		<code>{terms.version}</code>.
	</p>
</div>

<style>
	.terms-check {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		gap: 0.35rem 0.7rem;
		align-items: start;
		padding: 0.85rem 1rem;
		border: 2px solid var(--line);
		border-radius: 8px;
		background: var(--deck);
	}
	input {
		width: 1.35rem;
		height: 1.35rem;
		margin-top: 0.15rem;
		accent-color: var(--harbour);
		cursor: pointer;
	}
	label {
		font-weight: 700;
		cursor: pointer;
	}
	.summary {
		grid-column: 2;
		font-size: var(--text-sm);
		color: var(--ink-soft);
	}
</style>
