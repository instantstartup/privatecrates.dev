<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { api, ApiError, type CatalogEntry } from '$lib/api';
	import Callout from '$lib/components/Callout.svelte';
	import Seo from '$lib/components/Seo.svelte';

	let entries = $state<CatalogEntry[] | null>(null);
	let failure = $state<string | null>(null);
	let query = $state('');

	async function load() {
		failure = null;
		entries = null;
		try {
			const list = await api.errors();
			entries = [...list].sort((a, b) => a.code.localeCompare(b.code));
			// Links such as /docs/errors#publish::ci_only point at rows that exist only now.
			if (location.hash) {
				await tick();
				document.getElementById(decodeURIComponent(location.hash.slice(1)))?.scrollIntoView();
			}
		} catch (e) {
			failure = e instanceof ApiError && e.status !== 0 ? e.message : 'The error list could not be loaded.';
		}
	}

	onMount(() => {
		load();
	});

	const filtered = $derived.by(() => {
		if (!entries) return [];
		const q = query.trim().toLowerCase();
		if (!q) return entries;
		return entries.filter(
			(e) =>
				e.code.toLowerCase().includes(q) || e.message.toLowerCase().includes(q) || String(e.http_status) === q
		);
	});

	/** Renders `{placeholder}` fields as emphasised text. */
	function parts(message: string): { text: string; field: boolean }[] {
		return message.split(/(\{[a-z_]+\})/g).map((text) => ({ text, field: /^\{[a-z_]+\}$/.test(text) }));
	}
</script>

<Seo
	title="Error reference"
	description="Every error code the PrivateCrates registry can return, with its message and HTTP status."
	path="/docs/errors"
/>

<h1>Error reference</h1>
<p class="lede">
	Every error the registry returns carries a code, such as <code>publish::ci_only</code>. Cargo prints the
	message; search for the code here.
</p>

<div class="search">
	<label for="error-search">Search by code, message or HTTP status</label>
	<input
		id="error-search"
		type="search"
		autocomplete="off"
		spellcheck="false"
		placeholder="e.g. sso, 403 or publish::"
		bind:value={query}
		disabled={!entries}
	/>
</div>

{#if failure}
	<Callout tone="warn" title="The error list is not available right now">
		<p>
			{failure} The messages Cargo prints are written to stand on their own, and every one names what to do next.
		</p>
		<p><button type="button" class="btn btn-quiet" onclick={load}>Try loading it again</button></p>
	</Callout>
{:else if !entries}
	<p class="status" role="status">Loading the error list…</p>
{:else}
	<p class="status" role="status">
		{filtered.length === entries.length
			? `${entries.length} error codes`
			: `${filtered.length} of ${entries.length} error codes match`}
	</p>
	{#if filtered.length === 0}
		<p>
			No error codes match “{query}”. Try part of the code, such as <code>auth</code> or <code>publish</code>.
		</p>
	{:else}
		<div class="table-scroll">
			<table>
				<thead>
					<tr>
						<th scope="col">Code</th>
						<th scope="col">Message</th>
						<th scope="col" class="num">HTTP</th>
					</tr>
				</thead>
				<tbody>
					{#each filtered as entry (entry.code)}
						<tr id={entry.code}>
							<th scope="row"><code>{entry.code}</code></th>
							<td
								>{#each parts(entry.message) as p, i (i)}{#if p.field}<em class="field">{p.text}</em
										>{:else}{p.text}{/if}{/each}</td
							>
							<td class="num">{entry.http_status}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
{/if}

<style>
	/* Stays in reach while scrolling a long table. */
	.search {
		position: sticky;
		top: 0;
		z-index: 4;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		margin-top: 2rem !important;
		padding-block: 0.75rem;
		background: var(--surface);
		border-bottom: 1px solid var(--divider);
	}
	.search input {
		max-width: 28rem;
	}
	label {
		font-weight: 700;
	}
	input {
		min-height: 2.75rem;
		padding: 0.5rem 0.75rem;
		border: 2px solid var(--line);
		border-radius: 6px;
		background: var(--surface-raised);
		color: var(--ink);
	}
	input::placeholder {
		color: var(--ink-muted);
		opacity: 0.8;
	}
	.status {
		color: var(--ink-muted);
		font-size: var(--text-sm);
	}
	.num {
		text-align: right !important;
		white-space: nowrap;
	}
	tbody th {
		font-weight: 400;
		white-space: nowrap;
	}
	tr:target {
		background: var(--warn-bg);
	}
	.field {
		font-style: normal;
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 1em;
	}
</style>
