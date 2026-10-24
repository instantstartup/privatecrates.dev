<script lang="ts" module>
	import type { ApiError } from '$lib/api';

	/** Why a form can't send the address yet, or null. Only the obvious: the server validates it and has the final say. */
	export function missingBillingEmail(email: string): string | null {
		return email.trim() ? null : 'Enter the email address for billing.';
	}

	/** Whether the server's error is about the address, so the field shows it rather than the form. */
	export function aboutBillingEmail(error: ApiError | null): boolean {
		return !!error?.errors.some((e) => e.code === 'billing::email_invalid');
	}
</script>

<script lang="ts">
	// The billing email a trial goes to, with the server's verdict on it, for the forms that start a trial.
	interface Props {
		org: string;
		email: string;
		/** Set once the field has been left or the form submitted, to show a missing address. */
		touched: boolean;
		/** The form's last error; its `billing::email_invalid` is shown under the field. */
		error: ApiError | null;
		/** Called when the address changes after the server refused it, to clear that error. */
		onedit: () => void;
	}

	let { org, email = $bindable(), touched = $bindable(), error, onedit }: Props = $props();

	const problem = $derived(missingBillingEmail(email));
	const serverProblem = $derived(error?.errors.find((e) => e.code === 'billing::email_invalid') ?? null);
	const invalid = $derived((touched && !!problem) || !!serverProblem);
</script>

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
			if (serverProblem) onedit();
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

<style>
	.field {
		display: grid;
		gap: var(--space-label);
		max-width: var(--container-form);
	}
	label {
		font-weight: 700;
	}
	.hint {
		font-size: var(--text-meta);
		color: var(--ink-muted);
	}
	input {
		min-height: var(--size-control);
		padding: var(--space-3) var(--space-4);
		border: var(--border-outline) solid var(--line);
		border-radius: var(--radius-control);
		background: var(--surface-raised);
		color: var(--ink);
	}
	input[aria-invalid='true'] {
		border-color: var(--danger);
	}
	.problem {
		color: var(--danger);
		font-weight: 700;
		font-size: var(--text-meta);
	}
	.code {
		font-weight: 400;
		white-space: nowrap;
	}
</style>
