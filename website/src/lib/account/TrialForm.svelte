<script lang="ts">
	import { api, ApiError, type Onboarding } from '$lib/api';
	import { PRICE_USD, TRIAL_MONTHS } from '$lib/site';
	import ErrorNotice from './ErrorNotice.svelte';

	interface Props {
		org: string;
		/**
		 * `trial` starts the no-card trial with the address; `email` only saves it, for a trial that started by
		 * itself when the organisation grew past the free limit.
		 */
		purpose?: 'trial' | 'email';
		/** Called with the onboarding document once the trial has started or the address is saved. */
		onstarted: (doc: Onboarding) => void;
	}

	let { org, purpose = 'trial', onstarted }: Props = $props();

	// Prefilled with nothing: we do not know the user's email, and the billing contact may be someone else.
	let email = $state('');
	let touched = $state(false);
	let starting = $state(false);
	let error = $state<ApiError | null>(null);

	/** Only the obvious; the server validates the address and has the final say. */
	const problem = $derived(email.trim() ? null : 'Enter the email address for billing.');
	/** The server's own verdict on the address, shown under the field. */
	const serverProblem = $derived(error?.errors.find((e) => e.code === 'billing::email_invalid') ?? null);
	const otherError = $derived(error && !serverProblem ? error : null);
	const invalid = $derived((touched && !!problem) || !!serverProblem);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		touched = true;
		if (problem) return;
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
	<div class="field">
		<label for="billing-email-{org}">Billing email</label>
		<p class="hint" id="billing-email-hint-{org}">
			Stripe sends the reminder before the trial ends, and invoices after it, to this address. Use a shared
			inbox if you have one.
		</p>
		<input
			id="billing-email-{org}"
			name="billing_email"
			type="email"
			autocomplete="email"
			autocapitalize="none"
			spellcheck="false"
			required
			bind:value={email}
			oninput={() => {
				if (serverProblem) error = null;
			}}
			onblur={() => (touched = true)}
			aria-invalid={invalid ? 'true' : undefined}
			aria-describedby="billing-email-hint-{org}{invalid ? ` billing-email-problem-${org}` : ''}"
		/>
		{#if serverProblem}
			<p class="problem" id="billing-email-problem-{org}" role="alert">
				{serverProblem.detail}
				<span class="code"
					>(<a href="/docs/errors#billing::email_invalid"><code>billing::email_invalid</code></a>)</span
				>
			</p>
		{:else if touched && problem}
			<p class="problem" id="billing-email-problem-{org}">{problem}</p>
		{/if}
	</div>
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
				>No card needed. Afterwards ${PRICE_USD} per organisation per month; add a card any time under Manage billing.
				One free trial per organisation.</span
			>
		{/if}
	</div>
</form>

<style>
	.trial-form {
		display: grid;
		gap: 1rem;
	}
	.field {
		display: grid;
		gap: 0.35rem;
		max-width: 30rem;
	}
	label {
		font-weight: 700;
	}
	.hint,
	.fine {
		font-size: var(--text-sm);
		color: var(--ink-soft);
	}
	input {
		min-height: 2.75rem;
		padding: 0.5rem 0.75rem;
		border: 2px solid var(--line);
		border-radius: 6px;
		background: var(--deck);
		color: var(--ink);
	}
	input[aria-invalid='true'] {
		border-color: var(--danger);
	}
	.problem {
		color: var(--danger);
		font-weight: 700;
		font-size: var(--text-sm);
	}
	.code {
		font-weight: 400;
		white-space: nowrap;
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.75rem;
	}
</style>
