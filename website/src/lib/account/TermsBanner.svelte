<script lang="ts">
	import { api, ApiError, type Terms } from '$lib/api';
	import Callout from '$lib/components/Callout.svelte';
	import { TRIAL_MONTHS } from '$lib/site';
	import BillingEmailField, { aboutBillingEmail, missingBillingEmail } from './BillingEmailField.svelte';
	import ErrorNotice from './ErrorNotice.svelte';
	import TermsCheckbox from './TermsCheckbox.svelte';

	// A registry created before the terms (or before their current version): an admin is asked to accept them here,
	// and members are told who can. Billing waits for them, so one over the free limit has stopped until they are
	// accepted, and accepting them starts its trial (docs/website-api.md).
	interface Props {
		org: string;
		terms: Terms;
		admin: boolean;
		/** Over the free limit with no trial or subscription: the registry has stopped until the terms are accepted. */
		stopped?: boolean;
		/** Accepting the terms starts the free trial, so the form asks for its billing email too. */
		trial?: boolean;
		/** Called once the terms are accepted, to re-read the session. */
		onaccepted: () => void;
	}

	let { org, terms, admin, stopped = false, trial = false, onaccepted }: Props = $props();

	let accepted = $state(false);
	let email = $state('');
	let touched = $state(false);
	let saving = $state(false);
	let error = $state<ApiError | null>(null);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		touched = true;
		if (!accepted || (trial && missingBillingEmail(email))) return;
		saving = true;
		error = null;
		try {
			await api.acceptTerms(org, terms.version, trial ? email.trim() : undefined);
			onaccepted();
		} catch (e) {
			error = e instanceof ApiError ? e : new ApiError(0, [{ detail: String(e) }]);
		} finally {
			saving = false;
		}
	}
</script>

<Callout tone="warn" title={admin ? `Accept the terms for ${org}` : 'The terms are waiting for an admin'}>
	{#if admin}
		<p>
			PrivateCrates asks an admin of each organisation to accept its terms, and nobody has for {org} yet.
			{#if stopped && trial}
				{org} has more members than the free plan covers, so its registry has stopped until they are accepted. Accepting
				them starts its {TRIAL_MONTHS}-month free trial, no card needed.
			{:else if stopped}
				{org} has more members than the free plan covers, so its registry has stopped until they are accepted and
				it subscribes.
			{:else}
				The registry keeps working in the meantime.
			{/if}
		</p>
		<form class="accept" onsubmit={submit} novalidate>
			<TermsCheckbox
				{org}
				{terms}
				bind:checked={accepted}
				describedby={accepted ? undefined : `terms-wait-${org}`}
			/>
			{#if trial}
				<BillingEmailField {org} bind:email bind:touched {error} onedit={() => (error = null)} />
			{/if}
			{#if error && !(trial && aboutBillingEmail(error))}
				<ErrorNotice {error} title="The terms were not accepted" />
			{/if}
			<div class="submit">
				<button
					class="btn btn-primary"
					type="submit"
					disabled={saving || !accepted}
					aria-describedby={accepted ? undefined : `terms-wait-${org}`}
				>
					{#if trial}
						{saving ? 'Starting the trial…' : 'Accept the terms and start the free trial'}
					{:else}
						{saving ? 'Accepting the terms…' : 'Accept the terms'}
					{/if}
				</button>
				<p class="wait" id="terms-wait-{org}" aria-live="polite">
					{accepted ? '' : 'Tick the box above first.'}
				</p>
			</div>
		</form>
	{:else}
		<p>
			An admin of {org} needs to accept the <a href={terms.url}>terms</a>.
			{#if stopped}
				{org} has more members than the free plan covers, so its registry has stopped until they do.
			{:else}
				The registry keeps working in the meantime.
			{/if}
		</p>
	{/if}
</Callout>

<style>
	.accept {
		display: grid;
		gap: var(--space-5);
		max-width: var(--container-notice);
		margin-top: var(--space-4);
	}
	.submit {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3) var(--space-4);
	}
	.wait {
		font-size: var(--text-meta);
		color: var(--ink-muted);
	}
</style>
