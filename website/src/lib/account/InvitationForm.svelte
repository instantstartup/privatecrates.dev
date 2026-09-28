<script lang="ts">
	import { api, ApiError } from '$lib/api';
	import ErrorNotice from './ErrorNotice.svelte';

	interface Props {
		/** The organisation to ask for; without it, the person types one. */
		org?: string;
		/** Organisations (lowercase) this person has already asked for. */
		requested: string[];
		/** Called with the organisation (lowercase) once the request is saved. */
		onrequested: (org: string) => void;
	}

	let { org, requested, onrequested }: Props = $props();

	const id = $props.id();
	let typed = $state('');
	let email = $state('');
	let note = $state('');
	let touched = $state(false);
	let sending = $state(false);
	let error = $state<ApiError | null>(null);
	let editing = $state(false);

	const target = $derived((org ?? typed).trim());
	const already = $derived(!!org && requested.includes(org.toLowerCase()) && !editing);
	/** Only the obvious; the server has the final say. */
	const problems = $derived({
		org: org || target ? null : 'Enter your GitHub organisation’s name.',
		email: email.trim() ? null : 'Enter an email address we can reply to.'
	});
	const serverProblem = $derived(
		error?.errors.find(
			(e) => e.code === 'account::invitation_invalid' || e.code === 'account::already_invited'
		) ?? null
	);
	const otherError = $derived(error && !serverProblem ? error : null);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		touched = true;
		if (problems.org || problems.email) return;
		sending = true;
		error = null;
		try {
			const saved = await api.requestInvitation(target, email.trim(), note.trim());
			editing = false;
			typed = '';
			onrequested(saved.org);
		} catch (e) {
			error = e instanceof ApiError ? e : new ApiError(0, [{ detail: String(e) }]);
		} finally {
			sending = false;
		}
	}
</script>

{#if already}
	<div class="done" role="status">
		<p><strong>Invitation requested for {org}.</strong> We reply by email once we have looked at it.</p>
		<button class="btn btn-quiet" type="button" onclick={() => (editing = true)}>Change your request</button>
	</div>
{:else}
	<form class="invitation-form" onsubmit={submit} novalidate>
		{#if !org}
			<div class="field">
				<label for="{id}-org">GitHub organisation</label>
				<input
					id="{id}-org"
					name="org"
					autocomplete="organization"
					autocapitalize="none"
					spellcheck="false"
					placeholder="acme"
					required
					bind:value={typed}
					aria-invalid={touched && problems.org ? 'true' : undefined}
					aria-describedby={touched && problems.org ? `${id}-org-problem` : undefined}
				/>
				{#if touched && problems.org}
					<p class="problem" id="{id}-org-problem">{problems.org}</p>
				{/if}
			</div>
		{/if}
		<div class="field">
			<label for="{id}-email">Email</label>
			<p class="hint" id="{id}-email-hint">
				Where we reply about the invitation. Nothing else is sent to it.
			</p>
			<input
				id="{id}-email"
				name="email"
				type="email"
				autocomplete="email"
				autocapitalize="none"
				spellcheck="false"
				required
				bind:value={email}
				aria-invalid={touched && problems.email ? 'true' : undefined}
				aria-describedby="{id}-email-hint{touched && problems.email ? ` ${id}-email-problem` : ''}"
			/>
			{#if touched && problems.email}
				<p class="problem" id="{id}-email-problem">{problems.email}</p>
			{/if}
		</div>
		<div class="field">
			<label for="{id}-note">Anything we should know <span class="optional">(optional)</span></label>
			<p class="hint" id="{id}-note-hint">For example, how many people and crates, or what you use today.</p>
			<textarea
				id="{id}-note"
				name="note"
				rows="3"
				maxlength="2000"
				bind:value={note}
				aria-describedby="{id}-note-hint"></textarea>
		</div>
		{#if serverProblem}
			<p class="problem" role="alert">
				{serverProblem.detail}
				<span class="code"
					>(<a href="/docs/errors#{serverProblem.code}"><code>{serverProblem.code}</code></a>)</span
				>
			</p>
		{/if}
		{#if otherError}
			<ErrorNotice error={otherError} title="The request could not be sent" />
		{/if}
		<div class="row">
			<button class="btn btn-primary" type="submit" disabled={sending}>
				{sending ? 'Sending…' : 'Ask for an invitation'}
			</button>
			<span class="fine"
				>We keep your GitHub account, the organisation, this email and your note until we reply, and at most
				12 months: see the <a href="/legal/privacy">privacy notice</a>.</span
			>
		</div>
	</form>
{/if}

<style>
	.invitation-form {
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
	.optional {
		font-weight: 400;
		color: var(--ink-soft);
	}
	.hint,
	.fine {
		font-size: var(--text-sm);
		color: var(--ink-soft);
	}
	input,
	textarea {
		min-height: 2.75rem;
		padding: 0.5rem 0.75rem;
		border: 2px solid var(--line);
		border-radius: 6px;
		background: var(--deck);
		color: var(--ink);
		font: inherit;
	}
	[aria-invalid='true'] {
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
	.row,
	.done {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.75rem;
	}
</style>
