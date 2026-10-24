<script lang="ts">
	import { api, ApiError, type Onboarding } from '$lib/api';
	import { TRIAL_MONTHS } from '$lib/site';
	import BillingEmailField, { aboutBillingEmail, missingBillingEmail } from './BillingEmailField.svelte';
	import ErrorNotice from './ErrorNotice.svelte';

	interface Props {
		org: string;
		/** What the organisation will pay per month after the trial, at its member count. */
		price: number;
		/**
		 * `trial` starts the no-card trial with the address; `email` only saves it, for a trial that started by
		 * itself when the organisation grew past the free limit.
		 */
		purpose?: 'trial' | 'email';
		/** Called with the onboarding document once the trial has started or the address is saved. */
		onstarted: (doc: Onboarding) => void;
	}

	let { org, price, purpose = 'trial', onstarted }: Props = $props();

	// Prefilled with nothing: we do not know the user's email, and the billing contact may be someone else.
	let email = $state('');
	let touched = $state(false);
	let starting = $state(false);
	let error = $state<ApiError | null>(null);

	const otherError = $derived(aboutBillingEmail(error) ? null : error);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		touched = true;
		if (missingBillingEmail(email)) return;
		starting = true;
		error = null;
		try {
			onstarted(
				await (purpose === 'trial' ? api.trial(org, email.trim()) : api.billingEmail(org, email.trim()))
			);
		} catch (e) {
			error = e instanceof ApiError ? e : new ApiError(0, [{ detail: String(e) }]);
		} finally {
			starting = false;
		}
	}
</script>

<form class="trial-form" onsubmit={submit} novalidate>
	<BillingEmailField {org} bind:email bind:touched {error} onedit={() => (error = null)} />
	{#if otherError}
		<ErrorNotice
			error={otherError}
			title={purpose === 'trial' ? 'The free trial could not start' : 'The billing email could not be saved'}
		/>
	{/if}
	<div class="row">
		<button class="btn btn-primary" type="submit" disabled={starting}>
			{#if purpose === 'trial'}
				{starting ? 'Starting the trial…' : `Start ${TRIAL_MONTHS}-month free trial`}
			{:else}
				{starting ? 'Saving…' : 'Save billing email'}
			{/if}
		</button>
		{#if purpose === 'trial'}
			<span class="fine"
				>No card needed. Afterwards ${price} a month at your current size; add a card any time under Manage billing.
				One free trial per organisation.</span
			>
		{/if}
	</div>
</form>

<style>
	.trial-form {
		display: grid;
		gap: var(--space-5);
	}
	.fine {
		font-size: var(--text-meta);
		color: var(--ink-muted);
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-4);
	}
</style>
