<script lang="ts">
	import { untrack } from 'svelte';
	import {
		api,
		ApiError,
		hasSubscription,
		planOf,
		priceOf,
		type Onboarding,
		type Org,
		type Step,
		type Terms
	} from '$lib/api';
	import AgentPrompt from '$lib/components/AgentPrompt.svelte';
	import { FREE_MEMBER_LIMIT, TRIAL_MONTHS } from '$lib/site';
	import { setupPrompt } from '$lib/snippets';
	import ErrorNotice from './ErrorNotice.svelte';
	import { baseDomain, formatDate, plural } from './format';
	import SlugForm from './SlugForm.svelte';
	import TrialForm from './TrialForm.svelte';

	interface Props {
		doc: Onboarding;
		/** The organisation from the session: its plan, member count and whether a trial is available. */
		org: Org;
		admin: boolean;
		/** Re-reads the onboarding document (after a step done on GitHub). */
		onrefresh: () => Promise<void>;
		onchange: (doc: Onboarding) => void;
		/** Called after the free trial starts, with the new onboarding document already passed to onchange. */
		ontrialstarted?: () => void;
		refreshing: boolean;
		/** The terms to accept with the registry name (session.terms). */
		terms?: Terms;
		/** The preview (session.preview): every organisation is free, so the plan step offers nothing. */
		preview?: boolean;
	}

	let {
		doc,
		org,
		admin,
		onrefresh,
		onchange,
		ontrialstarted,
		refreshing,
		terms,
		preview = false
	}: Props = $props();

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
		settingsWithTerms: {
			title: 'Choose your registry name and accept the terms',
			body: 'The name is saved as privatecrates.toml in your storage repository, so later changes are a pull request. An admin accepts the preview terms on behalf of the organisation at the same time.'
		},
		plan: {
			title: 'Choose a plan',
			body: ''
		}
	};

	const limit = $derived(org.free_member_limit ?? FREE_MEMBER_LIMIT);
	const members = $derived(org.members ?? null);
	/** What the plan step offers an admin, per docs/website-api.md. */
	const planAction = $derived<'trial' | 'checkout' | null>(
		preview
			? null
			: org.trial_available
				? 'trial'
				: planOf(org) === 'inactive' && hasSubscription(org)
					? 'checkout'
					: null
	);

	function planCopy(step: Step): StepCopy {
		if (org.personal)
			return {
				title: 'Plan: free, always',
				body: 'Personal accounts are always free: nothing to choose, now or after general availability.'
			};
		if (preview)
			return {
				title: 'Plan: free during the preview',
				// The server's detail says "Free during the preview.", which the title already does.
				body: 'Nothing to choose: billing is off until general availability.'
			};
		const plan = planOf(org);
		if (step.status === 'done') {
			if (plan === 'free')
				return {
					title: 'Plan: free',
					body:
						members === null
							? `Free for organisations with up to ${limit} members.`
							: `Free: ${members} of ${limit} members. Every feature, no card.`
				};
			if (plan === 'trial') {
				const ends = formatDate(org.trial_ends_at ?? org.tenant?.trial_ends_at);
				return {
					title: 'Plan: free trial',
					body: step.detail ?? (ends ? `Your free trial runs until ${ends}.` : 'Your free trial has started.')
				};
			}
			return { title: 'Plan', body: step.detail ?? '' };
		}
		const count = members === null ? 'This organisation' : `${plural(members, 'member')}:`;
		if (planAction === 'trial')
			return {
				title: `Start your ${TRIAL_MONTHS}-month free trial`,
				body:
					step.detail ??
					`${count} more than the ${limit} the free plan covers. Start a ${TRIAL_MONTHS}-month free trial, no card needed.`
			};
		if (planAction === 'checkout')
			return {
				title: 'Subscribe',
				body:
					step.detail ??
					`This organisation has had its free trial. Subscribe to use the registry: $${priceOf(org)} a month.`
			};
		return { title: 'Choose a plan', body: step.detail ?? '' };
	}

	function stepCopy(step: Step): StepCopy {
		if (step.id === 'plan' || step.id === 'subscription') return planCopy(step);
		if (step.id === 'settings' && terms) return copy.settingsWithTerms;
		return (
			copy[step.id] ?? {
				title: step.id.replace(/_/g, ' ').replace(/^./, (c) => c.toUpperCase()),
				body: '',
				action: 'Continue on GitHub'
			}
		);
	}

	// The registry name typed into the form, so the agent prompt follows it.
	let chosenSlug = $state(untrack(() => (doc.suggested_slug ?? '').toLowerCase()));
	const SLUG_FORMAT = /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/;
	const promptSlug = $derived(
		org.tenant?.slug ??
			(SLUG_FORMAT.test(chosenSlug.trim())
				? chosenSlug.trim()
				: (doc.suggested_slug ?? doc.org.login).toLowerCase())
	);
	const agentPrompt = $derived(
		setupPrompt({
			org: doc.org.login,
			slug: promptSlug,
			apex: baseDomain(),
			// Action URLs may be relative (the mock's are); the agent needs them whole.
			steps: doc.steps.map((s) => ({
				...s,
				action_url: s.action_url ? new URL(s.action_url, location.origin).href : undefined
			})),
			plan: planAction === 'trial' ? 'trial' : planAction === 'checkout' ? 'subscribe' : null,
			personal: org.personal ?? false,
			terms,
			preview
		})
	);

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

	let busy = $state<'checkout' | null>(null);
	let planError = $state<ApiError | null>(null);
	let planErrorTitle = $state('');

	function toApiError(e: unknown): ApiError {
		return e instanceof ApiError ? e : new ApiError(0, [{ detail: String(e) }]);
	}

	function trialStarted(started: Onboarding) {
		onchange(started);
		ontrialstarted?.();
	}

	async function subscribe() {
		busy = 'checkout';
		planError = null;
		try {
			const { url } = await api.checkout(doc.org.login);
			location.assign(url);
		} catch (e) {
			planError = toApiError(e);
			planErrorTitle = 'Checkout could not be opened';
			busy = null;
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
					{#if step.id === 'plan' || step.id === 'subscription'}
						<!-- Built from the server's detail where there is one (see planCopy). -->
						{#if c.body}<p>{c.body}</p>{/if}
					{:else if step.status !== 'done'}
						<!-- The server's detail is organisation-specific; our own copy covers the rest. -->
						{@const text =
							step.detail && (step.id !== 'settings' || step.status === 'blocked') ? step.detail : c.body}
						{#if text}<p>{text}</p>{/if}
					{/if}

					{#if isCurrent}
						<div class="action">
							{#if step.id === 'settings'}
								<SlugForm
									org={doc.org.login}
									suggested={doc.suggested_slug}
									{terms}
									onsaved={onchange}
									bind:slug={chosenSlug}
								/>
							{:else if step.id === 'plan' || step.id === 'subscription'}
								{#if planError}
									<ErrorNotice error={planError} title={planErrorTitle} />
								{/if}
								{#if planAction === 'trial'}
									<TrialForm org={doc.org.login} price={priceOf(org)} onstarted={trialStarted} />
								{:else if planAction === 'checkout'}
									<div class="row">
										<button
											class="btn btn-primary"
											type="button"
											onclick={subscribe}
											disabled={busy !== null}
										>
											{busy === 'checkout'
												? 'Opening Stripe Checkout…'
												: `Subscribe, $${priceOf(org)} a month`}
										</button>
										<span class="fine"
											>You enter card details on Stripe. The first charge is taken today.</span
										>
									</div>
								{/if}
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

	{#if admin && doneCount < doc.steps.length}
		<AgentPrompt id="agent-{doc.org.login}" title="Set up with your AI agent" prompt={agentPrompt}>
			<p>
				Or hand the rest to Claude Code or another coding agent. This prompt is filled in for
				<strong>{doc.org.login}</strong>. The agent works with your own <code>gh</code> login, and stops to
				give you a link when GitHub needs you: to approve its sign-in, and to install each App.{terms
					? ' It also shows you the preview terms and waits for you to accept them: it never accepts for you.'
					: ''}
			</p>
			<p><a href="/docs/agents">What the agent does, step by step</a></p>
		</AgentPrompt>
	{/if}

	{#if doneCount === doc.steps.length}
		<p class="all-done" role="status">
			Every step is done. Opening your registry…
			<button class="btn btn-quiet" type="button" onclick={onrefresh} disabled={refreshing}>
				{refreshing ? 'Checking…' : 'Check again'}
			</button>
		</p>
	{/if}
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
		font-size: var(--text-heading);
	}
	.progress {
		color: var(--ink-muted);
	}
	.member-note {
		background: var(--warn-bg);
		border-left: 5px solid var(--action);
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
		background: var(--surface-raised);
		box-shadow: var(--shadow);
	}
	.marker {
		flex: none;
		display: grid;
		place-items: center;
		width: 2rem;
		height: 2rem;
		border: 2px solid var(--line);
		border-radius: 50%;
		font-weight: 700;
		font-size: var(--text-meta);
	}
	.done .marker {
		background: var(--ok);
		border-color: var(--ok);
		color: var(--ok-ink);
	}
	.current .marker {
		background: var(--action);
		color: var(--action-ink);
	}
	.blocked .marker {
		border-style: dashed;
		color: var(--ink-muted);
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
		font-size: var(--text-body);
		line-height: 1.35;
		padding-top: 0.2rem;
	}
	.done h4 {
		color: var(--ink-muted);
		font-weight: 400;
	}
	.step-body p {
		color: var(--ink-muted);
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
		font-size: var(--text-meta);
		color: var(--ink-muted);
	}
	.all-done {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.75rem;
		color: var(--ink-muted);
	}
</style>
