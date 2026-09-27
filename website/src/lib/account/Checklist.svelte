<script lang="ts">
	import { api, ApiError, type Onboarding, type Step } from '$lib/api';
	import { PRICE_USD, TRIAL_DAYS } from '$lib/site';
	import ErrorNotice from './ErrorNotice.svelte';
	import SlugForm from './SlugForm.svelte';

	interface Props {
		doc: Onboarding;
		admin: boolean;
		/** Re-reads the onboarding document (after a step done on GitHub). */
		onrefresh: () => Promise<void>;
		onchange: (doc: Onboarding) => void;
		refreshing: boolean;
	}

	let { doc, admin, onrefresh, onchange, refreshing }: Props = $props();

	interface StepCopy {
		title: string;
		body: string;
		action?: string;
	}

	const copy: Record<string, StepCopy> = {
		reader_app: {
			title: 'Install the reader App',
			body: 'Lets PrivateCrates sign your developers in and ask GitHub what each of them can access. It reads repository metadata and organisation membership, nothing else. Install it on all repositories, or on those that will own crates.',
			action: 'Install the reader App on GitHub'
		},
		storage_repo: {
			title: 'Choose a storage repository',
			body: 'A private repository that holds your registry: the index, and every crate file as an immutable release. Create a new one, or use an existing empty one; it must hold nothing but PrivateCrates’ data. Enable immutable releases in Settings → General → Releases, so published versions can never change. You pick it in the next step, when you install the storage App.',
			action: 'Create a new repository on GitHub'
		},
		storage_app: {
			title: 'Install the storage App on that repository',
			body: 'Choose “Only select repositories” and pick the storage repository alone. The App writes releases and index files there, and nowhere else.',
			action: 'Install the storage App on GitHub'
		},
		settings: {
			title: 'Choose your registry name',
			body: 'Saved as privatecrates.toml in your storage repository, so later changes are a pull request.'
		},
		subscription: {
			title: `Start your ${TRIAL_DAYS}-day free trial`,
			body: `$${PRICE_USD} per month after the trial, billed by Stripe. Cancel before the trial ends and you pay nothing.`
		}
	};

	function stepCopy(step: Step): StepCopy {
		return (
			copy[step.id] ?? {
				title: step.id.replace(/_/g, ' ').replace(/^./, (c) => c.toUpperCase()),
				body: '',
				action: 'Continue on GitHub'
			}
		);
	}

	const current = $derived(doc.steps.find((s) => s.status === 'todo')?.id ?? null);
	const doneCount = $derived(doc.steps.filter((s) => s.status === 'done').length);

	// GitHub steps open in a new tab; when the user comes back to this one, check what they did there.
	let awaitingGitHub = $state(false);

	$effect(() => {
		const onVisible = () => {
			if (document.visibilityState === 'visible' && awaitingGitHub && !refreshing) {
				awaitingGitHub = false;
				void onrefresh();
			}
		};
		document.addEventListener('visibilitychange', onVisible);
		return () => document.removeEventListener('visibilitychange', onVisible);
	});

	let checkingOut = $state(false);
	let checkoutError = $state<ApiError | null>(null);

	async function startTrial() {
		checkingOut = true;
		checkoutError = null;
		try {
			const { url } = await api.checkout(doc.org.login);
			location.assign(url);
		} catch (e) {
			checkoutError = e instanceof ApiError ? e : new ApiError(0, [{ detail: String(e) }]);
			checkingOut = false;
		}
	}
</script>

