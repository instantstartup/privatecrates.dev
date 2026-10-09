<script lang="ts">
	import { untrack } from 'svelte';
	import { api, ApiError, type Onboarding, type Terms } from '$lib/api';
	import ErrorNotice from './ErrorNotice.svelte';
	import TermsCheckbox from './TermsCheckbox.svelte';
	import { baseDomain } from './format';

	interface Props {
		org: string;
		suggested?: string;
		onsaved: (doc: Onboarding) => void;
		/** The name as typed, for the agent prompt beside the checklist. */
		slug?: string;
		/**
		 * The current terms (session.terms): an admin accepts them on behalf of the organisation before the registry
		 * is created. Absent from servers older than the terms, which then ask for nothing.
		 */
		terms?: Terms;
	}

	let {
		org,
		suggested = '',
		onsaved,
		terms,
		// The suggestion only seeds the field.
		slug = $bindable(untrack(() => suggested.toLowerCase()))
	}: Props = $props();

	const RESERVED = [
		'www',
		'dev',
		'api',
		'app',
		'docs',
		'status',
		'mail',
		'admin',
		'billing',
		'login',
		'static',
		'assets'
	];

	let touched = $state(false);
	let accepted = $state(false);
	/** Who may publish: written to privatecrates.toml as allow_manual_publish. CI only unless the admin chooses. */
	let publishing = $state<'ci' | 'machines'>('ci');
	/** The server refuses without the terms, so the button waits for the box (and says why, below it). */
	const waitingForTerms = $derived(!!terms && !accepted);
	let saving = $state(false);
	let error = $state<ApiError | null>(null);

	const domain = baseDomain();

	/** Quick checks before asking the server, which has the final say (format, reserved, taken). */
	const problem = $derived.by(() => {
		const s = slug.trim();
		if (!s) return 'Enter a name for your registry.';
		if (/[^a-z0-9-]/.test(s)) return 'Use lowercase letters, digits and hyphens only.';
		if (s.startsWith('-') || s.endsWith('-')) return 'Start and end with a letter or digit.';
		if (RESERVED.includes(s)) return `${s} is reserved. Choose another name.`;
		return null;
	});

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		touched = true;
		if (problem || waitingForTerms) return;
		saving = true;
		error = null;
		try {
			onsaved(await api.saveSettings(org, slug.trim(), terms?.version, publishing === 'machines'));
		} catch (e) {
			error = e instanceof ApiError ? e : new ApiError(0, [{ detail: String(e) }]);
		} finally {
			saving = false;
		}
	}
</script>

<form class="slug-form" onsubmit={submit} novalidate>
	<div class="field">
		<label for="slug-{org}">Registry name</label>
		<p class="hint" id="slug-hint-{org}">
			Your registry will be at <strong class="host">https://{slug.trim() || 'your-name'}.{domain}</strong>. It
			is also the name developers use in <code>Cargo.toml</code>.
		</p>
		<input
			id="slug-{org}"
			name="slug"
			type="text"
			inputmode="url"
			autocomplete="off"
			autocapitalize="none"
			spellcheck="false"
			maxlength="63"
			required
			bind:value={slug}
			onblur={() => (touched = true)}
			aria-invalid={touched && problem ? 'true' : undefined}
			aria-describedby="slug-hint-{org}{touched && problem ? ` slug-problem-{org}` : ''}"
		/>
		{#if touched && problem}
			<p class="problem" id="slug-problem-{org}">{problem}</p>
		{/if}
	</div>
	<fieldset class="publishing">
		<legend>Who can publish crates</legend>
		<label class="choice">
			<input type="radio" name="publishing-{org}" value="ci" bind:group={publishing} />
			<span>
				<strong>GitHub Actions only</strong> (recommended)
				<span class="choice-hint"
					>Developers publish by pushing a tag. Every version is built from a commit, with provenance signed
					by GitHub.</span
				>
			</span>
		</label>
		<label class="choice">
			<input type="radio" name="publishing-{org}" value="machines" bind:group={publishing} />
			<span>
				<strong>Also from developers’ machines</strong>
				<span class="choice-hint"
					>Anyone with write access to a crate’s repository can run <code>cargo publish</code> from a clean git
					checkout, first versions included. Those versions have no provenance, and a stolen sign-in could publish.</span
				>
			</span>
		</label>
		<p class="hint">
			Saved in <code>privatecrates.toml</code> as the default for every repository. Later, a pull request can
			change it, or set it for one repository.
			<a href="/docs/publishing#laptop" target="_blank" rel="noopener"
				>More about publishing<span class="visually-hidden"> (opens in a new tab)</span></a
			>
		</p>
	</fieldset>
	{#if terms}
		<TermsCheckbox
			{org}
			{terms}
			bind:checked={accepted}
			describedby={waitingForTerms ? `slug-wait-${org}` : undefined}
		/>
	{/if}
	{#if error}
		<ErrorNotice {error} title="The name was not saved" />
	{/if}
	<div class="submit">
		<button
			class="btn btn-primary"
			type="submit"
			disabled={saving || waitingForTerms}
			aria-describedby={waitingForTerms ? `slug-wait-${org}` : undefined}
		>
			{saving ? 'Creating registry…' : 'Create registry'}
		</button>
		<!-- Kept in place (and only emptied), so ticking the box does not move the button. -->
		<p class="wait" id="slug-wait-{org}" aria-live="polite">
			{waitingForTerms ? 'Tick the box above to accept the terms, then create the registry.' : ''}
		</p>
	</div>
</form>

<style>
	.slug-form {
		display: grid;
		gap: 1rem;
		max-width: 30rem;
	}
	.field {
		display: grid;
		gap: 0.35rem;
	}
	label {
		font-weight: 700;
	}
	.hint {
		font-size: var(--text-sm);
		color: var(--ink-muted);
	}
	.host {
		color: var(--ink);
		overflow-wrap: anywhere;
	}
	input {
		min-height: 2.75rem;
		padding: 0.5rem 0.75rem;
		border: 2px solid var(--line);
		border-radius: 6px;
		background: var(--surface-raised);
		color: var(--ink);
		font-family: var(--font-mono);
	}
	input[aria-invalid='true'] {
		border-color: var(--danger);
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
	.publishing {
		display: grid;
		gap: 0.6rem;
		border: 0;
		padding: 0;
		margin: 0;
	}
	legend {
		font-weight: 700;
		margin-bottom: 0.35rem;
	}
	.choice {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		gap: 0.7rem;
		align-items: start;
		padding: 0.75rem 1rem;
		border: 2px solid var(--divider);
		border-radius: 8px;
		cursor: pointer;
	}
	.choice:has(input:checked) {
		border-color: var(--line);
		background: var(--surface-raised);
	}
	.choice input {
		width: 1.2rem;
		height: 1.2rem;
		margin-top: 0.2rem;
		accent-color: var(--link);
	}
	.choice-hint {
		display: block;
		font-size: var(--text-sm);
		color: var(--ink-muted);
	}
	.problem {
		color: var(--danger);
		font-weight: 700;
		font-size: var(--text-sm);
	}
</style>
