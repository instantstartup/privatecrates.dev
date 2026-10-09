<script lang="ts">
	// Recommends the verifier workflow once the registry exists, until the storage repository has one (SPEC §10.3).
	// The admin adds it with their own GitHub account, on GitHub's new-file page filled in by the server: our storage
	// App cannot write workflows, and should not maintain what checks it.
	import { api, type Verifier } from '$lib/api';
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import { cli } from '$lib/snippets';

	interface Props {
		org: string;
	}

	let { org }: Props = $props();

	let verifier = $state<Verifier | null>(null);
	let checking = $state(false);
	let checked = $state(false);

	async function load() {
		checking = true;
		try {
			verifier = (await api.onboarding(org)).verifier ?? null;
		} catch {
			// Only a recommendation: without it the page is still complete.
		} finally {
			checking = false;
		}
	}

	$effect(() => {
		void org;
		void load();
	});

	async function recheck() {
		await load();
		checked = true;
	}
</script>

{#if verifier && !verifier.installed}
	<section class="verifier" aria-labelledby="verifier-{org}">
		<h3 id="verifier-{org}">Recommended: add the verifier</h3>
		<p>
			<code>privatecrates-verify</code> checks your registry independently of us, in your own storage repository:
			that every crate file is an immutable release matching the index, and that every version’s provenance is signed
			by GitHub. It runs on each publish and once a day, about a minute each time.
		</p>
		<p class="hint">
			You add it with your own GitHub account: GitHub opens the file in <strong>{verifier.repository}</strong
			>, and you commit it. We cannot write workflows to your repositories, and should not maintain what
			checks us.
		</p>
		<div class="actions">
			<a class="btn btn-primary" href={verifier.add_url} target="_blank" rel="noopener"
				>Add it on GitHub<span class="visually-hidden"> (opens in a new tab)</span></a
			>
			<button class="btn" type="button" onclick={recheck} disabled={checking}>
				{checking ? 'Checking…' : 'I have added it'}
			</button>
		</div>
		<p class="status" aria-live="polite">
			{checked && !checking && !verifier.installed
				? `Not found yet in ${verifier.repository}. Commit ${verifier.path} there, then check again.`
				: ''}
		</p>
		<details>
			<summary>See the workflow, or add it from a terminal</summary>
			<CodeBlock caption={verifier.path} code={verifier.workflow} />
			<CodeBlock caption="shell, with your own gh login" code={cli(`add-verifier ${org}`)} />
		</details>
	</section>
{/if}

<style>
	.verifier {
		display: grid;
		gap: var(--space-4);
		padding: var(--space-5) var(--space-card-x);
		border: var(--border-outline) dashed var(--line);
		border-radius: var(--radius-card);
		margin-bottom: var(--space-6);
	}
	h3 {
		font-size: var(--text-lead);
	}
	.hint,
	.status {
		font-size: var(--text-meta);
		color: var(--ink-muted);
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-3);
	}
	summary {
		cursor: pointer;
		font-weight: 700;
	}
	details[open] summary {
		margin-bottom: var(--space-3);
	}
</style>