<section class="checklist" aria-labelledby="checklist-{doc.org.login}">
	<div class="head">
		<h3 id="checklist-{doc.org.login}">Set up {doc.org.login}</h3>
		<p class="progress">{doneCount} of {doc.steps.length} steps done</p>
	</div>

	{#if !admin}
		<p class="member-note">
			Only admins of <strong>{doc.org.login}</strong> on GitHub can complete these steps. Ask one of them to finish
			setting up; this page shows their progress.
		</p>
	{/if}

	<ol>
		{#each doc.steps as step, i (step.id)}
			{@const c = stepCopy(step)}
			{@const isCurrent = step.id === current && admin}
			<li class={['step', step.status, isCurrent && 'current']}>
				<span class="marker" aria-hidden="true">
					{#if step.status === 'done'}
						<svg viewBox="0 0 20 20"><path d="M4.5 10.5l3.5 3.5 7.5-8" /></svg>
					{:else if step.status === 'blocked'}
						<svg viewBox="0 0 20 20"
							><rect x="4.5" y="9" width="11" height="8" rx="1.5" /><path d="M7 9V6.5a3 3 0 0 1 6 0V9" /></svg
						>
					{:else}
						{i + 1}
					{/if}
				</span>
				<div class="step-body">
					<h4>
						{c.title}
						<span class="visually-hidden"
							>({step.status === 'done'
								? 'done'
								: step.status === 'blocked'
									? 'needs an admin'
									: 'to do'})</span
						>
					</h4>
					{#if step.status !== 'done'}
						<!-- The server's detail is organisation-specific; our own copy covers the rest. -->
						{@const text =
							step.detail && (step.id !== 'settings' || step.status === 'blocked') ? step.detail : c.body}
						{#if text}<p>{text}</p>{/if}
					{/if}

					{#if isCurrent}
						<div class="action">
							{#if step.id === 'settings'}
								<SlugForm org={doc.org.login} suggested={doc.suggested_slug} onsaved={onchange} />
							{:else if step.id === 'subscription'}
								{#if checkoutError}
									<ErrorNotice error={checkoutError} title="Checkout could not be opened" />
								{/if}
								<div class="row">
									<button class="btn btn-primary" type="button" onclick={startTrial} disabled={checkingOut}>
										{checkingOut ? 'Opening Stripe Checkout…' : `Start ${TRIAL_DAYS}-day free trial`}
									</button>
									<span class="fine"
										>Then ${PRICE_USD} per month. You enter card details on Stripe. The free trial is for an organisation’s
										first subscription.</span
									>
								</div>
							{:else if step.action_url}
								<div class="row">
									<a
										class="btn btn-primary"
										href={step.action_url}
										target="_blank"
										rel="noopener noreferrer"
										onclick={() => (awaitingGitHub = true)}
										>{c.action ?? 'Continue on GitHub'}<span class="new-tab" aria-hidden="true">↗</span><span
											class="visually-hidden"
										>
											(opens in a new tab)</span
										></a
									>
									<button class="btn btn-quiet" type="button" onclick={onrefresh} disabled={refreshing}>
										{refreshing ? 'Checking…' : 'I’ve done this: check again'}
									</button>
								</div>
							{/if}
						</div>
					{/if}
				</div>
			</li>
		{/each}
	</ol>
</section>

<style>
	.checklist {
		display: grid;
		gap: 1rem;
	}
	.head {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: baseline;
		gap: 0.25rem 1rem;
	}
	h3 {
		font-size: var(--text-2xl);
	}
	.progress {
		color: var(--ink-soft);
	}
	.member-note {
		background: var(--warn-bg);
		border-left: 5px solid var(--signal);
		padding: 0.75rem 1rem;
		border-radius: 0 8px 8px 0;
	}
	ol {
		display: grid;
		gap: 0.5rem;
	}
	.step {
		display: flex;
		gap: 0.9rem;
		padding: 0.9rem 1rem;
		border: 2px solid transparent;
		border-radius: 10px;
	}
	.step.current {
		border-color: var(--ink);
		background: var(--deck);
		box-shadow: var(--shadow);
	}
	.marker {
		flex: none;
		display: grid;
		place-items: center;
		width: 2rem;
		height: 2rem;
		border: 2px solid var(--ink);
		border-radius: 50%;
		font-weight: 700;
		font-size: var(--text-sm);
	}
	.done .marker {
		background: var(--ok);
		border-color: var(--ok);
		color: var(--fog);
	}
	.current .marker {
		background: var(--signal);
		color: var(--signal-ink);
	}
	.blocked .marker {
		border-style: dashed;
		color: var(--ink-soft);
	}
	.marker svg {
		width: 1.1rem;
		height: 1.1rem;
		fill: none;
		stroke: currentColor;
		stroke-width: 2.5;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	.step-body {
		min-width: 0;
		flex: 1;
	}
	h4 {
		font-size: var(--text-base);
		line-height: 1.35;
		padding-top: 0.2rem;
	}
	.done h4 {
		color: var(--ink-soft);
		font-weight: 400;
	}
	.step-body p {
		color: var(--ink-soft);
		margin-top: 0.25rem;
		max-width: 40rem;
	}
	.action {
		margin-top: 1rem;
		display: grid;
		gap: 1rem;
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.75rem;
	}
	.fine {
		font-size: var(--text-sm);
		color: var(--ink-soft);
	}
</style>
