<script lang="ts">
	import { api, ApiError, type Terms } from '$lib/api';
	import Callout from '$lib/components/Callout.svelte';
	import ErrorNotice from './ErrorNotice.svelte';
	import TermsCheckbox from './TermsCheckbox.svelte';

	// A registry created before the terms (or before their current version) keeps working, but an admin is asked to
	// accept them here (docs/preview.md §2). Members are told who can.
	interface Props {
		org: string;
		terms: Terms;
		admin: boolean;
		/** Called once the terms are accepted, to re-read the session. */
		onaccepted: () => void;
	}

	let { org, terms, admin, onaccepted }: Props = $props();

	let accepted = $state(false);
	let saving = $state(false);
	let error = $state<ApiError | null>(null);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!accepted) return;
		saving = true;
		error = null;
		try {
			await api.acceptTerms(org, terms.version);
			onaccepted();
		} catch (e) {
			error = e instanceof ApiError ? e : new ApiError(0, [{ detail: String(e) }]);
		} finally {
			saving = false;
		}
	}
</script>

<Callout
	tone="warn"
	title={admin ? `Accept the preview terms for ${org}` : 'The preview terms are waiting for an admin'}
>
	{#if admin}
		<p>
			PrivateCrates asks an admin of each organisation to accept its terms, and nobody has for {org} yet. The registry
			keeps working in the meantime.
		</p>
		<form class="accept" onsubmit={submit} novalidate>
			<TermsCheckbox
				{org}
				{terms}
				bind:checked={accepted}
				describedby={accepted ? undefined : `terms-wait-${org}`}
			/>
			{#if error}
				<ErrorNotice {error} title="The terms were not accepted" />
			{/if}
			<div class="submit">
				<button
					class="btn btn-primary"
					type="submit"
					disabled={saving || !accepted}
					aria-describedby={accepted ? undefined : `terms-wait-${org}`}
				>
					{saving ? 'Accepting the preview terms…' : 'Accept the preview terms'}
				</button>
				<p class="wait" id="terms-wait-{org}" aria-live="polite">
					{accepted ? '' : 'Tick the box above first.'}
				</p>
			</div>
		</form>
	{:else}
		<p>
			An admin of {org} needs to accept the <a href={terms.url}>preview terms</a>. The registry keeps working
			in the meantime.
		</p>
	{/if}
</Callout>

<style>
	.accept {
		display: grid;
		gap: 1rem;
		max-width: 36rem;
		margin-top: 0.75rem;
	}
	.submit {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem 0.75rem;
	}
	.wait {
		font-size: var(--text-sm);
		color: var(--ink-muted);
	}
</style>
