<script lang="ts">
	import { untrack } from 'svelte';
	import { api, ApiError, type Onboarding } from '$lib/api';
	import ErrorNotice from './ErrorNotice.svelte';
	import { baseDomain } from './format';

	interface Props {
		org: string;
		suggested?: string;
		onsaved: (doc: Onboarding) => void;
	}

	let { org, suggested = '', onsaved }: Props = $props();

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

	// The suggestion only seeds the field.
	let slug = $state(untrack(() => suggested.toLowerCase()));
	let touched = $state(false);
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
		if (problem) return;
		saving = true;
		error = null;
		try {
			onsaved(await api.saveSettings(org, slug.trim()));
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
	{#if error}
		<ErrorNotice {error} title="The name was not saved" />
	{/if}
	<div>
		<button class="btn btn-primary" type="submit" disabled={saving}>
			{saving ? 'Saving registry name…' : 'Save registry name'}
		</button>
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
		color: var(--ink-soft);
	}
	.host {
		color: var(--ink);
		overflow-wrap: anywhere;
	}
	input {
		min-height: 2.75rem;
		padding: 0.5rem 0.75rem;
		border: 2px solid var(--ink);
		border-radius: 6px;
		background: var(--deck);
		color: var(--ink);
		font-family: var(--font-mono);
	}
	input[aria-invalid='true'] {
		border-color: var(--danger);
	}
	.problem {
		color: var(--danger);
		font-weight: 700;
		font-size: var(--text-sm);
	}
</style>
